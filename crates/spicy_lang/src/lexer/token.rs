use spicy_span::Span;

/// Index of a token in [`Tokens`].
pub type TokenIdx = u32;

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
    KwCircuit,
    KwSetup,
    KwContract,
    KwEnv,
    KwConst,
    KwPub,
    KwNet,
    KwLet,
    KwSpec,
    KwWithin,
    KwFor,
    KwIn,
    /// Any word reserved for later (`fn`, `trait`, `signal`, …); the text says which.
    KwReserved,

    /// A number with an optional glued suffix: `47k`, `4k7`, `1uF`, `-10°C` (without the `-`).
    Quantity,
    /// `"…"` on one line, with `\"` and `\\` as its only escapes.
    Str,
    /// A `"` with no closing `"` before the end of its line. Rejected by `check`.
    UnterminatedStr,

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

    /// Number of tokens, including the final `Eof`. Fits in a `TokenIdx`: the source is
    /// under 4 GiB and every token but `Eof` is at least a byte.
    pub fn len(&self) -> TokenIdx {
        self.kinds.len() as TokenIdx
    }

    /// Always false: there is at least the `Eof`. (Clippy wants it next to `len`.)
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    pub fn kind(&self, i: TokenIdx) -> TokenKind {
        self.kinds[i as usize]
    }

    /// Like [`kind`](Self::kind), but `Eof` past the end, for lookahead without bounds checks.
    pub fn kind_or_eof(&self, i: TokenIdx) -> TokenKind {
        self.kinds
            .get(i as usize)
            .copied()
            .unwrap_or(TokenKind::Eof)
    }

    pub fn span(&self, i: TokenIdx) -> Span {
        let i = i as usize;
        Span::new(self.starts[i], self.starts[i + 1])
    }

    pub fn text(&self, i: TokenIdx) -> &'src str {
        &self.src[self.span(i).range()]
    }
}
