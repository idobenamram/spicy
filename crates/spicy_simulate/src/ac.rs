use ndarray::{Array1, Array2, s};
use ndarray_linalg::{FactorizeInto, Solve};
use spicy_parser::{
    instance_parser::Deck,
    netlist_types::{AcCommand, AcSweepType},
};

use crate::SimulationConfig;
use crate::devices::Devices;
use spicy_parser::node_mapping::NodeMapping;
use std::f64::consts::PI;

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
        dev.stamp_ac(&mut ar, node_mapping);
    }
    for dev in &devices.capacitors {
        dev.stamp_ac(&mut ai, node_mapping, w);
    }
    for dev in &devices.inductors {
        dev.stamp_ac(&mut ar, &mut ai, node_mapping, w);
    }
    for dev in &devices.voltage_sources {
        dev.stamp_ac_voltage_source(&mut ar, &mut br, &mut bi, node_mapping);
    }
    for dev in &devices.current_sources {
        dev.stamp_ac_current_source(&mut br, &mut bi, node_mapping);
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

pub fn simulate_ac(
    deck: &Deck,
    cmd: &AcCommand,
    _sim_config: &SimulationConfig,
) -> Vec<(f64, Array1<f64>, Array1<f64>)> {
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
