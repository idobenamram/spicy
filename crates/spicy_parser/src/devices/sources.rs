use crate::{netlist_types::NodeIndex, netlist_types::Phasor, netlist_waveform::WaveForm};

#[derive(Debug, Clone)]
pub struct IndependentSourceSpec {
    pub name: String,
    // TODO: where span?
    pub positive: NodeIndex,
    pub negative: NodeIndex,
    pub dc: Option<WaveForm>,
    pub ac: Option<Phasor>,
}

impl IndependentSourceSpec {
    pub fn new(name: String, positive: NodeIndex, negative: NodeIndex) -> Self {
        Self {
            name,
            positive,
            negative,
            dc: None,
            ac: None,
        }
    }

    pub fn set_dc(&mut self, value: WaveForm) {
        self.dc = Some(value);
    }

    pub fn set_ac(&mut self, value: Phasor) {
        self.ac = Some(value);
    }
}
