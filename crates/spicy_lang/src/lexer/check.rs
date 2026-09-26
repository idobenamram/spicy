//! Pass 2: reads the tokens and reports every problem (lexer.md §6).

use codespan_reporting::diagnostic::{Diagnostic, Label};

use super::lookalike;
use super::quantity::{QuantityErrorKind, decode_quantity, is_suffix_char, scan_mantissa};
use super::token::{Span, TokenIdx, TokenKind, Tokens};

/// One lexer problem. Data, not text, so the editor and the AI get the fix as an edit
/// (lexer.md L10); [`LexError::diagnostic`] renders it.
#[derive(Clone, Debug, PartialEq)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub span: Span,
    pub fix: Option<Fix>,
}

/// A suggested edit: replace `span` with `replacement`.
#[derive(Clone, Debug, PartialEq)]
pub struct Fix {
    pub span: Span,
    pub replacement: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LexErrorKind {
    /// A `Quantity` that doesn't decode (`47q`, `1Meg`, `k°C`, `4.7k7`, `1e`, `1e400`).
    Quantity(QuantityErrorKind),
    /// `10 kΩ`: `word` is `kΩ`, `joined` is `10kΩ`.
    UnitSpace {
        word: String,
        joined: String,
    },
    /// `.5`
    LeadingDecimalPoint,
    /// `1.`
    TrailingDecimalPoint,
    /// `+-`
    PlusMinusTypo,
    /// `--`
    DoubleMinus,
    /// `%` not glued to a number.
    LonePercent,
    UnterminatedBlockComment,
    /// `−` (U+2212) instead of `-`, and the rest of the look-alike table.
    Lookalike {
        found: char,
        name: &'static str,
        replacement: &'static str,
    },
    UnexpectedChar(char),
    /// `r1_α`: `ident` is the whole identifier, `found` its first non-ASCII character.
    NonAsciiIdent {
        ident: String,
        found: char,
    },
}

impl LexErrorKind {
    /// The variant's name, for the test that every error kind has a case file.
    pub fn name(&self) -> &'static str {
        match self {
            LexErrorKind::Quantity(q) => match q {
                QuantityErrorKind::UnknownSuffix { .. } => "UnknownSuffix",
                QuantityErrorKind::Meg => "Meg",
                QuantityErrorKind::PrefixNotAllowed { .. } => "PrefixNotAllowed",
                QuantityErrorKind::SecondPrefix { .. } => "SecondPrefix",
                QuantityErrorKind::DecimalAndInfix => "DecimalAndInfix",
                QuantityErrorKind::MissingExponentDigits => "MissingExponentDigits",
                QuantityErrorKind::TooLarge => "TooLarge",
            },
            LexErrorKind::UnitSpace { .. } => "UnitSpace",
            LexErrorKind::LeadingDecimalPoint => "LeadingDecimalPoint",
            LexErrorKind::TrailingDecimalPoint => "TrailingDecimalPoint",
            LexErrorKind::PlusMinusTypo => "PlusMinusTypo",
            LexErrorKind::DoubleMinus => "DoubleMinus",
            LexErrorKind::LonePercent => "LonePercent",
            LexErrorKind::UnterminatedBlockComment => "UnterminatedBlockComment",
            LexErrorKind::Lookalike { .. } => "Lookalike",
            LexErrorKind::UnexpectedChar(_) => "UnexpectedChar",
            LexErrorKind::NonAsciiIdent { .. } => "NonAsciiIdent",
        }
    }

    /// Every value [`name`](Self::name) can return. The match in `name` is exhaustive,
    /// so a new variant won't compile until it has a name; add the name here too.
    pub const ALL_NAMES: &'static [&'static str] = &[
        "UnknownSuffix",
        "Meg",
        "PrefixNotAllowed",
        "SecondPrefix",
        "DecimalAndInfix",
        "MissingExponentDigits",
        "TooLarge",
        "UnitSpace",
        "LeadingDecimalPoint",
        "TrailingDecimalPoint",
        "PlusMinusTypo",
        "DoubleMinus",
        "LonePercent",
        "UnterminatedBlockComment",
        "Lookalike",
        "UnexpectedChar",
        "NonAsciiIdent",
    ];

    /// Stable code, in `language.md`'s style.
    pub fn code(&self) -> &'static str {
        match self {
            LexErrorKind::Quantity(q) => match q {
                QuantityErrorKind::MissingExponentDigits | QuantityErrorKind::TooLarge => {
                    "E-number"
                }
                _ => "E-unit-suffix",
            },
            LexErrorKind::UnitSpace { .. } => "E-unit-space",
            LexErrorKind::LeadingDecimalPoint | LexErrorKind::TrailingDecimalPoint => "E-number",
            LexErrorKind::PlusMinusTypo => "E-plus-minus",
            LexErrorKind::DoubleMinus => "E-double-minus",
            LexErrorKind::LonePercent => "E-percent",
            LexErrorKind::UnterminatedBlockComment => "E-unterminated-comment",
            LexErrorKind::Lookalike { .. } => "E-lookalike",
            LexErrorKind::UnexpectedChar(_) => "E-unexpected-char",
            LexErrorKind::NonAsciiIdent { .. } => "E-ascii-ident",
        }
    }
}

