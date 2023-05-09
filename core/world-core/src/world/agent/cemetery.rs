use super::Agent;

pub struct Cemetery(Vec<Agent>);

impl Cemetery {
    pub fn new(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    pub fn clear(&mut self, agents: &mut Vec<Agent>) {
        agents.append(&mut self.0);
    }

    pub fn add(&mut self, a: Agent) {
        self.0.push(a);
    }
}
