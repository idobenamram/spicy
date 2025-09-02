use crate::expr::Value;
use crate::expr::{Expr, Params};
use crate::lexer::{Span, TokenKind, token_text};
use crate::netlist_types::Node;
use crate::netlist_types::ValueSuffix;
use crate::statement_phase::StmtCursor;

pub(crate) fn parse_node(cursor: &mut StmtCursor, src: &str) -> Node {
    let node = cursor.next_non_whitespace().expect("Must be node");
    assert!(matches!(node.kind, TokenKind::Ident | TokenKind::Number));
    let node_string = token_text(src, node).to_string();
    Node { name: node_string }
}

pub(crate) fn parse_value(cursor: &mut StmtCursor, src: &str) -> Value {
    let mut number_str = String::new();
    let mut exponent: Option<f64> = None;
    let mut suffix: Option<ValueSuffix> = None;

    // Optional leading minus
    let mut t = cursor
        .next_non_whitespace()
        .expect("Must start with a value");
    if matches!(t.kind, TokenKind::Minus) {
        number_str.push('-');
        t = cursor
            .next_non_whitespace()
            .expect("Expected digits or '.' after '-'");
    }

    // Integer digits or leading '.' with fraction
    match t.kind {
        TokenKind::Number => {
            number_str.push_str(token_text(src, t));
            // Optional fractional part if next immediate token is a dot
            if let Some(peek) = cursor.peek() {
                if matches!(peek.kind, TokenKind::Dot) {
                    let _dot = cursor.next().unwrap();
                    number_str.push('.');
                    let frac = cursor
                        .next_non_whitespace()
                        .expect("Expected digits after '.'");
                    assert!(
                        matches!(frac.kind, TokenKind::Number),
                        "Expected digits after '.'"
                    );
                    number_str.push_str(token_text(src, frac));
                }
            }
        }
        TokenKind::Dot => {
            number_str.push('.');
            let frac = cursor
                .next_non_whitespace()
                .expect("Expected digits after '.'");
            assert!(
                matches!(frac.kind, TokenKind::Number),
                "Expected digits after '.'"
            );
            number_str.push_str(token_text(src, frac));
        }
        _ => panic!("Invalid start of numeric value"),
    }

    // Optional exponent: e|E [+-]? digits (no whitespace inside the literal)
    if let Some(peek) = cursor.peek() {
        if matches!(peek.kind, TokenKind::Ident) {
            let ident_text = token_text(src, peek);
            if ident_text == "e" || ident_text == "E" {
                let _e = cursor.next().unwrap();
                let mut exp_str = String::new();
                // optional sign
                if let Some(sign_peek) = cursor.peek() {
                    match sign_peek.kind {
                        TokenKind::Plus => {
                            let _ = cursor.next().unwrap();
                            exp_str.push('+');
                        }
                        TokenKind::Minus => {
                            let _ = cursor.next().unwrap();
                            exp_str.push('-');
                        }
                        _ => {}
                    }
                }
                let exp_digits = cursor.next().expect("Expected digits after exponent");
                assert!(matches!(exp_digits.kind, TokenKind::Number));
                exp_str.push_str(token_text(src, exp_digits));
                exponent = Some(exp_str.parse::<f64>().expect("Invalid exponent digits"));
            }
        }
    }

    // Optional suffix as trailing identifier without whitespace
    if let Some(peek) = cursor.peek() {
        if matches!(peek.kind, TokenKind::Ident) {
            let ident = cursor.next().unwrap();
            let ident_text = token_text(src, ident);
            suffix = ValueSuffix::from_str(ident_text);
        }
    }

    let value: f64 = number_str
        .parse()
        .unwrap_or_else(|_| panic!("Invalid numeric literal: {}", number_str));

    Value {
        value,
        exponent,
        suffix,
    }
}

pub(crate) fn parse_bool(cursor: &mut StmtCursor, src: &str) -> bool {
    let bool = cursor.next_non_whitespace().expect("Must be bool");
    assert_eq!(bool.kind, TokenKind::Number);
    let bool_text = token_text(src, bool);
    match bool_text {
        "0" => false,
        "1" => true,
        _ => panic!("expected '0' or '1'"),
    }
}

pub(crate) fn parse_ident(cursor: &mut StmtCursor, src: &str) -> String {
    let ident = cursor.next_non_whitespace().expect("Must be ident");
    assert_eq!(ident.kind, TokenKind::Ident);
    token_text(src, ident).to_string()
}

pub(crate) fn parse_value_or_placeholder(cursor: &mut StmtCursor, src: &str) -> Expr {
    if let Some(placeholder) = cursor.consume(TokenKind::Placeholder) {
        return Expr::placeholder(placeholder.id.unwrap(), placeholder.span);
    }
    // TODO: get the correct span
    Expr::value(parse_value(cursor, src), Span::new(0, 0))
}

pub(crate) fn parse_equal_expr(cursor: &mut StmtCursor, src: &str) -> (String, Expr) {
    let ident = parse_ident(cursor, src);
    let equal = cursor.next().expect("Must be equal");
    assert_eq!(equal.kind, TokenKind::Equal);
    let value = parse_value_or_placeholder(cursor, src);
    (ident, value)
}

// .param <ident>=<value> <ident>=<value> ...
pub(crate) fn parse_dot_param(cursor: &mut StmtCursor, src: &str, env: &mut Params) {
    while let Some(token) = cursor.next() {
        if token.kind != TokenKind::WhiteSpace {
            println!("warning: unexpected token: {:?}", token);
            break;
        }
        let (ident, value) = parse_equal_expr(cursor, src);
        env.set_param(ident, value);
    }
    assert!(cursor.done(), "Expected end of statement");
}
