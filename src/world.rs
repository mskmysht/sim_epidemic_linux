use crate::gathering::Gatherings;
use crate::log::StepLog;
use crate::testing::TestQueue;
use crate::{
    agent,
    commons::{
        math::{self, Point},
        LoopMode, MyCounter, RuntimeParams, WorldParams,
    },
};
use crate::{
    agent::{
        cont::{Cemetery, Field, Hospital, Warps},
        Agent, ParamsForStep, VaccineInfo, VariantInfo,
    },
    enum_map::EnumMap,
};
use rand::{self, distributions::Alphanumeric, Rng};
use std::{
    f64, fmt,
    sync::{Arc, Mutex},
    thread,
    time::{SystemTime, UNIX_EPOCH},
    usize,
};

// pub type AgentGrid<'a> = &'a [&'a [Mutex<Vec<MRef<Agent>>>]];

#[derive(Default)]
pub struct Hist {
    pub recov_p: Vec<MyCounter>,
    pub incub_p: Vec<MyCounter>,
    pub death_p: Vec<MyCounter>,
}

pub struct World {
    pub id: String,
    loop_mode: LoopMode,
    runtime_params: RuntimeParams,
    world_params: WorldParams,
    // tmp_world_params: WorldParams,
    // n_mesh: usize,
    agents: Vec<Agent>,
    // pub agents_: Mutex<Vec<Agent>>,
    // pub _pop: Mutex<Vec<VecDeque<MRef<Agent>>>>,
    field: Field,
    warps: Warps,
    hospital: Hospital,
    cemetery: Cemetery,
    test_queue: TestQueue,
    // pop: Vec<Option<MRef<Agent>>>,
    // n_pop: usize,
    // p_range: Vec<Range>,
    prev_time: f64,
    steps_per_sec: f64,
    // pub warp_list: Vec<(MRef<Agent>, WarpInfo)>,
    // new_warp_f: HashMap<usize, Arc<WarpInfo>>,
    // testees: HashMap<usize, TestReason>,
    // _testees: Mutex<HashMap<usize, TestType>>,
    stop_at_n_days: Option<u64>,
    // pub q_list: VecDeque<MRef<Agent>>,
    // pub _q_list: VecDeque<usize>,
    // pub c_list: VecDeque<MRef<Agent>>,
    // stat_info: StatInfo,
    step_log: StepLog,
    scenario_index: i32,
    scenario: Vec<i32>, // Vec<Scenario>
    // dsc: Mutex<DynStruct<ContactInfo>>,
    gatherings: Gatherings,
    gat_spots_fixed: Vec<Point>,
    // gathering_map: GatheringMap,
    // pub hist: Mutex<Hist>,
    // pub recov_p_hist: Vec<MyCounter>,
    // pub incub_p_hist: Vec<MyCounter>,
    // pub death_p_hist: Vec<MyCounter>,
    // predicate_to_stop: bool,
    // test_que: VecDeque<MRef<TestEntry>>,
    // dst: DynStruct<TestEntry>,
    variant_info: Vec<VariantInfo>,
    vaccine_info: Vec<VaccineInfo>,
}

impl fmt::Display for World {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "id:{}/steps:{}/loop_mode:{:?}",
            self.id, self.runtime_params.step, self.loop_mode,
        )
    }
}

impl World {
    pub fn new(runtime_params: RuntimeParams, world_params: WorldParams) -> World {
        let mut w = World {
            id: new_unique_string(),
            loop_mode: LoopMode::default(),
            runtime_params,
            world_params,
            // tmp_world_params: world_params,
            // n_mesh: 0,
            agents: Vec::with_capacity(world_params.init_n_pop),
            // n_pop: 0,
            prev_time: 0.0,
            steps_per_sec: 0.0,
            stop_at_n_days: None,
            // stat_info: StatInfo::new(),
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
            // predicate_to_stop: false,

            // hist: Default::default(),
            // recov_p_hist: Vec::new(),
            // incub_p_hist: Vec::new(),
            // death_p_hist: Vec::new(),
            // p_range: Vec::new(),
        };

        for _ in 0..world_params.init_n_pop {
            w.agents
                .push(agent::new_agent(&w.world_params, &w.runtime_params))
        }
        w.reset();
        w
    }

    // fn running(&self) -> bool {
    //     self.loop_mode == LoopMode::LoopRunning
    // }

