use std::sync::{Arc, Mutex};

pub struct MyIter<T> {
    cur: Option<Arc<Mutex<T>>>,
}

impl<T> MyIter<T> {
    pub fn new(cur: Option<Arc<Mutex<T>>>) -> MyIter<T> {
        MyIter { cur }
    }
}

pub trait Next<T> {
    fn n(&self) -> Option<Arc<Mutex<T>>>;
}

pub trait Prev<T> {
    fn p(&self) -> Option<Arc<Mutex<T>>>;
}

impl<T: Next<T>> Iterator for MyIter<T> {
    type Item = Arc<Mutex<T>>;

    fn next(&mut self) -> Option<Self::Item> {
        let cur = &self.cur;
        let mut res = None;
        self.cur = match cur {
            Some(at) => {
                res = Some(at.clone());
                at.lock().unwrap().n()
            }
            _ => None,
        };
        res
    }
}

impl<T: Prev<T> + Next<T>> DoubleEndedIterator for MyIter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let cur = &self.cur;
        let mut res = None;
        self.cur = match cur {
            Some(at) => {
                res = Some(at.clone());
                at.lock().unwrap().p()
            }
            _ => None,
        };
        res
    }
}
