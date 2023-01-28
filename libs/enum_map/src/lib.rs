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
}

#[derive(Clone)]
pub struct EnumMap<K: Enum, V> {
    arr: Vec<V>,
    _marker: PhantomData<K>,
}

impl<K: Enum, V> EnumMap<K, V> {
    fn new<F: Fn() -> V>(f: F) -> Self {
        Self {
            arr: (0..K::LEN).map(|_| f()).collect(),
            _marker: PhantomData,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> {
        self.arr.iter().enumerate().map(|(i, v)| (K::index(i), v))
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
        EnumMap::new(V::default)
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

pub use enum_map_derive as macros;
