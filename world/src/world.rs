use chrono::serde::ts_seconds;
use chrono::Local;
use ipc_channel::ipc::{IpcReceiver, IpcSender};
use serde::{Deserialize, Serialize};

use crate::agent::{
    location::{Cemetery, Field, Hospital, Warps},
    Agent, ParamsForStep, VaccineInfo, VariantInfo,
};
use crate::commons::{enum_map::EnumMap, math::Point, RuntimeParams, WorldParams};
use crate::commons::{DistInfo, HealthType, WrkPlcMode};
use crate::gathering::Gatherings;
use crate::log::StepLog;
use crate::testing::TestQueue;
use std::{error, io, thread::JoinHandle};
use std::{
    f64, fmt, thread,
    time::{SystemTime, UNIX_EPOCH},
    usize,
};

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
    is_finished: bool,
    //[todo] predicate_to_stop: bool,
    step_log: StepLog,
    scenario_index: i32,
    scenario: Vec<i32>, //[todo] Vec<Scenario>
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
            agents: Vec::with_capacity(world_params.init_n_pop),
            is_finished: false,
            step_log: StepLog::default(),
            scenario_index: 0,
            scenario: Vec::new(),
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
            let a = self.agents[i].clone();
            match t {
                HealthType::Symptomatic if n_q_symptomatic > 0 => {
                    n_q_symptomatic -= 1;
                    self.hospital.add(a);
                    continue;
                }
                HealthType::Asymptomatic if n_q_asymptomatic > 0 => {
                    n_q_asymptomatic -= 1;
                    self.hospital.add(a);
                    continue;
                }
                _ => {}
            }

            let idx = self.world_params.into_grid_index(&a.get_pt());
            self.field.add(a, idx);
        }

        // reset test queue
        self.runtime_params.step = 0;
        self.step_log.reset(
            n_pop - n_infected,
            n_symptomatic,
            n_infected - n_symptomatic,
        );
        self.scenario_index = 0;
        //[todo] self.exec_scenario();

        self.is_finished = false;
    }

    fn exec_scenario(&mut self) {
        todo!("execute scenario");
    }

    fn do_one_step(&mut self) {
        let pfs = ParamsForStep::new(
            &self.world_params,
            &self.runtime_params,
            &self.variant_info,
            &self.vaccine_info,
        );

        self.field.reset_for_step();

        let mut count_reason = EnumMap::default();
        let mut count_result = EnumMap::default();

        if !pfs.go_home_back() {
            self.gatherings.steps(
                &self.field,
                &self.gat_spots_fixed,
                &self.agents,
                pfs.wp,
                pfs.rp,
            );
        }

        self.field.intersect(&pfs);

        self.test_queue
            .accept(&pfs, &mut count_reason, &mut count_result);

        self.field.steps(
            &mut self.warps,
            &mut self.test_queue,
            &mut self.step_log,
            &pfs,
        );
        self.hospital
            .steps(&mut self.warps, &mut self.step_log, &pfs);
        self.warps.steps(
            &mut self.field,
            &mut self.hospital,
            &mut self.cemetery,
            &mut self.test_queue,
            &pfs,
        );

        self.is_finished = self.step_log.push();
        self.runtime_params.step += 1;
        // [todo] self.predicate_to_stop
    }

    fn export(&self, dir: &str) -> Result<(), Box<dyn error::Error>> {
        self.step_log.write(
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

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum LoopMode {
    LoopNone,
    LoopRunning,
    LoopFinished,
    LoopEndByUser,
    LoopEndAsDaysPassed,
    //[todo] LoopEndByCondition,
    //[todo] LoopEndByTimeLimit,
}

impl Default for LoopMode {
    fn default() -> Self {
        LoopMode::LoopNone
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct WorldStatus {
    step: u64,
    mode: LoopMode,
    #[serde(with = "ts_seconds")]
    time_stamp: chrono::DateTime<chrono::Utc>,
}

impl WorldStatus {
    fn new(step: u64, mode: LoopMode) -> Self {
        Self {
            step,
            mode,
            time_stamp: chrono::Utc::now(),
        }
    }
}

impl fmt::Display for WorldStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}]step:{},mode:{:?}",
            self.time_stamp, self.step, self.mode
        )
    }
}

