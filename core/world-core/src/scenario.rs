use std::collections::VecDeque;

use math::Permille;
use scenario_operation::{
    accessor, Assign, Assignment, ConditionField, EvalField, Interpolate, MyField, Operation,
    VaccinationStrategy,
};

use crate::world::commons::{self, RuntimeParams};

#[derive(Debug, Default)]
pub struct Scenario {
    index: usize,
    operations: Vec<Operation>,
    curr: Vec<VecDeque<MyField>>,
}

impl Scenario {
    pub fn new<T, F: Fn(T) -> Operation>(ops: Vec<T>, f: F) -> Self {
        Self {
            index: 0,
            operations: ops.into_iter().map(f).collect(),
            curr: Vec::new(),
        }
    }

    pub fn exec(&mut self, rp: &mut RuntimeParams) {
        let op = &self.operations[self.index];
        if op.condition.eval(rp) {
            for a in &op.assignments {
                match a {
                    Assignment::Value(v) => self.curr.push(VecDeque::from([v.clone()])),
                    Assignment::Interpolate(v, n) => {
                        self.curr.push(Interpolate::interpolate(rp, v, n));
                    }
                }
            }
        }

        for i in (0..self.curr.len()).rev() {
            let queue = self.curr.get_mut(i).unwrap();
            Assign::assign(rp, queue.pop_front().unwrap());
            if !queue.is_empty() {
                continue;
            }
            drop(queue);

            self.curr.swap_remove(i);
        }
    }

    pub fn reset(&mut self) {
        self.index = 0;
        self.curr.clear();
    }
}

accessor!(env: commons::VaccinationStrategy, VaccinationStrategy {
    PerformRate(v) =>
        get { &env.perform_rate.0 }
        set { env.perform_rate = Permille(v); }
});

accessor!(rp: RuntimeParams, MyField {
    GatheringFrequency(v) =>
        get { &rp.gat_fr }
        set { rp.gat_fr = v }
    Vaccination(v) =>
        get { &rp.vx_stg[&v.index] }
        set {
            if let Some(t) = rp.vx_stg.get_mut(&v.index) {
                Assign::assign(t, v.value);
            }
        }
});

impl EvalField<ConditionField> for RuntimeParams {
    fn eval<'a>(&self, field: &'a ConditionField) -> (ConditionField, &'a ConditionField) {
        match field {
            ConditionField::Days(_) => (ConditionField::Days(self.days_elapsed), field),
        }
    }
}
