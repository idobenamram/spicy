use clap::Parser;
use spicy_simulate::solver::klu::{
    KLU_PERM_DUMP_MAGIC, KLU_PERM_DUMP_VERSION, KLU_SOLVE_DUMP_MAGIC, KLU_SOLVE_DUMP_VERSION,
};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    about = "Compare two KLU dump prefixes: exact analyze+factor perms + tolerant solve output.",
    after_help = "Inputs are dump prefixes produced by `klu_mtx --dump-bin PREFIX`.\nThis compares:\n  - <PREFIX>.analyze_factor.bin (bit-exact)\n  - <PREFIX>.solve.bin (abs+rel + ULP cap)",
    version
)]
struct Args {
    /// Dump prefix A (produced by `klu_mtx --dump-bin <PREFIX>`)
    a: PathBuf,
    /// Dump prefix B (produced by `klu_mtx --dump-bin <PREFIX>`)
    b: PathBuf,

    /// Skip exact permutation dump comparison (<PREFIX>.analyze_factor.bin).
    #[arg(long)]
    skip_perms: bool,

    /// Skip solve dump comparison (<PREFIX>.solve.bin).
    #[arg(long)]
    skip_solve: bool,

    /// Absolute tolerance for per-element comparison.
    #[arg(long, default_value_t = 1e-12)]
    atol: f64,
    /// Relative tolerance for per-element comparison.
    #[arg(long, default_value_t = 1e-12)]
    rtol: f64,
    /// Maximum allowed ULP distance (default: 100).
    #[arg(long, default_value_t = 100)]
    max_ulp: u64,

    /// Print up to N mismatching entries.
    #[arg(long, default_value_t = 10)]
    max_report: usize,
}

#[derive(Debug, Clone)]
struct SolveDump {
    n: u32,
    d: u32,
    nrhs: u32,
    len: u32,
    bits: Vec<u64>,
}

#[derive(Debug, Clone)]
struct PermDump {
    stage: u32,
    n: u32,
    nblocks: u32,
    p: Vec<i32>,
    q: Vec<i32>,
    r: Vec<i32>,
    pnum: Vec<i32>,
    pinv: Vec<i32>,
}

fn append_suffix(prefix: &Path, suffix: &str) -> PathBuf {
    let mut os = prefix.as_os_str().to_os_string();
    os.push(suffix);
    PathBuf::from(os)
}

fn read_u32_le<R: Read>(r: &mut R) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_i32_le<R: Read>(r: &mut R) -> io::Result<i32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

fn read_u64_le<R: Read>(r: &mut R) -> io::Result<u64> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

fn read_perm_dump(path: &Path) -> io::Result<PermDump> {
    let mut f = File::open(path)?;

    let mut magic = [0u8; 8];
    f.read_exact(&mut magic)?;
    if magic != KLU_PERM_DUMP_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "bad magic in {}: expected {:?}, got {:?}",
                path.display(),
                KLU_PERM_DUMP_MAGIC,
                magic
            ),
        ));
    }

    let version = read_u32_le(&mut f)?;
    if version != KLU_PERM_DUMP_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unsupported version in {}: expected {}, got {}",
                path.display(),
                KLU_PERM_DUMP_VERSION,
                version
            ),
        ));
    }

    let stage = read_u32_le(&mut f)?;
    let n = read_u32_le(&mut f)?;
    let nblocks = read_u32_le(&mut f)?;

    let n_usize = usize::try_from(n)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("n out of range: {n}")))?;
    let nblocks_usize = usize::try_from(nblocks).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("nblocks out of range: {nblocks}"),
        )
    })?;
    let r_len = nblocks_usize
        .checked_add(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "nblocks+1 overflow"))?;

    let mut p = Vec::with_capacity(n_usize);
    let mut q = Vec::with_capacity(n_usize);
    let mut r = Vec::with_capacity(r_len);
    let mut pnum = Vec::with_capacity(n_usize);
    let mut pinv = Vec::with_capacity(n_usize);

    for _ in 0..n_usize {
        p.push(read_i32_le(&mut f)?);
    }
    for _ in 0..n_usize {
        q.push(read_i32_le(&mut f)?);
    }
    for _ in 0..r_len {
        r.push(read_i32_le(&mut f)?);
    }
    for _ in 0..n_usize {
        pnum.push(read_i32_le(&mut f)?);
    }
    for _ in 0..n_usize {
        pinv.push(read_i32_le(&mut f)?);
    }

    Ok(PermDump {
        stage,
        n,
        nblocks,
        p,
        q,
        r,
        pnum,
        pinv,
    })
}

