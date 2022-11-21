use ipc_channel::ipc::{self, IpcSender};

struct MyArgs {
    id: String,
    server_name: String,
}

fn parse_args() -> Result<MyArgs, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();
    Ok(MyArgs {
        id: pargs.value_from_str("--world-id")?,
        server_name: pargs.value_from_str("--server-name")?,
    })
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let (req_tx, req_rx) = ipc::channel().unwrap();
    let (res_tx, res_rx) = ipc::channel().unwrap();
    let (stream_tx, stream_rx) = ipc::channel().unwrap();
    let tx = IpcSender::connect(args.server_name).unwrap();
    let (handle, status) = world::WorldSpawner::spawn(
        args.id,
        world::IpcSpawnerChannel::new(stream_tx, req_rx, res_tx),
    )
    .unwrap();
    tx.send(world_if::WorldInfo::new(req_tx, res_rx, stream_rx, status))
        .unwrap();
    let id = match handle.join() {
        Ok(id) => id,
        Err(e) => {
            eprintln!("{e:?}");
            return;
        }
    };
    println!("[info] Delete world {id}.");
}
