use crate::solver::matrix::builder::MatrixBuilder;
use crate::solver::matrix::csc::CscMatrix;
use crate::solver::matrix::error::{MatrixError, MatrixMarketError};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// A larger-than-default buffer helps for huge `.mtx` files (tens of millions of lines).
///
/// Note: parsing tends to dominate, but this reduces syscalls and iterator overhead on some OSes.
const MTX_READER_CAPACITY_BYTES: usize = 1024 * 1024; // 1 MiB

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MmField {
    Integer,
    Real,
}

#[inline]
fn ascii_eq_ignore_case(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(&x, &y)| x.eq_ignore_ascii_case(&y))
}

#[inline]
fn parse_usize_ascii(tok: &[u8]) -> Option<usize> {
    if tok.is_empty() {
        return None;
    }
    let mut v: usize = 0;
    for &b in tok {
        if !b.is_ascii_digit() {
            return None;
        }
        #[cfg(debug_assertions)]
        {
            v = v.checked_mul(10)?.checked_add((b - b'0') as usize)?;
        }
        #[cfg(not(debug_assertions))]
        {
            v = v * 10 + (b - b'0') as usize;
        }
    }
    Some(v)
}

#[inline]
fn parse_f64_ascii(tok: &[u8]) -> Option<f64> {
    // Safety: MatrixMarket numeric tokens are ASCII (a subset of UTF-8).
    let s = unsafe { std::str::from_utf8_unchecked(tok) };
    s.parse::<f64>().ok()
}

#[inline]
fn line_lossy_trimmed(line: &[u8]) -> String {
    String::from_utf8_lossy(line).trim_end().to_string()
}

#[inline]
fn skip_ws_ascii(buf: &[u8], i: &mut usize) {
    while *i < buf.len() && buf[*i] <= b' ' {
        *i += 1;
    }
}

#[inline]
fn parse_usize_at(buf: &[u8], i: &mut usize) -> Option<usize> {
    if *i >= buf.len() || !buf[*i].is_ascii_digit() {
        return None;
    }
    let mut v: usize = 0;
    while *i < buf.len() {
        let b = buf[*i];
        if !b.is_ascii_digit() {
            break;
        }
        #[cfg(debug_assertions)]
        {
            v = v.checked_mul(10)?.checked_add((b - b'0') as usize)?;
        }
        #[cfg(not(debug_assertions))]
        {
            v = v * 10 + (b - b'0') as usize;
        }
        *i += 1;
    }
    Some(v)
}

#[inline]
fn parse_i64_at(buf: &[u8], i: &mut usize) -> Option<i64> {
    if *i >= buf.len() {
        return None;
    }
    let mut sign: i64 = 1;
    match buf[*i] {
        b'+' => *i += 1,
        b'-' => {
            sign = -1;
            *i += 1;
        }
        _ => {}
    }
    if *i >= buf.len() || !buf[*i].is_ascii_digit() {
        return None;
    }
    let mut v: i64 = 0;
    while *i < buf.len() {
        let b = buf[*i];
        if !b.is_ascii_digit() {
            break;
        }
        #[cfg(debug_assertions)]
        {
            v = v.checked_mul(10)?.checked_add((b - b'0') as i64)?;
        }
        #[cfg(not(debug_assertions))]
        {
            v = v * 10 + (b - b'0') as i64;
        }
        *i += 1;
    }
    if sign == 1 {
        Some(v)
    } else {
        #[cfg(debug_assertions)]
        {
            v.checked_neg()
        }
        #[cfg(not(debug_assertions))]
        {
            Some(-v)
        }
    }
}

#[inline]
fn parse_f64_token_at(buf: &[u8], i: &mut usize) -> Option<f64> {
    let start = *i;
    while *i < buf.len() && buf[*i] > b' ' {
        *i += 1;
    }
    if *i == start {
        return None;
    }
    parse_f64_ascii(&buf[start..*i])
}

struct TokenIter<'a> {
    buf: &'a [u8],
    i: usize,
}

