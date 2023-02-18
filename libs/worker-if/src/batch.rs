pub mod world_if {
    pub use world_if::batch::*;
    pub use world_if::pubsub::*;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Request {
    LaunchItem(String),
    Custom(String, world_if::Request),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum ResponseOk {
    Item,
    Custom(world_if::ResponseOk),
}

#[derive(Debug, thiserror::Error)]
pub enum ResponseError {
    #[error("failed to spawn item")]
    FailedToSpawn(anyhow::Error),
    #[error("error has occured in the child process: {0}")]
    FailedInProcess(anyhow::Error),
    #[error("abort child process")]
    Abort(anyhow::Error),
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
    pub fn process_any_error(error: anyhow::Error) -> Self {
        Self::FailedInProcess(error)
    }
    pub fn process_std_error<E: std::error::Error + Send + Sync + 'static>(error: E) -> Self {
        Self::FailedInProcess(anyhow::Error::new(error))
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Response {
    Ok(ResponseOk),
    Err(serde_error::Error),
}

impl From<ResponseOk> for Response {
    fn from(r: ResponseOk) -> Self {
        Response::Ok(r)
    }
}

impl From<ResponseError> for Response {
    fn from(e: ResponseError) -> Self {
        Response::Err(e.into())
    }
}

impl From<Result<world_if::ResponseOk, serde_error::Error>> for Response {
    #[inline]
    fn from(r: Result<world_if::ResponseOk, serde_error::Error>) -> Self {
        match r {
            Ok(t) => ResponseOk::Custom(t).into(),
            Err(e) => ResponseError::from(e).into(),
        }
    }
}
