use std::collections::HashMap;

/// A parser for basic latex expressions.
/// based on matklad's pratt parser blog https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html
use crate::lexer::{Token, TokenKind};
use crate::netlist_types::{CommandType, ElementType, ValueSuffix};
use crate::attributes::{Attributes, Attr};
use crate::statement_phase::{Statement, StatementStream};
use crate::expr::Value;

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
}

#[derive(Debug)]
pub struct Subcircuit {
    pub name: String,
    pub nodes: Vec<Node>,
    pub elements: Vec<Element>,
}

#[derive(Debug)]
pub struct Deck {
    pub title: String,
    pub params: HashMap<String, Value>,
    pub subcircuits: Vec<Subcircuit>,
    pub commands: Vec<Command>,
    pub elements: Vec<Element>,
}



#[derive(Debug, Clone)]
pub enum ValueOrParam {
    Value(Value),
    Param(String),
}

impl ValueOrParam {
    pub fn get_value(&self) -> f64 {
        match self {
            ValueOrParam::Value(value) => value.get_value(),
            ValueOrParam::Param(param) => {
                panic!("Param is not a value: {}", param)
            }
        }
    }
}


#[derive(Debug, Clone)]
pub struct Element {
    pub kind: ElementType,
    pub name: String,
    // maybe we can make this type safe with a postive/negative node type
    pub nodes: Vec<Node>,
    pub value: ValueOrParam,
    pub params: Attributes,
    pub start: usize,
    pub end: usize,
}

impl Element {
    pub fn name(&self) -> String {
        format!("{}{}", self.kind.to_char(), self.name)
    }
}


#[derive(Debug, Clone)]
pub struct Command {
    pub kind: CommandType,
    pub params: Attributes,
    pub start: usize,
    pub end: usize,
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

    fn parse_ident(&mut self, statement: &mut Statement) -> String {
        let ident = statement.next_non_whitespace().expect("Must be ident");
        assert_eq!(ident.kind, TokenKind::Ident);
        self.input[ident.start..=ident.end].to_string()
    }

    fn parse_equal_expr(&mut self, statement: &mut Statement) -> (String, Value) {
        let ident = self.parse_ident(statement);
        let equal = statement.next().expect("Must be equal");
        assert_eq!(equal.kind, TokenKind::Equal);
        let value = self.parse_value(statement);
        (ident, value)
    }

    fn parse_value(&mut self, statement: &mut Statement) -> Value {
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

        Value {
            value,
            exponent,
            suffix,
        }
    }

    fn parse_value_or_param(&mut self, statement: &mut Statement) -> ValueOrParam {
        let token = statement.peek_non_whitespace().expect("Must be value or param");
        if token.kind == TokenKind::LeftBracket {
            let _ = statement.next().expect("Must be left bracket");
            let param_name = self.parse_ident(statement);
            let right_bracket = statement.next().expect("Must be right bracket");
            assert_eq!(right_bracket.kind, TokenKind::RightBracket);
            return ValueOrParam::Param(param_name);
        }
        ValueOrParam::Value(self.parse_value(statement))
    }

    fn parse_node(&mut self, statement: &mut Statement) -> Node {
        let node = statement.next_non_whitespace().expect("Must be node");
        assert!(matches!(node.kind, TokenKind::Ident | TokenKind::Number));
        let node_string = self.input[node.start..=node.end].to_string();
        Node { name: node_string }
    }

    fn parse_element_params(&mut self, params_order: Vec<&str>, mut statement: &mut Statement) -> Attributes {
        let mut named_mode = false;
        let mut current_param = 0;
        let mut params = Attributes::new();

        while let Some(token) = statement.next() {
            if token.kind != TokenKind::WhiteSpace {
                println!("warning: unexpected token: {:?}", token);
                break;
            }
            let next_token = statement.next().expect("should have a token");
            match next_token.kind {
                TokenKind::Ident => {
                    named_mode = true;
                    let ident = self.parse_ident(&mut statement);
                    let equal_sign = statement.next().expect("Must be equal");
                    assert_eq!(equal_sign.kind, TokenKind::Equal);
                    let value = self.parse_value_or_param(&mut statement);
                    let old_value = params.insert(ident.clone(), value.into());
                    if old_value.is_some() {
                        panic!("duplicate param: {}", ident);
                    }
                }
                _ => {
                    if named_mode {
                        panic!("when in named mode can no longer work with positional")
                    }
                    let value = self.parse_value_or_param(&mut statement);
                    let old_value = params.insert(params_order[current_param].to_string(), value.into());
                    if old_value.is_some() {
                        panic!("duplicate param: {}", params_order[current_param]);
                    }   
                    current_param += 1;
                }
            }
        }

        params
    }

