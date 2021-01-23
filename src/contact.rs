use std::sync::{Arc, Mutex};

use crate::{
    agent::*,
    common_types::MRef,
    iter::{MyIter, Next, Prev},
};

#[derive(Default, Debug)]
pub struct ContactInfo {
    pub prev: Option<MRef<ContactInfo>>,
    pub next: Option<MRef<ContactInfo>>,
    time_stamp: i32,
    pub agent: MRef<Agent>,
}

impl ContactInfo {
    fn init(&mut self, ar: MRef<Agent>, tm: i32) {
        self.prev = None;
        self.next = None;
        self.agent = ar;
        self.time_stamp = tm;
    }
}

impl Next<MRef<ContactInfo>> for MRef<ContactInfo> {
    fn next(&self) -> Option<MRef<ContactInfo>> {
        self.lock().unwrap().next.clone()
    }
}

impl Prev<MRef<ContactInfo>> for MRef<ContactInfo> {
    fn prev(&self) -> Option<MRef<ContactInfo>> {
        self.lock().unwrap().prev.clone()
    }
}

static ALLOC_UNIT: usize = 2048;

// todo: replace to DynStruct
#[derive(Default)]
pub struct ContactState {
    cinfos: Vec<MRef<ContactInfo>>,
    pub free_cinfo: Option<MRef<ContactInfo>>,
    pub idx: usize,
}

impl ContactState {
    fn new_cinfo(&mut self, a: MRef<Agent>, tm: i32) -> MRef<ContactInfo> {
        if self.idx == self.cinfos.len() {
            self.cinfos.reserve_exact(ALLOC_UNIT);
            for _ in 0..ALLOC_UNIT - 1 {
                let c = ContactInfo::default();
                // c.init(a, tm);
                self.cinfos.push(Arc::new(Mutex::new(c)));
            }
        }
        let cr = &self.cinfos[self.idx];
        {
            cr.lock().unwrap().init(a, tm);
        }
        self.idx += 1;
        cr.clone()
    }

    pub fn add_new_cinfo(&mut self, ar: MRef<Agent>, br: MRef<Agent>, tm: i32) {
        // let c = &mut self.cinfos[p];
        let a = &mut ar.lock().unwrap();
        let cr = match &a.contact_info_head {
            None => {
                let cr = self.new_cinfo(br.clone(), tm);
                // a.contact_info_head = Some(cr.clone());
                a.contact_info_tail = Some(cr.clone());
                cr
            }
            Some(hr) => {
                let cr = self.new_cinfo(br.clone(), tm);
                {
                    let c = &mut cr.lock().unwrap();
                    c.next = Some(hr.clone());
                }
                {
                    let h = &mut hr.lock().unwrap();
                    h.prev = Some(cr.clone());
                }
                // *hr = cr.clone(); // a.contact_info_head = Some(cr.clone());
                cr
            }
        };
        a.contact_info_head = Some(cr.clone());
    }

    pub fn remove_old_cinfo(&mut self, ar: MRef<Agent>, tm: i32) {
        let a = &mut ar.lock().unwrap();
        let ah = a.contact_info_head.clone();
        let at = a.contact_info_tail.clone();
        if let (Some(hr), Some(tr)) = (ah, at) {
            let gb_tail = tr;
            // let mut op = a.contact_info_tail;
            let mut ocr = None;
            for cr in MyIter::new(a.contact_info_tail.clone()).rev() {
                ocr = Some(cr.clone());
                let c = cr.lock().unwrap(); // &self.cinfos[p as usize];
                if c.time_stamp > tm {
                    break;
                }
            }
            let mut gb_head = hr;
            match ocr {
                None => {
                    a.contact_info_head = None;
                    a.contact_info_tail = None;
                }
                Some(cr) => {
                    let c = &mut cr.lock().unwrap(); // &mut self.cinfos[cr as usize];
                    if let Some(nr) = &c.next {
                        gb_head = nr.clone();
                        a.contact_info_tail = Some(cr.clone());
                        c.next = None;
                    } else {
                        return;
                    }
                }
            }
            // self.cinfos[gb_tail as usize].next = Some(self.free_cinfo);
            gb_tail.lock().unwrap().next = self.free_cinfo.clone();
            self.free_cinfo = Some(gb_head.clone());
        }
    }
}
