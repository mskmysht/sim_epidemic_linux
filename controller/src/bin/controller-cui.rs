use clap::Parser;
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

use std::fmt::Debug;

use worker_if::realtime::{parse, Request};

struct WorkerParser;
impl repl::Parsable for WorkerParser {
    type Parsed = Request;
    fn parse(buf: &str) -> repl::nom::IResult<&str, Self::Parsed> {
        parse::request(buf)
    }
}

fn logging<T, E>(ret: Result<T, E>)
where
    T: Debug,
    E: Debug,
{
    match ret {
        Ok(res) => {
            println!("{res:?}");
        }
        Err(e) => {
            eprintln!("[error] {e:?}");
        }
    }
}

mod quic {
    use std::{error::Error, net::SocketAddr};

    use quinn::Connection;
    use worker_if::realtime::{Request, Response};

    use controller::server::Server;

    pub struct MyHandler(Connection);

    impl MyHandler {
        pub async fn new(
            client_addr: SocketAddr,
            cert_path: String,
            server_info: String,
        ) -> Result<Self, Box<dyn Error>> {
            Ok(Self(
                server_info
                    .parse::<Server>()?
                    .connect(client_addr, quic_config::get_client_config(cert_path)?)
                    .await?,
            ))
        }

        pub async fn callback(
            &mut self,
            req: Request,
        ) -> Result<Response, Box<dyn Error + Send + Sync>> {
            let (mut send, mut recv) = self.0.open_bi().await?;
            let n = protocol::quic::write_data(&mut send, &req).await?;
            eprintln!("[info] sent {n} bytes data");
            let res = protocol::quic::read_data(&mut recv).await?;
            Ok(res)
        }
    }
}

mod tcp {
    use std::{io, net::TcpStream};

    use worker_if::realtime::{Request, Response};

    pub struct MyHandler<'a>(pub TcpStream, pub &'a str);

    impl<'a> MyHandler<'a> {
        pub fn callback(&mut self, req: Request) -> io::Result<Response> {
            let n = protocol::write_data(&mut self.0, &req)?;
            eprintln!("[info] sent {n} bytes data");
            let res = protocol::read_data(&mut self.0)?;
            Ok(res)
        }
    }
}
