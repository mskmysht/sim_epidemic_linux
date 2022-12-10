mod job;

use poem::{endpoint::make_sync, middleware::Cors, web::Html, Endpoint, EndpointExt, Route};
use poem_openapi::{OpenApiService, Tags};

#[derive(Tags)]
enum ApiTags {
    /// Operations about job
    Job,
}

pub async fn create_app() -> impl Endpoint {
    let api_service =
        OpenApiService::new(job::Api, "Hello World", "1.0").server("http://localhost:3000/");
    let spec = api_service.spec_endpoint();
    Route::new()
        .nest("/", api_service)
        .nest("/spec.json", spec)
        .at("/doc", make_sync(|_| Html(include_str!("index.html"))))
        .with(Cors::new())
        .data(job::MyDb::default())
}
