use crate::util::{
    enum_map::Enum,
    math::{self, Percentage, Permille, Point},
    random::DistInfo,
};

use table::TableIndex;

use rand::Rng;

#[derive(Debug)]
pub struct RuntimeParams {
    pub mass: Percentage,
    pub friction: Percentage,
    pub avoidance: f64,
    pub max_speed: f64,
    /// activeness as individuality
    pub act_mode: Percentage,
    pub act_kurt: Percentage,
    /// pub mass_act: f64,
    pub mob_act: Percentage,
    /// bias for mility and gatherings
    pub gat_act: Percentage,
    pub incub_act: Percentage,
    pub fatal_act: Percentage,
    /// infection probability
    pub infec: Percentage,
    /// infection distance
    pub infec_dst: f64,
    /// contagion delay
    pub contag_delay: f64,
    /// contagion peak
    pub contag_peak: f64,
    pub incub: DistInfo<f64>,
    pub fatal: DistInfo<f64>,
    pub therapy_effc: Percentage,
    pub imn_max_dur: f64,
    pub imn_max_dur_sv: Percentage,
    pub imn_max_effc: Percentage,
    pub imn_max_effc_sv: Percentage,

    /// Distancing strength
    pub dst_st: f64,
    /// Distancing obedience
    pub dst_ob: Percentage,
    /// Participation frequency in long travel
    pub mob_freq: DistInfo<Permille>,
    pub mob_dist: DistInfo<Percentage>,
    pub back_hm_rt: Percentage,

    /// Gathering's frequency
    pub gat_fr: f64,
    /// Gathering's random spot rate (%)
    pub gat_rnd_rt: Percentage,
    /// gathering's size
    pub gat_sz: DistInfo<f64>,
    /// gathering's duration
    pub gat_dr: DistInfo<f64>,
    /// gathering's strength
    pub gat_st: DistInfo<f64>,
    /// Participation frequency in gathering
    pub gat_freq: DistInfo<Percentage>,
    /// Contact tracing
    pub cntct_trc: Percentage,

    pub tst_delay: f64,
    /// test process
    pub tst_proc: f64,
    pub tst_interval: f64,
    /// test sensitivity
    pub tst_sens: Percentage,
    /// test specificity
    pub tst_spec: Percentage,
    /// Subjects for test of asymptomatic. contacts are tested 100%.
    pub tst_sbj_asy: Percentage,
    /// Subjects for test of symptomatic. contacts are tested 100%.
    pub tst_sbj_sym: Percentage,
    /// Test capacity (per 1,000 persons per day)
    pub tst_capa: Permille,
    /// Test delay limit (days)
    pub tst_dly_lim: f64,
    //[todo] pub trc_ope: TracingOperation, // How to treat the contacts, tests or vaccination, or both
    //[todo] pub trc_vcn_type: u32, // vaccine type for tracing vaccination
    pub step: u64,
    //[todo] pub recov: DistInfo<f64>,
    //[todo] pub immun: DistInfo<f64>,
    //[todo] pub vcn_p_rate: f64,
}

#[derive(Eq, Hash, Enum, Clone, Copy, PartialEq, Debug)]
pub enum HealthType {
    Susceptible,
    Asymptomatic,
    Symptomatic,
    Recovered,
    Died,
    Vaccinated,
}

#[derive(Clone, Copy, Debug)]
pub struct WorldParams {
    pub init_n_pop: usize,
    pub field_size: usize,
    pub mesh: usize,
    pub steps_per_day: u64,
    pub infected: Percentage,
    pub recovered: Percentage,
    pub q_asymptomatic: Percentage,
    pub q_symptomatic: Percentage,
    pub wrk_plc_mode: WrkPlcMode,
    //[todo] pub av_clstr_rate: Percentage, // Anti-Vax
    //[todo] pub av_clstr_gran: Percentage, // Anti-Vax
    //[todo] pub av_test_rate: Percentage, // Anti-Vax
    pub rcv_bias: Percentage,
    pub rcv_temp: f64,
    pub rcv_upper: Percentage,
    pub rcv_lower: Percentage,

    pub vcn_1st_effc: Percentage,
    pub vcn_max_effc: Percentage,
    pub vcn_effc_symp: Percentage,
    pub vcn_e_delay: f64,
    pub vcn_e_period: f64,
    pub vcn_e_decay: f64,
    pub vcn_sv_effc: Percentage,
    _init_n_pop: f64,
    _field_size: f64,
    _mesh: f64,
    _steps_per_day: f64,
    _days_per_step: f64,
    _res_rate: f64,
}

