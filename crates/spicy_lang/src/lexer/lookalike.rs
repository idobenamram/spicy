//! Characters that look like ASCII but aren't (lexer.md §6.3). Mostly what datasheets,
//! PDFs and word processors produce. rustc keeps a table of 264
//! (`rustc_parse/src/lexer/unicode_chars.rs`); we start with the ones we expect.

pub(crate) struct Lookalike {
    pub found: char,
    pub name: &'static str,
    /// What to write instead. Empty means "delete it".
    pub replacement: &'static str,
}

const TABLE: &[Lookalike] = &[
    entry('\u{2212}', "MINUS SIGN", "-"),
    entry('\u{2013}', "EN DASH", "-"),
    entry('\u{2014}', "EM DASH", "-"),
    entry('\u{2010}', "HYPHEN", "-"),
    entry('\u{2011}', "NON-BREAKING HYPHEN", "-"),
    entry('\u{037E}', "GREEK QUESTION MARK", ";"),
    entry('\u{2264}', "LESS-THAN OR EQUAL TO", "<="),
    entry('\u{2265}', "GREATER-THAN OR EQUAL TO", ">="),
    entry('\u{00D7}', "MULTIPLICATION SIGN", "*"),
    entry('\u{00B7}', "MIDDLE DOT", "*"),
    entry('\u{2215}', "DIVISION SLASH", "/"),
    entry('\u{00F7}', "DIVISION SIGN", "/"),
    entry('\u{02DA}', "RING ABOVE", "°"),
    entry('\u{00A0}', "NO-BREAK SPACE", " "),
    entry('\u{2009}', "THIN SPACE", " "),
    entry('\u{202F}', "NARROW NO-BREAK SPACE", " "),
    entry('\u{200B}', "ZERO WIDTH SPACE", ""),
    entry('\u{FF1D}', "FULLWIDTH EQUALS SIGN", "="),
    entry('\u{FF1A}', "FULLWIDTH COLON", ":"),
    entry('\u{FF1B}', "FULLWIDTH SEMICOLON", ";"),
    entry('\u{FF0C}', "FULLWIDTH COMMA", ","),
    entry('\u{FF08}', "FULLWIDTH LEFT PARENTHESIS", "("),
    entry('\u{FF09}', "FULLWIDTH RIGHT PARENTHESIS", ")"),
    entry('\u{FF0B}', "FULLWIDTH PLUS SIGN", "+"),
];

const fn entry(found: char, name: &'static str, replacement: &'static str) -> Lookalike {
    Lookalike {
        found,
        name,
        replacement,
    }
}

pub(crate) fn lookup(c: char) -> Option<&'static Lookalike> {
    TABLE.iter().find(|l| l.found == c)
}

/// Spaces that should be treated like a space when checking `10 kΩ` (lexer.md §6.1).
pub(crate) fn is_space_like(c: char) -> bool {
    matches!(c, '\u{00A0}' | '\u{2009}' | '\u{202F}')
}
