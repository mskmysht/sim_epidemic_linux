pub mod server;
pub mod worker_client;

use std::{collections::BTreeMap, error::Error, fmt::Display, net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use tokio::{
    sync::{mpsc, RwLock},
    task::JoinHandle,
};
use tokio_postgres::{Client, NoTls};
use uuid::Uuid;

use crate::{
    api_server::{
        job::{self, JobState},
        task::TaskState,
        ResourceManager,
    },
    management::worker_client::WorkerManager,
};

use self::{
    job_impl::JobInner,
    server::ServerInfo,
    worker_client::{Task, TaskConsumptionStatus, TaskId},
};

#[derive(Clone, Debug)]
struct Job {
    id: JobId,
    inner: Arc<RwLock<JobInner>>,
}

impl Job {
    fn new(id: JobId, config: job::Config, state: JobState, tasks: Vec<Task>) -> Self {
        Self {
            id,
            inner: Arc::new(RwLock::new(JobInner::new(config, state, tasks))),
        }
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
        // job.task_termination_tx = None;
        println!("[info] job {} terminated", self.id);
    }

    // async fn notify_task_consumption(
    //     &self,
    //     task: Task,
    //     status: TaskConsumptionStatus,
    // ) -> Result<(), mpsc::error::SendError<(Task, TaskConsumptionStatus)>> {
    //     // let job = self.inner.read().await;
    //     // job.task_termination_tx
    //     self.task_termination_tx
    //         .read()
    //         .await
    //         .as_ref()
    //         .unwrap()
    //         .send((task, status))
    // }

    async fn make_obj(&self) -> job::Job {
        let job = self.inner.read().await;
        let mut tasks = BTreeMap::new();
        for task in &job.tasks {
            tasks.insert(task.id.to_string(), task.make_obj().await);
        }
        job::Job::new(
            self.id.to_string(),
            job.config.clone(),
            job.state.clone(),
            tasks,
        )
    }
}

mod job_impl {
    use super::worker_client::Task;
    use crate::api_server::job::{self, JobState};

    #[derive(Debug)]
    pub struct JobInner {
        pub config: job::Config,
        pub state: job::JobState,
        forced_termination: bool,
        pub tasks: Vec<Task>,
    }

    impl JobInner {
        pub fn new(config: job::Config, state: JobState, tasks: Vec<Task>) -> Self {
            Self {
                config,
                state,
                tasks,
                forced_termination: false,
                // task_termination_tx: None,
            }
        }

        pub async fn schedule(&mut self) -> Option<&Vec<Task>> {
            if self.forced_termination {
                self.state = JobState::Completed;
                return None;
            }

            let task_count = self.tasks.len();
            if task_count == 0 {
                self.state = JobState::Completed;
                return None;
            }
            self.state = JobState::Scheduled;

            // [todo] make states of all of tasks assigned
            // for task in &self.tasks {
            //     task.asigned().await;
            // }
            Some(&self.tasks)
        }

        pub async fn force_to_terminate(&mut self) -> Option<bool> {
            match self.state {
                JobState::Running => {
                    self.forced_termination = true;
                    for task in &self.tasks {
                        task.force_to_terminate().await;
                    }
                    Some(true)
                }
                JobState::Created | JobState::Queued | JobState::Scheduled => {
                    self.forced_termination = true;
                    Some(true)
                }
                JobState::Completed => Some(false),
            }
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug, PartialOrd, Ord)]
struct JobId(Uuid);

impl TryFrom<&str> for JobId {
    type Error = uuid::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(JobId(Uuid::try_from(value)?))
    }
}

impl Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

type TaskTable = BTreeMap<TaskId, Task>;
type TaskTableRef = Arc<RwLock<TaskTable>>;

struct JobManager {
    table: Arc<RwLock<BTreeMap<JobId, Job>>>,
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

    async fn new_job(
        &self,
        client: &Client,
        config: job::Config,
    ) -> Result<String, tokio_postgres::Error> {
        let task_count = config.iteration_count;
        let state = JobState::Created;
        let rows = client
            .query(
                "
                INSERT INTO job (id, state) VALUES (DEFAULT, $1) RETURNING id
                ",
                &[&state],
            )
            .await?;
        let id = JobId(rows[0].get(0));
        let mut tasks = Vec::new();
        let statement = client
            .prepare(
                "
                INSERT INTO task (id, job_id, state) VALUES (DEFAULT, $1, $2)
                RETURNING id",
            )
            .await?;

        let mut tt = self.task_table.write().await;
        let (tx, mut rx) = mpsc::unbounded_channel();

        for _ in 0..task_count {
            let rows = client
                .query(&statement, &[&id.0, &TaskState::default()])
                .await?;
            let task_id = TaskId(rows[0].get(0));
            let task = Task::new(task_id.clone(), tx.clone());
            tt.insert(task_id, task.clone());
            tasks.push(task);
        }
        drop(tt);

        let job = Job::new(id.clone(), config.clone(), state, tasks);
        let mut table = self.table.write().await;
        table.insert(id.clone(), job.clone());
        drop(table);

        let job_clone = job.clone();
        tokio::spawn(async move {
            for _ in 0..task_count {
                let Some((task, status)) = rx.recv().await else {
                    break;
                };
                match status {
                    TaskConsumptionStatus::ExecutionFailed => {
                        println!("[info] task {} could not execute", task.id);
                        task.update_state(TaskState::Failed).await;
                    }
                    TaskConsumptionStatus::ExitFailed => {
                        println!("[info] task {} failured in process", task.id);
                        task.update_state(TaskState::Failed).await;
                    }
                    TaskConsumptionStatus::ExitSucceeded => {
                        println!("[info] task {} successfully terminated", task.id);
                        task.update_state(TaskState::Succeeded).await;
                    }
                    TaskConsumptionStatus::NotConsumed => {
                        println!("[info] task {} is skipped", task.id)
                    }
                }
            }
            job_clone.completed().await;
        });

        let tx = self.job_request_tx.clone();
        tokio::spawn(async move {
            job.queued().await;
            tx.send((job, config)).await.unwrap();
        });

        Ok(id.to_string())
    }

    async fn get_job(&self, id: &str) -> Option<job::Job> {
        let id = id.try_into().ok()?;
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
    async fn terminate_job(&self, id: &str) -> Option<bool> {
        let id = id.try_into().ok()?;
        let table = self.table.read().await;
        let mut job = table.get(&id)?.inner.write().await;
        job.force_to_terminate().await
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
                println!("[info] received job {}", job.id);
                let mut job_mut = job.inner.write().await;
                let Some(tasks) = job_mut.schedule().await else {
                    break;
                };
                let mut worker_manager = self.worker_manager.clone();
                let tasks = tasks.clone();
                drop(job_mut);
                tokio::spawn(async move {
                    for (i, task) in tasks.into_iter().enumerate() {
                        match task.consume(config.clone(), &mut worker_manager).await {
                            Some(true) => {
                                if i == 0 {
                                    job.running().await;
                                }
                            }
                            Some(false) => {
                                task.notify_consumption(TaskConsumptionStatus::ExecutionFailed)
                            }
                            None => task.notify_consumption(TaskConsumptionStatus::NotConsumed),
                        }
                    }
                });
            }
        })
    }
}

pub struct Manager {
    job_manager: JobManager,
    sql_client: Client,
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

        let (sql_client, connection) =
            tokio_postgres::connect("host=localhost user=simepi password=simepi", NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                println!("[error] Postresql database connection error: {e}");
            }
        });
        println!("[info] created job manager");
        Ok(Self {
            job_manager,
            sql_client,
        })
    }
}

#[async_trait]
impl ResourceManager for Manager {
    async fn create_job(&self, config: job::Config) -> Option<String> {
        match self
            .job_manager
            .new_job(&self.sql_client, config.clone())
            .await
        {
            Ok(id) => Some(id),
            Err(e) => {
                println!("[error] {e}");
                None
            }
        }
    }

    async fn get_job(&self, id: &str) -> Option<job::Job> {
        self.job_manager.get_job(id).await
    }

    async fn get_all_jobs(&self) -> Vec<job::Job> {
        self.job_manager.get_all_jobs().await
    }

    async fn terminate_job(&self, id: &str) -> Option<bool> {
        self.job_manager.terminate_job(id).await
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
