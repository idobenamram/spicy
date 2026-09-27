//! The parser: tokens → syntax tree (ast.md). Recursive descent that builds nodes
//! directly, with a Pratt loop for expressions (`expr.rs`). It never fails: broken code
//! becomes `Error` nodes that keep their spans, and every problem is collected.

pub mod ast;
mod error;
mod expr;

pub use error::{BodyKind, ParseError, ParseErrorKind};

use codespan_reporting::diagnostic::{Diagnostic, Severity};

use crate::lexer::{LexError, Span, TokenIdx, TokenKind, Tokens, check, lookalike, scan};
use ast::{Attribute, Body, File, Ident, Item, ItemKind, Path, Relation, Stmt, StmtKind, Type};

/// Everything one parse produces (decision A5): the tokens (so the source can be rebuilt
/// exactly), the tree, and every problem the lexer and the parser found.
#[derive(Clone, Debug)]
pub struct Parsed<'src> {
    pub tokens: Tokens<'src>,
    pub file: File<'src>,
    pub lex_errors: Vec<LexError>,
    pub errors: Vec<ParseError>,
}

impl Parsed<'_> {
    /// Whether anything is wrong. Warnings (an unattached doc comment) don't count.
    pub fn has_errors(&self) -> bool {
        !self.lex_errors.is_empty()
            || self
                .errors
                .iter()
                .any(|e| e.kind.severity() == Severity::Error)
    }

    /// Every lexer and parser diagnostic, in source order, ready to render (with
    /// [`crate::diagnostic::render_plain`] or codespan-reporting's terminal output).
    pub fn diagnostics(&self) -> Vec<Diagnostic<()>> {
        let mut all: Vec<(Span, _)> = self
            .lex_errors
            .iter()
            .map(|e| (e.span, e.diagnostic()))
            .chain(self.errors.iter().map(|e| (e.span, e.diagnostic())))
            .collect();
        all.sort_by_key(|(span, _)| *span);
        all.into_iter().map(|(_, d)| d).collect()
    }
}

/// Lexes and parses `src`. Always returns a tree.
pub fn parse(src: &str) -> Parsed<'_> {
    let tokens = scan(src);
    let lex_errors = check(&tokens);
    let mut parser = Parser::new(&tokens);
    let file = parser.file();
    let follow_on = FollowOn::new(&lex_errors, &parser.sig, &tokens);
    let mut errors = parser.errors;
    errors.retain(|e| !follow_on.contains(e));
    // Several constructs left open at the end of the file each say "expected `)`/`}`"
    // at the same place; the innermost (first) one is enough.
    errors.dedup_by(|later, first| {
        matches!(later.kind, ParseErrorKind::Unclosed { .. })
            && matches!(first.kind, ParseErrorKind::Unclosed { .. })
            && later.span == first.span
    });
    Parsed {
        tokens,
        file,
        lex_errors,
        errors,
    }
}

/// Where parse errors are follow-ons of lexer errors, which the lexer's message already
/// explains (decision A7). A parse error is dropped if it starts inside a lexer error
/// (`.5` then "expected a name" at `5`), on the first token right after one (`1.;` then
/// "expected a name" at `;`), or after an unterminated `/*` (everything there is
/// comment). Anything else is kept, even when it overlaps: the missing `;` after `1Meg`
/// is a second, real mistake. Both lists are sorted, so each lookup is a binary search.
struct FollowOn {
    /// Lexer error spans, sorted and non-overlapping (`check` guarantees both).
    inside: Vec<Span>,
    /// Starts of the first significant token after each lexer error, sorted.
    after: Vec<u32>,
    /// Start of an unterminated block comment, if there is one.
    comment_from: Option<u32>,
}

impl FollowOn {
    fn new(lex_errors: &[LexError], sig: &[(u32, TokenKind)], tokens: &Tokens) -> Self {
        let starts: Vec<u32> = sig
            .iter()
            .map(|&(i, _)| tokens.span(i as TokenIdx).start)
            .collect();
        let after = lex_errors
            .iter()
            .filter_map(|e| {
                let k = starts.partition_point(|&s| s < e.span.end);
                starts.get(k).copied()
            })
            .collect();
        let comment_from = lex_errors
            .iter()
            .find(|e| e.kind == crate::lexer::LexErrorKind::UnterminatedBlockComment)
            .map(|e| e.span.start);
        Self {
            inside: lex_errors.iter().map(|e| e.span).collect(),
            after,
            comment_from,
        }
    }

