use crate::{
    commons::*,
    world::{WarpInfo, World},
};
use crate::{contact::*, dyn_struct::DynStruct};

use std::{collections::VecDeque, sync::Arc};

use rand::{self, prelude::ThreadRng, Rng};
use std::f64;

static AGENT_RADIUS: f64 = 0.75;
// static AGENT_SIZE: f64 = 0.665;

static AVOIDANCE: f64 = 0.2;

#[derive(Default, Debug)]
pub struct Agent {
    pub id: i32,
    app: f64,
    prf: f64,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub fx: f64,
    pub fy: f64,
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
    pub is_warping: bool,
    pub got_at_hospital: bool,
    pub in_test_queue: bool,
    pub last_tested: i32,
    pub best: Option<MRef<Agent>>,
    best_dist: f64,
    pub contact_info_list: VecDeque<MRef<ContactInfo>>,
}

impl Agent {
    pub fn reset(&mut self, world_size: f64, rp: &RuntimeParams) {
        let mut rng = rand::thread_rng();
        self.app = rng.gen();
        self.prf = rng.gen();
        self.x = rng.gen::<f64>() * (world_size - 6.0) + 3.0;
        self.y = rng.gen::<f64>() * (world_size - 6.0) + 3.0;
        let th: f64 = rng.gen::<f64>() * f64::consts::PI * 2.0;
        self.vx = th.cos();
        self.vy = th.sin();
        self.health = HealthType::Susceptible;
        self.n_infects = -1;
        self.is_out_of_field = true;
        self.last_tested = -999999;
        self.reset_days(rp)
    }

    pub fn reset_days(&mut self, rp: &RuntimeParams) {
        let mut gauss = Gaussian::new();
        self.days_to_recover = gauss.my_random(&rp.recov);
        self.days_to_onset = gauss.my_random(&rp.incub);
        self.days_to_die = gauss.my_random(&rp.fatal) + self.days_to_onset;
        self.im_expr = gauss.my_random(&rp.immun);
    }

    pub fn index_in_pop(&self, wp: &WorldParams) -> i32 {
        let ix = get_index(self.x, wp);
        let iy = get_index(self.x, wp);
        iy * wp.mesh + ix
    }

    pub fn reset_for_step(&mut self) {
        self.fx = 0.;
        self.fy = 0.;
        self.best = None;
        self.best_dist = f64::MAX; // BIG_NUM;
        self.new_health = self.health;
    }

    pub fn attracted(
        ar: MRef<Agent>,
        br: MRef<Agent>,
        wp: &WorldParams,
        rp: &RuntimeParams,
        d: f64,
        dsc: &mut DynStruct<ContactInfo>,
    ) {
        let spd = wp.steps_per_day as f64;
        let x = {
            let a = ar.lock().unwrap();
            let b = br.lock().unwrap();
            let x = (b.app - a.prf).abs();
            (if x < 0.5 { x } else { 1.0 - x }) * 2.0
        };
        {
            let a = &mut ar.lock().unwrap();
            if a.best_dist > x {
                a.best_dist = x;
                a.best = Some(br.clone());
            }
        }
        // check contact and infection
        if d < rp.infec_dst {
            if was_hit(spd, rp.cntct_trc / 100.) {
                add_new_cinfo(dsc, ar.clone(), br.clone(), rp.step);
            }
            let a = &mut ar.lock().unwrap();
            let b = &mut br.lock().unwrap();
            if a.health == HealthType::Susceptible
                && b.is_infected()
                && b.days_infected > rp.contag_delay
            {
                let time_factor = (1.0 as f64).min(
                    (b.days_infected - rp.contag_delay)
                        / (b.days_infected - rp.contag_peak.min(b.days_to_onset)),
                );
                let distance_factor = ((rp.infec_dst - d) / 2.).powf(2.).min(1.);
                if was_hit(spd, rp.infec / 100. * time_factor * distance_factor) {
                    a.new_health = HealthType::Asymptomatic;
                    if a.n_infects < 0 {
                        a.new_n_infects = 1;
                    }
                    b.new_n_infects += 1;
                }
            }
        }
    }

