use std::fmt::Debug;

pub struct WorkerParser;
impl repl::Parsable for WorkerParser {
    type Parsed = worker_if::Request;
    fn parse(buf: &str) -> repl::nom::IResult<&str, Self::Parsed> {
        worker_if::parse::request(buf)
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

    use crate::management::server::{MyConnection, ServerInfo};

    pub struct MyHandler(MyConnection);

    impl MyHandler {
        pub async fn new(
            addr: SocketAddr,
            cert_path: String,
            server_info: ServerInfo,
            name: String,
        ) -> Result<Self, Box<dyn Error>> {
            Ok(Self(
                MyConnection::new(
                    addr,
                    quic_config::get_client_config(cert_path)?,
                    server_info,
                    name,
                )
                .await?,
            ))
        }

        pub async fn callback(
            &mut self,
            req: worker_if::Request,
        ) -> Result<worker_if::Response, Box<dyn Error + Send + Sync>> {
            if self.0.connection.close_reason().is_some() {
                self.0.connect().await?;
            }
            let (mut send, mut recv) = self.0.connection.open_bi().await?;
            let n = protocol::quic::write_data(&mut send, &req).await?;
            eprintln!("[info] sent {n} bytes data");
            let res = protocol::quic::read_data(&mut recv).await?;
            Ok(res)
        }
    }
}

pub mod tcp {
    use std::{io, net::TcpStream};

    pub struct MyHandler<'a>(pub TcpStream, pub &'a str);

    impl<'a> MyHandler<'a> {
        pub fn callback(&mut self, req: worker_if::Request) -> io::Result<worker_if::Response> {
            let n = protocol::write_data(&mut self.0, &req)?;
            eprintln!("[info] sent {n} bytes data");
            let res = protocol::read_data(&mut self.0)?;
            Ok(res)
        }
    }
}