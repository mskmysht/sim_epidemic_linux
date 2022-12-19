use std::{
    collections::HashMap,
    process,
    sync::Arc,
    thread::{self, JoinHandle},
};

use world::{IpcSubscriber, Subscriber, WorldStatus};

use ipc_channel::ipc::IpcOneShotServer;
use parking_lot::{Mutex, RwLock};

type Request = worker_if::Request<world_if::Request>;
type Response = worker_if::Response<world_if::ResponseOk>;

fn new_unique_string<const LEN: usize>() -> String {
    use rand::Rng;
    use rand_distr::Alphanumeric;
    rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(LEN)
        .map(char::from)
        .collect()
}

struct WorldInfo {
    subscriber: IpcSubscriber,
    current_status: WorldStatus,
}

impl WorldInfo {
    fn new(subscriber: IpcSubscriber) -> Result<Self, <IpcSubscriber as Subscriber>::Err> {
        let status = subscriber.recv_status()?;
        Ok(Self {
            subscriber,
            current_status: status,
        })
    }
}

pub struct WorldManager {
    path: String,
    info_map: Arc<RwLock<HashMap<String, Mutex<WorldInfo>>>>,
    handles: Vec<JoinHandle<()>>,
}

impl WorldManager {
    pub fn new(path: String) -> Self {
        Self {
            path,
            info_map: Arc::new(RwLock::new(HashMap::default())),
            handles: Vec::new(),
        }
    }

    pub fn close(&mut self) {
        for handle in self.handles.drain(..) {
            handle.join().unwrap();
        }
    }

    fn delete_all(
        &mut self,
    ) -> Vec<(
        String,
        Result<world_if::Response, <IpcSubscriber as Subscriber>::Err>,
    )> {
        let all = self
            .info_map
            .write()
            .drain()
            .map(|(id, info)| {
                let res = info.lock().subscriber.request(world_if::Request::Delete);
                (id, res)
            })
            .collect();
        all
    }

    fn entry_world(&mut self) -> anyhow::Result<String> {
        use std::collections::hash_map::Entry;
        let mut map = self.info_map.write();
        let entry = {
            loop {
                let id = new_unique_string::<3>();
                if let Entry::Vacant(e) = map.entry(id) {
                    break e;
                }
            }
        };
        let id = entry.key().clone();
        let (server, server_name) = IpcOneShotServer::new()?;
        let mut child = process::Command::new(&self.path)
            .args(["--world-id", &id, "--server-name", &server_name])
            .spawn()?;
        let (_, subscriber): (_, IpcSubscriber) = server.accept()?;
        println!("[info] Create World {id}.");
        let handle = {
            let id = id.clone();
            let pid = child.id();
            let map = Arc::clone(&self.info_map);
            thread::spawn(move || {
                let s = child.wait().expect("command was not running.");
                if map.write().remove(&id).is_some() {
                    println!("[warn] World {id} (PID: {pid}) is stopped with exit status {s}.");
                } else {
                    println!("[info] World {id} (PID: {pid}) is closed with exit status {s}.");
                }
            })
        };
        entry.insert(Mutex::new(WorldInfo::new(subscriber)?));
        self.handles.push(handle);
        Ok(id)
    }

    fn get_mut_with<F: FnMut(&mut WorldInfo) -> Response>(&self, id: &str, mut f: F) -> Response {
        let map = self.info_map.read();
        let Some(info) = map.get(id) else {
            return worker_if::ResponseError::NoIdFound.into();
        };
        let mut info = info.lock();
        f(&mut info)
    }

    fn get_with<F: FnOnce(&WorldInfo) -> Response>(&self, id: &str, f: F) -> Response {
        let map = self.info_map.read();
        let Some(info) = map.get(id) else {
            return worker_if::ResponseError::NoIdFound.into();
        };
        let info = info.lock();
        f(&info)
    }

    fn remove_with<F: FnOnce(&WorldInfo) -> Response>(&self, id: &str, f: F) -> Response {
        let mut map = self.info_map.write();
        let Some(info) = map.remove(id) else {
            return worker_if::ResponseError::NoIdFound.into();
        };
        let info = info.lock();
        f(&info)
    }

    pub fn callback(&mut self, req: Request) -> Response {
        match req {
            Request::SpawnItem => match self.entry_world() {
                Ok(id) => worker_if::ResponseOk::Item(id).into(),
                Err(e) => worker_if::ResponseError::FailedToSpawn(e).into(),
            },
            Request::GetItemList => {
                worker_if::ResponseOk::ItemList(self.info_map.read().keys().cloned().collect())
                    .into()
            }
            Request::GetItemInfo(ref id) => self.get_mut_with(id, |info| {
                if let Some(s) = info.subscriber.seek_status().into_iter().last() {
                    info.current_status = s;
                }
                worker_if::ResponseOk::ItemInfo((&info.current_status).to_string()).into()
            }),
            Request::Custom(ref id, req) => {
                self.get_with(id, |info| match info.subscriber.request(req) {
                    Ok(r) => r.as_result().into(),
                    Err(e) => worker_if::ResponseError::process_io_error(e).into(),
                })
            }
            Request::DeleteItem(ref id) => {
                self.remove_with(id, |info| match info.subscriber.delete() {
                    Ok(r) => r.as_result().into(),
                    Err(e) => worker_if::ResponseError::process_io_error(e).into(),
                })
            }
        }
    }
}

impl Drop for WorldManager {
    fn drop(&mut self) {
        for (id, res) in self.delete_all().into_iter() {
            match res {
                Ok(r) => match r {
                    world_if::Response::Ok(_) => println!("[info] Delete World {id}."),
                    world_if::Response::Err(e) => eprintln!("[error] {:?}", e),
                },
                Err(e) => eprintln!("{:?}", e),
            }
        }
        self.close();
    }
}
