use std::marker::PhantomData;

use rayon::iter::{
    plumbing::{bridge, Consumer, Producer, ProducerCallback, UnindexedConsumer},
    IndexedParallelIterator, ParallelIterator,
};

use super::serial::{self, Double, DoubleMut, Single, SingleMut};
use super::T2;

pub struct HIterMut<'data, T: Send, V> {
    core: SingleMut<'data, T>,
    column: usize,
    _marker: PhantomData<V>,
}

impl<'data, T: Send + 'data, V> HIterMut<'data, T, V> {
    pub fn new(core: SingleMut<'data, T>, column: usize) -> Self {
        Self {
            core,
            column,
            _marker: PhantomData,
        }
    }
}

impl<'data, T, V> ParallelIterator for HIterMut<'data, T, V>
where
    T: Send + 'data,
    V: Send,
    &'data mut T: Into<V>,
{
    type Item = V;

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<'data, T, V> IndexedParallelIterator for HIterMut<'data, T, V>
where
    T: Send + 'data,
    V: Send,
    &'data mut T: Into<V>,
{
    fn len(&self) -> usize {
        self.core.head.len() + self.core.mid.len() * self.column + self.core.tail.len()
    }

    fn drive<C: Consumer<Self::Item>>(self, consumer: C) -> C::Result {
        bridge(self, consumer)
    }

    fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output {
        callback.callback(HIterMutProducer::new(self.core, self.column))
    }
}

struct HIterMutProducer<'data, T: Send, V> {
    core: SingleMut<'data, T>,
    column: usize,
    _marker: PhantomData<V>,
}

impl<'data, T: Send, V> HIterMutProducer<'data, T, V> {
    fn new(core: SingleMut<'data, T>, column: usize) -> Self {
        Self {
            core,
            column,
            _marker: PhantomData,
        }
    }
}

impl<'data, T, V> Producer for HIterMutProducer<'data, T, V>
where
    T: Send + 'data,
    V: Send,
    &'data mut T: Into<V>,
{
    type Item = V;
    type IntoIter = serial::HIterMut<'data, T, V>;

    fn into_iter(self) -> Self::IntoIter {
        serial::HIterMut::new(self.core)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let Self {
            core: this, column, ..
        } = self;
        let hlen = this.head.len();
        let mlen = this.mid.len() * column;
        let tlen = this.tail.len();

        let core0;
        let core1;
        if index < hlen {
            if hlen > 0 {
                let (hl, hr) = this.head.split_at_mut(index);
                core0 = SingleMut::new(hl, &mut [], Default::default());
                core1 = SingleMut::new(hr, this.mid, this.tail);
            } else {
                core0 = SingleMut::empty();
                core1 = SingleMut::new(Default::default(), this.mid, this.tail);
            }
        } else if index < hlen + mlen {
            let mid_index = index - hlen;
            let r_index = mid_index / self.column;
            let c_index = mid_index % self.column;
            if mlen >= 2 {
                let (ml, tmp) = this.mid.split_at_mut(r_index);
                let (r_tmp, mr) = tmp.split_first_mut().unwrap();
                let (rl, rr) = r_tmp.split_at_mut(c_index);
                core0 = SingleMut::new(this.head, ml, rl);
                core1 = SingleMut::new(rr, mr, this.tail);
            } else {
                core0 = SingleMut::new(this.head, &mut [], Default::default());
                core1 = SingleMut::new(this.tail, &mut [], Default::default());
            }
        } else if index < hlen + mlen + tlen {
            let tail_index = index - hlen - mlen;
            if tlen > 0 {
                let (tl, tr) = this.tail.split_at_mut(tail_index);
                core0 = SingleMut::new(this.head, this.mid, tl);
                core1 = SingleMut::new(tr, &mut [], Default::default());
            } else {
                core0 = SingleMut::new(this.head, this.mid, Default::default());
                core1 = SingleMut::empty();
            }
        } else {
            core0 = this;
            core1 = SingleMut::empty();
        }
        (
            HIterMutProducer::new(core0, self.column),
            HIterMutProducer::new(core1, self.column),
        )
    }
}

#[derive(Debug)]
pub struct VDoubleIndexMut<'a, T: Send, V: Send, const C0: usize, const C1: usize, const FLIP: bool>
{
    core: DoubleMut<'a, T, FLIP>,
    column: usize,
    _marker: PhantomData<V>,
}

