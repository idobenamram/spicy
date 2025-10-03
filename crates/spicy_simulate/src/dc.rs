use crate::nodes::Nodes;
use ndarray::{Array1, Array2, s};
use ndarray_linalg::{FactorizeInto, Solve};
use spicy_parser::{
    netlist_types::{DcCommand, Device, IndependentSource, Inductor, Resistor}, netlist_waveform::WaveForm, parser::Deck, Value
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

pub(crate) fn stamp_resistor(g: &mut Array2<f64>, resistor: &Resistor, nodes: &Nodes) {
    let node1 = nodes.get_node_index(&resistor.positive.name);
    let node2 = nodes.get_node_index(&resistor.negative.name);

    let conductance = 1.0 / resistor.resistance.get_value();

    if let Some(node1) = node1 {
        g[[node1, node1]] += conductance;
    }
    if let Some(node2) = node2 {
        g[[node2, node2]] += conductance;
    }
    if let Some(node1) = node1
        && let Some(node2) = node2
    {
        g[[node1, node2]] -= conductance;
        g[[node2, node1]] -= conductance;
    }
}

fn stamp_current_source(s: &mut Array1<f64>, device: &IndependentSource, nodes: &Nodes) {
    let node1 = nodes.get_node_index(&device.positive.name);
    let node2 = nodes.get_node_index(&device.negative.name);
    let value = match &device.dc {
        Some(value) => value.compute(0.0, 0.0, 0.0),
        None => 0.0,
    };

    if let Some(node1) = node1 {
        s[node1] += value;
    }
    if let Some(node2) = node2 {
        s[node2] -= value;
    }
}

fn stamp_voltage_source_incidence(m: &mut Array2<f64>, device: &IndependentSource, nodes: &Nodes) {
    let node1 = nodes.get_node_index(&device.positive.name);
    let node2 = nodes.get_node_index(&device.negative.name);
    let src_index = nodes
        .get_voltage_source_index(&device.name)
        .expect("should exist");

    // stamp in voltage incidence matrix (B)
    if let Some(node1) = node1 {
        m[[node1, src_index]] = 1.0;
    }
    if let Some(node2) = node2 {
        m[[node2, src_index]] = -1.0;
    }

    // stamp in voltage incidence matrix (B^T)
    if let Some(node1) = node1 {
        m[[src_index, node1]] = 1.0;
    }
    if let Some(node2) = node2 {
        m[[src_index, node2]] = -1.0;
    }
}

fn stamp_voltage_source_value(s: &mut Array1<f64>, device: &IndependentSource, nodes: &Nodes) {
    let src_index = nodes
        .get_voltage_source_index(&device.name)
        .expect("should exist");

    let value = match &device.dc {
        Some(value) => value.compute(0.0, 0.0, 0.0),
        None => 0.0,
    };
    s[src_index] = value;
}

pub(crate) fn stamp_voltage_source(
    m: &mut Array2<f64>,
    s: &mut Array1<f64>,
    device: &IndependentSource,
    nodes: &Nodes,
) {
    stamp_voltage_source_incidence(m, device, nodes);
    stamp_voltage_source_value(s, device, nodes);
}

fn stamp_inductor(m: &mut Array2<f64>, s: &mut Array1<f64>, device: &Inductor, nodes: &Nodes) {
    let node1 = nodes.get_node_index(&device.positive.name);
    let node2 = nodes.get_node_index(&device.negative.name);
    let src_index = nodes
        .get_voltage_source_index(&device.name)
        .expect("should exist");

    // stamp in voltage incidence matrix (B)
    if let Some(node1) = node1 {
        m[[node1, src_index]] = 1.0;
    }
    if let Some(node2) = node2 {
        m[[node2, src_index]] = -1.0;
    }

    // stamp in voltage incidence matrix (B^T)
    if let Some(node1) = node1 {
        m[[src_index, node1]] = 1.0;
    }
    if let Some(node2) = node2 {
        m[[src_index, node2]] = -1.0;
    }

    // stamp in voltage source vector (E)
    s[src_index] = 0.0;
}

pub(crate) fn simulate_op_inner(nodes: &Nodes, devices: &Vec<Device>) -> Array1<f64> {
    let n = nodes.node_len();
    let k = nodes.source_len();
    // Modified nodal analysis matrix
    // [G, B]
    // [B^T, 0]
    // conductance matrix (n) + incidence of each voltage-defined element (k)
    let mut m = Array2::<f64>::zeros((n + k, n + k));
    // [I] current vector
    // [E] source voltages
    // current and voltage source vectors
    let mut s = Array1::<f64>::zeros(n + k);

    for device in devices {
        match device {
            Device::Resistor(device) => stamp_resistor(&mut m, &device, &nodes),
            Device::Capacitor(_) => {} // capcitors are just open circuits
            Device::Inductor(device) => stamp_inductor(&mut m, &mut s, &device, &nodes),
            Device::CurrentSource(device) => stamp_current_source(&mut s, &device, &nodes),
            Device::VoltageSource(device) => stamp_voltage_source(&mut m, &mut s, &device, &nodes),
        }
    }

    // // println!("m: {:?}", m);
    // println!("s: {:?}", s);
    let lu = m.factorize_into().expect("Failed to factorize matrix");
    // [V] node voltages
    // [I] branch currents for voltage sources (also inductors)
    let x = lu.solve(&s).expect("Failed to solve linear system");

    x
}

pub fn simulate_op(deck: &Deck) -> OperatingPointResult {
    let nodes = Nodes::new(&deck.devices);
    let n = nodes.node_len();

    let x = simulate_op_inner(&nodes, &deck.devices);

    let mut voltages = Vec::new();
    let mut currents = Vec::new();
    let node_names = nodes.get_node_names();
    for (i, voltage) in x.slice(s![..n]).iter().enumerate() {
        let name = &node_names[i];
        voltages.push((name.to_string(), *voltage));
    }

    let source_names = nodes.get_source_names();
    for (i, current) in x.slice(s![n..]).iter().enumerate() {
        let name = &source_names[i];
        currents.push((name.to_string(), *current));
    }

    OperatingPointResult { voltages, currents }
}

fn sweep(vstart: f64, vstop: f64, vinc: f64) -> Vec<f64> {
    let nsteps = ((vstop - vstart) / vinc).floor() as usize;
    (0..=nsteps).map(|i| vstart + i as f64 * vinc).collect()
}

pub fn simulate_dc(deck: &Deck, command: &DcCommand) -> DcSweepResult {
    let srcnam = &command.srcnam;
    let vstart = command.vstart.get_value();
    let vstop = command.vstop.get_value();
    let vincr = command.vincr.get_value();

    let nodes = Nodes::new(&deck.devices);

    let n = nodes.node_len();
    let k = nodes.source_len();

    let mut m = Array2::<f64>::zeros((n + k, n + k));
    let mut s_before = Array1::<f64>::zeros(n + k);

    // println!("srcnam: {:?}", srcnam);
    let source_index = deck
        .devices
        .iter()
        .position(|d| d.name() == srcnam)
        .expect("Source not found");
    for device in &deck.devices {
        match device {
            Device::Resistor(device) => stamp_resistor(&mut m, &device, &nodes),
            Device::Capacitor(_) => {} // capcitors are just open circuits
            Device::Inductor(device) => stamp_inductor(&mut m, &mut s_before, &device, &nodes),
            Device::VoltageSource(device) => {
                stamp_voltage_source_incidence(&mut m, &device, &nodes);
            }
            Device::CurrentSource(device) => {
                if device.name != *srcnam {
                    stamp_current_source(&mut s_before, &device, &nodes);
                }
            }
        }
    }

    let lu = m.factorize_into().expect("Failed to factorize matrix");

    let sweep_values = sweep(vstart, vstop, vincr);

    let mut results = Vec::new();
    for v in sweep_values {
        let mut s = s_before.clone();
        let device = deck.devices[source_index].clone();
        // TODO: this sucks
        let value = WaveForm::Constant(Value::new(v, None, None));
        match device {
            Device::VoltageSource(mut device) => {
                device.dc = Some(value);
                stamp_voltage_source_value(&mut s, &device, &nodes);
            }
            Device::CurrentSource(mut device) => {
                device.dc = Some(value);
                stamp_current_source(&mut s, &device, &nodes);
            }
            _ => {}
        }
        let x = lu.solve(&s).expect("Failed to solve linear system");

        let node_names = nodes.get_node_names();
        let mut voltages = Vec::new();
        let mut currents = Vec::new();
        for (index, voltage) in x.slice(s![..n]).iter().enumerate() {
            let name = &node_names[index];
            voltages.push((name.to_string(), *voltage));
            // println!("{}: {:.6}V", name, voltage);
        }

        let source_names = nodes.get_source_names();
        for (i, current) in x.slice(s![n..]).iter().enumerate() {
            let name = &source_names[i];
            currents.push((name.to_string(), *current));
            // println!("{}: {:.6}A", name, current);
        }
        results.push((OperatingPointResult { voltages, currents }, v));
    }

    DcSweepResult { results }
}
