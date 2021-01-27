use commons::{DistInfo, MRef, RuntimeParams, WorldParams};

use peg::parser;
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

mod agent;
mod commons;
mod contact;
mod dyn_struct;
mod enum_map;
mod gathering;
mod stat;
mod world;

use world::*;

pub enum Command {
    Quit,
    List,
    New(i32),
    Start(i32, i32),
    Step(i32),
    Stop(i32),
    Reset(i32),
    Delete(i32),
    Export(i32, String),
    Debug(i32),
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
            / list()
            / new()
            / start()
            / step()
            / stop()
            / reset()
            / delete()
            / export()
            / debug()

        rule quit() -> Command = ":q" { Command::Quit }
        rule list() -> Command = "list" { Command::List }
        rule new() -> Command = "new" _ id:number() { Command::New(id) }
        rule start() -> Command = "start" _ id:number() _ days:number() { Command::Start(id, days) }
        rule step() -> Command = "step" _ id:number() { Command::Step(id) }
        rule stop() -> Command = "stop" _ id:number() { Command::Stop(id) }
        rule reset() -> Command = "reset" _ id:number() { Command::Reset(id) }
        rule delete() -> Command = "delete" _ id:number() { Command::Delete(id) }
        rule export() -> Command = "export" _ id:number() _ path:string() { Command::Export(id, path) }
        rule debug() -> Command = "debug" _ id:number() { Command::Debug(id) }

        rule _() = quiet!{ [' ' | '\t']* }
        rule eof() = quiet!{ ['\n'] }
        rule number() -> i32 = n:$(['0'..='9']+) { n.parse().unwrap() }
        rule string() -> String = s:$(['!'..='~'] [' '..='~']*) { String::from(s) }
    }
}

fn main() {
    let worlds: HashMap<i32, MRef<World>> = HashMap::new();
    let aws = Arc::new(Mutex::new(worlds));
    let (tx, rx) = mpsc::channel();

    let if_handle = thread::spawn(move || loop {
        let cws = aws.clone();

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
                Command::List => {
                    for (i, cw) in cws.lock().unwrap().iter() {
                        println!("id:{} world:{}", i, cw.lock().unwrap());
                    }
                }
                Command::New(id) => {
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
                    };
                    let wp = WorldParams {
                        init_pop: 10000,
                        world_size: 180,
                        mesh: 9,
                        n_init_infec: 4,
                        steps_per_day: 3,
                    };
                    let w = World::new(rp, wp);
                    let wr = Arc::new(Mutex::new(w));
                    let mut ws = cws.lock().unwrap();
                    if !ws.contains_key(&id) {
                        ws.insert(id, wr.clone());
                    } else {
                        println!("{} already exists.", id);
                    }
                }
                Command::Start(id, days) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        tx.send(Message::Run(start(wr, days))).unwrap();
                    }
                    None => {
                        println!("{} does not exist.", id);
                    }
                },
                Command::Step(id) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        tx.send(Message::Run(step(wr))).unwrap();
                    }
                    None => println!("{} does not exist.", id),
                },
                Command::Stop(id) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        tx.send(Message::Run(stop(wr))).unwrap();
                    }
                    None => println!("{} does not exist.", id),
                },
                Command::Reset(id) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        tx.send(Message::Run(reset(wr))).unwrap();
                    }
                    None => {
                        println!("{} does not exist.", id);
                    }
                },
                Command::Delete(id) => {
                    let mut ws = cws.lock().unwrap();
                    match &ws.remove(&id) {
                        Some(wr) => {
                            tx.send(Message::Run(stop(wr))).unwrap();
                        }
                        None => println!("{} does not exist.", id),
                    }
                }
                Command::Export(id, path) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => match export(wr, path.as_str()) {
                        Ok(_) => println!("{} was successfully exported", path),
                        Err(e) => println!("{}", e),
                    },
                    None => {
                        println!("{} does not exist.", id);
                    }
                },
                Command::Debug(id) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        debug(wr);
                    }
                    None => println!("{} does not exist.", id),
                },
            },
            Err(e) => println!("{}", e),
        }
    });

    let pool_handle = thread::spawn(move || loop {
        match rx.recv().unwrap() {
            Message::Quit => {
                break;
            }
            Message::Run(h) => {
                h.join().unwrap();
            }
        }
    });

    if_handle.join().unwrap();
    pool_handle.join().unwrap();
}
