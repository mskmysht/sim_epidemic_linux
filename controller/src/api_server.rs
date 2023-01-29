pub mod job;
pub mod task;

use std::net::SocketAddr;

use async_trait::async_trait;
use poem::{endpoint::make_sync, web::Html, Endpoint, Result, Route};
use poem_openapi::{
    param::Path,
    payload::{Json, PlainText},
    ApiResponse, OpenApi,
};
use poem_openapi::{OpenApiService, Tags};

use crate::management::{server::ServerInfo, JobManager};

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
}

#[derive(ApiResponse)]
enum GetResponse {
    #[oai(status = 200)]
    Job(Json<job::Job>),
    #[oai(status = 404)]
    NotFound(PlainText<String>),
    #[oai(status = 200)]
    Terminating,
    #[oai(status = 404)]
    Failed(PlainText<String>),
}

#[async_trait]
pub trait ResourceManager {
    async fn create_job(&self, config: job::Config) -> Option<String>;
    async fn get_job(&self, id: &str) -> Option<job::Job>;
    async fn get_all_jobs(&self) -> Vec<job::Job>;
    async fn terminate_job(&self, id: &str) -> bool;
}

pub struct Api<M: ResourceManager>(M);

#[OpenApi]
impl<M: ResourceManager + Send + Sync + 'static> Api<M> {
    #[oai(tag = "ApiTags::Job", path = "/jobs", method = "post")]
    async fn create_job(&self, config: Json<job::Config>) -> Result<CreateJobResponse> {
        if let Some(id) = self.0.create_job(config.0).await {
            Ok(CreateJobResponse::JobId(Json(id)))
        } else {
            Ok(CreateJobResponse::JobAlreadyExists)
        }
    }

    #[oai(tag = "ApiTags::Job", path = "/jobs/:id", method = "get")]
    async fn get_job(&self, id: Path<String>) -> Result<GetResponse> {
        match self.0.get_job(&id.0).await {
            Some(job) => Ok(GetResponse::Job(Json(job.clone()))),
            None => Ok(GetResponse::NotFound(PlainText(format!(
                "Job {} is not found.",
                id.0
            )))),
        }
    }

    #[oai(tag = "ApiTags::Job", path = "/jobs/:id", method = "put")]
    async fn terminate_job(&self, id: Path<String>) -> Result<GetResponse> {
        if self.0.terminate_job(&id.0).await {
            Ok(GetResponse::Terminating)
        } else {
            Ok(GetResponse::Failed(PlainText(format!(
                "Job {} cannot be terminated.",
                id.0
            ))))
        }
    }

    #[oai(path = "/jobs", method = "get")]
    async fn get_all_jobs(&self) -> Result<Json<Vec<job::Job>>> {
        Ok(Json(self.0.get_all_jobs().await))
    }
}

pub async fn create_app(
    addr: SocketAddr,
    cert_path: String,
    servers: Vec<ServerInfo>,
) -> impl Endpoint {
    let api_service = OpenApiService::new(
        Api(JobManager::new(addr, cert_path, servers, 127)
            .await
            .expect("Cannot connect servers.")),
        "Hello World",
        "1.0",
    )
    .server("/");
    let spec = api_service.spec_endpoint();
    Route::new()
        .nest("/", api_service)
        .nest("/spec.json", spec)
        .at(
            "/doc",
            make_sync(|_| Html(include_str!("api_server/index.html"))),
        )
}
