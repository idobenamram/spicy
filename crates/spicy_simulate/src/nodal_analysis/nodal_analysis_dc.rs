mod union_find;

use std::collections::HashMap;

use ndarray::{Array1, Array2};
use ndarray_linalg::{FactorizeInto, Solve};
use spicy_parser::netlist_types::{CommandType, ElementType};
use spicy_parser::parser::{Deck, Directive, Element, Value};
use union_find::UnionFind;

#[derive(Debug)]
pub struct Nodes {
    pub nodes: HashMap<String, usize>,
    pub uf: UnionFind,
}

impl Nodes {
    fn new(elements: &Vec<Element>) -> Self {
        let mut nodes = HashMap::new();
        // assume already validated that ground exists
        nodes.insert("0".to_string(), 0);
        let mut node_index = 1;
        for element in elements {
            for node in element.nodes.iter() {
                if !nodes.contains_key(&node.name) {
                    nodes.insert(node.name.clone(), node_index);
                    node_index += 1;
                }
            }
        }
        let mut uf = UnionFind::new(nodes.len());

        for element in elements {
            match element.kind {
                ElementType::Inductor => {
                    let node1 = nodes
                        .get(&element.nodes[0].name)
                        .expect("just went over nodes");
                    let node2 = nodes
                        .get(&element.nodes[1].name)
                        .expect("just went over nodes");
                    uf.union(*node1, *node2);
                }
                _ => {}
            }
        }

        Self { nodes, uf }
    }

    fn get_node_names(&self) -> Vec<String> {
        let mut names = vec![String::new(); self.nodes.len()];
        for (name, _) in &self.nodes {
            if let Some(index) = self.get(name) {
                names[index] = name.clone();
            }
        }
        names
    }

    fn get(&self, name: &str) -> Option<usize> {
        if name != "0" {
            let x = self.nodes.get(name).copied().expect("node not found");
            let index = self.uf.find_no_compress(x);
            if index != 0 { Some(index - 1) } else { None }
        } else {
            None
        }
    }

    fn len(&self) -> usize {
        self.nodes
            .iter()
            .map(|(_, x)| self.uf.find_no_compress(*x))
            .max()
            .expect("no nodes found")
    }
}

