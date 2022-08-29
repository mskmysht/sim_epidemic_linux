use sim_epidemic_linux::control::{self, stdio};

fn main() {
    stdio::input_handle::<control::MyCallback>().join().unwrap();
}
