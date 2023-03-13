use clap::Parser;
use std::{error::Error, path::Path};

#[derive(Debug, clap::Parser)]
struct Args {
    /// output directory
    #[arg(long, default_value = "./")]
    dir: String,
    /// certificate file name
    #[arg(long, default_value = "ca_cert.der")]
    cert_name: String,
    /// private key name
    #[arg(long, default_value = "ca_pkey.der")]
    pkey_name: String,
    /// domain names
    domains: Vec<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let Args {
        dir,
        cert_name,
        pkey_name,
        domains,
    } = Args::parse();
    let cert = rcgen::generate_simple_self_signed(domains)?;
    let dir = Path::new(&dir);
    file_io::dump(dir.join(cert_name), &cert.serialize_der()?)?;
    file_io::dump(dir.join(pkey_name), &cert.serialize_private_key_der())?;
    println!("Successfully generated a certificate & a private key.");
    Ok(())
}
