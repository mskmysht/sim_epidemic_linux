use futures_util::{SinkExt, StreamExt};
use quinn::{ClientConfig, RecvStream, SendStream};
use repl::nom::AsBytes;
use std::{error::Error, net::SocketAddr};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use super::{
    server::{MyConnection, ServerInfo},
    TaskId,
};

pub struct WorkerClient {
    connection: MyConnection,
    req_tx: FramedWrite<SendStream, LengthDelimitedCodec>,
    res_rx: FramedRead<RecvStream, LengthDelimitedCodec>,
}

impl WorkerClient {
    pub async fn new(
        addr: SocketAddr,
        config: ClientConfig,
        server_info: ServerInfo,
        index: usize,
        pool_tx: mpsc::Sender<usize>,
        status_tx: mpsc::Sender<(TaskId, bool)>,
    ) -> Result<Self, Box<dyn Error>> {
        let connection = MyConnection::new(
            addr,
            config,
            server_info,
            format!("worker-{index}").to_string(),
        )
        .await?;

        let recv = connection.connection.accept_uni().await?;
        let mut trans = FramedRead::new(recv, LengthDelimitedCodec::new());
        tokio::spawn(async move {
            while let Some(frame) = trans.next().await {
                let data = frame.unwrap();
                let a = bincode::deserialize::<bool>(data.as_bytes()).unwrap();
                if a {
                    pool_tx.send(index).await.unwrap();
                }
            }
        });

        let (send, recv) = connection.connection.open_bi().await?;
        let req_tx = FramedWrite::new(send, LengthDelimitedCodec::new());
        let res_rx = FramedRead::new(recv, LengthDelimitedCodec::new());
        Ok(Self {
            connection,
            req_tx,
            res_rx,
        })
    }

    async fn request(&mut self, req: &worker_if::Request) -> anyhow::Result<worker_if::ResponseOk> {
        self.req_tx.send(bincode::serialize(req)?.into()).await?;
        let res = bincode::deserialize::<worker_if::Response>(
            self.res_rx.next().await.unwrap()?.as_bytes(),
        )?;
        match res {
            worker_if::Response::Ok(ok) => Ok(ok),
            worker_if::Response::Err(e) => Err(anyhow::Error::new(e)),
        }
    }

    pub async fn run(&mut self, task_id: TaskId) {
        todo!()
    }

    pub async fn spawn_item(&mut self) -> anyhow::Result<String> {
        match self.request(&worker_if::Request::SpawnItem).await? {
            worker_if::ResponseOk::Item(id) => Ok(id),
            _ => unreachable!(),
        }
    }
}
