use crate::solver::matrix::builder::MatrixBuilder;
use crate::solver::matrix::csc::CscMatrix;
use crate::solver::matrix::error::{MatrixError, MatrixMarketError};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MmField {
    Integer,
    Real,
}

/// Load a sparse matrix from a MatrixMarket `.mtx` file (coordinate format) into a canonical CSC.
///
/// Supports:
/// - banner: `%%MatrixMarket matrix coordinate {integer|real} general`
/// - 1-based indices in the file, converted to 0-based indices internally.
pub fn load_matrix_market_csc_file(path: impl AsRef<Path>) -> Result<CscMatrix, MatrixError> {
    let f = File::open(path.as_ref()).map_err(MatrixMarketError::from)?;
    let reader = BufReader::new(f);
    load_matrix_market_csc_from_reader(reader)
}

/// Same as [`load_matrix_market_csc_file`], but reads from any buffered reader (useful for tests).
pub fn load_matrix_market_csc_from_reader<R: BufRead>(reader: R) -> Result<CscMatrix, MatrixError> {
    let mut lines = reader.lines().enumerate();

    // Header (first non-empty line)
    let (header_line_no, header) = loop {
        match lines.next() {
            None => {
                return Err(
                    MatrixMarketError::InvalidBanner("empty input".to_string()).into(),
                )
            }
            Some((i, line)) => {
                let line = line.map_err(MatrixMarketError::from)?;
                let t = line.trim();
                if t.is_empty() {
                    continue;
                }
                // tolerate BOM
                let t = t.trim_start_matches('\u{feff}');
                break (i + 1, t.to_string());
            }
        }
    };

    let tokens: Vec<&str> = header.split_whitespace().collect();
    if tokens.len() != 5 {
        return Err(MatrixMarketError::InvalidBanner(format!(
            "expected 5 tokens, got {} at line {}: {:?}",
            tokens.len(),
            header_line_no,
            header
        ))
        .into());
    }

    let mm = tokens[0];
    let object = tokens[1].to_ascii_lowercase();
    let format = tokens[2].to_ascii_lowercase();
    let field = tokens[3].to_ascii_lowercase();
    let symmetry = tokens[4].to_ascii_lowercase();

    if mm != "%%MatrixMarket" {
        return Err(MatrixMarketError::InvalidBanner(format!(
            "missing %%MatrixMarket at line {}: {}",
            header_line_no, header
        ))
        .into());
    }
    if object != "matrix" || format != "coordinate" {
        return Err(MatrixMarketError::UnsupportedType(format!(
            "only 'matrix coordinate' is supported, got '{}' '{}' (line {}): {}",
            tokens[1], tokens[2], header_line_no, header
        ))
        .into());
    }
    if symmetry != "general" {
        return Err(MatrixMarketError::UnsupportedType(format!(
            "only 'general' symmetry is supported, got '{}' (line {}): {}",
            tokens[4], header_line_no, header
        ))
        .into());
    }

    let field = match field.as_str() {
        "integer" => MmField::Integer,
        "real" => MmField::Real,
        other => {
            return Err(MatrixMarketError::UnsupportedType(format!(
                "only 'integer' and 'real' fields are supported, got '{}' (line {}): {}",
                other, header_line_no, header
            ))
            .into())
        }
    };

    // Size line (skip comments/empty)
    let (size_line_no, size_line) = loop {
        match lines.next() {
            None => {
                return Err(
                    MatrixMarketError::InvalidSizeLine("missing size line".to_string()).into(),
                )
            }
            Some((i, line)) => {
                let line = line.map_err(MatrixMarketError::from)?;
                let t = line.trim();
                if t.is_empty() || t.starts_with('%') {
                    continue;
                }
                break (i + 1, t.to_string());
            }
        }
    };

    let parts: Vec<&str> = size_line.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(MatrixMarketError::InvalidSizeLine(format!(
            "expected 3 integers at line {}: {}",
            size_line_no, size_line
        ))
        .into());
    }
    let nrows: usize = parts[0].parse().map_err(|e| {
        MatrixMarketError::InvalidSizeLine(format!(
            "bad nrows '{}' at line {}: {} ({})",
            parts[0], size_line_no, size_line, e
        ))
    })?;
    let ncols: usize = parts[1].parse().map_err(|e| {
        MatrixMarketError::InvalidSizeLine(format!(
            "bad ncols '{}' at line {}: {} ({})",
            parts[1], size_line_no, size_line, e
        ))
    })?;
    let nnz: usize = parts[2].parse().map_err(|e| {
        MatrixMarketError::InvalidSizeLine(format!(
            "bad nnz '{}' at line {}: {} ({})",
            parts[2], size_line_no, size_line, e
        ))
    })?;

    let mut b = MatrixBuilder::new(nrows, ncols);
    b.reserve(nnz);

    let mut read_entries = 0usize;
    for (i, line) in lines {
        let line_no = i + 1;
        let line = line.map_err(MatrixMarketError::from)?;
        let t = line.trim();
        if t.is_empty() || t.starts_with('%') {
            continue;
        }
        if read_entries >= nnz {
            return Err(MatrixMarketError::InvalidEntry {
                line: line_no,
                msg: format!("found more than nnz={} entries", nnz),
            }
            .into());
        }

        let parts: Vec<&str> = t.split_whitespace().collect();
        if parts.len() != 3 {
            return Err(MatrixMarketError::InvalidEntry {
                line: line_no,
                msg: format!("expected 3 tokens 'row col val', got: {}", t),
            }
            .into());
        }

        let row_1: usize = parts[0].parse().map_err(|e| MatrixMarketError::InvalidEntry {
            line: line_no,
            msg: format!("bad row index '{}': {}", parts[0], e),
        })?;
        let col_1: usize = parts[1].parse().map_err(|e| MatrixMarketError::InvalidEntry {
            line: line_no,
            msg: format!("bad col index '{}': {}", parts[1], e),
        })?;

        if row_1 == 0 || col_1 == 0 {
            return Err(MatrixMarketError::InvalidEntry {
                line: line_no,
                msg: "MatrixMarket indices are 1-based; found 0".to_string(),
            }
            .into());
        }

        let row = row_1 - 1;
        let col = col_1 - 1;

        let val = match field {
            MmField::Integer => {
                let v: i64 = parts[2].parse().map_err(|e| MatrixMarketError::InvalidEntry {
                    line: line_no,
                    msg: format!("bad integer value '{}': {}", parts[2], e),
                })?;
                v as f64
            }
            MmField::Real => {
                let v: f64 = parts[2].parse().map_err(|e| MatrixMarketError::InvalidEntry {
                    line: line_no,
                    msg: format!("bad real value '{}': {}", parts[2], e),
                })?;
                v
            }
        };

        // MatrixBuilder expects (column, row, value)
        b.push(col, row, val)?;
        read_entries += 1;
    }

    if read_entries != nnz {
        return Err(MatrixMarketError::EntryCountMismatch {
            expected: nnz,
            actual: read_entries,
        }
        .into());
    }

    Ok(b.build_csc()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parse_small_integer_coordinate_general() {
        // 3x3 with duplicates in same position (1,1) and a comment line.
        let mtx = r#"
%%MatrixMarket matrix coordinate integer general
% a comment
3 3 4
1 1 2
1 1 3
3 1 4
2 3 5
"#;

        let a = load_matrix_market_csc_from_reader(Cursor::new(mtx)).unwrap();
        debug_assert!(a.check_invariants().is_ok());

        assert_eq!(a.dim.nrows, 3);
        assert_eq!(a.dim.ncols, 3);
        // After combining duplicates at (row=0,col=0): 2+3=5 => 3 unique nnz
        assert_eq!(a.nnz(), 3);

        // Column 0 has rows [0,2] vals [5,4]
        let (r0, v0) = a.col(0);
        assert_eq!(r0, &[0, 2]);
        assert_eq!(v0, &[5.0, 4.0]);

        // Column 2 has row [1] val [5]
        let (r2, v2) = a.col(2);
        assert_eq!(r2, &[1]);
        assert_eq!(v2, &[5.0]);
    }

    #[test]
    fn rejects_non_general_symmetry() {
        let mtx = r#"%%MatrixMarket matrix coordinate integer symmetric
2 2 1
1 1 1
"#;
        let err = load_matrix_market_csc_from_reader(Cursor::new(mtx)).unwrap_err();
        let s = format!("{err}");
        assert!(s.contains("only 'general' symmetry is supported"));
    }
}


