use crate::contract::*;
use crate::{
    common_types::*,
    world::{WarpInfo, World},
};
use std::sync::{Arc, Mutex};

use rand::{self, Rng};
use std::f64;

static AGENT_RADIUS: f64 = 0.75;
static AGENT_SIZE: f64 = 0.665;

static AVOIDANCE: f64 = 0.2;

pub type AgentId = u32;

#[derive(Default, Debug)]
pub struct Agent {
    pub prev: Option<AgentId>,
    pub next: Option<AgentId>,
    pub id: i32,
    // struct AgentRec *prev, *next;
    app: f64,
    prf: f64,
    pub x: f64,
    pub y: f64,
    vx: f64,
    vy: f64,
    pub fx: f64,
    pub fy: f64,
    org_pt: Point,
    days_infected: f64,
    days_diseased: f64,
    days_to_recover: f64,
    days_to_onset: f64,
    days_to_die: f64,
    im_expr: f64,
    pub health: HealthType,
    new_health: HealthType,
    pub n_infects: i32,
    new_n_infects: i32,
    pub distancing: bool,
    pub is_out_of_field: bool,
    pub is_warping: bool,
    pub got_at_hospital: bool,
    pub in_test_queue: bool,
    pub last_tested: i32,
    best: Option<AgentId>,
    best_dist: f64,
    pub contact_info_head: Option<ContactInfoId>,
    pub contact_info_tail: Option<ContactInfoId>,
}

impl Agent {
    pub fn new() -> Agent {
        Agent::default()
    }

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
    pub fn reset_days(&mut self, rp: &RuntimeParams) {}

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

    fn attracted(&self, b: &Agent, wp: &WorldParams, rp: &RuntimeParams, d: f64) {
        // add_new_cinfo
    }
    pub fn interacts(&mut self, b: &mut Agent, wp: &WorldParams, rp: &RuntimeParams) {
        let dx = b.x - self.x;
        let dy = b.y - self.y;
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
        self.fx -= ax;
        self.fy -= ay;
        b.fx += ax;
        b.fy += ay;
        self.attracted(b, wp, rp, d);
        b.attracted(self, wp, rp, d);
    }

    pub fn is_infected(&self) -> bool {
        self.health == HealthType::Asymptomatic || self.health == HealthType::Symptomatic
    }

    fn starts_warping(self_: Arc<Mutex<Agent>>, mode: WarpType, new_pt: Point, world: &mut World) {
        world.add_new_warp(Arc::new(WarpInfo::new(self_.clone(), new_pt, mode)));
    }

