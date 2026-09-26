use super::stamp::NodeBranchPairStamp;
use crate::matrix::SolverMatrix;
use crate::unknowns::Layout;
use ndarray::Array2;
use spicy_circuit::{self as circuit, InductorParams};

#[derive(Debug, Clone)]
pub struct Inductor {
    /// MNA row of the positive terminal; `None` for ground.
    pub positive: Option<usize>,
    /// MNA row of the negative terminal; `None` for ground.
    pub negative: Option<usize>,
    /// MNA row of the inductor's current.
    pub branch: usize,
    /// Effective inductance (H) of the `m` devices in parallel.
    pub inductance: f64,
    /// Initial current (A), used when the transient skips the operating point.
    pub ic: f64,
    pub stamp: NodeBranchPairStamp,
}

impl Inductor {
    pub fn new(
        pins: &circuit::Inductor,
        params: &InductorParams,
        branch: usize,
        layout: &Layout,
    ) -> Self {
        Self {
            positive: layout.node(pins.positive),
            negative: layout.node(pins.negative),
            branch,
            // `m` devices in parallel divide the inductance, as in ngspice (indload.c).
            inductance: params.l / params.m,
            ic: params.ic,
            stamp: NodeBranchPairStamp::uninitialized(),
        }
    }

    /// Stamp DC MNA contributions for an inductor.
    ///
    /// In DC, an ideal inductor is a short circuit enforced via a branch current unknown and a
    /// KVL equation with zero RHS (similar to a 0V voltage source).
    pub(crate) fn stamp_dc(&self, m: &mut SolverMatrix) {
        let src_index = self.branch;

        if let Some((pos_branch, branch_pos)) = self.stamp.pos_branch {
            // stamp in voltage incidence matrix (B)
            *m.get_mut_nnz(pos_branch) = 1.0;
            // stamp in voltage incidence matrix (B^T)
            *m.get_mut_nnz(branch_pos) = 1.0;
        }

        if let Some((neg_branch, branch_neg)) = self.stamp.neg_branch {
            // stamp in voltage incidence matrix (B)
            *m.get_mut_nnz(neg_branch) = -1.0;
            // stamp in voltage incidence matrix (B^T)
            *m.get_mut_nnz(branch_neg) = -1.0;
        }

        // stamp in voltage source vector (E)
        *m.get_mut_rhs(src_index) = 0.0;
    }

    /// Stamp AC small-signal contributions for an inductor into the real/imag MNA matrices.
    pub(crate) fn stamp_ac(&self, ar: &mut Array2<f64>, ai: &mut Array2<f64>, w: f64) {
        let node1 = self.positive;
        let node2 = self.negative;
        let k = self.branch;

        // Incidence (real part): same as DC B and B^T
        if let Some(n1) = node1 {
            ar[[n1, k]] += 1.0;
            ar[[k, n1]] += 1.0;
        }
        if let Some(n2) = node2 {
            ar[[n2, k]] -= 1.0;
            ar[[k, n2]] -= 1.0;
        }

        // KVL: v = (Va - Vb) - j*w*L*i = 0 -> put -w*L on imag diagonal of KVL row/col
        let wl = w * self.inductance;
        ai[[k, k]] -= wl;
    }

    /// Stamp transient companion model for an inductor.
    ///
    /// KVL form: Vpos - Vneg - r_eq * I = v_hist
    pub(crate) fn stamp_trans(&self, m: &mut SolverMatrix, r_eq: f64, v_hist: f64) {
        let branch_index = self.branch;

        if let Some((pos_branch, branch_pos)) = self.stamp.pos_branch {
            *m.get_mut_nnz(pos_branch) = 1.0;
            *m.get_mut_nnz(branch_pos) = 1.0;
        }

        if let Some((neg_branch, branch_neg)) = self.stamp.neg_branch {
            *m.get_mut_nnz(neg_branch) = -1.0;
            *m.get_mut_nnz(branch_neg) = -1.0;
        }

        if self.stamp.branch_branch != usize::MAX {
            *m.get_mut_nnz(self.stamp.branch_branch) -= r_eq;
        }

        *m.get_mut_rhs(branch_index) = v_hist;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::build_devices;

    fn inductor(line: &str) -> Inductor {
        build_devices(&format!("inductor\n{line}\n.end\n"))
            .inductors
            .remove(0)
    }

    // ngspice indtemp.c, indload.c: the inductance is L * scale / m.

    #[test]
    fn m_puts_copies_in_parallel() {
        let plain = inductor("L1 a 0 1m");
        assert_eq!(inductor("L1 a 0 1m m=2").inductance, plain.inductance / 2.0);
    }

    #[test]
    fn scale_multiplies_the_inductance() {
        let plain = inductor("L1 a 0 1m");
        assert_eq!(
            inductor("L1 a 0 1m scale=3").inductance,
            plain.inductance * 3.0
        );
    }

    #[test]
    fn m_and_scale_combine() {
        let plain = inductor("L1 a 0 1m");
        let l = inductor("L1 a 0 1m m=2 scale=3");
        assert_eq!(l.inductance, plain.inductance * 3.0 / 2.0);
    }
}
