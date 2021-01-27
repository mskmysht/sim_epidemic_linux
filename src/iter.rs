pub struct MyIter<T> {
    cur: Option<T>,
}

impl<T> MyIter<T> {
    pub fn new(cur: Option<T>) -> MyIter<T> {
        MyIter { cur }
    }
}

pub trait Next<T> {
    fn next(&self) -> Option<T>;
    fn set_next(&mut self, n: Option<T>);
}

pub trait Prev<T> {
    fn prev(&self) -> Option<T>;
}

impl<T: Next<T> + Clone> Iterator for MyIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let cur = &self.cur;
        let mut res = None;
        self.cur = match cur {
            Some(at) => {
                res = cur.clone();
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
                res = cur.clone();
                at.prev()
            }
            _ => None,
        };
        res
    }
}
