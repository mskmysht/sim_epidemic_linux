use crate::{
    agent::Agent,
    commons::{RuntimeParams, WorldParams},
};
use std::vec::Drain;

// #[derive(Default)]
pub struct ContactInfo {
    pub agent: Agent,
    time_stamp: u64,
}

// impl Reset<ContactInfo> for ContactInfo {
//     fn reset(&mut self) {
//         self.time_stamp = Default::default();
//         self.agent = Default::default();
//     }
// }

impl ContactInfo {
    fn new(agent: Agent, time_stamp: u64) -> Self {
        Self { agent, time_stamp }
    }

    // pub fn init(&mut self, ar: MRef<Agent>, tm: i32) {
    //     self.agent = ar;
    //     self.time_stamp = tm;
    // }
}

/// a vector guarantees the ascending order of `time_stamp`
#[derive(Default)]
pub struct Contacts(Vec<ContactInfo>);

impl Contacts {
    const RETENTION_PERIOD: u64 = 14;
    pub fn add(&mut self, agent: Agent, step: u64) {
        self.0.push(ContactInfo::new(agent, step))
    }

    pub fn drain_agent<T, F: Fn(Agent) -> Option<T>>(&mut self, f: F) -> Vec<T> {
        self.0
            .drain(..)
            .into_iter()
            .filter_map(|ci| f(ci.agent))
            .collect()
    }

    pub fn remove_old(&mut self, rp: &RuntimeParams, wp: &WorldParams) {
        let old_step = rp.step - wp.steps_per_day * Self::RETENTION_PERIOD;
        if let Some(i) = self.0.iter().position(|ci| ci.time_stamp > old_step) {
            self.0.drain(..i);
        } else {
            self.0.drain(..);
        }
    }
}

// pub fn add_new_cinfo(dsc: &mut DynStruct<ContactInfo>, ar: MRef<Agent>, br: MRef<Agent>, tm: i32) {
//     let a = &mut ar.lock().unwrap();
//     let cr = dsc.new();
//     cr.lock().unwrap().init(br.clone(), tm);
//     a.contact_info_list.push_back(cr);
// }

// pub fn remove_old_cinfo(dsc: &mut DynStruct<ContactInfo>, ar: MRef<Agent>, tm: i32) {
//     let a = &mut ar.lock().unwrap();

//     if a.contact_info_list.is_empty() {
//         return;
//     }

//     let mut old_list = VecDeque::new();
//     loop {
//         if let Some(cr) = a.contact_info_list.back() {
//             let time_stamp = cr.lock().unwrap().time_stamp;
//             if time_stamp <= tm {
//                 old_list.push_front(a.contact_info_list.pop_back().unwrap());
//             } else {
//                 break;
//             }
//         } else {
//             break;
//         }
//     }

//     if !old_list.is_empty() {
//         dsc.restore_all(&mut old_list);
//     }
// }
