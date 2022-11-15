use crate::{
    commons::{
        math::{Percentage, Point},
        random::{self, modified_prob},
        DistInfo, Either, HealthType, RuntimeParams, WorldParams, WrkPlcMode,
    },
    contact::Contacts,
    gathering::Gathering,
    log::HealthDiff,
    stat::{HistInfo, HistgramType, InfectionCntInfo},
    table::TableIndex,
    testing::{TestReason, TestResult, Testee},
};
use health::AgentHealth;
use rand::{self, Rng};
use std::{
    f64,
    ops::ControlFlow,
    sync::{Arc, Mutex, Weak},
};

use self::health::{HealthState, InfMode};

const AGENT_RADIUS: f64 = 0.75;
//[todo] static AGENT_SIZE: f64 = 0.665;
const AVOIDANCE: f64 = 0.2;
const MAX_DAYS_FOR_RECOVERY: f64 = 7.0;
const TOXICITY_LEVEL: f64 = 0.5;

const BACK_HOME_RATE: bool = true;

pub struct VariantInfo {
    pub reproductivity: f64,
    toxicity: f64,
    efficacy: Vec<f64>,
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
    interval: usize,
    efficacy: Vec<f64>,
}

impl VaccineInfo {
    fn new(interval: usize, efficacy: Vec<f64>) -> Self {
        Self { interval, efficacy }
    }

    pub fn default_list() -> Vec<Self> {
        vec![VaccineInfo::new(21, vec![1.0])]
    }
}

