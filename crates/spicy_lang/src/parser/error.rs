//! Parser errors (docs/ecad/ast.md#decisions, docs/ecad/grammar.md#syntax-errors). Data
//! first, rendered on demand, like the lexer's.

use super::ast::{BinOp, BodyKind};
use super::expr::Infix;
use spicy_errors::{Diag, DiagKind, Fix, Severity, Text};

/// One problem the parser found.
pub type ParseError = Diag<ParseErrorKind>;

#[derive(Clone, Debug, PartialEq)]
pub enum ParseErrorKind {
    /// `expected` names what would fit (`` `;` ``, "an expression"); `found` describes
    /// what's there.
    Expected {
        expected: &'static str,
        found: String,
    },
    /// #1: a statement that isn't closed by `;`.
    MissingSemi,
    /// `block A { a: Pin b: Pin }`: an entry of a comma list in braces with no `,` before
    /// the next one.
    MissingComma,
    /// `circuit Child net a; }`: an item's contents without the `{` before them.
    MissingBrace,
    /// #21: `(`, `[` or `{` never closed; the opener is the error's `related` span.
    Unclosed { closer: &'static str },
    /// Top-level text that isn't an item.
    ExpectedItem { found: String },
    /// `pub circuit A { … }`: only a block or a spec can be `pub`.
    PubNotAllowed,
    /// #19: `fn`, `for`, … are reserved for later.
    Reserved { word: String },
    /// #18: `let net = …`
    KeywordAsName { keyword: String },
    /// #20: `spec` in a `circuit`, `net` in a `contract`. `home` is where it belongs.
    WrongBody {
        stmt: &'static str,
        body: BodyKind,
        home: BodyKind,
    },
    /// #17: `spec dc(out.v) within …;`
    SpecNeedsName,
    /// #16: `Resistor { a = vcc }`
    FieldEquals,
    /// A `spec` whose top level isn't `within`, `<`, `<=`, `>` or `>=`.
    NotARelation,
    /// #4: `capacity / 2h ± 10%`. Both readings, written out:
    /// `(capacity / 2h) ± 10%` and `capacity / (2h ± 10%)`.
    AmbiguousTolerance { whole: String, part: String },
    /// #5, #6: `a < b < c`, `1 ± 2% ± 1%`, `a..=b..=c`. `op` is the second operator.
    Chained { op: Infix },
    /// #7: `1V ± 1% ..= 2V`
    ToleranceInRange,
    /// #7: `x <= 1V..=2V`
    RangeInComparison,
    /// #8: `100..300` or `..300` as a value.
    HalfOpenRange,
    /// #25: a `///` with nothing after it.
    UnattachedDoc,
    /// Expressions or types nested deeper, or an expression tree taller, than the parser
    /// allows (a guard against stack overflow on pathological input).
    TooDeep,
}

/// The variant names, for the test that every error kind has a case file. Only tests
/// use them; [`DiagKind::code`] is the identifier users see.
#[cfg(any(test, fuzzing))]
impl ParseErrorKind {
    /// Every value [`name`](Self::name) can return; `name`'s match is exhaustive, so a
    /// new variant won't compile until it has a name. Add it here too.
    pub const ALL_NAMES: &'static [&'static str] = &[
        "Expected",
        "MissingSemi",
        "MissingComma",
        "MissingBrace",
        "Unclosed",
        "ExpectedItem",
        "PubNotAllowed",
        "Reserved",
        "KeywordAsName",
        "WrongBody",
        "SpecNeedsName",
        "FieldEquals",
        "NotARelation",
        "AmbiguousTolerance",
        "Chained",
        "ToleranceInRange",
        "RangeInComparison",
        "HalfOpenRange",
        "UnattachedDoc",
        "TooDeep",
    ];
}

