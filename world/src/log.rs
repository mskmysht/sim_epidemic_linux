use crate::world::commons::HealthType;

use std::{collections::VecDeque, fmt::Display, io};

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
    pub mode: HistgramType,
    pub days: f64,
}

#[derive(Default)]
pub struct LocalStepLog {
    hist: Option<HistInfo>,
    infct: Option<InfectionCntInfo>,
    health: Option<HealthDiff>,
}

impl LocalStepLog {
    pub fn set_infect(&mut self, prev_n_infects: u32, curr_n_infects: u32) {
        self.infct = Some(InfectionCntInfo::new(prev_n_infects, curr_n_infects))
    }

    pub fn set_hist(&mut self, mode: HistgramType, days: f64) {
        self.hist = Some(HistInfo { mode, days })
    }

    pub fn set_health(&mut self, from: HealthType, to: HealthType) {
        if from != to {
            self.health = Some(HealthDiff { from, to })
        }
    }
}

#[derive(Default)]
pub struct MyLog {
    hists: Vec<HistInfo>,
    health_counts: VecDeque<HealthLog>,
    infcts: Vec<InfectionCntInfo>,
}

impl Display for MyLog {
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

impl MyLog {
    pub fn reset(&mut self, n_susceptible: u32, n_symptomatic: u32, n_asymptomatic: u32) {
        let mut cnt = EnumMap::default();
        cnt[&HealthType::Susceptible] = n_susceptible;
        cnt[&HealthType::Symptomatic] = n_symptomatic;
        cnt[&HealthType::Asymptomatic] = n_asymptomatic;
        self.health_counts.clear();
        self.health_counts.push_front(HealthLog(cnt));
    }

    pub fn n_infected(&self) -> u32 {
        self.health_counts[0].n_infected()
    }

    pub fn apply(&mut self, local: LocalStepLog) {
        if let Some(h) = local.hist {
            self.hists.push(h);
        }
        if let Some(hd) = local.health {
            self.health_counts[0].apply_difference(hd);
        }
        if let Some(i) = local.infct {
            self.infcts.push(i);
        }
    }

    pub fn push(&mut self) {
        self.health_counts.push_front(self.health_counts[0].clone());
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

#[derive(Clone, Default, Debug)]
pub struct HealthLog(EnumMap<HealthType, u32>);

impl HealthLog {
    fn apply_difference(&mut self, hd: HealthDiff) {
        self.0[&hd.from] -= 1;
        self.0[&hd.to] += 1;
    }

    fn n_infected(&self) -> u32 {
        self.0[&HealthType::Symptomatic] + self.0[&HealthType::Asymptomatic]
    }
}
