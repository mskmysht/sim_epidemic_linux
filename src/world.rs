use crate::agent::*;
use crate::common_types::*;
use crate::contact::*;
use crate::gathering::*;
use crate::iter::MyIter;
use crate::stat::*;

use rand::{self, Rng};
use rayon::prelude::*;
use std::collections::{HashMap, LinkedList};
use std::f64;
use std::fmt::*;
use std::sync::MutexGuard;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Default, Debug)]
pub struct WarpInfo {
    agent: MRef<Agent>,
    goal: Point,
    mode: WarpType,
}

impl WarpInfo {
    pub fn new(ar: MRef<Agent>, p: Point, md: WarpType) -> WarpInfo {
        WarpInfo {
            agent: ar,
            goal: p,
            mode: md,
        }
    }
}

// type Operation = Box<dyn FnMut() + Sync + Send>;

#[derive(Default)]
pub struct World {
    value: i32,
    running: bool,
    loop_mode: LoopMode,
    runtime_params: RuntimeParams,
    world_params: WorldParams,
    n_mesh: i32,
    pub agents: Mutex<Vec<MRef<Agent>>>,
    _pop: Mutex<Vec<Option<MRef<Agent>>>>, // length == n_mesh * n_mesh
    pub pop: Mutex<Vec<Option<MRef<Agent>>>>,
    n_pop: i32,
    warp_list: Vec<WarpInfo>,
    new_warp_f: HashMap<i32, Arc<WarpInfo>>,
    testees: HashMap<i32, TestType>,
    q_list: Option<MRef<Agent>>, // LinkedList<u32>,
    c_list: LinkedList<u32>,
    stat_info: StatInfo,
    scenario_index: i32,
    p_range: Vec<Range>,
    // n_cores: i32,
    // operations: Vec<Operation>,
    contract_state: Mutex<ContactState>,
    gatherings: Vec<MRef<Gathering>>,
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
        self.gatherings.clear();

        // reset contact info heaps
        {
            let cs = &mut self.contract_state.lock().unwrap();
            for i in 0..self.n_pop as usize {
                let agents = self.agents.lock().unwrap();
                let a = &mut agents[i].lock().unwrap();
                let ah = a.contact_info_head.clone();
                let at = a.contact_info_tail.clone();
                if let (Some(hr), Some(tr)) = (ah, at) {
                    let t = &mut tr.lock().unwrap();
                    t.next = cs.free_cinfo.clone();
                    cs.free_cinfo = Some(hr.clone());
                    a.contact_info_head = None;
                    a.contact_info_tail = None;
                }
            }
        }

