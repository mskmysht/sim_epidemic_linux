use bytes::Buf;
use quinn::{Connection, ReadExactError, RecvStream, SendStream, WriteError};
use serde::{Deserialize, Serialize};

pub async fn write_data<T>(send: &mut SendStream, value: &T) -> Result<usize, WriteError>
where
    T: serde::Serialize,
{
    let data = super::serialize(value).expect("Failed to serialize");
    send.write_all(&data.len().to_ne_bytes()).await?;
    send.write_all(&data).await?;
    send.finish().await?;
    Ok(data.len())
}

pub async fn read_data<T>(reader: &mut RecvStream) -> Result<T, ReadExactError>
where
    T: for<'a> serde::Deserialize<'a>,
{
    let mut header = [0; 8];
    reader.read_exact(&mut header).await?;
    let data_len = usize::from_ne_bytes(header);
    let mut data = vec![0; data_len];

    while let Some(mut chunk) = reader.read_chunk(512, true).await? {
        chunk
            .bytes
            .copy_to_slice(&mut data[(chunk.offset as usize - 8)..]);
    }

    Ok(super::deserialize(&data).expect("Failed to deserialize"))
}

pub async fn request<T, U>(connection: &Connection, data: T) -> anyhow::Result<U>
where
    T: Serialize,
    U: for<'de> Deserialize<'de>,
{
    let (mut send, mut recv) = connection.open_bi().await?;
    write_data(&mut send, &data).await?;
    Ok(read_data(&mut recv).await?)
}
