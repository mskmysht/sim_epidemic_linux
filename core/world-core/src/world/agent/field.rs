use super::{
    super::{
        commons::ParamsForStep,
        testing::{TestQueue, Testee},
    },
    gathering::Gathering,
    warp::Warps,
    Agent, AgentHealth, Body, Location, LocationLabel, WarpParam,
};
use crate::{
    stat::{HealthCount, HealthDiff, HistInfo, InfectionCntInfo, Stat},
    util::{
        math::{Percentage, Point},
        random::{self, DistInfo},
        DrainMap,
    },
};

use std::{ops::DerefMut, sync::Arc};

use table::{Table, TableIndex};

use parking_lot::RwLock;
use rayon::iter::ParallelIterator;

#[derive(Default)]
struct TempParam {
    force: Point,
    best: Option<(Point, f64)>,
    new_n_infects: u32,
    new_contacts: Vec<Agent>,
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
            self.new_contacts.push(b.clone());
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
    agent: Agent,
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
        let mut agent = self.agent.write();

        let temp = std::mem::replace(&mut self.temp, TempParam::default());
        agent.contacts.append(temp.new_contacts, pfs.rp.step);
        agent
            .log
            .update_n_infects(temp.new_n_infects, &mut fsi.infct_info);

        let transfer = 'block: {
            let agent = agent.deref_mut();
            if let Some((w, testees)) = agent.check_quarantine(pfs) {
                fsi.contacted_testees = Some(testees);
                break 'block Some(Transfer::Extra(w));
            }
            fsi.testee = agent.reserve_test_in_field(self.agent.clone(), pfs);
            if let Some(w) = agent.health.field_step(
                temp.infected,
                agent.activeness,
                agent.age,
                &mut fsi.hist_info,
                &mut fsi.health_diff,
                pfs,
            ) {
                break 'block Some(Transfer::Extra(w));
            }
            if let Some(w) = agent.warp_inside(pfs) {
                break 'block Some(Transfer::Extra(w));
            }
            agent
                .move_internal(temp.force, temp.best, &self.idx, pfs)
                .map(Transfer::Intra)
        };

        (fsi, transfer)
    }

    fn interacts(&mut self, fb: &mut Self, pfs: &ParamsForStep) {
        let a = &mut self.agent.write();
        let b = &mut fb.agent.write();
        if let Some((df, d)) = a.body.calc_force_delta(&b.body, pfs) {
            self.temp.force -= df;
            fb.temp.force += df;
            self.temp.update_best(&a.body, &b.body);
            fb.temp.update_best(&b.body, &a.body);

            self.temp.infected(&a.health, &b.health, d, pfs);
            fb.temp.infected(&b.health, &a.health, d, pfs);

            self.temp.record_contact(&fb.agent, d, pfs);
            fb.temp.record_contact(&self.agent, d, pfs);
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

    pub fn clear(&mut self) {
        //[todo] fix mesh size
        self.table
            .par_iter_mut()
            .horizontal()
            .for_each(|(_, c)| c.clear());
    }

    pub fn step(
        &mut self,
        warps: &mut Warps,
        test_queue: &mut TestQueue,
        stat: &mut Stat,
        health_count: &mut HealthCount,
        pfs: &ParamsForStep,
    ) {
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

    pub fn replace_gathering(
        &self,
        row: usize,
        column: usize,
        gat_freq: &DistInfo<Percentage>,
        gat: &Arc<RwLock<Gathering>>,
    ) {
        for fa in self.table[(row, column)].iter() {
            fa.agent
                .write()
                .replace_gathering(gat_freq, Arc::downgrade(gat));
        }
    }
}
