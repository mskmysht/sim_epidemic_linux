use container::event::{self, stdio};

fn main() {
    stdio::input_handle::<event::MyCallback>().join().unwrap();
}
