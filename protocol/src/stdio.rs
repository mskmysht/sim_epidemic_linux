use peg::parser;
use std::{
    io::{self, Write},
    ops::ControlFlow,
    sync::Arc,
};
use tokio::sync::Mutex;

pub enum Command {
    Quit,
    None,
    Delegate(String),
}

parser! {
    grammar parse() for str {
        rule _() = quiet!{ [' ' | '\t']* }
        rule eof() = quiet!{ ['\n'] }

        pub rule command() -> Command
            = _ ":q" _ eof() { Command::Quit }
            / _ eof() { Command::None }
            / s:$([_]+) { Command::Delegate(String::from(s)) }
    }
}

pub type ParseResult<T> = Result<T, peg::error::ParseError<<str as peg::Parse>::PositionRepr>>;

pub trait InputLoop<Req, Ret> {
    fn parse(input: &str) -> ParseResult<Req>;
    fn logging(ret: Ret);
}

pub struct Runner<C> {
    core: C,
}

impl<C> Runner<C>
where
    C: super::SyncCallback + InputLoop<C::Req, C::Ret>,
{
    pub fn new(core: C) -> Self {
        Self { core }
    }

    pub fn step(
        &mut self,
        input: &str,
    ) -> Result<
        ControlFlow<(), Option<C::Ret>>,
        peg::error::ParseError<<str as peg::Parse>::PositionRepr>,
    > {
        let cmd = parse::command(input)?;
        let c = match cmd {
            Command::Quit => ControlFlow::Break(()),
            Command::Delegate(str) => {
                let req = C::parse(str.as_str())?;
                ControlFlow::Continue(Some(self.core.callback(req)))
            }
            Command::None => ControlFlow::Continue(None),
        };
        Ok(c)
    }

    pub fn run(mut self) {
        loop {
            let mut input = String::new();
            io::stdout().flush().unwrap();
            print!("> ");
            io::stdout().flush().unwrap();
            io::stdin().read_line(&mut input).unwrap();
            match self.step(input.as_str()) {
                Err(e) => eprintln!("{e}"),
                Ok(c) => match c {
                    ControlFlow::Continue(None) => {}
                    ControlFlow::Continue(Some(res)) => {
                        C::logging(res);
                    }
                    ControlFlow::Break(_) => break,
                },
            }
        }
        drop(self)
    }
}

pub struct AsyncRunner<C> {
    core: Arc<Mutex<C>>,
}

impl<C> AsyncRunner<C>
where
    C: super::AsyncCallback + InputLoop<C::Req, C::Ret> + Send + 'static,
    C::Req: Send,
    C::Ret: Send,
{
    pub fn new(core: C) -> Self {
        Self {
            core: Arc::new(Mutex::new(core)),
        }
    }

    pub async fn step<'a>(
        &mut self,
        input: &'a str,
    ) -> Result<ControlFlow<()>, peg::error::ParseError<<str as peg::Parse>::PositionRepr>> {
        let cmd = parse::command(input)?;
        let c = match cmd {
            Command::Quit => ControlFlow::Break(()),
            Command::Delegate(str) => {
                let req = C::parse(str.as_str())?;
                let core = Arc::clone(&self.core);
                tokio::spawn(async move {
                    let res = core.lock().await.callback(req).await;
                    C::logging(res);
                });
                ControlFlow::Continue(())
            }
            Command::None => ControlFlow::Continue(()),
        };
        Ok(c)
    }

    pub async fn run(mut self) {
        loop {
            let mut input = String::new();
            io::stdout().flush().unwrap();
            print!("> ");
            io::stdout().flush().unwrap();
            io::stdin().read_line(&mut input).unwrap();
            match self.step(input.as_str()).await {
                Err(e) => {
                    eprintln!("{e}");
                }
                Ok(c) => match c {
                    ControlFlow::Break(_) => break,
                    _ => {}
                },
            }
        }
        drop(self)
    }
}
