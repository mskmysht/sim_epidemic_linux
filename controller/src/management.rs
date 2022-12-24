pub mod server;
pub mod worker_client;

use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    net::SocketAddr,
    sync::Arc,
};

use async_trait::async_trait;
use parking_lot::RwLock;
use ulid::Ulid;

use crate::api_server::{job, task, ResourceManagerInterface};

use self::{server::ServerInfo, worker_client::WorkerClient};

#[derive(Default)]
struct Task {
    state: task::TaskState,
}

impl Task {
    fn make_obj(&self, id: u64) -> task::Task {
        task::Task::new(id, self.state.clone())
    }
}

struct Job {
    config: job::Config,
    state: job::JobState,
    tasks: Arc<RwLock<HashMap<u64, Task>>>,
}

impl Job {
    fn new(config: job::Config) -> Self {
        let mut tasks = HashMap::default();
        for i in 0..config.iteration_count {
            tasks.insert(i, Task::default());
        }

        Self {
            config,
            state: job::JobState::default(),
            tasks: Arc::new(RwLock::new(tasks)),
        }
    }

    fn make_obj(&self, id: String) -> job::Job {
        job::Job::new(
            id,
            self.config.clone(),
            self.state.clone(),
            self.make_task_map(),
        )
    }

    fn make_task_map(&self) -> HashMap<String, task::Task> {
        self.tasks
            .read()
            .iter()
            .map(|(id, task)| (id.to_string(), task.make_obj(*id)))
            .collect()
    }
}

#[derive(Default)]
pub struct ResourceManager {
    jobs: Arc<RwLock<HashMap<String, Job>>>,
    worker_clients: Vec<WorkerClient>,
}

impl ResourceManager {
    pub async fn new(
        addr: SocketAddr,
        cert_path: String,
        servers: Vec<ServerInfo>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut manager = Self::default();
        let config = quic_config::get_client_config(&cert_path)?;
        for (i, server_info) in servers.into_iter().enumerate() {
            manager
                .worker_clients
                .push(WorkerClient::new(addr.clone(), config.clone(), server_info, i).await?)
        }
        Ok(manager)
    }
}

#[async_trait]
impl ResourceManagerInterface for ResourceManager {
    fn create_job(&self, config: job::Config) -> Option<String> {
        let id = Ulid::new().to_string();
        let mut table = self.jobs.write();
        if let Entry::Vacant(e) = table.entry(id.clone()) {
            e.insert(Job::new(config));
        }
        Some(id)
    }

    fn get_job(&self, id: &String) -> Option<job::Job> {
        let db = self.jobs.read();
        db.get(id).map(|job| job.make_obj(id.clone()))
    }

    fn get_all_jobs(&self) -> Vec<job::Job> {
        self.jobs
            .read()
            .iter()
            .map(|(id, job)| job.make_obj(id.clone()))
            .collect()
    }
}
