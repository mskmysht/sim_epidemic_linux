use std::{
    collections::BTreeMap,
    error::Error,
    io::{self},
    process,
    sync::Arc,
};

use futures_util::SinkExt;
use ipc_channel::ipc::{self, IpcOneShotServer};
use parking_lot::Mutex;
use quinn::Connection;
use serde::{Deserialize, Serialize};
use shared_child::SharedChild;
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec};

use worker_if::batch::{self, world_if, Resource};

#[derive(Debug, thiserror::Error)]
pub enum ResponseError {
    #[error("Failed to execute process")]
    FailedToExecute(#[from] IpcServerConnectionError),
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
    max_resource: usize,
) -> Result<(), Box<dyn Error>> {
    let mut send = connection.open_uni().await?;
    protocol::quic::write_data(&mut send, &Resource(max_resource)).await?;

    let manager = Arc::new(WorldManager::new(world_path));

    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            let req: batch::Request = protocol::quic::read_data(&mut recv).await.unwrap();
            println!("[request] {req:?}");
            match req {
                batch::Request::Execute(id, param) => {
                    match manager.execute(id, param).await {
                        Ok(child) => {
                            let res = batch::Response::<()>::from_ok(());
                            println!("[response] {res:?}");
                            let mut stream = FramedWrite::new(send, LengthDelimitedCodec::new());
                            stream
                                .send(bincode::serialize(&res).unwrap().into())
                                .await
                                .unwrap();
                            tokio::spawn(async move {
                                // let pid = child.id();
                                let status = child.wait().unwrap();
                                stream
                                    .send(bincode::serialize(&status.success()).unwrap().into())
                                    .await
                                    .unwrap();
                            });
                        }
                        Err(e) => {
                            let res = batch::Response::<()>::from_err(e);
                            println!("[response] {res:?}");
                            protocol::quic::write_data(&mut send, &res).await.unwrap();
                        }
                    }
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
}

#[derive(thiserror::Error, Debug)]
pub enum IpcServerConnectionError {
    #[error("Failed to create IPC server: {0}")]
    IpcServer(io::Error),
    #[error("Failed to spawn child: {0}")]
    ChildProcess(io::Error),
    #[error("Failed to connect by IO: {0}")]
    FailedToConnect(bincode::Error),
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
    fn new(world_path: String) -> Self {
        Self {
            world_path,
            table: Default::default(),
        }
    }

    async fn execute(
        &self,
        world_id: String,
        param: world_if::JobParam,
    ) -> Result<SharedChild, ResponseError> {
        let ((bicon, stream), child) = self
            .connect_ipc_server::<(BiConnection, world_if::IpcReceiver<world_if::WorldStatus>)>(
                &world_id,
            )?;
        let mut table = self.table.lock();
        let bicon = table.entry(world_id.clone()).or_insert(bicon);

        tokio::spawn(async move {
            let mut status_hist = Vec::new();
            while let Ok(status) = stream.recv() {
                status_hist.push(status);
            }
            println!("{:?}", status_hist.last());
        });

        match request(&bicon, world_if::Request::Execute(param))? {
            world_if::Response::Ok(_) => Ok(child),
            world_if::Response::Err(e) => Err(e.into()),
        }
    }

    async fn terminate(&self, world_id: String) -> Result<(), ResponseError> {
        let table = self.table.lock();
        let Some(bicon) = table.get(&world_id) else {
            return Err(ResponseError::NoIdFound)
        };
        match request(bicon, world_if::Request::Terminate)? {
            world_if::Response::Ok(_) => Ok(()),
            world_if::Response::Err(e) => Err(e.into()),
        }
    }

    fn connect_ipc_server<T: for<'de> Deserialize<'de> + Serialize>(
        &self,
        world_id: &str,
    ) -> Result<(T, SharedChild), IpcServerConnectionError> {
        let (server, server_name) =
            IpcOneShotServer::<T>::new().map_err(IpcServerConnectionError::IpcServer)?;
        let mut command = process::Command::new(&self.world_path);
        command.args(["--world-id", world_id, "--server-name", &server_name]);
        let child = shared_child::SharedChild::spawn(&mut command)
            .map_err(IpcServerConnectionError::ChildProcess)?;
        let (_, value) = server
            .accept()
            .map_err(IpcServerConnectionError::FailedToConnect)?;
        Ok((value, child))
    }
}
