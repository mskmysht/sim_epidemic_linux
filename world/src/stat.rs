use crate::world::commons::HealthType;

use std::{
    fmt::Display,
    io,
    ops::{Index, IndexMut},
};

use enum_map::{macros, Enum, EnumMap};

use csv::Writer;

pub struct InfectionCntInfo {
    pub org_v: u32,
    pub new_v: u32,
}

impl InfectionCntInfo {
    pub fn new(org_v: u32, new_v: u32) -> Self {
        Self { org_v, new_v }
    }
}

#[derive(macros::Enum, Clone)]
pub enum HistgramType {
    HistIncub,
    HistRecov,
    HistDeath,
}

pub struct HistInfo {
    mode: HistgramType,
    days: f64,
}

impl HistInfo {
    pub fn new(mode: HistgramType, days: f64) -> Self {
        Self { mode, days }
    }
}

#[derive(Default)]
pub struct Stat {
    pub hists: Vec<HistInfo>,
    pub infcts: Vec<InfectionCntInfo>,
    pub health_counts: Vec<HealthCount>,
}

impl Display for Stat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "[{}], {:?}",
            self.health_counts.len(),
            self.health_counts[0].0
        )?;
        write!(f, "infcts: {}", self.infcts.len())
    }
}

impl Stat {
    pub fn reset(&mut self) {
        self.health_counts.clear();
    }

    pub fn n_infected(&self) -> u32 {
        self.health_counts[0].n_infected()
    }

    pub fn write(&self, name: &str, dir: &str) -> io::Result<()> {
        use std::path;
        let p = path::Path::new(dir);
        let p = p.join(format!("{}_log.csv", name));
        let mut wtr = Writer::from_path(p)?;
        for ht in <HealthType as Enum>::ALL.iter() {
            wtr.write_field(format!("{:?}", ht))?;
        }
        wtr.write_record(None::<&[u8]>)?;
        for cnt in self.health_counts.iter().rev() {
            for (_, v) in cnt.0.iter() {
                wtr.write_field(format!("{}", v))?;
            }
            wtr.write_record(None::<&[u8]>)?;
        }
        wtr.flush()?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct HealthDiff {
    from: HealthType,
    to: HealthType,
}

impl HealthDiff {
    pub fn new(from: HealthType, to: HealthType) -> Self {
        Self { from, to }
    }
}

#[derive(Clone, Default, Debug)]
pub struct HealthCount(EnumMap<HealthType, u32>);

impl HealthCount {
    pub fn apply_difference(&mut self, hd: HealthDiff) {
        self.0[&hd.from] -= 1;
        self.0[&hd.to] += 1;
    }

    fn n_infected(&self) -> u32 {
        self.0[&HealthType::Symptomatic] + self.0[&HealthType::Asymptomatic]
    }
}

impl<'a> Index<&'a HealthType> for HealthCount {
    type Output = u32;

    fn index(&self, index: &'a HealthType) -> &Self::Output {
        &self.0[index]
    }
}

impl<'a> IndexMut<&'a HealthType> for HealthCount {
    fn index_mut(&mut self, index: &HealthType) -> &mut Self::Output {
        &mut self.0[index]
    }
}
