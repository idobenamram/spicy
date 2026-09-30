//! The syntax tree (ast.md §4): what the text *says*, before names or units mean
//! anything. Elaboration (`spicy_model`, roadmap M1d) reads it and decides what each
//! name refers to.
//!
//! Three conventions hold for every type here:
//! - **`span`** is the byte range of exactly this node's text in the source, so an edit
//!   can replace a node by replacing `&src[span]` (decision A2).
//! - **Names borrow from the source** (`&'src str`): the tree lives next to the token list
//!   in [`Parsed`](super::Parsed) and copies nothing (A3).
//! - **Broken code is an `Error` variant** that keeps its span, so the rest of the tree is
//!   complete and every field is required, never "maybe missing" (A4).
//!
//! A node that can carry doc comments or attributes is split into a wrapper struct (the
//! parts every kind shares: docs, attributes, span) and a `…Kind` enum (the parts that
//! differ), as rustc does. That keeps each enum variant small and lets code that doesn't
//! care about the kind (the formatter, the editor) handle docs and spans once.

use spicy_errors::Reported;
use spicy_span::Span;

use crate::lexer::QuantityLit;

/// A name as written: `r1`, `Resistor`, `vcc`, a field name, a pin name.
///
/// Only the text: whether it's a part kind, a net or a port is decided in elaboration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ident<'src> {
    pub text: &'src str,
    pub span: Span,
}

/// A name that may be qualified: `vcc`, `std::prelude`, `onsemi::MMBT3904`. Used for
/// part kinds and block names in struct literals, for names in expressions, in types
/// and in attributes. Most paths have one segment.
#[derive(Clone, Debug, PartialEq)]
pub struct Path<'src> {
    pub segments: Vec<Ident<'src>>,
    pub span: Span,
}

/// A whole `.spl` file: its items in order. The span always covers the entire source.
#[derive(Clone, Debug, PartialEq)]
pub struct File<'src> {
    pub items: Vec<Item<'src>>,
    pub span: Span,
}

/// An item or a statement with the doc comments and attributes written above it (as
/// rustc's `ast::Item<K>` covers every kind of item). The span starts at the first doc
/// comment or attribute.
#[derive(Clone, Debug, PartialEq)]
pub struct Node<'src, K> {
    /// Spans of the `///` lines before it, in order (the text is `&src[span]`, including
    /// the `///`): an item's rationale (language §5.6), a part's or a spec's "why".
    pub docs: Vec<Span>,
    pub attrs: Vec<Attribute<'src>>,
    pub kind: K,
    pub span: Span,
}

/// One top-level item: a `block` or a `contract`.
pub type Item<'src> = Node<'src, ItemKind<'src>>;

/// What an item is. `block` and `contract` share their shape (a name and a list of
/// statements); which statements are allowed in which is checked by the parser
/// (`WrongBody`).
#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind<'src> {
    /// `block CeAmp { … }`: ports, nets and the parts inside.
    Block(Body<'src>),
    /// `contract CeAmp { … }`: the block's assumptions, measures and specs (language §8).
    Contract(Body<'src>),
    /// Text at the top level that couldn't be parsed as an item; the item's span covers
    /// all of it, up to the next `block` or `contract`.
    Error(Reported),
}

/// Which kind of body, without the body: the tag of `ItemKind::Block` and
/// `ItemKind::Contract`, for code that needs to know which one it's in (the parser's
/// `WrongBody` check) but not to hold it. Kept apart rather than folded into `ItemKind`
/// (as rustc keeps `DefKind` apart from `ItemKind`), so matching an item stays
/// `ItemKind::Block(body)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Block,
    Contract,
}

/// `Name { statements }`: the part of a `block` or `contract` after its keyword.
#[derive(Clone, Debug, PartialEq)]
pub struct Body<'src> {
    /// The block's name. A contract uses the name of the block it describes.
    pub name: Ident<'src>,
    pub stmts: Vec<Stmt<'src>>,
    /// The proof, if parsing it reported an error: then something written may be
    /// missing from it. An error found at the next item (a missing `}`) is the open
    /// body's, as rustc taints the body whose checking reported one, not the place the
    /// error points at.
    pub broken: Option<Reported>,
}

/// An attribute on an item or statement: `#[warn]`, `#[confidence(sigma(3))]`.
/// Kept as parsed; which attributes exist and what they mean is elaboration's job.
#[derive(Clone, Debug, PartialEq)]
pub struct Attribute<'src> {
    pub path: Path<'src>,
    /// `None` for `#[warn]`, `Some` for `#[confidence(…)]` (possibly empty: `#[a()]`).
    pub args: Option<Vec<Expr<'src>>>,
    pub span: Span,
}

/// One statement in a body. Its span ends after the `;` (or after the last token, when
/// the `;` is missing).
pub type Stmt<'src> = Node<'src, StmtKind<'src>>;

