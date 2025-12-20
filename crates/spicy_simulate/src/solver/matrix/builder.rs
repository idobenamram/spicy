use crate::solver::matrix::Dim;
use crate::solver::matrix::csc::CscMatrix;
use crate::solver::matrix::csr::CsrMatrix;
use crate::solver::matrix::error::CscError;
use crate::solver::matrix::error::CsrError;

/// Builder from triplets (COO → canonical CSC).
///
/// Usage:
///   let mut b = MatrixBuilder::new(nrows, ncols);
///   b.reserve(nnz_guess);
///   b.push(i, j, v); ...
///   let a_csc = b.build_csc();      
///   // or build CSR instead:
///   // let a_csr = b.build_csr();
#[derive(Debug)]
pub struct MatrixBuilder {
    dim: Dim,
    /// Sorted triplets (column, row, value)
    entries: Vec<(usize, usize, f64)>,
    /// If true, keep explicit zeros (as stored entries) instead of dropping them.
    keep_zeros: bool,
}

impl MatrixBuilder {
    pub fn new(nrows: usize, ncols: usize) -> Self {
        Self {
            dim: Dim { nrows, ncols },
            entries: Vec::new(),
            keep_zeros: false,
        }
    }

    pub fn new_keep_zeros(nrows: usize, ncols: usize) -> Self {
        Self {
            dim: Dim { nrows, ncols },
            entries: Vec::new(),
            keep_zeros: true,
        }
    }

    pub fn reserve(&mut self, nnz: usize) {
        self.entries.reserve(nnz);
    }

    /// push a COO (column, row, value) tuple
    pub fn push(&mut self, column: usize, row: usize, value: f64) -> Result<(), CscError> {
        if column >= self.dim.ncols {
            return Err(CscError::OutOfBoundsIndex {
                index: column,
                max: self.dim.ncols,
            });
        }
        if row >= self.dim.nrows {
            return Err(CscError::OutOfBoundsIndex {
                index: row,
                max: self.dim.nrows,
            });
        }

        if self.keep_zeros || value != 0.0 {
            // keep entries sorted by (column, row) on insertion
            let key = (column, row);
            let idx = match self
                .entries
                .binary_search_by(|(c, r, _)| (*c, *r).cmp(&key))
            {
                Ok(pos) | Err(pos) => pos,
            };
            self.entries.insert(idx, (column, row, value));
        }

        Ok(())
    }

    pub fn build_csc(self) -> Result<CscMatrix, CscError> {
        let n = self.dim.ncols;
        let keep_zeros = self.keep_zeros;

        // Combine duplicates and drop zeros; entries are already sorted by (col,row)
        let mut combined: Vec<(usize, usize, f64)> = Vec::with_capacity(self.entries.len());
        let mut last_col = usize::MAX;
        let mut last_row = usize::MAX;
        let mut acc = 0.0f64;
        for &(c, r, v) in &self.entries {
            if c == last_col && r == last_row {
                acc += v;
            } else {
                if last_col != usize::MAX && (keep_zeros || acc != 0.0) {
                    combined.push((last_col, last_row, acc));
                }
                last_col = c;
                last_row = r;
                acc = v;
            }
        }
        if last_col != usize::MAX && (keep_zeros || acc != 0.0) {
            combined.push((last_col, last_row, acc));
        }

        // Build CSC arrays with a counting pass then placement pass
        let mut column_pointers = vec![0usize; n + 1];
        for &(c, _r, _v) in &combined {
            column_pointers[c + 1] += 1;
        }
        for j in 0..n {
            column_pointers[j + 1] += column_pointers[j];
        }

        let nnz = combined.len();
        let mut row_indices = vec![0usize; nnz];
        let mut values = vec![0f64; nnz];
        let mut next = column_pointers.clone();
        for (c, r, v) in combined {
            let p = next[c];
            row_indices[p] = r;
            values[p] = v;
            next[c] += 1;
        }

        let a = CscMatrix {
            dim: self.dim,
            column_pointers,
            row_indices,
            values,
        };
        debug_assert!(a.check_invariants().is_ok());
        Ok(a)
    }

