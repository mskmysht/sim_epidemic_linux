use clap::Parser;
use ipc_channel::ipc;
use world::myprocess;
use world_if::realtime;

#[derive(clap::Parser)]
struct Args {
    #[arg(long)]
    server_name: String,
    #[arg(long)]
    world_id: String,
}

fn main() {
    let args = Args::parse();
    let (req_tx, req_rx) = ipc::channel().unwrap();
    let (res_tx, res_rx) = ipc::channel().unwrap();
    let (stream_tx, stream_rx) = ipc::channel().unwrap();
    let tx = ipc::IpcSender::connect(args.server_name).unwrap();
    tx.send(realtime::IpcSubscriber::new(req_tx, res_rx, stream_rx))
        .unwrap();
    let spawner = myprocess::realtime::WorldSpawner::new(
        args.world_id.clone(),
        realtime::IpcPublisher::new(stream_tx, req_rx, res_tx),
    );
    let handle = spawner.spawn().unwrap();
    handle.join().unwrap();
}
