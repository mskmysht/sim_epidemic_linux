use std::result;

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
pub enum Response<T> {
    Item(String),
    ItemList(Vec<String>),
    ItemInfo(String),
    Custom(T),
}

#[derive(Debug, thiserror::Error)]
pub enum ResponseError {
    #[error("failed to spawn item")]
    FailedToSpawn(#[from] std::io::Error),
    #[error("no id found")]
    NoIdFound,
    #[error("custom error")]
    Custom(#[from] anyhow::Error),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Error(serde_error::Error);

impl Error {
    pub fn new(e: &ResponseError) -> Self {
        Self(serde_error::Error::new(e))
    }

    pub fn no_id_found() -> Self {
        Self::new(&ResponseError::NoIdFound)
    }

    pub fn custom<E: std::error::Error + Send + Sync + 'static>(error: E) -> Self {
        Self::new(&ResponseError::Custom(anyhow::Error::new(error)))
    }
}

pub type Result<T> = result::Result<Response<T>, Error>;

pub fn from_result<T, E, R>(from: R) -> Result<T>
where
    E: std::error::Error + Send + Sync + 'static,
    R: Into<result::Result<T, E>>,
{
    match from.into() {
        Ok(s) => Ok(Response::Custom(s)),
        Err(e) => Err(Error::custom(e)),
    }
}
