use super::stamp::NodeVoltageSourceStamp;
use spicy_parser::devices::IndependentSourceSpec;
use spicy_parser::netlist_types::{CurrentBranchIndex, NodeIndex, Phasor};
use spicy_parser::netlist_waveform::WaveForm;
use spicy_parser::Value;

#[derive(Debug, Clone)]
pub struct IndependentSource {
    pub name: String,
    pub positive: NodeIndex,
    pub negative: NodeIndex,
    pub current_branch: CurrentBranchIndex,
    pub dc: WaveForm,
    pub ac: Option<Phasor>,
    pub stamp: NodeVoltageSourceStamp,
}

impl IndependentSource {
    pub fn from_spec(spec: &IndependentSourceSpec) -> Self {
        let dc = spec
            .dc
            .clone()
            .unwrap_or_else(|| WaveForm::Constant(Value::zero()));

        Self {
            name: spec.name.clone(),
            positive: spec.positive,
            negative: spec.negative,
            current_branch: spec.current_branch,
            dc,
            ac: spec.ac.clone(),
            stamp: NodeVoltageSourceStamp::unitialized(),
        }
    }
}