impl<'a, T: Send + 'a, V: Send + 'a, const C0: usize, const C1: usize, const FLIP: bool>
    VDoubleIndexMut<'a, T, V, C0, C1, FLIP>
{
    fn new(core: DoubleMut<'a, T, FLIP>, column: usize) -> Self {
        Self {
            core,
            column,
            _marker: PhantomData,
        }
    }

    pub(in super::super) fn from_slice(t: &'a mut [Vec<T>], column: usize) -> Self {
        let row = t.len();
        let mid = if row % 2 == 1 {
            &mut t[0..(row - 1)]
        } else {
            t
        };
        Self::new(
            DoubleMut::new(Default::default(), mid, Default::default()),
            column,
        )
    }
}

impl<'a, T, V, const C0: usize, const C1: usize, const FLIP: bool> ParallelIterator
    for VDoubleIndexMut<'a, T, V, C0, C1, FLIP>
where
    T: Send + 'a,
    V: Send + 'a,
    &'a mut T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    type Item = (V, V);

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<'a, T, V, const C0: usize, const C1: usize, const FLIP: bool> IndexedParallelIterator
    for VDoubleIndexMut<'a, T, V, C0, C1, FLIP>
where
    T: Send + 'a,
    V: Send + 'a,
    &'a mut T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    fn len(&self) -> usize {
        if self.column <= C0 + C1 {
            0
        } else {
            let col = self.column - C0 - C1;
            let hlen = self.core.head.0.len();
            let mlen = self.core.mid.len() / 2 * col;
            let tlen = self.core.tail.0.len();
            hlen + mlen + tlen
        }
    }

    fn drive<C: Consumer<Self::Item>>(self, consumer: C) -> C::Result {
        bridge(self, consumer)
    }

    fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output {
        callback.callback(VDoubleIndexMutProducer::<_, _, C0, C1, FLIP> {
            core: self.core,
            column: self.column,
            _marker: PhantomData,
        })
    }
}

struct VDoubleIndexMutProducer<
    'a,
    T: Send,
    V: Send,
    const C0: usize,
    const C1: usize,
    const FLIP: bool,
> {
    core: DoubleMut<'a, T, FLIP>,
    column: usize,
    _marker: PhantomData<V>,
}

impl<'a, T: Send, V: Send, const C0: usize, const C1: usize, const FLIP: bool>
    VDoubleIndexMutProducer<'a, T, V, C0, C1, FLIP>
{
    fn new(core: DoubleMut<'a, T, FLIP>, column: usize) -> Self {
        Self {
            core,
            column,
            _marker: PhantomData,
        }
    }
}

impl<'a, T, V, const C0: usize, const C1: usize, const FLIP: bool> Producer
    for VDoubleIndexMutProducer<'a, T, V, C0, C1, FLIP>
where
    T: 'a + Send,
    V: 'a + Send,
    &'a mut T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    type Item = (V, V);
    type IntoIter = serial::VDoubleIndexMut<'a, T, V, C0, C1, FLIP>;

    fn into_iter(self) -> Self::IntoIter {
        serial::VDoubleIndexMut::new(self.core, self.column)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let Self {
            core: this, column, ..
        } = self;
        let hlen = this.head.0.len();
        let mlen = this.mid.len() / 2 * column;
        let tlen = this.tail.0.len();

        let core0;
        let core1;
        if index <= hlen {
            if hlen > 0 {
                let (hl, hr) = this.head.split_at_mut(T2(index, index));
                core0 = DoubleMut::new(hl, &mut [], Default::default());
                core1 = DoubleMut::new(hr, this.mid, this.tail);
            } else {
                core0 = DoubleMut::empty();
                core1 = DoubleMut::new(Default::default(), this.mid, this.tail);
            }
        } else if index <= hlen + mlen {
            let mid_index = index - hlen;
            let c_index = mid_index / self.column;
            let r_index = mid_index % self.column;
            if mlen >= 2 {
                let (ml, tmp) = this.mid.split_at_mut(c_index * 2);
                let (rs_tmp, mr) = tmp.split_at_mut(2);
                let (rt0, rt1) = rs_tmp.split_at_mut(1);
                let r0 = &mut rt0[0][C0..(column - C1)];
                let r1 = &mut rt1[0][C1..(column - C0)];
                let (rl, rr) = T2(r0, r1).split_at_mut(T2(r_index, r_index));
                core0 = DoubleMut::new(this.head, ml, rl);
                core1 = DoubleMut::new(rr, mr, this.tail);
            } else {
                core0 = DoubleMut::new(this.head, &mut [], Default::default());
                core1 = DoubleMut::new(this.tail, &mut [], Default::default());
            }
        } else if index <= hlen + mlen + tlen {
            let tail_index = index - hlen - mlen;
            if tlen > 0 {
                let (tl, tr) = this.tail.split_at_mut(T2(tail_index, tail_index));
                core0 = DoubleMut::new(this.head, this.mid, tl);
                core1 = DoubleMut::new(tr, &mut [], Default::default());
            } else {
                core0 = DoubleMut::new(this.head, this.mid, Default::default());
                core1 = DoubleMut::empty();
            }
        } else {
            core0 = this;
            core1 = DoubleMut::empty();
        }
        (
            VDoubleIndexMutProducer::new(core0, self.column),
            VDoubleIndexMutProducer::new(core1, self.column),
        )
    }
}

