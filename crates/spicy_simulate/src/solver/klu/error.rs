// SPDX-License-Identifier: LGPL-2.1-or-later
//
// This module defines structured errors for the Rust port of SuiteSparse KLU.

use crate::solver::utils::SolverOverflowError;

#[derive(Debug, thiserror::Error)]
pub enum KluError {
    // --- Input validation ---
    #[error("leading dimension of B must be >= n (d={d}, n={n})")]
    InvalidLeadingDimension { d: usize, n: usize },

    #[error(
        "B too small: need at least d*nrhs = {required} entries (d={d}, nrhs={nrhs}), got {actual}"
    )]
    RhsTooSmall {
        required: usize,
        d: usize,
        nrhs: usize,
        actual: usize,
    },

    // --- Matrix properties ---
    #[error("KLU only supports square matrices (nrows={nrows}, ncols={ncols})")]
    NonSquareMatrix { nrows: usize, ncols: usize },

    #[error("matrix is structurally singular")]
    StructurallySingular,

    #[error("singular matrix at block {block}")]
    SingularAtBlock { block: usize },

    // --- Data/layout issues ---
    #[error("duplicate entry at column {col}, row {row}")]
    DuplicateEntry { col: usize, row: usize },

    // --- Sizing/overflow ---
    #[error("overflow: {0}")]
    Overflow(#[from] SolverOverflowError),

    // --- Capacity/limits ---
    #[error("problem too large: {context}")]
    TooLarge { context: &'static str },
}

pub type KluResult<T> = Result<T, KluError>;

impl KluError {
    pub(crate) fn overflow(context: &'static str) -> Self {
        Self::Overflow(SolverOverflowError::Overflow { context })
    }
}
