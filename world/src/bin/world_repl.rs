use std::{sync::mpsc, thread};

use world::{Publisher, Subscriber, SubscriberError, WorldStatus};
use world_if::Request;

pub struct MpscPublisher {
    stream_tx: mpsc::Sender<WorldStatus>,
    req_rx: mpsc::Receiver<Request>,
    res_tx: mpsc::Sender<world_if::Response>,
}

impl MpscPublisher {
    pub fn new(
        stream_tx: mpsc::Sender<WorldStatus>,
        req_rx: mpsc::Receiver<Request>,
        res_tx: mpsc::Sender<world_if::Response>,
    ) -> Self {
        Self {
            stream_tx,
            req_rx,
            res_tx,
        }
    }
}

impl Publisher for MpscPublisher {
    type SendError<T> = mpsc::SendError<T>;
    type RecvError = mpsc::RecvError;
    type TryRecvError = mpsc::TryRecvError;

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

struct MpscSubscriber {
    req_tx: mpsc::Sender<Request>,
    res_rx: mpsc::Receiver<world_if::Response>,
    stream_rx: mpsc::Receiver<WorldStatus>,
}

impl MpscSubscriber {
    fn new(
        req_tx: mpsc::Sender<Request>,
        res_rx: mpsc::Receiver<world_if::Response>,
        stream_rx: mpsc::Receiver<WorldStatus>,
    ) -> Self {
        Self {
            req_tx,
            res_rx,
            stream_rx,
        }
    }
}

impl Subscriber for MpscSubscriber {
    type Err = SubscriberError<mpsc::TryRecvError, mpsc::RecvError, mpsc::SendError<Request>>;

    fn recv_status(&self) -> Result<WorldStatus, Self::Err> {
        self.stream_rx.recv().map_err(SubscriberError::RecvError)
    }

    fn try_recv_status(&self) -> Result<WorldStatus, Self::Err> {
        self.stream_rx
            .try_recv()
            .map_err(SubscriberError::TryRecvError)
    }

    fn send(&self, req: Request) -> Result<(), Self::Err> {
        self.req_tx.send(req).map_err(SubscriberError::SendError)
    }

    fn recv(&self) -> Result<world_if::Response, Self::Err> {
        self.res_rx.recv().map_err(SubscriberError::RecvError)
    }
}

enum RequestWrapper {
    Info,
    Req(Request),
}

struct MyHandler {
    subscriber: MpscSubscriber,
    status: WorldStatus,
}

impl repl::Parsable for MyHandler {
    type Parsed = RequestWrapper;

    fn parse(buf: &str) -> repl::ParseResult<Self::Parsed> {
        if buf.starts_with("info") {
            return Ok(RequestWrapper::Info);
        }
        let req = world_if::parse::request(buf)?;
        Ok(RequestWrapper::Req(req))
    }
}

impl repl::Logging for MyHandler {
    type Arg = world_if::Response;

    fn logging(arg: Self::Arg) {
        match arg {
            world_if::Response::Ok(s) => println!("[info] {s:?}"),
            world_if::Response::Err(e) => eprintln!("[error] {e:?}"),
        }
    }
}

impl repl::Handler for MyHandler {
    type Input = RequestWrapper;
    type Output = world_if::Response;

    fn callback(&mut self, input: Self::Input) -> Self::Output {
        match input {
            RequestWrapper::Info => {
                if let Some(status) = self.subscriber.seek_status().into_iter().last() {
                    self.status = status;
                }
                world_if::ResponseOk::SuccessWithMessage((&self.status).to_string()).into()
            }
            RequestWrapper::Req(req) => self.subscriber.request(req).unwrap(),
        }
    }
}

impl Drop for MyHandler {
    fn drop(&mut self) {
        println!("{:?}", self.subscriber.delete().unwrap());
    }
}

fn main() {
    let (req_tx, req_rx) = mpsc::channel();
    let (res_tx, res_rx) = mpsc::channel();
    let (stream_tx, stream_rx) = mpsc::channel();
    let spawner = world::WorldSpawner::new(
        "test".to_string(),
        MpscPublisher::new(stream_tx, req_rx, res_tx),
    );
    let handle = spawner.spawn().unwrap();
    let input = thread::spawn(move || {
        let subscriber = MpscSubscriber::new(req_tx, res_rx, stream_rx);
        let status = subscriber.recv_status().unwrap();
        repl::Repl::new(MyHandler { subscriber, status }).run()
    });
    input.join().unwrap();
    match handle.join() {
        Ok(_) => println!("stopped"),
        Err(e) => eprintln!("{e:?}"),
    }
}