impl<'a> TokenIter<'a> {
    #[inline]
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, i: 0 }
    }

    #[inline]
    fn next(&mut self) -> Option<&'a [u8]> {
        // Fast-path whitespace for MatrixMarket: treat any ASCII control char (<= ' ')
        // as whitespace. This is slightly more permissive than `is_ascii_whitespace()`,
        // but MatrixMarket files are ASCII text and this avoids an extra range check.
        while self.i < self.buf.len() && self.buf[self.i] <= b' ' {
            self.i += 1;
        }
        if self.i >= self.buf.len() {
            return None;
        }
        let start = self.i;
        while self.i < self.buf.len() && self.buf[self.i] > b' ' {
            self.i += 1;
        }
        Some(&self.buf[start..self.i])
    }
}

/// Load a sparse matrix from a MatrixMarket `.mtx` file (coordinate format) into a canonical CSC.
///
/// Supports:
/// - banner: `%%MatrixMarket matrix coordinate {integer|real} general`
/// - 1-based indices in the file, converted to 0-based indices internally.
pub fn load_matrix_market_csc_file(path: impl AsRef<Path>) -> Result<CscMatrix, MatrixError> {
    let f = File::open(path.as_ref()).map_err(MatrixMarketError::from)?;
    let reader = BufReader::with_capacity(MTX_READER_CAPACITY_BYTES, f);
    load_matrix_market_csc_from_reader(reader)
}

/// Load a sparse matrix from a MatrixMarket `.mtx` file (coordinate format) into CSC,
/// **preserving explicit zero entries** from the file (i.e., they are kept as stored entries).
pub fn load_matrix_market_csc_file_keep_zeros(
    path: impl AsRef<Path>,
) -> Result<CscMatrix, MatrixError> {
    let f = File::open(path.as_ref()).map_err(MatrixMarketError::from)?;
    let reader = BufReader::with_capacity(MTX_READER_CAPACITY_BYTES, f);
    load_matrix_market_csc_from_reader_keep_zeros(reader)
}

/// Same as [`load_matrix_market_csc_file`], but reads from any buffered reader (useful for tests).
pub fn load_matrix_market_csc_from_reader<R: BufRead>(reader: R) -> Result<CscMatrix, MatrixError> {
    load_matrix_market_csc_from_reader_impl(reader, false)
}

/// Same as [`load_matrix_market_csc_file_keep_zeros`], but reads from any buffered reader.
pub fn load_matrix_market_csc_from_reader_keep_zeros<R: BufRead>(
    reader: R,
) -> Result<CscMatrix, MatrixError> {
    load_matrix_market_csc_from_reader_impl(reader, true)
}

