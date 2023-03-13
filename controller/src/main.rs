use std::{
    fs,
    net::{IpAddr, SocketAddr},
};

use clap::Parser;
use controller::{
    app::Api,
    manager::{worker::ServerConfig, Manager},
};

use poem::{endpoint::make_sync, listener::TcpListener, web::Html, Route, Server};
use poem_openapi::OpenApiService;

#[derive(clap::Parser)]
pub struct Args {
    config_path: String,
}

#[derive(serde::Deserialize, Debug)]
struct Config {
    addr: IpAddr,
    port: u16,
    db_username: String,
    db_password: String,
    max_job_request: usize,
    workers: Vec<ServerConfig>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Args { config_path } = Args::parse();
    let Config {
        addr,
        port,
        db_username,
        db_password,
        max_job_request,
        workers,
    } = toml::from_str::<Config>(&fs::read_to_string(&config_path)?)?;
    let api_service = OpenApiService::new(
        Api(
            Manager::new(db_username, db_password, max_job_request, addr, workers)
                .await
                .expect("Cannot connect servers."),
        ),
        "SimEpidemic for Linux",
        env!("CARGO_PKG_VERSION"),
    )
    .server("/");
    let spec = api_service.spec_endpoint();
    let endpoint = Route::new()
        .nest("/", api_service)
        .nest("/spec.json", spec)
        .at("/doc", make_sync(|_| Html(include_str!("index.html"))));
    Server::new(TcpListener::bind(SocketAddr::new(addr, port)))
        .run(endpoint)
        .await?;
    Ok(())
}