/// Pass 2: every lexer problem in `tokens`, in source order.
///
/// One pass over the tokens, each looked at once (as rustc cooks each raw token in
/// `next_token_from_cursor`, and rust-analyzer in `LexedStr::new`). Each error claims
/// its span, and an error starting inside a claimed span is dropped (lexer.md §6.2):
/// that's how `10 kΩ` reports only "remove the space", not also "identifiers are ASCII"
/// for `kΩ`. Every check reports at or after its own token, so claims only move
/// forward and one position is enough to track them.
pub fn check(tokens: &Tokens) -> Vec<LexError> {
    let mut out = Errors::default();
    for i in 0..tokens.len() {
        match tokens.kind(i) {
            // Most specific first: a spaced-out unit, then the number itself.
            TokenKind::Quantity => {
                unit_space(tokens, i, &mut out);
                quantity(tokens, i, &mut out);
                trailing_point(tokens, i, &mut out);
            }
            TokenKind::Dot => leading_point(tokens, i, &mut out),
            TokenKind::Plus => plus_minus_typo(tokens, i, &mut out),
            TokenKind::Minus => double_minus(tokens, i, &mut out),
            TokenKind::Unknown => unknown_char(tokens, i, &mut out),
            TokenKind::UnterminatedBlockComment => unterminated_comment(tokens, i, &mut out),
            TokenKind::IdentNonAscii => non_ascii_ident(tokens, i, &mut out),
            _ => {}
        }
    }
    out.errors
}

#[derive(Default)]
struct Errors {
    errors: Vec<LexError>,
    /// Everything before this offset belongs to an error already reported.
    claimed_until: u32,
}

impl Errors {
    fn push(&mut self, error: LexError) {
        if error.span.start < self.claimed_until {
            return;
        }
        self.claimed_until = error.span.end;
        self.errors.push(error);
    }
}

fn err(kind: LexErrorKind, span: Span) -> LexError {
    LexError {
        kind,
        span,
        fix: None,
    }
}

fn with_fix(kind: LexErrorKind, span: Span, replacement: impl Into<String>) -> LexError {
    LexError {
        kind,
        span,
        fix: Some(Fix {
            span,
            replacement: replacement.into(),
        }),
    }
}

/// The span from the start of token `first` to the end of token `last`.
fn span_of(tokens: &Tokens, first: TokenIdx, last: TokenIdx) -> Span {
    Span::new(tokens.span(first).start, tokens.span(last).end)
}

/// `10 V`, `10 kΩ`, `10 °C`, `10 kΩ` (a thin space): a number, one gap on the same
/// line, then a word that would be a valid suffix if it touched the number.
fn unit_space(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    let gap = i + 1;
    let is_gap = match tokens.kind_or_eof(gap) {
        TokenKind::Whitespace => !tokens.text(gap).contains('\n'),
        TokenKind::Unknown => tokens.text(gap).chars().all(lookalike::is_space_like),
        _ => false,
    };
    if !is_gap {
        return;
    }
    let src = tokens.src();
    let word_start = tokens.span(gap).end as usize;
    let word_len: usize = src[word_start..]
        .chars()
        .take_while(|&c| is_suffix_char(c))
        .map(char::len_utf8)
        .sum();
    if word_len == 0 {
        return;
    }
    let word = &src[word_start..word_start + word_len];
    let joined = format!("{}{}", tokens.text(i), word);
    if decode_quantity(&joined).is_err() {
        return;
    }
    let span = Span::new(tokens.span(i).start, (word_start + word_len) as u32);
    out.push(with_fix(
        LexErrorKind::UnitSpace {
            word: word.to_string(),
            joined: joined.clone(),
        },
        span,
        joined,
    ));
}

