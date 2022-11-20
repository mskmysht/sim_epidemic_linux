use std::{sync::mpsc, thread};

use world::MpscWorldChannel;
use world_if::{Request, Response, WorldStatus};

struct MyHandler {
    req_tx: mpsc::Sender<Request>,
    res_rx: mpsc::Receiver<Response>,
    stream_rx: mpsc::Receiver<WorldStatus>,
    status: WorldStatus,
}

impl repl::Parsable for MyHandler {
    type Parsed = container_if::Request<Request>;

    fn parse(buf: &str) -> repl::ParseResult<Self::Parsed> {
        protocol::parse::request(buf)
    }
}

impl repl::Logging for MyHandler {
    type Arg = Response;

    fn logging(arg: Self::Arg) {
        match arg {
            Ok(s) => println!("[info] {s:?}"),
            Err(e) => eprintln!("[error] {e:?}"),
        }
    }
}

impl repl::Handler for MyHandler {
    type Input = container_if::Request<Request>;
    type Output = Response;

    fn callback(&mut self, input: Self::Input) -> Self::Output {
        match input {
            container_if::Request::Info(_) => {
                if let Ok(status) = self.stream_rx.try_recv() {
                    self.status = status;
                }
                Ok(Some((&self.status).into()))
            }
            container_if::Request::Msg(_, req) => {
                self.req_tx.send(req).unwrap();
                self.res_rx.recv().unwrap()
            }
            _ => Ok(None),
        }
    }
}

impl Drop for MyHandler {
    fn drop(&mut self) {
        self.req_tx.send(Request::Delete).unwrap();
        println!("{:?}", self.res_rx.recv().unwrap());
    }
}

fn main() {
    // repl::Repl::new(runtime)
    let (req_tx, req_rx) = mpsc::channel();
    let (res_tx, res_rx) = mpsc::channel();
    let (stream_tx, stream_rx) = mpsc::channel();
    let (handle, status) = world::World::spawn(
        "test".into(),
        MpscWorldChannel::new(stream_tx, req_rx, res_tx),
    )
    .unwrap();
    let input = thread::spawn(move || {
        repl::Repl::new(MyHandler {
            req_tx,
            res_rx,
            stream_rx,
            status,
        })
        .run()
    });
    input.join().unwrap();
    let id = match handle.join() {
        Ok(id) => id,
        Err(e) => {
            eprintln!("{e:?}");
            return;
        }
    };
    println!("[info] Delete world {id}.");
}