    fn contains(&self, e: &ParseError) -> bool {
        let at = e.span.start;
        let k = self.inside.partition_point(|s| s.start <= at);
        let inside = k > 0 && at < self.inside[k - 1].end;
        inside
            || self.after.binary_search(&at).is_ok()
            || self.comment_from.is_some_and(|from| at >= from)
    }
}

/// A parse function failed; its error is already recorded. Unwinds to the nearest
/// statement or item, which recovers (decision A8).
struct Fatal;

type PResult<T> = Result<T, Fatal>;

/// How deep expressions and types may nest (`((((…`, `A<A<…>>`) before the parser gives
/// up on them, so pathological input can't overflow the stack while parsing.
const MAX_NESTING: u32 = 128;

/// How deep the tree may get, counting chained operators too: `a + a + … + a` is flat
/// text but a left-deep tree, and dropping a tree recurses once per level.
const MAX_TREE_DEPTH: u32 = 1024;

struct Parser<'t, 'src> {
    tokens: &'t Tokens<'src>,
    /// The significant tokens, as the parser sees them (decision A7), ending with `Eof`:
    /// each one's index in `tokens` (as `u32`, which halves the entry) and its kind.
    sig: Vec<(u32, TokenKind)>,
    pos: usize,
    /// Current recursion depth of the parse functions (see `MAX_NESTING`).
    nesting: u32,
    /// Current depth of the tree being built (see `MAX_TREE_DEPTH`).
    tree_depth: u32,
    errors: Vec<ParseError>,
}

/// What the parser reads a token as. Trivia is skipped. A look-alike the lexer already
/// reported reads as the character it looks like (`−` as `-`, like rustc's
/// `check_for_substitution`); other unknown characters are skipped. Non-ASCII
/// identifiers read as identifiers. So one mistake gives one error.
fn parser_kind(tokens: &Tokens, i: TokenIdx) -> Option<TokenKind> {
    let kind = tokens.kind(i);
    match kind {
        _ if kind.is_trivia() => None,
        TokenKind::IdentNonAscii => Some(TokenKind::Ident),
        TokenKind::Unknown => {
            let c = tokens.text(i).chars().next()?;
            lookalike::lookup(c)?.reads_as
        }
        _ => Some(kind),
    }
}

fn is_keyword(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        KwBlock | KwContract | KwPort | KwNet | KwLet | KwAssume | KwSpec | KwIn | KwReserved
    )
}

/// Tokens that can start a statement: where statement recovery stops.
fn starts_statement(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        KwPort | KwNet | KwLet | KwAssume | KwSpec | DocComment | Pound
    )
}

impl<'t, 'src> Parser<'t, 'src> {
    fn new(tokens: &'t Tokens<'src>) -> Self {
        // Roughly half the tokens are trivia; reserving up front avoids regrowing.
        let mut sig = Vec::with_capacity(tokens.len() / 2 + 1);
        for i in 0..tokens.len() {
            if let Some(kind) = parser_kind(tokens, i) {
                sig.push((i as u32, kind));
            }
        }
        Self {
            tokens,
            sig,
            pos: 0,
            nesting: 0,
            tree_depth: 0,
            errors: Vec::new(),
        }
    }

    // --- The cursor --------------------------------------------------------------

    fn peek(&self) -> TokenKind {
        self.sig[self.pos].1
    }

    fn nth(&self, n: usize) -> TokenKind {
        self.sig.get(self.pos + n).map_or(TokenKind::Eof, |t| t.1)
    }

    /// Span of the current token.
    fn span(&self) -> Span {
        self.tokens.span(self.sig[self.pos].0 as TokenIdx)
    }

