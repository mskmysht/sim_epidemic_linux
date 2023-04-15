use clap::Parser;
use quinn::Endpoint;
use std::{error::Error, net::SocketAddr, path::Path};
use worker::batch;

#[derive(clap::Parser)]
struct Args {
    config_path: String,
}

#[derive(serde::Deserialize)]
struct Config {
    /// path of certificate file
    cert_path: String,
    /// path of private key file
    pkey_path: String,
    /// world binary path
    world_path: String,
    /// address to listen
    addr: SocketAddr,
    /// max population size
    max_population_size: u32,
    /// max resource size
    max_resource: u32,
    /// directory where statistics are saved
    stat_dir: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let Args { config_path } = Args::parse();
    let Config {
        cert_path,
        pkey_path,
        world_path,
        addr,
        max_population_size,
        max_resource,
        stat_dir,
    } = toml::from_str(&std::fs::read_to_string(&config_path)?)?;
    let stat_dir_path = Path::new(&stat_dir).to_path_buf();
    assert!(stat_dir_path.exists(), "{} does not exist.", stat_dir);
    assert!(
        Path::new(&world_path).exists(),
        "{} does not exist.",
        world_path
    );

    tracing_subscriber::fmt::init();

    let endpoint = Endpoint::server(quic_config::get_server_config(cert_path, pkey_path)?, addr)?;
    let manager = batch::WorldManager::new(world_path, stat_dir, stat_dir_path);
    while let Some(connecting) = endpoint.accept().await {
        let connection = connecting.await.unwrap();
        let ip = connection.remote_address().to_string();
        let manager = manager.clone();

        let connection2 = connection.clone();
        let handle = tokio::spawn(batch::run(
            manager,
            connection,
            max_population_size,
            max_resource,
        ));
        tokio::spawn(async move {
            tracing::info!(accepted = ip);
            let err = connection2.closed().await;
            handle.abort();
            tracing::error!(closed = ip, ?err);
        });
    }
    Ok(())
}
