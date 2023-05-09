use std::sync::Arc;

use super::{
    super::{
        commons::ParamsForStep,
        testing::{TestQueue, Testee},
    },
    gathering::Gathering,
    warp::Warps,
    Agent, AgentHealth, AgentRef, Body, Location, LocationLabel, WarpParam,
};
use crate::{
    stat::{HealthCount, HealthDiff, HistInfo, InfectionCntInfo, Stat},
    util::{
        random::{self},
        DrainMap,
    },
    world::testing::TestReason,
};

use math::Point;
use parking_lot::RwLock;
use rand::Rng;
use table::{Table, TableIndex};

use rayon::{iter::ParallelIterator, prelude::IntoParallelIterator};

#[derive(Default)]
struct TempParam {
    force: Point,
    best: Option<(Point, f64)>,
    new_n_infects: u32,
    new_contacts: Vec<AgentRef>,
    infected: Option<(f64, usize)>,
}

impl TempParam {
    fn update_best(&mut self, a: &Body, b: &Body) {
        let x = a.calc_dist(b);
        match self.best {
            None => self.best = Some((b.pt, x)),
            Some((_, dist)) if dist > x => self.best = Some((b.pt, x)),
            _ => {}
        }
    }

    fn record_contact(&mut self, b: &Agent, d: f64, pfs: &ParamsForStep) {
        if d < pfs.rp.infec_dst
            && random::at_least_once_hit_in(pfs.wp.days_per_step(), pfs.rp.cntct_trc.r())
        {
            self.new_contacts.push(b.into());
        }
    }

    fn infected(&mut self, a: &AgentHealth, b: &AgentHealth, d: f64, pfs: &ParamsForStep) {
        if self.infected.is_none() {
            if let Some(infected) = a.infected_by(&b, d, pfs) {
                self.infected = Some(infected);
                self.new_n_infects += 1;
                // fb.new_n_infects = 1;
            }
        }
    }
}

pub struct FieldAgent {
    pub agent: Agent,
    idx: TableIndex,
    temp: TempParam,
}

impl LocationLabel for FieldAgent {
    const LABEL: Location = Location::Field;
}

#[derive(Default)]
struct FieldStepInfo {
    contacted_testees: Option<Vec<Testee>>,
    testee: Option<Testee>,
    infct_info: Option<InfectionCntInfo>,
    hist_info: Option<HistInfo>,
    health_diff: Option<HealthDiff>,
}

enum Transfer {
    Extra(WarpParam),
    Intra(TableIndex),
}

impl FieldAgent {
    fn new(agent: Agent, idx: TableIndex) -> Self {
        Self {
            agent: Self::label(agent),
            idx,
            temp: TempParam::default(),
        }
    }

    fn step(&mut self, pfs: &ParamsForStep) -> (FieldStepInfo, Option<Transfer>) {
        let mut fsi = FieldStepInfo::default();
        // let agent = &mut self.agent; //.write();

        let temp = std::mem::replace(&mut self.temp, TempParam::default());
        self.agent.contacts.append(temp.new_contacts, pfs.rp.step);
        self.agent
            .log
            .update_n_infects(temp.new_n_infects, &mut fsi.infct_info);

        let transfer = 'block: {
            // let agent = agent.deref_mut();
            if let Some((w, testees)) = self.agent.check_quarantine(pfs) {
                fsi.contacted_testees = Some(testees);
                break 'block Some(Transfer::Extra(w));
            }
            if self.agent.testing.read().is_reservable(pfs) {
                let mut r = None;
                if let Some(ip) = self.agent.health.read().get_symptomatic() {
                    if ip.days_diseased >= pfs.rp.tst_delay
                        && random::at_least_once_hit_in(
                            pfs.wp.days_per_step(),
                            pfs.rp.tst_sbj_sym.r(),
                        )
                    {
                        r = Some(TestReason::AsSymptom);
                    }
                } else if random::at_least_once_hit_in(
                    pfs.wp.days_per_step(),
                    pfs.rp.tst_sbj_asy.r(),
                ) {
                    r = Some(TestReason::AsSuspected);
                }
                if let Some(r) = r {
                    self.agent.testing.write().reserve();
                    fsi.testee = Some(Testee::new((&self.agent).into(), r, pfs.rp.step));
                }
            }
            if let Some(w) = self.agent.health.write().field_step(
                temp.infected,
                self.agent.activeness,
                self.agent.age,
                &mut fsi.hist_info,
                &mut fsi.health_diff,
                pfs,
            ) {
                break 'block Some(Transfer::Extra(w));
            }
            if let Some(w) = self.agent.warp_inside(pfs) {
                break 'block Some(Transfer::Extra(w));
            }
            self.agent
                .move_internal(temp.force, temp.best, &self.idx, pfs)
                .map(Transfer::Intra)
        };

