//! The lexer: text → tokens and errors (lexer.md). Two passes, read top to bottom:
//!
//! 1. [`scan`] (this file) cuts the text into tokens. It never fails: every byte lands
//!    in exactly one token, trivia included, and problems become token kinds
//!    (`Unknown`, `IdentNonAscii`, `UnterminatedBlockComment`) instead of errors, as
//!    in rustc_lexer.
//! 2. [`check`] (`check.rs`) reads the tokens and reports every problem, with fixes, as
//!    rustc_parse "cooks" each raw token. The errors are data (`error.rs`).
//!
//! Numbers and units have a module of their own (`quantity.rs`): the scanner asks it
//! where a `Quantity` ends ([`quantity::quantity_len`]), `check` asks it the same for
//! a spaced-out unit and to decode each number, and the parser calls
//! [`decode_quantity`] for the value. Look-alike characters (`−` for `-`) are a table
//! in `lookalike.rs`, read by `check` and the parser.

mod check;
mod error;
pub(crate) mod lookalike;
mod quantity;
mod token;

pub use check::check;
pub use error::{LexError, LexErrorKind};
pub use quantity::{
    QuantityError, QuantityErrorKind, QuantityLit, decode_quantity, suffix_suggestions,
};
pub use token::{TokenIdx, TokenKind, Tokens};

use quantity::quantity_len;

const BOM: &str = "\u{FEFF}";

/// Pass 1: cuts `src` into tokens (lexer.md §5).
///
/// Never fails. The tokens tile the input exactly and end with a zero-length `Eof`.
///
/// # Panics
/// If `src` is 4 GiB or larger; offsets are `u32` (lexer.md L3). Source loading checks
/// the size before lexing.
pub fn scan(src: &str) -> Tokens<'_> {
    assert!(
        src.len() < u32::MAX as usize,
        "source files are limited to 4 GiB"
    );
    let mut scanner = Scanner { src, pos: 0 };
    // A rough guess: one token per 4 bytes of source; `starts` has one extra entry.
    let mut kinds = Vec::with_capacity(src.len() / 4 + 1);
    let mut starts = Vec::with_capacity(src.len() / 4 + 2);
    if src.starts_with(BOM) {
        kinds.push(TokenKind::Whitespace);
        starts.push(0);
        scanner.pos = BOM.len();
    }
    while scanner.pos < src.len() {
        let start = scanner.pos as u32;
        let kind = scanner.next_kind();
        debug_assert!(
            scanner.pos as u32 > start,
            "every token but Eof is non-empty"
        );
        kinds.push(kind);
        starts.push(start);
    }
    // The zero-length Eof: its start, then the end entry that closes it.
    kinds.push(TokenKind::Eof);
    starts.push(src.len() as u32);
    starts.push(src.len() as u32);
    Tokens { src, kinds, starts }
}

/// A cursor over the source. `pos` is always on a character boundary.
struct Scanner<'src> {
    src: &'src str,
    pos: usize,
}

