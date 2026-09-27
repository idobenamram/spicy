//! Parser errors (ast.md §3, grammar.md §7). Data first, rendered on demand, like the
//! lexer's.

use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};

use super::ast::BinOp;
use crate::lexer::{Fix, Span};

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
    pub fix: Option<Fix>,
}

/// Which kind of body a statement is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Block,
    Contract,
}

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
    /// #21: `(`, `[` or `{` never closed; `opener` is its span.
    Unclosed { opener: Span, closer: &'static str },
    /// Top-level text that isn't `block` or `contract`.
    ExpectedItem { found: String },
    /// #19: `fn`, `for`, … are reserved for later.
    Reserved { word: String },
    /// #18: `let net = …`
    KeywordAsName { keyword: String },
    /// #20: `assume` in a `block`, `port` in a `contract`.
    WrongBody { stmt: &'static str, body: BodyKind },
    /// #17: `spec dc(out.v) in …;`
    SpecNeedsName,
    /// #16: `Resistor { a = vcc }`
    FieldEquals,
    /// An `assume` or `spec` whose top level isn't `in`, `<`, `<=`, `>` or `>=`.
    NotARelation,
    /// #4: `capacity / 2h ± 10%`. Both readings, written out:
    /// `(capacity / 2h) ± 10%` and `capacity / (2h ± 10%)`.
    AmbiguousTolerance { whole: String, part: String },
    /// #5, #6: `a < b < c`, `1 ± 2% ± 1%`, `a..=b..=c`. `op` is the second operator.
    Chained { op: BinOp },
    /// #7: `1V ± 1% ..= 2V`
    ToleranceInRange,
    /// #7: `x <= 1V..=2V`
    RangeInComparison,
    /// #8: `100..300` as a value.
    HalfOpenRange,
    /// #25: a `///` with nothing after it.
    UnattachedDoc,
    /// Expressions nested deeper than the parser allows (a guard against stack
    /// overflow on pathological input).
    TooDeep,
}

impl ParseErrorKind {
    /// The variant's name, for the test that every error kind has a case file.
    pub fn name(&self) -> &'static str {
        match self {
            ParseErrorKind::Expected { .. } => "Expected",
            ParseErrorKind::MissingSemi => "MissingSemi",
            ParseErrorKind::Unclosed { .. } => "Unclosed",
            ParseErrorKind::ExpectedItem { .. } => "ExpectedItem",
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

    /// Every value [`name`](Self::name) can return; `name`'s match is exhaustive, so a
    /// new variant won't compile until it has a name. Add it here too.
    pub const ALL_NAMES: &'static [&'static str] = &[
        "Expected",
        "MissingSemi",
        "Unclosed",
        "ExpectedItem",
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

    /// Only an unattached doc comment is a warning: the code still means what it says.
    pub fn severity(&self) -> Severity {
        match self {
            ParseErrorKind::UnattachedDoc => Severity::Warning,
            _ => Severity::Error,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            ParseErrorKind::Expected { .. }
            | ParseErrorKind::MissingSemi
            | ParseErrorKind::Unclosed { .. }
            | ParseErrorKind::ExpectedItem { .. }
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
}

impl ParseError {
    pub(super) fn new(kind: ParseErrorKind, span: Span) -> Self {
        Self {
            kind,
            span,
            fix: None,
        }
    }

    pub(super) fn with_fix(mut self, span: Span, replacement: impl Into<String>) -> Self {
        self.fix = Some(Fix {
            span,
            replacement: replacement.into(),
        });
        self
    }

    /// The diagnostic the user sees.
    pub fn diagnostic(&self) -> Diagnostic<()> {
        let mut labels = vec![Label::primary((), self.span.range())];
        let (message, label, notes): (String, String, Vec<String>) = match &self.kind {
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
            ParseErrorKind::Unclosed { opener, closer } => {
                labels.push(
                    Label::secondary((), opener.range()).with_message("…to close this".to_string()),
                );
                (
                    format!("expected `{closer}`"),
                    format!("expected `{closer}` here"),
                    vec![],
                )
            }
            ParseErrorKind::ExpectedItem { found } => (
                format!("expected `block` or `contract`, found {found}"),
                "not an item".to_string(),
                vec!["note: a file holds `block` and `contract` items".to_string()],
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
            ParseErrorKind::WrongBody { stmt, body } => {
                let (body, belongs) = match body {
                    BodyKind::Block => ("block", "the contract"),
                    BodyKind::Contract => ("contract", "the block"),
                };
                (
                    format!("`{stmt}` doesn't belong in a `{body}`"),
                    format!("`{stmt}` belongs in {belongs}"),
                    vec![],
                )
            }
            ParseErrorKind::SpecNeedsName => (
                "a spec needs a name".to_string(),
                "expected a name before the measure".to_string(),
                vec!["help: write `spec name: measure in bound;`".to_string()],
            ),
            ParseErrorKind::FieldEquals => (
                "fields use `:`, not `=`".to_string(),
                "write `:` here".to_string(),
                vec![],
            ),
            ParseErrorKind::NotARelation => (
                "expected a relation".to_string(),
                "not a relation".to_string(),
                vec!["help: write it like `x in a..=b` or `x <= b`".to_string()],
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
                    BinOp::Range => ("ranges", vec![]),
                    BinOp::Tol => ("tolerances", vec![]),
                    _ => (
                        "comparisons",
                        vec!["help: for a range, write `b in a..=c`".to_string()],
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
                vec!["help: to require a value inside a range, use `in`".to_string()],
            ),
            ParseErrorKind::HalfOpenRange => (
                "a value range is closed".to_string(),
                "write `..=`".to_string(),
                vec!["note: `..` is reserved for integer ranges in loops".to_string()],
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
        };
        labels[0] = labels[0].clone().with_message(label);
        Diagnostic::new(self.kind.severity())
            .with_code(self.kind.code())
            .with_message(message)
            .with_labels(labels)
            .with_notes(notes)
    }
}
