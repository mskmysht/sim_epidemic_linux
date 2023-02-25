use std::fmt::Debug;

use worker_if::realtime::{parse, Request};

pub struct WorkerParser;
impl repl::Parsable for WorkerParser {
    type Parsed = Request;
    fn parse(buf: &str) -> repl::nom::IResult<&str, Self::Parsed> {
        parse::request(buf)
    }
}

pub fn logging<T, E>(ret: Result<T, E>)
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

pub mod quic {
    use std::{error::Error, net::SocketAddr};

    use quinn::Connection;
    use worker_if::realtime::{Request, Response};

    use crate::management::server::Server;

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

pub mod tcp {
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
