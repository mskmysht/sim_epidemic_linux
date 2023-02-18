pub mod server;
pub mod worker_client;

use std::{
    collections::HashMap, error::Error, fmt::Display, net::SocketAddr, sync::Arc, thread,
    time::Duration,
};

use async_trait::async_trait;
use futures_util::future::join_all;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_postgres::{Client, NoTls};
use uuid::Uuid;

use crate::{
    api_server::{
        job::{self, JobState},
        task::{self, TaskState},
        ResourceManager,
    },
    management::worker_client::{WorkerLease, WorkerManager},
};

use self::{server::ServerInfo, worker_client::Worker};

pub type WorkerTableRef = Arc<RwLock<HashMap<TaskId, Worker>>>;
pub type TaskSenderTableRef = Arc<RwLock<HashMap<TaskId, oneshot::Sender<bool>>>>;
type JobTableRef = Arc<RwLock<HashMap<JobId, Job>>>;

#[derive(Clone, Debug)]
struct Job {
    id: JobId,
    inner: Arc<RwLock<JobInner>>,
    worker_table: WorkerTableRef,
}

impl Job {
    fn new(id: JobId, config: job::Config, state: JobState) -> Self {
        Self {
            worker_table: Default::default(),
            id,
            inner: Arc::new(RwLock::new(JobInner::new(config, state))),
        }
    }

    async fn is_foreced_termination(&self) -> bool {
        self.inner.read().await.forced_termination
    }

    async fn update_state(&self, state: JobState, db: &Db) {
        db.update_job_state(&self.id, &state).await;
        let mut job = self.inner.write().await;
        job.state = state;
    }

    async fn force_to_terminate(&self) -> bool {
        let mut inner = self.inner.write().await;
        match inner.state {
            JobState::Running => {
                inner.forced_termination = true;
                for (task_id, worker) in self.worker_table.read().await.iter() {
                    if worker.terminate(task_id).await.is_err() {
                        println!("[info] {task_id} is already terminated");
                    }
                }
                self.worker_table.write().await.clear();
                true
            }
            JobState::Created | JobState::Queued | JobState::Scheduled => {
                inner.forced_termination = true;
                true
            }
            JobState::Completed => false,
        }
    }
}

#[derive(Debug)]
struct JobInner {
    config: job::Config,
    state: job::JobState,
    forced_termination: bool,
}

impl JobInner {
    fn new(config: job::Config, state: JobState) -> Self {
        Self {
            config,
            state,
            forced_termination: false,
        }
    }
}
#[derive(PartialEq, Eq, Clone, Debug, Hash)]
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

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
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

async fn execute_task(
    id: &TaskId,
    config: job::Config,
    rx: oneshot::Receiver<bool>,
    worker: &Worker,
    worker_table: WorkerTableRef,
    db: &Db,
) -> bool {
    db.update_task_state(id, &TaskState::Assigned).await;

    if worker.execute(id, config).await {
        db.update_task_state(id, &TaskState::Running).await;
        println!("[debug] {id} is running");
        worker_table
            .write()
            .await
            .insert(id.clone(), worker.clone());
        println!("[debug] worker is registered");
    } else {
        db.update_task_state(id, &TaskState::Failed).await;
        println!("[info] task {} could not execute", id);
        return false;
    }

    if rx.await.unwrap() {
        db.update_task_state(id, &TaskState::Succeeded).await;
        println!("[info] task {} successfully terminated", id);
    } else {
        db.update_task_state(id, &TaskState::Failed).await;
        println!("[info] task {} failured in process", id);
    }
    worker_table.write().await.remove(id);
    println!("[debug] worker is removed");
    return true;
}

#[derive(Debug)]
struct JobQueued {
    job: Job,
    task_rxs: Vec<(TaskId, oneshot::Receiver<bool>)>,
    config: job::Config,
}

impl JobQueued {
    async fn dequeue(self, worker_manager: &Arc<RwLock<WorkerManager>>, db: &Db) {
        let job = self.job;

        if job.is_foreced_termination().await {
            job.update_state(JobState::Completed, db).await;
            return;
        }

        job.update_state(JobState::Scheduled, db).await;

        worker_manager.write().await.wait().await;
        job.update_state(JobState::Running, db).await;

        let mut handles = Vec::new();
        for (task_id, rx) in self.task_rxs {
            if job.is_foreced_termination().await {
                break;
            }
            let worker_lease = WorkerLease::new(worker_manager.clone());
            let worker_table = Arc::clone(&job.worker_table);
            let config = self.config.clone();
            let job = job.clone();
            let db = db.clone();
            handles.push(worker_lease.lease(|worker| async move {
                thread::sleep(Duration::from_secs(1));
                println!("[info] received task {}", task_id);
                if job.is_foreced_termination().await {
                    println!("[info] task {} is skipped", task_id);
                    return false;
                }
                execute_task(&task_id, config, rx, &worker, worker_table, &db).await
            }));
        }
        join_all(handles).await;
        println!("---");
        job.update_state(JobState::Completed, db).await;
    }
}

#[derive(Clone)]
struct Db(Arc<Client>);

impl Db {
    async fn insert(
        &self,
        state: &JobState,
        task_count: u64,
    ) -> Result<(JobId, Vec<TaskId>), tokio_postgres::Error> {
        let rows = self
            .0
            .query(
                "
                INSERT INTO job (id, state) VALUES (DEFAULT, $1) RETURNING id
                ",
                &[&state],
            )
            .await?;
        let job_id = JobId(rows[0].get(0));
        let statement = self
            .0
            .prepare(
                "
                INSERT INTO task (id, job_id, state) VALUES (DEFAULT, $1, $2)
                RETURNING id",
            )
            .await?;

        let mut task_ids = Vec::new();
        for _ in 0..task_count {
            let rows = self
                .0
                .query(&statement, &[&job_id.0, &TaskState::default()])
                .await?;
            task_ids.push(TaskId(rows[0].get(0)));
        }
        Ok((job_id, task_ids))
    }

