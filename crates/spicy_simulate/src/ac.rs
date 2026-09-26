use ndarray::{Array1, Array2, s};
use ndarray_linalg::{FactorizeInto, Solve};
use spicy_circuit::{AcSpacing, AcSweep, Circuit, Params};

use crate::SimulationConfig;
use crate::devices::Devices;
use crate::unknowns::Layout;
use std::f64::consts::PI;

fn ac_frequencies(sweep: &AcSweep) -> Vec<f64> {
    let fstart = sweep.start;
    let fstop = sweep.stop;
    assert!(
        fstop > fstart,
        ".AC: fstop {:?} must be > fstart {:?}",
        fstop,
        fstart
    );

    const EPS: f64 = 1e-12;

    match sweep.spacing {
        AcSpacing::Decade(n) => {
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
        AcSpacing::Octave(n) => {
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
        AcSpacing::Linear(n) => {
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
    layout: &Layout,
    w: f64,
) -> (Array2<f64>, Array1<f64>) {
    let n = layout.nodes();
    let k = layout.branches();

    // Real and Imag parts of the small-signal MNA (size (n+k) x (n+k))
    let mut ar = Array2::<f64>::zeros((n + k, n + k));
    let mut ai = Array2::<f64>::zeros((n + k, n + k));

    // RHS real/imag
    let mut br = Array1::<f64>::zeros(n + k);
    let mut bi = Array1::<f64>::zeros(n + k);

    for dev in &devices.resistors {
        dev.stamp_ac(&mut ar);
    }
    for dev in &devices.capacitors {
        dev.stamp_ac(&mut ai, w);
    }
    for dev in &devices.inductors {
        dev.stamp_ac(&mut ar, &mut ai, w);
    }
    for dev in &devices.voltage_sources {
        dev.stamp_ac(&mut ar, &mut br, &mut bi);
    }
    for dev in &devices.current_sources {
        dev.stamp_ac(&mut br, &mut bi);
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

/// AC results, one entry per frequency: (frequency in Hz, real parts, imaginary
/// parts). The vectors follow the order of [`crate::unknowns`].
pub type AcResult = Vec<(f64, Array1<f64>, Array1<f64>)>;

pub fn simulate_ac(
    circuit: &Circuit,
    params: &Params,
    sweep: &AcSweep,
    _sim_config: &SimulationConfig,
) -> AcResult {
    let freqs = ac_frequencies(sweep);
    let layout = Layout::new(circuit);
    let devices = Devices::new(circuit, params, &layout);

    let mut out = Vec::new();

    for f in freqs {
        let w = 2.0 * PI * f;
        let (m, s_vec) = assemble_ac_real_expansion(&devices, &layout, w);
        let lu = m.factorize_into().expect("Failed to factorize AC matrix");
        let x = lu.solve(&s_vec).expect("Failed to solve AC system");

        let dim = layout.dim();
        let xr = x.slice(s![0..dim]).to_owned();
        let xi = x.slice(s![dim..2 * dim]).to_owned();
        out.push((f, xr, xi));
    }

    out
}