/// What a statement is (grammar.md §3).
#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind<'src> {
    /// `port vcc: Power<In>;`: a connection point on the block's boundary.
    Port { name: Ident<'src>, ty: Type<'src> },
    /// `net base;` declares an internal node; `net x = [a, b];` merges existing ones
    /// (`merge` is the right-hand side).
    Net {
        name: Ident<'src>,
        merge: Option<Expr<'src>>,
    },
    /// `let name = value;`. The same syntax places a part (`Resistor { … }`), places a
    /// block (`CeAmp { … }`) or defines a measure (`ac(…)`); elaboration tells them apart
    /// by what the value's path refers to (grammar.md §3).
    Let {
        name: Ident<'src>,
        value: Expr<'src>,
    },
    /// `assume vcc.v in 12V ± 5%;`: a condition the world may be anywhere in.
    Assume { relation: Relation<'src> },
    /// `spec gain: h.at(1kHz).mag() in 4.6 ± 5%;`: a requirement the design must meet.
    Spec {
        name: Ident<'src>,
        relation: Relation<'src>,
    },
    /// A statement that couldn't be parsed; the statement's span covers the skipped text.
    Error(Reported),
}

/// The top level of an `assume` or `spec`, split out so elaboration gets the measured
/// side, the relation and the bound directly: in `dc(output.v) in 4.5V..=6.5V`, `lhs` is
/// `dc(output.v)`, `op` is `In`, and `rhs` is the range. The two sides have spans; the
/// relation as a whole spans from `lhs` to `rhs`.
#[derive(Clone, Debug, PartialEq)]
pub struct Relation<'src> {
    pub lhs: Expr<'src>,
    pub op: RelOp,
    pub rhs: Expr<'src>,
}

/// The relations an `assume` or `spec` can use (grammar.md §4.1, level 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelOp {
    /// `x in a..=b` or `x in n ± t`: inside a range or tolerance.
    In,
    Lt,
    Le,
    Gt,
    Ge,
}

/// A port's type, with its span: `Power<In>`, `Ground`, `Analog<Out>`.
#[derive(Clone, Debug, PartialEq)]
pub struct Type<'src> {
    pub kind: TypeKind<'src>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind<'src> {
    /// A named type. `args` are the generic arguments (`In`); they're types too, so
    /// `Bus<Analog<In>>` nests.
    Path {
        path: Path<'src>,
        args: Vec<Type<'src>>,
    },
    /// A type that couldn't be parsed (`port a: ;`). The port keeps its name, so its
    /// uses aren't reported again.
    Error(Reported),
}

/// An expression with its span. Every value in the language is one: a number, a
/// name, a part literal, a measure, a range.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr<'src> {
    pub kind: ExprKind<'src>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind<'src> {
    /// `47k`, `1uF`, `3`: already decoded (value with the prefix applied, and the unit
    /// symbol). What the unit means for this field (ohms for a resistor's `value`) is
    /// elaboration's job.
    Quantity(QuantityLit),
    /// A name: `vcc`, `temp`, `a::b`.
    Path(Path<'src>),
    /// `Resistor { a: vcc, value: 47k }`: placing a part or a block, with its pins and
    /// fields bound by name.
    StructLit {
        path: Path<'src>,
        fields: Vec<Field<'src>>,
    },
    /// `(a + b)`. Kept rather than dropped, because it matters: `(a + b) ± 1%` is
    /// allowed and `a + b ± 1%` isn't (grammar.md §4.2), and the formatter prints it.
    Paren(Box<Expr<'src>>),
    /// `[a, b]`: a list, as in `net x = [a, b];`.
    Array(Vec<Expr<'src>>),
    /// `output.v`: a field access; also how probes are written (language §8.4).
    Field {
        base: Box<Expr<'src>>,
        name: Ident<'src>,
    },
    /// `dc(output.v)`, `h.at(1kHz)`: a call. For a method, the callee is a `Field`
    /// (`h.at`), so `h.at(1kHz).mag()` is `Call(Field(Call(Field(h, at), [1kHz]), mag))`.
    Call {
        callee: Box<Expr<'src>>,
        args: Vec<Expr<'src>>,
    },
    /// `-3dB`, `-x`
    Neg(Box<Expr<'src>>),
    /// `a + b`, `12V ± 5%`, `4.5V..=6.5V`, `x in r`: every binary operator, including
    /// tolerances and ranges (which are values, not special syntax).
    Binary {
        op: BinOp,
        lhs: Box<Expr<'src>>,
        rhs: Box<Expr<'src>>,
    },
    /// A value that couldn't be parsed, over the text skipped to the end of its
    /// statement: the statement keeps its name (`net base = [g g];` still declares
    /// `base`). Or a number the lexer rejected (`47q`), which the lexer reported.
    Error(Reported),
}

/// Binary operators, loosest first (grammar.md §4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    /// A comparison: `in`, `<`, `<=`, `>`, `>=`. Only valid at the top of an `assume` or
    /// `spec`, which takes it out as a typed [`Relation`].
    Rel(RelOp),
    /// `..=`
    Range,
    /// `±`, `+/-`
    Tol,
    Add,
    Sub,
    Mul,
    Div,
}

/// One field of a struct literal: a pin binding (`a: vcc`), a parameter
/// (`value: 47k ± 1%`), or the shorthand `gnd` for `gnd: gnd`.
#[derive(Clone, Debug, PartialEq)]
pub struct Field<'src> {
    pub name: Ident<'src>,
    /// `None` for the shorthand: the value is the net with the same name.
    pub value: Option<Expr<'src>>,
    pub span: Span,
}
