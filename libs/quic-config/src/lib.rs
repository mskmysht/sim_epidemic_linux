use quinn::{crypto, ClientConfig, ServerConfig, TransportConfig, VarInt};
use std::{path::Path, sync::Arc, time::Duration};

#[derive(thiserror::Error, Debug)]
pub enum ServerConfigError {
    #[error("Failed to load a certificate file: {0}")]
    CertificateLoadError(std::io::Error),
    #[error("Failed to load a private key: {0}")]
    PrivateKeyLoadError(std::io::Error),
    #[error("TLS error: {0}")]
    TLSError(#[from] crypto::rustls::Error),
}

pub fn get_server_config<P: AsRef<Path>>(
    cert_path: P,
    pkey_path: P,
) -> Result<ServerConfig, ServerConfigError> {
    let cert = rustls::Certificate(
        file_io::load(cert_path).map_err(ServerConfigError::CertificateLoadError)?,
    );
    let key = rustls::PrivateKey(
        file_io::load(pkey_path).map_err(ServerConfigError::PrivateKeyLoadError)?,
    );
    let mut config = ServerConfig::with_single_cert(vec![cert], key)?;
    let mut tc = TransportConfig::default();
    tc.max_idle_timeout(Some(VarInt::from_u32(60_000).into()));
    tc.keep_alive_interval(Some(Duration::from_secs(30)));
    config.transport_config(Arc::new(tc));
    Ok(config)
}

#[derive(thiserror::Error, Debug)]
pub enum ClientConfigError {
    #[error("Failed to load a certificate file: {0}")]
    CertificateLoadError(#[from] std::io::Error),
    #[error("Root certificate error: {0}")]
    RootCertificateError(#[from] anyhow::Error),
}

pub fn get_client_config<P: AsRef<Path>>(cert_path: P) -> Result<ClientConfig, ClientConfigError> {
    let mut certs = rustls::RootCertStore::empty();
    certs
        .add(&rustls::Certificate(file_io::load(cert_path)?))
        .map_err(anyhow::Error::new)?;
    let mut config = ClientConfig::with_root_certificates(certs);
    let mut tc = TransportConfig::default();
    tc.max_idle_timeout(Some(VarInt::from_u32(60_000).into()));
    tc.keep_alive_interval(Some(Duration::from_secs(30)));
    config.transport_config(Arc::new(tc));
    Ok(config)
}
