use crate::agent::Agent;
use crate::commons::{math, DistInfo, WRef, WrkPlcMode};
use crate::commons::{math::Point, random, HealthType, MRef, RuntimeParams, WorldParams};
use crate::world::Field;
use rand::seq::SliceRandom;
use rand::Rng;
use rayon::prelude::*;
use std::f64;
use std::sync::Mutex;
use std::sync::{Arc, Weak};

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
    pub fn affect(&self, pt: &Point) -> Option<(f64, Point)> {
        let dp = self.p - *pt;
        let d = dp.x.hypot(dp.y);
        if d > self.size + SURROUND || d < 0.01 {
            return None;
        }
        let f = self.strength / SURROUND
            * GATHERING_FORCE
            * (if d > self.size {
                self.size + SURROUND - d
            } else {
                d * SURROUND / self.size
            });
        Some((d, dp / d * f))
    }

    fn collect_participants(
        grid: &Field,
        gat_freq: &DistInfo,
        wp: &WorldParams,
        r: &f64,
        x: &f64,
        dy: f64,
        row: usize,
        gathering: &WRef<Gathering>,
    ) {
        let dx = (r * r - dy * dy).sqrt();
        let left = math::quantize(0f64.max(x - dx), wp.res_rate(), wp.mesh);
        let right = math::quantize(x.min(x + dx), wp.res_rate(), wp.mesh);
        grid[row][left..=right].par_iter().for_each(|(_, agents)| {
            let rng = rand::thread_rng();
            for agent in agents {
                let mut agent = agent.lock().unwrap();
                if agent.health != HealthType::Symptomatic
                    && rng.gen::<f64>() < random::modified_prob(agent.gat_freq, gat_freq) / 100.0
                {
                    agent.gathering = Weak::clone(gathering);
                }
            }
        });
    }

    fn ix_right(w_size: usize, mesh: usize, x: f64, grid: f64) -> usize {
        let right = ((w_size as f64).min(x) / grid).ceil() as usize;
        if right <= mesh {
            right
        } else {
            mesh
        }
    }

    const CENTERED_BIAS: f64 = 0.25;
    fn centered_bias(p: Point) -> f64 {
        let a = Self::CENTERED_BIAS / (1.0 - Self::CENTERED_BIAS);
        a / (1.0 - (1.0 - a) * p.x.abs().max(p.y.abs()))
    }

    pub fn setup<R: Rng>(
        // map: &mut GatheringMap,
        agent_grid: &Field,
        gat_spots_fixed: &[Point],
        agents: &[MRef<Agent>],
        wp: &WorldParams,
        rp: &RuntimeParams,
        rng: &mut R,
    ) -> MRef<Gathering> {
        // let rng = &mut rand::thread_rng();
        let gat = {
            let p = if gat_spots_fixed.len() > 0 && rp.gat_rnd_rt.r() < rng.gen::<f64>() {
                *gat_spots_fixed.choose(rng).unwrap()
            } else if wp.wrk_plc_mode == WrkPlcMode::WrkPlcNone {
                Point {
                    x: rng.gen::<f64>() * wp.field_size(),
                    y: rng.gen::<f64>() * wp.field_size(),
                }
            } else {
                agents.choose(rng).unwrap().lock().unwrap().org_pt
            };
            let size = {
                let size = random::my_random(rng, &rp.gat_sz);
                if wp.wrk_plc_mode == WrkPlcMode::WrkPlcCentered {
                    size * Self::centered_bias(p / wp.field_size() * 2.0 - 1.0)
                        * f64::consts::SQRT_2
                } else {
                    size
                }
            };
            Gathering {
                size,
                duration: random::my_random(rng, &rp.gat_dr),
                strength: random::my_random(rng, &rp.gat_st),
                p,
            }
        };
        let r = gat.size + SURROUND;
        let p = &gat.p;
        let bottom = math::quantize(0f64.max(p.y - r), wp.res_rate(), wp.mesh);
        let top = math::quantize(wp.field_size().min(p.y + r), wp.res_rate(), wp.mesh);
        let center = math::quantize(p.y + 0.5, wp.res_rate(), wp.mesh); // rounding

        let gat = Arc::new(Mutex::new(gat));
        let wgat = Arc::downgrade(&gat);
        for row in bottom..center {
            Gathering::collect_participants(
                agent_grid,
                &rp.gat_freq,
                wp,
                &r,
                &p.x,
                p.y - math::dequantize(row + 1, wp.res_rate()),
                row,
                &wgat,
            );
        }
        for row in (center..=top).rev() {
            Gathering::collect_participants(
                agent_grid,
                &rp.gat_freq,
                wp,
                &r,
                &p.x,
                p.y - math::dequantize(row, wp.res_rate()),
                row,
                &wgat,
            );
        }
        gat
    }

    pub fn step(&mut self, steps_per_day: i32) -> bool {
        self.duration -= 24. / steps_per_day as f64;
        self.duration <= 0.
    }

    // pub fn remove_from_map(gr: &MRef<Gathering>, gat_map: &mut GatheringMap) {
    //     let g = gr.lock().unwrap();
    //     for num in g.cell_idxs.iter() {
    //         if let Some(gs) = gat_map.get_mut(num) {
    //             if gs.len() > 1 {
    //                 gs.remove_p(gr);
    //             } else {
    //                 gat_map.remove(num);
    //             }
    //         }
    //     }
    // }
}
