use super::stamp::NodeBranchPairStamp;
use crate::matrix::SolverMatrix;
use ndarray::Array2;
use spicy_parser::devices::InductorSpec;
use spicy_parser::netlist_types::{CurrentBranchIndex, NodeIndex};
use spicy_parser::node_mapping::NodeMapping;
use spicy_parser::Span;

#[derive(Debug, Clone)]
pub struct Inductor {
    // Stored for diagnostics / SPICE compatibility; not used by the solver yet.
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub span: Span,
    pub positive: NodeIndex,
    pub negative: NodeIndex,
    pub current_branch: CurrentBranchIndex,
    pub inductance: f64,
    #[allow(dead_code)]
    pub nt: f64,
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
    #[allow(dead_code)]
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

    /// Stamp DC MNA contributions for an inductor.
    ///
    /// In DC, an ideal inductor is a short circuit enforced via a branch current unknown and a
    /// KVL equation with zero RHS (similar to a 0V voltage source).
    pub(crate) fn stamp_dc(&self, m: &mut SolverMatrix) {
        let src_index = m.mna_branch_index(self.current_branch);

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
    pub(crate) fn stamp_ac(
        &self,
        ar: &mut Array2<f64>,
        ai: &mut Array2<f64>,
        node_mapping: &NodeMapping,
        w: f64,
    ) {
        let node1 = node_mapping.mna_node_index(self.positive);
        let node2 = node_mapping.mna_node_index(self.negative);
        let k = node_mapping.mna_branch_index(self.current_branch);

        // Incidence (real part): same as DC B and B^T
        if let Some(n1) = node1 {
            ar[[n1, k]] += 1.0;
            ar[[k, n1]] += 1.0;
        }
        if let Some(n2) = node2 {
            ar[[n2, k]] -= 1.0;
            ar[[k, n2]] -= 1.0;
        }

        // KVL: v = (Va - Vb) - j*w*L*i = 0 -> put +w*L on imag diagonal of KVL row/col
        let wl = w * self.inductance;
        ai[[k, k]] += wl;
    }
}

