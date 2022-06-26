pub mod iter;
use std::ops::{Deref, DerefMut, Index, IndexMut};

use self::iter::*;

pub type Indexed<T> = (TableIndex, T);

#[derive(Debug)]
pub struct Table<T> {
    container: Vec<Vec<Indexed<T>>>,
    row: usize,
    column: usize,
}

#[derive(Debug, PartialEq)]
pub struct TableIndex {
    row: usize,
    column: usize,
}

impl TableIndex {
    pub fn new(row: usize, column: usize) -> Self {
        Self { row, column }
    }
}

impl<T> Table<T> {
    pub fn new(container: Vec<Vec<T>>) -> Self {
        let row = container.len();
        let column = if row == 0 { 0 } else { container[0].len() };
        let mut _container = Vec::new();
        for (r, rows) in container.into_iter().enumerate() {
            let mut _rows = Vec::new();
            for (c, v) in rows.into_iter().enumerate() {
                _rows.push((TableIndex::new(r, c), v));
            }
            _container.push(_rows);
        }
        Self {
            container: _container,
            row,
            column,
        }
    }

    pub fn size(&self) -> (usize, usize) {
        (self.row, self.column)
    }

    pub fn h_iter_mut(&mut self) -> HIterMut<'_, Indexed<T>> {
        HIterMut(SingleMut::new(
            Default::default(),
            self.container.as_mut_slice(),
            Default::default(),
        ))
    }

    pub fn par_h_iter(&self) -> parallel::HIter<'_, Indexed<T>>
    where
        T: Sync,
    {
        parallel::HIter::new(
            Single::new(
                Default::default(),
                self.container.as_mut_slice(),
                Default::default(),
            ),
            self.column,
        )
    }

    pub fn par_h_iter_mut(&mut self) -> parallel::HIterMut<'_, Indexed<T>>
    where
        T: Send,
    {
        parallel::HIterMut::new(
            SingleMut::new(
                Default::default(),
                self.container.as_mut_slice(),
                Default::default(),
            ),
            self.column,
        )
    }

    fn vd_pair_mut<V>(t: &mut [Vec<V>], column: usize, offset: T2<usize>) -> VDPairsMut<'_, V> {
        let row = t.len();
        let mid;
        if row % 2 == 1 {
            mid = &mut t[0..(row - 1)];
        } else {
            mid = t;
        }
        VDPairsMut::new(
            DoubleMut::new(Default::default(), mid, Default::default()),
            column,
            offset,
        )
    }

    pub fn north_pair_mut(&mut self) -> VDPairsMut<'_, Indexed<T>> {
        Self::vd_pair_mut(self.container.as_mut_slice(), self.column, T2(0, 0))
    }

    pub fn northeast_pair_mut(&mut self) -> VDPairsMut<'_, Indexed<T>> {
        Self::vd_pair_mut(self.container.as_mut_slice(), self.column, T2(1, 0))
    }

    pub fn northwest_pair_mut(&mut self) -> VDPairsMut<'_, Indexed<T>> {
        Self::vd_pair_mut(self.container.as_mut_slice(), self.column, T2(0, 1))
    }

    pub fn south_pair_mut(&mut self) -> VDPairsMut<'_, Indexed<T>> {
        Self::vd_pair_mut(
            self.container.get_mut(1..).unwrap_or(&mut []),
            self.column,
            T2(0, 0),
        )
    }

    pub fn southeast_pair_mut(&mut self) -> VDPairsMut<'_, Indexed<T>> {
        Self::vd_pair_mut(
            self.container.get_mut(1..).unwrap_or(&mut []),
            self.column,
            T2(0, 1),
        )
    }

    pub fn southwest_pair_mut(&mut self) -> VDPairsMut<'_, Indexed<T>> {
        Self::vd_pair_mut(
            self.container.get_mut(1..).unwrap_or(&mut []),
            self.column,
            T2(1, 0),
        )
    }

    pub fn east_pair_mut(&mut self) -> HPairsMut<'_, Indexed<T>> {
        HPairsMut::new(
            SingleMut::new(
                Default::default(),
                self.container.as_mut_slice(),
                Default::default(),
            ),
            self.column,
            1,
        )
    }

    pub fn west_pair_mut(&mut self) -> HPairsMut<'_, Indexed<T>> {
        HPairsMut::new(
            SingleMut::new(
                Default::default(),
                self.container.as_mut_slice(),
                Default::default(),
            ),
            self.column,
            0,
        )
    }

    fn par_vd_pair<V>(
        t: &mut [Vec<V>],
        column: usize,
        offset: T2<usize>,
    ) -> parallel::VDPairs<'_, V>
    where
        V: Sync,
    {
        let row = t.len();
        let mid;
        if row % 2 == 1 {
            mid = &mut t[0..(row - 1)];
        } else {
            mid = t;
        }
        parallel::VDPairs::new(
            Double::new(Default::default(), mid, Default::default()),
            column,
            offset,
        )
    }

    fn par_vd_pair_mut<V>(
        t: &mut [Vec<V>],
        column: usize,
        offset: T2<usize>,
    ) -> parallel::VDPairsMut<'_, V>
    where
        V: Send,
    {
        let row = t.len();
        let mid;
        if row % 2 == 1 {
            mid = &mut t[0..(row - 1)];
        } else {
            mid = t;
        }
        parallel::VDPairsMut::new(
            DoubleMut::new(Default::default(), mid, Default::default()),
            column,
            offset,
        )
    }

    pub fn par_north_pair(&mut self) -> parallel::VDPairs<'_, Indexed<T>>
    where
        T: Sync,
    {
        Self::par_vd_pair(self.container.as_mut_slice(), self.column, T2(0, 0))
    }

    pub fn par_north_pair_mut(&mut self) -> parallel::VDPairsMut<'_, Indexed<T>>
    where
        T: Send,
    {
        Self::par_vd_pair_mut(self.container.as_mut_slice(), self.column, T2(0, 0))
    }

    pub fn par_northeast_pair(&mut self) -> parallel::VDPairs<'_, Indexed<T>>
    where
        T: Sync,
    {
        Self::par_vd_pair(self.container.as_mut_slice(), self.column, T2(1, 0))
    }

    pub fn par_northeast_pair_mut(&mut self) -> parallel::VDPairsMut<'_, Indexed<T>>
    where
        T: Send,
    {
        Self::par_vd_pair_mut(self.container.as_mut_slice(), self.column, T2(1, 0))
    }

    pub fn par_northwest_pair(&mut self) -> parallel::VDPairs<'_, Indexed<T>>
    where
        T: Sync,
    {
        Self::par_vd_pair(self.container.as_mut_slice(), self.column, T2(0, 1))
    }

    pub fn par_northwest_pair_mut(&mut self) -> parallel::VDPairsMut<'_, Indexed<T>>
    where
        T: Send,
    {
        Self::par_vd_pair_mut(self.container.as_mut_slice(), self.column, T2(0, 1))
    }

    pub fn par_south_pair(&mut self) -> parallel::VDPairs<'_, Indexed<T>>
    where
        T: Sync,
    {
        Self::par_vd_pair(
            self.container.get_mut(1..).unwrap_or(&mut []),
            self.column,
            T2(0, 0),
        )
    }

    pub fn par_south_pair_mut(&mut self) -> parallel::VDPairsMut<'_, Indexed<T>>
    where
        T: Send,
    {
        Self::par_vd_pair_mut(
            self.container.get_mut(1..).unwrap_or(&mut []),
            self.column,
            T2(0, 0),
        )
    }

    pub fn par_southeast_pair(&mut self) -> parallel::VDPairs<'_, Indexed<T>>
    where
        T: Sync,
    {
        Self::par_vd_pair(
            self.container.get_mut(1..).unwrap_or(&mut []),
            self.column,
            T2(0, 1),
        )
    }

    pub fn par_southeast_pair_mut(&mut self) -> parallel::VDPairsMut<'_, Indexed<T>>
    where
        T: Send,
    {
        Self::par_vd_pair_mut(
            self.container.get_mut(1..).unwrap_or(&mut []),
            self.column,
            T2(0, 1),
        )
    }

    pub fn par_southwest_pair(&mut self) -> parallel::VDPairs<'_, Indexed<T>>
    where
        T: Sync,
    {
        Self::par_vd_pair(
            self.container.get_mut(1..).unwrap_or(&mut []),
            self.column,
            T2(1, 0),
        )
    }

    pub fn par_southwest_pair_mut(&mut self) -> parallel::VDPairsMut<'_, Indexed<T>>
    where
        T: Send,
    {
        Self::par_vd_pair_mut(
            self.container.get_mut(1..).unwrap_or(&mut []),
            self.column,
            T2(1, 0),
        )
    }

    pub fn par_east_pair(&mut self) -> parallel::HPairs<'_, Indexed<T>>
    where
        T: Sync,
    {
        parallel::HPairs::new(
            Single::new(
                Default::default(),
                self.container.as_mut_slice(),
                Default::default(),
            ),
            self.column,
            1,
        )
    }

    pub fn par_east_pair_mut(&mut self) -> parallel::HPairsMut<'_, Indexed<T>>
    where
        T: Send,
    {
        parallel::HPairsMut::new(
            SingleMut::new(
                Default::default(),
                self.container.as_mut_slice(),
                Default::default(),
            ),
            self.column,
            1,
        )
    }

    pub fn par_west_pair(&mut self) -> parallel::HPairs<'_, Indexed<T>>
    where
        T: Sync,
    {
        parallel::HPairs::new(
            Single::new(
                Default::default(),
                self.container.as_mut_slice(),
                Default::default(),
            ),
            self.column,
            0,
        )
    }

    pub fn par_west_pair_mut(&mut self) -> parallel::HPairsMut<'_, Indexed<T>>
    where
        T: Send,
    {
        parallel::HPairsMut::new(
            SingleMut::new(
                Default::default(),
                self.container.as_mut_slice(),
                Default::default(),
            ),
            self.column,
            0,
        )
    }
}

