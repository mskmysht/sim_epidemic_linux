use crate::commons::*;
use crate::contact::*;
use crate::enum_map::*;
use crate::gathering::*;
use std::{error::Error, fs::File};

use crate::stat::*;
use crate::{agent::*, dyn_struct::DynStruct};

use csv::Writer;
use rand::distributions::Alphanumeric;
use rand::{self, Rng};
use rayon::prelude::*;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::thread;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use std::{collections::VecDeque, f64};

#[derive(Default, Debug)]
pub struct WarpInfo {
    pub agent: MRef<Agent>,
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

pub struct World {
    pub id: String,
    loop_mode: LoopMode,
    pub runtime_params: RuntimeParams,
    pub world_params: WorldParams,
    pub tmp_world_params: WorldParams,
    n_mesh: i32,
    pub agents: Mutex<Vec<MRef<Agent>>>,
    pub _pop: Mutex<Vec<VecDeque<MRef<Agent>>>>,
    pop: Vec<Option<MRef<Agent>>>,
    n_pop: i32,
    p_range: Vec<Range>,
    prev_time: f64,
    steps_per_sec: f64,
    pub warp_list: Vec<Arc<WarpInfo>>,
    new_warp_f: HashMap<i32, Arc<WarpInfo>>,
    testees: HashMap<i32, TestType>,
    stop_at_n_days: i32,
    pub q_list: VecDeque<MRef<Agent>>,
    pub c_list: VecDeque<MRef<Agent>>,
    stat_info: Mutex<StatInfo>,
    scenario_index: i32,
    scenario: Vec<i32>, // Vec<Scenario>
    dsc: Mutex<DynStruct<ContactInfo>>,
    gatherings: Vec<MRef<Gathering>>,
    gathering_map: GatheringMap,
    pub recov_p_hist: Vec<MyCounter>,
    pub incub_p_hist: Vec<MyCounter>,
    pub death_p_hist: Vec<MyCounter>,
    predicate_to_stop: bool,
    test_que: VecDeque<MRef<TestEntry>>,
    dst: DynStruct<TestEntry>,
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
    pub fn new(
        user_default_runtime_params: RuntimeParams,
        user_default_world_params: WorldParams,
    ) -> World {
        let mut w = World {
            id: new_unique_string(),
            loop_mode: Default::default(),
            runtime_params: user_default_runtime_params,
            world_params: user_default_world_params,
            tmp_world_params: user_default_world_params,
            n_mesh: 0,
            agents: Default::default(),
            _pop: Mutex::new(vec![]),
            pop: vec![],
            n_pop: 0,
            p_range: vec![],
            prev_time: 0.0,
            steps_per_sec: 0.0,
            warp_list: vec![],
            new_warp_f: HashMap::new(),
            testees: HashMap::new(),
            stop_at_n_days: -365,
            q_list: Default::default(),
            c_list: Default::default(),
            stat_info: Mutex::new(StatInfo::new()),
            scenario_index: 0,
            scenario: vec![],
            dsc: Default::default(),
            gatherings: vec![],
            gathering_map: HashMap::new(),
            recov_p_hist: vec![],
            incub_p_hist: vec![],
            death_p_hist: vec![],
            predicate_to_stop: false,
            test_que: Default::default(),
            dst: Default::default(),
        };

        w.reset_pop();
        w
    }

    // fn running(&self) -> bool {
    //     self.loop_mode == LoopMode::LoopRunning
    // }
    pub fn reset_pop(&mut self) {
        let mut rng = rand::thread_rng();
        let wp = &self.world_params;
        let rp = &self.runtime_params;

        // set runtime params of scenario != None
        self.gatherings.clear();

        // reset contact info heaps
        {
            let dsc = &mut self.dsc.lock().unwrap();
            for i in 0..self.n_pop as usize {
                let agents = self.agents.lock().unwrap();
                let a = &mut agents[i].lock().unwrap();
                dsc.restore_all(&mut a.contact_info_list);
                a.contact_info_list = Default::default();
            }
        }

        if self.n_mesh != wp.mesh {
            self.n_mesh = wp.mesh;
            let n_cnew = (wp.mesh * wp.mesh) as usize;
            self.p_range = vec![Range::default(); n_cnew];
        }
        self.gathering_map.clear();
        {
            let _pop = &mut self._pop.lock().unwrap();
            _pop.clear();
            _pop.resize((self.n_mesh * self.n_mesh) as usize, Default::default());
        }
        if self.n_pop != wp.init_pop {
            self.n_pop = wp.init_pop;
            self.pop.clear();
            self.pop.resize(self.n_pop as usize, None);
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
            {
                let a = &mut ar.lock().unwrap();
                a.reset(wp.world_size as f64, &rp);
                a.id = i;
                if i < n_dist {
                    a.distancing = true
                }
                if i_idx < (wp.n_init_infec as usize) && i == infec_idxs[i_idx] {
                    a.health = HealthType::Asymptomatic;
                    a.n_infects = 0;
                    i_idx += 1;
                }
            }
            {
                let _pop = &mut self._pop.lock().unwrap();
                add_agent(ar.clone(), _pop, &wp);
            }
        }
        self.q_list.clear();
        self.c_list.clear();
        self.warp_list.clear();
        // reset test queue
        self.runtime_params.step = 0;
        {
            self.stat_info
                .lock()
                .unwrap()
                .reset(self.n_pop, wp.n_init_infec);
        }
        self.scenario_index = 0;
        // self.exec_scenario();
        // [self forAllReporters:^(PeriodicReporter *rep) { [rep reset]; }];

        self.loop_mode = LoopMode::LoopNone;
    }

