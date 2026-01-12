use super::stamp::NodePairStamp;
use spicy_parser::devices::ResistorSpec;
use spicy_parser::netlist_types::NodeIndex;
use spicy_parser::Span;

#[derive(Debug, Clone)]
pub struct Resistor {
    // Stored for diagnostics / SPICE compatibility; not used by the solver yet.
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub span: Span,
    pub positive: NodeIndex,
    pub negative: NodeIndex,
    /// Resistor value (Ohms) resolved from instance/model/default.
    pub resistance: f64,
    /// Optional AC override value (Ohms). If not provided, defaults to `resistance`.
    pub ac: f64,
    /// Multiplier; replicates the resistor in parallel.
    #[allow(dead_code)]
    pub m: f64,
    /// Scaling factor applied to the resistance value.
    #[allow(dead_code)]
    pub scale: f64,
    /// Instance temperature (typically in °C).
    #[allow(dead_code)]
    pub temp: f64,
    /// Instance temperature delta applied on top of the ambient/circuit temperature.
    #[allow(dead_code)]
    pub dtemp: f64,
    /// First-order temperature coefficient.
    #[allow(dead_code)]
    pub tc1: f64,
    /// Second-order temperature coefficient.
    #[allow(dead_code)]
    pub tc2: f64,
    /// Enable/disable including this resistor in noise analysis.
    #[allow(dead_code)]
    pub noisy: bool,
    pub stamp: NodePairStamp,
}

impl Resistor {
    /// Compile a parsed resistor "spec" into a simulation-ready resistor.
    ///
    /// No validation is performed at this stage; missing parameters are replaced with defaults.
    pub fn from_spec(spec: &ResistorSpec) -> Self {
        // Resolve resistance (instance overrides model). If absent everywhere, use a small default.
        let resistance = spec
            .resistance
            .as_ref()
            .map(|v| v.get_value())
            .or_else(|| {
                spec.model
                    .as_ref()
                    .and_then(|m| m.resistance.as_ref().map(|v| v.get_value()))
            })
            .unwrap_or(1e-03);

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

        // TODO: get this from the config of the deck
        let temp = spec.temp.as_ref().map(|v| v.get_value()).unwrap_or(27.0);
        let dtemp = spec.dtemp.as_ref().map(|v| v.get_value()).unwrap_or(0.0);
        let noisy = spec.noisy.unwrap_or(true);

        let ac = spec.ac.as_ref().map(|v| v.get_value()).unwrap_or(resistance);

        Self {
            name: spec.name.clone(),
            span: spec.span,
            positive: spec.positive,
            negative: spec.negative,
            resistance,
            ac,
            m,
            scale,
            temp,
            dtemp,
            tc1,
            tc2,
            noisy,
            stamp: NodePairStamp::unitialized(),
        }
    }
}