impl<T> Index<(usize, usize)> for Table<T> {
    type Output = T;

    fn index(&self, index: (usize, usize)) -> &Self::Output {
        &self.container[index.0][index.1].1
    }
}

impl<T> IndexMut<(usize, usize)> for Table<T> {
    fn index_mut(&mut self, index: (usize, usize)) -> &mut Self::Output {
        &mut self.container[index.0][index.1].1
    }
}

impl<T> Index<TableIndex> for Table<T> {
    type Output = T;

    fn index(&self, index: TableIndex) -> &Self::Output {
        &self.container[index.row][index.column].1
    }
}

impl<T> IndexMut<TableIndex> for Table<T> {
    fn index_mut(&mut self, index: TableIndex) -> &mut Self::Output {
        &mut self.container[index.row][index.column].1
    }
}

impl<'a, T> Index<usize> for Table<T> {
    type Output = [(TableIndex, T)];

    fn index(&self, index: usize) -> &Self::Output {
        self.container[index].as_slice()
    }
}

#[derive(Default)]
pub struct T2<T>(T, T);

impl<T> T2<T> {
    pub fn swap(self) -> Self {
        Self(self.1, self.0)
    }
}

impl<T> From<T2<T>> for (T, T) {
    fn from(t: T2<T>) -> Self {
        (t.0, t.1)
    }
}

