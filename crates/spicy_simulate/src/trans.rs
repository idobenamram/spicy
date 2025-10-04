use std::collections::HashMap;

use ndarray::{Array1, Array2};
use ndarray_linalg::{FactorizeInto, Solve};
use spicy_parser::{
    Value,
    netlist_types::{Capacitor, Device, IndependentSource, TranCommand},
    parser::Deck,
};

use crate::{
    dc::{simulate_op_inner, stamp_resistor, stamp_voltage_source_incidence},
    nodes::Nodes,
};

fn steps(dt: f64, tstop: f64) -> Vec<f64> {
    let nsteps = (tstop / dt).floor() as usize;
    (0..=nsteps).map(|i| i as f64 * dt).collect()
}

fn get_previous_voltage(
    previous_voltages: &Array1<f64>,
    positive: Option<usize>,
    negative: Option<usize>,
    ic: &Option<Value>,
    use_device_ic: bool,
) -> f64 {
    let previous_voltage = if use_device_ic {
        // if we set the uic flag we should just take the initial condition from the device
        ic.clone().unwrap_or(Value::zero()).get_value()
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
    };

    previous_voltage
}

#[derive(Debug)]
pub enum Integrator<'a> {
    BackwardEuler {
        previous: Array1<f64>,
    },
    Trapezoidal {
        previous_output: Array1<f64>,
        previous_currents: HashMap<&'a str, f64>,
    },
}

impl<'a> Integrator<'a> {
    fn capcitor_values(
        &self,
        device: &Capacitor,
        positive: Option<usize>,
        negative: Option<usize>,
        config: &TransientConfig,
    ) -> (f64, f64) {
        match self {
            Integrator::BackwardEuler { previous } => {
                let c = device.capacitance.get_value();
                let g = c / config.step;
                let previous_voltage = get_previous_voltage(
                    previous,
                    positive,
                    negative,
                    &device.ic,
                    config.use_device_ic,
                );
                let i = g * previous_voltage;
                (g, i)
            }
            Integrator::Trapezoidal {
                previous_output,
                previous_currents,
            } => {
                let c = device.capacitance.get_value();
                let g = 2.0 * c / config.step;
                let previous_voltage = get_previous_voltage(
                    previous_output,
                    positive,
                    negative,
                    &device.ic,
                    config.use_device_ic,
                );
                let previous_current = previous_currents.get(device.name.as_str()).unwrap_or(&0.0);
                let i = -g * previous_voltage - previous_current;
                (g, i)
            }
        }
    }

    fn save_capcitor_current(&mut self, device: &'a Capacitor, current: f64) {
        match self {
            Integrator::BackwardEuler { previous: _ } => {}
            Integrator::Trapezoidal {
                previous_currents, ..
            } => {
                // TODO: i hate this clone
                previous_currents.insert(device.name.as_str(), current);
            }
        }
    }

    fn save_previous_voltage(&mut self, voltage: Array1<f64>) {
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

    fn get_previous_output(&self) -> &Array1<f64> {
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

fn stamp_capacitor_trans(
    m: &mut Array2<f64>,
    s: &mut Array1<f64>,
    positive: Option<usize>,
    negative: Option<usize>,
    g: f64,
    i: f64,
) {
    if let Some(p) = positive {
        m[[p, p]] += g;
    }
    if let Some(n) = negative {
        m[[n, n]] += g;
    }
    if let (Some(p), Some(n)) = (positive, negative) {
        m[[p, n]] -= g;
        m[[n, p]] -= g;
    }

    if let Some(p) = positive {
        s[p] += i;
    }
    if let Some(n) = negative {
        s[n] -= i;
    }
}

fn stamp_voltage_source_trans(
    m: &mut Array2<f64>,
    s: &mut Array1<f64>,
    device: &IndependentSource,
    nodes: &Nodes,
    config: &TransientConfig,
) {
    stamp_voltage_source_incidence(m, device, nodes);
    let src_index = nodes
        .get_voltage_source_index(&device.name)
        .expect("should exist");

    let value = match &device.dc {
        Some(value) => value.compute(config.t, config.step, config.tstop),
        None => 0.0,
    };
    s[src_index] = value;
}

fn simulation_step<'a>(
    nodes: &Nodes,
    devices: &'a Vec<Device>,
    config: &TransientConfig,
    integrator: &mut Integrator<'a>,
) -> Array1<f64> {
    let n = nodes.node_len();
    let k = nodes.source_len();

    let mut m = Array2::<f64>::zeros((n + k, n + k));
    let mut s = Array1::<f64>::zeros(n + k);

    for device in devices {
        match device {
            Device::Resistor(device) => stamp_resistor(&mut m, &device, &nodes),
            Device::Capacitor(device) => {
                let positive = nodes.get_node_index(&device.positive.name);
                let negative = nodes.get_node_index(&device.negative.name);

                let (g, i) = integrator.capcitor_values(device, positive, negative, config);
                stamp_capacitor_trans(&mut m, &mut s, positive, negative, g, i);
                integrator.save_capcitor_current(device, i);
            }
            // TODO: we don't support functions on the sources yet
            Device::VoltageSource(device) => stamp_voltage_source_trans(&mut m, &mut s, &device, &nodes, config),
            _ => {
                unimplemented!("Unsupported device type: {:?}", device)
            }
        }
    }

    let lu = m.factorize_into().expect("Failed to factorize matrix");
    let x = lu.solve(&s).expect("Failed to solve linear system");

    x
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

    let nodes = Nodes::new(&deck.devices);
    let n = nodes.node_len();
    let k = nodes.source_len();

    let initial_condition = match cmd.uic {
        // when there is no inital conditions we just the operating point as the inital condition
        false => simulate_op_inner(&nodes, &deck.devices),
        true => Array1::<f64>::zeros(n + k),
    };

    let mut config = TransientConfig {
        // TODO: this is not really correct but ok for now, tstep doesn't have to be the step size
        step: tstep,
        tstop: tstop,
        t: 0.0,
        use_device_ic: cmd.uic,
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
        let x = simulation_step(&nodes, &deck.devices, &config, &mut integrator);

        integrator.save_previous_voltage(x.clone());
        config.use_device_ic = false;
        config.t = step;

        times.push(step);
        samples.push(x.to_vec());
    }

    TransientResult {
        times,
        node_names: nodes.get_node_names(),
        source_names: nodes.get_source_names(),
        samples,
    }
}
