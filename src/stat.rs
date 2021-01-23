use std::sync::{Arc, Mutex};

use crate::{agent::Agent, dyn_struct::DynStruct, world::*};
use crate::{common_types::*, iter::MyIter};

use crate::enum_map::EnumMap;

const IMG_WIDTH: i32 = 320 * 4;
// const IMG_HEIGHT: i32 = 320;
const MAX_N_REC: i32 = IMG_WIDTH;

pub struct InfectionCntInfo {
    pub org_v: i32,
    pub new_v: i32,
}

/*
pub struct TimeEvoInfo {
    idx_bits: i32,
    n_indexes: i32,
    window_size: i32,
}
*/

#[derive(Copy, Clone, Default, Debug)]
pub struct TestResultCount {
    pub positive: i32,
    pub negative: i32,
}

// #[derive(Default)]
pub struct StatInfo {
    max_counts: UnionMap<HealthType, TestType, u32>,
    max_transit: UnionMap<HealthType, TestType, u32>,
    pop_size: i32,
    steps: i32,
    skip: i32,
    days: i32,
    skip_days: i32,
    stat_cumm: StatData,
    trans_daily: StatData,
    trans_cumm: StatData,
    test_cumm: EnumMap<TestType, u32>, // [u32; N_INT_TEST_TYPES as usize],
    test_results_w: [TestResultCount; 7],
    test_result_cnt: TestResultCount,
    max_step_p_rate: f64,
    max_daily_p_rate: f64,
    p_rate_cumm: f64,
    // phase_info: Vec<i32>,
    n_infects_hist: Vec<MyCounter>,
    // scenario_phases: Vec<i32>,
    statistics: Option<MRef<StatData>>, // StatData,
    transit: Option<MRef<StatData>>,
    ds: DynStruct<StatData>,
}

impl StatInfo {
    pub fn new() -> StatInfo {
        StatInfo {
            max_counts: UnionMap::default(),
            max_transit: UnionMap::default(),
            pop_size: 0,
            steps: 0,
            skip: 0,
            days: 0,
            skip_days: 0,
            stat_cumm: StatData::default(),
            trans_daily: StatData::default(),
            trans_cumm: StatData::default(),
            test_cumm: EnumMap::default(),
            test_results_w: [TestResultCount::default(); 7],
            test_result_cnt: TestResultCount::default(),
            max_step_p_rate: 0.,
            max_daily_p_rate: 0.,
            p_rate_cumm: 0.,
            // phase_info: vec![],
            n_infects_hist: vec![],
            // scenario_phases: vec![],
            statistics: None,
            transit: None,
            ds: Default::default(),
        }
    }

