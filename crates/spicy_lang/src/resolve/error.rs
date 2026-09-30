//! Resolve errors (model.md E18 tier 1, E23). Data first, rendered on demand, like the
//! lexer's and parser's. Each is reported once, at the definition.

use spicy_model::prelude::FieldType;
use spicy_model::units::{Dimension, QKind, Quantity};

use spicy_errors::{Diag, DiagKind, Fix, Text, list};

/// One problem resolve found. `related` is the first definition of a duplicate.
pub type ResolveError = Diag<ResolveErrorKind>;

/// What a name is, for messages ("`vcc` is a port, not a part kind").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameKind {
    Block,
    PartKind,
    SignalType,
    Port,
    Net,
    Instance,
    /// A pin, a block's port as bound in a placement, or a part's field.
    Binding,
    Circuit,
    Contract,
    Setup,
    Value,
}

impl NameKind {
    /// With its article: "a port", "an instance".
    pub fn a(self) -> &'static str {
        match self {
            NameKind::Block => "a block",
            NameKind::PartKind => "a part kind",
            NameKind::SignalType => "a signal type",
            NameKind::Port => "a port",
            NameKind::Net => "a net",
            NameKind::Instance => "an instance",
            NameKind::Binding => "a pin or field",
            NameKind::Circuit => "a circuit",
            NameKind::Contract => "a contract",
            NameKind::Setup => "a setup",
            NameKind::Value => "a value",
        }
    }

    fn noun(self) -> &'static str {
        &self.a()[self.a().find(' ').unwrap() + 1..]
    }
}

