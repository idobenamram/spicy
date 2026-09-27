//! The parser: tokens → syntax tree (ast.md). Recursive descent that builds nodes
//! directly, with a Pratt loop for expressions (`expr.rs`). It never fails: broken code
//! becomes `Error` nodes that keep their spans, and every problem is collected.
//!
//! One function per rule of grammar.md §3, in this file's order: `file` → `item` →
//! `body` → `stmts` → `stmt` → `port_stmt` … `spec_stmt`, then `expr` (`expr.rs`:
//! `expr_bp` → `unary` → `postfix` → `primary`).
//!
//! How an error travels (decision A8):
//! - **Fatal:** a rule that can't go on records its error and returns `Err(Fatal)`. `?`
//!   carries it up to the nearest item or statement loop (`file`, `stmts`), which calls
//!   `recover` to skip the broken text and stores an `Error` node in its place.
//! - **Soft:** a problem that doesn't stop the rule (a missing `;`, a statement in the
//!   wrong body, an ambiguous `±`) is recorded, and parsing goes on.
//!
//! Two guards keep pathological input from overflowing the stack (decision A11):
//! `nested` bounds recursion, and `node` (`expr.rs`) bounds an expression tree's height.

pub mod ast;
mod error;
mod expr;

pub use error::{ParseError, ParseErrorKind};

use codespan_reporting::diagnostic::Diagnostic;

use spicy_model::span::Span;

use crate::lexer::{LexError, TokenIdx, TokenKind, Tokens, check, lookalike, scan};
use ast::{
    Attribute, BinOp, Body, BodyKind, File, Ident, Item, ItemKind, Node, Path, Relation, Stmt,
    StmtKind, Type,
};

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
        !self.lex_errors.is_empty() || self.errors.iter().any(ParseError::is_error)
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
    // 1. Lex: the tokens, and the problems in them.
    let tokens = scan(src);
    let lex_errors = check(&tokens);

    // 2. Parse the significant tokens into a tree; broken parts become `Error` nodes.
    let mut parser = Parser::new(&tokens);
    let file = parser.file();

    // 3. One error per mistake: drop the parse errors that only repeat another one.
    let errors = parser.finish(&lex_errors);

    Parsed {
        tokens,
        file,
        lex_errors,
        errors,
    }
}

/// A parse function failed; its error is already recorded. Unwinds to the nearest
/// statement or item, which recovers (decision A8).
struct Fatal;

type PResult<T> = Result<T, Fatal>;

/// How deep expressions and types may nest (`((((…`, `A<A<…>>`, `- - -x`) before the
/// parser gives up on them, so pathological input can't overflow the stack while parsing.
const MAX_NESTING: u32 = 128;

/// How tall an expression tree may get, counting chained operators too: `a + a + … + a`
/// is flat text but a left-deep tree, and dropping a tree recurses once per level.
/// Checked bottom-up as each node is built (`expr.rs`).
const MAX_TREE_DEPTH: u32 = 1024;

struct Parser<'t, 'src> {
    tokens: &'t Tokens<'src>,
    /// The significant tokens, as the parser sees them (decision A7), ending with `Eof`:
    /// each one's index in `tokens` (as `u32`, which halves the entry) and its kind.
    sig: Vec<(TokenIdx, TokenKind)>,
    /// The current token, as an index into `sig`.
    pos: usize,
    /// Current recursion depth of the parse functions (see `MAX_NESTING`).
    nesting: u32,
    errors: Vec<ParseError>,
}

impl<'t, 'src> Parser<'t, 'src> {
    fn new(tokens: &'t Tokens<'src>) -> Self {
        // Roughly half the tokens are trivia; reserving up front avoids regrowing.
        let mut sig = Vec::with_capacity(tokens.len() as usize / 2 + 1);
        for i in 0..tokens.len() {
            if let Some(kind) = parser_kind(tokens, i) {
                sig.push((i, kind));
            }
        }
        Self {
            tokens,
            sig,
            pos: 0,
            nesting: 0,
            errors: Vec::new(),
        }
    }

