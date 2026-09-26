use super::stamp::NodePairStamp;
use crate::matrix::SolverMatrix;
use crate::unknowns::Layout;
use crate::util::get_voltage_diff;
use spicy_circuit::{self as circuit, DiodeModel, DiodeParams};

const DEFAULT_THERMAL_VOLTAGE: f64 = 0.02585;
const DEFAULT_EXP_LIMIT: f64 = 40.0;

#[derive(Debug, Clone)]
pub struct Diode {
    /// MNA row of the anode; `None` for ground.
    pub positive: Option<usize>,
    /// MNA row of the cathode; `None` for ground.
    pub negative: Option<usize>,
    /// Effective saturation current (A): the model's IS scaled by `area` and `m`.
    pub saturation_current: f64,
    // emission coefficient (dimensionless)
    pub emission_coeff: f64,
    /// Thermal voltage (Vt) used in exp(Vd / (n * Vt)).
    pub thermal_voltage: f64,
    /// Clamp limit for Vd/(n*Vt) to keep exp() bounded.
    pub exp_limit: f64,
    pub stamp: NodePairStamp,
}

impl Diode {
    pub fn new(
        pins: &circuit::Diode,
        model: &DiodeModel,
        params: &DiodeParams,
        layout: &Layout,
    ) -> Self {
        Self {
            positive: layout.node(pins.positive),
            negative: layout.node(pins.negative),
            // Every current in this model is proportional to IS, so `area` and
            // `m` scale it once, as in ngspice (diotemp.c).
            saturation_current: model.is * params.area * params.m,
            emission_coeff: model.n,
            thermal_voltage: DEFAULT_THERMAL_VOLTAGE,
            exp_limit: DEFAULT_EXP_LIMIT,
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
        let pos = self.positive;
        let neg = self.negative;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::build_devices;

    fn diode(instance_params: &str, model_params: &str) -> Diode {
        build_devices(&format!(
            "diode\nD1 a 0 DMOD {instance_params}\n.MODEL DMOD D {model_params}\n.end\n"
        ))
        .diodes
        .remove(0)
    }

    // ngspice diotemp.c: IS is scaled by area * m. Every current in this model
    // is proportional to IS.

    #[test]
    fn area_scales_the_saturation_current() {
        let plain = diode("", "is=1e-14");
        let d = diode("area=2", "is=1e-14");
        assert_eq!(d.saturation_current, plain.saturation_current * 2.0);
    }

    #[test]
    fn m_scales_the_saturation_current() {
        let plain = diode("", "is=1e-14");
        let d = diode("m=3", "is=1e-14");
        assert_eq!(d.saturation_current, plain.saturation_current * 3.0);
    }

    #[test]
    fn area_and_m_combine() {
        let plain = diode("", "is=1e-14");
        let d = diode("area=2 m=3", "is=1e-14");
        assert_eq!(d.saturation_current, plain.saturation_current * 2.0 * 3.0);
    }

    #[test]
    fn defaults_match_ngspice() {
        // diosetup.c: IS = 1e-14, N = 1.
        let d = diode("", "");
        assert_eq!(d.saturation_current, 1e-14);
        assert_eq!(d.emission_coeff, 1.0);
    }
}
