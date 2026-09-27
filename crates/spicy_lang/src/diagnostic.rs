//! What every stage's problems share (lexer, parser, resolve): one carrier, [`Diag`],
//! generic over the stage's own kind enum, the suggested [`Fix`], and rendering. As
//! rustc has one `Diag` and Zig one `ErrorBundle`: each stage types its kinds, and the
//! shape around them is written once.

use std::borrow::Cow;

use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Chars, Config};
use spicy_model::span::Span;

/// A suggested edit: replace `span` with `replacement`. Offered only when it's the one
/// right answer, so an editor or the AI can apply it without asking.
#[derive(Clone, Debug, PartialEq)]
pub struct Fix {
    pub span: Span,
    pub replacement: String,
}

/// One problem a stage found. Data, not text, so the editor and the AI get the kind and
/// the fix as values (lexer.md L10); [`Diag::diagnostic`] renders it for people.
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

    /// The variant's name (`UnknownName`), for the snapshots and the test that every
    /// kind has a case file. Only tests use it; [`code`](Self::code) is what users see.
    #[cfg(any(test, fuzzing))]
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

impl<K: DiagKind> Diag<K> {
    pub(crate) fn new(kind: K, span: Span) -> Self {
        Self {
            kind,
            span,
            related: None,
            fix: None,
        }
    }

    pub(crate) fn with_fix(mut self, span: Span, replacement: impl Into<String>) -> Self {
        self.fix = Some(Fix {
            span,
            replacement: replacement.into(),
        });
        self
    }

    pub(crate) fn with_related(mut self, span: Span) -> Self {
        self.related = Some(span);
        self
    }

    pub fn is_error(&self) -> bool {
        self.kind.severity() == Severity::Error
    }

    /// The diagnostic the user sees (rendered by [`render_plain`]).
    pub fn diagnostic(&self) -> Diagnostic<()> {
        let (message, label, notes) = self.kind.text(self.fix.as_ref());
        let mut labels = vec![Label::primary((), self.span.range()).with_message(label)];
        if let Some(related) = self.related {
            let label = self.kind.related_label();
            labels.push(Label::secondary((), related.range()).with_message(label));
        }
        Diagnostic::new(self.kind.severity())
            .with_code(self.kind.code())
            .with_message(message)
            .with_labels(labels)
            .with_notes(notes)
    }
}

/// Renders diagnostics as plain text (no color), the way snapshot tests and a
/// non-terminal output see them.
pub fn render_plain(file_name: &str, src: &str, diagnostics: &[Diagnostic<()>]) -> String {
    let shown = visible(src);
    let file = SimpleFile::new(file_name, shown.as_ref());
    // ASCII frames (`-->`, `|`), as rustc prints them.
    let config = Config {
        chars: Chars::ascii(),
        ..Config::default()
    };
    let mut out = String::new();
    for diagnostic in diagnostics {
        term::emit_to_string(&mut out, &config, &file, diagnostic)
            .expect("spans come from this source");
    }
    out
}

/// `src` with each invisible character (zero width: U+200B, U+FEFF, …) shown as `�`.
/// The renderer draws a label as wide as the text under it, so an error about an
/// invisible character would otherwise have no caret, under nothing you can see (rustc
/// replaces these the same way). Every one of them is three bytes in UTF-8, like `�`,
/// so every span still points at the same bytes.
fn visible(src: &str) -> Cow<'_, str> {
    const INVISIBLE: &[char] = &[
        '\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}', // zero-width
        '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', // bidi embeddings
        '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}', // bidi isolates
    ];
    if !src.contains(INVISIBLE) {
        return Cow::Borrowed(src);
    }
    Cow::Owned(src.replace(INVISIBLE, "\u{FFFD}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invisible_characters_keep_their_byte_length() {
        let src = "a\u{FEFF}b\u{200B}c\u{2067}d";
        let shown = visible(src);
        assert_eq!(shown, "a\u{FFFD}b\u{FFFD}c\u{FFFD}d");
        assert_eq!(shown.len(), src.len(), "spans index both alike");
        assert!(matches!(visible("plain"), Cow::Borrowed(_)));
    }
}
