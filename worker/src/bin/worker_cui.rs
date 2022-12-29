use std::error;

use repl::Parsable;

struct WorkerParser;
impl Parsable for WorkerParser {
    type Parsed = worker_if::Request;

    fn parse(input: &str) -> repl::nom::IResult<&str, Self::Parsed> {
        worker_if::parse::request(input)
    }
}

fn logging(response: &worker_if::Response) {
    match response {
        worker_if::Response::Ok(s) => println!("[info] {s:?}"),
        worker_if::Response::Err(e) => eprintln!("[error] {e:?}"),
    }
}

#[argopt::cmd]
fn main(
    /// world binary path
    #[opt(long)]
    world_path: String,
    /// enable async
    #[opt(short = 'a')]
    is_async: bool,
) -> Result<(), Box<dyn error::Error>> {
    let managing = worker::WorldManaging::new(world_path);
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
