use std::{
    collections::HashMap,
    ffi::OsStr,
    process::{self, ExitStatus},
    sync::{mpsc, Arc},
    thread::{self, JoinHandle},
};

use ipc_channel::ipc::IpcOneShotServer;
use parking_lot::{Mutex, RwLock};
use shared_child::SharedChild;
use worker_if::{
    world_if::{
        self,
        pubsub::{IpcSubscriber, Subscriber},
    },
    Response,
};

type Request = worker_if::Request;

fn new_unique_string<const LEN: usize>() -> String {
    use rand::Rng;
    use rand_distr::Alphanumeric;
    rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(LEN)
        .map(char::from)
        .collect()
}

struct WorldItem {
    subscriber: IpcSubscriber,
    current_status: world_if::WorldStatus,
    wait_handle: JoinHandle<ExitStatus>,
    child: Arc<SharedChild>,
    pid: u32,
}

impl WorldItem {
    fn new<S: AsRef<OsStr>>(
        program: S,
        id: String,
        child_tx: mpsc::SyncSender<Option<String>>,
    ) -> anyhow::Result<Self> {
        let (server, server_name) = IpcOneShotServer::new()?;
        let mut command = process::Command::new(program);
        command.args(["--world-id", &id, "--server-name", &server_name]);
        let child = Arc::new(SharedChild::spawn(&mut command)?);
        let (_, subscriber): (_, IpcSubscriber) = server.accept()?;
        let pid = child.id();
        let wait_handle = {
            let child = Arc::clone(&child);
            thread::spawn(move || {
                let s = child.wait().expect("command was not running.");
                println!("{s:?}");
                child_tx.send(Some(id)).unwrap();
                s
            })
        };
        let current_status = subscriber.recv_status()?;
        Ok(Self {
            subscriber,
            current_status,
            wait_handle,
            child,
            pid,
        })
    }
}

type WorldTable = Arc<RwLock<HashMap<String, Mutex<WorldItem>>>>;

#[derive(Clone)]
pub struct WorldManager {
    table: WorldTable,
    notify_tx: mpsc::SyncSender<Option<String>>,
    path: String,
}

impl WorldManager {
    pub fn request(&self, req: Request) -> Response {
        match req {
            Request::SpawnItem => self.spawn_item(),
            Request::GetItemList => self.get_item_list(),
            Request::GetItemInfo(ref id) => self.get_item_info(id),
            Request::Custom(ref id, req) => self.custom(id, req),
            Request::DeleteItem(ref id) => self.delete(id),
        }
    }

    fn spawn_item(&self) -> Response {
        use std::collections::hash_map::Entry;
        let mut map = self.table.write();
        let entry = {
            loop {
                let id = new_unique_string::<3>();
                if let Entry::Vacant(e) = map.entry(id) {
                    break e;
                }
            }
        };

        let id = entry.key().clone();
        match WorldItem::new(&self.path, id.clone(), self.notify_tx.clone()) {
            Ok(item) => {
                entry.insert(Mutex::new(item));
                println!("[info] Create World {id}.");
                worker_if::ResponseOk::Item(id).into()
            }
            Err(e) => worker_if::ResponseError::FailedToSpawn(e).into(),
        }
    }

    fn get_item_list(&self) -> Response {
        worker_if::ResponseOk::ItemList(self.table.read().keys().cloned().collect()).into()
    }

    fn get_item_info(&self, id: &str) -> Response {
        self.get_with(id, |entry| {
            let mut entry = entry.lock();
            if let Some(s) = entry.subscriber.seek_status().into_iter().last() {
                entry.current_status = s;
            }
            worker_if::ResponseOk::ItemInfo(entry.current_status.clone()).into()
        })
    }

    fn custom(&self, id: &str, req: world_if::Request) -> Response {
        self.get_with(id, |entry| {
            let entry = entry.lock();
            match entry.subscriber.request(req) {
                Ok(r) => r.as_result().into(),
                Err(e) => worker_if::ResponseError::process_io_error(e).into(),
            }
        })
    }

    fn delete(&self, id: &str) -> Response {
        let mut map = self.table.write();
        let Some(entry) = map.remove(id) else {
            return worker_if::ResponseError::NoIdFound.into();
        };
        let entry = entry.into_inner();
        match entry.subscriber.delete() {
            Ok(_) => {
                entry.wait_handle.join().unwrap();
                worker_if::ResponseOk::Deleted.into()
            }
            Err(e) => {
                entry.child.kill().unwrap();
                entry.wait_handle.join().unwrap();
                worker_if::ResponseError::Abort(e.into()).into()
            }
        }
    }

    #[inline]
    fn get_with<F: FnOnce(&Mutex<WorldItem>) -> Response>(&self, id: &str, f: F) -> Response {
        let map = self.table.read();
        let Some(entry) = map.get(id) else {
            return worker_if::ResponseError::NoIdFound.into();
        };
        f(entry)
    }
}

pub struct WorldManaging {
    manager: WorldManager,
    notify_handle: Option<JoinHandle<()>>,
}

impl WorldManaging {
    pub fn new(path: String) -> Self {
        let (notify_tx, notify_rx) = mpsc::sync_channel(64);
        let manager = WorldManager {
            table: WorldTable::default(),
            path,
            notify_tx,
        };
        let table = Arc::clone(&manager.table);
        let notify_handle = Some(thread::spawn(move || loop {
            match notify_rx.recv().unwrap() {
                Some(ref id) => {
                    if let Some(item) = table.write().remove(id) {
                        let item = item.into_inner();
                        let s = item.wait_handle.join().unwrap();
                        if s.success() {
                            println!(
                                "[warn] World {id} (PID: {}) stopped with exit status {s}.",
                                item.pid
                            );
                        } else {
                            eprintln!(
                                "[error] World {id} (PID: {}) aborted with exit status {s}.",
                                item.pid
                            );
                        }
                    }
                }
                None => break,
            }
        }));
        Self {
            manager,
            notify_handle,
        }
    }

    pub fn get_manager(&self) -> &WorldManager {
        &self.manager
    }
}

impl Drop for WorldManaging {
    fn drop(&mut self) {
        for (id, entry) in self.manager.table.write().drain() {
            let entry = entry.into_inner();
            match entry.subscriber.delete() {
                Ok(_) => {
                    entry.wait_handle.join().unwrap();
                    println!("[info] Delete World {id}.");
                }
                Err(e) => {
                    entry.child.kill().unwrap();
                    entry.wait_handle.join().unwrap();
                    eprintln!("[error] {:?}", e);
                }
            }
        }
        self.manager.notify_tx.send(None).unwrap();
        self.notify_handle.take().unwrap().join().unwrap();
    }
}
