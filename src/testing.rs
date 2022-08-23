use crate::agent::{Agent, ParamsForStep};
use crate::enum_map::{Enum, EnumMap};
use rand::Rng;
use std::collections::VecDeque;

#[derive(Eq, PartialEq, Clone, Enum, Debug)]
pub enum TestReason {
    AsSymptom,
    AsContact,
    AsSuspected,
    // TestPositiveRate,
    // NAllTestTypes,
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
    result: TestResult,
    time_stamp: u64,
}

impl Testee {
    pub fn new(agent: Agent, reason: TestReason, pfs: &ParamsForStep) -> Self {
        let rng = &mut rand::thread_rng();
        let p = if let Some(ip) = agent.lock().unwrap().is_infected() {
            rng.gen::<f64>()
                < 1.0
                    - (1.0 - pfs.rp.tst_sens.r()).powf(pfs.vr_info[ip.virus_variant].reproductivity)
        } else {
            rng.gen::<f64>() > pfs.rp.tst_spec.r()
        };
        Self {
            agent,
            reason,
            result: p.into(),
            time_stamp: pfs.rp.step,
        }
    }

    fn reserve_quarantine(&mut self) {
        self.agent.lock().unwrap().reserve_quarantine();
    }
}

impl Drop for Testee {
    fn drop(&mut self) {
        self.agent.lock().unwrap().finish_test(self.time_stamp);
    }
}

pub struct TestQueue(VecDeque<Testee>);

impl TestQueue {
    pub fn new() -> Self {
        Self(VecDeque::new())
    }

    // enqueue a new test
    pub fn push(&mut self, agent: Agent, reason: TestReason, pfs: &ParamsForStep) {
        self.0.push_back(Testee::new(agent, reason, pfs));
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
            let mut t = self.0.pop_front().unwrap();
            if t.time_stamp > oldest {
                max_tests -= 1;
                self.deliver(&mut t, count_reason, count_result);
            }
        }
    }

    fn deliver(
        &mut self,
        t: &mut Testee,
        count_reason: &mut EnumMap<TestReason, u64>,
        count_result: &mut EnumMap<TestResult, u64>,
    ) {
        if let TestResult::Positive = &t.result {
            t.reserve_quarantine();
        }
        count_result[&t.result] += 1;
        count_reason[&t.reason] += 1;
    }
}
