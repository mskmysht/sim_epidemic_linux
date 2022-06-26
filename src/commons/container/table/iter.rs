pub mod parallel;

use std::mem;
use super::T2;


pub(super) struct SingleMut<'a, T: 'a> {
    pub(super) head: &'a mut [T],
    pub(super) mid: &'a mut [Vec<T>],
    pub(super) tail: &'a mut [T],
}

impl<'a, T> SingleMut<'a, T> {
    #[inline]
    pub(super) fn new(head: &'a mut [T], mid: &'a mut [Vec<T>], tail: &'a mut [T]) -> Self {
        Self { head, mid, tail }
    }

    #[inline]
    pub(super) fn empty() -> Self {
        Self::new(Default::default(), &mut [], Default::default())
    }
}

pub struct DoubleMut<'a, T: 'a> {
    pub(super) head: T2<&'a mut [T]>,
    pub(super) mid: &'a mut [Vec<T>],
    pub(super) tail: T2<&'a mut [T]>,
}

impl<'a, T: 'a> DoubleMut<'a, T> {
    #[inline]
    pub(super) fn new(head: T2<&'a mut [T]>, mid: &'a mut [Vec<T>], tail: T2<&'a mut [T]>) -> Self {
        Self { head, mid, tail }
    }

    #[inline]
    pub(super) fn empty() -> Self {
        Self::new(Default::default(), &mut [], Default::default())
    }
}
pub struct HIterMut<'a, T: 'a>(pub(super) SingleMut<'a, T>);

impl<'a, T> Iterator for HIterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        let HIterMut(this) = &mut self;
        if !this.head.is_empty() {
            let tmp = mem::replace(&mut this.head, &mut []);
            let (v, h) = tmp.split_first_mut().unwrap();
            this.head = h;
            Some(v)
        } else if !this.mid.is_empty() {
            let tmp = mem::replace(&mut this.mid, &mut []);
            let (r, m) = tmp.split_first_mut().unwrap();
            let (v, h) = r.split_first_mut().unwrap();
            this.head = h;
            this.mid = m;
            Some(v)
        } else if !this.tail.is_empty() {
            let tmp = mem::replace(&mut this.tail, &mut []);
            let (v, t) = tmp.split_first_mut().unwrap();
            this.tail = t;
            Some(v)
        } else {
            None
        }
    }
}

impl<'a, T: 'a> DoubleEndedIterator for HIterMut<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let HIterMut(this) = &mut self;
        if !this.tail.is_empty() {
            let tmp = mem::replace(&mut this.head, &mut []);
            let (v, t) = tmp.split_last_mut().unwrap();
            this.tail = t;
            Some(v)
        } else if !this.mid.is_empty() {
            let tmp = mem::replace(&mut this.mid, &mut []);
            let (r, m) = tmp.split_last_mut().unwrap();
            let (v, t) = r.split_last_mut().unwrap();
            this.tail = t;
            this.mid = m;
            Some(v)
        } else if !this.head.is_empty() {
            let tmp = mem::replace(&mut this.tail, &mut []);
            let (v, h) = tmp.split_last_mut().unwrap();
            this.head = h;
            Some(v)
        } else {
            None
        }
    }
}

impl<T> ExactSizeIterator for HIterMut<'_, T> {}

// number of rows must be even
pub struct VDPairsMut<'a, T: 'a> {
    pub(super) core: DoubleMut<'a, T>,
    pub(super) column: usize,
    pub(super) offset: T2<usize>,
}

impl<'a, T> VDPairsMut<'a, T> {
    pub(super) fn new(core: DoubleMut<'a, T>, column: usize, offset: T2<usize>) -> Self {
        Self {
            core,
            column,
            offset,
        }
    }
}

