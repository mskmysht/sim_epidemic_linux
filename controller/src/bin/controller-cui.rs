use controller::{quic, tcp};
use std::{
    error::Error,
    net::{SocketAddr, TcpStream},
};

#[argopt::cmd_group(commands = [start_tcp, start])]
fn main() -> Result<(), Box<dyn Error>> {}

#[argopt::subcmd]
fn start(
    addr: SocketAddr,
    cert_path: String,
    server_addr: SocketAddr,
    server_name: String,
    name: String,
) -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(run(addr, cert_path, server_addr, server_name, name))
}

async fn run(
    addr: SocketAddr,
    cert_path: String,
    server_addr: SocketAddr,
    server_name: String,
    name: String,
) -> Result<(), Box<dyn Error>> {
    repl::AsyncRepl::new(
        quic::MyHandler::new(
            addr,
            quic_config::get_client_config(&cert_path)?,
            server_addr,
            server_name,
            name,
        )
        .await?,
    )
    .run()
    .await;
    Ok(())
}

#[argopt::subcmd]
fn start_tcp(container1: SocketAddr, /*, container2: Ipv4Addr */) -> Result<(), Box<dyn Error>> {
    if let Ok(stream) = TcpStream::connect(container1) {
        println!("Connected to the server!");
        repl::Repl::new(tcp::MyHandler(stream, "container-1")).run();
    } else {
        println!("Couldn't connect to server...");
    }
    Ok(())
}
