//! Pass 2: reads the tokens and reports every problem (lexer.md §6).

use spicy_model::span::Span;

use super::lookalike;
use super::quantity::{QuantityErrorKind, decode_quantity, is_suffix_char, suffix_suggestions};
use super::token::{TokenIdx, TokenKind, Tokens};
use crate::diagnostic::{Diag, DiagKind, Fix, Text};

/// One lexer problem (lexer.md L10).
pub type LexError = Diag<LexErrorKind>;

#[derive(Clone, Debug, PartialEq)]
pub enum LexErrorKind {
    /// A `Quantity` that doesn't decode (`47q`, `1Meg`, `k°C`, `4.7k7`, `1e`, `1e400`).
    Quantity(QuantityErrorKind),
    /// `10 kΩ`: `word` is `kΩ`; the fix is the joined `10kΩ`.
    UnitSpace {
        word: String,
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
    /// `−` (U+2212) instead of `-`, and the rest of the look-alike table, which holds
    /// its name and what to write instead (also the fix).
    Lookalike {
        found: char,
    },
    UnexpectedChar(char),
    /// `r1_α`: `ident` is the whole identifier, `found` its first non-ASCII character.
    NonAsciiIdent {
        ident: String,
        found: char,
    },
}

/// The variant names, for the test that every error kind has a case file. Only tests
/// use them; [`DiagKind::code`] is the identifier users see.
#[cfg(any(test, fuzzing))]
impl LexErrorKind {
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
        "TooSmall",
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
            // Most specific first: a spaced-out unit, then the number itself, then a
            // trailing `.`. The first to report claims the span.
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

/// The errors found so far, and how far they reach.
#[derive(Default)]
struct Errors {
    errors: Vec<LexError>,
    /// Everything before this offset belongs to an error already reported.
    claimed_until: u32,
}

impl Errors {
    /// Keeps `error` unless it starts inside a span already claimed.
    fn push(&mut self, error: LexError) {
        if error.span.start < self.claimed_until {
            return;
        }
        self.claimed_until = error.span.end;
        self.errors.push(error);
    }
}

/// The span from the start of token `first` to the end of token `last`.
fn span_of(tokens: &Tokens, first: TokenIdx, last: TokenIdx) -> Span {
    Span::new(tokens.span(first).start, tokens.span(last).end)
}

/// `10 V`, `10 kΩ`, `10 °C`, `10\u{2009}kΩ` (a thin space): a number, one gap on the
/// same line, then the word the scanner would have glued onto the number without the
/// gap, if the two together decode.
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
    let word_start = tokens.span(gap).end as usize;
    let rest = &tokens.src()[word_start..];
    let word_len = rest
        .chars()
        .take_while(|&c| is_suffix_char(c))
        .map(char::len_utf8)
        .sum();
    let word = &rest[..word_len];
    // A unit never starts with a digit: `1 2` is two numbers, not `12`.
    if word.is_empty() || word.starts_with(|c: char| c.is_ascii_digit()) {
        return;
    }
    let joined = format!("{}{}", tokens.text(i), word);
    if decode_quantity(&joined).is_err() {
        return;
    }
    let span = Span::new(tokens.span(i).start, (word_start + word.len()) as u32);
    let kind = LexErrorKind::UnitSpace {
        word: word.to_string(),
    };
    out.push(LexError::new(kind, span).with_fix(span, joined));
}

/// A `Quantity` that doesn't decode. The error points at the part that's wrong.
fn quantity(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    let Err(e) = decode_quantity(tokens.text(i)) else {
        return;
    };
    let start = tokens.span(i).start;
    let span = Span::new(start + e.span.start, start + e.span.end);
    let fix = quantity_fix(&e.kind);
    let mut error = LexError::new(LexErrorKind::Quantity(e.kind), span);
    if let Some(fix) = fix {
        error = error.with_fix(span, fix);
    }
    out.push(error);
}

/// What replaces the wrong part of a quantity, when there's exactly one right answer.
fn quantity_fix(kind: &QuantityErrorKind) -> Option<String> {
    match kind {
        QuantityErrorKind::Meg => Some("M".to_string()),
        // `4k7%` → `4.7%`: the letter stood for the decimal point.
        QuantityErrorKind::PrefixNotAllowed { infix: true, .. } => Some(".".to_string()),
        QuantityErrorKind::PrefixNotAllowed { .. } | QuantityErrorKind::SecondPrefix { .. } => {
            Some(String::new())
        }
        // A fix is applied without a second look (by the editor or the AI), so offer one
        // only when there's no choice to make: `1mhz` could mean `mHz` or `MHz`.
        QuantityErrorKind::UnknownSuffix {
            suffix,
            after_infix,
        } => match suffix_suggestions(suffix, *after_infix).as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        },
        QuantityErrorKind::DecimalAndInfix
        | QuantityErrorKind::MissingExponentDigits
        | QuantityErrorKind::TooLarge
        | QuantityErrorKind::TooSmall => None,
    }
}

