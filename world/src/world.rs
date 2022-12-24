mod agent;
pub(super) mod commons;
mod contact;
pub(super) mod testing;

use self::{
    agent::{
        cemetery::Cemetery, field::Field, gathering::Gatherings, hospital::Hospital, warp::Warps,
        Agent,
    },
    commons::{
        HealthType, ParamsForStep, RuntimeParams, VaccineInfo, VariantInfo, WorldParams, WrkPlcMode,
    },
    testing::TestQueue,
};
use crate::{
    log::MyLog,
    util::{enum_map::EnumMap, math::Point, random::DistInfo},
};

use std::{
    f64, io,
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
    usize,
};

use world_if::{
    pubsub::Publisher, Request, Response, ResponseError, ResponseOk, WorldState, WorldStatus,
};

use chrono::Local;

struct World {
    id: String,
    runtime_params: RuntimeParams,
    world_params: WorldParams,
    agents: Vec<Agent>,
    field: Field,
    warps: Warps,
    hospital: Hospital,
    cemetery: Cemetery,
    test_queue: TestQueue,
    // is_finished: bool,
    //[todo] predicate_to_stop: bool,
    log: MyLog,
    scenario_index: i32,
    //[todo] scenario: Vec<i32>,
    gatherings: Gatherings,
    gat_spots_fixed: Vec<Point>,
    //[todo] n_mesh: usize,
    //[todo] n_pop: usize,
    variant_info: Vec<VariantInfo>,
    vaccine_info: Vec<VaccineInfo>,
}

impl World {
    pub fn new(id: String, runtime_params: RuntimeParams, world_params: WorldParams) -> World {
        let mut w = World {
            id,
            runtime_params,
            world_params,
            agents: Vec::with_capacity(world_params.init_n_pop as usize),
            // is_finished: false,
            log: MyLog::default(),
            scenario_index: 0,
            gatherings: Gatherings::new(),
            variant_info: VariantInfo::default_list(),
            vaccine_info: VaccineInfo::default_list(),
            field: Field::new(world_params.mesh),
            warps: Warps::new(),
            hospital: Hospital::new(),
            cemetery: Cemetery::new(),
            test_queue: TestQueue::new(),
            gat_spots_fixed: Vec::new(),
        };

        for _ in 0..world_params.init_n_pop {
            w.agents.push(Agent::new())
        }
        w.reset();
        w
    }

    pub fn reset(&mut self) {
        //[todo] set runtime params of scenario != None
        let n_pop = self.world_params.init_n_pop;
        let n_dist = (self.runtime_params.dst_ob.r() * self.world_params.init_n_pop()) as usize;
        let n_infected = (self.world_params.init_n_pop() * self.world_params.infected.r()) as u32;
        let n_recovered = {
            let k = (self.world_params.init_n_pop() * self.world_params.recovered.r()) as u32;
            if n_infected + k > n_pop {
                n_pop - n_infected
            } else {
                k
            }
        };

        self.gatherings.clear();
        self.field.clear();
        self.hospital.clear();
        self.cemetery.clear();
        self.warps.clear();

        let (cats, n_symptomatic) = Agent::reset_all(
            &self.agents,
            n_pop as usize,
            n_infected as usize,
            n_recovered as usize,
            n_dist,
            &self.world_params,
            &self.runtime_params,
        );

        let mut n_q_symptomatic =
            (n_symptomatic as f64 * self.world_params.q_symptomatic.r()) as u32;
        let mut n_q_asymptomatic =
            ((n_infected - n_symptomatic) as f64 * self.world_params.q_asymptomatic.r()) as u32;
        for (i, t) in cats.into_iter().enumerate() {
            let agent = self.agents[i].clone();
            match t {
                HealthType::Symptomatic if n_q_symptomatic > 0 => {
                    n_q_symptomatic -= 1;
                    let back_to = agent.read().get_back_to();
                    self.hospital.add(agent, back_to);
                }
                HealthType::Asymptomatic if n_q_asymptomatic > 0 => {
                    n_q_asymptomatic -= 1;
                    let back_to = agent.read().get_back_to();
                    self.hospital.add(agent, back_to);
                }
                _ => {
                    let idx = self.world_params.into_grid_index(&agent.read().get_pt());
                    self.field.add(agent, idx);
                }
            }
        }

        // reset test queue
        self.runtime_params.step = 0;
        self.log.reset(
            n_pop - n_infected,
            n_symptomatic,
            n_infected - n_symptomatic,
        );
        self.scenario_index = 0;
        //[todo] self.exec_scenario();

        // self.is_finished = false;
    }

    fn step(&mut self) {
        let pfs = ParamsForStep::new(
            &self.world_params,
            &self.runtime_params,
            &self.variant_info,
            &self.vaccine_info,
        );

        let mut count_reason = EnumMap::default();
        let mut count_result = EnumMap::default();
        self.test_queue
            .accept(&pfs, &mut count_reason, &mut count_result);

        if !pfs.go_home_back() {
            self.gatherings.step(
                &self.field,
                &self.gat_spots_fixed,
                &self.agents,
                pfs.wp,
                pfs.rp,
            );
        }

        self.field
            .step(&mut self.warps, &mut self.test_queue, &mut self.log, &pfs);
        self.hospital.step(&mut self.warps, &mut self.log, &pfs);
        self.warps.step(
            &mut self.field,
            &mut self.hospital,
            &mut self.cemetery,
            &mut self.test_queue,
            &pfs,
        );

        self.log.push();
        self.runtime_params.step += 1;
        // [todo] self.predicate_to_stop
    }

    pub fn get_n_infected(&self) -> u32 {
        self.log.n_infected()
    }

    fn export(&self, dir: &str) -> io::Result<()> {
        self.log.write(
            &format!("{}_{}", self.id, Local::now().format("%F_%H-%M-%S")),
            dir,
        )
    }
}

/*
- (void)startTimeLimitTimer {
    runtimeTimer = [NSTimer scheduledTimerWithTimeInterval:maxRuntime repeats:NO
        block:^(NSTimer * _Nonnull timer) { [self stop:LoopEndByTimeLimit]; }];
}
*/

fn get_uptime() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime before UNIX EPOCH!")
        .as_secs_f64()
}

// #[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize, Default)]
// pub enum RunningMode {
//     #[default]
//     Stopped,
//     Over,
//     Started,
//     // LoopFinished,
//     // LoopEndByUser,
//     // LoopEndAsDaysPassed,
//     //[todo] LoopEndByCondition,
//     //[todo] LoopEndByTimeLimit,
// }

#[derive(Default, Debug)]
struct WorldStepInfo {
    prev_time: f64,
    steps_per_sec: f64,
}

pub struct WorldSpawner<P: Publisher> {
    world: World,
    info: WorldStepInfo,
    publisher: P,
}

impl<P> WorldSpawner<P>
where
    P: Publisher + Send + 'static,
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
        //    if loop_mode == LoopMode::LoopEndByCondition
        //        && world.scenario_index < self.scenario.len() as i32
        //    {
        //        world.exec_scenario();
        //        loop_mode = LoopMode::LoopRunning;
        //    }
        let new_time = get_uptime();
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
