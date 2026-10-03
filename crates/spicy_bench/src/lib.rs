//! Inputs for the front end's benchmarks (`benches/`, one file per stage).
//!
//! Every generator is a pure function of its size, so two runs, or two commits,
//! measure the same text. Names are numbered with a fixed width (`n00042`), so each
//! item is the same text at every size and a linear stage costs exactly 4× at 4× the
//! size. Each module's tests check that its inputs still have the problems they're
//! meant to have, and no others: a language change that breaks a "clean" input would
//! otherwise turn its benchmark into a measurement of the error path.
//!
//! The setups below build, outside the measurement, what the measured stage reads. They
//! leak it: each benchmark runs in a process of its own, and a `&'static` input means
//! the benchmark function never drops it (an input dropped inside is measured too).

pub mod hierarchy;
pub mod names;
pub mod syntax;
pub mod typical;

use spicy_errors::DiagKind;
use spicy_lang::elaborate::{Elaborated, elaborate};
use spicy_lang::parser::{Parsed, parse};
use spicy_lang::resolve::{Resolved, resolve};

/// `src`, for the rest of the process.
pub fn leak(src: String) -> &'static str {
    src.leak()
}

/// `src` parsed, for the rest of the process.
pub fn parsed(src: String) -> &'static Parsed<'static> {
    Box::leak(Box::new(parse(leak(src))))
}

/// `src` parsed and resolved, for the rest of the process.
pub fn resolved(src: String) -> &'static Resolved {
    Box::leak(Box::new(resolve(parsed(src))))
}

/// `src` parsed, owned: for measuring its drop.
pub fn owned_parsed(src: String) -> Parsed<'static> {
    parse(leak(src))
}

/// `src` elaborated, owned: for measuring its drop.
pub fn owned_elaborated(src: String) -> Elaborated {
    elaborate(parsed(src))
}

/// Each stage's problems, by kind name, in the order found: what the inputs' tests
/// check.
#[derive(Debug, Default, PartialEq)]
pub struct Problems {
    pub lex: Vec<&'static str>,
    pub parse: Vec<&'static str>,
    pub resolve: Vec<&'static str>,
    pub flatten: Vec<&'static str>,
}

impl Problems {
    /// Every stage's problems in `src`.
    pub fn of(src: &str) -> Self {
        let parsed = parse(src);
        let elaborated = elaborate(&parsed);
        Problems {
            lex: parsed.lex_errors.iter().map(|e| e.kind.name()).collect(),
            parse: parsed.errors.iter().map(|e| e.kind.name()).collect(),
            resolve: elaborated
                .resolved
                .errors
                .iter()
                .map(|e| e.kind.name())
                .collect(),
            flatten: elaborated
                .flattened
                .errors
                .iter()
                .map(|e| e.kind.name())
                .collect(),
        }
    }

    /// How many problems of `kind` any stage found.
    pub fn count(&self, kind: &str) -> usize {
        [&self.lex, &self.parse, &self.resolve, &self.flatten]
            .into_iter()
            .flatten()
            .filter(|&&k| k == kind)
            .count()
    }

    /// The number of problems at all stages.
    pub fn total(&self) -> usize {
        self.lex.len() + self.parse.len() + self.resolve.len() + self.flatten.len()
    }
}
