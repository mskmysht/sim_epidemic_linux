pub mod server;
pub mod worker_client;

use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    fmt::Display,
    net::SocketAddr,
    sync::Arc,
};

use async_trait::async_trait;
use tokio::{
    sync::{mpsc, RwLock},
    task::JoinHandle,
};
use ulid::Ulid;

use crate::{
    api_server::{
        job::{self, JobState},
        ResourceManager,
    },
    management::worker_client::WorkerManager,
};

use self::{
    server::ServerInfo,
    worker_client::{Task, TaskConsumer, TaskConsumptionStatus, TaskId, TaskListener},
};

#[derive(Clone, Debug)]
pub struct Job {
    id: JobId,
    inner: Arc<RwLock<JobInner>>,
}

impl Job {
    async fn new(id: JobId, config: job::Config, task_table: &TaskTableRef) -> Self {
        let iter_count = config.iteration_count;
        let job = Self {
            id,
            inner: Arc::new(RwLock::new(JobInner::new(config, Vec::new()))),
        };
        {
            let mut job_mut = job.inner.write().await;
            let mut tt = task_table.write().await;
            for _ in 0..iter_count {
                let task_id = Ulid::new().to_string();
                let task = Task::new(task_id.clone(), job.clone());
                job_mut.tasks.push(task.clone());
                tt.insert(task_id, task);
            }
        }
        job
    }

    async fn queued(&self) {
        let mut job = self.inner.write().await;
        job.state = JobState::Queued;
    }

    async fn running(&self) {
        let mut job = self.inner.write().await;
        job.state = JobState::Running;
    }

    async fn completed(&self) {
        let mut job = self.inner.write().await;
        job.state = JobState::Completed;
        job.task_termination_tx = None;
        println!("[info] job {} terminated", self.id);
    }

    /// Returns false if job is forced to terminate.
    async fn schedule(self, config: job::Config, worker_manager: &WorkerManager) -> bool {
        // println!("[info] received job {}", job.0);
        let mut job = self.inner.write().await;
        if job.forced_termination {
            job.state = JobState::Completed;
            return false;
        }

        let task_count = job.tasks.len();
        if task_count == 0 {
            self.completed().await;
        }

        let (tx, rx) = mpsc::unbounded_channel();
        job.state = JobState::Scheduled;
        job.task_termination_tx = Some(tx);

        let mut tasks = Vec::new();
        for task in &job.tasks {
            task.asigned().await;
            tasks.push(task.clone());
        }
        drop(job);

        let task_listener = TaskListener::new(rx, task_count);
        tokio::spawn(
            TaskConsumer::new(self.clone(), tasks, config, worker_manager.clone()).consume(),
        );
        tokio::spawn(async move {
            task_listener.listen().await;
            self.completed().await;
        });
        true
    }

    async fn force_to_terminate(&self) -> Option<bool> {
        let mut job = self.inner.write().await;
        match job.state {
            JobState::Running => {
                job.forced_termination = true;
                for task in &job.tasks {
                    task.force_to_terminate().await;
                }
                Some(true)
            }
            JobState::Created | JobState::Queued | JobState::Scheduled => {
                job.forced_termination = true;
                Some(true)
            }
            JobState::Completed => Some(false),
        }
    }
    async fn notify_task_consumption(
        &self,
        task: Task,
        status: TaskConsumptionStatus,
    ) -> Result<(), mpsc::error::SendError<(Task, TaskConsumptionStatus)>> {
        let job = self.inner.read().await;
        job.task_termination_tx
            .as_ref()
            .unwrap()
            .send((task, status))
    }

    async fn make_obj(&self) -> job::Job {
        let job = self.inner.read().await;
        let mut tasks = HashMap::new();
        for task in &job.tasks {
            tasks.insert(task.id.clone(), task.make_obj().await);
        }
        job::Job::new(
            self.id.0.clone(),
            job.config.clone(),
            job.state.clone(),
            tasks,
        )
    }
}

#[derive(Debug)]
struct JobInner {
    config: job::Config,
    state: job::JobState,
    forced_termination: bool,
    tasks: Vec<Task>,
    task_termination_tx: Option<mpsc::UnboundedSender<(Task, TaskConsumptionStatus)>>,
}

impl JobInner {
    fn new(config: job::Config, tasks: Vec<Task>) -> Self {
        Self {
            config,
            tasks,
            state: Default::default(),
            forced_termination: false,
            task_termination_tx: None,
        }
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Debug)]
struct JobId(pub String);

