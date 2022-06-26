// mod agent2;
mod agent;
pub mod area;
mod commons;
mod contact;
mod dyn_struct;
mod enum_map;
mod gathering;
pub mod log;
mod stat;
pub mod testing;
mod world;

use commons::{DistInfo, MRef, RuntimeParams, WorldParams};

use peg::parser;
use std::io::{self, Write};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use world::*;

pub enum Command {
    Quit,
    Show,
    Start(i32),
    Step,
    Stop,
    Reset,
    Export(String),
    Debug,
}

pub enum Cmd {
    Show,
    Start(i32),
    Stop,
    Step,
    Quit,
}

enum Message<T> {
    Quit,
    Run(T),
}

parser! {
    grammar parse() for str {
        pub rule expr() -> Command = _ c:command() _ eof() { c }
        rule command() -> Command
            = quit()
            / show()
            / start()
            / step()
            / stop()
            / reset()
            / export()

        rule quit() -> Command = ":q" { Command::Quit }
        rule show() -> Command = "show" { Command::Show }
        rule start() -> Command = "start" _ days:number() { Command::Start(days) }
        rule step() -> Command = "step" { Command::Step }
        rule stop() -> Command = "stop" { Command::Stop }
        rule reset() -> Command = "reset" { Command::Reset }
        rule export() -> Command = "export" _ path:string() { Command::Export(path) }
        rule debug() -> Command = "debug" { Command::Debug }

        rule _() = quiet!{ [' ' | '\t']* }
        rule eof() = quiet!{ ['\n'] }
        rule number() -> i32 = n:$(['0'..='9']+) { n.parse().unwrap() }
        rule string() -> String = s:$(['!'..='~'] [' '..='~']*) { String::from(s) }
    }
}

fn new_input_handle(
    wr: MRef<World>,
    tx: mpsc::Sender<Message<thread::JoinHandle<()>>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        let mut input = String::new();
        io::stdout().flush().unwrap();
        print!("> ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut input).unwrap();

        match parse::expr(input.as_str()) {
            Ok(cmd) => match cmd {
                Command::Quit => {
                    tx.send(Message::Quit).unwrap();
                    break;
                }
                Command::Show => {
                    let world = wr.lock().unwrap();
                    println!("{}", world);
                }
                Command::Start(stop_at) => {
                    if let Some(handle) = start_world(Arc::clone(&wr), stop_at) {
                        tx.send(Message::Run(handle)).unwrap();
                    }
                }
                Command::Step => step_world(Arc::clone(&wr)),
                Command::Stop => {
                    let world = &mut wr.lock().unwrap();
                    stop_world(world);
                }
                Command::Reset => {
                    let world = &mut wr.lock().unwrap();
                    reset_world(world);
                }
                Command::Export(path) => {
                    let world = &wr.lock().unwrap();
                    match export_world(world, path.as_str()) {
                        Ok(_) => println!("{} was successfully exported", path),
                        Err(e) => println!("{}", e),
                    }
                }
                Command::Debug => {
                    let world = &wr.lock().unwrap();
                    debug_world(world);
                }
            },
            Err(e) => print!("{}", e),
        }
    })
}

fn new_world() -> MRef<World> {
    let rp = RuntimeParams {
        mass: 50.0,
        friction: 50.0,
        avoidance: 50.0,
        contag_delay: 0.5,
        contag_peak: 3.0,
        infec: 80.0,
        infec_dst: 3.0,
        dst_st: 50.0,
        dst_ob: 20.0,
        mob_fr: 5.0,
        gat_fr: 30.0,
        cntct_trc: 20.0,
        tst_delay: 1.0,
        tst_proc: 1.0,
        tst_interval: 2.0,
        tst_sens: 70.0,
        tst_spec: 99.8,
        tst_sbj_asy: 1.0,
        tst_sbj_sym: 99.0,
        incub: DistInfo::new(1.0, 5.0, 14.0),
        fatal: DistInfo::new(4.0, 16.0, 20.0),
        recov: DistInfo::new(4.0, 10.0, 40.0),
        immun: DistInfo::new(400.0, 500.0, 600.0),
        mob_dist: DistInfo::new(10.0, 30.0, 80.0),
        gat_sz: DistInfo::new(5.0, 10.0, 20.0),
        gat_dr: DistInfo::new(24.0, 48.0, 168.0),
        gat_st: DistInfo::new(0.0, 50.0, 100.0),
        step: 0,
        act_mode: todo!(),
        act_kurt: todo!(),
        mass_act: todo!(),
        mob_act: todo!(),
        gat_act: todo!(),
        mob_freq: todo!(),
        gat_freq: todo!(),
    };
    let wp = WorldParams::new(1000, 180, 9, 4, 3, commons::WrkPlcMode::WrkPlcNone);
    let w = World::new(rp, wp);
    Arc::new(Mutex::new(w))
}

fn main() {
    let (tx, rx) = mpsc::channel();
    let input_handle = new_input_handle(new_world(), tx);
    let msg_handle = thread::spawn(move || loop {
        match rx.recv().unwrap() {
            Message::Quit => {
                break;
            }
            Message::Run(h) => {
                h.join().unwrap();
            }
        }
    });
    input_handle.join().unwrap();
    msg_handle.join().unwrap();
}
