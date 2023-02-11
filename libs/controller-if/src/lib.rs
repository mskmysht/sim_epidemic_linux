#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ProcessInfo {
    pub world_id: String,
    pub exit_status: bool,
}
