pub mod parse;

// pub trait Manager<Req, S, E> {
//     fn new(&mut self) -> io::Result<String>;
//     fn get_ids(&self) -> Vec<String>;
//     fn get_status(&self, id: &String) -> Option<String>;
//     fn delete(&mut self, id: &String) -> Option<Result<S, E>>;
//     fn send(&self, id: &String, req: Req) -> Option<Result<S, E>>;
// }

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Request<T> {
    New,
    List,
    Info(String),
    Delete(String),
    Msg(String, T),
}

impl<T> Request<T> {
    pub fn map<U, F: Fn(T) -> U>(self, f: F) -> Request<U> {
        match self {
            Request::New => Request::New,
            Request::List => Request::List,
            Request::Info(id) => Request::Info(id),
            Request::Delete(id) => Request::Delete(id),
            Request::Msg(id, t) => Request::Msg(id, f(t)),
        }
    }

    pub fn map_r<U, E, F: Fn(T) -> Result<U, E>>(self, f: F) -> Result<Request<U>, E> {
        match self {
            Request::New => Ok(Request::New),
            Request::List => Ok(Request::List),
            Request::Info(id) => Ok(Request::Info(id)),
            Request::Delete(id) => Ok(Request::Delete(id)),
            Request::Msg(id, t) => f(t).map(|u| Request::Msg(id, u)),
        }
    }

    // pub fn eval<S, E, M: Manager<T, S, E>>(self, manager: &mut M) -> Response<S, E> {
    //     match self {
    //         Request::New => match manager.new() {
    //             Ok(id) => Ok(Success::Created(id)),
    //             Err(e) => {
    //                 println!("[error] {e:?}");
    //                 Err(Error::Failure)
    //             }
    //         },
    //         Request::List => Ok(Success::IdList(manager.get_ids())),
    //         Request::Info(ref id) => manager.get_status(id).ok_or(Error::NoId).map(Success::Msg),
    //         Request::Delete(id) => manager
    //             .delete(&id)
    //             .ok_or(Error::NoId)?
    //             .map_err(Error::WorldError)
    //             .map(Success::Accepted),
    //         Request::Msg(id, req) => manager
    //             .send(&id, req)
    //             .ok_or(Error::NoId)?
    //             .map_err(Error::WorldError)
    //             .map(Success::Accepted),
    //     }
    // }
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
