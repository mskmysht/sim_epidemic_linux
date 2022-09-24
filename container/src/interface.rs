pub mod socket;
pub mod stdio;

use ipc_channel::ipc::IpcOneShotServer;
use rand::Rng;
use rand_distr::Alphanumeric;
use std::{
    collections::HashMap,
    io,
    process::Command,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};
use world::{self, WorldStatus};

fn new_unique_string<const LEN: usize>() -> String {
    rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(LEN)
        .map(char::from)
        .collect()
}

pub struct WorldManager {
    path: String,
    info_map: Arc<Mutex<HashMap<String, world::WorldInfo>>>,
    handles: HashMap<String, JoinHandle<()>>,
}

impl WorldManager {
    pub fn new(path: String) -> Self {
        Self {
            path,
            info_map: Arc::new(Mutex::new(HashMap::default())),
            handles: HashMap::default(),
        }
    }

    fn new_world(&mut self) -> io::Result<String> {
        let id = new_unique_string::<3>();
        let (server, server_name) = IpcOneShotServer::new()?;
        let mut child = Command::new(&self.path)
            .args(["--world-id", &id, "--server-name", &server_name])
            .spawn()?;
        let (_, info): (_, world::WorldInfo) = server.accept().unwrap();
        println!("[info] Create World {id}.");
        self.info_map.lock().unwrap().insert(id.clone(), info);
        let handle = {
            let id = id.clone();
            let map = Arc::clone(&self.info_map);
            thread::spawn(move || {
                let s = child.wait().unwrap();
                // println!("[info] child #{id}: {s}");
                map.lock().unwrap().remove(&id).unwrap();
                if !s.success() {
                    println!("[warn] Stopped and deleted World {id} with exit status {s}.");
                }
            })
        };
        self.handles.insert(id.clone(), handle);
        Ok(id)
    }

    fn send(&self, id: &String, req: world::Request) -> Option<world::Response> {
        let map = self.info_map.lock().unwrap();
        let info = map.get(id)?;
        Some(info.send(req))
    }

    fn get_status<R: for<'a> From<&'a WorldStatus>>(&self, id: &String) -> Option<R> {
        let info_map = &mut self.info_map.lock().unwrap();
        let info = info_map.get_mut(id)?;
        Some((info.seek_status()).into())
    }

    fn get_all_ids(&self) -> Vec<String> {
        self.info_map.lock().unwrap().keys().cloned().collect()
    }

    fn delete(&mut self, id: &String) -> Option<world::Response> {
        let res = self.send(id, world::Request::Delete)?;
        self.handles.remove(id).unwrap().join().unwrap();
        Some(res)
    }

    fn delete_all(&mut self) -> Vec<(String, world::Response)> {
        let all = self
            .info_map
            .lock()
            .unwrap()
            .drain()
            .map(|(key, info)| (key, info.send(world::Request::Delete)))
            .collect();
        for (_, handle) in self.handles.drain() {
            handle.join().unwrap();
        }
        all
    }

    // fn reset(&mut self, id: &String) -> Option<world::result::Result> {
    //     self.info_map
    //         .lock()
    //         .unwrap()
    //         .get(id)
    //         .map(|info| info.send(world::Command::Reset))
    // }

    // fn start(&mut self, id: &String, stop_at: u64) -> Option<world::result::Result> {
    //     self.info_map
    //         .lock()
    //         .unwrap()
    //         .get(id)
    //         .map(|info| info.send(world::Command::Start(stop_at)))
    // }

    // fn step(&mut self, id: &String) -> Option<world::result::Result> {
    //     self.info_map
    //         .lock()
    //         .unwrap()
    //         .get(id)
    //         .map(|info| info.send(world::Command::Step))
    // }

    // fn stop(&mut self, id: &String) -> Option<world::result::Result> {
    //     self.info_map
    //         .lock()
    //         .unwrap()
    //         .get(id)
    //         .map(|info| info.send(world::Command::Stop))
    // }

    // fn export(&mut self, id: &String, dir: String) -> Option<world::result::Result> {
    //     self.info_map
    //         .lock()
    //         .unwrap()
    //         .get(id)
    //         .map(|info| info.send(world::Command::Export(dir)))
    // }

    // fn debug(&mut self, id: &String) -> Option<world::result::Result> {
    //     self.info_map
    //         .lock()
    //         .unwrap()
    //         .get(id)
    //         .map(|info| info.send(world::Command::Debug))
    // }
}

// fn interact(
//     info_map: &HashMap<String, world::WorldInfo>,
//     id: &String,
//     msg: world::Command,
// ) -> Option<world::result::Result> {
//     let info = info_map.get(id)?;
//     Some(info.send(msg))
// }
