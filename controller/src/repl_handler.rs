pub mod quic {
    use async_trait::async_trait;
    use std::{error::Error, net::SocketAddr};

    use crate::management::server::{MyConnection, ServerInfo};

    type Req = worker_if::Request<world_if::Request>;
    type Ret = Result<worker_if::Response<world_if::ResponseOk>, Box<dyn Error + Send + Sync>>;

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
    }

    #[async_trait]
    impl repl::AsyncHandler for MyHandler {
        type Input = Req;
        type Output = Ret;

        async fn callback(&mut self, req: Self::Input) -> Self::Output {
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

    impl repl::Parsable for MyHandler {
        type Parsed = Req;
        fn parse(buf: &str) -> repl::ParseResult<Self::Parsed> {
            worker_if::parse::request(buf)?.try_map(|s| world_if::parse::request(&s))
        }
    }

    impl repl::Logging for MyHandler {
        type Arg = Ret;

        fn logging(ret: Self::Arg) {
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
    use std::{io, net::TcpStream};

    type Req = worker_if::Request<world_if::Request>;
    type Ret = io::Result<worker_if::Response<world_if::Response>>;

    pub struct MyHandler<'a>(pub TcpStream, pub &'a str);

    impl<'a> repl::Handler for MyHandler<'a> {
        type Input = Req;
        type Output = Ret;

        fn callback(&mut self, req: Self::Input) -> Self::Output {
            let n = protocol::write_data(&mut self.0, &req)?;
            eprintln!("[info] sent {n} bytes data");
            let res = protocol::read_data(&mut self.0)?;
            Ok(res)
        }
    }

    impl<'a> repl::Parsable for MyHandler<'a> {
        type Parsed = Req;
        fn parse(buf: &str) -> repl::ParseResult<Self::Parsed> {
            worker_if::parse::request(buf)?.try_map(|s| world_if::parse::request(&s))
        }
    }

    impl<'a> repl::Logging for MyHandler<'a> {
        type Arg = Ret;
        fn logging(output: Self::Arg) {
            match output {
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
