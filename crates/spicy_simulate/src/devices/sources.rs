use super::stamp::NodeVoltageSourceStamp;
use crate::matrix::SolverMatrix;
use ndarray::{Array1, Array2};
use spicy_parser::Value;
use spicy_parser::devices::IndependentSourceSpec;
use spicy_parser::netlist_types::{CurrentBranchIndex, NodeIndex, Phasor};
use spicy_parser::netlist_waveform::WaveForm;
use spicy_parser::node_mapping::NodeMapping;
use std::f64::consts::PI;

// TODO: should probably be split to voltage source and current source
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
            stamp: NodeVoltageSourceStamp::uninitialized(),
        }
    }

    /// Stamp the B / B^T incidence entries for a voltage-defined element.
    pub(crate) fn stamp_voltage_incidence(&self, m: &mut SolverMatrix) {
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
    }

    /// Stamp the DC value of a voltage source into the RHS (E vector).
    pub(crate) fn stamp_voltage_value_dc(&self, m: &mut SolverMatrix) {
        let src_index = m.mna_branch_index(self.current_branch);
        let value = self.dc.compute(0.0, 0.0, 0.0);
        *m.get_mut_rhs(src_index) = value;
    }

    /// Stamp a full DC voltage source: incidence + DC value.
    pub(crate) fn stamp_voltage_source_dc(&self, m: &mut SolverMatrix) {
        self.stamp_voltage_incidence(m);
        self.stamp_voltage_value_dc(m);
    }

    /// Stamp the DC value of a current source into the RHS (I vector).
    pub(crate) fn stamp_current_source_dc(&self, m: &mut SolverMatrix) {
        let pos = m.mna_node_index(self.positive);
        let neg = m.mna_node_index(self.negative);

        let value = self.dc.compute(0.0, 0.0, 0.0);

        if let Some(pos) = pos {
            *m.get_mut_rhs(pos) += value;
        }
        if let Some(neg) = neg {
            *m.get_mut_rhs(neg) -= value;
        }
    }

    /// Stamp a transient current source at time `t` with step `dt` and stop `tstop`.
    pub(crate) fn stamp_current_source_trans(
        &self,
        m: &mut SolverMatrix,
        t: f64,
        dt: f64,
        tstop: f64,
    ) {
        let pos = m.mna_node_index(self.positive);
        let neg = m.mna_node_index(self.negative);

        let value = self.dc.compute(t, dt, tstop);

        if let Some(pos) = pos {
            *m.get_mut_rhs(pos) += value;
        }
        if let Some(neg) = neg {
            *m.get_mut_rhs(neg) -= value;
        }
    }

    /// Stamp a transient voltage source at time `t` with step `dt` and stop `tstop`.
    pub(crate) fn stamp_voltage_source_trans(
        &self,
        m: &mut SolverMatrix,
        t: f64,
        dt: f64,
        tstop: f64,
    ) {
        self.stamp_voltage_incidence(m);
        let src_index = m.mna_branch_index(self.current_branch);
        let value = self.dc.compute(t, dt, tstop);
        *m.get_mut_rhs(src_index) = value;
    }

    /// Stamp AC small-signal contributions for a *voltage source*:
    /// - incidence into `ar`
    /// - phasor into `(br, bi)`
    pub(crate) fn stamp_ac_voltage_source(
        &self,
        ar: &mut Array2<f64>,
        br: &mut Array1<f64>,
        bi: &mut Array1<f64>,
        node_mapping: &NodeMapping,
    ) {
        let n1 = node_mapping.mna_node_index(self.positive);
        let n2 = node_mapping.mna_node_index(self.negative);
        let k = node_mapping.mna_branch_index(self.current_branch);

        if let Some(n1) = n1 {
            ar[[n1, k]] += 1.0;
            ar[[k, n1]] += 1.0;
        }
        if let Some(n2) = n2 {
            ar[[n2, k]] += -1.0;
            ar[[k, n2]] += -1.0;
        }

        if let Some(phasor) = &self.ac {
            let mag = phasor.mag.get_value();
            let phase = phasor.phase.as_ref().map(|v| v.get_value()).unwrap_or(0.0);
            let ph = phase * PI / 180.0;
            let re = mag * ph.cos();
            let im = mag * ph.sin();
            br[k] += re;
            bi[k] += im;
        }
    }

    /// Stamp AC small-signal contributions for a *current source* (phasor only, into RHS).
    pub(crate) fn stamp_ac_current_source(
        &self,
        br: &mut Array1<f64>,
        bi: &mut Array1<f64>,
        node_mapping: &NodeMapping,
    ) {
        if let Some(ac) = &self.ac {
            let mag = ac.mag.get_value();
            let phase = ac.phase.as_ref().map(|v| v.get_value()).unwrap_or(0.0);
            let ph = phase * PI / 180.0;
            let re = mag * ph.cos();
            let im = mag * ph.sin();

            let n1 = node_mapping.mna_node_index(self.positive);
            let n2 = node_mapping.mna_node_index(self.negative);

            if let Some(n1) = n1 {
                br[n1] -= re;
                bi[n1] -= im;
            }
            if let Some(n2) = n2 {
                br[n2] += re;
                bi[n2] += im;
            }
        }
    }
}
