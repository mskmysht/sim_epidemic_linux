use crate::{
    util::enum_map::EnumMap,
    world::{
        commons::HealthType,
        testing::{TestReason, TestResult},
    },
};

// const IMG_WIDTH: i32 = 320 * 4;
// const IMG_HEIGHT: i32 = 320;
// const MAX_N_REC: i32 = IMG_WIDTH;

pub type UnionMap<K0, K1, V> = (EnumMap<K0, V>, EnumMap<K1, V>);

#[derive(Default, Debug)]
pub struct StatData {
    pub cnt: UnionMap<HealthType, TestReason, u32>,
    pub p_rate: f64,
}

pub struct MyCounter {
    cnt: i32,
}

impl MyCounter {
    pub fn new() -> MyCounter {
        MyCounter { cnt: 0 }
    }

    pub fn inc(&mut self) {
        self.cnt += 1;
    }

    pub fn dec(&mut self) {
        self.cnt -= 1;
    }

    // pub fn description(&self) -> String {
    //     format!("<MyCounter: cnt={}>", self.cnt)
    // }
}

#[derive(Default)]
pub struct Hist {
    pub recov_p: Vec<MyCounter>,
    pub incub_p: Vec<MyCounter>,
    pub death_p: Vec<MyCounter>,
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

// pub const N_INT_TEST_TYPES: TestType = TestType::TestPositiveRate;
// pub const N_INT_INDEXES: usize = HealthType::NStateIndexes as usize + N_INT_TEST_TYPES as usize;
// pub const N_ALL_INDEXES: usize =
//     HealthType::NStateIndexes as usize + TestType::NAllTestTypes as usize;

pub struct StatInfo {
    // max_counts: UnionMap<HealthType, TestReason, u32>,
    // max_transit: UnionMap<HealthType, TestReason, u32>,
    // pop_size: usize,
    // steps: i32,
    // skip: i32,
    // days: i32,
    // skip_days: i32,
    // stat_cumm: StatData,
    // trans_daily: StatData,
    // trans_cumm: StatData,
    // test_cumm: EnumMap<TestReason, u32>,
    // test_results_w: [TestResultCount; 7],
    // test_result_cnt: TestResultCount,
    // max_step_p_rate: f64,
    // max_daily_p_rate: f64,
    // p_rate_cumm: f64,
    // // phase_info: Vec<i32>,
    // n_infects_hist: Vec<MyCounter>,
    // // scenario_phases: Vec<i32>,
    // statistics: VecDeque<StatData>,
    // transit: VecDeque<StatData>,
    // // ds: DynStruct<StatData>,
    // hist_map: EnumMap<HistgramType, Vec<MyCounter>>,
    asymptomatic_count: usize,
    symptomatic_count: usize,
}

impl StatInfo {
    pub fn new() -> StatInfo {
        StatInfo {
            // max_counts: UnionMap::default(),
            // max_transit: UnionMap::default(),
            // pop_size: 0,
            // steps: 0,
            // skip: 0,
            // days: 0,
            // skip_days: 0,
            // stat_cumm: StatData::default(),
            // trans_daily: StatData::default(),
            // trans_cumm: StatData::default(),
            // test_cumm: EnumMap::default(),
            // test_results_w: [TestResultCount::default(); 7],
            // test_result_cnt: TestResultCount::default(),
            // max_step_p_rate: 0.,
            // max_daily_p_rate: 0.,
            // p_rate_cumm: 0.,
            // // phase_info: vec![],
            // n_infects_hist: vec![],
            // // scenario_phases: vec![],
            // statistics: Default::default(),
            asymptomatic_count: 0,
            symptomatic_count: 0,
            // transit: Default::default(),
            // // ds: Default::default(),
            // hist_map: Default::default(),
        }
    }

