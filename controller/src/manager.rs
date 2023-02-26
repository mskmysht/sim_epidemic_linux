use std::{
    collections::HashMap, error::Error, fmt::Display, net::SocketAddr, sync::Arc, thread,
    time::Duration,
};

use async_trait::async_trait;
use futures_util::future::join_all;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_postgres::{Client, NoTls};
use uuid::Uuid;

use crate::app::{
    job::{self, JobState},
    task::{self, TaskState},
    ResourceManager,
};

use self::worker::{TaskId, WorkerClient, WorkerManager};

type WorkerTableRef = Arc<RwLock<HashMap<TaskId, WorkerClient>>>;

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

async fn execute_task(
    id: &TaskId,
    config: job::Config,
    worker: &WorkerClient,
    worker_table: WorkerTableRef,
    db: &Db,
) {
    db.update_task_state(id, &TaskState::Assigned).await;

    let (tx, rx) = oneshot::channel();
    if worker.execute(id, config, tx).await.is_err() {
        db.update_task_state(id, &TaskState::Failed).await;
        println!("[info] task {} could not execute", id);
        return;
    }
    db.update_task_state(id, &TaskState::Running).await;
    println!("[debug] {id} is running");
    worker_table
        .write()
        .await
        .insert(id.clone(), worker.clone());
    println!("[debug] worker is registered");

    match rx.await.unwrap() {
        Some(true) => {
            db.update_task_state(id, &TaskState::Succeeded).await;
            println!("[info] task {} successfully terminated", id);
        }
        _ => {
            db.update_task_state(id, &TaskState::Failed).await;
            println!("[info] task {} failured in process", id);
        }
    }
    worker_table.write().await.remove(id);
    println!("[debug] worker is removed");
}

#[derive(Debug)]
struct JobQueued {
    job: Job,
    task_ids: Vec<TaskId>,
    config: job::Config,
}

