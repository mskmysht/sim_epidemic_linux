use std::{sync::mpsc, thread};

use world::MpscSpawnerChannel;
use world_if::{Request, Response, WorldStatus};

struct MyHandler {
    req_tx: mpsc::Sender<Request>,
    res_rx: mpsc::Receiver<Response>,
    stream_rx: mpsc::Receiver<WorldStatus>,
    status: WorldStatus,
}

enum RequestWrapper {
    Info,
    Req(Request),
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
    type Arg = Response;

    fn logging(arg: Self::Arg) {
        match arg {
            Ok(s) => println!("[info] {s:?}"),
            Err(e) => eprintln!("[error] {e:?}"),
        }
    }
}

impl repl::Handler for MyHandler {
    type Input = RequestWrapper;
    type Output = Response;

    fn callback(&mut self, input: Self::Input) -> Self::Output {
        match input {
            RequestWrapper::Info => {
                while let Ok(status) = self.stream_rx.try_recv() {
                    self.status = status;
                }
                Ok(Some((&self.status).into()))
            }
            RequestWrapper::Req(req) => {
                self.req_tx.send(req).unwrap();
                self.res_rx.recv().unwrap()
            }
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
    let (req_tx, req_rx) = mpsc::channel();
    let (res_tx, res_rx) = mpsc::channel();
    let (stream_tx, stream_rx) = mpsc::channel();
    let (handle, status) = world::WorldSpawner::spawn(
        "test".into(),
        MpscSpawnerChannel::new(stream_tx, req_rx, res_tx),
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
