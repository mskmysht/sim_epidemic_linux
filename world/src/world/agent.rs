pub(super) mod cemetery;
pub(super) mod field;
pub(super) mod gathering;
pub(super) mod hospital;
pub(super) mod param;
pub(super) mod warp;

use self::{gathering::Gathering, param::*};
use super::{
    commons::{HealthType, ParamsForStep, RuntimeParams, WorldParams, WrkPlcMode},
    contact::Contacts,
    testing::{TestReason, TestResult, Testee},
};
use crate::{
    log::HealthDiff,
    stat::{HistInfo, InfectionCntInfo},
    util::{
        math::{Percentage, Point},
        random::{self, modified_prob, DistInfo},
        table::TableIndex,
    },
};

use std::{
    f64,
    ops::Deref,
    sync::{Arc, Weak},
};

use parking_lot::RwLock;
use rand::{self, Rng};

const AGENT_RADIUS: f64 = 0.75;
//[todo] static AGENT_SIZE: f64 = 0.665;
const AVOIDANCE: f64 = 0.2;

const BACK_HOME_RATE: bool = true;

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

#[derive(Default)]
enum HealthState {
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
struct VaccineState {
    pub param: Option<VaccinationParam>,
    pub vaccine_ticket: Option<usize>,
}

impl VaccineState {
    fn vaccinate(
        &mut self,
        immunity: f64,
        days_to: &mut DaysTo,
        pfs: &ParamsForStep,
    ) -> Option<HealthState> {
        let vaccine_type = self.vaccine_ticket.take()?;
        let vp = {
            let today = pfs.rp.step as f64 * pfs.wp.days_per_step();
            if let Some(mut vp) = self.param.take() {
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
        };
        Some(HealthState::Vaccinated(vp))
    }

    fn insert_param(&mut self, param: VaccinationParam) {
        self.param = Some(param);
    }
}

#[derive(Default)]
struct AgentHealth {
    state: HealthState,
    new_state: Option<HealthState>,
}

impl AgentHealth {
    pub fn reset(&mut self, state: HealthState) {
        self.state = state;
        self.new_state = None;
    }

    fn get_state(&self) -> &HealthState {
        &self.state
    }

    fn get_state_mut(&mut self) -> &mut HealthState {
        &mut self.state
    }

    fn has_new_state(&self) -> bool {
        self.new_state.is_some()
    }

    fn insert_new_state(&mut self, new_state: HealthState) -> &HealthState {
        self.new_state.insert(new_state)
    }

    fn get_immune_factor(&self, bip: &InfectionParam, pfs: &ParamsForStep) -> Option<f64> {
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

    fn get_immunity(&self) -> Option<f64> {
        match &self.state {
            HealthState::Susceptible => Some(0.0),
            HealthState::Infected(ip, InfMode::Asym) => Some(ip.immunity),
            HealthState::Vaccinated(vp) => Some(vp.immunity),
            _ => None,
        }
    }

    fn get_infected(&self) -> Option<&InfectionParam> {
        match &self.state {
            HealthState::Infected(ip, _) => Some(ip),
            _ => None,
        }
    }

    fn is_symptomatic(&self) -> bool {
        matches!(&self.state, HealthState::Infected(_, InfMode::Sym))
    }

    fn get_symptomatic(&self) -> Option<&InfectionParam> {
        match &self.state {
            HealthState::Infected(ip, InfMode::Sym) => Some(ip),
            _ => None,
        }
    }

    fn update(&mut self) -> Option<(HealthDiff, HealthState)> {
        if let Some(new_health) = self.new_state.take() {
            let from = (&self.state).into();
            let to = (&new_health).into();

            let old = std::mem::replace(&mut self.state, new_health);
            Some((HealthDiff::new(from, to), old))
        } else {
            None
        }
    }
}

#[derive(Default)]
struct DaysTo {
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

