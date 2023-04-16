use std::{
    error::Error,
    fmt::Display,
    net::{IpAddr, SocketAddr},
    path::Path,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use futures_util::{stream::FuturesUnordered, Future, StreamExt};
use quinn::{ClientConfig, Connection, Endpoint, TransportConfig, VarInt};
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};
use uuid::Uuid;

use worker_if::{Cost, Request, ResourceMeasure, Response};

use crate::manager::OneshotNotifyReceiver;
use crate::{
    app::{job, task::TaskState},
    database::Db,
};

#[derive(thiserror::Error, Debug)]
pub enum ClientConfigError {
    #[error("Failed to load a certificate file: {0}")]
    CertificateLoadError(#[from] std::io::Error),
    #[error("Root certificate error: {0}")]
    RootCertificateError(#[from] anyhow::Error),
}

pub fn get_client_config<P: AsRef<Path>>(cert_path: P) -> Result<ClientConfig, ClientConfigError> {
    let mut certs = rustls::RootCertStore::empty();
    certs
        .add(&rustls::Certificate(file_io::load(cert_path)?))
        .map_err(anyhow::Error::new)?;
    let mut config = ClientConfig::with_root_certificates(certs);
    let mut tc = TransportConfig::default();
    tc.max_idle_timeout(Some(VarInt::from_u32(60_000).into()));
    tc.keep_alive_interval(Some(Duration::from_secs(30)));
    config.transport_config(Arc::new(tc));
    Ok(config)
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TaskId(pub Uuid);

impl TryFrom<&str> for TaskId {
    type Error = uuid::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Self(value.try_into()?))
    }
}

impl Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.to_string())
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct ServerConfig {
    pub controller_port: u16,
    pub cert_path: String,
    pub addr: SocketAddr,
    pub domain: String,
}

#[derive(Clone, Debug)]
pub(super) struct WorkerClient {
    connection: Arc<Connection>,
    semaphore: Arc<Semaphore>,
    measure: ResourceMeasure,
    index: usize,
}

impl WorkerClient {
    async fn new(
        client_addr: IpAddr,
        server_config: ServerConfig,
        index: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let mut endpoint =
            Endpoint::client(SocketAddr::new(client_addr, server_config.controller_port))?;
        endpoint.set_default_client_config(get_client_config(&server_config.cert_path)?);

        let connection = endpoint
            .connect(server_config.addr, &server_config.domain)?
            .await?;

        let mut recv = connection.accept_uni().await?;
        let measure = protocol::quic::read_data::<ResourceMeasure>(&mut recv).await?;
        tracing::debug!("worker {} has {:?}", index, measure);

        Ok(Self {
            connection: Arc::new(connection),
            semaphore: Arc::new(Semaphore::new(measure.max_resource as usize)),
            measure,
            index,
        })
    }

    async fn acquire(self, n: u32) -> WorkerClientPermitted {
        let permit = self.semaphore.clone().acquire_many_owned(n).await.unwrap();
        WorkerClientPermitted(self, permit)
    }

    pub async fn execute(
        &self,
        task_id: &TaskId,
        config: job::Config,
    ) -> anyhow::Result<impl Future<Output = Option<bool>>> {
        let (mut send, recv) = self.connection.open_bi().await?;
        protocol::quic::write_data(
            &mut send,
            &Request::Execute(task_id.to_string(), config.param),
        )
        .await?;

        let mut stream = FramedRead::new(recv, LengthDelimitedCodec::new());
        if let Err(e) =
            bincode::deserialize::<Response<()>>(&stream.next().await.unwrap()?)?.as_result()
        {
            return Err(e.into());
        }

        Ok(async move {
            stream
                .next()
                .await
                .unwrap()
                .ok()
                .map(|data| bincode::deserialize::<bool>(&data).unwrap())
        })
    }

    pub async fn terminate(&self, task_id: &TaskId) -> anyhow::Result<()> {
        let (mut send, mut recv) = self.connection.open_bi().await?;
        protocol::quic::write_data(&mut send, &Request::Terminate(task_id.to_string())).await?;
        protocol::quic::read_data::<Response<()>>(&mut recv)
            .await?
            .as_result()?;
        Ok(())
    }

