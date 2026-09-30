//! What every stage's problems share (lexer, parser, resolve, flatten): one carrier,
//! [`Diag`], generic over the stage's own kind enum, the suggested [`Fix`], the proof
//! that an error was reported ([`Reported`]), and how a problem is shown ([`Render`]).
//! As rustc has one `Diag` in `rustc_errors`, with its emitters beside it, and Zig one
//! `ErrorBundle`: each stage types its kinds, and the shape around them is written
//! once.

mod render;

pub use render::{Render, render_plain};

use spicy_span::Span;

/// How bad a problem is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    /// The design still means what it says (an unattached doc comment, an isolated net).
    Warning,
}

/// A suggested edit: replace `span` with `replacement`. Offered only when it's the one
/// right answer, so an editor or the AI can apply it without asking.
#[derive(Clone, Debug, PartialEq)]
pub struct Fix {
    pub span: Span,
    pub replacement: String,
}

/// One problem a stage found. Data, not text, so the editor and the AI get the kind and
/// the fix as values (lexer.md L10); [`Render`] shows it to people.
#[derive(Clone, Debug, PartialEq)]
pub struct Diag<K> {
    pub kind: K,
    pub span: Span,
    /// A second place that explains it: the first definition of a duplicate, the
    /// opener of an unclosed bracket.
    pub related: Option<Span>,
    pub fix: Option<Fix>,
}

/// What a stage's kind enum says about each of its problems.
pub trait DiagKind {
    /// The stable identifier shown to the user (`E-unit`).
    fn code(&self) -> &'static str;

    /// The variant's name (`UnknownName`), for snapshots and the test that every kind
    /// has a case file. [`code`](Self::code) is what users see.
    fn name(&self) -> &'static str;

    /// Almost everything is an error; a warning means the code still means what it says.
    fn severity(&self) -> Severity {
        Severity::Error
    }

    /// The words. `fix` is the problem's own fix, which some messages quote.
    fn text(&self, fix: Option<&Fix>) -> Text;

    /// The label on the `related` span, if the kind has one.
    fn related_label(&self) -> &'static str {
        ""
    }
}

/// A diagnostic's message, primary label and notes.
pub type Text = (String, String, Vec<String>);

/// Proof that an error was reported (rustc's `ErrorGuaranteed`). A placeholder for
/// something broken holds one, so "broken, and the user has been told" can't be
/// mistaken for "not there": `None` means absent, `Err(Reported)` means broken. Made
/// only by reporting an error ([`Diag::report`]), or by finding one a stage already
/// reported ([`Reported::among`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Reported(());

impl Reported {
    /// The proof, if one of `diags`, what a stage reported, is an error: for a later
    /// stage that meets an earlier one's error instead of reporting its own (rustc's
    /// `DiagCtxt::has_errors`).
    pub fn among<K: DiagKind>(diags: &[Diag<K>]) -> Option<Reported> {
        diags.iter().any(Diag::is_error).then_some(Reported(()))
    }
}

impl<K: DiagKind> Diag<K> {
    pub fn new(kind: K, span: Span) -> Self {
        Self {
            kind,
            span,
            related: None,
            fix: None,
        }
    }

    pub fn with_fix(mut self, span: Span, replacement: impl Into<String>) -> Self {
        self.fix = Some(Fix {
            span,
            replacement: replacement.into(),
        });
        self
    }

    pub fn with_related(mut self, span: Span) -> Self {
        self.related = Some(span);
        self
    }

    pub fn is_error(&self) -> bool {
        self.kind.severity() == Severity::Error
    }

    /// Adds this error to `sink`, the stage's problems, and returns the proof. A
    /// warning is pushed as it is: it proves nothing is broken.
    pub fn report(self, sink: &mut Vec<Diag<K>>) -> Reported {
        debug_assert!(self.is_error(), "only an error is reported with a proof");
        sink.push(self);
        Reported(())
    }
}

/// `a`, `a and b`, `a, b and c`, each in backticks: a list inside a message.
pub fn list<S: AsRef<str>>(items: &[S]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| format!("`{}`", s.as_ref())).collect();
    match quoted.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}
