//! Test support shared by the test suite and the fuzz target (lexer.md §7).

use std::fmt::Write;

use crate::diagnostic::render_plain;
use crate::lexer::{LexError, TokenKind, Tokens, check, scan};

/// Checks the lexer invariants of lexer.md §7.3 on `src`; panics with a description of
/// the first one that fails.
pub fn check_invariants(src: &str) {
    let tokens = scan(src);
    let errors = check(&tokens);

    // Tokens tile the input, end with one zero-length Eof, and only Eof is empty.
    let n = tokens.len();
    assert!(n >= 1, "at least the Eof token");
    assert_eq!(tokens.kind(n - 1), TokenKind::Eof, "last token is Eof");
    assert_eq!(tokens.span(0).start, 0, "first token starts at 0");
    for i in 0..n {
        let span = tokens.span(i);
        assert!(
            src.is_char_boundary(span.start as usize),
            "token {i} starts on a char boundary"
        );
        if i + 1 < n {
            assert_ne!(tokens.kind(i), TokenKind::Eof, "Eof only at the end");
            assert!(
                !span.is_empty(),
                "token {i} ({:?}) is empty",
                tokens.kind(i)
            );
            assert_eq!(span.end, tokens.span(i + 1).start, "tokens are contiguous");
        } else {
            assert!(span.is_empty(), "Eof is empty");
            assert_eq!(span.start as usize, src.len(), "Eof is at the end");
        }
    }

    // Round trip: joining the tokens gives back the input.
    let joined: String = (0..n).map(|i| tokens.text(i)).collect();
    assert_eq!(joined, src, "tokens rebuild the source");

    // Errors: inside the input, on char boundaries, non-empty, and not overlapping.
    let in_bounds = |span: crate::lexer::Span, what: &str| {
        assert!(
            span.end as usize <= src.len(),
            "{what} span inside the source"
        );
        assert!(
            src.is_char_boundary(span.start as usize) && src.is_char_boundary(span.end as usize),
            "{what} span on char boundaries"
        );
    };
    for (k, e) in errors.iter().enumerate() {
        in_bounds(e.span, "error");
        assert!(!e.span.is_empty(), "error span is non-empty: {e:?}");
        if let Some(fix) = &e.fix {
            in_bounds(fix.span, "fix");
        }
        if let Some(next) = errors.get(k + 1) {
            assert!(e.span.end <= next.span.start, "errors don't overlap");
        }
        let _ = e.diagnostic();
    }
    let _ = render_plain("input.spl", src, &diagnostics(&errors));

    // Appending a newline changes only the last significant token (and adds trivia).
    let with_newline = format!("{src}\n");
    let longer = scan(&with_newline);
    let last = n - 1; // Eof
    for i in 0..last.saturating_sub(1) {
        assert_eq!(
            (tokens.kind(i), tokens.span(i)),
            (longer.kind(i), longer.span(i)),
            "token {i} unchanged by a trailing newline"
        );
    }
    if last >= 1 {
        let i = last - 1;
        assert_eq!(tokens.kind(i), longer.kind(i), "last token keeps its kind");
        assert_eq!(
            tokens.span(i).start,
            longer.span(i).start,
            "last token keeps its start"
        );
    }
}

pub fn diagnostics(errors: &[LexError]) -> Vec<codespan_reporting::diagnostic::Diagnostic<()>> {
    errors.iter().map(LexError::diagnostic).collect()
}

/// The token dump used by the case-file snapshots: one `Kind "text"` line per token
/// (rust-analyzer's format), then the errors as data, then as the user sees them.
pub fn dump(file_name: &str, src: &str) -> String {
    let tokens = scan(src);
    let errors = check(&tokens);
    dump_tokens(&tokens, &errors, file_name)
}

fn dump_tokens(tokens: &Tokens, errors: &[LexError], file_name: &str) -> String {
    let mut out = String::new();
    for i in 0..tokens.len() {
        let _ = writeln!(out, "{:?} {:?}", tokens.kind(i), tokens.text(i));
    }
    if !errors.is_empty() {
        out.push_str("\n--- errors ---\n");
        for e in errors {
            let text = &tokens.src()[e.span.range()];
            let _ = write!(
                out,
                "{} {} {}..{} {:?}",
                e.kind.code(),
                e.kind.name(),
                e.span.start,
                e.span.end,
                text
            );
            if let Some(fix) = &e.fix {
                let _ = write!(out, " fix {:?}", fix.replacement);
            }
            out.push('\n');
        }
        out.push_str("\n--- rendered ---\n");
        out.push_str(&render_plain(file_name, tokens.src(), &diagnostics(errors)));
    }
    out
}