impl<'a, T> Iterator for VDPairsMut<'a, T> {
    type Item = (&'a mut T, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        let this = &mut self.core;
        if !this.head.is_empty() {
            let tmp = mem::replace(&mut this.head, T2(&mut [], &mut []));
            let (p, h) = tmp.split_first_mut().unwrap();
            this.head = h;
            Some(p.into())
        } else if !this.mid.is_empty() {
            let m_tmp = mem::replace(&mut this.mid, &mut []);
            let (rs_tmp, m) = m_tmp.split_at_mut(2);
            let r0 = &mut rs_tmp[0][self.offset.0..(self.column - self.offset.1)];
            let r1 = &mut rs_tmp[1][self.offset.1..(self.column - self.offset.0)];
            let (p, h) = T2(&mut r0, &mut r1).split_first_mut().unwrap();
            this.head = h;
            this.mid = m;
            Some(p.into())
        } else if !this.tail.is_empty() {
            let tmp = mem::replace(&mut this.tail, T2(&mut [], &mut []));
            let (p, h) = tmp.split_first_mut().unwrap();
            this.tail = h;
            Some(p.into())
        } else {
            None
        }
    }
}

impl<'a, T> DoubleEndedIterator for VDPairsMut<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let this = &mut self.core;
        if !this.tail.is_empty() {
            let tmp = mem::replace(&mut this.tail, T2(&mut [], &mut []));
            let (p, h) = tmp.split_last_mut().unwrap();
            this.tail = h;
            Some(p.into())
        } else if !this.mid.is_empty() {
            let m_tmp = mem::replace(&mut this.mid, &mut []);
            let (rs_tmp, mid) = m_tmp.split_at_mut(m_tmp.len() - 2);
            let r0 = &mut rs_tmp[0][self.offset.0..(self.column - self.offset.1)];
            let r1 = &mut rs_tmp[1][self.offset.1..(self.column - self.offset.0)];
            let (p, t) = T2(&mut r0, &mut r1).split_last_mut().unwrap();
            this.tail = t;
            this.mid = mid;
            Some(p.into())
        } else if !this.head.is_empty() {
            let tmp = mem::replace(&mut this.head, T2(&mut [], &mut []));
            let (p, h) = tmp.split_last_mut().unwrap();
            this.head = h;
            Some(p.into())
        } else {
            None
        }
    }
}

impl<T> ExactSizeIterator for VDPairsMut<'_, T> {}

pub struct HPairsMut<'a, T: 'a> {
    pub(super) core: SingleMut<'a, T>,
    pub(super) column: usize,
    pub(super) offset: usize,
}

impl<'a, T: 'a> HPairsMut<'a, T> {
    pub(super) fn new(core: SingleMut<'a, T>, column: usize, offset: usize) -> Self {
        Self {
            core,
            column,
            offset,
        }
    }
}

impl<'a, T> Iterator for HPairsMut<'a, T> {
    type Item = (&'a mut T, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        if self.column < self.offset + 2 {
            return None;
        }
        let this = &mut self.core;
        if this.head.len() > 1 {
            let tmp = mem::replace(&mut this.head, &mut []);
            let (ps, h) = tmp.split_at_mut(2);
            this.head = h;
            Some((&mut ps[0], &mut ps[1]))
        } else if !this.mid.is_empty() {
            let tmp = mem::replace(&mut this.mid, &mut []);
            let (r, mid) = tmp.split_first_mut().unwrap();
            let k = (self.column % 2 + self.offset) % 2;
            let (ps, h) = r[self.offset..(self.column - k)].split_at_mut(2);
            this.head = h;
            this.mid = mid;
            Some((&mut ps[0], &mut ps[1]))
        } else if this.tail.len() > 1 {
            let tmp = mem::replace(&mut this.tail, &mut []);
            let (ps, t) = tmp.split_at_mut(2);
            this.tail = t;
            Some((&mut ps[0], &mut ps[1]))
        } else {
            None
        }
    }
}

impl<'a, T> DoubleEndedIterator for HPairsMut<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let col = self.column - self.offset;
        if col < 2 {
            return None;
        }
        let this = &mut self.core;
        if this.tail.len() > 1 {
            let tmp = mem::replace(&mut this.tail, &mut []);
            let (t, ps) = tmp.split_at_mut(tmp.len() - 2);
            this.tail = t;
            Some((&mut ps[0], &mut ps[1]))
        } else if !this.mid.is_empty() {
            let tmp = mem::replace(&mut this.mid, &mut []);
            let (r, mid) = tmp.split_first_mut().unwrap();
            let (t, ps) = r.split_at_mut(self.offset + col - 2 - (col % 2));
            this.mid = mid;
            this.tail = t;
            Some((&mut ps[0], &mut ps[1]))
        } else if this.head.len() > self.offset + 1 {
            let tmp = mem::replace(&mut this.head, &mut []);
            let l = tmp.len();
            let (h, ps) = tmp.split_at_mut(tmp.len() - 2);
            this.head = h;
            Some((&mut ps[0], &mut ps[1]))
        } else {
            None
        }
    }
}