        if self.n_mesh != wp.mesh {
            self.n_mesh = wp.mesh;
            // let n_cells = (wp.mesh * wp.mesh) as usize;
            let n_cnew = (wp.mesh * wp.mesh) as usize;
            self.p_range = vec![Range::default(); n_cnew];
        }
        self.gathering_map.clear();
        self._pop = Mutex::new(vec![None; (self.n_mesh * self.n_mesh) as usize]);
        if self.n_pop != wp.init_pop {
            self.n_pop = wp.init_pop;
            self.pop = Mutex::new(vec![None; self.n_pop as usize]);
            self.agents
                .lock()
                .unwrap()
                .resize_with(self.n_pop as usize, Default::default);
        }

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
            let agents = self.agents.lock().unwrap();
            let ar = &agents[i as usize];
            let a = &mut ar.lock().unwrap();
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
            add_agent(ar, &mut self._pop.lock().unwrap(), &wp);
        }
        self.q_list = None; //.clear();
        self.c_list.clear();
        self.warp_list.clear();
        // reset test queue
        self.runtime_params.step = 0;
        self.stat_info.reset(self.n_pop, wp.n_init_infec);
        self.scenario_index = 0;
        self.exec_scenario();
        // reset all periodic reporters
        self.loop_mode = LoopMode::LoopNone;
    }

    /*
    fn add_operation(&mut self, op: Operation) {
        self.operations.push(op);
    }
    */

    pub fn add_new_warp(&mut self, info: Arc<WarpInfo>) {
        let a = info.agent.lock().unwrap();
        let id = &a.id;
        if self.new_warp_f.contains_key(id) {
            self.new_warp_f.insert(*id, info.clone());
        }
    }

    pub fn test_infection_of_agent(&mut self, agent: &Agent, reason: TestType) {
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

    fn exec_scenario(&mut self) {
        todo!();
    }

    pub fn running_loop(self_: MRef<World>) {
        let world = self_.lock().unwrap();
        loop {
            match world.loop_mode {
                LoopMode::LoopRunning => break,
                _ => World::do_one_step(self_.clone()),
            }
        }
    }

    pub fn do_one_step(self_: MRef<World>) {
        let n_in_field = {
            let world = &mut self_.lock().unwrap();
            world.prepare_step()
        };

        // step
        let infectors = Mutex::new(vec![]);
        (0..n_in_field).into_par_iter().for_each(|i| {
            let infectors = &mut infectors.lock().unwrap();
            let world = &mut self_.lock().unwrap();
            let cw = self_.clone();
            let cw = cw.lock().unwrap();
            let pop = cw.pop.lock().unwrap();
            if let Some(ar) = &pop[i] {
                world.step_agent(ar);
                let a = &mut ar.lock().unwrap();
                if a.new_n_infects > 0 {
                    infectors.push(InfectionCntInfo {
                        org_v: a.n_infects,
                        new_v: a.n_infects + a.new_n_infects,
                    });
                    a.n_infects += a.new_n_infects;
                    a.new_n_infects = 0;
                }
            }
        });
        {
            let world = &mut self_.lock().unwrap();
            for ar in MyIter::new(world.q_list.clone()) {
                world.step_agent_in_quarantine(&ar);
            }
            // }
            // {
            //     let world = &mut self_.lock().unwrap();
            let cwd = self_.clone();
            for info in world.new_warp_f.values() {
                let ar = &info.agent;
                let a = &mut ar.lock().unwrap();
                let wd = &mut cwd.lock().unwrap();
                if a.is_warping {
                    wd.warp_list.retain(|w| {
                        let wa = &w.agent.lock().unwrap();
                        !std::ptr::eq(&**wa, &**a)
                    });
                } else {
                    a.is_warping = true;
                    match info.mode {
                        WarpType::WarpInside
                        | WarpType::WarpToHospital
                        | WarpType::WarpToCemeteryF => {
                            remove_agent(ar, &mut wd._pop.lock().unwrap(), &wd.world_params);
                        }
                        WarpType::WarpBack | WarpType::WarpToCemeteryH => {
                            remove_from_list(ar, &mut wd.q_list);
                        }
                    }
                }
            }
            for wr in world.new_warp_f.values() {
                // world.warp_list.push(wr);
            }
            world.new_warp_f.clear();
            {
                let cwd = self_.clone();
                world.warp_list.retain(|info| {
                    let wd = &mut cwd.lock().unwrap();
                    wd.warp_step(&info.agent, info.mode, info.goal)
                });
            }
        }
        todo!();
        /*
            NSUInteger testCount[NIntTestTypes];
            memset(testCount, 0, sizeof(testCount));
            [self deliverTestResults:testCount];

        //	BOOL finished = [statInfo calcStat:_Pop nCells:nCells
        //		qlist:_QList clist:_CList warp:_WarpList
        //		testCount:testCount stepsPerDay:worldParams.stepsPerDay];
            BOOL finished = [statInfo calcStatWithTestCount:testCount infects:
                [NSArray arrayWithObjects:infectors count:nCores]];
            [popLock unlock];
            runtimeParams.step ++;
            if (loopMode == LoopRunning) {
                if (finished) loopMode = LoopFinished;
                else if ([predicateToStop evaluateWithObject:statInfo])
                    loopMode = LoopEndByCondition;
            }
        */
    }

    fn warp_step(&mut self, ar: &MRef<Agent>, mode: WarpType, goal: Point) -> bool {
        todo!();
    }

    fn prepare_step(&mut self) -> usize {
        let n_cells = (self.world_params.mesh * self.world_params.mesh) as usize;
        self.p_range = Vec::with_capacity(n_cells);
        for _ in 0..n_cells {
            self.p_range.push(Range::default());
        }

        let n_in_field = {
            let mut _pop = self._pop.lock().unwrap();
            let mut n_in_field = 0;
            let pop = &mut self.pop.lock().unwrap();
            for i in 0..n_cells {
                self.p_range[i].location = n_in_field;
                for aa in MyIter::new(_pop[i].clone()) {
                    pop[n_in_field as usize] = Some(aa.clone());
                    n_in_field += 1;
                }
                self.p_range[i].length = n_in_field - self.p_range[i].location;
            }
            n_in_field
        };
        let old_time_stamp = self.runtime_params.step - self.world_params.steps_per_day * 14; // two weeks
        (0..n_in_field).into_par_iter().for_each(|i| {
            let pop = self.pop.lock().unwrap();
            if let Some(ar) = &pop[i as usize] {
                let a = &mut ar.lock().unwrap();
                a.reset_for_step();
                let mut cs = self.contract_state.lock().unwrap();
                cs.remove_old_cinfo(ar, old_time_stamp);
            }
        });

        manage_gatherings(
            &mut self.gatherings,
            &mut self.gathering_map,
            &self.world_params,
            &self.runtime_params,
        );

        self.gathering_map.par_iter().for_each(|(num, wrgs)| {
            let _pop = &mut self._pop.lock().unwrap();
            let opt_ar = &_pop[*num as usize];
            for aa in MyIter::new(opt_ar.clone()) {
                let a = &mut aa.lock().unwrap();
                if !a.is_infected() {
                    for amg in wrgs.iter() {
                        let g = amg.lock().unwrap();
                        g.affect_to_agent(a);
                    }
                }
            }
        });

        self.p_range.par_iter().for_each(|rng| {
            let _pop = self._pop.lock().unwrap();
            let len = rng.length as usize;
            let loc = rng.location as usize;
            for j in 0..len {
                if let Some(ar) = &_pop[(loc + j)] {
                    for k in (j + 1)..len {
                        if let Some(br) = &_pop[loc + k] {
                            Agent::interacts(
                                ar,
                                br,
                                &self.world_params,
                                &self.runtime_params,
                                &mut self.contract_state.lock().unwrap(),
                            );
                        }
                    }
                }
            }
        });
        let mesh = self.world_params.mesh as usize;
        self.iter_gtog(mesh, 1, 0, &|x, y| y * mesh + x, &|x, y| y * mesh + x - 1);
        self.iter_gtog(mesh, 2, 0, &|x, y| y * mesh + x, &|x, y| y * mesh + x - 1);
        self.iter_gtog(mesh, 1, 0, &|y, x| y * mesh + x, &|y, x| (y - 1) * mesh + x);
        self.iter_gtog(mesh, 2, 0, &|y, x| y * mesh + x, &|y, x| (y - 1) * mesh + x);
        self.iter_gtog(mesh, 1, 1, &|y, x| y * mesh + x, &|y, x| {
            (y - 1) * mesh + x - 1
        });
        self.iter_gtog(mesh, 2, 1, &|y, x| y * mesh + x, &|y, x| {
            (y - 1) * mesh + x - 1
        });
        self.iter_gtog(mesh, 1, 1, &|y, x| y * mesh + x - 1, &|y, x| {
            (y - 1) * mesh + x
        });
        self.iter_gtog(mesh, 2, 1, &|y, x| y * mesh + x - 1, &|y, x| {
            (y - 1) * mesh + x
        });
        n_in_field as usize
    }

    fn iter_gtog(
        &mut self,
        mesh: usize,
        a0: usize,
        b0: usize,
        ia: &(dyn Fn(usize, usize) -> usize + Sync),
        ib: &(dyn Fn(usize, usize) -> usize + Sync),
    ) {
        (a0..mesh).into_par_iter().step_by(2).for_each(|a| {
            (b0..mesh).into_par_iter().for_each(|b| {
                grid_to_grid_a(
                    &self.p_range,
                    &mut self.pop.lock().unwrap(),
                    &self.world_params,
                    &self.runtime_params,
                    &mut self.contract_state.lock().unwrap(),
                    ia(a, b),
                    ib(a, b),
                );
            });
        });
    }

    fn starts_warping(&mut self, ar: &MRef<Agent>, mode: WarpType, new_pt: Point) {
        self.add_new_warp(Arc::new(WarpInfo::new(ar.clone(), new_pt, mode)));
    }

    fn died(&mut self, ar: &MRef<Agent>, mode: WarpType) {
        let ws = self.world_params.world_size as f64;
        let mut a = ar.lock().unwrap();
        a.new_health = HealthType::Died;
        let mut rng = rand::thread_rng();
        self.starts_warping(
            ar,
            mode,
            Point {
                x: (rng.gen::<f64>() * 0.248 + 1.001) * ws,
                y: (rng.gen::<f64>() * 0.468 + 0.001) * ws,
            },
        );
    }

    fn patient_step(&mut self, ar: &MRef<Agent>, in_quarantine: bool) -> bool {
        let mut a = ar.lock().unwrap();
        if f64::MAX == a.days_to_die {
            // in the recovery phase
            if a.days_infected >= a.days_to_recover {
                if a.health == HealthType::Symptomatic {
                    cummulate_histgrm(&mut self.recov_p_hist, a.days_diseased);
                }
                a.new_health = HealthType::Recovered;
                a.days_infected = 0.;
            }
        } else if a.days_infected > a.days_to_recover {
            // starts recovery
            a.days_to_recover *= 1. + 10. / a.days_to_die;
            a.days_to_die = f64::MAX;
        } else if a.days_infected >= a.days_to_die {
            cummulate_histgrm(&mut self.death_p_hist, a.days_diseased);
            self.died(
                ar,
                if in_quarantine {
                    WarpType::WarpToCemeteryH
                } else {
                    WarpType::WarpToCemeteryF
                },
            );
            return true;
        } else if a.health == HealthType::Asymptomatic && a.days_infected >= a.days_to_onset {
            a.new_health = HealthType::Symptomatic;
            cummulate_histgrm(&mut self.incub_p_hist, a.days_infected);
        }
        return false;
    }

    fn step_agent(&mut self, ar: &MRef<Agent>) {
        let ws = self.world_params.world_size as f64;
        let spd = self.world_params.steps_per_day as f64;
        let mut a = ar.lock().unwrap();
        match a.health {
            HealthType::Asymptomatic => {
                a.days_infected += 1. / spd;
                if self.patient_step(ar, false) {
                    return;
                }
            }
            HealthType::Symptomatic => {
                a.days_infected += 1. / spd;
                a.days_diseased += 1. / spd;
                if self.patient_step(ar, false) {
                    return;
                } else if a.days_diseased >= self.runtime_params.tst_delay
                    && was_hit(spd, self.runtime_params.tst_sbj_sym / 100.)
                {
                    self.test_infection_of_agent(&a, TestType::TestAsSymptom);
                }
            }
            HealthType::Recovered => {
                a.days_infected += 1. / spd;
                if a.days_infected > a.im_expr {
                    a.new_health = HealthType::Susceptible;
                    a.days_infected = 0.;
                    a.days_diseased = 0.;
                    a.reset_days(&self.runtime_params);
                }
            }
            _ => {}
        }
        if a.health != HealthType::Symptomatic
            && was_hit(spd, self.runtime_params.tst_sbj_asy / 100.)
        {
            self.test_infection_of_agent(&a, TestType::TestAsSuspected);
        }
        let org_idx = a.index_in_pop(&self.world_params);
        if a.health != HealthType::Symptomatic && was_hit(spd, self.runtime_params.mob_fr / 1000.) {
            self.starts_warping(
                ar,
                WarpType::WarpInside,
                a.get_new_pt(ws, &self.runtime_params.mob_dist),
            );
        } else {
            a.update_position(&self.world_params, &self.runtime_params);
        }
        let new_idx = a.index_in_pop(&self.world_params);
        if new_idx != org_idx {
            let pop = &mut self.pop.lock().unwrap();
            remove_from_list(ar, &mut pop[org_idx as usize]);
            add_to_list(ar, &mut pop[new_idx as usize]);
        }
    }

    fn step_agent_in_quarantine(&mut self, ar: &MRef<Agent>) {
        let spd = self.world_params.steps_per_day as f64;
        let mut a = ar.lock().unwrap();
        match a.health {
            HealthType::Symptomatic => {
                a.days_diseased += 1. / spd;
            }
            HealthType::Asymptomatic => {
                a.days_infected += 1. / spd;
            }
            _ => {
                self.starts_warping(ar, WarpType::WarpBack, a.org_pt.clone());
                return;
            }
        }
        if !self.patient_step(ar, true) && a.health == HealthType::Recovered {
            self.starts_warping(ar, WarpType::WarpBack, a.org_pt.clone());
        }
    }
}

