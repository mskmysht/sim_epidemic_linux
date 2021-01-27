pub use custom_macro::Enum;

use std::{
    fmt::Debug,
    marker::PhantomData,
    ops::{Index, IndexMut},
};

pub trait Enum {
    const ENUM_SIZE: usize;
    fn from_usize(u: usize) -> Self;
    fn to_usize(self) -> usize;
    fn keys() -> Vec<Self>
    where
        Self: Sized,
    {
        (0..Self::ENUM_SIZE).map(Self::from_usize).collect()
    }
}

pub struct EnumMap<K, V> {
    arr: Vec<V>,
    _maker: PhantomData<K>,
}

impl<K: Enum + Debug, V: Debug> Debug for EnumMap<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(
                self.arr
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (<K as Enum>::from_usize(i), v)),
            )
            .finish()
    }
}

impl<K: Enum, V: Default> Default for EnumMap<K, V> {
    fn default() -> Self {
        EnumMap {
            arr: (0..<K as Enum>::ENUM_SIZE).map(|_| V::default()).collect(),
            _maker: PhantomData,
        }
    }
}

impl<K: Enum, V> Index<K> for EnumMap<K, V> {
    type Output = V;

    fn index(&self, index: K) -> &Self::Output {
        &self.arr[index.to_usize()]
    }
}

impl<K: Enum, V> IndexMut<K> for EnumMap<K, V> {
    fn index_mut(&mut self, index: K) -> &mut Self::Output {
        &mut self.arr[index.to_usize()]
    }
}

pub struct Iter<'a, K, V> {
    iterator: std::iter::Enumerate<std::slice::Iter<'a, V>>,
    _marker: PhantomData<K>,
}

pub struct IterMut<'a, K, V> {
    iterator: std::iter::Enumerate<std::slice::IterMut<'a, V>>,
    _marker: PhantomData<K>,
}

impl<'a, K: Enum, V> Iterator for Iter<'a, K, V> {
    type Item = (K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.iterator.next().map(|(i, v)| {
            let key = <K as Enum>::from_usize(i);
            (key, v)
        })
    }
}

impl<'a, K: Enum, V> Iterator for IterMut<'a, K, V> {
    type Item = (K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        self.iterator.next().map(|(i, v)| {
            let key = <K as Enum>::from_usize(i);
            (key, v)
        })
    }
}

impl<'a, K: Enum, V> IntoIterator for &'a EnumMap<K, V> {
    type Item = (K, &'a V);

    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            iterator: self.arr.iter().enumerate(),
            _marker: PhantomData,
        }
    }
}

impl<'a, K: Enum, V> IntoIterator for &'a mut EnumMap<K, V> {
    type Item = (K, &'a mut V);

    type IntoIter = IterMut<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        IterMut {
            iterator: self.arr.iter_mut().enumerate(),
            _marker: PhantomData,
        }
    }
}
