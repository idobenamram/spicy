use spicy_parser::{
    Value, instance_parser::Deck, netlist_types::DcCommand, netlist_waveform::WaveForm,
};

use crate::{
    NewtonMode, NewtonState, SimulationConfig, devices::Devices, error::SimulationError,
    matrix::SolverMatrix, trans::newton_solve,
};

#[derive(Debug)]
pub struct OperatingPointResult {
    pub voltages: Vec<(String, f64)>,
    pub currents: Vec<(String, f64)>,
}

#[derive(Debug)]
pub struct DcSweepResult {
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
        v.stamp_voltage_source_dc(matrix);
    }

    for c in &devices.current_sources {
        c.stamp_current_source_dc(matrix);
    }

    Ok(())
}

#[derive(Clone, Copy)]
enum SweepTarget {
    Voltage(usize),
    Current(usize),
}

fn find_sweep_target(devices: &Devices, srcnam: &str) -> SweepTarget {
    if let Some((idx, _)) = devices
        .voltage_sources
        .iter()
        .enumerate()
        .find(|(_, v)| v.name == srcnam)
    {
        return SweepTarget::Voltage(idx);
    }
    if let Some((idx, _)) = devices
        .current_sources
        .iter()
        .enumerate()
        .find(|(_, i)| i.name == srcnam)
    {
        return SweepTarget::Current(idx);
    }

    panic!("Source '{srcnam}' not found (expected a V or I source)");
}

fn set_sweep_value(devices: &mut Devices, target: SweepTarget, value: f64) {
    let waveform = WaveForm::Constant(Value::new(value, None, None));
    match target {
        SweepTarget::Voltage(index) => devices.voltage_sources[index].dc = waveform,
        SweepTarget::Current(index) => devices.current_sources[index].dc = waveform,
    }
}

pub(crate) fn simulate_op_inner(
    m: &mut SolverMatrix,
    devices: &Devices,
    state: &mut NewtonState,
) -> Result<(), SimulationError> {
    let initial_guess = vec![0.0; m.rhs().len()];
    let _ = newton_solve(
        m,
        state,
        initial_guess,
        None,
        |matrix, guess| stamp_dc(matrix, devices, guess),
    )?;

    Ok(())
}

pub fn simulate_op(
    deck: &Deck,
    sim_config: &SimulationConfig,
) -> Result<OperatingPointResult, SimulationError> {
    let mut devices = Devices::from_spec(&deck.devices);

    let mut matrix =
        SolverMatrix::create_matrix(&mut devices, deck.node_mapping.clone(), sim_config)?;

    let mut state = NewtonState::new(sim_config.newton, NewtonMode::InitOp);
    simulate_op_inner(&mut matrix, &devices, &mut state)?;

    let x = matrix.rhs();
    let node_names = deck.node_mapping.node_names_mna_order();
    let branch_names = deck.node_mapping.branch_names_mna_order();
    let n = node_names.len();

    let mut voltages = Vec::with_capacity(n);
    for (i, name) in node_names.into_iter().enumerate() {
        voltages.push((name, x[i]));
    }

    let mut currents = Vec::with_capacity(branch_names.len());
    for (i, name) in branch_names.into_iter().enumerate() {
        currents.push((name, x[n + i]));
    }

    Ok(OperatingPointResult { voltages, currents })
}

fn sweep(vstart: f64, vstop: f64, vinc: f64) -> Vec<f64> {
    let nsteps = ((vstop - vstart) / vinc).floor() as usize;
    (0..=nsteps).map(|i| vstart + i as f64 * vinc).collect()
}

pub fn simulate_dc(
    deck: &Deck,
    command: &DcCommand,
    sim_config: &SimulationConfig,
) -> DcSweepResult {
    let srcnam = &command.srcnam;
    let vstart = command.vstart.get_value();
    let vstop = command.vstop.get_value();
    let vincr = command.vincr.get_value();

    let mut devices = Devices::from_spec(&deck.devices);

    // Matrix pattern setup stores nnz indices into the compiled devices.
    let mut matrix =
        SolverMatrix::create_matrix(&mut devices, deck.node_mapping.clone(), sim_config)
            .expect("Failed to create matrix");

    let sweep_target = find_sweep_target(&devices, srcnam);
    let sweep_values = sweep(vstart, vstop, vincr);
    let node_names = deck.node_mapping.node_names_mna_order();
    let branch_names = deck.node_mapping.branch_names_mna_order();
    let n = node_names.len();

    let mut results = Vec::new();
    let mut guess = vec![0.0; matrix.rhs().len()];
    for v in sweep_values {
        set_sweep_value(&mut devices, sweep_target, v);
        let mut state = NewtonState::new(sim_config.newton, NewtonMode::InitOp);
        let (solution, _iters) = newton_solve(
            &mut matrix,
            &mut state,
            guess,
            None,
            |matrix, guess| stamp_dc(matrix, &devices, guess),
        )
        .expect("simulate_dc newton solve");

        let mut voltages = Vec::with_capacity(node_names.len());
        let mut currents = Vec::with_capacity(branch_names.len());
        for (i, name) in node_names.iter().enumerate() {
            voltages.push((name.clone(), solution[i]));
        }
        for (i, name) in branch_names.iter().enumerate() {
            currents.push((name.clone(), solution[n + i]));
        }

        results.push((OperatingPointResult { voltages, currents }, v));
        guess = solution;
    }

    DcSweepResult { results }
}
