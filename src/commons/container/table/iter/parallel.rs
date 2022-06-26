use rayon::iter::{
    plumbing::{bridge, Consumer, Producer, ProducerCallback, UnindexedConsumer},
    IndexedParallelIterator, ParallelIterator,
};

use super::T2;
use super::{Double, DoubleMut, Single, SingleMut};

pub struct HIterMut<'data, T: Send> {
    core: SingleMut<'data, T>,
    column: usize,
}

impl<'data, T: Send + 'data> HIterMut<'data, T> {
    pub fn new(core: SingleMut<'data, T>, column: usize) -> Self {
        Self { core, column }
    }
}

impl<'data, T: Send + 'data> ParallelIterator for HIterMut<'data, T> {
    type Item = &'data mut T;

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<'data, T: Send + 'data> IndexedParallelIterator for HIterMut<'data, T> {
    fn len(&self) -> usize {
        self.core.head.len() + self.core.mid.len() * self.column + self.core.tail.len()
    }

    fn drive<C: Consumer<Self::Item>>(self, consumer: C) -> C::Result {
        bridge(self, consumer)
    }

    fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output {
        callback.callback(HIterMutProducer {
            core: self.core,
            column: self.column,
        })
    }
}

struct HIterMutProducer<'data, T: Send> {
    core: SingleMut<'data, T>,
    column: usize,
}

impl<'data, T: Send> HIterMutProducer<'data, T> {
    fn new(core: SingleMut<'data, T>, column: usize) -> Self {
        Self { core, column }
    }
}

impl<'data, T: Send + 'data> Producer for HIterMutProducer<'data, T> {
    type Item = &'data mut T;
    type IntoIter = super::HIterMut<'data, T>;

    fn into_iter(self) -> Self::IntoIter {
        super::HIterMut(self.core)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let this = &mut self.core;
        let hlen = this.head.len();
        let mlen = this.mid.len() * self.column;
        let tlen = this.tail.len();

        let core0;
        let core1;
        if index <= hlen {
            if hlen > 0 {
                let (hl, hr) = this.head.split_at_mut(index * 2);
                core0 = SingleMut::new(hl, &mut [], Default::default());
                core1 = SingleMut::new(hr, this.mid, this.tail);
            } else {
                core0 = SingleMut::empty();
                core1 = SingleMut::new(Default::default(), this.mid, this.tail);
            }
        } else if index <= hlen + mlen {
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
        } else if index <= hlen + mlen + tlen {
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
            core0 = self.core;
            core1 = SingleMut::empty();
        }
        (
            HIterMutProducer::new(core0, self.column),
            HIterMutProducer::new(core1, self.column),
        )
    }
}

pub struct VDPairsMut<'data, T: Send> {
    core: DoubleMut<'data, T>,
    column: usize,
    offset: T2<usize>,
}

impl<'data, T: Send + 'data> VDPairsMut<'data, T> {
    pub fn new(core: DoubleMut<'data, T>, column: usize, offset: T2<usize>) -> Self {
        Self {
            core,
            column,
            offset,
        }
    }
}

impl<'data, T: Send + 'data> ParallelIterator for VDPairsMut<'data, T> {
    type Item = (&'data mut T, &'data mut T);

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<'data, T: Send + 'data> IndexedParallelIterator for VDPairsMut<'data, T> {
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
        callback.callback(VDPairMutProducer {
            core: self.core,
            column: self.column,
            offset: self.offset,
        })
    }
}

struct VDPairMutProducer<'data, T: Send> {
    core: DoubleMut<'data, T>,
    column: usize,
    offset: T2<usize>,
}

impl<'data, T: Send> VDPairMutProducer<'data, T> {
    fn new(core: DoubleMut<'data, T>, column: usize, offset: T2<usize>) -> Self {
        Self {
            core,
            column,
            offset,
        }
    }
}

