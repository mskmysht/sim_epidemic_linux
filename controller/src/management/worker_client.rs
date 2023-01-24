use futures_util::{SinkExt, StreamExt};
use quinn::{ClientConfig, SendStream};
use repl::nom::AsBytes;
use std::{error::Error, net::SocketAddr};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use worker_if::batch::{world_if, Request, Response, ResponseOk};

use crate::api_server::job;

use super::{
    server::{MyConnection, ServerInfo},
    TaskId,
};

pub struct WorkerClient {
    connection: MyConnection,
}

impl WorkerClient {
    pub async fn new(
        addr: SocketAddr,
        config: ClientConfig,
        server_info: ServerInfo,
        index: usize,
        pool_tx: mpsc::Sender<usize>,
    ) -> Result<Self, Box<dyn Error>> {
        let connection = MyConnection::new(
            addr,
            config,
            server_info,
            format!("worker-{index}").to_string(),
        )
        .await?;

        let signal = connection.connection.accept_uni().await?;
        let mut signal = FramedRead::new(signal, LengthDelimitedCodec::new());

        tokio::spawn(async move {
            while let Some(frame) = signal.next().await {
                let data = frame.unwrap();
                bincode::deserialize::<()>(data.as_bytes()).unwrap();
                pool_tx.send(index).await.unwrap();
            }
        });

        Ok(Self { connection })
    }

    async fn request(&mut self, req: &Request) -> anyhow::Result<ResponseOk> {
        let (send, recv) = self.connection.connection.open_bi().await?;
        let mut req_tx = FramedWrite::new(send, LengthDelimitedCodec::new());
        let mut res_rx = FramedRead::new(recv, LengthDelimitedCodec::new());

        req_tx.send(bincode::serialize(req)?.into()).await?;
        let res = bincode::deserialize::<Response>(res_rx.next().await.unwrap()?.as_bytes())?;
        match res {
            Response::Ok(ok) => Ok(ok),
            Response::Err(e) => Err(anyhow::Error::new(e)),
        }
    }

    pub async fn run(&mut self, task_id: &TaskId, config: &job::Config) -> anyhow::Result<bool> {
        self.request(&Request::LaunchItem(task_id.clone())).await?;
        match self
            .request(&Request::Custom(
                task_id.clone(),
                world_if::Request::Execute(config.param.stop_at),
            ))
            .await?
        {
            ResponseOk::Custom(_) => Ok(true),
            _ => unreachable!(),
        }
    }
}
