use ndarray::{Array1, Array2};
use ndarray_linalg::{FactorizeInto, Solve};
use spicy_parser::{
    Value,
    netlist_types::{Capacitor, Device, TranCommand},
    parser::Deck,
};

use std::fs::File;
use std::io::Write;

use crate::{
    dc::{simulate_op_inner, stamp_resistor, stamp_voltage_source},
    nodes::Nodes,
};

fn steps(dt: f64, tstop: f64) -> Vec<f64> {
    let nsteps = (tstop / dt).floor() as usize;
    (0..=nsteps).map(|i| i as f64 * dt).collect()
}

#[derive(Debug)]
struct TransientState {
    /// the increment time
    step: f64,
    /// should only be valid when uic is true and we are on the first iteration
    use_device_ic: bool,
    /// the previous output state
    previous: Array1<f64>,
}

fn stamp_capacitor_trans(
    m: &mut Array2<f64>,
    s: &mut Array1<f64>,
    device: &Capacitor,
    state: &TransientState,
    nodes: &Nodes,
) {
    let c = device.capacitance.get_value();
    let g = c / state.step;
    let positive = nodes.get_node_index(&device.positive.name);
    let negative = nodes.get_node_index(&device.negative.name);
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

    let previous_voltage = if state.use_device_ic {
        // if we set the uic flag we should just take the initial condition from the device
        device.ic.clone().unwrap_or(Value::zero()).get_value()
    } else {
        // TODO: breh this trash
        match (positive, negative) {
            (Some(positive), Some(negative)) => state.previous[positive] - state.previous[negative],
            (Some(positive), None) => state.previous[positive],
            (None, Some(negative)) => -state.previous[negative],
            (None, None) => 0.0, // TODO: should through an error
        }
    };

    let i = g * previous_voltage;
    if let Some(p) = positive {
        s[p] += i;
    }
    if let Some(n) = negative {
        s[n] -= i;
    }
}

fn simulation_step(nodes: &Nodes, devices: &Vec<Device>, state: &TransientState) -> Array1<f64> {
    let n = nodes.node_len();
    let k = nodes.source_len();

    let mut m = Array2::<f64>::zeros((n + k, n + k));
    let mut s = Array1::<f64>::zeros(n + k);

    for device in devices {
        match device {
            Device::Resistor(device) => stamp_resistor(&mut m, &device, &nodes),
            Device::Capacitor(device) => {
                stamp_capacitor_trans(&mut m, &mut s, &device, state, &nodes)
            }
            // TODO: we don't support functions on the sources yet
            Device::VoltageSource(device) => stamp_voltage_source(&mut m, &mut s, &device, &nodes),
            _ => {
                unimplemented!("Unsupported device type: {:?}", device)
            }
        }
    }

    let lu = m.factorize_into().expect("Failed to factorize matrix");
    let x = lu.solve(&s).expect("Failed to solve linear system");

    x
}

pub fn simulate_trans(deck: &Deck, cmd: &TranCommand) -> Vec<(f64, Array1<f64>)> {
    // TODO: this is not really correct but ok for now
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

    let mut state = TransientState {
        step: tstep,
        use_device_ic: cmd.uic,
        previous: initial_condition,
    };

    let mut results = Vec::new();

    // CSV output: time and node voltages (first n entries in solution)
    let mut csv = File::create("transient.csv").expect("failed to create transient.csv");
    let n = nodes.node_len();
    // header
    write!(csv, "time").unwrap();
    let node_names = nodes.get_node_names();
    for name in node_names.iter() {
        write!(csv, ",{}", name).unwrap();
    }
    writeln!(csv).unwrap();

    // initial sample at t=0 using current state (before any transient step)
    // note this means that for UIC even the the voltage source nodes will have a value of 0 at t=0
    write!(csv, "{}", 0.0).unwrap();
    for i in 0..n {
        write!(csv, ",{}", state.previous[i]).unwrap();
    }
    writeln!(csv).unwrap();


    let steps = steps(state.step, tstop);
    for step in steps.into_iter().skip(1) {
        let x = simulation_step(&nodes, &deck.devices, &state);

        // write CSV row: time, v[0..n)
        write!(csv, "{}", step).unwrap();
        for i in 0..n {
            write!(csv, ",{}", x[i]).unwrap();
        }
        writeln!(csv).unwrap();

        state.previous = x.clone();
        state.use_device_ic = false;
        results.push((step, x));
    }

    results
}