fn quantity(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    let Err(e) = decode_quantity(tokens.text(i)) else {
        return;
    };
    let start = tokens.span(i).start;
    let span = Span::new(start + e.range.start, start + e.range.end);
    let fix = match &e.kind {
        QuantityErrorKind::Meg => Some("M".to_string()),
        QuantityErrorKind::PrefixNotAllowed { .. } | QuantityErrorKind::SecondPrefix { .. } => {
            Some(String::new())
        }
        QuantityErrorKind::UnknownSuffix { suggestions, .. } => suggestions.first().cloned(),
        _ => None,
    };
    let mut error = err(LexErrorKind::Quantity(e.kind), span);
    error.fix = fix.map(|replacement| Fix { span, replacement });
    out.push(error);
}

/// `1.`: a plain number touching a `Dot` that isn't a field access.
fn trailing_point(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    let text = tokens.text(i);
    let plain_number = scan_mantissa(text.as_bytes()).len == text.len();
    let field_access = matches!(
        tokens.kind_or_eof(i + 2),
        TokenKind::Ident | TokenKind::IdentNonAscii
    );
    if plain_number && tokens.kind_or_eof(i + 1) == TokenKind::Dot && !field_access {
        out.push(with_fix(
            LexErrorKind::TrailingDecimalPoint,
            span_of(tokens, i, i + 1),
            format!("{text}.0"),
        ));
    }
}

/// `.5`: a `Dot` touching a following number.
fn leading_point(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    if tokens.kind_or_eof(i + 1) == TokenKind::Quantity {
        out.push(with_fix(
            LexErrorKind::LeadingDecimalPoint,
            span_of(tokens, i, i + 1),
            format!("0.{}", tokens.text(i + 1)),
        ));
    }
}

/// `+-` (P1: in Rust it means `+ (-…)`).
fn plus_minus_typo(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    if tokens.kind_or_eof(i + 1) == TokenKind::Minus {
        out.push(with_fix(
            LexErrorKind::PlusMinusTypo,
            span_of(tokens, i, i + 1),
            "±",
        ));
    }
}

/// `--` (P1: in Rust it means `-(-…)`).
fn double_minus(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    if tokens.kind_or_eof(i + 1) == TokenKind::Minus {
        out.push(err(LexErrorKind::DoubleMinus, span_of(tokens, i, i + 1)));
    }
}

fn unknown_char(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    let span = tokens.span(i);
    let c = tokens.text(i).chars().next().expect("tokens are non-empty");
    let error = if c == '%' {
        err(LexErrorKind::LonePercent, span)
    } else if let Some(l) = lookalike::lookup(c) {
        with_fix(
            LexErrorKind::Lookalike {
                found: c,
                name: l.name,
                replacement: l.replacement,
            },
            span,
            l.replacement,
        )
    } else {
        err(LexErrorKind::UnexpectedChar(c), span)
    };
    out.push(error);
}

/// Points at the `/*` that is never closed.
fn unterminated_comment(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    let start = tokens.span(i).start;
    out.push(err(
        LexErrorKind::UnterminatedBlockComment,
        Span::new(start, start + 2),
    ));
}

fn non_ascii_ident(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    let ident = tokens.text(i);
    let found = ident
        .chars()
        .find(|c| !c.is_ascii())
        .expect("IdentNonAscii has a non-ASCII character");
    out.push(err(
        LexErrorKind::NonAsciiIdent {
            ident: ident.to_string(),
            found,
        },
        tokens.span(i),
    ));
}

fn code_point(c: char) -> String {
    format!("U+{:04X}", c as u32)
}

