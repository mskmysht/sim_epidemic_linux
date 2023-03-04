use std::fmt;

pub use api;
use chrono::serde::ts_seconds;
pub use ipc_channel::ipc::{self, IpcReceiver, IpcSender};
use ipc_channel::ipc::{IpcBytesReceiver, IpcBytesSender};
use serde::{Deserialize, Serialize};

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum Request {
    Execute,
    Terminate,
}

pub type Error = serde_error::Error;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum Response {
    Ok(ResponseOk),
    Err(Error),
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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct IpcBiConnection {
    pub tx: IpcBytesSender,
    pub rx: IpcBytesReceiver,
}

impl IpcBiConnection {
    pub fn new(tx: IpcBytesSender, rx: IpcBytesReceiver) -> Self {
        Self { tx, rx }
    }

    pub fn send<T: Serialize>(&self, data: &T) -> anyhow::Result<()> {
        self.tx.send(&bincode::serialize(data)?)?;
        Ok(())
    }

    pub fn recv<T: for<'de> Deserialize<'de>>(&self) -> anyhow::Result<T> {
        Ok(bincode::deserialize(&self.rx.recv()?)?)
    }

    pub fn try_recv<T: for<'de> Deserialize<'de>>(&self) -> anyhow::Result<Option<T>> {
        match self.rx.try_recv() {
            Ok(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            Err(ipc::TryRecvError::Empty) => Ok(None),
            Err(ipc::TryRecvError::IpcError(e)) => Err(e.into()),
        }
    }
}
