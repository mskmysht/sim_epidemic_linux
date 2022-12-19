use crate::Request;

peg::parser! {
    grammar parse() for str {
        rule _() = quiet!{ [' ' | '\t']+ }
        rule __() = quiet!{ [' ' | '\t']* }
        rule eof() = quiet!{ ['\n'] }
        rule identifier() -> &'input str = s:$(['!'..='~']+) { s }

        rule id() -> &'input str
            = _ id:identifier() { id }
            / expected!("identifier")

        rule u64() -> u64 = n:$(['0'..='9']+) {? n.parse().or(Err("number")) }
        rule num() -> u64
            = _ n:u64() { n }
            / expected!("number")

        rule quoted() -> &'input str = "\"" s:$([' ' | '!' | '$'..='~']*) "\"" { s }
        rule non_space() -> &'input str = s:$(['!'..='~']+) { s }
        rule path() -> &'input str
            = _ s:quoted() { s }
            / _ s:non_space() { s }

        rule str() -> &'input str = s:$([_]+) { s }
        rule expr() -> Request
            = "list" __ eof() { Request::GetItemList }
            / "new"  __ eof() { Request::SpawnItem }
            / "info"   id:id() __ eof() { Request::GetItemInfo(id.into()) }
            / "delete" id:id() __ eof() { Request::DeleteItem(id.into()) }
            / "msg"    id:id() s:str() {? world_if::parse::request(s).map(|r| Request::Custom(id.into(), r)).map_err(|e| e.expected.tokens().next().unwrap()) }
        pub rule request() -> Request = __ c:expr() { c }
    }
}

pub use parse::request;

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parse_test() {
        println!("{:?}", parse::request("li\n"));
        println!("{:?}", parse::request("msg hoge debug\n"));
        println!("{:?}", parse::request("msg hoge start 1\n"));
    }
}
