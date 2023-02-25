use quinn::{ClientConfig, Connection, Endpoint};
use std::{net::SocketAddr, str::FromStr};

#[derive(Debug)]
pub struct Server {
    addr: SocketAddr,
    name: String,
}

impl Server {
    pub async fn connect(
        self,
        client_addr: SocketAddr,
        config: ClientConfig,
    ) -> anyhow::Result<Connection> {
        let mut endpoint = Endpoint::client(client_addr)?;
        endpoint.set_default_client_config(config);
        Ok(endpoint.connect(self.addr, &self.name)?.await?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseServerInfoError {
    #[error("invalid syntax")]
    InvalidSyntax,
    #[error("invalid address")]
    InvalidAddress(#[from] std::net::AddrParseError),
}

impl FromStr for Server {
    type Err = ParseServerInfoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (ls, rs) = s
            .split_once('/')
            .ok_or(ParseServerInfoError::InvalidSyntax)?;

        let addr = ls.trim().parse::<SocketAddr>()?;
        let name = rs.trim().to_string();
        Ok(Self { addr, name })
    }
}

#[cfg(test)]
mod tests {
    use super::{ParseServerInfoError, Server};

    #[test]
    fn parse_server_info_test() {
        let addr = "192.168.1.10:8000";
        let name = "fuga";
        let si = format!(" {addr} / {name} ").parse::<Server>().unwrap();
        assert_eq!(si.addr, addr.parse().unwrap());
        assert_eq!(si.name, name.to_string());

        let si = format!("hoge / {name} ").parse::<Server>();
        assert!(matches!(si, Err(ParseServerInfoError::InvalidAddress(_))));

        let si = format!(" {addr}, {name} ").parse::<Server>();
        assert!(matches!(si, Err(ParseServerInfoError::InvalidSyntax)));
    }
}
