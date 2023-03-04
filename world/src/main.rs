use clap::Parser;
use ipc_channel::ipc;
use world::myprocess;
use world_if::batch;

#[derive(clap::Parser)]
struct Args {
    #[arg(long)]
    server_name: String,
    #[arg(long)]
    world_id: String,
}

fn main() {
    let args = Args::parse();
    let (req_tx, req_rx) = ipc::bytes_channel().unwrap();
    let (res_tx, res_rx) = ipc::bytes_channel().unwrap();
    let (stream_tx, stream_rx) = ipc::channel().unwrap();
    let tx = ipc::IpcSender::connect(args.server_name).unwrap();
    tx.send((batch::IpcBiConnection::new(req_tx, res_rx), stream_rx))
        .unwrap();
    let bicon = batch::IpcBiConnection::new(res_tx, req_rx);
    let spawner =
        myprocess::batch::WorldSpawner::new(args.world_id.clone(), bicon, stream_tx).unwrap();
    let handle = spawner.spawn().unwrap();
    handle.join().unwrap();
}
