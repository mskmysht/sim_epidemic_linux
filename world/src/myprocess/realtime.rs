use world_if::{
    pubsub::Publisher,
    realtime::{Request, Response, ResponseError, ResponseOk, WorldState, WorldStatus},
};

use crate::{
    util::{self, random::DistInfo},
    world::{
        commons::{RuntimeParams, WorldParams, WrkPlcMode},
        World,
    },
};

use std::{
    io,
    thread::{self, JoinHandle},
};

#[derive(Default, Debug)]
struct WorldStepInfo {
    prev_time: f64,
    steps_per_sec: f64,
}

pub struct WorldSpawner<P> {
    world: World,
    info: WorldStepInfo,
    publisher: P,
}

impl<P> WorldSpawner<P>
where
    P: Publisher<Req = Request, Res = Response, Stat = WorldStatus> + Send + 'static,
    P::RecvErr: std::fmt::Debug,
    P::SendErr<Response>: std::fmt::Debug,
    P::SendErr<WorldStatus>: std::fmt::Debug,
{
    pub fn new(id: String, publisher: P) -> Self {
        let world = World::new(id, new_runtime_params(), new_world_params());
        let spawner = Self {
            world,
            info: WorldStepInfo::default(),
            publisher,
        };
        spawner.send_status(WorldState::Stopped);
        spawner
    }

    pub fn spawn(self) -> io::Result<JoinHandle<()>> {
        thread::Builder::new()
            .name(format!("world_{}", self.world.id.clone()))
            .spawn(move || self.listen())
    }

    #[inline]
    fn res_ok(&self) {
        self.publisher
            .send_response(ResponseOk::Success.into())
            .unwrap();
    }

    #[inline]
    fn res_ok_with(&self, msg: String) {
        self.publisher
            .send_response(ResponseOk::SuccessWithMessage(msg).into())
            .unwrap();
    }

    #[inline]
    fn res_err(&self, err: ResponseError) {
        self.publisher.send_response(err.into()).unwrap();
    }

    #[inline]
    fn send_status(&self, state: WorldState) {
        self.publisher
            .send_on_stream(WorldStatus::new(self.world.runtime_params.step, state))
            .unwrap();
    }

    #[inline]
    fn reset(&mut self) {
        self.world.reset();
        self.info = WorldStepInfo::default();
        self.send_status(WorldState::Stopped);
        self.res_ok();
    }

    #[inline]
    fn step(&mut self) {
        if self.is_ended() {
            self.res_err(ResponseError::AlreadyEnded);
        } else {
            self.inline_step();
            let state = if self.is_ended() {
                WorldState::Ended
            } else {
                WorldState::Stopped
            };

            self.send_status(state);
            self.res_ok();
        }
    }

    #[inline]
    fn stop(&mut self) {
        self.send_status(WorldState::Stopped);
        self.res_ok();
    }

    #[inline]
    fn debug(&self) {
        self.res_ok_with(format!("{}\n{:?}", self.world.log, self.info));
    }

    #[inline]
    fn export(&self, dir: String) {
        match self.world.export(&dir) {
            Ok(_) => self.res_ok_with(format!("{} was successfully exported", dir)),
            Err(_) => self.res_err(ResponseError::FileExportFailed),
        }
    }

    fn start(&mut self, stop_at: u32) -> bool {
        if self.is_ended() {
            self.res_err(ResponseError::AlreadyEnded);
            return false;
        }

        let step_to_end = stop_at * self.world.world_params.steps_per_day;
        self.res_ok();
        while self.step_cont(step_to_end) {
            if let Some(msg) = self.publisher.try_recv().unwrap() {
                match msg {
                    Request::Delete => {
                        self.res_ok();
                        return true;
                    }
                    Request::Stop => {
                        self.stop();
                        break;
                    }
                    Request::Reset => {
                        self.reset();
                        break;
                    }
                    #[cfg(debug_assertions)]
                    Request::Debug => self.debug(),
                    _ => self.res_err(ResponseError::AlreadyStarted),
                }
            }
        }
        false
    }

    #[inline]
    fn step_cont(&mut self, step_to_end: u32) -> bool {
        self.inline_step();
        let (state, cont) = if self.is_ended() {
            (WorldState::Ended, false)
        } else if self.world.runtime_params.step > step_to_end {
            (WorldState::Stopped, false)
        } else {
            (WorldState::Started, true)
        };
        self.send_status(state);
        cont
    }

    #[inline]
    fn inline_step(&mut self) {
        self.world.step();
        let new_time = util::get_uptime();
        let time_passed = new_time - self.info.prev_time;
        if time_passed < 1.0 {
            self.info.steps_per_sec +=
                ((1.0 / time_passed).min(30.0) - self.info.steps_per_sec) * 0.2;
        }
        self.info.prev_time = new_time;
    }

    #[inline]
    fn is_ended(&self) -> bool {
        self.world.get_n_infected() == 0
    }

    fn listen(mut self) {
        loop {
            match self.publisher.recv().unwrap() {
                Request::Delete => {
                    self.res_ok();
                    break;
                }
                Request::Reset => self.reset(),
                Request::Step => self.step(),
                Request::Start(stop_at) => {
                    if self.start(stop_at) {
                        break;
                    }
                    debug_assert!(false, "force to invoke panic");
                }
                #[cfg(debug_assertions)]
                Request::Debug => self.debug(),
                Request::Export(dir) => self.export(dir),
                Request::Stop => self.res_err(ResponseError::AlreadyStopped),
            }
        }
    }
}

fn new_world_params() -> WorldParams {
    WorldParams::new(
        10000,
        360,
        18,
        16,
        0.05.into(),
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

fn new_runtime_params() -> RuntimeParams {
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
        step: 0,
    }
}