    // RXXXXXXX n+ n- <resistance|r=>value <ac=val> <m=val>
    // + <scale=val> <temp=val> <dtemp=val> <tc1=val> <tc2=val>
    // + <noisy=0|1>
    fn parse_resistor(&mut self, name: String, mut statement: Statement) -> Element {
        let mut nodes: Vec<Node> = vec![];

        let start = statement.start;
        let end = statement.end;

        nodes.push(self.parse_node(&mut statement));
        nodes.push(self.parse_node(&mut statement));

        let value = self.parse_value_or_param(&mut statement);

        // TODO: i kinda want to support type safety on the params (like noisy is always a bool)
        let params_order = vec!["ac", "m", "scale", "temp", "dtemp", "tc1", "tc2", "noisy"];
        let params = self.parse_element_params(params_order, &mut statement);

        Element {
            kind: ElementType::Resistor,
            name,
            nodes,
            value,
            params,
            start,
            end,
        }
    }

    // CXXXXXXX n+ n- <value> <mname> <m=val> <scale=val> <temp=val>
    // + <dtemp=val> <tc1=val> <tc2=val> <ic=init_condition>
    fn parse_capacitor(&mut self, name: String, mut statement: Statement) -> Element {
        let mut nodes: Vec<Node> = vec![];

        let start = statement.start;
        let end = statement.end;

        nodes.push(self.parse_node(&mut statement));
        nodes.push(self.parse_node(&mut statement));

        // support models
        let value = self.parse_value_or_param(&mut statement);

        let params_order = vec!["m", "scale", "temp", "dtemp", "tc1", "tc2", "ic"];
        let params = self.parse_element_params(params_order, &mut statement);

        Element {
            kind: ElementType::Capacitor,
            name,
            nodes,
            value,
            params,
            start,
            end,
        }
    }
    
    // LYYYYYYY n+ n- <value> <mname> <nt=val> <m=val>
    // + <scale=val> <temp=val> <dtemp=val> <tc1=val>
    // + <tc2=val> <ic=init_condition>
    fn parse_inductor(&mut self, name: String, mut statement: Statement) -> Element {
        let mut nodes: Vec<Node> = vec![];

        let start = statement.start;
        let end = statement.end;

        nodes.push(self.parse_node(&mut statement));
        nodes.push(self.parse_node(&mut statement));

        let value = self.parse_value_or_param(&mut statement);

        let params_order = vec!["nt", "m", "scale", "temp", "dtemp", "tc1", "tc2", "ic"];
        let params = self.parse_element_params(params_order, &mut statement);

        Element {
            kind: ElementType::Inductor,
            name,
            nodes,
            value,
            params,
            start,
            end,
        }
        
    }

    // VXXXXXXX N+ N- <<DC> DC/TRAN VALUE> <AC <ACMAG <ACPHASE>>>
    // + <DISTOF1 <F1MAG <F1PHASE>>> <DISTOF2 <F2MAG <F2PHASE>>>
    // IYYYYYYY N+ N- <<DC> DC/TRAN VALUE> <AC <ACMAG <ACPHASE>>>
    // + <DISTOF1 <F1MAG <F1PHASE>>> <DISTOF2 <F2MAG <F2PHASE>>>
    fn parse_independent_source(
        &mut self,
        element_type: ElementType,
        name: String,
        mut statement: Statement,
    ) -> Element {
        let mut nodes: Vec<Node> = vec![];
        let mut params = Attributes::new();

        let start = statement.start;
        let end = statement.end;

        nodes.push(self.parse_node(&mut statement));
        nodes.push(self.parse_node(&mut statement));

        let operation = self.parse_ident(&mut statement);

        let value = match operation.as_str() {
            "DC" => self.parse_value_or_param(&mut statement),
            "AC" => panic!("AC not supported yet"),
            _ => panic!("Invalid operation: {}", operation),
        };

        Element {
            kind: element_type,
            name,
            nodes,
            value,
            params,
            start,
            end,
        }
    }

    fn parse_element(&mut self, mut statement: Statement) -> Element {
        let ident = statement.next().expect("Must be ident");
        assert_eq!(ident.kind, TokenKind::Ident);

        let ident_string = self.input[ident.start..=ident.end].to_string();
        let (first, name) = ident_string.split_at(1);

        let element_type = ElementType::from_str(first).expect("Must be element type");
        let name = name.to_string();

        match element_type {
            ElementType::Resistor => self.parse_resistor(name, statement),
            ElementType::Capacitor => self.parse_capacitor(name, statement),
            ElementType::Inductor => self.parse_inductor(name, statement),
            ElementType::VoltageSource => {
                self.parse_independent_source(ElementType::VoltageSource, name, statement)
            }
            ElementType::CurrentSource => {
                self.parse_independent_source(ElementType::CurrentSource, name, statement)
            }
            ElementType::Subcircuit => todo!(),
        }
    }

