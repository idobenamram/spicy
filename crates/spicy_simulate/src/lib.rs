use spicy_parser::netlist_types::Command;
use spicy_parser::parser::Deck;

use crate::{
    ac::simulate_ac,
    dc::{simulate_dc, simulate_op},
    trans::simulate_trans,
};

mod nodes;
pub mod ac;
pub mod dc;
pub mod trans;
pub use dc::{DcSweepResult, OperatingPointResult};

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
            Command::Tran(command_params) => {
                let _ = simulate_trans(&deck, &command_params);
            }
            Command::End => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use spicy_parser::Value;
    use spicy_parser::netlist_types::Node;
    use spicy_parser::netlist_types::{Capacitor, Device, Resistor};

    use spicy_parser::Span;
    use spicy_parser::parser::parse;

    use std::path::PathBuf;

    use crate::nodes::Nodes;

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
