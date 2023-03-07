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
    stat::LocalStepLog,
    util::{
        math::{Percentage, Point},
        random::{self, modified_prob, DistInfo},
    },
};

use std::{
    f64,
    ops::Deref,
    sync::{Arc, Weak},
};

use table::TableIndex;

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
struct AgentHealth {
    days_to: DaysTo,
    vaccine_state: VaccineState,
    state: HealthState,
}

impl AgentHealth {
    pub fn reset(&mut self, activeness: f64, age: f64, wp: &WorldParams, rp: &RuntimeParams) {
        self.days_to.reset(activeness, age, wp, rp);
        self.vaccine_state = VaccineState::default();
        self.state = HealthState::default();
    }

    pub fn force_susceptible(&mut self) {
        self.state = HealthState::Susceptible;
    }

    pub fn force_infected(&mut self) {
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
        self.state = HealthState::Infected(ip, inf_mode);
    }

    pub fn force_recovered(&mut self, rp: &RuntimeParams) {
        let rng = &mut rand::thread_rng();
        self.days_to.expire_immunity = rng.gen::<f64>() * rp.imn_max_dur;
        let days_recovered = rng.gen::<f64>() * self.days_to.expire_immunity;
        let mut rcp = RecoverParam::new(0.0, 0);
        rcp.days_recovered = days_recovered;
        self.state = HealthState::Recovered(rcp);
    }

    fn infected_by(&self, b: &Self, d: f64, pfs: &ParamsForStep) -> Option<(f64, usize)> {
        let ip = b.get_infected()?;
        let immunity = self.get_immune_factor(ip.virus_variant, pfs)?;
        if ip.check_infection(immunity, d, b.days_to.onset, pfs) {
            Some((immunity, ip.virus_variant))
        } else {
            None
        }
    }