    /// The parse errors, one per mistake: those a lexer error already explains are
    /// dropped (decision A7), and so are repeats of "expected `)`" at the end of the file.
    fn finish(self, lex_errors: &[LexError]) -> Vec<ParseError> {
        let shadows = self.follow_on_ranges(lex_errors);
        let mut errors = self.errors;
        errors.retain(|e| !is_follow_on(e, &shadows));
        // Several constructs left open at the end of the file each say "expected `)`/`}`"
        // at the same place; the innermost (first) one is enough.
        errors.dedup_by(|later, first| {
            matches!(later.kind, ParseErrorKind::Unclosed { .. })
                && matches!(first.kind, ParseErrorKind::Unclosed { .. })
                && later.span == first.span
        });
        errors
    }

    /// Where parse errors are follow-ons of lexer errors, which the lexer's message
    /// already explains. A lexer error shadows the parse errors that start inside it
    /// (`.5` then "expected a name" at `5`) or on the first significant token after it
    /// (`1.;` then "expected a name" at `;`). After an unterminated `/*` that token is
    /// `Eof`, so everything after the comment is shadowed. Any other parse error is kept,
    /// even when it overlaps: the missing `;` after `1Meg` is a second, real mistake.
    ///
    /// Returns one `(first, last)` range per lexer error, both inclusive: from the error's
    /// start to the start of the first significant token after it. Parse errors only start
    /// at significant tokens, so none starts in the gap between the two.
    fn follow_on_ranges(&self, lex_errors: &[LexError]) -> Vec<(u32, u32)> {
        lex_errors
            .iter()
            .map(|e| {
                // Always found: the last significant token, `Eof`, starts at the end.
                let next = self
                    .sig
                    .partition_point(|&(i, _)| self.tokens.span(i).start < e.span.end);
                (e.span.start, self.span_at(next).start)
            })
            .collect()
    }

    // --- Items -------------------------------------------------------------------

