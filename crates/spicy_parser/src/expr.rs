use crate::error::{ExpressionError, SpicyError};
use crate::{
    lexer::{Span, Token, TokenKind, token_text},
    netlist_types::ValueSuffix,
    netlist_types::{Name, NoCase, NodeName},
    parser_utils::parse_value,
    statement_phase::StmtCursor,
};
use serde::Serialize;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::f64::consts::PI;

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

    pub fn angle_radians(&self, default_degrees: bool) -> f64 {
        let value = self.get_value();
        match self.suffix {
            Some(ValueSuffix::Degree) => value * PI / 180.0,
            Some(ValueSuffix::Radian) => value,
            _ => {
                if default_degrees {
                    value * PI / 180.0
                } else {
                    value
                }
            }
        }
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
    #[cfg_attr(test, serde(serialize_with = "serialize_sorted_map"))] HashMap<Name, Expr>,
);

impl Params {
    pub fn new() -> Self {
        Self::default()
    }
    /// The parameter named `name`, with its name as defined.
    pub fn lookup(&self, name: &NoCase) -> Option<(&Name, &Expr)> {
        self.0.get_key_value(name)
    }
    pub fn set_param(&mut self, name: &str, value: Expr) {
        self.0.insert(Name::new(name), value);
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
    /// The instance's ports, by name, and the nodes they connect to.
    #[cfg_attr(test, serde(serialize_with = "serialize_sorted_map"))]
    pub node_mapping: HashMap<Name, NodeName>,
}

impl Scope {
    pub fn new(
        instance_name: Option<String>,
        param_map: Params,
        node_mapping: HashMap<Name, NodeName>,
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

    /// Name of a device written as `name` inside this scope, e.g. `R1` in
    /// instance `X1` becomes `X1.R1`.
    pub(crate) fn get_device_name(&self, name: &str) -> String {
        if let Some(instance_name) = &self.instance_name {
            return format!("{}.{}", instance_name, name);
        }
        name.to_string()
    }

    /// Circuit node for a node written as `node` inside this scope.
    ///
    /// A subcircuit port maps to the node the instance connects it to. Ground
    /// stays global. Any other node is internal to this instance and gets the
    /// instance name as a prefix (`mid` in `X1` becomes `X1.mid`, as in ngspice),
    /// so two instances never share an internal node.
    pub(crate) fn get_node_name(&self, node: NodeName) -> NodeName {
        if let Some(actual) = self.node_mapping.get(NoCase::new(&node.0)) {
            return actual.clone();
        }
        match &self.instance_name {
            Some(instance_name) if !node.is_ground() => {
                NodeName(format!("{}.{}", instance_name, node.0))
            }
            _ => node,
        }
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
}

/// A scope, with what's needed to evaluate expressions written in it.
#[derive(Clone, Copy)]
pub(crate) struct ScopeRef<'a> {
    arena: &'a ScopeArena,
    placeholders: &'a PlaceholderMap,
    id: ScopeId,
}

impl<'a> ScopeRef<'a> {
    pub(crate) fn new(
        arena: &'a ScopeArena,
        placeholders: &'a PlaceholderMap,
        id: ScopeId,
    ) -> Self {
        Self {
            arena,
            placeholders,
            id,
        }
    }

    /// See [`Scope::get_device_name`].
    pub(crate) fn get_device_name(&self, name: &str) -> String {
        self.arena.get(self.id).get_device_name(name)
    }

    /// See [`Scope::get_node_name`].
    pub(crate) fn get_node_name(&self, node: NodeName) -> NodeName {
        self.arena.get(self.id).get_node_name(node)
    }

