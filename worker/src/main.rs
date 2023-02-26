use clap::Parser;
use quinn::Endpoint;
use std::{error::Error, net::SocketAddr};
use worker::batch;

#[derive(clap::Parser)]
struct Args {
    /// path of certificate file
    #[arg(long)]
    cert_path: String,
    /// path of private key file
    #[arg(long)]
    pkey_path: String,
    /// world binary path
    #[arg(long)]
    world_path: String,
    /// address to listen
    #[arg(long)]
    addr: SocketAddr,
    /// max population size
    #[arg(long)]
    max_population_size: u32,
    /// max resource size
    #[arg(long)]
    max_resource: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let Args {
        cert_path,
        pkey_path,
        world_path,
        addr,
        max_population_size,
        max_resource,
    } = Args::parse();
    let endpoint = Endpoint::server(quic_config::get_server_config(cert_path, pkey_path)?, addr)?;
    while let Some(connecting) = endpoint.accept().await {
        let connection = connecting.await.unwrap();
        let ip = connection.remote_address().to_string();
        println!("[info] Acceept {}", ip);
        if let Err(e) = batch::run(
            world_path.clone(),
            connection,
            max_population_size,
            max_resource,
        )
        .await
        {
            println!("[info] Disconnect {} ({})", ip, e);
        }
    }
    Ok(())
}
