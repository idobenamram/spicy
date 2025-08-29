use unscanny::Scanner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Ident,
    Number,
    Equal,
    Dot,
    Asterisk,
    WhiteSpace,
    Newline,
    LeftBracket,
    RightBracket,
    Plus,
    Minus,
    EOF,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl Token {
    pub fn new(kind: TokenKind, start: usize, end: usize) -> Self {
        Self { kind, start, end }
    }
    pub fn single(kind: TokenKind, pos: usize) -> Self {
        Self {
            kind,
            start: pos,
            end: pos,
        }
    }

    pub fn end(pos: usize) -> Self {
        Self {
            kind: TokenKind::EOF,
            start: pos,
            end: pos,
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
            '.' => Token::single(TokenKind::Dot, start),
            '{' => Token::single(TokenKind::LeftBracket, start),
            '}' => Token::single(TokenKind::RightBracket, start),
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
