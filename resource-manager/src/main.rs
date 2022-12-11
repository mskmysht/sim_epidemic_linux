use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;
use ulid::Ulid;

use batch::types::job::{Job, Status};

pub type JobTable = Arc<RwLock<HashMap<String, Job>>>;

fn main() {
    let db = JobTable::default();
    let mut db = db.write();
    let id = Ulid::new().to_string();
    db.insert(
        id.clone(),
        Job {
            id: id.clone(),
            config: todo!(),
            status: Status::Scheduled,
        },
    );
}
