pub mod container;
pub mod math;
pub mod random;

use crate::{
    dyn_struct::Reset,
    enum_map::{Enum, EnumMap},
    testing::TestReason,
};
use math::Percentage;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, Weak},
};

use self::{
    container::table::TableIndex,
    math::{Permille, Point},
};

pub type MRef<T> = Arc<Mutex<T>>;
pub type WRef<T> = Weak<Mutex<T>>;

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
    pub mass: Percentage,
    pub friction: Percentage,
    pub avoidance: f64,
    // activeness as individuality
    pub act_mode: f64,
    pub act_kurt: f64,
    // bias for mility and gatherings
    pub mass_act: f64,
    pub mob_act: f64,
    pub gat_act: f64,
    // contagion delay and peak;
    pub contag_delay: f64,
    pub contag_peak: f64,
    // infection probability (%) and distance
    pub infec: Percentage,
    pub infec_dst: f64,
    // Distancing strength and obedience
    pub dst_st: f64,
    pub dst_ob: f64,
    // Mobility frequency
    pub mob_fr: f64,
    // Gathering's frequency
    pub gat_fr: f64,
    // Contact tracing
    pub gat_rnd_rt: Percentage,
    pub cntct_trc: Percentage,
    // test delay, process, interval, sensitivity, and specificity
    pub tst_delay: f64,
    pub tst_proc: f64,
    pub tst_interval: f64,
    pub tst_sens: Percentage,
    pub tst_spec: Percentage,
    // Subjects for test of asymptomatic, and symptomatic. contacts are tested 100%.
    pub tst_sbj_asy: Percentage,
    pub tst_sbj_sym: Percentage,
    // Test capacity (per 1,000 persons per day), test delay limit (days)
    pub tst_capa: Permille,
    pub tst_dly_lim: f64,
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
    pub mob_freq: DistInfo, // Participation frequency in long travel
    pub gat_freq: DistInfo, // Participation frequency in gathering
    pub step: u64,
    pub vcn_p_rate: f64,
    pub therapy_effc: Percentage,
    pub back_hm_rt: Percentage,
    pub max_speed: f64,
}

#[derive(Eq, Hash, Enum, Clone, Copy, PartialEq, Debug)]
pub enum HealthType {
    Susceptible,
    Asymptomatic,
    Symptomatic,
    Recovered,
    Died,
    Vaccinated,
    // QuarantineAsym,
    // QuarantineSymp,
    // NStateIndexes,
    // NHealthTypes = QuarantineAsym,
}

impl Default for HealthType {
    fn default() -> Self {
        HealthType::Susceptible
    }
}

pub type UnionMap<K0, K1, V> = (EnumMap<K0, V>, EnumMap<K1, V>);

// pub const N_INT_TEST_TYPES: TestType = TestType::TestPositiveRate;
// pub const N_INT_INDEXES: usize = HealthType::NStateIndexes as usize + N_INT_TEST_TYPES as usize;
// pub const N_ALL_INDEXES: usize =
//     HealthType::NStateIndexes as usize + TestType::NAllTestTypes as usize;

#[derive(Clone, Copy, Debug)]
pub struct WorldParams {
    pub init_pop: usize,
    pub field_size: usize,
    pub mesh: usize,
    pub n_init_infec: usize,
    pub steps_per_day: u64,
    pub wrk_plc_mode: WrkPlcMode,
    pub vcn_effc_symp: Percentage,
    pub vcn_sv_effc: Percentage,
    pub vcn_e_delay: f64,
    pub vcn_1st_effc: Percentage,
    pub vcn_max_effc: Percentage,
    pub vcn_e_period: f64,
    pub vcn_e_decay: f64,
    _field_size: f64,
    _mesh: f64,
    _steps_per_day: f64,
    _days_per_step: f64,
    _res_rate: f64,
}