impl Scanner<'_> {
    /// Consumes one token and returns its kind. Precondition: not at the end.
    fn next_kind(&mut self) -> TokenKind {
        use TokenKind::*;
        match self.src.as_bytes()[self.pos] {
            // A byte loop, not `take_while`: whitespace is the most common token, and
            // the closure over `char` makes the whole scan about a third slower.
            b' ' | b'\t' | b'\r' | b'\n' => {
                while matches!(self.peek(0), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                    self.pos += 1;
                }
                Whitespace
            }
            b'/' => match self.peek(1) {
                Some(b'/') => self.line_comment(),
                Some(b'*') => self.block_comment(),
                _ => self.take(1, Slash),
            },
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.ident(),
            b'0'..=b'9' => self.take(quantity_len(self.rest()), Quantity),
            // Operators of several characters, longest first.
            b'+' if self.at("+/-") => self.take(3, PlusMinus),
            b'.' if self.at("..=") => self.take(3, DotDotEq),
            b'.' if self.at("..") => self.take(2, DotDot),
            b':' if self.at("::") => self.take(2, ColonColon),
            b'<' if self.at("<=") => self.take(2, Le),
            b'>' if self.at(">=") => self.take(2, Ge),
            b'{' => self.take(1, LBrace),
            b'}' => self.take(1, RBrace),
            b'(' => self.take(1, LParen),
            b')' => self.take(1, RParen),
            b'[' => self.take(1, LBracket),
            b']' => self.take(1, RBracket),
            b'<' => self.take(1, Lt),
            b'>' => self.take(1, Gt),
            b',' => self.take(1, Comma),
            b';' => self.take(1, Semi),
            b':' => self.take(1, Colon),
            b'.' => self.take(1, Dot),
            b'=' => self.take(1, Eq),
            b'+' => self.take(1, Plus),
            b'-' => self.take(1, Minus),
            b'*' => self.take(1, Star),
            b'#' => self.take(1, Pound),
            b'?' => self.take(1, Question),
            0x80.. => match self.char_here() {
                '±' => self.take('±'.len_utf8(), PlusMinus),
                c if c.is_alphanumeric() => self.ident(),
                c => self.take(c.len_utf8(), Unknown),
            },
            // Any other ASCII character: `%` on its own, `!`, `"`, control characters, …
            _ => self.take(1, Unknown),
        }
    }

    /// `///` is a doc comment, `//` and `////…` are plain comments (as in Rust). Both
    /// run to the end of the line; the line break (`\n` or `\r\n`) is not part of them.
    fn line_comment(&mut self) -> TokenKind {
        let kind = if self.at("///") && !self.at("////") {
            TokenKind::DocComment
        } else {
            TokenKind::LineComment
        };
        let rest = self.rest_bytes();
        let line = &rest[..rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len())];
        self.pos += line.strip_suffix(b"\r").unwrap_or(line).len();
        kind
    }

    /// `/* … */`, nested as in Rust. Walking byte by byte is safe: we only stop on ASCII
    /// `*/` or at the end, both character boundaries.
    fn block_comment(&mut self) -> TokenKind {
        self.pos += 2;
        let mut depth = 1u32;
        while let Some(b) = self.peek(0) {
            match (b, self.peek(1)) {
                (b'/', Some(b'*')) => {
                    depth += 1;
                    self.pos += 2;
                }
                (b'*', Some(b'/')) => {
                    depth -= 1;
                    self.pos += 2;
                    if depth == 0 {
                        return TokenKind::BlockComment;
                    }
                }
                _ => self.pos += 1,
            }
        }
        TokenKind::UnterminatedBlockComment
    }

    /// Letters, digits and `_`, non-ASCII letters included (lexer.md L8). A word with
    /// any non-ASCII in it is `IdentNonAscii`, rejected by `check`; the others go through
    /// the keyword lookup.
    fn ident(&mut self) -> TokenKind {
        let start = self.pos;
        // A byte loop over the ASCII run, not `take_while`: names are the most common
        // token after whitespace, and the closure over `char` is measurably slower.
        while matches!(self.peek(0), Some(b) if b.is_ascii_alphanumeric() || b == b'_') {
            self.pos += 1;
        }
        if self.peek(0).is_none_or(|b| b.is_ascii()) {
            return keyword(&self.src[start..self.pos]).unwrap_or(TokenKind::Ident);
        }
        // A non-ASCII letter: the rest of the name, a char at a time.
        self.take_while(|c| c == '_' || c.is_alphanumeric());
        if self.src[start..self.pos].is_ascii() {
            keyword(&self.src[start..self.pos]).unwrap_or(TokenKind::Ident)
        } else {
            TokenKind::IdentNonAscii
        }
    }
}

/// MVP keywords get their own kinds; every reserved word is `KwReserved` (grammar.md §2.3).
fn keyword(word: &str) -> Option<TokenKind> {
    use TokenKind::*;
    Some(match word {
        "block" => KwBlock,
        "contract" => KwContract,
        "port" => KwPort,
        "net" => KwNet,
        "let" => KwLet,
        "assume" => KwAssume,
        "spec" => KwSpec,
        "in" => KwIn,
        "use" | "mod" | "pub" | "fn" | "const" | "enum" | "type" | "for" | "if" | "else"
        | "match" | "where" | "as" | "true" | "false" | "self" | "super" | "crate" | "signal"
        | "interface" | "family" | "env" | "param" | "bench" => KwReserved,
        _ => return None,
    })
}

/// The cursor: looking at and consuming the text (as rustc_lexer's `Cursor`, with
/// `first`, `bump` and `eat_while`), on bytes, decoding a `char` only for non-ASCII.
impl Scanner<'_> {
    /// The byte `offset` bytes ahead of the current position.
    fn peek(&self, offset: usize) -> Option<u8> {
        self.src.as_bytes().get(self.pos + offset).copied()
    }

    fn rest(&self) -> &str {
        &self.src[self.pos..]
    }

    fn rest_bytes(&self) -> &[u8] {
        &self.src.as_bytes()[self.pos..]
    }

    /// Whether the text at the current position starts with `s`.
    fn at(&self, s: &str) -> bool {
        self.rest_bytes().starts_with(s.as_bytes())
    }

    /// The character at the current position. Precondition: not at the end.
    fn char_here(&self) -> char {
        self.rest()
            .chars()
            .next()
            .expect("pos is inside the source")
    }

    /// Consumes `len` bytes as one token of `kind`.
    fn take(&mut self, len: usize, kind: TokenKind) -> TokenKind {
        self.pos += len;
        kind
    }

    /// Consumes every character that satisfies `keep`. ASCII is read byte by byte;
    /// only non-ASCII is decoded.
    fn take_while(&mut self, keep: impl Fn(char) -> bool) {
        while let Some(b) = self.peek(0) {
            let c = if b.is_ascii() {
                b as char
            } else {
                self.char_here()
            };
            if !keep(c) {
                break;
            }
            self.pos += c.len_utf8();
        }
    }
}

