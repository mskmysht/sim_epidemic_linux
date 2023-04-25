use std::ops;

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

macro_rules! impl_parts_per {
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

#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Percentage(pub f64);

#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Permille(pub f64);

impl_parts_per!(Percentage, 100.0);
impl_parts_per!(Permille, 1000.0);
