use crate::solver::{
    btf_max_transversal::btf_max_transversal, btf_scc::btf_scc, matrix::csc::CscMatrix, utils::flip,
};



pub fn btf(
    a: &CscMatrix,
    row_permutations: &mut [isize],
    column_permutations: &mut [isize],
    blocks: &mut [isize],
) -> (usize, usize) {
    let n = a.dim.ncols;
    let number_of_matches = btf_max_transversal(a, column_permutations);

    // complete permutation if the matrix is structurally singular
    // since the matrix is square, ensure unflip(column_permutations[0..n-1]) is a
    // permutation of the columns of A so that A has as many nonzeros on the
    // diagonal as possible.
    if number_of_matches < n {
        // TODO: technically we could have used the allocations in btf_max_transversal here
        //       which would have saved some allocations
        let mut flag = vec![0; n];

        // flag matched columns
        for col in 0..n {
            let j = column_permutations[col];
            if j != -1 {
                // row col and j are matched
                flag[j as usize] = 1;
            }
        }

        let mut nbadcol = 0;
        // TODO: allocations
        let mut unmatched = vec![0; n];
        // make a list of all the unmatched columns
        for j in (0..=(n - 1)).rev() {
            if flag[j] == 0 {
                unmatched[nbadcol] = j;
                nbadcol += 1;
            }
        }
        assert!(nbadcol + number_of_matches == n);

        // assign for each unmatched row
        for col in 0..n {
            if column_permutations[col] == -1 && nbadcol > 0 {
                let j = unmatched[nbadcol];
                nbadcol -= 1;
                column_permutations[col] = flip(j as isize);
            }
        }
    }

    let number_of_scc_blocks = btf_scc(a, column_permutations, row_permutations, blocks);

    (number_of_matches, number_of_scc_blocks)
}
