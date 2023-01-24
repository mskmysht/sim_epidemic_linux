pub trait Publisher {
    type Req;
    type Res;
    type Stat;
    type SendErr<T>;
    type RecvErr;

    fn recv(&self) -> Result<Self::Req, Self::RecvErr>;
    fn try_recv(&self) -> Result<Option<Self::Req>, Self::RecvErr>;
    fn send_response(&self, data: Self::Res) -> Result<(), Self::SendErr<Self::Res>>;
    fn send_on_stream(&self, data: Self::Stat) -> Result<(), Self::SendErr<Self::Stat>>;
}

#[derive(thiserror::Error, Debug)]
pub enum RequestError<R, S> {
    #[error("receive error")]
    RecvError(R),
    #[error("send error")]
    SendError(S),
}

pub trait Subscriber {
    type Req;
    type Res;
    type Stat;
    type RecvErr;
    type SendErr;

    fn recv_status(&self) -> Result<Self::Stat, Self::RecvErr>;
    fn try_recv_status(&self) -> Result<Option<Self::Stat>, Self::RecvErr>;

    fn send(&self, req: Self::Req) -> Result<(), Self::SendErr>;
    fn recv(&self) -> Result<Self::Res, Self::RecvErr>;

    fn seek_status(&self) -> Vec<Self::Stat> {
        let mut v = Vec::new();
        while let Ok(Some(s)) = self.try_recv_status() {
            v.push(s);
        }
        v
    }

    fn request(
        &self,
        req: Self::Req,
    ) -> Result<Self::Res, RequestError<Self::RecvErr, Self::SendErr>> {
        self.send(req).map_err(RequestError::SendError)?;
        self.recv().map_err(RequestError::RecvError)
    }
}
