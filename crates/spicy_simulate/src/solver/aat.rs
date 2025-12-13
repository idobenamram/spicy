// SPDX-License-Identifier: BSD-3-Clause
//
// This file is based on the SuiteSparse AMD implementation (amd_aat) by
// Timothy A. Davis and collaborators.
//
// AMD, Copyright (c) 1996-2022, Timothy A. Davis, Patrick R. Amestoy, and
// Iain S. Duff.  All Rights Reserved.
//
// Modifications/porting for this project:
// Copyright (c) 2025 Ido Ben Amram

use crate::solver::{matrix::csc::CscPointers, utils::EMPTY};

/// calculating the symmetric pattern of A (A + A^T)



pub struct AatInfo {
    // symmetry pattern of A
    pub sym: f64,
    // non zeros on the diagonal of A
    pub nz_diagonal: usize,
    // symmetric non zeros in original A
    pub nz_both: usize,
    // non zeros in the symmetric pattern of A
    pub nz_aat: usize,
}

pub fn aat_first_phase(
    a: &CscPointers,
    // len n
    column_lengths: &mut [usize],
    // len n, as the scan continues, the last_columns_positions[i] will be the position of the last entry in the column of i
    // that has been scanned
    last_columns_positions: &mut [isize],
) -> AatInfo {
    last_columns_positions.fill(EMPTY);
    column_lengths.fill(0);

    let mut nz_diagonal = 0;
    let mut nz_both = 0;
    let n = a.dim.ncols;
    let nz = a.nnz();

    for col in 0..a.dim.ncols {
        let mut column_position = a.col_start(col);
        let column_end = a.col_end(col);
        while column_position < column_end {
            // scan the upper triangular part of A
            let row = a.row_index(column_position);
            if row < col {
                // in the upper triangular part of A,
                // add both A[col, row] and A[row, col] to the column lengths
                column_lengths[row] += 1;
                column_lengths[col] += 1;
                column_position += 1;
            } else if row == col {
                // for diagonals only move the column position forward
                nz_diagonal += 1;
                column_position += 1;
                // the rest of the column is strictly below the diagonal (sorted indices),
                // so stop scanning the upper triangular part.
                break;
            } else {
                // row > col, we are in the lower triangular part of A
                // this is handled elsewhere
                break;
            }

            // scan lower triangular part of A from column "row" until row "col"
            debug_assert!(last_columns_positions[row] != EMPTY);
            let mut row_column_position = last_columns_positions[row] as usize;
            // A is square, so this is always valid
            let column_row_start = a.col_start(row);
            let column_row_end = a.col_end(row);
            debug_assert!(
                column_row_start <= row_column_position && row_column_position <= column_row_end
            );
            while row_column_position < column_row_end {
                // "row_row" the row index of the current entry in the column of "row"
                let row_row = a.row_index(row_column_position);
                if row_row < col {
                    // A (row, row_row) is **only** in the lower part of A.
                    // so add both A[row, row_row] and A[row_row, row] to the column lengths
                    // row_row is always less than col
                    column_lengths[row_row] += 1;
                    column_lengths[row] += 1;
                    row_column_position += 1;
                } else if row_row == col {
                    // both in upper and lower triangular part of A
                    // no need to add again as we already added when scanning the upper triangular part of A
                    row_column_position += 1;
                    nz_both += 1;
                    // Next entries (if any) are > col (sorted indices) and will be handled later.
                    break;
                } else {
                    // will be handled later, when col > row_row
                    break;
                }
            }
            // save the position of the last entry in the column of "row"
            last_columns_positions[row] = row_column_position as isize;
        }
        // save the position of the last entry in the column of "col"
        last_columns_positions[col] = column_position as isize;
    }

    // left overs in the lower triangular part of A
    for col in 0..n {
        // we have looked at all columns so last_columns_positions[col] != -1
        let mut column_position = last_columns_positions[col] as usize;
        let column_end = a.col_end(col);
        while column_position < column_end {
            let row = a.row_index(column_position);
            // A[col, row] is in the lower triangular part of A
            // and we have not seen it yet, so add both A[col, row]
            // and A[row, col] to the column lengths
            column_lengths[row] += 1;
            column_lengths[col] += 1;

            column_position += 1;
        }
    }

    // compute the symmetry of the non-zero pattern of A

    let sym;
    // only diagonal has non zeros
    if nz == nz_diagonal {
        sym = 1.0;
    } else {
        sym = (2.0 * nz_both as f64) / ((nz - nz_diagonal) as f64);
    }

    // excluding diagonals
    let nz_aat = column_lengths.iter().sum::<usize>();

    AatInfo {
        sym,
        nz_diagonal,
        nz_both,
        nz_aat,
    }
}