    /// `file = { item } EOF`. A broken item becomes an `ItemKind::Error` over the text
    /// skipped up to the next `block` or `contract`.
    fn file(&mut self) -> File<'src> {
        let mut items = Vec::new();
        while self.peek() != TokenKind::Eof {
            let start = self.pos;
            match self.item() {
                Ok(Some(item)) => items.push(item),
                Ok(None) => break,
                Err(Fatal) => {
                    let span = self.recover(start, Level::Item);
                    items.push(error_node(ItemKind::Error, span));
                }
            }
        }
        File {
            items,
            span: Span::new(0, self.src().len() as u32),
        }
    }

    /// `item = { DOC } { attribute } ( block | contract )`, or `None` when only doc
    /// comments or attributes are left before the end of the file.
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
            TokenKind::KwReserved => return Err(self.reserved()),
            _ => {
                let found = self.found();
                return Err(self.fail(ParseErrorKind::ExpectedItem { found }));
            }
        };
        Ok(Some(Item {
            docs,
            attrs,
            kind,
            span: Span::new(start, self.prev_end()),
        }))
    }

    /// `Name { statements }`, after `block` or `contract`. A missing `}` at the end of
    /// the file, or before the next item, is reported and the body ends there.
    fn body(&mut self, which: BodyKind) -> PResult<Body<'src>> {
        let name = self.name()?;
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let stmts = self.stmts(which);
        if self.eat(TokenKind::RBrace).is_none() {
            self.unclosed(open, "}");
        }
        Ok(Body { name, stmts })
    }

    // --- Statements --------------------------------------------------------------

    /// A body's statements, up to its `}` (or the end of the file, or the next item). A
    /// broken statement becomes a `StmtKind::Error` over the text skipped to recover.
    fn stmts(&mut self, body: BodyKind) -> Vec<Stmt<'src>> {
        let mut stmts = Vec::new();
        while !ends_body(self.peek()) {
            let start = self.pos;
            match self.stmt(body) {
                Ok(Some(stmt)) => stmts.push(stmt),
                Ok(None) => break,
                Err(Fatal) => {
                    let span = self.recover(start, Level::Stmt);
                    stmts.push(error_node(StmtKind::Error, span));
                }
            }
        }
        stmts
    }

    /// `{ DOC } { attribute } ( port | net | let | assume | spec )`, or `None` when only
    /// doc comments or attributes are left before the end of the body.
    fn stmt(&mut self, body: BodyKind) -> PResult<Option<Stmt<'src>>> {
        use TokenKind::*;
        let start = self.span().start;
        let (docs, attrs) = self.docs_and_attrs()?;
        let keyword = self.peek();
        if ends_body(keyword) {
            self.unattached(&docs, &attrs, "a statement after the attribute")?;
            return Ok(None);
        }
        // A statement in the wrong body is reported, and still parsed.
        if let Some((stmt, home)) = home_body(keyword)
            && home != body
        {
            self.error_here(ParseErrorKind::WrongBody { stmt, body });
        }
        let kind = match keyword {
            KwPort => self.port_stmt()?,
            KwNet => self.net_stmt()?,
            KwLet => self.let_stmt()?,
            KwAssume => self.assume_stmt()?,
            KwSpec => self.spec_stmt()?,
            KwReserved => return Err(self.reserved()),
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

    /// `port name: Type`
    fn port_stmt(&mut self) -> PResult<StmtKind<'src>> {
        self.bump(); // `port`
        let name = self.name()?;
        self.expect(TokenKind::Colon, "`:`")?;
        let ty = self.ty()?;
        Ok(StmtKind::Port { name, ty })
    }

    /// `net name` or `net name = merge`
    fn net_stmt(&mut self) -> PResult<StmtKind<'src>> {
        self.bump(); // `net`
        let name = self.name()?;
        let merge = match self.eat(TokenKind::Eq) {
            Some(_) => Some(self.expr()?),
            None => None,
        };
        Ok(StmtKind::Net { name, merge })
    }

    /// `let name = value`
    fn let_stmt(&mut self) -> PResult<StmtKind<'src>> {
        self.bump(); // `let`
        let name = self.name()?;
        self.expect(TokenKind::Eq, "`=`")?;
        let value = self.expr()?;
        Ok(StmtKind::Let { name, value })
    }

    /// `assume relation`
    fn assume_stmt(&mut self) -> PResult<StmtKind<'src>> {
        self.bump(); // `assume`
        Ok(match self.relation()? {
            Some(relation) => StmtKind::Assume { relation },
            None => StmtKind::Error,
        })
    }

    /// `spec name: relation`
    fn spec_stmt(&mut self) -> PResult<StmtKind<'src>> {
        use TokenKind::{Colon, Ident};
        self.bump(); // `spec`
        // A keyword before the `:` is a bad name, not a missing one (`name` reports it),
        // and `spec g x in y` is a missing `:`. Only a measure right after `spec`
        // (`spec dc(out.v) in …`) lacks the name.
        let named = self.nth(1) == Colon || (self.peek() == Ident && self.nth(1) == Ident);
        if !named {
            return Err(self.fail(ParseErrorKind::SpecNeedsName));
        }
        let name = self.name()?;
        self.expect(Colon, "`:`")?;
        Ok(match self.relation()? {
            Some(relation) => StmtKind::Spec { name, relation },
            None => StmtKind::Error,
        })
    }

    /// The expression of an `assume` or `spec`, split at its top-level relation. `None`
    /// when the top isn't one: reported, but a soft error, so the statement becomes
    /// `StmtKind::Error` and its `;` is still read.
    fn relation(&mut self) -> PResult<Option<Relation<'src>>> {
        let expr = self.expr()?;
        if let ast::ExprKind::Binary {
            op: BinOp::Rel(op),
            lhs,
            rhs,
        } = expr.kind
        {
            return Ok(Some(Relation {
                lhs: *lhs,
                op,
                rhs: *rhs,
            }));
        }
        self.error(ParseErrorKind::NotARelation, expr.span);
        Ok(None)
    }

    /// The `;` that ends a statement. If the next token starts something new, a missing
    /// `;` is reported with an insertion fix and parsing goes on (a soft error).
    fn semi(&mut self) -> PResult<()> {
        if self.eat(TokenKind::Semi).is_some() {
            return Ok(());
        }
        let next = self.peek();
        if starts_stmt(next) || ends_body(next) {
            let end = self.prev_end();
            let last = self.span_at(self.pos - 1);
            self.errors.push(
                ParseError::new(ParseErrorKind::MissingSemi, last)
                    .with_fix(Span::new(end, end), ";"),
            );
            return Ok(());
        }
        Err(self.expected("`;`"))
    }

    // --- Recovery ----------------------------------------------------------------

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
            self.nesting, 0,
            "`nested` restores the count, even on errors"
        );
        // Brackets open at the current token: `{` split by whether it was opened before
        // or after the error; `(` and `[` only keep a `;` inside them from ending the
        // statement.
        let (mut braces_before, mut braces_after, mut parens) = (0u32, 0u32, 0u32);
        while self.peek() != Eof {
            let kind = self.peek();
            let before_error = self.pos < failed_at;
            // Never at `start`, so the first token is always consumed.
            let at_or_after_error = !before_error && self.pos > start;
            // The next statement or item starts here.
            if at_or_after_error && braces_after == 0 && level.resumes_at(kind) {
                break;
            }
            // This statement ends here.
            if level == Level::Stmt && braces_before == 0 && braces_after == 0 {
                match kind {
                    // The `}` that closes the body.
                    RBrace => break,
                    Semi if parens == 0 => {
                        self.bump();
                        break;
                    }
                    // Strictly after the error: `let x = /// doc` isn't restarted at the
                    // doc comment it failed on.
                    DocComment | Pound if self.pos > failed_at => break,
                    _ => {}
                }
            }
            match kind {
                LBrace if before_error => braces_before += 1,
                LBrace => braces_after += 1,
                RBrace if braces_after > 0 => braces_after -= 1,
                RBrace => braces_before = braces_before.saturating_sub(1),
                LParen | LBracket => parens += 1,
                RParen | RBracket => parens = parens.saturating_sub(1),
                _ => {}
            }
            self.bump();
        }
        let from = self.span_at(start).start;
        Span::new(from, self.prev_end().max(from))
    }

    // --- Names, paths, types, attributes, lists ----------------------------------

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

    /// The doc comments and attributes before an item or a statement.
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

    /// `#[path]` or `#[path(args)]`.
    fn attribute(&mut self) -> PResult<Attribute<'src>> {
        let start = self.bump().start; // `#`
        let open = self.expect(TokenKind::LBracket, "`[`")?;
        let path = self.path()?;
        let args = match self.eat(TokenKind::LParen) {
            Some(open) => Some(self.list(open, TokenKind::RParen, Self::expr)?),
            None => None,
        };
        self.close(open, TokenKind::RBracket)?;
        Ok(Attribute {
            path,
            args,
            span: Span::new(start, self.prev_end()),
        })
    }

    /// Doc comments or attributes with nothing after them: a warning for the docs, an
    /// error ("expected `what`") for the attributes.
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
        let (closer, _, after_item) = closer_names(close);
        let mut out = Vec::new();
        while self.eat(close).is_none() {
            if self.peek() == TokenKind::Eof {
                return Err(self.unclosed_or_expected(open, closer, after_item));
            }
            out.push(item(self)?);
            // After an item, a `,` or the closer.
            if self.eat(TokenKind::Comma).is_none() && self.peek() != close {
                return Err(self.unclosed_or_expected(open, closer, after_item));
            }
        }
        Ok(out)
    }

    /// The `)`, `]` or `}` that closes `open`.
    fn close(&mut self, open: Span, close: TokenKind) -> PResult<()> {
        if self.eat(close).is_some() {
            return Ok(());
        }
        let (closer, expected, _) = closer_names(close);
        Err(self.unclosed_or_expected(open, closer, expected))
    }

    /// Runs `f` one nesting level deeper (a nested expression or type). Fails with
    /// `TooDeep` past `MAX_NESTING`.
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        if self.nesting >= MAX_NESTING {
            return Err(self.fail(ParseErrorKind::TooDeep));
        }
        self.nesting += 1;
        let result = f(self);
        self.nesting -= 1;
        result
    }

    // --- Errors ------------------------------------------------------------------
    //
    // As Zig's `warn` and `fail`: `error_here` records a soft error at the current
    // token, `fail` records one and returns the `Fatal` that unwinds.

    fn error(&mut self, kind: ParseErrorKind, span: Span) {
        self.errors.push(ParseError::new(kind, span));
    }

    /// Records a soft error at the current token.
    fn error_here(&mut self, kind: ParseErrorKind) {
        let span = self.span();
        self.error(kind, span);
    }

    /// Records an error at the current token, for a rule that can't go on.
    fn fail(&mut self, kind: ParseErrorKind) -> Fatal {
        self.error_here(kind);
        Fatal
    }

    /// "expected `what`, found …" at the current token.
    fn expected(&mut self, what: &'static str) -> Fatal {
        let found = self.found();
        self.fail(ParseErrorKind::Expected {
            expected: what,
            found,
        })
    }

    fn expect(&mut self, kind: TokenKind, what: &'static str) -> PResult<Span> {
        match self.eat(kind) {
            Some(span) => Ok(span),
            None => Err(self.expected(what)),
        }
    }

    /// "`fn` isn't supported yet" at the current token. It isn't consumed: recovery
    /// doesn't stop at a reserved word anyway.
    fn reserved(&mut self) -> Fatal {
        let word = self.text().to_string();
        self.fail(ParseErrorKind::Reserved { word })
    }

    /// "expected `}`" at the current token, pointing back at the `open`er it would close.
    fn unclosed(&mut self, open: Span, closer: &'static str) {
        let span = self.span();
        let error = ParseError::new(ParseErrorKind::Unclosed { closer }, span);
        self.errors.push(error.with_related(open));
    }

    /// At the end of the file: `unclosed`. Otherwise "expected `expected`, found …"
    /// (`` `)` `` or `` `,` or `)` ``).
    fn unclosed_or_expected(
        &mut self,
        open: Span,
        closer: &'static str,
        expected: &'static str,
    ) -> Fatal {
        if self.peek() == TokenKind::Eof {
            self.unclosed(open, closer);
            Fatal
        } else {
            self.expected(expected)
        }
    }

    // --- The cursor --------------------------------------------------------------

    fn peek(&self) -> TokenKind {
        self.sig[self.pos].1
    }

    /// The kind `n` tokens ahead (`nth(0)` is `peek()`); `Eof` past the end.
    fn nth(&self, n: usize) -> TokenKind {
        self.sig.get(self.pos + n).map_or(TokenKind::Eof, |t| t.1)
    }

    /// Span of the significant token at `pos`.
    fn span_at(&self, pos: usize) -> Span {
        self.tokens.span(self.sig[pos].0)
    }

    /// Span of the current token.
    fn span(&self) -> Span {
        self.span_at(self.pos)
    }

    /// Text of the current token.
    fn text(&self) -> &'src str {
        self.tokens.text(self.sig[self.pos].0)
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

    /// End of the last consumed token (0 before the first).
    fn prev_end(&self) -> u32 {
        self.pos.checked_sub(1).map_or(0, |p| self.span_at(p).end)
    }

    /// How the current token is described in "found …".
    fn found(&self) -> String {
        match self.peek() {
            TokenKind::Eof => "end of file".to_string(),
            _ => format!("`{}`", self.text()),
        }
    }
}

