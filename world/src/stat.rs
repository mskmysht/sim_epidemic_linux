use crate::world::commons::HealthType;

use std::{
    fs::File,
    ops::{Index, IndexMut},
    path::Path,
};

use arrow2::{
    array::{MutableArray, UInt32Vec},
    chunk::Chunk,
    datatypes::{DataType, Field, Schema},
    io::ipc::write::{Compression, FileWriter, WriteOptions},
};
use enum_map::{macros, Enum, EnumMap};

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
    pub health_stat: HealthStat,
}

impl Stat {
    pub fn reset(&mut self) {
        self.hists.clear();
        self.infcts.clear();
        self.health_stat = HealthStat::default();
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

    pub fn n_infected(&self) -> u32 {
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

#[derive(Default, Debug)]
pub struct HealthStat(EnumMap<HealthType, UInt32Vec>);

impl HealthStat {
    pub fn push(&mut self, count: HealthCount) {
        for health in &HealthType::ALL {
            self.0[health].push(Some(count[health]));
        }
    }

    pub fn export(&mut self, path: &Path) -> anyhow::Result<()> {
        let schema = Schema::from(
            HealthType::ALL
                .into_iter()
                .map(|h| Field::new(h.to_string(), DataType::UInt32, false))
                .collect::<Vec<_>>(),
        );
        let chunk = Chunk::try_new(self.0.iter_value_mut().map(|v| v.as_box()).collect())?;
        let mut writer = FileWriter::try_new(
            File::create(path)?,
            schema,
            None,
            WriteOptions {
                compression: Some(Compression::ZSTD),
            },
        )?;
        writer.write(&chunk, None)?;
        writer.finish()?;
        Ok(())
    }
}
