use ipc_channel::ipc::IpcSender;
use scenario_operation::Operation;
use world_if::{
    api, IpcBiConnection, Request, Response, ResponseError, ResponseOk, WorldState, WorldStatus,
};

use world_core::{
    scenario::Scenario,
    util::{self, random::DistInfo},
    world::{
        commons::{RuntimeParams, WorldParams},
        World,
    },
};

use std::{
    io,
    thread::{self, JoinHandle},
};

#[derive(Default, Debug)]
struct WorldStepInfo {
    prev_time: f64,
    steps_per_sec: f64,
}

pub struct WorldSpawner {
    world: World,
    stop_at: u32,
    info: WorldStepInfo,
    bicon: IpcBiConnection,
    stream: IpcSender<WorldStatus>,
    stat_dir: String,
}

impl WorldSpawner {
    pub fn new(
        id: String,
        bicon: IpcBiConnection,
        stream: IpcSender<WorldStatus>,
        stat_dir: String,
    ) -> anyhow::Result<Self> {
        let param: api::job::JobParam = bicon.recv().unwrap();
        let world = World::new(
            id,
            new_runtime_params(),
            new_world_params(&param.world_params),
            Scenario::new(param.scenario, |s| {
                Operation::new(
                    s.condition.parse().unwrap(),
                    serde_json::from_value(s.assignments).unwrap(),
                )
            }),
        );
        let spawner = Self {
            world,
            stop_at: param.stop_at,
            info: WorldStepInfo::default(),
            bicon,
            stream,
            stat_dir,
        };
        spawner.send_status(WorldState::Stopped)?;
        Ok(spawner)
    }

    pub fn spawn(self) -> io::Result<JoinHandle<anyhow::Result<()>>> {
        thread::Builder::new()
            .name(format!("world_{}", self.world.id.clone()))
            .spawn(move || self.listen())
    }

    fn listen(mut self) -> anyhow::Result<()> {
        loop {
            match self.bicon.recv()? {
                Request::Execute => {
                    self.execute()?;
                    break;
                }
                Request::Terminate => self.res_err(ResponseError::AlreadyStopped)?,
            }
        }
        tracing::info!("stopped {}", self.world.id);
        Ok(())
    }

    #[inline]
    fn res_ok(&self) -> anyhow::Result<()> {
        self.bicon.send(&Response::from(ResponseOk::Success))
    }

    #[inline]
    fn send_status(&self, state: WorldState) -> anyhow::Result<()> {
        self.stream.send(WorldStatus::new(
            self.world.runtime_params.step,
            state,
            String::new(),
        ))?;
        Ok(())
    }

    #[inline]
    fn stop(&mut self) -> anyhow::Result<()> {
        self.send_status(WorldState::Stopped)?;
        self.res_ok()
    }

    fn execute(&mut self) -> anyhow::Result<()> {
        let step_to_end = self.stop_at * self.world.world_params.steps_per_day;
        self.res_ok()?;
        while self.step(step_to_end)? {
            if let Some(msg) = self.bicon.try_recv()? {
                match msg {
                    Request::Terminate => {
                        self.stop()?;
                        break;
                    }
                    _ => self.res_err(ResponseError::AlreadyStarted)?,
                }
            }
        }
        self.world.export(&self.stat_dir)
    }

    #[inline]
    fn step(&mut self, step_to_end: u32) -> anyhow::Result<bool> {
        self.world.step();
        let new_time = util::get_uptime();
        let time_passed = new_time - self.info.prev_time;
        if time_passed < 1.0 {
            self.info.steps_per_sec +=
                ((1.0 / time_passed).min(30.0) - self.info.steps_per_sec) * 0.2;
        }
        self.info.prev_time = new_time;

        let (state, cont) = if self.world.is_ended() {
            (WorldState::Ended, false)
        } else if self.world.runtime_params.step > step_to_end {
            (WorldState::Stopped, false)
        } else {
            (WorldState::Started, true)
        };
        self.send_status_with(state, format!("{:?}", self.world.health_count))?;
        Ok(cont)
    }

    #[inline]
    fn res_err(&self, err: ResponseError) -> anyhow::Result<()> {
        self.bicon.send(&Response::from(err))
    }

    #[inline]
    fn send_status_with(&self, state: WorldState, custom: String) -> anyhow::Result<()> {
        self.stream.send(WorldStatus::new(
            self.world.runtime_params.step,
            state,
            custom,
        ))?;
        Ok(())
    }
}

fn new_world_params(param: &api::job::WorldParams) -> WorldParams {
    WorldParams::new(
        param.population_size,
        360,
        18,
        16,
        param.infected.into(),
        0.0.into(),
        20.0.into(),
        50.0.into(),
        None,
        150.0.into(),
        50.0,
        500.0.into(),
        40.0.into(),
        30.0.into(),
        90.0.into(),
        95.0.into(),
        14.0,
        7.0,
        120.0,
        90.0.into(),
    )
}

fn new_runtime_params() -> RuntimeParams {
    RuntimeParams {
        mass: 50.0.into(),
        friction: 80.0.into(),
        avoidance: 50.0.into(),
        max_speed: 50.0,
        act_mode: 50.0.into(),
        act_kurt: 0.0.into(),
        mob_act: 50.0.into(),
        gat_act: 50.0.into(),
        incub_act: 0.0.into(),
        fatal_act: 0.0.into(),
        infec: 50.0.into(),
        infec_dst: 3.0,
        contag_delay: 0.5,
        contag_peak: 3.0,
        incub: DistInfo::new(1.0, 5.0, 14.0),
        fatal: DistInfo::new(4.0, 16.0, 20.0),
        therapy_effc: 0.0.into(),
        imn_max_dur: 200.0,
        imn_max_dur_sv: 50.0.into(),
        imn_max_effc: 90.0.into(),
        imn_max_effc_sv: 20.0.into(),
        dst_st: 50.0,
        dst_ob: 20.0.into(),
        mob_freq: DistInfo::new(40.0.into(), 70.0.into(), 100.0.into()),
        mob_dist: DistInfo::new(10.0.into(), 30.0.into(), 80.0.into()),
        back_hm_rt: 75.0.into(),
        gat_fr: 50.0,
        gat_rnd_rt: 50.0.into(),
        gat_sz: DistInfo::new(5.0, 10.0, 20.0),
        gat_dr: DistInfo::new(6.0, 12.0, 24.0),
        gat_st: DistInfo::new(50.0, 80.0, 100.0),
        gat_freq: DistInfo::new(40.0.into(), 70.0.into(), 100.0.into()),
        cntct_trc: 20.0.into(),
        tst_delay: 1.0,
        tst_proc: 1.0,
        tst_interval: 2.0,
        tst_sens: 70.0.into(),
        tst_spec: 99.8.into(),
        tst_sbj_asy: 1.0.into(),
        tst_sbj_sym: 99.0.into(),
        tst_capa: 50.0.into(),
        tst_dly_lim: 3.0,
        ..Default::default()
    }
}
