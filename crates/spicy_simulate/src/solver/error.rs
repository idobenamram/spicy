
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SolverError {
    #[error(transparent)]
    Csc(#[from] CscError),
}

#[derive(Debug, Error)]
pub enum CscError {
    #[error("out of bounds index: {index} (max: {max})")]
    OutOfBoundsIndex { index: usize, max: usize },

    #[error("invalid column pointers length: {expected} (actual: {actual})")]
    InvalidColumnPointersLength { expected: usize, actual: usize },

    #[error("invalid column pointers: {index} (expected: {expected}, actual: {actual})")]
    InvalidColumnPointers { index: usize, expected: usize, actual: usize },

    #[error("row indices values length mismatch: {values} (actual: {row_indices})")]
    RowIndicesValuesLengthMismatch { values: usize, row_indices: usize },

    #[error("rows not strictly increasing: {index} (expected: {expected}, actual: {actual})")]
    RowsNotStrictlyIncreasing { index: usize, expected: usize, actual: usize },
}
