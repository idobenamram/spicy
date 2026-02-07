use thiserror::Error;

use crate::solver::{klu, matrix::error::CscError};

#[derive(Debug, Error)]
pub enum SimulationError {
    #[error(transparent)]
    Matrix(#[from] CscError),

    #[error(transparent)]
    KluError(#[from] klu::KluError),

    #[error(transparent)]
    NdarrayLinalgError(#[from] ndarray_linalg::error::LinalgError),

    #[error("Klu symbolic not analyzed")]
    KLUSymbolicNotAnalyzed,

    #[error("Klu numeric not factorized")]
    KluNumericNotFactorized,

    #[error("Blas LU not factorized")]
    BlasLUNotFactorized,

    #[error("Newton iteration did not converge (time={time:?}, iters={iters})")]
    NonConvergence {
        time: Option<f64>,
        iters: usize,
    },
}
