use super::stamp::NodePairStamp;
use crate::matrix::SolverMatrix;
use crate::unknowns::Layout;
use ndarray::Array2;
use spicy_circuit::{self as circuit, ResistorParams};

#[derive(Debug, Clone)]
pub struct Resistor {
    /// MNA row of the positive terminal; `None` for ground.
    pub positive: Option<usize>,
    /// MNA row of the negative terminal; `None` for ground.
    pub negative: Option<usize>,
    /// Effective resistance (Ω) of the `m` devices in parallel.
    pub resistance: f64,
    /// Effective resistance (Ω) in AC analysis.
    pub ac_resistance: f64,
    pub stamp: NodePairStamp,
}

impl Resistor {
    pub fn new(pins: &circuit::Resistor, params: &ResistorParams, layout: &Layout) -> Self {
        Self {
            positive: layout.node(pins.positive),
            negative: layout.node(pins.negative),
            // `m` devices in parallel divide the resistance, as in ngspice (restemp.c).
            resistance: params.r / params.m,
            ac_resistance: params.r_ac.unwrap_or(params.r) / params.m,
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
    pub(crate) fn stamp_ac(&self, ar: &mut Array2<f64>) {
        let g = 1.0 / self.ac_resistance;

        if let Some(n1) = self.positive {
            ar[[n1, n1]] += g;
        }
        if let Some(n2) = self.negative {
            ar[[n2, n2]] += g;
        }
        if let (Some(n1), Some(n2)) = (self.positive, self.negative) {
            ar[[n1, n2]] -= g;
            ar[[n2, n1]] -= g;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::build_devices;

    fn resistor(line: &str) -> Resistor {
        build_devices(&format!("resistor\n{line}\n.end\n"))
            .resistors
            .remove(0)
    }

    // ngspice restemp.c: the conductance is m / (R * scale), for DC and AC.

    #[test]
    fn m_puts_copies_in_parallel() {
        let plain = resistor("R1 a 0 1k");
        let r = resistor("R1 a 0 1k m=2");
        assert_eq!(r.resistance, plain.resistance / 2.0);
        assert_eq!(r.ac_resistance, plain.ac_resistance / 2.0);
    }

    #[test]
    fn scale_multiplies_the_resistance() {
        let plain = resistor("R1 a 0 1k");
        let r = resistor("R1 a 0 1k scale=3");
        assert_eq!(r.resistance, plain.resistance * 3.0);
        assert_eq!(r.ac_resistance, plain.ac_resistance * 3.0);
    }

    #[test]
    fn m_and_scale_combine() {
        let plain = resistor("R1 a 0 1k");
        let r = resistor("R1 a 0 1k m=2 scale=3");
        assert_eq!(r.resistance, plain.resistance * 3.0 / 2.0);
        assert_eq!(r.ac_resistance, plain.ac_resistance * 3.0 / 2.0);
    }
}
