use container_if::Manager;
use ipc_channel::ipc::IpcOneShotServer;
use rand::Rng;
use rand_distr::Alphanumeric;
use std::{
    collections::HashMap,
    io, process,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};
use world_if::{ErrorStatus, Request, Response, Success, WorldInfo};

fn new_unique_string<const LEN: usize>() -> String {
    rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(LEN)
        .map(char::from)
        .collect()
}

pub struct WorldManager {
    path: String,
    info_map: Arc<Mutex<HashMap<String, WorldInfo>>>,
    handles: Vec<JoinHandle<()>>,
}

impl WorldManager {
    pub fn new(path: String) -> Self {
        Self {
            path,
            info_map: Arc::new(Mutex::new(HashMap::default())),
            handles: Vec::new(),
        }
    }

    fn close(&mut self) {
        for handle in self.handles.drain(..) {
            handle.join().unwrap();
        }
    }

    fn delete_all(&mut self) -> Vec<(String, Response)> {
        let all = self
            .info_map
            .lock()
            .unwrap()
            .drain()
            .map(|(id, info)| {
                let res = info.send(Request::Delete);
                (id, res)
            })
            .collect();
        all
    }
}

impl Manager<Request, Success, ErrorStatus> for WorldManager {
    fn new(&mut self) -> io::Result<String> {
        let id = new_unique_string::<3>();
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
                // println!("[info] child #{id}: {s}");
                if map.lock().unwrap().remove(&id).is_some() {
                    println!("[warn] stopped world {id} process with exit status {s}.");
                } else {
                    println!("[info] closed world {id} process with exit status {s}.");
                }
            })
        };
        self.info_map.lock().unwrap().insert(id.clone(), info);
        self.handles.push(handle);
        Ok(id)
    }

    fn send(&self, id: &String, req: Request) -> Option<Response> {
        let map = self.info_map.lock().unwrap();
        let info = map.get(id)?;
        Some(info.send(req))
    }

    fn get_status(&self, id: &String) -> Option<String> {
        let info_map = &mut self.info_map.lock().unwrap();
        let info = info_map.get_mut(id)?;
        Some((info.seek_status()).into())
    }

    fn get_ids(&self) -> Vec<String> {
        self.info_map.lock().unwrap().keys().cloned().collect()
    }

    fn delete(&mut self, id: &String) -> Option<Response> {
        let info = self.info_map.lock().unwrap().remove(id)?;
        Some(info.send(Request::Delete))
    }
}

pub mod stdio {
    use container_if::Request as CReq;
    use protocol::stdio::{InputLoop, ParseResult};
    use world_if::Request as WReq;

    use crate::WorldManager;

    pub struct StdListener {
        manager: WorldManager,
    }

    impl StdListener {
        pub fn new(path: String) -> Self {
            Self {
                manager: WorldManager::new(path),
            }
        }
    }

    impl InputLoop for StdListener {
        type Req = CReq<WReq>;
        type Res = container_if::Response<world_if::Success, world_if::ErrorStatus>;

        fn parse(input: &str) -> ParseResult<Self::Req> {
            protocol::parse::request(input)
        }

        fn quit(&mut self) {
            for (id, res) in self.manager.delete_all().into_iter() {
                match res {
                    Ok(_) => println!("Deleted {id}."),
                    Err(err) => eprintln!("{:?}", err),
                }
            }
            self.manager.close();
        }

        fn callback(&mut self, req: Self::Req) -> Self::Res {
            req.eval(&mut self.manager)
        }

        fn logging(res: Self::Res) {
            match res {
                Ok(s) => println!("[info] {s:?}"),
                Err(e) => eprintln!("[error] {e:?}"),
            }
        }
    }
}

pub mod event {
    use std::net::TcpStream;

    use super::WorldManager;

    pub fn event_loop(stream: &mut TcpStream, manager: &mut WorldManager) {
        let mut buf = [0; 1];
        loop {
            if matches!(stream.peek(&mut buf), Ok(0)) {
                break;
            }
            if let Err(e) = callback(stream, manager) {
                println!("[error] {e}");
            }
        }
    }

    pub fn callback(stream: &mut TcpStream, manager: &mut WorldManager) -> std::io::Result<usize> {
        let req: container_if::Request<world_if::Request> = protocol::read_data(stream)?;
        println!("[request] {req:?}");
        let res = req.eval(manager);
        println!("[response] {res:?}");
        protocol::write_data(stream, &res)
    }
}
