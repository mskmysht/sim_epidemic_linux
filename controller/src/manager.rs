use std::{collections::HashMap, error::Error, fmt::Display, net::IpAddr, sync::Arc};

use async_trait::async_trait;
use futures_util::future::join_all;
use poem_openapi::types::ToJSON;
use tokio::sync::{mpsc, RwLock};
use tokio_postgres::{Client, NoTls};
use uuid::Uuid;

use crate::app::{
    job::{self, JobState},
    task::{self, TaskState},
    ResourceManager,
};

use self::worker::{ServerConfig, TaskId, WorkerManager};

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

fn termination_channel() -> (TerminationSender, TerminationReceiver) {
    let (tx, rx) = async_channel::bounded(1);
    (
        TerminationSender(tx),
        TerminationReceiver(rx, Default::default()),
    )
}

#[derive(Debug)]
struct TerminationSender(async_channel::Sender<()>);

impl TerminationSender {
    async fn send(self) -> bool {
        self.0.send(()).await.is_ok()
    }
}

#[derive(Clone, Debug)]
struct TerminationReceiver(async_channel::Receiver<()>, Arc<parking_lot::Mutex<bool>>);

impl TerminationReceiver {
    fn try_recv(&self) -> bool {
        self.0.try_recv().is_ok()
    }

    async fn recv(self) -> bool {
        match self.0.recv().await {
            Ok(_) => {
                *self.1.lock() = true;
                true
            }
            Err(_) => *self.1.lock(),
        }
    }
}

#[derive(Debug)]
struct Job {
    id: JobId,
    rx: TerminationReceiver,
    task_ids: Vec<TaskId>,
    config: job::Config,
}

