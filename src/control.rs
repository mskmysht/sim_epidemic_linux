pub mod stdio;

use rand::Rng;
use rand_distr::Alphanumeric;

use crate::world;
use std::{collections::HashMap, sync::mpsc};

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

pub fn callback(cmd: Command, channels: &mut HashMap<String, WorldInfo>) -> bool {
    match cmd {
        Command::None => {}
        Command::List => {
            for key in channels.keys() {
                println!("{key}");
            }
        }
        Command::Quit => {
            for (key, info) in channels.into_iter() {
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
            let id = new_unique_string::<3>();
            let status = world::spawn_world(id.clone(), stream_tx, req_rx, res_tx);
            println!("World {id} is created.");
            channels.insert(
                id,
                WorldInfo {
                    stream: stream_rx,
                    req: req_tx,
                    res: res_rx,
                    status,
                },
            );
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
                match info.res.recv().unwrap() {
                    Ok(None) => println!("succeed"),
                    Ok(Some(msg)) => println!("[info] {}", msg),
                    Err(err) => eprintln!("[error] {:?}", err),
                }
            } else {
                println!("World '{id}' not found.");
            }
        }
    }
    true
}

fn new_unique_string<const LEN: usize>() -> String {
    rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(LEN)
        .map(char::from)
        .collect()
}
