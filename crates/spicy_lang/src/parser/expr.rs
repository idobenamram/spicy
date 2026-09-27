//! Expressions: a Pratt loop over the precedence table of grammar.md §4.1, plus the
//! shape rules of §4.2–4.3 (the `±` operand rule, no chaining, no spread in a range
//! endpoint or a comparison).

use super::ast::{BinOp, Expr, ExprKind, Field};
use super::{PResult, ParseError, ParseErrorKind, Parser};
use crate::lexer::{Span, TokenKind, decode_quantity};

/// Precedence levels, loosest first. Levels 1–3 don't associate: `a < b < c`,
/// `a..=b..=c` and `a ± b ± c` are errors.
const COMPARE: u8 = 1;
const RANGE: u8 = 2;
const TOL: u8 = 3;
const SUM: u8 = 4;
const PRODUCT: u8 = 5;

fn infix(kind: TokenKind) -> Option<(BinOp, u8)> {
    use TokenKind::*;
    Some(match kind {
        KwIn => (BinOp::In, COMPARE),
        Lt => (BinOp::Lt, COMPARE),
        Le => (BinOp::Le, COMPARE),
        Gt => (BinOp::Gt, COMPARE),
        Ge => (BinOp::Ge, COMPARE),
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

fn is_arithmetic(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Binary {
            op: BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div,
            ..
        }
    )
}

fn is_spread(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Binary {
            op: BinOp::Range | BinOp::Tol,
            ..
        }
    )
}

impl<'src> Parser<'_, 'src> {
    pub(super) fn expr(&mut self) -> PResult<Expr<'src>> {
        self.expr_bp(0)
    }

    /// Parses operators binding tighter than `min`.
    fn expr_bp(&mut self, min: u8) -> PResult<Expr<'src>> {
        self.nested(|p| p.binary(min))
    }

    /// The body of `expr_bp`: an operand, then operators while they bind tighter than
    /// `min`. Each chained operator is one more tree level (`deepen`).
    fn binary(&mut self, min: u8) -> PResult<Expr<'src>> {
        let mut lhs = self.unary()?;
        let mut prev_level = None;
        while let Some((op, level)) = infix(self.peek()) {
            if level <= min {
                break;
            }
            let token = self.peek();
            let op_span = self.bump();
            if level <= TOL && prev_level == Some(level) {
                self.error(ParseErrorKind::Chained { op }, op_span);
            }
            let rhs = self.expr_bp(level)?;
            // After the right side parsed, so `x in 1..;` doesn't get a fix to `1..=;`.
            if token == TokenKind::DotDot {
                self.errors.push(
                    ParseError::new(ParseErrorKind::HalfOpenRange, op_span)
                        .with_fix(op_span, "..="),
                );
            }
            self.deepen()?;
            self.check_shape(op, &lhs, &rhs);
            let span = Span::new(lhs.span.start, rhs.span.end);
            lhs = Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            };
            prev_level = Some(level);
        }
        Ok(lhs)
    }

    /// grammar.md §4.2 and §4.3. These are soft errors: the node is still built.
    fn check_shape(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) {
        match op {
            BinOp::Tol => {
                let src = self.src();
                let text = |e: &Expr| &src[e.span.range()];
                // Both readings, written out. The "one term" reading attaches `±` to the
                // term right next to it, so the suggestion isn't ambiguous itself:
                // `a + b * c ± 1%` → `a + b * (c ± 1%)`.
                let readings = if is_arithmetic(lhs) {
                    let term = adjacent_term(lhs, true);
                    let before = &src[lhs.span.start as usize..term.span.start as usize];
                    Some((
                        lhs.span,
                        format!("({}) ± {}", text(lhs), text(rhs)),
                        format!("{before}({} ± {})", text(term), text(rhs)),
                    ))
                } else if is_arithmetic(rhs) {
                    let term = adjacent_term(rhs, false);
                    let after = &src[term.span.end as usize..rhs.span.end as usize];
                    Some((
                        rhs.span,
                        format!("{} ± ({})", text(lhs), text(rhs)),
                        format!("({} ± {}){after}", text(lhs), text(term)),
                    ))
                } else {
                    None
                };
                if let Some((span, whole, part)) = readings {
                    self.error(ParseErrorKind::AmbiguousTolerance { whole, part }, span);
                }
            }
            BinOp::Range => {
                for end in [lhs, rhs] {
                    if matches!(end.kind, ExprKind::Binary { op: BinOp::Tol, .. }) {
                        self.error(ParseErrorKind::ToleranceInRange, end.span);
                    }
                }
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                for side in [lhs, rhs] {
                    if is_spread(side) {
                        self.error(ParseErrorKind::RangeInComparison, side.span);
                    }
                }
            }
            // `x in 1..=2` is the point of `in`; `1..=2 in x` has it backwards.
            BinOp::In if is_spread(lhs) => {
                self.error(ParseErrorKind::RangeInComparison, lhs.span);
            }
            _ => {}
        }
    }

    /// `-x`, or a postfix expression.
    fn unary(&mut self) -> PResult<Expr<'src>> {
        let Some(minus) = self.eat(TokenKind::Minus) else {
            return self.postfix();
        };
        // Counted but not released here: the levels under this `-` stay counted until the
        // enclosing expression ends, so operators chained after it add on top of them.
        self.nest_without_scope()?;
        let operand = self.unary()?;
        let span = Span::new(minus.start, operand.span.end);
        Ok(Expr {
            kind: ExprKind::Neg(Box::new(operand)),
            span,
        })
    }

    /// A primary followed by any number of `.field` and `(args)`. Each one is one more
    /// tree level, released only when the enclosing expression ends (`expr_bp`'s
    /// `nested`), so that operators chained after it count on top: in
    /// `a.b.….b + a + … + a` the leftmost leaf is as deep as both chains together.
    fn postfix(&mut self) -> PResult<Expr<'src>> {
        let mut e = self.primary()?;
        loop {
            if matches!(self.peek(), TokenKind::Dot | TokenKind::LParen) {
                self.deepen()?;
            }
            match self.peek() {
                TokenKind::Dot => {
                    self.bump();
                    let name = self.name()?;
                    let span = Span::new(e.span.start, name.span.end);
                    e = Expr {
                        kind: ExprKind::Field {
                            base: Box::new(e),
                            name,
                        },
                        span,
                    };
                }
                TokenKind::LParen => {
                    let open = self.bump();
                    let args = self.list(open, TokenKind::RParen, Self::expr)?;
                    let span = Span::new(e.span.start, self.prev_end());
                    e = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(e),
                            args,
                        },
                        span,
                    };
                }
                _ => return Ok(e),
            }
        }
    }

    fn primary(&mut self) -> PResult<Expr<'src>> {
        let start = self.span().start;
        let kind = match self.peek() {
            TokenKind::Quantity => {
                let text = self.text();
                self.bump();
                // The lexer already reported a quantity that doesn't decode.
                match decode_quantity(text) {
                    Ok(q) => ExprKind::Quantity(q),
                    Err(_) => ExprKind::Error,
                }
            }
            TokenKind::Ident => {
                let path = self.path()?;
                match self.eat(TokenKind::LBrace) {
                    Some(open) => {
                        let fields = self.list(open, TokenKind::RBrace, Self::field)?;
                        ExprKind::StructLit { path, fields }
                    }
                    None => ExprKind::Path(path),
                }
            }
            TokenKind::LParen => {
                let open = self.bump();
                let inner = self.expr()?;
                if self.eat(TokenKind::RParen).is_none() {
                    return Err(self.unclosed_or_expected(open, ")", "`)`"));
                }
                ExprKind::Paren(Box::new(inner))
            }
            TokenKind::LBracket => {
                let open = self.bump();
                ExprKind::Array(self.list(open, TokenKind::RBracket, Self::expr)?)
            }
            TokenKind::KwReserved => return Err(self.reserved()),
            _ => return Err(self.expected("an expression")),
        };
        Ok(Expr {
            kind,
            span: Span::new(start, self.prev_end()),
        })
    }

    /// `a: vcc`, the shorthand `gnd`, or `a = vcc` (reported, then read as `:`).
    fn field(&mut self) -> PResult<Field<'src>> {
        let name = self.name()?;
        let value = match self.peek() {
            TokenKind::Colon => {
                self.bump();
                Some(self.expr()?)
            }
            TokenKind::Eq => {
                let eq = self.bump();
                self.errors
                    .push(ParseError::new(ParseErrorKind::FieldEquals, eq).with_fix(eq, ":"));
                Some(self.expr()?)
            }
            _ => None,
        };
        let span = Span::new(name.span.start, self.prev_end());
        Ok(Field { name, value, span })
    }
}

/// The operand of an arithmetic chain that sits next to an outside operator: the
/// rightmost term of `e` (`right`) or its leftmost.
fn adjacent_term<'a, 'src>(mut e: &'a Expr<'src>, right: bool) -> &'a Expr<'src> {
    while let ExprKind::Binary {
        op: BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div,
        lhs,
        rhs,
    } = &e.kind
    {
        e = if right { rhs } else { lhs };
    }
    e
}
