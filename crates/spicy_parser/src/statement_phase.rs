use crate::error::{ParserError, SpicyError};
use crate::libs_phase::SourceFileId;
use crate::{
    lexer::{Lexer, Span, Token, TokenKind, token_text},
    netlist_types::{CommandType, DeviceType},
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Statement {
    pub(crate) tokens: Vec<Token>,
    pub span: Span,
}

impl Statement {
    fn new(tokens: Vec<Token>) -> Result<Self, ParserError> {
        if tokens.is_empty() {
            return Err(ParserError::EmptyStatement);
        }

        let start = tokens[0].span.start;
        let end = tokens[tokens.len() - 1].span.end;
        // we assume all tokens are from the same span
        let source_index = tokens[0].span.source_index;

        Ok(Self {
            span: Span::new(start, end, source_index),
            tokens,
        })
    }

    pub(crate) fn as_cursor(&self) -> StmtCursor<'_> {
        StmtCursor::new(&self.tokens, self.span)
    }

    pub(crate) fn replace_tokens(&mut self, start: usize, end: usize, tokens: Vec<Token>) {
        self.tokens.splice(start..=end, tokens);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StmtCursor<'a> {
    pub span: Span,
    pub toks: &'a [Token],
    pub i: usize,
}

impl<'a> StmtCursor<'a> {
    pub(crate) fn new(tokens: &'a [Token], span: Span) -> Self {
        Self {
            toks: tokens,
            i: 0,
            span,
        }
    }

    #[inline]
    pub(crate) fn done(&self) -> bool {
        self.i >= self.toks.len()
    }

    #[inline]
    pub(crate) fn pos(&self) -> usize {
        self.i
    }

    #[inline]
    pub(crate) fn checkpoint(&self) -> usize {
        self.pos()
    }

    #[inline]
    pub(crate) fn rewind(&mut self, mark: usize) {
        self.i = mark;
    }

    pub(crate) fn skip_ws(&mut self) {
        while let Some(t) = self.toks.get(self.i) {
            if t.kind != TokenKind::WhiteSpace {
                break;
            }
            self.i += 1;
        }
    }

    #[inline]
    pub(crate) fn peek(&self) -> Option<&'a Token> {
        self.toks.get(self.i)
    }

    /// Peek skipping whitespace
    pub(crate) fn peek_non_whitespace(&self) -> Option<&'a Token> {
        let mut j = self.i;
        while let Some(t) = self.toks.get(j) {
            if t.kind != TokenKind::WhiteSpace {
                return Some(t);
            }
            j += 1;
        }
        None
    }

    pub(crate) fn peek_span(&self) -> Option<Span> {
        Some(self.peek()?.span)
    }

    pub(crate) fn next(&mut self) -> Option<&'a Token> {
        let t = self.toks.get(self.i);
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    pub(crate) fn next_non_whitespace(&mut self) -> Option<&'a Token> {
        self.skip_ws();
        self.next()
    }
    /// Consume a specific kind (skips ws first)
    pub(crate) fn consume(&mut self, kind: TokenKind) -> Option<&'a Token> {
        if self.peek()?.kind == kind {
            return self.next();
        }
        None
    }
    /// Expect a specific kind (nice for errors)
    pub(crate) fn expect(&mut self, kind: TokenKind) -> Result<&'a Token, SpicyError> {
        if let Some(tok) = self.peek() {
            if tok.kind == kind {
                return Ok(self.next().unwrap());
            }
            return Err(ParserError::UnexpectedToken {
                expected: format!("{:?}", kind),
                found: tok.kind,
                span: tok.span,
            }
            .into());
        }
        Err(ParserError::MissingToken {
            message: "token",
            span: self.peek_span(),
        }
        .into())
    }

    pub(crate) fn expect_non_whitespace(
        &mut self,
        kind: TokenKind,
    ) -> Result<&'a Token, SpicyError> {
        self.skip_ws();
        self.expect(kind)
    }

    pub(crate) fn consume_if_command(&mut self, input: &str, command: CommandType) -> bool {
        let checkpoint = self.checkpoint();
        self.skip_ws();
        if self.consume(TokenKind::Dot).is_some()
            && let Some(kind) = self.consume(TokenKind::Ident)
        {
            let found_command = token_text(input, kind).parse::<CommandType>().ok() == Some(command);
            if found_command {
                return true;
            }
        }
        self.rewind(checkpoint);
        false
    }

    pub(crate) fn consume_if_commands(
        &mut self,
        input: &str,
        commands: &[CommandType],
    ) -> Option<CommandType> {
        let checkpoint = self.checkpoint();
        self.skip_ws();
        if self.consume(TokenKind::Dot).is_some()
            && let Some(kind) = self.consume(TokenKind::Ident)
        {
            let command_type = token_text(input, kind).parse::<CommandType>().ok();
            for command in commands {
                if command_type == Some(*command) {
                    return Some(*command);
                }
            }
        }
        self.rewind(checkpoint);
        None
    }

    pub(crate) fn consume_if_device(
        &mut self,
        input: &'a str,
        device: DeviceType,
    ) -> Option<&'a str> {
        let checkpoint = self.checkpoint();
        self.skip_ws();
        if let Some(t) = self.consume(TokenKind::Ident) {
            let ident_string = token_text(input, t);
            // Identifiers can be UTF-8; don't use byte offsets.
            let mut chars = ident_string.chars();
            let Some(first) = chars.next() else {
                self.rewind(checkpoint);
                return None;
            };
            let name = chars.as_str();
            // TODO: not sure this is correct
            let found_device = DeviceType::from_char(first).ok() == Some(device);
            if found_device {
                return Some(name);
            }
        }
        self.rewind(checkpoint);
        None
    }

    pub(crate) fn contains(&self, kind: TokenKind) -> bool {
        self.toks.iter().any(|t| t.kind == kind)
    }

    pub(crate) fn split_on_whitespace(&self) -> Vec<StmtCursor<'a>> {
        let mut result = Vec::new();
        let mut start = self.i;

        for (offset, tok) in self.toks[self.i..].iter().enumerate() {
            let idx = self.i + offset;
            if matches!(tok.kind, TokenKind::WhiteSpace) {
                if start < idx {
                    // assume all tokens are from the same source index
                    let span_start = self.toks[start].span.start;
                    let span_end = self.toks[idx - 1].span.end;
                    let source_index = self.toks[start].span.source_index;
                    result.push(StmtCursor {
                        toks: &self.toks[start..idx],
                        span: Span::new(span_start, span_end, source_index),
                        i: 0,
                    });
                }
                start = idx + 1;
            }
        }

        if start < self.toks.len() {
            let span_start = self.toks[start].span.start;
            let span_end = self.toks[self.toks.len() - 1].span.end;
            let source_index = self.toks[start].span.source_index;
            result.push(StmtCursor {
                toks: &self.toks[start..],
                span: Span::new(span_start, span_end, source_index),
                i: 0,
            });
        }

        result
    }

    pub(crate) fn split_on(&mut self, stop_on: TokenKind) -> Result<StmtCursor<'a>, SpicyError> {
        let mut before = None;

        for (offset, tok) in self.toks[self.i..].iter().enumerate() {
            if tok.kind == stop_on {
                let source_index = self.toks[self.i].span.source_index;
                before = Some(StmtCursor {
                    toks: &self.toks[self.i..self.i + offset],
                    span: Span::new(self.i, self.i + offset, source_index),
                    i: 0,
                });
                self.i += offset;
                break;
            }
        }

        Ok(before.ok_or(ParserError::MissingToken {
            message: "expected token",
            span: Some(self.span),
        })?)
    }

    pub(crate) fn into_statement(self) -> Statement {
        Statement {
            tokens: self.toks[self.i..].to_vec(),
            // TODO: fix the span to only include the new statement?
            span: self.span,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Statements {
    pub(crate) statements: Vec<Statement>,
}

impl Statements {
    fn merge_statements(statements: Vec<Statement>) -> Result<Vec<Statement>, ParserError> {
        let mut merged: Vec<Statement> = Vec::new();

        for stmt in statements.into_iter() {
            // Find first non-whitespace token index
            let cursor = stmt.as_cursor();
            let starts_with_plus = match cursor.peek_non_whitespace() {
                Some(t) => t.kind == TokenKind::Plus,
                None => false,
            };

            if starts_with_plus {
                if let Some(prev) = merged.last_mut() {
                    // Append everything after the leading '+' to previous statement
                    let start_idx = cursor.pos() + 1;
                    // TODO: might be a better way to safely index into stmt.tokens
                    prev.tokens.extend_from_slice(&stmt.tokens[start_idx..]);
                    prev.span.end = stmt.span.end;
                } else {
                    return Err(ParserError::ContinuationWithoutPrevious { span: stmt.span });
                }
            } else {
                merged.push(stmt);
            }
        }

        Ok(merged)
    }

    pub(crate) fn new(input: &str, source_index: SourceFileId) -> Result<Self, SpicyError> {
        let mut lexer = Lexer::new(input, source_index);
        let mut statements = Vec::new();
        let mut token = lexer.next()?;

        let mut statement = Vec::with_capacity(1024);
        while token.kind != TokenKind::EOF {
            statement.clear();
            while token.kind != TokenKind::Newline && token.kind != TokenKind::EOF {
                statement.push(token);
                token = lexer.next()?;
            }

            // skip newlines
            token = lexer.next()?;
            statements.push(Statement::new(statement.clone())?);
        }

        // Merge statements with trailing '+' continuation
        let statements = Self::merge_statements(statements)?;
        Ok(Self { statements })
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::lexer::{TokenKind, token_text};
    use crate::libs_phase::SourceFileId;
    use std::path::PathBuf;

    #[rstest]
    fn test_statement_stream(#[files("tests/statement_inputs/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");

        let stream = Statements::new(&input_content, SourceFileId::new(0))
            .expect("failed to create statements");

        let name = format!(
            "statements-{}",
            input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        insta::assert_debug_snapshot!(name, stream);
    }

    #[test]
    fn test_split_on_whitespace() {
        // param1=value1 param2=123\n
        let input = "param1=value1 param2=123\n";
        let stmt = Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let cursor = stmt.statements[0].as_cursor();

        // split_on_whitespace should create two segments: "param1=value1" and "param2=123"
        let segments = cursor.split_on_whitespace();
        assert_eq!(segments.len(), 2);

        // First segment tokens should be: Ident '=' Ident
        assert_eq!(segments[0].toks.len(), 3);
        assert!(matches!(segments[0].toks[0].kind, TokenKind::Ident));
        assert!(matches!(segments[0].toks[1].kind, TokenKind::Equal));
        assert!(matches!(segments[0].toks[2].kind, TokenKind::Ident));

        // Second segment tokens should be: Ident '=' Number
        assert_eq!(segments[1].toks.len(), 3);
        assert!(matches!(segments[1].toks[0].kind, TokenKind::Ident));
        assert!(matches!(segments[1].toks[1].kind, TokenKind::Equal));
        assert!(matches!(segments[1].toks[2].kind, TokenKind::Number));
    }

    #[test]
    fn test_split_on() {
        // param1=value1\n
        let input = "param1=value1\n";
        let stmt = Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let mut cursor = stmt.statements[0].as_cursor();

        // split_on '=' should return the identifier before '=' and advance the cursor to '='
        let before_eq = cursor
            .split_on(TokenKind::Equal)
            .expect("should find '=' in segment");
        assert_eq!(before_eq.toks.len(), 1);
        assert_eq!(token_text(input, &before_eq.toks[0]), "param1");
        assert!(matches!(cursor.peek().unwrap().kind, TokenKind::Equal));
    }
}
