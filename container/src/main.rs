use container::WorldManager;
use parking_lot::Mutex;
use quinn::Endpoint;
use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener},
    sync::Arc,
};

type DynResult<T> = Result<T, Box<dyn Error>>;

#[argopt::cmd_group(commands = [start_tcp, start])]
fn main() -> DynResult<()> {}

#[argopt::subcmd]
fn start_tcp(
    /// world binary path
    #[opt(long)]
    world_path: String,
) -> DynResult<()> {
    run_tcp(world_path)
}

#[argopt::subcmd]
fn start(
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
    /// idle timeout
    timeout: u32,
) -> DynResult<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(run(cert_path, pkey_path, world_path, addr, timeout))
}

async fn run(
    cert_path: String,
    pkey_path: String,
    world_path: String,
    addr: SocketAddr,
    timeout: u32,
) -> DynResult<()> {
    let endpoint = Endpoint::server(
        quic_config::get_server_config(cert_path, pkey_path, timeout)?,
        addr,
    )?;
    let manager = Arc::new(Mutex::new(WorldManager::new(world_path)));

    while let Some(connecting) = endpoint.accept().await {
        let connection = connecting.await?;
        let ip = connection.remote_address().to_string();
        println!("[info] Acceept {}", ip);

        let e = loop {
            match connection.accept_bi().await {
                Ok((mut send, mut recv)) => {
                    let manager = Arc::clone(&manager);
                    tokio::spawn(async move {
                        let req = protocol::quic::read_data(&mut recv).await.unwrap();
                        println!("[request] {req:?}");
                        let res = manager.lock().callback(req);
                        println!("[response] {res:?}");
                        protocol::quic::write_data(&mut send, &res).await.unwrap();
                    });
                }
                Err(e) => {
                    break e;
                }
            }
        };
        println!("[info] Disconnect {} ({})", ip, e);
    }
    Ok(())
}

fn run_tcp(world_path: String) -> DynResult<()> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 8080))?;
    let mut manager = WorldManager::new(world_path);
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
            let res = manager.callback(req);
            println!("[response] {res:?}");
            if let Err(e) = protocol::write_data(&mut stream, &res) {
                eprintln!("{e}");
            }
        }
        println!("[info] Disconnect {addr}");
    }
    Ok(())
}
