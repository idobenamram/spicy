use super::stamp::NodePairStamp;
use crate::matrix::SolverMatrix;
use ndarray::Array2;
use spicy_parser::devices::CapacitorSpec;
use spicy_parser::netlist_types::NodeIndex;
use spicy_parser::node_mapping::NodeMapping;
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

        let pos = m.mna_node_index(self.positive);
        let neg = m.mna_node_index(self.negative);

        if let Some(p) = pos {
            *m.get_mut_rhs(p) += i;
        }
        if let Some(n) = neg {
            *m.get_mut_rhs(n) -= i;
        }
    }

    /// Stamp AC small-signal admittance for a capacitor into the imaginary part matrix.
    pub(crate) fn stamp_ac(&self, ai: &mut Array2<f64>, node_mapping: &NodeMapping, w: f64) {
        let node1 = node_mapping.mna_node_index(self.positive);
        let node2 = node_mapping.mna_node_index(self.negative);
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

