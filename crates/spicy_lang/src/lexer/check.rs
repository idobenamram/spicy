//! Pass 2: reads the tokens and reports every problem (lexer.md §6).

use spicy_span::Span;

use super::error::{LexError, LexErrorKind};
use super::lookalike;
use super::quantity::{QuantityErrorKind, decode_quantity, suffix_len, suffix_suggestions};
use super::token::{TokenIdx, TokenKind, Tokens};

/// Pass 2: every lexer problem in `tokens`, in source order.
///
/// One pass over the tokens, each looked at once (as rustc cooks each raw token in
/// `next_token_from_cursor`, and rust-analyzer in `LexedStr::new`). Each check below
/// looks at one token kind and returns at most one error; this loop decides what's
/// kept. Each error claims its span, and an error starting inside a claimed span is
/// dropped (lexer.md §6.2): that's how `10 kΩ` reports only "remove the space", not
/// also "identifiers are ASCII" for `kΩ`. Every check reports at or after its own
/// token, so claims only move forward and one position is enough to track them.
pub fn check(tokens: &Tokens) -> Vec<LexError> {
    let mut out = Errors::default();
    for i in 0..tokens.len() {
        match tokens.kind(i) {
            // Most specific first: a spaced-out unit, then the number itself, then a
            // trailing `.`. The first to report claims the span.
            TokenKind::Quantity => {
                out.push(unit_space(tokens, i));
                out.push(decode_error(tokens, i));
                out.push(trailing_point(tokens, i));
            }
            TokenKind::Dot => out.push(leading_point(tokens, i)),
            TokenKind::Plus => out.push(plus_minus_typo(tokens, i)),
            TokenKind::Minus => out.push(double_minus(tokens, i)),
            TokenKind::Unknown => out.push(Some(unknown_char(tokens, i))),
            TokenKind::UnterminatedBlockComment => out.push(Some(unterminated_comment(tokens, i))),
            TokenKind::IdentNonAscii => out.push(Some(non_ascii_ident(tokens, i))),
            _ => {}
        }
    }
    out.errors
}

/// The errors kept so far, and how far they reach.
#[derive(Default)]
struct Errors {
    errors: Vec<LexError>,
    /// Everything before this offset belongs to an error already reported.
    claimed_until: u32,
}

impl Errors {
    /// Keeps `error` unless it starts inside a span already claimed.
    #[inline]
    fn push(&mut self, error: Option<LexError>) {
        let Some(error) = error else { return };
        if error.span.start < self.claimed_until {
            return;
        }
        self.claimed_until = error.span.end;
        self.errors.push(error);
    }
}

/// An error whose fix replaces its span, as every lexer fix does.
fn fixed(kind: LexErrorKind, span: Span, replacement: impl Into<String>) -> LexError {
    LexError::new(kind, span).with_fix(span, replacement)
}

/// The span from the start of token `first` to the end of token `last`.
fn span_of(tokens: &Tokens, first: TokenIdx, last: TokenIdx) -> Span {
    Span::new(tokens.span(first).start, tokens.span(last).end)
}

/// `10 V`, `10 kΩ`, `10 °C`, `10\u{2009}kΩ` (a thin space): a number, one gap on the
/// same line, then the word the scanner would have glued onto the number without the
/// gap (the same [`suffix_len`]), if the two together decode.
fn unit_space(tokens: &Tokens, i: TokenIdx) -> Option<LexError> {
    let gap = i + 1;
    let is_gap = match tokens.kind_or_eof(gap) {
        TokenKind::Whitespace => !tokens.text(gap).contains('\n'),
        TokenKind::Unknown => tokens.text(gap).chars().all(lookalike::is_space_like),
        _ => false,
    };
    if !is_gap {
        return None;
    }
    let word_start = tokens.span(gap).end as usize;
    let rest = &tokens.src()[word_start..];
    let word = &rest[..suffix_len(rest)];
    // A unit never starts with a digit: `1 2` is two numbers, not `12`.
    if word.is_empty() || word.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let joined = format!("{}{}", tokens.text(i), word);
    decode_quantity(&joined).ok()?;
    let span = Span::new(tokens.span(i).start, (word_start + word.len()) as u32);
    let kind = LexErrorKind::UnitSpace {
        word: word.to_string(),
    };
    Some(fixed(kind, span, joined))
}

