use super::T2;
use std::{fmt::Debug, marker::PhantomData, mem};

pub struct SingleMut<'a, T: 'a> {
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

    fn hiter_mut_next(&mut self) -> Option<&'a mut T> {
        if !self.head.is_empty() {
            let tmp = mem::take(&mut self.head);
            let (v, h) = tmp.split_first_mut().unwrap();
            self.head = h;
            Some(v)
        } else if !self.mid.is_empty() {
            let tmp = mem::take(&mut self.mid);
            let (r, m) = tmp.split_first_mut().unwrap();
            let (v, h) = r.split_first_mut().unwrap();
            self.head = h;
            self.mid = m;
            Some(v)
        } else if !self.tail.is_empty() {
            let tmp = mem::take(&mut self.tail);
            let (v, t) = tmp.split_first_mut().unwrap();
            self.tail = t;
            Some(v)
        } else {
            None
        }
    }

    fn hiter_mut_next_back(&mut self) -> Option<&'a mut T> {
        if !self.tail.is_empty() {
            let tmp = mem::take(&mut self.tail);
            let (v, t) = tmp.split_last_mut().unwrap();
            self.tail = t;
            Some(v)
        } else if !self.mid.is_empty() {
            let tmp = mem::take(&mut self.mid);
            let (r, m) = tmp.split_last_mut().unwrap();
            let (v, t) = r.split_last_mut().unwrap();
            self.tail = t;
            self.mid = m;
            Some(v)
        } else if !self.head.is_empty() {
            let tmp = mem::take(&mut self.head);
            let (v, h) = tmp.split_last_mut().unwrap();
            self.head = h;
            Some(v)
        } else {
            None
        }
    }

    fn hdouble_mut_next<const FLIP: bool>(
        &mut self,
        column: usize,
        offset: usize,
    ) -> Option<T2<&'a mut T, FLIP>> {
        if column < offset + 2 {
            return None;
        }
        if self.head.len() > 1 {
            let tmp = mem::take(&mut self.head);
            let (ps, h) = tmp.split_at_mut(2);
            self.head = h;
            let (p0, p1) = ps.split_at_mut(1);
            Some(T2(&mut p0[0], &mut p1[0]))
        } else if !self.mid.is_empty() {
            let tmp = mem::take(&mut self.mid);
            let (r, mid) = tmp.split_first_mut().unwrap();
            let k = (column % 2 + offset) % 2;
            let (ps, h) = r[offset..(column - k)].split_at_mut(2);
            self.head = h;
            self.mid = mid;
            let (p0, p1) = ps.split_at_mut(1);
            Some(T2(&mut p0[0], &mut p1[0]))
        } else if self.tail.len() > 1 {
            let tmp = mem::take(&mut self.tail);
            let (ps, t) = tmp.split_at_mut(2);
            self.tail = t;
            let (p0, p1) = ps.split_at_mut(1);
            Some(T2(&mut p0[0], &mut p1[0]))
        } else {
            None
        }
    }

    fn hdouble_mut_next_back<const FLIP: bool>(
        &mut self,
        column: usize,
        offset: usize,
    ) -> Option<T2<&'a mut T, FLIP>> {
        let col = column - offset;
        if col < 2 {
            return None;
        }
        if self.tail.len() > 1 {
            let tmp = mem::take(&mut self.tail);
            let (t, ps) = tmp.split_at_mut(tmp.len() - 2);
            self.tail = t;
            let (p0, p1) = ps.split_at_mut(1);
            Some(T2(&mut p0[0], &mut p1[0]))
        } else if !self.mid.is_empty() {
            let tmp = mem::take(&mut self.mid);
            let (r, mid) = tmp.split_first_mut().unwrap();
            let (t, ps) = r.split_at_mut(offset + col - 2 - (col % 2));
            self.mid = mid;
            self.tail = t;
            let (p0, p1) = ps.split_at_mut(1);
            Some(T2(&mut p0[0], &mut p1[0]))
        } else if self.head.len() > offset + 1 {
            let tmp = mem::take(&mut self.head);
            let (h, ps) = tmp.split_at_mut(tmp.len() - 2);
            self.head = h;
            let (p0, p1) = ps.split_at_mut(1);
            Some(T2(&mut p0[0], &mut p1[0]))
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct DoubleMut<'a, T: 'a, const FLIP: bool> {
    pub(super) head: T2<&'a mut [T], FLIP>,
    pub(super) mid: &'a mut [Vec<T>],
    pub(super) tail: T2<&'a mut [T], FLIP>,
}

impl<'a, T: 'a, const FLIP: bool> DoubleMut<'a, T, FLIP> {
    #[inline]
    pub(super) fn new(
        head: T2<&'a mut [T], FLIP>,
        mid: &'a mut [Vec<T>],
        tail: T2<&'a mut [T], FLIP>,
    ) -> Self {
        Self { head, mid, tail }
    }

    #[inline]
    pub(super) fn empty() -> Self {
        Self::new(Default::default(), &mut [], Default::default())
    }
}

