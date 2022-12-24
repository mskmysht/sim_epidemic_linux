use nom::{
    bytes::complete::{tag, take_till1},
    character::complete::space1,
    combinator::map,
    error::ParseError,
    sequence::{separated_pair, tuple},
    IResult, Parser,
};

pub fn nullary<'a, F: FnMut() -> O, O>(
    keyword: &'static str,
    mut f: F,
) -> impl FnMut(&'a str) -> IResult<&'a str, O> {
    map(tag(keyword), move |_| f())
}

pub fn unary<'a, O1, O2, E, F, G>(
    keyword: &'static str,
    inner: F,
    mut f: G,
) -> impl FnMut(&'a str) -> IResult<&'a str, O2, E>
where
    E: ParseError<&'a str>,
    F: Parser<&'a str, O1, E>,
    G: FnMut(O1) -> O2,
{
    map(
        separated_pair(tag(keyword), space1, inner),
        move |(_, o1)| f(o1),
    )
}

pub fn binary<'a, O1, O2, O3, E, F, G, H>(
    keyword: &'static str,
    inner1: F,
    inner2: G,
    mut f: H,
) -> impl FnMut(&'a str) -> IResult<&'a str, O3, E>
where
    E: ParseError<&'a str>,
    F: Parser<&'a str, O1, E>,
    G: Parser<&'a str, O2, E>,
    H: FnMut(O1, O2) -> O3,
{
    map(
        tuple((tag(keyword), space1, inner1, inner2)),
        move |(_, _, o1, o2)| f(o1, o2),
    )
}

pub fn no_newline_str1(input: &str) -> IResult<&str, &str> {
    take_till1(|c| c == '\n' || c == '\r')(input)
}

pub fn no_newline_string1(input: &str) -> IResult<&str, String> {
    map(no_newline_str1, ToString::to_string)(input)
}

pub fn no_invisibles(input: &str) -> IResult<&str, &str> {
    take_till1(|c| c == '\n' || c == '\r' || c == '\t' || c == ' ')(input)
}
