use crate::common_types::*;
use crate::contact::*;
use crate::iter::Next;

use std::sync::{Arc, Mutex, MutexGuard};

use rand::{self, prelude::ThreadRng, Rng};
use std::f64;

static AGENT_RADIUS: f64 = 0.75;
// static AGENT_SIZE: f64 = 0.665;

static AVOIDANCE: f64 = 0.2;

#[derive(Default, Debug)]
pub struct Agent {
    pub prev: Option<MRef<Agent>>,
    pub next: Option<MRef<Agent>>,
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
    pub contact_info_head: Option<MRef<ContactInfo>>,
    pub contact_info_tail: Option<MRef<ContactInfo>>,
}

impl Next<Agent> for Agent {
    fn n(&self) -> Option<MRef<Agent>> {
        self.next.clone()
    }
}

impl Agent {
    pub fn reset(&mut self, wp: &WorldParams) {
        let mut rng = rand::thread_rng();
        self.app = rng.gen();
        self.prf = rng.gen();
        self.x = rng.gen::<f64>() * (wp.world_size - 6) as f64 + 3.0;
        self.y = rng.gen::<f64>() * (wp.world_size - 6) as f64 + 3.0;
        let th: f64 = rng.gen::<f64>() * f64::consts::PI * 2.0;
        self.vx = th.cos();
        self.vy = th.sin();
        self.health = HealthType::Susceptible;
        self.n_infects = -1;
        self.is_out_of_field = true;
        self.last_tested = -999999;
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
        ar: &MRef<Agent>,
        br: &MRef<Agent>,
        wp: &WorldParams,
        rp: &RuntimeParams,
        d: f64,
        cs: &mut MutexGuard<ContactState>,
    ) {
        // add_new_cinfo
        let a = &mut ar.lock().unwrap();
        let b = &mut br.lock().unwrap();
        let mut x = (b.app - a.prf).abs();
        let spd = wp.steps_per_day as f64;
        x = (if x < 0.5 { x } else { 1.0 - x }) * 2.0;
        if a.best_dist > x {
            a.best_dist = x;
            a.best = Some(br.clone());
        }
        // check contact and infection
        if d < rp.infec_dst {
            if was_hit(spd, rp.cntct_trc / 100.) {
                cs.add_new_cinfo(ar, br, rp.step);
            }
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
        ar: &MRef<Agent>,
        br: &MRef<Agent>,
        wp: &WorldParams,
        rp: &RuntimeParams,
        cs: &mut MutexGuard<ContactState>,
    ) {
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
        Agent::attracted(ar, br, wp, rp, d, cs);
        Agent::attracted(br, ar, wp, rp, d, cs);
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
}

pub fn add_agent(
    ar: &MRef<Agent>,
    pop: &mut MutexGuard<Vec<Option<MRef<Agent>>>>,
    wp: &WorldParams,
) {
    let a = ar.lock().unwrap();
    let k = a.index_in_pop(wp) as usize;
    add_to_list(ar, &mut pop[k])
}

pub fn remove_agent(
    ar: &MRef<Agent>,
    pop: &mut MutexGuard<Vec<Option<MRef<Agent>>>>,
    wp: &WorldParams,
) {
    todo!();
}

pub fn add_to_list(
    ar: &MRef<Agent>,
    opt_br: &mut Option<MRef<Agent>>, //  idx: usize
) {
    let a = &mut ar.lock().unwrap();
    a.next = opt_br.clone();
    a.prev = None;
    if let Some(br) = &opt_br {
        let b = &mut br.lock().unwrap();
        b.prev = Some(ar.clone());
    }
    *opt_br = Some(ar.clone());
}

pub fn remove_from_list(ar: &MRef<Agent>, opt_ar: &mut Option<MRef<Agent>>) {
    let a = &mut ar.lock().unwrap();
    if let Some(nr) = &a.prev {
        nr.lock().unwrap().prev = a.next.clone();
    } else {
        *opt_ar = a.next.clone();
    }
    if let Some(nr) = &a.next {
        nr.lock().unwrap().next = a.prev.clone();
    }
}

pub fn wall(d: f64) -> f64 {
    let d = if d < 0.02 { 0.02 } else { d };
    AVOIDANCE * 20. / d / d
}

pub fn was_hit(spd: f64, prob: f64) -> bool {
    let mut rng = rand::thread_rng();
    rng.gen::<f64>() > (1. - prob).powf(1. / spd) //wp.steps_per_day as f64)
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