impl<'a, T, const FLIP: bool> DoubleMut<'a, T, FLIP> {
    fn vdouble_nextr<const C0: usize, const C1: usize>(
        &mut self,
        column: usize,
    ) -> Option<T2<&'a mut T, FLIP>> {
        if !self.head.is_empty() {
            let tmp = mem::take(&mut self.head);
            let (p, h): (T2<&'a mut T, FLIP>, _) = tmp.split_first_mut().unwrap();
            self.head = h;
            Some(p)
        } else if !self.mid.is_empty() {
            let m_tmp = mem::take(&mut self.mid);
            let (rs_tmp, m) = m_tmp.split_at_mut(2);
            let (rt0, rt1) = rs_tmp.split_at_mut(1);
            let r0 = &mut rt0[0][C0..(column - C1)];
            let r1 = &mut rt1[0][C1..(column - C0)];
            let (p, h) = T2(r0, r1).split_first_mut().unwrap();
            self.head = h;
            self.mid = m;
            Some(p)
        } else if !self.tail.is_empty() {
            let tmp = mem::take(&mut self.tail);
            let (p, h) = tmp.split_first_mut().unwrap();
            self.tail = h;
            Some(p)
        } else {
            None
        }
    }

    fn vdouble_nextback_r<const C0: usize, const C1: usize>(
        &mut self,
        column: usize,
    ) -> Option<T2<&'a mut T, FLIP>> {
        if !self.tail.is_empty() {
            let tmp = mem::take(&mut self.tail);
            let (p, h) = tmp.split_last_mut().unwrap();
            self.tail = h;
            Some(p)
        } else if !self.mid.is_empty() {
            let m_tmp = mem::take(&mut self.mid);
            let (rs_tmp, mid) = m_tmp.split_at_mut(m_tmp.len() - 2);
            let (rt0, rt1) = rs_tmp.split_at_mut(1);
            let r0 = &mut rt0[0][C0..(column - C1)];
            let r1 = &mut rt1[0][C1..(column - C0)];
            let (p, t) = T2(r0, r1).split_last_mut().unwrap();
            self.tail = t;
            self.mid = mid;
            Some(p)
        } else if !self.head.is_empty() {
            let tmp = mem::take(&mut self.head);
            let (p, h) = tmp.split_last_mut().unwrap();
            self.head = h;
            Some(p)
        } else {
            None
        }
    }
}

pub struct HIterMut<'a, T: 'a, V> {
    core: SingleMut<'a, T>,
    _marker: PhantomData<V>,
}

impl<'a, T, V> HIterMut<'a, T, V> {
    pub(super) fn new(core: SingleMut<'a, T>) -> Self {
        Self {
            core,
            _marker: PhantomData,
        }
    }
}

impl<'a, T, V> Iterator for HIterMut<'a, T, V>
where
    &'a mut T: Into<V>,
{
    type Item = V;

    fn next(&mut self) -> Option<Self::Item> {
        self.core.hiter_mut_next().map(|v| v.into())
    }
}

