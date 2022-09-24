use container::interface::{
    socket::{self, Request},
    stdio,
};
use std::{
    error,
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    ops,
};

#[argopt::cmd]
fn main(container1: Ipv4Addr, /*, container2: Ipv4Addr */) -> Result<(), Box<dyn error::Error>> {
    if let Ok(stream) = TcpStream::connect(SocketAddrV4::new(container1, 8080)) {
        println!("Connected to the server!");
        stdio::input_loop(MyListener(stream, "container-1"));
    } else {
        println!("Couldn't connect to server...");
    }
    Ok(())
}

struct MyListener<'a>(TcpStream, &'a str);

impl<'a> stdio::Listener<()> for MyListener<'a> {
    type Arg = stdio::Command;

    fn callback(&mut self, arg: Self::Arg) -> ops::ControlFlow<()> {
        let req = match arg {
            stdio::Command::None => return ops::ControlFlow::Continue(()),
            stdio::Command::Quit => return ops::ControlFlow::Break(()),
            stdio::Command::List => Request::List,
            stdio::Command::New => Request::New,
            stdio::Command::Info(id) => Request::Info(id),
            stdio::Command::Delete(id) => Request::Delete(id),
            stdio::Command::Msg(_, _) => todo!(),
        };

        match comm::write_data(&mut self.0, &req) {
            Ok(n) => eprintln!("[info] sent {n} bytes data"),
            Err(e) => {
                eprintln!("[error] {e:?}");
                return ops::ControlFlow::Break(());
            }
        }
        match comm::read_data::<socket::Response, _>(&mut self.0) {
            Ok(res) => {
                println!("{res:?}");
            }
            Err(e) => {
                eprintln!("[error] {e:?}");
                return ops::ControlFlow::Break(());
            }
        }

        ops::ControlFlow::Continue(())
    }
}
