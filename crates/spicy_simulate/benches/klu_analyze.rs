// SPDX-License-Identifier: LGPL-2.1-or-later

use std::{fs, hint::black_box, path::{Path, PathBuf}, sync::OnceLock};

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use spicy_simulate::solver::{
    btf_max_transversal::btf_max_transversal,
    btf_scc::btf_scc,
    klu::{self, KluConfig},
    matrix::{csc::CscMatrix, mtx::load_matrix_market_csc_file},
};

const MATRICES_DIR: &str = "assets/matrices";

#[derive(Debug)]
struct Case {
    name: String,
    a: CscMatrix,
    n: usize,
    nnz: usize,
}

fn workspace_root() -> PathBuf {
    // crates/spicy_simulate -> workspace root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR should be crates/spicy_simulate")
        .to_path_buf()
}

fn load_matrix(path: &Path) -> CscMatrix {
    let a = load_matrix_market_csc_file(path)
        .unwrap_or_else(|e| panic!("failed to load MatrixMarket file {}: {e}", path.display()));

    a.check_invariants()
        .unwrap_or_else(|e| panic!("invalid CSC after parsing {}: {e}", path.display()));
    a
}


fn load_cases() -> &'static Vec<Case> {
    static CASES: OnceLock<Vec<Case>> = OnceLock::new();
    CASES.get_or_init(|| {
        let dir = workspace_root().join(MATRICES_DIR);
        if !dir.is_dir() {
            panic!(
                "Matrix directory not found: {}\n\
\n\
Run:\n\
  bash scripts/download_matrices.sh\n\
or copy your .mtx files into that directory.",
                dir.display()
            );
        }

        let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("failed to read dir {}: {e}", dir.display()))
            .filter_map(|res| res.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("mtx"))
            })
            .collect();

        paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        if paths.is_empty() {
            panic!("No .mtx files found under {}", dir.display());
        }

        let mut cases: Vec<Case> = Vec::new();
        for path in paths {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());

            let a = load_matrix(&path);
            let n = a.dim.ncols;
            let nnz = a.nnz();

            cases.push(Case { name, a, n, nnz });
        }

        if cases.is_empty() {
            panic!("no .mtx files loaded under {}", dir.display());
        }

        cases
    })
}

fn bench_btf_max_transversal(c: &mut Criterion) {
    let cases = load_cases();
    let mut group = c.benchmark_group("klu/btf_max_transversal");

    for case in cases {
        group.throughput(Throughput::Elements(case.nnz as u64));
        group.bench_with_input(BenchmarkId::from_parameter(&case.name), case, |b, case| {
            assert!(
                case.a.is_square(),
                "Matrix {} is not square ({}x{})",
                case.name,
                case.a.dim.nrows,
                case.a.dim.ncols
            );

            let n = case.n;
            b.iter_batched(
                || vec![-1isize; n],
                |mut col_perm| {
                    let matches = btf_max_transversal(&case.a, &mut col_perm);
                    black_box(matches);
                    black_box(col_perm);
                },
                BatchSize::PerIteration,
            );
        });
    }

    group.finish();
}

fn bench_btf_scc_after_max_transversal(c: &mut Criterion) {
    let cases = load_cases();
    let mut group = c.benchmark_group("klu/btf_scc_after_max_transversal");

    for case in cases {
        group.throughput(Throughput::Elements(case.nnz as u64));
        group.bench_with_input(BenchmarkId::from_parameter(&case.name), case, |b, case| {
            assert!(
                case.a.is_square(),
                "Matrix {} is not square ({}x{})",
                case.name,
                case.a.dim.nrows,
                case.a.dim.ncols
            );

            let n = case.n;
            let mut base_col_perm = vec![-1isize; n];
            let structural_rank = btf_max_transversal(&case.a, &mut base_col_perm);
            assert_eq!(
                structural_rank,
                n,
                "Matrix {} is structurally singular (rank {structural_rank} < n={n})",
                case.name
            );

            b.iter_batched(
                || (base_col_perm.clone(), vec![0isize; n], vec![0usize; n + 1]),
                |(mut col_perm, mut row_perm, mut boundaries)| {
                    let nblocks = btf_scc(&case.a, &mut col_perm, &mut row_perm, &mut boundaries);
                    black_box(nblocks);
                    black_box(col_perm);
                    black_box(row_perm);
                    black_box(boundaries);
                },
                BatchSize::PerIteration,
            );
        });
    }

    group.finish();
}


fn bench_full_analyze(c: &mut Criterion) {
    let cases = load_cases();
    let mut group = c.benchmark_group("klu/analyze");

    let config = KluConfig::default();

    for case in cases {
        group.throughput(Throughput::Elements(case.nnz as u64));
        group.bench_with_input(BenchmarkId::from_parameter(&case.name), case, |b, case| {
            assert!(
                case.a.is_square(),
                "Matrix {} is not square ({}x{})",
                case.name,
                case.a.dim.nrows,
                case.a.dim.ncols
            );
            b.iter(|| {
                let sym = klu::analyze(&case.a, &config).expect("klu analyze");
                black_box(sym);
            });
        });
    }

    group.finish();
}

criterion_group!(
    klu_analyze,
    bench_btf_max_transversal,
    bench_btf_scc_after_max_transversal,
    bench_full_analyze
);
criterion_main!(klu_analyze);

