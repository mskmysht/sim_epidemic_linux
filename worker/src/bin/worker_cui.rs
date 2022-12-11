use std::error;

use async_trait::async_trait;
use worker::WorldManager;

type Req = worker_if::Request<world_if::Request>;
type Ret = worker_if::Result<world_if::Response>;

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
    type Arg = Ret;

    fn logging(arg: Self::Arg) {
        match arg {
            Ok(s) => println!("[info] {s:?}"),
            Err(e) => eprintln!("[error] {e:?}"),
        }
    }
}

impl repl::Handler for StdHandler {
    type Input = Req;
    type Output = Ret;

    fn callback(&mut self, input: Self::Input) -> Self::Output {
        self.manager.callback(input)
    }
}

#[async_trait]
impl repl::AsyncHandler for StdHandler {
    type Input = Req;
    type Output = Ret;

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
