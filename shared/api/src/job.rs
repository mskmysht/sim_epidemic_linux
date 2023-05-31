use poem_openapi::Object;

#[derive(Object, Clone, Debug, serde::Deserialize, serde::Serialize, Default)]
#[oai(rename_all = "camelCase")]
pub struct WorldParams {
    #[oai(validator(minimum(value = "1", exclusive = false)))]
    pub population_size: u32,
    pub infected: f64,
}

#[derive(Object, Clone, Debug, serde::Deserialize, serde::Serialize)]
#[oai(rename_all = "camelCase")]
pub struct JobParam {
    #[oai(validator(minimum(value = "1", exclusive = false)))]
    pub stop_at: u32,
    pub world_params: WorldParams,
    pub scenario: Vec<Operation>,
    // vaccines
    // variants
    // gatherings
}

#[derive(Object, Clone, Debug, serde::Deserialize, serde::Serialize)]
#[oai(rename_all = "camelCase")]
pub struct Operation {
    pub condition: String,
    pub assignments: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use poem_openapi::types::ParseFromJSON;
    use serde_json::json;

    use scenario_operation::{Assignment, Condition, ConditionField, MyField};

    #[test]
    fn test_scenario() {
        let v = json!({
            "condition": "days == 10",
            "operations": [
                {"immediate": {
                    "gatheringFrequency": 0.1
                }}
            ]
        });
        let s: super::Operation = ParseFromJSON::parse_from_json(Some(v)).unwrap();
        let cond = s.condition.parse::<Condition<ConditionField>>().unwrap();
        let ops: Vec<Assignment<MyField>> = serde_json::from_value(s.assignments.clone()).unwrap();
        println!("{:?}, {:?}", cond, ops);
    }
}
