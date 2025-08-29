use crate::netlist_types::{CommandType, ValueSuffix};
// param_phase.rs
use crate::expr::{Expr, ParamEnv, PlaceholderMap};
use crate::lexer::{Span, Token, TokenKind, token_text};
use crate::expr::Value;
use crate::statement_phase::{Statement, StatementStream, StmtCursor};

#[derive(Debug)]
pub struct ParamPhase<'a> {
    pub env: ParamEnv<'a>,            // root param env
    pub placeholders: PlaceholderMap, // id -> (Expr, span)
    pub stmts: Vec<Statement>,        // stream after param pass
}

impl<'a> ParamPhase<'a> {
    pub fn new(input: &str) -> Self {
        let mut ss = StatementStream::new(input);
        let mut env = ParamEnv::new_root();
        let mut placeholders = PlaceholderMap::default();
        let mut out = Vec::new();

        while let Some(mut stmt) = ss.next() {
            // Skip blanks
            let mut cursor = stmt.into_cursor();

            // .param NAME = expr
            if cursor.consume_if_command(input, CommandType::Param) {
                parse_dot_param(&mut cursor, input, &mut env);
                // store in env; we don’t emit the .param line to downstream by default
                continue;
            }

            // // .if EXPR  / .else / .endif  (very small scaffold)
            // if is_command(&cursor, input, CommandType::If) {
            //     // let keep = eval_if_true(&mut stmt, input, &env); // implement as you like
            //     // if !keep { out.push(ParamProcessed::Pruned); }
            //     // continue;
            //     todo!("if command not implemented");
            // }
            // if is_command(&cursor, input, CommandType::Else) || is_command(&cursor, input, CommandType::Endif) {
            //     // handle block structure in a fuller implementation
            //     continue;
            // }

            // Replace { … } with placeholders in this statement
            brace_to_placeholders(&mut stmt, input, &mut placeholders);
            out.push(stmt);
        }

        Self {
            env,
            placeholders,
            stmts: out,
        }
    }
}

fn parse_value(cursor: &mut StmtCursor, src: &str) -> Value {
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

fn parse_ident(cursor: &mut StmtCursor, src: &str) -> String {
    let ident = cursor.next().expect("Must be ident");
    assert_eq!(ident.kind, TokenKind::Ident);
    token_text(src, ident).to_string()
}

fn parse_equal_expr(cursor: &mut StmtCursor, src: &str) -> (String, Value) {
    let ident = parse_ident(cursor, src);
    let equal = cursor.next().expect("Must be equal");
    assert_eq!(equal.kind, TokenKind::Equal);
    // TODO: support expressions here
    let value = parse_value(cursor, src);
    (ident, value)
}

fn parse_dot_param<'a>(cursor: &mut StmtCursor, src: &str, env: &mut ParamEnv<'a>) {
    // Consume ".param" IDENT '=' expr-tokens
    // Build Expr with parse_expr(); store in env.map.insert(name, expr)

    while let Some(token) = cursor.next() {
        if token.kind != TokenKind::WhiteSpace {
            println!("warning: unexpected token: {:?}", token);
            break;
        }
        let (ident, value) = parse_equal_expr(cursor, src);
        env.map.insert(ident, Expr::Value(value));
    }
    assert!(cursor.done(), "Expected end of statement");
}

/// Walk tokens, when seeing '{', collect until matching '}', parse inside to Expr,
/// allocate PlaceholderId and push a single Placeholder token instead.
fn brace_to_placeholders(statement: &mut Statement, src: &str, pm: &mut PlaceholderMap) {
    let mut cursor = statement.into_cursor();
    let mut replacements = Vec::new();

    while let Some(tok) = cursor.next() {
        if tok.kind == TokenKind::LeftBracket {
            let start_pos = cursor.pos() - 1;
            let start_span = tok.span.start;
            let ident = cursor.consume(TokenKind::Ident).expect("Expected ident");
            let ident_text = token_text(src, ident).to_string();
            let right_bracket = cursor.consume(TokenKind::RightBracket).expect("Expected }");
            let end_span = right_bracket.span.end;
            let end_pos = cursor.pos() - 1;
            let id = pm.fresh(Expr::Ident(ident_text), Span::new(start_span, end_span));
            replacements.push((start_pos, end_pos, Token::placeholder(id, Span::new(start_span, end_span))));
        }
    }

    println!("{:?}", statement);
    for (start_pos, end_pos, replacement) in replacements.into_iter().rev() {
        println!("replacment {:?}, start_pos: {}, end_pos: {}", replacement, start_pos, end_pos);
        statement.replace_tokens(start_pos, end_pos, vec![replacement]);
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use std::path::PathBuf;

    #[rstest]
    fn test_param_phase(#[files("tests/param_inputs/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");

        let output = ParamPhase::new(&input_content);

        let name = format!(
            "param-{}",
            input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        insta::assert_debug_snapshot!(name, output);
    }
}
