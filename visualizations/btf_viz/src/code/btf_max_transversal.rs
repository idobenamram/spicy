use crate::code::recorder::Recorder;
use spicy_simulate::solver::matrix::csc::CscMatrix;

fn try_augmenting_path(
    m: &CscMatrix,
    current_column: usize,
    column_permutations: &mut [isize],
    cheap: &mut [usize],
    visited: &mut [usize],
    row_stack: &mut [usize],
    column_stack: &mut [usize],
    position_stack: &mut [usize],
    recorder: &mut Recorder,
) -> bool {
    let mut found = false;
    let mut head: i64 = 0;
    recorder.push_number_step(line!() - 1, "head", &head);
    column_stack[head as usize] = current_column;
    recorder.push_array_step(
        line!() - 1,
        "column_stack",
        head as usize,
        &column_stack[head as usize],
    );
    assert!(visited[current_column] != current_column);

    while head >= 0 {
        recorder.push_step(line!() - 1);
        let col = column_stack[head as usize];
        recorder.push_number_step(line!() - 1, "col", &col);
        let end_of_column = m.col_start(col + 1);

        if visited[col] != current_column {
            recorder.push_step(line!() - 1);
            visited[col] = current_column;
            recorder.push_array_step(line!() - 1, "visited", col, &visited[col]);

            let mut current_row_ptr = cheap[col];
            let mut row = 0;
            while current_row_ptr < end_of_column && !found {
                recorder.push_step(line!() - 1);
                row = m.row_index(current_row_ptr);
                found = column_permutations[row] == -1;
                current_row_ptr += 1;
            }
            cheap[col] = current_row_ptr;
            recorder.push_array_step(line!() - 1, "cheap", col, &cheap[col]);

            if found {
                row_stack[head as usize] = row;
                recorder.push_array_step(
                    line!() - 1,
                    "row_stack",
                    head as usize,
                    &row_stack[head as usize],
                );
                break;
            }
            position_stack[head as usize] = m.col_start(col);
            recorder.push_array_step(
                line!() - 1,
                "position_stack",
                head as usize,
                &position_stack[head as usize],
            );
        }

        let mut row_ptr = position_stack[head as usize];
        recorder.push_number_step(line!() - 1, "row_ptr", &row_ptr);
        while row_ptr < end_of_column {
            recorder.push_step(line!() - 1);
            let row = m.row_index(row_ptr);
            recorder.push_number_step(line!() - 1, "row", &row);
            let col = column_permutations[row];
            recorder.push_number_step(line!() - 1, "col", &col);
            if visited[col as usize] != current_column {
                recorder.push_step(line!() - 1);
                position_stack[head as usize] = row_ptr + 1;
                recorder.push_array_step(
                    line!() - 1,
                    "position_stack",
                    head as usize,
                    &position_stack[head as usize],
                );
                row_stack[head as usize] = row;
                recorder.push_array_step(
                    line!() - 1,
                    "row_stack",
                    head as usize,
                    &row_stack[head as usize],
                );
                head += 1;
                recorder.push_number_step(line!() - 1, "head", &head);
                column_stack[head as usize] = col as usize;
                recorder.push_array_step(
                    line!() - 1,
                    "column_stack",
                    head as usize,
                    &column_stack[head as usize],
                );
                break;
            }
            row_ptr += 1;
            recorder.push_number_step(line!() - 1, "row_ptr", &row_ptr);
        }

        if row_ptr == end_of_column {
            head -= 1;
            recorder.push_number_step(line!() - 1, "head", &head);
        }
    }

    if found {
        recorder.push_step(line!() - 1);
        while head >= 0 {
            recorder.push_step(line!() - 1);
            let col = column_stack[head as usize];
            let row = row_stack[head as usize];
            column_permutations[row] = col as isize;
            recorder.push_array_step(
                line!() - 1,
                "column_permutations",
                row,
                &column_permutations[row],
            );
            head -= 1;
            recorder.push_number_step(line!() - 1, "head", &head);
        }
    }

    return found;
}

pub fn btf_max_transversal(m: &CscMatrix, recorder: &mut Recorder) -> (usize, Vec<isize>) {
    let n = m.dim.ncols;
    let out_of_bounds = n + 1;

    // Record matrix numeric entries (col, row, value)
    recorder.set_initial("matrix_rows", &m.dim.nrows);
    recorder.set_initial("matrix_cols", &m.dim.ncols);
    let entries: Vec<(usize, usize, f64)> = (0..m.dim.ncols)
        .flat_map(|col| {
            let start = m.col_start(col);
            let end = m.col_start(col + 1);
            (start..end).map(move |idx| (col, m.row_index(idx), m.values[idx]))
        })
        .collect();
    recorder.set_initial("matrix_entries", &entries);

    let mut column_permutations: Vec<isize> = vec![-1; n];
    recorder.set_initial("column_permutations", &column_permutations);
    let mut cheap: Vec<usize> = vec![0; n];
    recorder.set_initial("cheap", &cheap);
    let mut visited: Vec<usize> = vec![out_of_bounds; n];
    recorder.set_initial("visited", &visited);
    let mut row_stack: Vec<usize> = vec![out_of_bounds; m.dim.nrows];
    recorder.set_initial("row_stack", &row_stack);
    let mut column_stack: Vec<usize> = vec![out_of_bounds; n];
    recorder.set_initial("column_stack", &column_stack);
    let mut position_stack: Vec<usize> = vec![out_of_bounds; n];
    recorder.set_initial("position_stack", &position_stack);

    for col in 0..n {
        cheap[col] = m.col_start(col);
        recorder.push_array_step(line!() - 1, "cheap", col, &cheap[col]);
    }

    let mut number_of_matches = 0;
    for col in 0..n {
        recorder.push_number_step(line!() - 1, "col", &col);
        let found = try_augmenting_path(
            m,
            col,
            &mut column_permutations,
            &mut cheap,
            &mut visited,
            &mut row_stack,
            &mut column_stack,
            &mut position_stack,
            recorder,
        );

        if found {
            number_of_matches += 1;
        }
    }

    return (number_of_matches, column_permutations);
}