impl<T> ExactSizeIterator for HPairsMut<'_, T> {}

/// immutable iterator
pub(super) struct Single<'a, T: 'a> {
    pub(super) head: &'a [T],
    pub(super) mid: &'a [Vec<T>],
    pub(super) tail: &'a [T],
}

impl<'a, T> Single<'a, T> {
    #[inline]
    pub(super) fn new(head: &'a [T], mid: &'a [Vec<T>], tail: &'a [T]) -> Self {
        Self { head, mid, tail }
    }

    #[inline]
    pub(super) fn empty() -> Self {
        Self::new(Default::default(), &[], Default::default())
    }
}

pub struct HIter<'a, T: 'a>(pub(super) Single<'a, T>);

impl<'a, T> Iterator for HIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let HIter(this) = &mut self;
        if !this.head.is_empty() {
            let tmp = mem::replace(&mut this.head, &[]);
            let (v, h) = tmp.split_first().unwrap();
            this.head = h;
            Some(v)
        } else if !this.mid.is_empty() {
            let tmp = mem::replace(&mut this.mid, &[]);
            let (r, m) = tmp.split_first().unwrap();
            let (v, h) = r.split_first().unwrap();
            this.head = h;
            this.mid = m;
            Some(v)
        } else if !this.tail.is_empty() {
            let tmp = mem::replace(&mut this.tail, &[]);
            let (v, t) = tmp.split_first().unwrap();
            this.tail = t;
            Some(v)
        } else {
            None
        }
    }
}

impl<'a, T: 'a> DoubleEndedIterator for HIter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let HIter(this) = &mut self;
        if !this.tail.is_empty() {
            let tmp = mem::replace(&mut this.head, &[]);
            let (v, t) = tmp.split_last().unwrap();
            this.tail = t;
            Some(v)
        } else if !this.mid.is_empty() {
            let tmp = mem::replace(&mut this.mid, &[]);
            let (r, m) = tmp.split_last().unwrap();
            let (v, t) = r.split_last().unwrap();
            this.tail = t;
            this.mid = m;
            Some(v)
        } else if !this.head.is_empty() {
            let tmp = mem::replace(&mut this.tail, &[]);
            let (v, h) = tmp.split_last().unwrap();
            this.head = h;
            Some(v)
        } else {
            None
        }
    }
}

impl<T> ExactSizeIterator for HIter<'_, T> {}

pub struct Double<'a, T: 'a> {
    pub(super) head: T2<&'a [T]>,
    pub(super) mid: &'a [Vec<T>],
    pub(super) tail: T2<&'a [T]>,
}

impl<'a, T: 'a> Double<'a, T> {
    #[inline]
    pub(super) fn new(head: T2<&'a [T]>, mid: &'a [Vec<T>], tail: T2<&'a [T]>) -> Self {
        Self { head, mid, tail }
    }

    #[inline]
    pub(super) fn empty() -> Self {
        Self::new(Default::default(), &[], Default::default())
    }
}

// number of rows must be even
pub struct VDPairs<'a, T: 'a> {
    core: Double<'a, T>,
    column: usize,
    offset: T2<usize>,
}

impl<'a, T> VDPairs<'a, T> {
    pub(super) fn new(core: Double<'a, T>, column: usize, offset: T2<usize>) -> Self {
        Self {
            core,
            column,
            offset,
        }
    }
}

impl<'a, T> Iterator for VDPairs<'a, T> {
    type Item = (&'a T, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        let this = &mut self.core;
        if !this.head.is_empty() {
            let tmp = mem::replace(&mut this.head, T2(&[], &[]));
            let (p, h) = tmp.split_first().unwrap();
            this.head = h;
            Some(p.into())
        } else if !this.mid.is_empty() {
            let m_tmp = mem::replace(&mut this.mid, &mut []);
            let (rs_tmp, m) = m_tmp.split_at(2);
            let r0 = &rs_tmp[0][self.offset.0..(self.column - self.offset.1)];
            let r1 = &rs_tmp[1][self.offset.1..(self.column - self.offset.0)];
            let (p, h) = T2(r0, r1).split_first().unwrap();
            this.head = h;
            this.mid = m;
            Some(p.into())
        } else if !this.tail.is_empty() {
            let tmp = mem::replace(&mut this.tail, T2(&[], &[]));
            let (p, h) = tmp.split_first().unwrap();
            this.tail = h;
            Some(p.into())
        } else {
            None
        }
    }
}