/// Which namespace a name was looked up in (model.md E5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Namespace {
    /// Part kinds, blocks, signal types.
    Kind,
    /// Ports, nets, instances.
    Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResolveErrorKind {
    /// A name found nowhere. `suggestion` is a close name in the same namespace.
    UnknownName {
        name: String,
        namespace: Namespace,
        suggestion: Option<String>,
    },
    /// A name found as the wrong kind of thing: `vcc` used as a part kind, `Power` as a
    /// part kind, `Resistor` as a port type.
    WrongNamespace {
        name: String,
        is: NameKind,
        expected: &'static str,
    },
    /// Two of a name where one is allowed: two blocks, two ports, a net and an instance
    /// named alike, two circuits or contracts for one block, two setups of one name for
    /// one block, a pin bound twice.
    Duplicate { name: String, what: NameKind },
    /// The block a `circuit`, `contract` or `setup` (`item`) is for doesn't exist.
    UnknownBlock { item: NameKind, name: String },
    /// A placed block that has no `circuit`: there's nothing inside it to place.
    NoCircuit { block: String },
    /// A port type that isn't a signal type (`Bus`), or with the wrong arguments.
    BadSignalType { name: String, problem: &'static str },
    /// A `let` in a block whose value isn't a part or a block (`let x = 5k;`).
    LetNotInstance,
    /// A pin or field that the part kind or block doesn't have. `suggestion` is the one
    /// probably meant: the single pin or field left unset (`resistance:` for `value:`),
    /// or a close spelling among those not given yet.
    UnknownField {
        field: String,
        of: String,
        valid: Vec<String>,
        suggestion: Option<String>,
    },
    /// Pins or ports not bound, or required fields not given, all in one error.
    Missing { of: String, names: Vec<String> },
    /// Something bound or merged that isn't a net (`a: 47k`, `a: r1`, `[a, 5k]`). `pin`
    /// is the pin, or `None` for an item of a merge list.
    NotANet {
        pin: Option<String>,
        found: NameKind,
    },
    /// `net x = …` whose value isn't a list of nets.
    BadMerge,
    /// A value of the wrong unit: what the position takes, and what was written.
    UnitMismatch {
        expected: FieldType,
        found: Quantity,
    },
    /// Adding quantities of different units (`5k + 3V`), or a plain number to one with
    /// a unit (`1k + 5`).
    MixedUnits { left: String, right: String },
    /// A temperature or a dB level in arithmetic (`10°C * 2`, `2 * 3dB`). `what` is
    /// "temperatures" or "dB levels".
    NotArithmetic { what: &'static str },
    /// Something that isn't a number in a value position (a name, a call).
    NotAValue,
    /// A tolerance or range on a field that takes an exact value (a rating).
    SpreadNotAllowed { field: String },
    /// A tolerance or range in arithmetic other than scaling (`(1k ± 1%) + 1k`).
    SpreadInArithmetic,
    /// A range whose bounds are the wrong way round (`300..=100`).
    RangeReversed,
    /// `± -1%`: a tolerance is a distance, never negative.
    NegativeTolerance,
    /// `1k / 0`.
    DivisionByZero,
    /// Syntax the parser reads but resolve doesn't handle yet: generic arguments at a
    /// placement. `what` names it.
    Unsupported { what: &'static str },
}

/// The variant names, for the test that every error kind has a case file. Only tests
/// use them; [`DiagKind::code`] is the identifier users see.
#[cfg(any(test, fuzzing))]
impl ResolveErrorKind {
    /// Every value [`name`](Self::name) can return; `name`'s match is exhaustive, so a
    /// new variant won't compile until it has a name. Add it here too.
    pub const ALL_NAMES: &'static [&'static str] = &[
        "UnknownName",
        "WrongNamespace",
        "Duplicate",
        "UnknownBlock",
        "NoCircuit",
        "BadSignalType",
        "LetNotInstance",
        "UnknownField",
        "Missing",
        "NotANet",
        "BadMerge",
        "UnitMismatch",
        "MixedUnits",
        "NotArithmetic",
        "NotAValue",
        "SpreadNotAllowed",
        "SpreadInArithmetic",
        "RangeReversed",
        "NegativeTolerance",
        "DivisionByZero",
        "Unsupported",
    ];
}

impl DiagKind for ResolveErrorKind {
    fn name(&self) -> &'static str {
        use ResolveErrorKind::*;
        match self {
            UnknownName { .. } => "UnknownName",
            WrongNamespace { .. } => "WrongNamespace",
            Duplicate { .. } => "Duplicate",
            UnknownBlock { .. } => "UnknownBlock",
            NoCircuit { .. } => "NoCircuit",
            BadSignalType { .. } => "BadSignalType",
            LetNotInstance => "LetNotInstance",
            UnknownField { .. } => "UnknownField",
            Missing { .. } => "Missing",
            NotANet { .. } => "NotANet",
            BadMerge => "BadMerge",
            UnitMismatch { .. } => "UnitMismatch",
            MixedUnits { .. } => "MixedUnits",
            NotArithmetic { .. } => "NotArithmetic",
            NotAValue => "NotAValue",
            SpreadNotAllowed { .. } => "SpreadNotAllowed",
            SpreadInArithmetic => "SpreadInArithmetic",
            RangeReversed => "RangeReversed",
            NegativeTolerance => "NegativeTolerance",
            DivisionByZero => "DivisionByZero",
            Unsupported { .. } => "Unsupported",
        }
    }

    fn code(&self) -> &'static str {
        use ResolveErrorKind::*;
        match self {
            UnknownName { .. } | WrongNamespace { .. } | UnknownBlock { .. } => "E-name",
            Duplicate { .. } => "E-duplicate",
            NoCircuit { .. } => "E-circuit",
            BadSignalType { .. } => "E-type",
            LetNotInstance => "E-let",
            UnknownField { .. } | Missing { .. } | NotANet { .. } | BadMerge => "E-binding",
            UnitMismatch { .. } | MixedUnits { .. } | NotArithmetic { .. } => "E-unit",
            NotAValue
            | SpreadNotAllowed { .. }
            | SpreadInArithmetic
            | RangeReversed
            | NegativeTolerance
            | DivisionByZero => "E-value",
            Unsupported { .. } => "E-unsupported",
        }
    }

    fn related_label(&self) -> &'static str {
        match self {
            ResolveErrorKind::Duplicate { what, .. } => match what {
                NameKind::Circuit => "first circuit here",
                NameKind::Contract => "first contract here",
                NameKind::Binding => "first given here",
                _ => "first defined here",
            },
            _ => "",
        }
    }

    fn text(&self, _fix: Option<&Fix>) -> Text {
        use ResolveErrorKind::*;
        match self {
            UnknownName {
                name,
                namespace,
                suggestion,
            } => {
                let what = match namespace {
                    Namespace::Kind => "part kind or block",
                    Namespace::Value => "net, port or instance",
                };
                let notes = suggestion
                    .iter()
                    .map(|s| format!("help: did you mean `{s}`?"))
                    .collect();
                (
                    format!("unknown {what} `{name}`"),
                    "not defined".to_string(),
                    notes,
                )
            }
            WrongNamespace { name, is, expected } => (
                format!("`{name}` is {}, not {expected}", is.a()),
                format!("expected {expected}"),
                vec![],
            ),
            Duplicate { name, what } => match what {
                // One of each per block.
                NameKind::Circuit | NameKind::Contract => (
                    format!("block `{name}` has two {}s", what.noun()),
                    format!("second {}", what.noun()),
                    vec![format!("help: merge them into one `{}`", what.noun())],
                ),
                NameKind::Binding => (
                    format!("`{name}` is given twice"),
                    "second time".to_string(),
                    vec![],
                ),
                NameKind::Block | NameKind::Port | NameKind::Setup => (
                    format!("{} `{name}` is defined twice", what.noun()),
                    "second definition".to_string(),
                    vec![],
                ),
                // Nets and instances share a namespace with ports (model.md E5).
                _ => (
                    format!("name `{name}` is defined twice"),
                    "second definition".to_string(),
                    vec![],
                ),
            },
            UnknownBlock { item, name } => {
                let note = match item {
                    NameKind::Contract => "note: a contract describes the block of the same name",
                    NameKind::Setup => "note: a setup is for the block named after `for`",
                    _ => "note: a circuit is the inside of the block of the same name",
                };
                (
                    format!("{} for an unknown block `{name}`", item.noun()),
                    "no block with this name".to_string(),
                    vec![note.to_string()],
                )
            }
            NoCircuit { block } => (
                format!("block `{block}` has no circuit"),
                "placed here".to_string(),
                vec![format!(
                    "help: write `circuit {block} {{ … }}` with its nets and parts"
                )],
            ),
            BadSignalType { name, problem } => (
                format!("`{name}` {problem}"),
                "not a valid port type".to_string(),
                vec![
                    "note: port types are `Pin`, `Ground`, `Power<In|Out>`, `Analog<In|Out>`"
                        .to_string(),
                ],
            ),
            LetNotInstance => (
                "in a block, `let` places a part or a block".to_string(),
                "not a part or block".to_string(),
                vec![
                    "help: write `let name = Kind { … };`; measures belong in the contract"
                        .to_string(),
                ],
            ),
            UnknownField {
                field,
                of,
                valid,
                suggestion,
            } => {
                let mut notes = Vec::new();
                if let Some(s) = suggestion {
                    notes.push(format!("help: did you mean `{s}`?"));
                }
                notes.push(format!("note: `{of}` has {}", list(valid)));
                (
                    format!("`{of}` has no pin or field `{field}`"),
                    "unknown".to_string(),
                    notes,
                )
            }
            Missing { of, names } => (
                format!("`{of}` is missing {}", list(names)),
                format!("needs {}", list(names)),
                vec![],
            ),
            NotANet { pin, found } => (
                match pin {
                    Some(pin) => format!("pin `{pin}` must be bound to a net or port"),
                    None => "a merge lists nets or ports".to_string(),
                },
                format!("this is {}", found.a()),
                vec![],
            ),
            BadMerge => (
                "`net x = …` merges nets: write a list of nets".to_string(),
                "not a list of nets".to_string(),
                vec!["help: `net x = [a, b];`".to_string()],
            ),
            UnitMismatch { expected, found } => {
                let notes = if self.kelvin_for_kilo() {
                    vec!["help: `K` is kelvin; kilo is lowercase `k`".to_string()]
                } else {
                    vec![]
                };
                let (expected, found) = (
                    describe_expected(*expected),
                    describe(found.kind, found.dim),
                );
                (
                    format!("expected {expected}, found {found}"),
                    format!("this is {found}"),
                    notes,
                )
            }
            MixedUnits { left, right } => (
                format!("can't add {left} and {right}"),
                "different units".to_string(),
                vec![],
            ),
            NotArithmetic { what } => (
                format!("{what} can't be used in arithmetic here"),
                "not a plain quantity".to_string(),
                vec![],
            ),
            NotAValue => (
                "expected a number".to_string(),
                "not a number".to_string(),
                vec![],
            ),
            SpreadNotAllowed { field } => (
                format!("`{field}` takes an exact value"),
                "a tolerance or range".to_string(),
                vec![],
            ),
            SpreadInArithmetic => (
                "a tolerance or range can only be scaled".to_string(),
                "compute the nominal first, then add the spread".to_string(),
                vec!["note: `2 * (1k ± 1%)` works; adding to a spread doesn't".to_string()],
            ),
            RangeReversed => (
                "range bounds are the wrong way round".to_string(),
                "the lower bound comes first".to_string(),
                vec![],
            ),
            NegativeTolerance => (
                "a tolerance can't be negative".to_string(),
                "`±` already means both directions".to_string(),
                vec![],
            ),
            DivisionByZero => (
                "division by zero".to_string(),
                "the divisor is zero".to_string(),
                vec![],
            ),
            Unsupported { what } => (
                format!("not supported yet: {what}"),
                "not supported yet".to_string(),
                vec![],
            ),
        }
    }
}

