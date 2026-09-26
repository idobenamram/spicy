use super::stamp::NodeVoltageSourceStamp;
use super::waveform::{dc_value, value_at};
use crate::matrix::SolverMatrix;
use crate::unknowns::Layout;
use ndarray::{Array1, Array2};
use spicy_circuit::{Phasor, SourceParams, TwoTerminal, Waveform};

#[derive(Debug, Clone)]
pub struct VoltageSource {
    /// MNA row of the positive terminal; `None` for ground.
    pub positive: Option<usize>,
    /// MNA row of the negative terminal; `None` for ground.
    pub negative: Option<usize>,
    /// MNA row of the source's current.
    pub branch: usize,
    pub waveform: Waveform,
    pub ac: Phasor,
    pub stamp: NodeVoltageSourceStamp,
}

#[derive(Debug, Clone)]
pub struct CurrentSource {
    /// MNA row of the positive terminal; `None` for ground.
    pub positive: Option<usize>,
    /// MNA row of the negative terminal; `None` for ground.
    pub negative: Option<usize>,
    pub waveform: Waveform,
    pub ac: Phasor,
}

impl VoltageSource {
    pub fn new(pins: &TwoTerminal, params: &SourceParams, branch: usize, layout: &Layout) -> Self {
        Self {
            positive: layout.node(pins.positive),
            negative: layout.node(pins.negative),
            branch,
            waveform: params.waveform,
            ac: params.ac,
            stamp: NodeVoltageSourceStamp::uninitialized(),
        }
    }

    /// Stamp the B / B^T incidence entries for a voltage-defined element.
    fn stamp_incidence(&self, m: &mut SolverMatrix) {
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

    /// Stamp a DC voltage source: incidence + DC value (E vector).
    pub(crate) fn stamp_dc(&self, m: &mut SolverMatrix) {
        self.stamp_incidence(m);
        *m.get_mut_rhs(self.branch) = dc_value(&self.waveform);
    }

    /// Stamp the source at time `t` with step `dt` and stop time `tstop`.
    pub(crate) fn stamp_trans(&self, m: &mut SolverMatrix, t: f64, dt: f64, tstop: f64) {
        self.stamp_incidence(m);
        *m.get_mut_rhs(self.branch) = value_at(&self.waveform, t, dt, tstop);
    }

    /// Stamp AC small-signal contributions: incidence into `ar`, phasor into `(br, bi)`.
    pub(crate) fn stamp_ac(
        &self,
        ar: &mut Array2<f64>,
        br: &mut Array1<f64>,
        bi: &mut Array1<f64>,
    ) {
        let k = self.branch;
        if let Some(n1) = self.positive {
            ar[[n1, k]] += 1.0;
            ar[[k, n1]] += 1.0;
        }
        if let Some(n2) = self.negative {
            ar[[n2, k]] += -1.0;
            ar[[k, n2]] += -1.0;
        }

        let (re, im) = rectangular(&self.ac);
        br[k] += re;
        bi[k] += im;
    }
}

impl CurrentSource {
    pub fn new(pins: &TwoTerminal, params: &SourceParams, layout: &Layout) -> Self {
        Self {
            positive: layout.node(pins.positive),
            negative: layout.node(pins.negative),
            waveform: params.waveform,
            ac: params.ac,
        }
    }

    /// Stamp the DC value into the RHS (I vector).
    pub(crate) fn stamp_dc(&self, m: &mut SolverMatrix) {
        self.stamp_value(m, dc_value(&self.waveform));
    }

    /// Stamp the source at time `t` with step `dt` and stop time `tstop`.
    pub(crate) fn stamp_trans(&self, m: &mut SolverMatrix, t: f64, dt: f64, tstop: f64) {
        self.stamp_value(m, value_at(&self.waveform, t, dt, tstop));
    }

    fn stamp_value(&self, m: &mut SolverMatrix, value: f64) {
        if let Some(pos) = self.positive {
            *m.get_mut_rhs(pos) += value;
        }
        if let Some(neg) = self.negative {
            *m.get_mut_rhs(neg) -= value;
        }
    }

    /// Stamp AC small-signal contributions (phasor only, into the RHS).
    pub(crate) fn stamp_ac(&self, br: &mut Array1<f64>, bi: &mut Array1<f64>) {
        let (re, im) = rectangular(&self.ac);
        if let Some(n1) = self.positive {
            br[n1] -= re;
            bi[n1] -= im;
        }
        if let Some(n2) = self.negative {
            br[n2] += re;
            bi[n2] += im;
        }
    }
}

/// A phasor as (real, imaginary).
fn rectangular(phasor: &Phasor) -> (f64, f64) {
    (
        phasor.magnitude * phasor.phase.cos(),
        phasor.magnitude * phasor.phase.sin(),
    )
}
