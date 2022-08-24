use crate::commons::HealthType;
use crate::commons::{math::Point, LoopMode, RuntimeParams, WorldParams};
use crate::gathering::Gatherings;
use crate::log::StepLog;
use crate::testing::TestQueue;
use crate::{
    agent::{
        location::{Cemetery, Field, Hospital, Warps},
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

pub struct World {
    pub id: String,
    loop_mode: LoopMode,
    runtime_params: RuntimeParams,
    world_params: WorldParams,
    agents: Vec<Agent>,
    field: Field,
    warps: Warps,
    hospital: Hospital,
    cemetery: Cemetery,
    test_queue: TestQueue,
    prev_time: f64,
    steps_per_sec: f64,
    stop_at_n_days: Option<u64>,
    step_log: StepLog,
    scenario_index: i32,
    scenario: Vec<i32>, //[todo] Vec<Scenario>
    gatherings: Gatherings,
    gat_spots_fixed: Vec<Point>,
    //[todo] n_mesh: usize,
    //[todo] n_pop: usize,
    //[todo] predicate_to_stop: bool,
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
            agents: Vec::with_capacity(world_params.init_n_pop),
            prev_time: 0.0,
            steps_per_sec: 0.0,
            stop_at_n_days: None,
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

        dbg!(
            self.world_params.init_n_pop(),
            self.world_params.infected.r(),
            n_infected
        );

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

        self.loop_mode = LoopMode::LoopNone;
    }

    fn exec_scenario(&mut self) {
        todo!("execute scenario");
    }

    fn go_ahead(&mut self) {
        if self.loop_mode == LoopMode::LoopFinished {
            self.reset();
        } else if self.loop_mode == LoopMode::LoopEndByCondition {
            self.exec_scenario();
        }
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

        let is_finished = self.step_log.push();
        self.runtime_params.step += 1;
        if self.loop_mode == LoopMode::LoopRunning {
            if is_finished {
                self.loop_mode = LoopMode::LoopFinished;
                //[todo] } else if self.predicate_to_stop {
                //[todo]     self.loop_mode = LoopMode::LoopEndByCondition;
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
        //[todo] world.max_sps = max_sps;
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
        self.step_log.show_log();
    }

    pub fn export(&self, path: &str) -> Result<(), std::io::Error> {
        self.step_log.write(path)?;
        Ok(())
    }
}

fn running_loop(wr: Arc<Mutex<World>>) {
    loop {
        if !wr.lock().unwrap().running() {
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