impl JobId {
    fn new() -> Self {
        Self(Ulid::new().to_string())
    }
}

impl From<&str> for JobId {
    fn from(value: &str) -> Self {
        JobId(value.to_string())
    }
}

impl Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

struct JobManager {
    table: Arc<RwLock<HashMap<JobId, Job>>>,
    task_table: TaskTableRef,
    job_request_tx: mpsc::Sender<(Job, job::Config)>,
}

impl JobManager {
    fn new(job_request_tx: mpsc::Sender<(Job, job::Config)>) -> Self {
        Self {
            table: Default::default(),
            task_table: Default::default(),
            job_request_tx,
        }
    }

    fn clone_task_table(&self) -> TaskTableRef {
        Arc::clone(&self.task_table)
    }

    async fn new_job(&self, config: job::Config) -> Option<String> {
        let id = JobId::new();
        let mut table = self.table.write().await;
        let Entry::Vacant(e) = table.entry(id.clone()) else {
            return None;
        };
        let job = Job::new(id.clone(), config.clone(), &self.task_table).await;
        e.insert(job.clone());
        let tx = self.job_request_tx.clone();
        tokio::spawn(async move {
            job.queued().await;
            tx.send((job, config)).await.unwrap();
        });
        Some(id.0)
    }

    async fn get_job(&self, id: JobId) -> Option<job::Job> {
        let table = self.table.read().await;
        let job = table.get(&id)?;
        Some(job.make_obj().await)
    }

    async fn get_all_jobs(&self) -> Vec<job::Job> {
        let mut jobs = Vec::new();
        let table = self.table.read().await;
        for job in table.values() {
            jobs.push(job.make_obj().await)
        }
        jobs
    }
    async fn terminate_job(&self, id: &JobId) -> Option<bool> {
        let table = self.table.read().await;
        table.get(id)?.force_to_terminate().await
    }
}

struct JobScheduler {
    job_request_rx: mpsc::Receiver<(Job, job::Config)>,
    worker_manager: WorkerManager,
}

impl JobScheduler {
    fn new(
        job_request_rx: mpsc::Receiver<(Job, job::Config)>,
        worker_manager: WorkerManager,
    ) -> Self {
        Self {
            job_request_rx,
            worker_manager,
        }
    }
    fn spawn(mut self) -> JoinHandle<()> {
        tokio::spawn(async move {
            while let Some((job, config)) = self.job_request_rx.recv().await {
                if !job.schedule(config, &self.worker_manager).await {
                    continue;
                }
            }
        })
    }
}

type TaskTable = HashMap<TaskId, Task>;
type TaskTableRef = Arc<RwLock<TaskTable>>;

pub struct Manager {
    job_manager: JobManager,
}

impl Manager {
    pub async fn new(
        addr: SocketAddr,
        cert_path: String,
        servers: Vec<ServerInfo>,
        max_job_request: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let (job_request_tx, job_request_rx) = mpsc::channel(max_job_request);
        let job_manager = JobManager::new(job_request_tx);
        let worker_manager =
            WorkerManager::new(addr, cert_path, servers, job_manager.clone_task_table()).await?;

        JobScheduler::new(job_request_rx, worker_manager).spawn();

        println!("[info] created job manager.");
        Ok(Self { job_manager })
    }
}

#[async_trait]
impl ResourceManager for Manager {
    async fn create_job(&self, config: job::Config) -> Option<String> {
        self.job_manager.new_job(config.clone()).await
    }

    async fn get_job(&self, id: &str) -> Option<job::Job> {
        self.job_manager.get_job(id.into()).await
    }

    async fn get_all_jobs(&self) -> Vec<job::Job> {
        self.job_manager.get_all_jobs().await
    }

    async fn terminate_job(&self, id: &str) -> Option<bool> {
        self.job_manager.terminate_job(&id.into()).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Mutex;

    #[test]
    fn test_nest() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(nest_async());
    }

    struct Hoge;
    impl Hoge {
        async fn run(&self, i: usize) -> usize {
            i + 1
        }
    }

    async fn nest_async() {
        let arr = Arc::new(Mutex::new([0usize; 5]));
        let hoge = Hoge;
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        tokio::spawn(async move {
            while let Some(i) = rx.recv().await {
                let mut arr = arr.lock().await;
                let v = &mut arr[i];
                *v = hoge.run(i).await;
                println!("{}: {:?}", i, arr);
            }
            println!("{:?}", arr.lock().await);
        });

        for i in 0..5 {
            tx.send(i).await.unwrap();
        }
    }
}