fn grid_to_grid_a(
    p_range: &Vec<Range>,
    pop: &mut MutexGuard<Vec<Option<MRef<Agent>>>>,
    wp: &WorldParams,
    rp: &RuntimeParams,
    cs: &mut MutexGuard<ContactState>,
    ia: usize,
    ib: usize,
) {
    let ar = &p_range[ia];
    let aloc = ar.location as usize;
    let br = &p_range[ib];
    let bloc = br.location as usize;

    for j in 0..ar.length as usize {
        for k in 0..br.length as usize {
            let pa = aloc + j;
            let pb = bloc + k;
            if pa == pb {
                panic!("{}-{} and {}-{} are a same object", ia, j, ib, k);
            }

            let (p, q) = if pa < pb { (pa, pb) } else { (pb, pa) };
            let (lpop, rpop) = pop.split_at(q);
            if let (Some(ap), Some(aq)) = (&lpop[p], rpop.last().unwrap()) {
                // let ap = &mut ap.lock().unwrap();
                // let aq = &mut aq.lock().unwrap();
                let (ar, br) = if pa < pb { (ap, aq) } else { (aq, ap) };
                Agent::interacts(ar, br, wp, rp, cs);
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
pub fn new_handle(world: MRef<World>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        World::running_loop(world);
        // world.lock().unwrap().start();
        // while world.lock().unwrap().running {
        //     thread::sleep(std::time::Duration::from_secs(1));
        //     world.lock().unwrap().up();
        // }
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
