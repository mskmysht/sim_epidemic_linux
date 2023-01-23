pub mod parse;
pub mod pubsub;

use std::fmt;

use chrono::serde::ts_seconds;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum Request {
    Delete,
    Start(u32),
    Step,
    Stop,
    Reset,
    #[cfg(debug_assertions)]
    Debug,
    Export(String),
    Execute(u32),
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
    AlreadyEnded,
    #[error("world is already stopped")]
    AlreadyStopped,
    #[error("world is already running")]
    AlreadyStarted,
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

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub enum WorldState {
    Stopped,
    Started,
    Ended,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct WorldStatus {
    step: u32,
    state: WorldState,
    #[serde(with = "ts_seconds")]
    time_stamp: chrono::DateTime<chrono::Utc>,
}

impl WorldStatus {
    pub fn new(step: u32, state: WorldState) -> Self {
        Self {
            step,
            state,
            time_stamp: chrono::Utc::now(),
        }
    }
}

impl fmt::Display for WorldStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}]step:{},mode:{:?}",
            self.time_stamp, self.step, self.state
        )
    }
}
