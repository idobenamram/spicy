//! The lexer: text → tokens, in two passes (lexer.md).
//!
//! 1. [`scan`] cuts the text into tokens. It never fails: every byte lands in exactly
//!    one token, trivia included, and problems become token kinds (`Unknown`,
//!    `IdentNonAscii`, `UnterminatedBlockComment`) instead of errors.
//! 2. [`check`] reads the tokens and reports every problem, with fixes.

mod check;
pub(crate) mod lookalike;
mod quantity;
mod token;

pub use check::{Fix, LexError, LexErrorKind, check};
pub use quantity::{
    QuantityError, QuantityErrorKind, QuantityLit, UnitSym, decode_quantity, suffix_suggestions,
};
pub use token::{Span, TokenIdx, TokenKind, Tokens};

use quantity::{is_suffix_char, scan_mantissa};

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
    let mut scanner = Scanner {
        src,
        bytes: src.as_bytes(),
        pos: 0,
    };
    // A rough guess: one token per 4 bytes of source.
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
    kinds.push(TokenKind::Eof);
    starts.push(src.len() as u32);
    starts.push(src.len() as u32);
    Tokens { src, kinds, starts }
}

struct Scanner<'src> {
    src: &'src str,
    bytes: &'src [u8],
    pos: usize,
}

impl Scanner<'_> {
    fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn rest(&self) -> &str {
        &self.src[self.pos..]
    }

    /// The character at the current position (only called on non-ASCII bytes).
    fn char_here(&self) -> char {
        self.rest()
            .chars()
            .next()
            .expect("pos is inside the source")
    }

    /// Consumes one token and returns its kind. Precondition: not at the end.
    fn next_kind(&mut self) -> TokenKind {
        use TokenKind::*;
        let b = self.bytes[self.pos];
        let single = |s: &mut Self, kind| {
            s.pos += 1;
            kind
        };
        match b {
            b' ' | b'\t' | b'\r' | b'\n' => {
                while matches!(self.peek(0), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                    self.pos += 1;
                }
                Whitespace
            }
            b'/' => match self.peek(1) {
                Some(b'/') => self.line_comment(),
                Some(b'*') => self.block_comment(),
                _ => single(self, Slash),
            },
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.ident(),
            b'0'..=b'9' => self.quantity(),
            b'+' if self.rest().starts_with("+/-") => {
                self.pos += 3;
                PlusMinus
            }
            b'.' if self.rest().starts_with("..=") => {
                self.pos += 3;
                DotDotEq
            }
            b'.' if self.peek(1) == Some(b'.') => {
                self.pos += 2;
                DotDot
            }
            b':' if self.peek(1) == Some(b':') => {
                self.pos += 2;
                ColonColon
            }
            b'<' if self.peek(1) == Some(b'=') => {
                self.pos += 2;
                Le
            }
            b'>' if self.peek(1) == Some(b'=') => {
                self.pos += 2;
                Ge
            }
            b'{' => single(self, LBrace),
            b'}' => single(self, RBrace),
            b'(' => single(self, LParen),
            b')' => single(self, RParen),
            b'[' => single(self, LBracket),
            b']' => single(self, RBracket),
            b'<' => single(self, Lt),
            b'>' => single(self, Gt),
            b',' => single(self, Comma),
            b';' => single(self, Semi),
            b':' => single(self, Colon),
            b'.' => single(self, Dot),
            b'=' => single(self, Eq),
            b'+' => single(self, Plus),
            b'-' => single(self, Minus),
            b'*' => single(self, Star),
            b'#' => single(self, Pound),
            b'?' => single(self, Question),
            0x80.. => {
                let c = self.char_here();
                if c == '±' {
                    self.pos += c.len_utf8();
                    PlusMinus
                } else if c.is_alphanumeric() {
                    self.ident()
                } else {
                    self.pos += c.len_utf8();
                    Unknown
                }
            }
            // Any other ASCII character: `%` on its own, `!`, `"`, control characters, …
            _ => single(self, Unknown),
        }
    }

    /// `///` is a doc comment, `//` and `////…` are plain comments (as in Rust).
    /// Both run to the end of the line, not including the newline.
    fn line_comment(&mut self) -> TokenKind {
        let is_doc = self.rest().starts_with("///") && self.peek(3) != Some(b'/');
        let mut len = self.bytes[self.pos..]
            .iter()
            .position(|&b| b == b'\n')
            .unwrap_or(self.bytes.len() - self.pos);
        // With CRLF line ends the `\r` belongs to the line break, not the comment.
        if len > 2 && self.bytes[self.pos + len - 1] == b'\r' {
            len -= 1;
        }
        self.pos += len;
        if is_doc {
            TokenKind::DocComment
        } else {
            TokenKind::LineComment
        }
    }

    /// `/* … */`, nested as in Rust. Walking byte by byte is safe: we only stop on ASCII
    /// `*/` or at the end, both character boundaries.
    fn block_comment(&mut self) -> TokenKind {
        self.pos += 2;
        let mut depth = 1u32;
        while self.pos < self.bytes.len() {
            match (self.bytes[self.pos], self.peek(1)) {
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

    /// ASCII letters, digits and `_`, plus any non-ASCII alphanumeric (which makes the
    /// token `IdentNonAscii`, rejected by `check`). Then the keyword lookup.
    fn ident(&mut self) -> TokenKind {
        let start = self.pos;
        let mut ascii = true;
        while let Some(b) = self.peek(0) {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
            } else if b >= 0x80 && self.char_here().is_alphanumeric() {
                ascii = false;
                self.pos += self.char_here().len_utf8();
            } else {
                break;
            }
        }
        if !ascii {
            return TokenKind::IdentNonAscii;
        }
        keyword(&self.src[start..self.pos]).unwrap_or(TokenKind::Ident)
    }

    /// The number part (shared with the decoder), then every glued suffix character.
    fn quantity(&mut self) -> TokenKind {
        self.pos += scan_mantissa(&self.bytes[self.pos..]).len;
        while let Some(c) = self.rest().chars().next() {
            if !is_suffix_char(c) {
                break;
            }
            self.pos += c.len_utf8();
        }
        TokenKind::Quantity
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

/// The case-file suite and property tests (lexer.md §7).
#[cfg(test)]
mod tests {
    use super::{LexErrorKind, check, scan};
    use crate::testing::{
        Rng, assert_every_kind_has_a_case, check_invariants, dump, file_name, read,
    };

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
