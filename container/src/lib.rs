pub mod world {
    use container_if as cif;
    use ipc_channel::ipc::IpcOneShotServer;
    use parking_lot::{Mutex, RwLock};
    use protocol::SyncCallback;
    use std::{
        collections::HashMap,
        io, process,
        sync::Arc,
        thread::{self, JoinHandle},
    };
    use world_if as wif;
    use world_if::WorldInfo;

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

        pub fn delete_all(&mut self) -> Vec<(String, wif::Response)> {
            let all = self
                .info_map
                .write()
                .drain()
                .map(|(id, info)| {
                    let res = info.lock().send(wif::Request::Delete);
                    (id, res)
                })
                .collect();
            all
        }

        pub fn entry(&mut self) -> io::Result<String> {
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

        pub fn send(&self, id: &String, req: wif::Request) -> Option<wif::Response> {
            let map = self.info_map.read();
            let info = map.get(id)?.lock();
            Some(info.send(req))
        }

        pub fn get_status(&self, id: &String) -> Option<String> {
            let map = self.info_map.read();
            let mut info = map.get(id)?.lock();
            Some((info.seek_status()).into())
        }

        pub fn get_ids(&self) -> Vec<String> {
            self.info_map.read().keys().cloned().collect()
        }

        pub fn delete(&mut self, id: &String) -> Option<wif::Response> {
            let mut map = self.info_map.write();
            let info = map.remove(id)?;
            let info = info.lock();
            Some(info.send(wif::Request::Delete))
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

    impl SyncCallback for WorldManager {
        type Req = cif::Request<wif::Request>;
        type Ret = cif::Response<wif::Success, wif::ErrorStatus>;

        fn callback(&mut self, req: Self::Req) -> Self::Ret {
            match req {
                cif::Request::New => match self.entry() {
                    Ok(id) => Ok(cif::Success::Created(id)),
                    Err(e) => {
                        println!("[error] {e:?}");
                        Err(cif::Error::Failure)
                    }
                },
                cif::Request::List => Ok(cif::Success::IdList(self.get_ids())),
                cif::Request::Info(ref id) => self
                    .get_status(id)
                    .ok_or(cif::Error::NoId)
                    .map(cif::Success::Msg),
                cif::Request::Delete(id) => self
                    .delete(&id)
                    .ok_or(cif::Error::NoId)?
                    .map_err(cif::Error::WorldError)
                    .map(cif::Success::Accepted),
                cif::Request::Msg(id, req) => self
                    .send(&id, req)
                    .ok_or(cif::Error::NoId)?
                    .map_err(cif::Error::WorldError)
                    .map(cif::Success::Accepted),
            }
        }
    }
}

/*
impl Manager<Request, Success, ErrorStatus> for WorldManager {
    fn new(&mut self) -> io::Result<String> {
        use std::collections::hash_map::Entry;
        let mut map = self.info_map.lock().unwrap();
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
                if map.lock().unwrap().remove(&id).is_some() {
                    println!("[warn] stopped world {id} process with exit status {s}.");
                } else {
                    println!("[info] closed world {id} process with exit status {s}.");
                }
            })
        };
        entry.insert(info);
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
*/

pub mod stdio {
    use crate::world::WorldManager;
    use async_trait::async_trait;
    use container_if as cif;
    use protocol::{
        stdio::{InputLoop, ParseResult},
        AsyncCallback, SyncCallback,
    };
    use world_if as wif;

    type Req = cif::Request<wif::Request>;
    type Ret = cif::Response<wif::Success, wif::ErrorStatus>;

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

    // impl Drop for StdListener {
    //     fn drop(&mut self) {
    //         for (id, res) in self.manager.delete_all().into_iter() {
    //             match res {
    //                 Ok(_) => println!("Deleted {id}."),
    //                 Err(err) => eprintln!("{:?}", err),
    //             }
    //         }
    //         self.manager.close();
    //     }
    // }

    impl InputLoop<Req, Ret> for StdListener {
        fn parse(input: &str) -> ParseResult<Req> {
            protocol::parse::request(input)
        }

        fn logging(ret: Ret) {
            match ret {
                Ok(s) => println!("[info] {s:?}"),
                Err(e) => eprintln!("[error] {e:?}"),
            }
        }
    }

    impl SyncCallback for StdListener {
        type Req = Req;
        type Ret = Ret;

        fn callback(&mut self, req: Self::Req) -> Self::Ret {
            self.manager.callback(req)
        }
    }

    #[async_trait]
    impl AsyncCallback for StdListener {
        type Req = Req;
        type Ret = Ret;

        async fn callback(&mut self, req: Self::Req) -> Self::Ret {
            self.manager.callback(req)
        }
    }
}

/*
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
*/
