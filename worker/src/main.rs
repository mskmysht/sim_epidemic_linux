use quinn::Endpoint;
use std::{error::Error, net::SocketAddr};
use worker::batch;

#[argopt::cmd]
#[tokio::main]
async fn main(
    /// path of certificate file
    #[opt(long)]
    cert_path: String,
    /// path of private key file
    #[opt(long)]
    pkey_path: String,
    /// world binary path
    #[opt(long)]
    world_path: String,
    /// address to listen
    #[opt(long)]
    addr: SocketAddr,
    /// resource max size
    #[opt(long)]
    max_resource: usize,
) -> Result<(), Box<dyn Error>> {
    let endpoint = Endpoint::server(quic_config::get_server_config(cert_path, pkey_path)?, addr)?;
    while let Some(connecting) = endpoint.accept().await {
        let connection = connecting.await.unwrap();
        let ip = connection.remote_address().to_string();
        println!("[info] Acceept {}", ip);
        if let Err(e) = batch::run(world_path.clone(), connection, max_resource).await {
            println!("[info] Disconnect {} ({})", ip, e);
        }
    }
    Ok(())
}
