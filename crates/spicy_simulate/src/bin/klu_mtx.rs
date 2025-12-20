use spicy_simulate::solver::{
    klu,
    matrix::{
        csc::CscMatrix,
        mtx::{load_matrix_market_csc_file, load_matrix_market_csc_file_keep_zeros},
    },
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn fmt_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs >= 1.0 {
        format!("{secs:.3}s")
    } else if secs >= 1e-3 {
        format!("{:.3}ms", secs * 1e3)
    } else if secs >= 1e-6 {
        format!("{:.3}us", secs * 1e6)
    } else {
        format!("{}ns", d.as_nanos())
    }
}

fn print_timing_breakdown(stages: &[(&str, Duration)], total_elapsed: Duration) {
    let accounted_secs = stages
        .iter()
        .map(|(_, d)| d.as_secs_f64())
        .sum::<f64>();
    let total_secs = total_elapsed.as_secs_f64();

    println!();
    println!(
        "timing breakdown (accounted {}, total {}):",
        fmt_duration(Duration::from_secs_f64(accounted_secs)),
        fmt_duration(total_elapsed)
    );

    for (name, dur) in stages {
        let pct = if total_secs > 0.0 {
            (dur.as_secs_f64() / total_secs) * 100.0
        } else {
            0.0
        };
        println!("  {name:<22} {:>12} ({pct:>6.2}%)", fmt_duration(*dur));
    }

    let other_secs = (total_secs - accounted_secs).max(0.0);
    let other_pct = if total_secs > 0.0 {
        (other_secs / total_secs) * 100.0
    } else {
        0.0
    };
    println!(
        "  {:<22} {:>12} ({other_pct:>6.2}%)",
        "other_overhead",
        fmt_duration(Duration::from_secs_f64(other_secs))
    );
}

fn usage(exe: &str) -> String {
    format!(
        "Usage: {exe} [--keep-zeros|--drop-zeros] <path.mtx>\n\nLoads a MatrixMarket coordinate matrix (.mtx), solves Ax=b with KLU, and prints demo-style stats.\n\nNotes:\n  - By default, {exe} uses --keep-zeros to match SuiteSparse KLU demo-style nnz/nzoff counts when the file contains explicit zero entries."
    )
}

fn print_matrix_stats(path: &PathBuf, a: &CscMatrix) {
    println!("matrix: {}", path.display());
    println!("dim: {} x {}", a.dim.nrows, a.dim.ncols);
    println!("nnz: {}", a.nnz());
    println!("square: {}", a.is_square());
    println!("csc: col_ptr_len={} row_idx_len={} values_len={}", a.column_pointers.len(), a.row_indices.len(), a.values.len());
}

fn make_demo_rhs(n: usize) -> Vec<f64> {
    // Match kluldemo.c:
    // B[i] = 1 + (i+1)/n
    let nf = n as f64;
    (0..n)
        .map(|i| 1.0 + ((i + 1) as f64) / nf)
        .collect()
}

fn csc_matvec(a: &CscMatrix, x: &[f64]) -> Vec<f64> {
    debug_assert_eq!(a.dim.ncols, x.len());
    let mut y = vec![0.0; a.dim.nrows];
    for j in 0..a.dim.ncols {
        a.axpy_into_dense_col(j, x[j], &mut y);
    }
    y
}

fn matrix_one_norm(a: &CscMatrix) -> f64 {
    // max_j sum_i |A_ij|
    let mut anorm: f64 = 0.0;
    for j in 0..a.dim.ncols {
        let (_, vals) = a.col(j);
        let col_sum = vals.iter().map(|v| v.abs()).sum::<f64>();
        anorm = anorm.max(col_sum);
    }
    anorm
}

