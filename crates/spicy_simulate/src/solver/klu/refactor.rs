// SPDX-License-Identifier: LGPL-2.1-or-later
//
// This file is based on the SuiteSparse KLU implementation by Timothy A. Davis
// and Ekanathan Palamadai.
//
// KLU, Copyright (c) 2004-2024, University of Florida.  All Rights Reserved.
// Authors: Timothy A. Davis and Ekanathan Palamadai.
//
// Modifications/porting for this project:
// Copyright (c) 2025 Ido Ben Amram

use crate::solver::{
    klu::{
        get_pointers_to_lu, get_pointers_to_lu_mut, klu_valid, scale::scale, KluConfig, KluError,
        KluNumeric, KluResult, KluSymbolic,
    },
    matrix::csc::CscMatrix,
    utils::dunits,
};

pub fn refactor(
    a: &CscMatrix,
    symbolic: &mut KluSymbolic,
    numeric: &mut KluNumeric,
    config: &KluConfig,
) -> KluResult<()> {
    let n = symbolic.n;
    let nblocks = symbolic.nblocks;
    let maxblock = symbolic.maxblock;
    let nzoff = symbolic.nzoff;

    // Refactorization can change numerical singularity; reset the singular metrics.
    numeric.metrics.numerical_rank = None;
    numeric.metrics.singular_col = None;

    // Mirror SuiteSparse KLU behavior: refactorization may enable/disable scaling
    // relative to the initial factorization.
    match config.scale {
        None => {
            numeric.rs = None;
        }
        Some(_) => {
            if numeric.rs.is_none() {
                numeric.rs = Some(vec![0.0; n]);
            }
        }
    }

    // compute row scaling.
    // IMPORTANT: do NOT use `numeric.pnum` as a scratch workspace here; it must
    // remain intact (it is the final pivot permutation).
    if let Some(rs) = numeric.rs.as_mut() {
        scale(a, rs, None, config.scale)?;
    }

    // clear workspace X
    for k in 0..maxblock {
        numeric.work[k] = 0.0;
    }

    let mut poff = 0;

    // factor each block
    for block in 0..nblocks {
        let k1 = symbolic.block_boundaries[block];
        let k2 = symbolic.block_boundaries[block + 1];
        let nk = k2 - k1;

        if nk == 1 {
            // singleton case
            let oldcol = symbolic.column_permutation[k1] as usize;
            let start = a.col_start(oldcol);
            let end = a.col_end(oldcol);

            let mut s = 0.0;
            for p in start..end {
                let oldrow = a.row_index(p);
                let newrow = numeric.pinv[oldrow] - k1 as isize;
                if newrow < 0 && poff < nzoff {
                    // entry in off-diagonal block
                    // offx[poff] = a.value(p) / rs[oldrow]
                    let val = match &numeric.rs {
                        None => a.value(p),
                        Some(rs) => a.value(p) / rs[oldrow],
                    };
                    debug_assert!(
                        numeric.offi.get(poff).copied() == Some(numeric.pinv[oldrow] as usize),
                        "off-diagonal entry order mismatch at poff={}",
                        poff
                    );
                    numeric.offx[poff] = val;
                    poff += 1;
                } else {
                    // singleton
                    // s = a.value(p) / rs[oldrow]
                    s = match &numeric.rs {
                        None => a.value(p),
                        Some(rs) => a.value(p) / rs[oldrow],
                    };
                }
            }
            numeric.u_diag[k1] = s;
            if s == 0.0 && numeric.metrics.numerical_rank.is_none() {
                numeric.metrics.numerical_rank = Some(k1);
                numeric.metrics.singular_col = Some(oldcol);
            }
        } else {
            // construct and factor the kth block

            let (_, lip_after) = numeric.lip.split_at(k1);
            let (_, llen_after) = numeric.llen.split_at(k1);
            let (_, uip_after) = numeric.uip.split_at(k1);
            let (_, ulen_after) = numeric.ulen.split_at(k1);
            let lu = numeric.lu_bx[block].as_mut();
            let x = numeric.work.as_mut_slice();

            for k in 0..nk {
                // scatter kth column of the block into workspace X
                let oldcol = symbolic.column_permutation[k + k1] as usize;
                let start = a.col_start(oldcol);
                let end = a.col_end(oldcol);

                for p in start..end {
                    let oldrow = a.row_index(p);
                    let newrow = numeric.pinv[oldrow] - k1 as isize;
                    if newrow < 0 && poff < nzoff {
                        // entry in off-diagonal block
                        // offx[poff] = a.value(p) / rs[oldrow]
                        let val = match &numeric.rs {
                            None => a.value(p),
                            Some(rs) => a.value(p) / rs[oldrow],
                        };
                        debug_assert!(
                            numeric.offi.get(poff).copied()
                                == Some(numeric.pinv[oldrow] as usize),
                            "off-diagonal entry order mismatch at poff={}",
                            poff
                        );
                        numeric.offx[poff] = val;
                        poff += 1;
                    } else {
                        // singleton
                        // s = a.value(p) / rs[oldrow]
                        let val = match &numeric.rs {
                            None => a.value(p),
                            Some(rs) => a.value(p) / rs[oldrow],
                        };
                        x[newrow as usize] = val;
                    }
                }

                // compute keth column of U, and update keth column of A

                // NOTE: we intentionally avoid holding a long-lived immutable borrow of
                // `lu` (for Ui) at the same time as a mutable borrow (for Li) to satisfy
                // the Rust borrow checker.  `u_len` is the logical length of column k of U.
                let ulen = dunits::<usize>(ulen_after[k])?;
                for up in 0..ulen {
                    // the block scope is so the immutable borrow of `lu`
                    // does not overlap with the mutable borrow we take for L below.
                    let (j, ujk) = {
                        let (ui, ux, _) = get_pointers_to_lu_mut(lu, uip_after, ulen_after, k)?;
                        let j = ui[up];
                        let ujk = x[j];
                        x[j] = 0.0;
                        ux[up] = ujk;
                        (j, ujk)
                    };
                    let (li, lx, llen) = get_pointers_to_lu(lu, lip_after, llen_after, j)?;
                    for p in 0..llen {
                        let i = li[p];
                        let val = lx[p];
                        x[i] -= val * ujk;
                    }
                }
                // get the diagonal entry of u
                let ukk = x[k];
                x[k] = 0.0;
                if ukk == 0.0 {
                    // matrix is numerically singular
                    if numeric.metrics.numerical_rank.is_none() {
                        numeric.metrics.numerical_rank = Some(k + k1);
                        numeric.metrics.singular_col = Some(oldcol);
                    }
                    if config.halt_if_singular {
                        return Err(KluError::SingularAtBlock { block });
                    }
                }
                numeric.u_diag[k + k1] = ukk;
                // gather and divide by pibot to get kth column of L
                let (li, lx, llen) = get_pointers_to_lu_mut(lu, lip_after, llen_after, k)?;
                for p in 0..llen {
                    let i = li[p];
                    lx[p] = x[i] / ukk;
                    x[i] = 0.0;
                }
            }
        }
    }

    // permute the rwo scaling according to the pivtol row order
    match &mut numeric.rs {
        None => {}
        Some(rs) => {
            for k in 0..n {
                numeric.work[k] = rs[numeric.pnum[k] as usize];
            }
            for k in 0..n {
                rs[k] = numeric.work[k];
            }
        }
    }

    debug_assert!(numeric.offp[n] == poff);
    debug_assert!(symbolic.nzoff == poff);
    debug_assert!(klu_valid(n, &numeric.offp, &numeric.offi));

    Ok(())
}
