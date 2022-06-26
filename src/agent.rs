use crate::{
    commons::{
        self,
        container::table::TableIndex,
        math::Point,
        random::{self, modified_prob, ActivenessEffect},
        DistInfo, Either, HealthType, MyCounter, RuntimeParams, WorldParams, WrkPlcMode,
    },
    contact::Contacts,
    gathering::Gathering,
    stat::{HealthInfo, HistInfo, HistgramType, InfectionCntInfo},
    testing::{TestReason, Testee},
};
use rand::{self, Rng};
use std::{
    f64,
    ops::{ControlFlow, DerefMut},
    sync::{Arc, Mutex, Weak},
};

use self::cont::{Cemetery, Field, Hospital};

const VCN_1_2_SPAN: f64 = 21.0;
const AGENT_RADIUS: f64 = 0.75;
// static AGENT_SIZE: f64 = 0.665;
const AVOIDANCE: f64 = 0.2;
const MAX_DAYS_FOR_RECOVERY: f64 = 7.0;
const TOXICITY_LEVEL: f64 = 0.5;

const BACK_HOME_RATE: bool = true;

pub struct VariantInfo {
    pub reproductivity: f64,
    toxicity: f64,
    efficacy: [f64; MAX_N_VARIANTS],
}

pub struct VaccineInfo {
    interval: usize,
    efficacy: [f64; MAX_N_VARIANTS],
}

pub const MAX_N_VAXEN: usize = 8;
pub const MAX_N_VARIANTS: usize = 8;

pub struct ParamsForStep<'a> {
    pub rp: &'a RuntimeParams,
    pub wp: &'a WorldParams,
    pub vr_info: &'a [VariantInfo],
    pub vx_info: &'a [VaccineInfo],
}

impl<'a> ParamsForStep<'a> {
    pub fn new(
        rp: &'a RuntimeParams,
        wp: &'a WorldParams,
        vr_info: &'a [VariantInfo],
        vx_info: &'a [VaccineInfo],
    ) -> Self {
        ParamsForStep {
            rp,
            wp,
            vr_info,
            vx_info,
        }
    }
}

/// local functions
fn exacerbation(reproductivity: f64) -> f64 {
    reproductivity.powf(1.0 / 3.0)
}

#[derive(Default)]
pub struct AgentParam {
    pub id: usize,
    // pub idx: usize,
    app: f64,
    prf: f64,
    pub pt: Point,
    // pub x: f64,
    // pub y: f64,
    pub v: Point,
    // pub vx: f64,
    // pub vy: f64,
    pub f: Point,
    // pub fx: f64,
    // pub fy: f64,
    pub org_pt: Point,
    pub days_infected: f64,
    pub days_diseased: f64,
    pub days_to_recover: f64,
    pub days_to_onset: f64,
    pub days_to_die: f64,
    pub im_expr: f64,
    pub health: HealthType,
    pub new_health: HealthType,
    pub n_infects: i32,
    pub new_n_infects: i32,
    pub distancing: bool,
    pub is_out_of_field: bool,
    // pub is_warping: bool,
    // pub motion: Transfer,
    // pub got_at_hospital: bool,
    pub best_pt: Option<Point>,
    best_dist: f64,
    pub gathering: Weak<Mutex<Gathering>>,
    pub activeness: f64,
    pub mob_freq: f64,
    pub gat_freq: f64,
    pub vaccine_ticket: bool,
    days_to_complete_recov: f64,
    days_vaccinated: f64,
    agent_immunity: f64,
    first_dose_date: f64,
    vaccine_type: usize,
    on_recovery: bool,
    severity: f64,
    pub virus_variant: usize,
    gat_dist: f64,
    location: Area,
    // pub contact_info_list: VecDeque<MRef<ContactInfo>>,
    // pub _contact_info_list: VecDeque<ContactInfo>,
    contacts: Contacts,
    test_reserved: bool,
    last_tested: Option<u64>,
    quarantine_reserved: bool,
}

impl AgentParam {
    pub fn reset(&mut self, world_size: f64, rp: &RuntimeParams) {
        let rng = &mut rand::thread_rng();
        self.app = rng.gen();
        self.prf = rng.gen();
        self.pt.x = rng.gen::<f64>() * (world_size - 6.0) + 3.0;
        self.pt.y = rng.gen::<f64>() * (world_size - 6.0) + 3.0;
        let th: f64 = rng.gen::<f64>() * f64::consts::PI * 2.0;
        self.v.x = th.cos();
        self.v.y = th.sin();
        self.health = HealthType::Susceptible;
        self.n_infects = -1;
        self.is_out_of_field = true;
        self.reset_days(rng, rp);

        // self.days_to_complete_recov = 0.0;
        // self.last_tested = -999999;
        self.activeness = random::random_mk(rng, rp.act_mode / 100.0, rp.act_kurt / 100.0);
        self.gathering = Weak::new();
        let ae = ActivenessEffect::new(self.activeness, rp.act_mode / 100.0);
        let d_info = DistInfo::new(0.0, 0.5, 1.0);
        self.mob_freq = random::random_with_corr(rng, &d_info, &ae, rp.mob_act / 100.0);
        self.gat_freq = random::random_with_corr(rng, &d_info, &ae, rp.gat_act / 100.0);
    }

