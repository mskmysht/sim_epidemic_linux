use controller_if::ProcessInfo;
use futures_util::StreamExt;
use quinn::ClientConfig;
use repl::nom::AsBytes;
use std::{
    error::Error,
    fmt::Display,
    net::SocketAddr,
    sync::{Arc, Weak},
};
use tokio::sync::{mpsc, RwLock};
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};
use uuid::Uuid;
use worker_if::batch::{world_if, Request, Response, ResponseOk};

use crate::api_server::{
    job,
    task::{self, TaskState},
};

use super::{
    server::{MyConnection, ServerInfo},
    TaskTableRef,
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

#[derive(Clone)]
pub struct WorkerManager {
    pool_rx: async_channel::Receiver<usize>,
    workers: Arc<Vec<Arc<RwLock<WorkerClient>>>>,
}

impl WorkerManager {
    pub async fn new(
        addr: SocketAddr,
        cert_path: String,
        servers: Vec<ServerInfo>,
        task_table: TaskTableRef,
    ) -> Result<Self, Box<dyn Error>> {
        let (task_termination_tx, mut task_termination_rx) = mpsc::unbounded_channel();
        let config = quic_config::get_client_config(&cert_path)?;
        let (pool_tx, pool_rx) = async_channel::bounded(servers.len());
        let mut workers = Vec::new();
        for (i, server_info) in servers.into_iter().enumerate() {
            workers.push(Arc::new(RwLock::new(
                WorkerClient::new(
                    addr.clone(),
                    config.clone(),
                    server_info,
                    i,
                    pool_tx.clone(),
                    task_termination_tx.clone(),
                )
                .await?,
            )));
        }

        tokio::spawn(async move {
            while let Some(ProcessInfo {
                world_id,
                exit_status,
            }) = task_termination_rx.recv().await
            {
                let task_id: TaskId = TaskId::try_from(world_id.as_str()).unwrap();
                let task = &task_table.read().await[&task_id];
                let status = if exit_status {
                    TaskConsumptionStatus::ExitSucceeded
                } else {
                    TaskConsumptionStatus::ExitFailed
                };
                task.notify_consumption(status);
            }
        });

        Ok(Self {
            workers: Arc::new(workers),
            pool_rx,
        })
    }

    pub async fn get(&mut self) -> Result<Arc<RwLock<WorkerClient>>, async_channel::RecvError> {
        let index = self.pool_rx.recv().await?;
        Ok(Arc::clone(&self.workers[index]))
    }
}

#[derive(Clone, Debug)]
struct TaskInner {
    state: task::TaskState,
    worker: Weak<RwLock<WorkerClient>>,
    forced_termination: bool,
}

impl TaskInner {
    fn new() -> Self {
        Self {
            state: Default::default(),
            worker: Weak::new(),
            forced_termination: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(pub(super) Uuid);

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

#[derive(Clone, Debug)]
pub struct Task {
    pub id: TaskId,
    inner: Arc<RwLock<TaskInner>>,
    task_termination_tx: mpsc::UnboundedSender<(Task, TaskConsumptionStatus)>,
}

impl Task {
    pub fn new(
        id: TaskId,
        task_termination_tx: mpsc::UnboundedSender<(Task, TaskConsumptionStatus)>,
    ) -> Self {
        Self {
            id,
            inner: Arc::new(RwLock::new(TaskInner::new())),
            task_termination_tx,
        }
    }

    pub async fn make_obj(&self) -> task::Task {
        let task = self.inner.read().await;
        task::Task::new(self.id.to_string(), task.state.clone())
    }

    pub async fn force_to_terminate(&self) {
        let mut task = self.inner.write().await;
        task.forced_termination = true;
        if matches!(task.state, TaskState::Running) {
            let worker = task.worker.upgrade().unwrap();
            let mut worker = worker.write().await;
            if let Err(e) = worker.terminate(&self.id).await {
                eprintln!("[error] {e}");
            }
        }
    }

    pub fn notify_consumption(&self, status: TaskConsumptionStatus) {
        self.task_termination_tx
            .send((self.clone(), status))
            .unwrap();
    }

    pub async fn consume(
        &self,
        config: job::Config,
        worker_manager: &mut WorkerManager,
    ) -> Option<bool> {
        let id = self.id.clone();
        println!("[info] received task {}", id);
        let task = &self.inner;
        if task.read().await.forced_termination {
            return None;
        }
        let worker = worker_manager.get().await.unwrap();
        {
            let mut task = task.write().await;
            if task.forced_termination {
                return None;
            } else {
                task.state = TaskState::Assigned;
                task.worker = Arc::downgrade(&worker);
            }
        }
        let task = Arc::clone(task);
        let succeeded = tokio::spawn(async move {
            let mut task = task.write().await;
            let mut worker = worker.write().await;
            if worker.execute(&id, config).await.is_ok() {
                task.state = TaskState::Running;
                true
            } else {
                task.state = TaskState::Failed;
                false
            }
        })
        .await
        .unwrap();
        Some(succeeded)
    }

    pub async fn update_state(&self, state: TaskState) {
        let mut task = self.inner.write().await;
        task.state = state;
    }
}

#[derive(Debug)]
pub enum TaskConsumptionStatus {
    NotConsumed,
    ExecutionFailed,
    ExitFailed,
    ExitSucceeded,
}
