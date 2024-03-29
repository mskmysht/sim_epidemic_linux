use std::{
    collections::HashMap,
    error::Error,
    fmt::{Debug, Display},
    net::IpAddr,
    sync::Arc,
};

use async_trait::async_trait;
use futures_util::future::join_all;
use tokio::{
    select,
    sync::{mpsc, watch, Notify, RwLock},
};
use tokio_postgres::NoTls;
use tracing::Instrument;
use uuid::Uuid;

use crate::{
    app::{
        job::{self, JobState},
        task, ResourceManager,
    },
    database::Db,
    worker::{ServerConfig, TaskId, WorkerManager},
};

#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub struct JobId(pub Uuid);

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

fn oneshot_notify_channel() -> (OneshotNotifySender, OneshotNotifyReceiver) {
    let (tx, rx) = watch::channel(false);
    (OneshotNotifySender(tx), OneshotNotifyReceiver(rx))
}

#[derive(Debug)]
struct OneshotNotifySender(watch::Sender<bool>);

impl OneshotNotifySender {
    fn notify(self) -> bool {
        self.0.send(true).is_ok()
    }
}

#[derive(Clone, Debug)]
pub struct OneshotNotifyReceiver(watch::Receiver<bool>);

impl OneshotNotifyReceiver {
    pub fn has_notified(&self) -> Result<bool, watch::error::RecvError> {
        self.0.has_changed()
    }

    pub async fn notified(mut self) -> Result<(), watch::error::RecvError> {
        self.0.changed().await
    }
}

struct ForceQuitSignal {
    tx: OneshotNotifySender,
    notify: Arc<Notify>,
}

impl ForceQuitSignal {
    async fn force_quit(self) -> bool {
        if self.tx.notify() {
            self.notify.notified().await;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test_force_quit {
    use std::{sync::Arc, thread, time::Duration};

    use tokio::{
        runtime::Runtime,
        sync::{watch, Notify},
    };

    #[test]
    fn test_force_quit() {
        Runtime::new().unwrap().block_on(async {
            let (tx, mut rx) = watch::channel(false);
            let notify = Arc::new(Notify::new());
            let notify2 = notify.clone();
            tokio::spawn(async move {
                if rx.changed().await.is_ok() {
                    println!("changed");
                    notify2.notify_one();
                    println!("emit notification");
                    // thread::sleep(Duration::from_secs(3));
                }
            });
            let f = match tx.send(true) {
                Ok(_) => {
                    thread::sleep(Duration::from_secs(3));
                    println!("wait notification");
                    notify.notified().await;
                    true
                }
                Err(_) => false,
            };
            println!("{}", f);
        });
    }
}

#[derive(Debug)]
struct Job {
    id: JobId,
    fq_rx: OneshotNotifyReceiver,
    task_ids: Vec<TaskId>,
    config: job::Config,
}

impl Job {
    async fn consume(self, worker_manager: &WorkerManager, db: &Db) {
        let mut handles = Vec::new();
        for task_id in self.task_ids {
            tracing::debug!("received task {}", task_id);
            let lease = worker_manager.lease((&self.config.param).into()).await;
            let fq_rx = self.fq_rx.clone();
            let db = db.clone();
            let config = self.config.clone();
            handles.push(tokio::spawn(async move {
                select! {
                    Ok(_) = fq_rx.clone().notified() => {
                        tracing::info!("{} lease is canceled", task_id);
                    }
                    Ok(worker) = lease => {
                        let span = tracing::debug_span!("task", id = task_id.to_string(), worker = worker.index());
                        worker
                            .execute(
                                &task_id,
                                config,
                                &db,
                                fq_rx,
                            )
                            .instrument(span)
                            .await;
                    }
                }
            }));
        }
        join_all(handles).await;
    }
}

pub struct Manager {
    job_queue_tx: mpsc::Sender<(Job, Arc<Notify>)>,
    queued_jobs: Arc<RwLock<HashMap<JobId, ForceQuitSignal>>>,
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
                tracing::error!("Postresql database connection error: {e}");
            }
        });

