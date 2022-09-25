use protocol::stdio;
use std::{
    error, io,
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
};

#[argopt::cmd]
fn main(container1: Ipv4Addr, /*, container2: Ipv4Addr */) -> Result<(), Box<dyn error::Error>> {
    if let Ok(stream) = TcpStream::connect(SocketAddrV4::new(container1, 8080)) {
        println!("Connected to the server!");
        stdio::run(MyListener(stream, "container-1"));
    } else {
        println!("Couldn't connect to server...");
    }
    Ok(())
}

struct MyListener<'a>(TcpStream, &'a str);

impl<'a> stdio::InputLoop for MyListener<'a> {
    type Req = container_if::Request<world_if::Request>;
    type Res = io::Result<container_if::Response<world_if::Success, world_if::ErrorStatus>>;

    fn parse(
        input: &str,
    ) -> Result<Self::Req, peg::error::ParseError<<str as peg::Parse>::PositionRepr>> {
        protocol::parse::request(input)
    }

    fn quit(&mut self) {}

    fn logging(res: Self::Res) {
        match res {
            Ok(res) => {
                println!("{res:?}");
            }
            Err(e) => {
                eprintln!("[error] {e:?}");
            }
        }
    }

    fn callback(&mut self, req: Self::Req) -> Self::Res {
        let n = protocol::write_data(&mut self.0, &req)?;
        eprintln!("[info] sent {n} bytes data");
        protocol::read_data(&mut self.0)
    }
}