    fn get_immune_factor(&self, virus_variant: usize, pfs: &ParamsForStep) -> Option<f64> {
        let immune_factor = match &self.state {
            HealthState::Susceptible => 0.0,
            HealthState::Recovered(rp) => {
                rp.immunity * pfs.vr_info[rp.virus_variant].efficacy[virus_variant]
            }
            HealthState::Vaccinated(vp) => {
                vp.immunity * pfs.vx_info[vp.vaccine_type].efficacy[virus_variant]
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

    fn field_step(
        &mut self,
        infected: Option<(f64, usize)>,
        activeness: f64,
        age: f64,
        log: &mut LocalStepLog,
        pfs: &ParamsForStep,
    ) -> Option<WarpParam> {
        let from_hd = (&self.state).into();

        let new_state = 'block: {
            if let Some(immunity) = self.get_immunity() {
                if let Some(new_state) =
                    self.vaccine_state
                        .vaccinate(immunity, &mut self.days_to, pfs)
                {
                    break 'block Some(new_state);
                }
            }
            match &mut self.state {
                HealthState::Infected(ip, inf_mode) => ip.step::<false>(
                    &mut self.days_to,
                    inf_mode,
                    &self.vaccine_state.param,
                    log,
                    pfs,
                ),
                HealthState::Recovered(rp) => rp.step(&mut self.days_to, activeness, age, pfs),
                HealthState::Vaccinated(vp) => vp.step(&mut self.days_to, activeness, age, pfs),
                _ => infected.map(|(immunity, virus_variant)| {
                    HealthState::Infected(
                        InfectionParam::new(immunity, virus_variant),
                        InfMode::Asym,
                    )
                }),
            }
        };

        let mut warp = None;
        if let Some(new_state) = new_state {
            match std::mem::replace(&mut self.state, new_state) {
                HealthState::Vaccinated(vp) => {
                    self.vaccine_state.insert_param(vp);
                }
                HealthState::Died => warp = Some(WarpParam::cemetery(pfs.wp)),
                _ => {}
            }
        };
        log.set_health(from_hd, (&self.state).into());
        warp
    }

    fn hospital_step(
        &mut self,
        back_to: Point,
        log: &mut LocalStepLog,
        pfs: &ParamsForStep,
    ) -> Option<WarpParam> {
        let from_hd = (&self.state).into();

        let HealthState::Infected(ip, inf_mode) = &mut self.state else {
            return None;
        };
        let mut warp = None;
        if let Some(new_state) = ip.step::<true>(
            &mut self.days_to,
            inf_mode,
            &self.vaccine_state.param,
            log,
            pfs,
        ) {
            warp = match std::mem::replace(&mut self.state, new_state) {
                HealthState::Died => Some(WarpParam::cemetery(pfs.wp)),
                _ => Some(WarpParam::back(back_to)),
            };
        };

        log.set_health(from_hd, (&self.state).into());
        warp
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
    fn reset(&mut self, wp: &WorldParams) {
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

    fn get_new_pt(&self, pfs: &ParamsForStep) -> Point {
        let field_size = pfs.wp.field_size();
        let rng = &mut rand::thread_rng();
        let dst = random::my_random(rng, &pfs.rp.mob_dist).r() * field_size;
        let th = rng.gen::<f64>() * f64::consts::PI * 2.;
        let mut new_pt = Point {
            x: self.pt.x + th.cos() * dst,
            y: self.pt.y + th.sin() * dst,
        };
        if new_pt.x < 3. {
            new_pt.x = 3. - new_pt.x;
        } else if new_pt.x > field_size - 3. {
            new_pt.x = (field_size - 3.) * 2. - new_pt.x;
        }
        if new_pt.y < 3. {
            new_pt.y = 3. - new_pt.y;
        } else if new_pt.y > field_size - 3. {
            new_pt.y = (field_size - 3.) * 2. - new_pt.y;
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
struct AgentLog {
    n_infects: u32,
}

impl AgentLog {
    fn reset(&mut self) {
        *self = AgentLog::default();
    }

    fn update_n_infects(&mut self, new_n_infects: u32, log: &mut LocalStepLog) {
        if new_n_infects > 0 {
            let prev_n_infects = self.n_infects;
            self.n_infects += new_n_infects;
            log.set_infect(prev_n_infects, self.n_infects);
        }
    }
}

#[derive(Default)]
pub struct InnerAgent {
    pub id: usize,
    body: Body,
    /// [`None`] means it has no home. (e.g. [`wrk_plc_mode`](WorldParams::wrk_plc_mode) equals [`WrkPlcMode::WrkPlcNone`].)
    pub origin: Option<Point>,

    distancing: bool,
    activeness: f64,
    age: f64,
    mob_freq: f64,
    gat_freq: f64,

    gathering: Weak<RwLock<Gathering>>,
    location: Location,
    health: AgentHealth,
    testing: TestState,
    contacts: Contacts,

    log: AgentLog,
}

impl InnerAgent {
    fn reset(&mut self, wp: &WorldParams, rp: &RuntimeParams, id: usize, distancing: bool) {
        self.testing.reset();
        self.log.reset();

        let rng = &mut rand::thread_rng();
        self.health.reset(self.activeness, self.age, wp, rp);

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
        self.body.reset(wp);

        self.origin = match wp.wrk_plc_mode {
            WrkPlcMode::WrkPlcNone => None,
            _ => Some(self.body.pt),
        };
    }

    #[inline]
    pub fn get_pt(&self) -> &Point {
        &self.body.pt
    }

    fn check_test_in_field(&self, pfs: &ParamsForStep) -> Option<TestReason> {
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

    pub fn get_test(&mut self, time_stamp: u32, pfs: &ParamsForStep) -> TestResult {
        let rng = &mut rand::thread_rng();
        let b = if let Some(ip) = self.health.get_infected() {
            // P(U < 1 - (1-p)^x) = 1 - (1-p)^x = P(U > (1-p)^x)
            random::at_least_once_hit_in(
                pfs.vr_info[ip.virus_variant].reproductivity,
                pfs.rp.tst_sens.r(),
            )
        } else {
            rng.gen::<f64>() > pfs.rp.tst_spec.r()
        };
        let result = TestResult::from(b);
        self.testing.notify_result(time_stamp, result.clone());
        result
    }

    pub fn cancel_test(&mut self) {
        self.testing.cancel();
    }

    #[inline]
    pub fn is_in_field(&self) -> bool {
        matches!(self.location, Location::Field)
    }

    pub fn get_back_to(&self) -> Point {
        self.origin.unwrap_or(self.body.pt)
    }

    fn calc_gathering_effect(&self) -> (Option<Point>, Option<f64>) {
        match self.gathering.upgrade() {
            None => (None, None),
            Some(gat) => gat.read().get_effect(&self.body.pt),
        }
    }

    fn moves_inside(&self, pfs: &ParamsForStep) -> bool {
        random::at_least_once_hit_in(
            pfs.wp.days_per_step(),
            modified_prob(self.mob_freq, &pfs.rp.mob_freq).r(),
        )
    }

    fn is_away_from_home(dp: &Point, pfs: &ParamsForStep) -> bool {
        dp.x.hypot(dp.y) > pfs.rp.mob_dist.min.max(&MIN_AWAY_TO_HOME).r() * pfs.wp.field_size()
    }

    fn get_warp_inside_goal(&self, pfs: &ParamsForStep) -> Option<Point> {
        let Some(origin) = self.origin else {
            if self.moves_inside(pfs) {
                return Some(self.body.get_new_pt(pfs));
            }
            return None;
        };

        let dp = self.body.pt - origin;
        if BACK_HOME_RATE {
            if pfs.go_home_back()
                && Self::is_away_from_home(&dp, pfs)
                && random::at_least_once_hit_in(pfs.wp.days_per_step() * 3.0, pfs.rp.back_hm_rt.r())
            {
                return Some(origin);
            }
            if self.moves_inside(pfs) {
                return Some(self.body.get_new_pt(pfs));
            }
            return None;
        } else {
            if self.moves_inside(pfs) {
                if pfs.go_home_back() && Self::is_away_from_home(&dp, pfs) {
                    return Some(origin);
                }
                return Some(self.body.get_new_pt(pfs));
            }
            return None;
        }
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
        best: Option<(Point, f64)>,
        pfs: &ParamsForStep,
    ) -> (Point, Option<f64>) {
        let mut gat_dist = None;
        let mut f = force;
        match self.origin {
            Some(origin) if pfs.go_home_back() => {
                if let Some(df) = back_home_force(&self.body.pt, &origin) {
                    f += df;
                }
            }
            _ => {
                let (df, dist) = self.calc_gathering_effect();
                if let Some(df) = df {
                    f += df;
                }
                gat_dist = dist;
            }
        }

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
        best: Option<(Point, f64)>,
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

    pub fn reserve_test_in_field(&mut self, agent: Agent, pfs: &ParamsForStep) -> Option<Testee> {
        let reason = {
            if !self.testing.is_reservable(pfs) {
                return None;
            }
            self.check_test_in_field(pfs)?
        };
        self.testing.reserve();
        Some(Testee::new(agent, reason, pfs.rp.step))
    }

    pub fn reserve_test_with<F: Fn(&Self) -> Option<TestReason>>(
        &mut self,
        agent: Agent,
        pfs: &ParamsForStep,
        f: F,
    ) -> Option<Testee> {
        let reason = {
            if !self.testing.is_reservable(pfs) {
                return None;
            }
            f(&self)?
        };
        self.testing.reserve();
        Some(Testee::new(agent, reason, pfs.rp.step))
    }

    fn check_quarantine(
        &mut self,
        contacted_testees: &mut Option<Vec<Testee>>,
        pfs: &ParamsForStep,
    ) -> Option<WarpParam> {
        //[todo] prms.rp.trc_ope != TrcTst
        if matches!(self.testing.read_result(), Some(TestResult::Positive)) {
            *contacted_testees = Some(self.contacts.drain_testees(pfs));
            Some(WarpParam::hospital(self.get_back_to(), pfs.wp))
        } else {
            None
        }
    }

    fn field_step(
        &mut self,
        infected: Option<(f64, usize)>,
        agent: Agent,
        testee: &mut Option<Testee>,
        log: &mut LocalStepLog,
        pfs: &ParamsForStep,
    ) -> Option<WarpParam> {
        *testee = self.reserve_test_in_field(agent, pfs);
        self.health
            .field_step(infected, self.activeness, self.age, log, pfs)
    }

    fn hospital_step(
        &mut self,
        back_to: Point,
        log: &mut LocalStepLog,
        pfs: &ParamsForStep,
    ) -> Option<WarpParam> {
        self.health.hospital_step(back_to, log, pfs)
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
}

#[derive(Clone)]
pub struct Agent(Arc<RwLock<InnerAgent>>);

impl Agent {
    pub fn new() -> Self {
        Agent(Arc::new(RwLock::new(InnerAgent::default())))
    }

    pub fn reset_all(
        agents: &[Self],
        n_pop: usize,
        n_infected: usize,
        n_recovered: usize,
        n_dist: usize,
        wp: &WorldParams,
        rp: &RuntimeParams,
    ) -> (Vec<HealthType>, u32) {
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
                HealthType::Susceptible => a.health.force_susceptible(),
                HealthType::Asymptomatic => {
                    a.health.force_infected();
                    if a.health.is_symptomatic() {
                        n_symptomatic += 1;
                        *h = HealthType::Symptomatic;
                    }
                }
                HealthType::Recovered => a.health.force_recovered(rp),
                _ => {}
            }
        }
        (cats, n_symptomatic)
    }
}

#[derive(Default)]
struct TestState {
    reserved: bool,
    last_tested: Option<u32>,
    unread_result: Option<TestResult>,
}

impl TestState {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn is_reservable(&self, pfs: &ParamsForStep) -> bool {
        /*|| todo!("self.for_vcn == VcnNoTest") */
        if self.reserved {
            return false;
        }

        let Some(d) = self.last_tested else {
            return true;
        };
        let ds = (pfs.rp.step - d) as f64;
        ds >= pfs.rp.tst_interval * pfs.wp.steps_per_day()
    }

    fn reserve(&mut self) {
        self.reserved = true;
    }

    fn notify_result(&mut self, time_stamp: u32, result: TestResult) {
        self.reserved = false;
        self.last_tested = Some(time_stamp);
        self.unread_result = Some(result);
    }

    fn cancel(&mut self) {
        self.reserved = false;
    }

    fn read_result(&mut self) -> Option<TestResult> {
        self.unread_result.take()
    }
}

impl Deref for Agent {
    type Target = Arc<RwLock<InnerAgent>>;

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
    Hospital(Point),
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

    pub fn hospital(back_to: Point, wp: &WorldParams) -> Self {
        let rng = &mut rand::thread_rng();
        let goal = Point::new(
            (rng.gen::<f64>() * 0.248 + 1.001) * wp.field_size(),
            (rng.gen::<f64>() * 0.458 + 0.501) * wp.field_size(),
        );
        Self::new(WarpMode::Hospital(back_to), goal)
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
