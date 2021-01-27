use std::collections::VecDeque;

use crate::{
    agent::*,
    commons::MRef,
    dyn_struct::{DynStruct, Reset},
};

#[derive(Default, Debug)]
pub struct ContactInfo {
    time_stamp: i32,
    pub agent: MRef<Agent>,
}

impl Reset<ContactInfo> for ContactInfo {
    fn reset(&mut self) {
        self.time_stamp = Default::default();
        self.agent = Default::default();
    }
}

impl ContactInfo {
    pub fn init(&mut self, ar: MRef<Agent>, tm: i32) {
        self.agent = ar;
        self.time_stamp = tm;
    }
}

pub fn add_new_cinfo(dsc: &mut DynStruct<ContactInfo>, ar: MRef<Agent>, br: MRef<Agent>, tm: i32) {
    let a = &mut ar.lock().unwrap();
    let cr = dsc.new();
    cr.lock().unwrap().init(br.clone(), tm);
    a.contact_info_list.push_back(cr);
}

pub fn remove_old_cinfo(dsc: &mut DynStruct<ContactInfo>, ar: MRef<Agent>, tm: i32) {
    let a = &mut ar.lock().unwrap();

    if a.contact_info_list.is_empty() {
        return;
    }

    let mut old_list = VecDeque::new();
    loop {
        if let Some(cr) = a.contact_info_list.back() {
            let time_stamp = cr.lock().unwrap().time_stamp;
            if time_stamp <= tm {
                old_list.push_front(a.contact_info_list.pop_back().unwrap());
            } else {
                break;
            }
        } else {
            break;
        }
    }

    if !old_list.is_empty() {
        dsc.restore_all(&mut old_list);
    }
}
