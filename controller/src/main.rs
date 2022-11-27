use controller::api::job;

use poem::{
    endpoint::make_sync, listener::TcpListener, middleware::Cors, web::Html, EndpointExt, Result,
    Route,
};
use poem_openapi::OpenApiService;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let api_service =
        OpenApiService::new(job::Api, "Hello World", "1.0").server("http://localhost:3000/");
    let spec = api_service.spec_endpoint();
    let app = Route::new()
        .nest("/", api_service)
        .nest("/spec.json", spec)
        .at("/doc", make_sync(|_| Html(include_str!("index.html"))))
        .with(Cors::new())
        .data(job::MyDb::default());

    poem::Server::new(TcpListener::bind("0.0.0.0:3000"))
        .run(app)
        .await
}
