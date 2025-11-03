
use thiserror::Error;
use crate::solver::matrix::error::MatrixError;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum SolverError {
    #[error(transparent)]
    Matrix(#[from] MatrixError),
}

