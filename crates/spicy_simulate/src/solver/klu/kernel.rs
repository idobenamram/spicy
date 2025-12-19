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
        get_pointers_to_lu, get_pointers_to_lu_mut, KluConfig, KluError, KluNumericMetrics,
        KluResult,
    },
    matrix::csc::CscMatrix,
    utils::{dunits, EMPTY, f64_as_usize_slice, f64_as_usize_slice_mut, flip, unflip},
};

fn get_free_pointer(lu: &mut Vec<f64>, lup: usize) -> (&mut [f64], &mut [usize]) {
    let (before, xp) = lu.split_at_mut(lup);

    (before, unsafe { f64_as_usize_slice_mut(xp) })
}

fn get_column_pointer(lu: &mut [f64], pos: usize) -> (&[f64], &[usize]) {
    let (before, after) = lu.split_at(pos);
    (before, unsafe { f64_as_usize_slice(after) })
}

fn dfs(
    mut j: usize,
    k: usize,
    inverse_row_permutation: &mut [isize],

    llen: &[usize],
    lip: &mut [usize],

    stack: &mut [usize],

    flag: &mut [isize],
    lpend: &mut [isize],
    mut top: usize,
    lu_before: &mut [f64],
    lik: &mut [usize],
    plength: &mut usize,

    ap_pos: &mut [isize],
) -> usize {
    let mut l_length = *plength;
    let mut head: isize = 0;
    stack[0] = j;
    debug_assert!(flag[j] != k as isize);

    while head >= 0 {
        j = stack[head as usize];
        // j is pivotal
        debug_assert!(inverse_row_permutation[j] >= 0 && inverse_row_permutation[j] < k as isize);
        let jnew = inverse_row_permutation[j] as usize;

        if flag[j] != k as isize {
            // first time that j has been visited
            flag[j] = k as isize;

            // set ap_pos[head] to one past the last entry in col j to scan
            if lpend[jnew] == EMPTY {
                ap_pos[head as usize] = llen[jnew] as isize;
            } else {
                ap_pos[head as usize] = lpend[jnew];
            }
        }

        // add the adjacent nodes to the recursive stack by iterating through
        // until finding another non-visited pivotal node
        let (_, li) = get_column_pointer(lu_before, lip[jnew]);

        let mut pos = ap_pos[head as usize] - 1;
        while pos >= 0 {
            let i = li[pos as usize];
            if flag[i] != k as isize {
                // node i has not been visited yet
                if inverse_row_permutation[i] >= 0 {
                    // keep track of where we left off in the scan of the
                    // adjacency list of node j so we can restart j where we
                    // left off.
                    ap_pos[head as usize] = pos;

                    // node i is pivotal; push it onto the recursive stack
                    // and immediately break so we can recurse on node i.
                    head += 1;
                    stack[head as usize] = i;
                    break;
                } else {
                    // node i is not pivotal (no outgoing edges).
                    // Flag as visited and store directly into L,
                    // and continue with current node j.
                    flag[i] = k as isize;
                    lik[l_length] = i;
                    l_length += 1;
                }
            }
            pos -= 1;
        }

        if pos == -1 {
            // if all adjacent nodes of j are already visited, pop j from
            // recursive stack and push j onto output stack
            head -= 1;
            top -= 1;
            stack[top] = j;
        }
    }

    *plength = l_length;
    return top;
}

