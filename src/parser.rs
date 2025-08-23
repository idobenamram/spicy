use std::collections::HashMap;

/// A parser for basic latex expressions.
/// based on matklad's pratt parser blog https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html
use crate::lexer::{Lexer, Token, TokenKind};
use crate::netlist_types::{CommandType, ElementType, ValueSuffix};

#[derive(Debug)]
pub struct Node {
    name: String,
}

#[derive(Debug)]
pub struct Deck {
    title: String,
    directives: Vec<Directive>,
    elements: Vec<Element>,
}

#[derive(Debug)]
pub struct Element {
    kind: ElementType,
    name: String,
    nodes: Vec<Node>,
    value: f64,
    exponent: Option<f64>,
    suffix: Option<ValueSuffix>,
    params: HashMap<String, String>,
    start: usize,
    end: usize,
}

#[derive(Debug)]
pub struct Directive {
    kind: CommandType,
    params: HashMap<String, String>,
    start: usize,
    end: usize,
}

#[derive(Debug)]
struct Statement {
    tokens: Vec<Token>,
    start: usize,
    end: usize,
}

impl Statement {
    fn new(tokens: Vec<Token>) -> Self {
        if tokens.is_empty() {
            panic!("Statement must have at least one token");
        }

        let start = tokens[0].start;
        let end = tokens[tokens.len() - 1].end;

        Self {
            start,
            end,
            tokens: tokens.into_iter().rev().collect(),
        }
    }

    fn new_reversed(tokens: Vec<Token>) -> Self {
        let start = tokens[tokens.len() - 1].start;
        let end = tokens[0].end;
        Self {
            start,
            end,
            tokens,
        }
    }

    fn next(&mut self) -> Option<Token> {
        self.tokens.pop()
    }

    fn next_non_whitespace(&mut self) -> Option<Token> {
        while let Some(token) = self.tokens.pop() {
            if token.kind == TokenKind::WhiteSpace {
                continue;
            }
            return Some(token);
        }
        None
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.last()
    }
}

#[derive(Debug)]
struct StatementStream {
    statements: Vec<Statement>,
}

impl StatementStream {
    // fn new(input: &str) -> Self {
    //     let mut lexer = Lexer::new(input);
    //     let mut statements = vec![];
    //     let mut token = lexer.next();

    //     let mut last_non_whitespace_token = None;
    //     let mut statement = vec![];
    //     while token.kind != TokenKind::EOF {
    //         while token.kind != TokenKind::Newline {
    //             statement.push(token);
    //             if token.kind != TokenKind::WhiteSpace {
    //                 last_non_whitespace_token = Some(token);
    //             }
    //             token = lexer.next();
    //         }

    //         // skip newlines
    //         token = lexer.next();
    //         if let Some(last_non_whitespace_token) = last_non_whitespace_token {
    //             if last_non_whitespace_token.kind != TokenKind::Plus {
    //                 statements.push(Statement::new(&statement));
    //                 statement.clear();
    //             }
    //         }
    //     }

    //     // reverse statements to make it easier to pop
    //     statements.reverse();
    //     Self { statements }
    // }

    fn merge_statements(statements: Vec<Statement>) -> Vec<Statement> {
        let mut merged: Vec<Statement> = Vec::new();
        let mut iter = statements.into_iter();
        while let Some(stmt) = iter.next() {
            // Start with the current statement's tokens
            let mut tokens = stmt.tokens;

            loop {
                // Trim trailing whitespace
                while matches!(tokens.last(), Some(t) if t.kind == TokenKind::WhiteSpace) {
                    tokens.pop();
                }

                // If last non-whitespace token is a Plus, remove it and merge next
                let should_merge = matches!(tokens.last(), Some(t) if t.kind == TokenKind::Plus);
                if !should_merge {
                    break;
                }
                // Remove the trailing Plus token
                tokens.pop();

                // Fetch the next statement to merge into current
                if let Some(next_stmt) = iter.next() {
                    // Append next statement tokens
                    tokens.extend(next_stmt.tokens);
                } else {
                    break;
                }
            }

            merged.push(Statement::new_reversed(tokens));
        }
        merged
    }

