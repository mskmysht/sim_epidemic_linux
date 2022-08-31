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
    Start(String, u64),
    Step(String),
    Stop(String),
    Reset(String),
    Debug(String),
    Export(String, String),
}

parser! {
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

        pub rule command() -> Command = __ c:expr_command() __ eof() { c }
        rule expr_command() -> Command
            = ":q"   { Command::Quit }
            / "list" { Command::List }
            / "new"  { Command::New }
            / "info"   id:id() { Command::Info(id) }
            / "delete" id:id() { Command::Delete(id) }
            / "step"   id:id() { Command::Step (id) }
            / "stop"   id:id() { Command::Stop (id) }
            / "reset"  id:id() { Command::Reset(id) }
            / "debug"  id:id() { Command::Debug(id) }
            / "start"  id:id() n:num() {  Command::Start (id, n) }
            / "export" id:id() p:path() { Command::Export(id, p) }
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
    pub fn new() -> Self {
        Self {
            manager: super::WorldManager::new(),
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
                if let Some(status) = self.manager.get_info::<String>(id) {
                    println!("{}", status);
                } else {
                    println!("World '{id}' not found.");
                }
            }
            Command::Delete(ref id) => {
                if let Some(res) = self.manager.delete(id) {
                    // info.req.send(world::Command::Delete).unwrap();
                    match res {
                        Ok(None) => println!("succeed"),
                        Ok(Some(msg)) => println!("{}", msg),
                        Err(err) => eprintln!("{:?}", err),
                    }
                } else {
                    println!("World '{id}' not found.");
                }
            }
            Command::Start(ref id, stop_at) => log(id, self.manager.start(id, stop_at)),
            Command::Stop(ref id) => log(id, self.manager.stop(id)),
            Command::Step(ref id) => log(id, self.manager.step(id)),
            Command::Reset(ref id) => log(id, self.manager.reset(id)),
            Command::Debug(ref id) => log(id, self.manager.debug(id)),
            Command::Export(ref id, dir) => log(id, self.manager.export(id, dir)),
        }
        ops::ControlFlow::Continue(())
    }
}

fn log(id: &String, res: Option<world::result::Result>) {
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