    pub fn interacts(
        ar: MRef<Agent>,
        br: MRef<Agent>,
        wp: &WorldParams,
        rp: &RuntimeParams,
        dsc: &mut DynStruct<ContactInfo>,
    ) {
        let d = {
            let a = &mut ar.lock().unwrap();
            let b = &mut br.lock().unwrap();
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let d2 = (dx * dx + dy * dy).max(1e-4);
            let d = d2.sqrt();
            let view_range = wp.world_size as f64 / wp.mesh as f64;
            if d >= view_range {
                return;
            }
            let dd = (if d < view_range * 0.8 {
                1.0
            } else {
                (1. - d / view_range) / 0.2
            }) / d
                / d2
                * AVOIDANCE
                * rp.avoidance
                / 50.;
            let ax = dx * dd;
            let ay = dy * dd;
            a.fx -= ax;
            a.fy -= ay;
            b.fx += ax;
            b.fy += ay;
            d
        };
        Agent::attracted(ar.clone(), br.clone(), wp, rp, d, dsc);
        Agent::attracted(br.clone(), ar.clone(), wp, rp, d, dsc);
    }

    pub fn is_infected(&self) -> bool {
        self.health == HealthType::Asymptomatic || self.health == HealthType::Symptomatic
    }

    pub fn get_new_pt(&self, ws: f64, mob_dist: &DistInfo) -> Point {
        let mut rng = rand::thread_rng();
        let mut gauss = Gaussian::new();
        let dst = gauss.my_random(mob_dist) * ws / 100.;
        let th = rng.gen::<f64>() * f64::consts::PI * 2.;
        let mut new_pt = Point {
            x: self.x + th.cos() * dst,
            y: self.y + th.sin() * dst,
        };
        if new_pt.x < 3. {
            new_pt.x = 3. - new_pt.x;
        } else if new_pt.x > ws - 3. {
            new_pt.x = (ws - 3.) * 2. - new_pt.x;
        }
        if new_pt.y < 3. {
            new_pt.y = 3. - new_pt.y;
        } else if new_pt.y > ws - 3. {
            new_pt.y = (ws - 3.) * 2. - new_pt.y;
        }

        new_pt
    }
    pub fn update_position(&mut self, wp: &WorldParams, rp: &RuntimeParams) {
        let ws = wp.world_size as f64;
        let spd = wp.steps_per_day as f64;
        if self.distancing {
            let dst = 1.0 + rp.dst_st / 5.0;
            self.fx *= dst;
            self.fy *= dst;
        }
        self.fx += wall(self.x) - wall(ws - self.x);
        self.fy += wall(self.y) - wall(ws - self.y);
        let mass = (if self.health == HealthType::Symptomatic {
            200.
        } else {
            10.
        }) * rp.mass
            / 100.;
        if let Some(abest) = &self.best {
            let best = abest.lock().unwrap();
            if !self.distancing {
                let dx = best.x - self.x;
                let dy = best.y - self.y;
                let d = dx.hypot(dy).max(0.01) * 20.;
                self.fx += dx / d;
                self.fy += dy / d;
            }
        }
        let fric = (1. - 0.5 * rp.friction / 100.).powf(1. / spd);
        self.vx = self.vx * fric + self.fx / mass / spd;
        self.vy = self.vy * fric + self.fy / mass / spd;
        let v = self.vx.hypot(self.vy);
        let max_v = 80.0 / spd;
        if v > max_v {
            self.vx *= max_v / v;
            self.vy *= max_v / v;
        }
        self.x += self.vx / spd;
        self.y += self.vy / spd;
        if self.x < AGENT_RADIUS {
            self.x = AGENT_RADIUS * 2. - self.x;
        } else if self.x > ws - AGENT_RADIUS {
            self.x = (ws - AGENT_RADIUS) * 2. - self.x;
        }
        if self.y < AGENT_RADIUS {
            self.y = AGENT_RADIUS * 2. - self.y;
        } else if self.y > ws - AGENT_RADIUS {
            self.y = (ws - AGENT_RADIUS) * 2. - self.y;
        }
    }

