use crate::{
    agent::{Agent, ParamsForStep},
    testing::Testee,
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
        let old_time_stamp = pfs.rp.step - pfs.wp.steps_per_day * Self::RETENTION_PERIOD;
        self.0
            .drain(..)
            .filter_map(|ci| {
                if ci.time_stamp <= old_time_stamp {
                    None
                } else {
                    ci.agent.try_reserve_test(pfs)
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
