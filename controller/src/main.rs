use std::fs;

use clap::Parser;
use controller::{
    app::Api,
    manager::{Config, Manager},
};

use poem::{endpoint::make_sync, listener::TcpListener, web::Html, Route, Server};
use poem_openapi::OpenApiService;

#[derive(clap::Parser)]
pub struct Args {
    config_path: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Args { config_path } = Args::parse();
    let config = toml::from_str::<Config>(&fs::read_to_string(&config_path)?)?;
    let api_service = OpenApiService::new(
        Api(Manager::new(config).await.expect("Cannot connect servers.")),
        "SimEpidemic for Linux",
        env!("CARGO_PKG_VERSION"),
    )
    .server("/");
    let spec = api_service.spec_endpoint();
    let endpoint = Route::new()
        .nest("/", api_service)
        .nest("/spec.json", spec)
        .at("/doc", make_sync(|_| Html(include_str!("index.html"))));
    Server::new(TcpListener::bind("127.0.0.1:8080"))
        .run(endpoint)
        .await?;
    Ok(())
}
