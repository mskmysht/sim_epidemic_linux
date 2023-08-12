use crate::util::random;

use super::{agent::AgentRef, commons::ParamsForStep};
use enum_map::{macros::Enum, EnumMap};

use std::collections::VecDeque;

use rand::Rng;

#[derive(Eq, PartialEq, Clone, Enum, Debug)]
pub enum TestReason {
    AsSymptom,
    AsContact,
    AsSuspected,
    //[todo] TestPositiveRate,
}

#[derive(Enum, Clone)]
pub enum TestResult {
    Positive,
    Negative,
}

impl From<bool> for TestResult {
    fn from(is_positive: bool) -> Self {
        match is_positive {
            true => Self::Positive,
            false => Self::Negative,
        }
    }
}

pub struct Testee {
    agent: AgentRef,
    reason: TestReason,
    time_stamp: u32,
}

impl Testee {
    pub fn new(agent: AgentRef, reason: TestReason, time_stamp: u32) -> Self {
        Self {
            agent,
            reason,
            time_stamp,
        }
    }

    fn conduct(self, pfs: &ParamsForStep) -> (TestReason, TestResult) {
        // let mut agent = self.agent.write();
        let rng = &mut rand::thread_rng();
        let b = if let Some(ip) = self.agent.health.read().get_infected() {
            // P(U < 1 - (1-p)^x) = 1 - (1-p)^x = P(U > (1-p)^x)
            random::at_least_once_hit_in(ip.virus_variant.reproductivity, pfs.rp.tst_sens.r())
        } else {
            rng.gen::<f64>() > pfs.rp.tst_spec.r()
        };

        let result = TestResult::from(b);
        self.agent
            .testing
            .write()
            .notify_result(self.time_stamp, result.clone());
        (self.reason, result)
    }

    fn cancel(self) {
        self.agent.testing.write().cancel();
    }
}

pub struct TestQueue(VecDeque<Testee>);

impl TestQueue {
    pub fn new() -> Self {
        Self(VecDeque::new())
    }

    /// register a new testee
    pub fn push(&mut self, testee: Testee) {
        self.0.push_back(testee);
    }
    /// register new testees
    pub fn extend(&mut self, testees: Vec<Testee>) {
        self.0.extend(testees);
    }

    /// accept testees
    pub fn accept(
        &mut self,
        pfs: &ParamsForStep,
        count_reason: &mut EnumMap<TestReason, u32>,
        count_result: &mut EnumMap<TestResult, u32>,
    ) {
        let (latest, oldest) = {
            let l = pfs.rp.step as f64 - (pfs.rp.tst_proc * pfs.wp.steps_per_day());
            let o = l - pfs.rp.tst_dly_lim * pfs.wp.steps_per_day();
            (l as u32, o as u32)
        };
        let mut max_tests = {
            let rng = &mut rand::thread_rng();
            let m = pfs.wp.init_n_pop as f64 * pfs.rp.tst_capa.r() / pfs.wp.steps_per_day();
            if m.fract() > rng.gen() {
                m as usize + 1
            } else {
                m as usize
            }
        };

        while let Some(t) = self.0.front() {
            if t.time_stamp > latest || max_tests == 0 {
                break;
            }
            let t = self.0.pop_front().unwrap();
            if t.time_stamp > oldest && t.agent.location.read().in_field() {
                max_tests -= 1;
                let (reason, result) = t.conduct(pfs);
                count_reason[&reason] += 1;
                count_result[&result] += 1;
            } else {
                t.cancel();
            }
        }
    }
}