/// The case-file suite and property tests (lexer.md §7).
#[cfg(test)]
mod tests {
    use super::{LexErrorKind, check, scan};
    use crate::testing::{
        Rng, assert_every_kind_has_a_case, check_invariants, dump, file_name, read,
    };
    use spicy_errors::DiagKind;

    /// `ok/` cases: no errors, and a snapshot of the tokens.
    #[test]
    fn ok_cases() {
        insta::glob!("../../test_data/lexer", "ok/*.spl", |path| {
            let src = read(path);
            check_invariants(&src);
            let errors = check(&scan(&src));
            assert!(
                errors.is_empty(),
                "{}: unexpected errors {errors:#?}",
                path.display()
            );
            insta::assert_snapshot!(dump(&file_name(path), &src));
        });
    }

    /// `err/` cases: at least one error, and a snapshot of tokens, errors and rendering.
    #[test]
    fn err_cases() {
        insta::glob!("../../test_data/lexer", "err/*.spl", |path| {
            let src = read(path);
            check_invariants(&src);
            let errors = check(&scan(&src));
            assert!(!errors.is_empty(), "{}: expected errors", path.display());
            insta::assert_snapshot!(dump(&file_name(path), &src));
        });
    }

    /// The design the MVP is built around lexes cleanly and round-trips.
    #[test]
    fn ce_amp() {
        let src = include_str!("../../../../circuits/ce_amp.spl");
        check_invariants(src);
        assert!(check(&scan(src)).is_empty());
        insta::assert_snapshot!(dump("ce_amp.spl", src));
    }

    /// Every error kind appears in at least one `err/` case.
    #[test]
    fn every_error_kind_has_a_case() {
        assert_every_kind_has_a_case("lexer/err", LexErrorKind::ALL_NAMES, |src| {
            check(&scan(src)).iter().map(|e| e.kind.name()).collect()
        });
    }

    /// Pieces the random inputs are built from, weighted toward what the lexer cares about
    /// (as Zig's tokenizer fuzzing weights its bytes).
    const PIECES: &[&str] = &[
        "0", "1", "4", "7", "9", "k", "M", "m", "u", "e", "E", "V", "F", "Hz", "dB", "C", "q", "_",
        ".", "..", "..=", "=", "+", "-", "*", "/", "+/-", "±", "%", "µ", "μ", "Ω", "Ω", "°", "º",
        "(", ")", "{", "}", "[", "]", "<", ">", "<=", ">=", ",", ";", ":", "::", "#", "?", "//",
        "///", "////", "/*", "*/", " ", " ", " ", "\n", "\t", "\r", "let", "in", "net", "fn", "a",
        "r1", "α", "−", ";", "\u{A0}", "\u{2009}", "\u{FEFF}", "\0", "\"", "§", "Meg",
    ];

    #[test]
    fn invariants_hold_on_random_input() {
        let mut rng = Rng(0x5eed_1234_abcd_ef01);
        for _ in 0..10_000 {
            let len = rng.below(48);
            let mut src = String::new();
            for _ in 0..len {
                if rng.below(16) == 0 {
                    // Now and then, any character at all.
                    if let Some(c) = char::from_u32(rng.below(0x3000) as u32) {
                        src.push(c);
                    }
                } else {
                    src.push_str(PIECES[rng.below(PIECES.len())]);
                }
            }
            let result = std::panic::catch_unwind(|| check_invariants(&src));
            assert!(result.is_ok(), "invariant failed on input {src:?}");
        }
    }

    /// Throughput, for the record. Run with
    /// `cargo test -p spicy_lang --release -- --ignored --nocapture speed`.
    #[test]
    #[ignore]
    fn speed() {
        let one = include_str!("../../../../circuits/ce_amp.spl");
        let src = one.repeat(10_000_000 / one.len());
        let start = std::time::Instant::now();
        let tokens = scan(&src);
        let scanned = start.elapsed();
        let errors = check(&tokens);
        let total = start.elapsed();
        assert!(errors.is_empty());
        let mb = src.len() as f64 / 1e6;
        println!(
            "{mb:.1} MB, {} tokens: scan {:.0} MB/s, scan + check {:.0} MB/s",
            tokens.len(),
            mb / scanned.as_secs_f64(),
            mb / total.as_secs_f64()
        );
    }
}