impl DiagKind for ParseErrorKind {
    fn name(&self) -> &'static str {
        match self {
            ParseErrorKind::Expected { .. } => "Expected",
            ParseErrorKind::MissingSemi => "MissingSemi",
            ParseErrorKind::MissingComma => "MissingComma",
            ParseErrorKind::MissingBrace => "MissingBrace",
            ParseErrorKind::Unclosed { .. } => "Unclosed",
            ParseErrorKind::ExpectedItem { .. } => "ExpectedItem",
            ParseErrorKind::PubNotAllowed => "PubNotAllowed",
            ParseErrorKind::Reserved { .. } => "Reserved",
            ParseErrorKind::KeywordAsName { .. } => "KeywordAsName",
            ParseErrorKind::WrongBody { .. } => "WrongBody",
            ParseErrorKind::SpecNeedsName => "SpecNeedsName",
            ParseErrorKind::FieldEquals => "FieldEquals",
            ParseErrorKind::NotARelation => "NotARelation",
            ParseErrorKind::AmbiguousTolerance { .. } => "AmbiguousTolerance",
            ParseErrorKind::Chained { .. } => "Chained",
            ParseErrorKind::ToleranceInRange => "ToleranceInRange",
            ParseErrorKind::RangeInComparison => "RangeInComparison",
            ParseErrorKind::HalfOpenRange => "HalfOpenRange",
            ParseErrorKind::UnattachedDoc => "UnattachedDoc",
            ParseErrorKind::TooDeep => "TooDeep",
        }
    }

    /// Only an unattached doc comment is a warning: the code still means what it says.
    fn severity(&self) -> Severity {
        match self {
            ParseErrorKind::UnattachedDoc => Severity::Warning,
            _ => Severity::Error,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            ParseErrorKind::Expected { .. }
            | ParseErrorKind::MissingSemi
            | ParseErrorKind::MissingComma
            | ParseErrorKind::MissingBrace
            | ParseErrorKind::Unclosed { .. }
            | ParseErrorKind::ExpectedItem { .. }
            | ParseErrorKind::PubNotAllowed
            | ParseErrorKind::SpecNeedsName
            | ParseErrorKind::FieldEquals
            | ParseErrorKind::TooDeep => "E-syntax",
            ParseErrorKind::Reserved { .. } => "E-reserved",
            ParseErrorKind::KeywordAsName { .. } => "E-keyword",
            ParseErrorKind::WrongBody { .. } => "E-wrong-body",
            ParseErrorKind::NotARelation => "E-relation",
            ParseErrorKind::AmbiguousTolerance { .. } => "E-ambiguous-tolerance",
            ParseErrorKind::Chained { .. } => "E-chained",
            ParseErrorKind::ToleranceInRange | ParseErrorKind::RangeInComparison => "E-spread",
            ParseErrorKind::HalfOpenRange => "E-range",
            ParseErrorKind::UnattachedDoc => "W-doc",
        }
    }

    fn related_label(&self) -> &'static str {
        match self {
            ParseErrorKind::Unclosed { .. } => "…to close this",
            _ => "",
        }
    }

    fn text(&self, _fix: Option<&Fix>) -> Text {
        match self {
            ParseErrorKind::Expected { expected, found } => (
                format!("expected {expected}, found {found}"),
                format!("expected {expected}"),
                vec![],
            ),
            ParseErrorKind::MissingSemi => (
                "expected `;`".to_string(),
                "statements end with `;`".to_string(),
                vec!["help: add `;` at the end of this statement".to_string()],
            ),
            ParseErrorKind::MissingComma => (
                "expected `,`".to_string(),
                "entries are separated by `,`".to_string(),
                vec!["help: add `,` after this entry".to_string()],
            ),
            ParseErrorKind::MissingBrace => (
                "expected `{`".to_string(),
                "`{` goes after the name".to_string(),
                vec!["help: add `{` after the name".to_string()],
            ),
            ParseErrorKind::Unclosed { closer, .. } => (
                format!("expected `{closer}`"),
                format!("expected `{closer}` here"),
                vec![],
            ),
            ParseErrorKind::ExpectedItem { found } => (
                format!("expected an item, found {found}"),
                "not an item".to_string(),
                vec![
                    "note: a file holds `block`, `circuit`, `setup`, `contract`, `env` and \
                     `const` items"
                        .to_string(),
                ],
            ),
            ParseErrorKind::PubNotAllowed => (
                "only a block or a spec can be `pub`".to_string(),
                "remove `pub`".to_string(),
                vec![],
            ),
            ParseErrorKind::Reserved { word } => (
                format!("`{word}` isn't supported yet"),
                "reserved for a later version of the language".to_string(),
                vec![],
            ),
            ParseErrorKind::KeywordAsName { keyword } => (
                format!("`{keyword}` is a keyword"),
                "can't be used as a name".to_string(),
                vec!["help: choose another name".to_string()],
            ),
            ParseErrorKind::WrongBody { stmt, body, home } => (
                format!("`{stmt}` doesn't belong in a `{}`", body.keyword()),
                format!("`{stmt}` belongs in the {}", home.keyword()),
                vec![],
            ),
            ParseErrorKind::SpecNeedsName => (
                "a spec needs a name".to_string(),
                "expected a name before the measure".to_string(),
                vec!["help: write `spec name: measure within bound;`".to_string()],
            ),
            ParseErrorKind::FieldEquals => (
                "fields use `:`, not `=`".to_string(),
                "write `:` here".to_string(),
                vec![],
            ),
            ParseErrorKind::NotARelation => (
                "expected a relation".to_string(),
                "not a relation".to_string(),
                vec!["help: write it like `x within a..=b` or `x <= b`".to_string()],
            ),
            ParseErrorKind::AmbiguousTolerance { whole, part } => (
                "ambiguous tolerance".to_string(),
                "does `±` apply to this whole expression?".to_string(),
                vec![
                    format!("help: `{whole}` for a spread on the result"),
                    format!("help: `{part}` for a spread on one term"),
                ],
            ),
            ParseErrorKind::Chained { op } => {
                let (what, notes) = match op {
                    Infix::Range => ("ranges", vec![]),
                    Infix::Binary(BinOp::Tol) => ("tolerances", vec![]),
                    _ => (
                        "comparisons",
                        vec!["help: for a range, write `b within a..=c`".to_string()],
                    ),
                };
                (
                    format!("{what} can't be chained"),
                    "second operator here".to_string(),
                    notes,
                )
            }
            ParseErrorKind::ToleranceInRange => (
                "a range endpoint can't carry a tolerance".to_string(),
                "this endpoint has a tolerance".to_string(),
                vec!["help: use either a range `a..=b` or a tolerance `x ± t`".to_string()],
            ),
            ParseErrorKind::RangeInComparison => (
                "compare with a single value".to_string(),
                "a range or tolerance here".to_string(),
                vec!["help: to require a value inside a range, use `within`".to_string()],
            ),
            ParseErrorKind::HalfOpenRange => (
                "a range's end is written after `..=`".to_string(),
                "write `..=`".to_string(),
                vec!["note: `a..` with nothing after it is a range with no upper end".to_string()],
            ),
            ParseErrorKind::TooDeep => (
                "expression nested too deeply".to_string(),
                "nesting limit reached here".to_string(),
                vec![],
            ),
            ParseErrorKind::UnattachedDoc => (
                "doc comment isn't attached to anything".to_string(),
                "nothing follows this doc comment".to_string(),
                vec!["help: move it before an item or a statement, or use `//`".to_string()],
            ),
        }
    }
}
