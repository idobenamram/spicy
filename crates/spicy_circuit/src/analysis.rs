//! What to run on a circuit.

use crate::{IsourceId, VsourceId};

/// Solver tolerances a source asked for (SPICE `.options`). `None` means the
/// file didn't say. They describe how precisely to solve, not what the circuit
/// is, so they don't belong to it: the one running the circuit (the CLI, the
/// engine's tolerance profiles) decides, and may start from these.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SolverOptions {
    /// Relative tolerance.
    pub reltol: Option<f64>,
    /// Absolute voltage tolerance (V).
    pub vntol: Option<f64>,
    /// Absolute current tolerance (A).
    pub abstol: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Analysis {
    /// DC operating point.
    Op,
    Dc(DcSweep),
    Ac(AcSweep),
    Tran(Transient),
}

/// Operating points while one source's DC value steps from `start` to `stop`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DcSweep {
    pub source: SourceRef,
    pub start: f64,
    pub stop: f64,
    pub step: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRef {
    Voltage(VsourceId),
    Current(IsourceId),
}

/// Small-signal response from `start` to `stop` (Hz).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcSweep {
    pub spacing: AcSpacing,
    pub start: f64,
    pub stop: f64,
}

/// How the frequencies of an AC sweep are spaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcSpacing {
    /// This many points per decade.
    Decade(usize),
    /// This many points per octave.
    Octave(usize),
    /// This many points in total, evenly spaced.
    Linear(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transient {
    /// Time step (s).
    pub step: f64,
    /// Stop time (s).
    pub stop: f64,
    /// Start from the devices' initial conditions instead of the operating point.
    pub uic: bool,
}
