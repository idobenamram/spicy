use std::collections::HashMap;

use crate::expr::Value;



#[derive(Debug, Clone, PartialEq)]
pub enum Attr {
    Value(Value),
    String(String),
    Param(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attributes(HashMap<String, Attr>);

impl Attributes {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn get_value(&self, key: &str) -> Option<&Value> {
        if let Some(attr) = self.0.get(key) {
            if let Attr::Value(value) = attr {
                return Some(value);
            }
        }
        None
    }

    pub fn get_string(&self, key: &str) -> Option<&String> {
        if let Some(attr) = self.0.get(key) {
            if let Attr::String(value) = attr {
                return Some(value);
            }
        }
        None
    }

    pub fn insert(&mut self, key: String, value: Attr) -> Option<Attr> {
        self.0.insert(key, value)
    }

    pub fn from_iter(attrs: Vec<(String, Attr)>) -> Self {
        Self(HashMap::from_iter(attrs))
    }
}