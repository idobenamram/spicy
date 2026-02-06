//! Simple Ebers-Moll BJT model (NPN/PNP).
//!
//! Uses base-emitter/base-collector junctions and alpha gains, then
//! linearizes around the current Newton guess for MNA stamping.
use super::stamp::NodeTripletStamp;
use crate::matrix::SolverMatrix;
use crate::util::get_voltage_diff;
use spicy_parser::BjtPolarity;
use spicy_parser::Span;
use spicy_parser::devices::BjtSpec;
use spicy_parser::netlist_types::NodeIndex;

const DEFAULT_THERMAL_VOLTAGE: f64 = 0.02585;
const DEFAULT_EXP_LIMIT: f64 = 40.0;

#[derive(Debug, Clone)]
pub struct Bjt {
    // Stored for diagnostics / SPICE compatibility; not used by the solver yet.
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub span: Span,
    pub collector: NodeIndex,
    pub base: NodeIndex,
    pub emitter: NodeIndex,
    pub polarity: BjtPolarity,
    /// Saturation current (A).
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
    #[allow(dead_code)]
    pub area: f64,
    #[allow(dead_code)]
    pub m: f64,
    /// Thermal voltage (Vt) used in exp(V / (n * Vt)).
    pub thermal_voltage: f64,
    /// Clamp limit for V/Vt to keep exp() bounded.
    pub exp_limit: f64,
    #[allow(dead_code)]
    pub off: bool,
    #[allow(dead_code)]
    pub ic_vbe: f64,
    #[allow(dead_code)]
    pub ic_vce: Option<f64>,
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
    pub fn from_spec(spec: &BjtSpec) -> Self {
        let saturation_current = spec
            .model
            .is
            .as_ref()
            .map(|v| v.get_value())
            .unwrap_or(1e-14);

        let beta_forward = spec
            .model
            .bf
            .as_ref()
            .map(|v| v.get_value())
            .unwrap_or(100.0);
        let beta_reverse = spec.model.br.as_ref().map(|v| v.get_value()).unwrap_or(1.0);
        let emission_coeff_forward = spec.model.nf.as_ref().map(|v| v.get_value()).unwrap_or(1.0);
        let emission_coeff_reverse = spec.model.nr.as_ref().map(|v| v.get_value()).unwrap_or(1.0);

        let area = spec.area.as_ref().map(|v| v.get_value()).unwrap_or(1.0);
        let m = spec.m.as_ref().map(|v| v.get_value()).unwrap_or(1.0);
        let off = spec.off.unwrap_or(false);
        let ic_vbe = spec.ic_vbe.as_ref().map(|v| v.get_value()).unwrap_or(0.0);
        let ic_vce = spec.ic_vce.as_ref().map(|v| v.get_value());

        Self {
            name: spec.name.clone(),
            span: spec.span,
            collector: spec.collector,
            base: spec.base,
            emitter: spec.emitter,
            polarity: spec.model.polarity,
            saturation_current,
            beta_forward,
            beta_reverse,
            emission_coeff_forward,
            emission_coeff_reverse,
            area,
            m,
            thermal_voltage: DEFAULT_THERMAL_VOLTAGE,
            exp_limit: DEFAULT_EXP_LIMIT,
            off,
            ic_vbe,
            ic_vce,
            stamp: NodeTripletStamp::unitialized(),
        }
    }

    /// Return +1 for NPN, -1 for PNP.
    ///
    /// This flips the sign of control voltages and resulting currents to
    /// reuse the same Ebers-Moll equations for both polarities.
    fn polarity_sign(&self) -> f64 {
        match self.polarity {
            BjtPolarity::Npn => 1.0,
            BjtPolarity::Pnp => -1.0,
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
        let base = m.mna_node_index(self.base);
        let collector = m.mna_node_index(self.collector);
        let emitter = m.mna_node_index(self.emitter);

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
