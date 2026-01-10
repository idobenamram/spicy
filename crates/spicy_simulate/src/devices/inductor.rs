use super::stamp::NodeBranchPairStamp;
use spicy_parser::devices::InductorSpec;
use spicy_parser::netlist_types::{CurrentBranchIndex, NodeIndex};
use spicy_parser::Span;

#[derive(Debug, Clone)]
pub struct Inductor {
    pub name: String,
    pub span: Span,
    pub positive: NodeIndex,
    pub negative: NodeIndex,
    pub current_branch: CurrentBranchIndex,
    pub inductance: f64,
    pub nt: f64,
    pub m: f64,
    pub scale: f64,
    pub temp: f64,
    pub dtemp: f64,
    pub tc1: f64,
    pub tc2: f64,
    pub ic: f64,
    pub stamp: NodeBranchPairStamp,
}

impl Inductor {
    pub fn from_spec(spec: &InductorSpec) -> Self {
        let inductance = spec
            .inductance
            .as_ref()
            .map(|v| v.get_value())
            .or_else(|| {
                spec.model
                    .as_ref()
                    .and_then(|m| m.inductance.as_ref().map(|v| v.get_value()))
            })
            .unwrap_or(0.0);

        let nt = spec.nt.as_ref().map(|v| v.get_value()).unwrap_or(1.0);
        let m = spec.m.as_ref().map(|v| v.get_value()).unwrap_or(1.0);
        let scale = spec.scale.as_ref().map(|v| v.get_value()).unwrap_or(1.0);

        let tc1 = spec
            .tc1
            .as_ref()
            .map(|v| v.get_value())
            .or_else(|| {
                spec.model
                    .as_ref()
                    .and_then(|m| m.tc1.as_ref().map(|v| v.get_value()))
            })
            .unwrap_or(0.0);
        let tc2 = spec
            .tc2
            .as_ref()
            .map(|v| v.get_value())
            .or_else(|| {
                spec.model
                    .as_ref()
                    .and_then(|m| m.tc2.as_ref().map(|v| v.get_value()))
            })
            .unwrap_or(0.0);

        // TODO: get this from the deck config
        let temp = spec.temp.as_ref().map(|v| v.get_value()).unwrap_or(27.0);
        let dtemp = spec.dtemp.as_ref().map(|v| v.get_value()).unwrap_or(0.0);

        let ic = spec.ic.as_ref().map(|v| v.get_value()).unwrap_or(0.0);

        Self {
            name: spec.name.clone(),
            span: spec.span,
            positive: spec.positive,
            negative: spec.negative,
            current_branch: spec.current_branch,
            inductance,
            nt,
            m,
            scale,
            temp,
            dtemp,
            tc1,
            tc2,
            ic,
            stamp: NodeBranchPairStamp::unitialized(),
        }
    }
}

