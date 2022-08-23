use sim_epidemic_linux::commons::{DistInfo, RuntimeParams, WorldParams, WrkPlcMode};
use sim_epidemic_linux::world::*;

use peg::parser;
use std::io::{self, Write};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

pub enum Command {
    Quit,
    Show,
    Start(u64),
    Step,
    Stop,
    Reset,
    Export(String),
    Debug,
    None,
}

pub enum Cmd {
    Show,
    Start(u64),
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
            / debug()
            / none()

        rule quit() -> Command = ":q" { Command::Quit }
        rule show() -> Command = "show" { Command::Show }
        rule start() -> Command = "start" _ days:number() { Command::Start(days) }
        rule step() -> Command = "step" { Command::Step }
        rule stop() -> Command = "stop" { Command::Stop }
        rule reset() -> Command = "reset" { Command::Reset }
        rule export() -> Command = "export" _ path:string() { Command::Export(path) }
        rule debug() -> Command = "debug" { Command::Debug }
        rule none() -> Command = "" { Command::None }

        rule _() = quiet!{ [' ' | '\t']* }
        rule eof() = quiet!{ ['\n'] }
        rule number() -> u64 = n:$(['0'..='9']+) { n.parse().unwrap() }
        rule string() -> String = s:$(['!'..='~'] [' '..='~']*) { String::from(s) }
    }
}

fn new_input_handle(
    wr: Arc<Mutex<World>>,
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
                Command::None => {}
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
                Command::Step => wr.lock().unwrap().step(),
                Command::Stop => wr.lock().unwrap().stop(),
                Command::Reset => wr.lock().unwrap().reset(),
                Command::Export(path) => {
                    let world = &wr.lock().unwrap();
                    match world.export(path.as_str()) {
                        Ok(_) => println!("{} was successfully exported", path),
                        Err(e) => println!("{}", e),
                    }
                }
                Command::Debug => wr.lock().unwrap().debug(),
            },
            Err(e) => print!("{}", e),
        }
    })
}

fn new_world() -> Arc<Mutex<World>> {
    let rp = RuntimeParams {
        mass: 50.0.into(),
        friction: 80.0.into(),
        avoidance: 50.0.into(),
        max_speed: 50.0,
        act_mode: 50.0.into(),
        act_kurt: 0.0.into(),
        mob_act: 50.0.into(),
        gat_act: 50.0.into(),
        incub_act: 0.0.into(),
        fatal_act: 0.0.into(),
        infec: 50.0.into(),
        infec_dst: 3.0,
        contag_delay: 0.5,
        contag_peak: 3.0,
        incub: DistInfo::new(1.0, 5.0, 14.0),
        fatal: DistInfo::new(4.0, 16.0, 20.0),
        therapy_effc: 0.0.into(),
        imn_max_dur: 200.0,
        imn_max_dur_sv: 50.0.into(),
        imn_max_effc: 90.0.into(),
        imn_max_effc_sv: 20.0.into(),
        dst_st: 50.0,
        dst_ob: 20.0.into(),
        mob_freq: DistInfo::new(40.0.into(), 70.0.into(), 100.0.into()),
        mob_dist: DistInfo::new(10.0.into(), 30.0.into(), 80.0.into()),
        back_hm_rt: 75.0.into(),
        gat_fr: 50.0,
        gat_rnd_rt: 50.0.into(),
        gat_sz: DistInfo::new(5.0, 10.0, 20.0),
        gat_dr: DistInfo::new(6.0, 12.0, 24.0),
        gat_st: DistInfo::new(50.0, 80.0, 100.0),
        gat_freq: DistInfo::new(40.0.into(), 70.0.into(), 100.0.into()),
        cntct_trc: 20.0.into(),
        tst_delay: 1.0,
        tst_proc: 1.0,
        tst_interval: 2.0,
        tst_sens: 70.0.into(),
        tst_spec: 99.8.into(),
        tst_sbj_asy: 1.0.into(),
        tst_sbj_sym: 99.0.into(),
        tst_capa: 50.0.into(),
        tst_dly_lim: 3.0,
        step: 0,
    };

    let wp = WorldParams::new(
        10000,
        360,
        18,
        16,
        0.10.into(),
        0.0.into(),
        20.0.into(),
        50.0.into(),
        WrkPlcMode::WrkPlcNone,
        150.0.into(),
        50.0,
        500.0.into(),
        40.0.into(),
        30.0.into(),
        90.0.into(),
        95.0.into(),
        14.0,
        7.0,
        120.0,
        90.0.into(),
    );
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
