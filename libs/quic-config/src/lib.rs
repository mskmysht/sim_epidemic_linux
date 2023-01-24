use quinn::{ClientConfig, ServerConfig, TransportConfig};
use std::{error::Error, path::Path, sync::Arc, time::Duration};

pub fn get_server_config<P: AsRef<Path>>(
    cert_path: P,
    pkey_path: P,
    timeout: u32,
) -> Result<ServerConfig, Box<dyn Error>> {
    let cert = rustls::Certificate(file_io::load(cert_path)?);
    let key = rustls::PrivateKey(file_io::load(pkey_path)?);
    let mut config = ServerConfig::with_single_cert(vec![cert], key)?;
    let mut tc = TransportConfig::default();
    tc.max_idle_timeout(Some(Duration::from_secs(60).try_into()?));
    tc.keep_alive_interval(Some(Duration::from_secs(30).try_into()?));
    config.transport_config(Arc::new(tc));
    Ok(config)
}

pub fn get_client_config<P: AsRef<Path>>(cert_path: P) -> Result<ClientConfig, Box<dyn Error>> {
    let mut certs = rustls::RootCertStore::empty();
    certs.add(&rustls::Certificate(file_io::load(cert_path)?))?;
    let mut config = ClientConfig::with_root_certificates(certs);
    let mut tc = TransportConfig::default();
    tc.max_idle_timeout(Some(Duration::from_secs(60).try_into()?));
    tc.keep_alive_interval(Some(Duration::from_secs(30).try_into()?));
    config.transport_config(Arc::new(tc));
    Ok(config)
}
