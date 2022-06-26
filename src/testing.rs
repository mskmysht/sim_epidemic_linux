use rand::Rng;

use crate::agent::Agent;
use crate::agent::ParamsForStep;
use crate::agent::WarpInfo;
use crate::enum_map::Enum;
use crate::enum_map::EnumMap;

#[derive(Eq, PartialEq, Hash, Copy, Clone, Enum, Debug)]
pub enum TestReason {
    AsSymptom,
    AsContact,
    AsSuspected,
    // TestPositiveRate,
    // NAllTestTypes,
}

#[derive(Enum)]
enum TestResult {
    Positive,
    Negative,
}

impl From<bool> for TestResult {
    fn from(infected: bool) -> Self {
        match infected {
            true => Self::Positive,
            false => Self::Negative,
        }
    }
}

pub struct Testee {
    agent: Agent,
    reason: TestReason,
    result: TestResult,
}

impl Testee {
    pub fn new(agent: Agent, reason: TestReason, prms: &ParamsForStep) -> Self {
        let rng = &mut rand::thread_rng();
        let a = agent.lock().unwrap();
        let p = if a.is_infected() {
            rng.gen::<f64>()
                < 1.0
                    - (1.0 - prms.rp.tst_sens.r())
                        .powf(prms.vr_info[a.virus_variant].reproductivity)
        } else {
            rng.gen::<f64>() > prms.rp.tst_spec.r()
        };
        Self {
            agent,
            reason,
            result: p.into(),
        }
    }

    fn reserve_quarantine(&mut self) {
        let agent = self.agent.lock().unwrap();
        agent.reserve_quarantine();
    }
}

impl Drop for Testee {
    fn drop(&mut self) {
        let a = self.agent.lock().unwrap();
        a.finish_test(self.time_stamp);
    }
}

// #[derive(Default)]
struct TestEntry {
    testees: Vec<Testee>,
    time_stamp: u64,
}

impl TestEntry {
    pub fn new(
        testees: Vec<Testee>,
        time_stamp: u64,
        // vi: &[VariantInfo],
    ) -> Self {
        // let a = agent.lock().unwrap();
        // a.in_test_queue = true;
        TestEntry {
            // is_positive,
            // agent,
            testees,
            time_stamp,
        }
    }
}

pub struct TestInfo {
    pub warp: WarpInfo,
    pub testees: Vec<(Agent, TestReason)>,
}

pub struct TestQueue(Vec<TestEntry>);

impl TestQueue {
    // enqueue new tests
    pub fn add(&mut self, testees: Vec<Testee>, step: u64) {
        self.0.push(TestEntry::new(testees, step));
    }

    // check the results of tests
    pub fn accept(
        &mut self,
        prms: &ParamsForStep,
        count_reason: &mut EnumMap<TestReason, u64>,
        count_result: &mut EnumMap<TestResult, u64>,
    ) {
        let (latest, oldest) = {
            let l = prms.rp.step as f64 - (prms.rp.tst_proc * prms.wp.steps_per_day());
            let o = l - prms.rp.tst_dly_lim * prms.wp.steps_per_day();
            (l as u64, o as u64)
        };
        // prms.rp.trc_ope
        let mut max_tests = {
            let rng = &mut rand::thread_rng();
            let m = prms.wp.init_pop as f64 * prms.rp.tst_capa.r() / prms.wp.steps_per_day();
            if m.fract() > rng.gen() {
                m as usize + 1
            } else {
                m as usize
            }
        };

        let dl = self.0.len();
        let k = None;
        for (j, e) in self.0.iter().enumerate() {
            if e.time_stamp > latest {
                dl = j;
                break;
            }
            let tl = e.testees.len();
            if max_tests < tl {
                dl = j;
                k = Some(max_tests);
                break;
            } else {
                max_tests -= tl;
            }
        }
        for t in self.0.drain(..dl).flat_map(|e| e.testees) {
            self.deliver(t, prms, count_reason, count_result);
        }
        if let Some(k) = k {
            for t in self.0[0].testees.drain(..k) {
                self.deliver(t, prms, count_reason, count_result);
            }
        }
    }

    fn deliver(
        &mut self,
        t: Testee,
        prms: &ParamsForStep,
        count_reason: &mut EnumMap<TestReason, u64>,
        count_result: &mut EnumMap<TestResult, u64>,
    ) {
        count_result[t.result] += 1;
        count_reason[t.reason] += 1;
        if let TestResult::Positive = t.result {
            t.reserve_quarantine();
        }
    }
}
