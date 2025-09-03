use std::collections::HashMap;

use ndarray::{Array1, Array2, s};
use ndarray_linalg::{FactorizeInto, Solve};
use spicy_parser::netlist_types::{Command, Device};
use spicy_parser::netlist_types::{
    DcCommand, IndependentSource, IndependentSourceMode, Inductor, Resistor,
};
use spicy_parser::parser::Deck;
use spicy_parser::Value;

#[derive(Debug)]
pub struct Nodes {
    pub nodes: HashMap<String, usize>,
    pub voltage_sources: HashMap<String, usize>,
}

impl Nodes {
    fn new(devices: &Vec<Device>) -> Self {
        let mut nodes = HashMap::new();
        let mut voltage_sources = HashMap::new();
        let mut src_index = 0;

        // assume already validated that ground exists
        nodes.insert("0".to_string(), 0);
        let mut node_index = 1;
        for device in devices {
            match device {
                Device::Inductor(_) | Device::VoltageSource(_) => {
                    voltage_sources.insert(device.name().to_string(), src_index);
                    src_index += 1;
                }
                _ => {}
            }
            for node in device.nodes() {
                if !nodes.contains_key(&node.name) {
                    nodes.insert(node.name.clone(), node_index);
                    node_index += 1;
                }
            }
        }

        Self {
            nodes,
            voltage_sources,
        }
    }

    fn get_node_names(&self) -> Vec<String> {
        let mut names = vec![String::new(); self.nodes.len()];
        for (name, _) in &self.nodes {
            if let Some(index) = self.get_node_index(name) {
                names[index] = name.clone();
            }
        }
        names
    }

    fn get_source_names(&self) -> Vec<String> {
        let mut names = vec![String::new(); self.source_len()];
        for (name, _) in &self.voltage_sources {
            if let Some(index) = self.voltage_sources.get(name).copied() {
                names[index] = name.clone();
            }
        }
        names
    }

    fn get_node_index(&self, name: &str) -> Option<usize> {
        if name != "0" {
            let x = self.nodes.get(name).copied().expect("node not found");
            if x != 0 { Some(x - 1) } else { None }
        } else {
            None
        }
    }

    fn get_voltage_source_index(&self, name: &str) -> Option<usize> {
        if let Some(index) = self.voltage_sources.get(name).copied() {
            Some(self.node_len() + index)
        } else {
            None
        }
    }

    // TODO: save this?
    fn node_len(&self) -> usize {
        self.nodes
            .iter()
            .map(|(_, x)| *x)
            .max()
            .expect("no nodes found")
    }

    fn source_len(&self) -> usize {
        self.voltage_sources.len()
    }
}

