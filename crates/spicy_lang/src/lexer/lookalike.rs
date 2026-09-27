//! Characters that look like ASCII but aren't (lexer.md §6.3). Mostly what datasheets,
//! PDFs and word processors produce. rustc keeps a table of 264
//! (`rustc_parse/src/lexer/unicode_chars.rs`); we start with the ones we expect.

use super::token::TokenKind;

pub(crate) struct Lookalike {
    pub found: char,
    pub name: &'static str,
    /// What to write instead. Empty means "delete it".
    pub replacement: &'static str,
    /// The token the parser reads it as, so one mistake gives one error. `None` (the
    /// spaces, `˚`) means the parser skips it.
    pub reads_as: Option<TokenKind>,
}

#[rustfmt::skip] // one row per line, so it reads as a table
const TABLE: &[Lookalike] = &[
    entry('\u{2212}', "MINUS SIGN", "-", Some(TokenKind::Minus)),
    entry('\u{2013}', "EN DASH", "-", Some(TokenKind::Minus)),
    entry('\u{2014}', "EM DASH", "-", Some(TokenKind::Minus)),
    entry('\u{2010}', "HYPHEN", "-", Some(TokenKind::Minus)),
    entry('\u{2011}', "NON-BREAKING HYPHEN", "-", Some(TokenKind::Minus)),
    entry('\u{037E}', "GREEK QUESTION MARK", ";", Some(TokenKind::Semi)),
    entry('\u{2264}', "LESS-THAN OR EQUAL TO", "<=", Some(TokenKind::Le)),
    entry('\u{2265}', "GREATER-THAN OR EQUAL TO", ">=", Some(TokenKind::Ge)),
    entry('\u{00D7}', "MULTIPLICATION SIGN", "*", Some(TokenKind::Star)),
    entry('\u{00B7}', "MIDDLE DOT", "*", Some(TokenKind::Star)),
    entry('\u{2215}', "DIVISION SLASH", "/", Some(TokenKind::Slash)),
    entry('\u{00F7}', "DIVISION SIGN", "/", Some(TokenKind::Slash)),
    entry('\u{02DA}', "RING ABOVE", "°", None),
    entry('\u{00A0}', "NO-BREAK SPACE", " ", None),
    entry('\u{2009}', "THIN SPACE", " ", None),
    entry('\u{202F}', "NARROW NO-BREAK SPACE", " ", None),
    entry('\u{200B}', "ZERO WIDTH SPACE", "", None),
    entry('\u{FF1D}', "FULLWIDTH EQUALS SIGN", "=", Some(TokenKind::Eq)),
    entry('\u{FF1A}', "FULLWIDTH COLON", ":", Some(TokenKind::Colon)),
    entry('\u{FF1B}', "FULLWIDTH SEMICOLON", ";", Some(TokenKind::Semi)),
    entry('\u{FF0C}', "FULLWIDTH COMMA", ",", Some(TokenKind::Comma)),
    entry('\u{FF08}', "FULLWIDTH LEFT PARENTHESIS", "(", Some(TokenKind::LParen)),
    entry('\u{FF09}', "FULLWIDTH RIGHT PARENTHESIS", ")", Some(TokenKind::RParen)),
    entry('\u{FF0B}', "FULLWIDTH PLUS SIGN", "+", Some(TokenKind::Plus)),
];

const fn entry(
    found: char,
    name: &'static str,
    replacement: &'static str,
    reads_as: Option<TokenKind>,
) -> Lookalike {
    Lookalike {
        found,
        name,
        replacement,
        reads_as,
    }
}

pub(crate) fn lookup(c: char) -> Option<&'static Lookalike> {
    TABLE.iter().find(|l| l.found == c)
}

/// Spaces that should be treated like a space when checking `10 kΩ` (lexer.md §6.1).
pub(super) fn is_space_like(c: char) -> bool {
    lookup(c).is_some_and(|l| l.replacement == " ")
}
