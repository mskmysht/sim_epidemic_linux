use super::{
    super::commons::{ParamsForStep, RuntimeParams},
    DaysTo, HealthState,
};
use crate::{
    log::{HistgramType, LocalStepLog},
    util::random,
};

use std::f64;

const MAX_DAYS_FOR_RECOVERY: f64 = 7.0;
const TOXICITY_LEVEL: f64 = 0.5;

fn exacerbation(reproductivity: f64) -> f64 {
    reproductivity.powf(1.0 / 3.0)
}

#[derive(Debug, PartialEq, Eq)]
pub enum InfMode {
    Asym,
    Sym,
}

#[derive(Debug)]
pub(super) struct InfectionParam {
    pub virus_variant: usize,
    pub days_infected: f64,
    pub days_diseased: f64,
    pub immunity: f64,
    on_recovery: bool,
    severity: f64,
}

impl InfectionParam {
    pub fn new(immunity: f64, virus_variant: usize) -> Self {
        Self {
            virus_variant,
            days_infected: 0.0,
            days_diseased: 0.0,
            immunity,
            on_recovery: false,
            severity: 0.0,
        }
    }

    pub fn check_infection(
        &self,
        immunity: f64,
        d: f64,
        days_to_onset: f64,
        pfs: &ParamsForStep,
    ) -> bool {
        // check contact and infection
        let virus_x = pfs.vr_info[self.virus_variant].reproductivity;
        let infec_d_max = pfs.rp.infec_dst * virus_x.sqrt();
        if d > infec_d_max {
            return false;
        }

        let exacerbate = exacerbation(virus_x);
        let contag_delay = pfs.rp.contag_delay / exacerbate;
        let contag_peak = pfs.rp.contag_peak / exacerbate;

        if self.days_infected <= contag_delay {
            return false;
        }

        let time_factor = 1f64.min(
            (self.days_infected - contag_delay) / (contag_peak.min(days_to_onset) - contag_delay),
        );
        let distance_factor = 1f64.min(((infec_d_max - d) / 2.0).powf(2.0));
        let infec_prob = if virus_x < 1.0 {
            pfs.rp.infec.r() * virus_x
        } else {
            1.0 - (1.0 - pfs.rp.infec.r()) / virus_x
        };

        if !random::at_least_once_hit_in(
            pfs.wp.days_per_step(),
            infec_prob * time_factor * distance_factor * (1.0 - immunity),
        ) {
            return false;
        }
        true
    }

    pub fn step<const IS_IN_HOSPITAL: bool>(
        &mut self,
        days_to: &mut DaysTo,
        inf_mode: &mut InfMode,
        vp: &Option<VaccinationParam>,
        log: &mut LocalStepLog,
        pfs: &ParamsForStep,
    ) -> Option<HealthState> {
        fn new_recover(
            days_to: &mut DaysTo,
            rp: &RuntimeParams,
            virus_variant: usize,
        ) -> RecoverParam {
            RecoverParam::new(days_to.setup_acquired_immunity(&rp), virus_variant)
        }

        self.days_infected += pfs.wp.days_per_step();
        if inf_mode == &InfMode::Sym {
            self.days_diseased += pfs.wp.days_per_step();
        }

        if self.on_recovery {
            self.severity -= 1.0 / MAX_DAYS_FOR_RECOVERY * pfs.wp.days_per_step();
            // recovered
            if self.severity <= 0.0 {
                if inf_mode == &InfMode::Sym {
                    // SET_HIST(hist_recov, days_diseased);
                    log.set_hist(HistgramType::HistRecov, self.days_diseased);
                }
                return Some(HealthState::Recovered(new_recover(
                    days_to,
                    pfs.rp,
                    self.virus_variant,
                )));
            }
            return None;
        }

        let vr_info = &pfs.vr_info[self.virus_variant];
        let excrbt = exacerbation(vr_info.reproductivity);

        let days_to_recov = self.get_days_to_recov::<IS_IN_HOSPITAL>(days_to, pfs.rp);
        if inf_mode == &InfMode::Asym {
            if self.days_infected < days_to.onset / excrbt {
                if self.days_infected > days_to_recov {
                    return Some(HealthState::Recovered(new_recover(
                        days_to,
                        pfs.rp,
                        self.virus_variant,
                    )));
                }
                return None;
            }
        }

        let d_svr = {
            let mut v = 1.0 / (days_to.die - days_to.onset) * excrbt * pfs.wp.days_per_step();
            if let Some(vp) = &vp {
                v /= vp.vax_sv_effc(pfs);
            }
            if self.severity > TOXICITY_LEVEL {
                v *= vr_info.toxicity;
            }
            v
        };

        self.severity += d_svr;

        // died
        if self.severity >= 1.0 {
            // SET_HIST(hist_death, days_diseased)
            log.set_hist(HistgramType::HistDeath, self.days_diseased);
            return Some(HealthState::Died);
        }

        if self.days_infected > days_to_recov {
            self.on_recovery = true;
        }

        if inf_mode == &InfMode::Asym {
            // SET_HIST(hist_incub, days_infected)
            log.set_hist(HistgramType::HistIncub, self.days_infected);
            *inf_mode = InfMode::Sym;
        }

        None
    }

