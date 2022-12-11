pub mod task;

use poem_openapi::{Enum, Object};

#[derive(Object, Clone)]
#[oai(rename_all = "camelCase")]
pub struct WorldParams {
    population_size: u64,
}

#[derive(Object, Clone)]
#[oai(rename_all = "camelCase")]
pub struct JobConfig {
    stop_at: u64,
    world_params: WorldParams,
    // scenario: Scenario,
    iteration_count: u64,
    result_fields: Vec<String>,
    // load_state: Option<String>,
    // vaccines
    // variants
    // gatherings
}

#[derive(Enum, Clone)]
pub enum Status {
    Scheduled,
    Running,
    Failed,
    Succeeded,
}

#[derive(Object, Clone)]
#[oai(rename_all = "camelCase")]
pub struct Job {
    /// The id of this job.
    pub id: String,
    /// The config of this job.
    pub config: JobConfig,
    /// The status of this job.
    pub status: Status,
}
