//! The language front-end: `.spl` text → tokens → syntax tree, with diagnostics.
//!
//! Design: `docs/ecad/grammar.md` (the grammar), `docs/ecad/lexer.md` (lexer),
//! `docs/ecad/ast.md` (syntax tree and parser).

pub mod diagnostic;
pub mod lexer;
pub mod parser;

/// Test support: only in this crate's tests and in fuzz builds (cargo-fuzz sets `cfg(fuzzing)`).
#[cfg(any(test, fuzzing))]
pub mod testing;