fn lsolve_symbolic(
    k: usize,
    a: &CscMatrix,
    col_permutation: &[isize],
    inverse_row_permutation: &mut [isize],

    stack: &mut [usize],
    flag: &mut [isize],

    lpend: &mut [isize],
    ap_pos: &mut [isize],

    lu: &mut Vec<f64>,
    lup: usize,
    llen: &mut [usize],
    lip: &mut [usize],

    k1: usize,
    psinv: &[isize], // inverse of P from symbolic factorization
) -> KluResult<usize> {
    let n = a.dim.ncols;
    let mut top = n;
    let mut l_length = 0;
    let (lu_before, lik) = get_free_pointer(lu, lup);

    // btf factorization of a [k1:k2-1, k1:k2-1]

    let kglobal = k + k1;
    let oldcol = col_permutation[kglobal];
    let start = a.col_start(oldcol as usize);
    let end = a.col_end(oldcol as usize);

    for p in start..end {
        let i = psinv[a.row_index(p)] - k1 as isize;
        // outside the block
        if i < 0 {
            continue;
        }

        if flag[i as usize] != k as isize {
            if inverse_row_permutation[i as usize] >= 0 {
                top = dfs(
                    i as usize,
                    k,
                    inverse_row_permutation,
                    llen,
                    lip,
                    stack,
                    flag,
                    lpend,
                    top,
                    lu_before,
                    lik,
                    &mut l_length,
                    ap_pos,
                );
            } else {
                // not pivotal and not flagged, flag and put in l
                flag[i as usize] = k as isize;
                lik[l_length] = i as usize;
                l_length += 1;
            }
        }
    }

    // If Llen [k] is zero, the matrix is structurally singular
    llen[k] = l_length;
    Ok(top)
}

fn construct_column(
    k: usize,
    a: &CscMatrix,
    col_permutation: &[isize],

    x: &mut [f64],

    k1: usize,
    psinv: &[isize],
    row_scaling: Option<&[f64]>,

    offp: &mut [usize],
    offi: &mut [usize],
    offx: &mut [f64],
) {
    let kglobal = k + k1;
    let mut poff = offp[kglobal];
    let oldcol = col_permutation[kglobal];
    let start = a.col_start(oldcol as usize);
    let end = a.col_end(oldcol as usize);

    for p in start..end {
        let oldrow = a.row_index(p);
        let i = psinv[oldrow] - k1 as isize;

        let val = match row_scaling {
            Some(rs) => {
                let val = a.value(p);
                val / rs[oldrow]
            }
            None => a.value(p),
        };

        if i < 0 {
            // this is an entry in the off-diagonal part
            offi[poff] = oldrow;
            offx[poff] = val;
            poff += 1;
        } else {
            // (i,k) is an entry in the block. scatter into X
            x[i as usize] = val;
        }
    }

    // start of the next col of off-diag part
    offp[kglobal + 1] = poff;
}

// Computes the numerical values of x, for the solution of Lx=b.  Note that x
// may include explicit zeros if numerical cancelation occurs.  L is assumed
// to be unit-diagonal, with possibly unsorted columns (but the first entry in
// the column must always be the diagonal entry).
fn lsolve_numeric(
    inverse_row_permutation: &[isize],
    lu: &[f64],
    stack: &[usize],
    lip: &[usize],
    top: usize,
    n: usize,
    llen: &[usize],

    // on output X [Ui [up1..up-1]] and X [Li [lp1..lp-1]]
    x: &mut [f64],
) -> KluResult<()> {
    // solve Lx=b
    for s in top..n {
        let j = stack[s];
        debug_assert!(inverse_row_permutation[j] >= 0);
        let jnew = inverse_row_permutation[j] as usize;
        let xj = x[j];
        let (li, lx, len) = get_pointers_to_lu(lu, lip, llen, jnew)?;
        debug_assert!(lip[jnew] <= lip[jnew + 1]);
        for p in 0..len {
            //X [Li [p]] -= Lx [p] * xj ; */
            x[li[p]] -= lx[p] * xj;
        }
    }

    Ok(())
}

