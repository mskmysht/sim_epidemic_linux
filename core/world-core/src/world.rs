mod agent;
pub mod commons;
mod contact;
pub(super) mod testing;

use enum_map::EnumMap;
use std::path::Path;

use self::{
    agent::{
        cemetery::Cemetery, field::Field, gathering::Gatherings, hospital::Hospital, warp::Warps,
        Agent,
    },
    commons::{HealthType, ParamsForStep, RuntimeParams, VaccineInfo, VariantInfo, WorldParams},
    testing::TestQueue,
};
use crate::{
    scenario::Scenario,
    stat::{HealthCount, Stat},
    util::math::Point,
};

pub struct World {
    pub id: String,
    pub runtime_params: RuntimeParams,
    pub world_params: WorldParams,
    agents: Vec<Agent>,
    field: Field,
    warps: Warps,
    hospital: Hospital,
    cemetery: Cemetery,
    test_queue: TestQueue,
    //[todo] predicate_to_stop: bool,
    pub health_count: HealthCount,
    stat: Stat,
    scenario_index: i32,
    scenario: Scenario,
    gatherings: Gatherings,
    gat_spots_fixed: Vec<Point>,
    //[todo] n_mesh: usize,
    //[todo] n_pop: usize,
    variant_info: Vec<VariantInfo>,
    vaccine_info: Vec<VaccineInfo>,
}

impl World {
    pub fn new(
        id: String,
        runtime_params: RuntimeParams,
        world_params: WorldParams,
        scenario: Scenario,
    ) -> World {
        let mut w = World {
            id,
            runtime_params,
            world_params,
            scenario,
            agents: Vec::with_capacity(world_params.init_n_pop as usize),
            health_count: Default::default(),
            stat: Stat::default(),
            scenario_index: 0,
            gatherings: Gatherings::new(),
            variant_info: VariantInfo::default_list(),
            vaccine_info: VaccineInfo::default_list(),
            field: Field::new(world_params.mesh),
            warps: Warps::new(),
            hospital: Hospital::new(),
            cemetery: Cemetery::new(),
            test_queue: TestQueue::new(),
            gat_spots_fixed: Vec::new(),
        };

        w.reset();
        w
    }

    pub fn reset(&mut self) {
        //[todo] set runtime params of scenario != None
        let n_pop = self.world_params.init_n_pop as usize;
        let n_dist = (self.runtime_params.dst_ob.r() * self.world_params.init_n_pop()) as usize;
        let n_infected = (self.world_params.init_n_pop() * self.world_params.infected.r()) as usize;
        let n_recovered = {
            let k = (self.world_params.init_n_pop() * self.world_params.recovered.r()) as usize;
            if n_infected + k > n_pop {
                n_pop - n_infected
            } else {
                k
            }
        };

        let cur_len = self.agents.len();
        if n_pop < cur_len {
            for i in (n_pop..cur_len).rev() {
                self.agents.swap_remove(i);
            }
        } else {
            for _ in 0..(n_pop - cur_len) {
                self.agents.push(Agent::new())
            }
        }

        self.gatherings.clear();
        self.field.clear();
        self.hospital.clear();
        self.cemetery.clear();
        self.warps.clear();

        let (cats, n_symptomatic) = Agent::reset_all(
            &self.agents,
            n_pop,
            n_infected,
            n_recovered,
            n_dist,
            &self.world_params,
            &self.runtime_params,
        );

        let mut n_q_symptomatic =
            (n_symptomatic as f64 * self.world_params.q_symptomatic.r()) as u32;
        let mut n_q_asymptomatic = ((n_infected as u32 - n_symptomatic) as f64
            * self.world_params.q_asymptomatic.r()) as u32;
        for (i, t) in cats.into_iter().enumerate() {
            let agent = self.agents[i].clone();
            match t {
                HealthType::Symptomatic if n_q_symptomatic > 0 => {
                    n_q_symptomatic -= 1;
                    let back_to = agent.read().get_back_to();
                    self.hospital.add(agent, back_to);
                }
                HealthType::Asymptomatic if n_q_asymptomatic > 0 => {
                    n_q_asymptomatic -= 1;
                    let back_to = agent.read().get_back_to();
                    self.hospital.add(agent, back_to);
                }
                _ => {
                    let idx = self.world_params.into_grid_index(&agent.read().get_pt());
                    self.field.add(agent, idx);
                }
            }
        }

        // reset test queue
        self.runtime_params.step = 0;
        self.health_count[&HealthType::Susceptible] = (n_pop - n_infected) as u32;
        self.health_count[&HealthType::Symptomatic] = n_symptomatic;
        self.health_count[&HealthType::Asymptomatic] = n_infected as u32 - n_symptomatic;
        self.stat.reset();
        self.scenario_index = 0;
        //[todo] self.exec_scenario();
    }

    pub fn step(&mut self) {
        let pfs = ParamsForStep::new(
            &self.world_params,
            &self.runtime_params,
            &self.variant_info,
            &self.vaccine_info,
        );

        let mut count_reason = EnumMap::default();
        let mut count_result = EnumMap::default();
        self.test_queue
            .accept(&pfs, &mut count_reason, &mut count_result);

        if !pfs.go_home_back() {
            self.gatherings.step(
                &self.field,
                &self.gat_spots_fixed,
                &self.agents,
                pfs.wp,
                pfs.rp,
            );
        }

        self.field.step(
            &mut self.warps,
            &mut self.test_queue,
            &mut self.stat,
            &mut self.health_count,
            &pfs,
        );
        self.hospital.step(
            &mut self.warps,
            &mut self.stat,
            &mut self.health_count,
            &pfs,
        );
        self.warps.step(
            &mut self.field,
            &mut self.hospital,
            &mut self.cemetery,
            &mut self.test_queue,
            &pfs,
        );

        self.stat.health_stat.push(self.health_count.clone());
        self.runtime_params.step += 1;
        //[todo] self.predicate_to_stop
        //    if loop_mode == LoopMode::LoopEndByCondition
        //        && world.scenario_index < self.scenario.len() as i32
        //    {
        //        world.exec_scenario();
        //        loop_mode = LoopMode::LoopRunning;
        //    }
    }

    #[inline]
    pub fn is_ended(&self) -> bool {
        self.health_count.n_infected() == 0
    }

    pub fn export(&mut self, dir: &str) -> anyhow::Result<()> {
        let path = Path::new(dir);
        self.stat
            .health_stat
            .export(&path.join(&self.id).with_extension("arrow"))
    }
}

/*
- (void)startTimeLimitTimer {
    runtimeTimer = [NSTimer scheduledTimerWithTimeInterval:maxRuntime repeats:NO
        block:^(NSTimer * _Nonnull timer) { [self stop:LoopEndByTimeLimit]; }];
}
*/
