pub use nom;
use nom::{Finish, IResult};
use std::{
    fmt::Debug,
    io::{self, Write},
};

#[derive(Debug)]
pub enum Command<T> {
    Quit,
    None,
    Delegate(T),
}

pub trait Parsable {
    type Parsed;
    fn parse(input: &str) -> IResult<&str, Self::Parsed>;
    fn recv_input() -> Command<Self::Parsed> {
        loop {
            let mut buf = String::new();
            io::stdout().flush().unwrap();
            print!("> ");
            io::stdout().flush().unwrap();
            io::stdin().read_line(&mut buf).unwrap();
            match Self::command(&buf).finish() {
                Ok((_, cmd)) => {
                    break cmd;
                }
                Err(e) => println!("{e}"),
            }
        }
    }
    fn command(input: &str) -> IResult<&str, Command<Self::Parsed>> {
        use nom::{
            branch::alt,
            character::complete::{multispace0, space0},
            combinator::{all_consuming, map},
            sequence::delimited,
        };
        use parser::nullary;

        all_consuming(delimited(
            multispace0,
            alt((
                nullary(":q", || Command::Quit),
                map(Self::parse, |v| Command::Delegate(v)),
                map(space0, |_| Command::None),
            )),
            multispace0,
        ))(input)
    }
}
