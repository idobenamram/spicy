//! Device numbers: fully resolved, in SI units, one copy per run.
//!
//! Instance parameters are per device. Model parameters live in per-kind
//! model tables shared by the instances that use them. `Option` appears only
//! where a default depends on another value or on the analysis, so it can't
//! be resolved before the run.
//!
//! Every parameter the SPICE front-end accepts is carried here, including
//! ones the simulator doesn't use yet. Those say "Not simulated yet".

/// SPICE's default temperature, 27 °C in kelvin: of a circuit, and of the model
/// parameters when a model doesn't say (ngspice `cktntask.c`).
pub const DEFAULT_TEMPERATURE: f64 = 300.15;

/// All device numbers of a circuit, indexed like [`crate::Circuit`].
#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    /// The circuit's temperature (K). A per-run number like the rest: the engine's
    /// temperature knob sets it. Not simulated yet: every device runs at the thermal
    /// voltage of 27 °C until temperature support.
    pub temp: f64,
    pub resistor_models: Vec<ResistorModel>,
    pub resistors: Vec<ResistorParams>,
    pub capacitor_models: Vec<CapacitorModel>,
    pub capacitors: Vec<CapacitorParams>,
    pub inductor_models: Vec<InductorModel>,
    pub inductors: Vec<InductorParams>,
    pub diode_models: Vec<DiodeModel>,
    pub diodes: Vec<DiodeParams>,
    pub bjt_models: Vec<BjtModel>,
    pub bjts: Vec<BjtParams>,
    pub vsources: Vec<SourceParams>,
    pub isources: Vec<SourceParams>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            temp: DEFAULT_TEMPERATURE,
            resistor_models: Vec::new(),
            resistors: Vec::new(),
            capacitor_models: Vec::new(),
            capacitors: Vec::new(),
            inductor_models: Vec::new(),
            inductors: Vec::new(),
            diode_models: Vec::new(),
            diodes: Vec::new(),
            bjt_models: Vec::new(),
            bjts: Vec::new(),
            vsources: Vec::new(),
            isources: Vec::new(),
        }
    }
}

/// A device's temperature relative to the run's. Not simulated yet: every
/// device runs at the thermal voltage of 27 °C until temperature support.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceTemperature {
    /// The run's temperature plus this many kelvin (SPICE `dtemp`).
    Offset(f64),
    /// This temperature in kelvin, whatever the run's (SPICE `temp`). When a
    /// device gives both, `temp` wins and `dtemp` is ignored, as in ngspice.
    Fixed(f64),
}

