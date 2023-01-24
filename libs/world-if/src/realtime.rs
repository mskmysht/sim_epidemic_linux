pub mod parse;

use std::fmt;

use crate::pubsub::{Publisher, RequestError, Subscriber};

use chrono::serde::ts_seconds;
use ipc_channel::ipc::{self, IpcReceiver, IpcSender};

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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct IpcSubscriber {
    req_tx: IpcSender<Request>,
    res_rx: IpcReceiver<Response>,
    stream: IpcReceiver<WorldStatus>,
}

impl IpcSubscriber {
    pub fn new(
        req_tx: IpcSender<Request>,
        res_rx: IpcReceiver<Response>,
        stream: IpcReceiver<WorldStatus>,
    ) -> Self {
        Self {
            stream,
            req_tx,
            res_rx,
        }
    }

    pub fn delete(&self) -> Result<Response, RequestError<ipc::IpcError, ipc_channel::Error>> {
        self.request(Request::Delete)
    }
}

impl Subscriber for IpcSubscriber {
    type Req = Request;
    type Res = Response;
    type Stat = WorldStatus;
    type RecvErr = ipc::IpcError;
    type SendErr = ipc_channel::Error;

    fn recv_status(&self) -> Result<WorldStatus, Self::RecvErr> {
        self.stream.recv()
    }

    fn try_recv_status(&self) -> Result<Option<WorldStatus>, Self::RecvErr> {
        match self.stream.try_recv() {
            Ok(s) => Ok(Some(s)),
            Err(ipc::TryRecvError::Empty) => Ok(None),
            Err(ipc::TryRecvError::IpcError(e)) => Err(e),
        }
    }

    fn send(&self, req: Request) -> Result<(), Self::SendErr> {
        self.req_tx.send(req)
    }

    fn recv(&self) -> Result<Response, Self::RecvErr> {
        self.res_rx.recv()
    }
}

pub struct IpcPublisher {
    stream_tx: IpcSender<WorldStatus>,
    req_rx: IpcReceiver<Request>,
    res_tx: IpcSender<Response>,
}

impl IpcPublisher {
    pub fn new(
        stream_tx: IpcSender<WorldStatus>,
        req_rx: IpcReceiver<Request>,
        res_tx: IpcSender<Response>,
    ) -> Self {
        Self {
            stream_tx,
            req_rx,
            res_tx,
        }
    }
}

impl Publisher for IpcPublisher {
    type Req = Request;
    type Res = Response;
    type Stat = WorldStatus;
    type SendErr<T> = ipc_channel::Error;
    type RecvErr = ipc::IpcError;

    fn recv(&self) -> Result<Request, Self::RecvErr> {
        self.req_rx.recv()
    }

    fn try_recv(&self) -> Result<Option<Request>, Self::RecvErr> {
        match self.req_rx.try_recv() {
            Ok(r) => Ok(Some(r)),
            Err(ipc::TryRecvError::Empty) => Ok(None),
            Err(ipc::TryRecvError::IpcError(e)) => Err(e),
        }
    }

    fn send_response(&self, data: Response) -> Result<(), Self::SendErr<Response>> {
        self.res_tx.send(data)
    }

    fn send_on_stream(&self, data: WorldStatus) -> Result<(), Self::SendErr<WorldStatus>> {
        self.stream_tx.send(data)
    }
}
