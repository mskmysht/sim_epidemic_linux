use std::collections::HashMap;

use poem_openapi::{types::Example, Enum, Object};

use super::task::Task;

#[derive(Object, Clone, Debug)]
#[oai(rename_all = "camelCase")]
pub struct WorldParams {
    #[oai(validator(minimum(value = "1", exclusive = false)))]
    population_size: u64,
}

#[derive(Object, Clone, Debug)]
#[oai(rename_all = "camelCase")]
pub struct JobParam {
    #[oai(validator(minimum(value = "1", exclusive = false)))]
    stop_at: u64,
    world_params: WorldParams,
    // scenario: Scenario,
    // vaccines
    // variants
    // gatherings
}

#[derive(Object, Clone, Debug)]
#[oai(example, rename_all = "camelCase")]
pub struct Config {
    param: JobParam,
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
                    population_size: 100,
                },
            },
            iteration_count: 3,
            output_fields: Vec::new(),
        }
    }
}

#[derive(Enum, Clone, Debug, Default)]
pub enum JobState {
    #[default]
    Created,
    Queued,
    Scheduled,
    Running,
    Failed,
    Succeeded,
}

#[derive(Object, Clone, Debug)]
#[oai(rename_all = "camelCase")]
pub struct Job {
    /// A system generated unique ID (in ULID format) for the Job.
    pub id: String,
    /// Job configuration.
    pub config: Config,
    /// Job state.
    pub state: JobState,
    // Tasks in the Job.
    pub tasks: HashMap<String, Task>,
}

impl Job {
    pub fn new(id: String, config: Config, state: JobState, tasks: HashMap<String, Task>) -> Self {
        Self {
            id,
            config,
            state,
            tasks,
        }
    }
}
