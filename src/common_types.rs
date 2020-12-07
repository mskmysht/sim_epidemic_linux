use enum_map::{Enum, EnumMap};

#[derive(Default, Debug)]
pub struct DistInfo {
    min: f64,
    max: f64,
    mode: f64,
}

#[derive(Default, Debug)]
pub struct RuntimeParams {
    pub mass: f64,
    pub friction: f64,
    pub avoidance: f64,
    // contagion delay and peak;
    pub contag_delay: f64,
    pub contag_peak: f64,
    // infection probability and distance
    pub infec: f64,
    pub infec_dst: f64,
    // Distancing strength and obedience
    pub dst_st: f64,
    pub dst_ob: f64,
    // Mobility frequency
    pub mob_fr: f64,
    // Gathering's frequency
    pub gat_fr: f64,
    // Contact tracing
    pub cntct_trc: f64,
    // test delay, process, interval, sensitivity, and specificity
    pub tst_delay: f64,
    pub tst_proc: f64,
    pub tst_interval: f64,
    pub tst_sens: f64,
    pub tst_spec: f64,
    // Subjects for test of asymptomatic, and symptomatic. contacts are tested 100%.
    pub tst_sbj_asy: f64,
    pub tst_sbj_sym: f64,
    // contagiousness, incubation, fatality, recovery, immunity
    // and distance
    pub incub: DistInfo,
    pub fatal: DistInfo,
    pub recov: DistInfo,
    pub immun: DistInfo,
    pub mob_dist: DistInfo,
    // Event gatherings: size, duration, strength
    pub gat_sz: DistInfo,
    pub gat_dr: DistInfo,
    pub gat_st: DistInfo,
    pub step: i32,
}

#[derive(Enum, Clone, Copy, PartialEq, Debug)]
pub enum HealthType {
    Susceptible,
    Asymptomatic,
    Symptomatic,
    Recovered,
    Died,
    QuarantineAsym,
    QuarantineSymp,
    NStateIndexes,
    // NHealthTypes = QuarantineAsym,
}

impl Default for HealthType {
    fn default() -> Self {
        HealthType::Susceptible
    }
}

pub enum TestType {
    TestTotal,
    TestAsSymptom,
    TestAsContact,
    TestAsSuspected,
    TestPositive,
    TestNegative,
    TestPositiveRate,
    NAllTestTypes,
}

pub const N_INT_TEST_TYPES: TestType = TestType::TestPositiveRate;
pub const N_INT_INDEXES: usize = HealthType::NStateIndexes as usize + N_INT_TEST_TYPES as usize;
pub const N_ALL_INDEXES: usize =
    HealthType::NStateIndexes as usize + TestType::NAllTestTypes as usize;

#[derive(Default, PartialEq, Debug)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Default, Debug)]
pub struct Range {
    pub length: i32,
    pub location: i32,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct WorldParams {
    pub init_pop: i32,
    pub world_size: i32,
    pub mesh: i32,
    pub n_init_infec: i32,
    pub steps_per_day: i32,
}

#[derive(Debug)]
pub enum WarpType {
    WarpInside,
    WarpToHospital,
    WarpToCemeteryF,
    WarpToCemeteryH,
    WarpBack,
}

impl Default for WarpType {
    fn default() -> Self {
        WarpType::WarpInside
    }
}

#[derive(Default, Debug)]
pub struct StatData {
    next: Box<StatData>,
    pub cnt: EnumMap<HealthType, i32>, // [i32; N_INT_INDEXES],
    p_rate: f64,
}

#[derive(Clone, Copy, Debug)]
pub enum LoopMode {
    LoopNone,
    LoopRunning,
    LoopFinished,
    LoopEndByUser,
    LoopEndByCondition,
    LoopEndAsDaysPassed,
    LoopEndByTimeLimit,
}

impl Default for LoopMode {
    fn default() -> Self {
        LoopMode::LoopNone
    }
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

    fn dec(&mut self) {
        self.cnt -= 1;
    }
}
