use poem::{endpoint::make_sync, middleware::Cors, web::Html, Endpoint, EndpointExt, Route};
use poem::{web::Data, Result};
use poem_openapi::{
    param::Path,
    payload::{Json, PlainText},
    ApiResponse, OpenApi,
};
use poem_openapi::{OpenApiService, Tags};

use batch::client::BatchClient;
use batch::types::job::{Job, JobConfig};

#[derive(Tags)]
enum ApiTags {
    /// Operations about job
    Job,
}

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
    async fn create(
        &self,
        pool: Data<&BatchClient>,
        config: Json<JobConfig>,
    ) -> Result<Json<String>> {
        let id = pool.0.create(config.0);
        Ok(Json(id))
    }

    #[oai(path = "/jobs/:id", method = "get")]
    async fn get(&self, pool: Data<&BatchClient>, id: Path<String>) -> Result<GetResponse> {
        match pool.0.get(&id.0) {
            Some(job) => Ok(GetResponse::Job(Json(job.clone()))),
            None => Ok(GetResponse::NotFound(PlainText(format!(
                "Job {} is not found.",
                id.0
            )))),
        }
    }

    #[oai(path = "/jobs", method = "get")]
    async fn get_all(&self, pool: Data<&BatchClient>) -> Result<Json<Vec<Job>>> {
        Ok(Json(pool.0.get_all()))
    }
}

pub async fn create_app() -> impl Endpoint {
    let api_service =
        OpenApiService::new(Api, "Hello World", "1.0").server("http://localhost:3000/");
    let spec = api_service.spec_endpoint();
    Route::new()
        .nest("/", api_service)
        .nest("/spec.json", spec)
        .at("/doc", make_sync(|_| Html(include_str!("index.html"))))
        .with(Cors::new())
        .data(BatchClient::connect("localhost:4050"))
}