    async fn update_task_state(&self, task_id: &TaskId, state: &TaskState) {
        self.0
            .execute(
                "UPDATE task SET state = $1 WHERE id = $2",
                &[state, &task_id.0],
            )
            .await
            .unwrap();
    }

    async fn update_job_state(&self, job_id: &JobId, state: &JobState) {
        self.0
            .execute(
                "UPDATE job SET state = $1 WHERE id = $2",
                &[state, &job_id.0],
            )
            .await
            .unwrap();
    }

    async fn get_tasks(&self, job_id: &JobId) -> Vec<task::Task> {
        let mut tasks = Vec::new();
        for r in self
            .0
            .query("SELECT id, state FROM task WHERE job_id = $1", &[&job_id.0])
            .await
            .unwrap()
        {
            let id: Uuid = r.get(0);
            let state: task::TaskState = r.get(1);
            tasks.push(task::Task {
                id: id.to_string(),
                state,
            })
        }
        tasks
    }
}

pub struct Manager {
    job_queue_tx: mpsc::Sender<JobQueued>,
    job_table: JobTableRef,
    task_sender_table: TaskSenderTableRef,
    db: Db,
}

impl Manager {
    pub async fn new(
        addr: SocketAddr,
        cert_path: String,
        servers: Vec<ServerInfo>,
        max_job_request: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let (client, connection) =
            tokio_postgres::connect("host=localhost user=simepi password=simepi", NoTls).await?;
        let db = Db(Arc::new(client));
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                println!("[error] Postresql database connection error: {e}");
            }
        });

        let (job_queue_tx, mut job_queue_rx) = mpsc::channel::<JobQueued>(max_job_request);
        let task_sender_table = Default::default();
        let worker_manager = Arc::new(RwLock::new(
            WorkerManager::new(addr, cert_path, servers, Arc::clone(&task_sender_table)).await?,
        ));

        let db_clone = db.clone();
        tokio::spawn(async move {
            while let Some(job_queued) = job_queue_rx.recv().await {
                let id = job_queued.job.id.clone();
                println!("[info] received job {}", id);
                job_queued.dequeue(&worker_manager, &db_clone).await;
                println!("[info] job {} terminated", id);
            }
        });

        println!("[info] created job manager");
        Ok(Self {
            job_queue_tx,
            job_table: Default::default(),
            task_sender_table,
            db,
        })
    }

    async fn create_job(&self, config: job::Config) -> Result<String, tokio_postgres::Error> {
        let task_count = config.iteration_count;
        let state = JobState::Created;
        let (job_id, task_ids) = self.db.insert(&state, task_count).await?;

        let mut task_rxs = Vec::new();
        let mut task_sender_table = self.task_sender_table.write().await;
        for task_id in task_ids {
            let (tx, rx) = oneshot::channel();
            task_sender_table.insert(task_id.clone(), tx);
            task_rxs.push((task_id.clone(), rx));
        }
        drop(task_sender_table);

        let job = Job::new(job_id.clone(), config.clone(), state);
        let mut job_table = self.job_table.write().await;
        job_table.insert(job_id.clone(), job.clone());
        drop(job_table);

        if task_count == 0 {
            job.update_state(JobState::Completed, &self.db).await;
        } else {
            job.update_state(JobState::Queued, &self.db).await;
            self.job_queue_tx
                .send(JobQueued {
                    job,
                    task_rxs,
                    config,
                })
                .await
                .unwrap();
        }

        Ok(job_id.to_string())
    }

    async fn make_job(&self, job: &Job) -> job::Job {
        let job_id = &job.id;
        let tasks = self
            .db
            .get_tasks(job_id)
            .await
            .into_iter()
            .map(|task| (task.id.clone(), task))
            .collect();
        let job = job.inner.read().await;
        job::Job {
            id: job_id.to_string(),
            config: job.config.clone(),
            state: job.state.clone(),
            tasks,
        }
    }

    async fn get_job(&self, id: &str) -> Option<job::Job> {
        let id = id.try_into().ok()?;
        let job_table = self.job_table.read().await;
        let job = job_table.get(&id)?;
        Some(self.make_job(job).await)
    }

    async fn get_all_jobs(&self) -> Vec<job::Job> {
        let mut jobs = Vec::new();
        let job_table = self.job_table.read().await;
        for job in job_table.values() {
            jobs.push(self.make_job(job).await)
        }
        jobs
    }
    async fn terminate_job(&self, id: &str) -> Option<bool> {
        let id = id.try_into().ok()?;
        let table = self.job_table.read().await;
        let job = table.get(&id)?;
        Some(job.force_to_terminate().await)
    }
}

#[async_trait]
impl ResourceManager for Manager {
    async fn create_job(&self, config: job::Config) -> Option<String> {
        match self.create_job(config.clone()).await {
            Ok(id) => Some(id),
            Err(e) => {
                println!("[error] {e}");
                None
            }
        }
    }

    async fn get_job(&self, id: &str) -> Option<job::Job> {
        self.get_job(id).await
    }

    async fn get_all_jobs(&self) -> Vec<job::Job> {
        self.get_all_jobs().await
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
