//! Expressions: a Pratt loop over the precedence table of grammar.md §4.1, plus the
//! shape rules of §4.2–4.3 (the `±` operand rule, no chaining, no spread in a range
//! endpoint or a comparison).
//!
//! Every node is built by [`Parser::node`], which checks its height against
//! `MAX_TREE_DEPTH` (ast.md A11). The height is counted bottom-up, one more than the
//! tallest child, because trees also grow *above* what's already parsed: in
//! `a + (…) + a + … + a` each later `+` pushes `(…)` one level further down.

use super::ast::{BinOp, Expr, ExprKind, Field, RelOp};
use super::{MAX_TREE_DEPTH, PResult, ParseError, ParseErrorKind, Parser};
use spicy_span::Span;

use crate::lexer::{TokenKind, decode_quantity};

/// Precedence levels, loosest first.
const COMPARE: u8 = 1;
const RANGE: u8 = 2;
const TOL: u8 = 3;
const SUM: u8 = 4;
const PRODUCT: u8 = 5;

/// The binary operator a token reads as, and its level.
fn infix(kind: TokenKind) -> Option<(BinOp, u8)> {
    use TokenKind::*;
    Some(match kind {
        KwIn => (BinOp::Rel(RelOp::In), COMPARE),
        Lt => (BinOp::Rel(RelOp::Lt), COMPARE),
        Le => (BinOp::Rel(RelOp::Le), COMPARE),
        Gt => (BinOp::Rel(RelOp::Gt), COMPARE),
        Ge => (BinOp::Rel(RelOp::Ge), COMPARE),
        // `..` is reported and read as `..=` (grammar.md §7 #8).
        DotDotEq | DotDot => (BinOp::Range, RANGE),
        PlusMinus => (BinOp::Tol, TOL),
        Plus => (BinOp::Add, SUM),
        Minus => (BinOp::Sub, SUM),
        Star => (BinOp::Mul, PRODUCT),
        Slash => (BinOp::Div, PRODUCT),
        _ => return None,
    })
}

/// Whether operators of this level may follow one another (`a + b - c`). Comparisons,
/// ranges and tolerances may not: `a < b < c`, `a..=b..=c` and `a ± b ± c` are `Chained`.
fn chains(level: u8) -> bool {
    level > TOL
}

/// A parsed expression and its height: the levels in its tree, itself included (`a` is
/// 1, `-a + b` is 3). The height rides beside the tree rather than in it, so `Expr`
/// stays 64 bytes.
struct Tree<'src> {
    expr: Expr<'src>,
    height: u32,
}