impl LexError {
    /// The diagnostic the user sees (rendered by [`crate::diagnostic::render`]).
    pub fn diagnostic(&self) -> Diagnostic<()> {
        let primary = Label::primary((), self.span.range());
        let (message, label, notes): (String, String, Vec<String>) = match &self.kind {
            LexErrorKind::Quantity(q) => match q {
                QuantityErrorKind::UnknownSuffix {
                    suffix,
                    suggestions,
                } => {
                    let mut notes = Vec::new();
                    if !suggestions.is_empty() {
                        let list: Vec<String> =
                            suggestions.iter().map(|s| format!("`{s}`")).collect();
                        notes.push(format!("help: did you mean {}?", list.join(" or ")));
                    }
                    notes.push(
                        "note: units are V A Ω F H Hz s W K °C % dB, optionally after a \
                         prefix f p n u µ m k M G T (case matters)"
                            .to_string(),
                    );
                    (
                        format!("unknown unit `{suffix}`"),
                        "not a unit or prefix".to_string(),
                        notes,
                    )
                }
                QuantityErrorKind::Meg => (
                    "`Meg` is SPICE's spelling of mega".to_string(),
                    "write `M` here".to_string(),
                    vec!["help: `1Meg` is `1M`; lowercase `m` is milli".to_string()],
                ),
                QuantityErrorKind::PrefixNotAllowed { prefix, unit } => (
                    format!("`{unit}` takes no prefix"),
                    format!("remove `{prefix}`"),
                    vec![],
                ),
                QuantityErrorKind::SecondPrefix { prefix } => (
                    "a number takes one prefix".to_string(),
                    format!("second prefix `{prefix}`"),
                    vec!["help: in `4k7` the `k` is already the prefix (4.7k)".to_string()],
                ),
                QuantityErrorKind::DecimalAndInfix => (
                    "an infix prefix needs a plain integer before it".to_string(),
                    "decimal point or exponent, and an infix prefix".to_string(),
                    vec!["help: use one form: `4.7k` or `4k7`".to_string()],
                ),
                QuantityErrorKind::MissingExponentDigits => (
                    "missing digits after the exponent".to_string(),
                    "expected digits after `e`".to_string(),
                    vec!["help: for example `1e3` or `1e-3`".to_string()],
                ),
                QuantityErrorKind::TooLarge => (
                    "number is too large".to_string(),
                    "beyond the largest representable value".to_string(),
                    vec![],
                ),
            },
            LexErrorKind::UnitSpace { word, joined } => (
                "a unit must touch its number".to_string(),
                format!("`{word}` reads as a separate word"),
                vec![format!("help: remove the space: `{joined}`")],
            ),
            LexErrorKind::LeadingDecimalPoint => (
                "a number can't start with `.`".to_string(),
                "add a leading zero".to_string(),
                vec![format!(
                    "help: write `{}`",
                    self.fix.as_ref().map_or("", |f| &f.replacement)
                )],
            ),
            LexErrorKind::TrailingDecimalPoint => (
                "a number can't end with `.`".to_string(),
                "add a digit after the point".to_string(),
                vec![format!(
                    "help: write `{}`",
                    self.fix.as_ref().map_or("", |f| &f.replacement)
                )],
            ),
            LexErrorKind::PlusMinusTypo => (
                "`+-` means `+ (-…)` in Rust".to_string(),
                "not a tolerance".to_string(),
                vec!["help: for a tolerance write `±` or `+/-`".to_string()],
            ),
            LexErrorKind::DoubleMinus => (
                "`--` means `-(-…)` in Rust".to_string(),
                "two minus signs".to_string(),
                vec!["help: write `x`, or `-(-x)` if that's really meant".to_string()],
            ),
            LexErrorKind::LonePercent => (
                "`%` only means percent, glued to a number".to_string(),
                "not attached to a number".to_string(),
                vec!["help: write `5%`; there is no modulo operator".to_string()],
            ),
            LexErrorKind::UnterminatedBlockComment => (
                "unterminated block comment".to_string(),
                "this comment is never closed".to_string(),
                vec!["help: add `*/` (block comments nest, so each `/*` needs one)".to_string()],
            ),
            LexErrorKind::Lookalike {
                found,
                name,
                replacement,
            } => {
                let help = if replacement.is_empty() {
                    "help: delete it".to_string()
                } else {
                    format!("help: replace it with `{replacement}`")
                };
                (
                    format!("`{found}` ({} {name}) is not ASCII", code_point(*found)),
                    if replacement.is_empty() {
                        "invisible character".to_string()
                    } else {
                        format!("looks like `{replacement}`")
                    },
                    vec![help],
                )
            }
            LexErrorKind::UnexpectedChar(c) => (
                format!(
                    "unexpected character `{}` ({})",
                    c.escape_debug(),
                    code_point(*c)
                ),
                "not part of the language".to_string(),
                vec![],
            ),
            LexErrorKind::NonAsciiIdent { ident, found } => (
                "identifiers must be ASCII".to_string(),
                format!("`{ident}` contains `{found}` ({})", code_point(*found)),
                vec!["help: use ASCII letters, digits and `_`".to_string()],
            ),
        };
        Diagnostic::error()
            .with_code(self.kind.code())
            .with_message(message)
            .with_labels(vec![primary.with_message(label)])
            .with_notes(notes)
    }
}