    pub fn reset_days<R: Rng>(&mut self, rng: &mut R, rp: &RuntimeParams) {
        self.days_to_recover = random::my_random(rng, &rp.recov);
        self.days_to_onset = random::my_random(rng, &rp.incub);
        self.days_to_die = random::my_random(rng, &rp.fatal) + self.days_to_onset;
        self.im_expr = random::my_random(rng, &rp.immun);
    }

    pub fn reset_for_step(&mut self) {
        self.f.x = 0.;
        self.f.y = 0.;
        self.best_pt = None;
        self.best_dist = f64::MAX; // BIG_NUM;
        self.new_health = self.health;
    }

    fn try_record_contact(&mut self, br: Agent, d: f64, wp: &WorldParams, rp: &RuntimeParams) {
        if d < rp.infec_dst && was_hit(wp.steps_per_day(), rp.cntct_trc.r()) {
            self.contacts.add(br, rp.step);
        }
    }

    fn check_infection(&self, b: &Self, d: f64, prms: &ParamsForStep) -> Option<f64> {
        // check contact and infection
        if self.new_health != self.health || !b.is_infected() {
            return None;
        }

        let virus_x = prms.vr_info[b.virus_variant].reproductivity;
        let infec_d_max = prms.rp.infec_dst * virus_x.sqrt();
        if d > infec_d_max {
            return None;
        }

        let exacerbate = exacerbation(virus_x);
        let contag_delay = prms.rp.contag_delay / exacerbate;
        let contag_peak = prms.rp.contag_peak / exacerbate;

        if b.days_infected <= contag_delay {
            return None;
        }

        let immune_factor = match self.health {
            HealthType::Susceptible => 0.0,
            HealthType::Recovered => {
                self.agent_immunity * prms.vr_info[self.virus_variant].efficacy[b.virus_variant]
            }
            HealthType::Vaccinated => {
                self.agent_immunity * prms.vx_info[self.vaccine_type].efficacy[b.virus_variant]
            }
            _ => return None,
        };

        let time_factor = 1f64.min(
            (b.days_infected - contag_delay) / (contag_peak.min(b.days_to_onset) - contag_delay),
        );
        let distance_factor = 1f64.min(((infec_d_max - d) / 2.0).powf(2.0));
        let infec_prob = if virus_x < 1.0 {
            prms.rp.infec.r() * virus_x
        } else {
            1.0 - (1.0 - prms.rp.infec.r()) / virus_x
        };

        if !was_hit(
            prms.wp.steps_per_day(),
            infec_prob * time_factor * distance_factor * (1.0 - immune_factor),
        ) {
            return None;
        }

        Some(immune_factor)
    }

    fn try_infect(&mut self, b: &mut Self, d: f64, prms: &ParamsForStep) {
        if let Some(immune_factor) = self.check_infection(b, d, prms) {
            self.new_health = HealthType::Asymptomatic;
            self.agent_immunity = immune_factor;
            self.virus_variant = b.virus_variant;
            self.days_infected = 0.0;
            self.days_diseased = 0.0;
            if self.n_infects < 0 {
                self.new_n_infects = 1;
            }
            b.new_n_infects += 1;
        }
    }

    fn update_best_pt(&mut self, b: &Self, d: f64) {
        let x = {
            let x = (b.app - self.prf).abs();
            (if x < 0.5 { x } else { 1.0 - x }) * 2.0
        };
        if self.best_dist > x {
            self.best_dist = x;
            self.best_pt = Some(b.pt);
        }
    }

    const HOMING_FORCE: f64 = 0.2;
    const MAX_HOMING_FORCE: f64 = 2.0;
    const MIN_AWAY_TO_HOME: f64 = 50.0;
    fn going_back_home(&mut self) {
        let f = (self.org_pt - self.pt) * Self::HOMING_FORCE;
        let fa = f.x.hypot(f.y);
        if fa > Self::MIN_AWAY_TO_HOME * Self::HOMING_FORCE {
            return;
        }
        if fa > Self::MAX_HOMING_FORCE {
            f *= Self::MAX_HOMING_FORCE / fa;
        }
        self.f += f;
    }

