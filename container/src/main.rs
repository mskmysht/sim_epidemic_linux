use container::{stdio::StdListener, world::WorldManager};
use protocol::stdio;
use std::{
    io,
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
};

fn main() {
    let mut pargs = pico_args::Arguments::from_env();
    let world_path = pargs.value_from_str("--world-path").unwrap();
    if pargs.contains("-c") {
        stdio::run(StdListener::new(world_path));
    } else {
        if let Err(e) = connect(world_path) {
            eprintln!("{e:?}");
        }
    }
}

fn connect(world_path: String) -> io::Result<()> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 8080))?;
    let mut manager = WorldManager::new(world_path);
    for stream in listener.incoming() {
        let mut stream = stream?;
        let addr = stream.peer_addr()?;
        println!("[info] Acceept {addr}");
        protocol::channel::event_loop(&mut stream, &mut manager);
        println!("[info] Disconnect {addr}");
    }
    loop {}
}
