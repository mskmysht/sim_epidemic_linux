use ipc_channel::ipc;
use world::myprocess;
use world_if::batch;

#[argopt::cmd]
fn main(#[opt(long)] world_id: String, #[opt(long)] server_name: String) {
    let (req_tx, req_rx) = ipc::channel().unwrap();
    let (res_tx, res_rx) = ipc::channel().unwrap();
    let (stream_tx, stream_rx) = ipc::channel().unwrap();
    let tx = ipc::IpcSender::connect(server_name).unwrap();
    tx.send((batch::IpcBiConnection::new(req_tx, res_rx), stream_rx))
        .unwrap();
    let spawner = myprocess::batch::WorldSpawner::new(
        world_id.clone(),
        batch::IpcBiConnection::new(res_tx, req_rx),
        stream_tx,
    )
    .unwrap();
    let handle = spawner.spawn().unwrap();
    handle.join().unwrap();
    println!("<{world_id}> stopped");
}