fn stamp_resistor(g: &mut Array2<f64>, resistor: &Resistor, nodes: &Nodes) {
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

fn stamp_current_source(i: &mut Array1<f64>, device: &IndependentSource, nodes: &Nodes) {
    let node1 = nodes.get_node_index(&device.positive.name);
    let node2 = nodes.get_node_index(&device.negative.name);
    let value = match &device.mode {
        IndependentSourceMode::DC { value } => value.get_value(),
    };

    if let Some(node1) = node1 {
        i[node1] += value;
    }
    if let Some(node2) = node2 {
        i[node2] -= value;
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

    let value = match &device.mode {
        IndependentSourceMode::DC { value } => value.get_value(),
    };
    s[src_index] = value;
}

fn stamp_voltage_source(
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

fn simulate_op(deck: &Deck) -> Array1<f64> {
    let nodes = Nodes::new(&deck.devices);

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

    for device in &deck.devices {
        match device {
            Device::Resistor(device) => stamp_resistor(&mut m, &device, &nodes),
            Device::Capacitor(_) => {} // capcitors are just open circuits
            Device::Inductor(device) => stamp_inductor(&mut m, &mut s, &device, &nodes),
            Device::CurrentSource(device) => stamp_current_source(&mut s, &device, &nodes),
            Device::VoltageSource(device) => stamp_voltage_source(&mut m, &mut s, &device, &nodes),
        }
    }

    println!("m: {:?}", m);
    println!("s: {:?}", s);
    let lu = m.factorize_into().expect("Failed to factorize matrix");
    // [V] node voltages
    // [I] branch currents for voltage sources (also inductors)
    let x = lu.solve(&s).expect("Failed to solve linear system");

    let node_names = nodes.get_node_names();
    for (i, voltage) in x.slice(s![..n]).iter().enumerate() {
        let name = &node_names[i];
        println!("{}: {:.6}V", name, voltage);
    }

    let source_names = nodes.get_source_names();
    for (i, current) in x.slice(s![n..]).iter().enumerate() {
        let name = &source_names[i];
        println!("{}: {:.6}A", name, current);
    }

    x
}

fn sweep(vstart: f64, vstop: f64, vinc: f64) -> Vec<f64> {
    let nsteps = ((vstop - vstart) / vinc).floor() as usize;
    (0..=nsteps).map(|i| vstart + i as f64 * vinc).collect()
}

fn simulate_dc(deck: &Deck, command: &DcCommand) -> Vec<Array1<f64>> {
    let srcnam = &command.srcnam;
    let vstart = command.vstart.get_value();
    let vstop = command.vstop.get_value();
    let vincr = command.vincr.get_value();

    let nodes = Nodes::new(&deck.devices);

    let n = nodes.node_len();
    let k = nodes.source_len();

    let mut m = Array2::<f64>::zeros((n + k, n + k));
    let mut s_before = Array1::<f64>::zeros(n + k);

    println!("srcnam: {:?}", srcnam);
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
        let value = Value::new(v, None, None);
        match device {
            Device::VoltageSource(mut device) => {
                device.mode = IndependentSourceMode::DC { value };
                stamp_voltage_source_value(&mut s, &device, &nodes);
            }
            Device::CurrentSource(mut device) => {
                device.mode = IndependentSourceMode::DC { value };
                stamp_current_source(&mut s, &device, &nodes);
            }
            _ => {}
        }
        let x = lu.solve(&s).expect("Failed to solve linear system");

        let node_names = nodes.get_node_names();
        for (index, voltage) in x.slice(s![..n]).iter().enumerate() {
            let name = &node_names[index];
            println!("{}: {:.6}V", name, voltage);
        }

        let source_names = nodes.get_source_names();
        for (i, current) in x.slice(s![n..]).iter().enumerate() {
            let name = &source_names[i];
            println!("{}: {:.6}A", name, current);
        }
        results.push(x);
    }

    results
}

pub fn simulate(deck: Deck) {
    for command in &deck.commands {
        match command {
            Command::Op(_) => {
                let _ = simulate_op(&deck);
            }
            Command::Dc(command_params) => {
                let _ = simulate_dc(&deck, &command_params);
            }
            _ => panic!("Unsupported command: {:?}", command),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use spicy_parser::netlist_types::Capacitor;
    use spicy_parser::netlist_types::Node;

        use spicy_parser::parser::parse;
    use spicy_parser::parser::Parser;
    use spicy_parser::Span;

    use std::path::PathBuf;

    fn make_resistor(name: &str, n1: &str, n2: &str, value: f64) -> Resistor {
        Resistor::new(
            name.to_string(),
            Span::new(0, 0),
            Node {
                name: n1.to_string(),
            },
            Node {
                name: n2.to_string(),
            },
            Value::new(value, None, None),
        )
    }
    fn make_capacitor(name: &str, n1: &str, n2: &str, value: f64) -> Capacitor {
        Capacitor::new(
            name.to_string(),
            Span::new(0, 0),
            Node {
                name: n1.to_string(),
            },
            Node {
                name: n2.to_string(),
            },
            Value::new(value, None, None),
        )
    }


    #[test]
    fn test_nodes_indices_with_resistors() {
        let devices = vec![
            Device::Resistor(make_resistor("1", "n1", "0", 1_000.0)),
            Device::Resistor(make_resistor("2", "n2", "n1", 2_000.0)),
        ];

        let nodes = Nodes::new(&devices);

        assert_eq!(nodes.get_node_index("0"), None);
        assert_eq!(nodes.get_node_index("n1"), Some(0));
        assert_eq!(nodes.get_node_index("n2"), Some(1));
    }

    #[test]
    fn test_nodes_indices_with_capacitors() {
        let devices = vec![
            Device::Capacitor(make_capacitor("1", "n1", "0", 1e-6)),
            Device::Capacitor(make_capacitor("2", "n2", "n1", 2e-6)),
        ];

        let nodes = Nodes::new(&devices);

        assert_eq!(nodes.get_node_index("0"), None);
        assert_eq!(nodes.get_node_index("n1"), Some(0));
        assert_eq!(nodes.get_node_index("n2"), Some(1));
    }

    #[rstest]
    fn test_simulate_op(#[files("tests/*.spicy")] input: PathBuf) {

        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let deck = parse(&input_content);
        let output = simulate_op(&deck);
        let name = format!(
            "simulate-op-{}",
            input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        insta::assert_debug_snapshot!(name, output);
    }

    #[rstest]
    fn test_simulate_dc(#[files("tests/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let deck = parse(&input_content);
        let command = deck.commands[1].clone();
        let output = match command {
            Command::Dc(command) => {
                simulate_dc(&deck, &command);
            }
            _ => panic!("Unsupported command: {:?}", command),
        };

        let name = format!(
            "simulate-dc-{}",
            input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        insta::assert_debug_snapshot!(name, output);
    }
}
