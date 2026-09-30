//! The parser: tokens → syntax tree (ast.md). Recursive descent that builds nodes
//! directly, with a Pratt loop for expressions (`expr.rs`). It never fails: broken code
//! becomes `Error` nodes that keep their spans, and every problem is collected.
//!
//! One function per rule of grammar.md §3, in this file's order: `file` → `item` →
//! `block_decl` → `port`, or `body` → `stmts` → `stmt` → `net_stmt` … `spec_stmt`, then
//! `expr` (`expr.rs`: `expr_bp` → `unary` → `postfix` → `primary`).
//!
//! How an error travels (decision A8):
//! - **Fatal:** a rule that can't go on records its error and returns `Err(Reported)`. `?`
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
pub use expr::Infix;

use codespan_reporting::diagnostic::Diagnostic;

use spicy_errors::Reported;
use spicy_span::Span;

use crate::lexer::{LexError, TokenIdx, TokenKind, Tokens, check, lookalike, scan};
use ast::{
    Attribute, BinOp, BlockDecl, BlockEntry, Body, BodyKind, Expr, ExprKind, File, Ident, Item,
    ItemKind, Node, Path, Relation, Stmt, StmtKind, Type, TypeKind,
};
use spicy_errors::Render;

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
    /// [`spicy_errors::render_plain`] or codespan-reporting's terminal output).
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
    let mut parser = Parser::new(&tokens, Reported::among(&lex_errors));
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
type PResult<T> = Result<T, Reported>;

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
    /// The proof, if the lexer reported an error: a number it rejected is an `Error` node.
    lexed: Option<Reported>,
}

