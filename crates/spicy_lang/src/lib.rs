//! The language front-end: `.spl` text → tokens (→ syntax tree, later), with diagnostics.
//!
//! Design: `docs/ecad/lexer.md` (lexer), `docs/ecad/grammar.md` (the grammar).

pub mod diagnostic;
pub mod lexer;

/// Test support: only in this crate's tests and in fuzz builds (cargo-fuzz sets `cfg(fuzzing)`).
#[cfg(any(test, fuzzing))]
pub mod testing;
