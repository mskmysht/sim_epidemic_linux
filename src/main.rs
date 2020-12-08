use common_types::MRef;
use regex::Regex;
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

mod agent;
mod common_types;
mod contact;
mod gathering;
mod iter;
mod stat;
mod world;

use world::*;

enum Command {
    Quit,
    List,
    New(i32),
    Stop(i32),
    Resume(i32),
    Delete(i32),
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
    |^\s*(?P<c1>new|stop|resume|delete)\s+(?P<id>\d+)\s*$  # new|stop [id] ... single argumant commands
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
                        "stop" => Some(Command::Stop(id)),
                        "resume" => Some(Command::Resume(id)),
                        "delete" => Some(Command::Delete(id)),
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
                    let w = World::new(); //format!("world {}", id).as_str());
                    let aw = Arc::new(Mutex::new(w));
                    let mut ws = cws.lock().unwrap();
                    if !ws.contains_key(&id) {
                        ws.insert(id, Arc::clone(&aw));
                        tx.send(Message::Run(new_handle(Arc::clone(&aw)))).unwrap();
                    } else {
                        println!("{} already exists.", id);
                    }
                }
                Command::Stop(id) => match cws.lock().unwrap().get(&id) {
                    Some(_) => {
                        // cw.lock().unwrap().stop()
                    }
                    None => println!("{} does not exist.", id),
                },
                Command::Resume(id) => match cws.lock().unwrap().get(&id) {
                    Some(aw) => {
                        tx.send(Message::Run(new_handle(Arc::clone(&aw)))).unwrap();
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
