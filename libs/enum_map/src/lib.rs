use std::slice;
use std::{
    fmt::Debug,
    marker::PhantomData,
    ops::{Index, IndexMut},
};

pub trait Enum: Clone + Sized {
    type Arr: Index<usize, Output = Self> + IntoIterator<Item = Self>;
    const LEN: usize;
    const ALL: Self::Arr;

    fn index(idx: usize) -> Self {
        Self::ALL.index(idx).clone()
    }

    fn to_index(&self) -> usize;

    fn next(&self) -> Option<Self> {
        let i = self.to_index() + 1;
        if i < Self::LEN {
            Some(Self::ALL[i].clone())
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub struct EnumMap<K: Enum, V> {
    arr: Vec<V>,
    _marker: PhantomData<K>,
}

#[macro_export]
macro_rules! enum_map {
    (
        $enum_type:ty {
            $($key:ident => $value:expr,)*
            _ => $dvalue:expr,
        }
    ) => {
        {
            let mut arr = Vec::new();
            for key in <$enum_type as $crate::Enum>::ALL {
                arr.push(None);
            }
            $(
                arr[$crate::Enum::to_index(&<$enum_type>::$key)] = Some($value);
            )*

            $crate::EnumMap::<$enum_type, _>::from_arr(arr.into_iter().map(|v| v.unwrap_or($dvalue)).collect())
        }
    };
}

impl<K: Enum, V> EnumMap<K, V> {
    pub fn from_map<F: Fn() -> V>(f: F) -> Self {
        Self {
            arr: (0..K::LEN).map(|_| f()).collect(),
            _marker: PhantomData,
        }
    }

    pub fn from_arr(arr: Vec<V>) -> Self {
        Self {
            arr,
            _marker: PhantomData,
        }
    }

    pub fn iter(&self) -> Iter<K, V> {
        self.into_iter()
    }

    pub fn iter_mut(&mut self) -> IterMut<K, V> {
        self.into_iter()
    }

    pub fn values(&self) -> &[V] {
        &self.arr
    }

    pub fn values_mut(&mut self) -> &mut [V] {
        &mut self.arr
    }

    pub fn into_vec(self) -> Vec<V> {
        self.arr
    }
}

impl<K: Enum + Debug, V: Debug> Debug for EnumMap<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(self.arr.iter().enumerate().map(|(i, v)| (K::index(i), v)))
            .finish()
    }
}

impl<K: Enum, V: Default> Default for EnumMap<K, V> {
    fn default() -> Self {
        EnumMap::from_map(V::default)
    }
}

impl<K: Enum, V> Index<&K> for EnumMap<K, V> {
    type Output = V;

    fn index(&self, index: &K) -> &Self::Output {
        &self.arr[index.to_index()]
    }
}

impl<K: Enum, V> IndexMut<&K> for EnumMap<K, V> {
    fn index_mut(&mut self, index: &K) -> &mut Self::Output {
        &mut self.arr[index.to_index()]
    }
}

pub struct Iter<'a, K, V> {
    cur_key: Option<K>,
    value_iter: slice::Iter<'a, V>,
}

impl<'a, K: Enum, V> Iterator for Iter<'a, K, V> {
    type Item = (K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let k = self.cur_key.take()?;
        let v = self.value_iter.next()?;
        self.cur_key = k.next();
        Some((k, v))
    }
}

impl<'a, K: Enum, V> IntoIterator for &'a EnumMap<K, V> {
    type Item = (K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            cur_key: Some(K::index(0)),
            value_iter: self.arr.iter(),
        }
    }
}

pub struct IterMut<'a, K, V> {
    cur_key: Option<K>,
    value_iter: slice::IterMut<'a, V>,
}

impl<'a, K: Enum, V> Iterator for IterMut<'a, K, V> {
    type Item = (K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        let k = self.cur_key.take()?;
        let v = self.value_iter.next()?;
        self.cur_key = k.next();
        Some((k, v))
    }
}

impl<'a, K: Enum, V> IntoIterator for &'a mut EnumMap<K, V> {
    type Item = (K, &'a mut V);
    type IntoIter = IterMut<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        IterMut {
            cur_key: Some(K::index(0)),
            value_iter: self.arr.iter_mut(),
        }
    }
}

pub use enum_map_derive as macros;