    pub fn reset(&mut self) {
        // self.field.reset(&self.world_params, &&self.runtime_params);
        // set runtime params of scenario != None
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

        dbg!(
            self.world_params.init_n_pop(),
            self.world_params.infected.r(),
            n_infected
        );
        // 0 -> normal, 1 -> infected, 2 -> recovered
        const SUSCEPTIBLE: u8 = 0;
        const INFECTED: u8 = 1;
        const RECOVERED: u8 = 2;
        let mut cats = {
            let r = n_pop - n_infected;
            if r == 0 {
                vec![INFECTED; n_pop]
            } else {
                let mut cats = if r == n_recovered {
                    vec![RECOVERED; n_pop]
                } else {
                    vec![SUSCEPTIBLE; n_pop]
                };
                let idxs_inf = math::reservoir_sampling(n_pop, n_infected);
                let mut m = usize::MAX;
                for idx in idxs_inf {
                    cats[idx] = INFECTED;
                    if m > idx {
                        m = idx;
                    }
                }
                let cnts_inf = {
                    let mut is = vec![0; r];
                    let mut c = 0;
                    let mut k = m;
                    for i in is.iter_mut().take(r).skip(m) {
                        if cats[k] == 1 {
                            c += 1;
                            k += 1;
                        }
                        *i = c;
                        k += 1;
                    }
                    is
                };
                if r > n_recovered {
                    for i in math::reservoir_sampling(r, n_recovered) {
                        cats[i + cnts_inf[i]] = RECOVERED;
                    }
                }
                cats
            }
        };

        self.gatherings.clear();
        self.field.clear();
        self.hospital.clear();
        self.cemetery.clear();
        self.warps.clear();

        const ASYMPTOMATIC: u8 = 3;
        const SYMPTOMATIC: u8 = 4;
        let mut n_symptomatic = 0;
        for (i, t) in cats.iter_mut().enumerate() {
            let a = &self.agents[i];
            let mut ap = a.lock().unwrap();
            ap.reset(&self.world_params, &self.runtime_params, i, i < n_dist);
            match *t {
                SUSCEPTIBLE => {
                    ap.force_unfortified();
                }
                INFECTED => {
                    if ap.force_infected() {
                        n_symptomatic += 1;
                        *t = SYMPTOMATIC;
                    } else {
                        *t = ASYMPTOMATIC;
                    }
                }
                RECOVERED => {
                    ap.force_recovered(&self.runtime_params);
                }
                _ => {}
            }
        }

        let mut n_q_symptomatic =
            (n_symptomatic as f64 * self.world_params.q_symptomatic.r()) as u64;
        let mut n_q_asymptomatic =
            ((n_infected - n_symptomatic) as f64 * self.world_params.q_asymptomatic.r()) as u64;
        for (i, t) in cats.into_iter().enumerate() {
            let a = Arc::clone(&self.agents[i]);
            match t {
                SYMPTOMATIC if n_q_symptomatic > 0 => {
                    n_q_symptomatic -= 1;
                    self.hospital.add(a);
                    continue;
                }
                ASYMPTOMATIC if n_q_asymptomatic > 0 => {
                    n_q_asymptomatic -= 1;
                    self.hospital.add(a);
                    continue;
                }
                _ => {}
            }

            let idx = self
                .world_params
                .into_grid_index(&a.lock().unwrap().get_pt());
            self.field.add(a, idx);
        }

        // reset test queue
        self.runtime_params.step = 0;
        // self.stat_info.reset(n_pop);
        self.step_log.reset(
            n_pop - n_infected,
            n_symptomatic,
            n_infected - n_symptomatic,
        );
        self.scenario_index = 0;
        // self.exec_scenario();
        // [self forAllReporters:^(PeriodicReporter *rep) { [rep reset]; }];

        self.loop_mode = LoopMode::LoopNone;
    }

    fn exec_scenario(&mut self) {
        todo!("execute scenario");
    }

    // fn deliver_test_results(&mut self) -> EnumMap<TestType, usize> {
    //     // for i in TestType::TestAsSymptom..TestType::TestPositive {
    //     //     //     testCount[TestTotal] += testCount[i];
    //     // }
    //     let mut test_count = self.check_test_results();
    //     self.add_new_test(&mut test_count);
    //     test_count
    // }

