use std::error;

use async_trait::async_trait;
use worker::WorldManager;

type Req = worker_if::Request<world_if::Request>;
type Res = worker_if::Response<world_if::ResponseOk>;

pub struct StdHandler {
    manager: WorldManager,
}

impl StdHandler {
    pub fn new(path: String) -> Self {
        Self {
            manager: WorldManager::new(path),
        }
    }
}

impl repl::Parsable for StdHandler {
    type Parsed = Req;

    fn parse(buf: &str) -> repl::ParseResult<Self::Parsed> {
        worker_if::parse::request(buf)?.try_map(|s| world_if::parse::request(&s))
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
        self.manager.callback(input)
    }
}

#[async_trait]
impl repl::AsyncHandler for StdHandler {
    type Input = Req;
    type Output = Res;

    async fn callback(&mut self, input: Self::Input) -> Self::Output {
        self.manager.callback(input)
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
    if is_async {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(repl::AsyncRepl::new(StdHandler::new(world_path)).run());
    } else {
        repl::Repl::new(StdHandler::new(world_path)).run();
    }
    Ok(())
}
