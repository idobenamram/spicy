use crate::Span;
use crate::error::{ParserError, SpicyError};
use crate::expr::{PlaceholderMap, Scope, Value};
use crate::expr::{Expr, Params};
use crate::lexer::{TokenKind, token_text};
use crate::netlist_types::Node;
use crate::netlist_types::ValueSuffix;
use crate::statement_phase::StmtCursor;

pub(crate) struct Ident<'a> {
    pub text: &'a str,
    pub span: Span,
}

pub(crate) fn parse_node(cursor: &mut StmtCursor, src: &str) -> Result<Node, SpicyError> {
    let node = cursor
        .next_non_whitespace()
        .ok_or_else(|| ParserError::MissingToken {
            message: "node",
            span: cursor.peek_span(),
        })?;

    if !matches!(node.kind, TokenKind::Ident | TokenKind::Number) {
        return Err(ParserError::UnexpectedToken {
            expected: "identifier or number".to_string(),
            found: node.kind,
            span: node.span,
        }
        .into());
    }
    let node_string = token_text(src, node).to_string();
    Ok(Node { name: node_string })
}

pub(crate) fn parse_value(cursor: &mut StmtCursor, src: &str) -> Result<Value, SpicyError> {
    let mut number_str = String::new();
    let mut exponent: Option<f64> = None;
    let mut suffix: Option<ValueSuffix> = None;

    let mut t = cursor
        .next_non_whitespace()
        .ok_or_else(|| ParserError::MissingToken {
            message: "Expected number or minus",
            span: cursor.peek_span(),
        })?;

    // Optional leading minus
    if matches!(t.kind, TokenKind::Minus) {
        number_str.push('-');
        t = cursor
            .next_non_whitespace()
            .ok_or_else(|| ParserError::MissingToken {
                message: "Expected digits or '.' after '-'",
                span: Some(t.span),
            })?;
    }

    // Integer digits or leading '.' with fraction
    match t.kind {
        TokenKind::Number => {
            number_str.push_str(token_text(src, t));
            // Optional fractional part if next immediate token is a dot
            if let Some(peek) = cursor.peek() {
                if matches!(peek.kind, TokenKind::Dot) {
                    cursor.next().expect("just check for dot");
                    number_str.push('.');

                    let frac =
                        cursor
                            .next_non_whitespace()
                            .ok_or_else(|| ParserError::MissingToken {
                                message: "Expected token after '.'",
                                span: Some(peek.span),
                            })?;

                    if !matches!(frac.kind, TokenKind::Number) {
                        return Err(ParserError::ExpectedDigitsAfterDot { span: peek.span }.into());
                    }
                    number_str.push_str(token_text(src, frac));
                }
            }
        }
        TokenKind::Dot => {
            number_str.push('.');
            let frac = cursor
                .next_non_whitespace()
                .ok_or_else(|| ParserError::MissingToken {
                    message: "Expected digits after '.'",
                    span: cursor.peek_span(),
                })?;
            if !matches!(frac.kind, TokenKind::Number) {
                return Err(ParserError::ExpectedDigitsAfterDot { span: frac.span }.into());
            }
            number_str.push_str(token_text(src, frac));
        }
        _ => {
            return Err(ParserError::InvalidStartNumeric { span: t.span }.into());
        }
    }

    // TODO: i don't think you can have a suffix and an exponent at the same time
    // TODO: also this can definitly use a cleanup
    // Optional exponent: e|E [+-]? digits (no whitespace inside the literal)
    if let Some(peek) = cursor.peek() {
        if matches!(peek.kind, TokenKind::Ident) {
            let ident_text = token_text(src, peek);
            if ident_text == "e" || ident_text == "E" {
                cursor.next().expect("just peeked");

                let mut exp_str = String::new();
                // optional sign
                if let Some(sign_peek) = cursor.peek() {
                    match sign_peek.kind {
                        TokenKind::Plus => {
                            let _ = cursor.next().expect("just peeked");
                            exp_str.push('+');
                        }
                        TokenKind::Minus => {
                            let _ = cursor.next().expect("just peeked");
                            exp_str.push('-');
                        }
                        _ => {}
                    }
                }
                let exp_digits = cursor.next().ok_or(ParserError::MissingToken {
                    message: "Expected digits after exponent",
                    span: Some(peek.span),
                })?;

                let exp_digits_str = token_text(src, exp_digits).to_string();
                if !matches!(exp_digits.kind, TokenKind::Number) {
                    return Err(ParserError::InvalidExponentDigits {
                        span: exp_digits.span,
                        lexeme: exp_digits_str,
                    }
                    .into());
                }
                exp_str.push_str(&exp_digits_str);

                exponent = Some(exp_str.parse::<f64>().map_err(|_| {
                    ParserError::InvalidExponentDigits {
                        span: exp_digits.span,
                        lexeme: exp_digits_str,
                    }
                })?);
            } else if ident_text.starts_with("e") || ident_text.starts_with("E") {
                cursor.next().expect("just peeked");
                // Split ident after 'e' or 'E' and assume it is the exponent digits
                let (_e_char, exp_digits_str) = ident_text.split_at(1);
                if exp_digits_str.is_empty() {
                    return Err(ParserError::MissingToken {
                        message: "Expected digits after exponent",
                        span: Some(peek.span),
                    }
                    .into());
                }
                exponent = Some(exp_digits_str.parse::<f64>().map_err(|_| {
                    ParserError::InvalidExponentDigits {
                        span: peek.span,
                        lexeme: exp_digits_str.to_string(),
                    }
                })?);
            }
        }
    }

    // Optional suffix as trailing identifier without whitespace
    if let Some(peek) = cursor.peek() {
        if matches!(peek.kind, TokenKind::Ident) {
            let ident = cursor.next().expect("just peeked");
            let ident_text = token_text(src, ident);
            suffix = ValueSuffix::from_str(ident_text);
        }
    }

    let value: f64 = number_str
        .parse()
        .map_err(|_| ParserError::InvalidNumericLiteral {
            span: cursor.peek_span(),
            lexeme: number_str,
        })?;

    Ok(Value {
        value,
        exponent,
        suffix,
    })
}

