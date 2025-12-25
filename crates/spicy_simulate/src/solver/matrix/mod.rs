pub mod builder;
pub mod csc;
pub mod csr;
pub mod error;
pub mod mtx;
pub mod slice;

/// Compressed Sparse Column
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dim {
    pub nrows: usize,
    pub ncols: usize,
}
