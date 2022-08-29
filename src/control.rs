pub mod stdio;

use rand::Rng;
use rand_distr::Alphanumeric;

use crate::world;
use std::{
    collections::HashMap,
    sync::{mpsc, Arc, Mutex},
    thread,
};

pub trait Callback {
    fn init() -> Self;
    fn callback(&mut self, cmd: Command) -> bool;
}

pub enum Command {
    Quit,
    List,
    None,
    New,
    Info(String),
    Delete(String),
    Msg(String, world::Request),
}

pub struct WorldInfo {
    stream: mpsc::Receiver<world::WorldStatus>,
    req: mpsc::Sender<world::Request>,
    res: mpsc::Receiver<world::ResponseResult>,
    status: world::WorldStatus,
}

pub struct MyCallback {
    channels: Arc<Mutex<HashMap<String, WorldInfo>>>,
}

impl Callback for MyCallback {
    fn init() -> Self {
        MyCallback {
            channels: Arc::new(Mutex::new(HashMap::default())),
        }
    }

    fn callback(&mut self, cmd: Command) -> bool {
        let mut channels = self.channels.lock().unwrap();
        match cmd {
            Command::None => {}
            Command::List => {
                for key in channels.keys() {
                    println!("{key}");
                }
            }
            Command::Quit => {
                for (key, info) in channels.iter() {
                    info.req.send(world::Request::Delete).unwrap();
                    match info.res.recv().unwrap() {
                        Ok(_) => println!("Deleted {key}."),
                        Err(err) => eprintln!("{:?}", err),
                    }
                }
                return false;
            }
            Command::New => {
                let (req_tx, req_rx) = mpsc::channel();
                let (res_tx, res_rx) = mpsc::channel();
                let (stream_tx, stream_rx) = mpsc::channel();
                let (drop_tx, drop_rx) = mpsc::channel();

                let id = new_unique_string::<3>();
                match world::spawn_world(id.clone(), stream_tx, req_rx, res_tx, drop_tx) {
                    Ok(status) => {
                        println!("World {id} is created.");
                        channels.insert(
                            id.clone(),
                            WorldInfo {
                                stream: stream_rx,
                                req: req_tx,
                                res: res_rx,
                                status,
                            },
                        );
                        let channels = Arc::clone(&self.channels);
                        thread::spawn(move || {
                            if drop_rx.recv().unwrap() {
                                channels.lock().unwrap().remove(&id);
                                println!("thread was stopped automatically")
                            }
                        });
                    }
                    Err(e) => println!("{e}"),
                }
            }
            Command::Info(ref id) => {
                if let Some(info) = channels.get_mut(id) {
                    if let Some(status) = info.stream.try_iter().last() {
                        info.status = status;
                    }
                    println!("{}", info.status);
                } else {
                    println!("World '{id}' not found.");
                }
            }
            Command::Delete(ref id) => {
                if let Some(info) = channels.remove(id) {
                    info.req.send(world::Request::Delete).unwrap();
                    match info.res.recv().unwrap() {
                        Ok(None) => println!("succeed"),
                        Ok(Some(msg)) => println!("{}", msg),
                        Err(err) => eprintln!("{:?}", err),
                    }
                } else {
                    println!("World '{id}' not found.");
                }
            }
            Command::Msg(ref id, msg) => {
                if let Some(info) = channels.get_mut(id) {
                    info.req.send(msg).unwrap();
                    if let Ok(res) = info.res.recv() {
                        match res {
                            Ok(None) => println!("succeed"),
                            Ok(Some(msg)) => println!("[info] {msg}"),
                            Err(err) => eprintln!("[error] {err:?}"),
                        }
                    }
                } else {
                    println!("World '{id}' not found.");
                }
            }
        }
        true
    }
}

fn new_unique_string<const LEN: usize>() -> String {
    rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(LEN)
        .map(char::from)
        .collect()
}
