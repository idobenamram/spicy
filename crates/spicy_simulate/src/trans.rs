use std::collections::HashMap;

use spicy_parser::{instance_parser::Deck, netlist_types::TranCommand};

use crate::{
    dc::{simulate_op_inner, stamp_resistor, stamp_voltage_source_incidence},
    devices::{Capacitor, Devices, IndependentSource},
    matrix::SolverMatrix,
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
        // TODO: breh this trash
        match (positive, negative) {
            (Some(positive), Some(negative)) => {
                previous_voltages[positive] - previous_voltages[negative]
            }
            (Some(positive), None) => previous_voltages[positive],
            (None, Some(negative)) => -previous_voltages[negative],
            (None, None) => 0.0, // TODO: should through an error
        }
    }
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

fn stamp_capacitor_trans(m: &mut SolverMatrix, device: &Capacitor, g: f64, i: f64) {
    if let Some(index) = device.stamp.pos_pos {
        *m.get_mut_nnz(index) += g;
    }
    if let Some(index) = device.stamp.neg_neg {
        *m.get_mut_nnz(index) += g;
    }
    if let Some((pos_neg, neg_pos)) = device.stamp.off_diagonals {
        *m.get_mut_nnz(pos_neg) -= g;
        *m.get_mut_nnz(neg_pos) -= g;
    }

    let pos = m.mna_node_index(device.positive);
    let neg = m.mna_node_index(device.negative);

    if let Some(p) = pos {
        *m.get_mut_rhs(p) += i;
    }
    if let Some(n) = neg {
        *m.get_mut_rhs(n) -= i;
    }
}


fn stamp_voltage_source_trans(
    m: &mut SolverMatrix,
    device: &IndependentSource,
    config: &TransientConfig,
) {
    stamp_voltage_source_incidence(m, device);
    let src_index = m.mna_branch_index(device.current_branch);
    let value = device.dc.compute(config.t, config.step, config.tstop);
    *m.get_mut_rhs(src_index) = value;
}

fn simulation_step<'a>(
    matrix: &mut SolverMatrix,
    devices: &'a Devices,
    config: &TransientConfig,
    integrator: &mut Integrator<'a>,
) -> Vec<f64> {

    for r in &devices.resistors {
        stamp_resistor(matrix, r);
    }

    for c in &devices.capacitors {
        let pos = matrix.mna_node_index(c.positive);
        let neg = matrix.mna_node_index(c.negative);

        let (g, i) = integrator.capacitor_values(c, pos, neg, config);
        stamp_capacitor_trans(matrix, c, g, i);
        integrator.save_capacitor_current(&c, i);
    }

    // TODO: we don't support functions on the sources yet
    for vsrc in &devices.voltage_sources {
        stamp_voltage_source_trans(matrix, vsrc, config);
    }

    // TODO: stamp current sources

    // Solve.
    // TODO: don't analyze each step
    matrix.analyze().expect("Failed to analyze matrix");
    matrix.factorize().expect("Failed to factorize matrix");
    matrix.solve().expect("Failed to solve linear system");

    matrix.rhs().to_vec()
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
}

pub fn simulate_trans(deck: &Deck, cmd: &TranCommand) -> TransientResult {
    let tstep = cmd.tstep.get_value();
    let tstop = cmd.tstop.get_value();

    let mut devices = Devices::from_spec(&deck.devices);
    if !devices.inductors.is_empty() {
        unimplemented!("Transient analysis does not yet support inductors");
    }

    let mut matrix = SolverMatrix::create_matrix(&mut devices, deck.node_mapping.clone(), true)
        .expect("Failed to create matrix");

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
        simulate_op_inner(&mut matrix, &devices).expect("Failed to simulate OP");
        matrix.rhs().to_vec()
    };

    let mut integrator = Integrator::BackwardEuler {
        previous: initial_condition,
    };

    let mut times: Vec<f64> = Vec::new();
    let mut samples: Vec<Vec<f64>> = Vec::new();

    // initial sample at t=0 using current state (before any transient step)
    // note this means that for UIC even the the voltage source nodes will have a value of 0 at t=0
    times.push(0.0);
    samples.push(integrator.get_previous_output().to_vec());

    let steps = steps(config.step, tstop);
    for step in steps.into_iter().skip(1) {
        let x = simulation_step(&mut matrix, &devices, &config, &mut integrator);

        integrator.save_previous_voltage(x.clone());
        config.use_device_ic = false;
        config.t = step;

        times.push(step);
        samples.push(x.to_vec());
    }

    TransientResult {
        times,
        node_names: deck.node_mapping.node_names_mna_order(),
        source_names: deck.node_mapping.branch_names_mna_order(),
        samples,
    }
}
