use std::{fmt::Debug, result};

pub mod parse;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Request<T> {
    SpawnItem,
    GetItemList,
    GetItemInfo(String),
    DeleteItem(String),
    Custom(String, T),
}

impl<T> Request<T> {
    pub fn map<U, F: Fn(T) -> U>(self, f: F) -> Request<U> {
        match self {
            Request::SpawnItem => Request::SpawnItem,
            Request::GetItemList => Request::GetItemList,
            Request::GetItemInfo(id) => Request::GetItemInfo(id),
            Request::DeleteItem(id) => Request::DeleteItem(id),
            Request::Custom(id, t) => Request::Custom(id, f(t)),
        }
    }

    pub fn try_map<U, E, F: Fn(T) -> result::Result<U, E>>(
        self,
        f: F,
    ) -> result::Result<Request<U>, E> {
        match self {
            Request::SpawnItem => Ok(Request::SpawnItem),
            Request::GetItemList => Ok(Request::GetItemList),
            Request::GetItemInfo(id) => Ok(Request::GetItemInfo(id)),
            Request::DeleteItem(id) => Ok(Request::DeleteItem(id)),
            Request::Custom(id, t) => f(t).map(|u| Request::Custom(id, u)),
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum ResponseOk<T> {
    Item(String),
    ItemList(Vec<String>),
    ItemInfo(String),
    Custom(T),
}

#[derive(Debug, thiserror::Error)]
pub enum ResponseError {
    #[error("failed to spawn item")]
    FailedToSpawn(anyhow::Error),
    #[error("error has occured in the child process")]
    ProcessIOError(anyhow::Error),
    #[error("no id found")]
    NoIdFound,
    #[error("custom error")]
    Custom(#[from] serde_error::Error),
}

impl From<ResponseError> for serde_error::Error {
    fn from(e: ResponseError) -> Self {
        serde_error::Error::new(&e)
    }
}

impl ResponseError {
    pub fn process_io_error<E: std::error::Error + Send + Sync + 'static>(error: E) -> Self {
        Self::ProcessIOError(anyhow::Error::new(error))
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Response<T> {
    Ok(ResponseOk<T>),
    Err(serde_error::Error),
}

impl<T> From<ResponseOk<T>> for Response<T> {
    fn from(r: ResponseOk<T>) -> Self {
        Response::Ok(r)
    }
}

impl<T> From<ResponseError> for Response<T> {
    fn from(e: ResponseError) -> Self {
        Response::Err(e.into())
    }
}

impl<T> From<Result<T, serde_error::Error>> for Response<T> {
    #[inline]
    fn from(r: Result<T, serde_error::Error>) -> Self {
        match r {
            Ok(t) => ResponseOk::Custom(t).into(),
            Err(e) => ResponseError::from(e).into(),
        }
    }
}
