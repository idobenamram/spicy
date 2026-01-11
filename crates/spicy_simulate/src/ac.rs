use ndarray::{Array1, Array2, s};
use ndarray_linalg::{FactorizeInto, Solve};
use spicy_parser::{
    Value,
    instance_parser::Deck,
    netlist_types::{AcCommand, AcSweepType},
};

use crate::devices::{Capacitor, Devices, IndependentSource, Inductor, Resistor};
use spicy_parser::node_mapping::NodeMapping;
use std::f64::consts::PI;

fn stamp_resistor_ac(ar: &mut Array2<f64>, device: &Resistor, node_mapping: &NodeMapping) {
    let g = 1.0 / device.ac;
    let node1 = node_mapping.mna_node_index(device.positive);
    let node2 = node_mapping.mna_node_index(device.negative);

    if let Some(n1) = node1 {
        ar[[n1, n1]] += g;
    }
    if let Some(n2) = node2 {
        ar[[n2, n2]] += g;
    }
    if let (Some(n1), Some(n2)) = (node1, node2) {
        ar[[n1, n2]] -= g;
        ar[[n2, n1]] -= g;
    }
}

fn stamp_capacitor_ac(ai: &mut Array2<f64>, device: &Capacitor, node_mapping: &NodeMapping, w: f64) {
    let node1 = node_mapping.mna_node_index(device.positive);
    let node2 = node_mapping.mna_node_index(device.negative);
    // Yc = j * w * C -> purely imaginary admittance placed on ai
    let yc = w * device.capacitance;

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

fn stamp_inductor_ac_mna(
    ar: &mut Array2<f64>,
    ai: &mut Array2<f64>,
    device: &Inductor,
    node_mapping: &NodeMapping,
    w: f64,
) {
    let node1 = node_mapping.mna_node_index(device.positive);
    let node2 = node_mapping.mna_node_index(device.negative);
    let k = node_mapping.mna_branch_index(device.current_branch);

    // Incidence (real part): same as DC B and B^T
    if let Some(n1) = node1 {
        ar[[n1, k]] += 1.0;
        ar[[k, n1]] += 1.0;
    }
    if let Some(n2) = node2 {
        ar[[n2, k]] -= 1.0;
        ar[[k, n2]] -= 1.0;
    }

    // KVL: v = (Va - Vb) - j*w*L*i = 0 -> put +w*L on imag diagonal of KVL row/col
    let wl = w * device.inductance;
    ai[[k, k]] += wl;
}

/// TODO: this and stamp_voltage_source_incidence are pretty much the same
fn stamp_voltage_source_incidence_real(
    ar: &mut Array2<f64>,
    device: &IndependentSource,
    node_mapping: &NodeMapping,
) {
    let n1 = node_mapping.mna_node_index(device.positive);
    let n2 = node_mapping.mna_node_index(device.negative);
    let k = node_mapping.mna_branch_index(device.current_branch);

    if let Some(n1) = n1 {
        ar[[n1, k]] += 1.0;
        ar[[k, n1]] += 1.0;
    }
    if let Some(n2) = n2 {
        ar[[n2, k]] += -1.0;
        ar[[k, n2]] += -1.0;
    }
}

fn stamp_voltage_source_phasor_ac(
    br: &mut Array1<f64>,
    bi: &mut Array1<f64>,
    device: &IndependentSource,
    node_mapping: &NodeMapping,
) {
    let k = node_mapping.mna_branch_index(device.current_branch);

    if let Some(phasor) = &device.ac {
        let mag = phasor.mag.get_value();
        let phase = phasor.phase.as_ref().map(|v| v.get_value()).unwrap_or(0.0);
        let ph = phase * PI / 180.0;
        let re = mag * ph.cos();
        let im = mag * ph.sin();
        br[k] += re;
        bi[k] += im;
    }
}

fn stamp_current_source_phasor_ac(
    br: &mut Array1<f64>,
    bi: &mut Array1<f64>,
    device: &IndependentSource,
    node_mapping: &NodeMapping,
) {
    if let Some(ac) = &device.ac {
        let mag = ac.mag.get_value();
        let phase = ac.phase.as_ref().map(|v| v.get_value()).unwrap_or(0.0);
        let ph = phase * PI / 180.0;
        let re = mag * ph.cos();
        let im = mag * ph.sin();

        let n1 = node_mapping.mna_node_index(device.positive);
        let n2 = node_mapping.mna_node_index(device.negative);

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

fn ac_frequencies(cmd: &AcCommand) -> Vec<f64> {
    let fstart = cmd.fstart.get_value();
    let fstop = cmd.fstop.get_value();
    assert!(
        fstop > fstart,
        ".AC: fstop {:?} must be > fstart {:?}",
        fstop,
        fstart
    );

    const EPS: f64 = 1e-12;

    match &cmd.ac_sweep_type {
        AcSweepType::Dec(n) => {
            let n = *n;
            assert!(n >= 1, ".AC DEC: N must be >= 1");
            assert!(fstart > 0.0, ".AC DEC: fstart must be > 0");
            let r = 10f64.powf(1.0 / n as f64); // ratio per point
            let mut f = fstart;
            let mut out = Vec::new();
            while f <= fstop * (1.0 + EPS) {
                out.push(f);
                f *= r;
            }
            out
        }
        AcSweepType::Oct(n) => {
            let n = *n;
            assert!(n >= 1, ".AC OCT: N must be >= 1");
            assert!(fstart > 0.0, ".AC OCT: fstart must be > 0");
            let r = 2f64.powf(1.0 / n as f64); // ratio per point
            let mut f = fstart;
            let mut out = Vec::new();
            while f <= fstop * (1.0 + EPS) {
                out.push(f);
                f *= r;
            }
            out
        }
        AcSweepType::Lin(n) => {
            let n = *n;
            assert!(n >= 1, ".AC LIN: N must be >= 1");
            if n == 1 {
                return vec![fstart];
            }
            let step = (fstop - fstart) / ((n - 1) as f64);
            (0..n).map(|k| fstart + k as f64 * step).collect()
        }
    }
}

/// 2x2 block expansion explanation
/// in ac you need to solve:
///  (A_r + j A_i)(x_r + j x_i) = b_r + j b_i
/// that gives us 2 real equations (by expanding the product):
///  A_r x_r - A_i x_i = b_r
///  A_i x_r + A_r x_i = b_i
/// so we can solve for x_r and x_i by solving the system:
///  [A_r -A_i] [x_r] = [b_r]
///  [A_i  A_r] [x_i]   [b_i]
/// which is the same as the real system:
/// Assemble the AC small-signal system using a real 2x2 block expansion.
/// Returns (M, s) where M is 2*(n+k) square and s is length 2*(n+k).
fn assemble_ac_real_expansion(
    devices: &Devices,
    node_mapping: &NodeMapping,
    w: f64,
) -> (Array2<f64>, Array1<f64>) {
    let n = node_mapping.nodes_len();
    let k = node_mapping.branches_len();

    // Real and Imag parts of the small-signal MNA (size (n+k) x (n+k))
    let mut ar = Array2::<f64>::zeros((n + k, n + k));
    let mut ai = Array2::<f64>::zeros((n + k, n + k));

    // RHS real/imag
    let mut br = Array1::<f64>::zeros(n + k);
    let mut bi = Array1::<f64>::zeros(n + k);

    for dev in &devices.resistors {
        stamp_resistor_ac(&mut ar, dev, node_mapping);
    }
    for dev in &devices.capacitors {
        stamp_capacitor_ac(&mut ai, dev, node_mapping, w);
    }
    for dev in &devices.inductors {
        stamp_inductor_ac_mna(&mut ar, &mut ai, dev, node_mapping, w);
    }
    for dev in &devices.voltage_sources {
        stamp_voltage_source_incidence_real(&mut ar, dev, node_mapping);
        stamp_voltage_source_phasor_ac(&mut br, &mut bi, dev, node_mapping);
    }
    for dev in &devices.current_sources {
        stamp_current_source_phasor_ac(&mut br, &mut bi, dev, node_mapping);
    }

    // Build the 2x2 real system: [ Ar  -Ai ; Ai  Ar ] * [xr; xi] = [br; bi]
    let dim = n + k;
    let mut m = Array2::<f64>::zeros((2 * dim, 2 * dim));
    // Top-left Ar and top-right -Ai
    m.slice_mut(s![0..dim, 0..dim]).assign(&ar);
    m.slice_mut(s![0..dim, dim..2 * dim]).assign(&(-&ai));
    // Bottom-left Ai and bottom-right Ar
    m.slice_mut(s![dim..2 * dim, 0..dim]).assign(&ai);
    m.slice_mut(s![dim..2 * dim, dim..2 * dim]).assign(&ar);

    let mut s_vec = Array1::<f64>::zeros(2 * dim);
    s_vec.slice_mut(s![0..dim]).assign(&br);
    s_vec.slice_mut(s![dim..2 * dim]).assign(&bi);

    (m, s_vec)
}

pub fn simulate_ac(deck: &Deck, cmd: &AcCommand) -> Vec<(f64, Array1<f64>, Array1<f64>)> {
    let freqs = ac_frequencies(cmd);
    let devices = Devices::from_spec(&deck.devices);
    let node_mapping = &deck.node_mapping;
    let n = node_mapping.nodes_len();
    let k = node_mapping.branches_len();

    let mut out = Vec::new();

    for f in freqs {
        let w = 2.0 * PI * f;
        let (m, s_vec) = assemble_ac_real_expansion(&devices, node_mapping, w);
        let lu = m.factorize_into().expect("Failed to factorize AC matrix");
        let x = lu.solve(&s_vec).expect("Failed to solve AC system");

        let dim = n + k;
        let xr = x.slice(s![0..dim]).to_owned();
        let xi = x.slice(s![dim..2 * dim]).to_owned();

        // Optional: print node phasors
        let node_names = node_mapping.node_names_mna_order();
        for i in 0..n {
            let vr = xr[i];
            let vi = xi[i];
            let mag = (vr * vr + vi * vi).sqrt();
            let phase = vi.atan2(vr) * 180.0 / PI;
            println!(
                "f={:.6} Hz  {}: {:.6} ∠ {:.3}°",
                f, node_names[i], mag, phase
            );
        }

        out.push((f, xr, xi));
    }

    out
}
