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
        stdio::input_loop(MyListener(stream));
    } else {
        println!("Couldn't connect to server...");
    }
    Ok(())
}

struct MyListener(TcpStream);

impl stdio::Listener<()> for MyListener {
    type Arg = stdio::Command;

    fn callback(&mut self, arg: Self::Arg) -> ops::ControlFlow<()> {
        let req = match arg {
            stdio::Command::None => return ops::ControlFlow::Continue(()),
            stdio::Command::Quit => return ops::ControlFlow::Break(()),
            stdio::Command::List => Request::List,
            stdio::Command::New => Request::New,
            stdio::Command::Info(id) => Request::Info(id),
            stdio::Command::Delete(id) => Request::Delete(id),
            stdio::Command::Start(_, _) => todo!(),
            stdio::Command::Step(_) => todo!(),
            stdio::Command::Stop(_) => todo!(),
            stdio::Command::Reset(_) => todo!(),
            stdio::Command::Debug(_) => todo!(),
            stdio::Command::Export(_, _) => todo!(),
        };

        match net::write_data(&mut self.0, &net::serialize(&req).unwrap()) {
            Ok(n) => eprintln!("[info] sent {n} bytes data"),
            Err(e) => {
                eprintln!("[error] {e:?}");
                return ops::ControlFlow::Break(());
            }
        }
        match net::read_data(&mut self.0) {
            Ok(data) => match net::deserialize::<socket::Result>(&data) {
                Ok(res) => match res {
                    Ok(res) => println!("{res:?}"),
                    Err(err) => println!("{err:?}"),
                },
                Err(e) => println!("{e:?}"),
            },
            Err(e) => {
                eprintln!("[error] {e:?}");
                return ops::ControlFlow::Break(());
            }
        }

        ops::ControlFlow::Continue(())
    }
}
