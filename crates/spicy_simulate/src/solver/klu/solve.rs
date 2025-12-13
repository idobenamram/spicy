use crate::solver::klu::{KluConfig, KluNumeric, KluSymbolic, get_pointers_to_lu, klu_valid};

/// solve Lx = b, Assumes L is unit lower triangular and where the unit diagonal
/// entry is NOT stored.
/// B is n-by-nrhs and is stored in ROW form with row dimension nrhs.
/// nrhs must be in the range 1 to 4.
fn klu_lsolve(
    n: usize,
    lip: &[usize],
    llen: &[usize],
    lu: &[f64],
    nrhs: usize,
    // right-hand-side on input, solution to Lx=b on output
    x: &mut [f64],
) -> Result<(), String> {
    let mut temp = [0.0; 4];

    match nrhs {
        1 => {
            for k in 0..n {
                temp[0] = x[k];
                let (li, lx, len) = get_pointers_to_lu(lu, lip, llen, k)?;
                // unit diagonal of L is not stored
                for p in 0..len {
                    let i = li[p];
                    let val = lx[p];
                    x[i] -= val * temp[0];
                }
            }
        }
        2 => {
            for k in 0..n {
                temp[0] = x[2 * k];
                temp[1] = x[2 * k + 1];
                let (li, lx, len) = get_pointers_to_lu(lu, lip, llen, k)?;
                for p in 0..len {
                    let i = li[p];
                    let val = lx[p];
                    x[2 * i] -= val * temp[0];
                    x[2 * i + 1] -= val * temp[1];
                }
            }
        }
        3 => {
            for k in 0..n {
                temp[0] = x[3 * k];
                temp[1] = x[3 * k + 1];
                temp[2] = x[3 * k + 2];
                let (li, lx, len) = get_pointers_to_lu(lu, lip, llen, k)?;
                for p in 0..len {
                    let i = li[p];
                    let val = lx[p];
                    x[3 * i] -= val * temp[0];
                    x[3 * i + 1] -= val * temp[1];
                    x[3 * i + 2] -= val * temp[2];
                }
            }
        }
        4 => {
            for k in 0..n {
                temp[0] = x[4 * k];
                temp[1] = x[4 * k + 1];
                temp[2] = x[4 * k + 2];
                temp[3] = x[4 * k + 3];
                let (li, lx, len) = get_pointers_to_lu(lu, lip, llen, k)?;
                for p in 0..len {
                    let i = li[p];
                    let val = lx[p];
                    x[4 * i] -= val * temp[0];
                    x[4 * i + 1] -= val * temp[1];
                    x[4 * i + 2] -= val * temp[2];
                    x[4 * i + 3] -= val * temp[3];
                }
            }
        }
        _ => unreachable!("nrhs = {}", nrhs),
    }

    Ok(())
}

/// solve Ux = b, Assumes U is non-unit upper triangular and where the diagonal
/// entry is NOT stored.
/// B is n-by-nrhs and is stored in ROW form with row dimension nrhs.
/// nrhs must be in the range 1 to 4.
fn klu_usolve(
    n: usize,
    uip: &[usize],
    ulen: &[usize],
    lu: &[f64],
    u_diag: &[f64],
    nrhs: usize,
    // right-hand-side on input, solution to Ux=b on output
    x: &mut [f64],
) -> Result<(), String> {
    let mut temp = [0.0; 4];

    match nrhs {
        1 => {
            for k in (0..n).rev() {
                let (ui, ux, len) = get_pointers_to_lu(lu, uip, ulen, k)?;
                temp[0] = x[k] / u_diag[k];
                x[k] = temp[0];
                for p in 0..len {
                    let i = ui[p];
                    let val = ux[p];
                    x[i] -= val * temp[0];
                }
            }
        }
        2 => {
            for k in (0..n).rev() {
                let (ui, ux, len) = get_pointers_to_lu(lu, uip, ulen, k)?;
                temp[0] = x[2 * k] / u_diag[k];
                temp[1] = x[2 * k + 1] / u_diag[k];
                x[2 * k] = temp[0];
                x[2 * k + 1] = temp[1];
                for p in 0..len {
                    let i = ui[p];
                    let val = ux[p];
                    x[2 * i] -= val * temp[0];
                    x[2 * i + 1] -= val * temp[1];
                }
            }
        }
        3 => {
            for k in (0..n).rev() {
                let (ui, ux, len) = get_pointers_to_lu(lu, uip, ulen, k)?;
                temp[0] = x[3 * k] / u_diag[k];
                temp[1] = x[3 * k + 1] / u_diag[k];
                temp[2] = x[3 * k + 2] / u_diag[k];
                x[3 * k] = temp[0];
                x[3 * k + 1] = temp[1];
                x[3 * k + 2] = temp[2];
                for p in 0..len {
                    let i = ui[p];
                    let val = ux[p];
                    x[3 * i] -= val * temp[0];
                    x[3 * i + 1] -= val * temp[1];
                    x[3 * i + 2] -= val * temp[2];
                }
            }
        }
        4 => {
            for k in (0..n).rev() {
                let (ui, ux, len) = get_pointers_to_lu(lu, uip, ulen, k)?;
                temp[0] = x[4 * k] / u_diag[k];
                temp[1] = x[4 * k + 1] / u_diag[k];
                temp[2] = x[4 * k + 2] / u_diag[k];
                temp[3] = x[4 * k + 3] / u_diag[k];
                x[4 * k] = temp[0];
                x[4 * k + 1] = temp[1];
                x[4 * k + 2] = temp[2];
                x[4 * k + 3] = temp[3];
                for p in 0..len {
                    let i = ui[p];
                    let val = ux[p];
                    x[4 * i] -= val * temp[0];
                    x[4 * i + 1] -= val * temp[1];
                    x[4 * i + 2] -= val * temp[2];
                    x[4 * i + 3] -= val * temp[3];
                }
            }
        }
        _ => unreachable!("nrhs = {}", nrhs),
    }

    Ok(())
}

