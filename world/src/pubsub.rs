use ipc_channel::ipc::{self, IpcReceiver, IpcSender};
use world_if::{Request, Response};

use crate::world::WorldStatus;

pub trait Publisher {
    type SendError<T>;
    type RecvError;
    type TryRecvError;

    fn recv(&self) -> Result<Request, Self::RecvError>;
    fn try_recv(&self) -> Result<Request, Self::TryRecvError>;
    fn send_response(
        &self,
        data: world_if::Response,
    ) -> Result<(), Self::SendError<world_if::Response>>;
    fn send_on_stream(&self, data: WorldStatus) -> Result<(), Self::SendError<WorldStatus>>;
}

#[derive(thiserror::Error, Debug)]
pub enum SubscriberError<TR, R, S> {
    #[error("try receive error")]
    TryRecvError(TR),
    #[error("receive error")]
    RecvError(R),
    #[error("send error")]
    SendError(S),
}

pub trait Subscriber {
    type Err;

    fn recv_status(&self) -> Result<WorldStatus, Self::Err>;
    fn try_recv_status(&self) -> Result<WorldStatus, Self::Err>;

    fn send(&self, req: Request) -> Result<(), Self::Err>;
    fn recv(&self) -> Result<Response, Self::Err>;

    fn seek_status(&self) -> Vec<WorldStatus> {
        let mut v = Vec::new();
        while let Ok(s) = self.try_recv_status() {
            v.push(s);
        }
        v
    }

    fn request(&self, req: Request) -> Result<Response, Self::Err> {
        self.send(req)?;
        Ok(self.recv()?)
    }

    fn delete(&self) -> Result<Response, Self::Err> {
        self.send(Request::Delete)?;
        Ok(self.recv()?)
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct IpcSubscriber {
    req_tx: IpcSender<Request>,
    res_rx: IpcReceiver<world_if::Response>,
    stream: IpcReceiver<WorldStatus>,
}

impl IpcSubscriber {
    pub fn new(
        req_tx: IpcSender<Request>,
        res_rx: IpcReceiver<world_if::Response>,
        stream: IpcReceiver<WorldStatus>,
    ) -> Self {
        Self {
            stream,
            req_tx,
            res_rx,
        }
    }
}

impl Subscriber for IpcSubscriber {
    type Err = SubscriberError<ipc::TryRecvError, ipc::IpcError, ipc_channel::Error>;

    fn recv_status(&self) -> Result<WorldStatus, Self::Err> {
        self.stream.recv().map_err(SubscriberError::RecvError)
    }

    fn try_recv_status(&self) -> Result<WorldStatus, Self::Err> {
        self.stream
            .try_recv()
            .map_err(SubscriberError::TryRecvError)
    }

    fn send(&self, req: Request) -> Result<(), Self::Err> {
        self.req_tx.send(req).map_err(SubscriberError::SendError)
    }

    fn recv(&self) -> Result<Response, Self::Err> {
        self.res_rx.recv().map_err(SubscriberError::RecvError)
    }
}

pub struct IpcPublisher {
    stream_tx: IpcSender<WorldStatus>,
    req_rx: IpcReceiver<Request>,
    res_tx: IpcSender<world_if::Response>,
}

impl IpcPublisher {
    pub fn new(
        stream_tx: IpcSender<WorldStatus>,
        req_rx: IpcReceiver<Request>,
        res_tx: IpcSender<world_if::Response>,
    ) -> Self {
        Self {
            stream_tx,
            req_rx,
            res_tx,
        }
    }
}

impl Publisher for IpcPublisher {
    type SendError<T> = ipc_channel::Error;
    type RecvError = ipc::IpcError;
    type TryRecvError = ipc::TryRecvError;

    fn recv(&self) -> Result<Request, Self::RecvError> {
        self.req_rx.recv()
    }

    fn try_recv(&self) -> Result<Request, Self::TryRecvError> {
        self.req_rx.try_recv()
    }

    fn send_response(
        &self,
        data: world_if::Response,
    ) -> Result<(), Self::SendError<world_if::Response>> {
        self.res_tx.send(data)
    }

    fn send_on_stream(&self, data: WorldStatus) -> Result<(), Self::SendError<WorldStatus>> {
        self.stream_tx.send(data)
    }
}
