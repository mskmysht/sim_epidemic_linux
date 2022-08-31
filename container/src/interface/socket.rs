#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Request {
    New,
    List,
    Info(String),
    Delete(String),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Response {
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

impl From<Error> for Result {
    fn from(e: Error) -> Self {
        Err(e)
    }
}

pub type Result = std::result::Result<Response, Error>;

pub mod event {
    use std::{io, net::TcpStream};

    use super::{Error, Request, Response};
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

    pub fn callback(stream: &mut TcpStream, manager: &mut WorldManager) -> io::Result<usize> {
        let data = net::read_data(stream)?;
        let res = match net::deserialize(&data) {
            Ok(req) => {
                println!("[request] {req:?}");
                let res = eval_request(manager, req);
                println!("[response] {res:?}");
                res
            }
            Err(_) => Error::ParseError.into(),
        };
        net::write_data(stream, &net::serialize(&res).unwrap())
    }

    fn eval_request(manager: &mut WorldManager, req: Request) -> super::Result {
        match req {
            Request::New => match manager.new_world() {
                Ok(id) => Ok(Response::Created(id)),
                Err(e) => {
                    println!("[error] {e:?}");
                    Err(Error::Failure)
                }
            },
            Request::List => Ok(Response::IdList(manager.get_all_ids())),
            Request::Info(ref id) => manager.get_info(id).ok_or(Error::NoId).map(Response::Msg),
            Request::Delete(id) => {
                let msg = manager
                    .delete(&id)
                    .ok_or(Error::NoId)?
                    .map_err(Error::WorldError)?;
                Ok(msg.map_or(Response::Deleted(id), Response::Msg))
            }
        }
    }
}
