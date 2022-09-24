use container::interface::{socket::event, stdio, WorldManager};
use std::{
    io,
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
};

fn main() {
    let mut pargs = pico_args::Arguments::from_env();
    let world_path = pargs.value_from_str("--world-path").unwrap();
    if pargs.contains("-c") {
        stdio::input_loop(stdio::StdListener::new(world_path));
    } else {
        connect(world_path).unwrap();
    }
}

fn connect(world_path: String) -> io::Result<()> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 8080))?;
    let mut manager = WorldManager::new(world_path);
    loop {
        match listener.accept() {
            Ok((mut stream, addr)) => {
                {
                    let stream = stream.try_clone()?;
                    ctrlc::set_handler(move || {
                        println!("Shutting down a stream ...");
                        stream.shutdown(std::net::Shutdown::Both).unwrap();
                        println!("Done.");
                        std::process::exit(0);
                    })
                    .expect("Error setting Ctrl-C handler");
                }
                println!("[info] Acceept {addr}");
                event::event_loop(&mut stream, &mut manager);
                println!("[info] Disconnect {addr}");
            }
            Err(e) => println!("{e:?}"),
        }
    }
}
