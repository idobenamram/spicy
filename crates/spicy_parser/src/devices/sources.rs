use crate::{
    netlist_types::Phasor,
    netlist_types::{CurrentBranchIndex, NodeIndex},
    netlist_waveform::WaveForm,
};

#[derive(Debug, Clone)]
pub struct IndependentSourceSpec {
    pub name: String,
    // TODO: where span?
    pub positive: NodeIndex,
    pub negative: NodeIndex,
    pub current_branch: CurrentBranchIndex,
    pub dc: Option<WaveForm>,
    pub ac: Option<Phasor>,
}

impl IndependentSourceSpec {
    pub fn new(
        name: String,
        positive: NodeIndex,
        negative: NodeIndex,
        current_branch: CurrentBranchIndex,
    ) -> Self {
        Self {
            name,
            positive,
            negative,
            current_branch,
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
