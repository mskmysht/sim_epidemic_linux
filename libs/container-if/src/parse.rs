use crate::Request;

peg::parser! {
    grammar parse() for str {
        rule _() = quiet!{ [' ' | '\t']+ }
        rule __() = quiet!{ [' ' | '\t']* }
        rule eof() = quiet!{ ['\n'] }
        rule identifier() -> String = s:$(['!'..='~']+) { String::from(s) }

        rule id() -> String
            = _ id:identifier() { id }
            / expected!("world id")

        rule u64() -> u64 = n:$(['0'..='9']+) { n.parse().unwrap() }
        rule num() -> u64
            = _ n:u64() { n }
            / expected!("number")

        rule quoted() -> String = "\"" s:$([' ' | '!' | '$'..='~']*) "\"" { String::from(s) }
        rule non_space() -> String = s:$(['!'..='~']+) { String::from(s) }
        rule path() -> String
            = _ s:quoted() { String::from(s) }
            / _ s:non_space() { String::from(s) }

        rule string() -> String = s:$([_]+) { String::from(s) }
        rule expr<M>(x: rule<M>) -> Request<M>
            = "list" __ eof() { Request::List }
            / "new"  __ eof() { Request::New }
            / "info"   id:id() __ eof() { Request::Info(id) }
            / "delete" id:id() __ eof() { Request::Delete(id) }
            / "msg"    id:id() m:x() { Request::Msg(id, m) }
        pub rule request() -> Request<String> = __ c:expr(<string()>) { c }
    }
}

pub use parse::request;