    /// The value of `expr`, written in this scope.
    pub(crate) fn evaluate(&self, expr: &Expr) -> Result<Value, SpicyError> {
        let mut evaluation = Evaluation {
            arena: self.arena,
            placeholders: self.placeholders,
            defining: Vec::new(),
        };
        evaluation.eval(expr, self.id)
    }
}

/// One expression evaluation.
///
/// A parameter is looked up in the scope the expression is written in, then
/// outward through the scopes that placed it, up to the globals, as ngspice
/// does (xpressn.c, `entrynb`). Its definition is evaluated in the scope that
/// defines it.
struct Evaluation<'a> {
    arena: &'a ScopeArena,
    placeholders: &'a PlaceholderMap,
    /// The parameter definitions being evaluated, innermost last. A name isn't
    /// defined in a scope until its own definition is evaluated, so lookups of
    /// it skip that scope meanwhile: in an instance, `rt={rt}` reads the `rt`
    /// of the scope that placed it (ngspice evaluates an instance's parameters
    /// in its new scope, xpressn.c, `nupa_subcktcall`).
    defining: Vec<(ScopeId, &'a NoCase)>,
}

impl Evaluation<'_> {
    fn eval(&mut self, expr: &Expr, scope: ScopeId) -> Result<Value, SpicyError> {
        match &expr.r#type {
            ExprType::Value(value) => Ok(value.clone()),
            // A placeholder stands for the expression that was in braces.
            ExprType::Placeholder(id) => {
                let placeholders = self.placeholders;
                self.eval(placeholders.get(*id), scope)
            }
            ExprType::Ident(name) => self.param(name, scope, expr.span),
            ExprType::Unary { op, operand } => match op {
                TokenKind::Minus => {
                    let value = self.eval(operand, scope)?;
                    Ok(Value::new(-value.get_value(), None, None))
                }
                _ => Err(ExpressionError::UnsupportedUnaryOperator {
                    op: *op,
                    span: expr.span,
                }
                .into()),
            },
            ExprType::Binary { op, left, right } => {
                let combine: fn(Value, Value) -> Value = match op {
                    TokenKind::Plus => |a, b| a + b,
                    TokenKind::Minus => |a, b| a - b,
                    TokenKind::Asterisk => |a, b| a * b,
                    TokenKind::Slash => |a, b| a / b,
                    _ => {
                        return Err(ExpressionError::UnsupportedBinaryOperator {
                            op: *op,
                            span: expr.span,
                        }
                        .into());
                    }
                };
                let left = self.eval(left, scope)?;
                let right = self.eval(right, scope)?;
                Ok(combine(left, right))
            }
        }
    }

    /// The value of parameter `name` as seen from `scope`.
    fn param(&mut self, name: &str, scope: ScopeId, span: Span) -> Result<Value, SpicyError> {
        let wanted = NoCase::new(name);
        let arena = self.arena;
        let mut skipped_own_definition = false;
        let mut current = Some(scope);
        while let Some(id) = current {
            let candidate = arena.get(id);
            if let Some((defined, definition)) = candidate.param_map.lookup(wanted) {
                let entry: (ScopeId, &NoCase) = (id, defined.borrow());
                if self.defining.contains(&entry) {
                    skipped_own_definition = true;
                } else {
                    self.defining.push(entry);
                    let value = self.eval(definition, id);
                    self.defining.pop();
                    return value;
                }
            }
            current = candidate.parent;
        }
        let name = name.to_string();
        Err(if skipped_own_definition {
            ExpressionError::CyclicParameter { name, span }
        } else {
            ExpressionError::UnknownIdentifier { name, span }
        }
        .into())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The scope of subcircuit instance `X1` whose port `top` is wired to `a`.
    fn instance_scope() -> Scope {
        let ports = HashMap::from([(Name::new("top"), NodeName("a".into()))]);
        Scope::new(Some("X1".into()), Params::new(), ports)
    }

    fn node(name: &str) -> NodeName {
        NodeName(name.into())
    }

    #[test]
    fn port_maps_to_the_connected_node() {
        assert_eq!(instance_scope().get_node_name(node("top")), node("a"));
    }

    #[test]
    fn internal_node_gets_the_instance_prefix() {
        assert_eq!(instance_scope().get_node_name(node("mid")), node("X1.mid"));
    }

    #[test]
    fn ground_stays_global_inside_an_instance() {
        assert_eq!(instance_scope().get_node_name(node("0")), node("0"));
    }

    #[test]
    fn top_level_names_are_unchanged() {
        let root = Scope::new(None, Params::new(), HashMap::new());
        assert_eq!(root.get_node_name(node("mid")), node("mid"));
        assert_eq!(root.get_device_name("R1"), "R1");
    }

    #[test]
    fn device_gets_the_instance_prefix() {
        assert_eq!(instance_scope().get_device_name("R1"), "X1.R1");
    }
}
