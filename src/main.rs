use common_types::{DistInfo, MRef, RuntimeParams, WorldParams};
use regex::Regex;
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

mod agent;
mod common_types;
mod contact;
mod dyn_struct;
mod enum_map;
mod gathering;
mod iter;
mod stat;
mod world;

use world::*;

enum Command {
    Quit,
    List,
    New(i32),
    Start(i32),
    Stop(i32),
    // Resume(i32),
    Delete(i32),
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
    ^\s*(?P<c0>list|:q)\s*$  # list|:q ... no argument commnads
    |^\s*(?P<c1>new|start|stop|debug|delete)\s+(?P<id>\d+)\s*$  # new|stop [id] ... single argumant commands
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

        let ocmd = match re.captures(input.as_str()) {
            Some(caps) => match (caps.name("c0"), caps.name("c1"), caps.name("id")) {
                (Some(m), _, _) => match m.as_str() {
                    "list" => Some(Command::List),
                    ":q" => Some(Command::Quit),
                    _ => None,
                },
                (_, Some(c), Some(sid)) => {
                    let id: i32 = sid.as_str().parse().unwrap();
                    match c.as_str() {
                        "new" => Some(Command::New(id)),
                        "start" => Some(Command::Start(id)),
                        "stop" => Some(Command::Stop(id)),
                        // "resume" => Some(Command::Resume(id)),
                        "delete" => Some(Command::Delete(id)),
                        "debug" => Some(Command::Debug(id)),
                        _ => None,
                    }
                }
                _ => None,
            },
            _ => None,
        };

        match ocmd {
            Some(cmd) => match cmd {
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
                        mob_fr: 50.0,
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
                        immun: DistInfo::new(30.0, 180.0, 360.0),
                        mob_dist: DistInfo::new(10.0, 30.0, 80.0),
                        gat_sz: DistInfo::new(5.0, 10.0, 20.0),
                        gat_dr: DistInfo::new(24.0, 48.0, 168.0),
                        gat_st: DistInfo::new(50.0, 80.0, 100.0),
                        step: 0,
                    };
                    let wp = WorldParams {
                        init_pop: 10000,
                        world_size: 360,
                        mesh: 18,
                        n_init_infec: 4,
                        steps_per_day: 16,
                    };
                    let w = World::new(rp, wp); //format!("world {}", id).as_str());
                    let wr = Arc::new(Mutex::new(w));
                    let mut ws = cws.lock().unwrap();
                    if !ws.contains_key(&id) {
                        ws.insert(id, wr.clone());
                    // tx.send(Message::Run(new_handle(Arc::clone(&aw)))).unwrap();
                    } else {
                        println!("{} already exists.", id);
                    }
                }
                Command::Start(id) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        println!("{}", wr.lock().unwrap().id);
                        tx.send(Message::Run(start(wr.clone(), 10, 0.0, 0.0)))
                            .unwrap();
                    }
                    None => {
                        println!("{} does not exist.", id);
                    }
                },
                Command::Stop(id) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        stop(wr);
                        // cw.lock().unwrap().stop()
                    }
                    None => println!("{} does not exist.", id),
                },
                Command::Delete(id) => {
                    let mut ws = cws.lock().unwrap();
                    match ws.remove(&id) {
                        Some(_) => {
                            // cw.lock().unwrap().stop(),
                        }
                        None => println!("{} does not exist.", id),
                    }
                }
                Command::Debug(id) => match cws.lock().unwrap().get(&id) {
                    Some(wr) => {
                        let w = wr.lock().unwrap();
                        w.debug_pop_internal();
                    }
                    None => println!("{} does not exist.", id),
                },
            },
            None => {}
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
