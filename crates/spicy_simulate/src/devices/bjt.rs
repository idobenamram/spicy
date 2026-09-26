//! Simple Ebers-Moll BJT model (NPN/PNP).
//!
//! Uses base-emitter/base-collector junctions and alpha gains, then
//! linearizes around the current Newton guess for MNA stamping.
use super::stamp::NodeTripletStamp;
use crate::matrix::SolverMatrix;
use crate::unknowns::Layout;
use crate::util::get_voltage_diff;
use spicy_circuit::{self as circuit, BjtModel, BjtParams, Polarity};

const DEFAULT_THERMAL_VOLTAGE: f64 = 0.02585;
const DEFAULT_EXP_LIMIT: f64 = 40.0;

#[derive(Debug, Clone)]
pub struct Bjt {
    /// MNA rows of the terminals; `None` for ground.
    pub collector: Option<usize>,
    pub base: Option<usize>,
    pub emitter: Option<usize>,
    pub polarity: Polarity,
    /// Effective saturation current (A): the model's IS scaled by `area` and `m`.
    pub saturation_current: f64,
    /// Forward beta - approximate relation between I_c and I_e in active region.
    /// in ebers-moll model beta_forward is usually converted to alpha gains.
    /// alpha_f = beta_forward / (beta_forward + 1)
    /// this defines the coupling between the base and collector currents.
    pub beta_forward: f64,
    /// Reverse beta - approximate relation between I_e and I_b in reverse-active region.
    /// in ebers-moll model beta_reverse is usually converted to alpha gains.
    /// alpha_r = beta_reverse / (beta_reverse + 1)
    /// this defines the coupling between the base and emitter currents.
    pub beta_reverse: f64,
    /// Forward emission coefficient (ideality factor), dimensionless.
    pub emission_coeff_forward: f64,
    /// Reverse emission coefficient (ideality factor), dimensionless.
    pub emission_coeff_reverse: f64,
    /// Thermal voltage (Vt) used in exp(V / (n * Vt)).
    pub thermal_voltage: f64,
    /// Clamp limit for V/Vt to keep exp() bounded.
    pub exp_limit: f64,
    pub stamp: NodeTripletStamp,
}

#[derive(Debug, Clone, Copy)]
struct LinearizedBjt {
    g_bb: f64,
    g_bc: f64,
    g_be: f64,
    g_cb: f64,
    g_cc: f64,
    g_ce: f64,
    g_eb: f64,
    g_ec: f64,
    g_ee: f64,
    i_eq_b: f64,
    i_eq_c: f64,
    i_eq_e: f64,
}

impl Bjt {
    pub fn new(pins: &circuit::Bjt, model: &BjtModel, params: &BjtParams, layout: &Layout) -> Self {
        Self {
            collector: layout.node(pins.collector),
            base: layout.node(pins.base),
            emitter: layout.node(pins.emitter),
            polarity: model.polarity,
            // ngspice scales IS by `area` (bjttemp.c) and every contribution by
            // `m` (bjtload.c). In this model every current is proportional to
            // IS, so both fold into it.
            saturation_current: model.is * params.area * params.m,
            beta_forward: model.bf,
            beta_reverse: model.br,
            emission_coeff_forward: model.nf,
            emission_coeff_reverse: model.nr,
            thermal_voltage: DEFAULT_THERMAL_VOLTAGE,
            exp_limit: DEFAULT_EXP_LIMIT,
            stamp: NodeTripletStamp::uninitialized(),
        }
    }

    /// Return +1 for NPN, -1 for PNP.
    ///
    /// This flips the sign of control voltages and resulting currents to
    /// reuse the same Ebers-Moll equations for both polarities.
    fn polarity_sign(&self) -> f64 {
        match self.polarity {
            Polarity::Npn => 1.0,
            Polarity::Pnp => -1.0,
        }
    }

    /// Compute diode current, conductance, and clamped voltage.
    fn junction_values(&self, v: f64, emission_coeff: f64) -> (f64, f64, f64) {
        let nvt = emission_coeff * self.thermal_voltage;
        // TODO: this is very bad limiting,
        // we need the previous iteration votlage to limit correctly
        let v_limit = self.exp_limit * nvt;
        let v_eff = v.clamp(-v_limit, v_limit);

        let x = v_eff / nvt;
        let exp_v = x.exp();
        let isat = self.saturation_current;
        let i = isat * x.exp_m1();
        let g = isat * exp_v / nvt;
        (i, g, v_eff)
    }

