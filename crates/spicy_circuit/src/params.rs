//! Device numbers: fully resolved, in SI units, one copy per run.
//!
//! Instance parameters are per device. Model parameters live in per-kind
//! model tables shared by the instances that use them. `Option` appears only
//! where a default depends on another value or on the analysis, so it can't
//! be resolved before the run.

/// All device numbers of a circuit, indexed like [`crate::Circuit`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Params {
    pub resistors: Vec<ResistorParams>,
    pub capacitors: Vec<CapacitorParams>,
    pub inductors: Vec<InductorParams>,
    pub diode_models: Vec<DiodeModel>,
    pub diodes: Vec<DiodeParams>,
    pub bjt_models: Vec<BjtModel>,
    pub bjts: Vec<BjtParams>,
    pub vsources: Vec<SourceParams>,
    pub isources: Vec<SourceParams>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResistorParams {
    /// Resistance of one device (Ω).
    pub r: f64,
    /// Resistance of one device in AC analysis (Ω). `None` means `r`.
    pub r_ac: Option<f64>,
    /// Number of devices in parallel.
    pub m: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapacitorParams {
    /// Capacitance of one device (F).
    pub c: f64,
    /// Number of devices in parallel.
    pub m: f64,
    /// Initial voltage (V), used when the transient skips the operating point.
    pub ic: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InductorParams {
    /// Inductance of one device (H).
    pub l: f64,
    /// Number of devices in parallel.
    pub m: f64,
    /// Initial current (A), used when the transient skips the operating point.
    pub ic: f64,
}

/// Shockley diode model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiodeModel {
    /// Saturation current (A).
    pub is: f64,
    /// Emission coefficient.
    pub n: f64,
}

impl Default for DiodeModel {
    /// ngspice's defaults (`diosetup.c`).
    fn default() -> Self {
        Self { is: 1e-14, n: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiodeParams {
    /// Area factor: scales the model's currents.
    pub area: f64,
    /// Number of devices in parallel.
    pub m: f64,
}

impl Default for DiodeParams {
    fn default() -> Self {
        Self { area: 1.0, m: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Polarity {
    #[default]
    Npn,
    Pnp,
}

/// Ebers–Moll transistor model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BjtModel {
    pub polarity: Polarity,
    /// Saturation current (A).
    pub is: f64,
    /// Forward current gain.
    pub bf: f64,
    /// Reverse current gain.
    pub br: f64,
    /// Forward emission coefficient.
    pub nf: f64,
    /// Reverse emission coefficient.
    pub nr: f64,
}

impl Default for BjtModel {
    /// ngspice's defaults (`bjtsetup.c`), including NPN when no type is given.
    fn default() -> Self {
        Self {
            polarity: Polarity::Npn,
            is: 1e-16,
            bf: 100.0,
            br: 1.0,
            nf: 1.0,
            nr: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BjtParams {
    /// Area factor: scales the model's currents.
    pub area: f64,
    /// Number of devices in parallel.
    pub m: f64,
}

impl Default for BjtParams {
    fn default() -> Self {
        Self { area: 1.0, m: 1.0 }
    }
}

/// An independent voltage or current source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceParams {
    /// Value over time: the DC value in DC analyses, the waveform in transient.
    pub waveform: Waveform,
    /// Small-signal stimulus in AC analysis. Zero for sources without one.
    pub ac: Phasor,
}

/// A source's value over time (V or A). Fields that are `None` default to the
/// analysis' time step or stop time, as in SPICE.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Waveform {
    Dc(f64),
    /// SPICE `PULSE(V1 V2 TD TR TF PW PER NP)`.
    Pulse {
        initial: f64,
        pulsed: f64,
        delay: f64,
        /// Defaults to the time step.
        rise: Option<f64>,
        /// Defaults to the time step.
        fall: Option<f64>,
        /// Defaults to the stop time.
        width: Option<f64>,
        /// Defaults to the stop time.
        period: Option<f64>,
        /// Number of pulses; 0 repeats forever.
        count: u64,
    },
    /// SPICE `SIN(VO VA FREQ TD THETA PHASE)`.
    Sine {
        offset: f64,
        amplitude: f64,
        /// Hz. Defaults to one period over the stop time.
        frequency: Option<f64>,
        delay: f64,
        /// Damping factor (1/s).
        damping: f64,
        /// Radians.
        phase: f64,
    },
    /// SPICE `EXP(V1 V2 TD1 TAU1 TD2 TAU2)`.
    Exp {
        initial: f64,
        pulsed: f64,
        rise_delay: f64,
        /// Defaults to the time step.
        rise_tau: Option<f64>,
        /// Defaults to `rise_delay` plus the time step.
        fall_delay: Option<f64>,
        /// Defaults to the time step.
        fall_tau: Option<f64>,
    },
}

/// A sinusoid's amplitude and phase.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Phasor {
    pub magnitude: f64,
    /// Radians.
    pub phase: f64,
}