fn read_solve_dump(path: &Path) -> io::Result<SolveDump> {
    let mut f = File::open(path)?;

    let mut magic = [0u8; 8];
    f.read_exact(&mut magic)?;
    if magic != KLU_SOLVE_DUMP_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "bad magic in {}: expected {:?}, got {:?}",
                path.display(),
                KLU_SOLVE_DUMP_MAGIC,
                magic
            ),
        ));
    }

    let version = read_u32_le(&mut f)?;
    if version != KLU_SOLVE_DUMP_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unsupported version in {}: expected {}, got {}",
                path.display(),
                KLU_SOLVE_DUMP_VERSION,
                version
            ),
        ));
    }

    let n = read_u32_le(&mut f)?;
    let d = read_u32_le(&mut f)?;
    let nrhs = read_u32_le(&mut f)?;
    let len = read_u32_le(&mut f)?;

    let expected_len = (d as u64)
        .checked_mul(nrhs as u64)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "overflow computing d*nrhs"))?;
    if expected_len != len as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "len mismatch in {}: len={} but d*nrhs={}",
                path.display(),
                len,
                expected_len
            ),
        ));
    }

    let mut bits = Vec::with_capacity(len as usize);
    for _ in 0..len {
        bits.push(read_u64_le(&mut f)?);
    }

    Ok(SolveDump {
        n,
        d,
        nrhs,
        len,
        bits,
    })
}

fn ordered_f64_bits(bits: u64) -> u64 {
    const SIGN: u64 = 0x8000_0000_0000_0000;
    if (bits & SIGN) != 0 {
        !bits
    } else {
        bits | SIGN
    }
}

fn ulp_diff_bits(a_bits: u64, b_bits: u64) -> u64 {
    ordered_f64_bits(a_bits).abs_diff(ordered_f64_bits(b_bits))
}

fn abs_rel_ok(a: f64, b: f64, atol: f64, rtol: f64) -> bool {
    // Symmetric variant: |a-b| <= atol + rtol*max(|a|,|b|)
    let diff = (a - b).abs();
    if diff <= atol {
        return true;
    }
    let scale = a.abs().max(b.abs());
    diff <= atol + rtol * scale
}

