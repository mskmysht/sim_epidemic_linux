use std::{
    fs::File,
    io::{self, Read, Write},
};

const CA_CERT_DER_NAME: &'static str = "ca_cert.der";
const CA_PKEY_DER_NAME: &'static str = "ca_pkey.der";

pub fn load_buf(path: &str) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

pub fn load_ca_cert_der() -> io::Result<Vec<u8>> {
    load_buf(CA_CERT_DER_NAME)
}

pub fn load_ca_pkey_der() -> io::Result<Vec<u8>> {
    load_buf(CA_PKEY_DER_NAME)
}

fn dump_buf(path: &str, buf: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(buf)?;
    Ok(())
}

pub fn dump_ca_cert_der(buf: &[u8]) -> io::Result<()> {
    dump_buf(CA_CERT_DER_NAME, buf)
}

pub fn dump_ca_pkey_der(buf: &[u8]) -> io::Result<()> {
    dump_buf(CA_PKEY_DER_NAME, buf)
}
