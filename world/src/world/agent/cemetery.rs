use super::Agent;

pub struct Cemetery(Vec<Agent>);

impl Cemetery {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn add(&mut self, a: Agent) {
        self.0.push(a);
    }
}
