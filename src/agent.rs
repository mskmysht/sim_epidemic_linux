use crate::{
    commons::{
        math::{Percentage, Point},
        random::{self, modified_prob},
        DistInfo, Either, RuntimeParams, WorldParams, WrkPlcMode,
    },
    contact::Contacts,
    gathering::Gathering,
    log::HealthDiff,
    stat::{HistInfo, HistgramType, InfectionCntInfo},
    table::TableIndex,
    testing::{TestReason, Testee},
};
use health::{AgentHealth, HealthTransition};
use rand::{self, Rng};
use std::{
    f64,
    ops::ControlFlow,
    sync::{Arc, Mutex, Weak},
};

use self::cont::{Cemetery, Field, Hospital};

const AGENT_RADIUS: f64 = 0.75;
// static AGENT_SIZE: f64 = 0.665;
const AVOIDANCE: f64 = 0.2;
const MAX_DAYS_FOR_RECOVERY: f64 = 7.0;
const TOXICITY_LEVEL: f64 = 0.5;

const BACK_HOME_RATE: bool = true;

// pub const MAX_N_VAXEN: usize = 8;
// pub const MAX_N_VARIANTS: usize = 8;
pub struct VariantInfo {
    pub reproductivity: f64,
    toxicity: f64,
    efficacy: Vec<f64>, //; MAX_N_VARIANTS],
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
        // wp.wrk_plc_mode != WrkPlcMode::WrkPlcNone && self.is_daytime()
    }

    fn is_daytime(wp: &WorldParams, rp: &RuntimeParams) -> bool {
        if wp.steps_per_day < 3 {
            rp.step % 2 == 0
        } else {
            rp.step % wp.steps_per_day < wp.steps_per_day * 2 / 3
        }
    }
}

/// local functions
fn exacerbation(reproductivity: f64) -> f64 {
    reproductivity.powf(1.0 / 3.0)
}

fn was_hit(days_per_step: f64, prob: f64) -> bool {
    let rng = &mut rand::thread_rng();
    rng.gen::<f64>() > (1.0 - prob).powf(days_per_step)
}

pub fn wall(d: f64) -> f64 {
    let d = if d < 0.02 { 0.02 } else { d };
    AVOIDANCE * 20. / d / d
}

fn best_point_force(
    pt: &Point,
    best_pt: &Option<Point>,
    distancing: bool,
    field_size: f64,
) -> Point {
    let mut f = pt.map(wall) - pt.map(|p| wall(field_size - p));
    if let (Some(bp), false) = (best_pt, distancing) {
        let dp = bp - pt;
        let d = dp.x.hypot(dp.y).max(0.01) * 20.0;
        f += dp / d;
    }
    f
}