/// Whether `e` starts in one of `shadows` (`Parser::follow_on_ranges`). Lexer errors are
/// sorted and don't overlap (`check` guarantees both), so both ends of the ranges are
/// sorted, and the last range starting at or before a position is the only one that can
/// hold it.
fn is_follow_on(e: &ParseError, shadows: &[(u32, u32)]) -> bool {
    let at = e.span.start;
    let k = shadows.partition_point(|&(first, _)| first <= at);
    k > 0 && at <= shadows[k - 1].1
}

/// The node for a broken item or statement, over the text recovery skipped (decision
/// A4; rustc's `mk_stmt_err`).
fn error_node<'src, K>(kind: K, span: Span) -> Node<'src, K> {
    Node {
        docs: Vec::new(),
        attrs: Vec::new(),
        kind,
        span,
    }
}

/// Where `recover` resynchronizes: at the next statement or at the next item.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Item,
    Stmt,
}

impl Level {
    /// Whether `kind` starts what comes next at this level.
    fn resumes_at(self, kind: TokenKind) -> bool {
        match self {
            Level::Item => starts_item(kind),
            Level::Stmt => starts_item(kind) || is_stmt_keyword(kind),
        }
    }
}

/// How a closing bracket is named in errors: as the closer ("expected `)`" at the end of
/// the file), expected on its own, and expected after a list item.
fn closer_names(close: TokenKind) -> (&'static str, &'static str, &'static str) {
    match close {
        TokenKind::RParen => (")", "`)`", "`,` or `)`"),
        TokenKind::RBracket => ("]", "`]`", "`,` or `]`"),
        TokenKind::RBrace => ("}", "`}`", "`,` or `}`"),
        // `A<B, C>`, the only other list.
        _ => (">", "`>`", "`,` or `>`"),
    }
}

