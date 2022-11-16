use crate::{
    commons::HealthType,
    stat::{HistInfo, InfectionCntInfo},
    util::enum_map::{Enum, EnumMap},
};
use csv::Writer;
use std::{collections::VecDeque, error, fmt::Display};

#[derive(Default)]
pub struct StepLog {
    pub hists: Vec<HistInfo>,
    health_counts: VecDeque<HealthLog>,
    pub infcts: Vec<InfectionCntInfo>,
}

impl Display for StepLog {
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

impl StepLog {
    pub fn reset(&mut self, n_susceptible: usize, n_symptomatic: usize, n_asymptomatic: usize) {
        let mut cnt = EnumMap::default();
        cnt[&HealthType::Susceptible] = n_susceptible;
        cnt[&HealthType::Symptomatic] = n_symptomatic;
        cnt[&HealthType::Asymptomatic] = n_asymptomatic;
        self.health_counts.clear();
        self.health_counts.push_front(HealthLog(cnt));
    }

    fn n_infected(&self) -> usize {
        self.health_counts[0].n_infected()
    }

    pub fn push(&mut self) -> bool {
        self.health_counts.push_front(self.health_counts[0].clone());
        self.n_infected() == 0
    }

    pub fn apply_difference(&mut self, hd: HealthDiff) {
        self.health_counts[0].apply_difference(hd);
    }

    pub fn write(&self, name: &str, dir: &str) -> Result<(), Box<dyn error::Error>> {
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
pub struct HealthLog(EnumMap<HealthType, usize>);

impl HealthLog {
    fn apply_difference(&mut self, hd: HealthDiff) {
        self.0[&hd.from] -= 1;
        self.0[&hd.to] += 1;
    }

    fn n_infected(&self) -> usize {
        self.0[&HealthType::Symptomatic] + self.0[&HealthType::Asymptomatic]
    }
}
