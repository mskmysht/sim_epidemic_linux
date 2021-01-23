use std::sync::{Arc, Mutex};

use crate::common_types::MRef;

static ALLOC_UNIT: usize = 2048;

#[derive(Default)]
pub struct DynStruct<T> {
    array: Vec<MRef<T>>,
    pub free: Option<MRef<T>>,
    idx: usize,
}

impl<T> DynStruct<T> {
    pub fn new<F>(&mut self, f: F) -> MRef<T>
    where
        F: Fn() -> T,
    {
        if self.idx == self.array.len() {
            self.array.reserve_exact(ALLOC_UNIT);
            for _ in 0..ALLOC_UNIT - 1 {
                let c = f(); // Default::default();
                self.array.push(Arc::new(Mutex::new(c)));
            }
        }
        let cr = &self.array[self.idx];
        self.idx += 1;
        cr.clone()
    }

    pub fn restore(&mut self, n_opt: &mut Option<MRef<T>>, tr: MRef<T>) {
        *n_opt = self.free.clone();
        self.free = Some(tr.clone());
    }
}