impl<'data, T: 'data + Send> Producer for VDPairMutProducer<'data, T> {
    type Item = (&'data mut T, &'data mut T);
    type IntoIter = super::VDPairsMut<'data, T>;

    fn into_iter(self) -> Self::IntoIter {
        super::VDPairsMut::new(self.core, self.column, self.offset)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let this = &mut self.core;
        let hlen = this.head.0.len();
        let mlen = this.mid.len() / 2 * self.column;
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
                let r0 = &mut rs_tmp[0][self.offset.0..(self.column - self.offset.1)];
                let r1 = &mut rs_tmp[1][self.offset.1..(self.column - self.offset.0)];
                let (rl, rr) = T2(&mut r0, &mut r1).split_at_mut(T2(r_index, r_index));
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
            core0 = self.core;
            core1 = DoubleMut::empty();
        }
        (
            VDPairMutProducer::new(core0, self.column, self.offset),
            VDPairMutProducer::new(core1, self.column, self.offset),
        )
    }
}

pub struct HPairsMut<'data, T: Send> {
    core: SingleMut<'data, T>,
    column: usize,
    offset: usize,
}

impl<'data, T: Send + 'data> HPairsMut<'data, T> {
    pub fn new(core: SingleMut<'data, T>, column: usize, offset: usize) -> Self {
        Self {
            core,
            column,
            offset,
        }
    }
}

impl<'data, T: Send + 'data> ParallelIterator for HPairsMut<'data, T> {
    type Item = (&'data mut T, &'data mut T);

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<'data, T: Send + 'data> IndexedParallelIterator for HPairsMut<'data, T> {
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
        callback.callback(HPairMutProducer {
            core: self.core,
            column: self.column,
            offset: self.offset,
        })
    }
}

struct HPairMutProducer<'data, T: Send> {
    core: SingleMut<'data, T>,
    column: usize,
    offset: usize,
}

impl<'data, T: Send> HPairMutProducer<'data, T> {
    fn new(core: SingleMut<'data, T>, column: usize, offset: usize) -> Self {
        Self {
            core,
            column,
            offset,
        }
    }
}

impl<'data, T: 'data + Send> Producer for HPairMutProducer<'data, T> {
    type Item = (&'data mut T, &'data mut T);
    type IntoIter = super::HPairsMut<'data, T>;

    fn into_iter(self) -> Self::IntoIter {
        super::HPairsMut::new(self.core, self.column, self.offset)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let k = (self.column - self.offset) % 2;
        let m = self.column - k - self.offset;
        let this = &mut self.core;
        let hlen = this.head.len() / 2;
        let mlen = this.mid.len() * m;
        let tlen = this.tail.len() / 2;

        let core0;
        let core1;
        if index <= hlen {
            if hlen > 0 {
                let (hl, hr) = this.head.split_at_mut(index * 2);
                core0 = SingleMut::new(hl, &mut [], Default::default());
                core1 = SingleMut::new(hr, this.mid, this.tail);
            } else {
                core0 = SingleMut::empty();
                core1 = SingleMut::new(Default::default(), this.mid, this.tail);
            }
        } else if index <= hlen + mlen {
            let mid_index = index - hlen;
            let c_index = mid_index / m;
            let r_index = mid_index % m;
            if mlen >= 2 {
                let (ml, tmp) = this.mid.split_at_mut(c_index);
                let (r_tmp, mr) = tmp.split_first_mut().unwrap();
                let (rl, rr) = r_tmp[self.offset..(self.column - k)].split_at_mut(r_index * 2);
                core0 = SingleMut::new(this.head, ml, rl);
                core1 = SingleMut::new(rr, mr, this.tail);
            } else {
                core0 = SingleMut::new(this.head, &mut [], Default::default());
                core1 = SingleMut::new(this.tail, &mut [], Default::default());
            }
        } else if index <= hlen + mlen + tlen {
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
            core0 = self.core;
            core1 = SingleMut::empty();
        }
        (
            HPairMutProducer::new(core0, self.column, self.offset),
            HPairMutProducer::new(core1, self.column, self.offset),
        )
    }
}