impl<T: std::ops::Add<Output = T>> std::ops::Add<T> for T2<T> {
    type Output = Self;

    fn add(self, rhs: T) -> Self::Output {
        Self(self.0 + rhs, self.1 + rhs)
    }
}

impl<'a, T: 'a> T2<&'a [T]> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty() | self.1.is_empty()
    }
}

impl<'a, T: 'a> T2<&'a mut [T]> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty() | self.1.is_empty()
    }
}

impl<'a, T: 'a> T2<&'a [T]> {
    pub fn split_first(self) -> Option<(T2<&'a T>, T2<&'a [T]>)> {
        let T2(r0, r1) = self;
        let (h0, t0) = r0.split_first()?;
        let (h1, t1) = r1.split_first()?;

        Some((T2(h0, h1), T2(t0, t1)))
    }

    pub fn split_last(self) -> Option<(T2<&'a T>, T2<&'a [T]>)> {
        let T2(r0, r1) = self;
        let (t0, h0) = r0.split_last()?;
        let (t1, h1) = r1.split_last()?;

        Some((T2(t0, t1), T2(h0, h1)))
    }

    pub fn split_at(self, mid: T2<usize>) -> (T2<&'a [T]>, T2<&'a [T]>) {
        let T2(s0, s1) = self;
        let (l0, r0) = s0.split_at(mid.0);
        let (l1, r1) = s1.split_at(mid.1);

        (T2(l0, l1), T2(r0, r1))
    }

    pub fn split_back_at(self, mid: T2<usize>) -> (T2<&'a [T]>, T2<&'a [T]>) {
        let T2(s0, s1) = self;
        let (l0, r0) = s0.split_at(s0.len() - mid.0);
        let (l1, r1) = s1.split_at(s1.len() - mid.1);

        (T2(l0, l1), T2(r0, r1))
    }

    pub fn first(self) -> Option<T2<&'a T>> {
        let T2(s0, s1) = self;
        let v0 = s0.first()?;
        let v1 = s1.first()?;

        Some(T2(v0, v1))
    }

    pub fn last(self) -> Option<T2<&'a T>> {
        let T2(s0, s1) = self;
        let v0 = s0.last()?;
        let v1 = s1.last()?;

        Some(T2(v0, v1))
    }
}

