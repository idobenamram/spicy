pub mod csc;
pub mod builder;
pub mod csr;
pub mod error;

/// Compressed Sparse Column
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dim {
    pub nrows: usize,
    pub ncols: usize,
}