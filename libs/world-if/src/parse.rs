use crate::Request;

peg::parser! {
    grammar parse() for str {
        rule _() = quiet!{ [' ' | '\t']+ }
        rule __() = quiet!{ [' ' | '\t']* }
        rule eof() = quiet!{ ['\n'] }

        rule u32() -> u32 = n:$(['0'..='9']+) {? n.parse().or(Err("number")) }
        rule num() -> u32
            = _ n:u32() { n }
            / expected!("number")

        rule quoted() -> &'input str = "\"" s:$([' ' | '!' | '$'..='~']*) "\"" { s }
        rule non_space() -> &'input str = s:$(['!'..='~']+) { s }
        rule path() -> &'input str
            = _ s:quoted() { s }
            / _ s:non_space() { s }

        pub rule request() -> Request = __ c:expr() __ eof() { c }
        rule expr() -> Request
            = "step"   { Request::Step }
            / "stop"   { Request::Stop }
            / "reset"  { Request::Reset }
            / "debug"  { Request::Debug }
            / "start"  n:num()  { Request::Start(n) }
            / "export" p:path() { Request::Export(p.into()) }
    }
}

pub use parse::request;
