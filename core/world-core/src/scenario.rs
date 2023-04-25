use math::Permille;
use scenario_operation::{AssignmentField, AssignmentQueue, ConditionField, EvalField, Operation};

use crate::world::commons::{RuntimeParams, WorldParams};

#[derive(Debug, Default)]
pub struct Scenario {
    index: usize,
    operations: Vec<Operation>,
    curr: Vec<AssignmentQueue>,
}

impl Scenario {
    pub fn new<T, F: Fn(T) -> Operation>(ops: Vec<T>, f: F) -> Self {
        Self {
            index: 0,
            operations: ops.into_iter().map(f).collect(),
            curr: Vec::new(),
        }
    }

    pub fn exec(&mut self, env: &mut EnvMut) {
        let op = &self.operations[self.index];
        if op.condition.eval(env) {
            for a in &op.assignments {
                self.curr.push(a.expand(env));
            }
        }

        for i in (0..self.curr.len()).rev() {
            let queue = self.curr.get_mut(i).unwrap();
            queue.pop_front().unwrap().assign(env);
            if !queue.is_empty() {
                continue;
            }
            drop(queue);

            self.curr.swap_remove(i);
        }
    }
}

pub struct EnvMut<'a> {
    runtime: &'a mut RuntimeParams,
    world: &'a mut WorldParams,
}

scenario_operation::impl_accessor!(
    self: EnvMut<'_>;
    AssignmentField {
        GatheringFrequency =>
            get { self.runtime.gat_fr }
            set(v) { self.runtime.gat_fr = v; }
        VaccinePerformRate =>
            get { self.runtime.vcn_p_rate.0 }
            set(v) { self.runtime.vcn_p_rate = Permille(v); }
    }
);

impl<'b> EvalField<ConditionField> for EnvMut<'b> {
    fn eval<'a>(&self, field: &'a ConditionField) -> (ConditionField, &'a ConditionField) {
        match field {
            ConditionField::Days(_) => (ConditionField::Days(self.runtime.days_elapsed), field),
        }
    }
}
