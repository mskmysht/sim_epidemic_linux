pub mod parse;
pub mod spawner;

use std::{sync::mpsc, thread};

use parse::{RequestWrapper, WorldParser};
use repl::Parsable;
use spawner::{MpscPublisher, MpscSubscriber, Response, ResponseOk, WorldSpawner};

use world_core::{
    util::random::DistInfo,
    world::commons::{RuntimeParams, WorldParams, WrkPlcMode},
};

pub fn run(runtime_params: RuntimeParams, world_params: WorldParams) {
    let (req_tx, req_rx) = mpsc::channel();
    let (res_tx, res_rx) = mpsc::channel();
    let (stream_tx, stream_rx) = mpsc::channel();
    let spawner = WorldSpawner::new(
        "test".to_string(),
        MpscPublisher::new(stream_tx, req_rx, res_tx),
        runtime_params,
        world_params,
    );
    let handle = spawner.spawn().unwrap();
    let input = thread::spawn(move || {
        let subscriber = MpscSubscriber::new(req_tx, res_rx, stream_rx);
        let mut status = subscriber.recv_status().unwrap();
        loop {
            match WorldParser::recv_input() {
                repl::Command::Quit => break,
                repl::Command::None => {}
                repl::Command::Delegate(input) => {
                    let output = match input {
                        RequestWrapper::Info => {
                            if let Some(s) = subscriber.seek_status().into_iter().last() {
                                status = s;
                            }
                            ResponseOk::SuccessWithMessage((&status).to_string()).into()
                        }
                        RequestWrapper::Req(req) => subscriber.request(req).unwrap(),
                    };
                    match output {
                        Response::Ok(s) => println!("[info] {s:?}"),
                        Response::Err(e) => eprintln!("[error] {e:?}"),
                    }
                }
            }
        }
    });
    handle.join().unwrap();
    input.join().unwrap();
}

pub fn new_world_params(init_n_pop: u32, infected: f64) -> WorldParams {
    WorldParams::new(
        init_n_pop,
        360,
        18,
        16,
        infected.into(),
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
    )
}

pub fn new_runtime_params() -> RuntimeParams {
    RuntimeParams {
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
        ..Default::default()
    }
}