    /// Text of the current token.
    fn text(&self) -> &'src str {
        self.tokens.text(self.sig[self.pos].0 as TokenIdx)
    }

    fn src(&self) -> &'src str {
        self.tokens.src()
    }

    /// Consumes the current token and returns its span. Never moves past `Eof`.
    fn bump(&mut self) -> Span {
        let span = self.span();
        if self.peek() != TokenKind::Eof {
            self.pos += 1;
        }
        span
    }

    fn eat(&mut self, kind: TokenKind) -> Option<Span> {
        (self.peek() == kind).then(|| self.bump())
    }

    /// End of the last consumed token.
    fn prev_end(&self) -> u32 {
        match self.pos {
            0 => 0,
            p => self.tokens.span(self.sig[p - 1].0 as TokenIdx).end,
        }
    }

    fn start_of(&self, pos: usize) -> u32 {
        self.tokens.span(self.sig[pos].0 as TokenIdx).start
    }

    /// How the current token is described in "found …".
    fn found(&self) -> String {
        match self.peek() {
            TokenKind::Eof => "end of file".to_string(),
            _ => format!("`{}`", self.text()),
        }
    }

    fn error(&mut self, kind: ParseErrorKind, span: Span) {
        self.errors.push(ParseError::new(kind, span));
    }

    /// Records "expected `what`, found …" at the current token.
    fn expected(&mut self, what: &'static str) -> Fatal {
        let found = self.found();
        let span = self.span();
        self.error(
            ParseErrorKind::Expected {
                expected: what,
                found,
            },
            span,
        );
        Fatal
    }

    fn expect(&mut self, kind: TokenKind, what: &'static str) -> PResult<Span> {
        match self.eat(kind) {
            Some(span) => Ok(span),
            None => Err(self.expected(what)),
        }
    }

    /// Runs `f` one nesting level deeper (a nested expression or type). Fails with
    /// `TooDeep` past the limits.
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        if self.nesting >= MAX_NESTING {
            return Err(self.too_deep());
        }
        self.scoped(|p| {
            p.nesting += 1;
            p.deepen()?;
            f(p)
        })
    }

    /// One nesting level deeper without restoring it afterwards: for `-x`, whose levels
    /// the enclosing `nested` releases.
    fn nest_without_scope(&mut self) -> PResult<()> {
        if self.nesting >= MAX_NESTING {
            return Err(self.too_deep());
        }
        self.nesting += 1;
        self.deepen()
    }

    /// Runs `f`, then puts both depth counters back as they were, whether `f` failed or
    /// not. So the counts can't drift, and the `deepen` levels taken inside `f` for
    /// chained operators are released when `f` ends.
    fn scoped<T>(&mut self, f: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        let saved = (self.nesting, self.tree_depth);
        let result = f(self);
        (self.nesting, self.tree_depth) = saved;
        result
    }

    /// One more tree level without recursing, for a chained operator. Released by the
    /// enclosing `scoped`.
    fn deepen(&mut self) -> PResult<()> {
        if self.tree_depth >= MAX_TREE_DEPTH {
            return Err(self.too_deep());
        }
        self.tree_depth += 1;
        Ok(())
    }

    /// "`fn` isn't supported yet" at the current token, which is consumed so recovery
    /// doesn't restart at it.
    fn reserved(&mut self) -> Fatal {
        let word = self.text().to_string();
        let span = self.bump();
        self.error(ParseErrorKind::Reserved { word }, span);
        Fatal
    }

    fn too_deep(&mut self) -> Fatal {
        let span = self.span();
        self.error(ParseErrorKind::TooDeep, span);
        Fatal
    }

    // --- Names, paths, types, attributes -----------------------------------------

    fn name(&mut self) -> PResult<Ident<'src>> {
        match self.peek() {
            TokenKind::Ident => {
                let text = self.text();
                let span = self.bump();
                Ok(Ident { text, span })
            }
            TokenKind::KwReserved => Err(self.reserved()),
            kind if is_keyword(kind) => {
                let keyword = self.text().to_string();
                // Consumed, so recovery doesn't read it as the start of a statement.
                let span = self.bump();
                self.error(ParseErrorKind::KeywordAsName { keyword }, span);
                Err(Fatal)
            }
            _ => Err(self.expected("a name")),
        }
    }

    fn path(&mut self) -> PResult<Path<'src>> {
        let mut segments = vec![self.name()?];
        while self.eat(TokenKind::ColonColon).is_some() {
            segments.push(self.name()?);
        }
        let span = Span::new(segments[0].span.start, self.prev_end());
        Ok(Path { segments, span })
    }

    /// `Power<In>`, `Ground`.
    fn ty(&mut self) -> PResult<Type<'src>> {
        self.nested(|p| {
            let path = p.path()?;
            let args = match p.eat(TokenKind::Lt) {
                // `A<>`: grammar.md §3 requires at least one argument.
                Some(_) if p.peek() == TokenKind::Gt => return Err(p.expected("a type")),
                Some(open) => p.list(open, TokenKind::Gt, Self::ty)?,
                None => Vec::new(),
            };
            let span = Span::new(path.span.start, p.prev_end());
            Ok(Type { path, args, span })
        })
    }

    /// `#[path]` or `#[path(args)]`.
    fn attribute(&mut self) -> PResult<Attribute<'src>> {
        let start = self.bump().start; // `#`
        let open = self.expect(TokenKind::LBracket, "`[`")?;
        let path = self.path()?;
        let args = match self.eat(TokenKind::LParen) {
            Some(open) => Some(self.list(open, TokenKind::RParen, Self::expr)?),
            None => None,
        };
        if self.eat(TokenKind::RBracket).is_none() {
            return Err(self.unclosed_or_expected(open, "]", "`]`"));
        }
        Ok(Attribute {
            path,
            args,
            span: Span::new(start, self.prev_end()),
        })
    }

    fn docs_and_attrs(&mut self) -> PResult<(Vec<Span>, Vec<Attribute<'src>>)> {
        let mut docs = Vec::new();
        let mut attrs = Vec::new();
        loop {
            match self.peek() {
                TokenKind::DocComment => docs.push(self.bump()),
                TokenKind::Pound => attrs.push(self.attribute()?),
                _ => return Ok((docs, attrs)),
            }
        }
    }

    /// Doc comments or attributes with nothing after them.
    fn unattached(
        &mut self,
        docs: &[Span],
        attrs: &[Attribute],
        what: &'static str,
    ) -> PResult<()> {
        if let Some(&last) = docs.last() {
            self.error(ParseErrorKind::UnattachedDoc, last);
        }
        if !attrs.is_empty() {
            return Err(self.expected(what));
        }
        Ok(())
    }

    /// A comma-separated list after `open`, up to `close` (trailing comma allowed).
    fn list<T>(
        &mut self,
        open: Span,
        close: TokenKind,
        mut item: impl FnMut(&mut Self) -> PResult<T>,
    ) -> PResult<Vec<T>> {
        let (closer, separator) = match close {
            TokenKind::RParen => (")", "`,` or `)`"),
            TokenKind::RBracket => ("]", "`,` or `]`"),
            TokenKind::RBrace => ("}", "`,` or `}`"),
            // `A<B, C>`, the only other list.
            _ => (">", "`,` or `>`"),
        };
        let mut out = Vec::new();
        loop {
            if self.eat(close).is_some() {
                return Ok(out);
            }
            if self.peek() == TokenKind::Eof {
                return Err(self.unclosed_or_expected(open, closer, separator));
            }
            out.push(item(self)?);
            if self.eat(TokenKind::Comma).is_none() {
                if self.eat(close).is_some() {
                    return Ok(out);
                }
                return Err(self.unclosed_or_expected(open, closer, separator));
            }
        }
    }

    /// At the end of the file: "expected `)`", pointing at the opener. Otherwise
    /// "expected `,` or `)`, found …".
    fn unclosed_or_expected(
        &mut self,
        opener: Span,
        closer: &'static str,
        expected: &'static str,
    ) -> Fatal {
        if self.peek() == TokenKind::Eof {
            let span = self.span();
            self.error(ParseErrorKind::Unclosed { opener, closer }, span);
            Fatal
        } else {
            self.expected(expected)
        }
    }

    // --- Items -------------------------------------------------------------------

    fn file(&mut self) -> File<'src> {
        let mut items = Vec::new();
        while self.peek() != TokenKind::Eof {
            let start = self.pos;
            match self.item() {
                Ok(Some(item)) => items.push(item),
                Ok(None) => break,
                Err(Fatal) => {
                    let span = self.recover(start, Level::Item);
                    items.push(Item {
                        docs: Vec::new(),
                        attrs: Vec::new(),
                        kind: ItemKind::Error,
                        span,
                    });
                }
            }
        }
        File {
            items,
            span: Span::new(0, self.src().len() as u32),
        }
    }

    fn item(&mut self) -> PResult<Option<Item<'src>>> {
        let start = self.span().start;
        let (docs, attrs) = self.docs_and_attrs()?;
        let kind = match self.peek() {
            TokenKind::KwBlock => {
                self.bump();
                ItemKind::Block(self.body(BodyKind::Block)?)
            }
            TokenKind::KwContract => {
                self.bump();
                ItemKind::Contract(self.body(BodyKind::Contract)?)
            }
            TokenKind::Eof => {
                self.unattached(&docs, &attrs, "an item after the attribute")?;
                return Ok(None);
            }
            TokenKind::KwReserved => {
                let word = self.text().to_string();
                let span = self.span();
                self.error(ParseErrorKind::Reserved { word }, span);
                return Err(Fatal);
            }
            _ => {
                let found = self.found();
                let span = self.span();
                self.error(ParseErrorKind::ExpectedItem { found }, span);
                return Err(Fatal);
            }
        };
        Ok(Some(Item {
            docs,
            attrs,
            kind,
            span: Span::new(start, self.prev_end()),
        }))
    }

    /// `Name { statements }`. A missing `}` at the end of the file, or before the next
    /// item, is reported and the body ends there.
    fn body(&mut self, which: BodyKind) -> PResult<Body<'src>> {
        let name = self.name()?;
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let stmts = self.stmts(which);
        if self.eat(TokenKind::RBrace).is_none() {
            let span = self.span();
            self.error(
                ParseErrorKind::Unclosed {
                    opener: open,
                    closer: "}",
                },
                span,
            );
        }
        Ok(Body { name, stmts })
    }

    // --- Statements --------------------------------------------------------------

    fn stmts(&mut self, body: BodyKind) -> Vec<Stmt<'src>> {
        let mut stmts = Vec::new();
        loop {
            use TokenKind::*;
            if matches!(self.peek(), RBrace | Eof | KwBlock | KwContract) {
                return stmts;
            }
            let start = self.pos;
            match self.stmt(body) {
                Ok(Some(stmt)) => stmts.push(stmt),
                Ok(None) => return stmts,
                Err(Fatal) => {
                    let span = self.recover(start, Level::Stmt);
                    stmts.push(Stmt {
                        docs: Vec::new(),
                        attrs: Vec::new(),
                        kind: StmtKind::Error,
                        span,
                    });
                }
            }
        }
    }

    fn stmt(&mut self, body: BodyKind) -> PResult<Option<Stmt<'src>>> {
        use TokenKind::*;
        let start = self.span().start;
        let (docs, attrs) = self.docs_and_attrs()?;
        let keyword = self.peek();
        let keyword_span = self.span();
        let in_body = |p: &mut Self, stmt: &'static str, allowed: BodyKind| {
            if body != allowed {
                p.error(ParseErrorKind::WrongBody { stmt, body }, keyword_span);
            }
        };
        let kind = match keyword {
            RBrace | Eof | KwBlock | KwContract => {
                self.unattached(&docs, &attrs, "a statement after the attribute")?;
                return Ok(None);
            }
            KwPort => {
                in_body(self, "port", BodyKind::Block);
                self.bump();
                let name = self.name()?;
                self.expect(Colon, "`:`")?;
                let ty = self.ty()?;
                StmtKind::Port { name, ty }
            }
            KwNet => {
                in_body(self, "net", BodyKind::Block);
                self.bump();
                let name = self.name()?;
                let merge = match self.eat(Eq) {
                    Some(_) => Some(self.expr()?),
                    None => None,
                };
                StmtKind::Net { name, merge }
            }
            KwLet => {
                self.bump();
                let name = self.name()?;
                self.expect(Eq, "`=`")?;
                let value = self.expr()?;
                StmtKind::Let { name, value }
            }
            KwAssume => {
                in_body(self, "assume", BodyKind::Contract);
                self.bump();
                let expr = self.expr()?;
                match self.relation(expr) {
                    Some(relation) => StmtKind::Assume { relation },
                    None => StmtKind::Error,
                }
            }
            KwSpec => {
                in_body(self, "spec", BodyKind::Contract);
                self.bump();
                // `spec name: …`. A keyword before the `:` is a bad name, not a missing
                // one (`name` reports it), and `spec g x in y` is a missing `:`. Only a
                // measure right after `spec` (`spec dc(out.v) in …`) lacks the name.
                let named = self.nth(1) == Colon || (self.peek() == Ident && self.nth(1) == Ident);
                if !named {
                    let span = self.span();
                    self.error(ParseErrorKind::SpecNeedsName, span);
                    return Err(Fatal);
                }
                let name = self.name()?;
                self.expect(Colon, "`:`")?;
                let expr = self.expr()?;
                match self.relation(expr) {
                    Some(relation) => StmtKind::Spec { name, relation },
                    None => StmtKind::Error,
                }
            }
            KwReserved => {
                let word = self.text().to_string();
                self.error(ParseErrorKind::Reserved { word }, keyword_span);
                return Err(Fatal);
            }
            _ => {
                return Err(self.expected("a statement (`port`, `net`, `let`, `assume` or `spec`)"));
            }
        };
        self.semi()?;
        Ok(Some(Stmt {
            docs,
            attrs,
            kind,
            span: Span::new(start, self.prev_end()),
        }))
    }

    /// Splits the top level of an `assume` or `spec` into a relation, or reports that it
    /// isn't one.
    fn relation(&mut self, expr: ast::Expr<'src>) -> Option<Relation<'src>> {
        if let ast::ExprKind::Binary { op, lhs, rhs } = expr.kind
            && let Some(op) = op.relation()
        {
            return Some(Relation {
                lhs: *lhs,
                op,
                rhs: *rhs,
            });
        }
        self.error(ParseErrorKind::NotARelation, expr.span);
        None
    }

    /// The `;` that ends a statement. If the next token starts something new, a missing
    /// `;` is reported with an insertion fix and parsing goes on (a soft error).
    fn semi(&mut self) -> PResult<()> {
        use TokenKind::*;
        if self.eat(Semi).is_some() {
            return Ok(());
        }
        let next = self.peek();
        if starts_statement(next) || matches!(next, RBrace | Eof | KwBlock | KwContract) {
            let end = self.prev_end();
            let last = self.tokens.span(self.sig[self.pos - 1].0 as TokenIdx);
            self.errors.push(
                ParseError::new(ParseErrorKind::MissingSemi, last)
                    .with_fix(Span::new(end, end), ";"),
            );
            return Ok(());
        }
        Err(self.expected("`;`"))
    }

    /// Skips the rest of a broken statement or item that started at `start` and returns
    /// the span of everything it covered (decision A8). It scans again from `start`, so
    /// brackets opened before the error are known.
    ///
    /// Brackets opened *before* the error point are assumed broken: a statement or item
    /// keyword at or after the error ends the statement even inside them, so an unclosed
    /// `Resistor {` doesn't swallow the rest of the block. Brackets opened *after* the
    /// error are skipped whole (`for i in 0..N { … }`). A `;` ends the statement when no
    /// bracket is open, and a `}` that closes the enclosing body ends it too. At least
    /// one token is always consumed (unless at the end), so callers' loops progress.
    fn recover(&mut self, start: usize, level: Level) -> Span {
        use TokenKind::*;
        let failed_at = self.pos;
        self.pos = start;
        debug_assert_eq!(
            (self.nesting, self.tree_depth),
            (0, 0),
            "`scoped` restores the depth counters, even on errors"
        );
        // Open brackets, split by whether they were opened before the error.
        let (mut braces_before, mut braces_after, mut others) = (0u32, 0u32, 0u32);
        loop {
            let kind = self.peek();
            if kind == Eof {
                break;
            }
            let past_error = self.pos > start && self.pos >= failed_at;
            let is_item = matches!(kind, KwBlock | KwContract);
            match level {
                Level::Stmt => {
                    let no_braces = braces_before == 0 && braces_after == 0;
                    if kind == RBrace && no_braces {
                        break;
                    }
                    if kind == Semi && no_braces && others == 0 {
                        self.bump();
                        break;
                    }
                    let keyword = matches!(kind, KwPort | KwNet | KwLet | KwAssume | KwSpec);
                    if past_error && braces_after == 0 && (keyword || is_item) {
                        break;
                    }
                    // A doc comment or attribute starts the next statement only at the
                    // top level, and only strictly after the error (`let x = /// doc`).
                    let decoration = matches!(kind, DocComment | Pound);
                    if decoration && no_braces && self.pos > failed_at {
                        break;
                    }
                }
                Level::Item => {
                    if past_error && is_item && braces_after == 0 {
                        break;
                    }
                }
            }
            let before_error = self.pos < failed_at;
            match kind {
                LBrace if before_error => braces_before += 1,
                LBrace => braces_after += 1,
                RBrace if braces_after > 0 => braces_after -= 1,
                RBrace => braces_before = braces_before.saturating_sub(1),
                LParen | LBracket => others += 1,
                RParen | RBracket => others = others.saturating_sub(1),
                _ => {}
            }
            self.bump();
        }
        let from = self.start_of(start);
        Span::new(from, self.prev_end().max(from))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Item,
    Stmt,
}

/// The case-file suite and property tests (ast.md §5).
#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::testing::{
        Rng, assert_every_kind_has_a_case, check_parse_invariants, dump_parse, file_name, read,
    };

    /// `ok/` cases: no errors, and a snapshot of the tree.
    #[test]
    fn ok_cases() {
        insta::glob!("../../test_data/parser", "ok/*.spl", |path| {
            let src = read(path);
            check_parse_invariants(&src);
            let parsed = parse(&src);
            assert!(
                parsed.lex_errors.is_empty() && parsed.errors.is_empty(),
                "{}: unexpected errors {:#?}",
                path.display(),
                parsed.errors
            );
            insta::assert_snapshot!(dump_parse(&file_name(path), &src));
        });
    }

    /// `err/` cases: at least one problem, and a snapshot of tree, errors and rendering.
    #[test]
    fn err_cases() {
        insta::glob!("../../test_data/parser", "err/*.spl", |path| {
            let src = read(path);
            check_parse_invariants(&src);
            let parsed = parse(&src);
            assert!(
                !parsed.errors.is_empty() || !parsed.lex_errors.is_empty(),
                "{}: expected errors",
                path.display()
            );
            insta::assert_snapshot!(dump_parse(&file_name(path), &src));
        });
    }

    /// Every parser error kind appears in at least one `err/` case.
    #[test]
    fn every_error_kind_has_a_case() {
        assert_every_kind_has_a_case("parser/err", ParseErrorKind::ALL_NAMES, |src| {
            parse(src).errors.iter().map(|e| e.kind.name()).collect()
        });
    }

    /// Red team: pathological nesting and long flat chains give `TooDeep`, never a
    /// stack overflow, while parsing, cloning or dropping. Test threads have 2 MB stacks,
    /// like a language server's workers.
    #[test]
    fn deep_and_long_input_does_not_overflow() {
        let cases = [
            format!(
                "block B {{ port p: {}B{}; }}",
                "A<".repeat(20_000),
                ">".repeat(20_000)
            ),
            format!("block B {{ port p: {}B; }}", "A<".repeat(20_000)),
            format!("block B {{ let x = a{}; }}", " + a".repeat(100_000)),
            format!("block B {{ let x = a{}; }}", ".b".repeat(100_000)),
            format!("block B {{ let x = f{}; }}", "()".repeat(100_000)),
            format!("block B {{ let x = {}1; }}", "- ".repeat(20_000)),
            format!(
                "block B {{ let x = {}1{}; }}",
                "[".repeat(20_000),
                "]".repeat(20_000)
            ),
            format!("block B {{ let x = {}; }}", "R { a: ".repeat(20_000)),
        ];
        for src in &cases {
            let parsed = parse(src);
            assert!(
                parsed
                    .errors
                    .iter()
                    .any(|e| e.kind == ParseErrorKind::TooDeep),
                "expected TooDeep for {}…",
                &src[..40]
            );
            let copy = parsed.clone();
            drop(parsed);
            drop(copy);
        }
        // Chains of different kinds add up: a `.b` chain under an operator chain is as
        // deep as their sum (the cleanup review found the `.b` levels were released too
        // early, letting such trees reach twice the limit).
        for src in [
            format!(
                "block B {{ let x = a{}{}; }}",
                ".b".repeat(600),
                " + a".repeat(600)
            ),
            format!(
                "block B {{ let x = -a{}{}; }}",
                ".b".repeat(600),
                " + a".repeat(600)
            ),
        ] {
            let parsed = parse(&src);
            assert!(
                parsed
                    .errors
                    .iter()
                    .any(|e| e.kind == ParseErrorKind::TooDeep),
                "expected TooDeep for {}…",
                &src[..40]
            );
        }
        // Below the limits, long chains are fine.
        let sum = format!("block B {{ let x = a{}; }}", " + a".repeat(500));
        assert!(parse(&sum).errors.is_empty());
    }

    /// Code review and red team: the filter that drops follow-on errors was O(n·m) in
    /// the number of errors (40k errors took 2 s in release). It's linear now; quadratic
    /// would take far longer than this bound in a debug build.
    #[test]
    fn many_errors_parse_in_linear_time() {
        let src = format!("contract C {{ {} }}", "spec a: x; \u{a7} ".repeat(40_000));
        let start = std::time::Instant::now();
        let parsed = parse(&src);
        let elapsed = start.elapsed();
        assert_eq!(parsed.lex_errors.len(), 40_000);
        assert_eq!(parsed.errors.len(), 40_000);
        assert!(elapsed.as_secs_f64() < 5.0, "took {elapsed:?}");
    }

    /// Red team: every suggested fix, applied on its own, must remove the error it was
    /// offered for (an editor or the AI applies fixes without a second look).
    #[test]
    fn fixes_remove_their_error() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data");
        let mut checked = 0;
        for dir in ["lexer/err", "parser/err"] {
            for entry in std::fs::read_dir(root.join(dir)).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_none_or(|e| e != "spl") {
                    continue;
                }
                let src = read(&path);
                let parsed = parse(&src);
                let fixes = parsed
                    .lex_errors
                    .iter()
                    .filter_map(|e| Some((e.kind.name(), e.fix.clone()?)))
                    .chain(
                        parsed
                            .errors
                            .iter()
                            .filter_map(|e| Some((e.kind.name(), e.fix.clone()?))),
                    );
                for (name, fix) in fixes {
                    let mut fixed = src.clone();
                    fixed.replace_range(fix.span.range(), &fix.replacement);
                    let again = parse(&fixed);
                    let still = again
                        .lex_errors
                        .iter()
                        .map(|e| (e.kind.name(), e.span.start))
                        .chain(again.errors.iter().map(|e| (e.kind.name(), e.span.start)))
                        .any(|(n, start)| n == name && start == fix.span.start);
                    assert!(
                        !still,
                        "{}: fix {:?} for {name} didn't remove it",
                        path.display(),
                        fix.replacement
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 20, "only {checked} fixes checked");
    }

    /// Red team: both readings offered for an ambiguous `±` must parse without the
    /// ambiguity (the "one term" reading used to stay ambiguous for `a + b * c ± 1%`).
    #[test]
    fn tolerance_readings_are_unambiguous() {
        for input in [
            "capacity / t ± 10%",
            "a + b * c ± 1%",
            "x ± a * b + c",
            "10% ± a + b",
        ] {
            let parsed = parse_expr_stmt(input);
            let readings: Vec<(String, String)> = parsed
                .errors
                .iter()
                .filter_map(|e| match &e.kind {
                    ParseErrorKind::AmbiguousTolerance { whole, part } => {
                        Some((whole.clone(), part.clone()))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(readings.len(), 1, "{input}");
            for reading in [&readings[0].0, &readings[0].1] {
                let again = parse_expr_stmt(reading);
                assert!(
                    again.errors.is_empty(),
                    "{input} → {reading}: {:?}",
                    again.errors
                );
            }
        }
    }

    fn parse_expr_stmt(expr: &str) -> Parsed<'static> {
        let src: &'static str =
            Box::leak(format!("contract A {{ let x = {expr}; }}").into_boxed_str());
        parse(src)
    }

    /// Pieces for random input: mostly the language's own tokens, so the parser gets
    /// deep into its rules, plus some noise.
    const PIECES: &[&str] = &[
        "block", "contract", "port", "net", "let", "assume", "spec", "in", "fn", " A", " r1",
        " vcc", "Resistor", "{", "}", "(", ")", "[", "]", "<", ">", "<=", ">=", ",", ";", ":",
        "::", ".", "..", "..=", "=", "+", "-", "*", "/", "±", "#", "?", " 47k", " 1%", " 3",
        " 4k7", " 1e", "///doc\n", "//c\n", " ", "\n", "−", ";", "%", "\"", "α",
    ];

    #[test]
    fn invariants_hold_on_random_input() {
        let mut rng = Rng(0x00dd_b1a5_e5c0_ffee);
        for _ in 0..5_000 {
            let len = rng.below(64);
            let src: String = (0..len).map(|_| PIECES[rng.below(PIECES.len())]).collect();
            let result = std::panic::catch_unwind(|| check_parse_invariants(&src));
            assert!(result.is_ok(), "invariant failed on input {src:?}");
        }
    }

    #[test]
    fn ce_amp() {
        let src = include_str!("../../../../circuits/ce_amp.spl");
        check_parse_invariants(src);
        let parsed = parse(src);
        assert!(
            parsed.lex_errors.is_empty() && parsed.errors.is_empty(),
            "{:#?}",
            parsed.errors
        );
        let counts: Vec<(&str, usize)> = parsed
            .file
            .items
            .iter()
            .map(|item| match &item.kind {
                ItemKind::Block(b) => ("block", b.stmts.len()),
                ItemKind::Contract(b) => ("contract", b.stmts.len()),
                ItemKind::Error => ("error", 0),
            })
            .collect();
        assert_eq!(counts, [("block", 12), ("contract", 6)]);
        insta::assert_snapshot!(dump_parse("ce_amp.spl", src));
    }

    /// Throughput, for the record. Run with
    /// `cargo test -p spicy_lang --release -- --ignored --nocapture parser_speed`.
    #[test]
    #[ignore]
    fn parser_speed() {
        let one = include_str!("../../../../circuits/ce_amp.spl");
        let src = one.repeat(10_000_000 / one.len());
        let start = std::time::Instant::now();
        let parsed = parse(&src);
        let elapsed = start.elapsed();
        assert!(parsed.errors.is_empty() && parsed.lex_errors.is_empty());
        let mb = src.len() as f64 / 1e6;
        println!(
            "{mb:.1} MB, {} items: lex + check + parse {:.0} MB/s",
            parsed.file.items.len(),
            mb / elapsed.as_secs_f64()
        );
    }
}
