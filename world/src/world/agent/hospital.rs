use super::{warp::Warps, Agent, AgentCore, Location, LocationLabel, ParamsForStep, WarpParam};
use crate::{log::HealthDiff, log::StepLog, stat::HistInfo, util::DrainMap};

pub struct HospitalStepInfo {
    hist: Option<HistInfo>,
    health: Option<HealthDiff>,
}

pub struct HospitalAgent {
    agent: Agent,
}

impl LocationLabel for HospitalAgent {
    const LABEL: Location = Location::Hospital;
}

impl HospitalAgent {
    fn new(agent: Agent) -> Self {
        Self {
            agent: Self::label(agent),
        }
    }

    fn step(&mut self, pfs: &ParamsForStep) -> (HospitalStepInfo, Option<WarpParam>) {
        fn local_step(
            agent: &mut AgentCore,
            hist: &mut Option<HistInfo>,
            pfs: &ParamsForStep,
        ) -> Option<WarpParam> {
            agent.hospital_step(hist, pfs)
        }
        let agent = &mut self.agent.0.lock().unwrap();
        let mut hist = None;
        let warp = local_step(agent, &mut hist, pfs);
        (
            HospitalStepInfo {
                hist,
                health: agent.update_health(),
            },
            warp,
        )
    }
}

pub struct Hospital(Vec<HospitalAgent>);

impl Hospital {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn add(&mut self, agent: Agent) {
        self.0.push(HospitalAgent::new(agent));
    }

    pub fn steps(&mut self, warps: &mut Warps, step_log: &mut StepLog, pfs: &ParamsForStep) {
        let tmp = self.0.drain_map_mut(|ha| ha.step(pfs));

        for (hsi, opt) in tmp.into_iter() {
            if let Some(h) = hsi.hist {
                step_log.hists.push(h);
            }
            if let Some(h) = hsi.health {
                step_log.apply_difference(h);
            }
            if let Some((param, ha)) = opt {
                warps.add(ha.agent.clone(), param);
            }
        }
    }
}
