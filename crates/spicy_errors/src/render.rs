//! Rendering a [`Diag`] as the diagnostic the user sees, with codespan.

use std::borrow::Cow;

use codespan_reporting::diagnostic::{self as codespan, Diagnostic, Label};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Chars, Config};

use crate::{Diag, DiagKind, Severity};

/// Renders a [`Diag`] as the diagnostic the user sees (with [`render_plain`], or
/// codespan's terminal output).
pub trait Render {
    fn diagnostic(&self) -> Diagnostic<()>;
}

impl<K: DiagKind> Render for Diag<K> {
    fn diagnostic(&self) -> Diagnostic<()> {
        let (message, label, notes) = self.kind.text(self.fix.as_ref());
        let mut labels = vec![Label::primary((), self.span.range()).with_message(label)];
        if let Some(related) = self.related {
            let label = self.kind.related_label();
            labels.push(Label::secondary((), related.range()).with_message(label));
        }
        let severity = match self.kind.severity() {
            Severity::Error => codespan::Severity::Error,
            Severity::Warning => codespan::Severity::Warning,
        };
        Diagnostic::new(severity)
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