impl<'src> Parser<'_, 'src> {
    pub(super) fn expr(&mut self) -> PResult<Expr<'src>> {
        Ok(self.expr_bp(0)?.expr)
    }

    /// An operand, then the operators that bind tighter than `min`: the Pratt loop (as
    /// rust-analyzer's `expr_bp` and Zig's `parseExprPrecedence`). One nesting level.
    fn expr_bp(&mut self, min: u8) -> PResult<Tree<'src>> {
        self.nested(|p| {
            let mut lhs = p.unary()?;
            let mut prev_level = None;
            while let Some((op, level)) = infix(p.peek()) {
                if level <= min {
                    break;
                }
                let half_open = p.peek() == TokenKind::DotDot;
                let op_span = p.bump();
                // The last right side took every operator tighter than its own, so one
                // of the same level right after it is a chain: `a < b < c`.
                if !chains(level) && prev_level == Some(level) {
                    p.error(ParseErrorKind::Chained { op }, op_span);
                }
                let rhs = p.expr_bp(level)?;
                // After the right side parsed, so `x in 1..;` doesn't get a fix to
                // `1..=;`.
                if half_open {
                    p.errors.push(
                        ParseError::new(ParseErrorKind::HalfOpenRange, op_span)
                            .with_fix(op_span, "..="),
                    );
                }
                p.check_shape(op, &lhs.expr, &rhs.expr);
                let span = Span::new(lhs.expr.span.start, rhs.expr.span.end);
                let children = lhs.height.max(rhs.height);
                let kind = ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs.expr),
                    rhs: Box::new(rhs.expr),
                };
                lhs = p.node(kind, span, children)?;
                prev_level = Some(level);
            }
            Ok(lhs)
        })
    }

    /// grammar.md §4.2 and §4.3. These are soft errors: the node is still built.
    fn check_shape(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) {
        match op {
            BinOp::Tol => self.check_tolerance_operands(lhs, rhs),
            BinOp::Range => {
                for end in [lhs, rhs] {
                    if matches!(end.kind, ExprKind::Binary { op: BinOp::Tol, .. }) {
                        self.error(ParseErrorKind::ToleranceInRange, end.span);
                    }
                }
            }
            BinOp::Rel(RelOp::Lt | RelOp::Le | RelOp::Gt | RelOp::Ge) => {
                for side in [lhs, rhs] {
                    if is_spread(side) {
                        self.error(ParseErrorKind::RangeInComparison, side.span);
                    }
                }
            }
            // `x in 1..=2` is the point of `in`; `1..=2 in x` has it backwards.
            BinOp::Rel(RelOp::In) if is_spread(lhs) => {
                self.error(ParseErrorKind::RangeInComparison, lhs.span);
            }
            _ => {}
        }
    }

    /// The `±` operand rule (grammar.md §4.2): an unparenthesized `+ - * /` on either side
    /// is ambiguous. The error spells out both readings, each of which parses without
    /// the ambiguity: the "whole" one parenthesizes every such operand, and the "one term"
    /// one attaches `±` to the terms right next to it, so `a + b * c ± 1%` reads
    /// `a + b * (c ± 1%)`.
    fn check_tolerance_operands(&mut self, lhs: &Expr, rhs: &Expr) {
        let span = match (is_arithmetic(lhs), is_arithmetic(rhs)) {
            (false, false) => return,
            (true, false) => lhs.span,
            (false, true) => rhs.span,
            (true, true) => Span::new(lhs.span.start, rhs.span.end),
        };
        let src = self.src();
        let text = |e: &Expr| &src[e.span.range()];
        let whole_operand = |e: &Expr| match is_arithmetic(e) {
            true => format!("({})", text(e)),
            false => text(e).to_string(),
        };
        let whole = format!("{} ± {}", whole_operand(lhs), whole_operand(rhs));
        let (left, right) = (last_term(lhs), first_term(rhs));
        let before = &src[lhs.span.start as usize..left.span.start as usize];
        let after = &src[right.span.end as usize..rhs.span.end as usize];
        let part = format!("{before}({} ± {}){after}", text(left), text(right));
        self.error(ParseErrorKind::AmbiguousTolerance { whole, part }, span);
    }

    /// `-x`, or a postfix expression.
    fn unary(&mut self) -> PResult<Tree<'src>> {
        let Some(minus) = self.eat(TokenKind::Minus) else {
            return self.postfix();
        };
        let operand = self.nested(Self::unary)?;
        let span = Span::new(minus.start, operand.expr.span.end);
        let kind = ExprKind::Neg(Box::new(operand.expr));
        self.node(kind, span, operand.height)
    }

    /// A primary followed by any number of `.field` and `(args)`.
    fn postfix(&mut self) -> PResult<Tree<'src>> {
        let mut e = self.primary()?;
        let start = e.expr.span.start;
        loop {
            let (kind, children) = match self.peek() {
                TokenKind::Dot => {
                    self.bump();
                    let name = self.name()?;
                    let base = Box::new(e.expr);
                    (ExprKind::Field { base, name }, e.height)
                }
                TokenKind::LParen => {
                    let open = self.bump();
                    let (args, tallest) = self.exprs(open, TokenKind::RParen)?;
                    let callee = Box::new(e.expr);
                    (ExprKind::Call { callee, args }, e.height.max(tallest))
                }
                _ => return Ok(e),
            };
            e = self.node(kind, Span::new(start, self.prev_end()), children)?;
        }
    }

    fn primary(&mut self) -> PResult<Tree<'src>> {
        let start = self.span().start;
        let (kind, children) = match self.peek() {
            TokenKind::Quantity => {
                let text = self.text();
                self.bump();
                // The lexer already reported a quantity that doesn't decode.
                match decode_quantity(text) {
                    Ok(q) => (ExprKind::Quantity(q), 0),
                    Err(_) => {
                        let reported = self.lexed.expect("the lexer reports a number it rejects");
                        (ExprKind::Error(reported), 0)
                    }
                }
            }
            TokenKind::Ident => {
                let path = self.path()?;
                match self.eat(TokenKind::LBrace) {
                    Some(open) => {
                        let (fields, tallest) =
                            self.tallest_list(open, TokenKind::RBrace, Self::field)?;
                        (ExprKind::StructLit { path, fields }, tallest)
                    }
                    None => (ExprKind::Path(path), 0),
                }
            }
            TokenKind::LParen => {
                let open = self.bump();
                let inner = self.expr_bp(0)?;
                self.close(open, TokenKind::RParen)?;
                (ExprKind::Paren(Box::new(inner.expr)), inner.height)
            }
            TokenKind::LBracket => {
                let open = self.bump();
                let (items, tallest) = self.exprs(open, TokenKind::RBracket)?;
                (ExprKind::Array(items), tallest)
            }
            TokenKind::KwReserved => return Err(self.reserved()),
            _ => return Err(self.expected("an expression")),
        };
        self.node(kind, Span::new(start, self.prev_end()), children)
    }

    /// `a: vcc`, the shorthand `gnd`, or `a = vcc` (reported, then read as `:`); with the
    /// value's height (0 for the shorthand).
    fn field(&mut self) -> PResult<(Field<'src>, u32)> {
        let name = self.name()?;
        let value = match self.peek() {
            TokenKind::Colon => {
                self.bump();
                Some(self.expr_bp(0)?)
            }
            TokenKind::Eq => {
                let eq = self.bump();
                self.errors
                    .push(ParseError::new(ParseErrorKind::FieldEquals, eq).with_fix(eq, ":"));
                Some(self.expr_bp(0)?)
            }
            _ => None,
        };
        let span = Span::new(name.span.start, self.prev_end());
        let height = value.as_ref().map_or(0, |v| v.height);
        let value = value.map(|v| v.expr);
        Ok((Field { name, value, span }, height))
    }

    /// A comma-separated list of expressions after `open`, and the tallest one's height
    /// (0 for none).
    fn exprs(&mut self, open: Span, close: TokenKind) -> PResult<(Vec<Expr<'src>>, u32)> {
        self.tallest_list(open, close, |p| {
            let item = p.expr_bp(0)?;
            Ok((item.expr, item.height))
        })
    }

    /// A [`list`](Parser::list) of items that each come with a height, and the tallest
    /// one's height (0 for none): what the node holding them is built on.
    fn tallest_list<T>(
        &mut self,
        open: Span,
        close: TokenKind,
        mut item: impl FnMut(&mut Self) -> PResult<(T, u32)>,
    ) -> PResult<(Vec<T>, u32)> {
        let mut tallest = 0;
        let items = self.list(open, close, |p| {
            let (item, height) = item(p)?;
            tallest = tallest.max(height);
            Ok(item)
        })?;
        Ok((items, tallest))
    }

    /// A node one level above its tallest child (`children` is that child's height, 0 for
    /// a leaf). `TooDeep` past `MAX_TREE_DEPTH`.
    fn node(&mut self, kind: ExprKind<'src>, span: Span, children: u32) -> PResult<Tree<'src>> {
        let height = children + 1;
        if height > MAX_TREE_DEPTH {
            return Err(self.fail(ParseErrorKind::TooDeep));
        }
        Ok(Tree {
            expr: Expr { kind, span },
            height,
        })
    }
}