    /// Linearize the Ebers-Moll model at the current node voltages.
    fn linearize(&self, v_be_node: f64, v_bc_node: f64) -> LinearizedBjt {
        let polarity = self.polarity_sign();
        let v_be = polarity * v_be_node;
        let v_bc = polarity * v_bc_node;

        let (i_f, g_f, vbe_eff) = self.junction_values(v_be, self.emission_coeff_forward);
        let (i_r, g_r, vbc_eff) = self.junction_values(v_bc, self.emission_coeff_reverse);

        let vbe_eff_node = vbe_eff * polarity;
        let vbc_eff_node = vbc_eff * polarity;

        let alpha_f = self.beta_forward / (self.beta_forward + 1.0);
        let alpha_r = self.beta_reverse / (self.beta_reverse + 1.0);

        // g_f  = d(i_F)/d(v_BE)  where i_F = IS*(exp(v_BE/(NF*Vt)) - 1)
        // g_r  = d(i_R)/d(v_BC)  where i_R = IS*(exp(v_BC/(NR*Vt)) - 1)
        //
        // Ebers–Moll terminal currents sign convention (in the "polarity-normalized" domain):
        //   i_c0 =  αF * i_F  -  i_R
        //   i_b0 = (1-αF)*i_F + (1-αR)*i_R
        //   i_e0 =  -i_F      +  αR * i_R
        //
        // Step 1: partial derivatives of terminal currents w.r.t junction voltages.
        // These are "how much terminal current changes if v_BE changes" etc.
        let i_c0 = alpha_f * i_f - i_r;
        let i_b0 = (1.0 - alpha_f) * i_f + (1.0 - alpha_r) * i_r;
        let i_e0 = -i_f + alpha_r * i_r;

        let i_c = polarity * i_c0;
        let i_e = polarity * i_e0;
        let i_b = polarity * i_b0;

        // ∂Ic/∂vBE = αF * ∂iF/∂vBE = αF * g_f
        let g_c_be = alpha_f * g_f;

        // ∂Ic/∂vBC = - ∂iR/∂vBC = - g_r
        let g_c_bc = -g_r;

        // ∂Ib/∂vBE = (1-αF) * g_f
        let g_b_be = (1.0 - alpha_f) * g_f;

        // ∂Ib/∂vBC = (1-αR) * g_r
        let g_b_bc = (1.0 - alpha_r) * g_r;

        // ∂Ie/∂vBE = - g_f     (because i_e0 has -i_F term)
        let g_e_be = -g_f;

        // ∂Ie/∂vBC = αR * g_r
        let g_e_bc = alpha_r * g_r;

        // Step 2: convert junction-voltage derivatives into node-voltage derivatives.
        //
        // v_BE = Vb - Ve  =>  dv_BE/dVb = +1, dv_BE/dVe = -1, dv_BE/dVc = 0
        // v_BC = Vb - Vc  =>  dv_BC/dVb = +1, dv_BC/dVc = -1, dv_BC/dVe = 0
        //
        // Chain rule:
        // ∂I/∂Vb = ∂I/∂vBE * 1  + ∂I/∂vBC * 1  = g_*_be + g_*_bc
        // ∂I/∂Vc = ∂I/∂vBC * (-1)              = -g_*_bc
        // ∂I/∂Ve = ∂I/∂vBE * (-1)              = -g_*_be

        // Collector-row Jacobian entries: [∂Ic/∂Vb, ∂Ic/∂Vc, ∂Ic/∂Ve]
        let g_cb = g_c_be + g_c_bc; // ∂Ic/∂Vb
        let g_cc = -g_c_bc; // ∂Ic/∂Vc
        let g_ce = -g_c_be; // ∂Ic/∂Ve

        // Base-row Jacobian entries: [∂Ib/∂Vb, ∂Ib/∂Vc, ∂Ib/∂Ve]
        let g_bb = g_b_be + g_b_bc; // ∂Ib/∂Vb
        let g_bc = -g_b_bc; // ∂Ib/∂Vc
        let g_be = -g_b_be; // ∂Ib/∂Ve

        // Emitter-row Jacobian entries: [∂Ie/∂Vb, ∂Ie/∂Vc, ∂Ie/∂Ve]
        let g_eb = g_e_be + g_e_bc; // ∂Ie/∂Vb
        let g_ec = -g_e_bc; // ∂Ie/∂Vc
        let g_ee = -g_e_be; // ∂Ie/∂Ve

        let i_eq_c = i_c - g_c_be * vbe_eff_node - g_c_bc * vbc_eff_node;
        let i_eq_b = i_b - g_b_be * vbe_eff_node - g_b_bc * vbc_eff_node;
        let i_eq_e = i_e - g_e_be * vbe_eff_node - g_e_bc * vbc_eff_node;

        LinearizedBjt {
            g_bb,
            g_bc,
            g_be,
            g_cb,
            g_cc,
            g_ce,
            g_eb,
            g_ec,
            g_ee,
            i_eq_b,
            i_eq_c,
            i_eq_e,
        }
    }