    // .dc srcnam vstart vstop vincr [src2 start2 stop2 incr2]
    fn parse_dc_command(&mut self, mut statement: Statement) -> Attributes {
        
        let srcnam = self.parse_ident(&mut statement);
        let vstart = self.parse_value(&mut statement);
        let vstop = self.parse_value(&mut statement);
        let vincr = self.parse_value(&mut statement);

        Attributes::from_iter(vec![
            ("srcnam".to_string(), Attr::String(srcnam)),
            ("vstart".to_string(), Attr::Value(vstart)),
            ("vstop".to_string(), Attr::Value(vstop)),
            ("vincr".to_string(), Attr::Value(vincr)),
        ])
    }

    fn parse_param_command(&mut self, mut statement: Statement) -> HashMap<String, Value> {
        let mut params = HashMap::new();

        while let Some(token) = statement.next() {
            if token.kind != TokenKind::WhiteSpace {
                println!("warning: unexpected token: {:?}", token);
                break;
            }
            let (ident, value) = self.parse_equal_expr(&mut statement);
            params.insert(ident, value);
        }

        params
    }

    fn parse_subcircuit_command(&mut self, mut statement: Statement) -> Subcircuit {
        let name = self.parse_ident(&mut statement);
        let first_node = self.parse_node(&mut statement);
        let mut nodes = vec![first_node];
        while let Some(_) = statement.peek_non_whitespace() {
            // TODO: this needs to change when we support params
            nodes.push(self.parse_node(&mut statement));
        }

        Subcircuit {
            name,
            nodes,
            elements: vec![]
        }
    }

    fn parse_command_type(&mut self, statement: &mut Statement) -> CommandType {
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

        command
    }

    fn parse_command_attrs(&mut self, statement: Statement, command: CommandType) -> Command {

        let start = statement.start;
        let end = statement.end;

        let params = match command {
            CommandType::DC => self.parse_dc_command(statement),
            CommandType::Op => Attributes::from_iter(vec![]),
            _ => panic!("Invalid command: {:?}", command),
        };

        Command {
            kind: command,
            params,
            start,
            end,
        }
    }

    pub fn parse(&mut self) -> Deck {
        // first line should be a title
        let title = self.parse_title();

        let mut commands = vec![];
        let mut elements = vec![];
        let mut subcircuits = vec![];
        let mut params: HashMap<String, Value> = HashMap::new();

        let mut current_subcircuits: Vec<Subcircuit> = vec![];

        while let Some(mut statement) = self.stream.next() {
            let first_token = statement
                .peek()
                .expect("Statement should have at least one token");
            match first_token.kind {
                TokenKind::Dot => {
                    let command = self.parse_command_type(&mut statement);
                    match command {
                        CommandType::Subcircuit => {
                            let subcircuit = self.parse_subcircuit_command(statement);
                            current_subcircuits.push(subcircuit);
                        }
                        CommandType::Ends => {
                            let subcircuit = current_subcircuits.pop().expect("Subcircuit not started");
                            subcircuits.push(subcircuit);
                        }
                        CommandType::Param => {
                            let new_params = self.parse_param_command(statement);
                            params.extend(new_params);
                        }
                        CommandType::End => {
                            // once we see an end command we stop
                            break;
                        }
                        _ => {
                            commands.push(self.parse_command_attrs(statement, command));
                        }
                    }
                }
                // comment
                TokenKind::Asterisk => {
                    let _ = self.parse_comment(statement);
                    // TODO: save comments?
                }
                TokenKind::Ident => {
                    let element = self.parse_element(statement);
                    if let Some(subcircuit) = current_subcircuits.last_mut() {
                        subcircuit.elements.push(element);
                    } else {
                        elements.push(element);
                    }
                }
                _ => {
                    panic!("Expected command or element, got {:?}", first_token.kind);
                }
            }
        }

        Deck {
            title,
            params,
            subcircuits,
            commands,
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
    fn test_parser(#[files("tests/parser_inputs/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let mut parser = Parser::new(&input_content);
        let deck = parser.parse();

        let name = format!("parser-{}",input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string()));
        insta::assert_debug_snapshot!(name, deck);
    }
}
