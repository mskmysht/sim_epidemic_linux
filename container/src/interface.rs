pub mod socket;
pub mod stdio;

use rand::Rng;
use rand_distr::Alphanumeric;
use std::{
    collections::HashMap,
    io,
    sync::{mpsc, Arc, Mutex},
    thread,
};
use world::{self, WorldStatus};

pub struct WorldInfo {
    stream: mpsc::Receiver<world::WorldStatus>,
    req: mpsc::Sender<world::Command>,
    res: mpsc::Receiver<world::result::Result>,
    status: world::WorldStatus,
}

fn new_unique_string<const LEN: usize>() -> String {
    rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(LEN)
        .map(char::from)
        .collect()
}

pub struct WorldManager {
    info_map: Arc<Mutex<HashMap<String, WorldInfo>>>,
}

impl WorldManager {
    pub fn new() -> Self {
        Self {
            info_map: Arc::new(Mutex::new(HashMap::default())),
        }
    }

    fn new_world(&mut self) -> io::Result<String> {
        let (req_tx, req_rx) = mpsc::channel();
        let (res_tx, res_rx) = mpsc::channel();
        let (stream_tx, stream_rx) = mpsc::channel();
        let (drop_tx, drop_rx) = mpsc::channel();

        let id = new_unique_string::<3>();
        let status = world::spawn_world(id.clone(), stream_tx, req_rx, res_tx, drop_tx)?;
        println!("[info] Create World {id}.");
        self.info_map.lock().unwrap().insert(
            id.clone(),
            WorldInfo {
                stream: stream_rx,
                req: req_tx,
                res: res_rx,
                status,
            },
        );
        let map = Arc::clone(&self.info_map);
        {
            let id = id.clone();
            thread::spawn(move || {
                if drop_rx.recv().is_ok() {
                    map.lock().unwrap().remove(&id);
                    println!(
                        "[warn] Stopped and deleted World {id} because a local panic was raised.",
                    );
                }
            });
        }
        Ok(id)
    }

    fn get_info<R: for<'a> From<&'a WorldStatus>>(&self, id: &String) -> Option<R> {
        let info_map = &mut self.info_map.lock().unwrap();
        let info = info_map.get_mut(id)?;
        if let Some(status) = info.stream.try_iter().last() {
            info.status = status;
        }
        Some((&info.status).into())
    }

    fn get_all_ids(&self) -> Vec<String> {
        self.info_map.lock().unwrap().keys().cloned().collect()
    }

    fn delete(&mut self, id: &String) -> Option<world::result::Result> {
        let info = self.info_map.lock().unwrap().remove(id)?;
        info.req.send(world::Command::Delete).unwrap();
        Some(info.res.recv().unwrap())
    }

    fn delete_all(&mut self) -> Vec<(String, world::result::Result)> {
        self.info_map
            .lock()
            .unwrap()
            .drain()
            .map(|(key, info)| {
                info.req.send(world::Command::Delete).unwrap();
                (key, info.res.recv().unwrap())
            })
            .collect()
    }

    fn reset(&mut self, id: &String) -> Option<world::result::Result> {
        interact(&self.info_map.lock().unwrap(), id, world::Command::Reset)
    }

    fn start(&mut self, id: &String, stop_at: u64) -> Option<world::result::Result> {
        interact(
            &self.info_map.lock().unwrap(),
            id,
            world::Command::Start(stop_at),
        )
    }

    fn step(&mut self, id: &String) -> Option<world::result::Result> {
        interact(&self.info_map.lock().unwrap(), id, world::Command::Step)
    }

    fn stop(&mut self, id: &String) -> Option<world::result::Result> {
        interact(&self.info_map.lock().unwrap(), id, world::Command::Stop)
    }

    fn export(&mut self, id: &String, dir: String) -> Option<world::result::Result> {
        interact(
            &self.info_map.lock().unwrap(),
            id,
            world::Command::Export(dir),
        )
    }

    fn debug(&mut self, id: &String) -> Option<world::result::Result> {
        interact(&self.info_map.lock().unwrap(), id, world::Command::Debug)
    }
}

fn interact(
    info_map: &HashMap<String, WorldInfo>,
    id: &String,
    msg: world::Command,
) -> Option<world::result::Result> {
    let info = info_map.get(id)?;
    info.req.send(msg).unwrap();
    Some(info.res.recv().unwrap())
}
