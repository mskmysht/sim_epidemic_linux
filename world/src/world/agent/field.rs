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

use std::{
    ops::DerefMut,
    sync::{Arc, Mutex},
};

use rayon::iter::ParallelIterator;

struct InteractionUpdate {
    force: Point,
    best: Option<(Point, f64)>,
    new_n_infects: u64,
    new_contacts: Vec<Agent>,
}

impl InteractionUpdate {
    fn new() -> Self {
        Self {
            force: Point::new(0.0, 0.0),
            best: None,
            new_n_infects: 0,
            new_contacts: Vec::new(),
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

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
    interaction: InteractionUpdate,
}

impl LocationLabel for FieldAgent {
    const LABEL: Location = Location::Field;
}

pub struct FieldStepInfo {
    contact_testees: Option<Vec<Testee>>,
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
            interaction: InteractionUpdate::new(),
        }
    }

    fn reset_for_step(&mut self) {
        self.interaction.reset();
    }

    fn step(
        &mut self,
        pfs: &ParamsForStep,
    ) -> (FieldStepInfo, Option<Either<WarpParam, TableIndex>>) {
        let mut agent = self.agent.0.lock().unwrap();
        let mut hist = None;
        let mut contact_testees = None;
        let mut test_reason = None;
        let transfer = 'block: {
            let agent = agent.deref_mut();
            if let Some(w) = agent.quarantine(pfs, &mut contact_testees) {
                break 'block Some(Either::Left(w));
            }
            if let Some(w) = agent.field_step(&mut hist, pfs) {
                break 'block Some(Either::Left(w));
            }
            test_reason = agent.check_test(pfs.wp, pfs.rp);
            if let Some(w) = agent.warp_inside(pfs) {
                break 'block Some(Either::Left(w));
            }
            agent
                .move_internal(
                    self.interaction.force,
                    &self.interaction.best,
                    &self.idx,
                    pfs,
                )
                .map(Either::Right)
        };

        agent
            .contacts
            .append(&mut self.interaction.new_contacts, pfs.rp.step);

        let fsi = FieldStepInfo {
            contact_testees,
            testee: test_reason.map(|reason| agent.reserve_test(self.agent.clone(), reason, pfs)),
            hist,
            infct: agent.update_n_infects(self.interaction.new_n_infects),
            health: agent.update_health(),
        };
        (fsi, transfer)
    }

    fn interacts(&mut self, fb: &mut Self, pfs: &ParamsForStep) {
        let a = &mut self.agent.0.lock().unwrap();
        let b = &mut fb.agent.0.lock().unwrap();
        if let Some((df, d)) = a.body.calc_force_delta(&b.body, pfs.wp, pfs.rp) {
            self.interaction.force -= df;
            fb.interaction.force += df;
            self.interaction.update_best(&a.body, &b.body);
            fb.interaction.update_best(&b.body, &a.body);

            if b.infect(a, d, pfs) {
                self.interaction.new_n_infects = 1;
            }
            if a.infect(b, d, pfs) {
                fb.interaction.new_n_infects += 1;
            }

            self.interaction.record_contact(&fb.agent, d, pfs);
            fb.interaction.record_contact(&self.agent, d, pfs);
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

    pub fn reset_for_step(&mut self) {
        self.table.par_iter_mut().horizontal().for_each(|(_, ags)| {
            for fa in ags {
                fa.reset_for_step();
            }
        });
    }

    pub fn steps(
        &mut self,
        warps: &mut Warps,
        test_queue: &mut TestQueue,
        step_log: &mut StepLog,
        pfs: &ParamsForStep,
    ) {
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
            if let Some(testees) = fsi.contact_testees {
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

    pub fn intersect(&mut self, pfs: &ParamsForStep) {
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
        gat: &Arc<Mutex<Gathering>>,
    ) {
        for fa in self.table[(row, column)].iter() {
            fa.agent
                .0
                .lock()
                .unwrap()
                .replace_gathering(gat_freq, Arc::downgrade(gat));
        }
    }
}
