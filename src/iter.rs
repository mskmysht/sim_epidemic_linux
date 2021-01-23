// use crate::common_types::MRef;

pub struct MyIter<T> {
    // cur: Option<MRef<T>>,
    cur: Option<T>,
}

impl<T> MyIter<T> {
    // pub fn new(cur: Option<MRef<T>>) -> MyIter<T> {
    pub fn new(cur: Option<T>) -> MyIter<T> {
        MyIter { cur }
    }
}

pub trait Next<T> {
    // fn n(&self) -> Option<MRef<T>>;
    fn next(&self) -> Option<T>;
}

pub trait Prev<T> {
    // fn p(&self) -> Option<MRef<T>>;
    fn prev(&self) -> Option<T>;
}

impl<T: Next<T> + Clone> Iterator for MyIter<T> {
    // type Item = MRef<T>;
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let cur = &self.cur;
        let mut res = None;
        self.cur = match cur {
            Some(at) => {
                res = cur.clone(); // Some(*at.clone());
                                   // at.lock().unwrap().n()
                at.next()
            }
            _ => None,
        };
        res
    }
}

impl<T: Prev<T> + Next<T> + Clone> DoubleEndedIterator for MyIter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let cur = &self.cur;
        let mut res = None;
        self.cur = match cur {
            Some(at) => {
                res = cur.clone(); // Some(at.clone());
                                   // at.lock().unwrap().p()
                at.prev()
            }
            _ => None,
        };
        res
    }
}
