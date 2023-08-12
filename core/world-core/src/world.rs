mod agent;
pub mod commons;
mod contact;
pub(super) mod testing;

use enum_map::{enum_map, EnumMap};
use rand::{seq::SliceRandom, Rng};
use std::path::Path;

use self::{
    agent::{
        cemetery::Cemetery, field::Field, gathering::Gatherings, hospital::Hospital, warp::Warps,
        Agent, AgentRef,
    },
    commons::{
        FiniteTypePool, HealthType, ParamsForStep, RuntimeParams, VaccinePriority, WorldParams,
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
    agent_origins: Vec<Point>,
    agents: Vec<Agent>,
    field: Field,
    warps: Warps,
    hospital: Hospital,
    cemetery: Cemetery,
    test_queue: TestQueue,
    //[todo] predicate_to_stop: bool,
    pub health_count: HealthCount,
    stat: Stat,
    scenario: Scenario,
    gatherings: Gatherings,
    gat_spots_fixed: Vec<Point>,
    //[todo] n_mesh: usize,
    //[todo] n_pop: usize,
    // variant_info: Vec<VariantInfo>,
    vaccine_queue: EnumMap<VaccinePriority, Vec<AgentRef>>,
    vaccine_queue_idx: EnumMap<VaccinePriority, usize>,
}

impl World {
    pub fn new(
        id: String,
        runtime_params: RuntimeParams,
        world_params: WorldParams,
        scenario: Scenario,
    ) -> Self {
        let n_pop = world_params.init_n_pop as usize;
        let mut w = Self {
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
            gatherings: Gatherings::new(),
            test_queue: TestQueue::new(),
            vaccine_queue: enum_map!(VaccinePriority { _ => Vec::new(),}),
            vaccine_queue_idx: enum_map!(VaccinePriority { _ => 0,}),
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
        self.scenario.reset();
        self.gatherings.clear();

        // reset vaccine queue
        let q = {
            let mut q: Vec<usize> = (0..n_pop).collect();
            q.shuffle(&mut rand::thread_rng());
            q
        };
        for (key, queue) in &mut self.vaccine_queue {
            queue.clear();
            match key {
                VaccinePriority::Random | VaccinePriority::Booster => {
                    for idx in 0..n_pop {
                        queue.push((&self.agents[q[idx]]).into());
                    }
                }
                VaccinePriority::Central => {
                    let cx = self.world_params.field_size() / 2.0;
                    let mut q = match self.world_params.wrk_plc_mode {
                        None => (0..n_pop)
                            .map(|i| {
                                let p = self.agents[i].get_pt();
                                (i, (p.x + cx).hypot(p.y + cx))
                            })
                            .collect::<Vec<_>>(),
                        Some(_) => (0..n_pop)
                            .map(|i| {
                                let p = &self.agent_origins[i];
                                (i, (p.x + cx).hypot(p.y + cx))
                            })
                            .collect::<Vec<_>>(),
                    };
                    q.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                    for (idx, _) in q {
                        queue.push((&self.agents[idx]).into());
                    }
                }
                _ => {
                    for idx in 0..n_pop {
                        queue.push((&self.agents[idx]).into());
                    }
                }
            }
        }

        for idx in self.vaccine_queue_idx.values_mut() {
            *idx = 0;
        }
    }

    pub fn step(&mut self) {
        let pfs = ParamsForStep::new(&self.world_params, &self.runtime_params);

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

        // distribute vaccines
        let mut vcn_subj_rem = vec![0.0];
        // let mut trc_vcn_set = Vec::new();
        let n_pop = pfs.wp.init_n_pop as usize;
        for (&index, vp) in &pfs.rp.vx_stg {
            if vp.perform_rate.r() <= 0.0 {
                continue;
            }
            let v = &mut vcn_subj_rem[index];
            let f = pfs.wp.init_n_pop() * vp.perform_rate.r() * pfs.wp.days_per_step() + *v;
            let mut cnt = f.floor() as usize;
            *v = f.fract();
            if cnt == 0 {
                continue;
            }

            // tracing vaccination targets
            let idx = &mut self.vaccine_queue_idx[&vp.priority];
            let queue = &self.vaccine_queue[&vp.priority];
            let vaccine = pfs.rp.vaccine_pool.get(index);

            if matches!(vp.priority, VaccinePriority::Random) || vp.regularity.r() >= 1.0 {
                let (ql, qr) = queue.split_at(*idx);
                let q_iter = qr.into_iter().chain(ql.into_iter());
                for a in q_iter {
                    if a.try_give_vaccine_ticket(vaccine.clone()) {
                        cnt -= 1;
                    }
                    *idx += 1;
                    if cnt == 0 {
                        break;
                    }
                }
            } else {
                let mut rng = rand::thread_rng();
                for _ in 0..n_pop {
                    let d = if rng.gen::<f64>() > vp.regularity.r() {
                        rng.gen_range(0..(n_pop / 2)) + 1
                    } else {
                        0
                    };
                    let j = (*idx + d) % n_pop;
                    if queue[j].try_give_vaccine_ticket(vaccine.clone()) {
                        cnt -= 1;
                    }
                    *idx += 1;
                    if cnt == 0 {
                        break;
                    }
                }
            }
            *idx = *idx % n_pop;
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
        self.scenario.exec(&mut self.runtime_params);
        self.runtime_params.step += 1;
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
