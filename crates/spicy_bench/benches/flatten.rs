//! Flatten: every root of a resolved design, with its placements expanded. The setups
//! parse and resolve each input outside the measurement, so only flatten is counted.

mod common;

use std::hint::black_box;

use gungraun::prelude::*;

use spicy_bench::hierarchy::{deep, shorted, wide_circuit, wide_default_setup};
use spicy_bench::typical::{Contracts, board, library};
use spicy_bench::{owned_elaborated, resolved};
use spicy_lang::elaborate::Elaborated;
use spicy_lang::resolve::Resolved;
use spicy_model::flatten::{Flattened, flatten};

// Setups: the design each benchmark flattens, parsed and resolved outside the
// measurement.

fn typical(copies: usize) -> &'static Resolved {
    resolved(board(copies, Contracts::With))
}

fn typical_plain(copies: usize) -> &'static Resolved {
    resolved(board(copies, Contracts::Without))
}

fn typical_elaborated(copies: usize) -> Elaborated {
    owned_elaborated(board(copies, Contracts::With))
}

// The typical design: flatten's cost per placement, and its allocations. `typical_plain`
// is the same blocks without the env, setup and contract. Flatten reads a setup only on
// a root (pass 8), and the board's root has none: the copies are placed, not roots. So
// the two make the same allocations and differ only in malloc's work on the heap each
// setup leaves (under 0.3%). `many_roots` and `wide_default_setup` measure pass 8.
#[library_benchmark]
#[bench::typical(args = (100), setup = typical)]
#[bench::typical_plain(args = (100), setup = typical_plain)]
fn flatten_design(resolved: &'static Resolved) -> Flattened {
    let resolved = black_box(resolved);
    flatten(&resolved.design, &resolved.source_map)
}

// One shape at n and 4n (4× is linear):
// - `deep`: n levels, each placing the one below;
// - `wide_circuit`: one circuit of 2n parts, n log n from flatten's two sorts by name
//   (the parts, in `placed_parts`, and the nets, in `nets`);
// - `many_roots`: a library of n copies, each a root of its own;
// - `shorted`: a problem in each of n parts of a block placed twice, the error path;
// - `wide_default_setup`: a root whose default setup has n ports, each a Range knob.
#[library_benchmark]
#[benches::deep(args = [deep(500), deep(2000)], setup = resolved)]
#[benches::wide_circuit(args = [wide_circuit(500), wide_circuit(2000)], setup = resolved)]
#[benches::many_roots(args = [library(50, Contracts::With), library(200, Contracts::With)], setup = resolved)]
#[benches::shorted(args = [shorted(500), shorted(2000)], setup = resolved)]
#[benches::wide_default_setup(args = [wide_default_setup(500), wide_default_setup(2000)], setup = resolved)]
fn flatten_shape(resolved: &'static Resolved) -> Flattened {
    let resolved = black_box(resolved);
    flatten(&resolved.design, &resolved.source_map)
}

// What a re-elaboration throws away: the resolved design and the flat designs. DHAT sees
// no allocation here (none is made inside).
#[library_benchmark]
#[bench::typical(args = (100), setup = typical_elaborated)]
fn drop_elaborated(elaborated: Elaborated) {
    drop(black_box(elaborated));
}

library_benchmark_group!(
    name = typical,
    config = common::typical(),
    benchmarks = flatten_design
);
library_benchmark_group!(
    name = drop,
    config = common::drop(),
    benchmarks = drop_elaborated
);
library_benchmark_group!(
    name = scaling,
    config = common::scaling(),
    benchmarks = flatten_shape
);

main!(library_benchmark_groups = [typical, drop, scaling]);
