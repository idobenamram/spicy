use thiserror::Error;

#[derive(Debug, Error)]
pub enum MatrixError {
    #[error(transparent)]
    Csc(#[from] CscError),
    #[error(transparent)]
    Csr(#[from] CsrError),
    #[error(transparent)]
    MatrixMarket(#[from] MatrixMarketError),
}

#[derive(Debug, Error)]
pub enum MatrixMarketError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("invalid MatrixMarket banner: {0}")]
    InvalidBanner(String),

    #[error("unsupported MatrixMarket type: {0}")]
    UnsupportedType(String),

    #[error("invalid MatrixMarket size line: {0}")]
    InvalidSizeLine(String),

    #[error("invalid MatrixMarket entry at line {line}: {msg}")]
    InvalidEntry { line: usize, msg: String },

    #[error("expected {expected} entries but found {actual}")]
    EntryCountMismatch { expected: usize, actual: usize },
}

#[derive(Debug, Error)]
pub enum CscError {
    #[error("out of bounds index: {index} (max: {max})")]
    OutOfBoundsIndex { index: usize, max: usize },

    #[error("invalid column pointers length: {expected} (actual: {actual})")]
    InvalidColumnPointersLength { expected: usize, actual: usize },

    #[error("invalid column pointers: {index} (expected: {expected}, actual: {actual})")]
    InvalidColumnPointers {
        index: usize,
        expected: usize,
        actual: usize,
    },

    #[error("row indices values length mismatch: {values} (actual: {row_indices})")]
    RowIndicesValuesLengthMismatch { values: usize, row_indices: usize },

    #[error("rows not strictly increasing: {index} (expected: {expected}, actual: {actual})")]
    RowsNotStrictlyIncreasing {
        index: usize,
        expected: usize,
        actual: usize,
    },
}

#[derive(Debug, Error)]
pub enum CsrError {
    #[error("out of bounds index: {index} (max: {max})")]
    OutOfBoundsIndex { index: usize, max: usize },

    #[error("invalid row pointers length: {expected} (actual: {actual})")]
    InvalidRowPointersLength { expected: usize, actual: usize },

    #[error("invalid row pointers: {index} (expected: {expected}, actual: {actual})")]
    InvalidRowPointers {
        index: usize,
        expected: usize,
        actual: usize,
    },

    #[error("column indices values length mismatch: {values} (actual: {column_indices})")]
    ColumnIndicesValuesLengthMismatch {
        values: usize,
        column_indices: usize,
    },

    #[error("columns not strictly increasing: {index} (expected: {expected}, actual: {actual})")]
    ColumnsNotStrictlyIncreasing {
        index: usize,
        expected: usize,
        actual: usize,
    },
}
