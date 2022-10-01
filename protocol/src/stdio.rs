use peg::parser;
use std::{
    io::{self, Write},
    ops::ControlFlow,
};

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

pub trait InputLoop {
    type Req;
    type Res;
    fn parse(input: &str) -> ParseResult<Self::Req>;
    fn callback(&mut self, req: Self::Req) -> Self::Res;
    fn quit(&mut self);
    fn logging(res: Self::Res);
}

fn run_step<R: InputLoop>(
    runner: &mut R,
    str: &str,
) -> Result<
    ControlFlow<(), Option<R::Res>>,
    peg::error::ParseError<<str as peg::Parse>::PositionRepr>,
> {
    let cmd = parse::command(str)?;
    let c = match cmd {
        Command::Quit => {
            runner.quit();
            ControlFlow::Break(())
        }
        Command::Delegate(str) => {
            let req = R::parse(str.as_str())?;
            ControlFlow::Continue(Some(runner.callback(req)))
        }
        Command::None => ControlFlow::Continue(None),
    };
    Ok(c)
}

pub fn run<R>(mut runner: R)
where
    R: InputLoop,
{
    loop {
        let mut input = String::new();
        io::stdout().flush().unwrap();
        print!("> ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut input).unwrap();
        match run_step(&mut runner, input.as_str()) {
            Err(e) => eprintln!("{e}"),
            Ok(c) => match c {
                ControlFlow::Continue(None) => {}
                ControlFlow::Continue(Some(res)) => {
                    R::logging(res);
                }
                ControlFlow::Break(_) => break,
            },
        }
    }
}
