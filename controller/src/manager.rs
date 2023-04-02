use std::{
    collections::HashMap,
    error::Error,
    fmt::{Debug, Display},
    net::IpAddr,
    sync::Arc,
};

use async_trait::async_trait;
use poem_openapi::types::ToJSON;
use tokio::{
    select,
    sync::{mpsc, watch, Notify, RwLock},
};
use tokio_postgres::{Client, NoTls};
use uuid::Uuid;

use crate::app::{
    job::{self, JobState},
    task::{self, TaskState},
    ResourceManager,
};

use self::worker::{ServerConfig, TaskId, WorkerManager};

#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub struct JobId(Uuid);

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

fn force_quit_channel() -> (ForceQuitSender, ForceQuitReceiver) {
    let (tx, rx) = watch::channel(false);
    (ForceQuitSender(tx), ForceQuitReceiver(rx))
}

#[derive(Debug)]
struct ForceQuitSender(watch::Sender<bool>);

impl ForceQuitSender {
    fn send(&self) -> bool {
        self.0.send(true).is_ok()
    }
}

#[derive(Clone, Debug)]
struct ForceQuitReceiver(watch::Receiver<bool>);

impl ForceQuitReceiver {
    pub fn has_changed(&self) -> Result<bool, watch::error::RecvError> {
        self.0.has_changed()
    }

    async fn changed(mut self) -> Result<(), watch::error::RecvError> {
        self.0.changed().await
    }
}

#[derive(Debug)]
struct Job {
    id: JobId,
    fq_rx: ForceQuitReceiver,
    task_ids: Vec<TaskId>,
    config: job::Config,
}

impl Job {
    async fn consume(self, worker_manager: &WorkerManager, db: &Db) {
        for task_id in self.task_ids {
            let lease = worker_manager.lease((&self.config.param).into()).await;
            println!("[debug] received task {}", task_id);
            let fq_rx = self.fq_rx.clone();

            select! {
                Ok(_) = fq_rx.changed() => {
                    println!("[info] {} lease is canceled", task_id);
                }
                Ok(worker) = lease => {
                    worker
                        .execute(
                            &task_id,
                            self.config.clone(),
                            &db,
                            self.fq_rx.clone(),
                        )
                        .await;
                }
            }
        }
    }
}

#[derive(Clone)]
struct Db(Arc<Client>);

