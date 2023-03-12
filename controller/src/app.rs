pub mod job;
pub mod task;

use async_trait::async_trait;
use poem_openapi::payload::Binary;
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
    /// Operations about task
    Task,
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
enum GetJobResponse {
    #[oai(status = 200)]
    Job(Json<job::Job>),
    #[oai(status = 404)]
    NotFound(PlainText<String>),
}

#[derive(ApiResponse)]
enum GetTaskResponse {
    #[oai(status = 200)]
    Task(Json<task::Task>),
    #[oai(status = 404)]
    NotFound(PlainText<String>),
}

#[derive(ApiResponse)]
enum TerminateJobResponse {
    #[oai(status = 204)]
    Succeeded,
    #[oai(status = 405)]
    AlreadyTerminated,
    #[oai(status = 404)]
    NotFound(PlainText<String>),
}

#[derive(ApiResponse)]
enum DeleteJobResponse {
    #[oai(status = 202)]
    Accepted,
    #[oai(status = 500)]
    InternalError,
}

#[derive(ApiResponse)]
enum GetStatisticsResponse {
    #[oai(status = 200, content_type = "text/csv")]
    CSV(Binary<Vec<u8>>),
    #[oai(status = 404)]
    NotFound,
}

#[async_trait]
pub trait ResourceManager {
    async fn create_job(&self, config: job::Config) -> Option<String>;
    async fn get_job(&self, id: &str) -> Option<job::Job>;
    async fn get_all_jobs(&self) -> Vec<job::Job>;
    async fn delete_all_jobs(&self) -> bool;
    async fn terminate_job(&self, id: &str) -> Option<bool>;
    async fn get_task(&self, id: &str) -> Option<task::Task>;
    async fn get_statistics(&self, id: &str) -> Option<Vec<u8>>;
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
    async fn get_job(&self, id: Path<String>) -> poem::Result<GetJobResponse> {
        match self.0.get_job(&id.0).await {
            Some(job) => Ok(GetJobResponse::Job(Json(job.clone()))),
            None => Ok(GetJobResponse::NotFound(PlainText(format!(
                "Job {} is not found.",
                id.0
            )))),
        }
    }

    #[oai(tag = "ApiTags::Job", path = "/jobs/:id/terminate", method = "post")]
    async fn terminate_job(&self, id: Path<String>) -> poem::Result<TerminateJobResponse> {
        match self.0.terminate_job(&id.0).await {
            Some(true) => Ok(TerminateJobResponse::Succeeded),
            Some(false) => Ok(TerminateJobResponse::AlreadyTerminated),
            None => Ok(TerminateJobResponse::NotFound(PlainText(format!(
                "Job {} is not found.",
                id.0
            )))),
        }
    }

    #[oai(tag = "ApiTags::Job", path = "/jobs", method = "get")]
    async fn get_all_jobs(&self) -> poem::Result<Json<Vec<job::Job>>> {
        Ok(Json(self.0.get_all_jobs().await))
    }

    #[oai(tag = "ApiTags::Job", path = "/jobs", method = "delete")]
    async fn delete_all_jobs(&self) -> poem::Result<DeleteJobResponse> {
        if self.0.delete_all_jobs().await {
            Ok(DeleteJobResponse::Accepted)
        } else {
            Ok(DeleteJobResponse::InternalError)
        }
    }

    #[oai(tag = "ApiTags::Task", path = "/tasks/:id", method = "get")]
    async fn get_task(&self, id: Path<String>) -> poem::Result<GetTaskResponse> {
        match self.0.get_task(&id.0).await {
            Some(task) => Ok(GetTaskResponse::Task(Json(task.clone()))),
            None => Ok(GetTaskResponse::NotFound(PlainText(format!(
                "Task {} is not found.",
                id.0
            )))),
        }
    }

    #[oai(tag = "ApiTags::Task", path = "/tasks/:id/statistics", method = "get")]
    async fn get_statistics(&self, id: Path<String>) -> poem::Result<GetStatisticsResponse> {
        match self.0.get_statistics(&id.0).await {
            Some(bin) => Ok(GetStatisticsResponse::CSV(Binary(bin))),
            None => Ok(GetStatisticsResponse::NotFound),
        }
    }
}
