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

#[derive(Default)]
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
    tasks: Vec<TaskId>,
}

impl Job {
    fn new(config: job::Config) -> Self {
        Self {
            config,
            state: job::JobState::default(),
            tasks: Vec::new(),
        }
    }

    fn make_obj(&self, id: String, table: &TaskTable) -> job::Job {
        job::Job::new(
            id,
            self.config.clone(),
            self.state.clone(),
            self.make_task_map(table),
        )
    }

    fn make_task_map(&self, table: &TaskTable) -> HashMap<String, task::Task> {
        self.tasks
            .iter()
            .map(|id| (id.clone(), table.get(id).unwrap().make_obj(id.clone())))
            .collect()
    }
}

type JobId = String;
type JobTable = HashMap<JobId, Job>;
type TaskId = String;
type TaskTable = HashMap<TaskId, Task>;
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
        let task_table = Default::default();
        let (job_request_tx, mut job_request_rx) = mpsc::channel(max_job_request);
        let this = Self {
            job_table,
            task_table,
            job_request_tx,
        };

        let job_table = Arc::clone(&this.job_table);
        let task_table = Arc::clone(&this.task_table);
        tokio::spawn(async move {
            while let Some(ref job_id) = job_request_rx.recv().await {
                let mut table = job_table.write().await;
                if let Some(job) = table.get_mut(job_id) {
                    job.state = JobState::Running;
                    for task_id in &job.tasks {
                        let mut table = task_table.write().await;
                        if let Some(task) = table.get_mut(task_id) {
                            task.state = TaskState::Running;
                            if let Some(index) = pool_rx.recv().await {
                                let mut ws = workers.write().await;
                                ws[index].run(task_id.clone()).await;
                            }
                        }
                    }
                }
            }
        });

        let task_table = Arc::clone(&this.task_table);
        tokio::spawn(async move {
            while let Some((task_id, b)) = status_rx.recv().await {
                let mut table = task_table.write().await;
                if let Some(task) = table.get_mut(&task_id) {
                    task.state = match b {
                        true => TaskState::Succeeded,
                        false => TaskState::Failed,
                    };
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
        let table = &self.task_table.read().await;
        let db = self.job_table.read().await;
        db.get(id).map(|job| job.make_obj(id.clone(), table))
    }

    async fn get_all_jobs(&self) -> Vec<job::Job> {
        let table = &self.task_table.read().await;
        self.job_table
            .read()
            .await
            .iter()
            .map(|(id, job)| job.make_obj(id.clone(), table))
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
