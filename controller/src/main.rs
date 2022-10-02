use protocol::stdio;
use quinn::{ClientConfig, Endpoint, NewConnection};
use rustls::Certificate;
use std::{
    error::Error,
    net::{SocketAddr, TcpStream},
};

type DynResult<T> = Result<T, Box<dyn Error>>;

#[argopt::cmd_group(commands = [start_tcp, start])]
fn main() -> DynResult<()> {}

#[argopt::subcmd]
fn start(
    addr: SocketAddr,
    con_addr: SocketAddr,
    con_cert: String,
    con_name: String,
) -> DynResult<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(run(addr, con_addr, con_cert, con_name))
}

async fn run(
    addr: SocketAddr,
    con_addr: SocketAddr,
    con_cert: String,
    con_name: String,
) -> DynResult<()> {
    let mut endpoint = Endpoint::client(addr)?;
    endpoint.set_default_client_config(config_client(&con_cert)?);
    let NewConnection { connection, .. } = endpoint.connect(con_addr, "localhost")?.await?;
    stdio::AsyncRunner::new(quic::MyListener::new(connection, con_name))
        .run()
        .await;
    Ok(())
}

fn config_client(cert_path: &str) -> DynResult<ClientConfig> {
    let mut certs = rustls::RootCertStore::empty();
    certs.add(&Certificate(config::load_buf(cert_path)?))?;
    Ok(ClientConfig::with_root_certificates(certs))
}

#[argopt::subcmd]
fn start_tcp(container1: SocketAddr, /*, container2: Ipv4Addr */) -> Result<(), Box<dyn Error>> {
    if let Ok(stream) = TcpStream::connect(container1) {
        println!("Connected to the server!");
        stdio::Runner::new(tcp::MyListener(stream, "container-1")).run();
    } else {
        println!("Couldn't connect to server...");
    }
    Ok(())
}

pub mod quic {
    use async_trait::async_trait;
    use protocol::{stdio, AsyncCallback};
    use quinn::Connection;
    use std::error::Error;

    type Req = container_if::Request<world_if::Request>;
    type Ret = Result<
        container_if::Response<world_if::Success, world_if::ErrorStatus>,
        Box<dyn Error + Send + Sync>,
    >;

    pub struct MyListener {
        conn: Connection,
        // send: SendStream,
        // recv: RecvStream,
        _name: String,
    }

    impl MyListener {
        pub fn new(conn: Connection, name: String) -> Self {
            Self { conn, _name: name }
        }
    }

    #[async_trait]
    impl AsyncCallback for MyListener {
        type Req = Req;
        type Ret = Ret;

        async fn callback(&mut self, req: Self::Req) -> Self::Ret {
            let (mut send, mut recv) = self.conn.open_bi().await?;
            let n = protocol::quic::write_data(&mut send, &req).await?;
            eprintln!("[info] sent {n} bytes data");
            let res = protocol::quic::read_data(&mut recv).await?;
            Ok(res)
        }
    }

    impl stdio::InputLoop<Req, Ret> for MyListener {
        fn parse(input: &str) -> stdio::ParseResult<Req> {
            protocol::parse::request(input)
        }

        fn logging(ret: Ret) {
            match ret {
                Ok(res) => {
                    println!("{res:?}");
                }
                Err(e) => {
                    eprintln!("[error] {e:?}");
                }
            }
        }
    }
}

pub mod tcp {
    use protocol::{stdio, SyncCallback};
    use std::{io, net::TcpStream};

    type Req = container_if::Request<world_if::Request>;
    type Ret = io::Result<container_if::Response<world_if::Success, world_if::ErrorStatus>>;

    pub struct MyListener<'a>(pub TcpStream, pub &'a str);

    impl<'a> SyncCallback for MyListener<'a> {
        type Req = Req;
        type Ret = Ret;

        fn callback(&mut self, req: Self::Req) -> Self::Ret {
            let n = protocol::write_data(&mut self.0, &req)?;
            eprintln!("[info] sent {n} bytes data");
            let res = protocol::read_data(&mut self.0)?;
            Ok(res)
        }
    }

    impl<'a> stdio::InputLoop<Req, Ret> for MyListener<'a> {
        fn parse(input: &str) -> stdio::ParseResult<Req> {
            protocol::parse::request(input)
        }

        fn logging(ret: Ret) {
            match ret {
                Ok(res) => {
                    println!("{res:?}");
                }
                Err(e) => {
                    eprintln!("[error] {e:?}");
                }
            }
        }
    }
}