    fn affected_by_gathering(&mut self) {
        if let Some(g) = self.gathering.upgrade() {
            let g = g.lock().unwrap();
            if let Some((d, df)) = g.affect(&self.pt) {
                self.f += df;
                self.gat_dist = d;
            }
        }
    }

    fn update_force(&mut self, b: &mut Self, wp: &WorldParams, rp: &RuntimeParams) -> Option<f64> {
        let dp = b.pt - self.pt;
        let d2 = (dp.x * dp.x + dp.y * dp.y).max(1e-4);
        let d = d2.sqrt();
        let view_range = wp.view_range();
        if d >= view_range {
            return None;
        }

        let dd = (if d < view_range * 0.8 {
            1.0
        } else {
            (1.0 - d / view_range) / 0.2
        }) / d
            / d2
            * AVOIDANCE
            * rp.avoidance
            / 50.0;
        let ac = dp * dd;

        self.f -= ac;
        b.f += ac;

        Some(d)
    }

    pub fn is_infected(&self) -> bool {
        self.health == HealthType::Asymptomatic || self.health == HealthType::Symptomatic
    }

    pub fn get_new_pt(&self, world_size: f64, mob_dist: &DistInfo) -> Point {
        let rng = &mut rand::thread_rng();
        let dst = random::my_random(rng, mob_dist) * world_size / 100.;
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

    fn new_force(&self, best: Option<Point>, prms: &ParamsForStep) -> Point {
        let mut new_f = self.f;
        let ws = prms.wp.field_size();
        if self.distancing {
            let dst = 1.0 + prms.rp.dst_st / 5.0;
            new_f *= dst;
        }
        new_f += self.pt.map(wall) - self.pt.map(|p| wall(ws - p));
        if let (Some(bp), false) = (best, self.distancing) {
            let dp = bp - self.pt;
            let d = dp.x.hypot(dp.y).max(0.01) * 20.0;
            new_f += dp / d;
        }
        new_f
    }

    fn new_velocity(&self, prms: &ParamsForStep) -> Point {
        let spd = prms.wp.steps_per_day();
        let mass = {
            let v = prms.rp.mass.r();
            if self.health == HealthType::Symptomatic {
                v * 20.0
            } else {
                v
            }
        };

        let fric = {
            let v = ((1.0 - prms.rp.friction.r()) * 0.99).powf(prms.wp.days_per_step());
            if self.gat_dist < 1.0 {
                v * self.gat_dist * 0.5 + 0.5
            } else {
                v
            }
        };

        let mut new_v = self.v * fric + self.f / mass / spd;

        let v = new_v.x.hypot(new_v.y);
        let max_v = prms.rp.max_speed * 20.0 * prms.wp.days_per_step();
        if v > max_v {
            new_v *= max_v / v;
        }
        new_v
    }

    fn new_pos(&self, wp: &WorldParams) -> Point {
        let new_pt = self.pt;
        new_pt + self.v * wp.days_per_step()
    }

    fn update_position(&mut self, prms: &ParamsForStep) {
        self.f = self.new_force(self.best_pt, prms);
        self.v = self.new_velocity(prms);
        self.pt = self.new_pos(&prms.wp);

        let ws = prms.wp.field_size();
        if self.pt.x < AGENT_RADIUS {
            self.pt.x = AGENT_RADIUS * 2.0 - self.pt.x;
            self.v.x = -self.v.x;
        } else if self.pt.x > ws - AGENT_RADIUS {
            self.pt.x = (ws - AGENT_RADIUS) * 2.0 - self.pt.x;
            self.v.x = -self.v.x;
        }
        if self.pt.y < AGENT_RADIUS {
            self.pt.y = AGENT_RADIUS * 2.0 - self.pt.y;
            self.v.y = -self.v.y;
        } else if self.pt.y > ws - AGENT_RADIUS {
            self.pt.y = (ws - AGENT_RADIUS) * 2. - self.pt.y;
            self.v.y = -self.v.y;
        }
    }

    fn setup_acquired_immunity(&self, rp: &RuntimeParams) {
        todo!();
    }

    fn vax_sv_effc(&self, prms: &ParamsForStep) -> f64 {
        if self.first_dose_date < 0.0 {
            1.0
        } else {
            let days_vaccinated = self.days_vaccinated(prms);
            let span = self.vx_span(prms);
            let vcn_sv_effc = prms.wp.vcn_sv_effc.r();
            if days_vaccinated < span {
                1.0 + days_vaccinated / span * vcn_sv_effc
            } else if days_vaccinated < prms.wp.vcn_e_delay + span {
                (1.0 / (1.0 - vcn_sv_effc) - 1.0 - vcn_sv_effc) * (days_vaccinated - span)
                    / prms.wp.vcn_e_delay
                    + 1.0
                    + vcn_sv_effc
            } else {
                1.0 / (1.0 - vcn_sv_effc)
            }
        }
    }

    fn recoverd(&mut self, prms: &ParamsForStep) {
        self.new_health = HealthType::Recovered;
        self.days_infected = 0.0;
        self.on_recovery = false;
        self.setup_acquired_immunity(&prms.rp);
    }

    fn alter_days(&self, prms: &ParamsForStep) {
        let rng = rand::thread_rng();
        self.reset_days(&mut rng, &prms.rp);
    }

    fn expire_immunity(&mut self, prms: &ParamsForStep) {
        self.new_health = HealthType::Susceptible;
        self.days_infected = 0.0;
        self.days_diseased = 0.0;
        self.alter_days(prms);
    }

    fn days_vaccinated(&self, prms: &ParamsForStep) -> f64 {
        prms.rp.step as f64 * prms.wp.days_per_step() - self.first_dose_date
    }

    fn vx_span(&self, prms: &ParamsForStep) -> f64 {
        prms.vx_info[self.vaccine_type].interval as f64
    }

    fn vaccine(&mut self, prms: &ParamsForStep) {
        self.new_health = HealthType::Vaccinated;
        self.vaccine_ticket = false;
        let fdd = prms.rp.step as f64 * prms.wp.days_per_step();

        if self.first_dose_date >= 0.0 {
            // booster shot
            self.first_dose_date = fdd - prms.vx_info[self.vaccine_type].interval as f64;
        } else {
            // first done
            self.days_to_recover *= 1.0 - prms.wp.vcn_effc_symp.r();
            self.first_dose_date = fdd;
        }
    }

    fn patient_step<R: Rng>(
        &mut self,
        rng: &mut R,
        prms: &ParamsForStep,
        hist: &mut Option<HistInfo>,
    ) -> ControlFlow<WarpInfo> {
        if self.on_recovery {
            self.severity -= 1.0 / MAX_DAYS_FOR_RECOVERY * prms.wp.days_per_step();
            // recovered
            if self.severity <= 0.0 {
                if let HealthType::Symptomatic = self.health {
                    // SET_HIST(hist_recov, days_diseased);
                    *hist = Some(HistInfo {
                        mode: HistgramType::HistRecov,
                        days: self.days_diseased,
                    });
                }
                self.severity = 0.0;
                self.recoverd(prms);
            }
            return ControlFlow::Continue(());
        }

        let vr_info = &prms.vr_info[self.virus_variant];
        let excrbt = exacerbation(vr_info.reproductivity);
        let days_to_recv = {
            let v = (1.0 - self.agent_immunity) * self.days_to_recover;
            // equivalent to `self.is_in_hospital()`
            if !self.is_in_field() {
                v * (1.0 - prms.rp.therapy_effc.r())
            } else {
                v
            }
        };

        if let HealthType::Asymptomatic = self.health {
            if self.days_infected < self.days_to_onset / excrbt {
                if self.days_infected > days_to_recv {
                    self.recoverd(prms);
                }
                return ControlFlow::Continue(());
            }
            // present symptom
            self.new_health = HealthType::Symptomatic;
            // SET_HIST(hist_incub, days_infected)
            *hist = Some(HistInfo {
                mode: HistgramType::HistIncub,
                days: self.days_infected,
            });
        }

        let d_svr = {
            let v = 1.0 / (self.days_to_die - self.days_to_onset) / self.vax_sv_effc(prms)
                * excrbt
                * prms.wp.days_per_step();
            if self.severity > TOXICITY_LEVEL {
                v * vr_info.toxicity
            } else {
                v
            }
        };

        self.severity += d_svr;

        // died
        if self.severity >= 1.0 {
            // SET_HIST(hist_death, days_diseased)
            *hist = Some(HistInfo {
                mode: HistgramType::HistDeath,
                days: self.days_diseased,
            });
            self.new_health = HealthType::Died;
            return ControlFlow::Break(WarpInfo::cemetery(prms.wp));
        }

        if self.days_infected > days_to_recv {
            self.on_recovery = true;
        }

        ControlFlow::Continue(())
    }

    fn check_health<R: Rng>(
        &mut self,
        rng: &mut R,
        prms: &ParamsForStep,
        // go_home_back: bool,
        hist: &mut Option<HistInfo>,
        // test: &mut Option<TestType>,
    ) -> ControlFlow<WarpInfo> {
        let rp = prms.rp;
        let wp = prms.wp;

        match self.health {
            HealthType::Susceptible => {
                if self.vaccine_ticket {
                    self.vaccine(prms);
                }
            }
            HealthType::Symptomatic => {
                self.days_infected += wp.days_per_step();
                self.days_diseased += wp.days_per_step();
                self.patient_step(rng, prms, hist)?;
            }
            HealthType::Asymptomatic => {
                if self.vaccine_ticket {
                    self.vaccine(&prms);
                } else {
                    self.days_infected += wp.days_per_step();
                    self.patient_step(rng, prms, hist)?;
                }
            }
            HealthType::Vaccinated => {
                if self.vaccine_ticket {
                    self.vaccine(prms);
                } else {
                    let days_vaccinated = self.days_vaccinated(prms);
                    let span = self.vx_span(prms);
                    if days_vaccinated < span {
                        // only the first dose
                        self.agent_immunity = days_vaccinated * wp.vcn_1st_effc.r() / span;
                    } else if days_vaccinated < wp.vcn_e_delay + span {
                        // not fully vaccinated yet
                        self.agent_immunity = ((wp.vcn_max_effc - wp.vcn_1st_effc)
                            * ((days_vaccinated - span) / wp.vcn_e_delay)
                            + wp.vcn_1st_effc)
                            .r();
                    } else if days_vaccinated < wp.vcn_e_delay + span + wp.vcn_e_decay {
                        self.agent_immunity = wp.vcn_max_effc.r();
                    } else if days_vaccinated < wp.vcn_e_decay + span + wp.vcn_e_period {
                        self.agent_immunity =
                            wp.vcn_e_decay + span + wp.vcn_e_period - days_vaccinated;
                    } else {
                        self.expire_immunity(prms);
                    }
                }
            }
            HealthType::Recovered => {
                self.days_infected += wp.days_per_step();
                if self.days_infected > self.im_expr {
                    self.expire_immunity(prms);
                }
            }
            _ => {}
        }

        ControlFlow::Continue(())
    }

    // call after `check_health`
    fn check_test(&self, wp: &WorldParams, rp: &RuntimeParams) -> Option<TestReason> {
        if !self.is_testable(wp, rp) {
            None
        } else if self.health == HealthType::Symptomatic
            && self.days_diseased >= rp.tst_delay
            && was_hit(wp.days_per_step(), rp.tst_sbj_sym.r())
        {
            Some(TestReason::AsSymptom)
        } else if was_hit(wp.days_per_step(), rp.tst_sbj_asy.r()) {
            Some(TestReason::AsSuspected)
        } else {
            None
        }
    }

    fn try_go_inside(
        &self,
        go_home_back: bool,
        wp: &WorldParams,
        rp: &RuntimeParams,
    ) -> ControlFlow<WarpInfo> {
        if self.health == HealthType::Symptomatic {
            return ControlFlow::Continue(());
        }

        let dp = self.pt - self.org_pt;
        let goal = {
            if BACK_HOME_RATE
                && go_home_back
                && dp.x.hypot(dp.y) > rp.mob_dist.min.max(Self::MIN_AWAY_TO_HOME)
            {
                if was_hit(wp.days_per_step() * 3.0, rp.back_hm_rt.r()) {
                    self.org_pt
                } else if was_hit(
                    wp.days_per_step(),
                    modified_prob(self.mob_freq, &rp.mob_freq) / 1000.0,
                ) {
                    self.get_new_pt(wp.field_size(), &rp.mob_dist)
                } else {
                    return ControlFlow::Continue(());
                }
            } else if was_hit(
                wp.days_per_step(),
                modified_prob(self.mob_freq, &rp.mob_freq) / 1000.0,
            ) {
                if go_home_back && dp.x.hypot(dp.y) > rp.mob_dist.min.max(Self::MIN_AWAY_TO_HOME) {
                    self.org_pt
                } else {
                    self.get_new_pt(wp.field_size(), &rp.mob_dist)
                }
            } else {
                return ControlFlow::Continue(());
            }
        };

        ControlFlow::Break(WarpInfo::inside(goal, wp))
    }

    fn update_health(&mut self) -> HealthInfo {
        if self.health != self.new_health {
            self.health = self.new_health;
            HealthInfo::Tran(self.health)
        } else {
            HealthInfo::Stat(self.health)
        }
    }

    fn update_n_infects(&mut self) -> Option<InfectionCntInfo> {
        if self.new_n_infects > 0 {
            let prev_n_infects = self.n_infects;
            self.n_infects += self.new_n_infects;
            Some(InfectionCntInfo::new(prev_n_infects, self.n_infects))
        } else {
            None
        }
    }

    fn _step_field(
        &mut self,
        idx: &TableIndex,
        prms: &ParamsForStep,
        contact_testees: &mut Option<Vec<Testee>>,
        hist: &mut Option<HistInfo>,
        test: &mut Option<TestReason>,
    ) -> ControlFlow<WarpInfo> {
        self.try_quarantine(prms, contact_testees)?;
        let go_home_back = commons::go_home_back(prms.wp, prms.rp);
        if go_home_back {
            self.going_back_home();
        } else {
            self.affected_by_gathering();
        }

        let rng = &mut rand::thread_rng();
        let rp = prms.rp;
        let wp = prms.wp;

        self.check_health(rng, prms, hist)?;
        *test = self.check_test(wp, rp);
        self.try_go_inside(go_home_back, wp, rp)
    }

    fn step_field(&mut self, idx: &TableIndex, prms: &ParamsForStep, fsi: &mut FieldStepInfo) {
        fsi.transfer = match self._step_field(
            idx,
            prms,
            &mut fsi.contact_testees,
            &mut fsi.hist,
            &mut fsi.test,
        ) {
            ControlFlow::Continue(_) => {
                self.update_position(prms);
                let new_idx = prms.wp.into_grid_index(&self.pt);
                if *idx != new_idx {
                    Some(Either::Right(new_idx))
                } else {
                    None
                }
            }
            ControlFlow::Break(w) => Some(Either::Left(w)),
        };
        fsi.infct = self.update_n_infects();
        fsi.health = Some(self.update_health());
    }

    fn step_hospital(
        &mut self,
        prms: &ParamsForStep,
        hist: &mut Option<HistInfo>,
    ) -> ControlFlow<WarpInfo> {
        let rng = &mut rand::thread_rng();
        match self.health {
            HealthType::Symptomatic => {
                self.days_diseased += prms.wp.days_per_step();
            }
            HealthType::Asymptomatic => {
                self.days_diseased += prms.wp.days_per_step();
                self.days_infected += prms.wp.days_per_step();
            }
            _ => return ControlFlow::Break(WarpInfo::back(self.org_pt, &prms.wp)),
        };
        self.patient_step(rng, prms, &mut hist)?;
        if self.health == HealthType::Recovered {
            ControlFlow::Break(WarpInfo::back(self.org_pt, &prms.wp))
        } else {
            ControlFlow::Continue(())
        }
    }

    fn step_warp(&mut self, goal: &Point, prms: &ParamsForStep) -> bool {
        let dp = *goal - self.pt;
        let d = dp.y.hypot(dp.x);
        let v = prms.wp.field_size() / 5.0 * prms.wp.days_per_step();
        if d < v {
            self.pt = *goal;
            true
        } else {
            let th = dp.y.atan2(dp.x);
            self.pt.x += v * th.cos();
            self.pt.y += v * th.sin();
            false
        }
    }

    fn try_quarantine(
        &mut self,
        prms: &ParamsForStep,
        contact_testees: &mut Option<Vec<Testee>>,
    ) -> ControlFlow<WarpInfo> {
        if self.quarantine_reserved {
            let testees = self.contacts.drain_agent(|a| {
                let ap = a.lock().unwrap();
                if ap.is_testable(prms.wp, prms.rp) {
                    Some(Testee::new(a, TestReason::AsContact, prms))
                } else {
                    None
                }
            });
            *contact_testees = Some(testees);
            if let WrkPlcMode::WrkPlcNone = prms.wp.wrk_plc_mode {
                self.org_pt = self.pt;
            }
            return ControlFlow::Break(WarpInfo::hospital(prms.wp));
        }
        ControlFlow::Continue(())
    }

    pub fn reserve_quarantine(&mut self) {
        self.quarantine_reserved = true;
    }

    pub fn is_testable(&self, wp: &WorldParams, rp: &RuntimeParams) -> bool {
        if !self.is_in_field() || self.test_reserved
        /* || self.for_vcn == VcnNoTest */
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

    pub fn finish_test(&mut self, time_stamp: u64) {
        self.test_reserved = false;
        self.last_tested = Some(time_stamp);
    }

    fn is_in_field(&self) -> bool {
        match self.location {
            Area::Field(_) => true,
            _ => false,
        }
    }
}

pub struct WarpInfo {
    area: Area,
    goal: Point,
}

impl WarpInfo {
    fn back(goal: Point, wp: &WorldParams) -> Self {
        Self {
            area: Area::Field(wp.into_grid_index(&goal)),
            goal,
        }
    }

    fn inside(goal: Point, wp: &WorldParams) -> Self {
        Self {
            area: Area::Field(wp.into_grid_index(&goal)),
            goal,
        }
    }

    pub fn hospital(wp: &WorldParams) -> Self {
        let rng = &mut rand::thread_rng();
        let goal = Point::new(
            (rng.gen::<f64>() * 0.248 + 1.001) * wp.field_size(),
            (rng.gen::<f64>() * 0.458 + 0.501) * wp.field_size(),
        );
        Self {
            area: Area::Cemetery,
            goal,
        }
    }

    fn cemetery(wp: &WorldParams) -> Self {
        let rng = &mut rand::thread_rng();
        let goal = Point::new(
            (rng.gen::<f64>() * 0.248 + 1.001) * wp.field_size(),
            (rng.gen::<f64>() * 0.468 + 0.001) * wp.field_size(),
        );

        Self {
            area: Area::Cemetery,
            goal,
        }
    }
}

#[derive(Debug)]
pub enum Area {
    Cemetery,
    Field(TableIndex),
    // Back(TableIndex),
    Hospital,
    // CemeteryF,
    // CemeteryH,
    Warp(Point),
}

impl Default for Area {
    fn default() -> Self {
        Self::Cemetery
    }
}

pub type Agent = Arc<Mutex<AgentParam>>;
pub type WAgent = Weak<Mutex<AgentParam>>;

pub struct FieldStepInfo {
    agent: Agent,
    contact_testees: Option<Vec<Testee>>,
    transfer: Option<Either<WarpInfo, TableIndex>>,
    test: Option<TestReason>,
    hist: Option<HistInfo>,
    infct: Option<InfectionCntInfo>,
    health: Option<HealthInfo>,
}

pub struct FieldTag;
pub struct FieldAgent(Agent);

impl FieldAgent {
    fn new(a: Agent, idx: TableIndex) -> Self {
        let ap = &mut a.lock().unwrap();
        ap.location = Area::Field(idx);
        Self(a)
    }

    fn agent_mut(&self) -> &mut AgentParam {
        self.0.lock().unwrap().deref_mut()
    }

    fn get_location(&self) -> &Area {
        &self.0.lock().unwrap().location
    }

    pub fn step(&mut self, idx: &TableIndex, prms: &ParamsForStep) -> (bool, FieldStepInfo) {
        let mut fsi = FieldStepInfo {
            agent: Arc::clone(&self.0),
            contact_testees: None,
            transfer: None,
            test: None,
            hist: None,
            infct: None,
            health: None,
        };
        let agent = self.agent_mut();
        agent.step_field(idx, prms, &mut fsi);
        (fsi.transfer.is_none(), fsi)
    }

    pub fn interacts(&self, fb: &Self, prms: &ParamsForStep) {
        let a = self.agent_mut();
        let b = fb.agent_mut();
        if let Some(d) = a.update_force(b, prms.wp, prms.rp) {
            a.update_best_pt(b, d);
            b.update_best_pt(a, d);

            a.try_infect(b, d, prms);
            b.try_infect(a, d, prms);

            a.try_record_contact(fb.0, d, prms.wp, prms.rp);
            b.try_record_contact(self.0, d, prms.wp, prms.rp);
        }
    }
}

pub struct HospitalAgent(Agent);

pub struct HospitalStepInfo {
    agent: Agent,
    warp: Option<WarpInfo>,
    hist: Option<HistInfo>,
    health: Option<HealthInfo>,
}

impl HospitalAgent {
    fn agent_mut(&self) -> &mut AgentParam {
        self.0.lock().unwrap().deref_mut()
    }

    fn get_location(&self) -> &Area {
        &self.0.lock().unwrap().location
    }

    pub fn step(&mut self, prms: &ParamsForStep) -> (bool, HospitalStepInfo) {
        let agent = self.agent_mut();
        let mut hist = None;
        let warp = {
            match agent.step_hospital(prms, &mut hist) {
                ControlFlow::Continue(_) => None,
                ControlFlow::Break(t) => Some(t),
            }
        };

        (
            warp.is_none(),
            HospitalStepInfo {
                agent: Arc::clone(&self.0),
                warp,
                hist,
                health: Some(agent.update_health()),
            },
        )
    }
}

pub struct WarpAgent(Agent, WarpInfo);

impl WarpAgent {
    fn agent_mut(&self) -> &mut AgentParam {
        self.0.lock().unwrap().deref_mut()
    }

    fn step(&mut self, prms: &ParamsForStep) -> bool {
        let agent = self.agent_mut();
        agent.step_warp(&self.1.goal, prms)
    }

    fn get_area(self) -> Area {
        self.1.area
    }

    fn transfer(self, field: &mut Field, hospital: &mut Hospital, cemetery: &mut Cemetery) {
        match self.get_area() {
            Area::Field(idx) => {
                field.add(self.0, idx);
            }
            Area::Hospital => hospital.add(self.0),
            Area::Cemetery => cemetery.add(self.0),
            Area::Warp(_) => unreachable!("WarpAgent does not have Warp inside."),
        }
    }
}

pub fn wall(d: f64) -> f64 {
    let d = if d < 0.02 { 0.02 } else { d };
    AVOIDANCE * 20. / d / d
}

pub fn was_hit(days_per_step: f64, prob: f64) -> bool {
    let rng = &mut rand::thread_rng();
    rng.gen::<f64>() > (1.0 - prob).powf(days_per_step)
}

pub fn cummulate_histgrm(h: &mut Vec<MyCounter>, d: f64) {
    let ds = d.floor() as usize;
    if h.len() <= ds {
        let n = ds - h.len();
        for _ in 0..=n {
            h.push(MyCounter::new());
        }
    }
    h[ds].inc();
}

pub mod cont {
    use std::sync::Arc;

    use rayon::iter::ParallelIterator;

    use super::{Agent, FieldAgent, HospitalAgent, ParamsForStep, WarpAgent, WarpInfo};
    use crate::{
        commons::{
            container::table::{Table, TableIndex},
            DrainLike, DrainMap, Either,
        },
        log::StepLog,
        testing::{TestQueue, Testee},
    };

    pub struct Field(Table<Vec<FieldAgent>>);

    impl Field {
        pub fn steps(
            &mut self,
            warps: &mut Warps,
            test_queue: &mut TestQueue,
            step_log: &mut StepLog,
            prms: &ParamsForStep,
        ) {
            // step
            let tmp = self
                .0
                .par_h_iter_mut()
                .map(|(idx, ags)| ags.drain_map_mut(|fa| fa.step(idx, &prms)))
                .collect::<Vec<_>>();

            for sis in tmp.into_iter() {
                for mut fsi in sis {
                    if let Some(h) = fsi.hist {
                        step_log.hists.push(h);
                    }
                    if let Some(h) = fsi.health {
                        step_log.healths.push(h);
                    }
                    if let Some(i) = fsi.infct {
                        step_log.infcts.push(i);
                    }

                    if let Some(reason) = fsi.test {
                        let t = Testee::new(Arc::clone(&fsi.agent), reason, prms);
                        if let Some(mut testees) = fsi.contact_testees {
                            testees.push(t);
                            test_queue.add(testees, prms.rp.step);
                        } else {
                            test_queue.add(vec![t], prms.rp.step);
                        }
                    }

                    for t in fsi.transfer {
                        match t {
                            Either::Left(w) => warps.add(fsi.agent, w),
                            Either::Right(idx) => self.add(fsi.agent, idx),
                        }
                    }
                }
            }
        }

        pub fn add(&mut self, a: Agent, idx: TableIndex) {
            self.0[idx].push(FieldAgent::new(a, idx));
        }

        pub fn intersect(&mut self, prms: &ParamsForStep) {
            // let mesh = self.world_params.mesh as usize;
            // |x|a|b|a|b|..
            self.0.par_east_pair().for_each(|((_, a_ags), (_, b_ags))| {
                // let dsc = self.dsc.lock().unwrap();
                Self::interact_intercells(&a_ags, &b_ags, prms);
            });
            // |a|b|a|b|..
            self.0
                .par_west_pair_mut()
                .for_each(|((_, a_ags), (_, b_ags))| {
                    // let dsc = self.dsc.lock().unwrap();
                    Self::interact_intercells(&a_ags, &b_ags, prms);
                });

            // |b|
            // |a|

            // |b| |
            // | |a|

            // | |b|
            // |a| |
        }

        fn interact_intercells(a_ags: &[FieldAgent], b_ags: &[FieldAgent], prms: &ParamsForStep) {
            let rng = &mut rand::thread_rng();
            for fa in a_ags {
                for fb in b_ags {
                    fa.interacts(fb, prms);
                }
            }
        }
    }

    pub struct Hospital(Vec<HospitalAgent>);

    impl Hospital {
        pub fn add(&mut self, a: Agent) {
            self.0.push(HospitalAgent(a));
        }

        pub fn steps(&mut self, warps: &Warps, step_log: &mut StepLog, prms: &ParamsForStep) {
            let tmp = self.0.drain_map_mut(|ha| ha.step(prms));

            for hsi in tmp.into_iter() {
                for h in hsi.hist {
                    step_log.hists.push(h);
                }

                for h in hsi.health {
                    step_log.healths.push(h);
                }

                for w in hsi.warp {
                    warps.add(hsi.agent, w);
                }
            }
        }
    }

    pub struct Warps(Vec<WarpAgent>);

    impl Warps {
        pub fn add(&mut self, a: Agent, w: WarpInfo) {
            self.0.push(WarpAgent(a, w));
        }

        pub fn steps(
            &mut self,
            field: &mut Field,
            hospital: &mut Hospital,
            cemetery: &mut Cemetery,
            prms: &ParamsForStep,
        ) {
            let tmp = self.0.drain_mut(|a| a.step(prms));
            for wa in tmp.into_iter() {
                wa.transfer(field, hospital, cemetery)
            }
        }
    }

    pub struct Cemetery(Vec<Agent>);

    impl Cemetery {
        pub fn add(&mut self, a: Agent) {
            self.0.push(a);
        }
    }
}
