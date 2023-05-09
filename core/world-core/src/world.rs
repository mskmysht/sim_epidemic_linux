mod agent;
pub mod commons;
mod contact;
pub(super) mod testing;

use enum_map::{enum_map, Enum, EnumMap};
use rand::seq::SliceRandom;
use std::path::Path;

use self::{
    agent::{
        cemetery::Cemetery, field::Field, gathering::Gatherings, hospital::Hospital, warp::Warps,
        Agent,
    },
    commons::{
        HealthType, ParamsForStep, RuntimeParams, VaccineInfo, VaccinePriority, VariantInfo,
        WorldParams,
    },
    testing::TestQueue,
};
use crate::{
    scenario::Scenario,
    stat::{HealthCount, Stat},
};
use math::Point;

pub struct World {
    pub id: String,
    pub runtime_params: RuntimeParams,
    pub world_params: WorldParams,
    agents: Vec<Agent>,
    agent_origins: Vec<Point>,
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
    vaccine_queue: EnumMap<VaccinePriority, Vec<usize>>,
}

impl World {
    pub fn new(
        id: String,
        runtime_params: RuntimeParams,
        world_params: WorldParams,
        scenario: Scenario,
    ) -> World {
        let n_pop = world_params.init_n_pop as usize;
        let mut w = World {
            id,
            runtime_params,
            scenario,
            agents: Vec::with_capacity(n_pop),
            field: Field::new(world_params.mesh),
            world_params,
            warps: Warps::new(n_pop),
            hospital: Hospital::new(n_pop),
            cemetery: Cemetery::new(n_pop),
            agent_origins: Vec::with_capacity(n_pop),
            gat_spots_fixed: Vec::new(),
            health_count: Default::default(),
            stat: Stat::default(),
            scenario_index: 0,
            gatherings: Gatherings::new(),
            variant_info: VariantInfo::default_list(),
            vaccine_info: VaccineInfo::default_list(),
            test_queue: TestQueue::new(),
            vaccine_queue: enum_map!(VaccinePriority { _ => vec![0; n_pop],}),
        };

        w.reset();
        w
    }

    pub fn reset(&mut self) {
        //[todo] set runtime params of scenario != None
        self.field.clear(&mut self.agents);
        self.hospital.clear(&mut self.agents);
        self.cemetery.clear(&mut self.agents);
        self.warps.clear(&mut self.agents);
        self.agent_origins.clear();

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
            for _ in 0..(cur_len - n_pop) {
                self.agents.pop();
            }
        } else {
            for _ in 0..(n_pop - cur_len) {
                self.agents.push(Agent::new())
            }
        }

        let n_symptomatic = agent::allocation::allocate_agents(
            &mut self.agents,
            &mut self.field,
            &mut self.hospital,
            &mut self.agent_origins,
            n_pop,
            n_infected,
            n_recovered,
            n_dist,
            &self.world_params,
            &self.runtime_params,
        );

        // reset test queue
        self.runtime_params.step = 0;
        self.health_count[&HealthType::Susceptible] = (n_pop - n_infected) as u32;
        self.health_count[&HealthType::Symptomatic] = n_symptomatic as u32;
        self.health_count[&HealthType::Asymptomatic] = (n_infected - n_symptomatic) as u32;
        self.stat.reset();
        self.scenario_index = 0;
        //[todo] self.exec_scenario();

        self.gatherings.clear();

        // reset vaccine queue
        let q = {
            let mut q: Vec<usize> = (0..n_pop).collect();
            q.shuffle(&mut rand::thread_rng());
            q
        };
        for key in &VaccinePriority::ALL {
            if matches!(key, VaccinePriority::Random | VaccinePriority::Booster) {
                for (idx, i) in self.vaccine_queue[key].iter_mut().enumerate() {
                    *i = q[idx];
                }
            } else {
                for (idx, i) in self.vaccine_queue[key].iter_mut().enumerate() {
                    *i = idx
                }
            }
        }
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
                &mut self.field,
                &self.gat_spots_fixed,
                &self.agent_origins,
                &pfs,
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