    fn died(self_: Arc<Mutex<Agent>>, mode: WarpType, wp: &WorldParams, world: &mut World) {
        let mut a = self_.lock().unwrap();
        a.new_health = HealthType::Died;
        let mut rng = rand::thread_rng();
        Agent::starts_warping(
            self_.clone(),
            mode,
            Point {
                x: (rng.gen::<f64>() * 0.248 + 1.001) * wp.world_size as f64,
                y: (rng.gen::<f64>() * 0.468 + 0.001) * wp.world_size as f64,
            },
            world,
        );
    }
    fn patient_step(
        self_: Arc<Mutex<Agent>>,
        wp: &WorldParams,
        in_quarantine: bool,
        world: &mut World,
    ) -> bool {
        let mut a = self_.lock().unwrap();
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
        } else if a.days_infected >= a.days_to_die {
            cummulate_histgrm(&mut world.death_p_hist, a.days_diseased);
            Agent::died(
                self_.clone(),
                if in_quarantine {
                    WarpType::WarpToCemeteryH
                } else {
                    WarpType::WarpToCemeteryF
                },
                wp,
                world,
            );
            return true;
        } else if a.health == HealthType::Asymptomatic && a.days_infected >= a.days_to_onset {
            a.new_health = HealthType::Symptomatic;
            cummulate_histgrm(&mut world.incub_p_hist, a.days_infected);
        }
        return false;
    }

    pub fn step_agent(
        self_: Arc<Mutex<Agent>>,
        rp: &RuntimeParams,
        wp: &WorldParams,
        world: &mut World,
    ) {
        let ws = wp.world_size as f64;
        let spd = wp.steps_per_day as f64;
        let mut a = self_.lock().unwrap();
        match a.health {
            HealthType::Asymptomatic => {
                a.days_infected += 1. / spd;
                if Agent::patient_step(self_.clone(), wp, false, world) {
                    return;
                }
            }
            HealthType::Symptomatic => {
                a.days_infected += 1. / spd;
                a.days_diseased += 1. / spd;
                if Agent::patient_step(self_.clone(), wp, false, world) {
                    return;
                } else if a.days_diseased >= rp.tst_delay && was_hit(wp, rp.tst_sbj_sym / 100.) {
                    world.test_infection_of_agent(&a, TestType::TestAsSymptom);
                }
            }
            HealthType::Recovered => {
                a.days_infected += 1. / spd;
                if a.days_infected > a.im_expr {
                    a.new_health = HealthType::Susceptible;
                    a.days_infected = 0.;
                    a.days_diseased = 0.;
                    a.reset_days(rp);
                }
            }
            _ => {}
        }
        if a.health != HealthType::Symptomatic && was_hit(wp, rp.tst_sbj_asy / 100.) {
            world.test_infection_of_agent(&a, TestType::TestAsSuspected);
        }
        let org_idx = a.index_in_pop(wp);
        if a.health != HealthType::Symptomatic && was_hit(wp, rp.mob_fr / 1000.) {
            let mut rng = rand::thread_rng();
            let dst = my_random(&rp.mob_dist) * ws / 100.;
            let th = rng.gen::<f64>() * f64::consts::PI * 2.;
            let mut new_pt = Point {
                x: a.x + th.cos() * dst,
                y: a.y + th.sin() * dst,
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
            Agent::starts_warping(self_.clone(), WarpType::WarpInside, new_pt, world);
            return;
        } else {
            if a.distancing {
                let dst = 1.0 + rp.dst_st / 5.0;
                a.fx *= dst;
                a.fy *= dst;
            }
            a.fx += wall(a.x) - wall(ws - a.x);
            a.fy += wall(a.y) - wall(ws - a.y);
            let mass = (if a.health == HealthType::Symptomatic {
                200.
            } else {
                10.
            }) * rp.mass
                / 100.;
            if let Some(p) = a.best {
                let best = &world.agents.lock().unwrap()[p as usize];
                if !a.distancing {
                    let dx = best.x - a.x;
                    let dy = best.y - a.y;
                    let d = dx.hypot(dy).max(0.01) * 20.;
                    a.fx += dx / d;
                    a.fy += dy / d;
                }
            }
            let fric = (1. - 0.5 * rp.friction / 100.).powf(1. / spd);
            a.vx = a.vx * fric + a.fx / mass / spd;
            a.vy = a.vy * fric + a.fy / mass / spd;
            let v = a.vx.hypot(a.vy);
            let max_v = 80.0 / spd;
            if v > max_v {
                a.vx *= max_v / v;
                a.vy *= max_v / v;
            }
            a.x += a.vx / spd;
            a.y += a.vy / spd;
            if a.x < AGENT_RADIUS {
                a.x = AGENT_RADIUS * 2. - a.x;
            } else if a.x > ws - AGENT_RADIUS {
                a.x = (ws - AGENT_RADIUS) * 2. - a.x;
            }
            if a.y < AGENT_RADIUS {
                a.y = AGENT_RADIUS * 2. - a.y;
            } else if a.y > ws - AGENT_RADIUS {
                a.y = (ws - AGENT_RADIUS) * 2. - a.y;
            }
        }
        let new_idx = a.index_in_pop(wp);
        if new_idx != org_idx {
            world.remove_from_pop(org_idx as usize);
            world.add_to_pop(new_idx as usize);
        }
    }
}

fn wall(d: f64) -> f64 {
    let d = if d < 0.02 { 0.02 } else { d };
    AVOIDANCE * 20. / d / d
}

fn was_hit(wp: &WorldParams, prob: f64) -> bool {
    let mut rng = rand::thread_rng();
    rng.gen::<f64>() > (1. - prob).powf(1. / wp.steps_per_day as f64)
}

fn cummulate_histgrm(h: &mut Vec<MyCounter>, d: f64) {
    let ds = d.floor() as usize;
    if h.len() <= ds {
        let n = ds - h.len();
        for i in 0..=n {
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

pub fn my_random(p: &DistInfo) -> f64 {
    todo!();
}
// CGFloat my_random(DistInfo *p) {
// 	if (p->mode == p->min) return (pow(EXP_BASE, random() / (CGFloat)0x7fffffff) - EXP_BASE)
// 		/ (1. - EXP_BASE) * (p->max - p->min) + p->min;
// 	else if (p->mode == p->max) return (1. - pow(EXP_BASE, random() / (CGFloat)0x7fffffff))
// 		/ (1. - EXP_BASE) * (p->max - p->min) + p->min;
// 	CGFloat x = random_guassian(.5, .166667);
// 	if (x < 0.) x += floor(1. - x);
// 	else if (x > 1.) x -= floor(x);
// //	if (kurtosis != 0.) {
// //		CGFloat b = pow(2., -kurtosis);
// //		/* x = (x < .5)? b * x / ((b - 1) * x * 2. + 1.) :
// //			(x - .5) / ((x + b - b * x) * 2. - 1.) + .5;  */
// //		x = (x < .5)? pow(x * 2., b) * .5 : 1. - pow(2. - x * 2., b) * .5;
// //	}
// //	if (x < 0.) x = 0.; else if (x > 1.) x = 1.;
// 	CGFloat a = (p->mode - p->min) / (p->max - p->mode);
// 	return a * x / ((a - 1.) * x + 1.) * (p->max - p->min) + p->min;
// }
