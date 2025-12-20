use crate::solver::matrix::Dim;
use crate::solver::matrix::error::CsrError;

/// Compressed Sparse Row matrix
/// - row pointers are the indices of the start and end of each row
/// - column indices are the indices of the columns of the non zero values
/// - values are the non zero values
#[derive(Debug, Clone)]
pub struct CsrMatrix {
    pub dim: Dim,
    /// Row pointers, len = nrows + 1
    pub row_pointers: Vec<usize>,
    /// Column indices, len = nnz
    pub column_indices: Vec<usize>,
    /// Nonzero values, len = nnz
    pub values: Vec<f64>,
}

impl CsrMatrix {
    /// number of non zero values
    pub fn nnz(&self) -> usize {
        self.column_indices.len()
    }

    #[allow(clippy::collapsible_if)]
    pub fn check_invariants(&self) -> Result<(), CsrError> {
        if self.row_pointers.len() != self.dim.nrows + 1 {
            return Err(CsrError::InvalidRowPointersLength {
                expected: self.dim.nrows + 1,
                actual: self.row_pointers.len(),
            });
        }
        if *self.row_pointers.first().unwrap_or(&1) != 0 {
            return Err(CsrError::InvalidRowPointers {
                index: 0,
                expected: 0,
                actual: *self.row_pointers.first().unwrap_or(&1),
            });
        }
        if *self.row_pointers.last().unwrap() != self.nnz() {
            return Err(CsrError::InvalidRowPointers {
                index: self.dim.nrows,
                expected: self.nnz(),
                actual: *self.row_pointers.last().unwrap(),
            });
        }
        if self.column_indices.len() != *self.row_pointers.last().unwrap() {
            return Err(CsrError::ColumnIndicesValuesLengthMismatch {
                values: *self.row_pointers.last().unwrap(),
                column_indices: self.column_indices.len(),
            });
        }
        if self.column_indices.len() != self.values.len() {
            return Err(CsrError::ColumnIndicesValuesLengthMismatch {
                values: self.values.len(),
                column_indices: self.column_indices.len(),
            });
        }
        // per-row sorted & in-range
        for i in 0..self.dim.nrows {
            let (start, end) = (self.row_pointers[i], self.row_pointers[i + 1]);
            if start > end || end > self.nnz() {
                return Err(CsrError::InvalidRowPointers {
                    index: i,
                    expected: start,
                    actual: end,
                });
            }
            let mut prev = None;
            for &c in &self.column_indices[start..end] {
                if c >= self.dim.ncols {
                    return Err(CsrError::OutOfBoundsIndex {
                        index: c,
                        max: self.dim.ncols,
                    });
                }
                if let Some(p) = prev {
                    if c <= p {
                        return Err(CsrError::ColumnsNotStrictlyIncreasing {
                            index: i,
                            expected: p,
                            actual: c,
                        });
                    }
                }
                prev = Some(c);
            }
        }
        Ok(())
    }

    /// Return (column_indices, values) slice for row i
    pub fn row(&self, i: usize) -> (&[usize], &[f64]) {
        let (s, e) = (self.row_pointers[i], self.row_pointers[i + 1]);
        (&self.column_indices[s..e], &self.values[s..e])
    }

    pub fn row_start(&self, i: usize) -> usize {
        self.row_pointers[i]
    }

    pub fn col_index(&self, k: usize) -> usize {
        self.column_indices[k]
    }
}

#[cfg(test)]
mod tests {
    use crate::solver::matrix::builder::MatrixBuilder;

    #[test]
    fn build_and_access_rows() {
        // A as used in CSC tests
        let mut b = MatrixBuilder::new(3, 3);
        b.push(0, 0, 10.0).unwrap();
        b.push(2, 0, 3.0).unwrap();
        b.push(1, 1, 20.0).unwrap();
        b.push(0, 2, 2.0).unwrap();
        b.push(2, 2, 30.0).unwrap();
        b.push(2, 2, 5.0).unwrap();

        let a = b.build_csr().unwrap();
        assert_eq!(a.nnz(), 5);
        assert_eq!(a.row_pointers, vec![0, 2, 3, 5]);

        // Row 0 -> cols [0,2] vals [10,3]
        let (c0, v0) = a.row(0);
        assert_eq!(c0, &[0, 2]);
        assert_eq!(v0, &[10.0, 3.0]);

        // Row 1 -> cols [1] vals [20]
        let (c1, v1) = a.row(1);
        assert_eq!(c1, &[1]);
        assert_eq!(v1, &[20.0]);

        // Row 2 -> cols [0,2] vals [2,35]
        let (c2, v2) = a.row(2);
        assert_eq!(c2, &[0, 2]);
        assert_eq!(v2, &[2.0, 35.0]);

        debug_assert!(a.check_invariants().is_ok());
    }
}
