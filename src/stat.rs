use crate::common_types::*;
use crate::world::*;

use enum_map::EnumMap;

pub struct InfectionCntInfo {
    pub org_v: i32,
    pub new_v: i32,
}

pub struct TimeEvoInfo {
    idx_bits: i32,
    n_indexes: i32,
    window_size: i32,
}

#[derive(Default, Debug)]
pub struct TestResultCount {
    pub positive: i32,
    pub negative: i32,
}

#[derive(Default)]
pub struct StatInfo {
    world: Box<World>,
    max_counts: EnumMap<HealthType, i32>,
    max_transit: EnumMap<HealthType, i32>,
    pop_size: i32,
    steps: i32,
    skip: i32,
    days: i32,
    skip_days: i32,
    stat_cumm: StatData,
    trans_daily: StatData,
    trans_cumm: StatData,
    test_cumm: [u32; N_INT_TEST_TYPES as usize],
    test_results_w: [TestResultCount; 7],
    test_result_cnt: TestResultCount,
    max_step_p_rate: f64,
    max_daily_p_rate: f64,
    p_rate_cumm: f64,
    phase_info: Vec<i32>,
    scenario_phases: Vec<i32>,
    statistics: StatData,
    transit: StatData,
}

impl StatInfo {
    pub fn reset(&mut self, n_pop: i32, n_init_infec: i32) {
        self.p_rate_cumm = 0.;
        self.max_step_p_rate = 0.;
        self.max_daily_p_rate = 0.;
        self.test_result_cnt = TestResultCount {
            positive: 0,
            negative: 0,
        };
        self.statistics.cnt[HealthType::Susceptible] = n_pop - n_init_infec;
        self.max_counts[HealthType::Susceptible] = n_pop - n_init_infec;
        self.statistics.cnt[HealthType::Asymptomatic] = n_init_infec;
        self.max_counts[HealthType::Asymptomatic] = n_pop - n_init_infec;
        self.steps = 0;
        self.days = 0;
        self.skip = 1;
        self.skip_days = 1;
        self.pop_size = n_pop;

        // ------
        // incub_p_hist.clear();
    }
}
