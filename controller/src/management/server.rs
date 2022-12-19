use quinn::{ClientConfig, Connection, Endpoint};
use std::{error::Error, net::SocketAddr, str::FromStr};

pub struct MyConnection {
    pub endpoint: Endpoint,
    pub server_name: String,
    pub connection: Connection,
    pub name: String,
}

impl MyConnection {
    pub async fn new(
        addr: SocketAddr,
        config: ClientConfig,
        T2(server_addr, server_name): ServerInfo,
        name: String,
    ) -> Result<Self, Box<dyn Error>> {
        let mut endpoint = Endpoint::client(addr)?;
        endpoint.set_default_client_config(config);
        let connection = endpoint.connect(server_addr, &server_name)?.await?;
        Ok(Self {
            endpoint,
            server_name,
            connection,
            name,
        })
    }

    pub async fn connect(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.connection = self
            .endpoint
            .connect(self.connection.remote_address(), &self.server_name)?
            .await?;
        Ok(())
    }
}

pub type ServerInfo = T2<SocketAddr, String>;

#[derive(Debug, Eq, PartialEq)]
pub struct T2<T, U>(pub T, pub U);

#[derive(Debug, thiserror::Error)]
pub enum ParseTupleError<E0, E1> {
    #[error("invalid syntax of the tuple")]
    SyntaxError,
    #[error("invalid left argment:{0}")]
    LeftError(E0),
    #[error("invalid right argment: {0}")]
    RightError(E1),
}

impl<T: FromStr, U: FromStr> FromStr for T2<T, U> {
    type Err = ParseTupleError<T::Err, U::Err>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (ls, rs) = s
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .and_then(|s| s.split_once(','))
            .ok_or(ParseTupleError::SyntaxError)?;

        let l = ls.trim().parse().map_err(ParseTupleError::LeftError)?;
        let r = rs.trim().parse().map_err(ParseTupleError::RightError)?;
        Ok(T2(l, r))
    }
}

#[cfg(test)]
mod tests {
    use super::T2;

    #[test]
    fn parse_tuple_test() {
        let s = "(hoge,fuga)";
        assert_eq!(
            T2("hoge".to_string(), "fuga".to_string()),
            s.parse().unwrap()
        );

        let s = "( 10  ,  20  )";
        assert_eq!(T2(10, 20), s.parse().unwrap());
    }
}
