use std::error;

use async_trait::async_trait;
use worker::WorldManager;

type Req = worker_if::Request;
type Res = worker_if::Response;

pub struct StdHandler {
    manager: WorldManager,
}

impl StdHandler {
    pub fn new(manager: WorldManager) -> Self {
        Self { manager }
    }
}

impl repl::Parsable for StdHandler {
    type Parsed = Req;

    fn parse(buf: &str) -> repl::nom::IResult<&str, Self::Parsed> {
        worker_if::parse::request(buf)
    }
}

impl repl::Logging for StdHandler {
    type Arg = Res;

    fn logging(arg: Self::Arg) {
        match arg {
            worker_if::Response::Ok(s) => println!("[info] {s:?}"),
            worker_if::Response::Err(e) => eprintln!("[error] {e:?}"),
        }
    }
}

impl repl::Handler for StdHandler {
    type Input = Req;
    type Output = Res;

    fn callback(&mut self, input: Self::Input) -> Self::Output {
        self.manager.request(input)
    }
}

#[async_trait]
impl repl::AsyncHandler for StdHandler {
    type Input = Req;
    type Output = Res;

    async fn callback(&mut self, input: Self::Input) -> Self::Output {
        self.manager.request(input)
    }
}

#[argopt::cmd]
fn main(
    /// world binary path
    #[opt(long)]
    world_path: String,
    /// enable async
    #[opt(short = 'a')]
    is_async: bool,
) -> Result<(), Box<dyn error::Error>> {
    let (manager, _) = worker::channel(world_path);
    if is_async {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(repl::AsyncRepl::new(StdHandler::new(manager)).run());
    } else {
        repl::Repl::new(StdHandler::new(manager)).run();
    }
    Ok(())
}
