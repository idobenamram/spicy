// SPDX-License-Identifier: LGPL-2.1-or-later
//
// Deterministic binary dumps for comparing this Rust port against SuiteSparse KLU.

use std::io;

use super::{KluNumeric, KluSymbolic};

/// Binary permutation dump file magic (`SPKLPERM`).
///
/// This is used by the `klu_mtx` utility to emit deterministic, byte-for-byte
/// comparable dumps against SuiteSparse KLU.
pub const KLU_PERM_DUMP_MAGIC: [u8; 8] = *b"SPKLPERM";
/// Binary permutation dump format version.
pub const KLU_PERM_DUMP_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum KluPermDumpStage {
    AnalyzeFactor = 1,
    Solve = 2,
}

/// Write a deterministic binary dump of the permutation arrays from `symbolic`
/// and `numeric`.
///
/// Format (little-endian):
/// - 8 bytes: magic = `KLU_PERM_DUMP_MAGIC`
/// - u32: version = `KLU_PERM_DUMP_VERSION`
/// - u32: stage (`KluPermDumpStage` as u32)
/// - u32: n
/// - u32: nblocks
/// - i32[n]: P   (symbolic row permutation)
/// - i32[n]: Q   (symbolic column permutation)
/// - i32[nblocks+1]: R (block boundaries; only the used prefix is written)
/// - i32[n]: Pnum (numeric pivot permutation)
/// - i32[n]: Pinv (inverse pivot permutation)
pub fn write_perm_dump<W: io::Write>(
    mut w: W,
    stage: KluPermDumpStage,
    symbolic: &KluSymbolic,
    numeric: &KluNumeric,
) -> io::Result<()> {
    fn write_u32_le<W: io::Write>(w: &mut W, v: u32) -> io::Result<()> {
        w.write_all(&v.to_le_bytes())
    }
    fn write_i32_le<W: io::Write>(w: &mut W, v: i32) -> io::Result<()> {
        w.write_all(&v.to_le_bytes())
    }
    fn to_u32(v: usize, what: &'static str) -> io::Result<u32> {
        u32::try_from(v).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{what} out of range for u32: {v}"),
            )
        })
    }
    fn to_i32(v: isize, what: &'static str) -> io::Result<i32> {
        i32::try_from(v).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{what} out of range for i32: {v}"),
            )
        })
    }
    fn to_i32_from_usize(v: usize, what: &'static str) -> io::Result<i32> {
        i32::try_from(v).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{what} out of range for i32: {v}"),
            )
        })
    }

    // Basic consistency checks to avoid silently emitting malformed files.
    if numeric.n != symbolic.n {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "n mismatch: symbolic.n={} numeric.n={}",
                symbolic.n, numeric.n
            ),
        ));
    }
    if symbolic.row_permutation.len() != symbolic.n {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "symbolic.P length mismatch: len={} n={}",
                symbolic.row_permutation.len(),
                symbolic.n
            ),
        ));
    }
    if symbolic.column_permutation.len() != symbolic.n {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "symbolic.Q length mismatch: len={} n={}",
                symbolic.column_permutation.len(),
                symbolic.n
            ),
        ));
    }
    if numeric.pnum.len() != symbolic.n {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "numeric.Pnum length mismatch: len={} n={}",
                numeric.pnum.len(),
                symbolic.n
            ),
        ));
    }
    if numeric.pinv.len() != symbolic.n {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "numeric.Pinv length mismatch: len={} n={}",
                numeric.pinv.len(),
                symbolic.n
            ),
        ));
    }
    let r_len = symbolic
        .nblocks
        .checked_add(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "nblocks+1 overflow"))?;
    if symbolic.block_boundaries.len() < r_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "symbolic.R length too small: len={} need={}",
                symbolic.block_boundaries.len(),
                r_len
            ),
        ));
    }

    // Header
    w.write_all(&KLU_PERM_DUMP_MAGIC)?;
    write_u32_le(&mut w, KLU_PERM_DUMP_VERSION)?;
    write_u32_le(&mut w, stage as u32)?;
    write_u32_le(&mut w, to_u32(symbolic.n, "n")?)?;
    write_u32_le(&mut w, to_u32(symbolic.nblocks, "nblocks")?)?;

    // P, Q, R, Pnum, Pinv
    for &v in &symbolic.row_permutation {
        write_i32_le(&mut w, to_i32(v, "P entry")?)?;
    }
    for &v in &symbolic.column_permutation {
        write_i32_le(&mut w, to_i32(v, "Q entry")?)?;
    }
    for &v in &symbolic.block_boundaries[..r_len] {
        write_i32_le(&mut w, to_i32_from_usize(v, "R entry")?)?;
    }
    for &v in &numeric.pnum {
        write_i32_le(&mut w, to_i32(v, "Pnum entry")?)?;
    }
    for &v in &numeric.pinv {
        write_i32_le(&mut w, to_i32(v, "Pinv entry")?)?;
    }

    Ok(())
}

/// Binary solve-output dump file magic (`SPKLSOLV`).
///
/// This is used by the `klu_mtx` utility to dump the solved RHS / solution vector
/// in a byte-for-byte comparable way against SuiteSparse KLU.
pub const KLU_SOLVE_DUMP_MAGIC: [u8; 8] = *b"SPKLSOLV";
/// Binary solve-output dump format version.
pub const KLU_SOLVE_DUMP_VERSION: u32 = 1;

/// Write a deterministic binary dump of the solve output (the RHS overwritten with the solution).
///
/// Format (little-endian):
/// - 8 bytes: magic = `KLU_SOLVE_DUMP_MAGIC`
/// - u32: version = `KLU_SOLVE_DUMP_VERSION`
/// - u32: n        (problem size; typically `symbolic.n`)
/// - u32: d        (leading dimension)
/// - u32: nrhs     (number of right-hand-sides)
/// - u32: len      (number of `f64` entries written; must equal d*nrhs)
/// - u64[len]: raw IEEE754 `f64` bits, `to_bits()` written as little-endian u64
pub fn write_solve_dump<W: io::Write>(
    mut w: W,
    n: usize,
    d: usize,
    nrhs: usize,
    b: &[f64],
) -> io::Result<()> {
    fn write_u32_le<W: io::Write>(w: &mut W, v: u32) -> io::Result<()> {
        w.write_all(&v.to_le_bytes())
    }
    fn write_u64_le<W: io::Write>(w: &mut W, v: u64) -> io::Result<()> {
        w.write_all(&v.to_le_bytes())
    }
    fn to_u32(v: usize, what: &'static str) -> io::Result<u32> {
        u32::try_from(v).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{what} out of range for u32: {v}"),
            )
        })
    }

    let len = d.checked_mul(nrhs).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "overflow computing d*nrhs")
    })?;
    if b.len() < len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("rhs too small: len={} need={}", b.len(), len),
        ));
    }

    // Header
    w.write_all(&KLU_SOLVE_DUMP_MAGIC)?;
    write_u32_le(&mut w, KLU_SOLVE_DUMP_VERSION)?;
    write_u32_le(&mut w, to_u32(n, "n")?)?;
    write_u32_le(&mut w, to_u32(d, "d")?)?;
    write_u32_le(&mut w, to_u32(nrhs, "nrhs")?)?;
    write_u32_le(&mut w, to_u32(len, "len")?)?;

    // Payload: f64 bits
    for &v in &b[..len] {
        write_u64_le(&mut w, v.to_bits())?;
    }

    Ok(())
}


