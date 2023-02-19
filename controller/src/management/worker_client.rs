use futures_util::{Future, StreamExt};
use poem_openapi::__private::serde::Deserialize;
use quinn::ClientConfig;
use repl::nom::AsBytes;
use std::{error::Error, net::SocketAddr, ops::DerefMut, sync::Arc};
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
    resource: Resource,
}

impl WorkerClient {
    pub async fn new(
        addr: SocketAddr,
        config: ClientConfig,
        server_info: ServerInfo,
        index: usize,
        // pool_tx: mpsc::Sender<usize>,
        // termination_tx: mpsc::UnboundedSender<ProcessInfo>,
    ) -> Result<Self, Box<dyn Error>> {
        let connection = MyConnection::new(
            addr,
            config,
            server_info,
            format!("worker-{index}").to_string(),
        )
        .await?;

        let mut recv = connection.connection.accept_uni().await?;
        let resource = protocol::quic::read_data::<Resource>(&mut recv).await?;
        // let mut pool_stream = FramedRead::new(
        //     connection.connection.accept_uni().await?,
        //     LengthDelimitedCodec::new(),
        // );
        println!("[info] worker {index} has {resource:?}");

        // tokio::spawn(async move {
        //     while let Some(frame) = pool_stream.next().await {
        //         let data = frame.unwrap();
        //         bincode::deserialize::<()>(data.as_bytes()).unwrap();
        //         pool_tx.send(index).await.unwrap();
        //     }
        // });

        // let mut termination_stream = FramedRead::new(
        //     connection.connection.accept_uni().await?,
        //     LengthDelimitedCodec::new(),
        // );
        // // dump dummy data
        // termination_stream.next().await;
        // println!("[info] accepted a worker termination stream");
        // tokio::spawn(async move {
        //     while let Some(frame) = termination_stream.next().await {
        //         let data = frame.unwrap();
        //         let info = bincode::deserialize::<ProcessInfo>(data.as_bytes()).unwrap();
        //         termination_tx.send(info).unwrap();
        //     }
        // });

        Ok(Self {
            connection,
            resource,
        })
    }

    async fn request<T: for<'de> Deserialize<'de>>(
        &mut self,
        req: Request,
    ) -> anyhow::Result<Response<T>> {
        protocol::quic::request(&self.connection.connection, req).await
    }

    pub async fn execute(
        &mut self,
        task_id: &TaskId,
        config: job::Config,
        termination_tx: oneshot::Sender<Option<bool>>,
    ) -> anyhow::Result<()> {
        let (mut send, mut recv) = self.connection.connection.open_bi().await?;
        protocol::quic::write_data(
            &mut send,
            &Request::Execute(task_id.to_string(), config.param),
        )
        .await?;
        protocol::quic::read_data::<Response<()>>(&mut recv)
            .await?
            .as_result()?;
        tokio::spawn(async move {
            let exit_status = protocol::quic::read_data::<bool>(&mut recv).await.ok();
            termination_tx.send(exit_status).unwrap();
        });
        Ok(())
    }

    pub async fn terminate(&mut self, task_id: &TaskId) -> anyhow::Result<Response<()>> {
        self.request(Request::Terminate(task_id.to_string())).await
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

    // pub async fn execute(&self, task_id: &TaskId, config: job::Config) -> bool {
    //     let mut worker = self.0.write().await;
    //     worker.execute(task_id, config).await.is_ok()
    // }

    pub async fn terminate(&self, task_id: &TaskId) -> anyhow::Result<()> {
        let mut worker = self.0.write().await;
        worker.terminate(task_id).await?;
        Ok(())
    }
}

pub struct WorkerManager {
    pool_rx: mpsc::Receiver<usize>,
    workers: Arc<Vec<Worker>>,
    peeked: Arc<RwLock<Vec<Worker>>>,
}

impl WorkerManager {
    pub async fn new(
        addr: SocketAddr,
        cert_path: String,
        servers: Vec<ServerInfo>,
        // task_table: super::TaskSenderTableRef,
    ) -> Result<Self, Box<dyn Error>> {
        // let (task_termination_tx, mut task_termination_rx) = mpsc::unbounded_channel();
        let config = quic_config::get_client_config(&cert_path)?;
        let (pool_tx, pool_rx) = mpsc::channel(servers.len());
        let mut workers = Vec::new();
        for (i, server_info) in servers.into_iter().enumerate() {
            workers.push(Worker::new(
                WorkerClient::new(
                    addr.clone(),
                    config.clone(),
                    server_info,
                    i,
                    // pool_tx.clone(),
                    // task_termination_tx.clone(),
                )
                .await?,
            ));
        }

        // tokio::spawn(async move {
        //     while let Some(ProcessInfo {
        //         world_id,
        //         exit_status,
        //     }) = task_termination_rx.recv().await
        //     {
        //         println!("[termination] world id: {world_id}, exit status: {exit_status}");
        //         let task_id: TaskId = TaskId::try_from(world_id.as_str()).unwrap();
        //         let tx = task_table.write().await.remove(&task_id).unwrap();
        //         tx.send(exit_status).unwrap();
        //     }
        // });

        Ok(Self {
            workers: Arc::new(workers),
            pool_rx,
            peeked: Default::default(),
        })
    }

    pub async fn wait(&mut self) {
        let mut peeked = self.peeked.write().await;
        if peeked.is_empty() {
            let i = self.pool_rx.recv().await.unwrap();
            peeked.push(self.workers[i].clone());
        }
    }

    async fn get(&mut self) -> Worker {
        match self.peeked.write().await.pop() {
            Some(worker) => worker,
            None => self.workers[self.pool_rx.recv().await.unwrap()].clone(),
        }
    }

    async fn set(&self, worker: Worker) {
        println!("[debug: worker manager] try to release worker");
        self.peeked.write().await.push(worker);
        println!("[debug: worker manager] released worker");
    }
}

pub struct WorkerLease {
    manager: Arc<RwLock<WorkerManager>>,
}

impl WorkerLease {
    pub fn new(manager: Arc<RwLock<WorkerManager>>) -> Self {
        Self { manager }
    }
    pub async fn lease<F: FnOnce(Worker) -> Fut + Send, Fut: Future<Output = bool> + Send>(
        self,
        f: F,
    ) {
        let worker = self.manager.write().await.get().await;
        if f(worker.clone()).await {
            self.manager.write().await.set(worker).await;
        }
    }
}
