use std::io;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Request<M> {
    New,
    List,
    Info(String),
    Delete(String),
    Msg(String, M),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Success<S> {
    Created(String),
    Accepted(S),
    Msg(String),
    IdList(Vec<String>),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Error<E> {
    NoId,
    Failure,
    ParseError,
    WorldError(E),
}

pub type Response<S, E> = Result<Success<S>, Error<E>>;

impl<S, E> From<Error<E>> for Response<S, E> {
    fn from(e: Error<E>) -> Self {
        Err(e)
    }
}

pub trait Manager<Q, S, E> {
    fn new(&mut self) -> io::Result<String>;
    fn get_ids(&self) -> Vec<String>;
    fn get_status(&self, id: &String) -> Option<String>;
    fn delete(&mut self, id: &String) -> Option<Result<S, E>>;
    fn send(&self, id: &String, req: Q) -> Option<Result<S, E>>;
}

impl<Q> Request<Q> {
    pub fn eval<S, E, M: Manager<Q, S, E>>(self, manager: &mut M) -> Response<S, E> {
        match self {
            Request::New => match manager.new() {
                Ok(id) => Ok(Success::Created(id)),
                Err(e) => {
                    println!("[error] {e:?}");
                    Err(Error::Failure)
                }
            },
            Request::List => Ok(Success::IdList(manager.get_ids())),
            Request::Info(ref id) => manager.get_status(id).ok_or(Error::NoId).map(Success::Msg),
            Request::Delete(id) => manager
                .delete(&id)
                .ok_or(Error::NoId)?
                .map_err(Error::WorldError)
                .map(Success::Accepted),
            Request::Msg(id, req) => manager
                .send(&id, req)
                .ok_or(Error::NoId)?
                .map_err(Error::WorldError)
                .map(Success::Accepted),
        }
    }
}