    pub fn reset(&mut self, n_pop: usize) {
        self.asymptomatic_count = 0;
        self.symptomatic_count = 0;

        // self.p_rate_cumm = 0.;
        // self.max_step_p_rate = 0.;
        // self.max_daily_p_rate = 0.;
        // self.test_result_cnt = TestResultCount {
        //     positive: 0,
        //     negative: 0,
        // };
        // let n_not_inf = (n_pop - n_init_infec) as u32;
        // self.max_counts.0[HealthType::Susceptible] = n_not_inf;
        // self.max_counts.0[HealthType::Asymptomatic] = n_not_inf;
        // self.ds.restore_all(&mut self.transit);
        // self.ds.restore_all(&mut self.statistics);
        // self.statistics = Default::default();
        // {
        //     let sr = self.ds.new();
        //     let s = &mut sr.lock().unwrap();
        //     s.cnt.0[HealthType::Susceptible] = n_not_inf;
        //     s.cnt.0[HealthType::Asymptomatic] = n_init_infec as u32;
        //     self.statistics.push_front(sr.clone());
        // }
        // self.steps = 0;
        // self.days = 0;
        // self.skip = 1;
        // self.skip_days = 1;
        // self.pop_size = n_pop;

        // incub_p_hist.clear();
    }

    // pub fn calc(&mut self, count_result: EnumMap<TestResult, u64>) -> bool {
    //     self.asymptomatic_count += 1;
    //     self.symptomatic_count += 1;
    // }

