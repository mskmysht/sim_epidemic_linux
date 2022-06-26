use crate::stat::{HealthInfo, HistInfo, InfectionCntInfo};

pub struct StepLog {
    pub hists: Vec<HistInfo>,
    pub healths: Vec<HealthInfo>,
    pub infcts: Vec<InfectionCntInfo>,
}
