use ipc_channel::ipc;
use world_if::pubsub;

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
    let tx = ipc::IpcSender::connect(args.server_name).unwrap();
    let id = args.id;
    let spawner = world::WorldSpawner::new(
        id.clone(),
        pubsub::IpcPublisher::new(stream_tx, req_rx, res_tx),
    );
    let handle = spawner.spawn().unwrap();
    tx.send(pubsub::IpcSubscriber::new(req_tx, res_rx, stream_rx))
        .unwrap();
    match handle.join() {
        Ok(_) => println!("<{id}> stopped"),
        Err(e) => eprintln!("<{id}> {e:?}"),
    }
}
