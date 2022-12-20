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
    const RETENTION_PERIOD: u32 = 14; // two weeks

    pub fn append(&mut self, agents: Vec<Agent>, step: u32) {
        for agent in agents.into_iter() {
            self.0.push_back(ContactInfo::new(agent, step))
        }
    }

    pub fn drain_testees(&mut self, pfs: &ParamsForStep) -> Vec<Testee> {
        let retention_steps = pfs.wp.steps_per_day * Self::RETENTION_PERIOD;
        self.0
            .drain(..)
            .filter_map(|ci| {
                ci.agent
                    .write()
                    .reserve_test_with(ci.agent.clone(), pfs, |a| {
                        if a.is_in_field() && pfs.rp.step - ci.time_stamp < retention_steps {
                            Some(TestReason::AsContact)
                        } else {
                            None
                        }
                    })
            })
            .collect()
    }
}

struct ContactInfo {
    pub agent: Agent,
    pub time_stamp: u32,
}

impl ContactInfo {
    fn new(agent: Agent, time_stamp: u32) -> Self {
        Self { agent, time_stamp }
    }
}
