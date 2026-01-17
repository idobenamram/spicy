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

use std::cmp::max;

use crate::solver::{
    klu::{KluConfig, KluOrdering, KluResult, KluSymbolic, amd::amd, btf::btf, klu_valid},
    matrix::{
        Dim,
        csc::{CscMatrix, CscPointers},
    },
    utils::{EMPTY, inverse_permutation, unflip},
};

pub fn allocate_symbolic(a: &CscMatrix) -> KluSymbolic {
    debug_assert!(a.is_square(), "Klu analyze only supports square matrices");
    debug_assert!(
        a.check_invariants().is_ok(),
        "Klu analyze only supports valid CSC matrices"
    );
    let n = a.dim.ncols;
    let mut row_permutation = vec![-1; n];
    for col in 0..n {
        let start = a.col_start(col);
        let end = a.col_end(col);
        for p in start..end {
            let row = a.row_index(p);
            debug_assert!(row < n);
            row_permutation[row] = col as isize;
        }
    }

    let column_permutation = vec![0; n];
    let block_boundaries = vec![0; n + 1];
    let lower_nz = vec![0.0; n];

    KluSymbolic {
        ordering: KluOrdering::Amd,
        n,
        nz: a.nnz(),
        nzoff: 0,
        nblocks: 0,
        maxblock: 0,
        structural_rank: 0,
        symmetry: 0.0,
        lnz: 0.0,
        unz: 0.0,
        lower_nz,
        row_permutation,
        column_permutation,
        block_boundaries,
    }
}

fn analyze_worker(
    a: &CscMatrix,
    _config: &KluConfig,
    symbolic: &mut KluSymbolic,
    btf_row_permutation: Vec<isize>,
    btf_column_permutation: Vec<isize>,

    // for each scc block
    mut block_row_permutation: Vec<isize>,
    mut block_col_pointers: Vec<usize>,
    mut block_row_pointers: Vec<usize>,
    _ci_len: usize,
    mut row_inv_permutations: Vec<isize>,
) {
    let n = symbolic.n;

    // TODO: this doesn't have to happen in this funciton tbh
    // compute row permutation inverse
    inverse_permutation(n, &btf_row_permutation, &mut row_inv_permutations);

    let mut nzoff = 0;
    let mut lnz = 0.;
    let mut max_nz = 0;
    symbolic.symmetry = -1.0;

    for block in 0..symbolic.nblocks {
        // the block is from rows/columns k1 to k2-1
        let k1 = symbolic.block_boundaries[block];
        let k2 = symbolic.block_boundaries[block + 1];
        let size = k2 - k1;

        symbolic.lower_nz[block] = -1.0;
        let mut pc = 0;
        for k in k1..k2 {
            let newcol = k - k1;
            block_col_pointers[newcol] = pc;
            let old_col = btf_column_permutation[k];
            debug_assert!(old_col >= 0 && old_col < n as isize);
            let start = a.col_start(old_col as usize);
            let end = a.col_end(old_col as usize);

            for p in start..end {
                // a.row_index(p) holds the old row
                // to get the new row we use the inverse of the row permutation
                let mut new_row = row_inv_permutations[a.row_index(p)] as usize;
                if new_row < k1 {
                    // ignore entries outside the square block
                    nzoff += 1;
                } else {
                    debug_assert!(new_row < k2);
                    new_row -= k1;
                    block_row_pointers[pc] = new_row;
                    pc += 1;
                }
            }
        }
        block_col_pointers[size] = pc;
        max_nz = std::cmp::max(max_nz, pc);
        debug_assert!(klu_valid(size, &block_col_pointers, &block_row_pointers));

        let lnz1;
        if size <= 3 {
            for k in 0..size {
                block_row_permutation[k] = k as isize;
            }
            lnz1 = size as f64 * (size as f64 + 1.) / 2.;
        } else if symbolic.ordering == KluOrdering::Amd {
            let block_ptrs = CscPointers::new(
                Dim {
                    nrows: size,
                    ncols: size,
                },
                &block_col_pointers[..(size + 1)],
                &block_row_pointers[..pc],
            );

            let info = amd(block_ptrs, &mut block_row_permutation[..size]);
            lnz1 = info.lnz + size as f64;
        } else {
            todo!()
        }

        symbolic.lower_nz[block] = lnz1;
        lnz += lnz1;

        // combine the preordering with the btf ordering
        for k in 0..size {
            debug_assert!(k + k1 < n);
            debug_assert!(block_row_permutation[k] as usize + k1 < n);
            symbolic.column_permutation[k + k1] =
                btf_column_permutation[block_row_permutation[k] as usize + k1];
            symbolic.row_permutation[k + k1] =
                btf_row_permutation[block_row_permutation[k] as usize + k1];
        }
    }

    debug_assert!(nzoff <= a.nnz());
    symbolic.lnz = lnz;
    symbolic.unz = lnz;
    symbolic.nzoff = nzoff;
}

// a was already is validated by the caller to be a valid CSC matrix
pub fn analyze(a: &CscMatrix, config: &KluConfig) -> KluResult<KluSymbolic> {
    let mut symbolic = allocate_symbolic(a);
    symbolic.ordering = config.ordering;

    let ci_len = match symbolic.ordering {
        KluOrdering::Amd => symbolic.n + 1,
    };

    // allocate memory for btf
    let mut btf_row_permutation = vec![0; symbolic.n];
    // btf_max_transversal expects the match array to start at -1 (unmatched).
    let mut btf_column_permutation = vec![-1; symbolic.n];
    let number_of_scc_blocks;
    let mut maxblock;

    if config.btf {
        let (number_of_matches, scc_blocks) = btf(
            a,
            &mut btf_row_permutation,
            &mut btf_column_permutation,
            &mut symbolic.block_boundaries,
        );
        number_of_scc_blocks = scc_blocks;
        symbolic.structural_rank = number_of_matches;

        // unflip the column permutation if the matrix is structurally singular
        if symbolic.structural_rank < symbolic.n {
            for col in 0..symbolic.n {
                symbolic.column_permutation[col] = unflip(symbolic.column_permutation[col]);
            }
        }

        maxblock = 1;
        for b in 0..number_of_scc_blocks {
            let k1 = symbolic.block_boundaries[b] as usize;
            let k2 = symbolic.block_boundaries[b + 1] as usize;
            let size = k2 - k1;
            maxblock = std::cmp::max(maxblock, size);
        }
    } else {
        number_of_scc_blocks = 1;
        maxblock = symbolic.n;
        symbolic.block_boundaries[0] = 0;
        symbolic.block_boundaries[1] = symbolic.n;
        for i in 0..symbolic.n {
            btf_row_permutation[i] = i as isize;
            btf_column_permutation[i] = i as isize;
        }
    }

    symbolic.nblocks = number_of_scc_blocks;
    symbolic.maxblock = maxblock;

    let pblk = vec![0; maxblock];
    let cp = vec![0; maxblock + 1];
    let ci = vec![0; max(ci_len, symbolic.nz + 1)];
    let pinv = vec![EMPTY; symbolic.n];

    analyze_worker(
        a,
        config,
        &mut symbolic,
        btf_row_permutation,
        btf_column_permutation,
        pblk,
        cp,
        ci,
        ci_len,
        pinv,
    );

    Ok(symbolic)
}