    fn get_days_to_recov<const IS_IN_HOSPITAL: bool>(
        &self,
        days_to: &DaysTo,
        rp: &RuntimeParams,
    ) -> f64 {
        let mut v = (1.0 - self.immunity) * days_to.recover;
        if IS_IN_HOSPITAL {
            v *= 1.0 - rp.therapy_effc.r()
        }
        v
    }
}

#[derive(Debug)]
pub(super) struct RecoverParam {
    pub virus_variant: usize,
    pub days_recovered: f64,
    pub immunity: f64,
}

impl RecoverParam {
    pub fn new(immunity: f64, virus_variant: usize) -> Self {
        Self {
            immunity,
            virus_variant,
            days_recovered: 0.0,
        }
    }

    pub fn step(
        &mut self,
        days_to: &mut DaysTo,
        activeness: f64,
        age: f64,
        pfs: &ParamsForStep,
    ) -> Option<HealthState> {
        self.days_recovered += pfs.wp.days_per_step();
        if self.days_recovered > days_to.expire_immunity {
            Some(days_to.expire_immunity(activeness, age, pfs))
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub(super) struct VaccinationParam {
    pub vaccine_type: usize,
    pub dose_date: f64,
    pub immunity: f64,
    //[todo] days_vaccinated: f64,
    //[todo] first_dose_date: f64,
}

impl VaccinationParam {
    fn vax_sv_effc(&self, pfs: &ParamsForStep) -> f64 {
        let days_vaccinated = self.days_vaccinated(pfs);
        let span = self.vx_span(pfs);
        let vcn_sv_effc = pfs.wp.vcn_sv_effc.r();
        if days_vaccinated < span {
            1.0 + days_vaccinated / span * vcn_sv_effc
        } else if days_vaccinated < pfs.wp.vcn_e_delay + span {
            (1.0 / (1.0 - vcn_sv_effc) - 1.0 - vcn_sv_effc) * (days_vaccinated - span)
                / pfs.wp.vcn_e_delay
                + 1.0
                + vcn_sv_effc
        } else {
            1.0 / (1.0 - vcn_sv_effc)
        }
    }

    fn vx_span(&self, pfs: &ParamsForStep) -> f64 {
        pfs.vx_info[self.vaccine_type].interval as f64
    }

    fn days_vaccinated(&self, pfs: &ParamsForStep) -> f64 {
        pfs.rp.step as f64 * pfs.wp.days_per_step() - self.dose_date
    }

    fn new_immunity(&self, pfs: &ParamsForStep) -> Option<f64> {
        let days_vaccinated = self.days_vaccinated(pfs);
        let span = self.vx_span(pfs);
        if days_vaccinated < span {
            // only the first dose
            Some(days_vaccinated * pfs.wp.vcn_1st_effc.r() / span)
        } else if days_vaccinated < pfs.wp.vcn_e_delay + span {
            // not fully vaccinated yet
            Some(
                ((pfs.wp.vcn_max_effc - pfs.wp.vcn_1st_effc)
                    * ((days_vaccinated - span) / pfs.wp.vcn_e_delay)
                    + pfs.wp.vcn_1st_effc)
                    .r(),
            )
        } else if days_vaccinated < pfs.wp.vcn_e_delay + span + pfs.wp.vcn_e_decay {
            Some(pfs.wp.vcn_max_effc.r())
        } else if days_vaccinated < pfs.wp.vcn_e_decay + span + pfs.wp.vcn_e_period {
            Some(pfs.wp.vcn_e_decay + span + pfs.wp.vcn_e_period - days_vaccinated)
        } else {
            None
        }
    }

    pub fn step(
        &mut self,
        days_to: &mut DaysTo,
        activeness: f64,
        age: f64,
        pfs: &ParamsForStep,
    ) -> Option<HealthState> {
        if let Some(i) = self.new_immunity(pfs) {
            self.immunity = i;
            None
        } else {
            Some(days_to.expire_immunity(activeness, age, pfs))
        }
    }
}
