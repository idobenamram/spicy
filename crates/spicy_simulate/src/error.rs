use thiserror::Error;

use crate::solver::{klu, matrix::error::CscError};

#[derive(Debug, Error)]
pub enum SimulationError {
    #[error(transparent)]
    Matrix(#[from] CscError),

    #[error("missing sparsity-pattern entry at (row={row}, col={col})")]
    MissingPatternEntry { row: usize, col: usize },

    #[error(transparent)]
    KluError(#[from] klu::KluError),

    #[error(transparent)]
    NdarrayLinalgError(#[from] ndarray_linalg::error::LinalgError),

    #[error("symbolic not analyzed")]
    SymbolicNotAnalyzed,

    #[error("numeric not factorized")]
    NumericNotFactorized,
}

