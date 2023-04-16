use std::ops;

use rand::Rng;
use rand_distr::StandardNormal;

#[derive(Default, Debug)]
pub struct DistInfo<T> {
    pub min: T,
    pub mode: T,
    pub max: T,
}

impl<T> DistInfo<T> {
    pub fn new(min: T, mode: T, max: T) -> Self {
        Self { min, mode, max }
    }
}

pub fn at_least_once_hit_in(shots: f64, prob: f64) -> bool {
    rand::thread_rng().gen::<f64>() > (1.0 - prob).powf(shots)
}

static EXP_BASE: f64 = 0.02;

fn revise_prob(x: f64, mode: f64) -> f64 {
    let a = mode / (1.0 - mode);
    a * x / ((a - 1.0) * x + 1.0)
}

pub fn modified_prob<T>(x: f64, p: &DistInfo<T>) -> T
where
    T: ops::Sub<Output = T>
        + ops::Div<T, Output = f64>
        + ops::Mul<f64, Output = T>
        + ops::Add<Output = T>
        + Copy,
{
    let span = p.max - p.min;
    (span * revise_prob(x, (p.mode - p.min) / span)) + p.min
}

pub fn random_exp<R: Rng>(rng: &mut R) -> f64 {
    (EXP_BASE.powf(rng.gen()) - EXP_BASE) / (1.0 - EXP_BASE)
}

pub fn random_mk<R: Rng>(rng: &mut R, mode: f64, kurt: f64) -> f64 {
    let mut x = if mode <= 0.0 {
        random_exp(rng) * 2.0 - 1.0
    } else if mode >= 1.0 {
        1.0 - random_exp(rng) * 2.0
    } else {
        rng.sample::<f64, _>(StandardNormal) / 3.0
    };

    x = if x < -2.0 {
        -1.0
    } else if x < -1.0 {
        -2.0 - x
    } else if x > 2.0 {
        1.0
    } else if x > 1.0 {
        2.0 - x
    } else {
        x
    };

    if kurt != 0.0 {
        let b = f64::powf(2.0, -kurt);
        x = if x < 0.0 {
            (x + 1.0).powf(b) - 1.0
        } else {
            1.0 - (1.0 - x).powf(b)
        };
    }

    revise_prob((x + 1.0) / 2.0, mode)
}

pub fn my_random<T, R>(rng: &mut R, p: &DistInfo<T>) -> T
where
    T: ops::Sub<Output = T>
        + ops::Div<T, Output = f64>
        + ops::Mul<f64, Output = T>
        + ops::Add<Output = T>
        + PartialEq
        + Copy,
    R: Rng,
{
    if p.max == p.min {
        p.min
    } else {
        let span = p.max - p.min;
        span * random_mk(rng, (p.mode - p.min) / span, 0.0) + p.min
    }
}

pub fn random_with_corr<T, R>(rng: &mut R, p: &DistInfo<T>, x: f64, m_x: f64, c: f64) -> T
where
    T: ops::Sub<Output = T>
        + ops::Div<T, Output = f64>
        + ops::Mul<f64, Output = T>
        + ops::Add<Output = T>
        + PartialEq
        + Copy,
    R: Rng,
{
    if c == 0.0 {
        my_random(rng, p)
    } else {
        let m = (p.mode - p.min) / (p.max - p.min);
        let m_y = if c < 0.0 { 1.0 - m } else { m };
        let mut y = m_y * (1.0 - m_x) * x / (m_x * (1.0 - m_y) - (m_x - m_y) * x);
        y += (random_mk(rng, y * 0.1 + m_y * 0.9, 0.0) - y) * (1.0 - c.abs());
        if c < 0.0 {
            y = 1.0 - y;
        }
        (p.max - p.min) * y + p.min
    }
}

/*
pub struct Gaussian {
    z: f64,
    second_time: bool,
}

impl Gaussian {
    pub fn new() -> Gaussian {
        Gaussian {
            z: 0.0,
            second_time: false,
        }
    }

    fn gen<R: Rng>(&mut self, rng: &mut R) -> f64 {
        if self.second_time {
            self.second_time = false;
            self.z
        } else {
            self.second_time = true;
            let r = (-2. * rng.gen::<f64>().ln()).sqrt();
            let th = rng.gen::<f64>() * std::f64::consts::PI * 2.0;
            self.z = r * th.cos();
            r * th.sin()
        }
    }

    pub fn random_guassian<R: Rng>(&mut self, rng: &mut R) -> f64 {
        self.gen(rng)
    }
}
*/