impl<'a, T> DoubleEndedIterator for VDPairs<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let this = &mut self.core;
        if !this.tail.is_empty() {
            let tmp = mem::replace(&mut this.tail, T2(&[], &[]));
            let (p, h) = tmp.split_last().unwrap();
            this.tail = h;
            Some(p.into())
        } else if !this.mid.is_empty() {
            let m_tmp = mem::replace(&mut this.mid, &[]);
            let (rs_tmp, mid) = m_tmp.split_at(m_tmp.len() - 2);
            let r0 = &rs_tmp[0][self.offset.0..(self.column - self.offset.1)];
            let r1 = &rs_tmp[1][self.offset.1..(self.column - self.offset.0)];
            let (p, t) = T2(r0, r1).split_last().unwrap();
            this.tail = t;
            this.mid = mid;
            Some(p.into())
        } else if !this.head.is_empty() {
            let tmp = mem::replace(&mut this.head, T2(&[], &[]));
            let (p, h) = tmp.split_last().unwrap();
            this.head = h;
            Some(p.into())
        } else {
            None
        }
    }
}

impl<T> ExactSizeIterator for VDPairs<'_, T> {}

pub struct HPairs<'a, T: 'a> {
    core: Single<'a, T>,
    column: usize,
    offset: usize,
}

impl<'a, T: 'a> HPairs<'a, T> {
    pub(super) fn new(core: Single<'a, T>, column: usize, offset: usize) -> Self {
        Self {
            core,
            column,
            offset,
        }
    }
}

impl<'a, T> Iterator for HPairs<'a, T> {
    type Item = (&'a T, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        if self.column < self.offset + 2 {
            return None;
        }
        let this = &mut self.core;
        if this.head.len() > 1 {
            let tmp = mem::replace(&mut this.head, &[]);
            let (ps, h) = tmp.split_at(2);
            this.head = h;
            Some((&mut ps[0], &mut ps[1]))
        } else if !this.mid.is_empty() {
            let tmp = mem::replace(&mut this.mid, &[]);
            let (r, mid) = tmp.split_first().unwrap();
            let k = (self.column % 2 + self.offset) % 2;
            let (ps, h) = r[self.offset..(self.column - k)].split_at(2);
            this.head = h;
            this.mid = mid;
            Some((&mut ps[0], &mut ps[1]))
        } else if this.tail.len() > 1 {
            let tmp = mem::replace(&mut this.tail, &[]);
            let (ps, t) = tmp.split_at(2);
            this.tail = t;
            Some((&ps[0], &ps[1]))
        } else {
            None
        }
    }
}

impl<'a, T> DoubleEndedIterator for HPairs<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let col = self.column - self.offset;
        if col < 2 {
            return None;
        }
        let this = &mut self.core;
        if this.tail.len() > 1 {
            let tmp = mem::replace(&mut this.tail, &[]);
            let (t, ps) = tmp.split_at(tmp.len() - 2);
            this.tail = t;
            Some((&mut ps[0], &mut ps[1]))
        } else if !this.mid.is_empty() {
            let tmp = mem::replace(&mut this.mid, &[]);
            let (r, mid) = tmp.split_first().unwrap();
            let (t, ps) = r.split_at(self.offset + col - 2 - (col % 2));
            this.mid = mid;
            this.tail = t;
            Some((&mut ps[0], &mut ps[1]))
        } else if this.head.len() > self.offset + 1 {
            let tmp = mem::replace(&mut this.head, &[]);
            let l = tmp.len();
            let (h, ps) = tmp.split_at(tmp.len() - 2);
            this.head = h;
            Some((&mut ps[0], &mut ps[1]))
        } else {
            None
        }
    }
}

impl<T> ExactSizeIterator for HPairs<'_, T> {}