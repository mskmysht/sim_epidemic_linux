use super::{warp::Warps, Agent, Location, LocationLabel, ParamsForStep, WarpParam};
use crate::{
    stat::{HealthCount, HealthDiff, HistInfo, Stat},
    util::DrainMap,
};

use math::Point;

#[derive(Default)]
pub struct HospitalStepInfo {
    pub hist_info: Option<HistInfo>,
    pub health_diff: Option<HealthDiff>,
}
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

    fn step(&mut self, pfs: &ParamsForStep) -> (HospitalStepInfo, Option<WarpParam>) {
        let agent = &mut self.agent.write();
        let mut hsi = HospitalStepInfo::default();
        let warp = agent.hospital_step(self.back_to, &mut hsi.hist_info, &mut hsi.health_diff, pfs);

        (hsi, warp)
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

    pub fn step(
        &mut self,
        warps: &mut Warps,
        stat: &mut Stat,
        health_count: &mut HealthCount,
        pfs: &ParamsForStep,
    ) {
        let tmp = self.0.drain_map_mut(|ha| ha.step(pfs));

        for (hsi, opt) in tmp.into_iter() {
            if let Some(hist) = hsi.hist_info {
                stat.hists.push(hist);
            }
            if let Some(hd) = hsi.health_diff {
                health_count.apply_difference(hd);
            }
            if let Some((param, ha)) = opt {
                warps.add(ha.agent.clone(), param);
            }
        }
    }
}
