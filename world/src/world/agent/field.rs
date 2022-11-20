use super::{
    super::{
        commons::ParamsForStep,
        testing::{TestQueue, Testee},
    },
    gathering::Gathering,
    warp::Warps,
    Agent, Body, Location, LocationLabel, WarpParam,
};
use crate::{
    log::{HealthDiff, StepLog},
    stat::{HistInfo, InfectionCntInfo},
    util::{
        math::{Percentage, Point},
        random::{self, DistInfo},
        table::{Table, TableIndex},
        DrainMap, Either,
    },
};

use std::{ops::DerefMut, sync::Arc};

use parking_lot::RwLock;
use rayon::iter::ParallelIterator;

#[derive(Default)]
struct TempParam {
    force: Point,
    best: Option<(Point, f64)>,
    new_n_infects: u64,
    new_contacts: Vec<Agent>,
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
pub struct FieldStepInfo {
    contacted_testees: Option<Vec<Testee>>,
    testee: Option<Testee>,
    hist: Option<HistInfo>,
    infct: Option<InfectionCntInfo>,
    health: Option<HealthDiff>,
}

impl FieldAgent {
    fn new(agent: Agent, idx: TableIndex) -> Self {
        Self {
            agent: Self::label(agent),
            idx,
            temp: TempParam::default(),
        }
    }

    fn step(
        &mut self,
        pfs: &ParamsForStep,
    ) -> (FieldStepInfo, Option<Either<WarpParam, TableIndex>>) {
        let temp = std::mem::replace(&mut self.temp, TempParam::default());
        let mut fsi = FieldStepInfo::default();
        let mut agent = self.agent.write();
        let transfer = 'block: {
            let agent = agent.deref_mut();
            if let Some(w) = agent.check_quarantine(&mut fsi.contacted_testees, pfs) {
                break 'block Some(Either::Left(w));
            }
            if let Some(w) = agent.field_step(&mut fsi.hist, pfs) {
                break 'block Some(Either::Left(w));
            }
            if let Some(w) = agent.warp_inside(pfs) {
                break 'block Some(Either::Left(w));
            }
            agent
                .move_internal(temp.force, temp.best, &self.idx, pfs)
                .map(Either::Right)
        };

        agent.contacts.append(temp.new_contacts, pfs.rp.step);
        fsi.infct = agent.log.update_n_infects(temp.new_n_infects);
        fsi.health = agent.health.update();
        fsi.testee = self.agent.reserve_test(pfs, |a| a.check_test_in_field(pfs));

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

            if b.infect(a, d, pfs) {
                self.temp.new_n_infects = 1;
            }
            if a.infect(b, d, pfs) {
                fb.temp.new_n_infects += 1;
            }

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
        step_log: &mut StepLog,
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
            if let Some(h) = fsi.hist {
                step_log.hists.push(h);
            }
            if let Some(h) = fsi.health {
                step_log.apply_difference(h);
            }
            if let Some(i) = fsi.infct {
                step_log.infcts.push(i);
            }
            if let Some(testees) = fsi.contacted_testees {
                test_queue.extend(testees);
            }
            if let Some(testee) = fsi.testee {
                test_queue.push(testee);
            }
            if let Some((t, fa)) = opt {
                match t {
                    Either::Left(warp) => warps.add(fa.agent, warp),
                    Either::Right(idx) => self.add(fa.agent, idx),
                }
            }
        }
    }

    pub fn add(&mut self, agent: Agent, idx: TableIndex) {
        self.table[idx.clone()].push(FieldAgent::new(agent, idx));
    }

    fn interact(&mut self, pfs: &ParamsForStep) {
        // |x|a|b|a|b|
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

        // |x|
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

        // | | |
        // |a| |
        // | |b|
        self.table
            .par_iter_mut()
            .southeast()
            .for_each(|((_, a_ags), (_, b_ags))| {
                Self::interact_intercells(a_ags, b_ags, pfs);
            });

        // | | |
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
