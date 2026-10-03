//! The lexer and the parser. The setups build and leak the text, so a benchmark counts
//! neither its generation nor its drop, and what it returns is dropped after the
//! measurement; only `drop_parsed` measures a drop, on purpose. `parse` is the only
//! entry point and lexes first, so the parser alone costs `parse_file` − `lex`, exactly
//! (both counts are exact).

mod common;

use std::hint::black_box;

use gungraun::prelude::*;

use spicy_bench::syntax::{expressions, syntax_errors};
use spicy_bench::typical::{Contracts, board};
use spicy_bench::{leak, owned_parsed};
use spicy_lang::lexer::{LexError, Tokens, check, scan};
use spicy_lang::parser::{Parsed, parse};

// Setups: the typical design's text, and its tree (owned, for `drop_parsed`).

fn typical(copies: usize) -> &'static str {
    leak(board(copies, Contracts::With))
}

fn typical_tree(copies: usize) -> Parsed<'static> {
    owned_parsed(board(copies, Contracts::With))
}

// Both passes of the lexer: the tokens, then their problems.
#[library_benchmark]
#[bench::typical(args = (100), setup = typical)]
fn lex(src: &'static str) -> (Tokens<'static>, Vec<LexError>) {
    let tokens = scan(black_box(src));
    let errors = check(&tokens);
    (tokens, errors)
}

// The lexer and the parser on a clean file.
#[library_benchmark]
#[bench::typical(args = (100), setup = typical)]
// The expression path, which the typical design uses little: 250 specs of 24 terms
// each (the Pratt loop at every level, quantities of every form, parentheses).
#[bench::expressions(args = (expressions(250)), setup = leak)]
fn parse_file(src: &'static str) -> Parsed<'static> {
    parse(black_box(src))
}

// The lexer and the parser on a file of mistakes at n and 4n (17 broken lines per
// copy: ports, statements, setup entries, items): reporting each mistake and skipping
// past it stay linear.
#[library_benchmark]
#[benches::syntax_errors(args = [syntax_errors(64), syntax_errors(256)], setup = leak)]
fn parse_mistakes(src: &'static str) -> Parsed<'static> {
    parse(black_box(src))
}

// What a re-parse throws away: the drop of the typical design's tree, which is all
// this function does (its text stays leaked). DHAT would see nothing here, since no
// block is allocated inside.
#[library_benchmark]
#[bench::typical(args = (100), setup = typical_tree)]
fn drop_parsed(parsed: Parsed<'static>) {
    drop(black_box(parsed));
}

library_benchmark_group!(
    name = typical,
    config = common::typical(),
    benchmarks = [lex, parse_file]
);
library_benchmark_group!(
    name = drop,
    config = common::drop(),
    benchmarks = drop_parsed
);
library_benchmark_group!(
    name = scaling,
    config = common::scaling(),
    benchmarks = parse_mistakes
);

main!(library_benchmark_groups = [typical, drop, scaling]);
