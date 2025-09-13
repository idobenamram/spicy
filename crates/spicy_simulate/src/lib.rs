use std::collections::HashMap;

use ndarray::{Array1, Array2, s};
use ndarray_linalg::{FactorizeInto, Solve};
use spicy_parser::Value;
use spicy_parser::netlist_types::AcCommand;
use spicy_parser::netlist_types::{AcSweepType, Command, Device};
use spicy_parser::netlist_types::{DcCommand, IndependentSource, Inductor, Resistor};
use spicy_parser::parser::Deck;
use std::f64::consts::PI;

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
    let value = match &device.dc {
        Some(value) => value.get_value(),
        None => 0.0,
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

    let value = match &device.dc {
        Some(value) => value.get_value(),
        None => 0.0,
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

fn stamp_capacitor_ac(
    ai: &mut Array2<f64>,
    device: &spicy_parser::netlist_types::Capacitor,
    nodes: &Nodes,
    w: f64,
) {
    let node1 = nodes.get_node_index(&device.positive.name);
    let node2 = nodes.get_node_index(&device.negative.name);
    // Yc = j * w * C -> purely imaginary admittance placed on ai
    let yc = w * device.capacitance.get_value();

    if let Some(n1) = node1 {
        ai[[n1, n1]] += yc;
    }
    if let Some(n2) = node2 {
        ai[[n2, n2]] += yc;
    }
    if let (Some(n1), Some(n2)) = (node1, node2) {
        ai[[n1, n2]] -= yc;
        ai[[n2, n1]] -= yc;
    }
}

fn stamp_inductor_ac_mna(
    ar: &mut Array2<f64>,
    ai: &mut Array2<f64>,
    device: &Inductor,
    nodes: &Nodes,
    w: f64,
) {
    let node1 = nodes.get_node_index(&device.positive.name);
    let node2 = nodes.get_node_index(&device.negative.name);
    let k = nodes
        .get_voltage_source_index(&device.name)
        .expect("should exist");

    // Incidence (real part): same as DC B and B^T
    if let Some(n1) = node1 {
        ar[[n1, k]] += 1.0;
        ar[[k, n1]] += 1.0;
    }
    if let Some(n2) = node2 {
        ar[[n2, k]] -= 1.0;
        ar[[k, n2]] -= 1.0;
    }

    // KVL: v = (Va - Vb) - j*w*L*i = 0 -> put +w*L on imag diagonal of KVL row/col
    let wl = w * device.inductance.get_value();
    ai[[k, k]] += wl;
}

fn stamp_voltage_source_incidence_real(
    ar: &mut Array2<f64>,
    device: &IndependentSource,
    nodes: &Nodes,
) {
    let n1 = nodes.get_node_index(&device.positive.name);
    let n2 = nodes.get_node_index(&device.negative.name);
    let k = nodes
        .get_voltage_source_index(&device.name)
        .expect("should exist");

    if let Some(n1) = n1 {
        ar[[n1, k]] = 1.0;
        ar[[k, n1]] = 1.0;
    }
    if let Some(n2) = n2 {
        ar[[n2, k]] = -1.0;
        ar[[k, n2]] = -1.0;
    }
}

fn ac_frequencies(cmd: &AcCommand) -> Vec<f64> {
    let fstart = cmd.fstart.get_value();
    let fstop  = cmd.fstop.get_value();
    assert!(fstop > fstart, ".AC: fstop must be > fstart");

    const EPS: f64 = 1e-12;

    match &cmd.ac_sweep_type {
        AcSweepType::Dec(n) => {
            let n = *n;
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
        AcSweepType::Oct(n) => {
            let n = *n;
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
        AcSweepType::Lin(n) => {
            let n = *n;
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
fn assemble_ac_real_expansion(deck: &Deck, w: f64) -> (Array2<f64>, Array1<f64>) {
    let nodes = Nodes::new(&deck.devices);
    let n = nodes.node_len();
    let k = nodes.source_len();

    // Real and Imag parts of the small-signal MNA (size (n+k) x (n+k))
    let mut ar = Array2::<f64>::zeros((n + k, n + k));
    let mut ai = Array2::<f64>::zeros((n + k, n + k));

    // RHS real/imag
    let mut br = Array1::<f64>::zeros(n + k);
    let mut bi = Array1::<f64>::zeros(n + k);

    for device in &deck.devices {
        match device {
            Device::Resistor(dev) => {
                // purely real conductance -> stamp into ar using existing DC helper
                stamp_resistor(&mut ar, &dev, &nodes);
            }
            Device::Capacitor(dev) => {
                stamp_capacitor_ac(&mut ai, &dev, &nodes, w);
            }
            Device::Inductor(dev) => {
                stamp_inductor_ac_mna(&mut ar, &mut ai, &dev, &nodes, w);
            }
            Device::VoltageSource(dev) => {
                // Incidence into ar; AC magnitude/phase handling TBD in parser -> RHS remains 0 if not specified
                stamp_voltage_source_incidence_real(&mut ar, &dev, &nodes);
                // If your parser carries AC {mag, phase}, convert to (br/bi) here.
                // Example (pseudo): if let Some((mag, phase_deg)) = dev.ac_spec { let ph = phase_deg * PI/180.0; bi_or_br... }
            }
            Device::CurrentSource(_dev) => {
                // In small-signal AC, DC sources are turned off. If parser supplies AC value, add to (br/bi) here.
                // Example (pseudo):
                // let phasor = mag * (cos(ph) + j sin(ph));
                // br[node_a] -= Re(phasor); br[node_b] += Re(phasor);
                // bi[node_a] -= Im(phasor); bi[node_b] += Im(phasor);
            }
        }
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

fn simulate_ac(deck: &Deck, cmd: &AcCommand) -> Vec<(f64, Array1<f64>, Array1<f64>)> {
    let freqs = ac_frequencies(cmd);
    let nodes = Nodes::new(&deck.devices);
    let n = nodes.node_len();
    let k = nodes.source_len();

    let mut out = Vec::new();

    for f in freqs {
        let w = 2.0 * PI * f;
        let (m, s_vec) = assemble_ac_real_expansion(deck, w);
        let lu = m.factorize_into().expect("Failed to factorize AC matrix");
        let x = lu.solve(&s_vec).expect("Failed to solve AC system");

        let dim = n + k;
        let xr = x.slice(s![0..dim]).to_owned();
        let xi = x.slice(s![dim..2 * dim]).to_owned();

        // Optional: print node phasors
        let node_names = nodes.get_node_names();
        for i in 0..n {
            let vr = xr[i];
            let vi = xi[i];
            let mag = (vr * vr + vi * vi).sqrt();
            let phase = vi.atan2(vr) * 180.0 / PI;
            println!(
                "f={:.6} Hz  {}: {:.6} ∠ {:.3}°",
                f, node_names[i], mag, phase
            );
        }

        out.push((f, xr, xi));
    }

    out
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
            Command::Ac(command_params) => {
                let _ = simulate_ac(&deck, &command_params);
            }
            Command::End => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use spicy_parser::netlist_types::Capacitor;
    use spicy_parser::netlist_types::Node;

    use spicy_parser::Span;
    use spicy_parser::parser::parse;

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
        let deck = parse(&input_content).expect("parse");
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
        let deck = parse(&input_content).expect("parse");
        let command = deck.commands[1].clone();
        let output = match command {
            Command::Dc(command) => simulate_dc(&deck, &command),
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
