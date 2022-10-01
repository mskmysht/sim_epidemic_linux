use std::io::{self, Read, Write};

pub trait Channel: Read + Write {
    fn send<T: serde::Serialize + for<'a> serde::Deserialize<'a>>(
        &mut self,
        data: &T,
    ) -> io::Result<usize>;
    fn recv<T: serde::Serialize + for<'a> serde::Deserialize<'a>>(&mut self) -> io::Result<T>;
}

impl<S: Read + Write> Channel for S {
    fn send<T: serde::Serialize + for<'a> serde::Deserialize<'a>>(
        &mut self,
        data: &T,
    ) -> io::Result<usize> {
        super::write_data(self, data)
    }

    fn recv<T: serde::Serialize + for<'a> serde::Deserialize<'a>>(&mut self) -> io::Result<T> {
        super::read_data(self)
    }
}

pub trait Callback {
    type Req;
    type Res;
    fn callback(&mut self, req: Self::Req) -> Self::Res;
}

pub fn event_loop<S, M>(stream: &mut S, manager: &mut M)
where
    S: Channel,
    M: Callback,
    M::Req: serde::Serialize + for<'a> serde::Deserialize<'a> + std::fmt::Debug,
    M::Res: serde::Serialize + for<'a> serde::Deserialize<'a> + std::fmt::Debug,
{
    loop {
        let req = match stream.recv() {
            Ok(req) => {
                println!("[request] {req:?}");
                req
            }
            Err(_) => break,
        };
        let res = manager.callback(req);
        println!("[response] {res:?}");
        if let Err(e) = stream.send(&res) {
            println!("[error] {e}");
        }
    }
}