/// `1.`: a number touching a `Dot`, when the `.` isn't a field access (`x.0.y`) and the
/// fix `1.0` is a valid number. `1.5.`, `1e3.` and `1k.` have no place for another
/// point, so they're left to the parser.
fn trailing_point(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    if tokens.kind_or_eof(i + 1) != TokenKind::Dot || is_name(tokens.kind_or_eof(i + 2)) {
        return;
    }
    let fixed = format!("{}.0", tokens.text(i));
    if decode_quantity(&fixed).is_ok() {
        let span = span_of(tokens, i, i + 1);
        out.push(LexError::new(LexErrorKind::TrailingDecimalPoint, span).with_fix(span, fixed));
    }
}

/// `.5`: a `Dot` touching a following number, when the `.` isn't a field access and the
/// fix `0.5` is a valid number (`0.5.5` and `0.4k7` aren't).
fn leading_point(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    let field_access = i > 0 && ends_operand(tokens.kind(i - 1));
    if field_access || tokens.kind_or_eof(i + 1) != TokenKind::Quantity {
        return;
    }
    let fixed = format!("0.{}", tokens.text(i + 1));
    if decode_quantity(&fixed).is_ok() {
        let span = span_of(tokens, i, i + 1);
        out.push(LexError::new(LexErrorKind::LeadingDecimalPoint, span).with_fix(span, fixed));
    }
}

fn is_name(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Ident | TokenKind::IdentNonAscii)
}

/// Tokens that can end an operand. A `.` right after one is a field access, not a
/// decimal point (`x.5`, `f().5`), and `0.5` would be no fix.
fn ends_operand(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Ident
            | TokenKind::IdentNonAscii
            | TokenKind::Quantity
            | TokenKind::RParen
            | TokenKind::RBracket
            | TokenKind::RBrace
    )
}

/// `+-` (P1: in Rust it means `+ (-…)`).
fn plus_minus_typo(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    if tokens.kind_or_eof(i + 1) == TokenKind::Minus {
        let span = span_of(tokens, i, i + 1);
        out.push(LexError::new(LexErrorKind::PlusMinusTypo, span).with_fix(span, "±"));
    }
}

/// `--` (P1: in Rust it means `-(-…)`). The whole run gets one error, checked from its
/// first `-` only: `----x` is one mistake, not two, and each `-` walking the rest of the
/// run would be quadratic.
fn double_minus(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    if i > 0 && tokens.kind(i - 1) == TokenKind::Minus {
        return;
    }
    let mut last = i;
    while tokens.kind_or_eof(last + 1) == TokenKind::Minus {
        last += 1;
    }
    if last > i {
        let span = span_of(tokens, i, last);
        out.push(LexError::new(LexErrorKind::DoubleMinus, span));
    }
}

/// A character the language doesn't use: a lone `%`, a look-alike, or anything else.
fn unknown_char(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    let span = tokens.span(i);
    let c = tokens.text(i).chars().next().expect("tokens are non-empty");
    let error = if c == '%' {
        LexError::new(LexErrorKind::LonePercent, span)
    } else if let Some(l) = lookalike::lookup(c) {
        LexError::new(LexErrorKind::Lookalike { found: c }, span).with_fix(span, l.replacement)
    } else {
        LexError::new(LexErrorKind::UnexpectedChar(c), span)
    };
    out.push(error);
}

/// Points at the `/*` that is never closed.
fn unterminated_comment(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    let start = tokens.span(i).start;
    let span = Span::new(start, start + 2);
    out.push(LexError::new(LexErrorKind::UnterminatedBlockComment, span));
}

fn non_ascii_ident(tokens: &Tokens, i: TokenIdx, out: &mut Errors) {
    let ident = tokens.text(i);
    let found = ident
        .chars()
        .find(|c| !c.is_ascii())
        .expect("IdentNonAscii has a non-ASCII character");
    let kind = LexErrorKind::NonAsciiIdent {
        ident: ident.to_string(),
        found,
    };
    out.push(LexError::new(kind, tokens.span(i)));
}