    fn new(input: &str) -> Self {
        let mut lexer = Lexer::new(input);
        let mut statements = vec![];
        let mut token = lexer.next();

        while token.kind != TokenKind::EOF {
            let mut statement = vec![];
            // TODO: this might get stuck if we don't also add EOF here
            while token.kind != TokenKind::Newline {
                statement.push(token);
                token = lexer.next();
            }

            // skip newlines
            token = lexer.next();
            statements.push(Statement::new(statement));
        }

        println!("merge statements");
        // Merge statements with trailing '+' continuation
        let statements = Self::merge_statements(statements);
        // reverse statements to make it easier to pop
        let mut statements = statements;
        statements.reverse();
        Self { statements }
    }

    fn next(&mut self) -> Option<Statement> {
        self.statements.pop()
    }

    fn peek(&self) -> Option<&Statement> {
        self.statements.last()
    }
}

pub struct Parser<'s> {
    input: &'s str,
    stream: StatementStream,
}

impl<'s> Parser<'s> {
    pub fn new(input: &'s str) -> Self {
        Parser {
            input,
            stream: StatementStream::new(input),
        }
    }

    fn parse_title(&mut self) -> String {
        let statement = self.stream.next();
        if statement.is_none() {
            panic!("Expected title, got EOF");
        }
        let statement = statement.unwrap();
        self.input[statement.start..=statement.end].to_string()
    }

    fn parse_comment(&mut self, statement: Statement) -> String {
        let comment = self.input[statement.start..=statement.end].to_string();
        comment
    }

    fn parse_value(&mut self, mut statement: Statement) -> (f64, Option<f64>, Option<ValueSuffix>) {
        let mut number_str = String::new();
        let mut exponent: Option<f64> = None;
        let mut suffix: Option<ValueSuffix> = None;

        // Optional leading minus
        let mut t = statement
            .next_non_whitespace()
            .expect("Must start with a value");
        if matches!(t.kind, TokenKind::Minus) {
            number_str.push('-');
            t = statement
                .next_non_whitespace()
                .expect("Expected digits or '.' after '-'");
        }

        // Integer digits or leading '.' with fraction
        match t.kind {
            TokenKind::Number => {
                number_str.push_str(&self.input[t.start..=t.end]);
                // Optional fractional part if next immediate token is a dot
                if let Some(peek) = statement.peek() {
                    if matches!(peek.kind, TokenKind::Dot) {
                        let _dot = statement.next().unwrap();
                        number_str.push('.');
                        let frac = statement
                            .next_non_whitespace()
                            .expect("Expected digits after '.'");
                        assert!(
                            matches!(frac.kind, TokenKind::Number),
                            "Expected digits after '.'"
                        );
                        number_str.push_str(&self.input[frac.start..=frac.end]);
                    }
                }
            }
            TokenKind::Dot => {
                number_str.push('.');
                let frac = statement
                    .next_non_whitespace()
                    .expect("Expected digits after '.'");
                assert!(
                    matches!(frac.kind, TokenKind::Number),
                    "Expected digits after '.'"
                );
                number_str.push_str(&self.input[frac.start..=frac.end]);
            }
            _ => panic!("Invalid start of numeric value"),
        }

        // Optional exponent: e|E [+-]? digits (no whitespace inside the literal)
        if let Some(peek) = statement.peek() {
            if matches!(peek.kind, TokenKind::Ident) {
                let ident_text = &self.input[peek.start..=peek.end];
                if ident_text == "e" || ident_text == "E" {
                    let _e = statement.next().unwrap();
                    let mut exp_str = String::new();
                    // optional sign
                    if let Some(sign_peek) = statement.peek() {
                        match sign_peek.kind {
                            TokenKind::Plus => {
                                let _ = statement.next().unwrap();
                                exp_str.push('+');
                            }
                            TokenKind::Minus => {
                                let _ = statement.next().unwrap();
                                exp_str.push('-');
                            }
                            _ => {}
                        }
                    }
                    let exp_digits = statement.next().expect("Expected digits after exponent");
                    assert!(matches!(exp_digits.kind, TokenKind::Number));
                    exp_str.push_str(&self.input[exp_digits.start..=exp_digits.end]);
                    exponent = Some(exp_str.parse::<f64>().expect("Invalid exponent digits"));
                }
            }
        }

        // Optional suffix as trailing identifier without whitespace
        if let Some(peek) = statement.peek() {
            if matches!(peek.kind, TokenKind::Ident) {
                let ident = statement.next().unwrap();
                let ident_text = &self.input[ident.start..=ident.end];
                suffix = ValueSuffix::from_str(ident_text);
            }
        }

        let value: f64 = number_str
            .parse()
            .unwrap_or_else(|_| panic!("Invalid numeric literal: {}", number_str));

        (value, exponent, suffix)
    }