    /*
    pub fn calc_stat_with_test(&self, count_result: EnumMap<TestResult, u64>) {
        let k = (0..5).zip(0..5);
        for (a, b) in count_result.iter() {}

        {
            let mut new_stat = StatData::default();
            for (k, v) in &self.stat_cumm.cnt.0 {
                new_stat.cnt.0[k] = v / self.skip as u32;
            }
            for (k, v) in &self.stat_cumm.cnt.1 {
                new_stat.cnt.1[k] = v / self.skip as u32;
            }

            new_stat.p_rate = self.stat_cumm.p_rate / (self.skip as f64);
        }
    }

    fn stat_with_test(
        // w: &World,
        wp: &WorldParams,
        rp: &RuntimeParams,
        step_log: StepLog,
        // healths: Vec<HealthInfo>,
        // hosptital_healths: Vec<HealthInfo>,
        // step_log: &mut StepLog,
    ) -> bool {
        let mut tmp_stat = StatData::default();
        if rp.step % wp.steps_per_day == 0 {
            self.trans_daily = StatData::default();
        }
        // self.steps += 1;

        // infectors.into_par_iter().for_each(|a| {});

        // count health
        let (stat, tran) = healths
            .iter()
            .map(|(_, h)| {
                let mut stat: EnumMap<HealthType, usize> = Default::default();
                let mut tran: EnumMap<HealthType, usize> = Default::default();
                match *h {
                    HealthInfo::Stat(ht) => {
                        stat[ht] += 1;
                    }
                    HealthInfo::Tran(ht) => {
                        stat[ht] += 1;
                        tran[ht] += 1;
                    }
                };
                (stat, tran)
            })
            .reduce(
                || (Default::default(), Default::default()),
                |mut a, b| {
                    for (k, v) in &mut a.0 {
                        *v += b.0[*k];
                    }
                    for (k, v) in &mut a.1 {
                        *v += b.1[*k];
                    }
                    a
                },
            );

        let (stat, tran) =
            hosptital_healths
                .into_iter()
                .fold((stat, tran), |mut a @ (stat, tran), h| {
                    match h {
                        HealthInfo::Stat(ht) => {
                            stat[ht] += 1;
                        }
                        HealthInfo::Tran(ht) => {
                            stat[ht] += 1;
                            tran[ht] += 1;
                        }
                    };
                    a
                });

        // for ar in q_list {
        //     let a = &mut ar.lock().unwrap();
        //     let q_idx = if a.health == HealthType::Symptomatic {
        //         HealthType::QuarantineSymp
        //     } else {
        //         HealthType::QuarantineAsym
        //     };
        //     if a.got_at_hospital {
        //         self.trans_daily.cnt.0[q_idx] += 1;
        //         a.got_at_hospital = false;
        //     } else if a.health == HealthType::Asymptomatic
        //         && a.new_health == HealthType::Symptomatic
        //     {
        //         self.trans_daily.cnt.0[HealthType::QuarantineSymp] += 1;
        //     }
        //     count_health(a, &mut tmp_stat, &mut self.trans_daily);
        //     tmp_stat.cnt.0[q_idx] += 1;
        // }

        // for info in warp {
        //     count_health(
        //         &mut info.agent.lock().unwrap(),
        //         &mut tmp_stat,
        //         &mut self.trans_daily,
        //     );
        // }
        // for ar in c_list {
        //     let a = &mut ar.lock().unwrap();
        //     count_health(a, &mut tmp_stat, &mut self.trans_daily);
        // }

        for (k, v) in test_count {
            trans_daily.cnt.1[*k] += v;
            test_cumm[*k] += v;
            tmp_stat.cnt.1[*k] = test_cumm[*k];
        }
        tmp_stat.p_rate = calc_positive_rate(test_count);

        for (k, v) in &mut self.max_counts.0 {
            let c = tmp_stat.cnt.0[k];
            if *v < c {
                *v = c;
            }
        }
        for (k, v) in max_counts.1 {
            let c = tmp_stat.cnt.1[k];
            if *v < c {
                *v = c;
            }
        }
        if max_step_p_rate < tmp_stat.p_rate {
            max_step_p_rate = tmp_stat.p_rate;
        }

        let idx_in_cum = steps % skip;
        if idx_in_cum == 0 {
            stat_cumm = StatData::default();
        }
        for (k, v) in &tmp_stat.cnt.0 {
            stat_cumm.cnt.0[k] += v;
        }
        for (k, v) in &tmp_stat.cnt.1 {
            stat_cumm.cnt.1[k] += v;
        }
        stat_cumm.p_rate = tmp_stat.p_rate;
        hoge();

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

        match self.statistics.front() {
            Some(sr) => {
                let s = sr.lock().unwrap();
                s.cnt.0[HealthType::Asymptomatic] + s.cnt.0[HealthType::Symptomatic] == 0
            }
            _ => false,
        }
    }

    fn hoge() {
        if idx_in_cum + 1 >= skip {
            {
                let mut new_stat = nsr.lock().unwrap();
                for (k, v) in &self.stat_cumm.cnt.0 {
                    new_stat.cnt.0[k] = v / self.skip as u32;
                }
                for (k, v) in &self.stat_cumm.cnt.1 {
                    new_stat.cnt.1[k] = v / self.skip as u32;
                }

                new_stat.p_rate = self.stat_cumm.p_rate / (self.skip as f64);
            }

            self.statistics.push_front(nsr.clone());
            if self.steps / self.skip > MAX_N_REC {
                let mut new_list = VecDeque::new();
                loop {
                    if let Some(pr) = &self.statistics.pop_front() {
                        if let Some(qr) = &self.statistics.pop_front() {
                            let p = &mut pr.lock().unwrap();
                            let q = &mut qr.lock().unwrap();
                            for (k, v) in &mut p.cnt.0 {
                                *v = (*v + q.cnt.0[k]) / 2;
                            }
                            for (k, v) in &mut p.cnt.1 {
                                *v = (*v + q.cnt.1[k]) / 2;
                            }
                            p.p_rate = (p.p_rate + q.p_rate) / 2.;
                            self.ds.restore(qr.clone());
                        }
                        new_list.push_back(pr.clone());
                    } else {
                        break;
                    }
                }
                self.statistics = new_list;
                self.skip *= 2;
            }
        }
        if self.steps % steps_per_day == steps_per_day - 1 {
            let daily_tests = &self.trans_daily.cnt.1;
            self.trans_daily.p_rate = calc_positive_rate(&daily_tests);
            if self.days < 7 {
                let dtp = daily_tests[TestReason::TestPositive] as i32;
                let dtn = daily_tests[TestReason::TestNegative] as i32;
                self.test_results_w[self.days as usize].positive = dtp;
                self.test_results_w[self.days as usize].negative = dtn;
                self.test_result_cnt.positive += dtp;
                self.test_result_cnt.negative += dtn;
            } else {
                let idx = (self.days % 7) as usize;
                let dtp = daily_tests[TestReason::TestPositive] as i32;
                let dtn = daily_tests[TestReason::TestNegative] as i32;
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
                let new_tran_r = self.ds.new();
                {
                    let new_tran = &mut new_tran_r.lock().unwrap();
                    for (k, v) in &self.trans_cumm.cnt.0 {
                        new_tran.cnt.0[k] = v / self.skip_days as u32;
                    }
                    for (k, v) in &self.trans_cumm.cnt.1 {
                        new_tran.cnt.1[k] = v / self.skip_days as u32;
                    }
                    new_tran.p_rate = self.trans_cumm.p_rate / self.skip_days as f64;
                }
                self.transit.push_front(new_tran_r.clone());

                if self.days / self.skip_days >= MAX_N_REC {
                    let mut new_list = VecDeque::new();
                    loop {
                        if let Some(pr) = &self.transit.pop_front() {
                            if let Some(qr) = &self.transit.pop_front() {
                                let p = &mut pr.lock().unwrap();
                                let q = &mut qr.lock().unwrap();
                                for (k, v) in &mut p.cnt.0 {
                                    *v = (*v + q.cnt.0[k]) / 2;
                                }
                                for (k, v) in &mut p.cnt.1 {
                                    *v = (*v + q.cnt.1[k]) / 2;
                                }
                                p.p_rate = (p.p_rate + q.p_rate) / 2.;
                                self.ds.restore(qr.clone());
                            }
                            new_list.push_back(pr.clone());
                        } else {
                            break;
                        }
                    }
                    self.transit = new_list;
                    self.skip_days *= 2;
                }
            }
        }
    }
    */

