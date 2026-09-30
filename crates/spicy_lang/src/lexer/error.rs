//! Lexer errors (lexer.md §6.1): what `check` reports, as data, and the words each one
//! renders to (as `parser/error.rs` and `resolve/error.rs` for their stages).

use super::lookalike;
use super::quantity::{QuantityErrorKind, SUFFIX_NOTE, suffix_suggestions};
use spicy_errors::{Diag, DiagKind, Fix, Text};

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

fn code_point(c: char) -> String {
    format!("U+{:04X}", c as u32)
}

impl DiagKind for LexErrorKind {
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
            notes.push(SUFFIX_NOTE.to_string());
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