fn is_contacted(d: f64, wp: &WorldParams, rp: &RuntimeParams) -> bool {
    d < rp.infec_dst && was_hit(wp.steps_per_day(), rp.cntct_trc.r())
}

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
        commons::{math::Point, Either, HealthType},
        log::HealthDiff,
        stat::HistInfo,
    };
    use rand::{self, Rng};
    use std::{f64, ops::ControlFlow};

    #[derive(Debug)]
    pub enum HealthTransition {
        Susceptible,
        Asymptomatic { immunity: f64, virus_variant: usize },
        Symptomatic,
        Recovered,
        Vaccinated { vaccine_type: usize, immunity: f64 },
        Died,
    }

    impl From<&HealthTransition> for HealthType {
        fn from(t: &HealthTransition) -> Self {
            match t {
                HealthTransition::Susceptible => HealthType::Susceptible,
                HealthTransition::Asymptomatic { .. } => HealthType::Asymptomatic,
                HealthTransition::Symptomatic => HealthType::Symptomatic,
                HealthTransition::Recovered => HealthType::Recovered,
                HealthTransition::Vaccinated { .. } => HealthType::Vaccinated,
                HealthTransition::Died => HealthType::Died,
            }
        }
    }

    #[derive(Default)]
    pub enum HealthParam {
        #[default]
        Unfortified,
        Infected(InfectionParam),
        Recovered(RecoverParam),
        Vaccinated(VaccinationParam),
        Died,
    }

    impl From<&HealthParam> for HealthType {
        fn from(p: &HealthParam) -> Self {
            match p {
                HealthParam::Unfortified => HealthType::Susceptible,
                HealthParam::Infected(ip) if ip.is_symptomatic => HealthType::Symptomatic,
                HealthParam::Infected(_) => HealthType::Asymptomatic,
                HealthParam::Recovered(_) => HealthType::Recovered,
                HealthParam::Vaccinated(_) => HealthType::Vaccinated,
                HealthParam::Died => HealthType::Died,
            }
        }
    }

    #[derive(Default)]
    pub struct AgentHealth {
        state: HealthParam,
        transition: Option<HealthTransition>,
        vaccination: Option<VaccinationParam>,
        vaccine_ticket: Option<usize>,
    }

    impl AgentHealth {
        pub fn force_unfortified(&mut self) {
            self.transition = None;
            self.state = HealthParam::Unfortified;
        }

        pub fn force_infected(&mut self, days_to: &DaysTo) -> bool {
            let mut ip = InfectionParam::new(0.0, 0);
            ip.days_infected = rand::thread_rng().gen::<f64>() * days_to.recover.min(days_to.die);
            let d = ip.days_infected - days_to.onset;
            let is_symptomatic = d >= 0.0;
            if is_symptomatic {
                ip.is_symptomatic = is_symptomatic;
                ip.days_diseased = d;
            }
            self.state = HealthParam::Infected(ip);
            self.transition = None;
            is_symptomatic
        }

        pub fn force_recovered(&mut self, days_recovered: f64) {
            let mut rcp = RecoverParam::new(0.0, 0);
            rcp.days_recovered = days_recovered;
            self.state = HealthParam::Recovered(rcp);
            self.transition = None;
        }

        pub fn become_infected(&self) -> bool {
            matches!(self.transition, Some(HealthTransition::Asymptomatic { .. }))
        }

        pub fn get_immune_factor(&self, bip: &InfectionParam, pfs: &ParamsForStep) -> Option<f64> {
            let immune_factor = match &self.state {
                HealthParam::Unfortified => 0.0,
                HealthParam::Recovered(rp) => {
                    rp.immunity * pfs.vr_info[rp.virus_variant].efficacy[bip.virus_variant]
                }
                HealthParam::Vaccinated(vp) => {
                    vp.immunity * pfs.vx_info[vp.vaccine_type].efficacy[bip.virus_variant]
                }
                _ => return None,
            };
            Some(immune_factor)
        }

        pub fn get_immunity(&self) -> Option<f64> {
            match &self.state {
                HealthParam::Unfortified => Some(0.0),
                HealthParam::Infected(ip) if !ip.is_symptomatic => Some(ip.immunity),
                HealthParam::Vaccinated(vp) => Some(vp.immunity),
                _ => None,
            }
        }

        pub fn is_infected(&self) -> Option<&InfectionParam> {
            match &self.state {
                HealthParam::Infected(ip) => Some(ip),
                _ => None,
            }
        }

        pub fn is_symptomatic(&self) -> bool {
            matches!(&self.state, HealthParam::Infected(ip) if ip.is_symptomatic)
        }

        pub fn set_transition(&mut self, new: Option<HealthTransition>) {
            if new.is_some() {
                self.transition = new;
            }
        }

        pub fn update_health(
            &mut self,
            days_to: &mut DaysTo,
            pfs: &ParamsForStep,
        ) -> Option<HealthDiff> {
            if let Some(t) = self.transition.take() {
                let from = (&self.state).into();
                let to = (&t).into();
                use std::ptr;
                unsafe {
                    let tmp = ptr::read(&mut self.state);
                    let mut ip = None;
                    match tmp {
                        HealthParam::Vaccinated(vp) => {
                            self.vaccination = Some(vp);
                        }
                        HealthParam::Infected(_ip) => ip = Some(_ip),
                        _ => {}
                    }
                    let new = match t {
                        HealthTransition::Susceptible => HealthParam::Unfortified,
                        HealthTransition::Asymptomatic {
                            immunity,
                            virus_variant,
                        } => HealthParam::Infected(InfectionParam::new(immunity, virus_variant)),
                        HealthTransition::Symptomatic => {
                            let mut ip = ip.unwrap();
                            ip.is_symptomatic = true;
                            HealthParam::Infected(ip)
                        }
                        HealthTransition::Recovered => HealthParam::Recovered(RecoverParam::new(
                            days_to.setup_acquired_immunity(&pfs.rp),
                            ip.unwrap().virus_variant,
                        )),
                        HealthTransition::Vaccinated {
                            vaccine_type,
                            immunity,
                        } => HealthParam::Vaccinated(self.vaccinate(
                            days_to,
                            immunity,
                            vaccine_type,
                            pfs,
                        )),
                        HealthTransition::Died => HealthParam::Died,
                    };
                    ptr::write(&mut self.state, new);
                }

                Some(HealthDiff::new(from, to))
            } else {
                None
            }
        }

        pub fn field_step<R>(
            &mut self,
            days_to: &mut DaysTo,
            activeness: f64,
            age: f64,
            hist: &mut Option<HistInfo>,
            pfs: &ParamsForStep,
        ) -> ControlFlow<Either<WarpParam, R>> {
            let transition = self.try_vaccinate().or_else(|| match &mut self.state {
                HealthParam::Infected(ip) => ip.step(days_to, &self.vaccination, false, pfs, hist),
                HealthParam::Recovered(rp) => rp.step(days_to, activeness, age, pfs),
                HealthParam::Vaccinated(vp) => vp.step(days_to, activeness, age, pfs),
                _ => None,
            });

            self.set_transition(transition);
            if let Some(HealthTransition::Died) = self.transition {
                ControlFlow::Break(Either::Left(WarpParam::cemetery(pfs.wp)))
            } else {
                ControlFlow::Continue(())
            }
        }

        pub fn hospital_step(
            &mut self,
            days_to: &mut DaysTo,
            origin: Point,
            hist: &mut Option<HistInfo>,
            pfs: &ParamsForStep,
        ) -> Option<WarpParam> {
            if let HealthParam::Infected(ip) = &mut self.state {
                let t = ip.step(days_to, &self.vaccination, true, pfs, hist);
                self.set_transition(t);
                match self.transition {
                    Some(HealthTransition::Died) => Some(WarpParam::cemetery(pfs.wp)),
                    Some(HealthTransition::Recovered) => Some(WarpParam::back(origin)),
                    _ => None,
                }
            } else {
                Some(WarpParam::back(origin))
            }
        }

        fn try_vaccinate(&mut self) -> Option<HealthTransition> {
            let vaccine_type = self.vaccine_ticket.take()?;
            let immunity = self.get_immunity()?;
            Some(HealthTransition::Vaccinated {
                vaccine_type,
                immunity,
            })
            // Some(HealthParam::Vaccinated(self.vaccinate(
            //     immunity,
            //     vaccine_type,
            //     pfs,
            // )))
        }

        fn vaccinate(
            &mut self,
            days_to: &mut DaysTo,
            immunity: f64,
            vaccine_type: usize,
            pfs: &ParamsForStep,
        ) -> VaccinationParam {
            // self.new_health = HealthType::Vaccinated;
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

#[derive(Default)]
pub struct DaysTo {
    recover: f64,
    onset: f64,
    die: f64,
    expire_immunity: f64,
}

impl DaysTo {
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

    fn expire_immunity(
        &mut self,
        activeness: f64,
        age: f64,
        pfs: &ParamsForStep,
    ) -> HealthTransition {
        // self.new_health = HealthType::Susceptible;
        // self.state = AgentState::Unfortified;
        self.alter_days(activeness, age, pfs);
        HealthTransition::Susceptible
    }

    fn setup_acquired_immunity(&mut self, rp: &RuntimeParams) -> f64 {
        let max_severity = self.recover * (1.0 - rp.therapy_effc.r()) / self.die;
        self.expire_immunity = 1.0f64.min(max_severity / (rp.imn_max_dur_sv.r())) * rp.imn_max_dur;
        1.0f64.min(max_severity / (rp.imn_max_effc_sv.r())) * rp.imn_max_effc.r()
    }
}

pub struct InfectionParam {
    pub virus_variant: usize,
    on_recovery: bool,
    severity: f64,
    days_diseased: f64,
    days_infected: f64,
    immunity: f64,
    is_symptomatic: bool,
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
            is_symptomatic: false,
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

    fn step(
        &mut self,
        days_to: &mut DaysTo,
        vp: &Option<VaccinationParam>,
        is_in_hospital: bool,
        pfs: &ParamsForStep,
        hist: &mut Option<HistInfo>,
    ) -> Option<HealthTransition> {
        self.days_infected += pfs.wp.days_per_step();
        if self.is_symptomatic {
            self.days_diseased += pfs.wp.days_per_step();
        }

        if self.on_recovery {
            self.severity -= 1.0 / MAX_DAYS_FOR_RECOVERY * pfs.wp.days_per_step();
            // recovered
            if self.severity <= 0.0 {
                if self.is_symptomatic {
                    // SET_HIST(hist_recov, days_diseased);
                    *hist = Some(HistInfo {
                        mode: HistgramType::HistRecov,
                        days: self.days_diseased,
                    });
                }
                // return Some(self.recovered(days_to, pfs));
                return Some(HealthTransition::Recovered);
            }
            return None;
        }

        let vr_info = &pfs.vr_info[self.virus_variant];
        let excrbt = exacerbation(vr_info.reproductivity);

        let days_to_recov = self.get_days_to_recov(days_to, is_in_hospital, pfs.rp);
        if !self.is_symptomatic {
            if self.days_infected < days_to.onset / excrbt {
                if self.days_infected > days_to_recov {
                    // return Some(self.recovered(days_to, pfs));
                    return Some(HealthTransition::Recovered);
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
            // self.new_health = HealthType::Died;
            // *to_cemetery = Some(WarpInfo::cemetery(pfs.wp));
            return Some(HealthTransition::Died);
        }

        if self.days_infected > days_to_recov {
            self.on_recovery = true;
        }

        if !self.is_symptomatic {
            // self.new_health = HealthType::Symptomatic;
            // self.is_symptomatic = true;
            // SET_HIST(hist_incub, days_infected)
            *hist = Some(HistInfo {
                mode: HistgramType::HistIncub,
                days: self.days_infected,
            });
            return Some(HealthTransition::Symptomatic);
        }

        None
    }

    // fn recovered(&self, days_to: &mut DaysTo, pfs: &ParamsForStep) -> HealthParam {
    //     // self.new_health = HealthType::Recovered;
    //     HealthParam::Recovered(RecoverParam::new(
    //         days_to.setup_acquired_immunity(&pfs.rp),
    //         self.virus_variant,
    //     ))
    // }

    fn get_days_to_recov(&self, days_to: &DaysTo, is_in_hospital: bool, rp: &RuntimeParams) -> f64 {
        let mut v = (1.0 - self.immunity) * days_to.recover;
        // equivalent to `self.is_in_hospital()`
        if is_in_hospital {
            v *= 1.0 - rp.therapy_effc.r()
        }
        v
    }
}

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
    ) -> Option<HealthTransition> {
        self.days_recovered += pfs.wp.days_per_step();
        if self.days_recovered > days_to.expire_immunity {
            Some(days_to.expire_immunity(activeness, age, pfs))
        } else {
            None
        }
    }
}

pub struct VaccinationParam {
    vaccine_type: usize,
    dose_date: f64,
    immunity: f64,
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
    ) -> Option<HealthTransition> {
        if let Some(i) = self.new_immunity(pfs) {
            self.immunity = i;
            None
        } else {
            Some(days_to.expire_immunity(activeness, age, pfs))
        }
        // let vp = self.vaccination_param.unwrap();
        // self.state = AgentState::Vaccinated(vp);
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
        // if self.health == HealthType::Symptomatic {
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
    // vaccine_ticket: Option<usize>,
    // gat_dist: Option<f64>,
    // pub idx: usize,
    // days_to_complete_recov: f64,
    // days_vaccinated: f64,
    // new_n_infects: i32,
    // pub is_warping: bool,
    // pub motion: Transfer,
    // pub got_at_hospital: bool,
    // best_dist: f64,
    // infection_param: Option<InfectionParam>,
    // first_dose_date: f64,
    // vaccine_type: Option<usize>,
    // pub virus_variant: Option<usize>,
}

impl AgentCore {
    fn new(wp: &WorldParams, rp: &RuntimeParams, id: usize, distancing: bool) -> Self {
        let mut a = Self::default();
        a.reset(wp, rp, id, distancing);
        a
    }

    pub fn reset(&mut self, wp: &WorldParams, rp: &RuntimeParams, id: usize, distancing: bool) {
        // self.days_to_complete_recov = 0.0;
        // self.last_tested = -999999;
        let rng = &mut rand::thread_rng();
        self.n_infects = 0;
        self.is_out_of_field = true;
        self.reset_days(wp, rp);

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

    pub fn get_pt(&self) -> &Point {
        &self.body.pt
    }

    pub fn reset_days(&mut self, wp: &WorldParams, rp: &RuntimeParams) {
        self.days_to = DaysTo::new(self.activeness, self.age, wp, rp);
        // self.days_to.expire_immunity = random::my_random(rng, &rp.immun);
    }

    pub fn force_unfortified(&mut self) {
        // self.health = HealthType::Susceptible;
        // self.new_health = self.health;
        self.health.force_unfortified();
    }
    pub fn force_infected(&mut self) -> bool {
        self.health.force_infected(&self.days_to)
    }

    pub fn force_recovered(&mut self, rp: &RuntimeParams) {
        let rng = &mut rand::thread_rng();
        self.days_to.expire_immunity = rng.gen::<f64>() * rp.imn_max_dur;
        let days_recovered = rng.gen::<f64>() * self.days_to.expire_immunity;
        self.health.force_recovered(days_recovered);
    }

    pub fn reset_for_step(&mut self) {
        // self.best_dist = f64::MAX; // BIG_NUM;
        // self.new_health = self.health;
    }

    // fn update_best(&self, curr_best: &mut Option<(Point, f64)>, b: &Self) {
    //     let x = {
    //         let x = (b.app - self.prf).abs();
    //         (if x < 0.5 { x } else { 1.0 - x }) * 2.0
    //     };

    //     match curr_best {
    //         None => *curr_best = Some((b.pt, x)),
    //         Some((_, dist)) if *dist > x => *curr_best = Some((b.pt, x)),
    //         _ => {}
    //     }
    // }

    fn try_infect(&self, b: &mut Self, d: f64, pfs: &ParamsForStep) -> bool {
        fn infect(
            a: &AgentCore,
            b: &AgentCore,
            d: f64,
            pfs: &ParamsForStep,
        ) -> Option<HealthTransition> {
            let ip = a.health.is_infected()?;
            let immunity = b.health.get_immune_factor(ip, pfs)?;
            if ip.check_infection(immunity, d, a.days_to.onset, pfs) {
                Some(HealthTransition::Asymptomatic {
                    immunity,
                    virus_variant: ip.virus_variant,
                })
            } else {
                None
            }
        }

        if !b.health.become_infected() {
            if let Some(t) = infect(self, b, d, pfs) {
                b.health.set_transition(Some(t));
                return true;
            }
        }
        false
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
        if let Some(ip) = self.is_infected() {
            if ip.is_symptomatic
                && ip.days_diseased >= rp.tst_delay
                && was_hit(wp.days_per_step(), rp.tst_sbj_sym.r())
            {
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

    fn try_warp_inside<R>(&self, pfs: &ParamsForStep) -> ControlFlow<Either<WarpParam, R>> {
        if self.health.is_symptomatic() {
            return ControlFlow::Continue(());
        }
        if let Some(goal) = self.get_warp_inside_goal(pfs) {
            return ControlFlow::Break(Either::Left(WarpParam::inside(goal)));
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
        f += best_point_force(
            &self.body.pt,
            &interaction.best.map(|(p, _)| p),
            self.distancing,
            pfs.wp.field_size(),
        );
        (f, gat_dist)
    }

    fn move_internal<L>(
        &mut self,
        interaction: &InteractionUpdate,
        idx: &TableIndex,
        pfs: &ParamsForStep,
    ) -> ControlFlow<Either<L, TableIndex>> {
        let (f, gat_dist) = self.calc_force(interaction, pfs);
        self.body
            .field_update(self.health.is_symptomatic(), f, &gat_dist, pfs);
        let new_idx = pfs.wp.into_grid_index(&self.body.pt);
        if *idx != new_idx {
            ControlFlow::Break(Either::Right(new_idx))
        } else {
            ControlFlow::Continue(())
        }
    }

    fn try_quarantine<R>(
        &mut self,
        pfs: &ParamsForStep,
        test: &mut Option<Either<TestReason, Vec<Testee>>>,
    ) -> ControlFlow<Either<WarpParam, R>> {
        if self.quarantine_reserved {
            // prms.rp.trc_ope != TrcTst
            let old_time_stamp = pfs.rp.step - pfs.wp.steps_per_day * 14; // two weeks
            let testees = self
                .contacts
                .drain()
                .filter_map(|ci| {
                    if ci.time_stamp <= old_time_stamp {
                        None
                    } else if ci.agent.lock().unwrap().is_testable(pfs.wp, pfs.rp) {
                        Some(Testee::new(ci.agent, TestReason::AsContact, pfs))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            *test = Some(Either::Right(testees));
            if let WrkPlcMode::WrkPlcNone = pfs.wp.wrk_plc_mode {
                self.origin = self.body.pt;
            }
            ControlFlow::Break(Either::Left(WarpParam::hospital(pfs.wp)))
        } else {
            ControlFlow::Continue(())
        }
    }

    // fn step_warp(&mut self, goal: &Point, pfs: &ParamsForStep) -> bool {
    //     let dp = *goal - self.pt;
    //     let d = dp.y.hypot(dp.x);
    //     let v = pfs.wp.field_size() / 5.0 * pfs.wp.days_per_step();
    //     if d < v {
    //         self.pt = *goal;
    //         true
    //     } else {
    //         let th = dp.y.atan2(dp.x);
    //         self.pt.x += v * th.cos();
    //         self.pt.y += v * th.sin();
    //         false
    //     }
    // }

    fn update_health(&mut self, pfs: &ParamsForStep) -> Option<HealthDiff> {
        self.health.update_health(&mut self.days_to, pfs)
    }
    // fn update_health(&mut self) -> HealthInfo {
    //     if self.health != self.new_health {
    //         self.health = self.new_health;
    //         HealthInfo::Tran(self.health)
    //     } else {
    //         HealthInfo::Stat(self.health)
    //     }
    // }

    pub fn reserve_quarantine(&mut self) {
        self.quarantine_reserved = true;
    }

    pub fn finish_test(&mut self, time_stamp: u64) {
        self.test_reserved = false;
        self.last_tested = Some(time_stamp);
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

    pub fn is_infected(&self) -> Option<&InfectionParam> {
        self.health.is_infected()
    }

    pub fn is_testable(&self, wp: &WorldParams, rp: &RuntimeParams) -> bool {
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

    pub fn is_in_field(&self) -> bool {
        matches!(self.location, Location::Field)
    }
}

pub type Agent = Arc<Mutex<AgentCore>>;
pub fn new_agent(wp: &WorldParams, rp: &RuntimeParams) -> Agent {
    Arc::new(Mutex::new(AgentCore::new(wp, rp, 0, false)))
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
        agent.lock().unwrap().location = Self::LABEL;
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

    fn update_best(&mut self, a: &Body, b: &Body) {
        let x = a.calc_dist(b);
        match self.best {
            None => self.best = Some((b.pt, x)),
            Some((_, dist)) if dist > x => self.best = Some((b.pt, x)),
            _ => {}
        }
    }

    fn record_contact(&mut self, b: &Agent, d: f64, pfs: &ParamsForStep) {
        if is_contacted(d, pfs.wp, pfs.rp) {
            self.new_contacts.push(Arc::clone(b));
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
    transfer: ControlFlow<Either<WarpParam, TableIndex>>,
    test: Option<Either<TestReason, Vec<Testee>>>,
    hist: Option<HistInfo>,
    infct: Option<InfectionCntInfo>,
    health: Option<HealthDiff>,
    agent: Agent,
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
        self.interaction = InteractionUpdate::new();
        self.agent.lock().unwrap().reset_for_step();
    }

    fn step(&mut self, pfs: &ParamsForStep) -> (bool, FieldStepInfo) {
        fn local_step(
            agent: &mut AgentCore,
            interaction: &InteractionUpdate,
            idx: &TableIndex,
            pfs: &ParamsForStep,
            hist: &mut Option<HistInfo>,
            test: &mut Option<Either<TestReason, Vec<Testee>>>,
        ) -> ControlFlow<Either<WarpParam, TableIndex>> {
            agent.try_quarantine(pfs, test)?;
            agent
                .health
                .field_step(&mut agent.days_to, agent.activeness, agent.age, hist, pfs)?;
            // agent.step_by_health(hist, pfs)?;
            *test = agent.check_test(pfs.wp, pfs.rp).map(Either::Left);
            agent.try_warp_inside(pfs)?;
            agent.move_internal(&interaction, &idx, pfs)
        }

        let agent = &mut self.agent.lock().unwrap();
        let mut hist = None;
        let mut test = None;
        let transfer = local_step(
            agent,
            &self.interaction,
            &self.idx,
            pfs,
            &mut hist,
            &mut test,
        );

        let fsi = FieldStepInfo {
            transfer,
            test,
            hist,
            infct: agent.update_n_infects(self.interaction.new_n_infects),
            health: agent.update_health(pfs),
            agent: Arc::clone(&self.agent),
        };
        (fsi.transfer.is_break(), fsi)
    }

    fn interacts(&mut self, fb: &mut Self, pfs: &ParamsForStep) {
        let a = &mut self.agent.lock().unwrap();
        let b = &mut fb.agent.lock().unwrap();
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
    agent: Agent,
    warp: Option<WarpParam>,
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

    fn step(&mut self, pfs: &ParamsForStep) -> (bool, HospitalStepInfo) {
        fn local_step(
            agent: &mut AgentCore,
            // days_to: &mut DaysTo,
            hist: &mut Option<HistInfo>,
            pfs: &ParamsForStep,
        ) -> Option<WarpParam> {
            agent
                .health
                .hospital_step(&mut agent.days_to, agent.origin, hist, pfs)
        }
        let agent = &mut self.agent.lock().unwrap();
        let mut hist = None;
        let warp = local_step(agent, &mut hist, pfs);
        (
            warp.is_some(),
            HospitalStepInfo {
                agent: Arc::clone(&self.agent),
                warp,
                hist,
                health: agent.update_health(pfs),
            },
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

    fn step(&mut self, pfs: &ParamsForStep) -> bool {
        self.agent
            .lock()
            .unwrap()
            .body
            .warp_update(self.param.goal, pfs.wp)
    }

    fn transfer(
        self,
        field: &mut Field,
        hospital: &mut Hospital,
        cemetery: &mut Cemetery,
        wp: &WorldParams,
    ) {
        let WarpAgent {
            agent,
            param: WarpParam { mode, goal },
        } = self;
        match mode {
            WarpMode::Back => field.add(agent, wp.into_grid_index(&goal)),
            WarpMode::Inside => field.add(agent, wp.into_grid_index(&goal)),
            WarpMode::Hospital => hospital.add(agent),
            WarpMode::Cemetery => cemetery.add(agent),
        }
    }
}

// pub fn cummulate_histgrm(h: &mut Vec<MyCounter>, d: f64) {
//     let ds = d.floor() as usize;
//     if h.len() <= ds {
//         let n = ds - h.len();
//         for _ in 0..=n {
//             h.push(MyCounter::new());
//         }
//     }
//     h[ds].inc();
// }

pub mod cont {
    use super::{Agent, FieldAgent, HospitalAgent, ParamsForStep, WarpAgent, WarpParam};
    use crate::{
        commons::{math::Percentage, DistInfo, DrainLike, DrainMap, Either},
        gathering::Gathering,
        log::StepLog,
        table::{Table, TableIndex},
        testing::TestQueue,
    };
    use rayon::iter::ParallelIterator;
    use std::{
        ops::ControlFlow,
        sync::{Arc, Mutex},
    };

    pub struct Field {
        table: Table<Vec<FieldAgent>>,
        // count: usize,
    }

    impl Field {
        pub fn new(mesh: usize) -> Self {
            Self {
                table: Table::new(mesh, mesh, Vec::new),
                // count: 0,
            }
        }

        pub fn clear(&mut self) {
            // fixed mesh size
            self.table
                .par_iter_mut()
                .horizontal()
                .for_each(|(_, c)| c.clear());
            // self.count = 0;
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
            // step
            let tmp = self
                .table
                .par_iter_mut()
                .horizontal()
                .map(|(_, ags)| ags.drain_map_mut(|fa| fa.step(pfs)))
                .collect::<Vec<_>>();

            for fsi in tmp.into_iter().flatten() {
                if let Some(h) = fsi.hist {
                    step_log.hists.push(h);
                }
                if let Some(h) = fsi.health {
                    step_log.apply_difference(h);
                }
                if let Some(i) = fsi.infct {
                    step_log.infcts.push(i);
                }
                if let Some(test) = fsi.test {
                    match test {
                        Either::Left(reason) => {
                            test_queue.push(Arc::clone(&fsi.agent), reason, pfs);
                        }
                        Either::Right(contact_testees) => test_queue.extend(contact_testees),
                    }
                }
                if let ControlFlow::Break(t) = fsi.transfer {
                    let agent = Arc::clone(&fsi.agent);
                    match t {
                        Either::Left(warp) => {
                            warps.add(agent, warp);
                            // self.count += 1;
                        }
                        Either::Right(idx) => self.add(agent, idx),
                    }
                }
            }
        }

        pub fn add(&mut self, agent: Agent, idx: TableIndex) {
            self.table[idx.clone()].push(FieldAgent::new(agent, idx));
        }

        pub fn intersect(&mut self, pfs: &ParamsForStep) {
            // let mesh = self.world_params.mesh as usize;
            // |x|a|b|a|b|..
            self.table
                .par_iter_mut()
                .east()
                .for_each(move |((_, a_ags), (_, b_ags))| {
                    Self::interact_intercells(a_ags, b_ags, pfs);
                });
            // |a|b|a|b|..
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

            for hsi in tmp.into_iter() {
                if let Some(h) = hsi.hist {
                    step_log.hists.push(h);
                }
                if let Some(h) = hsi.health {
                    step_log.apply_difference(h);
                }
                if let Some(param) = hsi.warp {
                    warps.add(Arc::clone(&hsi.agent), param);
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
            pfs: &ParamsForStep,
        ) {
            let tmp = self.0.drain_mut(|a| a.step(pfs));
            for wa in tmp.into_iter() {
                wa.transfer(field, hospital, cemetery, pfs.wp);
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
