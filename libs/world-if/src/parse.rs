use nom::{
    branch::alt,
    character::complete::{multispace0, u32},
    combinator::all_consuming,
    sequence::delimited,
    IResult,
};
use parser::{no_newline_string1, nullary, unary};

use crate::Request;

fn _parse_expr(input: &str) -> IResult<&str, Request> {
    alt((
        nullary("step", || Request::Step),
        nullary("stop", || Request::Stop),
        nullary("reset", || Request::Reset),
        unary("start", u32, Request::Start),
        unary("export", no_newline_string1, Request::Export),
    ))(input)
}

#[cfg(debug_assertions)]
fn parse_expr(input: &str) -> IResult<&str, Request> {
    alt((nullary("debug", || Request::Debug), _parse_expr))(input)
}

#[cfg(not(debug_assertions))]
fn parse_expr(input: &str) -> IResult<&str, Request> {
    _parse_expr(input)
}

pub fn request(input: &str) -> IResult<&str, Request> {
    all_consuming(delimited(multispace0, parse_expr, multispace0))(input)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse() {
        println!("{:?}", super::request("step"));
        println!("{:?}", super::request("  stop   "));
        println!("{:?}", super::request("  step   \n"));
        println!("{:?}", super::request("  step\n"));
        println!("{:?}", super::request("  start 50\n"));
        println!("{:?}", super::request("  export hoge"));
        println!("{:?}", super::request("  export hoge\n"));
        println!("{:?}", super::request("  export\n"));
        println!("{:?}", super::request("  export \n"));
        println!("{:?}", super::request("  steppppp"));
    }
}
