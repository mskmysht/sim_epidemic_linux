pub mod enum_map;
pub mod math;
pub mod random;
pub mod table;

use self::enum_map::EnumMap;

pub type UnionMap<K0, K1, V> = (EnumMap<K0, V>, EnumMap<K1, V>);

pub enum Either<L, R> {
    Left(L),
    Right(R),
}

pub trait DrainMap<T, U, V, F: FnMut(&mut T) -> (U, Option<V>)> {
    type Target;
    fn drain_map_mut(&mut self, f: F) -> Self::Target;
}

pub trait DrainWith<T, U, F: FnMut(&mut T) -> (U, bool)> {
    type Target;
    fn drain_with_mut(&mut self, f: F) -> Self::Target;
}

impl<T, U, V, F: FnMut(&mut T) -> (U, Option<V>)> DrainMap<T, U, V, F> for Vec<T> {
    type Target = Vec<(U, Option<(V, T)>)>;

    fn drain_map_mut(&mut self, mut f: F) -> Self::Target {
        let is = self
            .iter_mut()
            .enumerate()
            .rev()
            .map(|(i, v)| {
                let (u, v) = f(v);
                (u, v.map(|v| (v, i)))
            })
            .collect::<Vec<_>>();

        is.into_iter()
            .map(|(u, vi)| (u, vi.map(|(v, i)| (v, self.swap_remove(i)))))
            .collect()
    }
}

impl<T, U, F: FnMut(&mut T) -> (U, bool)> DrainWith<T, U, F> for Vec<T> {
    type Target = Vec<(U, Option<T>)>;

    fn drain_with_mut(&mut self, mut f: F) -> Self::Target {
        let is = self
            .iter_mut()
            .enumerate()
            .rev()
            .map(|(i, v)| {
                let (u, b) = f(v);
                (u, if b { Some(i) } else { None })
            })
            .collect::<Vec<_>>();
        is.into_iter()
            .map(|(u, i)| (u, i.map(|i| self.swap_remove(i))))
            .collect()
    }
}

pub trait ContainerExt {
    fn set_with<F: Fn(&Self) -> Self>(&mut self, f: F) -> bool;
}

impl<T> ContainerExt for Option<T> {
    fn set_with<F: Fn(&Self) -> Self>(&mut self, f: F) -> bool {
        if self.is_some() {
            return false;
        }
        match f(self) {
            v @ Some(_) => {
                *self = v;
                true
            }
            None => false,
        }
    }
}
