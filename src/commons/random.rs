use rand::Rng;
use rand_distr::StandardNormal;

use super::DistInfo;

static EXP_BASE: f64 = 0.02;

pub fn revise_prob(x: f64, mode: f64) -> f64 {
    let a = mode / (1.0 - mode);
    a * x / ((a - 1.0) * x + 1.0)
}

pub fn modified_prob(x: f64, p: &DistInfo) -> f64 {
    let span = p.max - p.min;
    revise_prob(x, (p.mode - p.min) / span) * span + p.min
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

pub fn my_random<R: Rng>(rng: &mut R, p: &DistInfo) -> f64 {
    if p.max == p.min {
        p.min
    } else {
        let span = p.max - p.min;
        random_mk(rng, (p.mode - p.min) / span, 0.0) * span + p.min
    }
}

pub fn random_with_corr<R: Rng>(rng: &mut R, p: &DistInfo, a: &ActivenessEffect, c: f64) -> f64 {
    if c == 0.0 {
        my_random(rng, p)
    } else {
        let m = (p.mode - p.min) / (p.max - p.min);
        let m_y = if c < 0.0 { 1.0 - m } else { m };
        let mut y = m_y * (1.0 - a.m_x) * a.x / (a.m_x * (1.0 - m_y) - (a.m_x - m_y) * a.x);
        y += (random_mk(rng, y * 0.1 + m_y * 0.9, 0.0) - y) * (1.0 - c.abs());
        if c < 0.0 {
            y = 1.0 - y;
        }
        y * (p.max - p.min) + p.min
    }
}

// pub struct Gaussian {
//     z: f64,
//     second_time: bool,
// }

// impl Gaussian {
//     pub fn new() -> Gaussian {
//         Gaussian {
//             z: 0.0,
//             second_time: false,
//         }
//     }

//     fn gen<R: Rng>(&mut self, rng: &mut R) -> f64 {
//         if self.second_time {
//             self.second_time = false;
//             self.z
//         } else {
//             self.second_time = true;
//             let r = (-2. * rng.gen::<f64>().ln()).sqrt();
//             let th = rng.gen::<f64>() * std::f64::consts::PI * 2.0;
//             self.z = r * th.cos();
//             r * th.sin()
//         }
//     }

//     pub fn random_guassian<R: Rng>(&mut self, rng: &mut R) -> f64 {
//         self.gen(rng)
//     }
// }

// static GUASSIAN: MRef<Gaussian> = Arc::new(Mutex::new(Gaussian::new()));

pub struct ActivenessEffect {
    x: f64,
    m_x: f64,
}

impl ActivenessEffect {
    pub fn new(x: f64, m_x: f64) -> Self {
        Self { x, m_x }
    }
}
