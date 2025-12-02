use std::{cmp::max, iter::Empty};

use crate::solver::{
    klu::{KluConfig, KluOrdering, KluSymbolic, btf::btf, klu_valid},
    matrix::csc::CscMatrix,
    utils::{EMPTY, unflip},
};

pub fn allocate_symbolic(a: &CscMatrix) -> KluSymbolic {
    assert!(a.is_square(), "Klu analyze only supports square matrices");
    let n = a.dim.ncols;
    let mut row_permutation = vec![-1; n];
    for col in 0..n {
        let start = a.col_start(col);
        let end = a.col_end(col);
        for i in start..end {
            row_permutation[i] = col as isize;
        }
    }

    let column_permutation = vec![0; n];
    let row_scaling = vec![0; n + 1];
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
        row_scaling,
    }
}

fn analyze_worker(
    a: &CscMatrix,
    config: &KluConfig,
    symbolic: &mut KluSymbolic,
    btf_row_permutation: &[isize],
    btf_column_permutation: &[isize],

    block_row_permutation: &mut [isize],
    block_col_pointers: &mut [usize],
    block_row_pointers: &mut [usize],
    ci_len: usize,
    row_inv_permutations: &mut [isize],
) {
    let n = symbolic.n;

    // TODO: this doesn't have to happen in this funciton tbh
    // compute row permutation inverse
    for k in 0..n {
        assert!(btf_row_permutation[k] >= 0 && btf_row_permutation[k] < n as isize);
        row_inv_permutations[btf_row_permutation[k] as usize] = k as isize;
    }

    for k in 0..n {
        assert!(row_inv_permutations[k] != EMPTY);
    }

    let mut nzoff = 0;
    let mut lnz = 0;
    let mut max_nz = 0;
    symbolic.symmetry = -1.0;

    for block in 0..symbolic.nblocks {
        // the block is from rows/columns k1 to k2-1
        let k1 = symbolic.row_scaling[block] as usize;
        let k2 = symbolic.row_scaling[block + 1] as usize;
        let size = k2 - k1;

        symbolic.lower_nz[block] = -1.0;
        let mut pc = 0;
        for k in k1..k2 {
            let newcol = k - k1;
            block_col_pointers[newcol] = pc;
            let old_col = btf_column_permutation[k];
            assert!(old_col >= 0 && old_col < n as isize);
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
                    assert!(new_row < k2);
                    new_row = new_row - k1;
                    block_row_pointers[pc] = new_row;
                    pc += 1;
                }
            }
        }
        block_col_pointers[size] = pc;
        max_nz = std::cmp::max(max_nz, pc);
        assert!(klu_valid(size, block_col_pointers, block_row_pointers));

        let mut lnz1 = 0;
        let ok = true;
        if size <= 3 {
            for k in 0..size {
                block_row_permutation[k] = k as isize;
            }
            lnz = size * (size + 1) / 2;
        } else if symbolic.ordering == KluOrdering::Amd {

            amd()


        } else {
            todo!()
        }

    }
}

// a was already is validated by the caller to be a valid CSC matrix
pub fn analyze(a: &CscMatrix, config: &KluConfig) -> Result<(), String> {
    let mut symbolic = allocate_symbolic(a);
    symbolic.ordering = config.ordering;

    let ci_len = match symbolic.ordering {
        KluOrdering::Amd => symbolic.n + 1,
    };

    // allocate memory for btf
    let mut btf_row_permutation = vec![0; symbolic.n];
    let mut btf_column_permutation = vec![0; symbolic.n];
    let number_of_scc_blocks;
    let mut maxblock;

    if config.btf {
        let (number_of_matches, scc_blocks) = btf(
            a,
            &mut btf_row_permutation,
            &mut btf_column_permutation,
            &mut symbolic.row_scaling,
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
            let k1 = symbolic.row_scaling[b] as usize;
            let k2 = symbolic.row_scaling[b + 1] as usize;
            let size = k2 - k1;
            maxblock = std::cmp::max(maxblock, size);
        }
    } else {
        number_of_scc_blocks = 1;
        maxblock = symbolic.n;
        symbolic.row_scaling[0] = 0;
        symbolic.row_scaling[1] = symbolic.n as isize;
        for i in 0..symbolic.n {
            btf_row_permutation[i] = i as isize;
            btf_column_permutation[i] = i as isize;
        }
    }

    symbolic.nblocks = number_of_scc_blocks;
    symbolic.maxblock = maxblock;

    let mut pblk = vec![0; maxblock];
    let mut cp = vec![0; maxblock + 1];
    let mut ci = vec![0; max(ci_len, symbolic.nz + 1)];
    let mut pinv = vec![EMPTY; symbolic.n];

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
    todo!()
}
