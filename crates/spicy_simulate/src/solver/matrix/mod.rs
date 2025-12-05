pub mod builder;
pub mod csc;
pub mod csr;
pub mod error;

/// Compressed Sparse Column
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dim {
    pub nrows: usize,
    pub ncols: usize,
}
