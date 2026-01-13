// SPDX-License-Identifier: LGPL-2.1-or-later
//
// This module is based on the SuiteSparse KLU implementation by Timothy A. Davis
// and Ekanathan Palamadai.
//
// KLU, Copyright (c) 2004-2024, University of Florida.  All Rights Reserved.
// Authors: Timothy A. Davis and Ekanathan Palamadai.
//
// Modifications/porting for this project:
// Copyright (c) 2025 Ido Ben Amram

mod amd;
mod analyze;
mod btf;
mod dump;
mod error;
mod factor;
mod kernel;
mod refactor;
mod scale;
mod solve;

use crate::solver::utils::{dunits, f64_as_usize_slice, f64_as_usize_slice_mut};
pub use analyze::analyze;
pub use dump::{
    KLU_PERM_DUMP_MAGIC, KLU_PERM_DUMP_VERSION, KLU_SOLVE_DUMP_MAGIC, KLU_SOLVE_DUMP_VERSION,
    KluPermDumpStage, write_perm_dump, write_solve_dump,
};
pub use error::{KluError, KluResult};
pub use factor::factor;
pub use refactor::refactor;
pub use solve::solve;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KluScale {
    #[allow(dead_code)]
    Sum,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KluOrdering {
    Amd,
}

#[derive(Debug, Clone, Copy)]
pub struct KluConfig {
    /* pivot tolerance for diagonal preference */
    tol: f64,
    /* realloc memory growth size for LU factors */
    memgrow: f64,
    /* init. memory size with AMD: c*nnz(L) + n */
    initmem_amd: f64,
    /* init. memory size: c*nnz(A) + n */
    initmem: f64,
    /* use BTF pre-ordering, or not */
    btf: bool,
    ordering: KluOrdering,
    scale: Option<KluScale>,
    // how to handle a singular matrix:
    // FALSE: keep going.  Return a Numeric object with a zero U(k,k).  A
    //   divide-by-zero may occur when computing L(:,k).  The Numeric object
    //   can be passed to klu_solve (a divide-by-zero will occur).  It can
    //   also be safely passed to klu_refactor.
    // TRUE: stop quickly.  klu_factor will free the partially-constructed
    //   Numeric object.  klu_refactor will not free it, but will leave the
    //   numerical values only partially defined.  This is the default.
    halt_if_singular: bool,
}

impl Default for KluConfig {
    fn default() -> Self {
        Self {
            tol: 0.001,
            memgrow: 1.2,
            initmem_amd: 1.2,
            initmem: 10.0,
            btf: true,
            ordering: KluOrdering::Amd,
            scale: Some(KluScale::Max),
            halt_if_singular: true,
        }
    }
}

impl KluConfig {
    fn validate(&mut self) -> KluResult<()> {
        self.initmem_amd = self.initmem_amd.max(1.);
        self.initmem = self.initmem.max(10.);
        self.tol = self.tol.min(1.);
        self.tol = self.tol.max(0.);
        self.memgrow = self.memgrow.max(1.);

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct KluSymbolic {
    ordering: KluOrdering,

    n: usize,
    nz: usize,
    nzoff: usize,
    nblocks: usize,
    maxblock: usize,
    structural_rank: usize,
    symmetry: f64,
    lnz: f64,
    unz: f64,

    lower_nz: Vec<f64>,
    row_permutation: Vec<isize>,
    column_permutation: Vec<isize>,
    // used in btf to hold block boundaries (n + 1)
    block_boundaries: Vec<usize>,
}

/// Statistics produced by numeric factorization/refactorization.
///
/// In the original SuiteSparse KLU implementation these live in `KLU_common`.
/// For this Rust port we keep them on the `KluNumeric` object itself.
#[derive(Debug, Clone, Default)]
pub struct KluNumericMetrics {
    /// Number of LU workspace growth reallocations performed during factorization.
    pub nrealloc: usize,
    /// Number of off-diagonal pivots selected during factorization.
    pub noffdiag: usize,
    /// First \(k\) for which a zero pivot `U(k,k)` was encountered (0-based, in the
    /// permuted system), if detected.
    pub numerical_rank: Option<usize>,
    /// Original column index in the input matrix `A` that corresponds to `numerical_rank`, if any.
    pub singular_col: Option<usize>,
}

pub struct KluNumeric {
    // A is n-by-n
    pub n: usize,
    // number of diagonal blocks
    pub nblocks: usize,
    // actual nz in L, including diagonal
    pub lnz: usize,
    // actual nz in U, including diagonal
    pub unz: usize,
    // max actual nz in L in any one block, incl. diag
    pub max_lnz_block: usize,
    // max actual nz in U in any one block, incl. diag
    pub max_unz_block: usize,
    // size n. final pivot permutation
    pub pnum: Vec<isize>,
    // size n. inverse of final pivot permutation
    pub pinv: Vec<isize>,

    // size n. pointers into LUbx[block] for L
    pub lip: Vec<usize>,
    // size n. pointers into LUbx[block] for U
    pub uip: Vec<usize>,
    // size n. Llen [k] = # of entries in kth column of L
    pub llen: Vec<usize>,
    // size n. Ulen [k] = # of entries in kth column of U
    pub ulen: Vec<usize>,
    // L and U indices and entries (excl. diagonal of U)
    pub lu_bx: Vec<Vec<f64>>,
    // size of each LUbx [block], in sizeof (f64)
    pub lu_size: Vec<usize>,
    // diagonal of U
    pub u_diag: Vec<f64>,

    // scale factors; can be NULL if no scaling
    // size n. Rs [i] is scale factor for row i
    pub rs: Option<Vec<f64>>,

    // permanent workspace for factorization and solve (size in bytes, as in C)
    pub worksize: usize,
    // single contiguous workspace buffer backing both Xwork and Iwork in C
    pub work: Vec<f64>,

    // column pointers for off-diagonal entries
    pub offp: Vec<usize>,
    // row indices for off-diagonal entries
    pub offi: Vec<usize>,
    // numerical values for off-diagonal entries
    pub offx: Vec<f64>,
    // number of off-diagonal entries
    pub nzoff: usize,

    pub metrics: KluNumericMetrics,
}

impl std::fmt::Debug for KluNumeric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // NOTE: `lu_bx` and `work` are packed buffers (indices stored inside `Vec<f64>`),
        // and dumping them would be both huge and misleading.
        //
        // For snapshot tests we keep this intentionally minimal and stable:
        // - permutations / scaling (`pnum`, `rs`)
        // - structural metrics (`lnz`, `unz`, `max_*`, `lu_size`)

        struct Preview<'a, T> {
            v: &'a [T],
            max: usize,
        }
        impl<T: std::fmt::Debug> std::fmt::Debug for Preview<'_, T> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let n = self.v.len();
                let max = self.max.max(1);
                if n <= max {
                    return f.debug_list().entries(self.v.iter()).finish();
                }
                let head = max / 2;
                let tail = max - head;
                let mut list = f.debug_list();
                list.entries(self.v.iter().take(head));
                list.entry(&format_args!(".. (len={}) ..", n));
                list.entries(self.v.iter().skip(n - tail));
                list.finish()
            }
        }
        fn pv<'a, T: std::fmt::Debug>(v: &'a [T]) -> Preview<'a, T> {
            // big enough to show full vectors for small/medium fixtures (like n=100),
            // but still bounded for very large cases.
            Preview { v, max: 512 }
        }

        let mut s = f.debug_struct("KluNumeric");
        s.field("pnum", &pv(&self.pnum))
            .field("rs", &self.rs)
            .field("lnz", &self.lnz)
            .field("unz", &self.unz)
            .field("max_lnz_block", &self.max_lnz_block)
            .field("max_unz_block", &self.max_unz_block)
            .field("lu_size", &pv(&self.lu_size));
        s.finish()
    }
}

