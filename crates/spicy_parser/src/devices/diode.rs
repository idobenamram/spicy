use crate::netlist_models::DiodeModel;
use crate::{Span, Value, netlist_types::NodeIndex};

#[derive(Debug, Clone)]
pub struct DiodeSpec {
    pub name: String,
    pub span: Span,
    pub positive: NodeIndex,
    pub negative: NodeIndex,
    pub model: DiodeModel,
    pub area: Option<Value>,
    pub m: Option<Value>,
    pub pj: Option<Value>,
    pub off: Option<bool>,
    pub ic: Option<Value>,
    pub temp: Option<Value>,
    pub dtemp: Option<Value>,
    pub lm: Option<Value>,
    pub wm: Option<Value>,
    pub lp: Option<Value>,
    pub wp: Option<Value>,
}

impl DiodeSpec {
    pub fn new(
        name: String,
        span: Span,
        positive: NodeIndex,
        negative: NodeIndex,
        model: DiodeModel,
    ) -> Self {
        Self {
            name,
            span,
            positive,
            negative,
            model,
            area: None,
            m: None,
            pj: None,
            off: None,
            ic: None,
            temp: None,
            dtemp: None,
            lm: None,
            wm: None,
            lp: None,
            wp: None,
        }
    }

    pub fn set_area(&mut self, value: Value) {
        self.area = Some(value);
    }
    pub fn set_m(&mut self, value: Value) {
        self.m = Some(value);
    }
    pub fn set_pj(&mut self, value: Value) {
        self.pj = Some(value);
    }
    pub fn set_off(&mut self, value: bool) {
        self.off = Some(value);
    }
    pub fn set_ic(&mut self, value: Value) {
        self.ic = Some(value);
    }
    pub fn set_temp(&mut self, value: Value) {
        self.temp = Some(value);
    }
    pub fn set_dtemp(&mut self, value: Value) {
        self.dtemp = Some(value);
    }
    pub fn set_lm(&mut self, value: Value) {
        self.lm = Some(value);
    }
    pub fn set_wm(&mut self, value: Value) {
        self.wm = Some(value);
    }
    pub fn set_lp(&mut self, value: Value) {
        self.lp = Some(value);
    }
    pub fn set_wp(&mut self, value: Value) {
        self.wp = Some(value);
    }
}
