use spicy_parser::{instance_parser::Deck, netlist_types::DcCommand};

use crate::{SimulationConfig, devices::Devices, error::SimulationError, matrix::SolverMatrix};

#[derive(Debug)]
pub struct OperatingPointResult {
    pub voltages: Vec<(String, f64)>,
    pub currents: Vec<(String, f64)>,
}

#[derive(Debug)]
pub struct DcSweepResult {
    pub results: Vec<(OperatingPointResult, f64)>,
}

pub(crate) fn simulate_op_inner(
    m: &mut SolverMatrix,
    devices: &Devices,
) -> Result<(), SimulationError> {
    for r in &devices.resistors {
        r.stamp_dc(m);
    }
    // capcitors are just open circuits in dc

    for i in &devices.inductors {
        i.stamp_dc(m);
    }

    for v in &devices.voltage_sources {
        v.stamp_voltage_source_dc(m);
    }

    for c in &devices.current_sources {
        c.stamp_current_source_dc(m);
    }

    // KLU requires an analyze phase before factorization.
    m.analyze()?;
    m.factorize()?;
    // [V] node voltages
    // [I] branch currents for voltage sources (also inductors)
    m.solve()?;

    Ok(())
}

pub fn simulate_op(deck: &Deck, sim_config: &SimulationConfig) -> OperatingPointResult {
    let mut devices = Devices::from_spec(&deck.devices);

    let mut matrix =
        SolverMatrix::create_matrix(&mut devices, deck.node_mapping.clone(), sim_config)
            .expect("Failed to create matrix");

    simulate_op_inner(&mut matrix, &devices).expect("Failed to simulate OP");

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

    OperatingPointResult { voltages, currents }
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

    // Stamp matrix entries and baseline RHS (excluding the swept source).
    for r in &devices.resistors {
        r.stamp_dc(&mut matrix);
    }
    // capacitors are open circuits in DC
    for l in &devices.inductors {
        l.stamp_dc(&mut matrix);
    }
    for v in &devices.voltage_sources {
        v.stamp_voltage_incidence(&mut matrix);
        if v.name != *srcnam {
            v.stamp_voltage_value_dc(&mut matrix);
        }
    }
    for c in &devices.current_sources {
        if c.name != *srcnam {
            c.stamp_current_source_dc(&mut matrix);
        }
    }

    matrix.analyze().expect("Failed to analyze matrix");
    matrix.factorize().expect("Failed to factorize matrix");

    // Baseline RHS; will be overwritten in-place by `solve()`.
    let rhs_base = matrix.rhs().to_vec();

    // TODO: this could all be a little prettier & maybe we can run the solve in batches.
    // Determine how to apply the swept value.
    #[derive(Clone, Copy)]
    enum Sweep {
        Voltage {
            rhs_index: usize,
        },
        Current {
            pos: Option<usize>,
            neg: Option<usize>,
        },
    }

    let sweep_spec = if let Some(vsrc) = devices.voltage_sources.iter().find(|v| v.name == *srcnam)
    {
        Sweep::Voltage {
            rhs_index: matrix.mna_branch_index(vsrc.current_branch),
        }
    } else if let Some(isrc) = devices.current_sources.iter().find(|i| i.name == *srcnam) {
        Sweep::Current {
            pos: matrix.mna_node_index(isrc.positive),
            neg: matrix.mna_node_index(isrc.negative),
        }
    } else {
        panic!("Source '{srcnam}' not found (expected a V or I source)");
    };

    let sweep_values = sweep(vstart, vstop, vincr);

    let node_names = deck.node_mapping.node_names_mna_order();
    let branch_names = deck.node_mapping.branch_names_mna_order();
    let n = node_names.len();

    let mut results = Vec::new();
    for v in sweep_values {
        // Restore baseline RHS.
        matrix.rhs_mut().copy_from_slice(&rhs_base);

        // Apply swept value to RHS.
        match sweep_spec {
            Sweep::Voltage { rhs_index } => {
                *matrix.get_mut_rhs(rhs_index) = v;
            }
            Sweep::Current { pos, neg } => {
                if let Some(p) = pos {
                    *matrix.get_mut_rhs(p) += v;
                }
                if let Some(nidx) = neg {
                    *matrix.get_mut_rhs(nidx) -= v;
                }
            }
        }

        matrix.solve().expect("Failed to solve linear system");
        let x = matrix.rhs();

        let mut voltages = Vec::with_capacity(node_names.len());
        let mut currents = Vec::with_capacity(branch_names.len());
        for (i, name) in node_names.iter().enumerate() {
            voltages.push((name.clone(), x[i]));
        }
        for (i, name) in branch_names.iter().enumerate() {
            currents.push((name.clone(), x[n + i]));
        }

        results.push((OperatingPointResult { voltages, currents }, v));
    }

    DcSweepResult { results }
}
