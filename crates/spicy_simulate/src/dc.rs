use spicy_circuit::{Circuit, DcSweep, Params, SourceRef, Waveform};

use crate::{
    NewtonMode, NewtonState, SimulationConfig, devices::Devices, error::SimulationError,
    matrix::SolverMatrix, trans::newton_solve, unknowns::Layout,
};

#[derive(Debug)]
pub struct OperatingPointResult {
    /// One value per unknown, in the order of [`crate::unknowns`].
    pub solution: Vec<f64>,
}

#[derive(Debug)]
pub struct DcSweepResult {
    /// The operating point at each value of the swept source.
    pub results: Vec<(OperatingPointResult, f64)>,
}

fn stamp_dc(
    matrix: &mut SolverMatrix,
    devices: &Devices,
    guess: &[f64],
) -> Result<(), SimulationError> {
    for r in &devices.resistors {
        r.stamp_dc(matrix);
    }
    // capcitors are just open circuits in dc

    for d in &devices.diodes {
        d.stamp_nonlinear(matrix, guess);
    }

    for bjt in &devices.bjts {
        bjt.stamp_nonlinear(matrix, guess);
    }

    for i in &devices.inductors {
        i.stamp_dc(matrix);
    }

    for v in &devices.voltage_sources {
        v.stamp_dc(matrix);
    }

    for c in &devices.current_sources {
        c.stamp_dc(matrix);
    }

    Ok(())
}

fn set_sweep_value(devices: &mut Devices, source: SourceRef, value: f64) {
    let waveform = Waveform::Dc(value);
    match source {
        SourceRef::Voltage(id) => devices.voltage_sources[id.index()].waveform = waveform,
        SourceRef::Current(id) => devices.current_sources[id.index()].waveform = waveform,
    }
}

pub(crate) fn simulate_op_inner(
    m: &mut SolverMatrix,
    devices: &Devices,
    state: &mut NewtonState,
) -> Result<(), SimulationError> {
    let initial_guess = vec![0.0; m.rhs().len()];
    let _ = newton_solve(m, state, initial_guess, None, |matrix, guess| {
        stamp_dc(matrix, devices, guess)
    })?;

    Ok(())
}

pub fn simulate_op(
    circuit: &Circuit,
    params: &Params,
    sim_config: &SimulationConfig,
) -> Result<OperatingPointResult, SimulationError> {
    let layout = Layout::new(circuit);
    let mut devices = Devices::new(circuit, params, &layout);

    let mut matrix = SolverMatrix::create_matrix(&mut devices, layout.dim(), sim_config)?;

    let mut state = NewtonState::new(sim_config.newton, NewtonMode::InitOp);
    simulate_op_inner(&mut matrix, &devices, &mut state)?;

    Ok(OperatingPointResult {
        solution: matrix.rhs().to_vec(),
    })
}

fn sweep(vstart: f64, vstop: f64, vinc: f64) -> Vec<f64> {
    let nsteps = ((vstop - vstart) / vinc).floor() as usize;
    (0..=nsteps).map(|i| vstart + i as f64 * vinc).collect()
}

pub fn simulate_dc(
    circuit: &Circuit,
    params: &Params,
    dc: &DcSweep,
    sim_config: &SimulationConfig,
) -> DcSweepResult {
    let layout = Layout::new(circuit);
    let mut devices = Devices::new(circuit, params, &layout);

    // Matrix pattern setup stores nnz indices into the compiled devices.
    let mut matrix = SolverMatrix::create_matrix(&mut devices, layout.dim(), sim_config)
        .expect("Failed to create matrix");

    let mut results = Vec::new();
    let mut guess = vec![0.0; matrix.rhs().len()];
    for v in sweep(dc.start, dc.stop, dc.step) {
        set_sweep_value(&mut devices, dc.source, v);
        let mut state = NewtonState::new(sim_config.newton, NewtonMode::InitOp);
        let (solution, _iters) =
            newton_solve(&mut matrix, &mut state, guess, None, |matrix, guess| {
                stamp_dc(matrix, &devices, guess)
            })
            .expect("simulate_dc newton solve");

        results.push((
            OperatingPointResult {
                solution: solution.clone(),
            },
            v,
        ));
        guess = solution;
    }

    DcSweepResult { results }
}
