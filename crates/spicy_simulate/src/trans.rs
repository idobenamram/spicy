use std::collections::HashMap;

use spicy_parser::{instance_parser::Deck, netlist_types::TranCommand};

use crate::{
    NewtonConfig, NewtonMode, NewtonState, SimulationConfig, TransientIntegrator,
    dc::simulate_op_inner,
    devices::{Capacitor, Devices, Inductor},
    error::SimulationError,
    matrix::SolverMatrix,
    util::get_voltage_diff,
};

fn steps(dt: f64, tstop: f64) -> Vec<f64> {
    let nsteps = (tstop / dt).floor() as usize;
    (0..=nsteps).map(|i| i as f64 * dt).collect()
}

fn get_previous_voltage(
    previous_voltages: &[f64],
    positive: Option<usize>,
    negative: Option<usize>,
    ic: f64,
    use_device_ic: bool,
) -> f64 {
    if use_device_ic {
        // if we set the uic flag we should just take the initial condition from the device
        ic
    } else {
        get_voltage_diff(previous_voltages, positive, negative)
    }
}

fn get_previous_current(
    previous_solution: &[f64],
    branch_index: usize,
    ic: f64,
    use_device_ic: bool,
) -> f64 {
    if use_device_ic {
        ic
    } else {
        previous_solution[branch_index]
    }
}

fn abs_rel_ok(a: f64, b: f64, abs_tol: f64, rel_tol: f64) -> bool {
    let diff = (a - b).abs();
    if diff <= abs_tol {
        return true;
    }
    let scale = a.abs().max(b.abs());
    diff <= abs_tol + rel_tol * scale
}

fn converged(prev: &[f64], next: &[f64], config: &NewtonConfig) -> bool {
    prev.iter()
        .zip(next.iter())
        .all(|(&a, &b)| abs_rel_ok(a, b, config.abs_tol, config.rel_tol))
}

pub(crate) fn newton_solve<F>(
    matrix: &mut SolverMatrix,
    state: &mut NewtonState,
    mut guess: Vec<f64>,
    time: Option<f64>,
    mut stamp: F,
) -> Result<(Vec<f64>, usize), SimulationError>
where
    F: FnMut(&mut SolverMatrix, &[f64]) -> Result<(), SimulationError>,
{
    let max_iters = state.config.max_iters;
    for iter in 0..max_iters {
        matrix.clear();
        stamp(matrix, &guess)?;

        matrix.ensure_analyzed()?;
        let is_init = matches!(state.mode, NewtonMode::InitOp | NewtonMode::InitTrans);
        if is_init && iter == 0 {
            matrix.factorize()?;
        } else {
            // TODO: for KLU, consider retrying full factorization on refactor failure.
            matrix.refactor()?;
        }

        // after the first iteration we iterate
        if is_init {
            state.mode = NewtonMode::Iterate;
        }

        matrix.solve()?;
        let solution = matrix.rhs().to_vec();

        if iter > 0 && converged(&guess, &solution, &state.config) {
            return Ok((solution, iter + 1));
        }
        guess = solution;
    }

    Err(SimulationError::NonConvergence {
        time,
        iters: max_iters,
    })
}

#[derive(Debug)]
pub enum Integrator<'a> {
    BackwardEuler {
        previous: Vec<f64>,
    },
    Trapezoidal {
        previous_output: Vec<f64>,
        previous_currents: HashMap<&'a str, f64>,
    },
}

impl<'a> Integrator<'a> {
    fn capacitor_values(
        &self,
        device: &Capacitor,
        positive: Option<usize>,
        negative: Option<usize>,
        config: &TransientConfig,
    ) -> (f64, f64) {
        match self {
            Integrator::BackwardEuler { previous } => {
                let c = device.capacitance;
                let g = c / config.step;
                let previous_voltage = get_previous_voltage(
                    previous,
                    positive,
                    negative,
                    device.ic,
                    config.use_device_ic,
                );
                let i = g * previous_voltage;
                (g, i)
            }
            Integrator::Trapezoidal {
                previous_output,
                previous_currents,
            } => {
                let c = device.capacitance;
                let g = 2.0 * c / config.step;
                let previous_voltage = get_previous_voltage(
                    previous_output,
                    positive,
                    negative,
                    device.ic,
                    config.use_device_ic,
                );
                let previous_current = previous_currents.get(device.name.as_str()).unwrap_or(&0.0);
                let i = -g * previous_voltage - previous_current;
                (g, i)
            }
        }
    }

