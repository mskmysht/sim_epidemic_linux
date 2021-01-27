use commons::{DistInfo, MRef, RuntimeParams, WorldParams};
use csv::Writer;
use regex::Regex;
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

enum Command {
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

fn main() {
    let worlds: HashMap<i32, MRef<World>> = HashMap::new();
    let aws = Arc::new(Mutex::new(worlds));
    let (tx, rx) = mpsc::channel();

    let re = Regex::new(
        r"(?x)
    ^\s*(?P<c>new|start|step|stop|reset|delete|export|list|:q|debug)(\s+(?P<id>\d+))?(\s+(?P<days>\d+)|\s+(?P<path>.+))?\s*$
    ",
    )
    .unwrap();

    let if_handle = thread::spawn(move || loop {
        let cws = aws.clone();

        let mut input = String::new();
        io::stdout().flush().unwrap();
        print!("> ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut input).unwrap();

        let cmd_opt = if let Some(caps) = re.captures(input.as_str()) {
            match (
                caps.name("c"),
                caps.name("id"),
                caps.name("days"),
                caps.name("path"),
            ) {
                (Some(c), None, None, None) => match c.as_str() {
                    "list" => Some(Command::List),
                    ":q" => Some(Command::Quit),
                    _ => None,
                },
                (Some(c), Some(sid), None, None) => {
                    let id: i32 = sid.as_str().parse().unwrap();
                    match c.as_str() {
                        "new" => Some(Command::New(id)),
                        "step" => Some(Command::Step(id)),
                        "stop" => Some(Command::Stop(id)),
                        "reset" => Some(Command::Reset(id)),
                        "delete" => Some(Command::Delete(id)),
                        "debug" => Some(Command::Debug(id)),
                        _ => None,
                    }
                }
                (Some(c), Some(sid), Some(sdays), None) => {
                    let id = sid.as_str().parse::<i32>().unwrap();
                    let days = sdays.as_str().parse::<i32>().unwrap();
                    match c.as_str() {
                        "start" => Some(Command::Start(id, days)),
                        _ => None,
                    }
                }
                (Some(c), Some(sid), None, Some(path)) => {
                    let id = sid.as_str().parse::<i32>().unwrap();
                    match c.as_str() {
                        "export" => Some(Command::Export(id, String::from(path.as_str()))),
                        _ => None,
                    }
                }
                _ => None,
            }
        } else {
            None
        };

        if let Some(cmd) = cmd_opt {
            match cmd {
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
                        tx.send(Message::Run(start(wr.clone(), days))).unwrap();
                    }
                    None => {
                        println!("{} does not exist.", id);
                    }
                },
                Command::Step(id) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        step(wr);
                    }
                    None => println!("{} does not exist.", id),
                },
                Command::Stop(id) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        stop(wr);
                    }
                    None => println!("{} does not exist.", id),
                },
                Command::Reset(id) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        wr.lock().unwrap().reset_pop();
                    }
                    None => {
                        println!("{} does not exist.", id);
                    }
                },
                Command::Delete(id) => {
                    let mut ws = cws.lock().unwrap();
                    match &ws.remove(&id) {
                        Some(wr) => {
                            stop(wr);
                        }
                        None => println!("{} does not exist.", id),
                    }
                }
                Command::Export(id, path) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        let mut wtr = Writer::from_path(path).unwrap();
                        export(wr, &mut wtr).unwrap();
                        wtr.flush().unwrap();
                    }
                    None => {
                        println!("{} does not exist.", id);
                    }
                },
                Command::Debug(id) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        let w = wr.lock().unwrap();
                        w.debug();
                    }
                    None => println!("{} does not exist.", id),
                },
            }
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
