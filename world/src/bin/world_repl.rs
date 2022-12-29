use std::{sync::mpsc, thread};

use repl::Parsable;
use world_if::{
    pubsub::{Publisher, Subscriber},
    Request, WorldStatus,
};

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
    type SendErr<T> = mpsc::SendError<T>;
    type RecvErr = mpsc::RecvError;

    fn recv(&self) -> Result<Request, Self::RecvErr> {
        self.req_rx.recv()
    }

    fn try_recv(&self) -> Result<Option<Request>, Self::RecvErr> {
        match self.req_rx.try_recv() {
            Ok(r) => Ok(Some(r)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(mpsc::RecvError),
        }
    }

    fn send_response(
        &self,
        data: world_if::Response,
    ) -> Result<(), Self::SendErr<world_if::Response>> {
        self.res_tx.send(data)
    }

    fn send_on_stream(&self, data: WorldStatus) -> Result<(), Self::SendErr<WorldStatus>> {
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
    type RecvErr = mpsc::RecvError;
    type SendErr = mpsc::SendError<Request>;

    fn recv_status(&self) -> Result<WorldStatus, Self::RecvErr> {
        self.stream_rx.recv()
    }

    fn try_recv_status(&self) -> Result<Option<WorldStatus>, Self::RecvErr> {
        match self.stream_rx.try_recv() {
            Ok(s) => Ok(Some(s)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(mpsc::RecvError),
        }
    }

    fn send(&self, req: Request) -> Result<(), Self::SendErr> {
        self.req_tx.send(req)
    }

    fn recv(&self) -> Result<world_if::Response, Self::RecvErr> {
        self.res_rx.recv()
    }
}

#[derive(Debug)]
enum RequestWrapper {
    Info,
    Req(Request),
}

struct WorldParser;
impl Parsable for WorldParser {
    type Parsed = RequestWrapper;

    fn parse(input: &str) -> repl::nom::IResult<&str, Self::Parsed> {
        use repl::nom::branch::alt;
        use repl::nom::bytes::complete::tag;
        use repl::nom::combinator::map;
        alt((
            map(tag("info"), |_| RequestWrapper::Info),
            map(world_if::parse::request, RequestWrapper::Req),
        ))(input)
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
        let mut status = subscriber.recv_status().unwrap();
        loop {
            match WorldParser::recv_input() {
                repl::Command::Quit => break,
                repl::Command::None => {}
                repl::Command::Delegate(input) => {
                    let output = match input {
                        RequestWrapper::Info => {
                            if let Some(s) = subscriber.seek_status().into_iter().last() {
                                status = s;
                            }
                            world_if::ResponseOk::SuccessWithMessage((&status).to_string()).into()
                        }
                        RequestWrapper::Req(req) => subscriber.request(req).unwrap(),
                    };
                    match output {
                        world_if::Response::Ok(s) => println!("[info] {s:?}"),
                        world_if::Response::Err(e) => eprintln!("[error] {e:?}"),
                    }
                }
            }
        }
    });
    handle.join().unwrap();
    input.join().unwrap();
    println!("stopped");
}
