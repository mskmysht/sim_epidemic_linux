use super::{agent::Agent, commons::ParamsForStep};
use crate::util::enum_map::{Enum, EnumMap};

use std::collections::VecDeque;

use rand::Rng;

#[derive(Eq, PartialEq, Clone, Enum, Debug)]
pub enum TestReason {
    AsSymptom,
    AsContact,
    AsSuspected,
    // [todo] TestPositiveRate,
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
    agent: Agent,
    reason: TestReason,
    // result: TestResult,
    time_stamp: u64,
}

impl Testee {
    pub fn new(agent: Agent, reason: TestReason, time_stamp: u64) -> Self {
        Self {
            agent,
            reason,
            // result,
            time_stamp,
        }
    }

    fn conduct(self, pfs: &ParamsForStep) -> (TestReason, TestResult) {
        (
            self.reason,
            self.agent.write().get_test(self.time_stamp, pfs),
        )
    }

    fn cancel(self) {
        self.agent.write().cancel_test();
    }
}

pub struct TestQueue(VecDeque<Testee>);

impl TestQueue {
    pub fn new() -> Self {
        Self(VecDeque::new())
    }

    // enqueue a new test
    pub fn push(&mut self, testee: Testee) {
        self.0.push_back(testee);
    }
    // enqueue new tests
    pub fn extend(&mut self, testees: Vec<Testee>) {
        self.0.extend(testees);
    }

    // check the results of tests
    pub fn accept(
        &mut self,
        pfs: &ParamsForStep,
        count_reason: &mut EnumMap<TestReason, u64>,
        count_result: &mut EnumMap<TestResult, u64>,
    ) {
        let (latest, oldest) = {
            let l = pfs.rp.step as f64 - (pfs.rp.tst_proc * pfs.wp.steps_per_day());
            let o = l - pfs.rp.tst_dly_lim * pfs.wp.steps_per_day();
            (l as u64, o as u64)
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
            if t.time_stamp > oldest && t.agent.read().is_in_field() {
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