    fn calc_force_delta(&self, b: &Self, pfs: &ParamsForStep) -> Option<(Point, f64)> {
        let delta = b.pt - self.pt;
        let d2 = (delta.x * delta.x + delta.y * delta.y).max(1e-4);
        let d = d2.sqrt();
        let view_range = pfs.wp.view_range();
        if d >= view_range {
            return None;
        }

        let mut dd = if d < view_range * 0.8 {
            1.0
        } else {
            (1.0 - d / view_range) / 0.2
        };
        dd = dd / d / d2 * AVOIDANCE * pfs.rp.avoidance / 50.0;
        let df = delta * dd;

        Some((df, d))
    }

    fn get_new_pt(&self, world_size: f64, mob_dist: &DistInfo<Percentage>) -> Point {
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
    gathering: Weak<RwLock<Gathering>>,
    n_infects: u64,

    activeness: f64,
    age: f64,
    days_to: DaysTo,

    mob_freq: f64,
    gat_freq: f64,

    health: AgentHealth,
    vaccine_state: VaccineState,
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
    pub fn get_pt(&self) -> &Point {
        &self.body.pt
    }

    fn force_susceptible(&mut self) {
        self.health.reset(HealthState::Susceptible);
    }

    fn force_infected(&mut self) {
        let mut ip = InfectionParam::new(0.0, 0);
        ip.days_infected =
            rand::thread_rng().gen::<f64>() * self.days_to.recover.min(self.days_to.die);
        let d = ip.days_infected - self.days_to.onset;
        let inf_mode = if d >= 0.0 {
            ip.days_diseased = d;
            InfMode::Sym
        } else {
            InfMode::Asym
        };
        self.health.reset(HealthState::Infected(ip, inf_mode));
    }

    fn force_recovered(&mut self, rp: &RuntimeParams) {
        let rng = &mut rand::thread_rng();
        self.days_to.expire_immunity = rng.gen::<f64>() * rp.imn_max_dur;
        let days_recovered = rng.gen::<f64>() * self.days_to.expire_immunity;
        let mut rcp = RecoverParam::new(0.0, 0);
        rcp.days_recovered = days_recovered;
        self.health.reset(HealthState::Recovered(rcp));
    }

    pub fn reserve_test(
        &mut self,
        a: Agent,
        reason: TestReason,
        pfs: &ParamsForStep,
    ) -> Option<Testee> {
        if !self.is_testable(pfs) {
            return None;
        }
        self.test_reserved = true;
        let rng = &mut rand::thread_rng();
        let result = {
            let b = if let Some(ip) = self.health.get_infected() {
                // P(U < 1 - (1-p)^x) = 1 - (1-p)^x = P(U > (1-p)^x)
                random::at_least_once_hit_in(
                    pfs.vr_info[ip.virus_variant].reproductivity,
                    pfs.rp.tst_sens.r(),
                )
            } else {
                rng.gen::<f64>() > pfs.rp.tst_spec.r()
            };
            b.into()
        };
        Some(Testee::new(a, reason, result, pfs.rp.step))
    }

    pub fn deliver_test_result(&mut self, time_stamp: u64, result: TestResult) {
        self.test_reserved = false;
        self.last_tested = Some(time_stamp);
        if let TestResult::Positive = result {
            self.quarantine_reserved = true;
        }
    }

    fn is_testable(&self, pfs: &ParamsForStep) -> bool {
        if !self.is_in_field() || self.test_reserved
        /*|| todo!("self.for_vcn == VcnNoTest") */
        {
            return false;
        }

        if let Some(d) = self.last_tested {
            let ds = (pfs.rp.step - d) as f64;
            ds >= pfs.rp.tst_interval * pfs.wp.steps_per_day()
        } else {
            true
        }
    }

    #[inline]
    fn is_in_field(&self) -> bool {
        matches!(self.location, Location::Field)
    }

    /// `self` tries to infect `b` with the virus data of [InfectionParam] in `self`
    /// if `b.health.new_state` is `None` (`b` does not have a new health state)
    /// and `self.health` is [HealthState::Infected] (`self` is infected).
    fn infect(&self, b: &mut Self, d: f64, pfs: &ParamsForStep) -> bool {
        if b.health.has_new_state() {
            return false;
        }
        let HealthState::Infected(ip, _) = self.health.get_state() else {
            return false;
        };
        let Some(immunity) = b.health.get_immune_factor(ip, pfs) else {
            return false;
        };
        if !ip.check_infection(immunity, d, self.days_to.onset, pfs) {
            return false;
        }
        b.health.insert_new_state(HealthState::Infected(
            InfectionParam::new(immunity, ip.virus_variant),
            InfMode::Asym,
        ));
        true
    }

    fn calc_gathering_effect(&self) -> (Option<Point>, Option<f64>) {
        match self.gathering.upgrade() {
            None => (None, None),
            Some(gat) => gat.read().get_effect(&self.body.pt),
        }
    }

    fn check_test(&self, pfs: &ParamsForStep) -> Option<TestReason> {
        if !self.is_testable(pfs) {
            return None;
        }
        if let Some(ip) = self.health.get_symptomatic() {
            if ip.days_diseased >= pfs.rp.tst_delay
                && random::at_least_once_hit_in(pfs.wp.days_per_step(), pfs.rp.tst_sbj_sym.r())
            {
                return Some(TestReason::AsSymptom);
            }
        }
        if random::at_least_once_hit_in(pfs.wp.days_per_step(), pfs.rp.tst_sbj_asy.r()) {
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
                && random::at_least_once_hit_in(pfs.wp.days_per_step() * 3.0, pfs.rp.back_hm_rt.r())
            {
                return Some(self.origin);
            }
            if random::at_least_once_hit_in(
                pfs.wp.days_per_step(),
                modified_prob(self.mob_freq, &pfs.rp.mob_freq).r(),
            ) {
                return Some(self.body.get_new_pt(pfs.wp.field_size(), &pfs.rp.mob_dist));
            }
        } else {
            if random::at_least_once_hit_in(
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

    fn warp_inside(&self, pfs: &ParamsForStep) -> Option<WarpParam> {
        if self.health.is_symptomatic() {
            return None;
        }
        if let Some(goal) = self.get_warp_inside_goal(pfs) {
            return Some(WarpParam::inside(goal));
        }
        None
    }

    fn calc_force(
        &self,
        force: Point,
        best: &Option<(Point, f64)>,
        pfs: &ParamsForStep,
    ) -> (Point, Option<f64>) {
        let mut gat_dist = None;
        let mut f = force;
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
        f += self.best_point_force(&best.map(|(p, _)| p), pfs.wp.field_size());
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
        force: Point,
        best: &Option<(Point, f64)>,
        idx: &TableIndex,
        pfs: &ParamsForStep,
    ) -> Option<TableIndex> {
        let (f, gat_dist) = self.calc_force(force, best, pfs);
        self.body
            .field_update(self.health.is_symptomatic(), f, &gat_dist, pfs);
        let new_idx = pfs.wp.into_grid_index(&self.body.pt);
        if *idx != new_idx {
            Some(new_idx)
        } else {
            None
        }
    }

    fn quarantine(
        &mut self,
        contact_testees: &mut Option<Vec<Testee>>,
        pfs: &ParamsForStep,
    ) -> Option<WarpParam> {
        if !self.quarantine_reserved {
            return None;
        }
        self.quarantine_reserved = false;
        //[todo] prms.rp.trc_ope != TrcTst
        *contact_testees = Some(self.contacts.get_testees(pfs));
        if let WrkPlcMode::WrkPlcNone = pfs.wp.wrk_plc_mode {
            self.origin = self.body.pt;
        }
        Some(WarpParam::hospital(pfs.wp))
    }

    #[inline]
    fn update_health(&mut self) -> Option<HealthDiff> {
        let (hd, old_h) = self.health.update()?;
        if let HealthState::Vaccinated(vp) = old_h {
            self.vaccine_state.insert_param(vp);
        }
        Some(hd)
    }

    fn field_step(
        &mut self,
        hist: &mut Option<HistInfo>,
        pfs: &ParamsForStep,
    ) -> Option<WarpParam> {
        let new_health = 'block: {
            if let Some(immunity) = self.health.get_immunity() {
                if let Some(new_health) =
                    self.vaccine_state
                        .vaccinate(immunity, &mut self.days_to, pfs)
                {
                    break 'block new_health;
                }
            }
            match &mut self.health.get_state_mut() {
                HealthState::Infected(ip, inf_mode) => ip.step::<false>(
                    &mut self.days_to,
                    &self.vaccine_state.param,
                    pfs,
                    hist,
                    inf_mode,
                )?,
                HealthState::Recovered(rp) => {
                    rp.step(&mut self.days_to, self.activeness, self.age, pfs)?
                }
                HealthState::Vaccinated(vp) => {
                    vp.step(&mut self.days_to, self.activeness, self.age, pfs)?
                }
                _ => return None,
            }
        };
        if matches!(self.health.insert_new_state(new_health), HealthState::Died) {
            return Some(WarpParam::cemetery(pfs.wp));
        }
        None
    }

    fn hospital_step(
        &mut self,
        hist: &mut Option<HistInfo>,
        pfs: &ParamsForStep,
    ) -> Option<WarpParam> {
        let HealthState::Infected(ip, inf_mode) = &mut self.health.get_state_mut() else {
                return None;
            };
        let new_health = ip.step::<true>(
            &mut self.days_to,
            &self.vaccine_state.param,
            pfs,
            hist,
            inf_mode,
        )?;

        match self.health.insert_new_state(new_health) {
            HealthState::Died => Some(WarpParam::cemetery(pfs.wp)),
            HealthState::Recovered(..) => Some(WarpParam::back(self.origin)),
            _ => Some(WarpParam::back(self.origin)),
        }
    }

    fn replace_gathering(
        &mut self,
        gat_freq: &DistInfo<Percentage>,
        gathering: Weak<RwLock<Gathering>>,
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
pub struct Agent(Arc<RwLock<AgentCore>>);

impl Agent {
    pub fn new() -> Self {
        Agent(Arc::new(RwLock::new(AgentCore::default())))
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
        let mut cats = 'block: {
            use crate::util::math;
            let r = n_pop - n_infected;
            if r == 0 {
                break 'block vec![HealthType::Asymptomatic; n_pop];
            }
            let mut cats = if r == n_recovered {
                vec![HealthType::Recovered; n_pop]
            } else {
                vec![HealthType::Susceptible; n_pop]
            };
            let m = {
                let idxs_inf = math::reservoir_sampling(n_pop, n_infected);
                let mut m = usize::MAX;
                for idx in idxs_inf {
                    cats[idx] = HealthType::Asymptomatic;
                    if m > idx {
                        m = idx;
                    }
                }
                m
            };
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
        };

        let mut n_symptomatic = 0;
        for (i, h) in cats.iter_mut().enumerate() {
            let mut a = agents[i].0.write();
            a.reset(wp, rp, i, i < n_dist);
            match h {
                HealthType::Susceptible => a.force_susceptible(),
                HealthType::Asymptomatic => {
                    a.force_infected();
                    if a.health.is_symptomatic() {
                        n_symptomatic += 1;
                        *h = HealthType::Symptomatic;
                    }
                }
                HealthType::Recovered => a.force_recovered(rp),
                _ => {}
            }
        }
        (cats, n_symptomatic)
    }

    // pub fn reserve_test(&self, pfs: &ParamsForStep) -> Option<Testee> {
    //     let mut a = self.0.write();
    //     if a.is_testable(pfs.wp, pfs.rp) {
    //         Some(a.reserve_test(self.clone(), TestReason::AsContact, pfs))
    //     } else {
    //         None
    //     }
    // }
}

impl Deref for Agent {
    type Target = Arc<RwLock<AgentCore>>;

    fn deref(&self) -> &Self::Target {
        &self.0
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
        agent.write().location = Self::LABEL;
        agent
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
