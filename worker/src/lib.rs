use std::{
    collections::HashMap,
    io, process,
    sync::Arc,
    thread::{self, JoinHandle},
};

use world_if::WorldInfo;

use ipc_channel::ipc::IpcOneShotServer;
use parking_lot::{Mutex, RwLock};

type Req = worker_if::Request<world_if::Request>;
type Res = worker_if::Result<world_if::Response>;

fn new_unique_string<const LEN: usize>() -> String {
    use rand::Rng;
    use rand_distr::Alphanumeric;
    rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(LEN)
        .map(char::from)
        .collect()
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

    fn delete_all(&mut self) -> Vec<(String, world_if::Result)> {
        let all = self
            .info_map
            .write()
            .drain()
            .map(|(id, info)| {
                let res = info.lock().send(world_if::Request::Delete);
                (id, res)
            })
            .collect();
        all
    }

    fn entry_world(&mut self) -> io::Result<String> {
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
        let (_, info): (_, WorldInfo) = server.accept().unwrap();
        println!("[info] Create World {id}.");
        let handle = {
            let id = id.clone();
            let map = Arc::clone(&self.info_map);
            thread::spawn(move || {
                let s = child.wait().unwrap();
                if map.write().remove(&id).is_some() {
                    println!("[warn] stopped world {id} process with exit status {s}.");
                } else {
                    println!("[info] closed world {id} process with exit status {s}.");
                }
            })
        };
        entry.insert(Mutex::new(info));
        self.handles.push(handle);
        Ok(id)
    }

    fn get_mut_with<F: FnMut(&mut WorldInfo) -> Res>(&self, id: &str, mut f: F) -> Res {
        let map = self.info_map.read();
        let Some(info) = map.get(id) else {
            return Err(worker_if::Error::no_id_found());
        };
        let mut info = info.lock();
        f(&mut info)
    }

    fn get_with<F: FnOnce(&WorldInfo) -> Res>(&self, id: &str, f: F) -> Res {
        let map = self.info_map.read();
        let Some(info) = map.get(id) else {
            return Err(worker_if::Error::no_id_found());
        };
        let info = info.lock();
        f(&info)
    }

    fn remove_with<F: FnOnce(&WorldInfo) -> Res>(&self, id: &str, f: F) -> Res {
        let mut map = self.info_map.write();
        let Some(info) = map.remove(id) else {
            return Err(worker_if::Error::no_id_found());
        };
        let info = info.lock();
        f(&info)
    }

    pub fn callback(&mut self, req: Req) -> Res {
        match req {
            Req::SpawnItem => match self.entry_world() {
                Ok(id) => Ok(worker_if::Response::Item(id)),
                Err(e) => Err(worker_if::Error::new(&e.into())),
            },
            Req::GetItemList => Ok(worker_if::Response::ItemList(
                self.info_map.read().keys().cloned().collect(),
            )),
            Req::GetItemInfo(ref id) => self.get_mut_with(id, |info| {
                Ok(worker_if::Response::ItemInfo((info.seek_status()).into()))
            }),
            Req::Custom(ref id, req) => {
                self.get_with(id, |info| worker_if::from_result(info.send(req)))
            }
            Req::DeleteItem(ref id) => {
                self.remove_with(id, |info| worker_if::from_result(info.delete()))
            }
        }
    }
}

impl Drop for WorldManager {
    fn drop(&mut self) {
        for (id, res) in self.delete_all().into_iter() {
            match res {
                Ok(_) => println!("Deleted {id}."),
                Err(err) => eprintln!("{:?}", err),
            }
        }
        self.close();
    }
}
