pub mod job;
pub mod task;

use async_trait::async_trait;
use poem_openapi::Tags;
use poem_openapi::{
    param::Path,
    payload::{Json, PlainText},
    ApiResponse, OpenApi,
};

#[derive(Tags)]
enum ApiTags {
    /// Operations about job
    Job,
}

#[derive(ApiResponse)]
enum CreateJobResponse {
    #[oai(status = 200)]
    JobId(Json<String>),
    #[oai(status = 409)]
    JobAlreadyExists,
    // #[oai(status = 404)]
    // Failed(PlainText<String>),
}

#[derive(ApiResponse)]
enum GetResponse {
    #[oai(status = 200)]
    Job(Json<job::Job>),
    #[oai(status = 404)]
    NotFound(PlainText<String>),
}

#[derive(ApiResponse)]
enum TerminateResponse {
    #[oai(status = 204)]
    Succeeded,
    #[oai(status = 405)]
    AlreadyTerminated,
    #[oai(status = 404)]
    NotFound(PlainText<String>),
}

#[async_trait]
pub trait ResourceManager {
    async fn create_job(&self, config: job::Config) -> Option<String>;
    async fn get_job(&self, id: &str) -> Option<job::Job>;
    async fn get_all_jobs(&self) -> Vec<job::Job>;
    async fn terminate_job(&self, id: &str) -> Option<bool>;
}

pub struct Api<M: ResourceManager>(pub M);

#[OpenApi]
impl<M: ResourceManager + Send + Sync + 'static> Api<M> {
    #[oai(tag = "ApiTags::Job", path = "/jobs", method = "post")]
    async fn create_job(&self, config: Json<job::Config>) -> poem::Result<CreateJobResponse> {
        match self.0.create_job(config.0).await {
            Some(id) => Ok(CreateJobResponse::JobId(Json(id))),
            None => Ok(CreateJobResponse::JobAlreadyExists),
            // Err(_) => Ok(CreateJobResponse::Failed(PlainText(
            //     "Job creation failed.".to_string(),
            // ))),
        }
    }

    #[oai(tag = "ApiTags::Job", path = "/jobs/:id", method = "get")]
    async fn get_job(&self, id: Path<String>) -> poem::Result<GetResponse> {
        match self.0.get_job(&id.0).await {
            Some(job) => Ok(GetResponse::Job(Json(job.clone()))),
            None => Ok(GetResponse::NotFound(PlainText(format!(
                "Job {} is not found.",
                id.0
            )))),
        }
    }

    #[oai(tag = "ApiTags::Job", path = "/jobs/:id/terminate", method = "post")]
    async fn terminate_job(&self, id: Path<String>) -> poem::Result<TerminateResponse> {
        match self.0.terminate_job(&id.0).await {
            Some(true) => Ok(TerminateResponse::Succeeded),
            Some(false) => Ok(TerminateResponse::AlreadyTerminated),
            None => Ok(TerminateResponse::NotFound(PlainText(format!(
                "Job {} is not found.",
                id.0
            )))),
        }
    }

    #[oai(path = "/jobs", method = "get")]
    async fn get_all_jobs(&self) -> poem::Result<Json<Vec<job::Job>>> {
        Ok(Json(self.0.get_all_jobs().await))
    }
}