pub(crate) fn klu_valid(n: usize, column_pointers: &[usize], row_indices: &[usize]) -> bool {
    if n == 0 {
        return false;
    }

    // column pointers must start at column_pointers[0] = 0, and column_pointers[n] must be >= 0
    if column_pointers[0] != 0 {
        return false;
    }

    for j in 0..n {
        let p1 = column_pointers[j];
        let p2 = column_pointers[j + 1];

        // column pointers must be ascending
        if p1 > p2 {
            return false;
        }
        for p in p1..p2 {
            let i = row_indices[p];
            // row index out of range
            if i >= n {
                return false;
            }
        }
    }

    true
}

pub(crate) fn get_pointers_to_lu_mut<'a>(
    lu: &'a mut [f64],
    xip: &[usize],
    xlen: &[usize],
    k: usize,
) -> KluResult<(&'a mut [usize], &'a mut [f64], usize)> {
    let (_, xp) = lu.split_at_mut(xip[k]);
    let len = dunits::<usize>(xlen[k])?;
    let (xi, xx) = xp.split_at_mut(len);

    Ok((unsafe { f64_as_usize_slice_mut(xi) }, xx, len))
}

pub(crate) fn get_pointers_to_lu<'a>(
    lu: &'a [f64],
    xip: &[usize],
    xlen: &[usize],
    k: usize,
) -> KluResult<(&'a [usize], &'a [f64], usize)> {
    let (_, xp) = lu.split_at(xip[k]);
    let len = dunits::<usize>(xlen[k])?;
    let (xi, xx) = xp.split_at(len);

    Ok((unsafe { f64_as_usize_slice(xi) }, xx, len))
}

