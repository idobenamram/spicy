use std::path::PathBuf;

use thiserror::Error;

use crate::Span;

#[derive(Debug, Error)]
pub enum SpicyError {
    #[error(transparent)]
    Lexer(#[from] LexerError),
    #[error(transparent)]
    Parser(#[from] ParserError),
    #[error(transparent)]
    Expression(#[from] ExpressionError),
    #[error(transparent)]
    Subcircuit(#[from] SubcircuitError),
    #[error(transparent)]
    Include(#[from] IncludeError),
}

impl SpicyError {
    pub fn error_span(&self) -> Option<Span> {
        match self {
            SpicyError::Lexer(le) => match le {
                LexerError::UnexpectedCharacter { span, .. } => Some(*span),
                LexerError::InvalidIdentifierStart { span } => Some(*span),
            },
            SpicyError::Parser(pe) => match pe {
                ParserError::ContinuationWithoutPrevious { span }
                | ParserError::UnexpectedToken { span, .. }
                | ParserError::InvalidStartNumeric { span }
                | ParserError::ExpectedDigitsAfterDot { span }
                | ParserError::InvalidExponentDigits { span, .. }
                | ParserError::ExpectedBoolZeroOrOne { span }
                | ParserError::ExpectedIdent { span }
                | ParserError::MissingPlaceholderId { span }
                | ParserError::MissingScope { span }
                | ParserError::UnexpectedCommandType { span, .. }
                | ParserError::InvalidCommandType { span, .. }
                | ParserError::InvalidOperation { span, .. }
                | ParserError::InvalidParam { span, .. }
                | ParserError::UnmatchedBrace { span }
                | ParserError::EmptyExpressionInsideBraces { span }
                | ParserError::TooManyParameters { span, .. } => Some(*span),
                ParserError::MissingToken { .. }
                | ParserError::InvalidDeviceType { .. }
                | ParserError::EmptyStatement
                | ParserError::MissingTitle => None,
                ParserError::InvalidNumericLiteral { span, .. } => *span,
            },
            SpicyError::Expression(ee) => match ee {
                ExpressionError::UnexpectedToken { span, .. }
                | ExpressionError::BadPrefixOperator { span, .. }
                | ExpressionError::UnevaluatablePlaceholder { span, .. }
                | ExpressionError::UnknownIdentifier { span, .. }
                | ExpressionError::UnsupportedUnaryOperator { span, .. }
                | ExpressionError::UnsupportedBinaryOperator { span, .. } => Some(*span),
                ExpressionError::MissingToken { .. } => None,
            },
            SpicyError::Subcircuit(se) => match se {
                SubcircuitError::MissingSubcircuitName { span } => *span,
                SubcircuitError::InvalidDeviceModelType { span, .. } => Some(*span),
                SubcircuitError::NoNodes { span, .. } => Some(*span),
                SubcircuitError::NotFound { .. } | SubcircuitError::ArityMismatch { .. } => None,
                SubcircuitError::ModelAlreadyExists { span, .. } => Some(*span),
            },
            SpicyError::Include(ie) => match ie {
                IncludeError::ExpectedPath { span }
                | IncludeError::FileNotFound { span, .. }
                | IncludeError::IOError { span, .. }
                | IncludeError::MaxDepthExceeded { span, .. }
                | IncludeError::CycleDetected { span, .. }
                | IncludeError::LibSectionNotFound { span, .. } => Some(*span),
            },
        }
    }
}

#[derive(Debug, Error)]
pub enum LexerError {
    #[error("unexpected character '{ch}'")]
    UnexpectedCharacter { ch: char, span: Span },

    #[error("identifier must start with an alphabetic character")]
    InvalidIdentifierStart { span: Span },
}

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("empty statement")]
    EmptyStatement,

    #[error("line continuation '+' without a previous statement")]
    ContinuationWithoutPrevious { span: Span },

    #[error("unexpected token {found:?} (expected {expected})")]
    UnexpectedToken {
        expected: String,
        found: crate::lexer::TokenKind,
        span: Span,
    },

    #[error("missing token: {message}")]
    MissingToken {
        message: &'static str,
        span: Option<Span>,
    },

    #[error("invalid start of numeric value")]
    InvalidStartNumeric { span: Span },

    #[error("expected digits after '.'")]
    ExpectedDigitsAfterDot { span: Span },

    #[error("invalid exponent digits '{lexeme}'")]
    InvalidExponentDigits { span: Span, lexeme: String },

    #[error("invalid numeric literal '{lexeme}'")]
    InvalidNumericLiteral { span: Option<Span>, lexeme: String },

    #[error("expected boolean '0' or '1'")]
    ExpectedBoolZeroOrOne { span: Span },

    #[error("expected identifier")]
    ExpectedIdent { span: Span },

    #[error("missing placeholder id")]
    MissingPlaceholderId { span: Span },

    #[error("invalid param: {param}")]
    InvalidParam { param: String, span: Span },

    #[error("invalid operation: {operation}")]
    InvalidOperation { operation: String, span: Span },

    #[error("invalid command type: {s}")]
    InvalidCommandType { s: String, span: Span },

    #[error("unexpected command type: {s}")]
    UnexpectedCommandType { s: String, span: Span },

    #[error("invalid device type: {s}")]
    InvalidDeviceType { s: String },

    #[error("missing scope")]
    MissingScope { span: Span },

    #[error("missing title")]
    MissingTitle,

    #[error("unmatched '{{'")]
    UnmatchedBrace { span: Span },

    #[error("empty expression inside braces")]
    EmptyExpressionInsideBraces { span: Span },

    #[error("too many parameters provided (parameter {index} exceeds expected count)")]
    TooManyParameters { index: usize, span: Span },
}

#[derive(Debug, Error)]
pub enum ExpressionError {
    #[error("unexpected token {found:?}")]
    UnexpectedToken {
        found: crate::lexer::TokenKind,
        span: Span,
    },

    #[error("missing token: {message}")]
    MissingToken { message: &'static str },

    #[error("bad prefix operator {op:?}")]
    BadPrefixOperator {
        op: crate::lexer::TokenKind,
        span: Span,
    },

    #[error("placeholder not evaluatable: {id:?}")]
    UnevaluatablePlaceholder {
        id: crate::expr::PlaceholderId,
        span: Span,
    },

    #[error("unknown identifier '{name}'")]
    UnknownIdentifier { name: String, span: Span },

    #[error("unsupported unary operator {op:?}")]
    UnsupportedUnaryOperator {
        op: crate::lexer::TokenKind,
        span: Span,
    },

    #[error("unsupported binary operator {op:?}")]
    UnsupportedBinaryOperator {
        op: crate::lexer::TokenKind,
        span: Span,
    },
}

#[derive(Debug, Error)]
pub enum SubcircuitError {
    #[error("missing subcircuit name")]
    MissingSubcircuitName { span: Option<Span> },

    #[error("subcircuit not found: {name}")]
    NotFound { name: String },

    #[error("subcircuit {name} has {found} nodes, expected {expected}")]
    ArityMismatch {
        name: String,
        found: usize,
        expected: usize,
    },

    #[error("subcircuit {name} has no nodes")]
    NoNodes { name: String, span: Span },

    #[error("invalid device model type: {s}")]
    InvalidDeviceModelType { s: String, span: Span },

    #[error("model {name} already exists")]
    ModelAlreadyExists { name: String, span: Span },
}

#[derive(Debug, Error)]
pub enum IncludeError {
    #[error("expected path")]
    ExpectedPath { span: Span },

    #[error("file not found: {path}\nchecked paths:\n{checked_paths:#?}")]
    FileNotFound {
        path: PathBuf,
        checked_paths: Vec<PathBuf>,
        span: Span,
    },

    #[error("IO error: {error}")]
    IOError {
        path: PathBuf,
        span: Span,
        error: std::io::Error,
    },

    #[error("maximum include depth exceeded at depth {depth}")]
    MaxDepthExceeded { span: Span, depth: usize },

    #[error("include cycle detected involving: {path}")]
    CycleDetected { span: Span, path: PathBuf },

    #[error("library section '{lib}' not found in: {path}")]
    LibSectionNotFound { span: Span, lib: String, path: PathBuf },
}
