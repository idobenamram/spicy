//! Helpers shared by the tests.
//!
//! # Why the simulation snapshots don't use insta directly
//!
//! Simulation results are floating point, and their last digits depend on the
//! BLAS/LAPACK build and the CPU. Exact snapshots therefore failed on machines
//! other than the one that wrote them (e.g. `0.5020954917241712` vs `…713`),
//! even though nothing in the math had changed.
//!
//! insta compares snapshots as exact text and has no tolerance-based
//! comparison. Its only float support, `rounded_redaction(decimals)`, needs
//! serde-serialized snapshots (YAML/JSON), rounds to a fixed number of decimal
//! places (too coarse for small currents next to volts), and is still rounding.
//! Rounding hides real changes below the rounding step and can flip a digit
//! at a rounding boundary.
//!
//! So [`assert_numeric_snapshot`] keeps the snapshot files at full precision
//! (every change is visible in a diff) and compares them itself: the text must
//! match exactly, and each number must match within [`SNAPSHOT_REL_TOL`].

use std::fmt::Debug;
use std::fs;
use std::path::PathBuf;

/// Round `x` to `sig` significant digits.
pub(crate) fn round_sig(x: f64, sig: i32) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let exp10 = x.abs().log10().floor() as i32;
    let digits = sig - 1 - exp10;
    let scale = 10f64.powi(digits);
    (x * scale).round() / scale
}

/// Largest relative difference between a stored and a computed number that
/// still counts as equal. Noise between BLAS builds and CPUs is around 1e-16;
/// real changes to the math are far larger.
const SNAPSHOT_REL_TOL: f64 = 1e-12;

/// Absolute floor for numbers that are really zero (solver round-off, ~1e-16).
const SNAPSHOT_ABS_TOL: f64 = 1e-14;

/// Set this environment variable to rewrite snapshots that no longer match.
const SNAPSHOT_UPDATE_ENV: &str = "SPICY_UPDATE_SNAPSHOTS";

/// Header of every simulation snapshot file (the layout insta uses).
const SNAPSHOT_HEADER: &str =
    "---\nsource: crates/spicy_simulate/src/lib.rs\nexpression: output\n---\n";

/// Compare `value`'s pretty `Debug` output with the stored snapshot `name`.
///
/// Snapshots keep full precision, so every change shows up in the diff, but
/// numbers are compared with a tolerance so last-digit noise from a different
/// BLAS build or CPU doesn't fail the test. The text around the numbers must
/// match exactly.
///
/// On a mismatch the actual output is written next to the snapshot as
/// `<name>.snap.new`. Run with `SPICY_UPDATE_SNAPSHOTS=1` to accept it.
#[track_caller]
pub(crate) fn assert_numeric_snapshot(name: &str, value: &impl Debug) {
    let actual = format!("{value:#?}");
    let path = snapshot_path(name);
    let new_path = path.with_extension("snap.new");

    let problem = match fs::read_to_string(&path) {
        Ok(stored) => {
            let body = snapshot_body(&stored);
            let first_line = stored[..stored.len() - body.len()].matches('\n').count() + 1;
            compare_numeric(body, &actual, first_line).err()
        }
        Err(_) => Some("there is no stored snapshot".to_string()),
    };
    let Some(problem) = problem else {
        let _ = fs::remove_file(&new_path);
        return;
    };

    let contents = format!("{SNAPSHOT_HEADER}{actual}\n");
    if std::env::var_os(SNAPSHOT_UPDATE_ENV).is_some() {
        fs::write(&path, contents).expect("failed to write snapshot");
        let _ = fs::remove_file(&new_path);
        return;
    }
    fs::write(&new_path, contents).expect("failed to write snapshot");
    panic!(
        "snapshot `{name}` does not match: {problem}\n\
         actual output written to {}\n\
         accept it with: {SNAPSHOT_UPDATE_ENV}=1 cargo test -p spicy_simulate",
        new_path.display()
    );
}

fn snapshot_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/snapshots")
        .join(format!("spicy_simulate__tests__{name}.snap"))
}

/// The part of a snapshot file after its `---` header.
fn snapshot_body(stored: &str) -> &str {
    stored
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(stored, |(_, body)| body)
}

