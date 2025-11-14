use crate::error::{ExpressionError, SpicyError};
use crate::{
    lexer::{Span, Token, TokenKind, token_text},
    netlist_types::Node,
    netlist_types::ValueSuffix,
    parser_utils::parse_value,
    statement_phase::StmtCursor,
};
use serde::Serialize;
use std::collections::HashMap;

#[cfg(test)]
use crate::test_utils::serialize_sorted_map;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Value {
    pub value: f64,
    pub exponent: Option<f64>,
    pub suffix: Option<ValueSuffix>,
}

impl Value {
    pub fn new(value: f64, exponent: Option<f64>, suffix: Option<ValueSuffix>) -> Self {
        Self {
            value,
            exponent,
            suffix,
        }
    }

    pub fn zero() -> Self {
        Self::new(0.0, None, None)
    }

    pub fn get_value(&self) -> f64 {
        let mut value = self.value;
        if let Some(exponent) = self.exponent {
            value *= 10.0f64.powf(exponent);
        }
        if let Some(suffix) = &self.suffix {
            value *= suffix.scale();
        }
        value
    }
}

// Arithmetic operations for Value using fully-scaled numeric values.
// Results are returned normalized without exponent or suffix.
use std::ops::{Add, Div, Mul, Sub};

impl Add for Value {
    type Output = Value;
    fn add(self, rhs: Value) -> Self::Output {
        Value::new(self.get_value() + rhs.get_value(), None, None)
    }
}

impl Sub for Value {
    type Output = Value;
    fn sub(self, rhs: Value) -> Self::Output {
        Value::new(self.get_value() - rhs.get_value(), None, None)
    }
}

impl Mul for Value {
    type Output = Value;
    fn mul(self, rhs: Value) -> Self::Output {
        Value::new(self.get_value() * rhs.get_value(), None, None)
    }
}

