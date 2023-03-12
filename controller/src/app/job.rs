use api::job::{JobParam, WorldParams};
use poem_openapi::{types::Example, Enum, Object};
use tokio_postgres::types::{FromSql, ToSql};

use super::task::Task;

#[derive(Object, Clone, Debug)]
#[oai(example, rename_all = "camelCase")]
pub struct Config {
    pub param: JobParam,
    #[oai(validator(minimum(value = "1", exclusive = false)))]
    pub iteration_count: u64,
    output_fields: Vec<String>,
    // load_state: Option<String>,
}

impl Example for Config {
    fn example() -> Self {
        Config {
            param: JobParam {
                stop_at: 10,
                world_params: WorldParams {
                    population_size: 1000,
                    infected: 1.0,
                },
            },
            iteration_count: 3,
            output_fields: Vec::new(),
        }
    }
}

#[derive(Enum, Clone, Debug, Default, ToSql, FromSql)]
pub enum JobState {
    #[default]
    Created,
    Queued,
    Scheduled,
    Running,
    Completed,
}

/// Job
#[derive(Object, Clone, Debug)]
#[oai(rename_all = "camelCase")]
pub struct Job {
    /// Job ID automatically generated in ULID format.
    pub id: String,
    /// Job configuration.
    pub config: Config,
    /// Job state.
    pub state: JobState,
    // Tasks in the Job.
    pub tasks: Vec<Task>,
}

impl Job {
    pub fn new(id: String, config: Config, state: JobState, tasks: Vec<Task>) -> Self {
        Self {
            id,
            config,
            state,
            tasks,
        }
    }
}
