#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Request {
    New,
    List,
    Info(String),
    Delete(String),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Success {
    Created(String),
    Deleted(String),
    Accepted,
    IdList(Vec<String>),
    Msg(String),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Error {
    NoId,
    Failure,
    ParseError,
    WorldError(world::ErrorStatus),
}

pub type Response = Result<Success, Error>;

impl From<Error> for Response {
    fn from(e: Error) -> Self {
        Err(e)
    }
}

pub mod event {
    use std::net::TcpStream;

    use super::{Error, Request, Success};
    use crate::interface::WorldManager;

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
        let req = comm::read_data(stream)?;
        println!("[request] {req:?}");
        let res = eval_request(manager, req);
        println!("[response] {res:?}");
        // let res = match io::deserialize(&data) {
        //     Ok(req) => {}
        //     Err(_) => Error::ParseError.into(),
        // };
        comm::write_data(stream, &res)
    }

    fn eval_request(manager: &mut WorldManager, req: Request) -> super::Response {
        match req {
            Request::New => match manager.new_world() {
                Ok(id) => Ok(Success::Created(id)),
                Err(e) => {
                    println!("[error] {e:?}");
                    Err(Error::Failure)
                }
            },
            Request::List => Ok(Success::IdList(manager.get_all_ids())),
            Request::Info(ref id) => manager.get_status(id).ok_or(Error::NoId).map(Success::Msg),
            Request::Delete(id) => {
                let msg = manager
                    .delete(&id)
                    .ok_or(Error::NoId)?
                    .map_err(Error::WorldError)?;
                Ok(msg.map_or(Success::Deleted(id), Success::Msg))
            }
        }
    }
}
