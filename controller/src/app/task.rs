use poem_openapi::{Enum, Object};
use postgres_types::{FromSql, ToSql};

#[derive(Enum, Clone, Debug, Default, FromSql, ToSql)]
pub enum TaskState {
    #[default]
    Pending,
    Assigned,
    Running,
    Failed,
    Succeeded,
}

/// Task
#[derive(Object, Clone, Debug)]
#[oai(rename_all = "camelCase")]
pub struct Task {
    /// Task id.
    pub id: String,
    /// Task state.
    pub state: TaskState,
}

impl Task {
    pub fn new(id: String, state: TaskState) -> Self {
        Self { id, state }
    }
}
