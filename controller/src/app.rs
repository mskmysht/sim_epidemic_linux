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
}

#[derive(ApiResponse)]
enum GetAllJobsResponse {
    #[oai(status = 200)]
    Jobs(Json<Vec<job::Job>>),
    #[oai(status = 500)]
    InternalError,
}

#[derive(ApiResponse)]
enum GetJobResponse {
    #[oai(status = 200)]
    Job(Json<job::Job>),
    #[oai(status = 404)]
    NotFound(PlainText<String>),
    #[oai(status = 500)]
    InternalError,
}

#[derive(ApiResponse)]
enum GetTaskResponse {
    #[oai(status = 200)]
    Task(Json<task::Task>),
    #[oai(status = 404)]
    NotFound(PlainText<String>),
    #[oai(status = 500)]
    InternalError,
}

#[derive(ApiResponse)]
enum TerminateJobResponse {
    #[oai(status = 204)]
    Succeeded,
    #[oai(status = 405)]
    AlreadyTerminated,
    #[oai(status = 404)]
    NotFound(PlainText<String>),
    #[oai(status = 500)]
    InternalError,
}

#[derive(ApiResponse)]
enum DeleteJobResponse {
    #[oai(status = 202)]
    Accepted,
    #[oai(status = 404)]
    NotFound(PlainText<String>),
}

#[derive(ApiResponse)]
enum GetStatisticsResponse {
    #[oai(status = 200, content_type = "text/csv")]
    CSV(Binary<Vec<u8>>),
    #[oai(status = 404)]
    NotFound(PlainText<String>),
    #[oai(status = 500)]
    InternalError,
}

#[async_trait]
pub trait ResourceManager {
    async fn create_job(&self, config: job::Config) -> Option<String>;
    async fn get_job(&self, id: &str) -> anyhow::Result<Option<job::Job>>;
    async fn get_all_jobs(&self) -> anyhow::Result<Vec<job::Job>>;
    fn delete_job(&self, id: &str) -> Result<(), uuid::Error>;
    async fn terminate_job(&self, id: &str) -> anyhow::Result<Option<bool>>;
    async fn get_task(&self, id: &str) -> anyhow::Result<Option<task::Task>>;
    async fn get_statistics(&self, id: &str) -> anyhow::Result<Option<Vec<u8>>>;
}

pub struct Api<M: ResourceManager>(pub M);

#[OpenApi]
impl<M: ResourceManager + Send + Sync + 'static> Api<M> {
    #[oai(tag = "ApiTags::Job", path = "/jobs", method = "post")]
    async fn create_job(&self, config: Json<job::Config>) -> poem::Result<CreateJobResponse> {
        match self.0.create_job(config.0).await {
            Some(id) => Ok(CreateJobResponse::JobId(Json(id))),
            None => Ok(CreateJobResponse::JobAlreadyExists),
        }
    }

    #[oai(tag = "ApiTags::Job", path = "/jobs/:id", method = "get")]
    async fn get_job(&self, id: Path<String>) -> poem::Result<GetJobResponse> {
        match self.0.get_job(&id.0).await {
            Ok(Some(job)) => Ok(GetJobResponse::Job(Json(job.clone()))),
            Ok(None) => Ok(GetJobResponse::NotFound(PlainText(format!(
                "Job {} is not found.",
                id.0
            )))),
            Err(_) => Ok(GetJobResponse::InternalError),
        }
    }

    #[oai(tag = "ApiTags::Job", path = "/jobs/:id/terminate", method = "post")]
    async fn terminate_job(&self, id: Path<String>) -> poem::Result<TerminateJobResponse> {
        match self.0.terminate_job(&id.0).await {
            Ok(Some(true)) => Ok(TerminateJobResponse::Succeeded),
            Ok(Some(false)) => Ok(TerminateJobResponse::AlreadyTerminated),
            Ok(None) => Ok(TerminateJobResponse::NotFound(PlainText(format!(
                "Job {} is not found.",
                id.0
            )))),
            Err(_) => Ok(TerminateJobResponse::InternalError),
        }
    }

    #[oai(tag = "ApiTags::Job", path = "/jobs", method = "get")]
    async fn get_all_jobs(&self) -> poem::Result<GetAllJobsResponse> {
        match self.0.get_all_jobs().await {
            Ok(js) => Ok(GetAllJobsResponse::Jobs(Json(js))),
            Err(_) => Ok(GetAllJobsResponse::InternalError),
        }
    }

    #[oai(tag = "ApiTags::Job", path = "/jobs/:id", method = "delete")]
    async fn delete_job(&self, id: Path<String>) -> poem::Result<DeleteJobResponse> {
        match self.0.delete_job(&id.0) {
            Ok(_) => Ok(DeleteJobResponse::Accepted),
            Err(_) => Ok(DeleteJobResponse::NotFound(PlainText(format!(
                "Job {} is not found.",
                id.0
            )))),
        }
    }

    #[oai(tag = "ApiTags::Task", path = "/tasks/:id", method = "get")]
    async fn get_task(&self, id: Path<String>) -> poem::Result<GetTaskResponse> {
        match self.0.get_task(&id.0).await {
            Ok(Some(task)) => Ok(GetTaskResponse::Task(Json(task.clone()))),
            Ok(None) => Ok(GetTaskResponse::NotFound(PlainText(format!(
                "Task {} is not found.",
                id.0
            )))),
            Err(_) => Ok(GetTaskResponse::InternalError),
        }
    }

    #[oai(tag = "ApiTags::Task", path = "/tasks/:id/statistics", method = "get")]
    async fn get_statistics(&self, id: Path<String>) -> poem::Result<GetStatisticsResponse> {
        match self.0.get_statistics(&id.0).await {
            Ok(Some(bin)) => Ok(GetStatisticsResponse::CSV(Binary(bin))),
            Ok(None) => Ok(GetStatisticsResponse::NotFound(PlainText(format!(
                "Task {} is not found.",
                id.0
            )))),
            Err(_) => Ok(GetStatisticsResponse::InternalError),
        }
    }
}