/// second phase is actually constructing the AAT matrix based on phase 2
/// structurally very similar to phase 1
pub fn aat_second_phase(
    a: &CscPointers,
    free_position: usize,
    // will be filled with the AAT matrix
    aat_rows: &mut [usize],
    // the current position in the AAT matrix for row/col
    // on input, contains the start position of the column in the AAT matrix
    current_pos: &mut [usize],
    // on input, contains the end position of the column in the AAT matrix
    pe: &[usize],
    // for each column/row, the position of the last scanned entry in the column
    last_columns_positions: &mut [isize],
) {
    let n = a.dim.ncols;

    for col in 0..a.dim.ncols {
        let mut column_position = a.col_start(col);
        let column_end = a.col_end(col);
        while column_position < column_end {
            // scan the upper triangular part of A
            let row = a.row_index(column_position);
            if row < col {
                // in the upper triangular part of A,
                // add both A[col, row] and A[row, col] to the column lengths
                debug_assert!(
                    current_pos[row]
                        < if row == n - 1 {
                            free_position
                        } else {
                            pe[row + 1]
                        }
                );
                debug_assert!(
                    current_pos[col]
                        < if col == n - 1 {
                            free_position
                        } else {
                            pe[col + 1]
                        }
                );
                aat_rows[current_pos[row]] = col;
                aat_rows[current_pos[col]] = row;
                current_pos[row] += 1;
                current_pos[col] += 1;
                column_position += 1;
            } else if row == col {
                column_position += 1;
                // entries (if any) are below the diagonal (sorted indices).
                break;
            } else {
                // row > col, we are in the lower triangular part of A
                // this is handled elsewhere
                break;
            }

            // scan lower triangular part of A from column "row" until row "col"
            debug_assert!(last_columns_positions[row] != EMPTY);
            let mut row_column_position = last_columns_positions[row] as usize;
            // A is square, so this is always valid
            let column_row_start = a.col_start(row);
            let column_row_end = a.col_end(row);
            debug_assert!(
                column_row_start <= row_column_position && row_column_position <= column_row_end
            );
            while row_column_position < column_row_end {
                // "row_row" the row index of the current entry in the column of "row"
                let row_row = a.row_index(row_column_position);
                if row_row < col {
                    // A (row, row_row) is **only** in the lower part of A.
                    // so add both A[row, row_row] and A[row_row, row] to the column lengths
                    // row_row is always less than col
                    aat_rows[current_pos[row_row]] = row;
                    aat_rows[current_pos[row]] = row_row;
                    current_pos[row_row] += 1;
                    current_pos[row] += 1;
                    row_column_position += 1;
                } else if row_row == col {
                    // both in upper and lower triangular part of A
                    // no need to add again as we already added when scanning the upper triangular part of A
                    row_column_position += 1;
                    // Next entries (if any) are > col (sorted indices) and will be handled later.
                    break;
                } else {
                    // will be handled later, when col > row_row
                    break;
                }
            }
            // save the position of the last entry in the column of "row"
            last_columns_positions[row] = row_column_position as isize;
        }
        // save the position of the last entry in the column of "col"
        last_columns_positions[col] = column_position as isize;
    }

    // left overs in the lower triangular part of A
    for col in 0..n {
        // we have looked at all columns so last_columns_positions[col] != -1
        let mut column_position = last_columns_positions[col] as usize;
        let column_end = a.col_end(col);
        while column_position < column_end {
            let row = a.row_index(column_position);
            // A[col, row] is in the lower triangular part of A
            // and we have not seen it yet, so add both A[col, row]
            // and A[row, col] to the column lengths
            aat_rows[current_pos[row]] = col;
            aat_rows[current_pos[col]] = row;
            current_pos[row] += 1;
            current_pos[col] += 1;
            column_position += 1;
        }
    }

    for j in 0..n - 1 {
        debug_assert!(current_pos[j] == pe[j + 1]);
    }
    debug_assert!(current_pos[n - 1] == free_position);
}