    /*
    pub fn calc_stat_with_test(
        &mut self,
        // w: &World,
        wp: &WorldParams,
        rp: &RuntimeParams,
        // healths: Vec<HealthInfo>,
        // hosptital_healths: Vec<HealthInfo>,
        step_log: &mut StepLog,
    ) -> bool {
        let mut tmp_stat = StatData::default();
        if self.steps % wp.steps_per_day == 0 {
            self.trans_daily = StatData::default();
        }
        self.steps += 1;

        // infectors.into_par_iter().for_each(|a| {});

        // count health
        let (stat, tran) = healths
            .iter()
            .map(|(_, h)| {
                let mut stat: EnumMap<HealthType, usize> = Default::default();
                let mut tran: EnumMap<HealthType, usize> = Default::default();
                match *h {
                    HealthInfo::Stat(ht) => {
                        stat[ht] += 1;
                    }
                    HealthInfo::Tran(ht) => {
                        stat[ht] += 1;
                        tran[ht] += 1;
                    }
                };
                (stat, tran)
            })
            .reduce(
                || (Default::default(), Default::default()),
                |mut a, b| {
                    for (k, v) in &mut a.0 {
                        *v += b.0[*k];
                    }
                    for (k, v) in &mut a.1 {
                        *v += b.1[*k];
                    }
                    a
                },
            );

        let (stat, tran) =
            hosptital_healths
                .into_iter()
                .fold((stat, tran), |mut a @ (stat, tran), h| {
                    match h {
                        HealthInfo::Stat(ht) => {
                            stat[ht] += 1;
                        }
                        HealthInfo::Tran(ht) => {
                            stat[ht] += 1;
                            tran[ht] += 1;
                        }
                    };
                    a
                });

        // for ar in q_list {
        //     let a = &mut ar.lock().unwrap();
        //     let q_idx = if a.health == HealthType::Symptomatic {
        //         HealthType::QuarantineSymp
        //     } else {
        //         HealthType::QuarantineAsym
        //     };
        //     if a.got_at_hospital {
        //         self.trans_daily.cnt.0[q_idx] += 1;
        //         a.got_at_hospital = false;
        //     } else if a.health == HealthType::Asymptomatic
        //         && a.new_health == HealthType::Symptomatic
        //     {
        //         self.trans_daily.cnt.0[HealthType::QuarantineSymp] += 1;
        //     }
        //     count_health(a, &mut tmp_stat, &mut self.trans_daily);
        //     tmp_stat.cnt.0[q_idx] += 1;
        // }

        // for info in warp {
        //     count_health(
        //         &mut info.agent.lock().unwrap(),
        //         &mut tmp_stat,
        //         &mut self.trans_daily,
        //     );
        // }
        // for ar in c_list {
        //     let a = &mut ar.lock().unwrap();
        //     count_health(a, &mut tmp_stat, &mut self.trans_daily);
        // }

        for (k, v) in test_count {
            self.trans_daily.cnt.1[*k] += v;
            self.test_cumm[*k] += v;
            tmp_stat.cnt.1[*k] = self.test_cumm[*k];
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

        if idx_in_cum + 1 >= self.skip {
            let nsr = self.ds.new();
            {
                let mut new_stat = nsr.lock().unwrap();
                for (k, v) in &self.stat_cumm.cnt.0 {
                    new_stat.cnt.0[k] = v / self.skip as u32;
                }
                for (k, v) in &self.stat_cumm.cnt.1 {
                    new_stat.cnt.1[k] = v / self.skip as u32;
                }

                new_stat.p_rate = self.stat_cumm.p_rate / (self.skip as f64);
            }

            self.statistics.push_front(nsr.clone());
            if self.steps / self.skip > MAX_N_REC {
                let mut new_list = VecDeque::new();
                loop {
                    if let Some(pr) = &self.statistics.pop_front() {
                        if let Some(qr) = &self.statistics.pop_front() {
                            let p = &mut pr.lock().unwrap();
                            let q = &mut qr.lock().unwrap();
                            for (k, v) in &mut p.cnt.0 {
                                *v = (*v + q.cnt.0[k]) / 2;
                            }
                            for (k, v) in &mut p.cnt.1 {
                                *v = (*v + q.cnt.1[k]) / 2;
                            }
                            p.p_rate = (p.p_rate + q.p_rate) / 2.;
                            self.ds.restore(qr.clone());
                        }
                        new_list.push_back(pr.clone());
                    } else {
                        break;
                    }
                }
                self.statistics = new_list;
                self.skip *= 2;
            }
        }
        if self.steps % steps_per_day == steps_per_day - 1 {
            let daily_tests = &self.trans_daily.cnt.1;
            self.trans_daily.p_rate = calc_positive_rate(&daily_tests);
            if self.days < 7 {
                let dtp = daily_tests[TestReason::TestPositive] as i32;
                let dtn = daily_tests[TestReason::TestNegative] as i32;
                self.test_results_w[self.days as usize].positive = dtp;
                self.test_results_w[self.days as usize].negative = dtn;
                self.test_result_cnt.positive += dtp;
                self.test_result_cnt.negative += dtn;
            } else {
                let idx = (self.days % 7) as usize;
                let dtp = daily_tests[TestReason::TestPositive] as i32;
                let dtn = daily_tests[TestReason::TestNegative] as i32;
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
                let new_tran_r = self.ds.new();
                {
                    let new_tran = &mut new_tran_r.lock().unwrap();
                    for (k, v) in &self.trans_cumm.cnt.0 {
                        new_tran.cnt.0[k] = v / self.skip_days as u32;
                    }
                    for (k, v) in &self.trans_cumm.cnt.1 {
                        new_tran.cnt.1[k] = v / self.skip_days as u32;
                    }
                    new_tran.p_rate = self.trans_cumm.p_rate / self.skip_days as f64;
                }
                self.transit.push_front(new_tran_r.clone());

                if self.days / self.skip_days >= MAX_N_REC {
                    let mut new_list = VecDeque::new();
                    loop {
                        if let Some(pr) = &self.transit.pop_front() {
                            if let Some(qr) = &self.transit.pop_front() {
                                let p = &mut pr.lock().unwrap();
                                let q = &mut qr.lock().unwrap();
                                for (k, v) in &mut p.cnt.0 {
                                    *v = (*v + q.cnt.0[k]) / 2;
                                }
                                for (k, v) in &mut p.cnt.1 {
                                    *v = (*v + q.cnt.1[k]) / 2;
                                }
                                p.p_rate = (p.p_rate + q.p_rate) / 2.;
                                self.ds.restore(qr.clone());
                            }
                            new_list.push_back(pr.clone());
                        } else {
                            break;
                        }
                    }
                    self.transit = new_list;
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

        match self.statistics.front() {
            Some(sr) => {
                let s = sr.lock().unwrap();
                s.cnt.0[HealthType::Asymptomatic] + s.cnt.0[HealthType::Symptomatic] == 0
            }
            _ => false,
        }
    }
    */

    // pub fn debug_show(&self) {
    //     StatInfo::debug_show_all_stat(&self.statistics, "statistics");
    // }
}

fn calc_positive_rate(count: &EnumMap<TestResult, u32>) -> f64 {
    let tt: u32 = count[&TestResult::Positive] + count[&TestResult::Negative];
    if tt == 0 {
        0.0
    } else {
        (count[&TestResult::Positive] as f64) / (tt as f64)
    }
}
