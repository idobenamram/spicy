use crate::{lexer::Span, netlist_types::ValueSuffix};

#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub value: f64,
    pub exponent: Option<f64>,
    pub suffix: Option<ValueSuffix>,
}

impl Value {
    pub fn new(value: f64, exponent: Option<f64>, suffix: Option<ValueSuffix>) -> Self {
        Self { value, exponent, suffix }
    }

    pub fn get_value(&self) -> f64 {
        let mut value = self.value;
        if let Some(exponent) = self.exponent {
            value *= 10.0f64.powf(exponent);
        }
        if let Some(suffix) = &self.suffix {
            value *= suffix.scale();
        }
        value
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    Const(f64),
    Value(Value),
    Ident(String),
    // Unary { op: char, e: Box<Expr> },       // +, -
    // Binary { op: char, l: Box<Expr>, r: Box<Expr> }, // + - * /
    // Add Call { fun, args } if you want sin(), etc.
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlaceholderId(pub u64);

#[derive(Debug, Default)]
pub struct PlaceholderMap {
    next: u64,
    pub map: std::collections::HashMap<PlaceholderId, (Expr, Span)>,
}

impl PlaceholderMap {
    pub fn fresh(&mut self, expr: Expr, span: Span) -> PlaceholderId {
        let id = PlaceholderId(self.next);
        self.next += 1;
        self.map.insert(id, (expr, span));
        id
    }
}

#[derive(Debug)]
pub struct ParamEnv<'a> {
    pub parent: Option<&'a ParamEnv<'a>>,
    pub map: std::collections::HashMap<String, Expr>, // store Expr; evaluation is later
}

impl<'a> ParamEnv<'a> {
    pub fn new_root() -> Self {
        Self { parent: None, map: Default::default() }
    }
    pub fn child(&'a self) -> ParamEnv<'a> {
        ParamEnv { parent: Some(self), map: Default::default() }
    }
    pub fn get(&self, k: &str) -> Option<&Expr> {
        self.map.get(k).or_else(|| self.parent.and_then(|p| p.get(k)))
    }
}