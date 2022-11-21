pub mod quic {
    use async_trait::async_trait;
    use quinn::{ClientConfig, Connection, Endpoint, NewConnection};
    use rustls::Certificate;
    use std::{error::Error, net::SocketAddr};

    fn config_client(cert_path: &str) -> Result<ClientConfig, Box<dyn Error>> {
        let mut certs = rustls::RootCertStore::empty();
        certs.add(&Certificate(security::load_buf(cert_path)?))?;
        Ok(ClientConfig::with_root_certificates(certs))
    }

    pub async fn get_connection(
        addr: SocketAddr,
        con_addr: SocketAddr,
        con_cert: String,
    ) -> Result<Connection, Box<dyn Error>> {
        let mut endpoint = Endpoint::client(addr)?;
        endpoint.set_default_client_config(config_client(&con_cert)?);
        let NewConnection { connection, .. } = endpoint.connect(con_addr, "localhost")?.await?;
        Ok(connection)
    }

    type Req = container_if::Request<world_if::Request>;
    type Ret = Result<
        container_if::Response<world_if::Success, world_if::ErrorStatus>,
        Box<dyn Error + Send + Sync>,
    >;

    pub struct MyHandler {
        conn: Connection,
        _name: String,
    }

    impl MyHandler {
        pub fn new(conn: Connection, name: String) -> Self {
            Self { conn, _name: name }
        }
    }

    #[async_trait]
    impl repl::AsyncHandler for MyHandler {
        type Input = Req;
        type Output = Ret;

        async fn callback(&mut self, req: Self::Input) -> Self::Output {
            let (mut send, mut recv) = self.conn.open_bi().await?;
            let n = protocol::quic::write_data(&mut send, &req).await?;
            eprintln!("[info] sent {n} bytes data");
            let res = protocol::quic::read_data(&mut recv).await?;
            Ok(res)
        }
    }

    impl repl::Parsable for MyHandler {
        type Parsed = Req;
        fn parse(buf: &str) -> repl::ParseResult<Self::Parsed> {
            container_if::parse::request(buf)?.map_r(|s| world_if::parse::request(&s))
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

    type Req = container_if::Request<world_if::Request>;
    type Ret = io::Result<container_if::Response<world_if::Success, world_if::ErrorStatus>>;

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
            container_if::parse::request(buf)?.map_r(|s| world_if::parse::request(&s))
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