pub struct HDoubleMut<'data, T: Send, V, const FLIP: bool> {
    core: SingleMut<'data, T>,
    column: usize,
    offset: usize,
    _marker: PhantomData<V>,
}

impl<'data, T: Send + 'data, V, const FLIP: bool> HDoubleMut<'data, T, V, FLIP> {
    pub fn new(core: SingleMut<'data, T>, column: usize, offset: usize) -> Self {
        Self {
            core,
            column,
            offset,
            _marker: PhantomData,
        }
    }
}

impl<'data, T, V, const FLIP: bool> ParallelIterator for HDoubleMut<'data, T, V, FLIP>
where
    T: Send + 'data,
    V: Send + 'data,
    &'data mut T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    type Item = (V, V);

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<'data, T, V, const FLIP: bool> IndexedParallelIterator for HDoubleMut<'data, T, V, FLIP>
where
    T: Send + 'data,
    V: Send + 'data,
    &'data mut T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    fn len(&self) -> usize {
        if self.column <= self.offset {
            0
        } else {
            let k = (self.column - self.offset) % 2;
            let m = self.column - k - self.offset;
            let hlen = self.core.head.len() / 2;
            let mlen = self.core.mid.len() * m;
            let tlen = self.core.tail.len() / 2;
            hlen + mlen + tlen
        }
    }

    fn drive<C: Consumer<Self::Item>>(self, consumer: C) -> C::Result {
        bridge(self, consumer)
    }

    fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output {
        callback.callback(HDoubleMutProducer::new(self.core, self.column, self.offset))
    }
}

struct HDoubleMutProducer<'data, T: Send, V, const FLIP: bool> {
    core: SingleMut<'data, T>,
    column: usize,
    offset: usize,
    _marker: PhantomData<V>,
}

impl<'data, T: Send, V, const FLIP: bool> HDoubleMutProducer<'data, T, V, FLIP> {
    fn new(core: SingleMut<'data, T>, column: usize, offset: usize) -> Self {
        Self {
            core,
            column,
            offset,
            _marker: PhantomData,
        }
    }
}

