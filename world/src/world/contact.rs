use super::{
    agent::Agent,
    commons::ParamsForStep,
    testing::{TestReason, Testee},
};

use std::collections::VecDeque;

/// a vector guarantees the ascending order of `time_stamp`
#[derive(Default)]
pub struct Contacts(VecDeque<ContactInfo>);

impl Contacts {
    const RETENTION_PERIOD: u64 = 14; // two weeks

    pub fn append(&mut self, agents: &mut Vec<Agent>, step: u64) {
        for agent in agents.drain(..) {
            self.0.push_back(ContactInfo::new(agent, step))
        }
    }

    pub fn get_testees(&mut self, pfs: &ParamsForStep) -> Vec<Testee> {
        let retention_steps = pfs.wp.steps_per_day * Self::RETENTION_PERIOD;
        self.0
            .drain(..)
            .filter_map(|ci| {
                if pfs.rp.step - ci.time_stamp < retention_steps {
                    ci.agent
                        .write()
                        .reserve_test(ci.agent.clone(), TestReason::AsContact, pfs)
                } else {
                    None
                }
            })
            .collect()
    }
}

struct ContactInfo {
    pub agent: Agent,
    pub time_stamp: u64,
}

impl ContactInfo {
    fn new(agent: Agent, time_stamp: u64) -> Self {
        Self { agent, time_stamp }
    }
}
