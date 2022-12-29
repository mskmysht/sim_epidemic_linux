use controller::{
    management::server::ServerInfo,
    repl_handler::{logging, quic, tcp, WorkerParser},
};
use repl::Parsable;
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
    server_info: ServerInfo,
    name: String,
) -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut handler = quic::MyHandler::new(addr, cert_path, server_info, name).await?;
        loop {
            match WorkerParser::recv_input() {
                repl::Command::Quit => break,
                repl::Command::None => {}
                repl::Command::Delegate(input) => {
                    let output = handler.callback(input).await;
                    logging(output);
                }
            }
        }
        Ok(())
    })
}

#[argopt::subcmd]
fn start_tcp(container1: SocketAddr, /*, container2: Ipv4Addr */) -> Result<(), Box<dyn Error>> {
    if let Ok(stream) = TcpStream::connect(container1) {
        println!("Connected to the server!");
        let mut handler = tcp::MyHandler(stream, "container-1");
        loop {
            match WorkerParser::recv_input() {
                repl::Command::Quit => break,
                repl::Command::None => {}
                repl::Command::Delegate(input) => {
                    let output = handler.callback(input);
                    logging(output);
                }
            }
        }
    } else {
        println!("Couldn't connect to server...");
    }
    Ok(())
}
