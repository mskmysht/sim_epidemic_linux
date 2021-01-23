use crate::common_types::*;
use crate::contact::*;
use crate::enum_map::*;
use crate::gathering::*;
use crate::iter::MyIter;
use crate::stat::*;
use crate::{agent::*, dyn_struct::DynStruct};

use rand::distributions::Alphanumeric;
use rand::{self, Rng};
use rayon::prelude::*;
use std::f64;
use std::fmt::*;
use std::sync::{Arc, Mutex};
use std::thread;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

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

// #[derive(Default)]
pub struct World {
    pub id: String,
    loop_mode: LoopMode,
    pub runtime_params: RuntimeParams,
    pub world_params: WorldParams,
    pub tmp_world_params: WorldParams,
    n_mesh: i32,
    pub agents: Mutex<Vec<MRef<Agent>>>,
    pub _pop: Mutex<Vec<Option<MRef<Agent>>>>,
    pop: Vec<Option<MRef<Agent>>>,
    n_pop: i32,
    p_range: Vec<Range>,
    prev_time: f64,
    steps_per_sec: f64,
    pub warp_list: Vec<Arc<WarpInfo>>,
    new_warp_f: HashMap<i32, Arc<WarpInfo>>,
    testees: HashMap<i32, TestType>,
    stop_at_n_days: i32,
    pub q_list: Option<MRef<Agent>>,
    pub c_list: Option<MRef<Agent>>,
    stat_info: Mutex<StatInfo>,
    scenario_index: i32,
    scenario: Vec<i32>, // Vec<Scenario>
    // n_cores: i32,
    contact_state: Mutex<ContactState>,
    dsc: DynStruct<ContactInfo>,
    gatherings: Vec<MRef<Gathering>>,
    gathering_map: GatheringMap,
    pub recov_p_hist: Vec<MyCounter>,
    pub incub_p_hist: Vec<MyCounter>,
    pub death_p_hist: Vec<MyCounter>,
    predicate_to_stop: bool,
    test_que_head: Option<MRef<TestEntry>>,
    test_que_tail: Option<MRef<TestEntry>>,
    dst: DynStruct<TestEntry>,
}

