use crate::Request;

peg::parser! {
    grammar parse() for str {
        rule _() = quiet!{ [' ' | '\t']+ }
        rule __() = quiet!{ [' ' | '\t']* }
        rule eof() = quiet!{ ['\n'] }

        rule u64() -> u64 = n:$(['0'..='9']+) { n.parse().unwrap() }
        rule num() -> u64
            = _ n:u64() { n }
            / expected!("number")

        rule quoted() -> String = "\"" s:$([' ' | '!' | '$'..='~']*) "\"" { String::from(s) }
        rule non_space() -> String = s:$(['!'..='~']+) { String::from(s) }
        rule path() -> String
            = _ s:quoted() { String::from(s) }
            / _ s:non_space() { String::from(s) }

        pub rule request() -> Request = __ c:expr() __ eof() { c }
        rule expr() -> Request
            = "step"   { Request::Step }
            / "stop"   { Request::Stop }
            / "reset"  { Request::Reset }
            / "debug"  { Request::Debug }
            / "start"  n:num()  { Request::Start(n) }
            / "export" p:path() { Request::Export(p) }
    }
}

pub use parse::request;