impl<'data, T, V, const FLIP: bool> Producer for HDoubleMutProducer<'data, T, V, FLIP>
where
    T: Send + 'data,
    V: Send + 'data,
    &'data mut T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    type Item = (V, V);
    type IntoIter = serial::HDoubleMut<'data, T, V, FLIP>;

    fn into_iter(self) -> Self::IntoIter {
        serial::HDoubleMut::new(self.core, self.column, self.offset)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let Self {
            core: this,
            column,
            offset,
            ..
        } = self;
        let k = (column - offset) % 2;
        let m = (column - k - offset) / 2;
        let hlen = this.head.len() / 2;
        let mlen = this.mid.len() * m;
        let tlen = this.tail.len() / 2;

        let core0;
        let core1;
        if index < hlen {
            if hlen > 0 {
                let (hl, hr) = this.head.split_at_mut(index * 2);
                core0 = SingleMut::new(hl, &mut [], Default::default());
                core1 = SingleMut::new(hr, this.mid, this.tail);
            } else {
                core0 = SingleMut::empty();
                core1 = SingleMut::new(Default::default(), this.mid, this.tail);
            }
        } else if index < hlen + mlen {
            let mid_index = index - hlen;
            let r_index = mid_index / m;
            let c_index = mid_index % m;
            if mlen >= 2 {
                let (ml, tmp) = this.mid.split_at_mut(r_index);
                let (r_tmp, mr) = tmp.split_first_mut().unwrap();
                let (rl, rr) = r_tmp[self.offset..(self.column - k)].split_at_mut(c_index * 2);
                core0 = SingleMut::new(this.head, ml, rl);
                core1 = SingleMut::new(rr, mr, this.tail);
            } else {
                core0 = SingleMut::new(this.head, &mut [], Default::default());
                core1 = SingleMut::new(this.tail, &mut [], Default::default());
            }
        } else if index < hlen + mlen + tlen {
            let tail_index = index - hlen - mlen;
            if tlen > 0 {
                let (tl, tr) = this.tail.split_at_mut(tail_index * 2);
                core0 = SingleMut::new(this.head, this.mid, tl);
                core1 = SingleMut::new(tr, &mut [], Default::default());
            } else {
                core0 = SingleMut::new(this.head, this.mid, Default::default());
                core1 = SingleMut::empty();
            }
        } else {
            core0 = this;
            core1 = SingleMut::empty();
        }
        (
            HDoubleMutProducer::new(core0, column, offset),
            HDoubleMutProducer::new(core1, column, offset),
        )
    }
}

/// immutable itereator
pub struct HIter<'data, T, V> {
    core: Single<'data, T>,
    column: usize,
    _marker: PhantomData<V>,
}

impl<'data, T, V> HIter<'data, T, V> {
    pub fn new(core: Single<'data, T>, column: usize) -> Self {
        Self {
            core,
            column,
            _marker: PhantomData,
        }
    }
}

impl<'data, T, V> ParallelIterator for HIter<'data, T, V>
where
    T: Sync + 'data,
    V: Send,
    &'data T: Into<V>,
{
    type Item = V;

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<'data, T, V> IndexedParallelIterator for HIter<'data, T, V>
where
    T: Sync + 'data,
    V: Send,
    &'data T: Into<V>,
{
    fn len(&self) -> usize {
        self.core.head.len() + self.core.mid.len() * self.column + self.core.tail.len()
    }

    fn drive<C: Consumer<Self::Item>>(self, consumer: C) -> C::Result {
        bridge(self, consumer)
    }

    fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output {
        callback.callback(HIterProducer::new(self.core, self.column))
    }
}

struct HIterProducer<'data, T, V> {
    core: Single<'data, T>,
    column: usize,
    _marker: PhantomData<V>,
}

impl<'data, T, V> HIterProducer<'data, T, V> {
    fn new(core: Single<'data, T>, column: usize) -> Self {
        Self {
            core,
            column,
            _marker: PhantomData,
        }
    }
}

impl<'data, T, V> Producer for HIterProducer<'data, T, V>
where
    T: Sync + 'data,
    V: Send,
    &'data T: Into<V>,
{
    type Item = V;
    type IntoIter = serial::HIter<'data, T, V>;

    fn into_iter(self) -> Self::IntoIter {
        serial::HIter::new(self.core)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let Self {
            core: this, column, ..
        } = self;
        let hlen = this.head.len();
        let mlen = this.mid.len() * column;
        let tlen = this.tail.len();

        let core0;
        let core1;
        if index < hlen {
            if hlen > 0 {
                let (hl, hr) = this.head.split_at(index);
                core0 = Single::new(hl, &[], Default::default());
                core1 = Single::new(hr, this.mid, this.tail);
            } else {
                core0 = Single::empty();
                core1 = Single::new(Default::default(), this.mid, this.tail);
            }
        } else if index < hlen + mlen {
            let mid_index = index - hlen;
            let r_index = mid_index / self.column;
            let c_index = mid_index % self.column;
            if mlen >= 1 {
                let (ml, tmp) = this.mid.split_at(r_index);
                let (r_tmp, mr) = tmp.split_first().unwrap();
                let (rl, rr) = r_tmp.split_at(c_index);
                core0 = Single::new(this.head, ml, rl);
                core1 = Single::new(rr, mr, this.tail);
            } else {
                core0 = Single::new(this.head, &[], Default::default());
                core1 = Single::new(this.tail, &[], Default::default());
            }
        } else if index < hlen + mlen + tlen {
            let tail_index = index - hlen - mlen;
            if tlen > 0 {
                let (tl, tr) = this.tail.split_at(tail_index);
                core0 = Single::new(this.head, this.mid, tl);
                core1 = Single::new(tr, &[], Default::default());
            } else {
                core0 = Single::new(this.head, this.mid, Default::default());
                core1 = Single::empty();
            }
        } else {
            core0 = this;
            core1 = Single::empty();
        }
        (
            HIterProducer::new(core0, column),
            HIterProducer::new(core1, column),
        )
    }
}

