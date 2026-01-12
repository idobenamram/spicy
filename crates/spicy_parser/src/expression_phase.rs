use crate::error::{ParserError, SpicyError};
use crate::expr::{ExpressionParser, PlaceholderMap};
use crate::lexer::{Token, TokenKind};
use crate::statement_phase::{Statement, Statements};
use crate::{ParseOptions, Span};

pub fn substitute_expressions(
    statements: &mut Statements,
    input: &ParseOptions,
) -> Result<PlaceholderMap, SpicyError> {
    let mut placeholders = PlaceholderMap::default();

    let iterator = statements.statements.iter_mut();
    for stmt in iterator {
        // Replace { … } with placeholders in this statement
        brace_to_placeholders(stmt, input, &mut placeholders)?;
    }

    Ok(placeholders)
}

/// Walk tokens, when seeing '{', collect until matching '}', parse inside to Expr,
/// allocate PlaceholderId and push a single Placeholder token instead.
fn brace_to_placeholders(
    statement: &mut Statement,
    input: &ParseOptions,
    pm: &mut PlaceholderMap,
) -> Result<(), SpicyError> {
    let mut cursor = statement.into_cursor();
    let mut replacements = Vec::new();

    while let Some(tok) = cursor.next() {
        if tok.kind == TokenKind::LeftBrace {
            let start_pos = cursor.pos() - 1;
            let mut right_brace = None;

            let mut expression_tokens = Vec::new();
            while let Some(tok) = cursor.next() {
                if tok.kind == TokenKind::RightBrace {
                    right_brace = Some(tok);
                    break;
                }
                expression_tokens.push(*tok);
            }

            let Some(right_brace) = right_brace else {
                return Err(ParserError::UnmatchedBrace { span: tok.span })?;
            };

            if expression_tokens.is_empty()
                || expression_tokens
                    .iter()
                    .all(|t| t.kind == TokenKind::WhiteSpace)
            {
                // we found a {} with nothing inside
                Err(ParserError::EmptyExpressionInsideBraces {
                    span: Span::new(tok.span.start, right_brace.span.end, tok.span.source_index),
                })?;
            }

            let end_pos = cursor.pos() - 1;
            let src = &input.source_map.get_content(tok.span.source_index);
            let parsed_expression =
                ExpressionParser::new(src, expression_tokens.as_slice()).parse()?;

            let expanded_span = parsed_expression.span.expand();
            let id = pm.fresh(parsed_expression);
            replacements.push((start_pos, end_pos, Token::placeholder(id, expanded_span)));
        }
    }

    for (start_pos, end_pos, replacement) in replacements.into_iter().rev() {
        statement.replace_tokens(start_pos, end_pos, vec![replacement]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::SourceMap;
    use crate::libs_phase::SourceFileId;
    use std::path::PathBuf;

    #[rstest]
    fn test_expression_phase(#[files("tests/expression_inputs/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let source_map = SourceMap::new(input.clone(), input_content.clone());
        let input_options = ParseOptions {
            source_map,
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            max_include_depth: 10,
        };
        let mut statements =
            Statements::new(&input_content, SourceFileId::new(0)).expect("statements");

        let output = substitute_expressions(&mut statements, &input_options).expect("expressions");

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

    #[test]
    fn test_empty_expression_in_braces() {
        let input = "R1 N001 N002 { } 1k";
        let source_map = SourceMap::new(".".into(), input.to_string());
        let input_options = ParseOptions {
            source_map,
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            max_include_depth: 10,
        };
        let mut statements = Statements::new(input, SourceFileId::new(0)).expect("statements");

        let err = substitute_expressions(&mut statements, &input_options).unwrap_err();
        let err = match err {
            SpicyError::Parser(e) => e,
            _ => panic!("expected parser error"),
        };
        match err {
            ParserError::EmptyExpressionInsideBraces { span } => {
                // Make sure we include the entire `{ }` in the span.
                assert_eq!(span.start, 13);
                assert_eq!(span.end, 15);
                assert_eq!(span.source_index, input_options.source_map.main_index());
            }
            _ => panic!("expected EmptyExpressionInsideBraces"),
        }
    }
}
