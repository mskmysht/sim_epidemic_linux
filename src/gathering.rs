use rand::Rng;
use std::{collections::HashMap, sync::MutexGuard};

use std::sync::Arc;
use std::sync::Mutex;

use crate::agent::*;
use crate::common_types::*;

static SURROUND: f64 = 5.;
static GATHERING_FORCE: f64 = 5.;

pub type GatheringMap = HashMap<i32, Vec<Arc<Mutex<Gathering>>>>;

#[derive(Default, PartialEq)]
pub struct Gathering {
    size: f64,
    duration: f64,
    strength: f64,
    p: Point,
    pub cell_idxs: Vec<i32>,
}

impl Gathering {
    fn record_gat(
        amg: &Arc<Mutex<Gathering>>,
        map: &mut GatheringMap,
        row: i32,
        left: i32,
        right: i32,
    ) {
        for ix in left..right {
            let num = row + ix;
            {
                let mut g = amg.lock().unwrap();
                g.cell_idxs.push(num);
            }
            match map.get_mut(&num) {
                Some(g) => {
                    g.push(amg.clone());
                }
                None => {
                    map.insert(num, vec![amg.clone()]);
                }
            }
        }
    }
    pub fn new(
        // &'a mut self,
        map: &mut GatheringMap,
        wp: &WorldParams,
        rp: &RuntimeParams,
    ) -> Arc<Mutex<Gathering>> {
        let mut rng = rand::thread_rng();
        let mut gauss = Gaussian::new();
        let gat = Gathering {
            size: gauss.my_random(&rp.gat_sz),
            duration: gauss.my_random(&rp.gat_dr),
            strength: gauss.my_random(&rp.gat_st),
            cell_idxs: vec![],
            ..Gathering::default()
        };
        let w_size = wp.world_size;
        let p = Point {
            x: rng.gen::<f64>() * w_size as f64,
            y: rng.gen::<f64>() * w_size as f64,
        };
        let grid = wp.world_size as f64 / wp.mesh as f64;
        let r = gat.size + SURROUND;
        let bottom = ((p.y - r).max(0.) / grid).floor() as i32;
        let top = {
            let t = ((p.y + r).min(wp.world_size as f64) / grid).floor() as i32;
            if t >= wp.mesh {
                wp.mesh - 1
            } else {
                t
            }
        };
        let center = {
            let c = (p.y / grid).round() as i32;
            if c >= wp.mesh {
                wp.mesh - 1
            } else {
                c
            }
        };

        let amg = Arc::new(Mutex::new(gat));
        for iy in bottom..center {
            let dy = p.y - (iy + 1) as f64 * grid;
            let dx = (r * r - dy * dy).sqrt();
            Gathering::record_gat(
                &amg,
                map,
                iy * wp.mesh,
                ((p.x - dx).max(0.) / grid).floor() as i32,
                ((p.x + dx).min(wp.world_size as f64) / grid).ceil() as i32,
            );
        }
        for iy in (center..=top).rev() {
            let dy = p.y - iy as f64 * grid;
            let dx = (r * r - dy * dy).sqrt();
            Gathering::record_gat(
                &amg,
                map,
                iy * wp.mesh,
                ((p.x - dx).max(0.) / grid).floor() as i32,
                ((p.x + dx).min(wp.world_size as f64) / grid).ceil() as i32,
            );
        }
        amg
    }
    pub fn step(&mut self, steps_per_day: i32) -> bool {
        self.duration -= 24.;
        return (self.duration / steps_per_day as f64) <= 0.;
    }
    fn remove_from_map(&self, gat_map: &mut GatheringMap) {
        for num in self.cell_idxs.iter() {
            if let Some(gs) = gat_map.get_mut(num) {
                if gs.len() > 1 {
                    gs.retain(|amg| &(*amg.lock().unwrap()) != self);
                } else {
                    gat_map.remove(num);
                }
            }
        }
    }
    pub fn affect_to_agent(&self, a: &mut MutexGuard<Agent>) {
        let dx = self.p.x - a.x;
        let dy = self.p.y - a.y;
        let d = f64::hypot(dx, dy);
        if d > self.size + SURROUND || d < self.size - SURROUND {
            return;
        }
        let f = self.strength / SURROUND
            * GATHERING_FORCE
            * (if d > self.size {
                self.size + SURROUND - d
            } else if self.size > SURROUND {
                d - self.size + SURROUND
            } else {
                d * SURROUND / self.size
            });
        a.fx += dx / d * f;
        a.fy += dy / d * f;
    }
}

pub fn manage_gatherings(
    gatherings: &mut Vec<MRef<Gathering>>,
    gat_map: &mut GatheringMap,
    wp: &WorldParams,
    rp: &RuntimeParams,
) {
    gatherings.retain(|amg| {
        let mut g = amg.lock().unwrap();
        g.remove_from_map(gat_map);
        !g.step(wp.steps_per_day)
    });
    //	caliculate the numner of gathering circles
    //	using random number in exponetial distribution.
    let mut rng = rand::thread_rng();
    let n_new_gat = (rp.gat_fr / wp.steps_per_day as f64 * (wp.world_size * wp.world_size) as f64
        / 1e5
        * (-(rng.gen::<f64>() * 0.9999 + 0.0001).ln()))
    .round() as i32;
    for _ in 0..n_new_gat {
        gatherings.push(Gathering::new(gat_map, wp, rp));
    }
}
