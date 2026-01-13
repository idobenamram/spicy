use ndarray::{Array1, Array2, OwnedRepr};
use ndarray_linalg::{Factorize, LUFactorized, Solve};
use spicy_parser::netlist_types::{CurrentBranchIndex, NodeIndex};
use spicy_parser::node_mapping::NodeMapping;

use crate::{
    LinearSolver, SimulationConfig,
    devices::Devices,
    error::SimulationError,
    setup_pattern::{setup_dense_stamps, setup_pattern},
    solver::{
        klu::{self, KluConfig, KluNumeric, KluSymbolic},
        matrix::csc::CscMatrix,
    },
};

pub struct BlasMatrix {
    node_mapping: NodeMapping,
    lu: Option<LUFactorized<OwnedRepr<f64>>>,
    m: ndarray::Array2<f64>,
    s: ndarray::Array1<f64>,
}

impl BlasMatrix {
    pub fn new(n: usize, node_mapping: NodeMapping) -> Self {
        // Modified nodal analysis matrix
        // [G, B]
        // [B^T, 0]
        // conductance matrix + incidence of each voltage-defined element
        let m = Array2::<f64>::zeros((n, n));
        // [I] current vector
        // [E] source voltages
        // current and voltage source vectors
        let s = Array1::<f64>::zeros(n);
        Self {
            node_mapping,
            lu: None,
            m,
            s,
        }
    }
}

pub struct KluMatrix {
    config: KluConfig,
    // TODO: kinda sucks that its an option
    symbolic: Option<KluSymbolic>,
    numeric: Option<KluNumeric>,
    node_mapping: NodeMapping,
    matrix: CscMatrix,
    s: Vec<f64>,
}

impl KluMatrix {
    pub fn new(
        matrix: CscMatrix,
        s: Vec<f64>,
        node_mapping: NodeMapping,
        config: KluConfig,
    ) -> Self {
        Self {
            config,
            symbolic: None,
            numeric: None,
            matrix,
            s,
            node_mapping,
        }
    }
}

// We create 1 solver matrix per simulation and only use it by reference.
#[allow(clippy::large_enum_variant)]
pub enum SolverMatrix {
    Klu(KluMatrix),
    Blas(BlasMatrix),
}

impl SolverMatrix {
    pub fn create_matrix(
        devices: &mut Devices,
        node_mapping: NodeMapping,
        sim_config: &SimulationConfig,
    ) -> Result<SolverMatrix, SimulationError> {
        let matrix_dim = node_mapping.mna_matrix_dim();

        let sm = match sim_config.solver {
            LinearSolver::Klu { config } => {
                let matrix = setup_pattern(devices, &node_mapping)?;
                // KLU solve overwrites RHS in-place, so we allocate it up-front.
                Self::Klu(KluMatrix::new(
                    matrix,
                    vec![0.0; matrix_dim],
                    node_mapping,
                    config,
                ))
            }
            LinearSolver::Blas => {
                setup_dense_stamps(devices, &node_mapping);
                Self::Blas(BlasMatrix::new(matrix_dim, node_mapping))
            }
        };

        Ok(sm)
    }

    pub fn get_mut_nnz(&mut self, nnz: usize) -> &mut f64 {
        match self {
            Self::Klu(matrix) => matrix.matrix.get_mut_nnz(nnz),
            Self::Blas(matrix) => {
                let dim = matrix.m.ncols();
                let row = nnz / dim;
                let col = nnz % dim;
                &mut matrix.m[[row, col]]
            }
        }
    }

    pub fn get_mut_rhs(&mut self, index: usize) -> &mut f64 {
        match self {
            Self::Klu(matrix) => &mut matrix.s[index],
            Self::Blas(matrix) => &mut matrix.s[index],
        }
    }

    /// Zero out matrix entries + RHS (keeps sparsity pattern / mapping).
    pub fn clear(&mut self) {
        match self {
            Self::Klu(matrix) => {
                matrix.matrix.values.fill(0.0);
                matrix.s.fill(0.0);
            }
            Self::Blas(matrix) => {
                matrix.m.fill(0.0);
                matrix.s.fill(0.0);
                matrix.lu = None;
            }
        }
    }

