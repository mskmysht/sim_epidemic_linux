use clap::Parser;
use controller::{
    api_server::Api,
    management::{Args, Manager},
};

use poem::{endpoint::make_sync, listener::TcpListener, web::Html, Result, Route, Server};
use poem_openapi::OpenApiService;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let args = Args::parse();
    let api_service = OpenApiService::new(
        Api(Manager::new(args).await.expect("Cannot connect servers.")),
        "Hello World",
        "1.0",
    )
    .server("/");
    let spec = api_service.spec_endpoint();
    let endpoint = Route::new()
        .nest("/", api_service)
        .nest("/spec.json", spec)
        .at(
            "/doc",
            make_sync(|_| Html(include_str!("api_server/index.html"))),
        );
    Server::new(TcpListener::bind("0.0.0.0:8080"))
        .run(endpoint)
        .await
}
