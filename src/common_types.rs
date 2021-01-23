use std::sync::{Arc, Mutex};

use crate::{
    agent::Agent,
    enum_map::{Enum, EnumMap},
    iter::Next,
};

pub type MRef<T> = Arc<Mutex<T>>;

#[derive(Default, Debug)]
pub struct DistInfo {
    pub min: f64,
    pub mode: f64,
    pub max: f64,
}

impl DistInfo {
    pub fn new(min: f64, mode: f64, max: f64) -> DistInfo {
        DistInfo { min, mode, max }
    }
}

#[derive(Debug)]
pub struct RuntimeParams {
    pub mass: f64,
    pub friction: f64,
    pub avoidance: f64,
    // contagion delay and peak;
    pub contag_delay: f64,
    pub contag_peak: f64,
    // infection probability (%) and distance
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

#[derive(Eq, Hash, Enum, Clone, Copy, PartialEq, Debug)]
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

#[derive(Eq, PartialEq, Hash, Copy, Clone, Enum, Debug)]
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

pub type UnionMap<K0, K1, V> = (EnumMap<K0, V>, EnumMap<K1, V>);

/*
#[derive(Eq, PartialEq, Hash, Debug)]
pub enum HealthOrTest {
    HealthType(HealthType),
    TestType(TestType),
}

impl HealthType {
    pub fn to_stat(self) -> HealthOrTest {
        HealthOrTest::HealthType(self)
    }
}

impl TestType {
    pub fn to_stat(self) -> HealthOrTest {
        HealthOrTest::TestType(self)
    }
}
*/

pub const N_INT_TEST_TYPES: TestType = TestType::TestPositiveRate;
pub const N_INT_INDEXES: usize = HealthType::NStateIndexes as usize + N_INT_TEST_TYPES as usize;
pub const N_ALL_INDEXES: usize =
    HealthType::NStateIndexes as usize + TestType::NAllTestTypes as usize;

#[derive(Default, PartialEq, Clone, Copy, Debug)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Default, Clone, Debug)]
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

#[derive(Clone, Copy, Debug)]
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
    pub next: Option<MRef<StatData>>,
    pub cnt: UnionMap<HealthType, TestType, u32>, // [u32; N_INT_INDEXES],
    pub p_rate: f64,
}

impl StatData {
    pub fn reset(&mut self) {
        self.p_rate = 0.0;
        self.cnt = Default::default();
        self.next = None;
    }
}

impl Next<MRef<StatData>> for MRef<StatData> {
    fn next(&self) -> Option<MRef<StatData>> {
        self.lock().unwrap().next.clone()
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
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

    pub fn dec(&mut self) {
        self.cnt -= 1;
    }

    pub fn description(&self) -> String {
        format!("<MyCounter: cnt={}>", self.cnt)
    }
}

#[derive(Default)]
pub struct TestEntry {
    pub prev: Option<MRef<TestEntry>>,
    pub next: Option<MRef<TestEntry>>,
    pub time_stamp: i32,
    pub is_positive: bool,
    pub agent: Option<MRef<Agent>>,
}

impl Next<MRef<TestEntry>> for MRef<TestEntry> {
    fn next(&self) -> Option<MRef<TestEntry>> {
        self.lock().unwrap().next.clone()
    }
}
