use ndarray::{Array1, Array2, OwnedRepr};
use ndarray_linalg::{Factorize, LUFactorized};
use spicy_parser::netlist_types::{CurrentBranchIndex, NodeIndex};
use spicy_parser::node_mapping::NodeMapping;

use crate::{
    devices::Devices,
    error::SimulationError,
    setup_pattern::setup_pattern,
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
    pub fn new(matrix: CscMatrix, s: Vec<f64>, node_mapping: NodeMapping) -> Self {
        Self {
            config: KluConfig::default(),
            symbolic: None,
            numeric: None,
            matrix,
            s,
            node_mapping,
        }
    }
}

pub enum SolverMatrix {
    KLU(KluMatrix),
    BLAS(BlasMatrix),
}

impl SolverMatrix {
    pub fn create_matrix(
        devices: &mut Devices,
        node_mapping: NodeMapping,
        klu: bool,
    ) -> Result<SolverMatrix, SimulationError> {
        let matrix_dim = node_mapping.mna_matrix_dim();

        let sm = if klu {
            let matrix = setup_pattern(devices, &node_mapping)?;
            // KLU solve overwrites RHS in-place, so we allocate it up-front.
            Self::KLU(KluMatrix::new(matrix, vec![0.0; matrix_dim], node_mapping))
        } else {
            Self::BLAS(BlasMatrix::new(matrix_dim, node_mapping))
        };

        Ok(sm)
    }

    pub fn get_mut_nnz(&mut self, nnz: usize) -> &mut f64 {
        match self {
            Self::KLU(matrix) => matrix.matrix.get_mut_nnz(nnz),
            Self::BLAS(_matrix) => {
                todo!()
            }
        }
    }

    pub fn get_mut_rhs(&mut self, index: usize) -> &mut f64 {
        match self {
            Self::KLU(matrix) => &mut matrix.s[index],
            Self::BLAS(matrix) => &mut matrix.s[index],
        }
    }

    /// Current RHS vector (overwritten with solution after `solve()`).
    pub fn rhs(&self) -> &[f64] {
        match self {
            Self::KLU(matrix) => matrix.s.as_slice(),
            Self::BLAS(matrix) => matrix
                .s
                .as_slice()
                .expect("BLAS RHS should be contiguous"),
        }
    }

    /// Mutable RHS vector (overwritten with solution after `solve()`).
    pub fn rhs_mut(&mut self) -> &mut [f64] {
        match self {
            Self::KLU(matrix) => matrix.s.as_mut_slice(),
            Self::BLAS(matrix) => matrix
                .s
                .as_slice_mut()
                .expect("BLAS RHS should be contiguous"),
        }
    }


    pub fn mna_node_index(&self, node_index: NodeIndex) -> Option<usize> {
        match self {
            Self::KLU(matrix) => matrix.node_mapping.mna_node_index(node_index),
            Self::BLAS(matrix) => matrix.node_mapping.mna_node_index(node_index),
        }
    }
    pub fn mna_branch_index(&self, branch_index: CurrentBranchIndex) -> usize {
        match self {
            Self::KLU(matrix) => matrix.node_mapping.mna_branch_index(branch_index),
            Self::BLAS(matrix) => matrix.node_mapping.mna_branch_index(branch_index),
        }
    }

    pub fn analyze(&mut self) -> Result<(), SimulationError> {
        match self {
            Self::KLU(matrix) => {
                let symbolic = klu::analyze(&matrix.matrix, &matrix.config)?;
                matrix.symbolic = Some(symbolic);
            }
            // no analyze phase for blas
            Self::BLAS(_) => {}
        }
        Ok(())
    }

    pub fn factorize(&mut self) -> Result<(), SimulationError> {
        match self {
            Self::KLU(matrix) => {
                if matrix.symbolic.is_none() {
                    return Err(SimulationError::SymbolicNotAnalyzed);
                }
                let symbolic = matrix.symbolic.as_mut().unwrap();
                let numeric = klu::factor(&matrix.matrix, symbolic, &mut matrix.config)?;
                matrix.numeric = Some(numeric);
            }
            Self::BLAS(matrix) => {
                let lu = matrix.m.factorize()?;
                matrix.lu = Some(lu);
            }
        }
        Ok(())
    }

    pub fn solve(&mut self) -> Result<(), SimulationError> {
        match self {
            Self::KLU(matrix) => {
                if matrix.symbolic.is_none() {
                    return Err(SimulationError::SymbolicNotAnalyzed);
                }

                if matrix.numeric.is_none() {
                    return Err(SimulationError::NumericNotFactorized);
                }
                let symbolic = matrix.symbolic.as_ref().unwrap();
                let numeric = matrix.numeric.as_mut().unwrap();

                klu::solve(
                    symbolic,
                    numeric,
                    matrix.s.len(),
                    1,
                    &mut matrix.s,
                    &matrix.config,
                )?;
            }
            Self::BLAS(_matrix) => {
                todo!()
            }
        }
        Ok(())
    }
}