    /// Current RHS vector (overwritten with solution after `solve()`).
    pub fn rhs(&self) -> &[f64] {
        match self {
            Self::Klu(matrix) => matrix.s.as_slice(),
            Self::Blas(matrix) => matrix.s.as_slice().expect("BLAS RHS should be contiguous"),
        }
    }

    /// Mutable RHS vector (overwritten with solution after `solve()`).
    pub fn rhs_mut(&mut self) -> &mut [f64] {
        match self {
            Self::Klu(matrix) => matrix.s.as_mut_slice(),
            Self::Blas(matrix) => matrix
                .s
                .as_slice_mut()
                .expect("BLAS RHS should be contiguous"),
        }
    }

    pub fn mna_node_index(&self, node_index: NodeIndex) -> Option<usize> {
        match self {
            Self::Klu(matrix) => matrix.node_mapping.mna_node_index(node_index),
            Self::Blas(matrix) => matrix.node_mapping.mna_node_index(node_index),
        }
    }
    pub fn mna_branch_index(&self, branch_index: CurrentBranchIndex) -> usize {
        match self {
            Self::Klu(matrix) => matrix.node_mapping.mna_branch_index(branch_index),
            Self::Blas(matrix) => matrix.node_mapping.mna_branch_index(branch_index),
        }
    }

    pub fn analyze(&mut self) -> Result<(), SimulationError> {
        match self {
            Self::Klu(matrix) => {
                let symbolic = klu::analyze(&matrix.matrix, &matrix.config)?;
                matrix.symbolic = Some(symbolic);
            }
            // no analyze phase for blas
            Self::Blas(_) => {}
        }
        Ok(())
    }

    pub fn factorize(&mut self) -> Result<(), SimulationError> {
        match self {
            Self::Klu(matrix) => {
                let symbolic = matrix
                    .symbolic
                    .as_mut()
                    .ok_or(SimulationError::KLUSymbolicNotAnalyzed)?;
                let numeric = klu::factor(&matrix.matrix, symbolic, &mut matrix.config)?;
                matrix.numeric = Some(numeric);
            }
            Self::Blas(matrix) => {
                let lu = matrix.m.factorize()?;
                matrix.lu = Some(lu);
            }
        }
        Ok(())
    }

    pub fn refactor(&mut self) -> Result<(), SimulationError> {
        match self {
            Self::Klu(matrix) => {
                let symbolic = matrix
                    .symbolic
                    .as_mut()
                    .ok_or(SimulationError::KLUSymbolicNotAnalyzed)?;
                let numeric = matrix
                    .numeric
                    .as_mut()
                    .ok_or(SimulationError::KluNumericNotFactorized)?;
                klu::refactor(&matrix.matrix, symbolic, numeric, &matrix.config)?;
            }
            Self::Blas(matrix) => {
                let lu = matrix.m.factorize()?;
                matrix.lu = Some(lu);
            }
        }

        Ok(())
    }

    pub fn solve(&mut self) -> Result<(), SimulationError> {
        match self {
            Self::Klu(matrix) => {
                let symbolic = matrix
                    .symbolic
                    .as_ref()
                    .ok_or(SimulationError::KLUSymbolicNotAnalyzed)?;
                let numeric = matrix
                    .numeric
                    .as_mut()
                    .ok_or(SimulationError::KluNumericNotFactorized)?;

                klu::solve(
                    symbolic,
                    numeric,
                    matrix.s.len(),
                    1,
                    &mut matrix.s,
                    &matrix.config,
                )?;
            }
            Self::Blas(matrix) => {
                let lu = matrix
                    .lu
                    .as_mut()
                    .ok_or(SimulationError::BlasLUNotFactorized)?;
                let x = lu.solve(&matrix.s)?;
                matrix.s = x;
            }
        }
        Ok(())
    }
}
