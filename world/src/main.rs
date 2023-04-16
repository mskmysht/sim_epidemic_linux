use clap::Parser;
use ipc_channel::ipc;

#[derive(clap::Parser)]
struct Args {
    #[arg(long)]
    server_name: String,
    #[arg(long)]
    world_id: String,
    #[arg(long)]
    stat_dir: String,
}

fn main() {
    let Args {
        server_name,
        world_id,
        stat_dir,
    } = Args::parse();
    let (req_tx, req_rx) = ipc::bytes_channel().unwrap();
    let (res_tx, res_rx) = ipc::bytes_channel().unwrap();
    let (stream_tx, stream_rx) = ipc::channel().unwrap();
    let tx = ipc::IpcSender::connect(server_name).unwrap();
    tx.send((world_if::IpcBiConnection::new(req_tx, res_rx), stream_rx))
        .unwrap();
    let bicon = world_if::IpcBiConnection::new(res_tx, req_rx);
    let spawner = world::WorldSpawner::new(world_id, bicon, stream_tx, stat_dir).unwrap();
    let handle = spawner.spawn().unwrap();
    if let Err(e) = handle.join().unwrap() {
        tracing::error!("stopped with {e}");
    }
}
