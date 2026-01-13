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

use crate::solver::klu::{
    scale::scale, KluConfig, KluError, KluNumeric, KluNumericMetrics, KluResult, KluSymbolic,
};
use crate::solver::klu::{kernel, klu_valid, klu_valid_lu};
use crate::solver::matrix::csc::CscMatrix;
use crate::solver::utils::{as_usize_slice_mut, dunits, f64_as_isize_slice_mut, inverse_permutation};

pub fn allocate_klu_numeric(
    symbolic: &KluSymbolic,
    config: &KluConfig,
) -> KluResult<KluNumeric> {
    let n = symbolic.n;
    let nzoff = symbolic.nzoff;
    let nblocks = symbolic.nblocks;
    let maxblock = symbolic.maxblock;
    let n1 = n + 1;
    let nzoff1 = nzoff + 1;

    let lu_bx = vec![Vec::new(); nblocks];

    let rs = match config.scale {
        None => None,
        _ => Some(vec![0.0; n]),
    };

    /* allocate permanent workspace for factorization and solve.  Note that the
     * solver will use an Xwork of size 4n, whereas the factorization codes use
     * an Xwork of size n and integer space (Iwork) of size 6n. KLU_condest
     * uses an Xwork of size 2n.  Total size is:
     *
     *    n*sizeof(Entry) + max (6*maxblock*sizeof(Int), 3*n*sizeof(Entry))
     */
    let s = n
        .checked_mul(std::mem::size_of::<f64>())
        .ok_or(KluError::overflow("n * sizeof(f64) for workspace"))?;
    let n3 = n
        .checked_mul(3 * std::mem::size_of::<f64>())
        .ok_or(KluError::overflow("3 * n * sizeof(f64) for workspace"))?;
    let b6 = maxblock
        .checked_mul(6 * std::mem::size_of::<isize>())
        .ok_or(KluError::overflow("6 * maxblock * sizeof(isize) for workspace"))?;
    let worksize = s
        .checked_add(std::cmp::max(n3, b6))
        .ok_or(KluError::overflow("total workspace size"))?;
    let worksize_f64 = worksize.div_ceil(std::mem::size_of::<f64>());
    // allocate with f64 for alignment
    let work = vec![0.0; worksize_f64];

    let numeric = KluNumeric {
        n,
        nblocks,
        nzoff,
        lnz: 0,
        unz: 0,
        max_lnz_block: 0,
        max_unz_block: 0,
        pnum: vec![0; n],
        offp: vec![0; n1],
        offi: vec![0; nzoff1],
        offx: vec![0.0; nzoff1],

        lip: vec![0; n],
        uip: vec![0; n],
        llen: vec![0; n],
        ulen: vec![0; n],

        lu_size: vec![0; nblocks],
        lu_bx,

        u_diag: vec![0.0; n],
        rs,
        pinv: vec![0; n],

        worksize,
        work,

        metrics: KluNumericMetrics::default(),
    };

    Ok(numeric)
}

