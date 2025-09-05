use crate::error::{ParserError, SpicyError};
use crate::expr::{ExpressionParser, PlaceholderMap};
use crate::lexer::{Token, TokenKind};
use crate::statement_phase::{Statement, Statements};

pub fn substitute_expressions(
    statements: &mut Statements,
    input: &str,
) -> Result<PlaceholderMap, SpicyError> {
    let mut placeholders = PlaceholderMap::default();

    let mut iterator = statements.statements.iter_mut();
    while let Some(mut stmt) = iterator.next() {
        // Replace { … } with placeholders in this statement
        brace_to_placeholders(&mut stmt, input, &mut placeholders)?;
    }

    Ok(placeholders)
}

/// Walk tokens, when seeing '{', collect until matching '}', parse inside to Expr,
/// allocate PlaceholderId and push a single Placeholder token instead.
fn brace_to_placeholders(
    statement: &mut Statement,
    src: &str,
    pm: &mut PlaceholderMap,
) -> Result<(), SpicyError> {
    let mut cursor = statement.into_cursor();
    let mut replacements = Vec::new();

    while let Some(tok) = cursor.next() {
        if tok.kind == TokenKind::LeftBrace {
            let start_pos = cursor.pos() - 1;
            let mut found_right_brace = false;

            let mut expression_tokens = Vec::new();
            while let Some(tok) = cursor.next() {
                if tok.kind == TokenKind::RightBrace {
                    found_right_brace = true;
                    break;
                }
                expression_tokens.push(tok.clone());
            }

            if !found_right_brace {
                return Err(ParserError::UnmatchedBrace { span: tok.span })?;
            }

            let end_pos = cursor.pos() - 1;
            let parsed_expression =
                ExpressionParser::new(src, expression_tokens.as_slice()).parse()?;

            let expanded_span = parsed_expression.span.expand();
            let id = pm.fresh(parsed_expression);
            replacements.push((start_pos, end_pos, Token::placeholder(id, expanded_span)));
        }
    }

    for (start_pos, end_pos, replacement) in replacements.into_iter().rev() {
        println!(
            "replacement {:?}, start_pos: {}, end_pos: {}",
            replacement, start_pos, end_pos
        );
        statement.replace_tokens(start_pos, end_pos, vec![replacement]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json;

    use super::*;
    use std::path::PathBuf;

    #[rstest]
    fn test_expression_phase(#[files("tests/expression_inputs/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let mut statements = Statements::new(&input_content).expect("statements");

        let output = substitute_expressions(&mut statements, &input_content).expect("expressions");

        let name = format!(
            "expression-{}",
            input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        let json = serde_json::to_string_pretty(&output).expect("serialize output to json");
        insta::assert_snapshot!(name, json);
    }
}
