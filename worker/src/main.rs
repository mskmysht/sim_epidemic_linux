use futures_util::{SinkExt, StreamExt};
use ipc_channel::ipc::IpcOneShotServer;
use quinn::{Connection, Endpoint};
use repl::nom::AsBytes;
use std::collections::HashMap;
use std::sync::Arc;
use std::{error::Error, net::SocketAddr, process};
use tokio::sync::{mpsc, Mutex};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use worker_if::batch::world_if::IpcSubscriber;
use worker_if::batch::{Request, Response, ResponseError, ResponseOk};
use worker_if::realtime::world_if::Subscriber;

#[argopt::cmd]
fn main(
    /// path of certificate file
    #[opt(long)]
    cert_path: String,
    /// path of private key file
    #[opt(long)]
    pkey_path: String,
    /// world binary path
    #[opt(long)]
    world_path: String,
    /// address to listen
    #[opt(long)]
    addr: SocketAddr,
) -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    let endpoint = Endpoint::server(quic_config::get_server_config(cert_path, pkey_path)?, addr)?;
    rt.block_on(async {
        while let Some(connecting) = endpoint.accept().await {
            let connection = connecting.await.unwrap();
            let ip = connection.remote_address().to_string();
            println!("[info] Acceept {}", ip);
            if let Err(e) = run(world_path.clone(), connection).await {
                println!("[info] Disconnect {} ({})", ip, e);
            }
        }
    });
    Ok(())
}

async fn run(world_path: String, connection: Connection) -> Result<(), Box<dyn Error>> {
    let signal = connection.open_uni().await?;
    let mut signal = FramedWrite::new(signal, LengthDelimitedCodec::new());
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let available_item_count = 10;

    for _ in 0..available_item_count {
        tx.send(()).await.unwrap();
    }

    tokio::spawn(async move {
        while let Some(_) = rx.recv().await {
            signal
                .send(bincode::serialize(&()).unwrap().into())
                .await
                .unwrap();
        }
    });

    let table = Arc::new(Mutex::new(HashMap::new()));
    loop {
        let (send, recv) = connection.accept_bi().await?;
        let world_path = world_path.clone();
        let table = Arc::clone(&table);
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut res_tx = FramedWrite::new(send, LengthDelimitedCodec::new());
            let mut req_rx = FramedRead::new(recv, LengthDelimitedCodec::new());

            let data = req_rx.next().await.unwrap().unwrap();
            let req: Request = bincode::deserialize(data.as_bytes()).unwrap();
            println!("[request] {req:?}");

            let res: Response = match req {
                Request::LaunchItem(id) => match launch_item(world_path, id, table, tx).await {
                    Ok(_) => ResponseOk::Item.into(),
                    Err(e) => ResponseError::FailedToSpawn(e).into(),
                },
                Request::Custom(id, req) => {
                    let table = table.lock().await;
                    match table[&id].request(req) {
                        Ok(r) => r.as_result().into(),
                        Err(e) => ResponseError::process_io_error(e).into(),
                    }
                }
            };
            println!("[response] {res:?}");

            res_tx
                .send(bincode::serialize(&res).unwrap().into())
                .await
                .unwrap();
        });
    }
}

async fn launch_item(
    world_path: String,
    id: String,
    table: Arc<Mutex<HashMap<String, IpcSubscriber>>>,
    tx: mpsc::Sender<()>,
) -> anyhow::Result<()> {
    let (server, server_name) = IpcOneShotServer::new()?;
    let mut command = process::Command::new(&world_path);
    command.args(["--world-id", &id, "--server-name", &server_name]);
    let child = shared_child::SharedChild::spawn(&mut command)?;
    let (_, subscriber): (_, IpcSubscriber) = server.accept()?;
    let mut table = table.lock().await;
    table.insert(id, subscriber);
    tokio::spawn(async move {
        // let pid = child.id();
        child.wait().unwrap();
        tx.send(()).await.unwrap();
    });
    Ok(())
}
