use crate::agent::*;
use crate::common_types::*;
use crate::contract::*;
use crate::gathering::*;
use crate::stat::*;

use rand::{self, Rng};
use rayon::prelude::*;
use std::collections::{HashMap, LinkedList};
use std::fmt::*;
use std::sync::MutexGuard;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Default, Debug)]
pub struct WarpInfo {
    agent: Arc<Mutex<Agent>>,
    goal: Point,
    mode: WarpType,
}

impl WarpInfo {
    pub fn new(a: Arc<Mutex<Agent>>, p: Point, md: WarpType) -> WarpInfo {
        WarpInfo {
            agent: a,
            goal: p,
            mode: md,
        }
    }
}

type Operation = Box<dyn FnMut() + Sync + Send>;

#[derive(Default)]
pub struct World {
    value: i32,
    running: bool,
    loop_mode: LoopMode,
    runtime_params: RuntimeParams,
    world_params: WorldParams,
    n_mesh: i32,
    pub agents: Mutex<Vec<Agent>>,
    _pop: Mutex<Vec<Option<u32>>>, // length == n_mesh * n_mesh
    pub pop: Vec<Option<u32>>,
    n_pop: i32,
    warp_list: Vec<WarpInfo>,
    new_warp_f: HashMap<i32, Arc<WarpInfo>>,
    testees: HashMap<i32, TestType>,
    q_list: LinkedList<u32>,
    c_list: LinkedList<u32>,
    stat_info: StatInfo,
    scenario_index: i32,
    p_range: Vec<Range>,
    n_cores: i32,
    operations: Vec<Operation>,
    contract_state: Mutex<ContractState>,
    gatherings: Vec<Arc<Mutex<Gathering>>>,
    gathering_map: GatheringMap,
    pub recov_p_hist: Vec<MyCounter>,
    pub incub_p_hist: Vec<MyCounter>,
    pub death_p_hist: Vec<MyCounter>,
}

impl Display for World {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "value:{}/running:{}", self.value, self.running,)
    }
}

impl World {
    pub fn new() -> World {
        let mut w = World::default();
        w.reset_pop();
        todo!()
        // w
    }
    fn reset_pop(&mut self) {
        let mut rng = rand::thread_rng();
        let wp = self.world_params;
        // set runtime params of scenario != None
        if self.n_mesh != wp.mesh {
            self.n_mesh = wp.mesh;
            // gathering_maps = vec!.new();
        } // else { gathering_maps.remove_all(); }
          // self._pop = vec![];
        if self.n_pop != wp.init_pop {
            self.n_pop = wp.init_pop;
            self.agents
                .lock()
                .unwrap()
                .resize_with(self.n_pop as usize, Default::default);
        }
        let n_cells = (wp.mesh * wp.mesh) as usize;
        self._pop = Mutex::new(vec![None; n_cells]); // vec![Agent::default(); self.n_pop];
        self.pop = vec![None; self.n_pop as usize]; // vec![Agent::default(); self.n_pop];

        let n_dist = (self.runtime_params.dst_ob / 100. * (self.n_pop as f64)) as i32;
        let mut infec_idxs: Vec<i32> = Vec::with_capacity(wp.n_init_infec as usize);
        for i in 0..wp.n_init_infec {
            let mut k: i32 = ((self.n_pop - i - 1) as f64 * rng.gen::<f64>()) as i32;
            for &l in &infec_idxs {
                if l <= k {
                    k += 1;
                }
            }
            infec_idxs.push(k);
        }
        infec_idxs.sort();
        let mut i_idx = 0;
        for i in 0..self.n_pop {
            let mut agents = self.agents.lock().unwrap();
            let a = &mut agents[i as usize];
            a.reset(&wp);
            a.id = i;
            if i < n_dist {
                a.distancing = true
            }
            if i < wp.n_init_infec && i == infec_idxs[i_idx] {
                a.health = HealthType::Asymptomatic;
                a.n_infects = 0;
                i_idx += 1;
            }
            let k = a.index_in_pop(&wp) as usize;
            self.add_agent(&(i as u32), &k, &mut agents);
        }
        self.warp_list.clear();
        self.c_list.clear();
        self.q_list.clear();
        self.runtime_params.step = 0;
        self.stat_info.reset(self.n_pop, wp.n_init_infec);
        self.scenario_index = 0;
        self.exec_scenario();
        todo!();
    }

    fn add_agent(&self, i: &u32, k: &usize, agents: &mut MutexGuard<Vec<Agent>>) {
        // let mut agents = self.agents.lock().unwrap();
        let opt_i = Some(*i as u32);
        let opt_j = &mut self._pop.lock().unwrap()[*k];
        let a = &mut agents[*i as usize];
        a.next = *opt_j;
        a.prev = None;
        if let Some(ref j) = opt_j {
            let b = &mut agents[*j as usize];
            b.prev = opt_i;
        }
        *opt_j = opt_i;
    }

    pub fn add_to_pop(&mut self, idx: usize) {
        todo!()
    }

