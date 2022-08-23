pub(super) mod parallel;
pub(super) mod serial;

use std::fmt::Debug;

#[derive(Clone, Default)]
pub(super) struct T2<T, const FLIP: bool>(T, T);

impl<T: Debug, const FLIP: bool> Debug for T2<T, FLIP> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(format!("T2_{FLIP}").as_str())
            .field(&self.0)
            .field(&self.1)
            .finish()
    }
}

impl<T, const FLIP: bool> T2<T, FLIP> {
    // pub fn swap(self) -> Self {
    //     Self(self.1, self.0)
    // }

    pub fn map<F: Fn(T) -> U, U>(self, f: F) -> T2<U, FLIP> {
        T2(f(self.0), f(self.1))
    }
}

impl<T> From<T2<T, false>> for (T, T) {
    fn from(t: T2<T, false>) -> Self {
        (t.0, t.1)
    }
}

impl<T> From<T2<T, true>> for (T, T) {
    fn from(t: T2<T, true>) -> Self {
        (t.1, t.0)
    }
}

impl<'a, I, V, const FLIP: bool> From<T2<&'a mut (I, V), FLIP>>
    for ((&'a I, &'a mut V), (&'a I, &'a mut V))
where
    T2<&'a mut (I, V), FLIP>: Into<(&'a mut (I, V), &'a mut (I, V))>,
{
    fn from(t: T2<&'a mut (I, V), FLIP>) -> Self {
        let ((i0, v0), (i1, v1)) = t.into();
        ((i0, v0), (i1, v1))
    }
}

impl<T: std::ops::Add<Output = T> + Clone, const FLIP: bool> std::ops::Add<T> for T2<T, FLIP> {
    type Output = Self;

    fn add(self, rhs: T) -> Self::Output {
        Self(self.0 + rhs.clone(), self.1 + rhs)
    }
}

impl<'a, T: 'a, const FLIP: bool> T2<&'a [T], FLIP> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty() | self.1.is_empty()
    }
}

impl<'a, T: 'a, const FLIP: bool> T2<&'a mut [T], FLIP> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty() | self.1.is_empty()
    }
}

impl<'a, T: 'a, const FLIP: bool> T2<&'a [T], FLIP> {
    pub fn split_first(self) -> Option<(T2<&'a T, FLIP>, T2<&'a [T], FLIP>)> {
        let T2(r0, r1) = self;
        let (h0, t0) = r0.split_first()?;
        let (h1, t1) = r1.split_first()?;

        Some((T2(h0, h1), T2(t0, t1)))
    }

    pub fn split_last(self) -> Option<(T2<&'a T, FLIP>, T2<&'a [T], FLIP>)> {
        let T2(r0, r1) = self;
        let (t0, h0) = r0.split_last()?;
        let (t1, h1) = r1.split_last()?;

        Some((T2(t0, t1), T2(h0, h1)))
    }

    pub fn split_at(self, mid: T2<usize, FLIP>) -> (T2<&'a [T], FLIP>, T2<&'a [T], FLIP>) {
        let T2(s0, s1) = self;
        let (l0, r0) = s0.split_at(mid.0);
        let (l1, r1) = s1.split_at(mid.1);

        (T2(l0, l1), T2(r0, r1))
    }

    /*
    pub fn split_back_at(self, mid: T2<usize, FLIP>) -> (T2<&'a [T], FLIP>, T2<&'a [T], FLIP>) {
        let T2(s0, s1) = self;
        let (l0, r0) = s0.split_at(s0.len() - mid.0);
        let (l1, r1) = s1.split_at(s1.len() - mid.1);

        (T2(l0, l1), T2(r0, r1))
    }

    pub fn first(self) -> Option<T2<&'a T, FLIP>> {
        let T2(s0, s1) = self;
        let v0 = s0.first()?;
        let v1 = s1.first()?;

        Some(T2(v0, v1))
    }

    pub fn last(self) -> Option<T2<&'a T, FLIP>> {
        let T2(s0, s1) = self;
        let v0 = s0.last()?;
        let v1 = s1.last()?;

        Some(T2(v0, v1))
    }
    */
}

