use super::{Request, Response, WorldStatus};
use ipc_channel::ipc::{self, IpcReceiver, IpcSender};

pub trait Publisher {
    type SendErr<T>;
    type RecvErr;

    fn recv(&self) -> Result<Request, Self::RecvErr>;
    fn try_recv(&self) -> Result<Option<Request>, Self::RecvErr>;
    fn send_response(&self, data: Response) -> Result<(), Self::SendErr<Response>>;
    fn send_on_stream(&self, data: WorldStatus) -> Result<(), Self::SendErr<WorldStatus>>;
}

#[derive(thiserror::Error, Debug)]
pub enum RequestError<R, S> {
    #[error("receive error")]
    RecvError(R),
    #[error("send error")]
    SendError(S),
}

pub trait Subscriber {
    type RecvErr;
    type SendErr;

    fn recv_status(&self) -> Result<WorldStatus, Self::RecvErr>;
    fn try_recv_status(&self) -> Result<Option<WorldStatus>, Self::RecvErr>;

    fn send(&self, req: Request) -> Result<(), Self::SendErr>;
    fn recv(&self) -> Result<Response, Self::RecvErr>;

    fn seek_status(&self) -> Vec<WorldStatus> {
        let mut v = Vec::new();
        while let Ok(Some(s)) = self.try_recv_status() {
            v.push(s);
        }
        v
    }

    fn request(
        &self,
        req: Request,
    ) -> Result<Response, RequestError<Self::RecvErr, Self::SendErr>> {
        self.send(req).map_err(RequestError::SendError)?;
        self.recv().map_err(RequestError::RecvError)
    }

    fn delete(&self) -> Result<(), RequestError<Self::RecvErr, Self::SendErr>> {
        self.send(Request::Delete)
            .map_err(RequestError::SendError)?;
        self.recv().map(|_| ()).map_err(RequestError::RecvError)
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
}

impl Subscriber for IpcSubscriber {
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
