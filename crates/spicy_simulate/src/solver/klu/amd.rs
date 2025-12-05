use crate::solver;
use crate::solver::amd::AmdControl;
use crate::solver::utils::{as_usize_slice, as_usize_slice_mut};
use crate::solver::{
    aat::{aat_first_phase, aat_second_phase},
    matrix::csc::CscPointers,
};

/// Assumes A is square with sorted columns and no duplicates
pub fn amd(a: CscPointers, permutation: &mut [isize]) -> solver::amd::AmdInfo {
    debug_assert!(a.check_invariants().is_ok());

    let n = a.dim.ncols;
    let nz = a.nnz();
    // column_lengths[i] is the number of non-zero entries in column i excluding diagonals
    // len in timothys code
    let mut column_lengths = vec![0; n];
    // TODO: could techincally be allocated in the workspace
    let mut inverse_permutation = vec![0; n];

    let aat_info = aat_first_phase(&a, &mut column_lengths, permutation);
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

        aat_second_phase(&a, free_position, iw_usize, nv_usize, pe_usize, w);
    }

    solver::amd::amd(
        n,
        pe,
        iw,
        &mut column_lengths,
        iwlen,
        free_position,
        nv,
        &mut inverse_permutation, // inverse permutation workspace
        permutation,              // output permutation
        head,
        elen,
        degree,
        w,
        AmdControl::default(),
    )
}
