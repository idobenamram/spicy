// SPDX-License-Identifier: BSD-3-Clause
//
// This file is based on the SuiteSparse AMD implementation used by KLU.
//
// AMD, Copyright (c) 1996-2022, Timothy A. Davis, Patrick R. Amestoy, and
// Iain S. Duff.  All Rights Reserved.
//
// Modifications/porting for this project:
// Copyright (c) 2025 Ido Ben Amram

use crate::solver;
use crate::solver::amd::AmdControl;
use crate::solver::utils::{as_usize_slice, as_usize_slice_mut};
use crate::solver::{
    aat::{aat_first_phase, aat_second_phase},
    matrix::csc::CscPointers,
    matrix::slice::SpicySlice,
};

/// Assumes A is square with sorted columns and no duplicates
pub fn amd(a: CscPointers, permutation: &mut SpicySlice<isize>) -> solver::amd::AmdInfo {
    debug_assert!(a.check_invariants().is_ok());

    let n = a.dim.ncols;
    let nz = a.nnz();
    // column_lengths[i] is the number of non-zero entries in column i excluding diagonals
    // len in timothys code
    let mut column_lengths = vec![0; n];
    // TODO: could techincally be allocated in the workspace
    let mut inverse_permutation = vec![0; n];

    let aat_info = aat_first_phase(
        &a,
        SpicySlice::from_mut_slice(column_lengths.as_mut_slice()),
        permutation,
    );
    let nzaat = aat_info.nz_aat;
    debug_assert!((std::cmp::max(nz - n, 0) <= nzaat) && (nzaat <= 2 * nz));

    let mut workspace_size = nzaat;
    // amd expects elbow room (1.2) to work efficiently
    workspace_size = workspace_size.checked_add(nzaat / 5).expect("overflow");
    workspace_size = workspace_size.checked_add(7 * n).expect("overflow");

    let mut workspace: Vec<isize> = vec![0; workspace_size];

    let iwlen = workspace_size - 6 * n;
    let (pe, workspace) = workspace.split_at_mut(n);
    let (nv, workspace) = workspace.split_at_mut(n);
    let (head, workspace) = workspace.split_at_mut(n);
    let (elen, workspace) = workspace.split_at_mut(n);
    let (degree, workspace) = workspace.split_at_mut(n);
    let (w, workspace) = workspace.split_at_mut(n);
    let iw = workspace;
    debug_assert!(iw.len() == iwlen);

    // pfree in timothys code
    let mut free_position: usize = 0;

    for col in 0..n {
        nv[col] = free_position as isize;
        pe[col] = free_position as isize;
        free_position += column_lengths[col];
    }

    /* Note that this restriction on iwlen is slightly more restrictive than
     * what is strictly required in amd.  amd can operate with no elbow
     * room at all, but it will be very slow.  For better performance, at
     * least size-n elbow room is enforced. */
    debug_assert!(iwlen >= free_position + n);

    // `iw`, `nv`, and `pe` are all slices of `isize` in the AMD workspace,
    // but `aat_second_phase` operates on `usize` indices.  Reinterpret them
    // as `usize` just for this phase, then continue to use them as `isize`
    // for the AMD routine that follows.
    unsafe {
        let iw_usize: &mut [usize] = as_usize_slice_mut(iw);
        let nv_usize: &mut [usize] = as_usize_slice_mut(nv);
        let pe_usize: &[usize] = as_usize_slice(pe);

        aat_second_phase(
            &a,
            free_position,
            SpicySlice::from_mut_slice(iw_usize),
            SpicySlice::from_mut_slice(nv_usize),
            SpicySlice::from_slice(pe_usize),
            SpicySlice::from_mut_slice(w),
        );
    }

    solver::amd::amd(
        n,
        SpicySlice::from_mut_slice(pe),
        SpicySlice::from_mut_slice(iw),
        SpicySlice::from_mut_slice(column_lengths.as_mut_slice()),
        iwlen,
        free_position,
        SpicySlice::from_mut_slice(nv),
        SpicySlice::from_mut_slice(inverse_permutation.as_mut_slice()), // inverse permutation workspace
        permutation,              // output permutation
        SpicySlice::from_mut_slice(head),
        SpicySlice::from_mut_slice(elen),
        SpicySlice::from_mut_slice(degree),
        SpicySlice::from_mut_slice(w),
        AmdControl::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::amd;
    use crate::solver::matrix::Dim;
    use crate::solver::matrix::csc::CscPointers;
    use crate::solver::matrix::slice::SpicySlice;

    #[test]
    fn amd_regression_matrix_5x5_ap_ai() {
        // Matrix in CSC form (n=5) taken from a minimal AMD regression case.
        //
        // int32_t n = 5 ;
        // int32_t Ap [ ] = { 0,   2,       6,       10,  12, 14} ;
        // int32_t Ai [ ] = { 0,1, 0,1,2,4, 1,2,3,4, 2,3, 1,4   } ;
        let n = 5usize;
        let ap: [usize; 6] = [0, 2, 6, 10, 12, 14];
        let ai: [usize; 14] = [0, 1, 0, 1, 2, 4, 1, 2, 3, 4, 2, 3, 1, 4];

        let a = CscPointers::new(Dim { nrows: n, ncols: n }, &ap, &ai);
        a.check_invariants().unwrap();

        let mut p = vec![0isize; n];
        let info = amd(a, SpicySlice::from_mut_slice(p.as_mut_slice()));

        // For such a small n, the "dense" threshold (>=16) should not trigger.
        assert_eq!(info.ndense, 0);

        // Expected output permutation for this regression case.
        assert_eq!(p, vec![0, 3, 2, 4, 1]);

        // Output must be a valid permutation of 0..n-1.
        let mut seen = vec![false; n];
        for &v in &p {
            assert!(v >= 0);
            let u = v as usize;
            assert!(u < n);
            assert!(!seen[u], "duplicate index {u} in permutation {p:?}");
            seen[u] = true;
        }
        assert!(
            seen.into_iter().all(|x| x),
            "missing entries in permutation {p:?}"
        );
    }
}