    fn starts_warping(world: &mut World, ar: MRef<Agent>, mode: WarpType, new_pt: Point) {
        world.add_new_warp(Arc::new(WarpInfo::new(ar.clone(), new_pt, mode)));
    }

    fn died(world: &mut World, ar: MRef<Agent>, mode: WarpType) {
        {
            let mut a = ar.lock().unwrap();
            a.new_health = HealthType::Died;
        }
        let mut rng = rand::thread_rng();
        let ws = world.world_params.world_size as f64;
        Agent::starts_warping(
            world,
            ar,
            mode,
            Point {
                x: (rng.gen::<f64>() * 0.248 + 1.001) * ws,
                y: (rng.gen::<f64>() * 0.468 + 0.001) * ws,
            },
        );
    }

    fn patient_step(world: &mut World, ar: MRef<Agent>, in_quarantine: bool) -> bool {
        let is_died = {
            let a = &mut ar.lock().unwrap();
            a.days_infected >= a.days_to_die
        };
        if is_died {
            {
                let a = ar.lock().unwrap();
                cummulate_histgrm(&mut world.death_p_hist, a.days_diseased);
            }
            Agent::died(
                world,
                ar,
                if in_quarantine {
                    WarpType::WarpToCemeteryH
                } else {
                    WarpType::WarpToCemeteryF
                },
            );
            true
        } else {
            let mut a = ar.lock().unwrap();
            if f64::MAX == a.days_to_die {
                // in the recovery phase
                if a.days_infected >= a.days_to_recover {
                    if a.health == HealthType::Symptomatic {
                        cummulate_histgrm(&mut world.recov_p_hist, a.days_diseased);
                    }
                    a.new_health = HealthType::Recovered;
                    a.days_infected = 0.;
                }
            } else if a.days_infected > a.days_to_recover {
                // starts recovery
                a.days_to_recover *= 1. + 10. / a.days_to_die;
                a.days_to_die = f64::MAX;
            } else if a.health == HealthType::Asymptomatic && a.days_infected >= a.days_to_onset {
                a.new_health = HealthType::Symptomatic;
                cummulate_histgrm(&mut world.incub_p_hist, a.days_infected);
            }
            false
        }
    }

    pub fn step_agent(wr: &MRef<World>, ar: &MRef<Agent>) {
        let world = &mut wr.lock().unwrap();
        let ws = world.world_params.world_size as f64;
        let spd = world.world_params.steps_per_day as f64;

        let health = ar.lock().unwrap().health;
        match health {
            HealthType::Asymptomatic => {
                {
                    let a = &mut ar.lock().unwrap();
                    a.days_infected += 1. / spd;
                }
                if Agent::patient_step(world, ar.clone(), false) {
                    return;
                }
            }
            HealthType::Symptomatic => {
                {
                    let a = &mut ar.lock().unwrap();
                    a.days_infected += 1. / spd;
                    a.days_diseased += 1. / spd;
                }
                if Agent::patient_step(world, ar.clone(), false) {
                    return;
                } else if ar.lock().unwrap().days_diseased >= world.runtime_params.tst_delay
                    && was_hit(spd, world.runtime_params.tst_sbj_sym / 100.)
                {
                    world.test_infection_of_agent(&ar.lock().unwrap(), TestType::TestAsSymptom);
                }
            }
            HealthType::Recovered => {
                let a = &mut ar.lock().unwrap();
                a.days_infected += 1. / spd;
                if a.days_infected > a.im_expr {
                    a.new_health = HealthType::Susceptible;
                    a.days_infected = 0.;
                    a.days_diseased = 0.;
                    a.reset_days(&world.runtime_params);
                }
            }
            _ => {}
        }
        if health != HealthType::Symptomatic
            && was_hit(spd, world.runtime_params.tst_sbj_asy / 100.)
        {
            let a = ar.lock().unwrap();
            world.test_infection_of_agent(&a, TestType::TestAsSuspected);
        }
        let org_idx = ar.lock().unwrap().index_in_pop(&world.world_params);
        {
            if health != HealthType::Symptomatic
                && was_hit(spd, world.runtime_params.mob_fr / 1000.)
            {
                let new_pt = {
                    ar.lock()
                        .unwrap()
                        .get_new_pt(ws, &world.runtime_params.mob_dist)
                };
                Agent::starts_warping(world, ar.clone(), WarpType::WarpInside, new_pt);
            } else {
                ar.lock()
                    .unwrap()
                    .update_position(&world.world_params, &world.runtime_params);
            }
        };
        let new_idx = { ar.lock().unwrap().index_in_pop(&world.world_params) };
        if new_idx != org_idx {
            let pop = &mut world._pop.lock().unwrap();
            pop[org_idx as usize].remove_p(ar);
            pop[new_idx as usize].push_front(ar.clone());
        }
    }

