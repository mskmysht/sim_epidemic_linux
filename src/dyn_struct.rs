use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use crate::commons::MRef;

static ALLOC_UNIT: usize = 2048;

pub trait Reset<T> {
    fn reset(&mut self);
}

#[derive(Default)]
pub struct DynStruct<T> {
    pool: VecDeque<MRef<T>>,
}

impl<T> DynStruct<T> {
    pub fn new(&mut self) -> MRef<T>
    where
        T: Default,
        T: Reset<T>,
    {
        if self.pool.is_empty() {
            for _ in 0..ALLOC_UNIT {
                let c = Default::default();
                self.pool.push_back(Arc::new(Mutex::new(c)));
            }
        }
        let cr = &self.pool.pop_front().unwrap();
        cr.lock().unwrap().reset();
        cr.clone()
    }

    pub fn restore(&mut self, tr: MRef<T>) {
        self.pool.push_back(tr);
    }

    pub fn restore_all(&mut self, trs: &mut VecDeque<MRef<T>>) {
        self.pool.append(trs);
    }
}
