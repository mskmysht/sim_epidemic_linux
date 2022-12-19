pub mod parse;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum Request {
    Delete,
    Start(u64),
    Step,
    Stop,
    Reset,
    Debug,
    Export(String),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum Response {
    Ok(ResponseOk),
    Err(serde_error::Error),
}

impl Response {
    #[inline]
    pub fn as_result(self) -> Result<ResponseOk, serde_error::Error> {
        match self {
            Response::Ok(r) => Ok(r),
            Response::Err(e) => Err(e),
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum ResponseOk {
    Success,
    SuccessWithMessage(String),
}

impl From<ResponseOk> for Response {
    fn from(r: ResponseOk) -> Self {
        Response::Ok(r)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResponseError {
    #[error("world is already finished")]
    AlreadyFinished,
    #[error("world is already stopped")]
    AlreadyStopped,
    #[error("world is already running")]
    AlreadyRunning,
    #[error("failed to export file")]
    FileExportFailed,
}

impl From<ResponseError> for serde_error::Error {
    fn from(e: ResponseError) -> Self {
        serde_error::Error::new(&e)
    }
}

impl From<ResponseError> for Response {
    fn from(e: ResponseError) -> Self {
        Response::Err(e.into())
    }
}
