use std::sync::Arc;

use parking_lot::RwLock;

use crate::types::job::{Job, JobConfig};

#[derive(Clone)]
pub struct BatchClient(Arc<RwLock<ClientCore>>);

impl BatchClient {
    pub fn connect(addr: &str) -> Self {
        Self(Arc::new(RwLock::new(ClientCore::new(addr))))
    }

    pub fn create(&self, config: JobConfig) -> String {
        let mut cl = self.0.write();
        cl.create(config)
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.0.read().get(id)
    }

    pub fn get_all(&self) -> Vec<Job> {
        self.0.read().get_all()
    }
}

struct ClientCore;

impl ClientCore {
    pub fn new(addr: &str) -> Self {
        Self
    }

    pub fn create(&mut self, config: JobConfig) -> String {
        todo!()
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        todo!()
    }

    pub fn get_all(&self) -> Vec<Job> {
        todo!()
    }
}
