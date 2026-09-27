use std::ops::Range;

/// A byte range in one source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end);
        Self { start, end }
    }

    pub fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// Index of a token in [`Tokens`].
pub type TokenIdx = usize;

/// What a token is. One byte, so the token list stays compact (lexer.md §3, L3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TokenKind {
    // Trivia: kept for the round trip, skipped by the parser.
    Whitespace,
    LineComment,
    BlockComment,
    UnterminatedBlockComment,

    /// `///…`, attached by the parser to the next item or statement.
    DocComment,

    Ident,
    /// An identifier containing non-ASCII letters (`r1_α`). Rejected by `check`.
    IdentNonAscii,

    KwBlock,
    KwContract,
    KwPort,
    KwNet,
    KwLet,
    KwAssume,
    KwSpec,
    KwIn,
    /// Any word reserved for later (`fn`, `for`, `signal`, …); the text says which.
    KwReserved,

    /// A number with an optional glued suffix: `47k`, `4k7`, `1uF`, `-10°C` (without the `-`).
    Quantity,

    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Lt,
    Gt,
    Le,
    Ge,
    Comma,
    Semi,
    Colon,
    ColonColon,
    Dot,
    DotDot,
    DotDotEq,
    Eq,
    Plus,
    Minus,
    Star,
    Slash,
    /// `±` or `+/-`.
    PlusMinus,
    Pound,
    Question,

    /// One character the language doesn't use (`%` alone, `−`, `§`, a no-break space).
    Unknown,

    /// Zero-length, always last.
    Eof,
}

impl TokenKind {
    /// Whitespace and comments: tokens the parser skips.
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            TokenKind::Whitespace
                | TokenKind::LineComment
                | TokenKind::BlockComment
                | TokenKind::UnterminatedBlockComment
        )
    }
}

/// Output of [`scan`](super::scan): every token of one source, in order.
///
/// Stored as two parallel arrays (kind, start offset), as in Zig and rust-analyzer.
/// Token `i` covers `starts[i]..starts[i + 1]`; `starts` has one extra entry, the
/// source length, so the last token (always [`TokenKind::Eof`]) has an end too.
#[derive(Clone, Debug)]
pub struct Tokens<'src> {
    pub(super) src: &'src str,
    pub(super) kinds: Vec<TokenKind>,
    pub(super) starts: Vec<u32>,
}

impl<'src> Tokens<'src> {
    pub fn src(&self) -> &'src str {
        self.src
    }

    /// Number of tokens, including the final `Eof`.
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    pub fn kind(&self, i: TokenIdx) -> TokenKind {
        self.kinds[i]
    }

    /// Like [`kind`](Self::kind), but `Eof` past the end, for lookahead without bounds checks.
    pub fn kind_or_eof(&self, i: TokenIdx) -> TokenKind {
        self.kinds.get(i).copied().unwrap_or(TokenKind::Eof)
    }

    pub fn kinds(&self) -> &[TokenKind] {
        &self.kinds
    }

    pub fn span(&self, i: TokenIdx) -> Span {
        Span::new(self.starts[i], self.starts[i + 1])
    }

    pub fn text(&self, i: TokenIdx) -> &'src str {
        &self.src[self.span(i).range()]
    }
}
