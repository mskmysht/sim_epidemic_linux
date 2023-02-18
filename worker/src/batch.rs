use std::{collections::BTreeMap, error::Error, io, ops::Deref, process, sync::Arc};

use futures_util::SinkExt;
use ipc_channel::ipc::{self, IpcOneShotServer};
use quinn::Connection;
use tokio::sync::{mpsc, Mutex, MutexGuard};
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec};

use controller_if::ProcessInfo;
use worker_if::batch::{self, world_if};

#[derive(Debug, thiserror::Error)]
pub enum ResponseError {
    #[error("Failed to execute process")]
    FailedToExecute(#[from] ExecuteError),
    #[error("Ipc error has occured: {0}")]
    IpcError(#[from] IpcError),
    #[error("Some error has occured in the child process: {0}")]
    FailedInProcess(#[from] world_if::Error),
    #[error("No id found")]
    NoIdFound,
    // #[error("Abort child process")]
    // Abort(anyhow::Error),
}

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
            let req: batch::Request = protocol::quic::read_data(&mut recv).await.unwrap();
            println!("[request] {req:?}");
            match req {
                batch::Request::Execute(id, param) => {
                    let res: batch::Response<_> = manager.execute(id, param).await.into();
                    println!("[response] {res:?}");
                    protocol::quic::write_data(&mut send, &res).await.unwrap();
                }
                batch::Request::Terminate(id) => {
                    let res: batch::Response<_> = manager.terminate(id).await.into();
                    println!("[response] {res:?}");
                    protocol::quic::write_data(&mut send, &res).await.unwrap();
                }
            };
        });
    }
    Ok(())
}

type BiConnection = world_if::IpcBiConnection<world_if::Request, world_if::Response>;
struct WorldManager {
    world_path: String,
    table: Mutex<BTreeMap<String, BiConnection>>,
    termination_tx: mpsc::UnboundedSender<ProcessInfo>,
}

#[derive(thiserror::Error, Debug)]
pub enum ExecuteError {
    #[error("IO error has occured: {0}")]
    IOError(#[from] io::Error),
    #[error("Failed to connect by IO: {0}")]
    FailedToConnect(#[from] bincode::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum IpcError {
    #[error("send error: {0}")]
    SendError(#[from] bincode::Error),
    #[error("receive error: {0}")]
    RecvError(#[from] ipc::IpcError),
}

fn request(bicon: &BiConnection, req: world_if::Request) -> Result<world_if::Response, IpcError> {
    bicon.send(req)?;
    Ok(bicon.recv()?)
}

impl WorldManager {
    fn new(world_path: String, termination_tx: mpsc::UnboundedSender<ProcessInfo>) -> Self {
        Self {
            world_path,
            termination_tx,
            table: Default::default(),
        }
    }

    async fn execute(
        &self,
        world_id: String,
        param: world_if::JobParam,
    ) -> Result<(), ResponseError> {
        let bicon = self.launch_process(world_id).await?;
        match request(&bicon, world_if::Request::Execute(param))? {
            world_if::Response::Ok(_) => Ok(()),
            world_if::Response::Err(e) => Err(e.into()),
        }
    }

    async fn terminate(&self, world_id: String) -> Result<(), ResponseError> {
        let table = self.table.lock().await;
        let Some(bicon) = table.get(&world_id) else {
            return Err(ResponseError::NoIdFound)
        };
        match request(bicon, world_if::Request::Terminate)? {
            world_if::Response::Ok(_) => Ok(()),
            world_if::Response::Err(e) => Err(e.into()),
        }
    }

    async fn launch_process(
        &self,
        world_id: String,
    ) -> Result<impl Deref<Target = BiConnection> + '_, ExecuteError> {
        let (server, server_name) = IpcOneShotServer::new()?;
        let mut command = process::Command::new(&self.world_path);
        command.args(["--world-id", &world_id, "--server-name", &server_name]);
        let child = shared_child::SharedChild::spawn(&mut command)?;
        let (_, (bicon, stream)): (
            _,
            (BiConnection, world_if::IpcReceiver<world_if::WorldStatus>),
        ) = server.accept()?;
        let guard = self.table.lock().await;
        let bicon = MutexGuard::map(guard, |table| {
            table.entry(world_id.clone()).or_insert(bicon)
        });

        tokio::spawn(async move {
            let mut status_hist = Vec::new();
            while let Ok(status) = stream.recv() {
                status_hist.push(status);
            }
            println!("{:?}", status_hist.last());
        });

        let termination_tx = self.termination_tx.clone();
        tokio::spawn(async move {
            // let pid = child.id();
            let status = child.wait().unwrap();
            termination_tx
                .send(controller_if::ProcessInfo {
                    world_id,
                    exit_status: status.success(),
                })
                .unwrap();
        });
        Ok(bicon)
    }
}
