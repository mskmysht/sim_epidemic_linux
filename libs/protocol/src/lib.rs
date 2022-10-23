pub mod parse;
pub mod quic;

use std::io;

pub fn deserialize<D: for<'a> serde::Deserialize<'a>>(data: &[u8]) -> bincode::Result<D> {
    bincode::deserialize(&data)
}

pub fn serialize<D: serde::Serialize>(value: &D) -> bincode::Result<Vec<u8>> {
    bincode::serialize(&value)
}

pub fn write_data<T, W>(writer: &mut W, value: &T) -> io::Result<usize>
where
    T: serde::Serialize,
    W: io::Write,
{
    let data = serialize(value).expect("Failed to serialize");
    writer.write(&data.len().to_ne_bytes())?;
    writer.write(&data)
}

pub fn read_data<T, R>(reader: &mut R) -> io::Result<T>
where
    T: for<'a> serde::Deserialize<'a>,
    R: io::Read,
{
    let mut header = [0; 8];
    reader.read_exact(&mut header)?;
    let data_len = usize::from_ne_bytes(header);
    let mut data = vec![0; data_len];
    let mut buf = [0; 512];
    let mut offset = 0;
    if data_len == 0 {
        return Err(io::Error::from(io::ErrorKind::WriteZero));
    }
    while let Ok(n) = reader.read(&mut buf) {
        for i in 0..n {
            data[i] = buf[offset + i];
        }
        offset += n;
        if offset == data_len {
            break;
        }
    }
    Ok(deserialize(&data).expect("Failed to deserialize"))
}
