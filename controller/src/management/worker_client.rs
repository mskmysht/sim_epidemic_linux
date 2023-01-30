use futures_util::{SinkExt, StreamExt};
use quinn::ClientConfig;
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

#[derive(Debug)]
pub struct WorkerClient {
    connection: MyConnection,
}

impl WorkerClient {
    pub async fn new(
        addr: SocketAddr,
        config: ClientConfig,
        server_info: ServerInfo,
        index: usize,
        pool_tx: async_channel::Sender<usize>,
        termination_tx: mpsc::UnboundedSender<(String, bool)>,
    ) -> Result<Self, Box<dyn Error>> {
        let connection = MyConnection::new(
            addr,
            config,
            server_info,
            format!("worker-{index}").to_string(),
        )
        .await?;

        let mut pool_stream = FramedRead::new(
            connection.connection.accept_uni().await?,
            LengthDelimitedCodec::new(),
        );
        println!("[info] accepted a worker pool stream");

        tokio::spawn(async move {
            while let Some(frame) = pool_stream.next().await {
                let data = frame.unwrap();
                bincode::deserialize::<()>(data.as_bytes()).unwrap();
                pool_tx.send(index).await.unwrap();
            }
        });

        let mut termination_stream = FramedRead::new(
            connection.connection.accept_uni().await?,
            LengthDelimitedCodec::new(),
        );
        // dump dummy data
        termination_stream.next().await;
        println!("[info] accepted a worker termination stream");
        tokio::spawn(async move {
            while let Some(frame) = termination_stream.next().await {
                let data = frame.unwrap();
                let r = bincode::deserialize::<(String, bool)>(data.as_bytes()).unwrap();
                termination_tx.send(r).unwrap();
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

    pub async fn execute(&mut self, task_id: &TaskId, config: job::Config) -> anyhow::Result<()> {
        self.request(&Request::LaunchItem(task_id.clone())).await?;
        self.request(&Request::Custom(
            task_id.clone(),
            world_if::Request::Execute(config.param.stop_at),
        ))
        .await?;
        Ok(())
    }

    pub async fn terminate(&mut self, task_id: &TaskId) -> anyhow::Result<()> {
        self.request(&Request::Custom(
            task_id.clone(),
            world_if::Request::Terminate,
        ))
        .await?;
        Ok(())
    }
}
