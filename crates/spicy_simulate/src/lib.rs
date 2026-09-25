pub mod ac;
pub mod dc;
// mod nodes;
mod devices;
mod error;
mod matrix;
mod setup_pattern;
pub mod solver;
#[cfg(test)]
mod test_util;
pub mod trans;
mod util;
pub use ac::AcResult;
pub use dc::{DcSweepResult, OperatingPointResult};
pub use error::SimulationError;
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

#[derive(Debug, Clone, Copy)]
pub struct NewtonConfig {
    pub abs_tol: f64,
    pub rel_tol: f64,
    pub max_iters: usize,
}

impl Default for NewtonConfig {
    fn default() -> Self {
        Self {
            abs_tol: 1e-6,
            rel_tol: 1e-3,
            max_iters: 50,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum NewtonMode {
    InitOp,
    InitTrans,
    Iterate,
}

#[derive(Debug, Clone, Copy)]
pub struct NewtonState {
    pub config: NewtonConfig,
    pub mode: NewtonMode,
}

impl NewtonState {
    pub fn new(config: NewtonConfig, mode: NewtonMode) -> Self {
        Self { config, mode }
    }
}

#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub solver: LinearSolver,
    pub integrator: TransientIntegrator,
    pub newton: NewtonConfig,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            solver: LinearSolver::Klu {
                config: solver::klu::KluConfig::default(),
            },
            integrator: TransientIntegrator::BackwardEuler,
            newton: NewtonConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ac::simulate_ac;
    use crate::dc::{simulate_dc, simulate_op};
    use crate::test_util::assert_numeric_snapshot;
    use crate::trans::simulate_trans;
    use rstest::rstest;
    use spicy_parser::netlist_types::{Command, NodeIndex, NodeName};
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
    fn test_simulate_op(#[files("tests/op_dc/*.spicy")] input: PathBuf) {
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
        let output = simulate_op(&deck, &sim_config).expect("simulate_op");
        let name = format!(
            "simulate-op-{}",
            input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        assert_numeric_snapshot(&name, &output);
    }

    #[rstest]
    fn test_simulate_dc(#[files("tests/op_dc/simple_inductor_capacitor.spicy")] input: PathBuf) {
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
        assert_numeric_snapshot(&name, &output);
    }

    #[rstest]
    fn test_simulate_ac(#[files("tests/ac/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let source_map = SourceMap::new(input.clone(), input_content);
        let mut input_options = ParseOptions {
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            source_map,
            max_include_depth: 10,
        };
        let deck = parse(&mut input_options).expect("parse");
        let command = deck
            .commands
            .iter()
            .find_map(|cmd| match cmd {
                Command::Ac(ac) => Some(ac),
                _ => None,
            })
            .expect("expected .AC command");
        let sim_config = SimulationConfig::default();
        let output = simulate_ac(&deck, command, &sim_config);

        let name = format!(
            "simulate-ac-{}",
            input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        assert_numeric_snapshot(&name, &output);
    }

    #[rstest]
    fn test_simulate_tran(#[files("tests/trans/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let source_map = SourceMap::new(input.clone(), input_content);
        let mut input_options = ParseOptions {
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            source_map,
            max_include_depth: 10,
        };
        let deck = parse(&mut input_options).expect("parse");
        let command = deck
            .commands
            .iter()
            .find_map(|cmd| match cmd {
                Command::Tran(tran) => Some(tran),
                _ => None,
            })
            .expect("expected .TRAN command");
        let sim_config = SimulationConfig::default();
        let output = simulate_trans(&deck, command, &sim_config).expect("simulate_trans");

        let name = format!(
            "simulate-tran-{}",
            input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        assert_numeric_snapshot(&name, &output);
    }
}
