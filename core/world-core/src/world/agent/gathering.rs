use super::{
    super::commons::{CenteredBias, RuntimeParams, WorkPlaceMode, WorldParams},
    field::Field,
};
use crate::{
    util::random,
    world::commons::{self, ParamsForStep},
};

use std::{f64, ops, sync::Arc};

use math::{self, Point};

use parking_lot::RwLock;
use rand::{seq::SliceRandom, Rng};

const SURROUND: f64 = 5.;
const GATHERING_FORCE: f64 = 5.;

#[derive(Debug, Default)]
pub struct Gathering {
    size: f64,
    duration: f64,
    strength: f64,
    p: Point,
}

impl Gathering {
    pub fn new(
        gat_spots_fixed: &[Point],
        agent_origins: &Vec<Point>,
        wp: &WorldParams,
        rp: &RuntimeParams,
    ) -> Self {
        let rng = &mut rand::thread_rng();
        let p = if !gat_spots_fixed.is_empty() && rp.gat_rnd_rt.r() < rng.gen::<f64>() {
            *gat_spots_fixed.choose(rng).unwrap()
        } else {
            *agent_origins.choose(rng).unwrap_or(&Point {
                x: rng.gen::<f64>() * wp.field_size(),
                y: rng.gen::<f64>() * wp.field_size(),
            })
        };
        let size = {
            let size = random::my_random(rng, &rp.gat_sz);
            if matches!(wp.wrk_plc_mode, Some(WorkPlaceMode::Centered)) {
                size * p.map(|c| c / wp.field_size() * 2.0 - 1.0).centered_bias()
                    * f64::consts::SQRT_2
            } else {
                size
            }
        };
        Self {
            size,
            duration: random::my_random(rng, &rp.gat_dr),
            strength: random::my_random(rng, &rp.gat_st),
            p,
        }
    }

    pub fn get_effect(&self, pt: &Point) -> (Option<Point>, Option<f64>) {
        let delta = self.p - *pt;
        let d = delta.x.hypot(delta.y);
        if d > self.size + SURROUND || d < 0.01 {
            return (None, None);
        }
        let mut f_norm = self.strength / SURROUND * GATHERING_FORCE;
        if d > self.size {
            f_norm *= self.size + SURROUND - d;
        } else {
            f_norm *= d * SURROUND / self.size;
        }
        let f = delta / d * f_norm;
        if d < self.size {
            (Some(f), Some(d / self.size))
        } else {
            (Some(f), None)
        }
    }

    /*
    fn ix_right(w_size: usize, mesh: usize, x: f64, grid: f64) -> usize {
        let right = ((w_size as f64).min(x) / grid).ceil() as usize;
        if right <= mesh {
            right
        } else {
            mesh
        }
    }*/
    fn get_range(r: f64, row: usize, p: &Point, wp: &WorldParams) -> ops::RangeInclusive<usize> {
        let dy = p.y - commons::dequantize(row, wp.res_rate());
        let dx = (r * r - dy * dy).sqrt();
        let left = commons::quantize(0f64.max(p.x - dx), wp.res_rate(), wp.mesh);
        let right = commons::quantize(p.x.min(p.x + dx), wp.res_rate(), wp.mesh);
        left..=right
    }

    pub fn get_locations(&self, wp: &WorldParams) -> Vec<(usize, usize)> {
        let r = self.size + SURROUND;
        let p = self.p;
        let bottom = commons::quantize(0f64.max(p.y - r), wp.res_rate(), wp.mesh);
        let top = commons::quantize(wp.field_size().min(p.y + r), wp.res_rate(), wp.mesh);
        let center = commons::quantize(p.y + 0.5, wp.res_rate(), wp.mesh); // rounding

        let mut locs = Vec::new();
        for row in bottom..center {
            locs.extend(Self::get_range(r, row + 1, &p, wp).map(|column| (row, column)))
        }
        for row in center..=top {
            locs.extend(Self::get_range(r, row, &p, wp).map(|column| (row, column)))
        }
        locs
    }

    pub fn step(&mut self, days_per_step: f64) -> bool {
        self.duration -= 24.0 * days_per_step;
        self.duration <= 0.0
    }
}

pub struct Gatherings(Vec<Arc<RwLock<Gathering>>>);

impl Gatherings {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }

    pub fn step(
        &mut self,
        field: &Field,
        gat_spots_fixed: &[Point],
        agent_origins: &Vec<Point>,
        pfs: &ParamsForStep,
    ) {
        self.0
            .retain_mut(|gat| !gat.write().step(pfs.wp.days_per_step()));

        // caliculate the number of gathering circles
        // using random number in exponetial distribution.
        let rng = &mut rand::thread_rng();
        let n_new_gat = (pfs.rp.gat_fr
            * pfs.wp.days_per_step()
            * (pfs.wp.field_size * pfs.wp.field_size) as f64
            / 1e5
            * (-(rng.gen::<f64>() * 0.9999 + 0.0001).ln()))
        .round() as usize;
        for _ in 0..n_new_gat {
            let gat = Arc::new(RwLock::new(Gathering::new(
                gat_spots_fixed,
                agent_origins,
                pfs.wp,
                pfs.rp,
            )));
            field.replace_gathering(&gat, pfs);
            self.0.push(gat);
        }
    }
}