impl<'t, 'src> Parser<'t, 'src> {
    fn new(tokens: &'t Tokens<'src>, lexed: Option<Reported>) -> Self {
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
            lexed,
        }
    }

    /// The parse errors, one per mistake: those a lexer error already explains are
    /// dropped (decision A7), and so are repeats of "expected `)`" at the end of the file.
    /// A proof made from a dropped one still holds: the lexer error that explains it, or
    /// the first "expected `)`", is still reported.
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
    /// skipped up to the next item.
    fn file(&mut self) -> File<'src> {
        let mut items = Vec::new();
        while self.peek() != TokenKind::Eof {
            let start = self.pos;
            match self.item() {
                Ok(Some(item)) => items.push(item),
                Ok(None) => break,
                Err(reported) => {
                    let span = self.recover(start, Level::Item);
                    items.push(error_node(ItemKind::Error(reported), span));
                }
            }
        }
        File {
            items,
            span: Span::new(0, self.src().len() as u32),
        }
    }

    /// `item = { DOC } { attribute } [ "pub" ] ( block | circuit | contract )`, or `None`
    /// when only doc comments or attributes are left before the end of the file. `pub`
    /// on anything but a block is reported, and the item is read on.
    fn item(&mut self) -> PResult<Option<Item<'src>>> {
        let start = self.span().start;
        let (docs, attrs) = self.docs_and_attrs()?;
        let public = self.eat(TokenKind::KwPub);
        if let Some(public) = public
            && self.peek() != TokenKind::KwBlock
        {
            let removal = Span::new(public.start, self.span().start);
            let error = ParseError::new(ParseErrorKind::PubNotAllowed, public);
            self.errors.push(error.with_fix(removal, ""));
        }
        let kind = match self.peek() {
            TokenKind::KwBlock => {
                self.bump();
                ItemKind::Block(self.block_decl(public)?)
            }
            TokenKind::KwCircuit => {
                self.bump();
                ItemKind::Circuit(self.body(BodyKind::Circuit)?)
            }
            TokenKind::KwContract => {
                self.bump();
                ItemKind::Contract(self.body(BodyKind::Contract)?)
            }
            TokenKind::Eof => {
                self.unattached(&docs, &attrs, "an item after the attribute")?;
                return Ok(None);
            }
            kind if is_reserved(kind) => return Err(self.reserved()),
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

    /// `Name { ports }`, after `block`. Each port is `name: Type`, separated by `,`; a
    /// broken one becomes a `BlockEntry::Error` over the text skipped to the next `,`,
    /// and the others are kept. The `{` and `}` are handled as a body's are.
    fn block_decl(&mut self, public: Option<Span>) -> PResult<BlockDecl<'src>> {
        use TokenKind::{Colon, Comma, DocComment, Ident, Pound};
        let reported = self.errors.len();
        let name = self.name()?;
        let starts_port = |p: &Self| {
            matches!(p.peek(), DocComment | Pound) || (p.peek() == Ident && p.nth(1) == Colon)
        };
        let open = self.open_brace(&name, starts_port)?;
        let mut ports = Vec::new();
        while !ends_body(self.peek()) {
            let start = self.pos;
            match self.port() {
                Ok(Some(port)) => ports.push(port),
                Ok(None) => break,
                Err(reported) => {
                    let span = self.recover(start, Level::Entry);
                    ports.push(error_node(BlockEntry::Error(reported), span));
                }
            }
            self.eat(Comma);
        }
        self.close_body(open);
        let broken = Reported::among(&self.errors[reported..]);
        Ok(BlockDecl {
            public,
            name,
            ports,
            broken,
        })
    }

    /// `{ DOC } { attribute } name: Type`, up to the `,` or `}` after it, or `None` when
    /// only doc comments or attributes are left before the `}`. A type that doesn't
    /// parse becomes a `TypeKind::Error`, and the port keeps its name.
    fn port(&mut self) -> PResult<Option<Node<'src, BlockEntry<'src>>>> {
        let start = self.span().start;
        let (docs, attrs) = self.docs_and_attrs()?;
        if ends_body(self.peek()) {
            self.unattached(&docs, &attrs, "a port after the attribute")?;
            return Ok(None);
        }
        let name = self.name()?;
        self.expect(TokenKind::Colon, "`:`")?;
        let ty = self.rhs(Level::Entry, Self::ty, |reported, span| Type {
            kind: TypeKind::Error(reported),
            span,
        });
        Ok(Some(Node {
            docs,
            attrs,
            kind: BlockEntry::Port { name, ty },
            span: Span::new(start, self.prev_end()),
        }))
    }

    /// `Name { statements }`, after `circuit` or `contract`.
    fn body(&mut self, which: BodyKind) -> PResult<Body<'src>> {
        let reported = self.errors.len();
        let name = self.name()?;
        let open = self.open_brace(&name, |p| starts_stmt(p.peek()))?;
        let stmts = self.stmts(which);
        self.close_body(open);
        let broken = Reported::among(&self.errors[reported..]);
        Ok(Body {
            name,
            stmts,
            broken,
        })
    }

    /// The `{` after an item's name. A missing one, when what follows starts the item's
    /// contents (`starts_contents`) or is its `}`, is reported with the insertion and
    /// the item is read on, so it keeps its name (a soft error, like a missing `;`).
    fn open_brace(
        &mut self,
        name: &Ident,
        starts_contents: impl Fn(&Self) -> bool,
    ) -> PResult<Span> {
        if let Some(open) = self.eat(TokenKind::LBrace) {
            return Ok(open);
        }
        if !starts_contents(self) && self.peek() != TokenKind::RBrace {
            return Err(self.expected("`{`"));
        }
        let at = Span::new(name.span.end, name.span.end);
        let error = ParseError::new(ParseErrorKind::MissingBrace, name.span);
        self.errors.push(error.with_fix(at, " {"));
        Ok(at)
    }

    /// The `}` that closes an item opened at `open`. A missing one at the end of the
    /// file, or before the next item, is reported, and the item ends there.
    fn close_body(&mut self, open: Span) {
        if self.eat(TokenKind::RBrace).is_none() {
            self.unclosed(open, "}");
        }
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
                Err(reported) => {
                    let span = self.recover(start, Level::Stmt);
                    stmts.push(error_node(StmtKind::Error(reported), span));
                }
            }
        }
        stmts
    }

    /// `{ DOC } { attribute } ( net | let | assume | spec )`, or `None` when only doc
    /// comments or attributes are left before the end of the body.
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
        // Each reads through its `;`.
        let kind = match keyword {
            KwNet => self.net_stmt()?,
            KwLet => self.let_stmt()?,
            KwAssume => self.assume_stmt()?,
            KwSpec => self.spec_stmt()?,
            kind if is_reserved(kind) => return Err(self.reserved()),
            _ => {
                return Err(self.expected("a statement (`net`, `let`, `assume` or `spec`)"));
            }
        };
        Ok(Some(Stmt {
            docs,
            attrs,
            kind,
            span: Span::new(start, self.prev_end()),
        }))
    }

    /// `net name;` or `net name = merge;`
    fn net_stmt(&mut self) -> PResult<StmtKind<'src>> {
        self.bump(); // `net`
        let name = self.name()?;
        let merge = match self.eat(TokenKind::Eq) {
            Some(_) => Some(self.rhs(Level::Value, Self::expr, error_expr)),
            None => {
                self.semi()?;
                None
            }
        };
        Ok(StmtKind::Net { name, merge })
    }

    /// `let name = value;`
    fn let_stmt(&mut self) -> PResult<StmtKind<'src>> {
        self.bump(); // `let`
        let name = self.name()?;
        self.expect(TokenKind::Eq, "`=`")?;
        let value = self.rhs(Level::Value, Self::expr, error_expr);
        Ok(StmtKind::Let { name, value })
    }

    /// A right-hand side, then what ends it: a statement's value and its `;`
    /// (`Level::Value`), or a port's type and the `,` or `}` after it (`Level::Entry`).
    /// When the value doesn't parse, or something else follows it (`let z = 1 2;`), it
    /// becomes an `Error` node over the text up to the end, and its statement or port
    /// keeps its name: `net base = [g g];` still declares `base`, so its uses aren't
    /// reported again (rustc: "we still want a field even if its expr didn't parse").
    fn rhs<T>(
        &mut self,
        level: Level,
        parse: impl FnOnce(&mut Self) -> PResult<T>,
        error: impl FnOnce(Reported, Span) -> T,
    ) -> T {
        let start = self.pos;
        let value = parse(self).and_then(|value| self.end_of(level).map(|()| value));
        value.unwrap_or_else(|reported| {
            let span = self.recover(start, level);
            // The statement's `;`, if it has one: recovery stops before it. (An entry's
            // `,` is its list's.)
            if level == Level::Value {
                self.eat(TokenKind::Semi);
            }
            error(reported, span)
        })
    }

    /// What ends a right-hand side at `level` (see `rhs`): a statement's `;`, or the `,`
    /// or `}` after an entry, which the entry's list reads.
    fn end_of(&mut self, level: Level) -> PResult<()> {
        match level {
            Level::Value => self.semi(),
            _ if self.peek() == TokenKind::Comma || ends_body(self.peek()) => Ok(()),
            _ => Err(self.expected("`,` or `}`")),
        }
    }

    /// `assume relation;`
    fn assume_stmt(&mut self) -> PResult<StmtKind<'src>> {
        self.bump(); // `assume`
        let kind = match self.relation()? {
            Ok(relation) => StmtKind::Assume { relation },
            Err(reported) => StmtKind::Error(reported),
        };
        self.semi()?;
        Ok(kind)
    }

    /// `spec name: relation`
    fn spec_stmt(&mut self) -> PResult<StmtKind<'src>> {
        use TokenKind::{Colon, Ident};
        self.bump(); // `spec`
        // A keyword before the `:` is a bad name, not a missing one (`name` reports it),
        // and `spec g x within y` is a missing `:`. Only a measure right after `spec`
        // (`spec dc(out.v) within …`) lacks the name.
        let named = self.nth(1) == Colon || (self.peek() == Ident && self.nth(1) == Ident);
        if !named {
            return Err(self.fail(ParseErrorKind::SpecNeedsName));
        }
        let name = self.name()?;
        self.expect(Colon, "`:`")?;
        let kind = match self.relation()? {
            Ok(relation) => StmtKind::Spec { name, relation },
            Err(reported) => StmtKind::Error(reported),
        };
        self.semi()?;
        Ok(kind)
    }

    /// The expression of an `assume` or `spec`, split at its top-level relation. `None`
    /// when the top isn't one: reported, but a soft error, so the statement becomes
    /// `StmtKind::Error` and its `;` is still read.
    fn relation(&mut self) -> PResult<Result<Relation<'src>, Reported>> {
        let expr = self.expr()?;
        if let ast::ExprKind::Binary {
            op: BinOp::Rel(op),
            lhs,
            rhs,
        } = expr.kind
        {
            return Ok(Ok(Relation {
                lhs: *lhs,
                op,
                rhs: *rhs,
            }));
        }
        let kind = ParseErrorKind::NotARelation;
        Ok(Err(
            ParseError::new(kind, expr.span).report(&mut self.errors)
        ))
    }

    /// The `;` that ends a statement. If the next token starts something new, a missing
    /// `;` is reported with an insertion fix and parsing goes on (a soft error).
    fn semi(&mut self) -> PResult<()> {
        if self.eat(TokenKind::Semi).is_some() {
            return Ok(());
        }
        let next = self.peek();
        if starts_stmt(next) || ends_body(next) {
            let last = self.span_at(self.pos - 1);
            self.error_inserting(ParseErrorKind::MissingSemi, last, ";");
            return Ok(());
        }
        Err(self.expected("`;`"))
    }

    // --- Recovery ----------------------------------------------------------------

    /// Skips the rest of a broken statement, item or entry that started at `start` and
    /// returns the span of everything it covered (decision A8). It scans again from
    /// `start`, so brackets opened before the error are known, and stops at the first of
    /// its four rules that holds (in the loop, one match arm each).
    ///
    /// Brackets opened *before* the error point are assumed broken: a statement or item
    /// keyword at or after the error ends the statement even inside them, so an unclosed
    /// `Resistor {` doesn't swallow the rest of the block. Brackets opened *after* the
    /// error are skipped whole (`for i in 0..N { … }`). A statement's or item's first
    /// token is always consumed (unless at the end), so callers' loops progress; a value
    /// or an entry may be empty, and its list reads the `,` or `}` it stopped at.
    fn recover(&mut self, start: usize, level: Level) -> Span {
        use TokenKind::*;
        let failed_at = self.pos;
        self.pos = start;
        debug_assert_eq!(
            self.nesting, 0,
            "`nested` restores the count, even on errors"
        );
        // Brackets open at the current token: `{` split by whether it was opened before
        // or after the error; `(` and `[` only keep a `;` or `,` inside them from ending
        // the statement or entry.
        let (mut braces_before, mut braces_after, mut parens) = (0u32, 0u32, 0u32);
        while self.peek() != Eof {
            let (kind, at) = (self.peek(), self.pos);
            // With no brace open: between items, or between a body's statements.
            let outside = braces_before == 0 && braces_after == 0;
            let in_body = outside && level != Level::Item;
            // The four places it stops; no token fits more than one.
            match kind {
                // 1. Before the next statement or item: at or after the error, outside the
                // braces opened after it. Not at a statement's or item's own first token,
                // so that is always consumed; a value or an entry may be empty (`let r =`
                // before the next statement).
                _ if level.resumes_at(kind)
                    && at >= failed_at
                    && braces_after == 0
                    && (at > start || matches!(level, Level::Value | Level::Entry)) =>
                {
                    break;
                }
                // 2. Before the `}` that closes the body.
                RBrace if in_body => break,
                // 3. Before the doc comments and attributes of what comes next, which
                // keeps them: strictly after the error, so `let x = /// doc` isn't
                // restarted at the doc comment it failed on.
                DocComment | Pound if outside && at > failed_at => break,
                // 4. At the end of a statement (`;`) or an entry (`,`), with no bracket
                // open: a statement's recovery takes its `;`; a value's leaves it to its
                // statement, and an entry's `,` is its list's.
                _ if in_body && parens == 0 && level.ends_at(kind) => {
                    if level == Level::Stmt {
                        self.bump();
                    }
                    break;
                }
                _ => {}
            }
            // Otherwise it's skipped, and the brackets it opens or closes are counted.
            match kind {
                LBrace if at < failed_at => braces_before += 1,
                LBrace => braces_after += 1,
                RBrace if braces_after > 0 => braces_after -= 1,
                RBrace => braces_before = braces_before.saturating_sub(1),
                LParen | LBracket => parens += 1,
                RParen | RBracket => parens = parens.saturating_sub(1),
                _ => {}
            }
            self.bump();
        }
        // What was skipped. An empty value (`let j =` before the next statement) is at
        // the end of the text before it, inside its statement.
        let end = self.prev_end();
        let from = if self.pos == start {
            end
        } else {
            self.span_at(start).start
        };
        Span::new(from, end)
    }

    // --- Names, paths, types, attributes, lists ----------------------------------

    fn name(&mut self) -> PResult<Ident<'src>> {
        match self.peek() {
            TokenKind::Ident => {
                let text = self.text();
                let span = self.bump();
                Ok(Ident { text, span })
            }
            kind if is_reserved(kind) => Err(self.reserved()),
            kind if is_keyword(kind) => {
                let keyword = self.text().to_string();
                let reported = self.fail(ParseErrorKind::KeywordAsName { keyword });
                // Consumed, so recovery doesn't read it as the start of a statement.
                self.bump();
                Err(reported)
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
            let kind = TypeKind::Path { path, args };
            Ok(Type { kind, span })
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
        let mut out = Vec::new();
        // Items, each followed by a `,` or the closer.
        while self.peek() != close && self.peek() != TokenKind::Eof {
            out.push(item(self)?);
            if self.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        if self.eat(close).is_none() {
            let (closer, _, after_item) = closer_names(close);
            return Err(self.unclosed_or_expected(open, closer, after_item));
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
    // As Zig's `warn` and `fail`: the `error…` functions record a soft error, and parsing
    // goes on; `fail` records one and returns the proof that unwinds (`Err(Reported)`).

    fn error(&mut self, kind: ParseErrorKind, span: Span) {
        self.errors.push(ParseError::new(kind, span));
    }

    /// Records a soft error at `span` whose fix inserts `text` right after it (the `;`
    /// missing after a statement), and returns where `text` goes.
    fn error_inserting(&mut self, kind: ParseErrorKind, span: Span, text: &'static str) -> Span {
        let at = Span::new(span.end, span.end);
        let error = ParseError::new(kind, span);
        self.errors.push(error.with_fix(at, text));
        at
    }

    /// Records a soft error at `span` whose fix writes `text` in its place (`..=` for `..`),
    /// as every lexer fix does (`check::fixed`).
    fn error_replacing(&mut self, kind: ParseErrorKind, span: Span, text: &'static str) {
        let error = ParseError::new(kind, span);
        self.errors.push(error.with_fix(span, text));
    }

    /// Records a soft error at the current token.
    fn error_here(&mut self, kind: ParseErrorKind) {
        let span = self.span();
        self.error(kind, span);
    }

    /// Records an error at the current token, for a rule that can't go on.
    fn fail(&mut self, kind: ParseErrorKind) -> Reported {
        let span = self.span();
        ParseError::new(kind, span).report(&mut self.errors)
    }

    /// "expected `what`, found …" at the current token.
    fn expected(&mut self, what: &'static str) -> Reported {
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
    fn reserved(&mut self) -> Reported {
        let word = self.text().to_string();
        self.fail(ParseErrorKind::Reserved { word })
    }

    /// "expected `}`" at the current token, pointing back at the `open`er it would close.
    fn unclosed(&mut self, open: Span, closer: &'static str) -> Reported {
        let span = self.span();
        let error = ParseError::new(ParseErrorKind::Unclosed { closer }, span);
        error.with_related(open).report(&mut self.errors)
    }

    /// At the end of the file: `unclosed`. Otherwise "expected `expected`, found …"
    /// (`` `)` `` or `` `,` or `)` ``).
    fn unclosed_or_expected(
        &mut self,
        open: Span,
        closer: &'static str,
        expected: &'static str,
    ) -> Reported {
        if self.peek() == TokenKind::Eof {
            self.unclosed(open, closer)
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

/// A value that didn't parse, over the text skipped (see `rhs`).
fn error_expr<'src>(reported: Reported, span: Span) -> Expr<'src> {
    Expr {
        kind: ExprKind::Error(reported),
        span,
    }
}

/// What `recover` skips, which decides where it stops.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    /// A whole item, up to the next one.
    Item,
    /// A whole statement, through its `;` or up to the next statement or item.
    Stmt,
    /// A statement's right-hand side: the rest of the statement, up to its `;`, which is
    /// left to the statement. Unlike a statement, it may be empty.
    Value,
    /// An entry of a comma list in braces (a block's port), or the rest of it after its
    /// name: up to the next `,` or the `}`.
    Entry,
}

impl Level {
    /// Whether `kind` starts what comes next at this level.
    fn resumes_at(self, kind: TokenKind) -> bool {
        match self {
            Level::Item | Level::Entry => starts_item(kind),
            Level::Stmt | Level::Value => starts_item(kind) || is_stmt_keyword(kind),
        }
    }

    /// Whether `kind` ends what this level skips, with no bracket open: a statement's
    /// `;`, an entry's `,`. (In an entry, a `;` is only skipped.)
    fn ends_at(self, kind: TokenKind) -> bool {
        match self {
            Level::Item => false,
            Level::Stmt | Level::Value => kind == TokenKind::Semi,
            Level::Entry => kind == TokenKind::Comma,
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
        TokenKind::Gt => (">", "`>`", "`,` or `>`"),
        _ => unreachable!("{close:?} closes nothing"),
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
        KwBlock
            | KwCircuit
            | KwSetup
            | KwContract
            | KwEnv
            | KwConst
            | KwPub
            | KwNet
            | KwLet
            | KwAssume
            | KwSpec
            | KwRated
            | KwEnsure
            | KwWithin
            | KwFor
            | KwIn
            | KwReserved
    )
}

/// A word the parser doesn't read yet: "`fn` isn't supported yet". The v5 keywords the
/// parser doesn't handle yet are here too, until their steps.
pub(super) fn is_reserved(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        KwReserved | KwSetup | KwEnv | KwConst | KwRated | KwEnsure | KwFor
    )
}

fn starts_item(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(kind, KwPub | KwBlock | KwCircuit | KwContract)
}

fn is_stmt_keyword(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(kind, KwNet | KwLet | KwAssume | KwSpec)
}

/// Which body a statement belongs in (`let` goes in both, so it has none).
fn home_body(keyword: TokenKind) -> Option<(&'static str, BodyKind)> {
    match keyword {
        TokenKind::KwNet => Some(("net", BodyKind::Circuit)),
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
    use crate::testing::{
        Rng, assert_every_kind_has_a_case, check_parse_invariants, dump_parse, file_name, read,
    };
    use spicy_errors::DiagKind;

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
                "block B {{ p: {}B{} }}",
                "A<".repeat(20_000),
                ">".repeat(20_000)
            ),
            format!("block B {{ p: {}B }}", "A<".repeat(20_000)),
            format!("circuit B {{ let x = a{}; }}", " + a".repeat(100_000)),
            format!("circuit B {{ let x = a{}; }}", ".b".repeat(100_000)),
            format!("circuit B {{ let x = f{}; }}", "()".repeat(100_000)),
            format!("circuit B {{ let x = {}1; }}", "- ".repeat(20_000)),
            format!(
                "circuit B {{ let x = {}1{}; }}",
                "[".repeat(20_000),
                "]".repeat(20_000)
            ),
            format!("circuit B {{ let x = {}; }}", "R { a: ".repeat(20_000)),
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
                "circuit B {{ let x = a{}{}; }}",
                ".b".repeat(600),
                " + a".repeat(600)
            ),
            format!(
                "circuit B {{ let x = -a{}{}; }}",
                ".b".repeat(600),
                " + a".repeat(600)
            ),
            // A chain grows above an operand already parsed: the second review found
            // `(…)` here pushed 600 levels down without an error.
            format!(
                "circuit B {{ let x = a + (a{}){}; }}",
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
        let sum = format!("circuit B {{ let x = a{}; }}", " + a".repeat(500));
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

    /// Red team: every suggested fix, applied on its own, removes the error it was
    /// offered for and adds none (an editor or the AI applies fixes without a second
    /// look), as resolve's fixes do.
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
                let before = problem_names(&src);
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
                    let mut expected = before.clone();
                    let at = expected.iter().position(|&n| n == name).unwrap();
                    expected.remove(at);
                    assert_eq!(
                        problem_names(&fixed),
                        expected,
                        "{}: fix {:?} for {name} at {}..{}",
                        path.display(),
                        fix.replacement,
                        fix.span.start,
                        fix.span.end,
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 20, "only {checked} fixes checked");
    }

    /// Every lexer and parser problem of `src`, by name, sorted.
    fn problem_names(src: &str) -> Vec<&'static str> {
        let parsed = parse(src);
        let lex = parsed.lex_errors.iter().map(|e| e.kind.name());
        let mut names: Vec<&'static str> = lex
            .chain(parsed.errors.iter().map(|e| e.kind.name()))
            .collect();
        names.sort_unstable();
        names
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
    /// The invariants are checked on the way, so every source the tests write is.
    fn outline(src: &str) -> (Vec<String>, Vec<String>) {
        check_parse_invariants(src);
        let parsed = parse(src);
        let text = |span: Span| src[span.range()].to_string();
        let mut nodes = Vec::new();
        for item in &parsed.file.items {
            let (kind, body) = match &item.kind {
                ItemKind::Block(decl) => {
                    nodes.push(format!("block {}", decl.name.text));
                    for port in &decl.ports {
                        let kind = match port.kind {
                            BlockEntry::Port { .. } => "port",
                            BlockEntry::Error(_) => "port-error",
                        };
                        nodes.push(format!("  {kind} {:?}", text(port.span)));
                    }
                    continue;
                }
                ItemKind::Circuit(b) => ("circuit", Some(b)),
                ItemKind::Contract(b) => ("contract", Some(b)),
                ItemKind::Error(_) => ("item-error", None),
            };
            match body {
                Some(b) => nodes.push(format!("{kind} {}", b.name.text)),
                None => nodes.push(format!("{kind} {:?}", text(item.span))),
            }
            for stmt in body.map_or(&[][..], |b| &b.stmts) {
                let kind = match stmt.kind {
                    StmtKind::Net { .. } => "net",
                    StmtKind::Let { .. } => "let",
                    StmtKind::Assume { .. } => "assume",
                    StmtKind::Spec { .. } => "spec",
                    StmtKind::Error(_) => "error",
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
                format!(
                    "circuit B {{ let x = {}1{}; }}",
                    "(".repeat(n),
                    ")".repeat(n)
                )
            }),
            ("minus", 128, |n| {
                format!("circuit B {{ let x = {}1; }}", "- ".repeat(n))
            }),
            ("types", 128, |n| {
                format!("block B {{ p: {}B{} }}", "A<".repeat(n), ">".repeat(n))
            }),
            ("attribute args", 128, |n| {
                format!("#[a({}1{})] block B {{ }}", "(".repeat(n), ")".repeat(n))
            }),
            ("sum", 1024, |n| {
                format!("circuit B {{ let x = a{}; }}", " + a".repeat(n))
            }),
            ("fields", 1024, |n| {
                format!("circuit B {{ let x = a{}; }}", ".b".repeat(n))
            }),
            ("calls", 1024, |n| {
                format!("circuit B {{ let x = f{}; }}", "()".repeat(n))
            }),
            ("minus, fields and sum", 512, |n| {
                format!(
                    "circuit B {{ let x = -a{}{}; }}",
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
        assert_eq!(errors("circuit A { let x = 1. ; }"), none);
        // Everything after an unterminated comment, even two unclosed constructs.
        assert_eq!(errors("circuit A { let x = f(1, /* never"), none);
        // An error before the comment is kept.
        assert_eq!(
            errors("circuit A { let x = 1 /* never"),
            ["MissingSemi \"1\""]
        );
        // A later mistake, past other lexer errors, is kept.
        assert_eq!(
            errors("circuit A { let a = .5; let b = c − d; let e = f g; }"),
            ["Expected `;` \"g\""]
        );
    }

    /// Review: every list takes a trailing comma or none, may be empty (except type
    /// arguments, and generic arguments, which only `< name =` starts), names its own
    /// closer when a separator is missing, and reports an unclosed list at the end of
    /// the file.
    #[test]
    fn lists() {
        let none: [&str; 0] = [];
        for ok in [
            "circuit A { let x = f(); let y = f(1, 2); let z = f(1, 2,); }",
            "circuit A { let x = f(a: 1, b: 2,); }",
            "circuit A { let x = []; let y = [a, b]; let z = [a, b,]; }",
            "circuit A { let x = R {}; let y = R { a: b, c }; let z = R { a: b, }; }",
            "circuit A { let x = G<A = 1> {}; let y = G<A = 1, B = 2,> {}; }",
            "#[a()] #[b(1, 2)] #[c(1,)] block A { p: A<B, C>, q: A<B,> }",
        ] {
            assert_eq!(errors(ok), none, "{ok}");
        }
        for (src, expected) in [
            ("circuit A { let x = f(1 2); }", "Expected `,` or `)` \"2\""),
            ("circuit A { let x = [1 2]; }", "Expected `,` or `]` \"2\""),
            (
                "circuit A { let x = R { a: b c }; }",
                "Expected `,` or `}` \"c\"",
            ),
            ("block A { p: A<B C> }", "Expected `,` or `>` \"C\""),
            (
                "circuit A { let x = G<A = 1 B = 2> {}; }",
                "Expected `,` or `>` \"B\"",
            ),
            ("block A { p: A<> }", "Expected a type \">\""),
            ("circuit A { let x = f(1,", "Unclosed )"),
            ("circuit A { let x = [1", "Unclosed ]"),
            ("circuit A { let x = G<A = 1", "Unclosed >"),
            ("#[a(1", "Unclosed )"),
            ("#[a", "Unclosed ]"),
        ] {
            assert_eq!(errors(src), [expected], "{src}");
        }
    }

    /// Every closer outside a list (`)` of a parenthesis, `]` of an index or an
    /// attribute, `}` of a body) reports "expected" when something else is there, and
    /// `Unclosed`, pointing at its opener, at the end of the file. Constructs left open
    /// at the end give one error, the innermost's.
    #[test]
    fn closers() {
        for (src, expected) in [
            ("circuit A { let x = (1 2); }", &["Expected `)` \"2\""][..]),
            ("circuit A { let x = (1", &["Unclosed )"]),
            ("circuit A { let x = m[s t]; }", &["Expected `]` \"t\""]),
            ("circuit A { let x = m[s", &["Unclosed ]"]),
            ("#[a b] block A { }", &["Expected `]` \"b\""]),
            ("#[a(1) b] block A { }", &["Expected `]` \"b\""]),
            ("circuit A { net a;", &["Unclosed }"]),
            ("circuit A { net a; contract A { }", &["Unclosed }"]),
            ("circuit A { let x = f(1, [2", &["Unclosed ]"]),
            ("circuit A { let x = R { a: (1", &["Unclosed )"]),
        ] {
            assert_eq!(errors(src), expected, "{src}");
        }
        // The related span is the opener.
        let src = "circuit A { let x = f(1, [2";
        let parsed = parse(src);
        let related = parsed.errors[0].related.expect("an opener");
        assert_eq!(&src[related.range()], "[");
    }

    /// Review: what statement recovery skips and where it stops (grammar.md §6), one
    /// rule per case.
    #[test]
    fn statement_recovery() {
        let cases: [(&str, &[&str], &[&str]); 8] = [
            // A lone `;` is consumed, so the loop progresses.
            (
                "circuit A { ; net ok; }",
                &["circuit A", "  error \";\"", "  net \"net ok;\""],
                &["Expected a statement (`net`, `let`, `assume` or `spec`) \";\""],
            ),
            // A `{` opened before the error is assumed broken: the next keyword ends
            // the statement inside it, and the body's `}` still closes the body.
            (
                "circuit A { let r = R { a: 1 2 net ok; }",
                &[
                    "circuit A",
                    "  let \"let r = R { a: 1 2\"",
                    "  net \"net ok;\"",
                ],
                &["Expected `,` or `}` \"2\""],
            ),
            // A `{` opened after the error is skipped whole, keywords and all.
            (
                "circuit A { for x { net inner; } net ok; }",
                &[
                    "circuit A",
                    "  error \"for x { net inner; }\"",
                    "  net \"net ok;\"",
                ],
                &["Reserved \"for\""],
            ),
            // An attribute the statement failed on doesn't restart it.
            (
                "circuit A { let x = #[a] 1; net ok; }",
                &[
                    "circuit A",
                    "  let \"let x = #[a] 1;\"",
                    "  net \"net ok;\"",
                ],
                &["Expected an expression \"#\""],
            ),
            // A keyword used as a name is skipped, not read as the next statement.
            (
                "circuit A { let net = 1; net ok; }",
                &["circuit A", "  error \"let net = 1;\"", "  net \"net ok;\""],
                &["KeywordAsName \"net\""],
            ),
            // A doc comment after the error starts the next statement, which keeps it.
            (
                "circuit A { let x = fn\n/// doc\nnet ok; }",
                &[
                    "circuit A",
                    "  let \"let x = fn\"",
                    "  net \"/// doc\\nnet ok;\" (doc)",
                ],
                &["Reserved \"fn\""],
            ),
            // A value that breaks leaves its statement, and the name it declares:
            // `base` is still a net, so its uses aren't reported again.
            (
                "circuit A { net base = [g g]; net ok; }",
                &[
                    "circuit A",
                    "  net \"net base = [g g];\"",
                    "  net \"net ok;\"",
                ],
                &["Expected `,` or `]` \"g\""],
            ),
            // An empty value before the next statement is empty, inside its statement
            // (found by fuzzing: it was placed at the next statement).
            (
                "circuit A { let j =\n    let k = 1; }",
                &["circuit A", "  let \"let j =\"", "  let \"let k = 1;\""],
                &["Expected an expression \"let\""],
            ),
        ];
        for (src, nodes, errs) in cases {
            let (got_nodes, got_errors) = outline(src);
            assert_eq!(got_nodes, nodes, "{src}");
            assert_eq!(got_errors, errs, "{src}");
        }
    }

    /// Review: item recovery skips to the next item (so one error for `net x; fn f() …`),
    /// but not to one inside braces opened after the error.
    #[test]
    fn item_recovery() {
        let (nodes, errs) = outline("net x; fn f() { block inner {} } circuit A { net ok; }");
        assert_eq!(
            nodes,
            [
                "item-error \"net x; fn f() { block inner {} }\"",
                "circuit A",
                "  net \"net ok;\"",
            ]
        );
        assert_eq!(errs, ["ExpectedItem \"net\""]);
        // The next item keeps the doc comment and attribute written above it, as a
        // statement does (the flow review found they were skipped with the broken text,
        // and lost without a word). Inside braces opened after the error, they're skipped.
        let src = "net x;\n/// The amp.\n#[a]\nblock A { }";
        let parsed = parse(src);
        let [broken, block] = &parsed.file.items[..] else {
            panic!("{:#?}", parsed.file.items);
        };
        assert_eq!(&src[broken.span.range()], "net x;");
        assert_eq!((block.docs.len(), block.attrs.len()), (1, 1));
        assert_eq!(errors(src), ["ExpectedItem \"net\""]);
        let src = "fn f() { /// inner\n #[a] } block A { }";
        assert_eq!(
            outline(src).0[0],
            "item-error \"fn f() { /// inner\\n #[a] }\""
        );
    }

    /// Review: what port recovery skips and where it stops, one rule per case. A broken
    /// port ends at the next `,` or the `}` (a `;` is skipped), and a port whose type
    /// doesn't parse keeps its name.
    #[test]
    fn port_recovery() {
        let cases: [(&str, &[&str], &[&str]); 5] = [
            (
                "block A { a Pin, b: Pin }",
                &["block A", "  port-error \"a Pin\"", "  port \"b: Pin\""],
                &["Expected `:` \"Pin\""],
            ),
            (
                "block A { a: , b: Pin }",
                &["block A", "  port \"a:\"", "  port \"b: Pin\""],
                &["Expected a name \",\""],
            ),
            // A `,` inside brackets opened after the error doesn't end the port.
            (
                "block A { a: X Y(c, d), b: Pin }",
                &["block A", "  port \"a: X Y(c, d)\"", "  port \"b: Pin\""],
                &["Expected `,` or `}` \"Y\""],
            ),
            // `;` between ports, as between statements, is skipped with the rest.
            (
                "block A { a: Pin; b: Pin }",
                &["block A", "  port \"a: Pin; b: Pin\""],
                &["Expected `,` or `}` \";\""],
            ),
            // A header left open ends at the next item.
            (
                "block A { a: Pin, circuit A {}",
                &["block A", "  port \"a: Pin\"", "circuit A"],
                &["Unclosed }"],
            ),
        ];
        for (src, nodes, errs) in cases {
            crate::testing::check_parse_invariants(src);
            let (got_nodes, got_errors) = outline(src);
            assert_eq!(got_nodes, nodes, "{src}");
            assert_eq!(got_errors, errs, "{src}");
        }
    }

    /// Pieces for random input: mostly the language's own tokens, so the parser gets
    /// deep into its rules, plus some noise.
    const PIECES: &[&str] = &[
        "block", "circuit", "contract", "pub", "net", "let", "assume", "spec", "in", "fn", " A",
        " r1", " vcc", "Resistor", "{", "}", "(", ")", "[", "]", "<", ">", "<=", ">=", ",", ";",
        ":", "::", ".", "..", "..=", "=", "+", "-", "*", "/", "±", "#", "?", " 47k", " 1%", " 3",
        " 4k7", " 1e", "///doc\n", "//c\n", " ", "\n", "−", ";", "%", "\"", "α", "within", "\"s\"",
        "<A =",
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
                ItemKind::Block(decl) => ("block", decl.ports.len()),
                ItemKind::Circuit(b) => ("circuit", b.stmts.len()),
                ItemKind::Contract(b) => ("contract", b.stmts.len()),
                ItemKind::Error(_) => ("error", 0),
            })
            .collect();
        assert_eq!(counts, [("block", 4), ("circuit", 8), ("contract", 6)]);
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