pub(crate) fn klu_valid_lu(
    n: usize,
    flag_test_start_ptr: bool,
    xip: &[usize],
    xlen: &[usize],
    lu: &[f64],
) -> KluResult<bool> {
    if n == 0 {
        return Ok(false);
    }

    // column pointers must start at xip[0] = 0 when requested
    if flag_test_start_ptr && xip[0] != 0 {
        return Ok(false);
    }

    for j in 0..n {
        let p1 = xip[j];

        // column pointers must be ascending, if we can compare to the next one
        if j < n - 1 {
            let p2 = xip[j + 1];
            if p1 > p2 {
                return Ok(false);
            }
        }

        let (xi, _, len) = get_pointers_to_lu(lu, xip, xlen, j)?;
        for p in 0..len {
            let i = xi[p];
            if i >= n {
                // row index out of range
                return Ok(false);
            }
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::matrix::{csc::CscMatrix, mtx::load_matrix_market_csc_file};
    use rstest::rstest;
    use std::path::PathBuf;

    // TODO: clean this up
    #[allow(dead_code)]
    #[derive(Debug)]
    enum KluRunSnapshot {
        Skipped {
            reason: String,
        },
        AnalyzeError {
            error: KluError,
        },
        FactorError {
            symbolic: KluSymbolic,
            error: KluError,
        },
        SolveError {
            symbolic: KluSymbolic,
            numeric: KluNumeric,
            error: KluError,
        },
        Solved {
            symbolic: KluSymbolic,
            numeric: KluNumeric,
            max_abs_residual: f64,
        },
    }

    #[allow(dead_code)]
    #[derive(Debug)]
    struct VecPreview {
        len: usize,
        head: Vec<f64>,
        tail: Vec<f64>,
    }

    fn preview_vec(v: &[f64]) -> VecPreview {
        let len = v.len();
        if len <= 32 {
            return VecPreview {
                len,
                head: v.to_vec(),
                tail: vec![],
            };
        }
        let head = v.iter().copied().take(16).collect::<Vec<_>>();
        let tail = v.iter().copied().skip(len - 16).collect::<Vec<_>>();
        VecPreview { len, head, tail }
    }

    fn csc_matvec(a: &CscMatrix, x: &[f64]) -> Vec<f64> {
        debug_assert_eq!(a.dim.ncols, x.len());
        let mut y = vec![0.0; a.dim.nrows];
        for j in 0..a.dim.ncols {
            a.axpy_into_dense_col(j, x[j], &mut y);
        }
        y
    }

    fn fnv1a64(s: &str) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for &b in s.as_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    struct XorShift64 {
        state: u64,
    }

    impl XorShift64 {
        fn new(seed: u64) -> Self {
            Self {
                state: if seed == 0 { 0x9e3779b97f4a7c15 } else { seed },
            }
        }

        fn next_u64(&mut self) -> u64 {
            // xorshift64*
            let mut x = self.state;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.state = x;
            x.wrapping_mul(0x2545f4914f6cdd1d)
        }
    }

    fn seeded_random_b(n: usize, seed: u64) -> Vec<f64> {
        let mut rng = XorShift64::new(seed);
        let mut b = Vec::with_capacity(n);
        for _ in 0..n {
            // integer-valued RHS is deterministic and stable in debug output
            let v = (rng.next_u64() % 21) as i64 - 10; // [-10, 10]
            b.push(v as f64);
        }
        if b.iter().all(|&x| x == 0.0) && n > 0 {
            b[0] = 1.0;
        }
        b
    }

    #[rstest]
    fn snapshot_klu_fixtures(#[files("src/solver/tests/klu/*.mtx")] input: PathBuf) {
        let a = load_matrix_market_csc_file(&input).expect("load matrix market");
        a.check_invariants().expect("csc invariants");

        let n = a.dim.ncols;
        let nnz = a.nnz();
        let is_square = a.is_square();

        let fixture = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let seed = fnv1a64(&fixture) ^ ((n as u64) << 32) ^ (nnz as u64);

        let mut x = if is_square {
            seeded_random_b(n, seed)
        } else {
            vec![]
        };
        let b_original = x.clone();

        let run = if !is_square {
            KluRunSnapshot::Skipped {
                reason: "KLU only supports square matrices".to_string(),
            }
        } else {
            // deterministic config for snapshots
            let mut config = KluConfig {
                btf: false,
                scale: None,
                ..Default::default()
            };

            match analyze::analyze(&a, &config) {
                Err(error) => KluRunSnapshot::AnalyzeError { error },
                Ok(symbolic) => {
                    let mut symbolic = symbolic;
                    match factor::factor(&a, &mut symbolic, &mut config) {
                        Err(error) => KluRunSnapshot::FactorError { symbolic, error },
                        Ok(mut numeric) => match solve::solve(
                            &symbolic,
                            &mut numeric,
                            symbolic.n,
                            1,
                            &mut x,
                            &config,
                        ) {
                            Err(error) => KluRunSnapshot::SolveError {
                                symbolic,
                                numeric,
                                error,
                            },
                            Ok(()) => {
                                let ax = csc_matvec(&a, &x);
                                let max_abs_residual = ax
                                    .iter()
                                    .zip(b_original.iter())
                                    .map(|(ai, bi)| (ai - bi).abs())
                                    .fold(0.0, f64::max);
                                KluRunSnapshot::Solved {
                                    symbolic,
                                    numeric,
                                    max_abs_residual,
                                }
                            }
                        },
                    }
                }
            }
        };

        let name = format!("klu-{fixture}");
        // Keep a stable, bounded view of the output vector; it's the "x" result (or original b if failed).
        let x_preview = preview_vec(&x);

        insta::assert_debug_snapshot!(name, (n, nnz, is_square, seed, run, x_preview));
    }
}
