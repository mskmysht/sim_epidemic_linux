use std::net::SocketAddr;

use controller::{api_server, management::server::ServerInfo};

use poem::{listener::TcpListener, Result, Server};

#[argopt::cmd]
#[tokio::main]
async fn main(
    addr: SocketAddr,
    cert_path: String,
    servers: Vec<ServerInfo>,
) -> Result<(), std::io::Error> {
    Server::new(TcpListener::bind("0.0.0.0:8080"))
        .run(api_server::create_app(addr, cert_path, servers).await)
        .await
}