impl WorldParams {
    pub fn new(
        init_n_pop: usize,
        field_size: usize,
        mesh: usize,
        steps_per_day: u64,
        infected: Percentage,
        recovered: Percentage,
        q_asymptomatic: Percentage,
        q_symptomatic: Percentage,
        wrk_plc_mode: WrkPlcMode,
        rcv_bias: Percentage,
        rcv_temp: f64,
        rcv_upper: Percentage,
        rcv_lower: Percentage,
        vcn_1st_effc: Percentage,
        vcn_max_effc: Percentage,
        vcn_effc_symp: Percentage,
        vcn_e_delay: f64,
        vcn_e_period: f64,
        vcn_e_decay: f64,
        vcn_sv_effc: Percentage,
    ) -> Self {
        let _field_size = field_size as f64;
        let _mesh = mesh as f64;
        let _steps_per_day = steps_per_day as f64;
        Self {
            init_n_pop,
            field_size,
            mesh,
            infected,
            recovered,
            q_asymptomatic,
            q_symptomatic,
            steps_per_day,
            wrk_plc_mode,
            vcn_effc_symp,
            vcn_sv_effc,
            vcn_e_delay,
            vcn_1st_effc,
            vcn_max_effc,
            vcn_e_period,
            vcn_e_decay,
            _init_n_pop: init_n_pop as f64,
            _field_size,
            _mesh,
            _steps_per_day,
            _days_per_step: 1.0 / _steps_per_day,
            _res_rate: _mesh / _field_size,
            rcv_upper,
            rcv_lower,
            rcv_bias,
            rcv_temp,
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
    pub fn init_n_pop(&self) -> f64 {
        self._init_n_pop
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

    pub fn random_point(&self) -> Point {
        Point::new(
            rand::thread_rng().gen::<f64>() * self.field_size(),
            rand::thread_rng().gen::<f64>() * self.field_size(),
        )
    }

    pub fn centered_point(&self) -> Point {
        let mut p = Point::new(
            rand::thread_rng().gen::<f64>(),
            rand::thread_rng().gen::<f64>(),
        );
        p.apply_mut(|c| *c = *c * 2.0 - 1.0);
        let v = p.centered_bias();
        p.apply_mut(|c| *c = (*c * v + 1.0) * 0.5 * self.field_size());
        p
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum WrkPlcMode {
    WrkPlcNone,
    WrkPlcUniform,
    WrkPlcCentered,
    //[todo] WrkPlcPopDistImg,
}

pub struct VariantInfo {
    pub reproductivity: f64,
    pub toxicity: f64,
    pub efficacy: Vec<f64>,
}

impl VariantInfo {
    fn new(reproductivity: f64, toxicity: f64, efficacy: Vec<f64>) -> Self {
        Self {
            reproductivity,
            toxicity,
            efficacy,
        }
    }

    pub fn default_list() -> Vec<Self> {
        vec![VariantInfo::new(1.0, 1.0, vec![1.0])]
    }
}

pub struct VaccineInfo {
    pub interval: usize,
    pub efficacy: Vec<f64>,
}

impl VaccineInfo {
    fn new(interval: usize, efficacy: Vec<f64>) -> Self {
        Self { interval, efficacy }
    }

    pub fn default_list() -> Vec<Self> {
        vec![VaccineInfo::new(21, vec![1.0])]
    }
}

pub struct ParamsForStep<'a> {
    pub wp: &'a WorldParams,
    pub rp: &'a RuntimeParams,
    pub vr_info: &'a [VariantInfo],
    pub vx_info: &'a [VaccineInfo],
    go_home_back: bool,
}

impl<'a> ParamsForStep<'a> {
    pub fn new(
        wp: &'a WorldParams,
        rp: &'a RuntimeParams,
        vr_info: &'a [VariantInfo],
        vx_info: &'a [VaccineInfo],
    ) -> Self {
        ParamsForStep {
            rp,
            wp,
            vr_info,
            vx_info,
            go_home_back: wp.wrk_plc_mode != WrkPlcMode::WrkPlcNone && Self::is_daytime(wp, rp),
        }
    }

    pub fn go_home_back(&self) -> bool {
        self.go_home_back
        //[todo] wp.wrk_plc_mode != WrkPlcMode::WrkPlcNone && self.is_daytime()
    }

    fn is_daytime(wp: &WorldParams, rp: &RuntimeParams) -> bool {
        if wp.steps_per_day < 3 {
            rp.step % 2 == 0
        } else {
            rp.step % wp.steps_per_day < wp.steps_per_day * 2 / 3
        }
    }
}