fn lpivot(
    diag_row: usize,
    p_pivrow: &mut usize,
    p_pivot: &mut f64,
    p_abs_pivot: &mut f64,
    tol: f64,
    x: &mut [f64],
    lu: &mut Vec<f64>,
    lip: &[usize],
    llen: &mut [usize],
    k: usize,
    n: usize,

    inverse_row_permutation: &[isize],

    p_firstrow: &mut usize,
    config: &KluConfig,
) -> KluResult<bool> {
    let mut piv_row = EMPTY;
    if llen[k] == 0 {
        if config.halt_if_singular {
            return Err(KluError::StructurallySingular);
        }
        let mut firstrow = *p_firstrow;
        while firstrow < n {
            if inverse_row_permutation[firstrow] < 0 {
                // found the lowest-numbered non-pivotal row. Pick it.
                piv_row = firstrow as isize;
                break;
            }
            firstrow += 1;
        }

        debug_assert!(piv_row >= 0 && piv_row < n as isize);
        *p_pivrow = piv_row as usize;
        *p_pivot = 0.0;
        *p_abs_pivot = 0.0;
        *p_firstrow = piv_row as usize;
        return Ok(false);
    }

    let mut pdiag = EMPTY;
    let mut ppivrow = EMPTY;
    let mut abs_pivot = -1.0;
    let mut i = llen[k] - 1;
    let (li, lx, _) = get_pointers_to_lu(lu, lip, llen, k)?;
    let last_row_index = li[i];

    // decrement the length by 1
    llen[k] = i;
    let (li, lx, len) = get_pointers_to_lu_mut(lu, lip, llen, k)?;

    // look in Li [0 ..Llen [k] - 1 ] for a pivot row
    for p in 0..len {
        // gather the entry from X and store in L
        let i = li[p];
        let val = x[i];
        x[i] = 0.0;

        lx[p] = val;
        let val_abs = val.abs();

        // find the diagonal
        if i == diag_row {
            pdiag = p as isize;
        }

        // find the partial-pivoting choice
        if val_abs > abs_pivot {
            abs_pivot = val_abs;
            ppivrow = p as isize;
        }
    }

    let val_abs = x[last_row_index].abs();
    if val_abs > abs_pivot {
        abs_pivot = val_abs;
        ppivrow = EMPTY;
    }

    // compare the diagonal with the largest entry
    if last_row_index == diag_row {
        if val_abs >= tol * abs_pivot {
            abs_pivot = val_abs;
            ppivrow = EMPTY;
        }
    } else if pdiag != EMPTY {
        let val_abs = lx[pdiag as usize].abs();
        if val_abs >= tol * abs_pivot {
            abs_pivot = val_abs;
            ppivrow = pdiag;
        }
    }

    let pivot;
    if ppivrow != EMPTY {
        piv_row = li[ppivrow as usize] as isize;
        pivot = lx[ppivrow as usize];
        // overwrite the ppivrow values with last index values
        li[ppivrow as usize] = last_row_index;
        lx[ppivrow as usize] = x[last_row_index];
    } else {
        piv_row = last_row_index as isize;
        pivot = x[last_row_index];
    }
    x[last_row_index] = 0.0;

    debug_assert!(piv_row >= 0 && piv_row < n as isize);
    *p_pivrow = piv_row as usize;
    *p_pivot = pivot;
    *p_abs_pivot = abs_pivot;

    if pivot == 0.0 && config.halt_if_singular {
        return Err(KluError::StructurallySingular);
    }

    // divide L by the pivot value
    for p in 0..llen[k] {
        lx[p] /= pivot;
    }

    Ok(true)
}

fn prune(
    // lpend[j] marks symmetric pruning point for L(:,j)
    lpend: &mut [isize],

    inverse_row_permutation: &[isize],

    k: usize,
    pivrow: usize,

    lu: &mut Vec<f64>,
    lip: &[usize],
    llen: &[usize],
    uip: &[usize],
    ulen: &[usize],
) -> KluResult<()> {
    // check if any column of L can be pruned
    // NOTE: we intentionally avoid holding a long-lived immutable borrow of
    // `lu` (for Ui) at the same time as a mutable borrow (for Li) to satisfy
    // the Rust borrow checker.  `u_len` is the logical length of column k of U.
    let u_len = dunits::<usize>(ulen[k])?;
    for p in 0..u_len {
        // Get Ui only for this iteration, so the immutable borrow of `lu`
        // does not overlap with the mutable borrow we take for L below.
        let j = {
            let (ui, _, _) = get_pointers_to_lu(lu, uip, ulen, k)?;
            ui[p]
        };
        debug_assert!(j < k);
        if lpend[j] == EMPTY {
            // scan column j of L for the pivot row
            let (li, lx, l_len) = get_pointers_to_lu_mut(lu, lip, llen, j)?;
            for p2 in 0..l_len {
                if pivrow == li[p2] {
                    // this column can be pruned
                    // partition column j of L. the unit diagonal of L
                    // is not stored in the column of L.
                    let mut phead = 0;
                    let mut ptail = llen[j];
                    while phead < ptail {
                        let i = li[phead];
                        if inverse_row_permutation[i] >= 0 {
                            // leave at the head
                            phead += 1;
                        } else {
                            // swap with the tail
                            ptail -= 1;
                            li[phead] = li[ptail];
                            li[ptail] = i;
                            let x = lx[phead];
                            lx[phead] = lx[ptail];
                            lx[ptail] = x;
                        }
                    }

                    // set lpend to one past the last entry in the
                    // first part of the column of L. Entries in
                    // Li [0 ... Lpend [j]-1] are the only part of
                    // column j of L that needs to be scanned in the DFS.
                    // Lpend [j] was EMPTY; setting it >= 0 also flags
                    // column j as pruned.
                    lpend[j] = ptail as isize;
                    break;
                }
            }
        }
    }

    Ok(())
}

