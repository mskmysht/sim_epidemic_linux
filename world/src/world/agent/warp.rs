use super::{
    super::{
        commons::ParamsForStep,
        testing::{TestQueue, Testee},
    },
    cemetery::Cemetery,
    field::Field,
    hospital::Hospital,
    Agent, Location, LocationLabel, WarpMode, WarpParam,
};
use crate::util::DrainWith;

#[derive(Default)]
struct WarpStepInfo {
    contacted_testees: Option<Vec<Testee>>,
}

pub struct WarpAgent {
    agent: Agent,
    param: WarpParam,
}

impl LocationLabel for WarpAgent {
    const LABEL: Location = Location::Warp;
}

impl WarpAgent {
    fn new(agent: Agent, param: WarpParam) -> Self {
        Self {
            agent: Self::label(agent),
            param,
        }
    }

    fn step(&mut self, pfs: &ParamsForStep) -> (WarpStepInfo, bool) {
        let mut agent = self.agent.write();
        let mut wsi = WarpStepInfo::default();
        if let WarpMode::Inside = self.param.mode {
            if let Some(w) = agent.check_quarantine(&mut wsi.contacted_testees, pfs) {
                self.param = w;
            }
        }
        let at_goal = agent.body.warp_update(self.param.goal, pfs.wp);

        (wsi, at_goal)
    }
}

pub struct Warps(Vec<WarpAgent>);

impl Warps {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn add(&mut self, agent: Agent, param: WarpParam) {
        self.0.push(WarpAgent::new(agent, param));
    }

    pub fn step(
        &mut self,
        field: &mut Field,
        hospital: &mut Hospital,
        cemetery: &mut Cemetery,
        test_queue: &mut TestQueue,
        pfs: &ParamsForStep,
    ) {
        let tmp = self.0.drain_with_mut(|a| a.step(pfs));
        for (wsi, opt) in tmp.into_iter() {
            if let Some(testees) = wsi.contacted_testees {
                test_queue.extend(testees);
            }
            if let Some(wa) = opt {
                let WarpAgent {
                    agent,
                    param: WarpParam { mode, goal },
                } = wa;
                match mode {
                    WarpMode::Back => field.add(agent, pfs.wp.into_grid_index(&goal)),
                    WarpMode::Inside => field.add(agent, pfs.wp.into_grid_index(&goal)),
                    WarpMode::Hospital(back_to) => hospital.add(agent, back_to),
                    WarpMode::Cemetery => cemetery.add(agent),
                }
            }
        }
    }
}
