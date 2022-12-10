pub mod parse;

use chrono::serde::ts_seconds;
use ipc_channel::ipc::{IpcReceiver, IpcSender};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum LoopMode {
    LoopNone,
    LoopRunning,
    LoopFinished,
    LoopEndByUser,
    LoopEndAsDaysPassed,
    //[todo] LoopEndByCondition,
    //[todo] LoopEndByTimeLimit,
}

impl Default for LoopMode {
    fn default() -> Self {
        LoopMode::LoopNone
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct WorldStatus {
    step: u64,
    mode: LoopMode,
    #[serde(with = "ts_seconds")]
    time_stamp: chrono::DateTime<chrono::Utc>,
}

impl WorldStatus {
    pub fn new(step: u64, mode: LoopMode) -> Self {
        Self {
            step,
            mode,
            time_stamp: chrono::Utc::now(),
        }
    }
}

impl fmt::Display for WorldStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}]step:{},mode:{:?}",
            self.time_stamp, self.step, self.mode
        )
    }
}

impl From<&WorldStatus> for String {
    fn from(status: &WorldStatus) -> Self {
        status.to_string()
    }
}

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
    Success,
    SuccessWithMessage(String),
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

pub type Result = std::result::Result<Response, serde_error::Error>;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct WorldInfo {
    req: IpcSender<Request>,
    res: IpcReceiver<Result>,
    stream: IpcReceiver<WorldStatus>,
    status: WorldStatus,
}

impl WorldInfo {
    pub fn new(
        req: IpcSender<Request>,
        res: IpcReceiver<Result>,
        stream: IpcReceiver<WorldStatus>,
        status: WorldStatus,
    ) -> Self {
        Self {
            stream,
            req,
            res,
            status,
        }
    }

    pub fn seek_status(&mut self) -> &WorldStatus {
        let mut v = None;
        while let Ok(s) = self.stream.try_recv() {
            v = Some(s);
        }
        if let Some(s) = v {
            self.status = s;
        }
        &self.status
    }

    pub fn send(&self, req: Request) -> Result {
        self.req.send(req).unwrap();
        self.res.recv().unwrap()
    }

    pub fn delete(&self) -> Result {
        self.req.send(Request::Delete).unwrap();
        self.res.recv().unwrap()
    }
}
