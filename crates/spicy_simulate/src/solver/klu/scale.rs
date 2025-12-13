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

use crate::solver::{klu::KluScale, matrix::csc::CscMatrix, utils::EMPTY};

/// A must be a valid CSC matrix.
pub fn scale(
    a: &CscMatrix,
    rs: &mut Vec<f64>,
    mut w: Option<&mut Vec<isize>>,
    scale: Option<KluScale>,
) -> Result<(), String> {
    let n = a.dim.nrows;
    let ncols = a.dim.ncols;

    let scale = match scale {
        None => return Ok(()),
        Some(scale) => scale,
    };

    debug_assert!(a.check_invariants().is_ok());

    rs.fill(0.0);

    // if we passed w it means we want to check for duplicates
    if let Some(w) = w.as_deref_mut() {
        w.fill(EMPTY);
    }

    for col in 0..ncols {
        let start = a.col_start(col);
        let end = a.col_end(col);
        for p in start..end {
            let row = a.row_index(p);
            debug_assert!(row < n);

            if let Some(w) = w.as_deref_mut() {
                if w[row] == col as isize {
                    return Err(format!("duplicate entry in column {} and row {}", col, row));
                }
                w[row] = col as isize;
            }

            let val = a.value(p).abs();
            match scale {
                KluScale::Sum => rs[row] += val,
                KluScale::Max => rs[row] = val.max(rs[row]),
            }
        }
    }

    // do not scale empty rows
    for row in 0..n {
        if rs[row] == 0.0 {
            rs[row] = 1.0;
        }
    }

    Ok(())
}
