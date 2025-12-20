use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value, json};

pub struct Recorder {
    output_path: PathBuf,
    initial: Map<String, Value>,
    steps: Vec<Value>,
}

impl Recorder {
    pub fn new<P: AsRef<Path>>(output_path: P) -> Self {
        Self {
            output_path: output_path.as_ref().to_path_buf(),
            initial: Map::new(),
            steps: Vec::new(),
        }
    }

    pub fn set_initial<V: Serialize>(&mut self, name: &str, value: &V) {
        match serde_json::to_value(value) {
            Ok(v) => {
                self.initial.insert(name.to_string(), v);
            }
            Err(_) => {
                // ignore values that cannot be serialized
            }
        }
    }

    pub fn push_array_step<V: Serialize>(
        &mut self,
        line: u32,
        name: &str,
        index: usize,
        value: &V,
    ) {
        if let Ok(v) = serde_json::to_value(value) {
            self.steps.push(json!({
                "line": line,
                "type": "array",
                "name": name,
                "index": index,
                "value": v
            }));
        }
    }

    pub fn push_number_step<V: Serialize>(&mut self, line: u32, name: &str, value: &V) {
        if let Ok(v) = serde_json::to_value(value) {
            self.steps.push(json!({
                "line": line,
                "type": "number",
                "name": name,
                "value": v
            }));
        }
    }

    pub fn push_step(&mut self, line: u32) {
        self.steps.push(json!({
            "line": line,
            "type": "step",
        }));
    }

    pub fn flush(&self) -> std::io::Result<()> {
        if let Some(parent) = self.output_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let mut root = Map::new();
        root.insert("initial".to_string(), Value::Object(self.initial.clone()));
        root.insert("steps".to_string(), Value::Array(self.steps.clone()));

        let json =
            serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_else(|_| "{}".to_string());
        fs::write(&self.output_path, json)
    }
}
