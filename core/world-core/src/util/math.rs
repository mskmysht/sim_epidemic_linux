use std::{iter::FromIterator, ops};

use rand::Rng;
use rand_distr::Open01;

pub fn quantize(p: f64, res_rate: f64, n: usize) -> usize {
    let i = (p * res_rate).floor() as usize;
    if i >= n {
        n - 1
    } else {
        i
    }
}

pub fn dequantize(i: usize, res_rate: f64) -> f64 {
    (i as f64) / res_rate
}

#[derive(Default, PartialEq, Clone, Copy, Debug)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn map<F: Fn(f64) -> f64>(self, f: F) -> Self {
        Self {
            x: f(self.x),
            y: f(self.y),
        }
    }

    pub fn apply_mut<F: FnMut(&mut f64)>(&mut self, mut f: F) {
        f(&mut self.x);
        f(&mut self.y);
    }

    const CENTERED_BIAS: f64 = 0.25;
    pub fn centered_bias(&self) -> f64 {
        let a = Self::CENTERED_BIAS / (1.0 - Self::CENTERED_BIAS);
        a / (1.0 - (1.0 - a) * self.x.abs().max(self.y.abs()))
    }
}

impl ops::Add for Point {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl<'a, 'b> ops::Add<&'b Point> for &'a Point {
    type Output = Point;

    fn add(self, rhs: &'b Point) -> Self::Output {
        Point {
            x: (self.x + rhs.x),
            y: (self.y + rhs.y),
        }
    }
}

impl ops::AddAssign for Point {
    fn add_assign(&mut self, rhs: Self) {
        *self = Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        };
    }
}

impl ops::Sub for Point {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl<'a, 'b> ops::Sub<&'b Point> for &'a Point {
    type Output = Point;

    fn sub(self, rhs: &'b Point) -> Self::Output {
        Point {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl ops::Sub<f64> for Point {
    type Output = Self;

    fn sub(self, rhs: f64) -> Self::Output {
        Self {
            x: self.x - rhs,
            y: self.y - rhs,
        }
    }
}

impl ops::SubAssign for Point {
    fn sub_assign(&mut self, rhs: Self) {
        *self = Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        };
    }
}

impl ops::Mul<f64> for Point {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl<'a, 'b> ops::Mul<&'b f64> for &'a Point {
    type Output = Point;

    fn mul(self, rhs: &'b f64) -> Self::Output {
        Point {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl ops::MulAssign<f64> for Point {
    fn mul_assign(&mut self, rhs: f64) {
        *self = Self {
            x: self.x * rhs,
            y: self.y * rhs,
        };
    }
}

impl ops::Div<f64> for Point {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl<'a, 'b> ops::Div<&'b f64> for &'a Point {
    type Output = Point;

    fn div(self, rhs: &'b f64) -> Self::Output {
        Point {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl ops::DivAssign<f64> for Point {
    fn div_assign(&mut self, rhs: f64) {
        *self = Self {
            x: self.x / rhs,
            y: self.y / rhs,
        };
    }
}

macro_rules! num_field {
    ($t:ty, $e:expr) => {
        impl From<f64> for $t {
            fn from(v: f64) -> Self {
                Self(v)
            }
        }

        impl $t {
            pub const fn new(v: f64) -> Self {
                Self(v)
            }

            pub fn r(&self) -> f64 {
                self.0 / $e
            }

            pub fn min<'a>(&'a self, other: &'a Self) -> &'a Self {
                if self.0 < other.0 {
                    &self
                } else {
                    &other
                }
            }

            pub fn max<'a>(&'a self, other: &'a Self) -> &'a Self {
                if self.0 > other.0 {
                    &self
                } else {
                    &other
                }
            }
        }

        impl ops::Add for $t {
            type Output = Self;

            fn add(self, rhs: Self) -> Self::Output {
                Self(self.0 + rhs.0)
            }
        }

        impl ops::Sub for $t {
            type Output = Self;

            fn sub(self, rhs: Self) -> Self::Output {
                Self(self.0 - rhs.0)
            }
        }

        impl ops::Mul<f64> for $t {
            type Output = Self;

            fn mul(self, rhs: f64) -> Self::Output {
                Self(self.0 * rhs)
            }
        }

        impl ops::Div<f64> for $t {
            type Output = Self;

            fn div(self, rhs: f64) -> Self::Output {
                Self(self.0 / rhs)
            }
        }

        impl ops::Div for $t {
            type Output = f64;

            fn div(self, rhs: Self) -> Self::Output {
                self.0 / rhs.0
            }
        }
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Percentage(f64);

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Permille(f64);

num_field!(Percentage, 100.0);
num_field!(Permille, 1000.0);

#[derive(Default, Clone, Debug)]
pub struct Range {
    pub length: i32,
    pub location: i32,
}

pub fn reservoir_sampling(n: usize, k: usize) -> Vec<usize> {
    assert!(n >= k);
    let mut r = Vec::from_iter(0..k);
    if n == k || k == 0 {
        return r;
    }

    let rng = &mut rand::thread_rng();
    let kf = k as f64;
    // exp(log(random())/k)
    let mut w = (f64::ln(rng.sample(Open01)) / kf).exp();
    let mut i = k - 1;
    loop {
        i += 1 + (f64::ln(rng.sample(Open01)) / (1.0 - w).ln()).floor() as usize;
        if i < n {
            r[rng.gen_range(0..k)] = i;
            w *= (f64::ln(rng.sample(Open01)) / kf).exp()
        } else {
            break;
        }
    }
    r
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_reservoir_sampling() {
        use super::reservoir_sampling;
        for k in 0..10 {
            let s = reservoir_sampling(10, k);
            println!("{s:?}");
            assert!(s.len() == k, "s.len() = {}, k = {}", s.len(), k);
            for i in s {
                assert!(i < 10);
            }
        }
    }
}
