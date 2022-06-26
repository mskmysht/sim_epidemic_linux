use crate::agent::cont::{Cemetery, Field, Hospital, Warps};
use crate::agent::{
    self, Agent, Area, ParamsForStep, VaccineInfo, VariantInfo, MAX_N_VARIANTS, MAX_N_VAXEN,
};
use crate::commons::{self, DrainMap, WRef};
use crate::commons::{
    math::{Point, Range},
    LoopMode, MRef, MyCounter, RuntimeParams, WarpType, WorldParams,
};
use crate::dyn_struct::DynStruct;
use crate::enum_map::EnumMap;
// use crate::enum_map::*;
use crate::gathering::Gathering;
use crate::stat::{InfectionCntInfo, StatInfo};
use crate::testing::TestQueue;
use crate::{
    commons::{HealthType, WrkPlcMode},
    contact::ContactInfo,
    testing::{TestEntry, TestReason},
};
use csv::Writer;
use rand::distributions::Alphanumeric;
use rand::{self, Rng};
use rayon::prelude::*;
use std::error::Error;
use std::fmt;
use std::ops::DerefMut;
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use std::{collections::VecDeque, f64};

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
    pub runtime_params: RuntimeParams,
    pub world_params: WorldParams,
    pub tmp_world_params: WorldParams,
    n_mesh: usize,
    pub agents: Vec<MRef<Agent>>,
    // pub agents_: Mutex<Vec<Agent>>,
    pub _pop: Mutex<Vec<VecDeque<MRef<Agent>>>>,
    pop: Vec<Option<MRef<Agent>>>,
    n_pop: usize,
    p_range: Vec<Range>,
    prev_time: f64,
    steps_per_sec: f64,
    pub warp_list: Vec<(MRef<Agent>, WarpInfo)>,
    // new_warp_f: HashMap<usize, Arc<WarpInfo>>,
    testees: HashMap<usize, TestReason>,
    // _testees: Mutex<HashMap<usize, TestType>>,
    stop_at_n_days: i32,
    pub q_list: VecDeque<MRef<Agent>>,
    pub _q_list: VecDeque<usize>,
    pub c_list: VecDeque<MRef<Agent>>,
    stat_info: Mutex<StatInfo>,
    scenario_index: i32,
    scenario: Vec<i32>, // Vec<Scenario>
    dsc: Mutex<DynStruct<ContactInfo>>,
    gatherings: Vec<MRef<Gathering>>,
    gat_spots_fixed: Vec<Point>,
    // gathering_map: GatheringMap,
    pub hist: Mutex<Hist>,
    pub recov_p_hist: Vec<MyCounter>,
    pub incub_p_hist: Vec<MyCounter>,
    pub death_p_hist: Vec<MyCounter>,
    predicate_to_stop: bool,
    test_que: VecDeque<MRef<TestEntry>>,
    dst: DynStruct<TestEntry>,
    variant_info: [VariantInfo; MAX_N_VARIANTS],
    vaccine_info: [VaccineInfo; MAX_N_VAXEN],
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
            // agents_: Default::default()
            _pop: Mutex::new(Vec::new()),
            pop: Vec::new(),
            n_pop: 0,
            p_range: Vec::new(),
            prev_time: 0.0,
            steps_per_sec: 0.0,
            warp_list: Vec::new(),
            // new_warp_f: HashMap::new(),
            testees: HashMap::new(),
            // _testees: Default::default(),
            stop_at_n_days: -365,
            q_list: Default::default(),
            _q_list: Default::default(),
            c_list: Default::default(),
            stat_info: Mutex::new(StatInfo::new()),
            scenario_index: 0,
            scenario: Vec::new(),
            dsc: Default::default(),
            gatherings: Vec::new(),
            // gathering_map: HashMap::new(),
            hist: Default::default(),
            recov_p_hist: Vec::new(),
            incub_p_hist: Vec::new(),
            death_p_hist: Vec::new(),
            predicate_to_stop: false,
            test_que: Default::default(),
            dst: Default::default(),
            vaccine_info: todo!(),
            variant_info: todo!(),
        };

        w.reset_pop();
        w
    }

    // fn running(&self) -> bool {
    //     self.loop_mode == LoopMode::LoopRunning
    // }

    fn _reset_pop(&mut self) {}

    fn reset_pop(&mut self) {
        let mut rng = rand::thread_rng();
        let wp = &self.world_params;
        let rp = &self.runtime_params;

        // set runtime params of scenario != None
        self.gatherings.clear();

        // reset contact info heaps
        {
            let dsc = &mut self.dsc.lock().unwrap();
            for i in 0..self.n_pop as usize {
                let agents = self.agents;
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
        // self.gathering_map.clear();
        {
            let _pop = &mut self._pop.lock().unwrap();
            _pop.clear();
            _pop.resize(self.n_mesh * self.n_mesh, Default::default());
        }
        if self.n_pop != wp.init_pop {
            self.n_pop = wp.init_pop;
            self.pop.clear();
            self.pop.resize(self.n_pop, None);
            self.agents.resize_with(self.n_pop, Default::default);
        }

        let n_dist = (self.runtime_params.dst_ob / 100. * (self.n_pop as f64)) as usize;
        let mut infec_idxs: Vec<usize> = Vec::with_capacity(wp.n_init_infec);
        for i in 0..wp.n_init_infec {
            let mut k = ((self.n_pop - i - 1) as f64 * rng.gen::<f64>()) as usize;
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
            let agents = self.agents;
            let ar = &agents[i as usize];
            {
                let a = &mut ar.lock().unwrap();
                a.reset(wp.field_size as f64, &rp);
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

    // pub fn add_new_warp(&mut self, info: Arc<WarpInfo>) {
    //     let a = info.agent.lock().unwrap();
    //     self.new_warp_f.insert(a.id, info.clone());
    // }

    fn exec_scenario(&mut self) {
        todo!();
    }

    /*
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
    */

    fn manage_gatherings(&mut self, agent_grid: &mut Field) {
        let gats = &mut self.gatherings;
        let wp = &self.world_params;
        let rp = &self.runtime_params;

        gats.retain_mut(|gat| {
            let is_expired = {
                let mut gat = gat.lock().unwrap();
                gat.step(wp.steps_per_day)
            };
            if is_expired {
                drop(gat);
                false
            } else {
                true
            }
        });

        //	caliculate the number of gathering circles
        //	using random number in exponetial distribution.
        let rng = &mut rand::thread_rng();
        let n_new_gat =
            (rp.gat_fr / wp.steps_per_day as f64 * (wp.field_size * wp.field_size) as f64 / 1e5
                * (-(rng.gen::<f64>() * 0.9999 + 0.0001).ln()))
            .round() as usize;
        for _ in 0..n_new_gat {
            gats.push(Gathering::setup(
                agent_grid,
                &self.gat_spots_fixed,
                &self.agents,
                wp,
                rp,
                rng,
            ));
        }
    }

    // fn _grid_to_grid(&self, a_as: &mut [Agent], b_as: &mut [Agent]) {
    //     let wp = &self.world_params;
    //     let rp = &self.runtime_params;
    //     // let dsc = &mut self.dsc.lock().unwrap();

    //     for a in a_as {
    //         a._interacts(&grid[bl.0][bl.1].lock().unwrap(), wp, rp);
    //     }
    // }

    /*
    fn grid_to_grid_a(&self, ia: usize, ib: usize) {
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
    */

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

    /*
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
    */

    fn _do_one_step(&mut self) {
        let mut field = todo!();
        let mut warps = todo!();
        let mut hospital = todo!();
        let mut cemetery = todo!();
        let mut test_que = todo!();
        self._do_one_step_123th(
            &mut field,
            &mut warps,
            &mut hospital,
            &mut cemetery,
            &mut test_que,
        );
        // self.do_one_step_third(infectors);
    }

    fn _do_one_step_123th(
        &mut self,
        field: &mut Field,
        warps: &mut Warps,
        hospital: &mut Hospital,
        cemetery: &mut Cemetery,
        test_queue: &mut TestQueue,
    ) {
        let mesh = self.world_params.mesh as usize;
        // let n_cells = { mesh * mesh };

        // let mut grid_locs = Vec::new();
        // let mut n_in_field = 0;
        // {
        //     for (i, v) in grid.iter().enumerate() {
        //         let l = v.len();
        //         for j in 0..l {
        //             grid_locs.push((i, j));
        //         }
        //         n_in_field += l;
        //     }
        // }

        let prms = ParamsForStep::new(
            &self.runtime_params,
            &self.world_params,
            &self.variant_info,
            &self.vaccine_info,
        );

        // let mut grid = Vec::with_capacity(mesh);
        // {
        //     for _ in 0..mesh {
        //         let mut v = Vec::with_capacity(mesh);
        //         for _ in 0..mesh {
        //             v.push(Mutex::new(Vec::new()));
        //         }
        //         grid.push(v);
        //     }
        // }

        // two weeks
        let old_time_stamp = self.runtime_params.step - self.world_params.steps_per_day * 14;
        field.par_h_iter().for_each(|(_, ags)| {
            for (_, a) in ags {
                let a = a.lock().unwrap();
                a.reset_for_step();
                // remove contact logs as old as two weeks (14 days)
                // todo: refactor to move into deliver
                while let Some(ci) = a._contact_info_list.pop_back() {
                    if ci.time_stamp <= old_time_stamp {
                        a._contact_info_list.push_back(ci);
                        break;
                    }
                }
            }
        });

        // (0..mesh).into_par_iter().for_each(|r| {
        //     (0..mesh).into_par_iter().for_each(|c| {
        //         for a in grid[r][c].lock().unwrap().iter() {
        //             let mut a = a.lock().unwrap();
        //             a.reset_for_step();
        //             // let dsc = &mut self.dsc.lock().unwrap();
        //             // remove_old_cinfo(dsc, ar.clone(), old_time_stamp);
        //             while let Some(ci) = a._contact_info_list.pop_back() {
        //                 if ci.time_stamp <= old_time_stamp {
        //                     a._contact_info_list.push_back(ci);
        //                     break;
        //                 }
        //             }
        //         }
        //     });
        // });

        let mut step_log = todo!();
        let mut count_reason = todo!();
        let mut count_result = todo!();

        if !commons::go_home_back(&self.world_params, &self.runtime_params) {
            self.manage_gatherings(field);
        }

        let unit_j: usize = 20;
        field.intersect(&prms);
        test_queue.accept(&prms, count_reason, count_result);
        field.steps(warps, test_queue, &mut step_log, &prms);
        hospital.steps(warps, &mut step_log, &prms);
        warps.steps(field, hospital, cemetery, &prms);

        // for h in hists.into_iter() {
        //     todo!("cummulate hist");
        // }

        // for i in infcts.into_iter() {
        //     todo!("infect");
        // }

        let mut stat_info = StatInfo::new();
        let finished = {
            // let si = &mut self.stat_info.lock().unwrap();
            stat_info.calc_stat_with_test(&self, &test_count, &infectors.lock().unwrap())
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

    fn grid_par_iterate<F: Fn(usize, usize) + Sync + Send>(
        unit_j: usize,
        mesh: usize,
        trim: usize,
        m0: usize,
        f: F,
    ) {
        (0..unit_j).into_par_iter().for_each(|j| {
            let start = j * (mesh - trim) / unit_j;
            let end = (j + 1) * (mesh - trim) / unit_j;
            for n in start..end {
                for m in (m0..mesh).step_by(2) {
                    f(n, m);
                }
            }
        });
    }

    /*
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
    */

    fn debug(&self) {
        self.stat_info.lock().unwrap().debug_show();
    }
}

// fn add_agent(ar: MRef<Agent>, pop: &mut Vec<VecDeque<MRef<Agent>>>, wp: &WorldParams) {
//     let k = ar.lock().unwrap().index_in_pop(wp) as usize;
//     pop[k].push_front(ar);
// }

// fn remove_agent(ar: &MRef<Agent>, pop: &mut Vec<VecDeque<MRef<Agent>>>, wp: &WorldParams) {
//     let k = ar.lock().unwrap().index_in_pop(wp) as usize;
//     pop[k].remove_p(ar);
// }

fn running_loop(wr: MRef<World>) {
    loop {
        {
            let w = wr.lock().unwrap();
            if w.loop_mode != LoopMode::LoopRunning {
                break;
            }
        }
        // World::do_one_step(&Arc::clone(&wr));
        wr.lock().unwrap()._do_one_step();
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

/*
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
 */

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

pub fn stop_world(world: &mut World) {
    if world.loop_mode == LoopMode::LoopRunning {
        world.loop_mode = LoopMode::LoopEndByUser;
    }
}

pub fn step_world(wr: MRef<World>) {
    {
        let world = &mut wr.lock().unwrap();
        match world.loop_mode {
            LoopMode::LoopRunning => return,
            LoopMode::LoopFinished | LoopMode::LoopEndByCondition => {
                world.go_ahead();
            }
            _ => {}
        }
    }
    // World::do_one_step(&wr);
    wr.lock().unwrap().loop_mode = LoopMode::LoopEndByUser;
}

pub fn start_world(wr: MRef<World>, stop_at: i32) -> Option<thread::JoinHandle<()>> {
    {
        let world = &mut wr.lock().unwrap();
        if world.loop_mode == LoopMode::LoopRunning {
            return None;
        }
        if stop_at > 0 {
            world.stop_at_n_days = stop_at;
        }
        // world.max_sps = max_sps;
        world.go_ahead();
        world.loop_mode = LoopMode::LoopRunning;
    }
    Some(thread::spawn(move || {
        running_loop(Arc::clone(&wr));
    }))
}

pub fn reset_world(world: &mut World) {
    world.reset_pop();
}

pub fn debug_world(world: &World) {
    world.debug();
}

pub fn export_world(world: &World, path: &str) -> Result<(), Box<dyn Error>> {
    let stat_info = world.stat_info.lock().unwrap();
    let mut wtr = Writer::from_path(path)?;
    stat_info.write_statistics(&mut wtr)?;
    wtr.flush()?;
    Ok(())
}