/// immutable itereator
pub struct HIter<'data, T: Sync> {
    core: Single<'data, T>,
    column: usize,
}

impl<'data, T: Sync + 'data> HIter<'data, T> {
    pub fn new(core: Single<'data, T>, column: usize) -> Self {
        Self { core, column }
    }
}

impl<'data, T: Sync + 'data> ParallelIterator for HIter<'data, T> {
    type Item = &'data T;

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<'data, T: Sync + 'data> IndexedParallelIterator for HIter<'data, T> {
    fn len(&self) -> usize {
        self.core.head.len() + self.core.mid.len() * self.column + self.core.tail.len()
    }

    fn drive<C: Consumer<Self::Item>>(self, consumer: C) -> C::Result {
        bridge(self, consumer)
    }

    fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output {
        callback.callback(HIterProducer {
            core: self.core,
            column: self.column,
        })
    }
}

struct HIterProducer<'data, T: Sync> {
    core: Single<'data, T>,
    column: usize,
}

impl<'data, T: Sync> HIterProducer<'data, T> {
    fn new(core: Single<'data, T>, column: usize) -> Self {
        Self { core, column }
    }
}

impl<'data, T: Sync + 'data> Producer for HIterProducer<'data, T> {
    type Item = &'data T;
    type IntoIter = super::HIter<'data, T>;

    fn into_iter(self) -> Self::IntoIter {
        super::HIter(self.core)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let this = &mut self.core;
        let hlen = this.head.len();
        let mlen = this.mid.len() * self.column;
        let tlen = this.tail.len();

        let core0;
        let core1;
        if index <= hlen {
            if hlen > 0 {
                let (hl, hr) = this.head.split_at(index);
                core0 = Single::new(hl, &[], Default::default());
                core1 = Single::new(hr, this.mid, this.tail);
            } else {
                core0 = Single::empty();
                core1 = Single::new(Default::default(), this.mid, this.tail);
            }
        } else if index <= hlen + mlen {
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
        } else if index <= hlen + mlen + tlen {
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
            core0 = self.core;
            core1 = Single::empty();
        }
        (
            HIterProducer::new(core0, self.column),
            HIterProducer::new(core1, self.column),
        )
    }
}

pub struct VDPairs<'data, T: Sync> {
    core: Double<'data, T>,
    column: usize,
    offset: T2<usize>,
}

impl<'data, T: Sync + 'data> VDPairs<'data, T> {
    pub fn new(core: Double<'data, T>, column: usize, offset: T2<usize>) -> Self {
        Self {
            core,
            column,
            offset,
        }
    }
}

impl<'data, T: Sync + 'data> ParallelIterator for VDPairs<'data, T> {
    type Item = (&'data T, &'data T);

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<'data, T: Sync + 'data> IndexedParallelIterator for VDPairs<'data, T> {
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
        callback.callback(VDPairProducer {
            core: self.core,
            column: self.column,
            offset: self.offset,
        })
    }
}

struct VDPairProducer<'data, T: Sync> {
    core: Double<'data, T>,
    column: usize,
    offset: T2<usize>,
}

impl<'data, T: Sync> VDPairProducer<'data, T> {
    fn new(core: Double<'data, T>, column: usize, offset: T2<usize>) -> Self {
        Self {
            core,
            column,
            offset,
        }
    }
}

