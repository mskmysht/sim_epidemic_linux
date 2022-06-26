pub use custom_macro::Enum;

use std::{
    fmt::Debug,
    marker::PhantomData,
    ops::{Index, IndexMut},
};

pub trait Enum: Sized {
    const ENUM_SIZE: usize;

    fn from_usize(u: usize) -> Self;
    fn to_usize(self) -> usize;
    fn iter<'a>() -> EnumIter<'a, Self> {
        EnumIter(0, PhantomData)
    }
}

pub struct EnumIter<'a, T>(usize, PhantomData<&'a T>);
impl<'a, T: Enum> Iterator for EnumIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if T::ENUM_SIZE >= self.0 {
            None
        } else {
            let t = T::from_usize(self.0);
            self.0 += 1;
            Some(&t)
        }
    }
}

pub struct EnumMap<K: Enum, V> {
    arr: Vec<V>,
    _maker: PhantomData<K>,
}

impl<K: Enum, V> EnumMap<K, V> {
    fn new(arr: Vec<V>) -> Self {
        Self {
            arr,
            _maker: PhantomData,
        }
    }
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
        EnumMap::new((0..<K as Enum>::ENUM_SIZE).map(|_| V::default()).collect())
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
    keys: EnumIter<'a, K>,
    values: &'a [V],
}

pub struct IterMut<'a, K, V> {
    keys: EnumIter<'a, K>,
    values: &'a mut [V],
}

impl<'a, K: Enum, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        // let k_tmp = std::mem::replace(&mut self.keys, &mut []);
        let k = self.keys.next()?;
        let (v, vs) = self.values.split_first()?;
        self.values = vs;
        Some((k, v))
    }
}

impl<'a, K: Enum, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        let k = self.keys.next()?;
        let v_tmp = std::mem::replace(&mut self.values, &mut []);
        let (v, vs) = v_tmp.split_first_mut()?;
        self.values = vs;
        Some((k, v))
    }
}

impl<'a, K: Enum, V> IntoIterator for &'a EnumMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            keys: K::iter(),
            values: self.arr.as_slice(),
        }
    }
}

impl<'a, K: Enum, V> IntoIterator for &'a mut EnumMap<K, V> {
    type Item = (&'a K, &'a mut V);
    type IntoIter = IterMut<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        IterMut {
            keys: K::iter(),
            values: self.arr.as_mut_slice(),
        }
    }
}
