pub mod server;
pub mod worker_client;

use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    net::SocketAddr,
    sync::{Arc, Weak},
};

use async_trait::async_trait;
use tokio::sync::{mpsc, RwLock};
use ulid::Ulid;

use crate::api_server::{
    job::{self, JobState},
    task::{self, TaskState},
    ResourceManager,
};

use self::{server::ServerInfo, worker_client::WorkerClient};

#[derive(Clone, Debug)]
pub struct Task {
    state: task::TaskState,
    job: Weak<RwLock<Job>>,
    worker: Weak<RwLock<WorkerClient>>,
    forced_termination: bool,
}

impl Task {
    fn new(job: &Arc<RwLock<Job>>) -> Self {
        Self {
            state: Default::default(),
            job: Arc::downgrade(job),
            worker: Weak::new(),
            forced_termination: false,
        }
    }

    fn make_obj(&self, id: TaskId) -> task::Task {
        task::Task::new(id, self.state.clone())
    }
}

#[derive(Debug)]
struct Job {
    config: job::Config,
    state: job::JobState,
    tasks: Vec<TaskId>,
    termination_tx: Option<mpsc::UnboundedSender<(TaskId, Arc<RwLock<Task>>, Option<bool>)>>,
    forced_termination: bool,
}

impl Job {
    fn new(config: job::Config) -> Self {
        let tasks = (0..config.iteration_count)
            .map(|_| Ulid::new().to_string())
            .collect();
        Self {
            config,
            tasks,
            forced_termination: false,
            state: Default::default(),
            termination_tx: Default::default(),
        }
    }

