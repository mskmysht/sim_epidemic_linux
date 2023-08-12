use super::{
    agent::AgentRef,
    commons::ParamsForStep,
    testing::{TestReason, Testee},
};

use std::collections::VecDeque;

struct ContactInfo {
    agent: AgentRef,
    time_stamp: u32,
}

/// a vector guarantees the ascending order of `time_stamp`
#[derive(Default)]
pub struct Contacts(VecDeque<ContactInfo>);

impl Contacts {
    const RETENTION_PERIOD: u32 = 14; // two weeks

    pub fn append(&mut self, ars: Vec<AgentRef>, time_stamp: u32) {
        for ar in ars {
            self.0.push_back(ContactInfo {
                agent: ar,
                time_stamp,
            });
        }
    }

    pub fn drain_testees(&mut self, pfs: &ParamsForStep) -> Vec<Testee> {
        let retention_steps = pfs.wp.steps_per_day * Self::RETENTION_PERIOD;
        self.0
            .drain(..)
            .filter_map(|ci| {
                if pfs.rp.step - ci.time_stamp >= retention_steps {
                    return None;
                }
                // let a = ci.agent.read();
                // drop(a);
                if !ci.agent.testing.read().is_reservable(pfs)
                    || !ci.agent.location.read().in_field()
                {
                    return None;
                }
                ci.agent.testing.write().reserve();
                Some(Testee::new(ci.agent, TestReason::AsContact, pfs.rp.step))
            })
            .collect()
    }
}
