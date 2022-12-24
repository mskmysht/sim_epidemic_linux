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
        child_tx: mpsc::SyncSender<String>,
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
                child_tx.send(id).unwrap();
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

#[derive(Clone)]
pub struct WorldManager(Arc<WorldMangerInner>);

impl WorldManager {
    fn new(path: String, child_tx: mpsc::SyncSender<String>) -> Self {
        Self(Arc::new(WorldMangerInner {
            table: Default::default(),
            path,
            child_tx,
        }))
    }

    pub fn request(&self, req: Request) -> Response {
        self.0.request(req)
    }

    fn remove(&self, id: &String) -> Option<Mutex<WorldItem>> {
        self.0.table.write().remove(id)
    }

    fn listen(self, terminate_rx: mpsc::Receiver<()>, child_rx: mpsc::Receiver<String>) {
        loop {
            match terminate_rx.try_recv() {
                Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
                    break;
                }
                _ => {}
            }
            if let Ok(ref id) = child_rx.recv() {
                if let Some(item) = self.remove(id) {
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
        }
    }
}

pub struct Terminal {
    terminate_tx: mpsc::SyncSender<()>,
    handle: Option<JoinHandle<()>>,
}

impl Terminal {
    fn new(terminate_tx: mpsc::SyncSender<()>, handle: JoinHandle<()>) -> Self {
        Self {
            terminate_tx,
            handle: Some(handle),
        }
    }
}

pub fn channel(path: String) -> (WorldManager, Terminal) {
    let (terminate_tx, terminate_rx) = mpsc::sync_channel(1);
    let (child_tx, child_rx) = mpsc::sync_channel(64);
    let manager = WorldManager::new(path, child_tx);

    (
        manager.clone(),
        Terminal::new(
            terminate_tx,
            thread::spawn(move || manager.listen(terminate_rx, child_rx)),
        ),
    )
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.terminate_tx.send(()).unwrap();
        self.handle.take().unwrap().join().unwrap();
    }
}

struct WorldMangerInner {
    table: RwLock<HashMap<String, Mutex<WorldItem>>>,
    path: String,
    child_tx: mpsc::SyncSender<String>,
}

impl WorldMangerInner {
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
        match WorldItem::new(&self.path, id.clone(), self.child_tx.clone()) {
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

impl Drop for WorldMangerInner {
    fn drop(&mut self) {
        for (id, entry) in self.table.write().drain() {
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
    }
}
