use crate::{
    lexer::{Lexer, Span, Token, TokenKind, token_text},
    netlist_types::CommandType,
};

#[derive(Debug)]
pub struct Statement {
    pub tokens: Vec<Token>,
    pub span: Span,
}

impl Statement {
    fn new(tokens: Vec<Token>) -> Self {
        if tokens.is_empty() {
            panic!("Statement must have at least one token");
        }

        let start = tokens[0].span.start;
        let end = tokens[tokens.len() - 1].span.end;

        Self {
            span: Span::new(start, end),
            tokens,
        }
    }

    pub fn into_cursor(&self) -> StmtCursor<'_> {
        StmtCursor {
            toks: &self.tokens,
            i: 0,
        }
    }

    pub fn replace_tokens(&mut self, start: usize, end: usize, tokens: Vec<Token>) {
        self.tokens.splice(start..=end, tokens);
    }

    // pub fn next(&mut self) -> Option<Token> {
    //     self.tokens.
    // }

    // pub fn next_non_whitespace(&mut self) -> Option<Token> {
    //     while let Some(token) = self.tokens.remove(0) {
    //         if token.kind == TokenKind::WhiteSpace {
    //             continue;
    //         }
    //         return Some(token);
    //     }
    //     None
    // }
}

#[derive(Clone)]
pub struct StmtCursor<'a> {
    toks: &'a [Token],
    i: usize,
}

impl<'a> StmtCursor<'a> {
    pub fn done(&self) -> bool { self.i >= self.toks.len() }
    #[inline]
    pub fn pos(&self) -> usize {
        self.i
    }
    #[inline]
    pub fn checkpoint(&self) -> usize {
        self.i
    }
    #[inline]
    pub fn rewind(&mut self, mark: usize) {
        self.i = mark;
    }

    #[inline]
    pub fn peek(&self) -> Option<&'a Token> {
        self.toks.get(self.i)
    }
    /// Peek skipping whitespace
    pub fn peek_non_ws(&self) -> Option<&'a Token> {
        let mut j = self.i;
        while let Some(t) = self.toks.get(j) {
            if t.kind != TokenKind::WhiteSpace {
                return Some(t);
            }
            j += 1;
        }
        None
    }

    pub fn next(&mut self) -> Option<&'a Token> {
        let t = self.toks.get(self.i);
        if t.is_some() {
            self.i += 1;
        }
        t
    }
    /// Advance past whitespace only
    pub fn skip_ws(&mut self) {
        while let Some(t) = self.toks.get(self.i) {
            if t.kind != TokenKind::WhiteSpace {
                break;
            }
            self.i += 1;
        }
    }

    pub fn next_non_whitespace(&mut self) -> Option<&'a Token> {
        self.skip_ws();
        self.next()
    }
    /// Consume a specific kind (skips ws first)
    pub fn consume(&mut self, kind: TokenKind) -> Option<&'a Token> {
        if self.peek()?.kind == kind {
            return self.next();
        }
        None
    }
    /// Expect a specific kind (nice for errors)
    pub fn expect(&mut self, kind: TokenKind) -> Result<&'a Token, &'static str> {
        self.peek()
            .filter(|t| t.kind == kind)
            .map(|_| self.next().unwrap())
            .ok_or("unexpected token")
    }

    pub fn consume_if_command(&mut self, input: &str, command: CommandType) -> bool {
        let checkpoint = self.checkpoint();
        self.skip_ws();
        if let Some(_) = self.consume(TokenKind::Dot) {
            if let Some(kind) = self.consume(TokenKind::Ident) {
                let found_command = CommandType::from_str(token_text(input, kind)) == Some(command);
                if found_command {
                    return true;
                }
            }
        }
        self.rewind(checkpoint);
        false
    }
}

#[derive(Debug)]
pub struct StatementStream {
    statements: Vec<Statement>,
}

impl StatementStream {
    fn merge_statements(statements: Vec<Statement>) -> Vec<Statement> {
        let mut merged: Vec<Statement> = Vec::new();

        for stmt in statements.into_iter() {
            // Find first non-whitespace token index
            let cursor = stmt.into_cursor();
            let starts_with_plus = match cursor.peek_non_ws() {
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
                    panic!("No previous statement to merge with");
                }
            } else {
                merged.push(stmt);
            }
        }

        merged
    }

    pub fn new(input: &str) -> Self {
        let mut lexer = Lexer::new(input);
        let mut statements = vec![];
        let mut token = lexer.next();

        while token.kind != TokenKind::EOF {
            let mut statement = vec![];
            while token.kind != TokenKind::Newline && token.kind != TokenKind::EOF {
                statement.push(token);
                token = lexer.next();
            }

            // skip newlines
            token = lexer.next();
            statements.push(Statement::new(statement));
        }

        // Merge statements with trailing '+' continuation
        let mut statements = Self::merge_statements(statements);
        statements.reverse();

        Self { statements }
    }

    pub fn next(&mut self) -> Option<Statement> {
        self.statements.pop()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use std::path::PathBuf;

    #[rstest]
    fn test_statement_stream(#[files("tests/statement_inputs/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");

        let stream = StatementStream::new(&input_content);

        let name = format!(
            "stream-{}",
            input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        insta::assert_debug_snapshot!(name, stream);
    }
}
