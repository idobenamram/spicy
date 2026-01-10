use crate::{
    Span, Value, netlist_models::ResistorModel, netlist_types::NodeIndex,
};

#[derive(Debug, Clone)]
pub struct ResistorSpec {
    pub name: String,
    pub span: Span,
    pub positive: NodeIndex,
    pub negative: NodeIndex,
    pub resistance: Option<Value>,
    /// Optional resistor model (`.model`) associated with this instance.
    pub model: Option<ResistorModel>,
    /// Optional resistance value override used for AC analysis (if supported by the simulator).
    pub ac: Option<Value>,
    /// Multiplier; replicates the resistor in parallel.
    pub m: Option<Value>,
    /// Scaling factor applied to the resistance value.
    pub scale: Option<Value>,
    /// Instance temperature (typically in °C, depending on netlist conventions).
    pub temp: Option<Value>,
    /// Instance temperature delta applied on top of the ambient/circuit temperature.
    pub dtemp: Option<Value>,
    /// First-order temperature coefficient.
    pub tc1: Option<Value>,
    /// Second-order temperature coefficient.
    pub tc2: Option<Value>,
    /// Enable/disable including this resistor in noise analysis (if supported).
    pub noisy: Option<bool>,
}

impl ResistorSpec {
    pub fn new(name: String, span: Span, positive: NodeIndex, negative: NodeIndex) -> Self {
        Self {
            name,
            span,
            positive,
            negative,
            resistance: None,
            model: None,
            ac: None,
            m: None,
            scale: None,
            temp: None,
            dtemp: None,
            tc1: None,
            tc2: None,
            noisy: None,
        }
    }

    pub fn set_resistance(&mut self, value: Value) {
        self.resistance = Some(value);
    }
    pub fn set_model(&mut self, model: ResistorModel) {
        self.model = Some(model);
    }

    pub fn set_ac(&mut self, value: Value) {
        self.ac = Some(value);
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
    pub fn set_noisy(&mut self, value: bool) {
        self.noisy = Some(value);
    }
}