/// Compare two `Debug` outputs: text must match exactly, numbers within tolerance.
/// `first_line` is the file line `expected` starts on, for error messages.
fn compare_numeric(expected: &str, actual: &str, first_line: usize) -> Result<(), String> {
    let expected = split_numbers(expected.trim_end(), first_line);
    let actual = split_numbers(actual.trim_end(), first_line);

    let mut mismatches = Vec::new();
    for (e, a) in expected.iter().zip(&actual) {
        match (e, a) {
            (Piece::Text(e), Piece::Text(a)) if e == a => {}
            (Piece::Number { value: e, line }, Piece::Number { value: a, .. }) => {
                if !numbers_match(*e, *a) {
                    mismatches.push(format!("line {line}: expected {e:e}, got {a:e}"));
                }
            }
            _ => return Err(format!("the text changed: expected {e:?}, got {a:?}")),
        }
    }
    if expected.len() != actual.len() {
        return Err(format!(
            "the output has {} pieces, the snapshot {}",
            actual.len(),
            expected.len()
        ));
    }
    if mismatches.is_empty() {
        return Ok(());
    }
    let shown = mismatches.len().min(10);
    Err(format!(
        "{} number(s) differ by more than {SNAPSHOT_REL_TOL:e} relative:\n  {}",
        mismatches.len(),
        mismatches[..shown].join("\n  ")
    ))
}

fn numbers_match(expected: f64, actual: f64) -> bool {
    if expected == actual {
        return true;
    }
    let diff = (expected - actual).abs();
    diff <= SNAPSHOT_ABS_TOL || diff <= SNAPSHOT_REL_TOL * expected.abs().max(actual.abs())
}

#[derive(Debug, PartialEq)]
enum Piece<'a> {
    Text(&'a str),
    Number { value: f64, line: usize },
}

/// Split `s` into numbers and the text between them. A number is only
/// recognized when it isn't part of a name, so `V1` and `0xf` stay text.
fn split_numbers(s: &str, first_line: usize) -> Vec<Piece<'_>> {
    let bytes = s.as_bytes();
    let mut pieces = Vec::new();
    let mut text_start = 0;
    let mut line = first_line;
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = number_end(bytes, i) {
            if text_start < i {
                pieces.push(Piece::Text(&s[text_start..i]));
            }
            let value = s[i..end].parse().expect("scanned a valid number");
            pieces.push(Piece::Number { value, line });
            i = end;
            text_start = end;
        } else {
            if bytes[i] == b'\n' {
                line += 1;
            }
            i += 1;
        }
    }
    if text_start < bytes.len() {
        pieces.push(Piece::Text(&s[text_start..]));
    }
    pieces
}

/// If a number (`12`, `-0.5`, `5.0000000000000036e-17`) starts at `start`,
/// return the index just past it.
fn number_end(b: &[u8], start: usize) -> Option<usize> {
    let part_of_name = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    if start > 0 && (part_of_name(b[start - 1]) || b[start - 1] == b'.') {
        return None;
    }
    let digits = |i: &mut usize| {
        let first = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i > first
    };

    let mut i = start;
    if b[i] == b'-' {
        i += 1;
    }
    if !digits(&mut i) {
        return None;
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        digits(&mut i);
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'-' | b'+')) {
            j += 1;
        }
        if digits(&mut j) {
            i = j;
        }
    }
    if i < b.len() && part_of_name(b[i]) {
        return None;
    }
    Some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STORED: &str = "Result {\n    name: \"V1\",\n    layout: CFcf (0xf),\n    values: [\n        0.5020954917241712,\n        -1.0061396160665481e-16,\n        12.0,\n    ],\n}";

    #[test]
    fn identical_output_matches() {
        assert_eq!(compare_numeric(STORED, STORED, 1), Ok(()));
    }

    #[test]
    fn last_digit_noise_matches() {
        // What a different BLAS build produced on this project's AC tests.
        let noisy = STORED
            .replace("0.5020954917241712", "0.5020954917241713")
            .replace("-1.0061396160665481e-16", "5.0000000000000036e-17");
        assert_eq!(compare_numeric(STORED, &noisy, 1), Ok(()));
    }

    #[test]
    fn real_change_fails() {
        // A change in the 10th significant digit is a real change.
        let changed = STORED.replace("0.5020954917241712", "0.5020954918241712");
        let err = compare_numeric(STORED, &changed, 1).unwrap_err();
        assert!(err.contains("line 5"), "{err}");
    }

    #[test]
    fn text_change_fails() {
        let renamed = STORED.replace("\"V1\"", "\"V2\"");
        assert!(compare_numeric(STORED, &renamed, 1).is_err());
    }

    #[test]
    fn names_are_not_numbers() {
        let pieces = split_numbers("V1 0xf n2_3 x=-2.5e-3", 1);
        let numbers: Vec<_> = pieces
            .iter()
            .filter_map(|p| match p {
                Piece::Number { value, .. } => Some(*value),
                Piece::Text(_) => None,
            })
            .collect();
        assert_eq!(numbers, vec![-2.5e-3]);
    }
}
