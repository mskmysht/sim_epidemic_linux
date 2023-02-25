use clap::Parser;
use controller::repl_handler::{logging, quic, tcp, WorkerParser};
use repl::Parsable;
use std::{
    error::Error,
    net::{SocketAddr, TcpStream},
};

#[derive(clap::Parser)]
enum Command {
    QUIC(QuicArgs),
    TCP(TcpArgs),
}

fn main() -> Result<(), Box<dyn Error>> {
    let cmd = Command::parse();
    match cmd {
        Command::QUIC(args) => start_quic(args),
        Command::TCP(args) => start_tcp(args),
    }
}

#[derive(clap::Args)]
struct QuicArgs {
    addr: SocketAddr,
    cert_path: String,
    server_info: String,
}

fn start_quic(args: QuicArgs) -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut handler = quic::MyHandler::new(args.addr, args.cert_path, args.server_info).await?;
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

#[derive(clap::Args)]
struct TcpArgs {
    container1: SocketAddr,
    /*, container2: Ipv4Addr */
}

fn start_tcp(args: TcpArgs) -> Result<(), Box<dyn Error>> {
    if let Ok(stream) = TcpStream::connect(args.container1) {
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