impl Display for World {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "id:{}/running:{}", self.id, self.running(),)
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
            // init_params: user_default_runtime_params,
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
            q_list: None,
            c_list: None,
            stat_info: Mutex::new(StatInfo::new()),
            scenario_index: 0,
            scenario: vec![],
            contact_state: Default::default(),
            dsc: Default::default(),
            gatherings: vec![],
            gathering_map: HashMap::new(),
            recov_p_hist: vec![],
            incub_p_hist: vec![],
            death_p_hist: vec![],
            predicate_to_stop: false,
            test_que_head: None,
            test_que_tail: None,
            dst: Default::default(),
        };

        w.reset_pop();
        w
    }
    fn running(&self) -> bool {
        self.loop_mode == LoopMode::LoopRunning
    }
    fn reset_pop(&mut self) {
        let mut rng = rand::thread_rng();
        let wp = &self.world_params;
        // set runtime params of scenario != None
        self.gatherings.clear();

        // reset contact info heaps
        {
            let cs = &mut self.contact_state.lock().unwrap();
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
        {
            // Mutex::new(vec![None; (self.n_mesh * self.n_mesh) as usize]);
            let _pop = &mut self._pop.lock().unwrap();
            _pop.clear();
            _pop.resize((self.n_mesh * self.n_mesh) as usize, None);
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
                a.reset(wp.world_size as f64);
                a.id = i;
                if i < n_dist {
                    a.distancing = true
                }
                if i < wp.n_init_infec && i == infec_idxs[i_idx] {
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
        self.q_list = None; //.clear();
        self.c_list = None; // .clear();
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

        // self.debug_pop_internal();
        // self.debug_traverse_pop();

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

    // fn warp_step(&mut self, ar: &MRef<Agent>, mode: WarpType, goal: Point) -> bool {
    fn warp_steps(&mut self) {
        for info in self.new_warp_f.values() {
            let a = &mut info.agent.lock().unwrap();
            if a.is_warping {
                // let w = &mut wr.clone().lock().unwrap();
                self.warp_list.retain(|wi| {
                    let wa = &wi.agent.lock().unwrap();
                    println!("retain: {}, {}", a.id, wa.id);
                    !std::ptr::eq(&**wa, &**a)
                });
            } else {
                // let a = &mut ar.lock().unwrap();
                a.is_warping = true;
                match info.mode {
                    WarpType::WarpInside | WarpType::WarpToHospital | WarpType::WarpToCemeteryF => {
                        remove_agent(a, &mut self._pop.lock().unwrap(), &self.world_params);
                    }
                    WarpType::WarpBack | WarpType::WarpToCemeteryH => {
                        remove_from_list(a, &mut self.q_list);
                    }
                }
            }
        }

        for info in self.new_warp_f.values() {
            let ar = &info.agent;
            let mode = info.mode;
            let goal = info.goal;

            let wp = &self.world_params;
            let a = &mut ar.lock().unwrap();
            let dp = Point {
                x: goal.x - a.x,
                y: goal.y - a.y,
            };

            let d = dp.y.hypot(dp.x);
            let v = wp.world_size as f64 / 5. / wp.steps_per_day as f64;
            if d < v {
                a.x = goal.x;
                a.y = goal.y;
                a.is_warping = false;
                match mode {
                    WarpType::WarpInside | WarpType::WarpBack => {
                        add_agent(ar.clone(), &mut self._pop.lock().unwrap(), wp)
                    }
                    WarpType::WarpToHospital => {
                        add_to_list(ar.clone(), &mut self.q_list);
                        a.got_at_hospital = true;
                    }
                    WarpType::WarpToCemeteryF | WarpType::WarpToCemeteryH => {
                        add_to_list(ar.clone(), &mut self.c_list);
                    }
                }
            // true
            } else {
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

        gatherings.retain(|amg| {
            let mut g = amg.lock().unwrap();
            g.remove_from_map(gat_map);
            !g.step(wp.steps_per_day)
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
        let cs = &mut self.contact_state.lock().unwrap();

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
                    Agent::interacts(ar.clone(), br.clone(), wp, rp, cs);
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
        for er in MyIter::new(self.test_que_head.clone()) {
            let entry = &mut er.lock().unwrap();
            if entry.time_stamp > c_tm {
                break;
            }
            if entry.is_positive {
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
                    for cr in MyIter::new(a.contact_info_head.clone()) {
                        let c = cr.lock().unwrap();
                        self.test_infection_of_agent(
                            &c.agent.lock().unwrap(),
                            TestType::TestAsContact,
                        );
                    }
                    if let (Some(hr), Some(tr)) = (&a.contact_info_head, &a.contact_info_tail) {
                        self.dsc.restore(&mut tr.lock().unwrap().next, hr.clone());
                    }
                    a.contact_info_head = None;
                    a.contact_info_tail = None;
                }
            } else {
                test_count[TestType::TestNegative] += 1;
            }
            if let Some(ar) = &entry.agent {
                ar.lock().unwrap().in_test_queue = false;
            }
            self.test_que_head = entry.next.clone();
            if let Some(er) = &entry.next {
                er.lock().unwrap().prev = None;
            } else {
                self.test_que_tail = None;
            }
            self.dst.restore(&mut entry.next, er.clone())
        }

        // enqueue new tests
        for (&num, &v) in &self.testees {
            test_count[v] += 1;
            let ar = &self.agents.lock().unwrap()[num as usize];
            let agent = &mut ar.lock().unwrap();
            let er = self.dst.new(TestEntry::default);
            let entry = &mut er.lock().unwrap();
            entry.is_positive = if agent.is_infected() {
                rng.gen::<f64>() < self.runtime_params.tst_sens / 100.
            } else {
                rng.gen::<f64>() > self.runtime_params.tst_spec / 100.
            };
            agent.last_tested = self.runtime_params.step;
            entry.time_stamp = self.runtime_params.step;
            entry.agent = Some(ar.clone());
            entry.prev = self.test_que_tail.clone();
            if let Some(tr) = &self.test_que_tail {
                tr.lock().unwrap().next = Some(er.clone());
            } else {
                self.test_que_head = Some(er.clone());
            }
            entry.next = None;
            self.test_que_tail = Some(er.clone());
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
    fn touch(&mut self) -> bool {
        todo!()
        //     let mut result;
        //     result = self.docKey != nil;
        //     if ()) _lastTouch = NSDate.date;
        //     [_lastTLock unlock];
        //     return result;
    }

    fn do_one_step(wr: MRef<World>) {
        let n_in_field = {
            let w = &mut wr.lock().unwrap();
            w.do_one_step_first()
        };
        let infectors = World::do_one_step_second(wr.clone(), n_in_field);
        {
            let w = &mut wr.lock().unwrap();
            w.do_one_step_third(infectors)
        }
    }

    fn do_one_step_first(&mut self) -> usize {
        println!("do_one_step");

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
            let pop = &mut self.pop; //.lock().unwrap();
            for i in 0..n_cells {
                self.p_range[i].location = n_in_field;
                for aa in MyIter::new(_pop[i].clone()) {
                    pop[n_in_field as usize] = Some(aa.clone());
                    n_in_field += 1;
                }
                self.p_range[i].length = n_in_field - self.p_range[i].location;
            }
            n_in_field as usize
        };

        let old_time_stamp = {
            self.runtime_params.step - self.world_params.steps_per_day * 14
            // two weeks
        };
        let (ps, _) = self.pop.split_at(n_in_field);
        ps.into_par_iter().for_each(|ar_opt| {
            if let Some(ar) = ar_opt {
                {
                    let a = &mut ar.lock().unwrap();
                    a.reset_for_step();
                }
                let mut cs = self.contact_state.lock().unwrap();
                cs.remove_old_cinfo(ar.clone(), old_time_stamp);
            }
        });

        self.manage_gatherings();
        self.gathering_map.par_iter().for_each(|(num, wrgs)| {
            let iter = {
                let _pop = &mut self._pop.lock().unwrap();
                let opt_ar = { &_pop[*num as usize] };
                MyIter::new(opt_ar.clone())
            };
            for aa in iter {
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
            // let world = &mut wr.lock().unwrap();
            // let rng = &wr.lock().unwrap().p_range[i];
            let len = rng.length as usize;
            let loc = rng.location as usize;

            for j in 0..len {
                if let Some(ar) = &self.pop[(loc + j)] {
                    for k in (j + 1)..len {
                        if let Some(br) = &self.pop[loc + k] {
                            let cs = &mut self.contact_state.lock().unwrap();
                            Agent::interacts(
                                ar.clone(),
                                br.clone(),
                                &self.world_params,
                                &self.runtime_params,
                                cs,
                            );
                        }
                    }
                }
            }
        });
        n_in_field
    }

    fn do_one_step_second(wr: MRef<World>, n_in_field: usize) -> Mutex<Vec<InfectionCntInfo>> {
        intersect_grids(wr.clone());

        // step
        let infectors = Mutex::new(vec![]);
        (0..n_in_field).into_par_iter().for_each(|i| {
            let ar_opt = {
                let w = wr.lock().unwrap();
                w.pop[i].clone()
            };
            if let Some(ar) = ar_opt {
                Agent::step_agent(wr.clone(), ar.clone());
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
        infectors
    }

    fn do_one_step_third(&mut self, infectors: Mutex<Vec<InfectionCntInfo>>) {
        for ref ar in MyIter::new(self.q_list.clone()) {
            Agent::step_agent_in_quarantine(self, ar.clone());
        }
        /*
        let new_warp_f = &self.new_warp_f;
        for info in new_warp_f.values() {
            // if !self.warp_step(&info.agent, info.mode, info.goal) {
            if !self.warp_step(info) {
                self.warp_list.push(info.clone());
            }
        }
        */
        self.warp_steps();

        let mut test_count = EnumMap::default();
        self.deliver_test_results(&mut test_count);

        let finished = {
            // let w = wr.lock().unwrap();
            let si = &mut self.stat_info.lock().unwrap();
            si.calc_stat_with_test(&self, &test_count, &infectors.lock().unwrap())
        };

        self.runtime_params.step += 1;
        if self.loop_mode == LoopMode::LoopRunning {
            if finished {
                self.loop_mode = LoopMode::LoopFinished;
            } else if self.predicate_to_stop {
                // if ([predicateToStop evaluateWithObject:statInfo])
                self.loop_mode = LoopMode::LoopEndByCondition;
            }
        }
    }
    fn debug_list_pop(pop: &Vec<Option<MRef<Agent>>>) {
        for (i, ar_opt) in pop.iter().enumerate() {
            print!("{}:", i);
            if let Some(ar) = &ar_opt {
                let a = ar.lock().unwrap();
                print!("{},", a.id);
                if let Some(nr) = &a.next {
                    let n = nr.lock().unwrap();
                    print!("{},", n.id);
                } else {
                    print!(",");
                }
                if let Some(pr) = &a.prev {
                    let p = pr.lock().unwrap();
                    print!("{}", p.id);
                }
            }
            println!();
        }
    }
    pub fn debug_pop(&self) {
        World::debug_list_pop(&self._pop.lock().unwrap());
    }
    pub fn debug_pop_internal(&self) {
        World::debug_list_pop(&self.pop);
    }
    pub fn debug_traverse_pop(&self) {
        for (i, ar_opt) in self._pop.lock().unwrap().iter().enumerate() {
            print!("{}:", i);
            for ar in MyIter::new(ar_opt.clone()) {
                let a = ar.lock().unwrap();
                let p = a.prev.clone();
                print!("{}({}),", a.id, p.map_or(-1, |ar| ar.lock().unwrap().id));
            }
            println!();
        }
    }
}

/*
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
*/

pub fn start(wr: MRef<World>, stop_at: i32, max_sps: f64, prio: f64) -> thread::JoinHandle<()> {
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
        running_loop(wr);
    })
}

pub fn step(wr: MRef<World>) {
    let world = &mut wr.lock().unwrap();
    match world.loop_mode {
        LoopMode::LoopRunning => {}
        LoopMode::LoopFinished | LoopMode::LoopEndByCondition => {
            world.go_ahead();
        }
        _ => World::do_one_step(wr.clone()),
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

/*
*/

fn running_loop(wr: MRef<World>) {
    loop {
        {
            let wr = wr.clone();
            let w = wr.lock().unwrap();
            if w.loop_mode != LoopMode::LoopRunning {
                break;
            }
        }
        World::do_one_step(wr.clone());
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
                    == world.stop_at_n_days * world.world_params.steps_per_day - 1
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