// --- Token classes -------------------------------------------------------------------

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

/// Words that can't be names (`KeywordAsName`, `Reserved`).
fn is_keyword(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        KwBlock | KwContract | KwPort | KwNet | KwLet | KwAssume | KwSpec | KwIn | KwReserved
    )
}

fn starts_item(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::KwBlock | TokenKind::KwContract)
}

fn is_stmt_keyword(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(kind, KwPort | KwNet | KwLet | KwAssume | KwSpec)
}

/// Which body a statement belongs in (`let` goes in both, so it has none).
fn home_body(keyword: TokenKind) -> Option<(&'static str, BodyKind)> {
    match keyword {
        TokenKind::KwPort => Some(("port", BodyKind::Block)),
        TokenKind::KwNet => Some(("net", BodyKind::Block)),
        TokenKind::KwAssume => Some(("assume", BodyKind::Contract)),
        TokenKind::KwSpec => Some(("spec", BodyKind::Contract)),
        _ => None,
    }
}

/// Tokens that can start a statement, including its doc comments and attributes.
fn starts_stmt(kind: TokenKind) -> bool {
    is_stmt_keyword(kind) || matches!(kind, TokenKind::DocComment | TokenKind::Pound)
}

/// Tokens that end a body's statements: its `}`, the end of the file, or the next item
/// (a body left unclosed).
fn ends_body(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::RBrace | TokenKind::Eof) || starts_item(kind)
}

