use crate::agent::*;

pub type ContactInfoId = usize;
#[derive(Default, Debug)]
pub struct ContactInfo {
    prev: Option<ContactInfoId>,
    next: Option<ContactInfoId>,
    time_stamp: i32,
    agent: Box<Agent>,
}

impl ContactInfo {
    fn init(&mut self, a: Box<Agent>, tm: i32) {
        self.prev = None;
        self.next = None;
        self.agent = a;
        self.time_stamp = tm;
    }
}

static ALLOC_UNIT: usize = 2048;

#[derive(Default)]
pub struct ContractState {
    cinfos: Vec<ContactInfo>,
    free_cinfo: ContactInfoId,
}

impl ContractState {
    fn new_cinfo(&mut self, a: Box<Agent>, tm: i32) -> ContactInfoId {
        if self.free_cinfo == self.cinfos.len() {
            self.cinfos.reserve_exact(ALLOC_UNIT);
            let mut c = ContactInfo::default();
            c.init(a, tm);
            self.cinfos.push(c);
        } else {
            self.cinfos[self.free_cinfo].init(a, tm);
        }
        self.free_cinfo += 1;
        self.free_cinfo - 1
    }
    pub fn add_new_cinfo(&mut self, a: &mut Agent, b: Box<Agent>, tm: i32) {
        let p = self.new_cinfo(b, tm);
        let c = &mut self.cinfos[p];

        match a.contact_info_head {
            Some(q) => {
                c.next = a.contact_info_head;
                a.contact_info_head = Some(p);
                self.cinfos[q].prev = Some(p);
            }
            None => {
                a.contact_info_head = Some(p);
                a.contact_info_tail = Some(p);
            }
        }
    }
    pub fn remove_old_cinfo(&mut self, a: &mut Agent, tm: i32) {
        if let (Some(p), Some(q)) = (a.contact_info_head, a.contact_info_tail) {
            let gb_tail = q;
            let mut op = a.contact_info_tail;
            loop {
                match op {
                    None => break,
                    Some(p) => {
                        let c = &self.cinfos[p as usize];
                        if c.time_stamp > tm {
                            break;
                        }
                        op = c.prev;
                    }
                }
            }
            let mut gb_head = p;
            match op {
                None => {
                    a.contact_info_head = None;
                    a.contact_info_tail = None;
                }
                Some(q) => {
                    let c = &mut self.cinfos[q as usize];
                    if let Some(r) = c.next {
                        gb_head = r;
                        a.contact_info_tail = op;
                        c.next = None;
                    } else {
                        return;
                    }
                }
            }
            self.cinfos[gb_tail as usize].next = Some(self.free_cinfo);
            self.free_cinfo = gb_head;
        }
    }
}