impl<'data, T: 'data + Sync> Producer for VDPairProducer<'data, T> {
    type Item = (&'data T, &'data T);
    type IntoIter = super::VDPairs<'data, T>;

    fn into_iter(self) -> Self::IntoIter {
        super::VDPairs::new(self.core, self.column, self.offset)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let this = &mut self.core;
        let hlen = this.head.0.len();
        let mlen = this.mid.len() / 2 * self.column;
        let tlen = this.tail.0.len();

        let core0;
        let core1;
        if index <= hlen {
            if hlen > 0 {
                let (hl, hr) = this.head.split_at(T2(index, index));
                core0 = Double::new(hl, &mut [], Default::default());
                core1 = Double::new(hr, this.mid, this.tail);
            } else {
                core0 = Double::empty();
                core1 = Double::new(Default::default(), this.mid, this.tail);
            }
        } else if index <= hlen + mlen {
            let mid_index = index - hlen;
            let r_index = mid_index / self.column;
            let c_index = mid_index % self.column;
            if mlen >= 2 {
                let (ml, tmp) = this.mid.split_at(r_index * 2);
                let (rs_tmp, mr) = tmp.split_at(2);
                let r0 = &rs_tmp[0][self.offset.0..(self.column - self.offset.1)];
                let r1 = &rs_tmp[1][self.offset.1..(self.column - self.offset.0)];
                let (rl, rr) = T2(r0, r1).split_at(T2(c_index, c_index));
                core0 = Double::new(this.head, ml, rl);
                core1 = Double::new(rr, mr, this.tail);
            } else {
                core0 = Double::new(this.head, &mut [], Default::default());
                core1 = Double::new(this.tail, &mut [], Default::default());
            }
        } else if index <= hlen + mlen + tlen {
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
            core0 = self.core;
            core1 = Double::empty();
        }
        (
            VDPairProducer::new(core0, self.column, self.offset),
            VDPairProducer::new(core1, self.column, self.offset),
        )
    }
}

pub struct HPairs<'data, T: Sync> {
    core: Single<'data, T>,
    column: usize,
    offset: usize,
}

impl<'data, T: Sync + 'data> HPairs<'data, T> {
    pub fn new(core: Single<'data, T>, column: usize, offset: usize) -> Self {
        Self {
            core,
            column,
            offset,
        }
    }
}

impl<'data, T: Sync + 'data> ParallelIterator for HPairs<'data, T> {
    type Item = (&'data T, &'data T);

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<'data, T: Sync + 'data> IndexedParallelIterator for HPairs<'data, T> {
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
        callback.callback(HPairProducer {
            core: self.core,
            column: self.column,
            offset: self.offset,
        })
    }
}

struct HPairProducer<'data, T: Sync> {
    core: Single<'data, T>,
    column: usize,
    offset: usize,
}

impl<'data, T: Sync> HPairProducer<'data, T> {
    fn new(core: Single<'data, T>, column: usize, offset: usize) -> Self {
        Self {
            core,
            column,
            offset,
        }
    }
}

impl<'data, T: 'data + Sync + Sync> Producer for HPairProducer<'data, T> {
    type Item = (&'data T, &'data T);
    type IntoIter = super::HPairs<'data, T>;

    fn into_iter(self) -> Self::IntoIter {
        super::HPairs::new(self.core, self.column, self.offset)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let k = (self.column - self.offset) % 2;
        let m = self.column - k - self.offset;
        let this = &mut self.core;
        let hlen = this.head.len() / 2;
        let mlen = this.mid.len() * m;
        let tlen = this.tail.len() / 2;

        let core0;
        let core1;
        if index <= hlen {
            if hlen > 0 {
                let (hl, hr) = this.head.split_at(index * 2);
                core0 = Single::new(hl, &[], Default::default());
                core1 = Single::new(hr, this.mid, this.tail);
            } else {
                core0 = Single::empty();
                core1 = Single::new(Default::default(), this.mid, this.tail);
            }
        } else if index <= hlen + mlen {
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
        } else if index <= hlen + mlen + tlen {
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
            core0 = self.core;
            core1 = Single::empty();
        }
        (
            HPairProducer::new(core0, self.column, self.offset),
            HPairProducer::new(core1, self.column, self.offset),
        )
    }
}