pub fn kernel_factor(
    n: usize,
    a: &CscMatrix,
    col_permutation: &[isize],
    mut lsize: f64,
    k1: usize,
    // inverse of P from symbolic factorization
    ps_inv: &[isize],
    row_scaling: Option<&[f64]>,

    // outputs
    lu_block: &mut Vec<f64>,
    u_diag: &mut [f64],
    llen: &mut [usize],
    ulen: &mut [usize],
    lip: &mut [usize],
    uip: &mut [usize],
    p: &mut [isize],
    lnz: &mut usize,
    unz: &mut usize,

    // inputs, modified on output
    offp: &mut [usize],
    offi: &mut [usize],
    offx: &mut [f64],

    // workspace
    x: &mut [f64],
    work: &mut [f64],
    metrics: &mut KluNumericMetrics,
    config: &KluConfig,
) -> KluResult<usize> {
    debug_assert!(n > 0);

    if lsize <= 0. {
        let anz = a.col_start(k1 + n) - a.col_start(k1);
        lsize = -lsize;
        lsize = lsize.max(1.0);
        lsize = lsize * (anz as f64) + (n as f64);
    }

    // TODO: overflow
    // In KLU this is (n*n + n)/2, the number of entries in a full lower/upper
    // triangle including diagonal.
    let max_lnz = ((n as f64) * (n as f64) + (n as f64)) / 2.;

    let l_size = (lsize as usize).max(n + 1).min(max_lnz as usize);
    let u_size = (lsize as usize).max(n + 1).min(max_lnz as usize);

    let work = unsafe { f64_as_isize_slice_mut(work) };
    let (pinv, work) = work.split_at_mut(n);
    let (stack, work) = work.split_at_mut(n);
    let (flag, work) = work.split_at_mut(n);
    let (lpend, work) = work.split_at_mut(n);
    let (ap_pos, _) = work.split_at_mut(n);

    let stack = unsafe { as_usize_slice_mut(stack) };
    let lusize = dunits::<isize>(l_size)?
        + dunits::<f64>(l_size)?
        + dunits::<isize>(u_size)?
        + dunits::<f64>(u_size)?;

    lu_block.resize(lusize as usize, 0.0);

    kernel::kernel(
        n,
        a,
        col_permutation,
        lusize,
        pinv,
        p,
        lu_block,
        u_diag,
        llen,
        ulen,
        lip,
        uip,
        lnz,
        unz,
        x,
        stack,
        flag,
        ap_pos,
        lpend,
        k1,
        ps_inv,
        row_scaling,
        offp,
        offi,
        offx,
        metrics,
        config,
    )
}