impl<'a, T, V> DoubleEndedIterator for HIterMut<'a, T, V>
where
    &'a mut T: Into<V>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.core.hiter_mut_next_back().map(|v| v.into())
    }
}

impl<'a, T, V> ExactSizeIterator for HIterMut<'a, T, V> where &'a mut T: Into<V> {}

// number of rows must be even
#[derive(Debug)]
pub struct VDoubleIndexMut<'a, T: 'a, V: 'a, const C0: usize, const C1: usize, const FLIP: bool> {
    pub(super) core: DoubleMut<'a, T, FLIP>,
    pub(super) column: usize,
    _marker: PhantomData<V>,
}

impl<'a, T, V, const C0: usize, const C1: usize, const FLIP: bool>
    VDoubleIndexMut<'a, T, V, C0, C1, FLIP>
{
    pub(super) fn new(core: DoubleMut<'a, T, FLIP>, column: usize) -> Self {
        Self {
            core,
            column,
            _marker: PhantomData,
        }
    }

    pub(super) fn from_slice(slice: &'a mut [Vec<T>], column: usize) -> Self {
        let row = slice.len();
        let mid = if row % 2 == 1 {
            &mut slice[0..(row - 1)]
        } else {
            slice
        };
        Self::new(
            DoubleMut::new(Default::default(), mid, Default::default()),
            column,
        )
    }
}

impl<'a, T, V, const C0: usize, const C1: usize, const FLIP: bool> Iterator
    for VDoubleIndexMut<'a, T, V, C0, C1, FLIP>
where
    &'a mut T: Into<V>,
    V: 'a,
    T2<V, FLIP>: Into<(V, V)>,
{
    type Item = (V, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.core
            .vdouble_nextr::<C0, C1>(self.column)
            .map(|t| t.map(|e| e.into()).into())
    }
}

impl<'a, T, V, const C0: usize, const C1: usize, const FLIP: bool> DoubleEndedIterator
    for VDoubleIndexMut<'a, T, V, C0, C1, FLIP>
where
    &'a mut T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.core
            .vdouble_nextback_r::<C0, C1>(self.column)
            .map(|t| t.map(|e| e.into()).into())
    }
}

impl<'a, T, V, const C0: usize, const C1: usize, const FLIP: bool> ExactSizeIterator
    for VDoubleIndexMut<'a, T, V, C0, C1, FLIP>
where
    &'a mut T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
}

pub struct HDoubleMut<'a, T: 'a, V, const FLIP: bool> {
    pub(super) core: SingleMut<'a, T>,
    pub(super) column: usize,
    pub(super) offset: usize,
    _marker: PhantomData<V>,
}

impl<'a, T: 'a, V, const FLIP: bool> HDoubleMut<'a, T, V, FLIP> {
    pub(super) fn new(core: SingleMut<'a, T>, column: usize, offset: usize) -> Self {
        Self {
            core,
            column,
            offset,
            _marker: PhantomData,
        }
    }
}

impl<'a, T, V, const FLIP: bool> Iterator for HDoubleMut<'a, T, V, FLIP>
where
    &'a mut T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    type Item = (V, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.core
            .hdouble_mut_next(self.column, self.offset)
            .map(|t| t.map(|e| e.into()).into())
    }
}

impl<'a, T, V, const FLIP: bool> DoubleEndedIterator for HDoubleMut<'a, T, V, FLIP>
where
    &'a mut T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.core
            .hdouble_mut_next_back(self.column, self.offset)
            .map(|t| t.map(|e| e.into()).into())
    }
}

impl<'a, T, V, const FLIP: bool> ExactSizeIterator for HDoubleMut<'a, T, V, FLIP>
where
    &'a mut T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
}

/// immutable iterator
pub struct Single<'a, T: 'a> {
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