    fn save_capacitor_current(&mut self, device: &'a Capacitor, current: f64) {
        match self {
            Integrator::BackwardEuler { previous: _ } => {}
            Integrator::Trapezoidal {
                previous_currents, ..
            } => {
                previous_currents.insert(device.name.as_str(), current);
            }
        }
    }

    fn inductor_values(
        &self,
        device: &Inductor,
        positive: Option<usize>,
        negative: Option<usize>,
        branch_index: usize,
        config: &TransientConfig,
    ) -> (f64, f64) {
        match self {
            Integrator::BackwardEuler { previous } => {
                let r_eq = device.inductance / config.step;
                let i_prev =
                    get_previous_current(previous, branch_index, device.ic, config.use_device_ic);
                let v_hist = -r_eq * i_prev;
                (r_eq, v_hist)
            }
            Integrator::Trapezoidal {
                previous_output, ..
            } => {
                let r_eq = 2.0 * device.inductance / config.step;
                let i_prev = get_previous_current(
                    previous_output,
                    branch_index,
                    device.ic,
                    config.use_device_ic,
                );
                let v_prev = get_voltage_diff(previous_output, positive, negative);
                let v_hist = -v_prev - r_eq * i_prev;
                (r_eq, v_hist)
            }
        }
    }

    fn save_previous_voltage(&mut self, voltage: Vec<f64>) {
        match self {
            Integrator::BackwardEuler { previous } => {
                *previous = voltage;
            }
            Integrator::Trapezoidal {
                previous_output, ..
            } => {
                *previous_output = voltage;
            }
        }
    }

    fn get_previous_output(&self) -> &[f64] {
        match self {
            Integrator::BackwardEuler { previous } => previous,
            Integrator::Trapezoidal {
                previous_output, ..
            } => previous_output,
        }
    }
}

#[derive(Debug)]
struct TransientConfig {
    /// the increment time
    step: f64,
    /// the final time for the simulation
    tstop: f64,
    /// the current time
    t: f64,
    /// should only be valid when uic is true and we are on the first iteration
    use_device_ic: bool,
}

fn stamp_transient<'a>(
    matrix: &mut SolverMatrix,
    devices: &'a Devices,
    config: &TransientConfig,
    integrator: &Integrator<'a>,
    guess: &[f64],
) -> Result<(), SimulationError> {
    for r in &devices.resistors {
        r.stamp_dc(matrix);
    }

    for d in &devices.diodes {
        d.stamp_nonlinear(matrix, guess);
    }

    for bjt in &devices.bjts {
        bjt.stamp_nonlinear(matrix, guess);
    }

    for c in &devices.capacitors {
        let pos = matrix.mna_node_index(c.positive);
        let neg = matrix.mna_node_index(c.negative);

        let (g, i) = integrator.capacitor_values(c, pos, neg, config);
        c.stamp_trans(matrix, g, i);
    }

    for l in &devices.inductors {
        let pos = matrix.mna_node_index(l.positive);
        let neg = matrix.mna_node_index(l.negative);
        let branch = matrix.mna_branch_index(l.current_branch);

        let (r_eq, v_hist) = integrator.inductor_values(l, pos, neg, branch, config);
        l.stamp_trans(matrix, r_eq, v_hist);
    }

    for vsrc in &devices.voltage_sources {
        vsrc.stamp_voltage_source_trans(matrix, config.t, config.step, config.tstop);
    }

    for isrc in &devices.current_sources {
        isrc.stamp_current_source_trans(matrix, config.t, config.step, config.tstop);
    }

    Ok(())
}

fn simulation_step<'a>(
    matrix: &mut SolverMatrix,
    devices: &'a Devices,
    config: &TransientConfig,
    integrator: &mut Integrator<'a>,
    newton: &mut NewtonState,
    time: f64,
) -> Result<(Vec<f64>, usize), SimulationError> {
    let initial_guess = integrator.get_previous_output().to_vec();
    let (solution, iters) =
        newton_solve(matrix, newton, initial_guess, Some(time), |m, guess| {
            stamp_transient(m, devices, config, integrator, guess)
        })?;

    if matches!(&*integrator, Integrator::Trapezoidal { .. }) {
        for c in &devices.capacitors {
            let pos = matrix.mna_node_index(c.positive);
            let neg = matrix.mna_node_index(c.negative);
            let (g, i_hist) = integrator.capacitor_values(c, pos, neg, config);
            let v_new = get_voltage_diff(&solution, pos, neg);
            let i_new = g * v_new + i_hist;
            integrator.save_capacitor_current(c, i_new);
        }
    }

    Ok((solution, iters))
}

