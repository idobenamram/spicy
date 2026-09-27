//! Where something was written: one byte range type for every stage, from the lexer
//! to the design's source map (as rustc has one `Span` in `rustc_span`). It lives here,
//! the lowest crate that needs it: `spicy_lang` depends on this crate and re-exports it.

use std::ops::Range;

/// A byte range in one source file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end);
        Self { start, end }
    }

    pub fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}
