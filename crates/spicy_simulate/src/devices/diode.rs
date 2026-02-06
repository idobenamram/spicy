use super::stamp::NodePairStamp;
use crate::matrix::SolverMatrix;
use crate::util::get_voltage_diff;
use spicy_parser::Span;
use spicy_parser::devices::DiodeSpec;
use spicy_parser::netlist_types::NodeIndex;

const DEFAULT_THERMAL_VOLTAGE: f64 = 0.02585;
const DEFAULT_EXP_LIMIT: f64 = 40.0;

#[derive(Debug, Clone)]
pub struct Diode {
    // Stored for diagnostics / SPICE compatibility; not used by the solver yet.
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub span: Span,
    pub positive: NodeIndex,
    pub negative: NodeIndex,
    // saturation current (A)
    pub saturation_current: f64,
    // emission coefficient (dimensionless)
    pub emission_coeff: f64,
    #[allow(dead_code)]
    pub area: f64,
    #[allow(dead_code)]
    pub m: f64,
    /// Thermal voltage (Vt) used in exp(Vd / (n * Vt)).
    pub thermal_voltage: f64,
    /// Clamp limit for Vd/(n*Vt) to keep exp() bounded.
    pub exp_limit: f64,
    #[allow(dead_code)]
    pub off: bool,
    #[allow(dead_code)]
    pub ic: f64,
    /// Series resistance (Ohms) parsed but not used yet.
    #[allow(dead_code)]
    pub series_resistance: f64,
    pub stamp: NodePairStamp,
}

impl Diode {
    pub fn from_spec(spec: &DiodeSpec) -> Self {
        let saturation_current = spec
            .model
            .is
            .as_ref()
            .map(|v| v.get_value())
            .unwrap_or(1e-14);
        let emission_coeff = spec
            .model
            .n
            .as_ref()
            .map(|v| v.get_value())
            .unwrap_or(1.0);
        let series_resistance = spec
            .model
            .rs
            .as_ref()
            .map(|v| v.get_value())
            .unwrap_or(0.0);

        let area = spec.area.as_ref().map(|v| v.get_value()).unwrap_or(1.0);
        let m = spec.m.as_ref().map(|v| v.get_value()).unwrap_or(1.0);
        let off = spec.off.unwrap_or(false);
        let ic = spec.ic.as_ref().map(|v| v.get_value()).unwrap_or(0.0);

        Self {
            name: spec.name.clone(),
            span: spec.span,
            positive: spec.positive,
            negative: spec.negative,
            saturation_current,
            emission_coeff,
            area,
            m,
            thermal_voltage: DEFAULT_THERMAL_VOLTAGE,
            exp_limit: DEFAULT_EXP_LIMIT,
            off,
            ic,
            series_resistance,
            stamp: NodePairStamp::uninitialized(),
        }
    }

    // Shockley diode model: I = Is * (exp(Vd / (n * Vt)) - 1).
    // - Vd: diode voltage (pos - neg) from the current Newton guess.
    // - Is: saturation current from the model, scaled by area * m.
    // - n: emission coefficient (ideality factor), dimensionless.
    // - Vt: thermal voltage (model parameter, defaulted for now).
    // For Newton, we linearize around Vd with:
    // - g = dI/dV: small-signal conductance - derivative around the guess.
    // - Ieq = I - g * Vd: equivalent current source for MNA.
    // this converts the non linear equation to the first order Taylor series approximation
    // i(v) ~ i(v_guess) + g * (v - v_guess)
    // Vd is clamped by exp_limit to keep exp() in a safe range.
    fn linearize(&self, v_d: f64) -> (f64, f64) {
        let n = self.emission_coeff;
        let nvt = n * self.thermal_voltage;
        let isat = self.saturation_current;

        let v_limit = self.exp_limit * nvt;

        // clamp of the voltage diff for the current guess
        let v_eff = v_d.clamp(-v_limit, v_limit);

        let x = v_eff / nvt;
        let exp_v = x.exp();

        // current through the diode for the given guess
        let i = isat * x.exp_m1();

        // conductance (dI/dV) for the given guess
        let g = isat * exp_v / nvt;
        let i_eq = i - g * v_eff;
        (g, i_eq)
    }

    pub(crate) fn stamp_nonlinear(&self, m: &mut SolverMatrix, guess: &[f64]) {
        let pos = m.mna_node_index(self.positive);
        let neg = m.mna_node_index(self.negative);
        // Step 1: get the voltage diff across the diode
        let v_d = get_voltage_diff(guess, pos, neg);

        // Step 2: linearize the diode around the voltage diff
        let (g, i_eq) = self.linearize(v_d);

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

        if let Some(pos) = pos {
            *m.get_mut_rhs(pos) -= i_eq;
        }
        if let Some(neg) = neg {
            *m.get_mut_rhs(neg) += i_eq;
        }
    }
}