/// The operands of `e` if it's an unparenthesized `+ - * /`.
fn arithmetic<'a, 'src>(e: &'a Expr<'src>) -> Option<(&'a Expr<'src>, &'a Expr<'src>)> {
    match &e.kind {
        ExprKind::Binary {
            op: BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div,
            lhs,
            rhs,
        } => Some((lhs, rhs)),
        _ => None,
    }
}

fn is_arithmetic(e: &Expr) -> bool {
    arithmetic(e).is_some()
}

/// A range or a tolerance.
fn is_spread(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Binary {
            op: BinOp::Range | BinOp::Tol,
            ..
        }
    )
}

/// The last term of an arithmetic chain: `c` in `a + b * c`, `e` itself if it isn't one.
fn last_term<'a, 'src>(mut e: &'a Expr<'src>) -> &'a Expr<'src> {
    while let Some((_, rhs)) = arithmetic(e) {
        e = rhs;
    }
    e
}

/// The first term of an arithmetic chain: `a` in `a * b + c`, `e` itself if it isn't one.
fn first_term<'a, 'src>(mut e: &'a Expr<'src>) -> &'a Expr<'src> {
    while let Some((lhs, _)) = arithmetic(e) {
        e = lhs;
    }
    e
}

#[cfg(test)]
mod tests {
    use crate::parser::{ParseErrorKind, parse};

    fn too_deep(expr: &str) -> bool {
        let src = format!("block B {{ let x = {expr}; }}");
        let parsed = parse(&src);
        assert!(parsed.lex_errors.is_empty());
        match parsed.errors.as_slice() {
            [] => false,
            [e] if e.kind == ParseErrorKind::TooDeep => true,
            other => panic!("unexpected errors {other:?}"),
        }
    }

    /// The limit is on the tree's height, exactly: `a + a + … + a` with n operators is
    /// n + 1 levels.
    #[test]
    fn height_limit_is_exact() {
        assert!(!too_deep(&format!("a{}", " + a".repeat(1023))));
        assert!(too_deep(&format!("a{}", " + a".repeat(1024))));
        // Each prefix and postfix operator is a level too.
        assert!(!too_deep(&format!(
            "-a{}{}",
            ".b".repeat(511),
            " + a".repeat(511)
        )));
        assert!(too_deep(&format!(
            "-a{}{}",
            ".b".repeat(511),
            " + a".repeat(512)
        )));
    }

    /// Red team (second review): the old limit counted the levels *above* the parse point,
    /// but a chain grows above operands it already parsed. In `a + (…) + a + … + a` every
    /// later `+` pushes `(…)` one level down, so trees reached 48 000 levels with no error
    /// and overflowed the stack when dropped.
    #[test]
    fn height_counts_levels_added_above_an_operand() {
        let inner = format!("a{}", " + a".repeat(600));
        assert!(too_deep(&format!("a + ({inner}){}", " + a".repeat(600))));
        let mut nested = "a".to_string();
        for _ in 0..60 {
            nested = format!("a + ({nested}){}", " + a".repeat(800));
        }
        assert!(too_deep(&nested));
    }
}
