use std::{
    fmt, io,
    sync::mpsc,
    thread::{self, JoinHandle},
};

use world_core::{
    scenario::Scenario,
    util,
    world::{
        commons::{RuntimeParams, WorldParams},
        World,
    },
};

use chrono::serde::ts_seconds;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum Request {
    Delete,
    Start(u32),
    Step,
    Stop,
    Reset,
    #[cfg(debug_assertions)]
    Debug,
    Export(String),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum Response {
    Ok(ResponseOk),
    Err(serde_error::Error),
}

impl Response {
    #[inline]
    pub fn as_result(self) -> Result<ResponseOk, serde_error::Error> {
        match self {
            Response::Ok(r) => Ok(r),
            Response::Err(e) => Err(e),
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum ResponseOk {
    Success,
    SuccessWithMessage(String),
}

impl From<ResponseOk> for Response {
    fn from(r: ResponseOk) -> Self {
        Response::Ok(r)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResponseError {
    #[error("world is already finished")]
    AlreadyEnded,
    #[error("world is already stopped")]
    AlreadyStopped,
    #[error("world is already running")]
    AlreadyStarted,
    #[error("failed to export file")]
    FileExportFailed,
}

impl From<ResponseError> for serde_error::Error {
    fn from(e: ResponseError) -> Self {
        serde_error::Error::new(&e)
    }
}

impl From<ResponseError> for Response {
    fn from(e: ResponseError) -> Self {
        Response::Err(e.into())
    }
}

pub struct MpscPublisher {
    stream_tx: mpsc::Sender<WorldStatus>,
    req_rx: mpsc::Receiver<Request>,
    res_tx: mpsc::Sender<Response>,
}

impl MpscPublisher {
    pub fn new(
        stream_tx: mpsc::Sender<WorldStatus>,
        req_rx: mpsc::Receiver<Request>,
        res_tx: mpsc::Sender<Response>,
    ) -> Self {
        Self {
            stream_tx,
            req_rx,
            res_tx,
        }
    }

    pub fn recv(&self) -> Result<Request, mpsc::RecvError> {
        self.req_rx.recv()
    }

    pub fn try_recv(&self) -> Result<Option<Request>, mpsc::RecvError> {
        match self.req_rx.try_recv() {
            Ok(r) => Ok(Some(r)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(mpsc::RecvError),
        }
    }

    pub fn send_response(&self, data: Response) -> Result<(), mpsc::SendError<Response>> {
        self.res_tx.send(data)
    }

    pub fn send_on_stream(&self, data: WorldStatus) -> Result<(), mpsc::SendError<WorldStatus>> {
        self.stream_tx.send(data)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RequestError<R, S> {
    #[error("receive error")]
    RecvError(R),
    #[error("send error")]
    SendError(S),
}

pub struct MpscSubscriber {
    req_tx: mpsc::Sender<Request>,
    res_rx: mpsc::Receiver<Response>,
    stream_rx: mpsc::Receiver<WorldStatus>,
}

impl MpscSubscriber {
    pub fn new(
        req_tx: mpsc::Sender<Request>,
        res_rx: mpsc::Receiver<Response>,
        stream_rx: mpsc::Receiver<WorldStatus>,
    ) -> Self {
        Self {
            req_tx,
            res_rx,
            stream_rx,
        }
    }

    pub fn recv_status(&self) -> Result<WorldStatus, mpsc::RecvError> {
        self.stream_rx.recv()
    }

    pub fn try_recv_status(&self) -> Result<Option<WorldStatus>, mpsc::RecvError> {
        match self.stream_rx.try_recv() {
            Ok(s) => Ok(Some(s)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(mpsc::RecvError),
        }
    }

    pub fn send(&self, req: Request) -> Result<(), mpsc::SendError<Request>> {
        self.req_tx.send(req)
    }

    pub fn recv(&self) -> Result<Response, mpsc::RecvError> {
        self.res_rx.recv()
    }

    pub fn seek_status(&self) -> Vec<WorldStatus> {
        let mut v = Vec::new();
        while let Ok(Some(s)) = self.try_recv_status() {
            v.push(s);
        }
        v
    }

    pub fn request(
        &self,
        req: Request,
    ) -> Result<Response, RequestError<mpsc::RecvError, mpsc::SendError<Request>>> {
        self.send(req).map_err(RequestError::SendError)?;
        self.recv().map_err(RequestError::RecvError)
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub enum WorldState {
    Stopped,
    Started,
    Ended,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct WorldStatus {
    step: u32,
    state: WorldState,
    custom: String,
    #[serde(with = "ts_seconds")]
    time_stamp: chrono::DateTime<chrono::Utc>,
}

impl WorldStatus {
    pub fn new(step: u32, state: WorldState, custom: String) -> Self {
        Self {
            step,
            state,
            custom,
            time_stamp: chrono::Utc::now(),
        }
    }
}

impl fmt::Display for WorldStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}]step:{},mode:{:?}",
            self.time_stamp, self.step, self.state
        )
    }
}

#[derive(Default, Debug)]
struct WorldStepInfo {
    prev_time: f64,
    steps_per_sec: f64,
}

pub struct WorldSpawner {
    world: World,
    info: WorldStepInfo,
    publisher: MpscPublisher,
}

impl WorldSpawner {
    pub fn new(
        id: String,
        publisher: MpscPublisher,
        runtime_params: RuntimeParams,
        world_params: WorldParams,
    ) -> Self {
        let world = World::new(id, runtime_params, world_params, Scenario::default());
        let spawner = Self {
            world,
            info: WorldStepInfo::default(),
            publisher,
        };
        spawner.send_status(WorldState::Stopped);
        spawner
    }

    pub fn spawn(self) -> io::Result<JoinHandle<()>> {
        thread::Builder::new()
            .name(format!("world_{}", self.world.id.clone()))
            .spawn(move || self.listen())
    }

    #[inline]
    fn res_ok(&self) {
        self.publisher
            .send_response(ResponseOk::Success.into())
            .unwrap();
    }

    #[inline]
    fn res_ok_with(&self, msg: String) {
        self.publisher
            .send_response(ResponseOk::SuccessWithMessage(msg).into())
            .unwrap();
    }

    #[inline]
    fn res_err(&self, err: ResponseError) {
        self.publisher.send_response(err.into()).unwrap();
    }

    #[inline]
    fn send_status(&self, state: WorldState) {
        self.publisher
            .send_on_stream(WorldStatus::new(
                self.world.runtime_params.step,
                state,
                format!("{:?}", self.world.health_count),
            ))
            .unwrap();
    }

    #[inline]
    fn reset(&mut self) {
        self.world.reset();
        self.info = WorldStepInfo::default();
        self.send_status(WorldState::Stopped);
        self.res_ok();
    }

    #[inline]
    fn step(&mut self) {
        if self.world.is_ended() {
            self.res_err(ResponseError::AlreadyEnded);
        } else {
            self.inline_step();
            let state = if self.world.is_ended() {
                WorldState::Ended
            } else {
                WorldState::Stopped
            };

            self.send_status(state);
            self.res_ok();
        }
    }

    #[inline]
    fn stop(&mut self) {
        self.send_status(WorldState::Stopped);
        self.res_ok();
    }

    #[inline]
    fn debug(&self) {
        self.res_ok_with(format!("{:?}\n{:?}", self.world.health_count, self.info));
    }

    #[inline]
    fn export(&mut self, dir: String) {
        match self.world.export(&dir) {
            Ok(_) => self.res_ok_with(format!("{} was successfully exported", dir)),
            Err(_) => self.res_err(ResponseError::FileExportFailed),
        }
    }

    fn start(&mut self, stop_at: u32) -> bool {
        if self.world.is_ended() {
            self.res_err(ResponseError::AlreadyEnded);
            return false;
        }

        let step_to_end = stop_at * self.world.world_params.steps_per_day;
        self.res_ok();
        while self.step_cont(step_to_end) {
            if let Some(msg) = self.publisher.try_recv().unwrap() {
                match msg {
                    Request::Delete => {
                        self.res_ok();
                        return true;
                    }
                    Request::Stop => {
                        self.stop();
                        break;
                    }
                    Request::Reset => {
                        self.reset();
                        break;
                    }
                    #[cfg(debug_assertions)]
                    Request::Debug => self.debug(),
                    _ => self.res_err(ResponseError::AlreadyStarted),
                }
            }
        }
        false
    }

    #[inline]
    fn step_cont(&mut self, step_to_end: u32) -> bool {
        self.inline_step();
        let (state, cont) = if self.world.is_ended() {
            (WorldState::Ended, false)
        } else if self.world.runtime_params.step > step_to_end {
            (WorldState::Stopped, false)
        } else {
            (WorldState::Started, true)
        };
        self.send_status(state);
        cont
    }

    #[inline]
    fn inline_step(&mut self) {
        self.world.step();
        let new_time = util::get_uptime();
        let time_passed = new_time - self.info.prev_time;
        if time_passed < 1.0 {
            self.info.steps_per_sec +=
                ((1.0 / time_passed).min(30.0) - self.info.steps_per_sec) * 0.2;
        }
        self.info.prev_time = new_time;
    }

    fn listen(mut self) {
        while let Ok(req) = self.publisher.recv() {
            match req {
                Request::Delete => {
                    self.res_ok();
                    break;
                }
                Request::Reset => self.reset(),
                Request::Step => self.step(),
                Request::Start(stop_at) => {
                    if self.start(stop_at) {
                        break;
                    }
                }
                #[cfg(debug_assertions)]
                Request::Debug => self.debug(),
                Request::Export(dir) => self.export(dir),
                Request::Stop => self.res_err(ResponseError::AlreadyStopped),
            }
        }
        println!("<{}> stopped", self.world.id);
    }
}
