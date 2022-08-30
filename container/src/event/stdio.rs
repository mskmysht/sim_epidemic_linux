use std::{
    io::{self, Write},
    thread,
};

use crate::event::Command;
use peg::parser;

parser! {
    grammar parse() for str {
        use crate::world::Request;
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

        pub rule command() -> Command = __ c:expr_command() __ eof() { c }
        rule expr_command() -> Command
            = ":q"   { Command::Quit }
            / "list" { Command::List }
            / "new"  { Command::New }
            / "info"   id:id() { Command::Info(id) }
            / "delete" id:id() { Command::Delete(id) }
            / "step"   id:id() { Command::Msg(id, Request::Step) }
            / "stop"   id:id() { Command::Msg(id, Request::Stop) }
            / "reset"  id:id() { Command::Msg(id, Request::Reset) }
            / "debug"  id:id() { Command::Msg(id, Request::Debug) }
            / "start"  id:id() n:num() { Command::Msg(id, Request::Start(n)) }
            / "export" id:id() p:path() { Command::Msg(id, Request::Export(p)) }
            / ""     { Command::None }
    }
}

pub fn input_handle<C>() -> thread::JoinHandle<()>
where
    C: super::Callback + Send + 'static,
{
    let mut c = C::init();
    thread::spawn(move || loop {
        let mut input = String::new();
        io::stdout().flush().unwrap();
        print!("> ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut input).unwrap();

        match parse::command(input.as_str()) {
            Ok(cmd) => {
                if !c.callback(cmd) {
                    break;
                }
            }
            Err(e) => println!("{}", e),
        }
    })
}