    pub fn build_csr(self) -> Result<CsrMatrix, CsrError> {
        let m = self.dim.nrows;
        let keep_zeros = self.keep_zeros;

        // Combine duplicates and drop zeros; sort by (row,col)
        let mut entries = self.entries;
        entries.sort_by(|a, b| (a.1, a.0).cmp(&(b.1, b.0)));

        let mut combined: Vec<(usize, usize, f64)> = Vec::with_capacity(entries.len());
        let mut last_row = usize::MAX;
        let mut last_col = usize::MAX;
        let mut acc = 0.0f64;
        for &(c, r, v) in &entries {
            if r == last_row && c == last_col {
                acc += v;
            } else {
                if last_row != usize::MAX && (keep_zeros || acc != 0.0) {
                    combined.push((last_col, last_row, acc));
                }
                last_row = r;
                last_col = c;
                acc = v;
            }
        }
        if last_row != usize::MAX && (keep_zeros || acc != 0.0) {
            combined.push((last_col, last_row, acc));
        }

        // Build CSR arrays with a counting pass then placement pass
        let mut row_pointers = vec![0usize; m + 1];
        for &(_c, r, _v) in &combined {
            row_pointers[r + 1] += 1;
        }
        for i in 0..m {
            row_pointers[i + 1] += row_pointers[i];
        }

        let nnz = combined.len();
        let mut column_indices = vec![0usize; nnz];
        let mut values = vec![0f64; nnz];
        let mut next = row_pointers.clone();
        for (c, r, v) in combined {
            let p = next[r];
            column_indices[p] = c;
            values[p] = v;
            next[r] += 1;
        }

        let a = CsrMatrix {
            dim: self.dim,
            row_pointers,
            column_indices,
            values,
        };
        debug_assert!(a.check_invariants().is_ok());
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_csc_basic() {
        // A = [ 10  0  3
        //       0 20  0
        //       2  0 30 ] with duplicate (2,2)+=5 => 35
        let mut b = MatrixBuilder::new(3, 3);
        b.push(0, 0, 10.0).unwrap();
        b.push(2, 0, 3.0).unwrap();
        b.push(1, 1, 20.0).unwrap();
        b.push(0, 2, 2.0).unwrap();
        b.push(2, 2, 30.0).unwrap();
        b.push(2, 2, 5.0).unwrap(); // duplicate -> combine to 35

        let a = b.build_csc().unwrap();
        assert_eq!(a.column_pointers, vec![0, 2, 3, 5]);
        assert_eq!(a.row_indices, vec![0, 2, 1, 0, 2]);
        assert_eq!(a.values, vec![10.0, 2.0, 20.0, 3.0, 35.0]);
        debug_assert!(a.check_invariants().is_ok());
    }

    #[test]
    fn build_csr_basic() {
        // same matrix as above
        let mut b = MatrixBuilder::new(3, 3);
        b.push(0, 0, 10.0).unwrap();
        b.push(2, 0, 3.0).unwrap();
        b.push(1, 1, 20.0).unwrap();
        b.push(0, 2, 2.0).unwrap();
        b.push(2, 2, 30.0).unwrap();
        b.push(2, 2, 5.0).unwrap();

        let a = b.build_csr().unwrap();
        assert_eq!(a.row_pointers, vec![0, 2, 3, 5]);
        assert_eq!(a.column_indices, vec![0, 2, 1, 0, 2]);
        assert_eq!(a.values, vec![10.0, 3.0, 20.0, 2.0, 35.0]);
        debug_assert!(a.check_invariants().is_ok());
    }

    #[test]
    fn csc_transpose_matches_builder_csr() {
        let mut b1 = MatrixBuilder::new(3, 3);
        let mut b2 = MatrixBuilder::new(3, 3);
        let entries = vec![
            (0, 0, 10.0),
            (2, 0, 3.0),
            (1, 1, 20.0),
            (0, 2, 2.0),
            (2, 2, 30.0),
            (2, 2, 5.0),
        ];
        for (c, r, v) in &entries {
            b1.push(*c, *r, *v).unwrap();
            b2.push(*c, *r, *v).unwrap();
        }
        let csc = b1.build_csc().unwrap();
        let csr_from_transpose = csc.transpose_to_csr();
        let csr_direct = b2.build_csr().unwrap();

        assert_eq!(csr_from_transpose.row_pointers, csr_direct.row_pointers);
        assert_eq!(csr_from_transpose.column_indices, csr_direct.column_indices);
        assert_eq!(csr_from_transpose.values, csr_direct.values);
    }
}