        (fsi, transfer)
    }

    fn interacts(&mut self, fb: &mut Self, pfs: &ParamsForStep) {
        let a = &mut self.agent; //.write();
        let b = &mut fb.agent; //.write();
        if let Some((df, d)) = a.body.calc_force_delta(&b.body, pfs) {
            self.temp.force -= df;
            fb.temp.force += df;
            self.temp.update_best(&a.body, &b.body);
            fb.temp.update_best(&b.body, &a.body);

            let a_health = a.health.read();
            let b_health = b.health.read();
            self.temp.infected(&a_health, &b_health, d, pfs);
            fb.temp.infected(&b_health, &a_health, d, pfs);

            self.temp.record_contact(&b, d, pfs);
            fb.temp.record_contact(&a, d, pfs);
        }
    }
}

pub struct Field {
    table: Table<Vec<FieldAgent>>,
}

impl Field {
    pub fn new(mesh: usize) -> Self {
        Self {
            table: Table::new(mesh, mesh, Vec::new),
        }
    }

    pub fn clear(&mut self, agents: &mut Vec<Agent>) {
        //[todo] fix mesh size
        for (_, c) in self.table.iter_mut().horizontal() {
            for fa in c.drain(..) {
                agents.push(fa.agent);
            }
        }
    }

    pub fn step(
        &mut self,
        warps: &mut Warps,
        test_queue: &mut TestQueue,
        stat: &mut Stat,
        health_count: &mut HealthCount,
        pfs: &ParamsForStep,
    ) {
        // give vaccine ticket
        let mut vcn_subj_rem = 0.0;
        // let mut trc_vcn_set = Vec::new();
        for vp in &pfs.rp.vcn_info {
            if vp.perform_rate.r() <= 0.0 {
                continue;
            }
            vcn_subj_rem += pfs.wp.init_n_pop() * vp.perform_rate.r() * pfs.wp.days_per_step();
            let cnt = vcn_subj_rem.floor() as usize;
            if cnt == 0 {
                continue;
            }
            vcn_subj_rem = vcn_subj_rem.fract();
            // for num in &trc_vcn_set {}
            // for k in (0..)
        }

        self.interact(&pfs);
        let tmp = self
            .table
            .par_iter_mut()
            .horizontal()
            .map(|(_, ags)| ags.drain_map_mut(|fa| fa.step(pfs)))
            .collect::<Vec<_>>();

        for (fsi, opt) in tmp.into_iter().flatten() {
            if let Some(hist) = fsi.hist_info {
                stat.hists.push(hist);
            }
            if let Some(infct) = fsi.infct_info {
                stat.infcts.push(infct);
            }
            if let Some(hd) = fsi.health_diff {
                health_count.apply_difference(hd);
            }
            if let Some(testees) = fsi.contacted_testees {
                test_queue.extend(testees);
            }
            if let Some(testee) = fsi.testee {
                test_queue.push(testee);
            }
            if let Some((t, fa)) = opt {
                match t {
                    Transfer::Extra(warp) => warps.add(fa.agent, warp),
                    Transfer::Intra(idx) => self.add(fa.agent, idx),
                }
            }
        }
    }

    pub fn replace_gathering(&self, gathering: &Arc<RwLock<Gathering>>, pfs: &ParamsForStep) {
        let locs = gathering.read().get_locations(pfs.wp);
        locs.into_par_iter().for_each(|loc| {
            for fa in &self.table[loc] {
                if !fa.agent.health.read().is_symptomatic()
                    && rand::thread_rng().gen::<f64>()
                        < random::modified_prob(fa.agent.gat_info.read().gat_freq, &pfs.rp.gat_freq)
                            .r()
                {
                    fa.agent.gat_info.write().gathering = Arc::downgrade(gathering);
                }
            }
        });
    }

    pub fn add(&mut self, agent: Agent, idx: TableIndex) {
        self.table[idx.clone()].push(FieldAgent::new(agent, idx));
    }

    fn interact(&mut self, pfs: &ParamsForStep) {
        // |-|a|b|a|b|
        self.table
            .par_iter_mut()
            .east()
            .for_each(move |((_, a_ags), (_, b_ags))| {
                Self::interact_intercells(a_ags, b_ags, pfs);
            });
        // |a|b|a|b|
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

        // |-|
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

        // |-|-|
        // |a| |
        // | |b|
        self.table
            .par_iter_mut()
            .southeast()
            .for_each(|((_, a_ags), (_, b_ags))| {
                Self::interact_intercells(a_ags, b_ags, pfs);
            });

        // |-|-|
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
}
