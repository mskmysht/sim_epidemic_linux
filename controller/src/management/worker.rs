use quinn::ClientConfig;
use std::{error::Error, net::SocketAddr};

use super::server::{MyConnection, ServerInfo};

pub struct WorkerClient(MyConnection);

impl WorkerClient {
    pub async fn new(
        addr: SocketAddr,
        config: ClientConfig,
        server_info: ServerInfo,
        index: usize,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Self(
            MyConnection::new(
                addr,
                config,
                server_info,
                format!("worker-{index}").to_string(),
            )
            .await?,
        ))
    }
}