    pub fn add_new_warp(&mut self, info: Arc<WarpInfo>) {
        let a = info.agent.lock().unwrap();
        self.new_warp_f.insert(a.id, info.clone());
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

    fn warp_steps(&mut self) {
        for info in self.new_warp_f.values() {
            let is_warping = info.agent.lock().unwrap().is_warping;
            if is_warping {
                self.warp_list
                    .retain(|wi| !Arc::ptr_eq(&info.agent, &wi.agent));
            } else {
                info.agent.lock().unwrap().is_warping = true;
                match info.mode {
                    WarpType::WarpInside | WarpType::WarpToHospital | WarpType::WarpToCemeteryF => {
                        remove_agent(
                            &info.agent,
                            &mut self._pop.lock().unwrap(),
                            &self.world_params,
                        );
                    }
                    WarpType::WarpBack | WarpType::WarpToCemeteryH => {
                        self.q_list.remove_p(&info.agent);
                    }
                }
            }
        }

        for info in self.new_warp_f.values() {
            let ar = &info.agent;
            let mode = info.mode;
            let goal = info.goal;

            let wp = &self.world_params;
            let dp = {
                let a = &mut ar.lock().unwrap();
                Point {
                    x: goal.x - a.x,
                    y: goal.y - a.y,
                }
            };

            let d = dp.y.hypot(dp.x);
            let v = wp.world_size as f64 / 5. / wp.steps_per_day as f64;
            if d < v {
                {
                    let a = &mut ar.lock().unwrap();
                    a.x = goal.x;
                    a.y = goal.y;
                    a.is_warping = false;
                }
                match mode {
                    WarpType::WarpInside | WarpType::WarpBack => {
                        add_agent(ar.clone(), &mut self._pop.lock().unwrap(), wp)
                    }
                    WarpType::WarpToHospital => {
                        self.q_list.push_front(ar.clone());
                        ar.lock().unwrap().got_at_hospital = true;
                    }
                    WarpType::WarpToCemeteryF | WarpType::WarpToCemeteryH => {
                        self.c_list.push_front(ar.clone());
                    }
                }
            // true
            } else {
                let a = &mut ar.lock().unwrap();
                let th = dp.y.atan2(dp.x);
                a.x += v * th.cos();
                a.y += v * th.sin();
                // false
                self.warp_list.push(info.clone());
            }
        }
        self.new_warp_f.clear();
    }

    fn manage_gatherings(&mut self) {
        let gatherings = &mut self.gatherings;
        let gat_map = &mut self.gathering_map;
        let wp = &self.world_params;
        let rp = &self.runtime_params;

        gatherings.retain(|gr| {
            Gathering::remove_from_map(gr, gat_map);
            !gr.lock().unwrap().step(wp.steps_per_day)
        });
        //	caliculate the numner of gathering circles
        //	using random number in exponetial distribution.
        let mut rng = rand::thread_rng();
        let n_new_gat =
            (rp.gat_fr / wp.steps_per_day as f64 * (wp.world_size * wp.world_size) as f64 / 1e5
                * (-(rng.gen::<f64>() * 0.9999 + 0.0001).ln()))
            .round() as i32;
        for _ in 0..n_new_gat {
            gatherings.push(Gathering::new(gat_map, wp, rp));
        }
    }

    fn grid_to_grid_a(&mut self, ia: usize, ib: usize) {
        let p_range = &self.p_range;
        let pop = &self.pop;
        let wp = &self.world_params;
        let rp = &self.runtime_params;
        let dsc = &mut self.dsc.lock().unwrap();

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
                    let (ar, br) = if pa < pb { (ap, aq) } else { (aq, ap) };
                    Agent::interacts(ar.clone(), br.clone(), wp, rp, dsc);
                }
            }
        }
    }

    fn deliver_test_results(&mut self, test_count: &mut EnumMap<TestType, u32>) {
        let mut rng = rand::thread_rng();
        // check the results of tests
        let c_tm = (self.runtime_params.step as f64
            - (self.runtime_params.tst_proc * self.world_params.steps_per_day as f64))
            as i32;

        let mut old_list = VecDeque::new();
        while !self.test_que.is_empty() {
            let er = self.test_que.pop_front().unwrap();
            let entry = &mut er.lock().unwrap();
            if entry.time_stamp > c_tm {
                self.test_que.push_front(er.clone());
                break;
            }
            if entry.is_positive {
                let entry = &mut er.lock().unwrap();
                test_count[TestType::TestPositive] += 1;
                if let Some(ar) = &entry.agent {
                    let a = &mut ar.lock().unwrap();
                    a.org_pt = Point { x: a.x, y: a.y };
                    let new_pt = Point {
                        x: (rng.gen::<f64>() * 0.248 + 1.001) * self.world_params.world_size as f64,
                        y: (rng.gen::<f64>() * 0.458 + 0.501) * self.world_params.world_size as f64,
                    };
                    self.add_new_warp(Arc::new(WarpInfo::new(
                        ar.clone(),
                        new_pt,
                        WarpType::WarpToHospital,
                    )));
                    for cr in &a.contact_info_list {
                        let c = cr.lock().unwrap();
                        self.test_infection_of_agent(
                            &c.agent.lock().unwrap(),
                            TestType::TestAsContact,
                        );
                    }
                    let dsc = &mut self.dsc.lock().unwrap();
                    dsc.restore_all(&mut a.contact_info_list);
                    a.contact_info_list = Default::default();
                }
            } else {
                test_count[TestType::TestNegative] += 1;
            }

            if let Some(ar) = &entry.agent {
                ar.lock().unwrap().in_test_queue = false;
            }

            old_list.push_back(er.clone());
        }
        self.dst.restore_all(&mut old_list);

        // enqueue new tests
        for (&num, &v) in &self.testees {
            test_count[v] += 1;
            let ar = &self.agents.lock().unwrap()[num as usize];
            let er = self.dst.new();
            {
                let agent = &mut ar.lock().unwrap();
                let entry = &mut er.lock().unwrap();
                entry.is_positive = if agent.is_infected() {
                    rng.gen::<f64>() < self.runtime_params.tst_sens / 100.
                } else {
                    rng.gen::<f64>() > self.runtime_params.tst_spec / 100.
                };
                agent.last_tested = self.runtime_params.step;
                entry.time_stamp = self.runtime_params.step;
                entry.agent = Some(ar.clone());
                agent.in_test_queue = true;
            }
            self.test_que.push_back(er);
        }
        self.testees.clear();
        // for i in TestType::TestAsSymptom..TestType::TestPositive {
        //     //     testCount[TestTotal] += testCount[i];
        // }
    }
    fn go_ahead(&mut self) {
        if self.loop_mode == LoopMode::LoopFinished {
            self.reset_pop();
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

    fn do_one_step(wr: &MRef<World>) {
        let n_in_field = {
            let w = &mut wr.lock().unwrap();
            w.do_one_step_first()
        };
        let infectors = World::do_one_step_second(wr, n_in_field);
        {
            let w = &mut wr.lock().unwrap();
            w.do_one_step_third(infectors)
        }
    }

    fn do_one_step_first(&mut self) -> usize {
        let n_cells = {
            let mesh = self.world_params.mesh as usize;
            mesh * mesh
        };

        self.p_range = Vec::with_capacity(n_cells);
        for _ in 0..n_cells {
            self.p_range.push(Range::default());
        }

        let n_in_field = {
            let _pop = self._pop.lock().unwrap();
            let mut n_in_field = 0;
            let pop = &mut self.pop;
            for i in 0..n_cells {
                self.p_range[i].location = n_in_field;
                for aa in &_pop[i] {
                    pop[n_in_field as usize] = Some(aa.clone());
                    n_in_field += 1;
                }
                self.p_range[i].length = n_in_field - self.p_range[i].location;
            }
            n_in_field as usize
        };

        let old_time_stamp = {
            // two weeks
            self.runtime_params.step - self.world_params.steps_per_day * 14
        };
        let (ps, _) = self.pop.split_at(n_in_field);
        ps.into_par_iter().for_each(|ar_opt| {
            if let Some(ar) = ar_opt {
                {
                    let a = &mut ar.lock().unwrap();
                    a.reset_for_step();
                }
                let dsc = &mut self.dsc.lock().unwrap();
                remove_old_cinfo(dsc, ar.clone(), old_time_stamp);
            }
        });

        self.manage_gatherings();
        self.gathering_map.par_iter().for_each(|(&num, wrgs)| {
            let _pop = &mut self._pop.lock().unwrap();
            for aa in &_pop[num as usize] {
                let a = &mut aa.lock().unwrap();
                if !a.is_infected() {
                    for amg in wrgs.iter() {
                        let g = amg.lock().unwrap();
                        g.affect_to_agent(a);
                    }
                }
            }
        });

        let (rs, _) = self.p_range.split_at(n_cells);
        rs.into_iter().for_each(|rng| {
            let len = rng.length as usize;
            let loc = rng.location as usize;

            for j in 0..len {
                if let Some(ar) = &self.pop[(loc + j)] {
                    for k in (j + 1)..len {
                        if let Some(br) = &self.pop[loc + k] {
                            let dsc = &mut self.dsc.lock().unwrap();
                            Agent::interacts(
                                ar.clone(),
                                br.clone(),
                                &self.world_params,
                                &self.runtime_params,
                                dsc,
                            );
                        }
                    }
                }
            }
        });
        n_in_field
    }

    fn do_one_step_second(wr: &MRef<World>, n_in_field: usize) -> Mutex<Vec<InfectionCntInfo>> {
        intersect_grids(wr.clone());

        // step
        let infectors = Mutex::new(vec![]);
        (0..n_in_field).into_par_iter().for_each(|i| {
            let ar_opt = {
                let w = wr.lock().unwrap();
                w.pop[i].clone()
            };
            if let Some(ar) = &ar_opt {
                Agent::step_agent(wr, ar);
                {
                    let a = &mut ar.lock().unwrap();
                    if a.new_n_infects > 0 {
                        let infs = &mut infectors.lock().unwrap();
                        infs.push(InfectionCntInfo {
                            org_v: a.n_infects,
                            new_v: a.n_infects + a.new_n_infects,
                        });
                        a.n_infects += a.new_n_infects;
                        a.new_n_infects = 0;
                    }
                }
            }
        });

        for ar in &wr.lock().unwrap().q_list {
            Agent::step_agent_in_quarantine(wr.clone(), ar.clone());
        }

        infectors
    }

    fn do_one_step_third(&mut self, infectors: Mutex<Vec<InfectionCntInfo>>) {
        self.warp_steps();

        let mut test_count = EnumMap::default();
        self.deliver_test_results(&mut test_count);

        let finished = {
            let si = &mut self.stat_info.lock().unwrap();
            si.calc_stat_with_test(&self, &test_count, &infectors.lock().unwrap())
        };

        self.runtime_params.step += 1;
        if self.loop_mode == LoopMode::LoopRunning {
            if finished {
                self.loop_mode = LoopMode::LoopFinished;
            } else if self.predicate_to_stop {
                self.loop_mode = LoopMode::LoopEndByCondition;
            }
        }
    }

    pub fn debug(&self) {
        self.stat_info.lock().unwrap().debug_show();
    }
}

fn add_agent(ar: MRef<Agent>, pop: &mut Vec<VecDeque<MRef<Agent>>>, wp: &WorldParams) {
    let k = ar.lock().unwrap().index_in_pop(wp) as usize;
    pop[k].push_front(ar);
}

fn remove_agent(ar: &MRef<Agent>, pop: &mut Vec<VecDeque<MRef<Agent>>>, wp: &WorldParams) {
    let k = ar.lock().unwrap().index_in_pop(wp) as usize;
    pop[k].remove_p(ar);
}

pub fn start(
    wr: MRef<World>,
    stop_at: i32, /*, max_sps: f64, prio: f64*/
) -> thread::JoinHandle<()> {
    {
        let world = &mut wr.lock().unwrap();
        if world.loop_mode == LoopMode::LoopRunning {
            return thread::spawn(|| {});
        }
        if stop_at > 0 {
            world.stop_at_n_days = stop_at;
        }
        // world.max_sps = max_sps;
        world.go_ahead();
        world.loop_mode = LoopMode::LoopRunning;
    }
    let wr = wr.clone();
    thread::spawn(move || {
        running_loop(&wr);
        println!("loop end");
    })
}

pub fn step(wr: &MRef<World>) {
    let world = &mut wr.lock().unwrap();
    match world.loop_mode {
        LoopMode::LoopRunning => {}
        LoopMode::LoopFinished | LoopMode::LoopEndByCondition => {
            world.go_ahead();
        }
        _ => World::do_one_step(wr),
    }
    world.loop_mode = LoopMode::LoopEndByUser;
    // [self forAllReporters:^(PeriodicReporter *rep) { [rep sendReport]; }];
}

pub fn stop(wr: &MRef<World>) {
    let world = &mut wr.lock().unwrap();
    if world.loop_mode == LoopMode::LoopRunning {
        world.loop_mode = LoopMode::LoopEndByUser;
    }
}

pub fn export(wr: &MRef<World>, wtr: &mut Writer<File>) -> Result<(), Box<dyn Error>> {
    let world = &mut wr.lock().unwrap();
    let stat_info = world.stat_info.lock().unwrap();
    stat_info.write_statistics(wtr)?;
    Ok(())
}

fn running_loop(wr: &MRef<World>) {
    loop {
        {
            let w = wr.lock().unwrap();
            if w.loop_mode != LoopMode::LoopRunning {
                break;
            }
        }
        World::do_one_step(wr);
        {
            let world = &mut wr.lock().unwrap();
            if world.loop_mode == LoopMode::LoopEndByCondition
                && world.scenario_index < world.scenario.len() as i32
            {
                world.exec_scenario();
                world.loop_mode = LoopMode::LoopRunning;
            }
            if world.stop_at_n_days > 0
                && world.runtime_params.step
                    >= world.stop_at_n_days * world.world_params.steps_per_day - 1
            {
                world.loop_mode = LoopMode::LoopEndAsDaysPassed;
                break;
            }
            let new_time = get_uptime();
            let time_passed = new_time - world.prev_time;
            if time_passed < 1.0 {
                world.steps_per_sec += ((1.0 / time_passed).min(30.0) - world.steps_per_sec) * 0.2;
            }
            world.prev_time = new_time;
        }
        // [self forAllReporters:^(PeriodicReporter *rep) { [rep sendReportPeriodic]; }];

        // ignore to sleep
    }
    // [self forAllReporters:^(PeriodicReporter *rep) { [rep start]; }];
    /*
    {
        let world = &mut wr.lock().unwrap();
        if world.loop_mode != LoopMode::LoopEndByUser {
            world.touch();
        }
    }
    */
    // [self forAllReporters:^(PeriodicReporter *rep) { [rep pause]; }];
    /*
    if let Some(cb) = &world.stop_call_back {
        cb(world.loop_mode);
    }
    */
}

fn iter_gtog(
    wr: &MRef<World>,
    mesh: usize,
    a0: usize,
    b0: usize,
    ia: &(dyn Fn(usize, usize) -> usize + Sync),
    ib: &(dyn Fn(usize, usize) -> usize + Sync),
) {
    (a0..mesh).into_par_iter().step_by(2).for_each(|a| {
        (b0..mesh).into_par_iter().for_each(|b| {
            wr.lock().unwrap().grid_to_grid_a(ia(a, b), ib(a, b));
        });
    });
}

fn intersect_grids(wr: MRef<World>) {
    let mesh = { wr.lock().unwrap().world_params.mesh as usize };
    iter_gtog(&wr, mesh, 1, 0, &|x, y| y * mesh + x, &|x, y| {
        y * mesh + x - 1
    });
    iter_gtog(&wr, mesh, 2, 0, &|x, y| y * mesh + x, &|x, y| {
        y * mesh + x - 1
    });
    iter_gtog(&wr, mesh, 1, 0, &|y, x| y * mesh + x, &|y, x| {
        (y - 1) * mesh + x
    });
    iter_gtog(&wr, mesh, 2, 0, &|y, x| y * mesh + x, &|y, x| {
        (y - 1) * mesh + x
    });
    iter_gtog(&wr, mesh, 1, 1, &|y, x| y * mesh + x, &|y, x| {
        (y - 1) * mesh + x - 1
    });
    iter_gtog(&wr, mesh, 2, 1, &|y, x| y * mesh + x, &|y, x| {
        (y - 1) * mesh + x - 1
    });
    iter_gtog(&wr, mesh, 1, 1, &|y, x| y * mesh + x - 1, &|y, x| {
        (y - 1) * mesh + x
    });
    iter_gtog(&wr, mesh, 2, 1, &|y, x| y * mesh + x - 1, &|y, x| {
        (y - 1) * mesh + x
    });
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
        .collect()
}

fn get_uptime() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime before UNIX EPOCH!")
        .as_secs_f64()
}
