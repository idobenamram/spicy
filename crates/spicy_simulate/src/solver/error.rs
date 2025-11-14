use crate::solver::matrix::error::MatrixError;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum SolverError {
    #[error(transparent)]
    Matrix(#[from] MatrixError),
}