    pub fn step_agent_in_quarantine(wr: MRef<World>, ar: MRef<Agent>) {
        let world = &mut wr.lock().unwrap();
        let spd = world.world_params.steps_per_day as f64;
        let health = ar.lock().unwrap().health;
        match health {
            HealthType::Symptomatic => {
                let a = &mut ar.lock().unwrap();
                a.days_diseased += 1. / spd;
            }
            HealthType::Asymptomatic => {
                let a = &mut ar.lock().unwrap();
                a.days_infected += 1. / spd;
            }
            _ => {
                let new_pt = ar.lock().unwrap().org_pt;
                Agent::starts_warping(world, ar, WarpType::WarpBack, new_pt);
                return;
            }
        }
        if !Agent::patient_step(world, ar.clone(), true) && health == HealthType::Recovered {
            let new_pt = ar.lock().unwrap().org_pt;
            Agent::starts_warping(world, ar, WarpType::WarpBack, new_pt);
        }
    }
}

pub fn wall(d: f64) -> f64 {
    let d = if d < 0.02 { 0.02 } else { d };
    AVOIDANCE * 20. / d / d
}

pub fn was_hit(spd: f64, prob: f64) -> bool {
    let mut rng = rand::thread_rng();
    rng.gen::<f64>() > (1. - prob).powf(1. / spd)
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

fn get_index(p: f64, wp: &WorldParams) -> i32 {
    let ip = (p * wp.mesh as f64 / wp.world_size as f64).floor() as i32;
    if ip < 0 {
        0
    } else if ip >= wp.mesh {
        wp.mesh - 1
    } else {
        ip
    }
}

pub struct Gaussian {
    z: f64,
    second_time: bool,
    rng: ThreadRng,
}

static EXP_BASE: f64 = 0.02;
impl Gaussian {
    pub fn new() -> Gaussian {
        Gaussian {
            z: 0.,
            second_time: false,
            rng: rand::thread_rng(),
        }
    }

    fn gen(&mut self, mu: f64, sigma: f64) -> f64 {
        let x = if self.second_time {
            self.second_time = false;
            self.z
        } else {
            self.second_time = true;
            let r = (-2. * self.rng.gen::<f64>().ln()).sqrt();
            let th = self.rng.gen::<f64>() * f64::consts::PI * 2.;
            self.z = r * th.cos();
            r * th.sin()
        };
        x * sigma + mu
    }

    pub fn my_random(&mut self, p: &DistInfo) -> f64 {
        if p.mode == p.min {
            EXP_BASE.powf(self.rng.gen::<f64>() - EXP_BASE) / (1. - EXP_BASE) * (p.max - p.min)
                + p.min
        } else if p.mode == p.max {
            1. - EXP_BASE.powf(self.rng.gen::<f64>()) / (1. - EXP_BASE) * (p.max - p.min) + p.min
        } else {
            let mut x = self.gen(0.5, 0.166667);
            if x < 0. {
                x += (1.0 - x).floor();
            } else if x > 1. {
                x -= x.floor();
            }
            let a = (p.mode - p.min) / (p.max - p.mode);
            a * x / ((a - 1.) * x + 1.) * (p.max - p.min) + p.min
        }
    }
}
