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
    error::{self, Error},
    f64, io,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
    usize,
};

use world_if::{ErrorStatus, LoopMode, Request, Response, WorldStatus};

use chrono::Local;
use ipc_channel::ipc::{IpcReceiver, IpcSender};

struct InnerWorld {
    id: String,
    runtime_params: RuntimeParams,
    world_params: WorldParams,
    agents: Vec<Agent>,
    field: Field,
    warps: Warps,
    hospital: Hospital,
    cemetery: Cemetery,
    test_queue: TestQueue,
    is_finished: bool,
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

impl InnerWorld {
    pub fn new(id: String, runtime_params: RuntimeParams, world_params: WorldParams) -> InnerWorld {
        let mut w = InnerWorld {
            id,
            runtime_params,
            world_params,
            agents: Vec::with_capacity(world_params.init_n_pop),
            is_finished: false,
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
        let n_infected = (self.world_params.init_n_pop() * self.world_params.infected.r()) as usize;
        let n_recovered = {
            let k = (self.world_params.init_n_pop() * self.world_params.recovered.r()) as usize;
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
            n_pop,
            n_infected,
            n_recovered,
            n_dist,
            &self.world_params,
            &self.runtime_params,
        );

        let mut n_q_symptomatic =
            (n_symptomatic as f64 * self.world_params.q_symptomatic.r()) as u64;
        let mut n_q_asymptomatic =
            ((n_infected - n_symptomatic) as f64 * self.world_params.q_asymptomatic.r()) as u64;
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

        self.is_finished = false;
    }

    fn do_one_step(&mut self) {
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

        self.is_finished = self.log.push();
        self.runtime_params.step += 1;
        // [todo] self.predicate_to_stop
    }

    fn export(&self, dir: &str) -> Result<(), Box<dyn error::Error>> {
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

#[derive(Default, Debug)]
struct WorldStepInfo {
    prev_time: f64,
    steps_per_sec: f64,
}

pub struct World<C: WorldChannel> {
    inner: InnerWorld,
    info: WorldStepInfo,
    channel: C,
}

pub trait WorldChannel {
    fn recv(&self) -> Result<Request, Box<dyn Error>>;
    fn try_recv(&self) -> Result<Request, Box<dyn Error>>;
    fn send_response(&self, data: Response) -> Result<(), Box<dyn Error>>;
    fn send_on_stream(&self, data: WorldStatus) -> Result<(), Box<dyn Error>>;
}

pub struct MpscWorldChannel {
    stream_tx: mpsc::Sender<WorldStatus>,
    req_rx: mpsc::Receiver<Request>,
    res_tx: mpsc::Sender<Response>,
}

impl MpscWorldChannel {
    pub fn new(
        stream_tx: mpsc::Sender<WorldStatus>,
        req_rx: mpsc::Receiver<Request>,
        res_tx: mpsc::Sender<Response>,
    ) -> Self {
        Self {
            stream_tx,
            req_rx,
            res_tx,
        }
    }
}

impl WorldChannel for MpscWorldChannel {
    fn recv(&self) -> Result<Request, Box<dyn Error>> {
        Ok(self.req_rx.recv()?)
    }

    fn try_recv(&self) -> Result<Request, Box<dyn Error>> {
        Ok(self.req_rx.try_recv()?)
    }

    fn send_response(&self, data: Response) -> Result<(), Box<dyn Error>> {
        Ok(self.res_tx.send(data)?)
    }

    fn send_on_stream(&self, data: WorldStatus) -> Result<(), Box<dyn Error>> {
        Ok(self.stream_tx.send(data)?)
    }
}

pub struct IpcWorldChannel {
    stream_tx: IpcSender<WorldStatus>,
    req_rx: IpcReceiver<Request>,
    res_tx: IpcSender<Response>,
}

impl IpcWorldChannel {
    pub fn new(
        stream_tx: IpcSender<WorldStatus>,
        req_rx: IpcReceiver<Request>,
        res_tx: IpcSender<Response>,
    ) -> Self {
        Self {
            stream_tx,
            req_rx,
            res_tx,
        }
    }
}

impl WorldChannel for IpcWorldChannel {
    fn recv(&self) -> Result<Request, Box<dyn Error>> {
        Ok(self.req_rx.recv()?)
    }

    fn try_recv(&self) -> Result<Request, Box<dyn Error>> {
        Ok(self.req_rx.try_recv()?)
    }

    fn send_response(&self, data: Response) -> Result<(), Box<dyn Error>> {
        Ok(self.res_tx.send(data)?)
    }

    fn send_on_stream(&self, data: WorldStatus) -> Result<(), Box<dyn Error>> {
        Ok(self.stream_tx.send(data)?)
    }
}

impl<C: WorldChannel + Send + 'static> World<C> {
    pub fn spawn(id: String, channel: C) -> io::Result<(JoinHandle<String>, WorldStatus)> {
        let world = Self {
            inner: InnerWorld::new(id, new_runtime_params(), new_world_params()),
            info: WorldStepInfo::default(),
            channel,
        };
        let status = WorldStatus::new(world.inner.runtime_params.step, LoopMode::LoopNone);
        Ok((world.get_handle()?, status))
    }

    #[inline]
    fn res_status_ok(&self, msg: Option<String>) {
        self.channel.send_response(Ok(msg)).unwrap();
    }

    #[inline]
    fn res_status_err(&self, err: ErrorStatus) {
        self.channel.send_response(Err(err)).unwrap();
    }

    #[inline]
    fn send_status(&self, mode: LoopMode) {
        self.channel
            .send_on_stream(WorldStatus::new(self.inner.runtime_params.step, mode))
            .unwrap();
    }

    #[inline]
    fn reset(&mut self) {
        self.inner.reset();
        self.info = WorldStepInfo::default();
        self.send_status(LoopMode::LoopNone);
    }

    #[inline]
    fn step(&mut self, default_mode: LoopMode) -> bool {
        self.inner.do_one_step();
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
        if self.inner.is_finished {
            self.send_status(LoopMode::LoopFinished);
            true
        } else {
            self.send_status(default_mode);
            false
        }
    }

    #[inline]
    fn auto_stopped(&self, stop_at: u64) -> bool {
        if self.inner.runtime_params.step >= stop_at * self.inner.world_params.steps_per_day - 1 {
            self.send_status(LoopMode::LoopEndAsDaysPassed);
            true
        } else {
            false
        }
    }

    #[inline]
    fn stop(&self) {
        self.send_status(LoopMode::LoopEndByUser);
    }

    #[inline]
    fn debug(&self) -> String {
        format!("{}\n{:?}", self.inner.log, self.info)
    }

    fn get_handle(self) -> io::Result<JoinHandle<String>> {
        thread::Builder::new()
            .name(format!("world_{}", self.inner.id.clone()))
            .spawn(move || self.run_loop())
    }

    fn run_loop(mut self) -> String {
        'outer: loop {
            match self.channel.recv().unwrap() {
                Request::Delete => {
                    self.res_status_ok(None);
                    break self.inner.id;
                }
                Request::Reset => {
                    self.reset();
                    self.res_status_ok(None);
                }
                Request::Step => {
                    if !self.inner.is_finished {
                        self.step(LoopMode::LoopEndByUser);
                        self.res_status_ok(None);
                    } else {
                        self.res_status_err(ErrorStatus::AlreadyFinished);
                    }
                }
                Request::Start(stop_at) => {
                    if self.inner.is_finished {
                        self.res_status_err(ErrorStatus::AlreadyFinished);
                    } else {
                        self.res_status_ok(None);
                        loop {
                            if self.auto_stopped(stop_at) {
                                break;
                            }
                            if self.step(LoopMode::LoopRunning) {
                                break;
                            }
                            if let Ok(msg) = self.channel.try_recv() {
                                match msg {
                                    Request::Delete => {
                                        self.res_status_ok(None);
                                        break 'outer self.inner.id;
                                    }
                                    Request::Stop => {
                                        self.stop();
                                        self.res_status_ok(None);
                                        break;
                                    }
                                    Request::Reset => {
                                        self.reset();
                                        self.res_status_ok(None);
                                        break;
                                    }
                                    Request::Debug => self.res_status_ok(Some(self.debug())),
                                    _ => self.res_status_err(ErrorStatus::AlreadyRunning),
                                }
                            }
                        }
                    }
                }
                Request::Debug => self.res_status_ok(Some(self.debug())),
                Request::Export(dir) => match self.inner.export(&dir) {
                    Ok(_) => self.res_status_ok(Some(format!("{} was successfully exported", dir))),
                    Err(_) => self.res_status_err(ErrorStatus::FileExportFailed),
                },
                Request::Stop => self.res_status_err(ErrorStatus::AlreadyStopped),
            }
        }
    }
}

/**/
pub fn spawn_world(
    id: String,
    stream_tx: IpcSender<WorldStatus>,
    req_rx: IpcReceiver<Request>,
    res_tx: IpcSender<Response>,
    // drop_tx: mpsc::Sender<()>,
) -> io::Result<(JoinHandle<String>, WorldStatus)> {
    let mut world = InnerWorld::new(id, new_runtime_params(), new_world_params());
    let mut info = WorldStepInfo::default();
    let status = WorldStatus::new(world.runtime_params.step, LoopMode::LoopNone);

    let _res_tx = res_tx.clone();
    macro_rules! res_status {
        (ok) => {
            res_tx.send(Ok(None)).unwrap()
        };
        (ok; $msg:expr) => {
            res_tx.send(Ok(Some($msg))).unwrap()
        };
        (err; $err:expr) => {
            res_tx.send(Err($err)).unwrap()
        };
    }

    macro_rules! debug {
        () => {{
            format!("{}\n{:?}", world.log, info)
        }};
    }

    macro_rules! send_status {
        ($mode:expr) => {
            stream_tx
                .send(WorldStatus::new(world.runtime_params.step, $mode))
                .unwrap()
        };
    }

    macro_rules! reset {
        () => {
            world.reset();
            info = WorldStepInfo::default();
            send_status!(LoopMode::LoopNone);
        };
    }

    macro_rules! step {
        ($default_mode:expr) => {{
            world.do_one_step();
            //    if loop_mode == LoopMode::LoopEndByCondition
            //        && world.scenario_index < self.scenario.len() as i32
            //    {
            //        world.exec_scenario();
            //        loop_mode = LoopMode::LoopRunning;
            //    }
            let new_time = get_uptime();
            let time_passed = new_time - info.prev_time;
            if time_passed < 1.0 {
                info.steps_per_sec += ((1.0 / time_passed).min(30.0) - info.steps_per_sec) * 0.2;
            }
            info.prev_time = new_time;
            if world.is_finished {
                send_status!(LoopMode::LoopFinished);
                true
            } else {
                send_status!($default_mode);
                false
            }
        }};
    }

    macro_rules! auto_stopped {
        ($stop_at:expr) => {
            if world.runtime_params.step >= $stop_at * world.world_params.steps_per_day - 1 {
                send_status!(LoopMode::LoopEndAsDaysPassed);
                true
            } else {
                false
            }
        };
    }

    macro_rules! stop {
        () => {
            send_status!(LoopMode::LoopEndByUser);
        };
    }

    let handle = thread::Builder::new()
        .name(format!("world_{}", world.id.clone()))
        .spawn(move || 'outer: loop {
            match req_rx.recv().unwrap() {
                Request::Delete => {
                    res_status!(ok);
                    break world.id;
                }
                Request::Reset => {
                    reset!();
                    res_status!(ok);
                }
                Request::Step => {
                    if !world.is_finished {
                        step!(LoopMode::LoopEndByUser);
                        res_status!(ok);
                    } else {
                        res_status!(err; ErrorStatus::AlreadyFinished);
                    }
                }
                Request::Start(stop_at) => {
                    if world.is_finished {
                        res_status!(err; ErrorStatus::AlreadyFinished);
                    } else {
                        res_status!(ok);
                        loop {
                            if auto_stopped!(stop_at) {
                                break;
                            }
                            if step!(LoopMode::LoopRunning) {
                                break;
                            }
                            if let Ok(msg) = req_rx.try_recv() {
                                match msg {
                                    Request::Delete => {
                                        res_status!(ok);
                                        break 'outer world.id;
                                    }
                                    Request::Stop => {
                                        stop!();
                                        res_status!(ok);
                                        break;
                                    }
                                    Request::Reset => {
                                        reset!();
                                        res_status!(ok);
                                        break;
                                    }
                                    Request::Debug => res_status!(ok; debug!()),
                                    _ => res_status!(err; ErrorStatus::AlreadyRunning),
                                }
                            }
                        }
                    }
                }
                Request::Debug => res_status!(ok; debug!()),
                Request::Export(dir) => match world.export(&dir) {
                    Ok(_) => res_status!(ok; format!("{} was successfully exported", dir)),
                    Err(_) => res_status!(err; ErrorStatus::FileExportFailed),
                },
                Request::Stop => res_status!(err; ErrorStatus::AlreadyStopped),
            }
        })?;

    Ok((handle, status))
}
/**/

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
