use crate::netlist_models::InductorModel;
use crate::{
    Span,
    expr::Value,
    netlist_types::{NodeIndex, CurrentBranchIndex},
};

#[derive(Debug, Clone)]
pub struct InductorSpec {
    pub name: String,
    pub span: Span,
    pub positive: NodeIndex,
    pub negative: NodeIndex,
    pub current_branch: CurrentBranchIndex,
    pub inductance: Option<Value>,
    pub model: Option<InductorModel>,
    pub nt: Option<Value>,
    pub m: Option<Value>,
    pub scale: Option<Value>,
    pub temp: Option<Value>,
    pub dtemp: Option<Value>,
    pub tc1: Option<Value>,
    pub tc2: Option<Value>,
    pub ic: Option<Value>,
}

impl InductorSpec {
    pub fn new(
        name: String,
        span: Span,
        positive: NodeIndex,
        negative: NodeIndex,
        current_branch: CurrentBranchIndex,
    ) -> Self {
        Self {
            name,
            span,
            positive,
            negative,
            current_branch,
            inductance: None,
            model: None,
            nt: None,
            m: None,
            scale: None,
            temp: None,
            dtemp: None,
            tc1: None,
            tc2: None,
            ic: None,
        }
    }

    pub fn set_inductance(&mut self, value: Value) {
        self.inductance = Some(value);
    }

    pub fn set_model(&mut self, model: InductorModel) {
        self.model = Some(model);
    }

    pub fn set_nt(&mut self, value: Value) {
        self.nt = Some(value);
    }

    pub fn set_m(&mut self, value: Value) {
        self.m = Some(value);
    }

    pub fn set_scale(&mut self, value: Value) {
        self.scale = Some(value);
    }
    pub fn set_temp(&mut self, value: Value) {
        self.temp = Some(value);
    }

    pub fn set_dtemp(&mut self, value: Value) {
        self.dtemp = Some(value);
    }
    pub fn set_tc1(&mut self, value: Value) {
        self.tc1 = Some(value);
    }

    pub fn set_tc2(&mut self, value: Value) {
        self.tc2 = Some(value);
    }
    pub fn set_ic(&mut self, value: Value) {
        self.ic = Some(value);
    }
}