use poem_openapi::{Enum, Object};

#[derive(Enum, Clone, Debug, Default)]
pub enum TaskState {
    #[default]
    Pending,
    Assigned,
    Running,
    Failed,
    Succeeded,
}

#[derive(Object, Clone, Debug)]
#[oai(rename_all = "camelCase")]
pub struct Task {
    /// Task id.
    pub id: u64,
    /// Task state.
    pub state: TaskState,
}

impl Task {
    pub fn new(id: u64, state: TaskState) -> Self {
        Self { id, state }
    }
}