fn code_point(c: char) -> String {
    format!("U+{:04X}", c as u32)
}

impl DiagKind for LexErrorKind {
    #[cfg(any(test, fuzzing))]
    fn name(&self) -> &'static str {
        match self {
            LexErrorKind::Quantity(q) => match q {
                QuantityErrorKind::UnknownSuffix { .. } => "UnknownSuffix",
                QuantityErrorKind::Meg => "Meg",
                QuantityErrorKind::PrefixNotAllowed { .. } => "PrefixNotAllowed",
                QuantityErrorKind::SecondPrefix { .. } => "SecondPrefix",
                QuantityErrorKind::DecimalAndInfix => "DecimalAndInfix",
                QuantityErrorKind::MissingExponentDigits => "MissingExponentDigits",
                QuantityErrorKind::TooLarge => "TooLarge",
                QuantityErrorKind::TooSmall => "TooSmall",
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

    /// Stable code, in `language.md`'s style.
    fn code(&self) -> &'static str {
        match self {
            LexErrorKind::Quantity(q) => match q {
                QuantityErrorKind::MissingExponentDigits
                | QuantityErrorKind::TooLarge
                | QuantityErrorKind::TooSmall => "E-number",
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

    fn text(&self, fix: Option<&Fix>) -> Text {
        let fix = fix.map_or("", |f| f.replacement.as_str());
        match self {
            LexErrorKind::Quantity(q) => quantity_text(q),
            LexErrorKind::UnitSpace { word } => (
                "a unit must touch its number".to_string(),
                format!("`{word}` reads as a separate word"),
                vec![format!("help: remove the space: `{fix}`")],
            ),
            LexErrorKind::LeadingDecimalPoint => (
                "a number can't start with `.`".to_string(),
                "add a leading zero".to_string(),
                vec![format!("help: write `{fix}`")],
            ),
            LexErrorKind::TrailingDecimalPoint => (
                "a number can't end with `.`".to_string(),
                "add a digit after the point".to_string(),
                vec![format!("help: write `{fix}`")],
            ),
            LexErrorKind::PlusMinusTypo => (
                "`+-` means `+ (-…)` in Rust".to_string(),
                "not a tolerance".to_string(),
                vec!["help: for a tolerance write `±` or `+/-`".to_string()],
            ),
            LexErrorKind::DoubleMinus => (
                "`--` means `-(-…)` in Rust".to_string(),
                "repeated minus signs".to_string(),
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
            LexErrorKind::Lookalike { found } => {
                let row = lookalike::lookup(*found).expect("only table characters are Lookalike");
                let (name, replacement) = (row.name, row.replacement);
                let (label, help) = if replacement.is_empty() {
                    (
                        "invisible character".to_string(),
                        "help: delete it".to_string(),
                    )
                } else {
                    (
                        format!("looks like `{replacement}`"),
                        format!("help: replace it with `{replacement}`"),
                    )
                };
                (
                    format!("`{found}` ({} {name}) is not ASCII", code_point(*found)),
                    label,
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
        }
    }
}

/// The message, label and notes for a quantity that doesn't decode.
fn quantity_text(q: &QuantityErrorKind) -> Text {
    match q {
        QuantityErrorKind::UnknownSuffix {
            suffix,
            after_infix,
        } => {
            let suggestions = suffix_suggestions(suffix, *after_infix);
            let mut notes = Vec::new();
            if !suggestions.is_empty() {
                let list: Vec<String> = suggestions.iter().map(|s| format!("`{s}`")).collect();
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
        QuantityErrorKind::PrefixNotAllowed {
            prefix,
            unit,
            infix: false,
        } => (
            format!("`{}` takes no prefix", unit.symbol()),
            format!("remove `{prefix}`"),
            vec![],
        ),
        QuantityErrorKind::PrefixNotAllowed {
            prefix,
            unit,
            infix: true,
        } => (
            format!("`{}` takes no prefix", unit.symbol()),
            format!("`{prefix}` can't be a prefix here"),
            vec![format!(
                "help: in this form `{prefix}` marks the decimal point, but `{}` takes \
                 no prefix: write `.` instead",
                unit.symbol()
            )],
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
        QuantityErrorKind::TooSmall => (
            "number is too small".to_string(),
            "rounds to zero".to_string(),
            vec![],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::super::scan;
    use super::check;
    use crate::diagnostic::DiagKind;

    /// Each error as `(name, text, fix)`.
    fn errors(src: &str) -> Vec<(&'static str, &str, Option<String>)> {
        check(&scan(src))
            .into_iter()
            .map(|e| {
                let fix = e.fix.map(|f| f.replacement);
                (e.kind.name(), &src[e.span.range()], fix)
            })
            .collect()
    }

    fn names(src: &str) -> Vec<&'static str> {
        errors(src).into_iter().map(|(name, ..)| name).collect()
    }

    /// Regression: every `-` of a run used to walk the rest of the run, so a long run was
    /// quadratic (200 000 `-` took 5 s in release).
    #[test]
    fn minus_run_is_linear() {
        let src = "-".repeat(200_000);
        let start = std::time::Instant::now();
        assert_eq!(names(&src), ["DoubleMinus"]);
        assert!(
            start.elapsed().as_secs_f64() < 2.0,
            "took {:?}",
            start.elapsed()
        );
    }

    /// A run is one error even when `+-` claims its first `-`: the rest isn't reported
    /// as a run of its own.
    #[test]
    fn minus_run_after_plus_minus() {
        assert_eq!(names("+---x"), ["PlusMinusTypo"]);
        assert_eq!(names("-- +-"), ["DoubleMinus", "PlusMinusTypo"]);
    }

    /// Regression: `1.5.` got "write `1.5.0`" and `.5.5` "write `0.5.5`", fixes that
    /// aren't numbers. A decimal-point error is offered only when its fix decodes; the
    /// rest is left to the parser (`.4k7` to the number's own check).
    #[test]
    fn decimal_point_only_when_the_fix_is_a_number() {
        for src in ["1.5.;", "1e3.;", "1k.;", "x.0.y", ".5.5", ".4k7"] {
            assert_eq!(names(src), Vec::<&str>::new(), "{src}");
        }
        assert_eq!(names(".47q"), ["UnknownSuffix"]);
        assert_eq!(
            errors("1_000.;"),
            [("TrailingDecimalPoint", "1_000.", Some("1_000.0".into()))]
        );
        assert_eq!(
            errors(".5k"),
            [("LeadingDecimalPoint", ".5k", Some("0.5k".into()))]
        );
    }

    /// The decimal-point fixes leave no lexer error behind.
    #[test]
    fn decimal_point_fixes_are_clean() {
        for (src, fixed) in [("a = 1.;", "a = 1.0;"), ("a = -.5;", "a = -0.5;")] {
            let e = &check(&scan(src))[0];
            let fix = e.fix.as_ref().unwrap();
            let mut applied = src.to_string();
            applied.replace_range(fix.span.range(), &fix.replacement);
            assert_eq!(applied, fixed);
            assert!(check(&scan(&applied)).is_empty(), "{applied}");
        }
    }

    /// A `.` after whitespace isn't a field access, so `x .5` is a leading point.
    #[test]
    fn leading_point_after_a_gap() {
        assert_eq!(
            errors("x .5"),
            [("LeadingDecimalPoint", ".5", Some("0.5".into()))]
        );
    }

    /// What counts as the gap in `10 V`: spaces and space look-alikes on one line.
    #[test]
    fn unit_space_gap() {
        assert_eq!(
            errors("10   V"),
            [("UnitSpace", "10   V", Some("10V".into()))]
        );
        assert_eq!(
            errors("10\u{A0}V"),
            [("UnitSpace", "10\u{A0}V", Some("10V".into()))]
        );
        // Not a gap: a line break, or a zero-width space (which is deleted, not a space).
        assert_eq!(names("10\nV"), Vec::<&str>::new());
        assert_eq!(names("10\u{200B}V"), ["Lookalike"]);
        // Not a unit: a word starting with a digit, or one that doesn't decode after it.
        assert_eq!(names("10 1k"), Vec::<&str>::new());
        assert_eq!(names("10 kΩ2"), ["NonAsciiIdent"]);
    }

    /// The word after the gap is exactly what the scanner would glue on: it stops at
    /// the first character that can't be in a suffix.
    #[test]
    fn unit_space_word_ends_like_a_suffix() {
        assert_eq!(
            errors("10 kΩ+x"),
            [("UnitSpace", "10 kΩ", Some("10kΩ".into()))]
        );
        assert_eq!(names("10 °C.x"), ["UnitSpace"]);
    }

    /// A too-large integer ending in `.` gets one error, the number's own.
    #[test]
    fn too_large_integer_ending_in_point_is_one_error() {
        let src = format!("1{}.;", "0".repeat(400));
        assert_eq!(names(&src), ["TooLarge"]);
    }
}
