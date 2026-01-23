use spicy_parser::instance_parser::Deck;
use spicy_parser::netlist_types::Command;

use crate::{
    ac::simulate_ac,
    dc::{simulate_dc, simulate_op},
    trans::simulate_trans,
};

pub mod ac;
pub mod dc;
// mod nodes;
mod devices;
mod error;
mod matrix;
pub(crate) mod raw_writer;
mod setup_pattern;
pub mod solver;
pub mod trans;
pub use dc::{DcSweepResult, OperatingPointResult};
pub use trans::TransientResult;

#[derive(Debug, Clone)]
pub enum LinearSolver {
    Klu { config: solver::klu::KluConfig },
    Blas,
}

#[derive(Debug, Clone, Copy)]
pub enum TransientIntegrator {
    BackwardEuler,
    Trapezoidal,
}

#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub solver: LinearSolver,
    pub integrator: TransientIntegrator,
    /// if true, write raw files
    pub write_raw: bool,
    /// optional output base path (without extension). If None, use deck.title in CWD
    pub output_base: Option<String>,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            solver: LinearSolver::Klu {
                config: solver::klu::KluConfig::default(),
            },
            integrator: TransientIntegrator::BackwardEuler,
            write_raw: false,
            output_base: None,
        }
    }
}

impl SimulationConfig {
    pub fn get_output_base(&self, deck: &Deck, extension: &str) -> String {
        self.output_base
            .clone()
            .unwrap_or_else(|| format!("{}-{}", deck.title.clone(), extension))
    }
}

pub fn simulate(deck: Deck, sim_config: SimulationConfig) {
    for command in &deck.commands {
        match command {
            Command::Op(_) => {
                let op = simulate_op(&deck, &sim_config);
                if sim_config.write_raw {
                    let base = sim_config.get_output_base(&deck, "op");
                    let _ = raw_writer::write_operating_point_raw(&deck, &op, &base);
                }
            }
            Command::Dc(command_params) => {
                let dc = simulate_dc(&deck, command_params, &sim_config);
                if sim_config.write_raw {
                    let base = sim_config.get_output_base(&deck, "dc");
                    // detect if sweep is a voltage source by scanning devices
                    let is_voltage = deck
                        .devices
                        .voltage_sources
                        .iter()
                        .any(|v| v.name == command_params.srcnam);
                    let _ = raw_writer::write_dc_raw(
                        &deck,
                        &dc,
                        &base,
                        &command_params.srcnam,
                        is_voltage,
                    );
                }
            }
            Command::Ac(command_params) => {
                let ac = simulate_ac(&deck, command_params, &sim_config);
                if sim_config.write_raw {
                    let base = sim_config.get_output_base(&deck, "ac");
                    let _ = raw_writer::write_ac_raw(&deck, &ac, &base);
                }
            }
            Command::Tran(command_params) => {
                let result = simulate_trans(&deck, command_params, &sim_config);
                if sim_config.write_raw {
                    let base = sim_config.get_output_base(&deck, "tran");
                    let _ = raw_writer::write_transient_raw(&deck, &result, &base);
                }
            }
            Command::End => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use spicy_parser::netlist_types::{NodeIndex, NodeName};
    use spicy_parser::node_mapping::NodeMapping;

    use spicy_parser::parse;
    use spicy_parser::{ParseOptions, SourceMap};

    use std::path::PathBuf;

    #[test]
    fn test_node_mapping_mna_indices() {
        let mut mapping = NodeMapping::new();
        let n1 = mapping.insert_node(NodeName("n1".to_string()));
        let n2 = mapping.insert_node(NodeName("n2".to_string()));

        assert_eq!(mapping.mna_node_index(NodeIndex(0)), None);
        assert_eq!(mapping.mna_node_index(n1), Some(0));
        assert_eq!(mapping.mna_node_index(n2), Some(1));
    }

    #[test]
    fn test_node_mapping_names_mna_order() {
        let mut mapping = NodeMapping::new();
        mapping.insert_node(NodeName("n1".to_string()));
        mapping.insert_node(NodeName("n2".to_string()));

        assert_eq!(
            mapping.node_names_mna_order(),
            vec!["n1".to_string(), "n2".to_string()]
        );
    }

    #[rstest]
    fn test_simulate_op(#[files("tests/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let source_map = SourceMap::new(input.clone(), input_content);
        let mut input_options = ParseOptions {
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            source_map,
            max_include_depth: 10,
        };
        let deck = parse(&mut input_options).expect("parse");
        let sim_config = SimulationConfig::default();
        let output = simulate_op(&deck, &sim_config);
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
    fn test_simulate_dc(#[files("tests/simple_inductor_capacitor.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let source_map = SourceMap::new(input.clone(), input_content);
        let mut input_options = ParseOptions {
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            source_map,
            max_include_depth: 10,
        };
        let deck = parse(&mut input_options).expect("parse");
        let command = deck.commands[1].clone();
        let output = match command {
            Command::Dc(command) => {
                let sim_config = SimulationConfig::default();
                simulate_dc(&deck, &command, &sim_config)
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