    pub fn remove_from_pop(&mut self, idx: usize) {
        let mut agents = self.agents.lock().unwrap();
        let op = &mut self.pop[idx];
        if let Some(i) = *op {
            let a = &agents[i as usize];
            let an = a.next;
            let ap = a.prev;
            if let Some(p) = ap {
                agents[p as usize].next = an;
            } else {
                *op = an;
            }
            if let Some(p) = an {
                agents[p as usize].prev = ap;
            }
        }
    }

    fn add_operation(&mut self, op: Operation) {
        self.operations.push(op);
    }
    pub fn add_new_warp(&mut self, info: Arc<WarpInfo>) {
        let a = info.agent.lock().unwrap();
        let id = &a.id;
        if self.new_warp_f.contains_key(id) {
            self.new_warp_f.insert(*id, info.clone());
        }
    }

    pub fn test_infection_of_agent(&mut self, agent: &MutexGuard<Agent>, reason: TestType) {
        let ds = (self.runtime_params.step - agent.last_tested) as f64;
        if ds < self.runtime_params.tst_interval * self.world_params.steps_per_day as f64
            || agent.is_out_of_field
            || agent.in_test_queue
        {
            return;
        }
        if let Some(tt) = self.testees.get_mut(&agent.id) {
            *tt = reason;
        }
    }
    fn exec_scenario(&mut self) {}
    fn up(&mut self) {
        self.value += 1;
    }
    fn start(&mut self) {
        self.running = true;
    }
    pub fn stop(&mut self) {
        self.running = false;
    }
    fn running_loop(&mut self) {
        loop {
            match self.loop_mode {
                LoopMode::LoopRunning => break,
                _ => self.do_one_step(),
            }
        }
    }
    fn do_one_step(&mut self) {
        let n_cells = (self.world_params.mesh * self.world_params.mesh) as usize;
        self.p_range = Vec::with_capacity(n_cells);
        for _ in 0..n_cells {
            self.p_range.push(Range::default());
        }

        let _pop = self._pop.lock().unwrap();

        let n_in_field = {
            let agents = self.agents.lock().unwrap();
            let mut n_in_field = 0;
            for i in 0..n_cells {
                self.p_range[i].location = n_in_field;
                let mut opt_j = &_pop[i];
                loop {
                    match *opt_j {
                        Some(j) => {
                            self.pop[n_in_field as usize] = *opt_j;
                            n_in_field += 1;
                            opt_j = &agents[j as usize].next;
                        }
                        None => break,
                    };
                }
                self.p_range[i].length = n_in_field - self.p_range[i].location;
            }
            n_in_field
        };
        let old_time_stamp = self.runtime_params.step - self.world_params.steps_per_day * 14; // two weeks
        (0..n_in_field).into_par_iter().for_each(|i| {
            if let Some(k) = self.pop[i as usize] {
                let mut agents = self.agents.lock().unwrap();
                let a = &mut agents[k as usize];
                a.reset_for_step();
                let mut cs = self.contract_state.lock().unwrap();
                cs.remove_old_cinfo(a, old_time_stamp);
            }
        });

        manage_gatherings(
            &mut self.gatherings,
            &mut self.gathering_map,
            &self.world_params,
            &self.runtime_params,
        );

        self.gathering_map.par_iter().for_each(|(num, wrgs)| {
            let mut op = _pop[*num as usize];
            loop {
                match op {
                    Some(p) => {
                        let mut agents = self.agents.lock().unwrap();
                        let a = &mut agents[p as usize];
                        op = a.next;
                        if !a.is_infected() {
                            for amg in wrgs.iter() {
                                let g = amg.lock().unwrap();
                                g.affect_to_agent(a);
                            }
                        }
                    }
                    None => {
                        break;
                    }
                }
            }
        });

        self.p_range.par_iter().for_each(|rng| {
            if let Some(i) = _pop[rng.location as usize] {
                let i = i as usize;
                let l = rng.length as usize;
                let mut agents = self.agents.lock().unwrap();
                let part = &mut agents[i..i + l];
                for j in 1..l {
                    let (las, ras) = part.split_at_mut(j);
                    let a = las.last_mut().unwrap();
                    for b in ras {
                        a.interacts(b, &self.world_params, &self.runtime_params);
                    }
                }
            }
        });
        let mesh = self.world_params.mesh as usize;
        let f = |a0,
                 b0,
                 ia: &(dyn Fn(usize, usize) -> usize + Sync),
                 ib: &(dyn Fn(usize, usize) -> usize + Sync)| {
            (a0..mesh).into_par_iter().step_by(2).for_each(|a| {
                (b0..mesh).into_par_iter().for_each(|b| {
                    grid_to_grid_a(
                        &self.agents,
                        &self.p_range,
                        &self.pop,
                        &self.world_params,
                        &self.runtime_params,
                        ia(a, b),
                        ib(a, b),
                    );
                });
            });
        };
        f(1, 0, &|x, y| y * mesh + x, &|x, y| y * mesh + x - 1);
        f(2, 0, &|x, y| y * mesh + x, &|x, y| y * mesh + x - 1);
        f(1, 0, &|y, x| y * mesh + x, &|y, x| (y - 1) * mesh + x);
        f(2, 0, &|y, x| y * mesh + x, &|y, x| (y - 1) * mesh + x);
        f(1, 1, &|y, x| y * mesh + x, &|y, x| (y - 1) * mesh + x - 1);
        f(2, 1, &|y, x| y * mesh + x, &|y, x| (y - 1) * mesh + x - 1);
        f(1, 1, &|y, x| y * mesh + x - 1, &|y, x| (y - 1) * mesh + x);
        f(2, 1, &|y, x| y * mesh + x - 1, &|y, x| (y - 1) * mesh + x);

        // step
        (0..n_in_field as usize).into_par_iter().for_each(|i| {
            let mut agents = self.agents.lock().unwrap();
            if let Some(p) = self.pop[i] {
                let a = &mut agents[p as usize];
                Agent::step_agent(
                    Arc::new(Mutex::new(*a)),
                    &self.runtime_params,
                    &self.world_params,
                    self,
                );
            }
        });
    }
}

