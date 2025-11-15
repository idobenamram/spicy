/// Block Triangular Form (BTF), Maximal Transversal (MAXTRAN)
/// the algorithm is described in the paper:
/// On Algorithms for Obtaining a Maximum Transversal by I. S. Duff
/// but to be honest, the paper was very hard to understand.
/// the easier thing is to read the implementation of Timothy A. Davis.
/// here: https://github.com/DrTimothyAldenDavis/SuiteSparse/blob/stable/BTF/Include/btf.h
/// the code is pretty well documented and much easier to understand.
use super::recorder::Recorder;
use crate::solver::matrix::csc::CscMatrix;
use spicy_macros::recorded;

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

/// for the given column, try to find a column permutation that will match this row and
/// there are 2 main parts to the algorithm:
/// 1. the "cheap test" which is a way to greedily try to match a
///    column's nonzero to a row, creating a permutation
/// 2. the "augmenting path" which happens if there are no cheap options,
///    try to backtrack ("depth first") the current matches so the matches work with the non-zeroes in the current column
#[recorded]
pub(crate) fn btf_max_transversal(m: &CscMatrix, recorder: &mut Recorder) -> (usize, Vec<isize>) {
    let n = m.dim.ncols;
    let out_of_bounds = n + 1;
    // match in davis's code
    let mut column_permutations: Vec<isize> = vec![-1; n];
    let mut cheap: Vec<usize> = vec![0; n];
    // flag in davis's code
    let mut visited: Vec<usize> = vec![out_of_bounds; n];

    // istack
    let mut row_stack: Vec<usize> = vec![out_of_bounds; m.dim.nrows];
    // jstack
    let mut column_stack: Vec<usize> = vec![out_of_bounds; n];
    let mut position_stack: Vec<usize> = vec![out_of_bounds; n];

    for (col, c) in cheap.iter_mut().enumerate() {
        *c = m.col_start(col);
    }

    let mut number_of_matches = 0;
    for col in 0..n {
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

    (number_of_matches, column_permutations)
}

#[cfg(test)]
mod tests {
    use super::super::recorder::Recorder;
    use super::*;
    use crate::solver::matrix::builder::MatrixBuilder;

    fn build_5x5(triplets: &[(usize, usize)]) -> CscMatrix {
        let mut b = MatrixBuilder::new(5, 5);
        for &(c, r) in triplets {
            b.push(c, r, 1.0).unwrap();
        }
        b.build_csc().unwrap()
    }

    fn run_btf(matrix: &CscMatrix) -> (usize, Vec<isize>) {
        let mut recorder = Recorder::new("/tmp/btf_max_transversal_test.json");
        let result = btf_max_transversal(matrix, &mut recorder);
        recorder.flush().unwrap();
        result
    }

    #[test]
    fn identity_pattern_has_full_matching() {
        // Nonzeros on the diagonal: unique perfect matching
        let a = build_5x5(&[(0, 0), (1, 1), (2, 2), (3, 3), (4, 4)]);
        let (k, q) = run_btf(&a);
        assert_eq!(k, 5);
        assert_eq!(q, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn permuted_diagonal_is_found() {
        // Unique permutation mapping row -> column = [2,0,4,1,3]
        let a = build_5x5(&[(2, 0), (0, 1), (4, 2), (1, 3), (3, 4)]);
        let (k, q) = run_btf(&a);
        assert_eq!(k, 5);
        assert_eq!(q, vec![2, 0, 4, 1, 3]);
    }

    #[test]
    fn rank_deficient_has_four_matchings() {
        // Column 4 is empty; rows 0..3 match uniquely to cols 0..3
        let a = build_5x5(&[(0, 0), (1, 1), (2, 2), (3, 3)]);
        let (k, q) = run_btf(&a);
        assert_eq!(k, 4);
        assert_eq!(q, vec![0, 1, 2, 3, -1]);
    }

    #[test]
    fn chain_requires_augmenting_path_finds_full_match() {
        // Column adjacency (by rows):
        // c0: r0
        // c1: r0, r1
        // c2: r1, r2
        // c3: r2, r3
        // c4: r3, r4
        // Unique full matching exists: row j -> col j
        let a = build_5x5(&[
            (0, 0),
            (1, 0),
            (1, 1),
            (2, 1),
            (2, 2),
            (3, 2),
            (3, 3),
            (4, 3),
            (4, 4),
        ]);
        let (k, q) = run_btf(&a);
        assert_eq!(k, 5);
        assert_eq!(q, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn deep_dfs_augmenting_path_reassigns_chain() {
        // Construct a case where the last column (c4) has only r0, which is already
        // matched when we reach it. An augmenting path must be found:
        // c4 -> r0 -(matched to)-> c0 -> r1 -(matched to)-> c1 -> r2 -(matched to)-> c2
        //     -> r3 -(matched to)-> c3 -> r4 (free)
        // Expected final matching: r0->c4, r1->c0, r2->c1, r3->c2, r4->c3
        // Column adjacency (by rows):
        // c0: r0, r1
        // c1: r1, r2
        // c2: r2, r3
        // c3: r3, r4
        // c4: r0
        let a = build_5x5(&[
            (0, 0),
            (0, 1),
            (1, 1),
            (1, 2),
            (2, 2),
            (2, 3),
            (3, 3),
            (3, 4),
            (4, 0),
        ]);
        let (k, q) = run_btf(&a);
        assert_eq!(k, 5);
        assert_eq!(q, vec![4, 0, 1, 2, 3]);
    }

    #[test]
    fn dfs_with_backtracking_on_decoy_branch() {
        // 7x7 case designed to force DFS to take a wrong turn first, then backtrack.
        // Initial greedy matches (by processing columns 0..5):
        // c0->r0, c1->r1, c2->r2, c3->r3, c4->r4, c5->r5
        // Column c6 only connects to already-matched rows r0 and r2, so DFS starts.
        // At c6, it first tries r0->c0 which dead-ends, backtracks, then tries r2->c2.
        // From c2 it first explores decoy r5->c5 (dead-ends), backtracks, then r3->c3->r4->c4
        // where c4 exposes the free row r6 and the augmenting path succeeds.
        // Final expected matching after augmentation:
        // r0->c0, r1->c1, r2->c6, r3->c2, r4->c3, r5->c5, r6->c4
        let mut b = MatrixBuilder::new(7, 7);
        // c0
        b.push(0, 0, 1.0).unwrap();
        // c1
        b.push(1, 1, 1.0).unwrap();
        // c2 (decoy to c5 via r5 comes before the good edge via r3)
        b.push(2, 2, 1.0).unwrap();
        b.push(2, 5, 1.0).unwrap();
        b.push(2, 3, 1.0).unwrap();
        // c3
        b.push(3, 3, 1.0).unwrap();
        b.push(3, 4, 1.0).unwrap();
        // c4 (only place exposing the free row r6)
        b.push(4, 4, 1.0).unwrap();
        b.push(4, 6, 1.0).unwrap();
        // c5 (decoy branch: loops back to visited columns only)
        b.push(5, 5, 1.0).unwrap();
        b.push(5, 0, 1.0).unwrap();
        // c6 (root of augmenting search; tries r0 dead-end first, then r2)
        b.push(6, 0, 1.0).unwrap();
        b.push(6, 2, 1.0).unwrap();

        let a = b.build_csc().unwrap();
        let (k, q) = run_btf(&a);
        assert_eq!(k, 7);
        assert_eq!(q, vec![0, 1, 6, 2, 3, 5, 4]);
    }
}