pub fn factor(
    a: &CscMatrix,
    symbolic: &mut KluSymbolic,
    config: &mut KluConfig,
) -> KluResult<KluNumeric> {
    config.validate()?;
    let mut numeric = allocate_klu_numeric(symbolic, config)?;

    let n = symbolic.n;
    let nzoff = symbolic.nzoff;
    let mut max_lnz_block = 1;
    let mut max_unz_block = 1;
    let mut lnz = 0;
    let mut unz = 0;

    let (x, work) = numeric.work.split_at_mut(n);
    let (work, pblock) = work.split_at_mut(5 * symbolic.maxblock);
    let pblock = unsafe { f64_as_isize_slice_mut(pblock) };

    /* compute the inverse of P from symbolic analysis.  Will be updated to
     * become the inverse of the numerical factorization when the factorization
     * is done, for use in KLU_refactor */
    inverse_permutation(n, &symbolic.row_permutation, &mut numeric.pinv);

    numeric.offp[0] = 0;

    match &mut numeric.rs {
        None => (),
        Some(rs) => {
            scale(a, rs, Some(&mut numeric.pnum), config.scale)?;
        }
    }

    for block in 0..symbolic.nblocks {
        let k1 = symbolic.block_boundaries[block];
        let k2 = symbolic.block_boundaries[block + 1];
        let block_size = k2 - k1;

        if block_size == 1 {
            // singleton case
            let mut poff = numeric.offp[k1];
            let oldcol = symbolic.column_permutation[k1] as usize;
            let start = a.col_start(oldcol);
            let end = a.col_end(oldcol);
            let mut diag_val = 0.;

            for p in start..end {
                let oldrow = a.row_index(p);
                let newrow = numeric.pinv[oldrow] as usize;
                if newrow < k1 {
                    numeric.offi[poff] = oldrow;
                    /* row scaling.  NOTE: scale factors are not yet permuted
                     * according to the pivot row permutation, so Rs [oldrow] is
                     * used below.  When the factorization is done, the scale
                     * factors are permuted, so that Rs [newrow] will be used in
                     * klu_solve, klu_tsolve, and klu_rgrowth */
                    let val = match &numeric.rs {
                        None => a.value(p),
                        Some(rs) => a.value(p) / rs[oldrow],
                    };

                    numeric.offx[poff] = val;
                    poff += 1;
                } else {
                    debug_assert!(newrow == k1);
                    let val = match &numeric.rs {
                        None => a.value(p),
                        Some(rs) => a.value(p) / rs[oldrow],
                    };
                    diag_val = val;
                }
            }

            numeric.u_diag[k1] = diag_val;
            if diag_val == 0. {
                if numeric.metrics.numerical_rank.is_none() {
                    numeric.metrics.numerical_rank = Some(k1);
                    numeric.metrics.singular_col = Some(oldcol);
                }
                if config.halt_if_singular {
                    return Err(KluError::SingularAtBlock { block });
                }
            }

            numeric.offp[k1 + 1] = poff;
            numeric.pnum[k1] = symbolic.row_permutation[k1];
            lnz += 1;
            unz += 1;
        } else {
            let lsize = if symbolic.lower_nz[block] < 0. {
                -(config.initmem)
            } else {
                config.initmem_amd * symbolic.lower_nz[block] + block_size as f64
            };
            let mut lnz_block = 0;
            let mut unz_block = 0;

            numeric.lu_size[block] = kernel_factor(
                block_size,
                a,
                &symbolic.column_permutation,
                lsize,
                k1,
                &numeric.pinv,
                numeric.rs.as_deref(),
                &mut numeric.lu_bx[block],
                &mut numeric.u_diag[k1..],
                &mut numeric.llen[k1..],
                &mut numeric.ulen[k1..],
                &mut numeric.lip[k1..],
                &mut numeric.uip[k1..],
                pblock,
                &mut lnz_block,
                &mut unz_block,
                &mut numeric.offp,
                &mut numeric.offi,
                &mut numeric.offx,
                x,
                work,
                &mut numeric.metrics,
                config,
            )?;

            debug_assert!(matches!(
                klu_valid_lu(
                    block_size,
                    true,
                    &numeric.lip[k1..],
                    &numeric.llen[k1..],
                    &numeric.lu_bx[block],
                ),
                Ok(true)
            ));
            debug_assert!(matches!(
                klu_valid_lu(
                    block_size,
                    false,
                    &numeric.uip[k1..],
                    &numeric.ulen[k1..],
                    &numeric.lu_bx[block],
                ),
                Ok(true)
            ));

            lnz += lnz_block;
            unz += unz_block;
            max_lnz_block = max_lnz_block.max(lnz_block);
            max_unz_block = max_unz_block.max(unz_block);

            if symbolic.lower_nz[block] < 0. {
                // revise estimate for subsequent factorization
                symbolic.lower_nz[block] = lnz_block.max(unz_block) as f64;
            }

            // combine the klu row ordering with the symbolic pre-ordering
            for k in 0..block_size {
                debug_assert!(k + k1 < n);
                debug_assert!(pblock[k] as usize + k1 < n);
                numeric.pnum[k + k1] = symbolic.row_permutation[pblock[k] as usize + k1] as isize;
            }

            // the local pivot row permutation Pblock is no longer needed
        }
    }

    debug_assert!(nzoff == numeric.offp[n]);
    debug_assert!(klu_valid(n, &numeric.offp, &numeric.offi));

    numeric.lnz = lnz;
    numeric.unz = unz;
    numeric.max_lnz_block = max_lnz_block;
    numeric.max_unz_block = max_unz_block;

    // compute inverse of pnum
    inverse_permutation(n, &numeric.pnum, &mut numeric.pinv);

    // permute the rwo scaling according to the pivtol row order
    match &mut numeric.rs {
        None => {}
        Some(rs) => {
            for k in 0..n {
                x[k] = rs[numeric.pnum[k] as usize];
            }
            rs[..n].copy_from_slice(&x[..n]);
        }
    }

    for p in 0..nzoff {
        debug_assert!(numeric.offi[p] < n);
        numeric.offi[p] = numeric.pinv[numeric.offi[p]] as usize;
    }

    debug_assert!(klu_valid(n, &numeric.offp, &numeric.offi));

    Ok(numeric)
}
