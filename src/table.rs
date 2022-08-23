mod iter;

use self::iter::*;
use std::{
    fmt::Debug,
    ops::{Index, IndexMut},
};

pub type Indexed<T> = (TableIndex, T);

#[derive(Debug)]
pub struct Item<Idx, T>(Idx, T);

#[derive(Debug)]
pub struct Table<T> {
    container: Vec<Vec<Item<TableIndex, T>>>,
    row: usize,
    column: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
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
    pub fn new<F: Fn() -> T>(row: usize, column: usize, f: F) -> Self {
        let mut container = Vec::with_capacity(row);
        for ir in 0..row {
            let mut r = Vec::with_capacity(column);
            for ic in 0..column {
                r.push(Item(TableIndex::new(ir, ic), f()))
            }
            container.push(r);
        }
        Self {
            container,
            row,
            column,
        }
    }

    pub fn from_container(container: Vec<Vec<T>>) -> Self {
        let row = container.len();
        let column = if row == 0 { 0 } else { container[0].len() };
        let mut _container = Vec::new();
        for (r, rows) in container.into_iter().enumerate() {
            let mut _rows = Vec::new();
            for (c, v) in rows.into_iter().enumerate() {
                _rows.push(Item(TableIndex::new(r, c), v));
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

    pub fn par_iter(&self) -> parallel::Iter<'_, Item<TableIndex, T>, (&TableIndex, &T)>
    where
        T: Sync + Send,
    {
        parallel::Iter::new(&self.container, self.column)
    }

    pub fn par_iter_mut(
        &mut self,
    ) -> parallel::IterMut<'_, Item<TableIndex, T>, (&TableIndex, &mut T)>
    where
        T: Send,
    {
        parallel::IterMut::new(&mut self.container, self.column)
    }

    pub fn iter_mut(&mut self) -> serial::IterMut<'_, Item<TableIndex, T>, (&TableIndex, &mut T)> {
        serial::IterMut::new(&mut self.container, self.column)
    }
}

impl<'a, Idx, T> From<&'a mut Item<Idx, T>> for (&'a Idx, &'a mut T) {
    fn from(Item(idx, t): &'a mut Item<Idx, T>) -> Self {
        (idx, t)
    }
}

impl<'a, Idx, T> From<&'a Item<Idx, T>> for (&'a Idx, &'a T) {
    fn from(Item(idx, t): &'a Item<Idx, T>) -> Self {
        (idx, t)
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

#[cfg(test)]
mod tests {
    use rayon::iter::ParallelIterator;

    use super::{Table, TableIndex};

    fn dump<H: Fn(&TableIndex)>(d: bool, h: H) -> impl Fn(&TableIndex, &TableIndex) {
        move |idx0, idx1| {
            if d {
                dbg!(idx0, idx1);
            }
            h(idx0);
        }
    }

    fn iter_check_builder<F, G, H>(
        name: &str,
        f: F,
        g: G,
        h: H,
    ) -> impl Fn(&[(&TableIndex, &TableIndex)])
    where
        F: Fn(&TableIndex, &TableIndex),
        G: Fn(usize, &TableIndex, &TableIndex),
        H: Fn(usize),
    {
        println!("{name}");
        move |ips| {
            let mut max_k = 0;
            let (idx0, idx1) = ips.get(0).unwrap();
            f(idx0, idx1);
            for (k, (idx0, idx1)) in ips.iter().enumerate() {
                g(k, idx0, idx1);
                if max_k < k {
                    max_k = k;
                }
            }
            h(max_k + 1);
        }
    }

    fn get_idx<'a, A>(
        ((idx0, _), (idx1, _)): ((&'a TableIndex, A), (&'a TableIndex, A)),
    ) -> (&'a TableIndex, &'a TableIndex) {
        (idx0, idx1)
    }

    fn horizontal_checker<const R: usize, const C: usize>() -> impl Fn(&[&TableIndex]) {
        println!("horizontal");
        |ips| {
            let mut max_k = 0;
            for (k, idx) in ips.iter().enumerate() {
                assert!(idx.row == k / C);
                assert!(idx.column == k % C);
                if max_k < k {
                    max_k = k;
                }
            }
            assert!(max_k + 1 == R * C)
        }
    }

    fn north_checker<const R: usize, const C: usize>() -> impl Fn(&[(&TableIndex, &TableIndex)]) {
        iter_check_builder(
            "north",
            dump(false, |idx0| {
                assert!(idx0.row == 1 && idx0.column == 0);
            }),
            |k, idx0, idx1| {
                assert!(idx0.row == idx1.row + 1);
                assert!(idx0.column == idx1.column);
                assert!((k / C) * 2 + 1 == idx0.row);
                assert!(k % C == idx0.column);
            },
            |n| {
                assert!(n / C == R / 2);
                assert!(n % C == 0);
            },
        )
    }

    fn northeast_checker<const R: usize, const C: usize>() -> impl Fn(&[(&TableIndex, &TableIndex)])
    {
        iter_check_builder(
            "northeast",
            dump(false, |idx0| {
                assert!(idx0.row == 1 && idx0.column == 0);
            }),
            |_, idx0, idx1| {
                assert!(idx0.row == idx1.row + 1);
                assert!(idx0.column + 1 == idx1.column);
            },
            |n| {
                assert!(n / (C - 1) == R / 2);
                assert!(n % (C - 1) == 0);
            },
        )
    }

    fn east_checker<const R: usize, const C: usize>() -> impl Fn(&[(&TableIndex, &TableIndex)]) {
        iter_check_builder(
            "east",
            dump(false, |idx0| {
                assert!(idx0.row == 0 && idx0.column == 1);
            }),
            |_, idx0, idx1| {
                assert!(idx0.row == idx1.row);
                assert!(idx0.column + 1 == idx1.column);
            },
            |n| {
                assert!(n / ((C - 1) / 2) == R);
                assert!(n % ((C - 1) / 2) == 0);
            },
        )
    }

    fn southeast_checker<const R: usize, const C: usize>() -> impl Fn(&[(&TableIndex, &TableIndex)])
    {
        iter_check_builder(
            "southeast",
            dump(false, |idx0| {
                assert!(idx0.row == 1 && idx0.column == 0);
            }),
            |_, idx0, idx1| {
                assert!(idx0.row + 1 == idx1.row);
                assert!(idx0.column + 1 == idx1.column);
            },
            |n| {
                assert!(n / (C - 1) == (R - 1) / 2);
                assert!(n % (C - 1) == 0);
            },
        )
    }

    fn south_checker<const R: usize, const C: usize>() -> impl Fn(&[(&TableIndex, &TableIndex)]) {
        iter_check_builder(
            "south",
            dump(false, |idx0| {
                assert!(idx0.row == 1 && idx0.column == 0);
            }),
            |_, idx0, idx1| {
                assert!(idx0.row + 1 == idx1.row);
                assert!(idx0.column == idx1.column);
            },
            |n| {
                assert!(n / C == (R - 1) / 2);
                assert!(n % C == 0);
            },
        )
    }

    fn southwest_checker<const R: usize, const C: usize>() -> impl Fn(&[(&TableIndex, &TableIndex)])
    {
        iter_check_builder(
            "southwest",
            dump(false, |idx0| {
                assert!(idx0.row == 1 && idx0.column == 1);
            }),
            |_, idx0, idx1| {
                assert!(idx0.row + 1 == idx1.row);
                assert!(idx0.column == idx1.column + 1);
            },
            |n| {
                assert!(n / (C - 1) == (R - 1) / 2);
                assert!(n % (C - 1) == 0);
            },
        )
    }

    fn west_checker<const R: usize, const C: usize>() -> impl Fn(&[(&TableIndex, &TableIndex)]) {
        iter_check_builder(
            "west",
            dump(false, |idx0| {
                assert!(idx0.row == 0 && idx0.column == 1);
            }),
            |_, idx0, idx1| {
                assert!(idx0.row == idx1.row);
                assert!(idx0.column == idx1.column + 1);
            },
            |n| {
                assert!(n / (C / 2) == R);
                assert!(n % (C / 2) == 0);
            },
        )
    }

    fn northwest_checker<const R: usize, const C: usize>() -> impl Fn(&[(&TableIndex, &TableIndex)])
    {
        iter_check_builder(
            "northwest",
            dump(false, |idx0| {
                assert!(idx0.row == 1 && idx0.column == 1);
            }),
            |_, idx0, idx1| {
                assert!(idx0.row == idx1.row + 1);
                assert!(idx0.column == idx1.column + 1);
            },
            |n| {
                assert!(n / (C - 1) == R / 2);
                assert!(n % (C - 1) == 0);
            },
        )
    }

    fn iter_mut_all_direction<const R: usize, const C: usize>() {
        println!("({R}, {C})");
        let table = &mut Table::new(R, C, || 0);

        north_checker::<R, C>()(&table.iter_mut().north().map(get_idx).collect::<Vec<_>>());
        northeast_checker::<R, C>()(
            &table
                .iter_mut()
                .northeast()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );
        east_checker::<R, C>()(&table.iter_mut().east().map(get_idx).collect::<Vec<_>>());
        southeast_checker::<R, C>()(
            &table
                .iter_mut()
                .southeast()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );
        south_checker::<R, C>()(&table.iter_mut().south().map(get_idx).collect::<Vec<_>>());
        southwest_checker::<R, C>()(
            &table
                .iter_mut()
                .southwest()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );
        west_checker::<R, C>()(&table.iter_mut().west().map(get_idx).collect::<Vec<_>>());
        northwest_checker::<R, C>()(
            &table
                .iter_mut()
                .northwest()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );

        horizontal_checker::<R, C>()(
            &table
                .iter_mut()
                .horizontal()
                .map(|a| a.0)
                .collect::<Vec<_>>(),
        );
    }

    fn par_iter_mut_all_direction<const R: usize, const C: usize>() {
        println!("({R}, {C})");
        let table = &mut Table::new(R, C, || 0);

        north_checker::<R, C>()(
            &table
                .par_iter_mut()
                .north()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );
        northeast_checker::<R, C>()(
            &table
                .par_iter_mut()
                .northeast()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );
        east_checker::<R, C>()(&table.par_iter_mut().east().map(get_idx).collect::<Vec<_>>());
        southeast_checker::<R, C>()(
            &table
                .par_iter_mut()
                .southeast()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );
        south_checker::<R, C>()(
            &table
                .par_iter_mut()
                .south()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );
        southwest_checker::<R, C>()(
            &table
                .par_iter_mut()
                .southwest()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );
        west_checker::<R, C>()(&table.par_iter_mut().west().map(get_idx).collect::<Vec<_>>());
        northwest_checker::<R, C>()(
            &table
                .par_iter_mut()
                .northwest()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );

        horizontal_checker::<R, C>()(
            &table
                .par_iter_mut()
                .horizontal()
                .map(|a| a.0)
                .collect::<Vec<_>>(),
        );
    }

    fn par_iter_all_direction<const R: usize, const C: usize>() {
        println!("({R}, {C})");
        let table = &mut Table::new(R, C, || 0);

        north_checker::<R, C>()(&table.par_iter().north().map(get_idx).collect::<Vec<_>>());
        northeast_checker::<R, C>()(
            &table
                .par_iter()
                .northeast()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );
        east_checker::<R, C>()(&table.par_iter().east().map(get_idx).collect::<Vec<_>>());
        southeast_checker::<R, C>()(
            &table
                .par_iter()
                .southeast()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );
        south_checker::<R, C>()(&table.par_iter().south().map(get_idx).collect::<Vec<_>>());
        southwest_checker::<R, C>()(
            &table
                .par_iter()
                .southwest()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );
        west_checker::<R, C>()(&table.par_iter().west().map(get_idx).collect::<Vec<_>>());
        northwest_checker::<R, C>()(
            &table
                .par_iter()
                .northwest()
                .map(get_idx)
                .collect::<Vec<_>>(),
        );

        horizontal_checker::<R, C>()(
            &table
                .par_iter()
                .horizontal()
                .map(|a| a.0)
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn table_test() {
        iter_mut_all_direction::<7, 6>();
        iter_mut_all_direction::<6, 6>();
        iter_mut_all_direction::<6, 7>();
        iter_mut_all_direction::<7, 7>();
    }

    #[test]
    fn table_test_par_mut() {
        par_iter_mut_all_direction::<7, 6>();
        par_iter_mut_all_direction::<6, 6>();
        par_iter_mut_all_direction::<6, 7>();
        par_iter_mut_all_direction::<7, 7>();
    }

    #[test]
    fn table_test_par() {
        par_iter_all_direction::<7, 6>();
        par_iter_all_direction::<6, 6>();
        par_iter_all_direction::<6, 7>();
        par_iter_all_direction::<7, 7>();
    }
}
