pub mod quic {
    use async_trait::async_trait;
    use quinn::{ClientConfig, Connection, Endpoint};
    use std::{error::Error, net::SocketAddr};

    fn get_endpoint(addr: SocketAddr, config: ClientConfig) -> Result<Endpoint, Box<dyn Error>> {
        let mut endpoint = Endpoint::client(addr)?;
        endpoint.set_default_client_config(config);
        Ok(endpoint)
    }

    type Req = worker_if::Request<world_if::Request>;
    type Ret = Result<worker_if::Result<world_if::Response>, Box<dyn Error + Send + Sync>>;

    pub struct MyHandler {
        endpoint: Endpoint,
        // server_addr: SocketAddr,
        server_name: String,
        connection: Connection,
        _name: String,
    }

    impl MyHandler {
        pub async fn new(
            addr: SocketAddr,
            config: ClientConfig,
            server_addr: SocketAddr,
            server_name: String,
            name: String,
        ) -> Result<Self, Box<dyn Error>> {
            let endpoint = get_endpoint(addr, config)?;
            let connection = endpoint.connect(server_addr, &server_name)?.await?;
            Ok(Self {
                endpoint,
                // server_addr: connection.remote_address(),
                server_name,
                connection,
                _name: name,
            })
        }
    }

    impl MyHandler {
        async fn connect(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
            self.connection = self
                .endpoint
                .connect(self.connection.remote_address(), &self.server_name)?
                .await?;
            Ok(())
        }
    }

    #[async_trait]
    impl repl::AsyncHandler for MyHandler {
        type Input = Req;
        type Output = Ret;

        async fn callback(&mut self, req: Self::Input) -> Self::Output {
            if self.connection.close_reason().is_some() {
                self.connect().await?;
            }
            let (mut send, mut recv) = self.connection.open_bi().await?;
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
    type Ret = io::Result<worker_if::Result<world_if::Response>>;

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
