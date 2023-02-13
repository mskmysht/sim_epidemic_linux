use controller_if::ProcessInfo;
use futures_util::{Future, StreamExt};
use quinn::ClientConfig;
use repl::nom::AsBytes;
use std::{error::Error, net::SocketAddr, sync::Arc};
use tokio::sync::{mpsc, RwLock};
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};
use worker_if::batch::{world_if, Request, Response, ResponseOk};

use crate::api_server::job;

use super::{
    server::{MyConnection, ServerInfo},
    TaskId,
};

#[derive(Debug)]
pub struct WorkerClient {
    connection: MyConnection,
}

impl WorkerClient {
    pub async fn new(
        addr: SocketAddr,
        config: ClientConfig,
        server_info: ServerInfo,
        index: usize,
        pool_tx: async_channel::Sender<usize>,
        termination_tx: mpsc::UnboundedSender<ProcessInfo>,
    ) -> Result<Self, Box<dyn Error>> {
        let connection = MyConnection::new(
            addr,
            config,
            server_info,
            format!("worker-{index}").to_string(),
        )
        .await?;

        let mut pool_stream = FramedRead::new(
            connection.connection.accept_uni().await?,
            LengthDelimitedCodec::new(),
        );
        println!("[info] accepted a worker pool stream");

        tokio::spawn(async move {
            while let Some(frame) = pool_stream.next().await {
                let data = frame.unwrap();
                bincode::deserialize::<()>(data.as_bytes()).unwrap();
                pool_tx.send(index).await.unwrap();
            }
        });

        let mut termination_stream = FramedRead::new(
            connection.connection.accept_uni().await?,
            LengthDelimitedCodec::new(),
        );
        // dump dummy data
        termination_stream.next().await;
        println!("[info] accepted a worker termination stream");
        tokio::spawn(async move {
            while let Some(frame) = termination_stream.next().await {
                let data = frame.unwrap();
                let info = bincode::deserialize::<ProcessInfo>(data.as_bytes()).unwrap();
                termination_tx.send(info).unwrap();
            }
        });

        Ok(Self { connection })
    }

    async fn request(&mut self, req: &Request) -> anyhow::Result<ResponseOk> {
        let (mut send, mut recv) = self.connection.connection.open_bi().await?;

        protocol::quic::write_data(&mut send, req).await?;
        let res = protocol::quic::read_data(&mut recv).await?;
        match res {
            Response::Ok(ok) => Ok(ok),
            Response::Err(e) => Err(anyhow::Error::new(e)),
        }
    }

    pub async fn execute(&mut self, task_id: &TaskId, config: job::Config) -> anyhow::Result<()> {
        self.request(&Request::LaunchItem(task_id.to_string()))
            .await?;
        self.request(&Request::Custom(
            task_id.to_string(),
            world_if::Request::Execute(config.param.stop_at),
        ))
        .await?;
        Ok(())
    }

    pub async fn terminate(&mut self, task_id: &TaskId) -> anyhow::Result<()> {
        self.request(&Request::Custom(
            task_id.to_string(),
            world_if::Request::Terminate,
        ))
        .await?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Worker(Arc<RwLock<WorkerClient>>);

impl Worker {
    fn new(client: WorkerClient) -> Self {
        Self(Arc::new(RwLock::new(client)))
    }

    pub async fn execute(&self, task_id: &TaskId, config: job::Config) -> bool {
        let mut worker = self.0.write().await;
        worker.execute(task_id, config).await.is_ok()
    }

    pub async fn terminate(&self, task_id: &TaskId) {
        let mut worker = self.0.write().await;
        if let Err(e) = worker.terminate(task_id).await {
            eprintln!("[error] {e}");
        }
    }
}

#[derive(Clone)]
pub struct WorkerManager {
    pool_rx: async_channel::Receiver<usize>,
    workers: Arc<Vec<Worker>>,
    peeked: Arc<RwLock<Option<Worker>>>,
}

impl WorkerManager {
    pub async fn new(
        addr: SocketAddr,
        cert_path: String,
        servers: Vec<ServerInfo>,
        task_table: super::TaskSenderTableRef,
    ) -> Result<Self, Box<dyn Error>> {
        let (task_termination_tx, mut task_termination_rx) = mpsc::unbounded_channel();
        let config = quic_config::get_client_config(&cert_path)?;
        let (pool_tx, pool_rx) = async_channel::bounded(servers.len());
        let mut workers = Vec::new();
        for (i, server_info) in servers.into_iter().enumerate() {
            workers.push(Worker::new(
                WorkerClient::new(
                    addr.clone(),
                    config.clone(),
                    server_info,
                    i,
                    pool_tx.clone(),
                    task_termination_tx.clone(),
                )
                .await?,
            ));
        }

        tokio::spawn(async move {
            while let Some(ProcessInfo {
                world_id,
                exit_status,
            }) = task_termination_rx.recv().await
            {
                let task_id: TaskId = TaskId::try_from(world_id.as_str()).unwrap();
                let tx = task_table.write().await.remove(&task_id).unwrap();
                tx.send(exit_status).unwrap();
            }
        });

        Ok(Self {
            workers: Arc::new(workers),
            pool_rx,
            peeked: Default::default(),
        })
    }

    pub async fn wait(&self) {
        let mut peeked = self.peeked.write().await;
        if peeked.is_none() {
            let i = self.pool_rx.recv().await.unwrap();
            *peeked = Some(self.workers[i].clone());
        }
    }

    pub async fn lease<F: FnOnce(Worker) -> Fut + Send, Fut: Future<Output = bool> + Send>(
        self,
        f: F,
    ) {
        let worker = self.get().await;
        if f(worker.clone()).await {
            self.set(worker).await;
        }
    }

    async fn get(&self) -> Worker {
        match self.peeked.write().await.take() {
            Some(worker) => worker,
            None => self.workers[self.pool_rx.recv().await.unwrap()].clone(),
        }
    }

    async fn set(&self, worker: Worker) {
        *self.peeked.write().await = Some(worker);
    }
}