impl<'a, T: 'a> T2<&'a mut [T]> {
    pub fn split_first_mut(self) -> Option<(T2<&'a mut T>, T2<&'a mut [T]>)> {
        let T2(r0, r1) = self;
        let (h0, t0) = r0.split_first_mut()?;
        let (h1, t1) = r1.split_first_mut()?;

        Some((T2(h0, h1), T2(t0, t1)))
    }

    pub fn split_last_mut(self) -> Option<(T2<&'a mut T>, T2<&'a mut [T]>)> {
        let T2(r0, r1) = self;
        let (t0, h0) = r0.split_last_mut()?;
        let (t1, h1) = r1.split_last_mut()?;

        Some((T2(t0, t1), T2(h0, h1)))
    }

    pub fn split_at_mut(self, mid: T2<usize>) -> (T2<&'a mut [T]>, T2<&'a mut [T]>) {
        let T2(s0, s1) = self;
        let (l0, r0) = s0.split_at_mut(mid.0);
        let (l1, r1) = s1.split_at_mut(mid.1);

        (T2(l0, l1), T2(r0, r1))
    }

    pub fn split_back_at_mut(self, mid: T2<usize>) -> (T2<&'a mut [T]>, T2<&'a mut [T]>) {
        let T2(s0, s1) = self;
        let (l0, r0) = s0.split_at_mut(s0.len() - mid.0);
        let (l1, r1) = s1.split_at_mut(s1.len() - mid.1);

        (T2(l0, l1), T2(r0, r1))
    }

    pub fn first_mut(self) -> Option<T2<&'a mut T>> {
        let T2(s0, s1) = self;
        let v0 = s0.first_mut()?;
        let v1 = s1.first_mut()?;

        Some(T2(v0, v1))
    }

    pub fn last_mut(self) -> Option<T2<&'a mut T>> {
        let T2(s0, s1) = self;
        let v0 = s0.last_mut()?;
        let v1 = s1.last_mut()?;

        Some(T2(v0, v1))
    }
}

impl<'a, T: 'a, V: Deref<Target = [T]>> T2<&'a V> {
    pub fn split_first(self) -> Option<(T2<&'a T>, T2<&'a [T]>)> {
        let T2(r0, r1) = self;
        T2(r0.deref(), r1.deref()).split_first()
    }

    pub fn split_last(self) -> Option<(T2<&'a T>, T2<&'a [T]>)> {
        let T2(r0, r1) = self;
        T2(r0.deref(), r1.deref()).split_last()
    }

    pub fn split_at(self, mid: T2<usize>) -> (T2<&'a [T]>, T2<&'a [T]>) {
        let T2(r0, r1) = self;
        T2(r0.deref(), r1.deref()).split_at(mid)
    }

    pub fn split_back_at(self, mid: T2<usize>) -> (T2<&'a [T]>, T2<&'a [T]>) {
        let T2(r0, r1) = self;
        T2(r0.deref(), r1.deref()).split_back_at(mid)
    }

    pub fn first(self) -> Option<T2<&'a T>> {
        let T2(r0, r1) = self;
        T2(r0.deref(), r1.deref()).first()
    }

    pub fn last(self) -> Option<T2<&'a T>> {
        let T2(r0, r1) = self;
        T2(r0.deref(), r1.deref()).last()
    }
}

impl<'a, T: 'a, V: DerefMut<Target = [T]>> T2<&'a mut V> {
    pub fn split_first_mut(self) -> Option<(T2<&'a mut T>, T2<&'a mut [T]>)> {
        let T2(r0, r1) = self;
        T2(r0.deref_mut(), r1.deref_mut()).split_first_mut()
    }

    pub fn split_last_mut(self) -> Option<(T2<&'a mut T>, T2<&'a mut [T]>)> {
        let T2(r0, r1) = self;
        T2(r0.deref_mut(), r1.deref_mut()).split_last_mut()
    }

    pub fn split_at_mut(self, mid: T2<usize>) -> (T2<&'a mut [T]>, T2<&'a mut [T]>) {
        let T2(r0, r1) = self;
        T2(r0.deref_mut(), r1.deref_mut()).split_at_mut(mid)
    }

    pub fn split_back_at_mut(self, mid: T2<usize>) -> (T2<&'a mut [T]>, T2<&'a mut [T]>) {
        let T2(r0, r1) = self;
        T2(r0.deref_mut(), r1.deref_mut()).split_back_at_mut(mid)
    }

    pub fn first_mut(self) -> Option<T2<&'a mut T>> {
        let T2(r0, r1) = self;
        T2(r0.deref_mut(), r1.deref_mut()).first_mut()
    }

    pub fn last_mut(self) -> Option<T2<&'a mut T>> {
        let T2(r0, r1) = self;
        T2(r0.deref_mut(), r1.deref_mut()).last_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::Table;

    #[test]
    fn vpair_test() {
        let mut table = Table::new(vec![vec![0; 3]; 5]);
        for (k, ((_, i), (_, j))) in table.north_pair_mut().enumerate() {
            *i = k as isize;
            *j = -(k as isize);
        }
        println!("{:?}", table);
    }
}
