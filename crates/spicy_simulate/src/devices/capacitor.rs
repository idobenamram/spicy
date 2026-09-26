use super::stamp::NodePairStamp;
use crate::matrix::SolverMatrix;
use crate::unknowns::Layout;
use ndarray::Array2;
use spicy_circuit::{self as circuit, CapacitorParams};

#[derive(Debug, Clone)]
pub struct Capacitor {
    /// MNA row of the positive terminal; `None` for ground.
    pub positive: Option<usize>,
    /// MNA row of the negative terminal; `None` for ground.
    pub negative: Option<usize>,
    /// Effective capacitance (F) of the `m` devices in parallel.
    pub capacitance: f64,
    /// Initial voltage (V), used when the transient skips the operating point.
    pub ic: f64,
    pub stamp: NodePairStamp,
}

impl Capacitor {
    pub fn new(pins: &circuit::Capacitor, params: &CapacitorParams, layout: &Layout) -> Self {
        Self {
            positive: layout.node(pins.positive),
            negative: layout.node(pins.negative),
            // `m` devices in parallel multiply the capacitance, as in ngspice (capload.c).
            capacitance: params.c * params.m,
            ic: params.ic,
            stamp: NodePairStamp::uninitialized(),
        }
    }

    /// Stamp transient companion model (conductance + history current) into the solver matrix.
    pub(crate) fn stamp_trans(&self, m: &mut SolverMatrix, g: f64, i: f64) {
        if let Some(index) = self.stamp.pos_pos {
            *m.get_mut_nnz(index) += g;
        }
        if let Some(index) = self.stamp.neg_neg {
            *m.get_mut_nnz(index) += g;
        }
        if let Some((pos_neg, neg_pos)) = self.stamp.off_diagonals {
            *m.get_mut_nnz(pos_neg) -= g;
            *m.get_mut_nnz(neg_pos) -= g;
        }

        if let Some(p) = self.positive {
            *m.get_mut_rhs(p) += i;
        }
        if let Some(n) = self.negative {
            *m.get_mut_rhs(n) -= i;
        }
    }

    /// Stamp AC small-signal admittance for a capacitor into the imaginary part matrix.
    pub(crate) fn stamp_ac(&self, ai: &mut Array2<f64>, w: f64) {
        let node1 = self.positive;
        let node2 = self.negative;
        // Yc = j * w * C -> purely imaginary admittance placed on ai
        let yc = w * self.capacitance;

        if let Some(n1) = node1 {
            ai[[n1, n1]] += yc;
        }
        if let Some(n2) = node2 {
            ai[[n2, n2]] += yc;
        }
        if let (Some(n1), Some(n2)) = (node1, node2) {
            ai[[n1, n2]] -= yc;
            ai[[n2, n1]] -= yc;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::build_devices;

    fn capacitor(line: &str) -> Capacitor {
        build_devices(&format!("capacitor\n{line}\n.end\n"))
            .capacitors
            .remove(0)
    }

    // ngspice captemp.c, capload.c: the capacitance is C * scale, loaded m times.

    #[test]
    fn m_puts_copies_in_parallel() {
        let plain = capacitor("C1 a 0 1u");
        assert_eq!(
            capacitor("C1 a 0 1u m=2").capacitance,
            plain.capacitance * 2.0
        );
    }

    #[test]
    fn scale_multiplies_the_capacitance() {
        let plain = capacitor("C1 a 0 1u");
        assert_eq!(
            capacitor("C1 a 0 1u scale=3").capacitance,
            plain.capacitance * 3.0
        );
    }

    #[test]
    fn m_and_scale_combine() {
        let plain = capacitor("C1 a 0 1u");
        let c = capacitor("C1 a 0 1u m=2 scale=3");
        assert_eq!(c.capacitance, plain.capacitance * 3.0 * 2.0);
    }
}
