use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Debug, Clone)]
pub struct TraceFile {
    pub initial: std::collections::HashMap<String, Value>,
    pub steps: Vec<Step>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Step {
    #[serde(rename = "array")]
    Array {
        line: u32,
        name: String,
        index: usize,
        value: Value,
    },
    #[serde(rename = "number")]
    Number {
        line: u32,
        name: String,
        value: Value,
    },
    #[serde(rename = "step")]
    Step {
        line: u32,
    },
}