fn grid_to_grid_a(
    agents: &Mutex<Vec<Agent>>,
    p_range: &Vec<Range>,
    pop: &Vec<Option<u32>>,
    wp: &WorldParams,
    rp: &RuntimeParams,
    ia: usize,
    ib: usize,
) {
    let mut agents = agents.lock().unwrap();
    let ar = &p_range[ia];
    let br = &p_range[ib];
    for j in 0..ar.length as usize {
        for k in 0..br.length as usize {
            if let (Some(p), Some(q)) = (pop[j], pop[k]) {
                if p == q {
                    continue;
                }
                let (p, q) = if p > q { (q, p) } else { (p, q) };
                let part = &mut agents[0..1 + q as usize];
                let (las, ras) = part.split_at_mut(1 + p as usize);
                let aa = las.last_mut().unwrap();
                let ab = ras.last_mut().unwrap();
                aa.interacts(ab, wp, rp);
            }
        }
    }
}
/*
- (void)startTimeLimitTimer {
    runtimeTimer = [NSTimer scheduledTimerWithTimeInterval:maxRuntime repeats:NO
        block:^(NSTimer * _Nonnull timer) { [self stop:LoopEndByTimeLimit]; }];
}
*/
pub fn new_handle(world: Arc<Mutex<World>>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        world.lock().unwrap().start();
        while world.lock().unwrap().running {
            thread::sleep(std::time::Duration::from_secs(1));
            world.lock().unwrap().up();
        }
    })
}

/*
- (void)start:(NSInteger)stopAt maxSPS:(CGFloat)maxSps priority:(CGFloat)prio {
    if (loopMode == LoopRunning) return;
    if (stopAt > 0) stopAtNDays = stopAt;
    maxSPS = maxSps;
    [self goAhead];
    loopMode = LoopRunning;
    NSThread *thread = [NSThread.alloc initWithTarget:self
        selector:@selector(runningLoop) object:nil];
    thread.threadPriority = fmax(0., NSThread.mainThread.threadPriority + prio);
    [thread start];
}
*/

/*
- (void)runningLoop {
#ifdef NOGUI
    in_main_thread(^{ [self startTimeLimitTimer]; });
    [self forAllReporters:^(PeriodicReporter *rep) { [rep start]; }];
#endif
    while (loopMode == LoopRunning) {
        [self doOneStep];
        if (loopMode == LoopEndByCondition && scenarioIndex < scenario.count) {
            [self execScenario];
            loopMode = LoopRunning;
        }
        if (stopAtNDays > 0 && runtimeParams.step
            == stopAtNDays * worldParams.stepsPerDay - 1) {
            loopMode = LoopEndAsDaysPassed;
            break;
        }
        CGFloat newTime = get_uptime(), timePassed = newTime - prevTime;
        if (timePassed < 1.)
            stepsPerSec += (fmin(30., 1. / timePassed) - stepsPerSec) * 0.2;
        prevTime = newTime;
#ifdef NOGUI
//		if (runtimeParams.step % 100 == 0) NSLog(@"%ld", runtimeParams.step);
        [self forAllReporters:^(PeriodicReporter *rep) { [rep sendReportPeriodic]; }];
        if (maxSPS > 0) {
            NSInteger usToWait = (1./maxSPS - timePassed) * 1e6;
#else
        if (runtimeParams.step % animeSteps == 0) {
            in_main_thread(^{ [self showAllAfterStep]; });
            NSInteger usToWait = (1./30. - timePassed) * 1e6;
#endif
            usleep((uint32)((usToWait < 0)? 1 : usToWait));
        } else usleep(1);
    }
#ifdef NOGUI
//NSLog(@"runningLoop will stop %d.", loopMode);
    in_main_thread(^{ [self stopTimeLimitTimer]; });
    if (loopMode != LoopEndByUser) [self touch];
    [self forAllReporters:^(PeriodicReporter *rep) { [rep pause]; }];
    if (_stopCallBack != nil) _stopCallBack(loopMode);
#else
    in_main_thread(^{
        self->view.needsDisplay = YES;
        self->startBtn.title = NSLocalizedString(@"Start", nil);
        self->stepBtn.enabled = YES;
        [self->scenarioPanel adjustControls:NO];
    });
#endif
}
*/
