use container::world::WorldManager;
use futures_util::StreamExt;
use parking_lot::Mutex;
use quinn::{Endpoint, ServerConfig};
use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener},
    sync::Arc,
};

type DynResult<T> = Result<T, Box<dyn Error>>;

#[argopt::cmd_group(commands = [gen, start_tcp, start])]
fn main() -> DynResult<()> {}

#[argopt::subcmd]
fn gen(
    /// subject alternative names
    #[opt(long)]
    names: Vec<String>,
) -> DynResult<()> {
    println!("{names:?}");
    let cert = rcgen::generate_simple_self_signed(names)?;
    security::dump_ca_cert_der(&cert.serialize_der()?)?;
    security::dump_ca_pkey_der(&cert.serialize_private_key_der())?;
    println!("successfully generated a certificate & a private key.");
    Ok(())
}

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
    /// world binary path
    #[opt(long)]
    world_path: String,
    /// world binary path
    #[opt(long)]
    addr: SocketAddr,
) -> DynResult<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(run(world_path, addr))
}

fn config_server() -> DynResult<ServerConfig> {
    let cert = rustls::Certificate(security::load_ca_cert_der()?);
    let key = rustls::PrivateKey(security::load_ca_pkey_der()?);
    Ok(ServerConfig::with_single_cert(vec![cert], key)?)
}

async fn run(world_path: String, addr: SocketAddr) -> DynResult<()> {
    let (_, mut incoming) = Endpoint::server(config_server()?, addr)?;
    let manager = Arc::new(Mutex::new(WorldManager::new(world_path)));

    while let Some(conn) = incoming.next().await {
        let ip = conn.remote_address().to_string();
        println!("[info] Acceept {}", ip);

        let mut new_conn = conn.await?;
        while let Some(Ok((mut send, mut recv))) = new_conn.bi_streams.next().await {
            let manager = Arc::clone(&manager);
            tokio::spawn(async move {
                let req = protocol::quic::read_data(&mut recv).await.unwrap();
                println!("[request] {req:?}");
                let res = manager.lock().callback(req);
                println!("[response] {res:?}");
                protocol::quic::write_data(&mut send, &res).await.unwrap();
            });
        }
        println!("[info] Disconnect {}", ip);
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
