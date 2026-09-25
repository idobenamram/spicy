use super::stamp::NodePairStamp;
use crate::matrix::SolverMatrix;
use ndarray::Array2;
use spicy_parser::Span;
use spicy_parser::devices::ResistorSpec;
use spicy_parser::netlist_types::NodeIndex;
use spicy_parser::node_mapping::NodeMapping;

#[derive(Debug, Clone)]
pub struct Resistor {
    // Stored for diagnostics / SPICE compatibility; not used by the solver yet.
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub span: Span,
    pub positive: NodeIndex,
    pub negative: NodeIndex,
    /// Effective resistance (Ohms): instance/model/default value, with `scale`
    /// and `m` applied.
    pub resistance: f64,
    /// Effective AC resistance (Ohms). Defaults to the DC value; `scale` and `m` applied.
    pub ac: f64,
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

        let ac = spec
            .ac
            .as_ref()
            .map(|v| v.get_value())
            .unwrap_or(resistance);

        Self {
            name: spec.name.clone(),
            span: spec.span,
            positive: spec.positive,
            negative: spec.negative,
            // `scale` multiplies the resistance and `m` puts m copies in parallel,
            // so the conductance is m / (R * scale), as in ngspice (restemp.c).
            resistance: resistance * scale / m,
            ac: ac * scale / m,
            temp,
            dtemp,
            tc1,
            tc2,
            noisy,
            stamp: NodePairStamp::uninitialized(),
        }
    }

    /// Stamp DC MNA contributions for a resistor into the solver matrix.
    pub(crate) fn stamp_dc(&self, m: &mut SolverMatrix) {
        let conductance = 1.0 / self.resistance;

        if let Some(index) = self.stamp.pos_pos {
            *m.get_mut_nnz(index) += conductance;
        }
        if let Some(index) = self.stamp.neg_neg {
            *m.get_mut_nnz(index) += conductance;
        }
        if let Some((pos_neg, neg_pos)) = self.stamp.off_diagonals {
            *m.get_mut_nnz(pos_neg) -= conductance;
            *m.get_mut_nnz(neg_pos) -= conductance;
        }
    }

    /// Stamp AC small-signal admittance for a resistor into the real part matrix.
    pub(crate) fn stamp_ac(&self, ar: &mut Array2<f64>, node_mapping: &NodeMapping) {
        let g = 1.0 / self.ac;
        let node1 = node_mapping.mna_node_index(self.positive);
        let node2 = node_mapping.mna_node_index(self.negative);

        if let Some(n1) = node1 {
            ar[[n1, n1]] += g;
        }
        if let Some(n2) = node2 {
            ar[[n2, n2]] += g;
        }
        if let (Some(n1), Some(n2)) = (node1, node2) {
            ar[[n1, n2]] -= g;
            ar[[n2, n1]] -= g;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::parse_netlist;

    fn resistor(line: &str) -> Resistor {
        let deck = parse_netlist(&format!("resistor\n{line}\n.end\n"));
        Resistor::from_spec(&deck.devices.resistors[0])
    }

    // ngspice restemp.c: the conductance is m / (R * scale), for DC and AC.

    #[test]
    fn m_puts_copies_in_parallel() {
        let plain = resistor("R1 a 0 1k");
        let r = resistor("R1 a 0 1k m=2");
        assert_eq!(r.resistance, plain.resistance / 2.0);
        assert_eq!(r.ac, plain.ac / 2.0);
    }

    #[test]
    fn scale_multiplies_the_resistance() {
        let plain = resistor("R1 a 0 1k");
        let r = resistor("R1 a 0 1k scale=3");
        assert_eq!(r.resistance, plain.resistance * 3.0);
        assert_eq!(r.ac, plain.ac * 3.0);
    }

    #[test]
    fn m_and_scale_combine() {
        let plain = resistor("R1 a 0 1k");
        let r = resistor("R1 a 0 1k m=2 scale=3");
        assert_eq!(r.resistance, plain.resistance * 3.0 / 2.0);
        assert_eq!(r.ac, plain.ac * 3.0 / 2.0);
    }
}
