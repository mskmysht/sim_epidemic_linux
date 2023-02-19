use futures_util::{Future, StreamExt};
use parking_lot::Mutex;
use quinn::ClientConfig;
use repl::nom::AsBytes;
use std::{
    collections::VecDeque, error::Error, net::SocketAddr, ops::DerefMut, pin::Pin, sync::Arc,
};
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};
use worker_if::batch::{Request, Resource, Response};

use crate::api_server::job;

use super::{
    server::{MyConnection, ServerInfo},
    TaskId,
};

#[derive(Debug)]
pub struct WorkerClient {
    connection: MyConnection,
    index: usize,
    release_tx: mpsc::UnboundedSender<(usize, Resource)>,
}

impl WorkerClient {
    pub async fn new(
        addr: SocketAddr,
        config: ClientConfig,
        server_info: ServerInfo,
        index: usize,
        release_tx: mpsc::UnboundedSender<(usize, Resource)>,
    ) -> Result<(Self, Resource), Box<dyn Error>> {
        let connection = MyConnection::new(
            addr,
            config,
            server_info,
            format!("worker-{index}").to_string(),
        )
        .await?;

        let mut recv = connection.connection.accept_uni().await?;
        let max_resource = protocol::quic::read_data::<Resource>(&mut recv).await?;
        println!("[info] worker {index} has {max_resource:?}");

        Ok((
            Self {
                connection,
                index,
                release_tx,
            },
            max_resource,
        ))
    }

    pub async fn execute(
        &mut self,
        task_id: &TaskId,
        config: job::Config,
        termination_tx: oneshot::Sender<Option<bool>>,
    ) -> anyhow::Result<()> {
        let idx_resource = (self.index, (&config.param).into());
        let (mut send, recv) = self.connection.connection.open_bi().await?;
        protocol::quic::write_data(
            &mut send,
            &Request::Execute(task_id.to_string(), config.param),
        )
        .await?;

        let mut stream = FramedRead::new(recv, LengthDelimitedCodec::new());
        bincode::deserialize::<Response<()>>(stream.next().await.unwrap()?.as_bytes())?
            .as_result()?;
        let release_tx = self.release_tx.clone();
        tokio::spawn(async move {
            let exit_status = stream
                .next()
                .await
                .unwrap()
                .ok()
                .map(|data| bincode::deserialize::<bool>(data.as_bytes()).unwrap());
            termination_tx.send(exit_status).unwrap();
            release_tx.send(idx_resource).unwrap();
        });
        Ok(())
    }

    pub async fn terminate(&mut self, task_id: &TaskId) -> anyhow::Result<()> {
        let (mut send, mut recv) = self.connection.connection.open_bi().await?;
        protocol::quic::write_data(&mut send, &Request::Terminate(task_id.to_string())).await?;
        protocol::quic::read_data::<Response<()>>(&mut recv)
            .await?
            .as_result()?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Worker(Arc<RwLock<WorkerClient>>);

impl Worker {
    fn new(client: WorkerClient) -> Self {
        Self(Arc::new(RwLock::new(client)))
    }

    pub async fn write(&self) -> impl DerefMut<Target = WorkerClient> + '_ {
        self.0.write().await
    }
}

#[derive(Debug)]
struct WorkerPool {
    resource: Resource,
    tx: oneshot::Sender<Worker>,
}

pub struct WorkerManager {
    queue_tx: mpsc::UnboundedSender<WorkerPool>,
}

impl WorkerManager {
    pub async fn new(
        addr: SocketAddr,
        cert_path: String,
        servers: Vec<ServerInfo>,
    ) -> Result<Self, Box<dyn Error>> {
        let (release_tx, mut release_rx) = mpsc::unbounded_channel();
        let config = quic_config::get_client_config(&cert_path)?;
        let mut workers = Vec::new();
        for (i, server_info) in servers.into_iter().enumerate() {
            let (worker, max_resource) = WorkerClient::new(
                addr.clone(),
                config.clone(),
                server_info,
                i,
                release_tx.clone(),
            )
            .await?;
            workers.push((Worker::new(worker), parking_lot::RwLock::new(max_resource)));
        }

        let workers = Arc::new(workers);
        let queue = Arc::new(Mutex::new(VecDeque::new()));

        let (queue_tx, mut queue_rx) = mpsc::unbounded_channel::<WorkerPool>();
        {
            let workers = Arc::clone(&workers);
            let queue = Arc::clone(&queue);
            tokio::spawn(async move {
                while let Some(pool) = queue_rx.recv().await {
                    let mut max_res = pool.resource;
                    let mut max_worker = None;
                    for (w, r) in workers.iter() {
                        let res = *r.read();
                        if res >= max_res {
                            max_res = res;
                            max_worker = Some(w)
                        }
                    }
                    match max_worker {
                        Some(w) => {
                            pool.tx.send(w.clone()).unwrap();
                        }
                        None => {
                            queue.lock().push_back(pool);
                        }
                    }
                }
            });
        }

        tokio::spawn(async move {
            while let Some((index, released)) = release_rx.recv().await {
                let (worker, resource) = &workers[index];
                let mut resource = resource.write();
                *resource += released;
                let mut queue = queue.lock();
                if let Some(pool) = queue.front() {
                    if pool.resource >= *resource {
                        let pool = queue.pop_front().unwrap();
                        pool.tx.send(worker.clone()).unwrap();
                    }
                }
            }
        });

        Ok(Self { queue_tx })
    }

    pub fn lease(&self, resource: Resource) -> WorkerLease {
        let (tx, rx) = oneshot::channel();
        self.queue_tx.send(WorkerPool { resource, tx }).unwrap();
        WorkerLease(rx)
    }
}

pub struct WorkerLease(oneshot::Receiver<Worker>);

impl Future for WorkerLease {
    type Output = Result<Worker, oneshot::error::RecvError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}