    pub fn reset(&mut self, n_pop: i32, n_init_infec: i32) {
        self.p_rate_cumm = 0.;
        self.max_step_p_rate = 0.;
        self.max_daily_p_rate = 0.;
        self.test_result_cnt = TestResultCount {
            positive: 0,
            negative: 0,
        };
        let n_not_inf = (n_pop - n_init_infec) as u32;
        self.max_counts.0[HealthType::Susceptible] = n_not_inf;
        self.max_counts.0[HealthType::Asymptomatic] = n_not_inf;
        if let Some(sr) = &self.statistics {
            let s = &mut sr.lock().unwrap();
            s.cnt.0[HealthType::Susceptible] = n_not_inf;
            s.cnt.0[HealthType::Asymptomatic] = n_init_infec as u32;
        }
        self.steps = 0;
        self.days = 0;
        self.skip = 1;
        self.skip_days = 1;
        self.pop_size = n_pop;

        // ------
        // incub_p_hist.clear();
    }
    pub fn calc_stat_with_test(
        &mut self,
        w: &World,
        test_count: &EnumMap<TestType, u32>,
        infectors: &Vec<InfectionCntInfo>,
    ) -> bool {
        // let w = &wr.lock().unwrap();
        let pop = &w._pop.lock().unwrap();
        let q_list = &w.q_list;
        let c_list = &w.c_list;
        let warp = &w.warp_list;
        let wp = &w.world_params;
        let steps_per_day = wp.steps_per_day;
        let n_cells = (wp.mesh * wp.mesh) as usize;

        self.debug_show();

        let mut tmp_stat = StatData::default();
        if self.steps % steps_per_day == 0 {
            self.trans_daily = StatData::default();
        }
        self.steps += 1;

        for i in 0..n_cells {
            for ar in MyIter::new(pop[i].clone()) {
                count_health(
                    &mut ar.lock().unwrap(),
                    &mut tmp_stat,
                    &mut self.trans_daily,
                );
            }
        }

        self.debug_show();

        for ar in MyIter::new(q_list.clone()) {
            let a = &mut ar.lock().unwrap();
            let q_idx = if a.health == HealthType::Symptomatic {
                HealthType::QuarantineSymp
            } else {
                HealthType::QuarantineAsym
            };
            if a.got_at_hospital {
                self.trans_daily.cnt.0[q_idx] += 1;
                a.got_at_hospital = false;
            } else if a.health == HealthType::Asymptomatic
                && a.new_health == HealthType::Symptomatic
            {
                self.trans_daily.cnt.0[HealthType::QuarantineSymp] += 1;
            }
            count_health(a, &mut tmp_stat, &mut self.trans_daily);
            tmp_stat.cnt.0[q_idx] += 1;
        }

        for info in warp {
            count_health(
                &mut info.agent.lock().unwrap(),
                &mut tmp_stat,
                &mut self.trans_daily,
            );
        }
        for ar in MyIter::new(c_list.clone()) {
            let a = &mut ar.lock().unwrap();
            count_health(a, &mut tmp_stat, &mut self.trans_daily);
        }

        for (k, v) in test_count {
            self.trans_daily.cnt.1[k] += v;
            self.test_cumm[k] += v;
            tmp_stat.cnt.1[k] = self.test_cumm[k];
        }
        tmp_stat.p_rate = calc_positive_rate(test_count);

        for (k, v) in &mut self.max_counts.0 {
            let c = tmp_stat.cnt.0[k];
            if *v < c {
                *v = c;
            }
        }
        for (k, v) in &mut self.max_counts.1 {
            let c = tmp_stat.cnt.1[k];
            if *v < c {
                *v = c;
            }
        }
        if self.max_step_p_rate < tmp_stat.p_rate {
            self.max_step_p_rate = tmp_stat.p_rate;
        }

        let idx_in_cum = self.steps % self.skip;
        if idx_in_cum == 0 {
            self.stat_cumm = StatData::default();
        }
        for (k, v) in &tmp_stat.cnt.0 {
            self.stat_cumm.cnt.0[k] += v;
        }
        for (k, v) in &tmp_stat.cnt.1 {
            self.stat_cumm.cnt.1[k] += v;
        }
        self.stat_cumm.p_rate = tmp_stat.p_rate;

        self.debug_show();

        if idx_in_cum + 1 >= self.skip {
            let mut new_stat = StatData::default(); // new_stat();
            for (k, v) in &self.stat_cumm.cnt.0 {
                new_stat.cnt.0[k] = v / self.skip as u32;
            }
            for (k, v) in &self.stat_cumm.cnt.1 {
                new_stat.cnt.1[k] = v / self.skip as u32;
            }

            new_stat.p_rate = self.stat_cumm.p_rate / (self.skip as f64);
            new_stat.next = self.statistics.clone();
            let nsr_opt = Some(Arc::new(Mutex::new(new_stat)));
            self.statistics = nsr_opt.clone();
            if self.steps / self.skip > MAX_N_REC {
                for pr in MyIter::new(nsr_opt.clone()) {
                    let p = &mut pr.lock().unwrap();
                    if let Some(qr) = &p.next.clone() {
                        let q = qr.lock().unwrap();
                        for (k, v) in &mut p.cnt.0 {
                            *v = (*v + q.cnt.0[k]) / 2;
                        }
                        for (k, v) in &mut p.cnt.1 {
                            *v = (*v + q.cnt.1[k]) / 2;
                        }
                        p.p_rate = (p.p_rate + q.p_rate) / 2.;
                        p.next = q.next.clone();
                        // q.next = freeStat;
                        // freeStat = q;
                    }
                }
                self.skip *= 2;
            }
        }
        if self.steps % steps_per_day == steps_per_day - 1 {
            let daily_tests = &self.trans_daily.cnt.1;
            self.trans_daily.p_rate = calc_positive_rate(&daily_tests);
            if self.days < 7 {
                let dtp = daily_tests[TestType::TestPositive] as i32;
                let dtn = daily_tests[TestType::TestNegative] as i32;
                self.test_results_w[self.days as usize].positive = dtp;
                self.test_results_w[self.days as usize].negative = dtn;
                self.test_result_cnt.positive += dtp;
                self.test_result_cnt.negative += dtn;
            } else {
                let idx = (self.days % 7) as usize;
                let dtp = daily_tests[TestType::TestPositive] as i32;
                let dtn = daily_tests[TestType::TestNegative] as i32;
                self.test_result_cnt.positive += dtp - self.test_results_w[idx].positive;
                self.test_results_w[idx].positive = dtp;
                self.test_result_cnt.negative += dtn - self.test_results_w[idx].negative;
                self.test_results_w[idx].negative = dtn;
            }
            self.days += 1;
            if self.max_daily_p_rate < self.trans_daily.p_rate {
                self.max_daily_p_rate = self.trans_daily.p_rate;
            }
            for (k, v) in &mut self.max_transit.0 {
                if *v < self.trans_daily.cnt.0[k] {
                    *v = self.trans_daily.cnt.0[k];
                }
            }
            for (k, v) in &mut self.max_transit.1 {
                if *v < self.trans_daily.cnt.1[k] {
                    *v = self.trans_daily.cnt.1[k];
                }
            }
            let idx_in_cum = self.days % self.skip_days;
            if idx_in_cum == 0 {
                self.trans_cumm.reset();
            }
            for (k, v) in &mut self.trans_cumm.cnt.0 {
                *v += self.trans_daily.cnt.0[k];
            }
            for (k, v) in &mut self.trans_cumm.cnt.1 {
                *v += self.trans_daily.cnt.1[k];
            }
            self.trans_cumm.p_rate += self.trans_daily.p_rate;

            if idx_in_cum + 1 >= self.skip_days {
                let new_tran_r = self.ds.new(|| StatData::default());
                let new_tran = &mut new_tran_r.lock().unwrap();
                for (k, v) in &self.trans_cumm.cnt.0 {
                    new_tran.cnt.0[k] = v / self.skip_days as u32;
                }
                for (k, v) in &self.trans_cumm.cnt.1 {
                    new_tran.cnt.1[k] = v / self.skip_days as u32;
                }
                new_tran.p_rate = self.trans_cumm.p_rate / self.skip_days as f64;
                new_tran.next = self.transit.clone();
                let new_tran_r_opt = Some(new_tran_r.clone());
                self.transit = new_tran_r_opt.clone();

                if self.days / self.skip_days >= MAX_N_REC {
                    for pr in MyIter::new(new_tran_r_opt) {
                        let p = &mut pr.lock().unwrap();
                        if let Some(qr) = p.next.clone() {
                            let q = &mut qr.lock().unwrap();
                            for (k, v) in &mut p.cnt.0 {
                                *v = (*v + q.cnt.0[k]) / 2;
                            }
                            for (k, v) in &mut p.cnt.1 {
                                *v = (*v + q.cnt.1[k]) / 2;
                            }
                            p.p_rate = (p.p_rate + q.p_rate) / 2.;
                            p.next = q.next.clone();
                            self.ds.restore(&mut q.next, qr.clone());
                        }
                    }
                    self.skip_days *= 2;
                }
            }
        }
        for info in infectors {
            let nv = info.new_v as usize;
            if self.n_infects_hist.len() < nv + 1 {
                let n = nv + 1 - self.n_infects_hist.len();
                for _ in 0..n {
                    self.n_infects_hist.push(MyCounter::new());
                }
            }
            if info.org_v >= 0 {
                self.n_infects_hist[info.org_v as usize].dec();
            };
            self.n_infects_hist[nv].inc();
        }

        match &self.statistics {
            Some(sr) => {
                let s = sr.lock().unwrap();
                s.cnt.0[HealthType::Asymptomatic] + s.cnt.0[HealthType::Symptomatic] == 0
            }
            _ => false,
        }
    }
    fn debug_show(&self) {
        StatInfo::debug_show_stat(&self.stat_cumm, "stat_cumm");
        StatInfo::debug_show_stat(&self.trans_cumm, "trans_cumm");
        StatInfo::debug_show_stat(&self.trans_daily, "trans_daily");
        StatInfo::debug_show_statistics(&self.statistics);
    }
    fn debug_show_statistics(stat: &Option<MRef<StatData>>) {
        if let Some(sr) = &stat {
            let s = sr.lock().unwrap();
            StatInfo::debug_show_stat(&s, "statistics");
        } else {
            println!("-")
        }
    }
    fn debug_show_stat(stat: &StatData, name: &str) {
        println!("----{}----", name);
        println!(" {:?}", stat.cnt.0);
        println!(" {:?}", stat.cnt.1);
        println!(" {}", stat.p_rate);
    }
}

fn count_health(a: &mut Agent, stat: &mut StatData, tran: &mut StatData) {
    if a.health != a.new_health {
        a.health = a.new_health;
        tran.cnt.0[a.health] += 1;
    }
    stat.cnt.0[a.health] += 1;
}

fn calc_positive_rate(count: &EnumMap<TestType, u32>) -> f64 {
    let tt: u32 = count[TestType::TestPositive] + count[TestType::TestNegative];
    if tt == 0 {
        0.0
    } else {
        (count[TestType::TestPositive] as f64) / (tt as f64)
    }
}