impl ResolveErrorKind {
    /// A `UnitMismatch` whose value is in kelvin: a plain quantity in kelvin comes only
    /// from a `K` (`°C` is a temperature point), which was probably meant as kilo.
    pub fn kelvin_for_kilo(&self) -> bool {
        matches!(self, ResolveErrorKind::UnitMismatch { found, .. }
            if found.kind == QKind::Plain && found.dim == Dimension::KELVIN)
    }
}

/// What a position expects, for `UnitMismatch`: "a temperature (`°C` or `K`)".
fn describe_expected(t: FieldType) -> String {
    match t.kind {
        QKind::TempPoint => "a temperature (`°C` or `K`)".to_string(),
        _ => describe(t.kind, t.dim),
    }
}

/// A quantity's type, for messages: "`Ω` (a resistance)", "a level in `dB`".
pub(super) fn describe(kind: QKind, dim: Dimension) -> String {
    match kind {
        QKind::TempPoint => "a temperature".to_string(),
        QKind::Db => "a level in `dB`".to_string(),
        QKind::Plain if dim.is_none() => "a plain number".to_string(),
        QKind::Plain => match dim.quantity_name() {
            Some(name) => format!("`{}` ({name})", dim.symbol()),
            None => format!("`{}`", dim.symbol()),
        },
    }
}