        let (job_queue_tx, mut job_queue_rx) = mpsc::channel(max_job_request);
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
            while let Some((job, notify)) = job_queue_rx.recv().await {
                let id = job.id.clone();

                tracing::info!("received job {}", id);
                if !job.fq_rx.has_notified().unwrap() {
                    db.update_job_state(&id, &JobState::Running).await;
                    job.consume(&worker_manager, &db).await;
                }
                notify.notify_waiters();
                queued_jobs.write().await.remove(&id);
                db.update_job_state(&id, &JobState::Completed).await;
                tracing::info!("job {} terminated", id);
            }
        });

        tracing::debug!("Manager is created");
        Ok(manager)
    }

    async fn create_job(&self, config: job::Config) -> Result<String, tokio_postgres::Error> {
        let (job_id, task_ids) = self.db.insert_job(&config).await?;

        if config.iteration_count > 0 {
            let (fq_tx, fq_rx) = oneshot_notify_channel();
            let notify = Arc::new(Notify::new());
            let signal = ForceQuitSignal {
                tx: fq_tx,
                notify: notify.clone(),
            };
            self.queued_jobs
                .write()
                .await
                .insert(job_id.clone(), signal);

            self.job_queue_tx
                .send((
                    Job {
                        id: job_id.clone(),
                        fq_rx,
                        task_ids,
                        config,
                    },
                    notify,
                ))
                .await
                .unwrap();
        }

        Ok(job_id.to_string())
    }

    fn delete_job(&self, id: &JobId) {
        let id = id.clone();
        let queued_jobs = self.queued_jobs.clone();
        let worker_manager = self.worker_manager.clone();
        let db = self.db.clone();
        tokio::spawn(async move {
            if let Some(signal) = queued_jobs.write().await.remove(&id) {
                signal.force_quit().await;
            }
            let mut task_ids_map = vec![Vec::new(); worker_manager.get_worker_count()];
            for (task_id, worker_index) in db.get_all_tasks_with_stats(&id).await.unwrap() {
                task_ids_map[worker_index].push(task_id);
            }
            for (worker_index, task_ids) in task_ids_map.into_iter().enumerate() {
                let worker = worker_manager.get_worker(worker_index);
                match worker.remove_statistics(&task_ids).await {
                    Ok(failed) => {
                        for id in failed {
                            tracing::error!("failed to remove {}", id);
                        }
                    }
                    Err(e) => tracing::error!("{}", e),
                }
            }
            db.delete_job(&id).await.unwrap();
        });
    }

    async fn terminate_job(&self, id: &JobId) -> bool {
        let Some(signal) = self.queued_jobs.write().await.remove(&id) else {
            return false;
        };
        tokio::spawn(async move {
            signal.force_quit().await;
        });
        true
    }

    async fn get_statistics(&self, id: &TaskId) -> anyhow::Result<Option<Vec<u8>>> {
        let Some(worker_index) = self.db.get_worker_index(&id).await? else {
            return Ok(None);
        };
        let client = self.worker_manager.get_worker(worker_index);
        Ok(Some(client.get_statistics(&id).await?))
    }
}

#[async_trait]
impl ResourceManager for Manager {
    async fn create_job(&self, config: job::Config) -> Option<String> {
        match self.create_job(config.clone()).await {
            Ok(id) => Some(id),
            Err(e) => {
                tracing::error!("{}", e);
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

    fn delete_job(&self, id: &str) -> Result<(), uuid::Error> {
        let id = JobId::try_from(id)?;
        self.delete_job(&id);
        Ok(())
    }

    async fn terminate_job(&self, id: &str) -> anyhow::Result<bool> {
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

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use futures_util::{future::join_all, stream::FuturesUnordered, StreamExt};
    use poem_openapi::types::ToJSON;
    use tokio::{runtime::Runtime, sync::Semaphore, time};
    use tokio_postgres::{types::Json, NoTls};

    use super::oneshot_notify_channel;

    #[test]
    fn test_notify() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut futs = Vec::new();
            let (tx, rx) = oneshot_notify_channel();
            tx.notify();

            for i in 0..5 {
                let rx = rx.clone();
                futs.push(async move {
                    time::sleep(Duration::from_secs(1)).await;
                    if rx.notified().await.is_ok() {
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