impl WorldParams {
    pub fn new(
        init_pop: usize,
        field_size: usize,
        mesh: usize,
        n_init_infec: usize,
        steps_per_day: u64,
        wrk_plc_mode: WrkPlcMode,
        vcn_effc_symp: Percentage,
        vcn_sv_effc: Percentage,
        vcn_e_delay: f64,
        vcn_1st_effc: Percentage,
        vcn_max_effc: Percentage,
        vcn_e_period: f64,
        vcn_e_decay: f64,
    ) -> Self {
        let _field_size = field_size as f64;
        let _mesh = mesh as f64;
        let _steps_per_day = steps_per_day as f64;
        Self {
            init_pop,
            field_size,
            mesh,
            n_init_infec,
            steps_per_day,
            wrk_plc_mode,
            vcn_effc_symp,
            vcn_sv_effc,
            vcn_e_delay,
            vcn_1st_effc,
            vcn_max_effc,
            vcn_e_period,
            vcn_e_decay,
            _field_size,
            _mesh,
            _steps_per_day,
            _days_per_step: 1.0 / _steps_per_day,
            _res_rate: _mesh / _field_size,
        }
    }
    #[inline]
    pub fn steps_per_day(&self) -> f64 {
        self._steps_per_day
    }

    #[inline]
    pub fn days_per_step(&self) -> f64 {
        self._days_per_step
    }

    #[inline]
    pub fn field_size(&self) -> f64 {
        self._field_size
    }

    #[inline]
    pub fn mesh(&self) -> f64 {
        self._mesh
    }

    #[inline]
    pub fn res_rate(&self) -> f64 {
        self._res_rate
    }

    #[inline]
    pub fn into_grid_index(&self, p: &Point) -> TableIndex {
        TableIndex::new(
            math::quantize(p.y, self.res_rate(), self.mesh),
            math::quantize(p.x, self.res_rate(), self.mesh),
        )
    }

    #[inline]
    pub fn view_range(&self) -> f64 {
        self._field_size / self._mesh
    }
}

pub fn go_home_back(wp: &WorldParams, rp: &RuntimeParams) -> bool {
    wp.wrk_plc_mode != WrkPlcMode::WrkPlcNone && is_daytime(wp, rp)
}

pub fn is_daytime(wp: &WorldParams, rp: &RuntimeParams) -> bool {
    if wp.steps_per_day < 3 {
        rp.step % 2 == 0
    } else {
        rp.step % wp.steps_per_day < wp.steps_per_day * 2 / 3
    }
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

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum WrkPlcMode {
    WrkPlcNone,
    WrkPlcUniform,
    WrkPlcCentered,
    WrkPlcPopDistImg,
}

#[derive(Default, Debug)]
pub struct StatData {
    pub cnt: UnionMap<HealthType, TestReason, u32>,
    pub p_rate: f64,
}

impl Reset<StatData> for StatData {
    fn reset(&mut self) {
        self.p_rate = 0.0;
        self.cnt = Default::default();
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
    // LoopEndByTimeLimit,
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

    // pub fn description(&self) -> String {
    //     format!("<MyCounter: cnt={}>", self.cnt)
    // }
}

pub trait PointerVec<T> {
    fn remove_p(&mut self, t: &Arc<T>);
}

impl<T> PointerVec<T> for Vec<Arc<T>> {
    fn remove_p(&mut self, t: &Arc<T>) {
        self.retain(|u| !Arc::ptr_eq(u, t));
    }
}

impl<T> PointerVec<T> for VecDeque<Arc<T>> {
    fn remove_p(&mut self, t: &Arc<T>) {
        self.retain(|u| !Arc::ptr_eq(u, t));
    }
}

pub enum Either<L, R> {
    Left(L),
    Right(R),
}

pub trait DrainMap<T, U, F: FnMut(&mut T) -> (bool, U)> {
    type Target;
    fn drain_map_mut(&mut self, f: F) -> Self::Target;
}

pub trait DrainLike<T, F: FnMut(&mut T) -> bool> {
    fn drain_mut(&mut self, f: F) -> Self;
}

impl<T, U, F: FnMut(&mut T) -> (bool, U)> DrainMap<T, U, F> for Vec<T> {
    type Target = Vec<U>;

    fn drain_map_mut(&mut self, mut f: F) -> Self::Target {
        let mut us = Vec::new();
        self.retain_mut(|v| {
            let (b, u) = f(v);
            us.push(u);
            b
        });
        us
    }
}

impl<T, F: FnMut(&mut T) -> bool> DrainLike<T, F> for Vec<T> {
    fn drain_mut(&mut self, f: F) -> Self {
        let is = self
            .iter_mut()
            .enumerate()
            .rev()
            .filter_map(|(i, v)| if f(v) { Some(i) } else { None })
            .collect::<Vec<_>>();
        is.into_iter().map(|i| self.swap_remove(i)).collect()
    }
}
