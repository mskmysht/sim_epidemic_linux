type WReq = world_if::Request;
type CReq = container_if::Request<WReq>;

peg::parser! {
    grammar parse() for str {
        rule _() = quiet!{ [' ' | '\t']+ }
        rule __() = quiet!{ [' ' | '\t']* }
        rule eof() = quiet!{ ['\n'] }
        rule u64() -> u64 = n:$(['0'..='9']+) { n.parse().unwrap() }
        rule identifier() -> String = s:$(['!'..='~']+) { String::from(s) }
        rule quoted() -> String = "\"" s:$([' ' | '!' | '$'..='~']*) "\"" { String::from(s) }
        rule non_space() -> String = s:$(['!'..='~']+) { String::from(s) }

        rule id() -> String
            = _ id:identifier() { id }
            / expected!("world id")

        rule num() -> u64
            = _ n:u64() { n }
            / expected!("number")

        rule path() -> String
            = _ s:quoted() { String::from(s) }
            / _ s:non_space() { String::from(s) }

        pub rule request() -> CReq = __ c:expr() __ eof() { c }
        rule expr() -> CReq
            = "list" { CReq::List }
            / "new"  { CReq::New }
            / "info"   id:id() { CReq::Info(id) }
            / "delete" id:id() { CReq::Delete(id) }
            / "step"   id:id() { CReq::Msg(id, WReq::Step) }
            / "stop"   id:id() { CReq::Msg(id, WReq::Stop) }
            / "reset"  id:id() { CReq::Msg(id, WReq::Reset) }
            / "debug"  id:id() { CReq::Msg(id, WReq::Debug) }
            / "start"  id:id() n:num() {  CReq::Msg(id, WReq::Start(n)) }
            / "export" id:id() p:path() { CReq::Msg(id, WReq::Export(p)) }
    }
}

pub use parse::request;