pub fn kernel(
    n: usize,
    a: &CscMatrix,
    col_permutation: &[isize],
    mut lusize: usize,

    // output
    inverse_row_permutation: &mut [isize],
    // row_permutation [k] = i if row i is the kth pivot row
    // TODO: can this be usize?
    row_permutation: &mut [isize],

    lu_block: &mut Vec<f64>,
    u_diag: &mut [f64],
    llen: &mut [usize],
    ulen: &mut [usize],
    lip: &mut [usize],
    uip: &mut [usize],
    lnz: &mut usize,
    unz: &mut usize,

    x: &mut [f64],

    stack: &mut [usize],
    flag: &mut [isize],
    ap_pos: &mut [isize],

    // for pruning
    lpend: &mut [isize],

    k1: usize,
    psinv: &[isize],
    row_scaling: Option<&[f64]>,

    // TODO: this is technically a csc matrix?
    offp: &mut [usize],
    offi: &mut [usize],
    offx: &mut [f64],
    metrics: &mut KluNumericMetrics,
    config: &KluConfig,
) -> KluResult<usize> {
    *lnz = 0;
    *unz = 0;

    let mut piv_row = 0;
    let mut abs_pivot = 0.0;
    let mut pivot = 0.0;
    let mut first_row = 0;

    // lu_pointer
    let mut lup = 0;

    for k in 0..n {
        x[k] = 0.0;
        flag[k] = EMPTY;
        lpend[k] = EMPTY;
    }

    // mark all rows as non-pivotal and set initial diagonal mapping
    for k in 0..n {
        row_permutation[k] = k as isize;
        inverse_row_permutation[k] = flip(k as isize);
    }

    offp[0] = 0;
    // a row is pivotal if inverse_row_permutation[row] >= 0
    // all rows are initially "flipped", and marked unflipped when
    // it becomes pivotal

    for k in 0..n {
        // (n - k) entries for L and k entries for U
        // number of rows in lower triangle goes down, upper triangle goes up.
        let col_max_size = dunits::<usize>(n - k)?
            + dunits::<usize>(k)?
            + dunits::<f64>(n - k)?
            + dunits::<f64>(k)?;

        let max_matrix_size = lup + col_max_size;
        if max_matrix_size > lusize {
            // how much to grow
            let max_size = config.memgrow * (lusize as f64) + (4 * n + 1) as f64;
            if max_size.is_infinite() {
                return Err(KluError::TooLarge {
                    context: "kernel LU workspace",
                });
            }
            let new_lusize = config.memgrow * (lusize as f64) + (2 * n + 1) as f64;
            lu_block.resize(new_lusize as usize, 0.);
            metrics.nrealloc += 1;
            lusize = new_lusize as usize;
        }

        // start the kth column of L and U
        lip[k] = lup;

        // compute the nonzero patter of the kth column and L and U
        let top = lsolve_symbolic(
            k,
            a,
            col_permutation,
            inverse_row_permutation,
            stack,
            flag,
            lpend,
            ap_pos,
            lu_block,
            lup,
            llen,
            lip,
            k1,
            psinv,
        )?;

        // get the column of the matrix to factorize and scatter into X
        construct_column(
            k,
            a,
            col_permutation,
            x,
            k1,
            psinv,
            row_scaling,
            offp,
            offi,
            offx,
        );

        // compute the numerical values of the kth column
        lsolve_numeric(
            inverse_row_permutation,
            lu_block.as_slice(),
            stack,
            lip,
            top,
            n,
            llen,
            x,
        )?;

        // partial pivoting with diagonal preference

        // determine what the "diagonal" is
        debug_assert!(row_permutation[k] >= 0 && row_permutation[k] < n as isize);
        let diag_row = row_permutation[k] as usize;

        if !lpivot(
            diag_row,
            &mut piv_row,
            &mut pivot,
            &mut abs_pivot,
            config.tol,
            x,
            lu_block,
            lip,
            llen,
            k,
            n,
            inverse_row_permutation,
            &mut first_row,
            config,
        )? {
            // Matrix is structurally (or numerically) singular, but we keep going.
            // Record the first column where a zero pivot was encountered.
            if metrics.numerical_rank.is_none() {
                metrics.numerical_rank = Some(k + k1);
                let oldcol = col_permutation[k + k1];
                debug_assert!(oldcol >= 0);
                metrics.singular_col = Some(oldcol as usize);
            }
        }

        debug_assert!(piv_row >= 0 && piv_row < n);
        debug_assert!(inverse_row_permutation[piv_row] < 0);

        let lower_col_length = dunits::<usize>(llen[k])? + dunits::<f64>(llen[k])?;

        // set the Uip pointer
        uip[k] = lip[k] + lower_col_length;

        // move the lup pointer to the position where the indices of U
        // should be stored
        lup += lower_col_length;

        ulen[k] = n - top;

        // extract Stack [top..n-1] to Ui and the values to Ux and clear X
        let (ui, ux, _) = get_pointers_to_lu_mut(lu_block, uip, ulen, k)?;
        let mut i = 0;
        for p in top..n {
            let j = stack[p];
            debug_assert!(
                inverse_row_permutation[j] >= 0 && inverse_row_permutation[j] < n as isize
            );
            ui[i] = inverse_row_permutation[j] as usize;
            ux[i] = x[j];
            x[j] = 0.0;
            i += 1;
        }

        lup += dunits::<usize>(ulen[k])? + dunits::<f64>(ulen[k])?;

        // U(k,k) = pivot
        u_diag[k] = pivot;

        // log the pivot permutation
        debug_assert!(unflip(inverse_row_permutation[diag_row]) < n as isize);
        debug_assert!(
            row_permutation[unflip(inverse_row_permutation[diag_row]) as usize]
                == diag_row as isize
        );

        if piv_row != diag_row {
            // an off-diagonal pivot has been chosen
            metrics.noffdiag += 1;

            if inverse_row_permutation[diag_row as usize] < 0 {
                // the former diagonal row index, diagrow, has not yet been
                // chosen as a pivot row. Log this diagrow as the "diagonal"
                // entry in the column kbar for which the chosen pivot row,
                // pivrow, was originally logged as the "diagonal"
                let kbar = flip(inverse_row_permutation[piv_row as usize]);
                row_permutation[kbar as usize] = diag_row as isize;
                inverse_row_permutation[diag_row] = flip(kbar);
            }
        }

        row_permutation[k] = piv_row as isize;
        inverse_row_permutation[piv_row] = k as isize;

        prune(
            lpend,
            inverse_row_permutation,
            k,
            piv_row,
            lu_block,
            lip,
            llen,
            uip,
            ulen,
        )?;

        *lnz += llen[k] + 1; // 1 added to lnz for diagonal
        *unz += ulen[k] + 1; // 1 added to unz for diagonal
    }

    // finalize column pointers for L and U, and put L in the pivotal order
    for p in 0..n {
        let (li, _, len) = get_pointers_to_lu_mut(lu_block, lip, llen, p)?;
        for i in 0..len {
            li[i] = inverse_row_permutation[li[i]] as usize;
        }
    }

    // shrink the LU factors to just the required size
    let new_lusize = lup;
    debug_assert!(new_lusize <= lusize);
    lu_block.resize(new_lusize as usize, 0.0);

    Ok(new_lusize as usize)
}