fn compare_perm_exact(a_path: &Path, b_path: &Path) -> io::Result<bool> {
    let a = read_perm_dump(a_path)?;
    let b = read_perm_dump(b_path)?;

    if a.stage != b.stage {
        eprintln!(
            "perms: stage mismatch: A.stage={} B.stage={}",
            a.stage, b.stage
        );
        return Ok(false);
    }
    if a.n != b.n || a.nblocks != b.nblocks {
        eprintln!("perms: header mismatch:");
        eprintln!("  A: n={} nblocks={}", a.n, a.nblocks);
        eprintln!("  B: n={} nblocks={}", b.n, b.nblocks);
        return Ok(false);
    }

    fn first_mismatch(name: &str, a: &[i32], b: &[i32]) -> Option<(String, usize, i32, i32)> {
        if a.len() != b.len() {
            return Some((format!("{name} length"), 0, a.len() as i32, b.len() as i32));
        }
        for (i, (&av, &bv)) in a.iter().zip(b.iter()).enumerate() {
            if av != bv {
                return Some((name.to_string(), i, av, bv));
            }
        }
        None
    }

    if let Some((name, i, av, bv)) = first_mismatch("P", &a.p, &b.p)
        .or_else(|| first_mismatch("Q", &a.q, &b.q))
        .or_else(|| first_mismatch("R", &a.r, &b.r))
        .or_else(|| first_mismatch("Pnum", &a.pnum, &b.pnum))
        .or_else(|| first_mismatch("Pinv", &a.pinv, &b.pinv))
    {
        eprintln!("perms: DIFF in {name}[{i}]: A={av} B={bv}");
        return Ok(false);
    }

    Ok(true)
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    if args.atol < 0.0 || args.rtol < 0.0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "atol/rtol must be non-negative",
        ));
    }

    let perms_a_path = append_suffix(&args.a, ".analyze_factor.bin");
    let perms_b_path = append_suffix(&args.b, ".analyze_factor.bin");
    let solve_a_path = append_suffix(&args.a, ".solve.bin");
    let solve_b_path = append_suffix(&args.b, ".solve.bin");

    let mut ok_all = true;

    if !args.skip_perms {
        match compare_perm_exact(&perms_a_path, &perms_b_path) {
            Ok(true) => {
                println!("OK: analyze_factor permutations match exactly");
            }
            Ok(false) => {
                ok_all = false;
            }
            Err(e) => {
                eprintln!(
                    "perms: failed to read/compare {} vs {}: {e}",
                    perms_a_path.display(),
                    perms_b_path.display()
                );
                ok_all = false;
            }
        }
    }

    if args.skip_solve {
        if ok_all {
            return Ok(());
        }
        std::process::exit(1);
    }

    let a = read_solve_dump(&solve_a_path)?;
    let b = read_solve_dump(&solve_b_path)?;

    if a.n != b.n || a.d != b.d || a.nrhs != b.nrhs || a.len != b.len {
        eprintln!("header mismatch:");
        eprintln!("  A: n={} d={} nrhs={} len={}", a.n, a.d, a.nrhs, a.len);
        eprintln!("  B: n={} d={} nrhs={} len={}", b.n, b.d, b.nrhs, b.len);
        std::process::exit(2);
    }

    let n = a.n as usize;
    let d = a.d as usize;
    let nrhs = a.nrhs as usize;
    let len = a.len as usize;

    let mut bad = 0usize;
    let mut reported = 0usize;

    let mut max_abs = 0.0f64;
    let mut max_abs_i = 0usize;
    let mut max_rel = 0.0f64;
    let mut max_rel_i = 0usize;
    let mut max_ulp = 0u64;
    let mut max_ulp_i = 0usize;

    for i in 0..len {
        let a_bits = a.bits[i];
        let b_bits = b.bits[i];
        let av = f64::from_bits(a_bits);
        let bv = f64::from_bits(b_bits);
        let row = i % d;
        let rhs = i / d;

        // Exact equality (handles +/-0 and infinities).
        if av == bv {
            continue;
        }

        // Treat NaNs as mismatches (solutions should not contain NaNs).
        if av.is_nan() || bv.is_nan() {
            bad += 1;
            if reported < args.max_report {
                eprintln!(
                    "mismatch[{i}] (rhs={rhs} row={row}): NaN encountered: a={av:?} (0x{a_bits:016x}) b={bv:?} (0x{b_bits:016x})"
                );
                reported += 1;
            }
            continue;
        }

        let abs = (av - bv).abs();
        let scale = av.abs().max(bv.abs());
        let rel = if scale > 0.0 { abs / scale } else { abs };
        let ulp = ulp_diff_bits(a_bits, b_bits);

        if abs > max_abs {
            max_abs = abs;
            max_abs_i = i;
        }
        if rel > max_rel {
            max_rel = rel;
            max_rel_i = i;
        }
        if ulp > max_ulp {
            max_ulp = ulp;
            max_ulp_i = i;
        }

        let ok_abs_rel = abs_rel_ok(av, bv, args.atol, args.rtol);
        let ok_ulp = ulp <= args.max_ulp;

        if !(ok_abs_rel && ok_ulp) {
            bad += 1;
            if reported < args.max_report {
                eprintln!(
                    "mismatch[{i}] (rhs={rhs} row={row}): abs={abs:.3e} rel={rel:.3e} ulp={ulp}  a={av:.17e} (0x{a_bits:016x})  b={bv:.17e} (0x{b_bits:016x})"
                );
                eprintln!(
                    "             thresholds: abs<=atol+rtol*max(|a|,|b|) with atol={} rtol={} ; ulp<={}",
                    args.atol, args.rtol, args.max_ulp
                );
                reported += 1;
            }
        }
    }

    if bad == 0 {
        println!(
            "OK: solve dumps match within tolerances (n={} d={} nrhs={} len={})",
            n, d, nrhs, len
        );
        println!(
            "max_abs={:.3e} @{} ; max_rel={:.3e} @{} ; max_ulp={} @{}",
            max_abs, max_abs_i, max_rel, max_rel_i, max_ulp, max_ulp_i
        );
        if ok_all {
            Ok(())
        } else {
            std::process::exit(1);
        }
    } else {
        eprintln!(
            "FAIL: {bad} / {len} entries exceeded tolerances (atol={} rtol={} max_ulp={})",
            args.atol, args.rtol, args.max_ulp
        );
        eprintln!(
            "max_abs={:.3e} @{} ; max_rel={:.3e} @{} ; max_ulp={} @{}",
            max_abs, max_abs_i, max_rel, max_rel_i, max_ulp, max_ulp_i
        );
        std::process::exit(1);
    }
}
