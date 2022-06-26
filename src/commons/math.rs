use std::ops;

use rand_distr::num_traits::Pow;

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

impl ops::DivAssign<f64> for Point {
    fn div_assign(&mut self, rhs: f64) {
        *self = Self {
            x: self.x / rhs,
            y: self.y / rhs,
        };
    }
}

pub type Percentage = PartsPerPo10<2>;
pub type Permille = PartsPerPo10<3>;

#[derive(Clone, Copy, Debug)]
struct PartsPerPo10<const E: u8>(f64);

impl<const E: u8> PartsPerPo10<E> {
    pub fn new(v: f64) -> Self {
        Self(v)
    }

    pub fn r(&self) -> f64 {
        self.0 / 10.0.pow(E)
    }
}

impl<const E: u8> ops::Add for PartsPerPo10<E> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl<const E: u8> ops::Sub for PartsPerPo10<E> {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl<const E: u8> ops::Mul<f64> for PartsPerPo10<E> {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl<const E: u8> ops::Div<f64> for PartsPerPo10<E> {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self(self.0 / rhs)
    }
}

#[derive(Default, Clone, Debug)]
pub struct Range {
    pub length: i32,
    pub location: i32,
}
