use super::ApiTags;

use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;
use poem::{web::Data, Result};
use poem_openapi::{
    param::Path,
    payload::{Json, PlainText},
    ApiResponse, Enum, Object, OpenApi,
};
use ulid::Ulid;

#[derive(Object, Clone)]
#[oai(rename_all = "camelCase")]
struct WorldParams {
    population_size: u64,
}

#[derive(Object, Clone)]
#[oai(rename_all = "camelCase")]
struct JobConfig {
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
enum Status {
    Pending,
    Assigned,
    Running,
    Failed,
    Succeeded,
}

#[derive(Object, Clone)]
#[oai(rename_all = "camelCase")]
struct Job {
    /// The id of this job.
    id: String,
    /// The config of this job.
    config: JobConfig,
    /// The status of this job.
    status: Status,
}

#[derive(Default)]
pub struct Jobs {
    table: HashMap<String, Job>,
}

pub type MyDb = Arc<RwLock<Jobs>>;

#[derive(ApiResponse)]
enum GetResponse {
    #[oai(status = 200)]
    Job(Json<Job>),
    #[oai(status = 404)]
    NotFound(PlainText<String>),
}

pub struct Api;

#[OpenApi(tag = "ApiTags::Job")]
impl Api {
    #[oai(path = "/jobs", method = "post")]
    async fn create(&self, pool: Data<&MyDb>, config: Json<JobConfig>) -> Result<Json<String>> {
        let mut db = pool.0.write();
        let id = Ulid::new().to_string();
        db.table.insert(
            id.clone(),
            Job {
                id: id.clone(),
                config: config.0,
                status: Status::Pending,
            },
        );
        Ok(Json(id))
    }

    #[oai(path = "/jobs/:id", method = "get")]
    async fn get(&self, pool: Data<&MyDb>, id: Path<String>) -> Result<GetResponse> {
        let db = pool.0.read();
        match db.table.get(&id.0) {
            Some(job) => Ok(GetResponse::Job(Json(job.clone()))),
            None => Ok(GetResponse::NotFound(PlainText(format!(
                "Job {} is not found.",
                id.0
            )))),
        }
    }

    #[oai(path = "/jobs", method = "get")]
    async fn get_all(&self, pool: Data<&MyDb>) -> Result<Json<Vec<Job>>> {
        let db = pool.0.read();
        Ok(Json(db.table.values().cloned().collect()))
    }
}