impl JobQueued {
    async fn dequeue(self, worker_manager: &WorkerManager, db: &Db) {
        let job = self.job;

        if job.is_foreced_termination().await {
            job.update_state(JobState::Completed, db).await;
            return;
        }

        job.update_state(JobState::Scheduled, db).await;
        job.update_state(JobState::Running, db).await;

        let mut handles = Vec::new();
        for task_id in self.task_ids {
            if job.is_foreced_termination().await {
                break;
            }
            let worker_lease = worker_manager.lease((&self.config.param).into());
            let worker_table = Arc::clone(&job.worker_table);
            let config = self.config.clone();
            let job = job.clone();
            let db = db.clone();
            thread::sleep(Duration::from_secs(1));
            handles.push(tokio::spawn(async move {
                println!("[debug] waiting to receive worker");
                let worker = worker_lease.await.unwrap();
                println!("[debug] received task {}", task_id);
                if job.is_foreced_termination().await {
                    println!("[info] task {} is skipped", task_id);
                    return;
                }
                execute_task(&task_id, config, &worker, worker_table, &db).await;
            }));
        }
        join_all(handles).await;
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
    job_table: Arc<RwLock<HashMap<JobId, Job>>>,
    db: Db,
}

impl Manager {
    pub async fn new(
        client_addr: SocketAddr,
        cert_path: String,
        db_username: String,
        db_password: String,
        max_job_request: usize,
        servers: Vec<String>,
    ) -> Result<Self, Box<dyn Error>> {
        let (client, connection) = tokio_postgres::connect(
            &format!(
                "host=localhost user={} password={}",
                db_username, db_password
            ),
            NoTls,
        )
        .await?;
        let db = Db(Arc::new(client));
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                println!("[error] Postresql database connection error: {e}");
            }
        });

        let (job_queue_tx, mut job_queue_rx) = mpsc::channel::<JobQueued>(max_job_request);
        let worker_manager = WorkerManager::new(client_addr, cert_path, servers).await?;

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
            db,
        })
    }

    async fn create_job(&self, config: job::Config) -> Result<String, tokio_postgres::Error> {
        let task_count = config.iteration_count;
        let state = JobState::Created;
        let (job_id, task_ids) = self.db.insert(&state, task_count).await?;

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
                    task_ids,
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

mod worker {
    use std::{
        collections::VecDeque, error::Error, fmt::Display, net::SocketAddr, pin::Pin, sync::Arc,
    };

    use futures_util::{Future, StreamExt};
    use parking_lot::Mutex;
    use quinn::{Connection, Endpoint};
    use repl::nom::AsBytes;
    use tokio::sync::{mpsc, oneshot, RwLock};
    use tokio_util::codec::{FramedRead, LengthDelimitedCodec};
    use uuid::Uuid;

    use worker_if::batch::{Cost, Request, Resource, ResourceMeasure, Response};

    use crate::{app::job, server::Server};

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

    #[derive(Clone, Debug)]
    pub struct WorkerClient {
        inner: Arc<RwLock<Inner>>,
        index: usize,
    }

    impl WorkerClient {
        async fn new(
            endpoint: &mut Endpoint,
            server_info: String,
            index: usize,
            release_tx: mpsc::UnboundedSender<(usize, Resource)>,
        ) -> Result<(Self, ResourceMeasure), Box<dyn Error>> {
            let connection = server_info.parse::<Server>()?.connect(endpoint).await?;

            let mut recv = connection.accept_uni().await?;
            let rm = protocol::quic::read_data::<ResourceMeasure>(&mut recv).await?;
            println!("[info] worker {index} has {rm:?}");

            Ok((
                Self {
                    inner: Arc::new(RwLock::new(Inner {
                        connection,
                        release_tx,
                    })),
                    index,
                },
                rm,
            ))
        }

        pub async fn execute(
            &self,
            task_id: &TaskId,
            config: job::Config,
            termination_tx: oneshot::Sender<Option<bool>>,
        ) -> anyhow::Result<()> {
            self.inner
                .write()
                .await
                .execute(self.index, task_id, config, termination_tx)
                .await
        }

        pub async fn terminate(&self, task_id: &TaskId) -> anyhow::Result<()> {
            self.inner.write().await.terminate(task_id).await
        }
    }

    #[derive(Debug)]
    pub struct Inner {
        connection: Connection,
        release_tx: mpsc::UnboundedSender<(usize, Resource)>,
    }

    impl Inner {
        async fn execute(
            &mut self,
            index: usize,
            task_id: &TaskId,
            config: job::Config,
            termination_tx: oneshot::Sender<Option<bool>>,
        ) -> anyhow::Result<()> {
            let (mut send, recv) = self.connection.open_bi().await?;
            protocol::quic::write_data(
                &mut send,
                &Request::Execute(task_id.to_string(), config.param),
            )
            .await?;

            let mut stream = FramedRead::new(recv, LengthDelimitedCodec::new());
            let res =
                bincode::deserialize::<Resource>(stream.next().await.unwrap()?.as_bytes()).unwrap();
            if let Err(e) =
                bincode::deserialize::<Response<()>>(stream.next().await.unwrap()?.as_bytes())?
                    .as_result()
            {
                self.release_tx.send((index, res)).unwrap();
                return Err(e.into());
            }

            let release_tx = self.release_tx.clone();
            tokio::spawn(async move {
                let exit_status = stream
                    .next()
                    .await
                    .unwrap()
                    .ok()
                    .map(|data| bincode::deserialize::<bool>(data.as_bytes()).unwrap());
                termination_tx.send(exit_status).unwrap();
                release_tx.send((index, res)).unwrap();
            });

            Ok(())
        }

        async fn terminate(&mut self, task_id: &TaskId) -> anyhow::Result<()> {
            let (mut send, mut recv) = self.connection.open_bi().await?;
            protocol::quic::write_data(&mut send, &Request::Terminate(task_id.to_string())).await?;
            protocol::quic::read_data::<Response<()>>(&mut recv)
                .await?
                .as_result()?;
            Ok(())
        }
    }

    #[derive(Debug)]
    struct WorkerPool {
        cost: Cost,
        tx: oneshot::Sender<WorkerClient>,
    }

    impl WorkerPool {
        fn register(self, client: WorkerClient, cr: &mut Resource, res: Resource) {
            *cr -= res;
            self.tx.send(client).unwrap();
            println!("[debug] sent client");
        }
    }

    struct WorkerInfo {
        client: WorkerClient,
        current_resource: parking_lot::RwLock<Resource>,
        measure: ResourceMeasure,
    }

    pub struct WorkerManager {
        queue_tx: mpsc::UnboundedSender<WorkerPool>,
    }

    impl WorkerManager {
        pub async fn new(
            addr: SocketAddr,
            cert_path: String,
            servers: Vec<String>,
        ) -> Result<Self, Box<dyn Error>> {
            let mut endpoint = Endpoint::client(addr)?;
            endpoint.set_default_client_config(quic_config::get_client_config(&cert_path)?);

            let (release_tx, mut release_rx) = mpsc::unbounded_channel();

            let mut workers = Vec::new();
            for (i, server_info) in servers.into_iter().enumerate() {
                let (client, measure) =
                    WorkerClient::new(&mut endpoint, server_info, i, release_tx.clone()).await?;
                workers.push(WorkerInfo {
                    client,
                    current_resource: parking_lot::RwLock::new(measure.max_resource),
                    measure,
                });
            }

            let workers = Arc::new(workers);
            let queue = Arc::new(Mutex::new(VecDeque::new()));

            let (queue_tx, mut queue_rx) = mpsc::unbounded_channel::<WorkerPool>();
            {
                let workers = Arc::clone(&workers);
                let queue = Arc::clone(&queue);
                tokio::spawn(async move {
                    while let Some(pool) = queue_rx.recv().await {
                        let mut min_rate = 1.0;
                        let mut info_res = None;
                        for info in workers.iter() {
                            let cr = info.current_resource.read();
                            let Ok(res) = info.measure.measure(&pool.cost) else {
                                continue;
                            };
                            println!("[debug] {res:?}/{cr:?}");
                            let Some(r) = *cr - &res else {
                                continue;
                            };
                            let rate = r / info.measure.max_resource;
                            if rate <= min_rate {
                                min_rate = rate;
                                info_res = Some((info, res))
                            }
                        }
                        match info_res {
                            Some((info, res)) => {
                                pool.register(
                                    info.client.clone(),
                                    &mut info.current_resource.write(),
                                    res,
                                );
                                println!(
                                    "[debug] current resource of worker {}: ({:?})",
                                    info.client.index,
                                    info.current_resource.read()
                                );
                            }
                            None => {
                                println!("[debug] queue {:?}", pool.cost);
                                queue.lock().push_back(pool);
                            }
                        }
                    }
                });
            }

            tokio::spawn(async move {
                while let Some((index, released)) = release_rx.recv().await {
                    println!("[debug] worker {index} released {released:?}");

                    let info = &workers[index];
                    let mut cr = info.current_resource.write();
                    *cr += released;
                    let mut queue = queue.lock();
                    while let Some(pool) = queue.front() {
                        match info.measure.measure(&pool.cost) {
                            Ok(res) if res <= *cr => {
                                let pool = queue.pop_front().unwrap();
                                println!("[debug] pop pool {:?}", pool.cost);
                                pool.register(info.client.clone(), &mut cr, res);
                                println!(
                                    "[debug] current resource of worker {}: ({:?})",
                                    info.client.index, cr
                                );
                            }
                            _ => break,
                        }
                    }
                }
            });

            Ok(Self { queue_tx })
        }

        pub fn lease(&self, cost: Cost) -> WorkerLease {
            let (tx, rx) = oneshot::channel();
            self.queue_tx.send(WorkerPool { cost, tx }).unwrap();
            WorkerLease(rx)
        }
    }

    pub struct WorkerLease(oneshot::Receiver<WorkerClient>);

    impl Future for WorkerLease {
        type Output = Result<WorkerClient, oneshot::error::RecvError>;

        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            Pin::new(&mut self.0).poll(cx)
        }
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