fn exacerbation(reproductivity: f64) -> f64 {
    reproductivity.powf(1.0 / 3.0)
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

fn was_hit(days_per_step: f64, prob: f64) -> bool {
    rand::thread_rng().gen::<f64>() > (1.0 - prob).powf(days_per_step)
}

/*
fn cummulate_histgrm(h: &mut Vec<MyCounter>, d: f64) {
    let ds = d.floor() as usize;
    if h.len() <= ds {
        let n = ds - h.len();
        for _ in 0..=n {
            h.push(MyCounter::new());
        }
    }
    h[ds].inc();
}
*/

const HOMING_FORCE: f64 = 0.2;
const MAX_HOMING_FORCE: f64 = 2.0;
const MIN_AWAY_TO_HOME: Percentage = Percentage::new(50.0);
fn back_home_force(pt: &Point, origin: &Point) -> Option<Point> {
    let mut df = origin - pt;
    let fa = df.x.hypot(df.y);
    if fa > MIN_AWAY_TO_HOME.r() {
        return None;
    }
    if fa * HOMING_FORCE > MAX_HOMING_FORCE {
        df *= MAX_HOMING_FORCE / fa;
    }
    Some(df)
}

mod health {
    use super::{DaysTo, InfectionParam, ParamsForStep, RecoverParam, VaccinationParam, WarpParam};
    use crate::{
        commons::{math::Point, HealthType},
        log::HealthDiff,
        stat::HistInfo,
    };
    use rand::{self, Rng};
    use std::{f64, ops::ControlFlow};

    #[derive(Debug, PartialEq, Eq)]
    pub enum InfMode {
        Asym,
        Sym,
    }

    #[derive(Default)]
    pub enum HealthState {
        #[default]
        Susceptible,
        Infected(InfectionParam, InfMode),
        Recovered(RecoverParam),
        Vaccinated(VaccinationParam),
        Died,
    }

    impl From<&HealthState> for HealthType {
        fn from(p: &HealthState) -> Self {
            match p {
                HealthState::Susceptible => HealthType::Susceptible,
                HealthState::Infected(_, InfMode::Asym) => HealthType::Asymptomatic,
                HealthState::Infected(_, InfMode::Sym) => HealthType::Symptomatic,
                HealthState::Recovered(_) => HealthType::Recovered,
                HealthState::Vaccinated(_) => HealthType::Vaccinated,
                HealthState::Died => HealthType::Died,
            }
        }
    }

    #[derive(Default)]
    pub struct AgentHealth {
        state: HealthState,
        new_health: Option<HealthState>,
        vaccination: Option<VaccinationParam>,
        vaccine_ticket: Option<usize>,
    }

    impl AgentHealth {
        pub fn force_susceptible(&mut self) {
            self.new_health = None;
            self.state = HealthState::Susceptible;
        }

        pub fn force_infected(&mut self, days_to: &DaysTo) {
            let mut ip = InfectionParam::new(0.0, 0);
            ip.days_infected = rand::thread_rng().gen::<f64>() * days_to.recover.min(days_to.die);
            let d = ip.days_infected - days_to.onset;
            let inf_mode = if d >= 0.0 {
                ip.days_diseased = d;
                InfMode::Sym
            } else {
                InfMode::Asym
            };
            self.state = HealthState::Infected(ip, inf_mode);
            self.new_health = None;
        }

        pub fn force_recovered(&mut self, days_recovered: f64) {
            let mut rcp = RecoverParam::new(0.0, 0);
            rcp.days_recovered = days_recovered;
            self.state = HealthState::Recovered(rcp);
            self.new_health = None;
        }

        pub fn become_infected(&self) -> bool {
            matches!(
                self.new_health,
                Some(HealthState::Infected(_, InfMode::Asym))
            )
        }

        pub fn get_immune_factor(&self, bip: &InfectionParam, pfs: &ParamsForStep) -> Option<f64> {
            let immune_factor = match &self.state {
                HealthState::Susceptible => 0.0,
                HealthState::Recovered(rp) => {
                    rp.immunity * pfs.vr_info[rp.virus_variant].efficacy[bip.virus_variant]
                }
                HealthState::Vaccinated(vp) => {
                    vp.immunity * pfs.vx_info[vp.vaccine_type].efficacy[bip.virus_variant]
                }
                _ => return None,
            };
            Some(immune_factor)
        }

        pub fn get_immunity(&self) -> Option<f64> {
            match &self.state {
                HealthState::Susceptible => Some(0.0),
                HealthState::Infected(ip, InfMode::Asym) => Some(ip.immunity),
                HealthState::Vaccinated(vp) => Some(vp.immunity),
                _ => None,
            }
        }

        pub fn get_infected(&self) -> Option<&InfectionParam> {
            match &self.state {
                HealthState::Infected(ip, _) => Some(ip),
                _ => None,
            }
        }

        pub fn is_symptomatic(&self) -> bool {
            matches!(&self.state, HealthState::Infected(_, InfMode::Sym))
        }

        pub fn get_symptomatic(&self) -> Option<&InfectionParam> {
            match &self.state {
                HealthState::Infected(ip, InfMode::Sym) => Some(ip),
                _ => None,
            }
        }

        pub fn set_new_health(&mut self, new_health: Option<HealthState>) -> bool {
            if new_health.is_some() {
                self.new_health = new_health;
                true
            } else {
                false
            }
        }

        pub fn update(&mut self) -> Option<HealthDiff> {
            if let Some(new_health) = self.new_health.take() {
                let from = (&self.state).into();
                let to = (&new_health).into();

                let old = std::mem::replace(&mut self.state, new_health);
                match old {
                    HealthState::Vaccinated(vp) => {
                        self.vaccination = Some(vp);
                    }
                    _ => {}
                }
                Some(HealthDiff::new(from, to))
            } else {
                None
            }
        }

        pub fn field_step(
            &mut self,
            days_to: &mut DaysTo,
            activeness: f64,
            age: f64,
            hist: &mut Option<HistInfo>,
            pfs: &ParamsForStep,
        ) -> ControlFlow<WarpParam> {
            let new_health = self
                .try_vaccinate(days_to, pfs)
                .or_else(|| match &mut self.state {
                    HealthState::Infected(ip, inf_mode) => {
                        ip.step::<false>(days_to, &self.vaccination, pfs, hist, inf_mode)
                    }
                    HealthState::Recovered(rp) => rp.step(days_to, activeness, age, pfs),
                    HealthState::Vaccinated(vp) => vp.step(days_to, activeness, age, pfs),
                    _ => None,
                });

            self.set_new_health(new_health);
            match self.new_health {
                Some(HealthState::Died) => ControlFlow::Break(WarpParam::cemetery(pfs.wp)),
                _ => ControlFlow::Continue(()),
            }
        }

        pub fn hospital_step(
            &mut self,
            days_to: &mut DaysTo,
            origin: Point,
            hist: &mut Option<HistInfo>,
            pfs: &ParamsForStep,
        ) -> Option<WarpParam> {
            let new_health = match &mut self.state {
                HealthState::Infected(ip, inf_mode) => {
                    ip.step::<true>(days_to, &self.vaccination, pfs, hist, inf_mode)
                }
                _ => None,
            };

            self.set_new_health(new_health);
            match self.new_health {
                Some(HealthState::Died) => Some(WarpParam::cemetery(pfs.wp)),
                Some(HealthState::Recovered(..)) => Some(WarpParam::back(origin)),
                Some(_) => Some(WarpParam::back(origin)),
                _ => None,
            }
        }

        fn try_vaccinate(
            &mut self,
            days_to: &mut DaysTo,
            pfs: &ParamsForStep,
        ) -> Option<HealthState> {
            let vaccine_type = self.vaccine_ticket.take()?;
            let immunity = self.get_immunity()?;
            Some(HealthState::Vaccinated(self.vaccinate(
                days_to,
                immunity,
                vaccine_type,
                pfs,
            )))
        }

        fn vaccinate(
            &mut self,
            days_to: &mut DaysTo,
            immunity: f64,
            vaccine_type: usize,
            pfs: &ParamsForStep,
        ) -> VaccinationParam {
            let today = pfs.rp.step as f64 * pfs.wp.days_per_step();

            if let Some(mut vp) = self.vaccination.take() {
                if (vp.dose_date + pfs.vx_info[vaccine_type].interval as f64) < today {
                    // first done
                    days_to.update_recover(pfs.wp);
                }
                vp.dose_date = today;
                vp.vaccine_type = vaccine_type;
                vp
            } else {
                // first done
                days_to.update_recover(pfs.wp);
                VaccinationParam {
                    dose_date: today,
                    vaccine_type,
                    immunity,
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct InfectionParam {
    pub virus_variant: usize,
    on_recovery: bool,
    severity: f64,
    days_diseased: f64,
    days_infected: f64,
    immunity: f64,
}

impl InfectionParam {
    fn new(immunity: f64, virus_variant: usize) -> Self {
        Self {
            virus_variant,
            on_recovery: false,
            severity: 0.0,
            days_diseased: 0.0,
            days_infected: 0.0,
            immunity,
        }
    }

    fn check_infection(
        &self,
        immunity: f64,
        d: f64,
        days_to_onset: f64,
        pfs: &ParamsForStep,
    ) -> bool {
        // check contact and infection
        let virus_x = pfs.vr_info[self.virus_variant].reproductivity;
        let infec_d_max = pfs.rp.infec_dst * virus_x.sqrt();
        if d > infec_d_max {
            return false;
        }

        let exacerbate = exacerbation(virus_x);
        let contag_delay = pfs.rp.contag_delay / exacerbate;
        let contag_peak = pfs.rp.contag_peak / exacerbate;

        if self.days_infected <= contag_delay {
            return false;
        }

        let time_factor = 1f64.min(
            (self.days_infected - contag_delay) / (contag_peak.min(days_to_onset) - contag_delay),
        );
        let distance_factor = 1f64.min(((infec_d_max - d) / 2.0).powf(2.0));
        let infec_prob = if virus_x < 1.0 {
            pfs.rp.infec.r() * virus_x
        } else {
            1.0 - (1.0 - pfs.rp.infec.r()) / virus_x
        };

        if !was_hit(
            pfs.wp.steps_per_day(),
            infec_prob * time_factor * distance_factor * (1.0 - immunity),
        ) {
            return false;
        }
        true
    }

    fn step<const IS_IN_HOSPITAL: bool>(
        &mut self,
        days_to: &mut DaysTo,
        vp: &Option<VaccinationParam>,
        pfs: &ParamsForStep,
        hist: &mut Option<HistInfo>,
        inf_mode: &mut InfMode,
    ) -> Option<HealthState> {
        fn new_recover(
            days_to: &mut DaysTo,
            rp: &RuntimeParams,
            virus_variant: usize,
        ) -> RecoverParam {
            RecoverParam::new(days_to.setup_acquired_immunity(&rp), virus_variant)
        }

        self.days_infected += pfs.wp.days_per_step();
        if inf_mode == &InfMode::Sym {
            self.days_diseased += pfs.wp.days_per_step();
        }

        if self.on_recovery {
            self.severity -= 1.0 / MAX_DAYS_FOR_RECOVERY * pfs.wp.days_per_step();
            // recovered
            if self.severity <= 0.0 {
                if inf_mode == &InfMode::Sym {
                    // SET_HIST(hist_recov, days_diseased);
                    *hist = Some(HistInfo {
                        mode: HistgramType::HistRecov,
                        days: self.days_diseased,
                    });
                }
                return Some(HealthState::Recovered(new_recover(
                    days_to,
                    pfs.rp,
                    self.virus_variant,
                )));
            }
            return None;
        }

        let vr_info = &pfs.vr_info[self.virus_variant];
        let excrbt = exacerbation(vr_info.reproductivity);

        let days_to_recov = self.get_days_to_recov::<IS_IN_HOSPITAL>(days_to, pfs.rp);
        if inf_mode == &InfMode::Asym {
            if self.days_infected < days_to.onset / excrbt {
                if self.days_infected > days_to_recov {
                    return Some(HealthState::Recovered(new_recover(
                        days_to,
                        pfs.rp,
                        self.virus_variant,
                    )));
                }
                return None;
            }
        }

        let d_svr = {
            let mut v = 1.0 / (days_to.die - days_to.onset) * excrbt * pfs.wp.days_per_step();
            if let Some(vp) = &vp {
                v /= vp.vax_sv_effc(pfs);
            }
            if self.severity > TOXICITY_LEVEL {
                v *= vr_info.toxicity;
            }
            v
        };

        self.severity += d_svr;

        // died
        if self.severity >= 1.0 {
            // SET_HIST(hist_death, days_diseased)
            *hist = Some(HistInfo {
                mode: HistgramType::HistDeath,
                days: self.days_diseased,
            });
            return Some(HealthState::Died);
        }

        if self.days_infected > days_to_recov {
            self.on_recovery = true;
        }

        if inf_mode == &InfMode::Asym {
            // SET_HIST(hist_incub, days_infected)
            *hist = Some(HistInfo {
                mode: HistgramType::HistIncub,
                days: self.days_infected,
            });
            *inf_mode = InfMode::Sym;
        }

        None
    }

    fn get_days_to_recov<const IS_IN_HOSPITAL: bool>(
        &self,
        days_to: &DaysTo,
        rp: &RuntimeParams,
    ) -> f64 {
        let mut v = (1.0 - self.immunity) * days_to.recover;
        if IS_IN_HOSPITAL {
            v *= 1.0 - rp.therapy_effc.r()
        }
        v
    }
}

#[derive(Debug)]
pub struct RecoverParam {
    virus_variant: usize,
    days_recovered: f64,
    immunity: f64,
}

impl RecoverParam {
    fn new(immunity: f64, virus_variant: usize) -> Self {
        Self {
            immunity,
            virus_variant,
            days_recovered: 0.0,
        }
    }

    fn step(
        &mut self,
        days_to: &mut DaysTo,
        activeness: f64,
        age: f64,
        pfs: &ParamsForStep,
    ) -> Option<HealthState> {
        self.days_recovered += pfs.wp.days_per_step();
        if self.days_recovered > days_to.expire_immunity {
            Some(days_to.expire_immunity(activeness, age, pfs))
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct VaccinationParam {
    vaccine_type: usize,
    dose_date: f64,
    immunity: f64,
    //[todo] days_vaccinated: f64,
    //[todo] first_dose_date: f64,
}

impl VaccinationParam {
    fn vax_sv_effc(&self, pfs: &ParamsForStep) -> f64 {
        let days_vaccinated = self.days_vaccinated(pfs);
        let span = self.vx_span(pfs);
        let vcn_sv_effc = pfs.wp.vcn_sv_effc.r();
        if days_vaccinated < span {
            1.0 + days_vaccinated / span * vcn_sv_effc
        } else if days_vaccinated < pfs.wp.vcn_e_delay + span {
            (1.0 / (1.0 - vcn_sv_effc) - 1.0 - vcn_sv_effc) * (days_vaccinated - span)
                / pfs.wp.vcn_e_delay
                + 1.0
                + vcn_sv_effc
        } else {
            1.0 / (1.0 - vcn_sv_effc)
        }
    }

    fn vx_span(&self, pfs: &ParamsForStep) -> f64 {
        pfs.vx_info[self.vaccine_type].interval as f64
    }

    fn days_vaccinated(&self, pfs: &ParamsForStep) -> f64 {
        pfs.rp.step as f64 * pfs.wp.days_per_step() - self.dose_date
    }

    fn new_immunity(&self, pfs: &ParamsForStep) -> Option<f64> {
        let days_vaccinated = self.days_vaccinated(pfs);
        let span = self.vx_span(pfs);
        if days_vaccinated < span {
            // only the first dose
            Some(days_vaccinated * pfs.wp.vcn_1st_effc.r() / span)
        } else if days_vaccinated < pfs.wp.vcn_e_delay + span {
            // not fully vaccinated yet
            Some(
                ((pfs.wp.vcn_max_effc - pfs.wp.vcn_1st_effc)
                    * ((days_vaccinated - span) / pfs.wp.vcn_e_delay)
                    + pfs.wp.vcn_1st_effc)
                    .r(),
            )
        } else if days_vaccinated < pfs.wp.vcn_e_delay + span + pfs.wp.vcn_e_decay {
            Some(pfs.wp.vcn_max_effc.r())
        } else if days_vaccinated < pfs.wp.vcn_e_decay + span + pfs.wp.vcn_e_period {
            Some(pfs.wp.vcn_e_decay + span + pfs.wp.vcn_e_period - days_vaccinated)
        } else {
            None
        }
    }

    fn step(
        &mut self,
        days_to: &mut DaysTo,
        activeness: f64,
        age: f64,
        pfs: &ParamsForStep,
    ) -> Option<HealthState> {
        if let Some(i) = self.new_immunity(pfs) {
            self.immunity = i;
            None
        } else {
            Some(days_to.expire_immunity(activeness, age, pfs))
        }
    }
}

#[derive(Default)]
pub struct DaysTo {
    recover: f64,
    onset: f64,
    die: f64,
    expire_immunity: f64,
}

impl DaysTo {
    fn reset(&mut self, activeness: f64, age: f64, wp: &WorldParams, rp: &RuntimeParams) {
        *self = Self::new(activeness, age, wp, rp);
        //[todo] self.days_to.expire_immunity = random::my_random(rng, &rp.immun);
    }

    fn new(activeness: f64, age: f64, wp: &WorldParams, rp: &RuntimeParams) -> Self {
        let rng = &mut rand::thread_rng();
        let onset = random::random_with_corr(
            rng,
            &rp.incub,
            activeness,
            rp.act_mode.r(),
            rp.incub_act.r(),
        );
        let die = random::random_with_corr(
            rng,
            &rp.fatal,
            activeness,
            rp.act_mode.r(),
            rp.fatal_act.r(),
        ) + onset;
        let mode = wp.rcv_bias.r() * ((age - 105.0) / wp.rcv_temp).exp();
        let low = wp.rcv_lower.r() * mode;
        let span = wp.rcv_upper.r() * mode - low;
        let recover = {
            let r = if span == 0.0 {
                mode
            } else {
                random::random_mk(rng, (mode - low) / span, 0.0) * span + low
            };
            r * (rp.incub.mode + rp.fatal.mode)
        };
        Self {
            recover,
            onset,
            die,
            expire_immunity: 0.0,
        }
    }

    fn update_recover(&mut self, wp: &WorldParams) {
        self.recover *= 1.0 - wp.vcn_effc_symp.r();
    }

    const ALT_RATE: f64 = 0.1;
    fn alter_days(&mut self, activeness: f64, age: f64, pfs: &ParamsForStep) {
        let temp = DaysTo::new(activeness, age, pfs.wp, pfs.rp);
        self.die += Self::ALT_RATE * (temp.die - self.die);
        self.onset += Self::ALT_RATE * (temp.onset - self.onset);
        self.recover += Self::ALT_RATE * (temp.recover - self.recover);
        self.expire_immunity += Self::ALT_RATE * (temp.expire_immunity - self.expire_immunity);
    }

    fn expire_immunity(&mut self, activeness: f64, age: f64, pfs: &ParamsForStep) -> HealthState {
        self.alter_days(activeness, age, pfs);
        HealthState::Susceptible
    }

    fn setup_acquired_immunity(&mut self, rp: &RuntimeParams) -> f64 {
        let max_severity = self.recover * (1.0 - rp.therapy_effc.r()) / self.die;
        self.expire_immunity = 1.0f64.min(max_severity / (rp.imn_max_dur_sv.r())) * rp.imn_max_dur;
        1.0f64.min(max_severity / (rp.imn_max_effc_sv.r())) * rp.imn_max_effc.r()
    }
}

#[derive(Default)]
struct Body {
    pt: Point,
    v: Point,
    app: f64,
    prf: f64,
}

impl Body {
    fn reset(&mut self, wp: &WorldParams) -> Point {
        let rng = &mut rand::thread_rng();
        self.app = rng.gen();
        self.prf = rng.gen();
        let th: f64 = rng.gen::<f64>() * f64::consts::PI * 2.0;
        self.v.x = th.cos();
        self.v.y = th.sin();

        self.pt = match wp.wrk_plc_mode {
            WrkPlcMode::WrkPlcNone | WrkPlcMode::WrkPlcUniform => wp.random_point(),
            WrkPlcMode::WrkPlcCentered => wp.centered_point(),
        };
        self.pt
    }

    fn calc_dist(&self, b: &Self) -> f64 {
        let x = (b.app - self.prf).abs();
        (if x < 0.5 { x } else { 1.0 - x }) * 2.0
    }

    fn calc_force_delta(
        &self,
        b: &Self,
        wp: &WorldParams,
        rp: &RuntimeParams,
    ) -> Option<(Point, f64)> {
        let delta = b.pt - self.pt;
        let d2 = (delta.x * delta.x + delta.y * delta.y).max(1e-4);
        let d = d2.sqrt();
        let view_range = wp.view_range();
        if d >= view_range {
            return None;
        }

        let mut dd = if d < view_range * 0.8 {
            1.0
        } else {
            (1.0 - d / view_range) / 0.2
        };
        dd = dd / d / d2 * AVOIDANCE * rp.avoidance / 50.0;
        let df = delta * dd;

        Some((df, d))
    }

    pub fn get_new_pt(&self, world_size: f64, mob_dist: &DistInfo<Percentage>) -> Point {
        let rng = &mut rand::thread_rng();
        let dst = random::my_random(rng, mob_dist).r() * world_size;
        let th = rng.gen::<f64>() * f64::consts::PI * 2.;
        let mut new_pt = Point {
            x: self.pt.x + th.cos() * dst,
            y: self.pt.y + th.sin() * dst,
        };
        if new_pt.x < 3. {
            new_pt.x = 3. - new_pt.x;
        } else if new_pt.x > world_size - 3. {
            new_pt.x = (world_size - 3.) * 2. - new_pt.x;
        }
        if new_pt.y < 3. {
            new_pt.y = 3. - new_pt.y;
        } else if new_pt.y > world_size - 3. {
            new_pt.y = (world_size - 3.) * 2. - new_pt.y;
        }

        new_pt
    }

    fn warp_update(&mut self, goal: Point, wp: &WorldParams) -> bool {
        let dp = goal - self.pt;
        let d = dp.y.hypot(dp.x);
        let v = wp.field_size() / 5.0 * wp.days_per_step();
        if d < v {
            self.pt = goal;
            true
        } else {
            let th = dp.y.atan2(dp.x);
            self.pt.x += v * th.cos();
            self.pt.y += v * th.sin();
            false
        }
    }

    fn field_update(
        &mut self,
        is_symptomatic: bool,
        f: Point,
        gat_dist: &Option<f64>,
        pfs: &ParamsForStep,
    ) {
        self.update_velocity(is_symptomatic, gat_dist, f, pfs);
        self.pt += self.v * pfs.wp.days_per_step();

        if let Some(x) = Self::check_bounce(self.pt.x, pfs.wp.field_size()) {
            self.pt.x = x;
            self.v.x = -self.v.x;
        }
        if let Some(y) = Self::check_bounce(self.pt.y, pfs.wp.field_size()) {
            self.pt.y = y;
            self.v.y = -self.v.y;
        }
    }

    fn update_velocity(
        &mut self,
        is_symptomatic: bool,
        gat_dist: &Option<f64>,
        f: Point,
        pfs: &ParamsForStep,
    ) {
        let mut fric = ((1.0 - pfs.rp.friction.r()) * 0.99).powf(pfs.wp.days_per_step());
        if let Some(dist) = gat_dist {
            fric *= dist * 0.5 + 0.5;
        }

        let mut dv = f * (pfs.wp.days_per_step() / pfs.rp.mass.r());
        if is_symptomatic {
            dv /= 20.0;
        }

        self.v *= fric;
        self.v += dv;
        let v_norm = self.v.x.hypot(self.v.y);
        let max_v = pfs.rp.max_speed * 20.0 * pfs.wp.days_per_step();
        if v_norm > max_v {
            self.v *= max_v / v_norm;
        }
    }

    fn check_bounce(p: f64, field_size: f64) -> Option<f64> {
        if p < AGENT_RADIUS {
            Some(AGENT_RADIUS * 2.0 - p)
        } else if p > field_size - AGENT_RADIUS {
            Some((field_size - AGENT_RADIUS) * 2.0 - p)
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct AgentCore {
    pub id: usize,
    body: Body,
    pub origin: Point,

    distancing: bool,
    is_out_of_field: bool,
    location: Location,
    quarantine_reserved: bool,
    test_reserved: bool,
    last_tested: Option<u64>,

    contacts: Contacts,
    gathering: Weak<Mutex<Gathering>>,
    n_infects: u64,

    activeness: f64,
    age: f64,
    days_to: DaysTo,

    mob_freq: f64,
    gat_freq: f64,

    health: AgentHealth,
}

impl AgentCore {
    fn reset(&mut self, wp: &WorldParams, rp: &RuntimeParams, id: usize, distancing: bool) {
        self.quarantine_reserved = false;
        self.last_tested = None;
        let rng = &mut rand::thread_rng();
        self.n_infects = 0;
        self.is_out_of_field = true;
        self.days_to.reset(self.activeness, self.age, wp, rp);

        self.activeness = random::random_mk(rng, rp.act_mode.r(), rp.act_kurt.r());
        self.gathering = Weak::new();
        let d_info = DistInfo::new(0.0, 0.5, 1.0);
        self.mob_freq = random::random_with_corr(
            rng,
            &d_info,
            self.activeness,
            rp.act_mode.r(),
            rp.mob_act.r(),
        );
        self.gat_freq = random::random_with_corr(
            rng,
            &d_info,
            self.activeness,
            rp.act_mode.r(),
            rp.gat_act.r(),
        );

        self.id = id;
        self.distancing = distancing;

        self.origin = self.body.reset(wp);
    }

    #[inline]
    fn get_pt(&self) -> Point {
        self.body.pt
    }

    #[inline]
    fn force_susceptible(&mut self) {
        self.health.force_susceptible();
    }

    #[inline]
    fn force_infected(&mut self) -> bool {
        self.health.force_infected(&self.days_to);
        self.health.is_symptomatic()
    }

    fn force_recovered(&mut self, rp: &RuntimeParams) {
        let rng = &mut rand::thread_rng();
        self.days_to.expire_immunity = rng.gen::<f64>() * rp.imn_max_dur;
        let days_recovered = rng.gen::<f64>() * self.days_to.expire_immunity;
        self.health.force_recovered(days_recovered);
    }

    fn reserve_test(&mut self, a: Agent, reason: TestReason, pfs: &ParamsForStep) -> Testee {
        self.test_reserved = true;
        let rng = &mut rand::thread_rng();
        let p = if let Some(ip) = self.health.get_infected() {
            rng.gen::<f64>()
                < 1.0
                    - (1.0 - pfs.rp.tst_sens.r()).powf(pfs.vr_info[ip.virus_variant].reproductivity)
        } else {
            rng.gen::<f64>() > pfs.rp.tst_spec.r()
        };
        Testee::new(a, reason, p.into(), pfs.rp.step)
    }

    fn deliver_test_result(&mut self, time_stamp: u64, result: TestResult) {
        self.test_reserved = false;
        self.last_tested = Some(time_stamp);
        if let TestResult::Positive = result {
            self.quarantine_reserved = true;
        }
    }

    fn is_testable(&self, wp: &WorldParams, rp: &RuntimeParams) -> bool {
        if !self.is_in_field() || self.test_reserved
        /*|| todo!("self.for_vcn == VcnNoTest") */
        {
            return false;
        }

        match self.last_tested {
            Some(d) => {
                let ds = (rp.step - d) as f64;
                ds >= rp.tst_interval * wp.steps_per_day()
            }
            None => true,
        }
    }

    #[inline]
    fn is_in_field(&self) -> bool {
        matches!(self.location, Location::Field)
    }

    fn try_infect(&self, b: &mut Self, d: f64, pfs: &ParamsForStep) -> bool {
        fn infect(
            a: &AgentCore,
            b: &AgentCore,
            d: f64,
            pfs: &ParamsForStep,
        ) -> Option<HealthState> {
            let ip = a.health.get_infected()?;
            let immunity = b.health.get_immune_factor(ip, pfs)?;
            if ip.check_infection(immunity, d, a.days_to.onset, pfs) {
                Some(HealthState::Infected(
                    InfectionParam::new(immunity, ip.virus_variant),
                    InfMode::Asym,
                ))
            } else {
                None
            }
        }

        if !b.health.become_infected() {
            b.health.set_new_health(infect(self, b, d, pfs))
        } else {
            false
        }
    }

    fn calc_gathering_effect(&self) -> (Option<Point>, Option<f64>) {
        match self.gathering.upgrade() {
            None => (None, None),
            Some(gat) => gat.lock().unwrap().get_effect(&self.body.pt),
        }
    }

    fn check_test(&self, wp: &WorldParams, rp: &RuntimeParams) -> Option<TestReason> {
        if !self.is_testable(wp, rp) {
            return None;
        }
        if let Some(ip) = self.health.get_symptomatic() {
            if ip.days_diseased >= rp.tst_delay && was_hit(wp.days_per_step(), rp.tst_sbj_sym.r()) {
                return Some(TestReason::AsSymptom);
            }
        }
        if was_hit(wp.days_per_step(), rp.tst_sbj_asy.r()) {
            return Some(TestReason::AsSuspected);
        }
        None
    }

    fn get_warp_inside_goal(&self, pfs: &ParamsForStep) -> Option<Point> {
        let dp = self.body.pt - self.origin;
        if BACK_HOME_RATE {
            if pfs.go_home_back()
                && dp.x.hypot(dp.y)
                    > pfs.rp.mob_dist.min.max(&MIN_AWAY_TO_HOME).r() * pfs.wp.field_size()
                && was_hit(pfs.wp.days_per_step() * 3.0, pfs.rp.back_hm_rt.r())
            {
                return Some(self.origin);
            }
            if was_hit(
                pfs.wp.days_per_step(),
                modified_prob(self.mob_freq, &pfs.rp.mob_freq).r(),
            ) {
                return Some(self.body.get_new_pt(pfs.wp.field_size(), &pfs.rp.mob_dist));
            }
        } else {
            if was_hit(
                pfs.wp.days_per_step(),
                modified_prob(self.mob_freq, &pfs.rp.mob_freq).r(),
            ) {
                if pfs.go_home_back()
                    && dp.x.hypot(dp.y)
                        > pfs.rp.mob_dist.min.max(&MIN_AWAY_TO_HOME).r() * pfs.wp.field_size()
                {
                    return Some(self.origin);
                } else {
                    return Some(self.body.get_new_pt(pfs.wp.field_size(), &pfs.rp.mob_dist));
                }
            }
        }
        None
    }

    fn try_warp_inside(&self, pfs: &ParamsForStep) -> ControlFlow<WarpParam> {
        if self.health.is_symptomatic() {
            return ControlFlow::Continue(());
        }
        if let Some(goal) = self.get_warp_inside_goal(pfs) {
            return ControlFlow::Break(WarpParam::inside(goal));
        }
        ControlFlow::Continue(())
    }

    fn calc_force(
        &self,
        interaction: &InteractionUpdate,
        pfs: &ParamsForStep,
    ) -> (Point, Option<f64>) {
        let mut gat_dist = None;
        let mut f = interaction.force;
        if pfs.go_home_back() {
            if let Some(df) = back_home_force(&self.body.pt, &self.origin) {
                f += df;
            }
        } else {
            let (df, dist) = self.calc_gathering_effect();
            if let Some(df) = df {
                f += df;
            }
            gat_dist = dist;
        };

        if self.distancing {
            f *= 1.0 + pfs.rp.dst_st / 5.0;
        }
        f += self.best_point_force(&interaction.best.map(|(p, _)| p), pfs.wp.field_size());
        (f, gat_dist)
    }

    fn best_point_force(&self, best_pt: &Option<Point>, field_size: f64) -> Point {
        fn wall(d: f64) -> f64 {
            let d = if d < 0.02 { 0.02 } else { d };
            AVOIDANCE * 20. / d / d
        }
        let pt = &self.body.pt;
        let mut f = pt.map(wall) - pt.map(|p| wall(field_size - p));
        if let (Some(bp), false) = (best_pt, self.distancing) {
            let dp = bp - &self.body.pt;
            let d = dp.x.hypot(dp.y).max(0.01) * 20.0;
            f += dp / d;
        }
        f
    }

    fn move_internal(
        &mut self,
        interaction: &InteractionUpdate,
        idx: &TableIndex,
        pfs: &ParamsForStep,
    ) -> Option<TableIndex> {
        let (f, gat_dist) = self.calc_force(interaction, pfs);
        self.body
            .field_update(self.health.is_symptomatic(), f, &gat_dist, pfs);
        let new_idx = pfs.wp.into_grid_index(&self.body.pt);
        if *idx != new_idx {
            Some(new_idx)
        } else {
            None
        }
    }

    fn try_quarantine(
        &mut self,
        pfs: &ParamsForStep,
        contact_testees: &mut Option<Vec<Testee>>,
    ) -> ControlFlow<WarpParam> {
        if self.quarantine_reserved {
            self.quarantine_reserved = false;
            //[todo] prms.rp.trc_ope != TrcTst
            *contact_testees = Some(self.contacts.get_testees(pfs));
            if let WrkPlcMode::WrkPlcNone = pfs.wp.wrk_plc_mode {
                self.origin = self.body.pt;
            }
            ControlFlow::Break(WarpParam::hospital(pfs.wp))
        } else {
            ControlFlow::Continue(())
        }
    }

    #[inline]
    fn update_health(&mut self) -> Option<HealthDiff> {
        self.health.update()
    }

    fn replace_gathering(
        &mut self,
        gat_freq: &DistInfo<Percentage>,
        gathering: Weak<Mutex<Gathering>>,
    ) {
        if !self.health.is_symptomatic()
            && rand::thread_rng().gen::<f64>() < random::modified_prob(self.gat_freq, gat_freq).r()
        {
            self.gathering = gathering;
        }
    }

    fn update_n_infects(&mut self, new_n_infects: u64) -> Option<InfectionCntInfo> {
        if new_n_infects > 0 {
            let prev_n_infects = self.n_infects;
            self.n_infects += new_n_infects;
            Some(InfectionCntInfo::new(prev_n_infects, self.n_infects))
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub struct Agent(Arc<Mutex<AgentCore>>);
impl Agent {
    pub fn new() -> Self {
        Agent(Arc::new(Mutex::new(AgentCore::default())))
    }

    pub fn reset_all(
        agents: &[Self],
        n_pop: usize,
        n_infected: usize,
        n_recovered: usize,
        n_dist: usize,
        wp: &WorldParams,
        rp: &RuntimeParams,
    ) -> (Vec<HealthType>, usize) {
        use crate::commons::math;
        let mut cats = {
            let r = n_pop - n_infected;
            if r == 0 {
                vec![HealthType::Asymptomatic; n_pop]
            } else {
                let mut cats = if r == n_recovered {
                    vec![HealthType::Recovered; n_pop]
                } else {
                    vec![HealthType::Susceptible; n_pop]
                };
                let idxs_inf = math::reservoir_sampling(n_pop, n_infected);
                let mut m = usize::MAX;
                for idx in idxs_inf {
                    cats[idx] = HealthType::Asymptomatic;
                    if m > idx {
                        m = idx;
                    }
                }
                let cnts_inf = {
                    let mut is = vec![0; r];
                    let mut c = 0;
                    let mut k = m;
                    for i in is.iter_mut().take(r).skip(m) {
                        if let HealthType::Asymptomatic = cats[k] {
                            c += 1;
                            k += 1;
                        }
                        *i = c;
                        k += 1;
                    }
                    is
                };
                if r > n_recovered {
                    for i in math::reservoir_sampling(r, n_recovered) {
                        cats[i + cnts_inf[i]] = HealthType::Recovered;
                    }
                }
                cats
            }
        };

        let mut n_symptomatic = 0;
        for (i, t) in cats.iter_mut().enumerate() {
            let mut a = agents[i].0.lock().unwrap();
            a.reset(wp, rp, i, i < n_dist);
            match t {
                HealthType::Susceptible => a.force_susceptible(),
                HealthType::Asymptomatic => {
                    if a.force_infected() {
                        n_symptomatic += 1;
                        *t = HealthType::Symptomatic;
                    }
                }
                HealthType::Recovered => a.force_recovered(rp),
                _ => {}
            }
        }
        (cats, n_symptomatic)
    }

    pub fn try_reserve_test(&self, pfs: &ParamsForStep) -> Option<Testee> {
        let mut a = self.0.lock().unwrap();
        if a.is_testable(pfs.wp, pfs.rp) {
            Some(a.reserve_test(self.clone(), TestReason::AsContact, pfs))
        } else {
            None
        }
    }

    pub fn deliver_test_result(&self, time_stamp: u64, result: TestResult) {
        self.0
            .lock()
            .unwrap()
            .deliver_test_result(time_stamp, result);
    }

    fn set_location(&self, location: Location) {
        self.0.lock().unwrap().location = location;
    }

    pub fn get_origin(&self) -> Point {
        self.0.lock().unwrap().origin
    }

    pub fn get_pt(&self) -> Point {
        self.0.lock().unwrap().get_pt()
    }
}

#[derive(Default, Debug)]
pub enum Location {
    // Cemetery,
    #[default]
    Field,
    Hospital,
    Warp,
}

trait LocationLabel {
    const LABEL: Location;
    fn label(agent: Agent) -> Agent {
        agent.set_location(Self::LABEL);
        agent
    }
}

struct InteractionUpdate {
    force: Point,
    best: Option<(Point, f64)>,
    new_n_infects: u64,
    new_contacts: Vec<Agent>,
}

impl InteractionUpdate {
    fn new() -> Self {
        Self {
            force: Point::new(0.0, 0.0),
            best: None,
            new_n_infects: 0,
            new_contacts: Vec::new(),
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn update_best(&mut self, a: &Body, b: &Body) {
        let x = a.calc_dist(b);
        match self.best {
            None => self.best = Some((b.pt, x)),
            Some((_, dist)) if dist > x => self.best = Some((b.pt, x)),
            _ => {}
        }
    }

    fn record_contact(&mut self, b: &Agent, d: f64, pfs: &ParamsForStep) {
        if d < pfs.rp.infec_dst && was_hit(pfs.wp.days_per_step(), pfs.rp.cntct_trc.r()) {
            self.new_contacts.push(b.clone());
        }
    }
}

pub struct FieldAgent {
    agent: Agent,
    idx: TableIndex,
    interaction: InteractionUpdate,
}

impl LocationLabel for FieldAgent {
    const LABEL: Location = Location::Field;
}

pub struct FieldStepInfo {
    contact_testees: Option<Vec<Testee>>,
    testee: Option<Testee>,
    hist: Option<HistInfo>,
    infct: Option<InfectionCntInfo>,
    health: Option<HealthDiff>,
}

impl FieldAgent {
    fn new(agent: Agent, idx: TableIndex) -> Self {
        Self {
            agent: Self::label(agent),
            idx,
            interaction: InteractionUpdate::new(),
        }
    }

    fn reset_for_step(&mut self) {
        self.interaction.reset();
    }

    fn step(
        &mut self,
        pfs: &ParamsForStep,
    ) -> (FieldStepInfo, Option<Either<WarpParam, TableIndex>>) {
        fn local_step(
            agent: &mut AgentCore,
            pfs: &ParamsForStep,
            hist: &mut Option<HistInfo>,
            contact_testees: &mut Option<Vec<Testee>>,
            test_reason: &mut Option<TestReason>,
        ) -> ControlFlow<WarpParam> {
            agent.try_quarantine(pfs, contact_testees)?;
            agent
                .health
                .field_step(&mut agent.days_to, agent.activeness, agent.age, hist, pfs)?;
            *test_reason = agent.check_test(pfs.wp, pfs.rp);
            agent.try_warp_inside(pfs)
        }

        let agent = &mut self.agent.0.lock().unwrap();
        let mut hist = None;
        let mut contact_testees = None;
        let mut test_reason = None;
        let transfer = match local_step(
            agent,
            pfs,
            &mut hist,
            &mut contact_testees,
            &mut test_reason,
        ) {
            ControlFlow::Break(t) => Some(Either::Left(t)),
            ControlFlow::Continue(_) => agent
                .move_internal(&self.interaction, &self.idx, pfs)
                .map(Either::Right),
        };

        agent
            .contacts
            .append(&mut self.interaction.new_contacts, pfs.rp.step);

        let fsi = FieldStepInfo {
            contact_testees,
            testee: test_reason.map(|reason| agent.reserve_test(self.agent.clone(), reason, pfs)),
            hist,
            infct: agent.update_n_infects(self.interaction.new_n_infects),
            health: agent.update_health(),
        };
        (fsi, transfer)
    }

    fn interacts(&mut self, fb: &mut Self, pfs: &ParamsForStep) {
        let a = &mut self.agent.0.lock().unwrap();
        let b = &mut fb.agent.0.lock().unwrap();
        if let Some((df, d)) = a.body.calc_force_delta(&b.body, pfs.wp, pfs.rp) {
            self.interaction.force -= df;
            fb.interaction.force += df;
            self.interaction.update_best(&a.body, &b.body);
            fb.interaction.update_best(&b.body, &a.body);

            if b.try_infect(a, d, pfs) {
                self.interaction.new_n_infects = 1;
            }
            if a.try_infect(b, d, pfs) {
                fb.interaction.new_n_infects += 1;
            }

            self.interaction.record_contact(&fb.agent, d, pfs);
            fb.interaction.record_contact(&self.agent, d, pfs);
        }
    }
}

pub struct HospitalStepInfo {
    hist: Option<HistInfo>,
    health: Option<HealthDiff>,
}

pub struct HospitalAgent {
    agent: Agent,
}

impl LocationLabel for HospitalAgent {
    const LABEL: Location = Location::Hospital;
}

impl HospitalAgent {
    fn new(agent: Agent) -> Self {
        Self {
            agent: Self::label(agent),
        }
    }

    fn step(&mut self, pfs: &ParamsForStep) -> (HospitalStepInfo, Option<WarpParam>) {
        fn local_step(
            agent: &mut AgentCore,
            hist: &mut Option<HistInfo>,
            pfs: &ParamsForStep,
        ) -> Option<WarpParam> {
            agent
                .health
                .hospital_step(&mut agent.days_to, agent.origin, hist, pfs)
        }
        let agent = &mut self.agent.0.lock().unwrap();
        let mut hist = None;
        let warp = local_step(agent, &mut hist, pfs);
        (
            HospitalStepInfo {
                hist,
                health: agent.update_health(),
            },
            warp,
        )
    }
}

pub enum WarpMode {
    Back,
    Inside,
    Hospital,
    Cemetery,
}

pub struct WarpParam {
    mode: WarpMode,
    goal: Point,
}

impl WarpParam {
    fn new(mode: WarpMode, goal: Point) -> Self {
        Self { mode, goal }
    }

    fn back(goal: Point) -> Self {
        Self::new(WarpMode::Back, goal)
    }

    fn inside(goal: Point) -> Self {
        Self::new(WarpMode::Inside, goal)
    }

    pub fn hospital(wp: &WorldParams) -> Self {
        let rng = &mut rand::thread_rng();
        let goal = Point::new(
            (rng.gen::<f64>() * 0.248 + 1.001) * wp.field_size(),
            (rng.gen::<f64>() * 0.458 + 0.501) * wp.field_size(),
        );
        Self::new(WarpMode::Hospital, goal)
    }

    fn cemetery(wp: &WorldParams) -> Self {
        let rng = &mut rand::thread_rng();
        let goal = Point::new(
            (rng.gen::<f64>() * 0.248 + 1.001) * wp.field_size(),
            (rng.gen::<f64>() * 0.468 + 0.001) * wp.field_size(),
        );
        Self::new(WarpMode::Cemetery, goal)
    }
}

struct WarpStepInfo {
    contact_testees: Option<Vec<Testee>>,
}

pub struct WarpAgent {
    agent: Agent,
    param: WarpParam,
}

impl LocationLabel for WarpAgent {
    const LABEL: Location = Location::Warp;
}

impl WarpAgent {
    fn new(agent: Agent, param: WarpParam) -> Self {
        Self {
            agent: Self::label(agent),
            param,
        }
    }

    fn step(&mut self, pfs: &ParamsForStep) -> (WarpStepInfo, bool) {
        let mut agent = self.agent.0.lock().unwrap();
        let mut contact_testees = None;
        if let WarpMode::Inside = self.param.mode {
            match agent.try_quarantine(pfs, &mut contact_testees) {
                ControlFlow::Break(w) => self.param = w,
                _ => {}
            }
        }
        let at_goal = agent.body.warp_update(self.param.goal, pfs.wp);

        (WarpStepInfo { contact_testees }, at_goal)
    }
}

pub mod location {
    use super::{Agent, FieldAgent, HospitalAgent, ParamsForStep, WarpAgent, WarpMode, WarpParam};
    use crate::{
        commons::{math::Percentage, DistInfo, DrainMap, DrainWith, Either},
        gathering::Gathering,
        log::StepLog,
        table::{Table, TableIndex},
        testing::TestQueue,
    };
    use rayon::iter::ParallelIterator;
    use std::sync::{Arc, Mutex};

    pub struct Field {
        table: Table<Vec<FieldAgent>>,
    }

    impl Field {
        pub fn new(mesh: usize) -> Self {
            Self {
                table: Table::new(mesh, mesh, Vec::new),
            }
        }

        pub fn clear(&mut self) {
            //[todo] fix mesh size
            self.table
                .par_iter_mut()
                .horizontal()
                .for_each(|(_, c)| c.clear());
        }

        pub fn reset_for_step(&mut self) {
            self.table.par_iter_mut().horizontal().for_each(|(_, ags)| {
                for fa in ags {
                    fa.reset_for_step();
                }
            });
        }

        pub fn steps(
            &mut self,
            warps: &mut Warps,
            test_queue: &mut TestQueue,
            step_log: &mut StepLog,
            pfs: &ParamsForStep,
        ) {
            let tmp = self
                .table
                .par_iter_mut()
                .horizontal()
                .map(|(_, ags)| ags.drain_map_mut(|fa| fa.step(pfs)))
                .collect::<Vec<_>>();

            for (fsi, opt) in tmp.into_iter().flatten() {
                if let Some(h) = fsi.hist {
                    step_log.hists.push(h);
                }
                if let Some(h) = fsi.health {
                    step_log.apply_difference(h);
                }
                if let Some(i) = fsi.infct {
                    step_log.infcts.push(i);
                }
                if let Some(testees) = fsi.contact_testees {
                    test_queue.extend(testees);
                }
                if let Some(testee) = fsi.testee {
                    test_queue.push(testee);
                }
                if let Some((t, fa)) = opt {
                    match t {
                        Either::Left(warp) => warps.add(fa.agent, warp),
                        Either::Right(idx) => self.add(fa.agent, idx),
                    }
                }
            }
        }

        pub fn add(&mut self, agent: Agent, idx: TableIndex) {
            self.table[idx.clone()].push(FieldAgent::new(agent, idx));
        }

        pub fn intersect(&mut self, pfs: &ParamsForStep) {
            // |x|a|b|a|b|
            self.table
                .par_iter_mut()
                .east()
                .for_each(move |((_, a_ags), (_, b_ags))| {
                    Self::interact_intercells(a_ags, b_ags, pfs);
                });
            // |a|b|a|b|
            self.table
                .par_iter_mut()
                .west()
                .for_each(|((_, a_ags), (_, b_ags))| {
                    Self::interact_intercells(a_ags, b_ags, pfs);
                });

            // |a|
            // |b|
            self.table
                .par_iter_mut()
                .north()
                .for_each(|((_, a_ags), (_, b_ags))| {
                    Self::interact_intercells(a_ags, b_ags, pfs);
                });

            // |x|
            // |a|
            // |b|
            self.table
                .par_iter_mut()
                .south()
                .for_each(|((_, a_ags), (_, b_ags))| {
                    Self::interact_intercells(a_ags, b_ags, pfs);
                });

            // | |a|
            // |b| |
            self.table
                .par_iter_mut()
                .northeast()
                .for_each(|((_, a_ags), (_, b_ags))| {
                    Self::interact_intercells(a_ags, b_ags, pfs);
                });

            // |a| |
            // | |b|
            self.table
                .par_iter_mut()
                .northwest()
                .for_each(|((_, a_ags), (_, b_ags))| {
                    Self::interact_intercells(a_ags, b_ags, pfs);
                });

            // | | |
            // |a| |
            // | |b|
            self.table
                .par_iter_mut()
                .southeast()
                .for_each(|((_, a_ags), (_, b_ags))| {
                    Self::interact_intercells(a_ags, b_ags, pfs);
                });

            // | | |
            // | |a|
            // |b| |
            self.table
                .par_iter_mut()
                .southwest()
                .for_each(|((_, a_ags), (_, b_ags))| {
                    Self::interact_intercells(a_ags, b_ags, pfs);
                });
        }

        fn interact_intercells(
            a_ags: &mut [FieldAgent],
            b_ags: &mut [FieldAgent],
            pfs: &ParamsForStep,
        ) {
            for fa in a_ags {
                for fb in b_ags.iter_mut() {
                    fa.interacts(fb, pfs);
                }
            }
        }

        pub fn replace_gathering(
            &self,
            row: usize,
            column: usize,
            gat_freq: &DistInfo<Percentage>,
            gat: &Arc<Mutex<Gathering>>,
        ) {
            for fa in self.table[(row, column)].iter() {
                fa.agent
                    .0
                    .lock()
                    .unwrap()
                    .replace_gathering(gat_freq, Arc::downgrade(gat));
            }
        }
    }

    pub struct Hospital(Vec<HospitalAgent>);

    impl Hospital {
        pub fn new() -> Self {
            Self(Vec::new())
        }

        pub fn clear(&mut self) {
            self.0.clear();
        }

        pub fn add(&mut self, agent: Agent) {
            self.0.push(HospitalAgent::new(agent));
        }

        pub fn steps(&mut self, warps: &mut Warps, step_log: &mut StepLog, pfs: &ParamsForStep) {
            let tmp = self.0.drain_map_mut(|ha| ha.step(pfs));

            for (hsi, opt) in tmp.into_iter() {
                if let Some(h) = hsi.hist {
                    step_log.hists.push(h);
                }
                if let Some(h) = hsi.health {
                    step_log.apply_difference(h);
                }
                if let Some((param, ha)) = opt {
                    warps.add(ha.agent.clone(), param);
                }
            }
        }
    }

    pub struct Warps(Vec<WarpAgent>);

    impl Warps {
        pub fn new() -> Self {
            Self(Vec::new())
        }

        pub fn clear(&mut self) {
            self.0.clear();
        }

        pub fn add(&mut self, agent: Agent, param: WarpParam) {
            self.0.push(WarpAgent::new(agent, param));
        }

        pub fn steps(
            &mut self,
            field: &mut Field,
            hospital: &mut Hospital,
            cemetery: &mut Cemetery,
            test_queue: &mut TestQueue,
            pfs: &ParamsForStep,
        ) {
            let tmp = self.0.drain_with_mut(|a| a.step(pfs));
            for (wsi, opt) in tmp.into_iter() {
                if let Some(testees) = wsi.contact_testees {
                    test_queue.extend(testees);
                }
                if let Some(wa) = opt {
                    let WarpAgent {
                        agent,
                        param: WarpParam { mode, goal },
                    } = wa;
                    match mode {
                        WarpMode::Back => field.add(agent, pfs.wp.into_grid_index(&goal)),
                        WarpMode::Inside => field.add(agent, pfs.wp.into_grid_index(&goal)),
                        WarpMode::Hospital => hospital.add(agent),
                        WarpMode::Cemetery => cemetery.add(agent),
                    }
                }
            }
        }
    }

    pub struct Cemetery(Vec<Agent>);

    impl Cemetery {
        pub fn new() -> Self {
            Self(Vec::new())
        }

        pub fn clear(&mut self) {
            self.0.clear();
        }

        pub fn add(&mut self, a: Agent) {
            self.0.push(a);
        }
    }
}
