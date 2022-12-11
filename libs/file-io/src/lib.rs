use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

pub fn dump<P: AsRef<Path>>(path: P, buf: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(buf)?;
    Ok(())
}
