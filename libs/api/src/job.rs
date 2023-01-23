use poem_openapi::Object;

#[derive(Object, Clone, Debug, serde::Deserialize, serde::Serialize)]
#[oai(rename_all = "camelCase")]
pub struct WorldParams {
    #[oai(validator(minimum(value = "1", exclusive = false)))]
    pub population_size: u64,
}

#[derive(Object, Clone, Debug, serde::Deserialize, serde::Serialize)]
#[oai(rename_all = "camelCase")]
pub struct JobParam {
    #[oai(validator(minimum(value = "1", exclusive = false)))]
    pub stop_at: u32,
    pub world_params: WorldParams,
    // scenario: Scenario,
    // vaccines
    // variants
    // gatherings
}
