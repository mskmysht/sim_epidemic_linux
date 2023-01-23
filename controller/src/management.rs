pub mod server;
pub mod worker_client;

use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    net::SocketAddr,
    sync::Arc,
};

use async_trait::async_trait;
use futures_util::future::try_join_all;
use tokio::sync::{mpsc, RwLock};
use ulid::Ulid;

use crate::api_server::{
    job::{self, JobState},
    task::{self, TaskState},
    ResourceManager,
};

use self::{server::ServerInfo, worker_client::WorkerClient};

#[derive(Clone, Debug, Default)]
pub struct Task {
    state: task::TaskState,
}

impl Task {
    fn make_obj(&self, id: String) -> task::Task {
        task::Task::new(id, self.state.clone())
    }
}

struct Job {
    config: job::Config,
    state: job::JobState,
    tasks: TaskTable,
    remained_count: usize,
    succeeded_count: usize,
    failed_count: usize,
}

impl Job {
    fn new(config: job::Config) -> Self {
        let tasks = (0..config.iteration_count)
            .map(|_| (Ulid::new().to_string(), Task::default()))
            .collect();
        let remained_count = config.iteration_count as usize;
        Self {
            config,
            state: job::JobState::default(),
            tasks,
            remained_count,
            succeeded_count: 0,
            failed_count: 0,
        }
    }

    fn make_obj(&self, id: String) -> job::Job {
        job::Job::new(
            id,
            self.config.clone(),
            self.state.clone(),
            self.tasks
                .iter()
                .map(|(id, task)| (id.clone(), task.make_obj(id.clone())))
                .collect(),
        )
    }
}

type JobId = String;
type JobTable = HashMap<JobId, Job>;
type TaskId = String;
type TaskTable = HashMap<TaskId, Task>;
pub struct JobManager {
    job_table: Arc<RwLock<JobTable>>,
    job_request_tx: mpsc::Sender<JobId>,
}

impl JobManager {
    pub async fn new(
        addr: SocketAddr,
        cert_path: String,
        servers: Vec<ServerInfo>,
        max_job_request: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let config = quic_config::get_client_config(&cert_path)?;
        let (pool_tx, mut pool_rx) = mpsc::channel(servers.len());
        let (status_tx, mut status_rx) = mpsc::channel(servers.len());
        let workers = try_join_all(servers.into_iter().enumerate().map(|(i, server_info)| {
            WorkerClient::new(
                addr.clone(),
                config.clone(),
                server_info,
                i,
                pool_tx.clone(),
                status_tx.clone(),
            )
        }))
        .await?;
        let workers = Arc::new(RwLock::new(workers));

        let job_table = Default::default();
        let (job_request_tx, mut job_request_rx) = mpsc::channel(max_job_request);
        let this = Self {
            job_table,
            job_request_tx,
        };

        let job_table = Arc::clone(&this.job_table);
        tokio::spawn(async move {
            while let Some(job_id) = job_request_rx.recv().await {
                let mut table = job_table.write().await;
                if let Some(job) = table.get_mut(&job_id) {
                    job.state = JobState::Running;
                    for (task_id, task) in job.tasks.iter_mut() {
                        task.state = TaskState::Running;
                        if let Some(index) = pool_rx.recv().await {
                            let mut ws = workers.write().await;
                            ws[index].run(job_id.clone(), task_id.clone()).await;
                        }
                    }
                }
            }
        });

        let job_table = Arc::clone(&this.job_table);
        tokio::spawn(async move {
            while let Some((ref job_id, ref task_id, succeeded)) = status_rx.recv().await {
                let mut jt = job_table.write().await;
                let job = jt.get_mut(job_id).unwrap();
                let task = job.tasks.get_mut(task_id).unwrap();
                job.remained_count -= 1;
                if succeeded {
                    task.state = TaskState::Succeeded;
                    job.succeeded_count += 1;
                } else {
                    task.state = TaskState::Failed;
                    job.failed_count += 1;
                }
                if job.remained_count == 0 {
                    if job.failed_count == 0 {
                        job.state = JobState::Succeeded;
                    } else {
                        job.state = JobState::Failed;
                    }
                }
            }
        });

        Ok(this)
    }
}

#[async_trait]
impl ResourceManager for JobManager {
    async fn create_job(&self, config: job::Config) -> Option<String> {
        let job_id = Ulid::new().to_string();
        let mut table = self.job_table.write().await;
        if let Entry::Vacant(e) = table.entry(job_id.clone()) {
            e.insert(Job::new(config));
            self.job_request_tx.send(job_id.clone()).await.unwrap();
        }
        Some(job_id)
    }

    async fn get_job(&self, id: &String) -> Option<job::Job> {
        let db = self.job_table.read().await;
        db.get(id).map(|job| job.make_obj(id.clone()))
    }

    async fn get_all_jobs(&self) -> Vec<job::Job> {
        self.job_table
            .read()
            .await
            .iter()
            .map(|(id, job)| job.make_obj(id.clone()))
            .collect()
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
