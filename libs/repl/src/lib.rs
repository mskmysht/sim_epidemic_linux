use async_trait::async_trait;
pub use nom;
use nom::{Finish, IResult};
use std::{
    borrow::BorrowMut,
    fmt::Debug,
    io::{self, Write},
    sync::Arc,
};
use tokio::sync::Mutex;

#[derive(Debug)]
pub enum Command<T> {
    Quit,
    None,
    Delegate(T),
}

mod parser {
    use super::{Command, Parsable};
    use nom::{
        branch::alt,
        character::complete::{multispace0, space0},
        combinator::{all_consuming, map},
        sequence::delimited,
        IResult,
    };
    use parser::nullary;

    pub fn command<P: Parsable>(input: &str) -> IResult<&str, Command<P::Parsed>> {
        all_consuming(delimited(
            multispace0,
            alt((
                nullary(":q", || Command::Quit),
                map(P::parse, |v| Command::Delegate(v)),
                map(space0, |_| Command::None),
            )),
            multispace0,
        ))(input)
    }
}

pub trait Repler {
    type Parsed;
    type Arg;
    fn parse(input: &str) -> IResult<&str, Self::Parsed>;
    fn logging(output: Self::Arg);
}

pub trait Parsable {
    type Parsed;
    fn parse(input: &str) -> IResult<&str, Self::Parsed>;
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

fn parse_input<P>() -> Command<P::Parsed>
where
    P: Parsable,
    P::Parsed: Debug,
{
    loop {
        let mut buf = String::new();
        io::stdout().flush().unwrap();
        print!("> ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut buf).unwrap();
        match parser::command::<P>(&buf).finish() {
            Ok((_, cmd)) => {
                break cmd;
            }
            Err(e) => println!("{e}"),
        }
    }
}

pub struct Repl<R: Handler> {
    runtime: R,
}

impl<R> Repl<R>
where
    R: Handler + Parsable<Parsed = R::Input> + Logging<Arg = R::Output>,
    R::Input: Debug,
{
    pub fn new(runtime: R) -> Self {
        Self { runtime }
    }

    pub fn run(mut self) {
        loop {
            match parse_input::<R>() {
                Command::Quit => break,
                Command::None => {}
                Command::Delegate(input) => {
                    let output = R::callback(&mut self.runtime, input);
                    R::logging(output);
                }
            }
        }
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
    R::Input: Send + Debug,
{
    pub fn new(runtime: R) -> Self {
        Self {
            runtime: Arc::new(Mutex::new(runtime)),
        }
    }

    pub async fn run(self) {
        loop {
            match parse_input::<R>() {
                Command::Quit => break,
                Command::None => {}
                Command::Delegate(input) => {
                    let runtime = Arc::clone(&self.runtime);
                    tokio::spawn(async move {
                        let output = R::callback(runtime.lock().await.borrow_mut(), input).await;
                        R::logging(output);
                    });
                }
            }
        }
    }
}