impl Job {
    async fn consume(self, worker_manager: &WorkerManager, db: &Db) {
        let workings = Default::default();
        let mut handles = Vec::new();
        for task_id in self.task_ids {
            let lease = worker_manager.lease((&self.config.param).into());
            let rx = self.rx.clone();
            let lease2 = lease.clone();
            let id = task_id.clone();
            tokio::spawn(async move {
                if rx.recv().await {
                    println!("[debug] catched termination signal at {}", id);
                    if lease2.close() {
                        println!("[info] {} lease is canceled", id);
                    }
                }
            });

            let workings = Arc::clone(&workings);
            let config = self.config.clone();
            let db = db.clone();
            handles.push(tokio::spawn(async move {
                println!("[debug] received task {}", task_id);
                println!("[debug] waiting to receive worker");
                let Ok(worker) = lease.recv().await else {
                    println!("[debug] {} is skipped", task_id);
                    return;
                };
                worker.execute(&task_id, config, &db, workings).await;
            }));
        }

        tokio::spawn(async move {
            if self.rx.recv().await {
                for (task_id, client) in workings.read().await.iter() {
                    if client.terminate(task_id).await.is_err() {
                        println!("[info] task {} is already terminated", self.id);
                    }
                }
                println!("[debug] {} is released", self.id);
            }
            println!("[debug] job {} termination has ended", self.id);
        });
        join_all(handles).await;
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

    async fn get_task(&self, task_id: &TaskId) -> Option<task::Task> {
        let rs = self
            .0
            .query("SELECT id, state FROM task WHERE id = $1", &[&task_id.0])
            .await
            .unwrap();
        let r = rs.get(0)?;
        let id: Uuid = r.get(0);
        let state: task::TaskState = r.get(1);
        Some(task::Task {
            id: id.to_string(),
            state,
        })
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

    async fn get_job(&self, id: &JobId) -> Option<job::Job> {
        let rs = self
            .0
            .query("SELECT state, config FROM job WHERE id = $1", &[&id.0])
            .await
            .unwrap();
        let r = rs.get(0)?;
        let state: job::JobState = r.get(0);
        let config_json: postgres_types::Json<serde_json::Value> = r.get(1);
        let config: job::Config =
            poem_openapi::types::ParseFromJSON::parse_from_json(Some(config_json.0)).unwrap();

        Some(job::Job {
            id: id.to_string(),
            state,
            config,
            tasks: self.get_tasks(id).await,
        })
    }

    async fn get_jobs(&self) -> Vec<job::Job> {
        let mut jobs = Vec::new();
        for r in self
            .0
            .query("SELECT id, state, config FROM job", &[])
            .await
            .unwrap()
        {
            let id: Uuid = r.get(0);
            let state: job::JobState = r.get(1);
            let config_json: postgres_types::Json<serde_json::Value> = r.get(2);
            let config: job::Config =
                poem_openapi::types::ParseFromJSON::parse_from_json(Some(config_json.0)).unwrap();
            jobs.push(job::Job {
                id: id.to_string(),
                state,
                config,
                tasks: self.get_tasks(&JobId(id)).await,
            })
        }
        jobs
    }

    async fn delete_jobs(&self) -> anyhow::Result<()> {
        self.0.execute("DELETE FROM task", &[]).await?;
        self.0.execute("DELETE FROM job", &[]).await?;
        Ok(())
    }

    async fn get_worker_index(&self, id: &TaskId) -> Option<usize> {
        let rs = self
            .0
            .query("SELECT worker_index FROM task WHERE id = $1", &[&id.0])
            .await
            .unwrap();
        let r = rs.get(0)?;
        let i = r.get::<_, Option<i32>>(0)?;
        Some(i as usize)
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct Config {
    client_addr: IpAddr,
    db_username: String,
    db_password: String,
    max_job_request: usize,
    servers: Vec<ServerConfig>,
}

pub struct Manager {
    job_queue_tx: mpsc::Sender<Job>,
    job_terminations: Arc<RwLock<HashMap<JobId, TerminationSender>>>,
    db: Db,
    worker_manager: Arc<WorkerManager>,
}

impl Manager {
    pub async fn new(config: Config) -> Result<Self, Box<dyn Error>> {
        let (client, connection) = tokio_postgres::connect(
            &format!(
                "host=localhost user={} password={}",
                config.db_username, config.db_password
            ),
            NoTls,
        )
        .await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                println!("[error] Postresql database connection error: {e}");
            }
        });

        let (job_queue_tx, mut job_queue_rx) = mpsc::channel::<Job>(config.max_job_request);
        let worker_manager =
            Arc::new(WorkerManager::new(config.client_addr, config.servers).await?);

        let manager = Self {
            job_queue_tx,
            job_terminations: Default::default(),
            db: Db(Arc::new(client)),
            worker_manager: worker_manager.clone(),
        };

        let job_terminations = Arc::clone(&manager.job_terminations);
        let db = manager.db.clone();
        tokio::spawn(async move {
            while let Some(job) = job_queue_rx.recv().await {
                let id = job.id.clone();

                println!("[info] received job {}", id);
                if !job.rx.try_recv() {
                    db.update_job_state(&id, &JobState::Running).await;
                    job.consume(&worker_manager, &db).await;
                }
                job_terminations.write().await.remove(&id);
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
            let (tx, rx) = termination_channel();
            self.job_terminations
                .write()
                .await
                .insert(job_id.clone(), tx);

            self.job_queue_tx
                .send(Job {
                    id: job_id.clone(),
                    rx,
                    task_ids,
                    config,
                })
                .await
                .unwrap();
        }

        Ok(job_id.to_string())
    }

    async fn get_job(&self, id: &str) -> Option<job::Job> {
        let id = id.try_into().ok()?;
        self.db.get_job(&id).await
    }

    async fn get_all_jobs(&self) -> Vec<job::Job> {
        self.db.get_jobs().await
    }

    async fn delete_all_jobs(&self) -> bool {
        self.db.delete_jobs().await.is_ok()
    }

    async fn terminate_job(&self, id: &str) -> Option<bool> {
        let id = id.try_into().ok()?;
        let mut table = self.job_terminations.write().await;
        let tx = table.remove(&id)?;
        Some(tx.send().await)
    }

    async fn get_task(&self, id: &str) -> Option<task::Task> {
        let id = id.try_into().ok()?;
        self.db.get_task(&id).await
    }

    async fn get_statistics(&self, id: &str) -> Option<Vec<u8>> {
        let id = id.try_into().ok()?;
        let worker_index = self.db.get_worker_index(&id).await?;
        let client = self.worker_manager.get_worker(worker_index);
        client.get_statistics(&id).await.ok()
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

    async fn delete_all_jobs(&self) -> bool {
        self.delete_all_jobs().await
    }

    async fn terminate_job(&self, id: &str) -> Option<bool> {
        self.terminate_job(id).await
    }

    async fn get_task(&self, id: &str) -> Option<task::Task> {
        self.get_task(id).await
    }

    async fn get_statistics(&self, id: &str) -> Option<Vec<u8>> {
        self.get_statistics(id).await
    }
}

pub mod worker {
    use std::{
        collections::HashMap,
        error::Error,
        fmt::Display,
        net::{IpAddr, SocketAddr},
        sync::Arc,
    };

    use futures_util::StreamExt;
    use quinn::{Connection, Endpoint};
    use repl::nom::AsBytes;
    use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, RwLock, Semaphore};
    use tokio_util::codec::{FramedRead, LengthDelimitedCodec};
    use uuid::Uuid;

    use worker_if::batch::{Cost, Request, ResourceMeasure, Response};

    use crate::app::{job, task::TaskState};

    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    pub(super) struct TaskId(pub Uuid);

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
        pub client_port: u16,
        pub addr: SocketAddr,
        pub domain: String,
        pub cert_path: String,
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
                Endpoint::client(SocketAddr::new(client_addr, server_config.client_port))?;
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

        fn available(&self) -> u32 {
            self.semaphore.available_permits() as u32
        }

        async fn acquire(self, n: u32) -> WorkerClientPermitted {
            let permit = self.semaphore.clone().acquire_many_owned(n).await.unwrap();
            WorkerClientPermitted(self, permit)
        }

        pub async fn execute(
            &self,
            task_id: &TaskId,
            config: job::Config,
        ) -> anyhow::Result<oneshot::Receiver<Option<bool>>> {
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

            let (tx, rx) = oneshot::channel();
            tokio::spawn(async move {
                let exit_status = stream
                    .next()
                    .await
                    .unwrap()
                    .ok()
                    .map(|data| bincode::deserialize::<bool>(data.as_bytes()).unwrap());
                tx.send(exit_status).unwrap();
            });

            Ok(rx)
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
            println!("[debug] get stat");
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
    }

    #[derive(Debug)]
    pub(super) struct WorkerClientPermitted(WorkerClient, OwnedSemaphorePermit);

    impl WorkerClientPermitted {
        pub async fn execute(
            self,
            task_id: &TaskId,
            config: job::Config,
            db: &super::Db,
            workings: Arc<RwLock<HashMap<TaskId, WorkerClient>>>,
        ) {
            println!("[debug] executing...");
            db.update_task_state(&task_id, &TaskState::Assigned).await;

            let Ok(rx) = self.0.execute(&task_id, config).await else {
                    db.update_task_state(&task_id, &TaskState::Failed).await;
                    println!("[info] task {} could not execute", task_id);
                    return;
                };
            db.update_task_state(&task_id, &TaskState::Running).await;

            println!("[debug] {} is running", task_id);
            let worker_index = self.0.index;
            workings.write().await.insert(task_id.clone(), self.0);
            println!("[debug] worker is registered");

            let result = rx.await.unwrap();
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
            workings.write().await.remove(&task_id);
        }
    }

    pub(super) struct WorkerManager {
        workers: Vec<WorkerClient>,
        queue_tx: mpsc::Sender<(async_channel::Sender<WorkerClientPermitted>, Cost)>,
    }

    impl WorkerManager {
        fn find_optimal_worker(
            workers: &[WorkerClient],
            cost: &Cost,
        ) -> Result<(WorkerClient, u32), Vec<(WorkerClient, u32)>> {
            let mut best = -1.0;
            let mut info_res = None;
            let mut lackings = Vec::new();
            for client in workers {
                let cr = client.available();
                let Ok(res) = client.measure.measure(cost) else {
                    continue;
                };
                println!("[debug] {res:?}/{cr:?}");
                let Some(r) = cr.checked_sub(res) else {
                    lackings.push((client.clone(), res));
                    continue;
                };
                let remaining = r as f32 / client.measure.max_resource as f32;
                if remaining > best {
                    best = remaining;
                    info_res = Some((client.clone(), res))
                }
            }
            info_res.ok_or(lackings)
        }

        pub async fn new(
            client_addr: IpAddr,
            servers: Vec<ServerConfig>,
        ) -> Result<Self, Box<dyn Error>> {
            let mut _workers = Vec::new();
            for (i, server_config) in servers.into_iter().enumerate() {
                _workers.push(WorkerClient::new(client_addr, server_config, i).await?);
            }

            let (queue_tx, mut queue_rx): (mpsc::Sender<(async_channel::Sender<_>, _)>, _) =
                mpsc::channel(1);

            let workers = _workers.clone();
            tokio::spawn(async move {
                while let Some((tx, cost)) = queue_rx.recv().await {
                    match Self::find_optimal_worker(&workers, &cost) {
                        Ok((client, res)) => {
                            println!(
                                "[debug] current resource of worker {}: ({:?})",
                                client.index, client.semaphore
                            );
                            if !tx.is_closed() {
                                tx.send(client.acquire(res).await).await.unwrap();
                            }
                        }
                        Err(lackings) => {
                            println!("[debug] lacking...");
                            let (lackings_tx, mut lackings_rx) = mpsc::channel(1);
                            let mut handles = Vec::new();
                            for (client, res) in lackings {
                                let tx = lackings_tx.clone();
                                let client = client.clone();
                                handles.push(tokio::spawn(async move {
                                    println!("[debug] temporally acqurired");
                                    if !tx.is_closed() {
                                        tx.send(client.acquire(res).await).await.unwrap();
                                    }
                                }));
                            }
                            tokio::spawn(async move {
                                if let Some(permit) = lackings_rx.recv().await {
                                    for handle in handles {
                                        handle.abort();
                                    }
                                    println!("[debug] acqurired and aborted");
                                    if !tx.is_closed() {
                                        tx.send(permit).await.unwrap();
                                    }
                                    println!("[debug] sent");
                                }
                            });
                        }
                    }
                }
            });
            Ok(Self {
                queue_tx,
                workers: _workers,
            })
        }

        pub fn get_worker(&self, index: usize) -> &WorkerClient {
            &self.workers[index]
        }

        pub fn lease(&self, cost: Cost) -> WorkerLease {
            let (tx, rx) = async_channel::bounded(1);
            let queue = self.queue_tx.clone();
            tokio::spawn(async move {
                queue.send((tx, cost)).await.unwrap();
            });
            WorkerLease(rx)
        }
    }

    #[derive(Clone)]
    pub(super) struct WorkerLease(async_channel::Receiver<WorkerClientPermitted>);

    impl WorkerLease {
        pub fn close(&self) -> bool {
            self.0.close()
        }

        pub fn recv(&self) -> async_channel::Recv<'_, WorkerClientPermitted> {
            self.0.recv()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_util::future::join_all;
    use poem_openapi::types::ToJSON;
    use tokio::{
        runtime::Runtime,
        sync::{mpsc, Semaphore},
    };
    use tokio_postgres::{types::Json, NoTls};

    use super::termination_channel;

    #[test]
    fn test_notify() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut handles = Vec::new();
            let (tx, rx) = termination_channel();
            for i in 0..5 {
                let rx = rx.clone();
                handles.push(tokio::spawn(async move {
                    if rx.recv().await {
                        println!("received signal at {i}");
                    }
                }));
            }

            tx.send().await;
            join_all(handles).await;
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

            let mut handles = Vec::new();
            let (tx, mut rx) = mpsc::channel(1);
            for (i, semaphore) in semaphores.iter().enumerate() {
                let semaphore = semaphore.clone();
                let tx = tx.clone();
                handles.push(tokio::spawn(async move {
                    let permit = semaphore.acquire_many_owned(2).await.unwrap();
                    println!("temporally acquired from semphore-{i}");
                    tx.send((i, permit)).await.unwrap();
                }));
            }

            let semaphores2 = semaphores.clone();
            tokio::spawn(async move {
                if let Some((i, permit)) = rx.recv().await {
                    for handle in handles {
                        handle.abort();
                    }
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
            });

            let mut handles2 = Vec::new();
            for (i, semaphore) in semaphores.iter().enumerate() {
                let semaphore = semaphore.clone();
                handles2.push(tokio::spawn(async move {
                    let permit = semaphore.acquire_owned().await.unwrap();
                    println!("aquired a permit from semaphore-{i}");
                    drop(permit);
                }));
            }

            join_all(handles2).await;
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
