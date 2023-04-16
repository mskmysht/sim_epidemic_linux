use clap::Parser;
use world_repl::{new_runtime_params, new_world_params};

#[derive(clap::Parser)]
struct Args {
    init_n_pop: u32,
    infected: f64,
}

fn main() {
    let Args {
        init_n_pop,
        infected,
    } = Args::parse();

    world_repl::run(new_runtime_params(), new_world_params(init_n_pop, infected));
    println!("stopped");
}