// solve Ax =b using the symbolic and numeric objects from analyze
// and factor.
pub(crate) fn solve(
    symbolic: &KluSymbolic,
    numeric: &mut KluNumeric,

    // leading dimension of B
    d: usize,
    // number of right-hand-sides
    nrhs: usize,

    // right-hand-side on input, overwritten with solution to Ax=b on output
    b: &mut [f64],
    config: &KluConfig,
) -> Result<(), String> {
    if d < symbolic.n {
        return Err(format!(
            "leading dimension of B must be >= n: d = {}, n = {}",
            d, symbolic.n
        ));
    }

    let n = symbolic.n;
    let nblocks = symbolic.nblocks;
    let q = &symbolic.column_permutation;
    let r = &symbolic.row_permutation;

    debug_assert!(nblocks == numeric.nblocks);
    let pnum = &numeric.pnum;
    let offp = &numeric.offp;
    let offi = &numeric.offi;
    let offx = &numeric.offx;

    let lip = &numeric.lip;
    let llen = &numeric.llen;
    let uip = &numeric.uip;
    let ulen = &numeric.ulen;
    let lu_bx = &numeric.lu_bx;
    let u_diag = &numeric.u_diag;

    let rs = &numeric.rs;
    let x = numeric.work.as_mut_slice();
    let mut temp = [0.0; 4];

    debug_assert!(klu_valid(n, offp, offi));

    let mut bz = b;

    // solve in chunks of 4 columns at a time

    for chunk in (0..nrhs).step_by(4) {
        let nr = std::cmp::min(nrhs - chunk, 4);

        // scale and permute the right hand side
        match rs {
            Some(rs) => {
                for k in 0..n {
                    match nr {
                        1 => {
                            for k in 0..n {
                                let i = pnum[k] as usize;
                                let rs = rs[k];
                                x[k] = bz[i] / rs;
                            }
                        }
                        2 => {
                            for k in 0..n {
                                let i = pnum[k] as usize;
                                let rs = rs[k];
                                x[2 * k] = bz[i] / rs;
                                x[2 * k + 1] = bz[i + d] / rs;
                            }
                        }
                        3 => {
                            for k in 0..n {
                                let i = pnum[k] as usize;
                                let rs = rs[k];
                                x[3 * k] = bz[i] / rs;
                                x[3 * k + 1] = bz[i + d] / rs;
                                x[3 * k + 2] = bz[i + 2 * d] / rs;
                            }
                        }
                        4 => {
                            for k in 0..n {
                                let i = pnum[k] as usize;
                                let rs = rs[k];
                                x[4 * k] = bz[i] / rs;
                                x[4 * k + 1] = bz[i + d] / rs;
                                x[4 * k + 2] = bz[i + 2 * d] / rs;
                                x[4 * k + 3] = bz[i + 3 * d] / rs;
                            }
                        }
                        _ => unreachable!("nr = {}", nr),
                    }
                }
            }
            None => match nr {
                1 => {
                    for k in 0..n {
                        let i = pnum[k] as usize;
                        x[k] = bz[i];
                    }
                }
                2 => {
                    for k in 0..n {
                        let i = pnum[k] as usize;
                        x[2 * k] = bz[i];
                        x[2 * k + 1] = bz[i + d];
                    }
                }
                3 => {
                    for k in 0..n {
                        let i = pnum[k] as usize;
                        x[3 * k] = bz[i];
                        x[3 * k + 1] = bz[i + d];
                        x[3 * k + 2] = bz[i + 2 * d];
                    }
                }
                4 => {
                    for k in 0..n {
                        let i = pnum[k] as usize;
                        x[4 * k] = bz[i];
                        x[4 * k + 1] = bz[i + d];
                        x[4 * k + 2] = bz[i + 2 * d];
                        x[4 * k + 3] = bz[i + 3 * d];
                    }
                }
                _ => unreachable!("nr = {}", nr),
            },
        }

        // solve X = (L*U + Off)\X

        // we use "back substitution" on the blocks to solve the system
        // look at the KLU article page 6
        for block in (0..nblocks).rev() {
            let k1 = r[block] as usize;
            let k2 = r[block + 1] as usize;
            let nk = k2 - k1;

            // solve the block system
            if nk == 1 {
                let s = u_diag[k1];
                match nr {
                    1 => {
                        x[k1] /= s;
                    }
                    2 => {
                        x[2 * k1] /= s;
                        x[2 * k1 + 1] /= s;
                    }
                    3 => {
                        x[3 * k1] /= s;
                        x[3 * k1 + 1] /= s;
                        x[3 * k1 + 2] /= s;
                    }
                    4 => {
                        x[4 * k1] /= s;
                        x[4 * k1 + 1] /= s;
                        x[4 * k1 + 2] /= s;
                        x[4 * k1 + 3] /= s;
                    }
                    _ => unreachable!("nr = {}", nr),
                }
            } else {
                let (_, lip_after) = lip.split_at(k1);
                let (_, llen_after) = llen.split_at(k1);
                let (_, uip_after) = uip.split_at(k1);
                let (_, ulen_after) = ulen.split_at(k1);
                let (_, u_diag_after) = u_diag.split_at(k1);
                let lu = lu_bx[block].as_slice();
                let (_, x_after) = x.split_at_mut(k1 * nr);

                klu_lsolve(nk, lip_after, llen_after, lu, nr, x_after);
                klu_usolve(nk, uip_after, ulen_after, lu, u_diag_after, nr, x_after);
            }

            // block back-substitution for the off-diagonal-block entries
            if block > 0 {
                match nr {
                    1 => {
                        for k in k1..k2 {
                            let start = offp[k];
                            let end = offp[k + 1];
                            temp[0] = x[k];
                            for p in start..end {
                                let i = offi[p] as usize;
                                x[i] -= offx[p] * temp[0];
                            }
                        }
                    }
                    2 => {
                        for k in k1..k2 {
                            let start = offp[k];
                            let end = offp[k + 1];
                            temp[0] = x[2 * k];
                            temp[1] = x[2 * k + 1];
                            for p in start..end {
                                let i = offi[p] as usize;
                                x[2 * i] -= offx[p] * temp[0];
                                x[2 * i + 1] -= offx[p] * temp[1];
                            }
                        }
                    }
                    3 => {
                        for k in k1..k2 {
                            let start = offp[k];
                            let end = offp[k + 1];
                            temp[0] = x[3 * k];
                            temp[1] = x[3 * k + 1];
                            temp[2] = x[3 * k + 2];
                            for p in start..end {
                                let i = offi[p] as usize;
                                x[3 * i] -= offx[p] * temp[0];
                                x[3 * i + 1] -= offx[p] * temp[1];
                                x[3 * i + 2] -= offx[p] * temp[2];
                            }
                        }
                    }
                    4 => {
                        for k in k1..k2 {
                            let start = offp[k];
                            let end = offp[k + 1];
                            temp[0] = x[4 * k];
                            temp[1] = x[4 * k + 1];
                            temp[2] = x[4 * k + 2];
                            temp[3] = x[4 * k + 3];
                            for p in start..end {
                                let i = offi[p] as usize;
                                x[4 * i] -= offx[p] * temp[0];
                                x[4 * i + 1] -= offx[p] * temp[1];
                                x[4 * i + 2] -= offx[p] * temp[2];
                                x[4 * i + 3] -= offx[p] * temp[3];
                            }
                        }
                    }
                    _ => unreachable!("nr = {}", nr),
                }
            }
        }

        // permute the result, Bz = Q*X
        match nr {
            1 => {
                for k in 0..n {
                    let i = q[k] as usize;
                    bz[i] = x[k];
                }
            }
            2 => {
                for k in 0..n {
                    let i = q[k] as usize;
                    bz[i] = x[2 * k];
                    bz[i + d] = x[2 * k + 1];
                }
            }
            3 => {
                for k in 0..n {
                    let i = q[k] as usize;
                    bz[i] = x[3 * k];
                    bz[i + d] = x[3 * k + 1];
                    bz[i + 2 * d] = x[3 * k + 2];
                }
            }
            4 => {
                for k in 0..n {
                    let i = q[k] as usize;
                    bz[i] = x[4 * k];
                    bz[i + d] = x[4 * k + 1];
                    bz[i + 2 * d] = x[4 * k + 2];
                    bz[i + 3 * d] = x[4 * k + 3];
                }
            }
            _ => unreachable!("nr = {}", nr),
        }

        // go to the next chunk of B
        bz = &mut bz[d * 4..];
    }
    Ok(())
}
