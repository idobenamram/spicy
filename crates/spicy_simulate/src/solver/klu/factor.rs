use crate::solver::klu::klu_valid;
use crate::solver::klu::{KluConfig, KluNumeric, KluScale, KluSymbolic, scale::scale};
use crate::solver::matrix::csc::CscMatrix;
use crate::solver::utils::inverse_permutation;

pub fn allocate_klu_numeric(
    symbolic: &KluSymbolic,
    config: &KluConfig,
) -> Result<KluNumeric, String> {
    let n = symbolic.n;
    let nzoff = symbolic.nzoff;
    let nblocks = symbolic.nblocks;
    let maxblock = symbolic.maxblock;
    let n1 = n + 1;
    let nzoff1 = nzoff + 1;

    // TODO: figure thisout
    let mut lu_bx = Vec::with_capacity(nblocks);

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
        .ok_or("overflow")?;
    let n3 = n
        .checked_mul(3 * std::mem::size_of::<f64>())
        .ok_or("overflow")?;
    let b6 = maxblock
        .checked_mul(6 * std::mem::size_of::<isize>())
        .ok_or("overflow")?;
    let worksize = s + std::cmp::max(n3, b6);
    // TODO: figure this out
    let work = vec![0.0; worksize];
    let xwork = vec![0.0; n];
    let iwork = vec![0; 6 * n];

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
        xwork,
        iwork,
    };

    Ok(numeric)
}

pub fn factor(
    a: &CscMatrix,
    symbolic: &KluSymbolic,
    config: &mut KluConfig,
) -> Result<KluNumeric, String> {
    config.validate()?;
    let mut numeric = allocate_klu_numeric(symbolic, config)?;

    let n = symbolic.n;
    let nzoff = symbolic.nzoff;
    let mut max_lnz_block = 1;
    let mut max_unz_block = 1;
    let mut lnz = 0;
    let mut unz = 0;
    let mut noffdiag = 0;

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
        let k1 = symbolic.row_scaling[block] as usize;
        let k2 = symbolic.row_scaling[block + 1] as usize;
        let size = k2 - k1;

        if size == 1 {
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
                    assert!(newrow == k1);
                    let val = match &numeric.rs {
                        None => a.value(p),
                        Some(rs) => a.value(p) / rs[oldrow],
                    };
                    diag_val = val;
                }
            }

            numeric.u_diag[k1] = diag_val;
            if diag_val == 0. {
                if config.halt_if_singular {
                    return Err(format!("singular matrix at block {}", block));
                }
            }

            numeric.offp[k1 + 1] = poff;
            numeric.pnum[k1] = symbolic.row_permutation[k1];
            lnz += 1;
            unz += 1;
        } else {
            todo!()
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
                numeric.xwork[k] = rs[numeric.pnum[k] as usize];
            }
            for k in 0..n {
                rs[k] = numeric.xwork[k];
            }
        }
    }


    for p in 0..nzoff {
        debug_assert!(numeric.offi[p] >= 0 && numeric.offi[p] < n);
        numeric.offi[p] = numeric.pinv[numeric.offi[p]] as usize;
    }

    debug_assert!(klu_valid(n, &numeric.offp, &numeric.offi));

    Ok(numeric)
}
