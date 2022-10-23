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
    con_addr: SocketAddr,
    con_cert: String,
    con_name: String,
) -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(run(addr, con_addr, con_cert, con_name))
}

async fn run(
    addr: SocketAddr,
    con_addr: SocketAddr,
    con_cert: String,
    con_name: String,
) -> Result<(), Box<dyn Error>> {
    let connection = quic::get_connection(addr, con_addr, con_cert).await?;
    repl::AsyncRepl::new(quic::MyHandler::new(connection, con_name))
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