    fn hdouble_next<const FLIP: bool>(
        &mut self,
        column: usize,
        offset: usize,
    ) -> Option<T2<&'a T, FLIP>> {
        if column < offset + 2 {
            return None;
        }
        if self.head.len() > 1 {
            let tmp = mem::take(&mut self.head);
            let (ps, h) = tmp.split_at(2);
            self.head = h;
            Some(T2(&ps[0], &ps[1]))
        } else if !self.mid.is_empty() {
            let tmp = mem::take(&mut self.mid);
            let (r, mid) = tmp.split_first().unwrap();
            let k = (column % 2 + offset) % 2;
            let (ps, h) = r[offset..(column - k)].split_at(2);
            self.head = h;
            self.mid = mid;
            Some(T2(&ps[0], &ps[1]))
        } else if self.tail.len() > 1 {
            let tmp = mem::take(&mut self.tail);
            let (ps, t) = tmp.split_at(2);
            self.tail = t;
            Some(T2(&ps[0], &ps[1]))
        } else {
            None
        }
    }

    fn hdouble_next_back<const FLIP: bool>(
        &mut self,
        column: usize,
        offset: usize,
    ) -> Option<T2<&'a T, FLIP>> {
        let col = column - offset;
        if col < 2 {
            return None;
        }
        if self.tail.len() > 1 {
            let tmp = mem::take(&mut self.tail);
            let (t, ps) = tmp.split_at(tmp.len() - 2);
            self.tail = t;
            let (p0, p1) = ps.split_at(1);
            Some(T2(&p0[0], &p1[0]))
        } else if !self.mid.is_empty() {
            let tmp = mem::take(&mut self.mid);
            let (r, mid) = tmp.split_first().unwrap();
            let (t, ps) = r.split_at(offset + col - 2 - (col % 2));
            self.mid = mid;
            self.tail = t;
            let (p0, p1) = ps.split_at(1);
            Some(T2(&p0[0], &p1[0]))
        } else if self.head.len() > offset + 1 {
            let tmp = mem::take(&mut self.head);
            let (h, ps) = tmp.split_at(tmp.len() - 2);
            self.head = h;
            let (p0, p1) = ps.split_at(1);
            Some(T2(&p0[0], &p1[0]))
        } else {
            None
        }
    }

    fn hiter_next(&mut self) -> Option<&'a T> {
        if !self.head.is_empty() {
            let tmp = mem::take(&mut self.head);
            let (v, h) = tmp.split_first().unwrap();
            self.head = h;
            Some(v)
        } else if !self.mid.is_empty() {
            let tmp = mem::take(&mut self.mid);
            let (r, m) = tmp.split_first().unwrap();
            let (v, h) = r.split_first().unwrap();
            self.head = h;
            self.mid = m;
            Some(v)
        } else if !self.tail.is_empty() {
            let tmp = mem::take(&mut self.tail);
            let (v, t) = tmp.split_first().unwrap();
            self.tail = t;
            Some(v)
        } else {
            None
        }
    }

    fn hiter_next_back(&mut self) -> Option<&'a T> {
        if !self.tail.is_empty() {
            let tmp = mem::take(&mut self.head);
            let (v, t) = tmp.split_last().unwrap();
            self.tail = t;
            Some(v)
        } else if !self.mid.is_empty() {
            let tmp = mem::take(&mut self.mid);
            let (r, m) = tmp.split_last().unwrap();
            let (v, t) = r.split_last().unwrap();
            self.tail = t;
            self.mid = m;
            Some(v)
        } else if !self.head.is_empty() {
            let tmp = mem::take(&mut self.tail);
            let (v, h) = tmp.split_last().unwrap();
            self.head = h;
            Some(v)
        } else {
            None
        }
    }
}

pub struct HIter<'a, T: 'a, V> {
    core: Single<'a, T>,
    _marker: PhantomData<V>,
}

impl<'a, T, V> HIter<'a, T, V> {
    pub(super) fn new(core: Single<'a, T>) -> Self {
        HIter {
            core,
            _marker: PhantomData,
        }
    }
}

impl<'a, T, V> Iterator for HIter<'a, T, V>
where
    &'a T: Into<V>,
{
    type Item = V;

    fn next(&mut self) -> Option<Self::Item> {
        self.core.hiter_next().map(|t| t.into())
    }
}

impl<'a, T: 'a, V> DoubleEndedIterator for HIter<'a, T, V>
where
    &'a T: Into<V>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.core.hiter_next_back().map(|t| t.into())
    }
}

