use std::ops::{Add, AddAssign, Sub};

use scenario_operation::{Assignment, AssignmentField, ConditionField, EvalField, Operation};

use crate::world::commons::{RuntimeParams, WorldParams};

#[derive(Debug, Default)]
pub struct Scenario {
    index: usize,
    operations: Vec<Operation>,
}

impl Scenario {
    pub fn new<T, F: Fn(T) -> Operation>(ops: Vec<T>, f: F) -> Self {
        Self {
            index: 0,
            operations: ops.into_iter().map(f).collect(),
        }
    }

    pub fn exec(&self, env: EnvMut) {
        env.world.init_n_pop += 1;
        let op = &self.operations[self.index];
        if op.condition.eval(&env) {
            for a in &op.assignments {
                todo!();
                // env.assign(f);
            }
        }
    }
}

pub struct EnvMut<'a> {
    runtime: &'a mut RuntimeParams,
    world: &'a mut WorldParams,
}

// impl EnvMut {
//     fn assign(&self, f: &AssignmentField) {
//         match f {
//             AssignmentField::GatheringFrequency(a) => self.hoge(a),
//             AssignmentField::VaccinePerformRate(a) => todo!(),
//         }
//     }
// }

impl<'b> EvalField<ConditionField> for EnvMut<'b> {
    fn eval<'a>(&self, field: &'a ConditionField) -> (ConditionField, &'a ConditionField) {
        match field {
            ConditionField::Days(_) => (ConditionField::Days(self.runtime.days_elapsed), field),
        }
    }
}
