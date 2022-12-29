pub use nom;
use nom::{Finish, IResult};
use std::{
    fmt::Debug,
    io::{self, Write},
};

#[derive(Debug)]
pub enum Command<T> {
    Quit,
    None,
    Delegate(T),
}

pub trait Parsable {
    type Parsed;
    fn parse(input: &str) -> IResult<&str, Self::Parsed>;
    fn recv_input() -> Command<Self::Parsed> {
        loop {
            let mut buf = String::new();
            io::stdout().flush().unwrap();
            print!("> ");
            io::stdout().flush().unwrap();
            io::stdin().read_line(&mut buf).unwrap();
            match Self::command(&buf).finish() {
                Ok((_, cmd)) => {
                    break cmd;
                }
                Err(e) => println!("{e}"),
            }
        }
    }
    fn command(input: &str) -> IResult<&str, Command<Self::Parsed>> {
        use nom::{
            branch::alt,
            character::complete::{multispace0, space0},
            combinator::{all_consuming, map},
            sequence::delimited,
        };
        use parser::nullary;

        all_consuming(delimited(
            multispace0,
            alt((
                nullary(":q", || Command::Quit),
                map(Self::parse, |v| Command::Delegate(v)),
                map(space0, |_| Command::None),
            )),
            multispace0,
        ))(input)
    }
}

// pub trait Logging {
//     type Arg;
//     fn logging(arg: Self::Arg);
// }

// pub trait Handler {
//     type Input;
//     type Output;
//     fn callback(&mut self, input: Self::Input) -> Self::Output;
// }

// struct Repl<R: Handler> {
//     runtime: R,
// }

// impl<R> Repl<R>
// where
//     R: Handler + Parsable<Parsed = R::Input> + Logging<Arg = R::Output>,
//     R::Input: Debug,
// {
//     pub fn new(runtime: R) -> Self {
//         Self { runtime }
//     }

//     pub fn run(mut self) {
//         loop {
//             match R::recv_input() {
//                 Command::Quit => break,
//                 Command::None => {}
//                 Command::Delegate(input) => {
//                     let output = self.runtime.callback(input);
//                     R::logging(output);
//                 }
//             }
//         }
//     }
// }

// #[async_trait]
// pub trait AsyncHandler {
//     type Input;
//     type Output;
//     async fn callback(&mut self, input: Self::Input) -> Self::Output;
// }

// struct AsyncRepl<R>
// where
//     R: AsyncHandler,
// {
//     runtime: Arc<Mutex<R>>,
// }

// impl<R> AsyncRepl<R>
// where
//     R: AsyncHandler + Parsable<Parsed = R::Input> + Logging<Arg = R::Output> + Send + 'static,
//     R::Input: Send + Debug,
// {
//     pub fn new(runtime: R) -> Self {
//         Self {
//             runtime: Arc::new(Mutex::new(runtime)),
//         }
//     }

//     pub async fn run(self) {
//         loop {
//             match R::recv_input() {
//                 Command::Quit => break,
//                 Command::None => {}
//                 Command::Delegate(input) => {
//                     let runtime = Arc::clone(&self.runtime);
//                     tokio::spawn(async move {
//                         let output = R::callback(runtime.lock().await.borrow_mut(), input).await;
//                         R::logging(output);
//                     });
//                 }
//             }
//         }
//     }
// }
