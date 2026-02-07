use crate::netlist_models::BjtModel;
use crate::{Span, Value, netlist_types::NodeIndex};

#[derive(Debug, Clone)]
pub struct BjtSpec {
    pub name: String,
    pub span: Span,
    pub collector: NodeIndex,
    pub base: NodeIndex,
    pub emitter: NodeIndex,
    pub model: BjtModel,
    pub area: Option<Value>,
    pub m: Option<Value>,
    pub off: Option<bool>,
    pub ic_vbe: Option<Value>,
    pub ic_vce: Option<Value>,
}

impl BjtSpec {
    pub fn new(
        name: String,
        span: Span,
        collector: NodeIndex,
        base: NodeIndex,
        emitter: NodeIndex,
        model: BjtModel,
    ) -> Self {
        Self {
            name,
            span,
            collector,
            base,
            emitter,
            model,
            area: None,
            m: None,
            off: None,
            ic_vbe: None,
            ic_vce: None,
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
}
