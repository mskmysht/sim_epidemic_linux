use container::interface::{socket::event, WorldManager};
use std::{
    error,
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
};

fn main() {
    // stdio::input_loop(stdio::MyListner::new());
    connect().unwrap();
}

fn connect() -> Result<(), Box<dyn error::Error>> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 8080))?;
    let mut manager = WorldManager::new();
    loop {
        match listener.accept() {
            Ok((mut stream, addr)) => {
                println!("[info] Acceept {addr}");
                event::event_loop(&mut stream, &mut manager);
                println!("[info] Disconnect {addr}");
                stream.shutdown(std::net::Shutdown::Both)?;
            }
            Err(e) => println!("{e:?}"),
        }
    }
}
