//! Rendering diagnostics, shared by the lexer and (later) the parser.

use codespan_reporting::diagnostic::Diagnostic;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Chars, Config};

/// Renders diagnostics as plain text (no color), the way snapshot tests and a
/// non-terminal output see them.
pub fn render_plain(file_name: &str, src: &str, diagnostics: &[Diagnostic<()>]) -> String {
    let file = SimpleFile::new(file_name, src);
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