fn main() {
    let total_start = Instant::now();
    let mut stages: Vec<(&str, Duration)> = Vec::new();

    let mut args = std::env::args();
    let exe = args.next().unwrap_or_else(|| "klu_mtx".to_string());

    // CLI:
    // - default is --keep-zeros (to match SuiteSparse demo nnz/nzoff for UF matrices that include explicit zeros)
    // - allow override with --drop-zeros
    let mut keep_zeros = true;
    let mut path_arg: Option<String> = None;
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{}", usage(&exe));
                return;
            }
            "--keep-zeros" => keep_zeros = true,
            "--drop-zeros" => keep_zeros = false,
            other => {
                if path_arg.is_none() {
                    path_arg = Some(other.to_string());
                } else {
                    eprintln!("{}", usage(&exe));
                    std::process::exit(2);
                }
            }
        }
    }
    let Some(arg1) = path_arg else {
        eprintln!("{}", usage(&exe));
        std::process::exit(2);
    };

    let t = Instant::now();
    let path = PathBuf::from(arg1);
    let a = match if keep_zeros {
        load_matrix_market_csc_file_keep_zeros(&path)
    } else {
        load_matrix_market_csc_file(&path)
    } {
        Ok(a) => a,
        Err(e) => {
            eprintln!("failed to load MatrixMarket file: {e}");
            print_timing_breakdown(&stages, total_start.elapsed());
            std::process::exit(1);
        }
    };
    stages.push(("load_matrix", t.elapsed()));

    let t = Instant::now();
    if let Err(e) = a.check_invariants() {
        eprintln!("invalid CSC matrix after parsing: {e}");
        stages.push(("check_invariants", t.elapsed()));
        print_timing_breakdown(&stages, total_start.elapsed());
        std::process::exit(1);
    }
    stages.push(("check_invariants", t.elapsed()));

    let t = Instant::now();
    print_matrix_stats(&path, &a);
    stages.push(("print_matrix_stats", t.elapsed()));

    if !a.is_square() {
        eprintln!("KLU only supports square matrices; skipping factorization.");
        print_timing_breakdown(&stages, total_start.elapsed());
        std::process::exit(2);
    }

    let n = a.dim.ncols;
    let t = Instant::now();
    let b = make_demo_rhs(n);
    let mut x = b.clone();
    stages.push(("make_rhs", t.elapsed()));

    let mut config = klu::KluConfig::default();
    let t = Instant::now();
    let mut symbolic = match klu::analyze(&a, &config) {
        Ok(symbolic) => symbolic,
        Err(e) => {
            eprintln!("klu analyze failed: {e}");
            stages.push(("klu_analyze", t.elapsed()));
            print_timing_breakdown(&stages, total_start.elapsed());
            std::process::exit(1);
        }
    };
    stages.push(("klu_analyze", t.elapsed()));

    let t = Instant::now();
    let mut numeric = match klu::factor(&a, &mut symbolic, &mut config) {
        Ok(numeric) => numeric,
        Err(e) => {
            eprintln!("klu factor failed: {e}");
            stages.push(("klu_factor", t.elapsed()));
            print_timing_breakdown(&stages, total_start.elapsed());
            std::process::exit(1);
        }
    };
    stages.push(("klu_factor", t.elapsed()));

    let t = Instant::now();
    if let Err(e) = klu::solve(&symbolic, &mut numeric, n, 1, &mut x, &config) {
        eprintln!("klu solve failed: {e}");
        stages.push(("klu_solve", t.elapsed()));
        print_timing_breakdown(&stages, total_start.elapsed());
        std::process::exit(1);
    }
    stages.push(("klu_solve", t.elapsed()));

    // Residual: r = b - A*x, report max |r_i| (like kluldemo.c)
    let t = Instant::now();
    let ax = csc_matvec(&a, &x);
    let rnorm = ax
        .iter()
        .zip(b.iter())
        .map(|(ai, bi)| (bi - ai).abs())
        .fold(0.0, f64::max);
    stages.push(("residual", t.elapsed()));

    let t = Instant::now();
    let anorm = matrix_one_norm(&a);
    stages.push(("anorm_1", t.elapsed()));

    let lunz = numeric.lnz + numeric.unz - n + numeric.nzoff;

    let t = Instant::now();
    println!();
    println!(
        "n {} nnz(A) {} nnz(L+U+F) {} resid {:.5e}",
        n,
        a.nnz(),
        lunz,
        rnorm
    );
    println!(
        "anorm_1 {} (note: we don't yet report rgrowth/condest/rcond/flops like SuiteSparse demo)",
        anorm
    );
    println!("klu: nblocks={} nzoff={}", numeric.nblocks, numeric.nzoff);
    println!(
        "lu_nnz: lnz={} unz={} (lnz+unz={})",
        numeric.lnz,
        numeric.unz,
        numeric.lnz + numeric.unz
    );
    println!(
        "blocks: max_lnz_block={} max_unz_block={}",
        numeric.max_lnz_block, numeric.max_unz_block
    );
    println!(
        "metrics: nrealloc={} noffdiag={} numerical_rank={:?} singular_col={:?}",
        numeric.metrics.nrealloc,
        numeric.metrics.noffdiag,
        numeric.metrics.numerical_rank,
        numeric.metrics.singular_col
    );

    stages.push(("report", t.elapsed()));
    print_timing_breakdown(&stages, total_start.elapsed());
}


