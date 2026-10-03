//! Resolve: the design from the tree, each block once. The setups parse outside the
//! measurement and leak the tree, so a benchmark counts resolve alone (not the parse,
//! nor the tree's drop); the `Resolved` it returns is dropped after the measurement.

mod common;

use std::hint::black_box;

use gungraun::prelude::*;

use spicy_bench::names::{
    broken_reads, contract_specs, duplicates, measure_chain, named_values, unknown_names,
    unknown_setup_keys, wide_setup,
};
use spicy_bench::parsed;
use spicy_bench::typical::{Contracts, board};
use spicy_lang::parser::Parsed;
use spicy_lang::resolve::{Resolved, resolve};

// Setups: the tree each benchmark resolves, parsed outside the measurement.

fn typical(copies: usize) -> &'static Parsed<'static> {
    parsed(board(copies, Contracts::With))
}

fn typical_plain(copies: usize) -> &'static Parsed<'static> {
    parsed(board(copies, Contracts::Without))
}

// The typical design, clean, with and without each copy's env, setup and contract:
// `typical` − `typical_plain` is what those three items cost.
#[library_benchmark]
#[bench::typical(args = (100), setup = typical)]
#[bench::typical_plain(args = (100), setup = typical_plain)]
fn resolve_file(parsed: &'static Parsed<'static>) -> Resolved {
    resolve(black_box(parsed))
}

// One shape at n and 4n each: a linear cost grows 4×, a quadratic one 16×.
#[library_benchmark]
// The file's consts and envs, and reads of the consts.
#[benches::named_values(args = [named_values(500), named_values(2000)], setup = parsed)]
// "Defined twice", for every kind of item.
#[benches::duplicates(args = [duplicates(250), duplicates(1000)], setup = parsed)]
// Reads of a broken const: tainted, not reported.
#[benches::broken_reads(args = [broken_reads(250), broken_reads(1000)], setup = parsed)]
// Unknown names: only the first 64 get a "did you mean" search.
#[benches::unknown_names(args = [unknown_names(250), unknown_names(1000)], setup = parsed)]
// A setup of a block with many ports, each port's two entries apart: grouping them
// by port stays linear.
#[benches::wide_setup(args = [wide_setup(500), wide_setup(2000)], setup = parsed)]
// Unknown setup keys on a block with as many ports: quadratic today (each report
// copies every slot's name), so the sizes are small. At 500, less `wide_setup` at 500:
// the cost of reporting 500 keys.
#[benches::unknown_setup_keys(args = [unknown_setup_keys(125), unknown_setup_keys(500)], setup = parsed)]
// A contract's measures, each typed on demand by the one before it, one call deeper
// each: the sizes stay far below the ~1,100 that overflow a 2 MiB stack (release).
#[benches::measure_chain(args = [measure_chain(125), measure_chain(500)], setup = parsed)]
// A contract's specs, each a measure, a limit and a bound; each fourth one `pub`.
#[benches::contract_specs(args = [contract_specs(250), contract_specs(1000)], setup = parsed)]
fn resolve_shape(parsed: &'static Parsed<'static>) -> Resolved {
    resolve(black_box(parsed))
}

library_benchmark_group!(
    name = typical,
    config = common::typical(),
    benchmarks = resolve_file
);
library_benchmark_group!(
    name = scaling,
    config = common::scaling(),
    benchmarks = resolve_shape
);

main!(library_benchmark_groups = [typical, scaling]);
