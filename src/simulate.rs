use crate::parser::Deck;

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
        }
    }

    let lu = G.lu();
    let v = lu.solve(&I);

    for (i, v) in v.iter().enumerate() {
        println!("Node {}: {}", i, v);
    }

}