impl<'a, T, V> ExactSizeIterator for HIter<'a, T, V> where &'a T: Into<V> {}

pub struct Double<'a, T: 'a, const FLIP: bool> {
    pub(super) head: T2<&'a [T], FLIP>,
    pub(super) mid: &'a [Vec<T>],
    pub(super) tail: T2<&'a [T], FLIP>,
}

impl<'a, T: 'a, const FLIP: bool> Double<'a, T, FLIP> {
    #[inline]
    pub(super) fn new(head: T2<&'a [T], FLIP>, mid: &'a [Vec<T>], tail: T2<&'a [T], FLIP>) -> Self {
        Self { head, mid, tail }
    }

    #[inline]
    pub(super) fn empty() -> Self {
        Self::new(Default::default(), &[], Default::default())
    }

    fn v_next(&mut self, column: usize, offset: (usize, usize)) -> Option<T2<&'a T, FLIP>> {
        if !self.head.is_empty() {
            let tmp = mem::take(&mut self.head);
            let (p, h) = tmp.split_first().unwrap();
            self.head = h;
            Some(p)
        } else if !self.mid.is_empty() {
            let m_tmp = mem::take(&mut self.mid);
            let (rs_tmp, m) = m_tmp.split_at(2);
            let r0 = &rs_tmp[0][offset.0..(column - offset.1)];
            let r1 = &rs_tmp[1][offset.1..(column - offset.0)];
            let (p, h) = T2(r0, r1).split_first().unwrap();
            self.head = h;
            self.mid = m;
            Some(p)
        } else if !self.tail.is_empty() {
            let tmp = mem::take(&mut self.tail);
            let (p, h) = tmp.split_first().unwrap();
            self.tail = h;
            Some(p)
        } else {
            None
        }
    }

    fn v_next_back(&mut self, column: usize, offset: (usize, usize)) -> Option<T2<&'a T, FLIP>> {
        if !self.tail.is_empty() {
            let tmp = mem::take(&mut self.tail);
            let (p, h) = tmp.split_last().unwrap();
            self.tail = h;
            Some(p)
        } else if !self.mid.is_empty() {
            let m_tmp = mem::take(&mut self.mid);
            let (rs_tmp, mid) = m_tmp.split_at(m_tmp.len() - 2);
            let r0 = &rs_tmp[0][offset.0..(column - offset.1)];
            let r1 = &rs_tmp[1][offset.1..(column - offset.0)];
            let (p, t) = T2(r0, r1).split_last().unwrap();
            self.tail = t;
            self.mid = mid;
            Some(p)
        } else if !self.head.is_empty() {
            let tmp = mem::take(&mut self.head);
            let (p, h) = tmp.split_last().unwrap();
            self.head = h;
            Some(p)
        } else {
            None
        }
    }
}

// number of rows must be even
pub struct VDouble<'a, T: 'a, V, const FLIP: bool> {
    core: Double<'a, T, FLIP>,
    column: usize,
    offset: (usize, usize),
    _marker: PhantomData<V>,
}

impl<'a, T, V, const FLIP: bool> VDouble<'a, T, V, FLIP> {
    pub(super) fn new(core: Double<'a, T, FLIP>, column: usize, offset: (usize, usize)) -> Self {
        Self {
            core,
            column,
            offset,
            _marker: PhantomData,
        }
    }
}

impl<'a, T, V, const FLIP: bool> Iterator for VDouble<'a, T, V, FLIP>
where
    &'a T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    type Item = (V, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.core
            .v_next(self.column, self.offset)
            .map(|t| t.map(|e| e.into()).into())
    }
}

impl<'a, T, V, const FLIP: bool> DoubleEndedIterator for VDouble<'a, T, V, FLIP>
where
    &'a T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.core
            .v_next_back(self.column, self.offset)
            .map(|t| t.map(|e| e.into()).into())
    }
}

impl<'a, T, V, const FLIP: bool> ExactSizeIterator for VDouble<'a, T, V, FLIP>
where
    &'a T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
}

