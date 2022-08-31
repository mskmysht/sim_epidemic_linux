use std::{
    io::{self, Read, Write},
    net::TcpStream,
};

pub fn read_data(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut header = [0; 8];
    stream.read_exact(&mut header)?;
    let data_len = usize::from_be_bytes(header);
    let mut data = vec![0; data_len];
    let mut buf = [0; 128];
    let mut offset = 0;
    while let Ok(n) = stream.read(&mut buf) {
        for i in 0..n {
            data[i + offset] = buf[i];
        }
        offset += n;
        if offset == data_len {
            break;
        }
    }
    Ok(data)
}

pub fn deserialize<D: for<'a> serde::Deserialize<'a>>(data: &[u8]) -> bincode::Result<D> {
    bincode::deserialize(&data)
}

pub fn serialize<D: serde::Serialize>(value: &D) -> bincode::Result<Vec<u8>> {
    bincode::serialize(&value)
}

pub fn write_data(stream: &mut TcpStream, data: &[u8]) -> io::Result<usize> {
    let header: [u8; 8] = data.len().to_be_bytes();
    stream.write(&header)?;
    stream.write(&data)
}
