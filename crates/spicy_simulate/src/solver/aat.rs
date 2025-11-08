use crate::solver::matrix::csc::CscMatrix;

/// calculating the symmetric pattern of A (A + A^T)

/// Assumes A is square with sorted columns and no duplicates
pub fn aat(a: &CscMatrix) {
    let n = a.dim.ncols;
    let mut last_columns_positions = vec![-1; n];
    // column_lengths[i] is the number of non-zero entries in column i excluding diagonals
    let mut column_lengths = vec![0; n];

    assert!(a.check_invariants().is_ok());

    let aat_info = aat_first_phase(a, &mut last_columns_positions, &mut column_lengths);

    // TODO: allocate space for matrix, extra space, 6 n sized vectors

    aat_second_phase(a, &column_lengths, &mut last_columns_positions, &aat_info);
}

struct AatInfo {
    sym: f64,
    nz_diagonal: usize,
    nz_both: usize,
    nz_aat: usize,
}

fn aat_first_phase(
    a: &CscMatrix,
    last_columns_positions: &mut [isize],
    column_lengths: &mut [usize],
) -> AatInfo {
    last_columns_positions.fill(-1);
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
            } else {
                // row > col, we are in the lower triangular part of A
                // this is handled elsewhere
                break;
            }

            // scan lower triangular part of A from column "row" until row "col"
            assert!(last_columns_positions[row] != -1);
            let mut row_column_position = last_columns_positions[row] as usize;
            // A is square, so this is always valid
            let column_row_start = a.col_start(row);
            let column_row_end = a.col_end(row);
            assert!(
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

    let mut sym = 0.0;
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
fn aat_second_phase(
    a: &CscMatrix,
    column_lengths: &[usize],
    last_columns_positions: &mut [isize],
    aat_info: &AatInfo,
) {
    last_columns_positions.fill(-1);
    let n = a.dim.ncols;

    // extra space for AAT matrix
    let mut aat_rows = vec![0; aat_info.nz_aat + (0.2 * aat_info.nz_aat as f64) as usize];
    let mut sp = vec![0; n];
    let mut pe = vec![0; n];
    let mut free_position = 0;

    for col in 0..n {
        sp[col] = free_position;
        pe[col] = free_position;
        free_position += column_lengths[col];
    }

    for col in 0..a.dim.ncols {
        let mut column_position = a.col_start(col);
        let column_end = a.col_end(col);
        while column_position < column_end {
            // scan the upper triangular part of A
            let row = a.row_index(column_position);
            if row < col {
                // in the upper triangular part of A,
                // add both A[col, row] and A[row, col] to the column lengths
                assert!(
                    sp[row]
                        < if row == n - 1 {
                            free_position
                        } else {
                            pe[row + 1]
                        }
                );
                assert!(
                    sp[col]
                        < if col == n - 1 {
                            free_position
                        } else {
                            pe[col + 1]
                        }
                );
                aat_rows[sp[row]] = col;
                aat_rows[sp[col]] = row;
                sp[row] += 1;
                sp[col] += 1;
                column_position += 1;
            } else if row == col {
                column_position += 1;
            } else {
                // row > col, we are in the lower triangular part of A
                // this is handled elsewhere
                break;
            }

            // scan lower triangular part of A from column "row" until row "col"
            assert!(last_columns_positions[row] != -1);
            let mut row_column_position = last_columns_positions[row] as usize;
            // A is square, so this is always valid
            let column_row_start = a.col_start(row);
            let column_row_end = a.col_end(row);
            assert!(
                column_row_start <= row_column_position && row_column_position <= column_row_end
            );
            while row_column_position < column_row_end {
                // "row_row" the row index of the current entry in the column of "row"
                let row_row = a.row_index(row_column_position);
                if row_row < col {
                    // A (row, row_row) is **only** in the lower part of A.
                    // so add both A[row, row_row] and A[row_row, row] to the column lengths
                    // row_row is always less than col
                    aat_rows[sp[row_row]] = row;
                    aat_rows[sp[row]] = row_row;
                    sp[row_row] += 1;
                    sp[row] += 1;
                    row_column_position += 1;
                } else if row_row == col {
                    // both in upper and lower triangular part of A
                    // no need to add again as we already added when scanning the upper triangular part of A
                    row_column_position += 1;
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
            aat_rows[sp[row]] = col;
            aat_rows[sp[col]] = row;
            sp[row] += 1;
            sp[col] += 1;
            column_position += 1;
        }
    }
}
