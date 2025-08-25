use std::collections::HashMap;

use ndarray::{Array1, Array2};
use ndarray_linalg::{FactorizeInto, Solve};
use spicy_parser::netlist_types::{CommandType, ElementType};
use spicy_parser::parser::{Deck, Directive, Element, Value};

#[derive(Debug)]
pub struct Nodes {
    pub nodes: HashMap<String, usize>,
    pub node_names: Vec<String>,
}

impl Nodes {
    fn new(elements: &Vec<Element>) -> Self {
        let mut nodes = HashMap::new();
        let mut node_index = 0;
        for element in elements {
            for node in element.nodes.iter() {
                if node.name != "0" {
                    if !nodes.contains_key(&node.name) {
                        nodes.insert(node.name.clone(), node_index);
                        node_index += 1;
                    }
                }
            }
        }

        let node_names = nodes.keys().cloned().collect();

        Self { nodes, node_names }
    }

    pub fn get(&self, name: &str) -> Option<usize> {
        if name != "0" {
            self.nodes.get(name).copied()
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }
}

fn stamp_resistor(g: &mut Array2<f64>, element: &Element, nodes: &Nodes) {
    let node1 = nodes.get(&element.nodes[0].name);
    let node2 = nodes.get(&element.nodes[1].name);

    let conductance = 1.0 / element.value.get_value();

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

fn stamp_current_source(i: &mut Array1<f64>, element: &Element, nodes: &Nodes) {
    let node1 = nodes.get(&element.nodes[0].name);
    let node2 = nodes.get(&element.nodes[1].name);
    let value = element.value.get_value();

    if let Some(node1) = node1 {
        i[node1] += value;
    }
    if let Some(node2) = node2 {
        i[node2] -= value;
    }
}

fn simulate_op(deck: &Deck) {
    let nodes = Nodes::new(&deck.elements);

    let n = nodes.len();
    // conductance matrix
    let mut g = Array2::<f64>::zeros((n, n));
    // current vector
    let mut i = Array1::<f64>::zeros(n);

    for element in &deck.elements {
        match element.kind {
            ElementType::Resistor => stamp_resistor(&mut g, &element, &nodes),
            ElementType::CurrentSource => stamp_current_source(&mut i, &element, &nodes),
            _ => panic!("Unsupported element type: {:?}", element.kind),
        }
    }

    println!("g: {:?}", g);
    let lu = g.factorize_into().expect("Failed to factorize matrix");
    let v = lu.solve(&i).expect("Failed to solve linear system");

    for (i, voltage) in v.iter().enumerate() {
        let name = &nodes.node_names[i];
        println!("{}: {:.6}", name, voltage);
    }
}

fn sweep(vstart: f64, vstop: f64, vinc: f64) -> Vec<f64> {
    let nsteps = ((vstop - vstart) / vinc).floor() as usize;
    (0..=nsteps).map(|i| vstart + i as f64 * vinc).collect()
}

fn simulate_dc(deck: &Deck, directive: &Directive) {
    let srcnam = directive
        .params
        .get_string("srcnam")
        .expect("srcnam is required");
    let vstart = directive
        .params
        .get_value("vstart")
        .expect("vstart is required");
    let vstop = directive
        .params
        .get_value("vstop")
        .expect("vstop is required");
    let vincr = directive
        .params
        .get_value("vincr")
        .expect("vincr is required");

    let vstart = vstart.get_value();
    let vstop = vstop.get_value();
    let vincr = vincr.get_value();

    let nodes = Nodes::new(&deck.elements);

    let n = nodes.len();

    // conductance matrix
    let mut g = Array2::<f64>::zeros((n, n));
    let mut i = Array1::<f64>::zeros(n);

    let source_index = deck
        .elements
        .iter()
        .position(|e| e.name() == *srcnam)
        .expect("Source not found");
    for element in &deck.elements {
        match element.kind {
            ElementType::Resistor => stamp_resistor(&mut g, &element, &nodes),
            ElementType::CurrentSource => {
                if element.name() != *srcnam {
                    stamp_current_source(&mut i, &element, &nodes);
                }
            }
            _ => panic!("Unsupported element type: {:?}", element.kind),
        }
    }

    let lu = g.factorize_into().expect("Failed to factorize matrix");

    let sweep_values = sweep(vstart, vstop, vincr);
    for v in sweep_values {
        let mut element = deck.elements[source_index].clone();
        // TODO: this sucks
        let value = Value::new(v, None, None);
        element.value = value;
        stamp_current_source(&mut i, &element, &nodes);
        let v = lu.solve(&i).expect("Failed to solve linear system");

        for (i, voltage) in v.iter().enumerate() {
            let name = &nodes.node_names[i];
            println!("{}: {:.6}", name, voltage);
        }
    }
}

pub fn simulate(deck: Deck) {
    for directive in &deck.directives {
        match directive.kind {
            CommandType::Op => simulate_op(&deck),
            CommandType::DC => simulate_dc(&deck, &directive),
            CommandType::End => break,
            _ => panic!("Unsupported directive: {:?}", directive.kind),
        }
    }
}