    async fn make_obj(&self, id: String, task_table: &TaskTable) -> job::Job {
        let mut tasks = HashMap::new();
        for id in &self.tasks {
            let task = task_table[id].read().await;
            tasks.insert(id.clone(), task.make_obj(id.clone()));
        }
        job::Job::new(id, self.config.clone(), self.state.clone(), tasks)
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

#[derive(Clone)]
struct WorkerManager {
    pool_rx: async_channel::Receiver<usize>,
    workers: Arc<Vec<Arc<RwLock<WorkerClient>>>>,
}

impl WorkerManager {
    async fn new(
        addr: SocketAddr,
        cert_path: String,
        servers: Vec<ServerInfo>,
    ) -> Result<(Self, mpsc::UnboundedReceiver<(String, bool)>), Box<dyn Error>> {
        let config = quic_config::get_client_config(&cert_path)?;
        let (pool_tx, pool_rx) = async_channel::bounded(servers.len());
        let (termination_tx, termination_rx) = mpsc::unbounded_channel();
        let mut workers = Vec::new();
        for (i, server_info) in servers.into_iter().enumerate() {
            workers.push(Arc::new(RwLock::new(
                WorkerClient::new(
                    addr.clone(),
                    config.clone(),
                    server_info,
                    i,
                    pool_tx.clone(),
                    termination_tx.clone(),
                )
                .await?,
            )));
        }
        Ok((
            Self {
                workers: Arc::new(workers),
                pool_rx,
            },
            termination_rx,
        ))
    }

    async fn get(&mut self) -> Result<Arc<RwLock<WorkerClient>>, async_channel::RecvError> {
        let index = self.pool_rx.recv().await?;
        Ok(Arc::clone(&self.workers[index]))
    }
}

type JobTable = HashMap<JobId, Arc<RwLock<Job>>>;
type TaskId = String;
type TaskTable = HashMap<TaskId, Arc<RwLock<Task>>>;
pub struct JobManager {
    job_table: Arc<RwLock<JobTable>>,
    task_table: Arc<RwLock<TaskTable>>,
    job_request_tx: mpsc::Sender<JobId>,
}

impl JobManager {
    pub async fn new(
        addr: SocketAddr,
        cert_path: String,
        servers: Vec<ServerInfo>,
        max_job_request: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let (worker_manager, termination_rx) = WorkerManager::new(addr, cert_path, servers).await?;
        let job_table = Default::default();
        let (job_request_tx, job_request_rx) = mpsc::channel(max_job_request);
        let task_table = Default::default();

        tokio::spawn(Self::listen_job_request(
            Arc::clone(&job_table),
            Arc::clone(&task_table),
            job_request_rx,
            worker_manager,
        ));

        tokio::spawn(Self::listen_task_termination(
            Arc::clone(&task_table),
            termination_rx,
        ));

        println!("[info] created job manager.");
        Ok(Self {
            job_table,
            task_table,
            job_request_tx,
        })
    }

    async fn listen_job_request(
        job_table: Arc<RwLock<JobTable>>,
        task_table: Arc<RwLock<TaskTable>>,
        mut job_request_rx: mpsc::Receiver<JobId>,
        worker_manager: WorkerManager,
    ) {
        while let Some(job_id) = job_request_rx.recv().await {
            println!("[info] received job {}", job_id.0);
            let jt = job_table.read().await;
            let job = &jt[&job_id];
            {
                let mut job = job.write().await;
                if job.forced_termination {
                    job.state = JobState::Completed;
                    continue;
                }
            }
            let (tx, mut rx) = mpsc::unbounded_channel();
            let (tasks, config) = {
                let mut tasks = Vec::new();
                let mut job = job.write().await;
                job.state = JobState::Scheduled;
                job.termination_tx = Some(tx);

                for task_id in &job.tasks {
                    let task = &task_table.read().await[task_id];
                    {
                        let mut task = task.write().await;
                        task.state = TaskState::Assigned;
                    }
                    tasks.push((task_id.clone(), Arc::clone(task)));
                }
                (tasks, job.config.clone())
            };

            let n = tasks.len();
            {
                let job = Arc::clone(job);
                let mut worker_manager = worker_manager.clone();
                tokio::spawn(async move {
                    for (i, (task_id, task)) in tasks.into_iter().enumerate() {
                        match Self::consume_task(
                            task_id.clone(),
                            task.clone(),
                            config.clone(),
                            &mut worker_manager,
                        )
                        .await
                        {
                            Ok(Some(true)) => {
                                let mut job = job.write().await;
                                if i == 0 {
                                    job.state = JobState::Running;
                                }
                            }
                            Ok(Some(false)) | Err(_) => {
                                let job = job.read().await;
                                job.termination_tx
                                    .as_ref()
                                    .unwrap()
                                    .send((task_id, task, Some(false)))
                                    .unwrap();
                            }
                            Ok(None) => {
                                let job = job.read().await;
                                job.termination_tx
                                    .as_ref()
                                    .unwrap()
                                    .send((task_id, task, None))
                                    .unwrap();
                            }
                        }
                    }
                });
            }

            let job = Arc::clone(&job);
            tokio::spawn(async move {
                let mut i = 0;
                // let mut f = 0;
                // let mut s = 0;
                while i < n {
                    match rx.recv().await {
                        Some((task_id, task, Some(s))) => {
                            println!("[info] task {task_id} terminated");
                            let mut task = task.write().await;
                            if s {
                                task.state = TaskState::Succeeded;
                            } else {
                                task.state = TaskState::Failed;
                                // f += 1;
                            }
                        }
                        Some((task_id, _, None)) => {
                            println!("[info] task {task_id} is skipped");
                            // s += 1;
                        }
                        None => break,
                    }
                    i += 1;
                }
                let mut job = job.write().await;
                // if f > 0 || s > 0 {
                //     job.state = JobState::Failed;
                // } else {
                //     job.state = JobState::Succeeded;
                // }
                job.state = JobState::Completed;
                job.termination_tx = None;
                println!("[info] job {} terminated", job_id.0);
            });
        }
    }

    async fn consume_task(
        task_id: TaskId,
        task: Arc<RwLock<Task>>,
        config: job::Config,
        worker_manager: &mut WorkerManager,
    ) -> Result<Option<bool>, async_channel::RecvError> {
        println!("[info] received task {task_id}");
        if task.read().await.forced_termination {
            return Ok(None);
        }
        let worker = worker_manager.get().await?;
        {
            let mut task = task.write().await;
            if task.forced_termination {
                return Ok(None);
            } else {
                task.state = TaskState::Assigned;
                task.worker = Arc::downgrade(&worker);
            }
        }
        let succeeded = tokio::spawn(async move {
            let mut task = task.write().await;
            let mut worker = worker.write().await;
            if worker.execute(&task_id, config).await.is_ok() {
                task.state = TaskState::Running;
                true
            } else {
                task.state = TaskState::Failed;
                false
            }
        })
        .await
        .unwrap();
        Ok(Some(succeeded))
    }

    async fn listen_task_termination(
        task_table: Arc<RwLock<TaskTable>>,
        mut termination_rx: mpsc::UnboundedReceiver<(String, bool)>,
    ) {
        while let Some((id, succeeded)) = termination_rx.recv().await {
            let task_id: TaskId = id;
            let task = &task_table.read().await[&task_id];
            let job = task.read().await.job.upgrade().unwrap();
            job.read()
                .await
                .termination_tx
                .as_ref()
                .unwrap()
                .send((task_id.clone(), Arc::clone(task), Some(succeeded)))
                .unwrap();
        }
    }

    async fn create_job(&self, config: job::Config) -> anyhow::Result<Option<String>> {
        let job_id = JobId::new();
        let mut jt = self.job_table.write().await;
        if let Entry::Vacant(e) = jt.entry(job_id.clone()) {
            let job = Arc::new(RwLock::new(Job::new(config)));
            let mut tt = self.task_table.write().await;
            for task_id in &job.read().await.tasks {
                tt.insert(task_id.clone(), Arc::new(RwLock::new(Task::new(&job))));
            }
            e.insert(job);
            self.job_request_tx.send(job_id.clone()).await?;
            Ok(Some(job_id.0))
        } else {
            Ok(None)
        }
    }

    async fn terminate_job(&self, id: &str) -> Option<bool> {
        let jt = self.job_table.read().await;
        let job = jt.get(&id.into())?;
        let tt = self.task_table.read().await;
        let mut job = job.write().await;
        match job.state {
            JobState::Running => {
                job.forced_termination = true;
                for task_id in &job.tasks {
                    let task = &tt[task_id];
                    let mut task = task.write().await;
                    task.forced_termination = true;
                    if matches!(task.state, TaskState::Running) {
                        let worker = task.worker.upgrade().unwrap();
                        let mut worker = worker.write().await;
                        if let Err(e) = worker.terminate(task_id).await {
                            eprintln!("[error] {e}");
                        }
                    }
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
}

#[async_trait]
impl ResourceManager for JobManager {
    async fn create_job(&self, config: job::Config) -> anyhow::Result<Option<String>> {
        self.create_job(config).await
    }

    async fn get_job(&self, id: &str) -> Option<job::Job> {
        let db = self.job_table.read().await;
        let id = id.into();
        let job = db.get(&id)?.read().await;
        let table = self.task_table.read().await;
        Some(job.make_obj(id.0, &table).await)
    }

    async fn get_all_jobs(&self) -> Vec<job::Job> {
        let mut jobs = Vec::new();
        let jt = self.job_table.read().await;
        let tt = self.task_table.read().await;
        for (id, job) in jt.iter() {
            let job = job.read().await;
            jobs.push(job.make_obj(id.0.to_string(), &tt).await)
        }
        jobs
    }

    async fn terminate_job(&self, id: &str) -> Option<bool> {
        self.terminate_job(id).await
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
