use clap::Parser;
use quinn::Endpoint;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener};
use worker::realtime::WorldManaging;

#[derive(clap::Parser)]
enum Command {
    QUIC(QuicArgs),
    TCP(TcpArgs),
}

fn main() -> anyhow::Result<()> {
    match Command::parse() {
        Command::QUIC(QuicArgs {
            cert_path,
            pkey_path,
            world_path,
            addr,
        }) => {
            let endpoint =
                Endpoint::server(quic_config::get_server_config(cert_path, pkey_path)?, addr)?;
            let managing = worker::realtime::WorldManaging::new(world_path);
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(run_quic(endpoint, managing))
        }
        Command::TCP(TcpArgs { world_path }) => run_tcp(world_path),
    }
}

#[derive(clap::Args)]
struct QuicArgs {
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
}

async fn run_quic(endpoint: Endpoint, managing: WorldManaging) -> anyhow::Result<()> {
    while let Some(connecting) = endpoint.accept().await {
        let connection = connecting.await?;
        let ip = connection.remote_address().to_string();
        println!("[info] Acceept {}", ip);

        loop {
            match connection.accept_bi().await {
                Ok((mut send, mut recv)) => {
                    let manager = managing.get_manager().clone();
                    tokio::spawn(async move {
                        let req = protocol::quic::read_data(&mut recv).await.unwrap();
                        println!("[request] {req:?}");
                        let res = manager.request(req);
                        println!("[response] {res:?}");
                        protocol::quic::write_data(&mut send, &res).await.unwrap();
                    });
                }
                Err(e) => {
                    println!("[info] Disconnect {} ({})", ip, e);
                    break;
                }
            }
        }
    }
    Ok(())
}

#[derive(clap::Args)]
struct TcpArgs {
    /// world binary path
    #[arg(long)]
    world_path: String,
}

fn run_tcp(world_path: String) -> anyhow::Result<()> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 8080))?;

    let managing = worker::realtime::WorldManaging::new(world_path);
    let manager = managing.get_manager();

    for stream in listener.incoming() {
        let mut stream = stream?;
        let addr = stream.peer_addr()?.to_string();
        println!("[info] Acceept {addr}");
        loop {
            let req = match protocol::read_data(&mut stream) {
                Ok(req) => req,
                Err(_) => break,
            };
            println!("[request] {req:?}");
            let res = manager.request(req);
            println!("[response] {res:?}");
            if let Err(e) = protocol::write_data(&mut stream, &res) {
                eprintln!("{e}");
            }
        }
        println!("[info] Disconnect {addr}");
    }

    Ok(())
}
