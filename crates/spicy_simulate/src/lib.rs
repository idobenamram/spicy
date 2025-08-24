use spicy_parser::parser::{Deck, Element, Nodes};
use spicy_parser::netlist_types::ElementType;
use ndarray::{Array2, Array1};
use ndarray_linalg::{FactorizeInto, Solve};


fn stamp_resistor(G: &mut Array2<f64>, element: &Element, nodes: &Nodes) {
    let node1 = nodes.get(&element.nodes[0].name);
    let node2 = nodes.get(&element.nodes[1].name);

    let conductance = 1.0 / element.value.get_value();

    if let Some(node1) = node1 {
        G[[node1, node1]] += conductance;
    }
    if let Some(node2) = node2 {
        G[[node2, node2]] += conductance;
    }
    if let Some(node1) = node1 && let Some(node2) = node2 {
        G[[node1, node2]] -= conductance;
        G[[node2, node1]] -= conductance;
    }
}

fn stamp_current_source(I: &mut Array1<f64>, element: &Element, nodes: &Nodes) {
    let node1 = nodes.get(&element.nodes[0].name);
    let node2 = nodes.get(&element.nodes[1].name);
    let value = element.value.get_value();

    if let Some(node1) = node1 {
        I[node1] += value;
    }
    if let Some(node2) = node2 {
        I[node2] -= value;
    }
}


pub fn simulate(deck: Deck) {
    let nodes = deck.nodes();

    let N = nodes.len();
    // conductance matrix
    let mut G = Array2::<f64>::zeros((N, N));
    let mut I = Array1::<f64>::zeros(N);

    for element in deck.elements {
        match element.kind {
            ElementType::Resistor => stamp_resistor(&mut G, &element, &nodes),
            ElementType::CurrentSource => stamp_current_source(&mut I, &element, &nodes),
            _ => panic!("Unsupported element type: {:?}", element.kind),
        }
    }

    println!("G: {:?}", G);
    let lu = G.factorize_into().expect("Failed to factorize matrix");
    let v = lu
        .solve(&I)
        .expect("Failed to solve linear system");

    // Map indices back to node names for display
    let mut index_to_name: Vec<String> = vec![String::new(); N];
    for (name, idx) in &nodes.nodes {
        if *idx < N {
            index_to_name[*idx] = name.clone();
        }
    }

    // Debug: print mapping from index to name
    // println!("node index mapping: {:?}", index_to_name);

    for (i, voltage) in v.iter().enumerate() {
        let name = &index_to_name[i];
        println!("{}: {:.6}", name, voltage);
    }

}
