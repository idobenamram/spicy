use crate::{error::LexerError, libs_phase::SourceFileId};
use serde::Serialize;
use unscanny::Scanner;

use crate::expr::PlaceholderId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[allow(clippy::upper_case_acronyms)]
pub enum TokenKind {
    Ident,
    Number,
    Equal,
    Dot,
    Colon,
    GreaterThan,
    LessThan,
    Placeholder,
    Asterisk,
    WhiteSpace,
    Newline,
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    Comma,
    Plus,
    Minus,
    Slash,
    EOF,
    Underscore,
}

impl TokenKind {
    pub fn ident_or_numeric(&self) -> bool {
        matches!(self, TokenKind::Ident | TokenKind::Number)
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub source_index: SourceFileId,
}

impl Span {
    pub fn new(start: usize, end: usize, source_index: SourceFileId) -> Self {
        Self {
            start,
            end,
            source_index,
        }
    }

    pub fn single(pos: usize, source_index: SourceFileId) -> Self {
        Self {
            start: pos,
            end: pos,
            source_index,
        }
    }

    pub fn expand(&self) -> Self {
        Self {
            start: self.start - 1,
            end: self.end + 1,
            source_index: self.source_index,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) id: Option<PlaceholderId>,
    pub(crate) span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self {
            kind,
            id: None,
            span,
        }
    }
    pub fn single(kind: TokenKind, start: usize, source_index: SourceFileId) -> Self {
        Self {
            kind,
            id: None,
            span: Span::single(start, source_index),
        }
    }

    pub fn end(pos: usize, source_index: SourceFileId) -> Self {
        Self {
            kind: TokenKind::EOF,
            id: None,
            span: Span::new(pos, pos, source_index),
        }
    }

    pub fn placeholder(id: PlaceholderId, span: Span) -> Self {
        Self {
            kind: TokenKind::Placeholder,
            id: Some(id),
            span,
        }
    }
}

pub(crate) struct Lexer<'s> {
    s: Scanner<'s>,
    source_index: SourceFileId,
}

impl<'s> Lexer<'s> {
    pub fn new(input: &'s str, source_index: SourceFileId) -> Self {
        Lexer {
            s: Scanner::new(input),
            source_index,
        }
    }

    fn whitespace(&mut self, start: usize) -> Token {
        self.s.eat_while(|c: char| c.is_whitespace() && c != '\n');
        Token::new(
            TokenKind::WhiteSpace,
            Span::new(start, self.s.cursor() - 1, self.source_index),
        )
    }

    fn newline(&mut self, start: usize) -> Token {
        self.s.eat_while(|c: char| c == '\n');
        Token::new(
            TokenKind::Newline,
            Span::new(start, self.s.cursor() - 1, self.source_index),
        )
    }

    fn identifier(&mut self, first_char: char, start: usize) -> Result<Token, LexerError> {
        // ensure first character is alphabetic, then consume remaining alphanumeric characters
        if !first_char.is_alphabetic() {
            return Err(LexerError::InvalidIdentifierStart {
                span: Span::new(start, start, self.source_index),
            });
        }
        self.s.eat_while(|c: char| c.is_alphanumeric());
        let identifier_end = self.s.cursor() - 1;

        Ok(Token::new(
            TokenKind::Ident,
            Span::new(start, identifier_end, self.source_index),
        ))
    }

    fn number(&mut self, start: usize) -> Token {
        // eat while numeric characters
        self.s.eat_while(|c: char| c.is_numeric());
        let number_end = self.s.cursor() - 1;
        Token::new(
            TokenKind::Number,
            Span::new(start, number_end, self.source_index),
        )
    }

    fn netlist(&mut self, c: char, start: usize) -> Result<Token, LexerError> {
        match c {
            c if c.is_alphabetic() => self.identifier(c, start),
            c if c.is_ascii_digit() => Ok(self.number(start)),
            '*' => Ok(Token::single(TokenKind::Asterisk, start, self.source_index)),
            '-' => Ok(Token::single(TokenKind::Minus, start, self.source_index)),
            '+' => Ok(Token::single(TokenKind::Plus, start, self.source_index)),
            '=' => Ok(Token::single(TokenKind::Equal, start, self.source_index)),
            '/' => Ok(Token::single(TokenKind::Slash, start, self.source_index)),
            '.' => Ok(Token::single(TokenKind::Dot, start, self.source_index)),
            '{' => Ok(Token::single(
                TokenKind::LeftBrace,
                start,
                self.source_index,
            )),
            '}' => Ok(Token::single(
                TokenKind::RightBrace,
                start,
                self.source_index,
            )),
            '(' => Ok(Token::single(
                TokenKind::LeftParen,
                start,
                self.source_index,
            )),
            ')' => Ok(Token::single(
                TokenKind::RightParen,
                start,
                self.source_index,
            )),
            ',' => Ok(Token::single(TokenKind::Comma, start, self.source_index)),
            ':' => Ok(Token::single(TokenKind::Colon, start, self.source_index)),
            '>' => Ok(Token::single(
                TokenKind::GreaterThan,
                start,
                self.source_index,
            )),
            '<' => Ok(Token::single(TokenKind::LessThan, start, self.source_index)),
            '_' => Ok(Token::single(
                TokenKind::Underscore,
                start,
                self.source_index,
            )),
            _ => Err(LexerError::UnexpectedCharacter {
                ch: c,
                span: Span::single(start, self.source_index),
            }),
        }
    }

    pub fn next(&mut self) -> Result<Token, LexerError> {
        let start = self.s.cursor();

        match self.s.eat() {
            Some('\n') => Ok(self.newline(start)),
            Some(c) if c.is_whitespace() => Ok(self.whitespace(start)),
            Some(c) => self.netlist(c, start),
            None => Ok(Token::end(start, self.source_index)),
        }
    }
}

pub fn token_text<'a>(src: &'a str, t: &Token) -> &'a str {
    &src[t.span.start..=t.span.end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::path::PathBuf;

    #[rstest]
    fn test_lexer(#[files("tests/lexer_inputs/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");

        let mut lexer = Lexer::new(&input_content, SourceFileId::new(0));
        let mut tokens = vec![];
        loop {
            let token = lexer.next().expect("lexing should succeed in tests");
            if token.kind == TokenKind::EOF {
                break;
            }
            tokens.push(token);
        }

        let name = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        insta::assert_debug_snapshot!(name, tokens);
    }
}
