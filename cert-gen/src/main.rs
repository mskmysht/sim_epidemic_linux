use clap::Parser;
use std::error::Error;

#[derive(Debug, clap::Parser)]
struct Args {
    /// path of certificat file
    cert_path: String,
    /// path of private key
    pkey_path: String,
    /// subject alternative names
    #[arg(long)]
    names: Vec<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let Args {
        cert_path,
        pkey_path,
        names,
    } = Args::parse();
    let cert = rcgen::generate_simple_self_signed(names)?;
    file_io::dump(cert_path, &cert.serialize_der()?)?;
    file_io::dump(pkey_path, &cert.serialize_private_key_der())?;
    println!("Successfully generated a certificate & a private key.");
    Ok(())
}