impl From<&WorldStatus> for String {
    fn from(status: &WorldStatus) -> Self {
        status.to_string()
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum Request {
    Delete,
    Start(u64),
    Step,
    Stop,
    Reset,
    Debug,
    Export(String),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum ErrorStatus {
    AlreadyFinished,
    AlreadyStopped,
    AlreadyRunning,
    FileExportFailed,
}

pub type Response = std::result::Result<Option<String>, super::ErrorStatus>;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct WorldInfo {
    req: IpcSender<Request>,
    res: IpcReceiver<Response>,
    stream: IpcReceiver<WorldStatus>,
    status: WorldStatus,
}

impl WorldInfo {
    pub fn new(
        req: IpcSender<Request>,
        res: IpcReceiver<Response>,
        stream: IpcReceiver<WorldStatus>,
        status: WorldStatus,
    ) -> Self {
        Self {
            stream,
            req,
            res,
            status,
        }
    }

    pub fn seek_status(&mut self) -> &WorldStatus {
        let mut v = None;
        while let Ok(s) = self.stream.try_recv() {
            v = Some(s);
        }
        if let Some(s) = v {
            self.status = s;
        }
        &self.status
    }

    // pub fn delete(&self) -> Response {
    //     self.send(Request::Delete)
    // }

    // pub fn reset(&self) -> Response {
    //     self.send(Request::Reset)
    // }

    // pub fn start(&self, stop_at: u64) -> Response {
    //     self.send(Request::Start(stop_at))
    // }

    // pub fn step(&self) -> Response {
    //     self.send(Request::Step)
    // }

    // pub fn stop(&self) -> Response {
    //     self.send(Request::Stop)
    // }

    // pub fn export(&self, dir: String) -> Response {
    //     self.send(Request::Export(dir))
    // }

    // pub fn debug(&self) -> Response {
    //     self.send(Request::Debug)
    // }

    pub fn send(&self, req: Request) -> Response {
        self.req.send(req).unwrap();
        self.res.recv().unwrap()
    }
}

pub fn spawn_world(
    id: String,
    stream_tx: IpcSender<WorldStatus>,
    req_rx: IpcReceiver<Request>,
    res_tx: IpcSender<Response>,
    // drop_tx: mpsc::Sender<()>,
) -> io::Result<(JoinHandle<String>, WorldStatus)> {
    #[derive(Default, Debug)]
    struct StepInfo {
        prev_time: f64,
        steps_per_sec: f64,
    }

    let mut world = World::new(id, new_runtime_params(), new_world_params());
    let mut info = StepInfo::default();
    let status = WorldStatus::new(world.runtime_params.step, LoopMode::LoopNone);

    let _res_tx = res_tx.clone();
    macro_rules! res_status {
        (ok) => {
            res_tx.send(Ok(None)).unwrap()
        };
        (ok; $msg:expr) => {
            res_tx.send(Ok(Some($msg))).unwrap()
        };
        (ok_) => {
            _res_tx.send(Ok(None)).unwrap()
        };
        (err; $err:expr) => {
            res_tx.send(Err($err)).unwrap()
        };
    }

    macro_rules! debug {
        () => {{
            format!("{}\n{:?}", world.step_log, info)
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
            info = StepInfo::default();
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

    // thread::spawn(move || {
    //     if let Ok(id) = handle.join() {
    //         println!("[info] Delete world {id}.");
    //         res_status!(ok_);
    //     } else {
    //         // drop_tx.send(()).unwrap();
    //     }
    // });
    Ok((handle, status))
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
