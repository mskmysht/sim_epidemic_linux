use std::net::SocketAddr;

use clap::Parser;
use controller::{app::Api, manager::Manager};

use poem::{endpoint::make_sync, listener::TcpListener, web::Html, Result, Route, Server};
use poem_openapi::OpenApiService;

#[derive(clap::Parser)]
pub struct Args {
    #[arg(long)]
    client_addr: SocketAddr,
    #[arg(long)]
    cert_path: String,
    #[arg(long)]
    db_username: String,
    #[arg(long)]
    db_password: String,
    #[arg(long)]
    max_job_request: usize,
    // #[arg(long)]
    servers: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let Args {
        client_addr,
        cert_path,
        db_username,
        db_password,
        max_job_request,
        servers,
    } = Args::parse();

    let api_service = OpenApiService::new(
        Api(Manager::new(
            client_addr,
            cert_path,
            db_username,
            db_password,
            max_job_request,
            servers,
        )
        .await
        .expect("Cannot connect servers.")),
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
        .await
}
