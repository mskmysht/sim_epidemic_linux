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
use worker_if::realtime::{
    world_if::{IpcSubscriber, Subscriber},
    Request, Response, ResponseError, ResponseOk,
};

fn new_unique_string<const LEN: usize>() -> String {
    use rand::Rng;
    use rand_distr::Alphanumeric;
    rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(LEN)
        .map(char::from)
        .collect()
}

pub struct WorldItem {
    subscriber: IpcSubscriber,
    current_status: <IpcSubscriber as Subscriber>::Stat,
    wait_handle: JoinHandle<ExitStatus>,
    child: Arc<SharedChild>,
    id: String,
    pid: u32,
}

impl WorldItem {
    fn new<P: AsRef<OsStr>>(
        program: P,
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
            let id = id.clone();
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
            id,
            pid,
        })
    }

    pub fn will_drop(self) {
        match self.subscriber.delete() {
            Ok(_) => {
                self.wait_handle.join().unwrap();
                println!("[info] Delete World {}.", self.id);
            }
            Err(e) => {
                self.child.kill().unwrap();
                self.wait_handle.join().unwrap();
                eprintln!("[error] {:?}", e);
            }
        }
    }
}

type WorldTable = Arc<RwLock<HashMap<String, Mutex<WorldItem>>>>;

#[derive(Clone)]
pub struct WorldManager {
    table: WorldTable,
    pub notify_tx: mpsc::SyncSender<Option<String>>,
    path: String,
}

impl WorldManager {
    fn new(path: String, notify_tx: mpsc::SyncSender<Option<String>>) -> Self {
        Self {
            table: WorldTable::default(),
            path,
            notify_tx,
        }
    }

    fn entry_item(&self) -> anyhow::Result<String> {
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
        let item = WorldItem::new(&self.path, id.clone(), self.notify_tx.clone())?;
        entry.insert(Mutex::new(item));
        println!("[info] Create World {id}.");
        Ok(id)
    }

    pub fn request(&self, req: Request) -> Response {
        match req {
            Request::SpawnItem => match self.entry_item() {
                Ok(id) => ResponseOk::Item(id).into(),
                Err(e) => ResponseError::FailedToSpawn(e).into(),
            },
            Request::GetItemList => {
                ResponseOk::ItemList(self.table.read().keys().cloned().collect()).into()
            }
            Request::GetItemInfo(ref id) => self.get_with(id, |entry| {
                let mut entry = entry.lock();
                if let Some(s) = entry.subscriber.seek_status().into_iter().last() {
                    entry.current_status = s;
                }
                ResponseOk::ItemInfo(entry.current_status.clone()).into()
            }),
            Request::Custom(ref id, req) => self.get_with(id, |entry| {
                let entry = entry.lock();
                match entry.subscriber.request(req) {
                    Ok(r) => r.as_result().into(),
                    Err(e) => ResponseError::process_io_error(e).into(),
                }
            }),
            Request::DeleteItem(ref id) => {
                let mut map = self.table.write();
                let Some(entry) = map.remove(id) else {
                    return ResponseError::NoIdFound.into();
                };
                let entry = entry.into_inner();
                match entry.subscriber.delete() {
                    Ok(_) => {
                        entry.wait_handle.join().unwrap();
                        ResponseOk::Deleted.into()
                    }
                    Err(e) => {
                        entry.child.kill().unwrap();
                        entry.wait_handle.join().unwrap();
                        ResponseError::Abort(e.into()).into()
                    }
                }
            }
        }
    }

    #[inline]
    fn get_with<F: FnOnce(&Mutex<WorldItem>) -> Response>(&self, id: &str, f: F) -> Response {
        let map = self.table.read();
        let Some(entry) = map.get(id) else {
            return ResponseError::NoIdFound.into();
        };
        f(entry)
    }

    fn will_drop(&mut self) {
        for (_, entry) in self.table.write().drain() {
            let entry = entry.into_inner();
            entry.will_drop();
        }
        self.notify_tx.send(None).unwrap();
    }
}

pub struct WorldManaging {
    manager: WorldManager,
    notify_handle: Option<JoinHandle<()>>,
}

impl WorldManaging {
    pub fn new(path: String) -> Self {
        let (notify_tx, notify_rx) = mpsc::sync_channel(64);
        let manager = WorldManager::new(path, notify_tx);
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
        self.manager.will_drop();
        self.notify_handle.take().unwrap().join().unwrap();
    }
}
