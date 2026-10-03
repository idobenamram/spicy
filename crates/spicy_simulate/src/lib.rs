//! Our circuit simulator: modified nodal analysis (MNA) with a KLU sparse solver.
//! It reads one circuit from `spicy_circuit` (a `Circuit` and its `Params`) and runs
//! the operating point and DC sweeps ([`dc`]), AC ([`ac`]) and transient ([`trans`]).
//! Today the CLI calls it. Later the engine calls it through a backend (`ARCHITECTURE.md`).

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
mod unknowns;
mod util;
pub use ac::AcResult;
pub use dc::{DcSweepResult, OperatingPointResult};
pub use error::SimulationError;
pub use trans::TransientResult;
pub use unknowns::{Branch, Unknown, unknowns};

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
    use crate::test_util::{
        DcSnapshot, OpSnapshot, TranSnapshot, assert_numeric_snapshot, lower_netlist,
    };
    use crate::trans::simulate_trans;
    use rstest::rstest;
    use spicy_circuit::{Analysis, Lowered};
    use std::path::{Path, PathBuf};

    fn lower_file(path: &Path) -> Lowered {
        lower_netlist(&std::fs::read_to_string(path).expect("failed to read input file"))
    }

    fn snapshot_name(analysis: &str, input: &Path) -> String {
        let stem = input.file_stem().expect("file name").to_string_lossy();
        format!("simulate-{analysis}-{stem}")
    }

    #[test]
    fn subcircuit_instances_have_separate_internal_nodes() {
        // Two identical dividers on different supplies: each midpoint is half its own supply.
        let lowered = lower_netlist(
            "two dividers
.SUBCKT DIV top bot
R1 top mid 1k
R2 mid bot 1k
.ENDS
V1 a 0 DC 10
V2 b 0 DC 4
X1 a 0 DIV
X2 b 0 DIV
.op
.END
",
        );
        let op = simulate_op(
            &lowered.circuit,
            &lowered.params,
            &SimulationConfig::default(),
        )
        .expect("simulate_op");
        let voltages: Vec<(&str, f64)> = unknowns(&lowered.circuit)
            .iter()
            .zip(&op.solution)
            .filter(|(unknown, _)| matches!(unknown, Unknown::Voltage(_)))
            .map(|(unknown, &v)| (unknown.name(&lowered.names), v))
            .collect();
        assert_eq!(
            voltages,
            [("a", 10.0), ("b", 4.0), ("X1.mid", 5.0), ("X2.mid", 2.0)]
        );
    }

    #[test]
    fn nested_subcircuits_solve_to_the_expected_voltages() {
        // XA divides 8 V twice: X1 down to XA's internal `m`, X2 from there to ground.
        let lowered = lower_netlist(
            "nested dividers
.SUBCKT HALF a b
R1 a mid 1k
R2 mid b 1k
.ENDS
.SUBCKT QUARTER top bot
X1 top m HALF
X2 m bot HALF
.ENDS
V1 in 0 DC 8
XA in 0 QUARTER
.op
.END
",
        );
        let op = simulate_op(
            &lowered.circuit,
            &lowered.params,
            &SimulationConfig::default(),
        )
        .expect("simulate_op");
        let expected = [
            ("in", 8.0),
            ("XA.X1.mid", 6.0),
            ("XA.m", 4.0),
            ("XA.X2.mid", 2.0),
        ];
        let unknowns = unknowns(&lowered.circuit);
        for (name, volts) in expected {
            let (_, &value) = unknowns
                .iter()
                .zip(&op.solution)
                .find(|(unknown, _)| unknown.name(&lowered.names) == name)
                .unwrap_or_else(|| panic!("no unknown named {name}"));
            assert!(
                (value - volts).abs() < 1e-9,
                "V({name}) = {value}, expected {volts}"
            );
        }
    }

    #[test]
    fn operating_point_uses_each_waveforms_value_at_time_zero() {
        // ngspice evaluates transient sources at t = 0 in DC analyses (vsrcload.c):
        // PULSE and EXP give V1, SIN gives VO + VA·sin(phase), whatever its delay.
        let lowered = lower_netlist(
            "sources at t = 0
V1 a 0 SIN(1 2)
R1 a 0 1k
V2 b 0 SIN(1 2 1k 1m 0 90)
R2 b 0 1k
V3 c 0 PULSE(0.5 5 0 0 0 1u 2u)
R3 c 0 1k
V4 d 0 EXP(0.25 5)
R4 d 0 1k
.op
.end
",
        );
        let op = simulate_op(
            &lowered.circuit,
            &lowered.params,
            &SimulationConfig::default(),
        )
        .expect("simulate_op");
        assert_eq!(op.solution[..4], [1.0, 3.0, 0.5, 0.25]);
    }

    #[rstest]
    fn test_simulate_op(#[files("tests/op_dc/*.spicy")] input: PathBuf) {
        let lowered = lower_file(&input);
        let op = simulate_op(
            &lowered.circuit,
            &lowered.params,
            &SimulationConfig::default(),
        )
        .expect("simulate_op");
        let snapshot = OpSnapshot::new(&lowered, &op.solution);
        assert_numeric_snapshot(&snapshot_name("op", &input), &snapshot);
    }

    #[rstest]
    fn test_simulate_dc(#[files("tests/op_dc/simple_inductor_capacitor.spicy")] input: PathBuf) {
        let lowered = lower_file(&input);
        let Analysis::Dc(sweep) = lowered.analyses[1] else {
            panic!("expected .dc, got {:?}", lowered.analyses[1]);
        };
        let dc = simulate_dc(
            &lowered.circuit,
            &lowered.params,
            &sweep,
            &SimulationConfig::default(),
        );
        let snapshot = DcSnapshot {
            results: dc
                .results
                .iter()
                .map(|(op, value)| (OpSnapshot::new(&lowered, &op.solution), *value))
                .collect(),
        };
        assert_numeric_snapshot(&snapshot_name("dc", &input), &snapshot);
    }

    #[rstest]
    fn test_simulate_ac(#[files("tests/ac/*.spicy")] input: PathBuf) {
        let lowered = lower_file(&input);
        let sweep = lowered
            .analyses
            .iter()
            .find_map(|analysis| match analysis {
                Analysis::Ac(sweep) => Some(sweep),
                _ => None,
            })
            .expect("expected .AC command");
        let ac = simulate_ac(
            &lowered.circuit,
            &lowered.params,
            sweep,
            &SimulationConfig::default(),
        );
        assert_numeric_snapshot(&snapshot_name("ac", &input), &ac);
    }

    #[rstest]
    fn test_simulate_tran(#[files("tests/trans/*.spicy")] input: PathBuf) {
        let lowered = lower_file(&input);
        let tran = lowered
            .analyses
            .iter()
            .find_map(|analysis| match analysis {
                Analysis::Tran(tran) => Some(tran),
                _ => None,
            })
            .expect("expected .TRAN command");
        let result = simulate_trans(
            &lowered.circuit,
            &lowered.params,
            tran,
            &SimulationConfig::default(),
        )
        .expect("simulate_trans");
        let snapshot = TranSnapshot::new(&lowered, result);
        assert_numeric_snapshot(&snapshot_name("tran", &input), &snapshot);
    }
}