    fn go_ahead(&mut self) {
        if self.loop_mode == LoopMode::LoopFinished {
            self.reset();
        } else if self.loop_mode == LoopMode::LoopEndByCondition {
            self.exec_scenario();
        }
    }
    /*
    fn touch(&mut self) -> bool {
        todo!()
        //     let mut result;
        //     result = self.docKey != nil;
        //     if ()) _lastTouch = NSDate.date;
        //     [_lastTLock unlock];
        //     return result;
    }
    */

    fn do_one_step(&mut self) {
        // let mesh = self.world_params.mesh as usize;
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

        // let n_infected = 0;
        // let (tx, rx) = mpsc::sync_channel(self.field.count() - n_infected);
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
            &pfs,
        );

        // let mut stat_info = StatInfo::new();
        /*
        let finished = {
            stat_info.calc_stat_with_test(prms.wp, prms.rp, &mut step_log)
        };
        */

        self.runtime_params.step += 1;
        if self.loop_mode == LoopMode::LoopRunning {
            if self.step_log.n_infected() == 0 {
                self.loop_mode = LoopMode::LoopFinished;
                // } else if self.predicate_to_stop {
                //     self.loop_mode = LoopMode::LoopEndByCondition;
            }
        }
    }

    fn running(&mut self) -> bool {
        if self.loop_mode != LoopMode::LoopRunning {
            return false;
        }
        self.do_one_step();
        if self.loop_mode == LoopMode::LoopEndByCondition
            && self.scenario_index < self.scenario.len() as i32
        {
            self.exec_scenario();
            self.loop_mode = LoopMode::LoopRunning;
        }
        if let Some(stop_at) = self.stop_at_n_days {
            if self.runtime_params.step >= stop_at * self.world_params.steps_per_day - 1 {
                self.loop_mode = LoopMode::LoopEndAsDaysPassed;
                return false;
            }
        }

        // [self forAllReporters:^(PeriodicReporter *rep) { [rep sendReportPeriodic]; }];
        // ignore to sleep
        /*
        if self.loop_mode != LoopMode::LoopEndByUser {
            self.touch();
        }
        */
        // [self forAllReporters:^(PeriodicReporter *rep) { [rep pause]; }];

        let new_time = get_uptime();
        let time_passed = new_time - self.prev_time;
        if time_passed < 1.0 {
            self.steps_per_sec += ((1.0 / time_passed).min(30.0) - self.steps_per_sec) * 0.2;
        }
        self.prev_time = new_time;
        true
    }

    pub fn start(&mut self, stop_at: u64) -> bool {
        if self.loop_mode == LoopMode::LoopRunning {
            return false;
        }
        if stop_at > 0 {
            self.stop_at_n_days = Some(stop_at);
        }
        // world.max_sps = max_sps;
        self.go_ahead();
        self.loop_mode = LoopMode::LoopRunning;
        true
    }

    pub fn step(&mut self) {
        match self.loop_mode {
            LoopMode::LoopRunning => return,
            LoopMode::LoopFinished | LoopMode::LoopEndByCondition => {
                self.go_ahead();
            }
            _ => {}
        }
        self.do_one_step();
        self.loop_mode = LoopMode::LoopEndByUser;
    }

    pub fn stop(&mut self) {
        if self.loop_mode == LoopMode::LoopRunning {
            self.loop_mode = LoopMode::LoopEndByUser;
        }
    }

    pub fn debug(&self) {
        // self.stat_info.debug_show();
        self.step_log.show_log();
    }

    pub fn export(&self, path: &str) -> Result<(), std::io::Error> {
        self.step_log.write(path)?;
        // self.stat_info.write_statistics(&mut wtr)?;
        Ok(())
    }
}

fn running_loop(wr: Arc<Mutex<World>>) {
    loop {
        let world = &mut wr.lock().unwrap();
        if !world.running() {
            break;
        }
    }
}

pub fn start_world(wr: Arc<Mutex<World>>, stop_at: u64) -> Option<thread::JoinHandle<()>> {
    if wr.lock().unwrap().start(stop_at) {
        Some(thread::spawn(move || {
            running_loop(Arc::clone(&wr));
        }))
    } else {
        None
    }
}

/*
- (void)startTimeLimitTimer {
    runtimeTimer = [NSTimer scheduledTimerWithTimeInterval:maxRuntime repeats:NO
        block:^(NSTimer * _Nonnull timer) { [self stop:LoopEndByTimeLimit]; }];
}
*/

fn new_unique_string() -> String {
    rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(7)
        .map(char::from)
        .collect()
}

fn get_uptime() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime before UNIX EPOCH!")
        .as_secs_f64()
}
