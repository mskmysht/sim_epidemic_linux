use quinn::{ClientConfig, Connection, Endpoint};
use std::{net::SocketAddr, str::FromStr};

#[derive(Debug)]
pub struct Server {
    addr: SocketAddr,
    domain: String,
}

impl Server {
    pub async fn connect(
        self,
        client_addr: SocketAddr,
        config: ClientConfig,
    ) -> anyhow::Result<Connection> {
        let mut endpoint = Endpoint::client(client_addr)?;
        endpoint.set_default_client_config(config);
        Ok(endpoint.connect(self.addr, &self.domain)?.await?)
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
        let (addr, domain) = s
            .split_once('#')
            .ok_or(ParseServerInfoError::InvalidSyntax)?;

        let domain = domain.trim().to_string();
        let addr = addr.trim().parse::<SocketAddr>()?;
        Ok(Self { addr, domain })
    }
}

#[cfg(test)]
mod tests {
    use super::{ParseServerInfoError, Server};

    #[test]
    fn parse_server_info_test() {
        let addr = "192.168.1.10:8000";
        let domain = "fuga";
        let si = format!(" {addr} # {domain} ").parse::<Server>().unwrap();
        assert_eq!(si.addr, addr.parse().unwrap());
        assert_eq!(si.domain, domain.to_string());

        let si = format!("hoge # {domain} ").parse::<Server>();
        assert!(matches!(si, Err(ParseServerInfoError::InvalidAddress(_))));

        let si = format!(" {addr}, {domain} ").parse::<Server>();
        assert!(matches!(si, Err(ParseServerInfoError::InvalidSyntax)));
    }
}
