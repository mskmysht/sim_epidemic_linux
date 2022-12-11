use quinn::{ClientConfig, ServerConfig, TransportConfig, VarInt};
use std::{error::Error, path::Path, sync::Arc};

pub fn get_server_config<P: AsRef<Path>>(
    cert_path: P,
    pkey_path: P,
    timeout: u32,
) -> Result<ServerConfig, Box<dyn Error>> {
    let cert = rustls::Certificate(file_io::load(cert_path)?);
    let key = rustls::PrivateKey(file_io::load(pkey_path)?);
    let mut config = ServerConfig::with_single_cert(vec![cert], key)?;
    let mut tc = TransportConfig::default();
    tc.max_idle_timeout(Some(VarInt::from_u32(timeout).into()));
    config.transport_config(Arc::new(tc));
    Ok(config)
}

pub fn get_client_config<P: AsRef<Path>>(cert_path: P) -> Result<ClientConfig, Box<dyn Error>> {
    let mut certs = rustls::RootCertStore::empty();
    certs.add(&rustls::Certificate(file_io::load(cert_path)?))?;
    Ok(ClientConfig::with_root_certificates(certs))
}