    /// Stamp the linearized BJT conductance matrix and RHS into MNA.
    pub(crate) fn stamp_nonlinear(&self, m: &mut SolverMatrix, guess: &[f64]) {
        let base = self.base;
        let collector = self.collector;
        let emitter = self.emitter;

        // compute the junctions voltage diffs
        let v_be = get_voltage_diff(guess, base, emitter);
        let v_bc = get_voltage_diff(guess, base, collector);

        let linearized = self.linearize(v_be, v_bc);

        if let Some(index) = self.stamp.bb {
            *m.get_mut_nnz(index) += linearized.g_bb;
        }
        if let Some(index) = self.stamp.bc {
            *m.get_mut_nnz(index) += linearized.g_bc;
        }
        if let Some(index) = self.stamp.be {
            *m.get_mut_nnz(index) += linearized.g_be;
        }
        if let Some(index) = self.stamp.cb {
            *m.get_mut_nnz(index) += linearized.g_cb;
        }
        if let Some(index) = self.stamp.cc {
            *m.get_mut_nnz(index) += linearized.g_cc;
        }
        if let Some(index) = self.stamp.ce {
            *m.get_mut_nnz(index) += linearized.g_ce;
        }
        if let Some(index) = self.stamp.eb {
            *m.get_mut_nnz(index) += linearized.g_eb;
        }
        if let Some(index) = self.stamp.ec {
            *m.get_mut_nnz(index) += linearized.g_ec;
        }
        if let Some(index) = self.stamp.ee {
            *m.get_mut_nnz(index) += linearized.g_ee;
        }

        if let Some(base) = base {
            *m.get_mut_rhs(base) -= linearized.i_eq_b;
        }
        if let Some(collector) = collector {
            *m.get_mut_rhs(collector) -= linearized.i_eq_c;
        }
        if let Some(emitter) = emitter {
            *m.get_mut_rhs(emitter) -= linearized.i_eq_e;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::build_devices;

    fn bjt(instance_params: &str, model_params: &str) -> Bjt {
        build_devices(&format!(
            "bjt\nQ1 c b 0 QN {instance_params}\n.MODEL QN NPN {model_params}\n.end\n"
        ))
        .bjts
        .remove(0)
    }

    // ngspice scales IS by area (bjttemp.c) and every contribution by m
    // (bjtload.c). In this Ebers-Moll model both fold into IS.

    #[test]
    fn area_scales_the_saturation_current() {
        let plain = bjt("", "is=1e-16");
        let q = bjt("area=2", "is=1e-16");
        assert_eq!(q.saturation_current, plain.saturation_current * 2.0);
    }

    #[test]
    fn m_scales_the_saturation_current() {
        let plain = bjt("", "is=1e-16");
        let q = bjt("m=3", "is=1e-16");
        assert_eq!(q.saturation_current, plain.saturation_current * 3.0);
    }

    #[test]
    fn area_and_m_combine() {
        let plain = bjt("", "is=1e-16");
        let q = bjt("area=2 m=3", "is=1e-16");
        assert_eq!(q.saturation_current, plain.saturation_current * 2.0 * 3.0);
    }

    #[test]
    fn defaults_match_ngspice() {
        // bjtsetup.c: IS = 1e-16, BF = 100, BR = 1, NF = 1, NR = 1.
        let q = bjt("", "");
        assert_eq!(q.saturation_current, 1e-16);
        assert_eq!(q.beta_forward, 100.0);
        assert_eq!(q.beta_reverse, 1.0);
        assert_eq!(q.emission_coeff_forward, 1.0);
        assert_eq!(q.emission_coeff_reverse, 1.0);
    }
}