pub struct VDouble<'data, T, V, const FLIP: bool> {
    core: Double<'data, T, FLIP>,
    column: usize,
    offset: (usize, usize),
    _marker: PhantomData<V>,
}

impl<'data, T, V, const FLIP: bool> VDouble<'data, T, V, FLIP> {
    fn new(core: Double<'data, T, FLIP>, column: usize, offset: (usize, usize)) -> Self {
        Self {
            core,
            column,
            offset: offset,
            _marker: PhantomData,
        }
    }

    fn from_slice(slice: &'data [Vec<T>], column: usize, offset: (usize, usize)) -> Self {
        let row = slice.len();
        let mid = if row % 2 == 1 {
            &slice[0..(row - 1)]
        } else {
            slice
        };
        Self::new(
            Double::new(Default::default(), mid, Default::default()),
            column,
            offset,
        )
    }
}

impl<'data, T, V, const FLIP: bool> ParallelIterator for VDouble<'data, T, V, FLIP>
where
    T: Sync + 'data,
    V: Send,
    &'data T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    type Item = (V, V);

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<'data, T, V, const FLIP: bool> IndexedParallelIterator for VDouble<'data, T, V, FLIP>
where
    T: Sync + 'data,
    V: Send,
    &'data T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    fn len(&self) -> usize {
        if self.column <= self.offset.0 + self.offset.1 {
            0
        } else {
            let col = self.column - self.offset.0 - self.offset.1;
            let hlen = self.core.head.0.len();
            let mlen = self.core.mid.len() / 2 * col;
            let tlen = self.core.tail.0.len();
            hlen + mlen + tlen
        }
    }

    fn drive<C: Consumer<Self::Item>>(self, consumer: C) -> C::Result {
        bridge(self, consumer)
    }

    fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output {
        callback.callback(VDoubleProducer {
            core: self.core,
            column: self.column,
            offset: self.offset,
            _marker: PhantomData,
        })
    }
}

struct VDoubleProducer<'data, T, V, const FLIP: bool> {
    core: Double<'data, T, FLIP>,
    column: usize,
    offset: (usize, usize),
    _marker: PhantomData<V>,
}

impl<'data, T, V, const FLIP: bool> VDoubleProducer<'data, T, V, FLIP> {
    fn new(core: Double<'data, T, FLIP>, column: usize, offset: (usize, usize)) -> Self {
        Self {
            core,
            column,
            offset,
            _marker: PhantomData,
        }
    }
}

