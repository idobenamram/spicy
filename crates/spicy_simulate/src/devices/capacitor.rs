use super::stamp::NodePairStamp;
use spicy_parser::devices::CapacitorSpec;
use spicy_parser::netlist_types::NodeIndex;
use spicy_parser::Span;

#[derive(Debug, Clone)]
pub struct Capacitor {
    pub name: String,
    // Stored for diagnostics / SPICE compatibility; not used by the solver yet.
    #[allow(dead_code)]
    pub span: Span,
    pub positive: NodeIndex,
    pub negative: NodeIndex,
    pub capacitance: f64,
    #[allow(dead_code)]
    pub m: f64,
    #[allow(dead_code)]
    pub scale: f64,
    #[allow(dead_code)]
    pub temp: f64,
    #[allow(dead_code)]
    pub dtemp: f64,
    #[allow(dead_code)]
    pub tc1: f64,
    #[allow(dead_code)]
    pub tc2: f64,
    pub ic: f64,
    pub stamp: NodePairStamp,
}

impl Capacitor {
    pub fn from_spec(spec: &CapacitorSpec) -> Self {
        let capacitance = spec
            .capacitance
            .as_ref()
            .map(|v| v.get_value())
            .or_else(|| {
                spec.model
                    .as_ref()
                    .and_then(|m| m.cap.as_ref().map(|v| v.get_value()))
            })
            .unwrap_or(0.0);

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
            capacitance,
            m,
            scale,
            temp,
            dtemp,
            tc1,
            tc2,
            ic,
            stamp: NodePairStamp::unitialized(),
        }
    }
}

