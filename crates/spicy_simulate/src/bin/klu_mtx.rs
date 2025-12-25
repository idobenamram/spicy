use clap::Parser;
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

#[derive(Parser, Debug)]
#[command(
    about = "Loads a MatrixMarket coordinate matrix (.mtx), solves Ax=b with KLU, and prints demo-style stats.",
    after_help = "Notes:\n  - By default, uses --keep-zeros to match SuiteSparse KLU demo-style nnz/nzoff counts when the file contains explicit zero entries.\n  - --print-both-residuals prints both the SuiteSparse demo residual accumulation order and a matvec-based variant (they may differ slightly due to floating-point rounding).",
    version
)]
struct Args {
    /// Keep explicit zeros in the MatrixMarket file (default; matches SuiteSparse demo nnz/nzoff counts).
    #[arg(long, conflicts_with = "drop_zeros")]
    keep_zeros: bool,

    /// Drop explicit zeros while loading the MatrixMarket file.
    #[arg(long, conflicts_with = "keep_zeros")]
    drop_zeros: bool,

    /// Print both the SuiteSparse demo residual accumulation order and a matvec-based variant.
    #[arg(long)]
    print_both_residuals: bool,

    /// Path to MatrixMarket coordinate matrix (.mtx)
    #[arg(value_name = "PATH")]
    path: PathBuf,
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

/// Compute the demo residual exactly like SuiteSparse `KLU/Demo/kluldemo.c`:
/// - start with `R = B`
/// - for each stored A(i,j): `R[i] -= A(i,j) * X[j]`
/// - return `max_i |R[i]|`
///
/// Note: this differs from computing `Ax = A*X` and then `max|B-Ax|` only in
/// floating-point rounding/accumulation order (the math is identical).
fn residual_demo_style_max_abs(a: &CscMatrix, b: &[f64], x: &[f64]) -> f64 {
    debug_assert_eq!(a.dim.nrows, b.len());
    debug_assert_eq!(a.dim.ncols, x.len());

    let n = a.dim.ncols;
    let mut r = b.to_vec();
    for j in 0..n {
        let xj = x[j];
        let (rows, vals) = a.col(j);
        for (&i, &aij) in rows.iter().zip(vals.iter()) {
            r[i] -= aij * xj;
        }
    }
    r.into_iter().map(|ri| ri.abs()).fold(0.0, f64::max)
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

fn vec_inf_norm(x: &[f64]) -> f64 {
    x.iter().map(|v| v.abs()).fold(0.0, f64::max)
}

fn main() {
    let total_start = Instant::now();
    let mut stages: Vec<(&str, Duration)> = Vec::new();

    let args = Args::parse();
    // Default behavior is keep zeros unless the user explicitly requests dropping them.
    let keep_zeros = !args.drop_zeros;
    let print_both_residuals = args.print_both_residuals;

    let t = Instant::now();
    let path = args.path;
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

    // Residual: compute exactly like kluldemo.c (in-place subtraction order).
    let t = Instant::now();
    let rnorm_demo = residual_demo_style_max_abs(&a, &b, &x);
    let rnorm_matvec = if print_both_residuals {
        let ax = csc_matvec(&a, &x);
        Some(
            ax.iter()
                .zip(b.iter())
                .map(|(ai, bi)| (bi - ai).abs())
                .fold(0.0, f64::max),
        )
    } else {
        None
    };
    stages.push(("residual", t.elapsed()));

    let t = Instant::now();
    let anorm = matrix_one_norm(&a);
    stages.push(("anorm_1", t.elapsed()));

    let lunz = numeric.lnz + numeric.unz - n + numeric.nzoff;

    // A common solver-agnostic correctness measure:
    // eta = ||b - A*x||_inf / (||A||_1 * ||x||_inf + ||b||_inf)
    // If eta is ~1e-15..1e-12, you're typically looking at plain f64 rounding/cancellation.
    let bnorm_inf = vec_inf_norm(&b);
    let xnorm_inf = vec_inf_norm(&x);
    let denom = (anorm * xnorm_inf) + bnorm_inf;
    let eta_demo = if denom > 0.0 { rnorm_demo / denom } else { 0.0 };
    let eta_matvec = rnorm_matvec.map(|rm| if denom > 0.0 { rm / denom } else { 0.0 });

    let t = Instant::now();
    println!();
    println!(
        "n {} nnz(A) {} nnz(L+U+F) {} resid {:.5e}",
        n,
        a.nnz(),
        lunz,
        rnorm_demo
    );
    if let Some(rm) = rnorm_matvec {
        println!(
            "resid_matvec {:.5e} (matvec then subtract; may differ due to rounding)",
            rm
        );
    }
    println!(
        "norms: ||b||_inf {:.5e}  ||x||_inf {:.5e}  (||A||_1 {:.5e})",
        bnorm_inf, xnorm_inf, anorm
    );
    println!("backward_error_eta {:.5e}", eta_demo);
    if let Some(e) = eta_matvec {
        println!("backward_error_eta_matvec {:.5e}", e);
    }
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