pub struct HDouble<'a, T: 'a, V, const FLIP: bool> {
    core: Single<'a, T>,
    column: usize,
    offset: usize,
    _marker: PhantomData<V>,
}

impl<'a, T: 'a, V, const FLIP: bool> HDouble<'a, T, V, FLIP> {
    pub(super) fn new(core: Single<'a, T>, column: usize, offset: usize) -> Self {
        Self {
            core,
            column,
            offset,
            _marker: PhantomData,
        }
    }
}

impl<'a, T, V, const FLIP: bool> Iterator for HDouble<'a, T, V, FLIP>
where
    &'a T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    type Item = (V, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.core
            .hdouble_next(self.column, self.offset)
            .map(|t| t.map(|e| e.into()).into())
    }
}

impl<'a, T, V, const FLIP: bool> DoubleEndedIterator for HDouble<'a, T, V, FLIP>
where
    &'a T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.core
            .hdouble_next_back(self.column, self.offset)
            .map(|t| t.map(|e| e.into()).into())
    }
}

impl<'a, T, V, const FLIP: bool> ExactSizeIterator for HDouble<'a, T, V, FLIP>
where
    &'a T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
}

pub struct IterMut<'a, T, V> {
    container: &'a mut [Vec<T>],
    column: usize,
    _marker: PhantomData<V>,
}

impl<'a, T, V> IterMut<'a, T, V> {
    pub(in super::super) fn new(container: &'a mut [Vec<T>], column: usize) -> Self {
        Self {
            container,
            column,
            _marker: PhantomData,
        }
    }

    pub fn horizontal(&mut self) -> HIterMut<'_, T, V> {
        HIterMut::new(SingleMut::new(
            Default::default(),
            self.container,
            Default::default(),
        ))
    }

    pub fn north(&mut self) -> VDoubleIndexMut<'_, T, V, 0, 0, true> {
        VDoubleIndexMut::from_slice(self.container, self.column)
    }

    pub fn northeast(&mut self) -> VDoubleIndexMut<'_, T, V, 1, 0, true> {
        VDoubleIndexMut::from_slice(self.container, self.column)
    }

    pub fn east(&mut self) -> HDoubleMut<'_, T, V, false> {
        HDoubleMut::new(
            SingleMut::new(Default::default(), self.container, Default::default()),
            self.column,
            1,
        )
    }

    pub fn southeast(&mut self) -> VDoubleIndexMut<'_, T, V, 0, 1, false> {
        VDoubleIndexMut::from_slice(self.container.get_mut(1..).unwrap_or(&mut []), self.column)
    }

    pub fn south(&mut self) -> VDoubleIndexMut<'_, T, V, 0, 0, false> {
        VDoubleIndexMut::from_slice(self.container.get_mut(1..).unwrap_or(&mut []), self.column)
    }

    pub fn southwest(&mut self) -> VDoubleIndexMut<'_, T, V, 1, 0, false> {
        VDoubleIndexMut::from_slice(self.container.get_mut(1..).unwrap_or(&mut []), self.column)
    }

    pub fn west(&mut self) -> HDoubleMut<'_, T, V, true> {
        HDoubleMut::new(
            SingleMut::new(Default::default(), self.container, Default::default()),
            self.column,
            0,
        )
    }

    pub fn northwest(&mut self) -> VDoubleIndexMut<'_, T, V, 0, 1, true> {
        VDoubleIndexMut::from_slice(self.container, self.column)
    }
}

#[cfg(test)]
mod tests {
    use super::{super::T2, DoubleMut};

    #[test]
    fn test_flip() {
        let mut r = (vec![(0usize, 0); 3], vec![(1usize, 1); 3]);
        {
            let mut d = DoubleMut::<_, true>::empty();
            d.head = T2(&mut r.0, &mut r.1);
            dbg!(d.vdouble_nextr::<0, 0>(3));
        }
        {
            let mut d = DoubleMut::<_, false>::empty();
            d.head = T2(&mut r.0, &mut r.1);
            dbg!(d.vdouble_nextr::<0, 0>(3));
        }
    }
}