impl<'data, T, V, const FLIP: bool> Producer for VDoubleProducer<'data, T, V, FLIP>
where
    T: Sync + 'data,
    V: Send,
    &'data T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    type Item = (V, V);
    type IntoIter = serial::VDouble<'data, T, V, FLIP>;

    fn into_iter(self) -> Self::IntoIter {
        serial::VDouble::new(self.core, self.column, self.offset)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let Self {
            core: this,
            column,
            offset,
            ..
        } = self;
        let hlen = this.head.0.len();
        let mlen = this.mid.len() / 2 * column;
        let tlen = this.tail.0.len();

        let core0;
        let core1;
        if index < hlen {
            if hlen > 0 {
                let (hl, hr) = this.head.split_at(T2(index, index));
                core0 = Double::new(hl, &[], Default::default());
                core1 = Double::new(hr, this.mid, this.tail);
            } else {
                core0 = Double::empty();
                core1 = Double::new(Default::default(), this.mid, this.tail);
            }
        } else if index < hlen + mlen {
            let mid_index = index - hlen;
            let r_index = mid_index / self.column;
            let c_index = mid_index % self.column;
            if mlen >= 2 {
                let (ml, tmp) = this.mid.split_at(r_index * 2);
                let (rs_tmp, mr) = tmp.split_at(2);
                let r0 = &rs_tmp[0][offset.0..(column - offset.1)];
                let r1 = &rs_tmp[1][offset.1..(column - offset.0)];
                let (rl, rr) = T2(r0, r1).split_at(T2(c_index, c_index));
                core0 = Double::new(this.head, ml, rl);
                core1 = Double::new(rr, mr, this.tail);
            } else {
                core0 = Double::new(this.head, &[], Default::default());
                core1 = Double::new(this.tail, &[], Default::default());
            }
        } else if index < hlen + mlen + tlen {
            let tail_index = index - hlen - mlen;
            if tlen > 0 {
                let (tl, tr) = this.tail.split_at(T2(tail_index, tail_index));
                core0 = Double::new(this.head, this.mid, tl);
                core1 = Double::new(tr, &[], Default::default());
            } else {
                core0 = Double::new(this.head, this.mid, Default::default());
                core1 = Double::empty();
            }
        } else {
            core0 = this;
            core1 = Double::empty();
        }
        (
            VDoubleProducer::new(core0, column, offset.clone()),
            VDoubleProducer::new(core1, column, offset),
        )
    }
}

pub struct HDouble<'data, T, V, const FLIP: bool> {
    core: Single<'data, T>,
    column: usize,
    offset: usize,
    _marker: PhantomData<V>,
}

impl<'data, T, V, const FLIP: bool> HDouble<'data, T, V, FLIP> {
    pub fn new(core: Single<'data, T>, column: usize, offset: usize) -> Self {
        Self {
            core,
            column,
            offset,
            _marker: PhantomData,
        }
    }
}

impl<'data, T, V, const FLIP: bool> ParallelIterator for HDouble<'data, T, V, FLIP>
where
    T: Sync + 'data,
    V: Send,
    &'data T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    type Item = (V, V);

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<'data, T, V, const FLIP: bool> IndexedParallelIterator for HDouble<'data, T, V, FLIP>
where
    T: Sync + 'data,
    V: Send,
    &'data T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    fn len(&self) -> usize {
        if self.column <= self.offset {
            0
        } else {
            let k = (self.column - self.offset) % 2;
            let m = self.column - k - self.offset;
            let hlen = self.core.head.len() / 2;
            let mlen = self.core.mid.len() * m;
            let tlen = self.core.tail.len() / 2;
            hlen + mlen + tlen
        }
    }

    fn drive<C: Consumer<Self::Item>>(self, consumer: C) -> C::Result {
        bridge(self, consumer)
    }

    fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output {
        callback.callback(HDoubleProducer::new(self.core, self.column, self.offset))
    }
}

struct HDoubleProducer<'data, T, V, const FLIP: bool> {
    core: Single<'data, T>,
    column: usize,
    offset: usize,
    _marker: PhantomData<V>,
}

impl<'data, T, V, const FLIP: bool> HDoubleProducer<'data, T, V, FLIP> {
    fn new(core: Single<'data, T>, column: usize, offset: usize) -> Self {
        Self {
            core,
            column,
            offset,
            _marker: PhantomData,
        }
    }
}

