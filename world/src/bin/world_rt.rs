use ipc_channel::ipc;
use world::myprocess;
use world_if::realtime;

#[argopt::cmd]
fn main(#[opt(long)] world_id: String, #[opt(long)] server_name: String) {
    let (req_tx, req_rx) = ipc::channel().unwrap();
    let (res_tx, res_rx) = ipc::channel().unwrap();
    let (stream_tx, stream_rx) = ipc::channel().unwrap();
    let tx = ipc::IpcSender::connect(server_name).unwrap();
    tx.send(realtime::IpcSubscriber::new(req_tx, res_rx, stream_rx))
        .unwrap();
    let spawner = myprocess::realtime::WorldSpawner::new(
        world_id.clone(),
        realtime::IpcPublisher::new(stream_tx, req_rx, res_tx),
    );
    let handle = spawner.spawn().unwrap();
    handle.join().unwrap();
    println!("<{world_id}> stopped");
}