impl Db {
    async fn insert_job(
        &self,
        config: &job::Config,
    ) -> Result<(JobId, Vec<TaskId>), tokio_postgres::Error> {
        let state = if config.iteration_count == 0 {
            JobState::Completed
        } else {
            JobState::Queued
        };
        let rows = self
            .0
            .query(
                "
                INSERT INTO job (id, state, config) VALUES (DEFAULT, $1, $2) RETURNING id
                ",
                &[&state, &config.to_json().unwrap()],
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
        for _ in 0..config.iteration_count {
            let rows = self
                .0
                .query(&statement, &[&job_id.0, &TaskState::default()])
                .await?;
            task_ids.push(TaskId(rows[0].get(0)));
        }
        Ok((job_id, task_ids))
    }

    async fn update_task_succeeded(&self, task_id: &TaskId, worker_index: usize) {
        self.0
            .execute(
                "UPDATE task SET worker_index = $1, state = $2 WHERE id = $3",
                &[&(worker_index as i32), &TaskState::Succeeded, &task_id.0],
            )
            .await
            .unwrap();
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

    async fn get_task(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<task::Task>, tokio_postgres::Error> {
        let rs = self
            .0
            .query("SELECT id, state FROM task WHERE id = $1", &[&task_id.0])
            .await?;
        let Some(r) = rs.get(0) else { return Ok(None) };
        let id: Uuid = r.get(0);
        let state: task::TaskState = r.get(1);
        Ok(Some(task::Task {
            id: id.to_string(),
            state,
        }))
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

    async fn get_job(&self, id: &JobId) -> anyhow::Result<Option<job::Job>> {
        let rs = self
            .0
            .query("SELECT state, config FROM job WHERE id = $1", &[&id.0])
            .await?;
        let Some(r) = rs.get(0) else { return Ok(None) };
        let state: job::JobState = r.get(0);
        let config_json: postgres_types::Json<serde_json::Value> = r.get(1);
        let config: job::Config =
            poem_openapi::types::ParseFromJSON::parse_from_json(Some(config_json.0))
                .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;

        Ok(Some(job::Job {
            id: id.to_string(),
            state,
            config,
            tasks: self.get_tasks(id).await,
        }))
    }

    async fn get_jobs(&self) -> anyhow::Result<Vec<job::Job>> {
        let mut jobs = Vec::new();
        for r in self
            .0
            .query("SELECT id, state, config FROM job", &[])
            .await?
        {
            let id: Uuid = r.get(0);
            let state: job::JobState = r.get(1);
            let config_json: postgres_types::Json<serde_json::Value> = r.get(2);
            let config: job::Config =
                poem_openapi::types::ParseFromJSON::parse_from_json(Some(config_json.0))
                    .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;

            jobs.push(job::Job {
                id: id.to_string(),
                state,
                config,
                tasks: self.get_tasks(&JobId(id)).await,
            })
        }
        Ok(jobs)
    }

    async fn get_all_tasks_with_stats(
        &self,
        id: &JobId,
    ) -> Result<Vec<(TaskId, usize)>, tokio_postgres::Error> {
        let mut v = Vec::new();
        for r in self
            .0
            .query(
                "SELECT id, worker_index FROM task WHERE job_id = $1 AND worker_index IS NOT NULL",
                &[&id.0],
            )
            .await?
        {
            let task_id: Uuid = r.get(0);
            let worker_index: i32 = r.get(1);
            v.push((TaskId(task_id), worker_index as usize));
        }
        Ok(v)
    }

    async fn delete_job(&self, id: &JobId) -> Result<(), tokio_postgres::Error> {
        self.0
            .execute("DELETE FROM task WHERE job_id = $1", &[&id.0])
            .await?;
        self.0
            .execute("DELETE FROM job WHERE id = $1", &[&id.0])
            .await?;
        Ok(())
    }

    async fn get_worker_index(&self, id: &TaskId) -> Result<Option<usize>, tokio_postgres::Error> {
        let rs = self
            .0
            .query(
                "SELECT worker_index FROM task WHERE id = $1 AND worker_index IS NOT NULL",
                &[&id.0],
            )
            .await?;
        let Some(r) = rs.get(0) else {return Ok(None)};
        let i = r.get::<_, i32>(0);
        Ok(Some(i as usize))
    }
}

pub struct Manager {
    job_queue_tx: mpsc::Sender<Job>,
    queued_jobs: Arc<RwLock<HashMap<JobId, (ForceQuitSender, Notify)>>>,
    db: Db,
    worker_manager: Arc<WorkerManager>,
}

impl Manager {
    pub async fn new(
        db_username: String,
        db_password: String,
        max_job_request: usize,
        addr: IpAddr,
        workers: Vec<ServerConfig>,
    ) -> Result<Self, Box<dyn Error>> {
        let (client, connection) = tokio_postgres::connect(
            &format!(
                "host=localhost user={} password={}",
                db_username, db_password
            ),
            NoTls,
        )
        .await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                println!("[error] Postresql database connection error: {e}");
            }
        });

        let (job_queue_tx, mut job_queue_rx) = mpsc::channel::<Job>(max_job_request);
        let worker_manager = Arc::new(WorkerManager::new(addr, workers).await?);

        let db = Db(Arc::new(client));
        let manager = Self {
            job_queue_tx,
            db: db.clone(),
            worker_manager: worker_manager.clone(),
            queued_jobs: Default::default(),
        };
        let queued_jobs = manager.queued_jobs.clone();

        tokio::spawn(async move {
            while let Some(job) = job_queue_rx.recv().await {
                let id = job.id.clone();

                println!("[info] received job {}", id);
                if !job.fq_rx.has_changed().unwrap() {
                    db.update_job_state(&id, &JobState::Running).await;
                    job.consume(&worker_manager, &db).await;
                }
                let (_, notify) = queued_jobs.write().await.remove(&id).unwrap();
                notify.notify_one();
                db.update_job_state(&id, &JobState::Completed).await;
                println!("[info] job {} terminated", id);
            }
        });

        println!("[info] created job manager");
        Ok(manager)
    }

    async fn create_job(&self, config: job::Config) -> Result<String, tokio_postgres::Error> {
        let (job_id, task_ids) = self.db.insert_job(&config).await?;

        if config.iteration_count > 0 {
            let (fq_tx, fq_rx) = force_quit_channel();
            self.queued_jobs
                .write()
                .await
                .insert(job_id.clone(), (fq_tx, Notify::new()));

            self.job_queue_tx
                .send(Job {
                    id: job_id.clone(),
                    fq_rx,
                    task_ids,
                    config,
                })
                .await
                .unwrap();
        }

        Ok(job_id.to_string())
    }

    async fn delete_job(&self, id: &JobId) -> anyhow::Result<()> {
        if let Some((tx, notify)) = self.queued_jobs.read().await.get(&id) {
            tx.send();
            notify.notified().await;
        }

        let mut task_ids_map = vec![Vec::new(); self.worker_manager.get_worker_count()];
        for (task_id, worker_index) in self.db.get_all_tasks_with_stats(&id).await? {
            task_ids_map[worker_index].push(task_id);
        }
        for (worker_index, task_ids) in task_ids_map.into_iter().enumerate() {
            let worker = self.worker_manager.get_worker(worker_index);
            match worker.remove_statistics(&task_ids).await {
                Ok(failed) => {
                    for id in failed {
                        eprintln!("[error] failed to remove {id}");
                    }
                }
                Err(e) => eprintln!("[error] {e}"),
            }
        }
        self.db.delete_job(&id).await?;
        Ok(())
    }

    async fn terminate_job(&self, id: &JobId) -> Option<bool> {
        let map = self.queued_jobs.read().await;
        let (tx, _) = map.get(&id)?;
        Some(tx.send())
    }

    async fn get_statistics(&self, id: &TaskId) -> anyhow::Result<Option<Vec<u8>>> {
        let Some(worker_index) = self.db.get_worker_index(&id).await? else {
            return Ok(None);
        };
        let client = self.worker_manager.get_worker(worker_index);
        Ok(Some(client.get_statistics(&id).await?))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum JobDeleteError {
    #[error("job not found")]
    NotFound(#[from] uuid::Error),
    #[error("internal error")]
    InternalError(#[from] anyhow::Error),
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

    async fn get_job(&self, id: &str) -> anyhow::Result<Option<job::Job>> {
        let id = JobId::try_from(id)?;
        self.db.get_job(&id).await
    }

    async fn get_all_jobs(&self) -> anyhow::Result<Vec<job::Job>> {
        self.db.get_jobs().await
    }

    async fn delete_job(&self, id: &str) -> Result<(), JobDeleteError> {
        let id = JobId::try_from(id)?;
        self.delete_job(&id).await?;
        Ok(())
    }

    async fn terminate_job(&self, id: &str) -> anyhow::Result<Option<bool>> {
        let id = id.try_into()?;
        Ok(self.terminate_job(&id).await)
    }

    async fn get_task(&self, id: &str) -> anyhow::Result<Option<task::Task>> {
        let id = TaskId::try_from(id)?;
        Ok(self.db.get_task(&id).await?)
    }

    async fn get_statistics(&self, id: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let id = id.try_into()?;
        Ok(self.get_statistics(&id).await?)
    }
}

pub mod worker {
    use std::{
        error::Error,
        fmt::Display,
        net::{IpAddr, SocketAddr},
        pin::Pin,
        sync::Arc,
    };

    use futures_util::{stream::FuturesUnordered, Future, StreamExt};
    use quinn::{Connection, Endpoint};
    use repl::nom::AsBytes;
    use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};
    use tokio_util::codec::{FramedRead, LengthDelimitedCodec};
    use uuid::Uuid;

    use worker_if::batch::{Cost, Request, ResourceMeasure, Response};

    use crate::app::{job, task::TaskState};

    use super::ForceQuitReceiver;

    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    pub struct TaskId(pub Uuid);

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

    #[derive(serde::Deserialize, Debug)]
    pub struct ServerConfig {
        pub controller_port: u16,
        pub cert_path: String,
        pub addr: SocketAddr,
        pub domain: String,
    }

    #[derive(Clone, Debug)]
    pub(super) struct WorkerClient {
        connection: Arc<Connection>,
        semaphore: Arc<Semaphore>,
        measure: ResourceMeasure,
        index: usize,
    }

    impl WorkerClient {
        async fn new(
            client_addr: IpAddr,
            server_config: ServerConfig,
            index: usize,
        ) -> Result<Self, Box<dyn Error>> {
            let mut endpoint =
                Endpoint::client(SocketAddr::new(client_addr, server_config.controller_port))?;
            endpoint.set_default_client_config(quic_config::get_client_config(
                &server_config.cert_path,
            )?);

            let connection = endpoint
                .connect(server_config.addr, &server_config.domain)?
                .await?;

            let mut recv = connection.accept_uni().await?;
            let measure = protocol::quic::read_data::<ResourceMeasure>(&mut recv).await?;
            println!("[info] worker {index} has {measure:?}");

            Ok(Self {
                connection: Arc::new(connection),
                semaphore: Arc::new(Semaphore::new(measure.max_resource as usize)),
                measure,
                index,
            })
        }

        async fn acquire(self, n: u32) -> WorkerClientPermitted {
            let permit = self.semaphore.clone().acquire_many_owned(n).await.unwrap();
            WorkerClientPermitted(self, permit)
        }

        pub async fn execute(
            &self,
            task_id: &TaskId,
            config: job::Config,
        ) -> anyhow::Result<impl Future<Output = Option<bool>>> {
            let (mut send, recv) = self.connection.open_bi().await?;
            protocol::quic::write_data(
                &mut send,
                &Request::Execute(task_id.to_string(), config.param),
            )
            .await?;

            let mut stream = FramedRead::new(recv, LengthDelimitedCodec::new());
            if let Err(e) =
                bincode::deserialize::<Response<()>>(stream.next().await.unwrap()?.as_bytes())?
                    .as_result()
            {
                return Err(e.into());
            }

            Ok(async move {
                stream
                    .next()
                    .await
                    .unwrap()
                    .ok()
                    .map(|data| bincode::deserialize::<bool>(data.as_bytes()).unwrap())
            })
        }

        pub async fn terminate(&self, task_id: &TaskId) -> anyhow::Result<()> {
            let (mut send, mut recv) = self.connection.open_bi().await?;
            protocol::quic::write_data(&mut send, &Request::Terminate(task_id.to_string())).await?;
            protocol::quic::read_data::<Response<()>>(&mut recv)
                .await?
                .as_result()?;
            Ok(())
        }

        pub async fn get_statistics(&self, task_id: &TaskId) -> anyhow::Result<Vec<u8>> {
            let (mut send, recv) = self.connection.open_bi().await?;
            protocol::quic::write_data(&mut send, &Request::ReadStatistics(task_id.to_string()))
                .await?;
            let mut stream = FramedRead::new(recv, LengthDelimitedCodec::new());
            match bincode::deserialize::<Response<Vec<u8>>>(
                stream.next().await.unwrap()?.as_bytes(),
            )?
            .as_result()
            {
                Ok(buf) => Ok(buf),
                Err(e) => {
                    eprintln!("[error] {:?}", e);
                    Err(e.into())
                }
            }
        }

        pub async fn remove_statistics(&self, task_ids: &[TaskId]) -> anyhow::Result<Vec<String>> {
            let (mut send, mut recv) = self.connection.open_bi().await?;
            protocol::quic::write_data(
                &mut send,
                &Request::RemoveStatistics(task_ids.into_iter().map(|id| id.to_string()).collect()),
            )
            .await?;
            Ok(protocol::quic::read_data::<Vec<String>>(&mut recv).await?)
        }
    }

    #[derive(Debug)]
    pub(super) struct WorkerClientPermitted(WorkerClient, OwnedSemaphorePermit);

    impl WorkerClientPermitted {
        pub async fn execute(
            self,
            task_id: &TaskId,
            config: job::Config,
            db: &super::Db,
            fq_rx: ForceQuitReceiver,
        ) {
            println!("[debug] executing...");
            db.update_task_state(&task_id, &TaskState::Assigned).await;

            let Ok(fut) = self.0.execute(&task_id, config).await else {
                    db.update_task_state(&task_id, &TaskState::Failed).await;
                    println!("[info] task {} could not execute", task_id);
                    return;
                };
            db.update_task_state(&task_id, &TaskState::Running).await;

            println!("[debug] {} is running", task_id);
            let worker_index = self.0.index;

            println!("[debug] worker is registered");
            let id = task_id.clone();
            let fq_handle = tokio::spawn(async move {
                if let Ok(_) = fq_rx.changed().await {
                    if self.0.terminate(&id).await.is_err() {
                        println!("[info] task {} is already terminated", id);
                    }
                }
            });
            let result = fut.await;
            fq_handle.abort();
            drop(self.1);

            match result {
                Some(true) => {
                    db.update_task_succeeded(&task_id, worker_index).await;
                    println!("[info] task {} successfully terminated", task_id);
                }
                _ => {
                    db.update_task_state(&task_id, &TaskState::Failed).await;
                    println!("[info] task {} failured in process", task_id);
                }
            }
        }
    }

    pub(super) struct WorkerManager {
        workers: Vec<WorkerClient>,
        queue_tx: mpsc::Sender<(oneshot::Sender<WorkerClientPermitted>, Cost)>,
    }

    impl WorkerManager {
        pub async fn new(
            client_addr: IpAddr,
            servers: Vec<ServerConfig>,
        ) -> Result<Self, Box<dyn Error>> {
            let mut _workers = Vec::new();
            for (i, server_config) in servers.into_iter().enumerate() {
                _workers.push(WorkerClient::new(client_addr, server_config, i).await?);
            }

            let (queue_tx, mut queue_rx): (mpsc::Sender<(oneshot::Sender<_>, _)>, _) =
                mpsc::channel(1);

            let workers = _workers.clone();
            tokio::spawn(async move {
                while let Some((tx, cost)) = queue_rx.recv().await {
                    let mut futs = FuturesUnordered::new();
                    for client in &workers {
                        let Ok(res) = client.measure.measure(&cost) else {
                            continue;
                        };
                        let client = client.clone();
                        futs.push(async move { client.acquire(res).await });
                    }
                    if let Some(permit) = futs.next().await {
                        println!("[debug] acquired at {}", permit.0.index);
                        if let Err(_) = tx.send(permit) {
                            println!("[debug] A lease has already dropped.");
                        }
                    }
                    drop(futs);
                }
            });
            Ok(Self {
                queue_tx,
                workers: _workers,
            })
        }

        pub fn get_worker_count(&self) -> usize {
            self.workers.len()
        }

        pub fn get_worker(&self, index: usize) -> &WorkerClient {
            &self.workers[index]
        }

        pub async fn lease(&self, cost: Cost) -> WorkerLease {
            let (tx, rx) = oneshot::channel();
            self.queue_tx.send((tx, cost)).await.unwrap();
            WorkerLease(rx)
        }
    }

    pub(super) struct WorkerLease(oneshot::Receiver<WorkerClientPermitted>);

    impl Future for WorkerLease {
        type Output = Result<WorkerClientPermitted, oneshot::error::RecvError>;

        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            Pin::new(&mut self.as_mut().0).poll(cx)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use futures_util::{future::join_all, stream::FuturesUnordered, StreamExt};
    use poem_openapi::types::ToJSON;
    use tokio::{runtime::Runtime, sync::Semaphore, time};
    use tokio_postgres::{types::Json, NoTls};

    use super::force_quit_channel;

    #[test]
    fn test_notify() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut futs = Vec::new();
            let (tx, rx) = force_quit_channel();
            tx.send();

            for i in 0..5 {
                let rx = rx.clone();
                futs.push(async move {
                    time::sleep(Duration::from_secs(1)).await;
                    if rx.changed().await.is_ok() {
                        println!("received signal at {i}");
                    }
                });
            }

            join_all(futs).await;
        });
    }

    #[test]
    fn test_watch() {
        use tokio::sync::watch;
        let (tx, mut rx) = watch::channel(false);
        tx.send(true).unwrap();
        assert!(rx.has_changed().unwrap());
        assert_eq!(*rx.borrow_and_update(), true);
        tx.send(true).unwrap();
        assert!(rx.has_changed().unwrap());
        assert_eq!(*rx.borrow_and_update(), true);
        tx.send(false).unwrap();
        assert!(rx.has_changed().unwrap());
        assert_eq!(*rx.borrow_and_update(), false);
    }

    #[test]
    fn abort_handle() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let h = tokio::spawn(async {
                // time::sleep(Duration::from_secs(1)).await;
                println!("done.");
            });
            time::sleep(Duration::from_secs(1)).await;
            if h.is_finished() {
                println!("finished");
            }
            h.abort();
        });
    }

    #[test]
    fn test_semaphores() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let semaphores = [1, 1, 2, 2, 1].map(|n| Arc::new(Semaphore::new(n)));

            println!(
                "{:?}",
                semaphores
                    .iter()
                    .map(|s| s.available_permits())
                    .collect::<Vec<_>>()
            );

            let mut futs = FuturesUnordered::new();
            for (i, semaphore) in semaphores.iter().enumerate() {
                let semaphore = semaphore.clone();
                futs.push(async move {
                    let permit = semaphore.acquire_many_owned(2).await.unwrap();
                    println!("temporally acquired from semphore-{i}");
                    (i, permit)
                });
            }

            let semaphores2 = semaphores.clone();
            tokio::spawn(async move {
                if let Some((i, permit)) = futs.next().await {
                    println!("acquired 2 permits from semaphore-{i}");
                    println!(
                        "{:?}",
                        semaphores2
                            .iter()
                            .map(|s| s.available_permits())
                            .collect::<Vec<_>>()
                    );
                    drop(permit);
                }
                drop(futs);
            });

            let mut futs2 = Vec::new();
            for (i, semaphore) in semaphores.iter().enumerate() {
                let semaphore = semaphore.clone();
                futs2.push(async move {
                    let permit = semaphore.acquire_owned().await.unwrap();
                    println!("acquired a permit from semaphore-{i}");
                    drop(permit);
                });
            }

            join_all(futs2).await;
        });
    }

    #[derive(poem_openapi::Object, Debug, PartialEq)]
    struct JsonTest {
        hoge: u32,
    }

    #[test]
    fn test_jsonb_db() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (client, connection) =
                tokio_postgres::connect("host=localhost user=simepi password=simepi", NoTls)
                    .await
                    .unwrap();
            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    println!("[error] Postresql database connection error: {e}");
                }
            });
            let test = JsonTest { hoge: 100 };
            client
                .execute(
                    "INSERT INTO test VALUES (10, $1)",
                    &[&(test).to_json().unwrap()],
                )
                .await
                .unwrap();

            let rs = client
                .query("select * from test where id = 10", &[])
                .await
                .unwrap();
            let Json(colj): Json<serde_json::Value> = rs[0].get(1);
            let test2: JsonTest =
                poem_openapi::types::ParseFromJSON::parse_from_json(Some(colj)).unwrap();
            println!("{:?}", test2);
            assert_eq!(test, test2);
        })
    }
}