impl<'data, T, V, const FLIP: bool> Producer for HDoubleProducer<'data, T, V, FLIP>
where
    T: Sync + 'data,
    V: Send,
    &'data T: Into<V>,
    T2<V, FLIP>: Into<(V, V)>,
{
    type Item = (V, V);
    type IntoIter = serial::HDouble<'data, T, V, FLIP>;

    fn into_iter(self) -> Self::IntoIter {
        serial::HDouble::new(self.core, self.column, self.offset)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let k = (self.column - self.offset) % 2;
        let m = (self.column - k - self.offset) / 2;
        let Self {
            core: this,
            column,
            offset,
            ..
        } = self;
        let hlen = this.head.len() / 2;
        let mlen = this.mid.len() * m;
        let tlen = this.tail.len() / 2;

        let core0;
        let core1;
        if index < hlen {
            if hlen > 0 {
                let (hl, hr) = this.head.split_at(index * 2);
                core0 = Single::new(hl, &[], Default::default());
                core1 = Single::new(hr, this.mid, this.tail);
            } else {
                core0 = Single::empty();
                core1 = Single::new(Default::default(), this.mid, this.tail);
            }
        } else if index < hlen + mlen {
            let mid_index = index - hlen;
            let r_index = mid_index / m;
            let c_index = mid_index % m;
            if mlen >= 2 {
                let (ml, tmp) = this.mid.split_at(r_index);
                let (r_tmp, mr) = tmp.split_first().unwrap();
                let (rl, rr) = r_tmp[self.offset..(self.column - k)].split_at(c_index * 2);
                core0 = Single::new(this.head, ml, rl);
                core1 = Single::new(rr, mr, this.tail);
            } else {
                core0 = Single::new(this.head, &[], Default::default());
                core1 = Single::new(this.tail, &[], Default::default());
            }
        } else if index < hlen + mlen + tlen {
            let tail_index = index - hlen - mlen;
            if tlen > 0 {
                let (tl, tr) = this.tail.split_at(tail_index * 2);
                core0 = Single::new(this.head, this.mid, tl);
                core1 = Single::new(tr, &[], Default::default());
            } else {
                core0 = Single::new(this.head, this.mid, Default::default());
                core1 = Single::empty();
            }
        } else {
            core0 = this;
            core1 = Single::empty();
        }
        (
            HDoubleProducer::new(core0, column, offset),
            HDoubleProducer::new(core1, column, offset),
        )
    }
}

pub struct IterMut<'a, T, V> {
    container: &'a mut [Vec<T>],
    column: usize,
    _marker: PhantomData<V>,
}

impl<'a, T, V> IterMut<'a, T, V>
where
    T: Send,
    V: Send,
{
    pub(in super::super) fn new(container: &'a mut [Vec<T>], column: usize) -> Self {
        Self {
            container,
            column,
            _marker: PhantomData,
        }
    }

    pub fn horizontal(&mut self) -> HIterMut<'_, T, V> {
        HIterMut::new(
            SingleMut::new(Default::default(), self.container, Default::default()),
            self.column,
        )
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

pub struct Iter<'a, T, V> {
    container: &'a [Vec<T>],
    column: usize,
    _marker: PhantomData<V>,
}

impl<'a, T, V> Iter<'a, T, V>
where
    T: Send,
    V: Send,
{
    pub(in super::super) fn new(container: &'a [Vec<T>], column: usize) -> Self {
        Self {
            container,
            column,
            _marker: PhantomData,
        }
    }

    pub fn horizontal(&self) -> HIter<'_, T, V> {
        HIter::new(
            Single::new(Default::default(), &self.container, Default::default()),
            self.column,
        )
    }

    pub fn north(&'a mut self) -> VDouble<'a, T, V, true> {
        VDouble::from_slice(self.container, self.column, (0, 0))
    }

    pub fn northeast(&mut self) -> VDouble<'_, T, V, true> {
        VDouble::from_slice(self.container, self.column, (1, 0))
    }

    pub fn east(&mut self) -> HDouble<'_, T, V, false> {
        HDouble::new(
            Single::new(Default::default(), self.container, Default::default()),
            self.column,
            1,
        )
    }

    pub fn southeast(&mut self) -> VDouble<'_, T, V, false> {
        VDouble::from_slice(
            self.container.get(1..).unwrap_or(&mut []),
            self.column,
            (0, 1),
        )
    }

    pub fn south(&mut self) -> VDouble<'_, T, V, false> {
        VDouble::from_slice(
            self.container.get(1..).unwrap_or(&mut []),
            self.column,
            (0, 0),
        )
    }

    pub fn southwest(&mut self) -> VDouble<'_, T, V, false> {
        VDouble::from_slice(
            self.container.get(1..).unwrap_or(&mut []),
            self.column,
            (1, 0),
        )
    }

    pub fn west(&mut self) -> HDouble<'_, T, V, true> {
        HDouble::new(
            Single::new(Default::default(), self.container, Default::default()),
            self.column,
            0,
        )
    }

    pub fn northwest(&mut self) -> VDouble<'_, T, V, true> {
        VDouble::from_slice(self.container, self.column, (0, 1))
    }
}