fn load_matrix_market_csc_from_reader_impl<R: BufRead>(
    mut reader: R,
    keep_zeros: bool,
) -> Result<CscMatrix, MatrixError> {
    // Performance notes:
    // - Use `read_until(b'\n', ...)` to reuse a single `Vec<u8>` buffer and avoid per-line `String`
    //   allocations + UTF-8 validation.
    // - Tokenize by scanning bytes (no `split_whitespace().collect()` allocations).
    let mut buf: Vec<u8> = Vec::with_capacity(256);
    let mut line_no: usize = 0;

    // Header (first non-empty line)
    let field = loop {
        buf.clear();
        let nread = reader
            .read_until(b'\n', &mut buf)
            .map_err(MatrixMarketError::from)?;
        if nread == 0 {
            return Err(MatrixMarketError::InvalidBanner("empty input".to_string()).into());
        }
        line_no += 1;

        // tolerate UTF-8 BOM (0xEF 0xBB 0xBF) at start of file
        let mut line: &[u8] = &buf;
        if line_no == 1 && line.starts_with(&[0xEF, 0xBB, 0xBF]) {
            line = &line[3..];
        }

        let mut it = TokenIter::new(line);
        let mm = match it.next() {
            None => continue,
            Some(mm) => mm,
        };

        let object = it.next().ok_or_else(|| {
            MatrixMarketError::InvalidBanner(format!(
                "expected 5 tokens, got 1 at line {}: {}",
                line_no,
                line_lossy_trimmed(line)
            ))
        })?;
        let format = it.next().ok_or_else(|| {
            MatrixMarketError::InvalidBanner(format!(
                "expected 5 tokens, got 2 at line {}: {}",
                line_no,
                line_lossy_trimmed(line)
            ))
        })?;
        let field_s = it.next().ok_or_else(|| {
            MatrixMarketError::InvalidBanner(format!(
                "expected 5 tokens, got 3 at line {}: {}",
                line_no,
                line_lossy_trimmed(line)
            ))
        })?;
        let symmetry = it.next().ok_or_else(|| {
            MatrixMarketError::InvalidBanner(format!(
                "expected 5 tokens, got 4 at line {}: {}",
                line_no,
                line_lossy_trimmed(line)
            ))
        })?;
        if it.next().is_some() {
            return Err(MatrixMarketError::InvalidBanner(format!(
                "expected 5 tokens, got more at line {}: {}",
                line_no,
                line_lossy_trimmed(line)
            ))
            .into());
        }

        if mm != b"%%MatrixMarket" {
            return Err(MatrixMarketError::InvalidBanner(format!(
                "missing %%MatrixMarket at line {}: {}",
                line_no,
                line_lossy_trimmed(line)
            ))
            .into());
        }
        if !ascii_eq_ignore_case(object, b"matrix") || !ascii_eq_ignore_case(format, b"coordinate")
        {
            return Err(MatrixMarketError::UnsupportedType(format!(
                "only 'matrix coordinate' is supported, got '{}' '{}' (line {}): {}",
                line_lossy_trimmed(object),
                line_lossy_trimmed(format),
                line_no,
                line_lossy_trimmed(line)
            ))
            .into());
        }
        if !ascii_eq_ignore_case(symmetry, b"general") {
            return Err(MatrixMarketError::UnsupportedType(format!(
                "only 'general' symmetry is supported, got '{}' (line {}): {}",
                line_lossy_trimmed(symmetry),
                line_no,
                line_lossy_trimmed(line)
            ))
            .into());
        }

        let field = if ascii_eq_ignore_case(field_s, b"integer") {
            MmField::Integer
        } else if ascii_eq_ignore_case(field_s, b"real") {
            MmField::Real
        } else {
            return Err(MatrixMarketError::UnsupportedType(format!(
                "only 'integer' and 'real' fields are supported, got '{}' (line {}): {}",
                line_lossy_trimmed(field_s),
                line_no,
                line_lossy_trimmed(line)
            ))
            .into());
        };

        break field;
    };

    // Size line (skip comments/empty)
    let (nrows, ncols, nnz) = loop {
        buf.clear();
        let nread = reader
            .read_until(b'\n', &mut buf)
            .map_err(MatrixMarketError::from)?;
        if nread == 0 {
            return Err(MatrixMarketError::InvalidSizeLine("missing size line".to_string()).into());
        }
        line_no += 1;

        let mut it = TokenIter::new(&buf);
        let first = match it.next() {
            None => continue,
            Some(t) => t,
        };
        if first.first() == Some(&b'%') {
            continue;
        }

        let nrows_s = first;
        let ncols_s = it.next().ok_or_else(|| {
            MatrixMarketError::InvalidSizeLine(format!(
                "expected 3 integers at line {}: {}",
                line_no,
                line_lossy_trimmed(&buf)
            ))
        })?;
        let nnz_s = it.next().ok_or_else(|| {
            MatrixMarketError::InvalidSizeLine(format!(
                "expected 3 integers at line {}: {}",
                line_no,
                line_lossy_trimmed(&buf)
            ))
        })?;
        if it.next().is_some() {
            return Err(MatrixMarketError::InvalidSizeLine(format!(
                "expected 3 integers at line {}: {}",
                line_no,
                line_lossy_trimmed(&buf)
            ))
            .into());
        }

        let nrows: usize = parse_usize_ascii(nrows_s).ok_or_else(|| {
            MatrixMarketError::InvalidSizeLine(format!(
                "bad nrows '{}' at line {}: {}",
                line_lossy_trimmed(nrows_s),
                line_no,
                line_lossy_trimmed(&buf)
            ))
        })?;
        let ncols: usize = parse_usize_ascii(ncols_s).ok_or_else(|| {
            MatrixMarketError::InvalidSizeLine(format!(
                "bad ncols '{}' at line {}: {}",
                line_lossy_trimmed(ncols_s),
                line_no,
                line_lossy_trimmed(&buf)
            ))
        })?;
        let nnz: usize = parse_usize_ascii(nnz_s).ok_or_else(|| {
            MatrixMarketError::InvalidSizeLine(format!(
                "bad nnz '{}' at line {}: {}",
                line_lossy_trimmed(nnz_s),
                line_no,
                line_lossy_trimmed(&buf)
            ))
        })?;

        break (nrows, ncols, nnz);
    };

    let mut b = MatrixBuilder::new(nrows, ncols);
    b.reserve(nnz);

    let mut read_entries = 0usize;
    loop {
        buf.clear();
        let nread = reader
            .read_until(b'\n', &mut buf)
            .map_err(MatrixMarketError::from)?;
        if nread == 0 {
            break;
        }
        line_no += 1;

        // Avoid tokenization for the hot-path entry lines: parse `row col val` in a single
        // cursor pass (scan whitespace once, parse digits inline).
        let mut idx: usize = 0;
        skip_ws_ascii(&buf, &mut idx);
        if idx >= buf.len() {
            continue;
        }
        if buf[idx] == b'%' {
            continue;
        }
        if read_entries >= nnz {
            return Err(MatrixMarketError::InvalidEntry {
                line: line_no,
                msg: format!("found more than nnz={} entries", nnz),
            }
            .into());
        }

        // row
        let row_tok_start = idx;
        let row_1: usize = parse_usize_at(&buf, &mut idx).ok_or_else(|| {
            // capture the offending token for a nicer message
            let mut end = row_tok_start;
            while end < buf.len() && buf[end] > b' ' {
                end += 1;
            }
            MatrixMarketError::InvalidEntry {
                line: line_no,
                msg: format!(
                    "bad row index '{}'",
                    line_lossy_trimmed(&buf[row_tok_start..end])
                ),
            }
        })?;

        // col
        skip_ws_ascii(&buf, &mut idx);
        if idx >= buf.len() {
            return Err(MatrixMarketError::InvalidEntry {
                line: line_no,
                msg: format!(
                    "expected 3 tokens 'row col val', got: {}",
                    line_lossy_trimmed(&buf)
                ),
            }
            .into());
        }
        let col_tok_start = idx;
        let col_1: usize = parse_usize_at(&buf, &mut idx).ok_or_else(|| {
            let mut end = col_tok_start;
            while end < buf.len() && buf[end] > b' ' {
                end += 1;
            }
            MatrixMarketError::InvalidEntry {
                line: line_no,
                msg: format!(
                    "bad col index '{}'",
                    line_lossy_trimmed(&buf[col_tok_start..end])
                ),
            }
        })?;

        // val
        skip_ws_ascii(&buf, &mut idx);
        if idx >= buf.len() {
            return Err(MatrixMarketError::InvalidEntry {
                line: line_no,
                msg: format!(
                    "expected 3 tokens 'row col val', got: {}",
                    line_lossy_trimmed(&buf)
                ),
            }
            .into());
        }
        let val_tok_start = idx;

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
                let v: i64 = parse_i64_at(&buf, &mut idx).ok_or_else(|| {
                    let mut end = val_tok_start;
                    while end < buf.len() && buf[end] > b' ' {
                        end += 1;
                    }
                    MatrixMarketError::InvalidEntry {
                        line: line_no,
                        msg: format!(
                            "bad integer value '{}'",
                            line_lossy_trimmed(&buf[val_tok_start..end])
                        ),
                    }
                })?;
                v as f64
            }
            MmField::Real => {
                let v: f64 = parse_f64_token_at(&buf, &mut idx).ok_or_else(|| {
                    let mut end = val_tok_start;
                    while end < buf.len() && buf[end] > b' ' {
                        end += 1;
                    }
                    MatrixMarketError::InvalidEntry {
                        line: line_no,
                        msg: format!(
                            "bad real value '{}'",
                            line_lossy_trimmed(&buf[val_tok_start..end])
                        ),
                    }
                })?;
                v
            }
        };

        // Extra trailing tokens are treated as an error (to match the previous behavior).
        skip_ws_ascii(&buf, &mut idx);
        if idx < buf.len() {
            return Err(MatrixMarketError::InvalidEntry {
                line: line_no,
                msg: format!(
                    "expected 3 tokens 'row col val', got: {}",
                    line_lossy_trimmed(&buf)
                ),
            }
            .into());
        }

        read_entries += 1;
        // MatrixBuilder expects (column, row, value)
        if !keep_zeros && val == 0.0 {
            continue;
        }

        b.push(col, row, val)?;
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