/// The case-file suite and property tests (ast.md §5).
#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::diagnostic::DiagKind;
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
            // A chain grows above an operand already parsed: the second review found
            // `(…)` here pushed 600 levels down without an error.
            format!(
                "block B {{ let x = a + (a{}){}; }}",
                " + a".repeat(600),
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
            "a + b ± c * d",
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

    /// A compact view of a parse, for the tests below: each item and statement as
    /// `kind "text"` (nested under its item), and each parse error as `Kind "text"`.
    fn outline(src: &str) -> (Vec<String>, Vec<String>) {
        let parsed = parse(src);
        let text = |span: Span| src[span.range()].to_string();
        let mut nodes = Vec::new();
        for item in &parsed.file.items {
            let (kind, body) = match &item.kind {
                ItemKind::Block(b) => ("block", Some(b)),
                ItemKind::Contract(b) => ("contract", Some(b)),
                ItemKind::Error => ("item-error", None),
            };
            match body {
                Some(b) => nodes.push(format!("{kind} {}", b.name.text)),
                None => nodes.push(format!("{kind} {:?}", text(item.span))),
            }
            for stmt in body.map_or(&[][..], |b| &b.stmts) {
                let kind = match stmt.kind {
                    StmtKind::Port { .. } => "port",
                    StmtKind::Net { .. } => "net",
                    StmtKind::Let { .. } => "let",
                    StmtKind::Assume { .. } => "assume",
                    StmtKind::Spec { .. } => "spec",
                    StmtKind::Error => "error",
                };
                let docs = if stmt.docs.is_empty() { "" } else { " (doc)" };
                nodes.push(format!("  {kind} {:?}{docs}", text(stmt.span)));
            }
        }
        let errors = parsed
            .errors
            .iter()
            .map(|e| match &e.kind {
                ParseErrorKind::Expected { expected, .. } => {
                    format!("Expected {expected} {:?}", text(e.span))
                }
                ParseErrorKind::Unclosed { closer, .. } => format!("Unclosed {closer}"),
                kind => format!("{} {:?}", kind.name(), text(e.span)),
            })
            .collect();
        (nodes, errors)
    }

    fn errors(src: &str) -> Vec<String> {
        outline(src).1
    }

    /// Review: the depth limits are exact, one construct at a time: `n - 1` levels
    /// parse, `n` give `TooDeep`. `-` counts like a nested expression, and the levels
    /// of different chains add up (`-a.b….b + a + … + a` is two levels per `n`).
    #[test]
    fn depth_limits_are_exact() {
        let too_deep = |src: &str| {
            let parsed = parse(src);
            let deep = parsed
                .errors
                .iter()
                .any(|e| e.kind == ParseErrorKind::TooDeep);
            assert!(deep || parsed.errors.is_empty(), "{:?}", parsed.errors);
            deep
        };
        // What the case is, its limit, and the source with `n` levels.
        type Make = fn(usize) -> String;
        let cases: [(&str, usize, Make); 8] = [
            ("parens", 128, |n| {
                format!("block B {{ let x = {}1{}; }}", "(".repeat(n), ")".repeat(n))
            }),
            ("minus", 128, |n| {
                format!("block B {{ let x = {}1; }}", "- ".repeat(n))
            }),
            ("types", 128, |n| {
                format!(
                    "block B {{ port p: {}B{}; }}",
                    "A<".repeat(n),
                    ">".repeat(n)
                )
            }),
            ("attribute args", 128, |n| {
                format!("#[a({}1{})] block B {{ }}", "(".repeat(n), ")".repeat(n))
            }),
            ("sum", 1024, |n| {
                format!("block B {{ let x = a{}; }}", " + a".repeat(n))
            }),
            ("fields", 1024, |n| {
                format!("block B {{ let x = a{}; }}", ".b".repeat(n))
            }),
            ("calls", 1024, |n| {
                format!("block B {{ let x = f{}; }}", "()".repeat(n))
            }),
            ("minus, fields and sum", 512, |n| {
                format!(
                    "block B {{ let x = -a{}{}; }}",
                    ".b".repeat(n),
                    " + a".repeat(n)
                )
            }),
        ];
        for (what, limit, make) in cases {
            assert!(!too_deep(&make(limit - 1)), "{what}: {} levels", limit - 1);
            assert!(too_deep(&make(limit)), "{what}: {limit} levels");
        }
    }

    /// Review: the follow-on filter drops parse errors inside a lexer error or on the
    /// first significant token after it (trivia in between or not), everything after an
    /// unterminated `/*`, and nothing else.
    #[test]
    fn follow_on_filter() {
        let none: [&str; 0] = [];
        // Right after the lexer error, across a space.
        assert_eq!(errors("block A { let x = 1. ; }"), none);
        // Everything after an unterminated comment, even two unclosed constructs.
        assert_eq!(errors("block A { let x = f(1, /* never"), none);
        // An error before the comment is kept.
        assert_eq!(
            errors("block A { let x = 1 /* never"),
            ["MissingSemi \"1\""]
        );
        // A later mistake, past other lexer errors, is kept.
        assert_eq!(
            errors("block A { let a = .5; let b = c − d; let e = f g; }"),
            ["Expected `;` \"g\""]
        );
    }

    /// Review: every list takes a trailing comma or none, may be empty (except type
    /// arguments), names its own closer when a separator is missing, and reports an
    /// unclosed list at the end of the file.
    #[test]
    fn lists() {
        let none: [&str; 0] = [];
        for ok in [
            "block A { let x = f(); let y = f(1, 2); let z = f(1, 2,); }",
            "block A { let x = []; let y = [a, b]; let z = [a, b,]; }",
            "block A { let x = R {}; let y = R { a: b, c }; let z = R { a: b, }; }",
            "#[a()] #[b(1, 2)] #[c(1,)] block A { port p: A<B, C>; port q: A<B,>; }",
        ] {
            assert_eq!(errors(ok), none, "{ok}");
        }
        for (src, expected) in [
            ("block A { let x = f(1 2); }", "Expected `,` or `)` \"2\""),
            ("block A { let x = [1 2]; }", "Expected `,` or `]` \"2\""),
            (
                "block A { let x = R { a: b c }; }",
                "Expected `,` or `}` \"c\"",
            ),
            ("block A { port p: A<B C>; }", "Expected `,` or `>` \"C\""),
            ("block A { port p: A<>; }", "Expected a type \">\""),
            ("block A { let x = f(1,", "Unclosed )"),
            ("block A { let x = [1", "Unclosed ]"),
            ("#[a(1", "Unclosed )"),
            ("#[a", "Unclosed ]"),
        ] {
            assert_eq!(errors(src), [expected], "{src}");
        }
    }

    /// Every closer outside a list (`)` of a parenthesis, `]` of an attribute, `}` of a
    /// body) reports "expected" when something else is there, and `Unclosed`, pointing
    /// at its opener, at the end of the file. Constructs left open at the end give one
    /// error, the innermost's.
    #[test]
    fn closers() {
        for (src, expected) in [
            ("block A { let x = (1 2); }", &["Expected `)` \"2\""][..]),
            ("block A { let x = (1", &["Unclosed )"]),
            ("#[a b] block A { }", &["Expected `]` \"b\""]),
            ("#[a(1) b] block A { }", &["Expected `]` \"b\""]),
            ("block A { net a;", &["Unclosed }"]),
            ("block A { net a; contract A { }", &["Unclosed }"]),
            ("block A { let x = f(1, [2", &["Unclosed ]"]),
            ("block A { let x = R { a: (1", &["Unclosed )"]),
        ] {
            assert_eq!(errors(src), expected, "{src}");
        }
        // The related span is the opener.
        let src = "block A { let x = f(1, [2";
        let parsed = parse(src);
        let related = parsed.errors[0].related.expect("an opener");
        assert_eq!(&src[related.range()], "[");
    }

    /// Review: what statement recovery skips and where it stops (grammar.md §6), one
    /// rule per case.
    #[test]
    fn statement_recovery() {
        let cases: [(&str, &[&str], &[&str]); 6] = [
            // A lone `;` is consumed, so the loop progresses.
            (
                "block A { ; net ok; }",
                &["block A", "  error \";\"", "  net \"net ok;\""],
                &["Expected a statement (`port`, `net`, `let`, `assume` or `spec`) \";\""],
            ),
            // A `{` opened before the error is assumed broken: the next keyword ends
            // the statement inside it, and the body's `}` still closes the body.
            (
                "block A { let r = R { a: 1 2 net ok; }",
                &[
                    "block A",
                    "  error \"let r = R { a: 1 2\"",
                    "  net \"net ok;\"",
                ],
                &["Expected `,` or `}` \"2\""],
            ),
            // A `{` opened after the error is skipped whole, keywords and all.
            (
                "block A { for x { net inner; } net ok; }",
                &[
                    "block A",
                    "  error \"for x { net inner; }\"",
                    "  net \"net ok;\"",
                ],
                &["Reserved \"for\""],
            ),
            // An attribute the statement failed on doesn't restart it.
            (
                "block A { let x = #[a] 1; net ok; }",
                &[
                    "block A",
                    "  error \"let x = #[a] 1;\"",
                    "  net \"net ok;\"",
                ],
                &["Expected an expression \"#\""],
            ),
            // A keyword used as a name is skipped, not read as the next statement.
            (
                "block A { let net = 1; net ok; }",
                &["block A", "  error \"let net = 1;\"", "  net \"net ok;\""],
                &["KeywordAsName \"net\""],
            ),
            // A doc comment after the error starts the next statement, which keeps it.
            (
                "block A { let x = fn\n/// doc\nnet ok; }",
                &[
                    "block A",
                    "  error \"let x = fn\"",
                    "  net \"/// doc\\nnet ok;\" (doc)",
                ],
                &["Reserved \"fn\""],
            ),
        ];
        for (src, nodes, errs) in cases {
            let (got_nodes, got_errors) = outline(src);
            assert_eq!(got_nodes, nodes, "{src}");
            assert_eq!(got_errors, errs, "{src}");
        }
    }

    /// Review: item recovery skips to the next `block` or `contract` (so one error for
    /// `port x; fn f() …`), but not to one inside braces opened after the error.
    #[test]
    fn item_recovery() {
        let (nodes, errs) = outline("port x; fn f() { block inner {} } block A { net ok; }");
        assert_eq!(
            nodes,
            [
                "item-error \"port x; fn f() { block inner {} }\"",
                "block A",
                "  net \"net ok;\"",
            ]
        );
        assert_eq!(errs, ["ExpectedItem \"port\""]);
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
