use peg::parser;
use std::{
    io::{self, Write},
    ops::{self, ControlFlow},
};

pub enum Command {
    Quit,
    List,
    None,
    New,
    Info(String),
    Delete(String),
    // Start(String, u64),
    // Step(String),
    // Stop(String),
    // Reset(String),
    // Debug(String),
    // Export(String, String),
    Msg(String, world::Request),
}

parser! {
    grammar parse() for str {
        use world::Request;
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
            / "start"  id:id() n:num() {  Command::Msg(id, Request::Start(n)) }
            / "export" id:id() p:path() { Command::Msg(id, Request::Export(p)) }
            / ""     { Command::None }
    }
}

pub trait Listener<B> {
    type Arg;
    fn callback(&mut self, arg: Self::Arg) -> ops::ControlFlow<B>;
}

pub fn input_loop<L, B>(mut listener: L) -> B
where
    L: Listener<B, Arg = Command> + Send + 'static,
{
    loop {
        let mut input = String::new();
        io::stdout().flush().unwrap();
        print!("> ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut input).unwrap();

        match parse::command(input.as_str()) {
            Ok(cmd) => {
                if let ops::ControlFlow::Break(b) = listener.callback(cmd) {
                    break b;
                }
            }
            Err(e) => println!("{}", e),
        }
    }
}

pub struct StdListener {
    manager: super::WorldManager,
    // channels: Arc<Mutex<HashMap<String, WorldInfo>>>,
}

impl StdListener {
    pub fn new(path: String) -> Self {
        Self {
            manager: super::WorldManager::new(path),
        }
    }
}

impl Listener<()> for StdListener {
    type Arg = Command;

    fn callback(&mut self, cmd: Command) -> ops::ControlFlow<()> {
        match cmd {
            Command::None => {}
            Command::List => {
                for id in self.manager.get_all_ids() {
                    println!("{id}");
                }
            }
            Command::Quit => {
                for (id, res) in self.manager.delete_all().into_iter() {
                    // info.req.send(world::Command::Delete).unwrap();
                    match res {
                        Ok(_) => println!("Deleted {id}."),
                        Err(err) => eprintln!("{:?}", err),
                    }
                }
                return ControlFlow::Break(());
            }
            Command::New => match self.manager.new_world() {
                Ok(id) => println!("World {id} is created."),
                Err(e) => println!("{e}"),
            },
            Command::Info(ref id) => {
                if let Some(status) = self.manager.get_status::<String>(id) {
                    println!("{}", status);
                } else {
                    println!("World '{id}' not found.");
                }
            }
            Command::Delete(ref id) => log(id, self.manager.delete(id)),
            Command::Msg(ref id, req) => log(id, self.manager.send(id, req)),
        }
        ops::ControlFlow::Continue(())
    }
}

fn log(id: &String, res: Option<world::Response>) {
    if let Some(res) = res {
        match res {
            Ok(None) => println!("succeed"),
            Ok(Some(msg)) => println!("[info] {msg}"),
            Err(err) => eprintln!("[error] {err:?}"),
        }
    } else {
        println!("World '{id}' not found.");
    }
}