    pub async fn get_statistics(&self, task_id: &TaskId) -> anyhow::Result<Vec<u8>> {
        let (mut send, recv) = self.connection.open_bi().await?;
        protocol::quic::write_data(&mut send, &Request::ReadStatistics(task_id.to_string()))
            .await?;
        let mut stream = FramedRead::new(recv, LengthDelimitedCodec::new());
        match bincode::deserialize::<Response<Vec<u8>>>(&stream.next().await.unwrap()?)?.as_result()
        {
            Ok(buf) => Ok(buf),
            Err(e) => {
                tracing::error!("{}", e);
                Err(e.into())
            }
        }
    }

    /// Returns a string vector of `TaskId`s whose statistics could not removed.
    pub async fn remove_statistics(&self, task_ids: &[TaskId]) -> anyhow::Result<Vec<String>> {
        let (mut send, mut recv) = self.connection.open_bi().await?;
        protocol::quic::write_data(
            &mut send,
            &Request::RemoveStatistics(task_ids.into_iter().map(|id| id.to_string()).collect()),
        )
        .await?;
        Ok(protocol::quic::read_data::<Vec<String>>(&mut recv).await?)
    }
}

#[derive(Debug)]
pub(super) struct WorkerClientPermitted(WorkerClient, OwnedSemaphorePermit);

impl WorkerClientPermitted {
    pub fn index(&self) -> usize {
        self.0.index
    }

    pub async fn execute(
        self,
        task_id: &TaskId,
        config: job::Config,
        db: &Db,
        fq_rx: OneshotNotifyReceiver,
    ) {
        let worker = self.0;
        let semphore = self.1;
        let index = worker.index;

        tracing::debug!("preparing");
        db.update_task_state(&task_id, &TaskState::Assigned).await;
        let Ok(fut) = worker.execute(&task_id, config).await else {
                    db.update_task_state(&task_id, &TaskState::Failed).await;
                    tracing::error!("could not execute");
                    return;
                };

        db.update_task_state(&task_id, &TaskState::Running).await;
        tracing::info!("executing");
        let id = task_id.clone();
        let fq_handle = tokio::spawn(async move {
            if let Ok(_) = fq_rx.notified().await {
                if worker.terminate(&id).await.is_err() {
                    tracing::info!("already terminated");
                }
            }
        });

        let result = fut.await;
        fq_handle.abort();
        drop(semphore);

        match result {
            Some(true) => {
                db.update_task_succeeded(&task_id, index).await;
                tracing::info!("terminated");
            }
            _ => {
                db.update_task_state(&task_id, &TaskState::Failed).await;
                tracing::error!("failed due to an process error");
            }
        }
    }
}

pub(super) struct WorkerManager {
    workers: Vec<WorkerClient>,
    queue_tx: mpsc::Sender<(oneshot::Sender<WorkerClientPermitted>, Cost)>,
}

impl WorkerManager {
    pub async fn new(
        client_addr: IpAddr,
        servers: Vec<ServerConfig>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut _workers = Vec::new();
        for (i, server_config) in servers.into_iter().enumerate() {
            _workers.push(WorkerClient::new(client_addr, server_config, i).await?);
        }

        let (queue_tx, mut queue_rx): (mpsc::Sender<(oneshot::Sender<_>, _)>, _) = mpsc::channel(1);

        let workers = _workers.clone();
        tokio::spawn(async move {
            while let Some((tx, cost)) = queue_rx.recv().await {
                let mut futs = FuturesUnordered::new();
                for client in &workers {
                    let Ok(res) = client.measure.measure(&cost) else {
                            continue;
                        };
                    let client = client.clone();
                    tracing::debug!("register {}", client.index);
                    futs.push(client.acquire(res));
                }
                if let Some(permit) = futs.next().await {
                    tracing::debug!("acquired at {}", permit.0.index);
                    if let Err(_) = tx.send(permit) {
                        tracing::debug!("a lease has already dropped");
                    }
                }
                drop(futs);
            }
        });
        Ok(Self {
            queue_tx,
            workers: _workers,
        })
    }

    pub fn get_worker_count(&self) -> usize {
        self.workers.len()
    }

    pub fn get_worker(&self, index: usize) -> &WorkerClient {
        &self.workers[index]
    }

    pub async fn lease(&self, cost: Cost) -> WorkerLease {
        let (tx, rx) = oneshot::channel();
        self.queue_tx.send((tx, cost)).await.unwrap();
        WorkerLease(rx)
    }
}

pub(super) struct WorkerLease(oneshot::Receiver<WorkerClientPermitted>);

impl Future for WorkerLease {
    type Output = Result<WorkerClientPermitted, oneshot::error::RecvError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        Pin::new(&mut self.as_mut().0).poll(cx)
    }
}
