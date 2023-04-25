use scenario_operation::{vec, AssignmentField, ConditionField, EvalField, Extract, Operation};

use crate::world::commons::{RuntimeParams, WorldParams};

#[derive(Debug, Default)]
pub struct Scenario {
    index: usize,
    operations: Vec<Operation>,
    curr: Vec<vec::AssignmentField>,
}

impl Scenario {
    pub fn new<T, F: Fn(T) -> Operation>(ops: Vec<T>, f: F) -> Self {
        Self {
            index: 0,
            operations: ops.into_iter().map(f).collect(),
            curr: Vec::new(),
        }
    }

    pub fn exec(&mut self, env: EnvMut) {
        env.world.init_n_pop += 1;
        let op = &self.operations[self.index];
        if op.condition.eval(&env) {
            for a in &op.assignments {
                self.curr.push(a.expand(&env));
            }
        }
    }
}

pub struct EnvMut<'a> {
    runtime: &'a mut RuntimeParams,
    world: &'a mut WorldParams,
}

scenario_operation::impl_extract!(
    AssignmentField -> EnvMut<'_>[self] {
        GatheringFrequency => self.runtime.gat_fr,
        VaccinePerformRate => self.runtime.gat_fr,
    }
);

impl<'b> EvalField<ConditionField> for EnvMut<'b> {
    fn eval<'a>(&self, field: &'a ConditionField) -> (ConditionField, &'a ConditionField) {
        match field {
            ConditionField::Days(_) => (ConditionField::Days(self.runtime.days_elapsed), field),
        }
    }
}
