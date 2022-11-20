use super::{warp::Warps, Agent, Location, LocationLabel, ParamsForStep, WarpParam};
use crate::{
    log::{LocalStepLog, MyLog},
    util::{math::Point, DrainMap},
};

pub struct HospitalAgent {
    agent: Agent,
    back_to: Point,
}

impl LocationLabel for HospitalAgent {
    const LABEL: Location = Location::Hospital;
}

impl HospitalAgent {
    fn new(agent: Agent, back_to: Point) -> Self {
        Self {
            agent: Self::label(agent),
            back_to,
        }
    }

    fn step(&mut self, pfs: &ParamsForStep) -> (LocalStepLog, Option<WarpParam>) {
        let agent = &mut self.agent.write();
        let mut log = LocalStepLog::default();
        let warp = agent.hospital_step(self.back_to, &mut log, pfs);

        (log, warp)
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

    pub fn add(&mut self, agent: Agent, back_to: Point) {
        self.0.push(HospitalAgent::new(agent, back_to));
    }

    pub fn step(&mut self, warps: &mut Warps, log: &mut MyLog, pfs: &ParamsForStep) {
        let tmp = self.0.drain_map_mut(|ha| ha.step(pfs));

        for (llog, opt) in tmp.into_iter() {
            log.apply(llog);
            if let Some((param, ha)) = opt {
                warps.add(ha.agent.clone(), param);
            }
        }
    }
}
