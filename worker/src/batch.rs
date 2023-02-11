use controller_if::ProcessInfo;
use futures_util::SinkExt;
use ipc_channel::ipc::IpcOneShotServer;
use quinn::Connection;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::{error::Error, process};
use tokio::sync::{mpsc, Mutex};
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec};
use worker_if::batch::world_if::IpcSubscriber;
use worker_if::batch::{Request, Response, ResponseError, ResponseOk};
use worker_if::realtime::world_if::Subscriber;

pub async fn run(
    world_path: String,
    connection: Connection,
    pool_len: usize,
) -> Result<(), Box<dyn Error>> {
    let (pool_tx, mut pool_rx) = mpsc::unbounded_channel();
    for _ in 0..pool_len {
        pool_tx.send(()).unwrap();
    }

    let mut pool_stream =
        FramedWrite::new(connection.open_uni().await?, LengthDelimitedCodec::new());
    tokio::spawn(async move {
        while let Some(_) = pool_rx.recv().await {
            pool_stream
                .send(bincode::serialize(&()).unwrap().into())
                .await
                .unwrap();
        }
    });

    let mut termination_stream =
        FramedWrite::new(connection.open_uni().await?, LengthDelimitedCodec::new());
    // send dummy data to make a peer accept
    termination_stream.send(vec![].into()).await.unwrap();

    let (termination_tx, mut termination_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(info) = termination_rx.recv().await {
            pool_tx.send(()).unwrap();
            termination_stream
                .send(bincode::serialize(&info).unwrap().into())
                .await
                .unwrap();
        }
    });
    let manager = Arc::new(WorldManager::new(world_path, termination_tx));
    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            let req = protocol::quic::read_data(&mut recv).await.unwrap();
            println!("[request] {req:?}");
            let res: Response = match req {
                Request::LaunchItem(id) => match manager.launch_item(id).await {
                    Ok(_) => ResponseOk::Item.into(),
                    Err(e) => ResponseError::FailedToSpawn(e).into(),
                },
                Request::Custom(id, req) => {
                    let table = manager.table.lock().await;
                    match table[&id].request(req) {
                        Ok(r) => r.as_result().into(),
                        Err(e) => ResponseError::process_io_error(e).into(),
                    }
                }
            };
            println!("[response] {res:?}");
            protocol::quic::write_data(&mut send, &res).await.unwrap();
        });
    }
    Ok(())
}

struct WorldManager {
    world_path: String,
    table: Mutex<BTreeMap<String, IpcSubscriber>>,
    termination_tx: mpsc::UnboundedSender<ProcessInfo>,
}

impl WorldManager {
    fn new(world_path: String, termination_tx: mpsc::UnboundedSender<ProcessInfo>) -> Self {
        Self {
            world_path,
            termination_tx,
            table: Default::default(),
        }
    }

    async fn launch_item(&self, world_id: String) -> anyhow::Result<()> {
        let (server, server_name) = IpcOneShotServer::new()?;
        let mut command = process::Command::new(&self.world_path);
        command.args(["--world-id", &world_id, "--server-name", &server_name]);
        let child = shared_child::SharedChild::spawn(&mut command)?;
        let (_, subscriber): (_, IpcSubscriber) = server.accept()?;
        let mut table = self.table.lock().await;
        table.insert(world_id.clone(), subscriber);
        let termination_tx = self.termination_tx.clone();
        tokio::spawn(async move {
            // let pid = child.id();
            let status = child.wait().unwrap();
            termination_tx
                .send(controller_if::ProcessInfo {
                    world_id: world_id,
                    exit_status: status.success(),
                })
                .unwrap();
        });
        Ok(())
    }
}