pub(crate) fn parse_usize(cursor: &mut StmtCursor, src: &str) -> Result<usize, SpicyError> {
    let usize_token = cursor.expect_non_whitespace(TokenKind::Number)?;
    let usize_text = token_text(src, usize_token);
    usize_text.parse::<usize>().map_err(|_| {
        ParserError::InvalidNumericLiteral {
            span: Some(usize_token.span),
            lexeme: usize_text.to_string(),
        }
        .into()
    })
}

pub(crate) fn parse_bool(cursor: &mut StmtCursor, src: &str) -> Result<bool, SpicyError> {
    let bool = cursor.expect_non_whitespace(TokenKind::Number)?;
    let bool_text = token_text(src, bool);
    match bool_text {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(ParserError::ExpectedBoolZeroOrOne { span: bool.span }.into()),
    }
}

pub(crate) fn parse_ident<'a>(
    cursor: &mut StmtCursor,
    src: &'a str,
) -> Result<Ident<'a>, SpicyError> {
    let ident = cursor.expect_non_whitespace(TokenKind::Ident)?;
    Ok(Ident {
        text: token_text(src, ident),
        span: ident.span,
    })
}

pub(crate) fn parse_expr_into_value(
    cursor: &mut StmtCursor,
    src: &str,
    placeholder_map: &PlaceholderMap,
    scope: &Scope,
) -> Result<Value, SpicyError> {
    cursor.skip_ws();
    if let Some(token) = cursor.consume(TokenKind::Placeholder) {
        let id = token.id.expect("must have a placeholder id");
        // TODO: maybe we can change the expression to only evaluate once
        let expr = placeholder_map.get(id).clone();
        let evaluated = expr.evaluate(scope)?;
        return Ok(evaluated);
    }
    Ok(parse_value(cursor, src)?)
}

pub(crate) fn parse_value_or_placeholder(
    cursor: &mut StmtCursor,
    src: &str,
) -> Result<Expr, SpicyError> {
    if let Some(placeholder) = cursor.consume(TokenKind::Placeholder) {
        return Ok(Expr::placeholder(placeholder.id.unwrap(), placeholder.span));
    }
    // TODO: i think value should just have a span
    let cursor_span = cursor
        .peek_span()
        .ok_or_else(|| ParserError::MissingToken {
            message: "Expected cursor span",
            span: cursor.peek_span(),
        })?;
    Ok(Expr::value(parse_value(cursor, src)?, cursor_span))
}

pub(crate) fn parse_equal_expr<'a>(
    cursor: &mut StmtCursor,
    src: &'a str,
) -> Result<(Ident<'a>, Expr), SpicyError> {
    let ident = parse_ident(cursor, src)?;
    cursor.expect(TokenKind::Equal)?;
    let value = parse_value_or_placeholder(cursor, src)?;
    Ok((ident, value))
}

// .param <ident>=<value> <ident>=<value> ...
pub(crate) fn parse_dot_param(
    cursor: &mut StmtCursor,
    src: &str,
    env: &mut Params,
) -> Result<(), SpicyError> {
    while let Some(token) = cursor.next() {
        if token.kind != TokenKind::WhiteSpace {
            println!("warning: unexpected token: {:?}", token);
            break;
        }
        let (ident, value) = parse_equal_expr(cursor, src)?;
        env.set_param(ident.text.to_string(), value);
    }
    assert!(cursor.done(), "Expected end of statement");
    Ok(())
}
