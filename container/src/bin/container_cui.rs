use std::error;

use async_trait::async_trait;
use container::world::WorldManager;
use world_if as wif;

type Req = container_if::Request<wif::Request>;
type Ret = container_if::Response<wif::Success, wif::ErrorStatus>;

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
        container_if::parse::request(buf)?.map_r(|s| world_if::parse::request(&s))
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
