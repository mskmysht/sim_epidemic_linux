use std::error;

use clap::Parser;
use repl::Parsable;
use worker_if::realtime::{parse, Request, Response};

struct WorkerParser;
impl Parsable for WorkerParser {
    type Parsed = Request;

    fn parse(input: &str) -> repl::nom::IResult<&str, Self::Parsed> {
        parse::request(input)
    }
}

fn logging(response: &Response) {
    match response {
        Response::Ok(s) => println!("[info] {s:?}"),
        Response::Err(e) => eprintln!("[error] {e:?}"),
    }
}

#[derive(Debug, clap::Parser)]
struct Args {
    /// world binary path
    #[arg(long)]
    world_path: String,
    /// enable async
    #[arg(short = 'a')]
    is_async: bool,
}

fn main() -> Result<(), Box<dyn error::Error>> {
    let Args {
        world_path,
        is_async,
    } = Args::parse();
    let managing = worker::realtime::WorldManaging::new(world_path);
    if is_async {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            loop {
                match WorkerParser::recv_input() {
                    repl::Command::Quit => break,
                    repl::Command::None => {}
                    repl::Command::Delegate(input) => {
                        let manager = managing.get_manager().clone();
                        tokio::spawn(async move {
                            logging(&manager.request(input));
                        });
                    }
                }
            }
        });
    } else {
        loop {
            match WorkerParser::recv_input() {
                repl::Command::Quit => break,
                repl::Command::None => {}
                repl::Command::Delegate(input) => {
                    logging(&managing.get_manager().request(input));
                }
            }
        }
    }
    Ok(())
}
