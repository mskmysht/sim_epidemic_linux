use nom::{
    branch::alt,
    character::complete::multispace0,
    combinator::{all_consuming, map},
    sequence::delimited,
    IResult,
};
use parser::{binary, no_invisibles, no_newline_string1, nullary, unary};

use super::Request;

fn parse_expr(input: &str) -> IResult<&str, Request> {
    alt((
        nullary("list", || Request::GetItemList),
        nullary("new", || Request::SpawnItem),
        unary("info", no_newline_string1, Request::GetItemInfo),
        unary("delete", no_newline_string1, Request::DeleteItem),
        binary(
            "msg",
            map(no_invisibles, ToString::to_string),
            world_if::realtime::parse::request,
            Request::Custom,
        ),
    ))(input)
}

pub fn request(input: &str) -> IResult<&str, Request> {
    all_consuming(delimited(multispace0, parse_expr, multispace0))(input)
}

#[cfg(test)]
mod tests {
    use super::request;

    #[test]
    fn parse_test() {
        println!("{:?}", request("li\n"));
        println!("{:?}", request("new\n"));
        println!("{:?}", request("info hoge"));
        println!("{:?}", request("msg hoge step "));
        println!("{:?}", request("msg hoge start 1\n"));
    }
}