    // RXXXXXXX n+ n- <resistance|r=>value <ac=val> <m=val>
    // + <scale=val> <temp=val> <dtemp=val> <tc1=val> <tc2=val>
    // + <noisy=0|1>
    fn parse_resistor(&mut self, name: String, mut statement: Statement) -> Element {
        let mut nodes: Vec<Node> = vec![];
        let params = HashMap::new();

        let start = statement.start;
        let end = statement.end;

        let node1 = statement.next_non_whitespace().expect("Must be node1");
        assert!(matches!(node1.kind, TokenKind::Ident | TokenKind::Number));
        let node1_string = self.input[node1.start..=node1.end].to_string();
        nodes.push(Node { name: node1_string });

        let node2 = statement.next_non_whitespace().expect("Must be node2");
        assert!(matches!(node2.kind, TokenKind::Ident | TokenKind::Number));
        let node2_string = self.input[node2.start..=node2.end].to_string();
        nodes.push(Node { name: node2_string });

        let (value, exponent, suffix) = self.parse_value(statement);

        Element {
            kind: ElementType::Resistor,
            name,
            nodes,
            value,
            exponent,
            suffix,
            params,
            start,
            end,
        }
    }

    pub fn parse_element(&mut self, mut statement: Statement) -> Element {
        let ident = statement.next().expect("Must be ident");
        assert_eq!(ident.kind, TokenKind::Ident);

        let ident_string = self.input[ident.start..=ident.end].to_string();
        let (first, name) = ident_string.split_at(1);

        let element_type = ElementType::from_str(first).expect("Must be element type");
        let name = name.to_string();

        match element_type {
            ElementType::Resistor => self.parse_resistor(name, statement),
            _ => panic!("Invalid element type: {:?}", element_type),
        }
    }

    // .dc srcnam vstart vstop vincr [src2 start2 stop2 incr2]
    fn parse_dc_command(&mut self, statement: Statement) -> Directive {
        todo!()
    }

    pub fn parse_directive(&mut self, mut statement: Statement) -> Directive {
        let dot = statement.next().expect("Must be dot");
        assert_eq!(dot.kind, TokenKind::Dot);

        let kind = statement.next().expect("Must be element type");
        assert_eq!(kind.kind, TokenKind::Ident);

        let command = match CommandType::from_str(&self.input[kind.start..=kind.end]) {
            Some(command) => command,
            None => panic!(
                "Invalid element type: {}",
                &self.input[kind.start..=kind.end]
            ),
        };

        Directive {
            kind: command,
            params: HashMap::new(),
            start: statement.start,
            end: statement.end,
        }
    }

    pub fn parse(&mut self) -> Deck {
        // first line should be a title
        let title = self.parse_title();

        println!("title {:?}", title);
        let mut directives = vec![];
        let mut elements = vec![];

        while let Some(statement) = self.stream.next() {
            let first_token = statement
                .peek()
                .expect("Statement should have at least one token");
            println!("statement {:?}", statement);
            match first_token.kind {
                TokenKind::Dot => {
                    let directive = self.parse_directive(statement);
                    directives.push(directive);
                }
                // comment
                TokenKind::Asterisk => {
                    let comment = self.parse_comment(statement);
                    // TODO: save comments?
                }
                TokenKind::Ident => {
                    let element = self.parse_element(statement);
                    elements.push(element);
                }
                _ => {
                    panic!("Expected directive or element, got {:?}", first_token.kind);
                }
            }
        }

        Deck {
            title,
            directives,
            elements,
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use std::path::PathBuf;

    #[rstest]
    fn test_statement_stream(
        #[files("tests/test_inputs/with_line_continuation.spicy")] input: PathBuf,
    ) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");

        let stream = StatementStream::new(&input_content);

        let name = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        insta::assert_debug_snapshot!(name, stream);
    }

    #[rstest]
    fn test_parser(#[files("tests/test_inputs/basic_resistor.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let mut parser = Parser::new(&input_content);
        let deck = parser.parse();

        let name = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        insta::assert_debug_snapshot!(name, deck);
    }
}