impl Default for DeviceTemperature {
    fn default() -> Self {
        Self::Offset(0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResistorModel {
    /// First-order temperature coefficient (1/K). Not simulated yet.
    pub tc1: f64,
    /// Second-order temperature coefficient (1/K²). Not simulated yet.
    pub tc2: f64,
    /// Default width (m) of a geometric resistor. Not simulated yet: it needs
    /// the sheet resistance, which isn't parsed.
    pub default_width: f64,
    /// Default length (m) of a geometric resistor. Not simulated yet.
    pub default_length: f64,
    /// Temperature (K) the parameters were measured at, filled in when the source is
    /// lowered: the model's own TNOM, else the circuit's. Not simulated yet.
    pub tnom: f64,
}

impl Default for ResistorModel {
    /// ngspice's defaults (`ressetup.c`).
    fn default() -> Self {
        Self {
            tc1: 0.0,
            tc2: 0.0,
            default_width: 10e-6,
            default_length: 10e-6,
            tnom: DEFAULT_TEMPERATURE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResistorParams {
    /// Resistance of one device (Ω).
    pub r: f64,
    /// Resistance of one device in AC analysis (Ω). `None` means `r`.
    pub r_ac: Option<f64>,
    /// Number of devices in parallel.
    pub m: f64,
    pub temperature: DeviceTemperature,
    /// Whether the resistor adds thermal noise. Not simulated yet: there's no
    /// noise analysis.
    pub noisy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapacitorModel {
    /// First-order temperature coefficient (1/K). Not simulated yet.
    pub tc1: f64,
    /// Second-order temperature coefficient (1/K²). Not simulated yet.
    pub tc2: f64,
    /// Temperature (K) the parameters were measured at, filled in when the source is
    /// lowered: the model's own TNOM, else the circuit's. Not simulated yet.
    pub tnom: f64,
}

impl Default for CapacitorModel {
    fn default() -> Self {
        Self {
            tc1: 0.0,
            tc2: 0.0,
            tnom: DEFAULT_TEMPERATURE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapacitorParams {
    /// Capacitance of one device (F).
    pub c: f64,
    /// Number of devices in parallel.
    pub m: f64,
    /// Initial voltage (V), used when the transient skips the operating point.
    pub ic: f64,
    pub temperature: DeviceTemperature,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InductorModel {
    /// First-order temperature coefficient (1/K). Not simulated yet.
    pub tc1: f64,
    /// Second-order temperature coefficient (1/K²). Not simulated yet.
    pub tc2: f64,
    /// Temperature (K) the parameters were measured at, filled in when the source is
    /// lowered: the model's own TNOM, else the circuit's. Not simulated yet.
    pub tnom: f64,
}

impl Default for InductorModel {
    fn default() -> Self {
        Self {
            tc1: 0.0,
            tc2: 0.0,
            tnom: DEFAULT_TEMPERATURE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InductorParams {
    /// Inductance of one device (H).
    pub l: f64,
    /// Number of devices in parallel.
    pub m: f64,
    /// Initial current (A), used when the transient skips the operating point.
    pub ic: f64,
    pub temperature: DeviceTemperature,
    /// Number of turns. Not simulated yet: it only matters for an inductance
    /// given per turn by the model, which isn't parsed.
    pub nt: f64,
}

/// Shockley diode model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiodeModel {
    /// Saturation current (A).
    pub is: f64,
    /// Emission coefficient.
    pub n: f64,
    /// Series resistance (Ω); 0 means none. Not simulated yet: a nonzero
    /// value adds an internal node (docs/ecad/circuit.md#internal-nodes).
    pub rs: f64,
    /// Energy gap (eV), in the saturation current's temperature law. Not simulated yet.
    pub eg: f64,
    /// Saturation current temperature exponent. Not simulated yet.
    pub xti: f64,
    /// Temperature (K) the parameters were measured at, filled in when the source is
    /// lowered: the model's own TNOM, else the circuit's. Not simulated yet.
    pub tnom: f64,
}

impl Default for DiodeModel {
    /// ngspice's defaults (`diosetup.c`).
    fn default() -> Self {
        Self {
            is: 1e-14,
            n: 1.0,
            rs: 0.0,
            eg: 1.11,
            xti: 3.0,
            tnom: DEFAULT_TEMPERATURE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiodeParams {
    /// Area factor: scales the model's currents.
    pub area: f64,
    /// Number of devices in parallel.
    pub m: f64,
    /// Perimeter factor: scales the sidewall currents. Not simulated yet: the
    /// sidewall parameters aren't parsed.
    pub pj: f64,
    /// Metal and polysilicon capacitor dimensions (m) of the level 3 diode:
    /// length and width of each. Not simulated yet.
    pub lm: f64,
    pub wm: f64,
    pub lp: f64,
    pub wp: f64,
    /// Start the first DC iteration with the diode off. Not simulated yet.
    pub off: bool,
    /// Initial voltage (V) for a transient that skips the operating point.
    /// Not simulated yet.
    pub ic: f64,
    pub temperature: DeviceTemperature,
}

impl Default for DiodeParams {
    fn default() -> Self {
        Self {
            area: 1.0,
            m: 1.0,
            pj: 0.0,
            lm: 0.0,
            wm: 0.0,
            lp: 0.0,
            wp: 0.0,
            off: false,
            ic: 0.0,
            temperature: DeviceTemperature::default(),
        }
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
    /// Forward and reverse β temperature exponent. Not simulated yet.
    pub xtb: f64,
    /// Saturation current temperature exponent. Not simulated yet.
    pub xti: f64,
    /// Energy gap (eV), in the saturation current's temperature law. Not simulated yet.
    pub eg: f64,
    /// Temperature (K) the parameters were measured at, filled in when the source is
    /// lowered: the model's own TNOM, else the circuit's. Not simulated yet.
    pub tnom: f64,
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
            xtb: 0.0,
            xti: 3.0,
            eg: 1.11,
            tnom: DEFAULT_TEMPERATURE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BjtParams {
    /// Area factor: scales the model's currents.
    pub area: f64,
    /// Number of devices in parallel.
    pub m: f64,
    /// Start the first DC iteration with the transistor off. Not simulated yet.
    pub off: bool,
    /// Initial base-emitter voltage (V) for a transient that skips the
    /// operating point. Not simulated yet.
    pub ic_vbe: f64,
    /// Initial collector-emitter voltage (V), as `ic_vbe`. Not simulated yet.
    pub ic_vce: f64,
    pub temperature: DeviceTemperature,
}

impl Default for BjtParams {
    fn default() -> Self {
        Self {
            area: 1.0,
            m: 1.0,
            off: false,
            ic_vbe: 0.0,
            ic_vce: 0.0,
            temperature: DeviceTemperature::default(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SolverOptions;

    /// ngspice's defaults: 27 °C for the circuit and every model's TNOM
    /// (`cktntask.c`), XTB 0, XTI 3 and EG 1.11 eV for the BJT (`bjtsetup.c`),
    /// XTI 3 and EG 1.11 eV for the diode (`diosetup.c`).
    #[test]
    fn temperature_defaults_match_ngspice() {
        assert_eq!(DEFAULT_TEMPERATURE, 300.15);
        assert_eq!(Params::default().temp, DEFAULT_TEMPERATURE);
        let bjt = BjtModel::default();
        assert_eq!(
            (bjt.xtb, bjt.xti, bjt.eg, bjt.tnom),
            (0.0, 3.0, 1.11, DEFAULT_TEMPERATURE)
        );
        let diode = DiodeModel::default();
        assert_eq!(
            (diode.xti, diode.eg, diode.tnom),
            (3.0, 1.11, DEFAULT_TEMPERATURE)
        );
        assert_eq!(ResistorModel::default().tnom, DEFAULT_TEMPERATURE);
        assert_eq!(CapacitorModel::default().tnom, DEFAULT_TEMPERATURE);
        assert_eq!(InductorModel::default().tnom, DEFAULT_TEMPERATURE);
        assert_eq!(
            BjtParams::default().temperature,
            DeviceTemperature::Offset(0.0)
        );
    }

    /// Solver options record what a source file asked for; nothing asked, nothing set.
    #[test]
    fn solver_options_start_unset() {
        let options = SolverOptions::default();
        assert_eq!(
            (options.reltol, options.vntol, options.abstol),
            (None, None, None)
        );
    }
}
