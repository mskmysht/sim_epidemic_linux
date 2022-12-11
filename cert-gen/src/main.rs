use std::error::Error;

#[argopt::cmd]
fn main(
    /// path of certificat file
    cert_path: String,
    /// path of private key
    pkey_path: String,
    /// subject alternative names
    #[opt(long)]
    names: Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let cert = rcgen::generate_simple_self_signed(names)?;
    file_io::dump(cert_path, &cert.serialize_der()?)?;
    file_io::dump(pkey_path, &cert.serialize_private_key_der())?;
    println!("Successfully generated a certificate & a private key.");
    Ok(())
}