/// A `Quantity` that doesn't decode. The error points at the part that's wrong.
fn decode_error(tokens: &Tokens, i: TokenIdx) -> Option<LexError> {
    let e = decode_quantity(tokens.text(i)).err()?;
    let start = tokens.span(i).start;
    let span = Span::new(start + e.span.start, start + e.span.end);
    let fix = quantity_fix(&e.kind);
    let kind = LexErrorKind::Quantity(e.kind);
    Some(match fix {
        Some(fix) => fixed(kind, span, fix),
        None => LexError::new(kind, span),
    })
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
fn trailing_point(tokens: &Tokens, i: TokenIdx) -> Option<LexError> {
    if tokens.kind_or_eof(i + 1) != TokenKind::Dot || is_name(tokens.kind_or_eof(i + 2)) {
        return None;
    }
    let fixed_text = format!("{}.0", tokens.text(i));
    decode_quantity(&fixed_text).ok()?;
    let span = span_of(tokens, i, i + 1);
    Some(fixed(LexErrorKind::TrailingDecimalPoint, span, fixed_text))
}

/// `.5`: a `Dot` touching a following number, when the `.` isn't a field access and the
/// fix `0.5` is a valid number (`0.5.5` and `0.4k7` aren't).
fn leading_point(tokens: &Tokens, i: TokenIdx) -> Option<LexError> {
    let field_access = i > 0 && ends_operand(tokens.kind(i - 1));
    if field_access || tokens.kind_or_eof(i + 1) != TokenKind::Quantity {
        return None;
    }
    let fixed_text = format!("0.{}", tokens.text(i + 1));
    decode_quantity(&fixed_text).ok()?;
    let span = span_of(tokens, i, i + 1);
    Some(fixed(LexErrorKind::LeadingDecimalPoint, span, fixed_text))
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
fn plus_minus_typo(tokens: &Tokens, i: TokenIdx) -> Option<LexError> {
    if tokens.kind_or_eof(i + 1) != TokenKind::Minus {
        return None;
    }
    let span = span_of(tokens, i, i + 1);
    Some(fixed(LexErrorKind::PlusMinusTypo, span, "±"))
}

/// `--` (P1: in Rust it means `-(-…)`). The whole run gets one error, checked from its
/// first `-` only: `----x` is one mistake, not two, and each `-` walking the rest of the
/// run would be quadratic.
fn double_minus(tokens: &Tokens, i: TokenIdx) -> Option<LexError> {
    if i > 0 && tokens.kind(i - 1) == TokenKind::Minus {
        return None;
    }
    let mut last = i;
    while tokens.kind_or_eof(last + 1) == TokenKind::Minus {
        last += 1;
    }
    (last > i).then(|| LexError::new(LexErrorKind::DoubleMinus, span_of(tokens, i, last)))
}

/// A character the language doesn't use: a lone `%`, a look-alike, or anything else.
fn unknown_char(tokens: &Tokens, i: TokenIdx) -> LexError {
    let span = tokens.span(i);
    let c = tokens.text(i).chars().next().expect("tokens are non-empty");
    if c == '%' {
        LexError::new(LexErrorKind::LonePercent, span)
    } else if let Some(l) = lookalike::lookup(c) {
        fixed(LexErrorKind::Lookalike { found: c }, span, l.replacement)
    } else {
        LexError::new(LexErrorKind::UnexpectedChar(c), span)
    }
}

/// Points at the `/*` that is never closed.
fn unterminated_comment(tokens: &Tokens, i: TokenIdx) -> LexError {
    let start = tokens.span(i).start;
    LexError::new(
        LexErrorKind::UnterminatedBlockComment,
        Span::new(start, start + 2),
    )
}

fn non_ascii_ident(tokens: &Tokens, i: TokenIdx) -> LexError {
    let ident = tokens.text(i);
    let found = ident
        .chars()
        .find(|c| !c.is_ascii())
        .expect("IdentNonAscii has a non-ASCII character");
    let kind = LexErrorKind::NonAsciiIdent {
        ident: ident.to_string(),
        found,
    };
    LexError::new(kind, tokens.span(i))
}

#[cfg(test)]
mod tests {
    use super::super::scan;
    use super::check;
    use spicy_errors::DiagKind;

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

    /// A spaced-out unit's fix is exactly one `Quantity` token: `unit_space` takes the
    /// word by the scanner's own rule (`suffix_len`), so the joined text can't end
    /// anywhere else. Checked on fixed cases and on random ones.
    #[test]
    fn unit_space_fix_is_one_quantity() {
        let fixes = |src: &str| -> Vec<String> {
            check(&scan(src))
                .into_iter()
                .filter(|e| e.kind.name() == "UnitSpace")
                .map(|e| e.fix.expect("UnitSpace has a fix").replacement)
                .collect()
        };
        let one_quantity = |fix: &str| {
            let t = scan(fix);
            t.len() == 2 && t.kind(0) == super::TokenKind::Quantity
        };
        for src in ["10 kΩ+x", "10 °C.x", "10\u{2009}kΩ;", "1 µF,", "4 k7Ω]"] {
            let found = fixes(src);
            assert_eq!(found.len(), 1, "{src}");
            assert!(one_quantity(&found[0]), "{src}: {found:?}");
        }
        const PIECES: &[&str] = &[
            "1", "4", "7", " ", "\u{A0}", "k", "M", "u", "µ", "Ω", "°", "º", "C", "V", "Hz", "%",
            "_", ".", "e", "-", "+", "α", "\n", ";",
        ];
        let mut rng = crate::testing::Rng(0x0123_4567_89ab_cdef);
        for _ in 0..20_000 {
            let src: String = (0..rng.below(12))
                .map(|_| PIECES[rng.below(PIECES.len())])
                .collect();
            for fix in fixes(&src) {
                assert!(one_quantity(&fix), "{src:?}: {fix:?}");
            }
        }
    }

    /// A too-large integer ending in `.` gets one error, the number's own.
    #[test]
    fn too_large_integer_ending_in_point_is_one_error() {
        let src = format!("1{}.;", "0".repeat(400));
        assert_eq!(names(&src), ["TooLarge"]);
    }
}