fn stamp_resistor(g: &mut Array2<f64>, element: &Element, nodes: &Nodes) {
    let node1 = nodes.get(&element.nodes[0].name);
    let node2 = nodes.get(&element.nodes[1].name);
    println!("stamp_resistor: {:?}, {:?}", node1, node2);

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

fn simulate_op(deck: &Deck) -> Array1<f64> {
    let nodes = Nodes::new(&deck.elements);
    println!("nodes: {:?}", nodes);

    let n = nodes.len();
    // conductance matrix
    let mut g = Array2::<f64>::zeros((n, n));
    // current vector
    let mut i = Array1::<f64>::zeros(n);

    for element in &deck.elements {
        match element.kind {
            ElementType::Resistor => stamp_resistor(&mut g, &element, &nodes),
            ElementType::Capacitor => {} // capcitors are just open circuits
            ElementType::Inductor => {}  // inductors are just short circuits
            ElementType::CurrentSource => stamp_current_source(&mut i, &element, &nodes),
            _ => panic!("Unsupported element type: {:?}", element.kind),
        }
    }

    println!("g: {:?}", g);
    let lu = g.factorize_into().expect("Failed to factorize matrix");
    let v = lu.solve(&i).expect("Failed to solve linear system");

    let node_names = nodes.get_node_names();
    for (i, voltage) in v.iter().enumerate() {
        let name = &node_names[i];
        println!("{}: {:.6}", name, voltage);
    }

    v
}

fn sweep(vstart: f64, vstop: f64, vinc: f64) -> Vec<f64> {
    let nsteps = ((vstop - vstart) / vinc).floor() as usize;
    (0..=nsteps).map(|i| vstart + i as f64 * vinc).collect()
}

fn simulate_dc(deck: &Deck, directive: &Directive) -> Vec<Array1<f64>> {
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
    let mut i_before = Array1::<f64>::zeros(n);

    let source_index = deck
        .elements
        .iter()
        .position(|e| e.name() == *srcnam)
        .expect("Source not found");
    for element in &deck.elements {
        match element.kind {
            ElementType::Resistor => stamp_resistor(&mut g, &element, &nodes),
            ElementType::Capacitor => {} // capcitors are just open circuits
            ElementType::Inductor => {}  // inductors are just short circuits
            ElementType::CurrentSource => {
                if element.name() != *srcnam {
                    stamp_current_source(&mut i_before, &element, &nodes);
                }
            }
            _ => panic!("Unsupported element type: {:?}", element.kind),
        }
    }

    let lu = g.factorize_into().expect("Failed to factorize matrix");

    let sweep_values = sweep(vstart, vstop, vincr);
    
    let mut results = Vec::new();
    for v in sweep_values {
        let mut i = i_before.clone();
        let mut element = deck.elements[source_index].clone();
        // TODO: this sucks
        let value = Value::new(v, None, None);
        element.value = value;
        stamp_current_source(&mut i, &element, &nodes);
        let v = lu.solve(&i).expect("Failed to solve linear system");

        let node_names = nodes.get_node_names();
        for (index, voltage) in v.iter().enumerate() {
            let name = &node_names[index];
            println!("{}: {:.6}V", name, voltage);
        }
        results.push(v);
    }

    results
}

pub fn simulate(deck: Deck) {
    for directive in &deck.directives {
        match directive.kind {
            CommandType::Op => {
                let _ = simulate_op(&deck);
            }
            CommandType::DC => {
                let _ = simulate_dc(&deck, &directive);
            }
            CommandType::End => break,
            _ => panic!("Unsupported directive: {:?}", directive.kind),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use spicy_parser::parser::Node;
    use std::collections::HashMap;

    use spicy_parser::parser::Parser;

    use std::path::PathBuf;

    fn make_element(kind: ElementType, name: &str, n1: &str, n2: &str, value: f64) -> Element {
        Element {
            kind,
            name: name.to_string(),
            nodes: vec![
                Node {
                    name: n1.to_string(),
                },
                Node {
                    name: n2.to_string(),
                },
            ],
            value: Value::new(value, None, None),
            params: HashMap::new(),
            start: 0,
            end: 0,
        }
    }

    #[test]
    fn test_nodes_indices_with_resistors() {
        let elements = vec![
            make_element(ElementType::Resistor, "1", "n1", "0", 1_000.0),
            make_element(ElementType::Resistor, "2", "n2", "n1", 2_000.0),
        ];

        let nodes = Nodes::new(&elements);

        assert_eq!(nodes.get("0"), None);
        assert_eq!(nodes.get("n1"), Some(0));
        assert_eq!(nodes.get("n2"), Some(1));
    }

    #[test]
    fn test_nodes_indices_with_capacitors() {
        let elements = vec![
            make_element(ElementType::Capacitor, "1", "n1", "0", 1e-6),
            make_element(ElementType::Capacitor, "2", "n2", "n1", 2e-6),
        ];

        let nodes = Nodes::new(&elements);

        assert_eq!(nodes.get("0"), None);
        assert_eq!(nodes.get("n1"), Some(0));
        assert_eq!(nodes.get("n2"), Some(1));
    }

    #[test]
    fn test_nodes_indices_with_inductors_union() {
        let elements = vec![
            make_element(ElementType::Inductor, "1", "n1", "n2", 1e-3),
            // include a resistor so order is deterministic after the inductor-created nodes
            make_element(ElementType::Resistor, "2", "n3", "0", 1_000.0),
        ];

        let nodes = Nodes::new(&elements);

        let n1_idx = nodes.get("n1");
        let n2_idx = nodes.get("n2");
        assert_eq!(n1_idx, n2_idx);
        assert!(n1_idx.is_some());
        assert_eq!(nodes.get("0"), None);
    }

    #[test]
    fn test_nodes_indices_with_multiple_inductors_union() {
        let elements = vec![
            make_element(ElementType::Inductor, "1", "n1", "n2", 1e-3),
            make_element(ElementType::Inductor, "2", "n2", "n3", 1e-3),
            make_element(ElementType::Inductor, "3", "n3", "n4", 1e-3),
        ];

        let nodes = Nodes::new(&elements);

        let n1_idx = nodes.get("n1");
        let n2_idx = nodes.get("n2");
        let n3_idx = nodes.get("n3");
        let n4_idx = nodes.get("n4");

        assert_eq!(n1_idx, n2_idx);
        assert_eq!(n2_idx, n3_idx);
        assert_eq!(n3_idx, n4_idx);
        assert!(n1_idx.is_some());
        assert_eq!(nodes.get("0"), None);
    }

    #[rstest]
    fn test_simulate_op(#[files("tests/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let deck = Parser::new(&input_content).parse();
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
        use spicy_parser::{
            netlist_types::ValueSuffix,
            parser::{Attr, Attributes},
        };

        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let deck = Parser::new(&input_content).parse();
        let output = simulate_dc(
            &deck,
            &Directive {
                kind: CommandType::DC,
                params: Attributes::from_iter(vec![
                    ("srcnam".to_string(), Attr::String("I1".to_string())),
                    (
                        "vstart".to_string(),
                        Attr::Value(Value::new(1.0, None, Some(ValueSuffix::Milli))),
                    ),
                    (
                        "vstop".to_string(),
                        Attr::Value(Value::new(5.0, None, Some(ValueSuffix::Milli))),
                    ),
                    (
                        "vincr".to_string(),
                        Attr::Value(Value::new(1.0, None, Some(ValueSuffix::Milli))),
                    ),
                ]),
                start: 0,
                end: 0,
            },
        );
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
