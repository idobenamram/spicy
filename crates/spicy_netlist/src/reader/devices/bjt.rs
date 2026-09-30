use crate::reader::netlist_models::BjtModel;
use crate::reader::{Span, Value, netlist_types::NodeIndex};

#[derive(Debug, Clone)]
pub struct BjtSpec {
    pub name: String,
    pub span: Span,
    pub collector: NodeIndex,
    pub base: NodeIndex,
    pub emitter: NodeIndex,
    /// The `.model` card's name, as the card spells it.
    pub model_name: String,
    pub model: BjtModel,
    pub area: Option<Value>,
    pub m: Option<Value>,
    pub off: Option<bool>,
    pub ic_vbe: Option<Value>,
    pub ic_vce: Option<Value>,
    pub temp: Option<Value>,
    pub dtemp: Option<Value>,
}

impl BjtSpec {
    pub fn new(
        name: String,
        span: Span,
        collector: NodeIndex,
        base: NodeIndex,
        emitter: NodeIndex,
        model_name: String,
        model: BjtModel,
    ) -> Self {
        Self {
            name,
            span,
            collector,
            base,
            emitter,
            model_name,
            model,
            area: None,
            m: None,
            off: None,
            ic_vbe: None,
            ic_vce: None,
            temp: None,
            dtemp: None,
        }
    }

    pub fn set_area(&mut self, value: Value) {
        self.area = Some(value);
    }

    pub fn set_m(&mut self, value: Value) {
        self.m = Some(value);
    }

    pub fn set_off(&mut self, value: bool) {
        self.off = Some(value);
    }

    pub fn set_ic(&mut self, vbe: Value, vce: Option<Value>) {
        self.ic_vbe = Some(vbe);
        self.ic_vce = vce;
    }

    pub fn set_temp(&mut self, value: Value) {
        self.temp = Some(value);
    }

    pub fn set_dtemp(&mut self, value: Value) {
        self.dtemp = Some(value);
    }
}
