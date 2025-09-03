use unscanny::Scanner;
use serde::Serialize;

use crate::expr::PlaceholderId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TokenKind {
    Ident,
    Number,
    Equal,
    Dot,
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
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn expand(&self) -> Self {
        Self {
            start: self.start - 1,
            end: self.end + 1
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
    pub fn new(kind: TokenKind, start: usize, end: usize) -> Self {
        Self { kind, id: None, span: Span::new(start, end) }
    }
    pub fn single(kind: TokenKind, pos: usize) -> Self {
        Self {
            kind,
            id: None,
            span: Span::new(pos, pos),
        }
    }

    pub fn end(pos: usize) -> Self {
        Self {
            kind: TokenKind::EOF,
            id: None,
            span: Span::new(pos, pos),
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
}

impl<'s> Lexer<'s> {
    pub fn new(input: &'s str) -> Self {
        Lexer {
            s: Scanner::new(input),
        }
    }

    fn whitespace(&mut self, start: usize) -> Token {
        self.s.eat_while(|c: char| c.is_whitespace() && c != '\n');
        Token::new(TokenKind::WhiteSpace, start, self.s.cursor() - 1)
    }

    fn newline(&mut self, start: usize) -> Token {
        self.s.eat_while(|c: char| c == '\n');
        Token::new(TokenKind::Newline, start, self.s.cursor() - 1)
    }

    fn identifier(&mut self, first_char: char, start: usize) -> Token {
        // ensure first character is alphabetic, then consume remaining alphanumeric characters
        if !first_char.is_alphabetic() {
            panic!("Identifier must start with an alphabetic character");
        }
        self.s.eat_while(|c: char| c.is_alphanumeric());
        let identifier_end = self.s.cursor() - 1;

        Token::new(TokenKind::Ident, start, identifier_end)
    }

    fn number(&mut self, start: usize) -> Token {
        // eat while numeric characters
        self.s.eat_while(|c: char| c.is_numeric());
        let number_end = self.s.cursor() - 1;
        Token::new(TokenKind::Number, start, number_end)
    }

    fn netlist(&mut self, c: char, start: usize) -> Token {
        match c {
            c if c.is_alphabetic() => self.identifier(c, start),
            c if c.is_ascii_digit() => self.number(start),
            '*' => Token::single(TokenKind::Asterisk, start),
            '-' => Token::single(TokenKind::Minus, start),
            '+' => Token::single(TokenKind::Plus, start),
            '=' => Token::single(TokenKind::Equal, start),
            '/' => Token::single(TokenKind::Slash, start),
            '.' => Token::single(TokenKind::Dot, start),
            '{' => Token::single(TokenKind::LeftBrace, start),
            '}' => Token::single(TokenKind::RightBrace, start),
            '(' => Token::single(TokenKind::LeftParen, start),
            ')' => Token::single(TokenKind::RightParen, start),
            ',' => Token::single(TokenKind::Comma, start),
            _ => panic!("Unexpected character: {}", c),
        }
    }

    pub fn next(&mut self) -> Token {
        let start = self.s.cursor();
        match self.s.eat() {
            Some(c) if c == '\n' => self.newline(start),
            Some(c) if c.is_whitespace() => self.whitespace(start),
            Some(c) => self.netlist(c, start),
            None => Token::end(start),
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

        let mut lexer = Lexer::new(&input_content);
        let mut tokens = vec![];
        let mut token = lexer.next();
        while token.kind != TokenKind::EOF {
            tokens.push(token);
            token = lexer.next();
        }

        let name = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        insta::assert_debug_snapshot!(name, tokens);
    }
}