/*
struct IntoSingle<H, M, T, V>
where
    H: Iterator<Item=V> + ExactSizeIterator,
    M: Iterator<Item=Vec<V>> + ExactSizeIterator,
    T: Iterator<Item=V> + ExactSizeIterator,
{
    // head: std::vec::IntoIter<T>,
    // mid:  std::vec::IntoIter<Vec<T>>,
    // tail: std::vec::IntoIter<T>,
    head: H,
    mid:  M,
    tail: T,
}

impl<H, M, T, V> IntoSingle<H, M, T, V>
where
    H: Iterator<Item=V> + ExactSizeIterator,
    M: Iterator<Item=Vec<V>> + ExactSizeIterator,
    T: Iterator<Item=V> + ExactSizeIterator,
{
    fn new(
        // head: std::vec::IntoIter<T>,
        // mid:  std::vec::IntoIter<Vec<T>>,
        // tail: std::vec::IntoIter<T>,
    head: H,
    mid:  M,
    tail: T,
    ) -> Self {
        Self { head, mid, tail }
    }
}

pub(super) struct HIntoIter<H, M, T, V>
where
    H: Iterator<Item=V> + ExactSizeIterator,
    M: Iterator<Item=Vec<V>> + ExactSizeIterator,
    T: Iterator<Item=V> + ExactSizeIterator,

{
    core: IntoSingle<H, M, T, V>,
    column: usize,
}

impl<H, M, T, V> HIntoIter<H, M, T, V>
where
    H: Iterator<Item=V> + ExactSizeIterator,
    M: Iterator<Item=Vec<V>> + ExactSizeIterator,
    T: Iterator<Item=V> + ExactSizeIterator,
{
    pub(super) fn new(
        core: IntoSingle<H, M, T, V>,
        column: usize,
    ) -> Self {
        Self { core, column }
    }
}

impl<H, M, T, V: Send> ParallelIterator for HIntoIter<H, M, T, V>
where
    H: Iterator<Item=V> + ExactSizeIterator + Send,
    M: Iterator<Item=Vec<V>> + ExactSizeIterator + Send,
    T: Iterator<Item=V> + ExactSizeIterator + Send,
{
    type Item = V;

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge(self, consumer)
    }
}

impl<H, M, T, V: Send> IndexedParallelIterator for HIntoIter<H, M, T, V>
where
    H: Iterator<Item=V> + ExactSizeIterator + Send,
    M: Iterator<Item=Vec<V>> + ExactSizeIterator + Send,
    T: Iterator<Item=V> + ExactSizeIterator + Send,
{
    fn len(&self) -> usize {
        self.core.head.len() + self.core.mid.len() * self.column + self.core.tail.len()
    }

    fn drive<C: Consumer<Self::Item>>(self, consumer: C) -> C::Result {
        bridge(self, consumer)
    }

    fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output {
        callback.callback(HIntoIterProducer {
            core: self.core,
            column: self.column,
        })
    }
}

impl<M, T, V: Send> Iterator for HIntoIter<std::vec::IntoIter<V>, M, T, V>
where
    // H: Iterator<Item=V> + ExactSizeIterator,
    M: Iterator<Item=Vec<V>> + ExactSizeIterator,
    T: Iterator<Item=V> + ExactSizeIterator,
{
    type Item = V;

    fn next(&mut self) -> Option<Self::Item> {
        let this = &mut self.core;
        if this.head.len() > 0 {
            this.head.next()
        } else if let Some(r) = this.mid.next() {
            this.head = r.into_iter();
            this.head.next()
        } else {
            this.tail.next()
        }
    }
}

impl<H, M, T, V: Send> DoubleEndedIterator for HIntoIter<H, M, T, V>
where
    H: Iterator<Item=V> + ExactSizeIterator,
    M: Iterator<Item=Vec<V>> + ExactSizeIterator,
    T: Iterator<Item=V> + ExactSizeIterator,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let this = &mut self.core;
        if this.tail.len() > 0 {
            this.tail.next_back()
        } else if let Some(r) = this.mid.next_back() {
            this.tail = r.into_iter();
            this.tail.next_back()
        } else {
            this.head.next_back()
        }
    }
}

impl<H, M, T, V: Send> ExactSizeIterator for HIntoIter<H, M, T, V>
where
    H: Iterator<Item=V> + ExactSizeIterator,
    M: Iterator<Item=Vec<V>> + ExactSizeIterator,
    T: Iterator<Item=V> + ExactSizeIterator,
{ }

struct HIntoIterProducer<H, M, T, V: Send>
where
    H: Iterator<Item=V> + ExactSizeIterator,
    M: Iterator<Item=Vec<V>> + ExactSizeIterator,
    T: Iterator<Item=V> + ExactSizeIterator,
{
    core: IntoSingle<H, M, T, V>,
    column: usize,
}

impl<H, M, T, V: Send> HIntoIterProducer<H, M, T, V>
where
    H: Iterator<Item=V> + ExactSizeIterator,
    M: Iterator<Item=Vec<V>> + ExactSizeIterator,
    T: Iterator<Item=V> + ExactSizeIterator,
{
    fn new( core: IntoSingle<H, M, T, V>, column: usize,) -> Self { Self { core, column } }
}

impl<H, M, T, V: Send> Producer for HIntoIterProducer<H, M, T, V>
where
    H: Iterator<Item=V> + ExactSizeIterator + Send,
    M: Iterator<Item=Vec<V>> + ExactSizeIterator + Send,
    T: Iterator<Item=V> + ExactSizeIterator + Send,
{
    type Item = V;
    type IntoIter = HIntoIter<H, M, T, V>;

    fn into_iter(self) -> Self::IntoIter {
        HIntoIter::new(self.core, self.column)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let this = self.core;
        let hlen = this.head.len();
        let mlen = this.mid.len() * self.column;
        let tlen = this.tail.len();

        let core0;
        let core1;
        if index <= hlen {
            if hlen > 0 {
                let hl = this.head.take(index);
                let hr = this.head.skip(index);
                // let (hl, hr) = this.head.split_at(index);
                // core0 = IntoSingle::new(hl, Vec::new().into_iter(), Vec::new().into_iter());
                core0 = HIntoIterProducer::new(hl, Vec::new().into_iter(), Vec::new().into_iter(), self.column)
                core1 = IntoSingle::new(hr, this.mid, this.tail, self.column);
            } else {
                core0 = IntoSingle::empty();
                core1 = IntoSingle::new(Default::default(), this.mid, this.tail);
            }
        } else if index <= hlen + mlen {
            let mid_index = index - hlen;
            let r_index = mid_index / self.column;
            let c_index = mid_index % self.column;
            if mlen >= 1 {
                let (ml, tmp) = this.mid.split_at(r_index);
                let (r_tmp, mr) = tmp.split_first().unwrap();
                let (rl, rr) = r_tmp.split_at(c_index);
                core0 = IntoSingle::new(this.head, ml, rl);
                core1 = IntoSingle::new(rr, mr, this.tail);
            } else {
                core0 = IntoSingle::new(this.head, &[], Default::default());
                core1 = IntoSingle::new(this.tail, &[], Default::default());
            }
        } else if index <= hlen + mlen + tlen {
            let tail_index = index - hlen - mlen;
            if tlen > 0 {
                let (tl, tr) = this.tail.split_at(tail_index);
                core0 = IntoSingle::new(this.head, this.mid, tl);
                core1 = IntoSingle::new(tr, &[], Default::default());
            } else {
                core0 = IntoSingle::new(this.head, this.mid, Default::default());
                core1 = IntoSingle::empty();
            }
        } else {
            core0 = self.core;
            core1 = IntoSingle::empty();
        }
        (
            HIntoIterProducer::new(core0, self.column),
            HIntoIterProducer::new(core1, self.column),
        )
    }
}
*/
