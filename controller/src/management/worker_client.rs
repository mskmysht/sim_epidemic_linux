use controller_if::ProcessInfo;
use futures_util::StreamExt;
use quinn::ClientConfig;
use repl::nom::AsBytes;
use std::{
    error::Error,
    net::SocketAddr,
    sync::{Arc, Weak},
};
use tokio::sync::{mpsc, RwLock, RwLockReadGuard};
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};
use worker_if::batch::{world_if, Request, Response, ResponseOk};

use crate::api_server::{
    job,
    task::{self, TaskState},
};

use super::{
    server::{MyConnection, ServerInfo},
    Job, TaskTableRef,
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
        self.request(&Request::LaunchItem(task_id.clone())).await?;
        self.request(&Request::Custom(
            task_id.clone(),
            world_if::Request::Execute(config.param.stop_at),
        ))
        .await?;
        Ok(())
    }

    pub async fn terminate(&mut self, task_id: &TaskId) -> anyhow::Result<()> {
        self.request(&Request::Custom(
            task_id.clone(),
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
                let task_id: TaskId = world_id;
                let task = &task_table.read().await[&task_id];
                let job = &task.get_job().await;
                let status = if exit_status {
                    TaskConsumptionStatus::ExitSucceeded
                } else {
                    TaskConsumptionStatus::ExitFailed
                };
                job.notify_task_consumption(task.clone(), status)
                    .await
                    .unwrap();
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
    job: Job,
    worker: Weak<RwLock<WorkerClient>>,
    forced_termination: bool,
}

impl TaskInner {
    fn new(job: Job) -> Self {
        Self {
            state: Default::default(),
            job,
            worker: Weak::new(),
            forced_termination: false,
        }
    }
}

pub type TaskId = String;

#[derive(Clone, Debug)]
pub struct Task {
    pub id: TaskId,
    inner: Arc<RwLock<TaskInner>>,
}

impl Task {
    pub fn new(id: TaskId, job: Job) -> Self {
        Self {
            id,
            inner: Arc::new(RwLock::new(TaskInner::new(job))),
        }
    }

    pub async fn make_obj(&self) -> task::Task {
        let task = self.inner.read().await;
        task::Task::new(self.id.clone(), task.state.clone())
    }

    pub async fn get_job(&self) -> RwLockReadGuard<'_, Job> {
        RwLockReadGuard::map(self.inner.read().await, |task| &task.job)
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

    async fn succeeded(&self) {
        let mut task = self.inner.write().await;
        task.state = TaskState::Succeeded;
    }

    pub async fn asigned(&self) {
        let mut task = self.inner.write().await;
        task.state = TaskState::Assigned;
    }

    async fn failed(&self) {
        let mut task = self.inner.write().await;
        task.state = TaskState::Failed;
    }
}

pub struct TaskConsumer {
    job: Job,
    tasks: Vec<Task>,
    config: job::Config,
    worker_manager: WorkerManager,
}

impl TaskConsumer {
    pub fn new(
        job: Job,
        tasks: Vec<Task>,
        config: job::Config,
        worker_manager: WorkerManager,
    ) -> Self {
        Self {
            job,
            tasks,
            config,
            worker_manager,
        }
    }

    pub async fn consume(mut self) {
        for (i, task) in self.tasks.into_iter().enumerate() {
            match task
                .consume(self.config.clone(), &mut self.worker_manager)
                .await
            {
                Some(true) => {
                    if i == 0 {
                        self.job.running().await;
                    }
                }
                Some(false) => self
                    .job
                    .notify_task_consumption(task, TaskConsumptionStatus::ExecutionFailed)
                    .await
                    .unwrap(),

                None => self
                    .job
                    .notify_task_consumption(task, TaskConsumptionStatus::NotConsumed)
                    .await
                    .unwrap(),
            }
        }
    }
}

#[derive(Debug)]
pub enum TaskConsumptionStatus {
    NotConsumed,
    ExecutionFailed,
    ExitFailed,
    ExitSucceeded,
}

pub struct TaskListener {
    task_termination_rx: mpsc::UnboundedReceiver<(Task, TaskConsumptionStatus)>,
    task_count: usize,
}

impl TaskListener {
    pub fn new(
        task_termination_rx: mpsc::UnboundedReceiver<(Task, TaskConsumptionStatus)>,
        task_count: usize,
    ) -> Self {
        Self {
            task_termination_rx,
            task_count,
        }
    }
    pub async fn listen(mut self) {
        for _ in 0..self.task_count {
            let Some((task, status)) = self.task_termination_rx.recv().await else {
                break;
            };
            match status {
                TaskConsumptionStatus::ExecutionFailed => {
                    println!("[info] task {} could not execute", task.id);
                    task.failed().await;
                }
                TaskConsumptionStatus::ExitFailed => {
                    println!("[info] task {} failured in process", task.id);
                    task.failed().await;
                }
                TaskConsumptionStatus::ExitSucceeded => {
                    println!("[info] task {} successfully terminated", task.id);
                    task.succeeded().await;
                }
                TaskConsumptionStatus::NotConsumed => {
                    println!("[info] task {} is skipped", task.id)
                }
            }
        }
    }
}
