use crate::solver::error::CscError;

/// Compressed Sparse Column
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dim {
    pub nrows: usize,
    pub ncols: usize,
}

/// Compressed Sparse Column matrix
/// - column pointers are the indices of the start and end of each column
/// - row indices are the indices of the rows of the non zero values
/// - values are the non zero values
#[derive(Debug, Clone)]
pub struct CscMatrix {
    pub dim: Dim,
    /// Column pointers, len = ncols + 1
    pub column_pointers: Vec<usize>,
    /// Row indices, len = nnz
    pub row_indices: Vec<usize>,
    /// Nonzero values, len = nnz
    pub values: Vec<f64>,
}

impl CscMatrix {
    /// number of non zero values
    pub fn nnz(&self) -> usize {
        self.row_indices.len()
    }

    pub fn check_invariants(&self) -> Result<(), CscError> {
        if self.column_pointers.len() != self.dim.ncols + 1 {
            return Err(CscError::InvalidColumnPointersLength {
                expected: self.dim.ncols + 1,
                actual: self.column_pointers.len(),
            });
        }
        if *self.column_pointers.first().unwrap_or(&1) != 0 {
            return Err(CscError::InvalidColumnPointers {
                index: 0,
                expected: 0,
                actual: *self.column_pointers.first().unwrap_or(&1),
            });
        }
        if *self.column_pointers.last().unwrap() != self.nnz() {
            return Err(CscError::InvalidColumnPointers {
                index: self.dim.ncols,
                expected: self.nnz(),
                actual: *self.column_pointers.last().unwrap(),
            });
        }
        if self.row_indices.len() != self.values.len() {
            return Err(CscError::RowIndicesValuesLengthMismatch {
                values: self.values.len(),
                row_indices: self.row_indices.len(),
            });
        }
        // per-column sorted & in-range
        for j in 0..self.dim.ncols {
            let (start, end) = (self.column_pointers[j], self.column_pointers[j + 1]);
            if start > end || end > self.nnz() {
                return Err(CscError::InvalidColumnPointers {
                    index: j,
                    expected: start,
                    actual: end,
                });
            }
            let mut prev = None;
            for &r in &self.row_indices[start..end] {
                if r >= self.dim.nrows {
                    return Err(CscError::OutOfBoundsIndex {
                        index: r,
                        max: self.dim.nrows,
                    });
                }
                if let Some(p) = prev {
                    if r <= p {
                        return Err(CscError::RowsNotStrictlyIncreasing {
                            index: j,
                            expected: p,
                            actual: r,
                        });
                    }
                }
                prev = Some(r);
            }
        }
        Ok(())
    }

    /// Return (row_indices, values) slice for column j
    pub fn col(&self, j: usize) -> (&[usize], &[f64]) {
        let (s, e) = (self.column_pointers[j], self.column_pointers[j + 1]);
        (&self.row_indices[s..e], &self.values[s..e])
    }

    /// y[rows] += alpha * x (in-place axpy into sparse positions).
    pub fn axpy_into_dense_col(&self, j: usize, x: f64, y: &mut [f64]) {
        let (rows, vals) = self.col(j);
        for (&i, &a) in rows.iter().zip(vals.iter()) {
            y[i] += x * a;
        }
    }

    /// Scatter column j into dense work vector w with scaling:
    /// for k in col(j): w[row[k]] = alpha * val[k]; returns number of entries written.
    /// Caller typically tracks a separate pattern stack; here we return count for convenience.
    pub fn scatter_col(&self, j: usize, alpha: f64, w: &mut [f64]) -> usize {
        let (rows, vals) = self.col(j);
        for (&i, &a) in rows.iter().zip(vals.iter()) {
            w[i] = alpha * a;
        }
        rows.len()
    }

    /// Apply a column permutation: returns A(:, q) where q maps new_j -> old_j (i.e., A * P)
    pub fn permute_columns(&self, q: &[usize]) -> CscMatrix {
        assert_eq!(q.len(), self.dim.ncols);
        let mut column_pointers = Vec::with_capacity(self.dim.ncols + 1);
        column_pointers.push(0);
        let mut row_indices = Vec::with_capacity(self.nnz());
        let mut values = Vec::with_capacity(self.nnz());

        for &old_j in q {
            let (rows, vals) = self.col(old_j);
            column_pointers.push(column_pointers.last().unwrap() + rows.len());
            row_indices.extend_from_slice(rows);
            values.extend_from_slice(vals);
        }
        CscMatrix {
            dim: self.dim.clone(),
            column_pointers,
            row_indices,
            values,
        }
    }