#[derive(Debug, Clone)]
pub struct TransientResult {
    pub times: Vec<f64>,
    /// names for node voltages (index aligned with solution vector 0..n-1)
    pub node_names: Vec<String>,
    /// names for voltage source currents (index aligned after nodes)
    pub source_names: Vec<String>,
    /// one sample per time with all unknowns (node voltages and source currents)
    pub samples: Vec<Vec<f64>>,
    /// number of Newton iterations per time sample (aligned with `times`)
    pub newton_iterations: Vec<usize>,
}

pub fn simulate_trans(
    deck: &Deck,
    cmd: &TranCommand,
    sim_config: &SimulationConfig,
) -> Result<TransientResult, SimulationError> {
    let tstep = cmd.tstep.get_value();
    let tstop = cmd.tstop.get_value();

    let mut devices = Devices::from_spec(&deck.devices);

    let mut matrix =
        SolverMatrix::create_matrix(&mut devices, deck.node_mapping.clone(), sim_config)?;

    let mut config = TransientConfig {
        // TODO: this is not really correct but ok for now, tstep doesn't have to be the step size
        step: tstep,
        tstop,
        t: 0.0,
        use_device_ic: cmd.uic,
    };

    // Initialize previous solution vector.
    let initial_condition: Vec<f64> = if cmd.uic {
        unimplemented!("UIC is not supported yet");
    } else {
        // When there is no initial conditions we use the operating point as the initial condition.
        let mut op_state = NewtonState::new(sim_config.newton, NewtonMode::InitOp);
        simulate_op_inner(&mut matrix, &devices, &mut op_state)?;
        matrix.rhs().to_vec()
    };

    let mut integrator = match sim_config.integrator {
        TransientIntegrator::BackwardEuler => Integrator::BackwardEuler {
            previous: initial_condition,
        },
        TransientIntegrator::Trapezoidal => Integrator::Trapezoidal {
            previous_output: initial_condition,
            previous_currents: HashMap::new(),
        },
    };

    let mut times: Vec<f64> = Vec::new();
    let mut samples: Vec<Vec<f64>> = Vec::new();
    let mut newton_iterations: Vec<usize> = Vec::new();
    let mut newton_state = NewtonState::new(sim_config.newton, NewtonMode::InitTrans);

    // initial sample at t=0 using current state (before any transient step)
    // note this means that for UIC even the the voltage source nodes will have a value of 0 at t=0
    times.push(0.0);
    samples.push(integrator.get_previous_output().to_vec());
    newton_iterations.push(0);

    let steps = steps(config.step, tstop);
    for step in steps.into_iter().skip(1) {
        config.t = step;
        let (x, iters) = simulation_step(
            &mut matrix,
            &devices,
            &config,
            &mut integrator,
            &mut newton_state,
            step,
        )?;

        integrator.save_previous_voltage(x.clone());
        config.use_device_ic = false;

        times.push(step);
        samples.push(x.to_vec());
        newton_iterations.push(iters);
    }

    Ok(TransientResult {
        times,
        node_names: deck.node_mapping.node_names_mna_order(),
        source_names: deck.node_mapping.branch_names_mna_order(),
        samples,
        newton_iterations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::klu::KluConfig;
    use crate::{LinearSolver, SimulationConfig};
    use spicy_parser::{ParseOptions, SourceMap, netlist_types::Command, parse};
    use std::path::PathBuf;

    fn round_sig(x: f64, sig: i32) -> f64 {
        if x == 0.0 || !x.is_finite() {
            return x;
        }
        let exp10 = x.abs().log10().floor() as i32;
        let digits = sig - 1 - exp10;
        let scale = 10f64.powi(digits);
        (x * scale).round() / scale
    }

    #[test]
    fn trans_klu_and_blas_are_similar() {
        // Simple RC with a sinusoidal source (no inductors, no UIC).
        let netlist = "* RC driven by sinusoidal source\n\
V1 in 0 SIN(0 1 10)\n\
R1 in out 1k\n\
C1 out 0 1u\n\
.TRAN 0.001 0.01\n\
.END";

        let source_path = PathBuf::from("trans_klu_vs_blas.spicy");
        let source_map = SourceMap::new(source_path.clone(), netlist.to_string());
        let mut parse_options = ParseOptions {
            source_map,
            work_dir: PathBuf::from("."),
            source_path,
            max_include_depth: 10,
        };
        let deck = parse(&mut parse_options).expect("parse");

        let tran_cmd = deck
            .commands
            .iter()
            .find_map(|c| match c {
                Command::Tran(cmd) => Some(cmd),
                _ => None,
            })
            .expect("expected .TRAN command");

        let klu_cfg = SimulationConfig {
            solver: LinearSolver::Klu {
                config: KluConfig::default(),
            },
            ..SimulationConfig::default()
        };
        let blas_cfg = SimulationConfig {
            solver: LinearSolver::Blas,
            ..SimulationConfig::default()
        };

        let klu = simulate_trans(&deck, tran_cmd, &klu_cfg).expect("simulate_trans klu");
        let blas = simulate_trans(&deck, tran_cmd, &blas_cfg).expect("simulate_trans blas");

        assert_eq!(klu.times, blas.times, "time grids differ");
        assert_eq!(
            klu.node_names, blas.node_names,
            "node name ordering differs"
        );
        assert_eq!(
            klu.source_names, blas.source_names,
            "source name ordering differs"
        );
        assert_eq!(
            klu.samples.len(),
            blas.samples.len(),
            "sample count differs"
        );
        assert_eq!(
            klu.newton_iterations.len(),
            klu.times.len(),
            "klu newton iteration count differs from times"
        );
        assert_eq!(
            blas.newton_iterations.len(),
            blas.times.len(),
            "blas newton iteration count differs from times"
        );

        // Compare with "smart rounding" (significant digits) to avoid tiny solver-dependent noise.
        const SIG: i32 = 10;
        for (t_idx, (a, b)) in klu.samples.iter().zip(blas.samples.iter()).enumerate() {
            assert_eq!(a.len(), b.len(), "sample width differs at t_idx={}", t_idx);
            for (i, (&xa, &xb)) in a.iter().zip(b.iter()).enumerate() {
                let ra = round_sig(xa, SIG);
                let rb = round_sig(xb, SIG);
                assert!(
                    ra == rb,
                    "value differs at t_idx={}, i={}: klu={} blas={} (rounded: {} vs {})",
                    t_idx,
                    i,
                    xa,
                    xb,
                    ra,
                    rb
                );
            }
        }
    }

    #[test]
    fn trapezoidal_saves_capacitor_current_not_history_source() {
        let netlist = "* RC with capacitor to ground\n\
V1 in 0 DC 1\n\
R1 in out 1k\n\
C1 out 0 1u\n\
.END";

        let source_path = PathBuf::from("trans_trapezoidal_cap_current.spicy");
        let source_map = SourceMap::new(source_path.clone(), netlist.to_string());
        let mut parse_options = ParseOptions {
            source_map,
            work_dir: PathBuf::from("."),
            source_path,
            max_include_depth: 10,
        };
        let deck = parse(&mut parse_options).expect("parse");

        let sim_cfg = SimulationConfig {
            solver: LinearSolver::Blas,
            ..SimulationConfig::default()
        };

        let mut devices = Devices::from_spec(&deck.devices);
        let mut matrix =
            SolverMatrix::create_matrix(&mut devices, deck.node_mapping.clone(), &sim_cfg)
                .expect("Failed to create matrix");

        let previous_output = vec![0.0; matrix.rhs().len()];
        let mut integrator = Integrator::Trapezoidal {
            previous_output,
            previous_currents: HashMap::new(),
        };

        let config = TransientConfig {
            step: 1e-3,
            tstop: 1e-3,
            t: 0.0,
            use_device_ic: false,
        };

        let mut newton_state = NewtonState::new(sim_cfg.newton, NewtonMode::InitTrans);
        let (solution, _iters) = simulation_step(
            &mut matrix,
            &devices,
            &config,
            &mut integrator,
            &mut newton_state,
            config.step,
        )
        .expect("simulation_step");

        let cap = devices.capacitors.first().expect("expected capacitor");
        let pos = matrix.mna_node_index(cap.positive);
        let neg = matrix.mna_node_index(cap.negative);
        let g = 2.0 * cap.capacitance / config.step;
        let v_new = get_voltage_diff(&solution, pos, neg);
        let i_hist = 0.0;
        let expected_current = g * v_new + i_hist;

        let stored_current = match integrator {
            Integrator::Trapezoidal {
                previous_currents, ..
            } => *previous_currents
                .get(cap.name.as_str())
                .expect("expected saved capacitor current"),
            _ => unreachable!("expected trapezoidal integrator"),
        };

        assert!(
            v_new.abs() > 1e-9,
            "expected non-zero capacitor voltage after first step"
        );
        assert!(
            (stored_current - expected_current).abs() < 1e-9,
            "stored current should match capacitor current"
        );
        assert!(
            (stored_current - i_hist).abs() > 1e-9,
            "stored current should not equal history source"
        );
    }

    #[test]
    fn trans_inductor_backward_euler_one_step() {
        let netlist = "* RL with current source\n\
I1 n1 0 DC 1\n\
R1 n1 0 1\n\
L1 n1 0 1\n\
.END";

        let source_path = PathBuf::from("trans_inductor_be_one_step.spicy");
        let source_map = SourceMap::new(source_path.clone(), netlist.to_string());
        let mut parse_options = ParseOptions {
            source_map,
            work_dir: PathBuf::from("."),
            source_path,
            max_include_depth: 10,
        };
        let deck = parse(&mut parse_options).expect("parse");

        let sim_cfg = SimulationConfig {
            solver: LinearSolver::Blas,
            ..SimulationConfig::default()
        };

        let mut devices = Devices::from_spec(&deck.devices);
        let mut matrix =
            SolverMatrix::create_matrix(&mut devices, deck.node_mapping.clone(), &sim_cfg)
                .expect("Failed to create matrix");

        let previous = vec![0.0; matrix.rhs().len()];
        let mut integrator = Integrator::BackwardEuler { previous };

        let config = TransientConfig {
            step: 1.0,
            tstop: 1.0,
            t: 0.0,
            use_device_ic: false,
        };

        let mut newton_state = NewtonState::new(sim_cfg.newton, NewtonMode::InitTrans);
        let (solution, _iters) = simulation_step(
            &mut matrix,
            &devices,
            &config,
            &mut integrator,
            &mut newton_state,
            config.step,
        )
        .expect("simulation_step");

        let ind = devices.inductors.first().expect("expected inductor");
        let node = matrix
            .mna_node_index(ind.positive)
            .expect("expected inductor node");
        let branch = matrix.mna_branch_index(ind.current_branch);

        let v = solution[node];
        let i = solution[branch];

        let r_eq = ind.inductance / config.step;
        let i_expected = 1.0 / (1.0 + r_eq / 1.0);
        let v_expected = r_eq * i_expected;

        assert!(
            (i - i_expected).abs() < 1e-9,
            "inductor current mismatch: expected {}, got {}",
            i_expected,
            i
        );
        assert!(
            (v - v_expected).abs() < 1e-9,
            "node voltage mismatch: expected {}, got {}",
            v_expected,
            v
        );
    }

    #[test]
    fn trans_inductor_trapezoidal_one_step() {
        let netlist = "* RL with current source\n\
I1 n1 0 DC 1\n\
R1 n1 0 1\n\
L1 n1 0 1\n\
.END";

        let source_path = PathBuf::from("trans_inductor_trap_one_step.spicy");
        let source_map = SourceMap::new(source_path.clone(), netlist.to_string());
        let mut parse_options = ParseOptions {
            source_map,
            work_dir: PathBuf::from("."),
            source_path,
            max_include_depth: 10,
        };
        let deck = parse(&mut parse_options).expect("parse");

        let sim_cfg = SimulationConfig {
            solver: LinearSolver::Blas,
            ..SimulationConfig::default()
        };

        let mut devices = Devices::from_spec(&deck.devices);
        let mut matrix =
            SolverMatrix::create_matrix(&mut devices, deck.node_mapping.clone(), &sim_cfg)
                .expect("Failed to create matrix");

        let previous_output = vec![0.0; matrix.rhs().len()];
        let mut integrator = Integrator::Trapezoidal {
            previous_output,
            previous_currents: HashMap::new(),
        };

        let config = TransientConfig {
            step: 1.0,
            tstop: 1.0,
            t: 0.0,
            use_device_ic: false,
        };

        let mut newton_state = NewtonState::new(sim_cfg.newton, NewtonMode::InitTrans);
        let (solution, _iters) = simulation_step(
            &mut matrix,
            &devices,
            &config,
            &mut integrator,
            &mut newton_state,
            config.step,
        )
        .expect("simulation_step");

        let ind = devices.inductors.first().expect("expected inductor");
        let node = matrix
            .mna_node_index(ind.positive)
            .expect("expected inductor node");
        let branch = matrix.mna_branch_index(ind.current_branch);

        let v = solution[node];
        let i = solution[branch];

        let r_eq = 2.0 * ind.inductance / config.step;
        let i_expected = 1.0 / (1.0 + r_eq / 1.0);
        let v_expected = r_eq * i_expected;

        assert!(
            (i - i_expected).abs() < 1e-9,
            "inductor current mismatch: expected {}, got {}",
            i_expected,
            i
        );
        assert!(
            (v - v_expected).abs() < 1e-9,
            "node voltage mismatch: expected {}, got {}",
            v_expected,
            v
        );
    }
}