impl<'a, T: 'a, const FLIP: bool> T2<&'a mut [T], FLIP> {
    pub fn split_first_mut(self) -> Option<(T2<&'a mut T, FLIP>, T2<&'a mut [T], FLIP>)> {
        let T2(r0, r1) = self;
        let (h0, t0) = r0.split_first_mut()?;
        let (h1, t1) = r1.split_first_mut()?;

        Some((T2(h0, h1), T2(t0, t1)))
    }

    pub fn split_last_mut(self) -> Option<(T2<&'a mut T, FLIP>, T2<&'a mut [T], FLIP>)> {
        let T2(r0, r1) = self;
        let (t0, h0) = r0.split_last_mut()?;
        let (t1, h1) = r1.split_last_mut()?;

        Some((T2(t0, t1), T2(h0, h1)))
    }

    pub fn split_at_mut(
        self,
        mid: T2<usize, FLIP>,
    ) -> (T2<&'a mut [T], FLIP>, T2<&'a mut [T], FLIP>) {
        let T2(s0, s1) = self;
        let (l0, r0) = s0.split_at_mut(mid.0);
        let (l1, r1) = s1.split_at_mut(mid.1);

        (T2(l0, l1), T2(r0, r1))
    }

    /*
    pub fn split_back_at_mut(
        self,
        mid: T2<usize, FLIP>,
    ) -> (T2<&'a mut [T], FLIP>, T2<&'a mut [T], FLIP>) {
        let T2(s0, s1) = self;
        let (l0, r0) = s0.split_at_mut(s0.len() - mid.0);
        let (l1, r1) = s1.split_at_mut(s1.len() - mid.1);

        (T2(l0, l1), T2(r0, r1))
    }

    pub fn first_mut(self) -> Option<T2<&'a mut T, FLIP>> {
        let T2(s0, s1) = self;
        let v0 = s0.first_mut()?;
        let v1 = s1.first_mut()?;

        Some(T2(v0, v1))
    }

    pub fn last_mut(self) -> Option<T2<&'a mut T, FLIP>> {
        let T2(s0, s1) = self;
        let v0 = s0.last_mut()?;
        let v1 = s1.last_mut()?;

        Some(T2(v0, v1))
    }
    */
}

/*
impl<'a, T: 'a, V: Deref<Target = [T]>, const FLIP: bool> T2<&'a V, FLIP> {
    pub fn split_first(self) -> Option<(T2<&'a T, FLIP>, T2<&'a [T], FLIP>)> {
        let T2(r0, r1) = self;
        T2(r0.deref(), r1.deref()).split_first()
    }

    pub fn split_last(self) -> Option<(T2<&'a T, FLIP>, T2<&'a [T], FLIP>)> {
        let T2(r0, r1) = self;
        T2(r0.deref(), r1.deref()).split_last()
    }

    pub fn split_at(self, mid: T2<usize, FLIP>) -> (T2<&'a [T], FLIP>, T2<&'a [T], FLIP>) {
        let T2(r0, r1) = self;
        T2(r0.deref(), r1.deref()).split_at(mid)
    }

    pub fn split_back_at(self, mid: T2<usize, FLIP>) -> (T2<&'a [T], FLIP>, T2<&'a [T], FLIP>) {
        let T2(r0, r1) = self;
        T2(r0.deref(), r1.deref()).split_back_at(mid)
    }

    pub fn first(self) -> Option<T2<&'a T, FLIP>> {
        let T2(r0, r1) = self;
        T2(r0.deref(), r1.deref()).first()
    }

    pub fn last(self) -> Option<T2<&'a T, FLIP>> {
        let T2(r0, r1) = self;
        T2(r0.deref(), r1.deref()).last()
    }
}

impl<'a, T: 'a, V: DerefMut<Target = [T]>, const FLIP: bool> T2<&'a mut V, FLIP> {
    pub fn split_first_mut(self) -> Option<(T2<&'a mut T, FLIP>, T2<&'a mut [T], FLIP>)> {
        let T2(r0, r1) = self;
        T2(r0.deref_mut(), r1.deref_mut()).split_first_mut()
    }

    pub fn split_last_mut(self) -> Option<(T2<&'a mut T, FLIP>, T2<&'a mut [T], FLIP>)> {
        let T2(r0, r1) = self;
        T2(r0.deref_mut(), r1.deref_mut()).split_last_mut()
    }

    pub fn split_at_mut(
        self,
        mid: T2<usize, FLIP>,
    ) -> (T2<&'a mut [T], FLIP>, T2<&'a mut [T], FLIP>) {
        let T2(r0, r1) = self;
        T2(r0.deref_mut(), r1.deref_mut()).split_at_mut(mid)
    }

    pub fn split_back_at_mut(
        self,
        mid: T2<usize, FLIP>,
    ) -> (T2<&'a mut [T], FLIP>, T2<&'a mut [T], FLIP>) {
        let T2(r0, r1) = self;
        T2(r0.deref_mut(), r1.deref_mut()).split_back_at_mut(mid)
    }

    pub fn first_mut(self) -> Option<T2<&'a mut T, FLIP>> {
        let T2(r0, r1) = self;
        T2(r0.deref_mut(), r1.deref_mut()).first_mut()
    }

    pub fn last_mut(self) -> Option<T2<&'a mut T, FLIP>> {
        let T2(r0, r1) = self;
        T2(r0.deref_mut(), r1.deref_mut()).last_mut()
    }
}
*/