    /// Transpose into CSR (useful for building elimination trees or symbolic steps).
    /// This is O(n + nnz) with counting sort by row.
    pub fn transpose_to_csr(&self) -> (Vec<usize>, Vec<usize>, Vec<f64>) {
        let m = self.dim.nrows;
        let n = self.dim.ncols;
        let nnz = self.nnz();

        let mut rp = vec![0usize; m + 1];
        // count entries per row
        for &r in &self.row_indices {
            rp[r + 1] += 1;
        }
        // prefix sum
        for i in 0..m {
            rp[i + 1] += rp[i];
        }

        let mut ci = vec![0usize; nnz];
        let mut cx = vec![0f64; nnz];
        let mut next = rp.clone();

        for j in 0..n {
            let (rows, vals) = self.col(j);
            for (&r, &v) in rows.iter().zip(vals.iter()) {
                let p = next[r];
                ci[p] = j;
                cx[p] = v;
                next[r] += 1;
            }
        }
        (rp, ci, cx)
    }
}

/// Builder from triplets (COO → canonical CSC).
///
/// Usage:
///   let mut b = CscBuilder::new(nrows, ncols);
///   b.reserve(nnz_guess);
///   b.push(i, j, v); ...
///   let a = b.build();  // sorted rows per col, duplicates summed, zeros dropped
#[derive(Debug)]
pub struct CscBuilder {
    dim: Dim,
    /// Sorted triplets (column, row, value)
    entries: Vec<(usize, usize, f64)>,
}

impl CscBuilder {
    pub fn new(nrows: usize, ncols: usize) -> Self {
        Self {
            dim: Dim { nrows, ncols },
            entries: Vec::new(),
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

        if value != 0.0 {
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

    pub fn build(self) -> Result<CscMatrix, CscError> {
        let n = self.dim.ncols;

        // Combine duplicates and drop zeros; entries are already sorted by (col,row)
        let mut combined: Vec<(usize, usize, f64)> = Vec::with_capacity(self.entries.len());
        let mut last_col = usize::MAX;
        let mut last_row = usize::MAX;
        let mut acc = 0.0f64;
        for &(c, r, v) in &self.entries {
            if c == last_col && r == last_row {
                acc += v;
            } else {
                if last_col != usize::MAX && acc != 0.0 {
                    combined.push((last_col, last_row, acc));
                }
                last_col = c;
                last_row = r;
                acc = v;
            }
        }
        if last_col != usize::MAX && acc != 0.0 {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_access() {
        // A = [ 10  0  3
        //       0 20  0
        //       2  0 30 ]
        let mut b = CscBuilder::new(3, 3);
        b.push(0, 0, 10.0).unwrap();
        b.push(2, 0, 3.0).unwrap();
        b.push(1, 1, 20.0).unwrap();
        b.push(0, 2, 2.0).unwrap();
        b.push(2, 2, 30.0).unwrap();
        // also push a duplicate to test combine
        b.push(2, 2, 5.0).unwrap();

        let a = b.build().unwrap();
        assert_eq!(a.nnz(), 6 - 1); // 5 unique nonzeros after combine

        // Column 0 -> rows [0,2] vals [10,2]
        let (r0, v0) = a.col(0);
        assert_eq!(r0, &[0, 2]);
        assert_eq!(v0, &[10.0, 2.0]);

        // Column 2 -> rows [0,2] vals [3,35]
        let (r2, v2) = a.col(2);
        assert_eq!(r2, &[0, 2]);
        assert_eq!(v2, &[3.0, 35.0]);

        assert!(a.check_invariants().is_ok());
    }

    #[test]
    fn transpose_roundtrip_shape() {
        let mut b = CscBuilder::new(3, 3);
        // A = [ 1  2  0
        //       0  0  0 
        //       0  3  0 ]
        b.push(0, 0, 1.0).unwrap();
        b.push(1, 0, 2.0).unwrap();
        b.push(1, 2, 3.0).unwrap();
        let a = b.build().unwrap();
        let (rp, ci, _cx) = a.transpose_to_csr();
        // CSR rows = 3; rp len = 4
        assert_eq!(rp.len(), 4);
        // nnz preserved
        assert_eq!(*rp.last().unwrap(), a.nnz());
        // some sanity on column indices
        assert!(ci.iter().all(|&j| j < a.dim.ncols));
    }
}