impl Div for Value {
    type Output = Value;
    fn div(self, rhs: Value) -> Self::Output {
        Value::new(self.get_value() / rhs.get_value(), None, None)
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum ExprType {
    Value(Value),
    Placeholder(PlaceholderId),
    Ident(String),
    Unary {
        op: TokenKind,
        operand: Box<Expr>,
    }, // +, -
    Binary {
        op: TokenKind,
        left: Box<Expr>,
        right: Box<Expr>,
    }, // + - * /
       // Add Call { fun, args } if you want sin(), etc.
}

#[derive(Debug, Clone, Serialize)]
pub struct Expr {
    pub span: Span,
    pub r#type: ExprType,
}

impl Expr {
    fn identifier(name: String, span: Span) -> Expr {
        Expr {
            span,
            r#type: ExprType::Ident(name),
        }
    }

    pub fn value(value: Value, span: Span) -> Expr {
        Expr {
            span,
            r#type: ExprType::Value(value),
        }
    }

    pub fn placeholder(id: PlaceholderId, span: Span) -> Expr {
        Expr {
            span,
            r#type: ExprType::Placeholder(id),
        }
    }

    fn unary(op: Token, operand: Expr) -> Expr {
        Expr {
            span: op.span,
            r#type: ExprType::Unary {
                op: op.kind,
                operand: Box::new(operand),
            },
        }
    }

    fn binary(op: TokenKind, lhs: Expr, rhs: Expr) -> Expr {
        Expr {
            // we assume lhs and rhs are both from the same source
            span: Span::new(lhs.span.start, rhs.span.end, lhs.span.source_index),
            r#type: ExprType::Binary {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            },
        }
    }
    pub fn expand(self) -> Expr {
        Expr {
            span: self.span.expand(),
            r#type: self.r#type.clone(),
        }
    }

    pub fn evaluate(self, scope: &Scope) -> Result<Value, SpicyError> {
        match self.r#type {
            ExprType::Value(value) => Ok(value),
            // TODO: support layered expressions with no loops
            ExprType::Placeholder(id) => Err(ExpressionError::UnevaluatablePlaceholder {
                id,
                span: self.span,
            }
            .into()),
            ExprType::Ident(name) => {
                let Some(expr) = scope.param_map.get_param(&name).cloned() else {
                    return Err(ExpressionError::UnknownIdentifier {
                        name,
                        span: self.span,
                    }
                    .into());
                };
                expr.evaluate(scope)
            }
            ExprType::Unary { op, operand } => match op {
                TokenKind::Minus => {
                    let value = operand.evaluate(scope)?;
                    Ok(Value::new(-value.get_value(), None, None))
                }
                _ => Err(ExpressionError::UnsupportedUnaryOperator {
                    op,
                    span: self.span,
                }
                .into()),
            },
            ExprType::Binary { op, left, right } => match op {
                TokenKind::Plus => {
                    let left_value = left.evaluate(scope)?;
                    let right_value = right.evaluate(scope)?;
                    Ok(left_value + right_value)
                }
                TokenKind::Minus => {
                    let left_value = left.evaluate(scope)?;
                    let right_value = right.evaluate(scope)?;
                    Ok(left_value - right_value)
                }
                TokenKind::Asterisk => {
                    let left_value = left.evaluate(scope)?;
                    let right_value = right.evaluate(scope)?;
                    Ok(left_value * right_value)
                }
                TokenKind::Slash => {
                    let left_value = left.evaluate(scope)?;
                    let right_value = right.evaluate(scope)?;
                    Ok(left_value / right_value)
                }
                _ => Err(ExpressionError::UnsupportedBinaryOperator {
                    op,
                    span: self.span,
                }
                .into()),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Ord, PartialOrd, PartialEq, Eq, Hash, Serialize)]
pub struct PlaceholderId(u64);

#[derive(Debug, Default, Serialize)]
pub struct PlaceholderMap {
    pub(crate) next: u64,
    pub(crate) map: Vec<Expr>,
}

impl PlaceholderMap {
    pub fn fresh(&mut self, expr: Expr) -> PlaceholderId {
        let id = PlaceholderId(self.next);
        self.next += 1;
        self.map.push(expr);
        id
    }

    pub fn get(&self, id: PlaceholderId) -> &Expr {
        // techinically you can unwrap here
        self.map.get(id.0 as usize).expect("id should be in map")
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Params(
    #[cfg_attr(test, serde(serialize_with = "serialize_sorted_map"))] HashMap<String, Expr>,
);

impl Params {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get_param(&self, k: &str) -> Option<&Expr> {
        self.0.get(k)
    }
    pub fn set_param(&mut self, k: String, v: Expr) {
        self.0.insert(k, v);
    }
    pub fn merge(&mut self, other: Params) {
        self.0.extend(other.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ScopeId(usize);

#[derive(Debug, Clone, Serialize)]
pub struct Scope {
    pub parent: Option<ScopeId>,
    pub instance_name: Option<String>,
    pub param_map: Params, // store Expr; evaluation is later
    #[cfg_attr(test, serde(serialize_with = "crate::test_utils::serialize_node_map"))]
    pub node_mapping: HashMap<Node, Node>,
}

impl Scope {
    pub fn new(
        instance_name: Option<String>,
        param_map: Params,
        node_mapping: HashMap<Node, Node>,
    ) -> Self {
        Self {
            parent: None,
            instance_name,
            param_map,
            node_mapping,
        }
    }

    pub(crate) fn set_parent(&mut self, parent: ScopeId) {
        self.parent = Some(parent);
    }

    pub(crate) fn get_device_name(&self, name: &str) -> String {
        if let Some(instance_name) = &self.instance_name {
            return format!("{}_{}", instance_name, name);
        }
        name.to_string()
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ScopeArena {
    nodes: Vec<Scope>,
}

impl ScopeArena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_root(&mut self) -> (&mut Scope, ScopeId) {
        let id = ScopeId(self.nodes.len());
        self.nodes.push(Scope {
            parent: None,
            instance_name: None,
            param_map: Default::default(),
            node_mapping: Default::default(),
        });
        (self.get_mut(id), id)
    }

    pub fn new_child(&mut self, parent: ScopeId, mut env: Scope) -> ScopeId {
        let id = ScopeId(self.nodes.len());
        env.set_parent(parent);
        self.nodes.push(env);
        id
    }

    pub fn get(&self, id: ScopeId) -> &Scope {
        self.nodes
            .get(id.0)
            .expect("scopeId only created by this arena")
    }

    pub fn get_mut(&mut self, id: ScopeId) -> &mut Scope {
        self.nodes
            .get_mut(id.0)
            .expect("scopeId only created by this arena")
    }

    /// Get by key, walking up parents until found (rootward)
    #[allow(dead_code)]
    pub fn get_param_in_scope(&self, id: ScopeId, key: &str) -> Option<&Expr> {
        let mut cur = Some(id);
        while let Some(eid) = cur {
            let scope = self.nodes.get(eid.0)?;
            if let Some(v) = scope.param_map.0.get(key) {
                return Some(v);
            }
            cur = scope.parent;
        }
        None
    }
}

// mini partt parser

fn prefix_binding_power(op: &Token) -> ((), u8) {
    match op.kind {
        TokenKind::Minus => ((), 7),
        _ => panic!("bad prefix operator: {:?}", op),
    }
}

fn infix_binding_power(op: &TokenKind) -> Option<(u8, u8)> {
    match op {
        TokenKind::Plus | TokenKind::Minus => Some((3, 4)),
        // multiplication and division
        TokenKind::Asterisk | TokenKind::Slash => Some((5, 6)),
        _ => None,
    }
}

pub(crate) struct ExpressionParser<'s> {
    input: &'s str,
    expression_cursor: StmtCursor<'s>,
}

impl<'s> ExpressionParser<'s> {
    pub(crate) fn new(input: &'s str, tokens: &'s [Token]) -> Self {
        // todo: can we assume all tokens are from the source index?
        let source_index = tokens[0].span.source_index;
        let span = Span::new(
            tokens[0].span.start,
            tokens[tokens.len() - 1].span.end,
            source_index,
        );
        ExpressionParser {
            input,
            expression_cursor: StmtCursor::new(tokens, span),
        }
    }

    pub(crate) fn parse(&mut self) -> Result<Expr, SpicyError> {
        self.parse_expr(0)
    }

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, SpicyError> {
        let checkpoint = self.expression_cursor.checkpoint();
        let token = self.expression_cursor.next_non_whitespace();

        let mut lhs = match token {
            Some(t) if t.kind == TokenKind::Ident => {
                let name = token_text(self.input, t).to_string();
                Expr::identifier(name, t.span)
            }
            Some(t) if t.kind == TokenKind::Number => {
                // kinda weird but, rewind to before we parsed the number then give it to parse_value
                self.expression_cursor.rewind(checkpoint);
                let value = parse_value(&mut self.expression_cursor, self.input)?;
                Expr::value(value, t.span)
            }
            Some(t) if t.kind == TokenKind::LeftParen => {
                let lhs = self.parse_expr(0)?;
                self.expression_cursor.expect(TokenKind::RightParen)?;
                // expand to include the parentheses
                lhs.expand()
            }
            Some(t) if t.kind == TokenKind::Minus => {
                let ((), r_bp) = prefix_binding_power(t);
                let rhs = self.parse_expr(r_bp)?;
                Expr::unary(*t, rhs)
            }
            Some(t) => {
                return Err(ExpressionError::UnexpectedToken {
                    found: t.kind,
                    span: t.span,
                }
                .into());
            }
            None => {
                return Err(ExpressionError::MissingToken {
                    message: "no token",
                }
                .into());
            }
        };

        loop {
            let op = match self.expression_cursor.peek_non_whitespace() {
                Some(t)
                    if matches!(
                        t.kind,
                        TokenKind::Asterisk | TokenKind::Plus | TokenKind::Minus | TokenKind::Slash
                    ) =>
                {
                    t
                }
                Some(t) if t.kind.ident_or_numeric() => t,
                Some(t) => {
                    return Err(ExpressionError::UnexpectedToken {
                        found: t.kind,
                        span: t.span,
                    }
                    .into());
                }
                None => break,
            };

            // in the case of no operator, we should assume multiplication
            if op.kind == TokenKind::LeftParen || op.kind.ident_or_numeric() {
                let (l_bp, r_bp) = infix_binding_power(&TokenKind::Asterisk)
                    .expect("multiplication is an infix operator");
                if l_bp < min_bp {
                    break;
                }

                let rhs = self.parse_expr(r_bp)?;
                lhs = Expr::binary(TokenKind::Asterisk, lhs, rhs);
                continue;
            }

            if let Some((l_bp, r_bp)) = infix_binding_power(&op.kind) {
                if l_bp < min_bp {
                    break;
                }
                self.expression_cursor
                    .next_non_whitespace()
                    .expect("already peeked");

                let rhs = self.parse_expr(r_bp)?;
                lhs = Expr::binary(op.kind, lhs, rhs);
                continue;
            }

            break;
        }

        Ok(lhs)
    }
}
