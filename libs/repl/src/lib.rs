use async_trait::async_trait;
use peg::parser;
use std::{
    borrow::BorrowMut,
    error,
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

pub trait Repler {
    type Parsed;
    type Arg;
    fn parse(input: &str) -> ParseResult<Self::Parsed>;
    fn logging(output: Self::Arg);
}

pub trait Parsable {
    type Parsed;
    fn parse(buf: &str) -> ParseResult<Self::Parsed>;
}

pub trait Logging {
    type Arg;
    fn logging(arg: Self::Arg);
}

pub trait Handler {
    type Input;
    type Output;
    fn callback(&mut self, input: Self::Input) -> Self::Output;
}

pub struct Repl<R: Handler> {
    runtime: R,
}

impl<R> Repl<R>
where
    R: Handler + Parsable<Parsed = R::Input> + Logging<Arg = R::Output>,
{
    pub fn new(runtime: R) -> Self {
        Self { runtime }
    }

    pub fn step(&mut self) -> Result<ControlFlow<()>, Box<dyn error::Error>> {
        let mut buf = String::new();
        io::stdout().flush()?;
        print!("> ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut buf)?;
        let cmd = parse::command(buf.as_str())?;
        match cmd {
            Command::Quit => return Ok(ControlFlow::Break(())),
            Command::Delegate(str) => {
                let input = R::parse(str.as_str())?;
                let output = R::callback(&mut self.runtime, input);
                R::logging(output);
            }
            Command::None => {}
        };
        Ok(ControlFlow::Continue(()))
    }

    pub fn run(mut self) {
        loop {
            match self.step() {
                Err(e) => eprintln!("{e}"),
                Ok(ControlFlow::Break(_)) => break,
                Ok(_) => {}
            }
        }
        drop(self)
    }
}

#[async_trait]
pub trait AsyncHandler {
    type Input;
    type Output;
    async fn callback(&mut self, input: Self::Input) -> Self::Output;
}

pub struct AsyncRepl<R>
where
    R: AsyncHandler,
{
    runtime: Arc<Mutex<R>>,
}

impl<R> AsyncRepl<R>
where
    R: AsyncHandler + Parsable<Parsed = R::Input> + Logging<Arg = R::Output> + Send + 'static,
    R::Input: Send,
{
    pub fn new(runtime: R) -> Self {
        Self {
            runtime: Arc::new(Mutex::new(runtime)),
        }
    }

    async fn step(&mut self) -> Result<ControlFlow<()>, Box<dyn error::Error>> {
        let mut buf = String::new();
        io::stdout().flush()?;
        print!("> ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut buf)?;
        let cmd = parse::command(buf.as_str())?;
        match cmd {
            Command::Quit => return Ok(ControlFlow::Break(())),
            Command::None => {}
            Command::Delegate(str) => {
                let input = R::parse(str.as_str())?;
                let runtime = Arc::clone(&self.runtime);
                tokio::spawn(async move {
                    let output = R::callback(runtime.lock().await.borrow_mut(), input).await;
                    R::logging(output);
                });
            }
        }
        Ok(ControlFlow::Continue(()))
    }

    pub async fn run(mut self) {
        loop {
            match self.step().await {
                Err(e) => eprintln!("{e}"),
                Ok(ControlFlow::Break(_)) => break,
                Ok(_) => {}
            }
        }
        drop(self)
    }
}

// pub struct StdListener<T> {}
