use std::fmt::Display;

use ::world_if::api::job;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Request {
    Execute(String, job::JobParam),
    Terminate(String),
    ReadStatistics(String),
    RemoveStatistics(Vec<String>),
}

impl Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Request::Execute(_, _) => write!(f, "execute"),
            Request::Terminate(_) => write!(f, "terminate"),
            Request::ReadStatistics(_) => write!(f, "read statistics"),
            Request::RemoveStatistics(_) => write!(f, "remove statistics"),
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Response<T>(Result<T, serde_error::Error>);

impl<T> Response<T> {
    pub fn as_result(self) -> Result<T, serde_error::Error> {
        self.0
    }

    pub fn from_ok(value: T) -> Self {
        Self(Ok(value))
    }

    pub fn from_err<E: std::error::Error>(err: E) -> Self {
        Self(Err(serde_error::Error::new(&err)))
    }
}

impl<T, E: std::error::Error> From<Result<T, E>> for Response<T> {
    fn from(value: Result<T, E>) -> Self {
        Self(value.map_err(|e| serde_error::Error::new(&e)))
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Cost(u64);

impl From<&job::JobParam> for Cost {
    fn from(value: &job::JobParam) -> Self {
        (&value.world_params).into()
    }
}

impl From<&job::WorldParams> for Cost {
    fn from(value: &job::WorldParams) -> Self {
        Self((value.population_size as u64).pow(2))
    }
}

impl From<job::WorldParams> for Cost {
    fn from(value: job::WorldParams) -> Self {
        Self((value.population_size as u64).pow(2))
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ResourceMeasure {
    pub max_cost: Cost,
    pub max_resource: u32,
}

#[derive(thiserror::Error, Debug)]
pub enum ResourceSizeError {
    #[error("cost exceeds the maximum resource")]
    ExceedMaxResource,
}

impl ResourceMeasure {
    pub fn new(max_param: job::WorldParams, max_resource: u32) -> Self {
        Self {
            max_cost: max_param.into(),
            max_resource,
        }
    }

    pub fn measure(&self, cost: &Cost) -> Result<u32, ResourceSizeError> {
        if cost.0 > self.max_cost.0 {
            return Err(ResourceSizeError::ExceedMaxResource);
        }

        let k = self.max_resource as u64 * cost.0;
        if k <= self.max_cost.0 {
            Ok(1)
        } else {
            Ok((k / self.max_cost.0) as u32)
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum ResponseOk {
    Item,
    Custom(world_if::ResponseOk),
}

#[derive(Debug, thiserror::Error)]
pub enum ResponseError {
    #[error("failed to spawn item")]
    FailedToSpawn(anyhow::Error),
    #[error("error has occured in the child process: {0}")]
    FailedInProcess(anyhow::Error),
    #[error("abort child process")]
    Abort(anyhow::Error),
    #[error("no id found")]
    NoIdFound,
    #[error("custom error")]
    Custom(#[from] serde_error::Error),
}

impl From<ResponseError> for serde_error::Error {
    fn from(e: ResponseError) -> Self {
        serde_error::Error::new(&e)
    }
}

impl ResponseError {
    pub fn process_any_error(error: anyhow::Error) -> Self {
        Self::FailedInProcess(error)
    }
    pub fn process_std_error<E: std::error::Error + Send + Sync + 'static>(error: E) -> Self {
        Self::FailedInProcess(anyhow::Error::new(error))
    }
}

pub use world_if;
