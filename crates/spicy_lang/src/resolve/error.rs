//! Resolve errors (model.md E18 tier 1, E23). Data first, rendered on demand, like the
//! lexer's and parser's. Each is reported once, at the definition.

use spicy_model::measure::{MeasureType, Method};
use spicy_model::prelude::{FieldType, Shape, SignalType};
use spicy_model::units::{Dimension, QKind, Quantity, SHOWN_DIGITS, significant, to_celsius};

use spicy_errors::{Diag, DiagKind, Fix, Severity, Text, list};

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
    /// A field of a setup's shape (`Supply`'s `v`).
    Field,
    Circuit,
    Contract,
    Setup,
    /// A contract's `setup = S;`.
    DefaultSetup,
    /// A contract's `let`: a measure.
    Measure,
    /// A contract's `spec`.
    Spec,
    Env,
    Const,
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
            NameKind::Field => "a field",
            NameKind::Circuit => "a circuit",
            NameKind::Contract => "a contract",
            NameKind::Setup => "a setup",
            NameKind::DefaultSetup => "a default setup",
            NameKind::Measure => "a measure",
            NameKind::Spec => "a spec",
            NameKind::Env => "an env",
            NameKind::Const => "a const",
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
    /// Ports, nets, instances; and the file's envs and consts.
    Value,
    /// The types an `env` or `const` declares (`Temperature`, `Ohm`).
    Type,
    /// The shapes a setup puts on a port (`Supply`), prelude names like the part kinds,
    /// looked up only where a shape goes (`vcc: Supply { … }`).
    Shape,
    /// A block's setups, which a contract's `setup = S;` names.
    Setup,
    /// The measure table's functions and methods (`dc`, `.mag()`).
    Function,
    /// The probes of a net (`.v`, language.md §8.4).
    Probe,
    /// What a name can be where a measure goes (`h` in `h.at(1kHz)`): a contract's
    /// measures and the file's consts, not a net.
    Measure,
}

/// Why a setup entry doesn't fit its port's role ([`ResolveErrorKind::WrongRole`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleMismatch {
    /// The shape written, which is the other direction's (`vcc: Load {}` on an input).
    Shape(Shape),
    /// Any entry on a ground: it's the reference.
    Ground,
    /// A source's field on an output (`output.v`): what the block itself drives.
    Drives(&'static str),
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
        /// What `of` has: pins and fields (`Binding`) for a part or block, fields for a
        /// shape, ports for a setup's block.
        what: NameKind,
        /// The first of `of`'s pins, fields or ports, in order, and how many more there
        /// are: a block can have thousands of ports, and each error would copy them all.
        valid: Vec<String>,
        unlisted: usize,
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
    /// `env t: Temperature in 25°C;`: an env is a range the engine searches; a fixed
    /// value is a `const`.
    EnvNeedsRange { name: String },
    /// A temperature's spread that is a temperature, which is a point (`25°C ± T_ROOM`),
    /// not a difference. `± 5K` and `± 5°C` are both 5 K (model.md E13).
    TemperatureSpread,
    /// A percentage spread on a value whose zero is arbitrary, so which percentage is
    /// meant isn't written (model.md E13): `25°C ± 10%` is 2.5 K of the °C reading or
    /// 29.8 K of kelvin; `12dB ± 10%` is 1.2 dB of the level, or 10% of the gain.
    /// `kind` is the value's (`TempPoint` or `Db`), `nominal` its SI value if it typed,
    /// and `percent` the spread's.
    PercentSpread {
        kind: QKind,
        nominal: Option<f64>,
        percent: f64,
    },
    /// A temperature written as a plain number (`25`) or in K (`300K`, which is a
    /// difference): a temperature point is written in `°C` (model.md E13).
    TemperaturePoint { kelvin: bool },
    /// A setup entry that doesn't fit its port's role (language.md §8.2): a shape for the
    /// other direction, anything on a ground, or a quantity the block drives.
    WrongRole {
        port: String,
        signal: SignalType,
        wrong: RoleMismatch,
    },
    /// A setup that leaves out a port with a role, a source's voltage or `temp`, all
    /// listed in one error (v5 rule 1.3.1).
    IncompleteSetup { setup: String, missing: Vec<String> },
    /// `v: ..=5V` in a setup: a voltage or a current needs both ends of its range. Only
    /// a field that can't be negative starts at 0 (`z: ..=0.5Ω`, plan §2.9 b).
    OpenRange { field: String },
    /// A port given a value where it takes a shape (`vcc: 12V`). `takes` is the shape
    /// its role takes.
    NotAShape { port: String, takes: Shape },
    /// A contract with no `setup = S;`: its specs have nothing to be checked in.
    /// `setups` are its block's, in file order.
    NoDefaultSetup {
        contract: String,
        setups: Vec<String>,
    },
    /// A contract's `let` that measures nothing: a part (`Resistor { … }`) or a constant
    /// (`5V`). `is` says which.
    NotAMeasure { name: String, is: &'static str },
    /// A probe outside an analysis (`let x = output.v;`, plan §2.9 a): fixed to
    /// `dc(output.v)` when it's the whole value.
    NeedsAnalysis,
    /// A method on a measure it doesn't apply to: `h.mag()` on a response (it needs
    /// `.at(f)` first), `dc(output.v).db()` (volts aren't a ratio).
    WrongMeasureType { method: Method, on: MeasureType },
    /// A function or method given the wrong arguments: `h.at()`, `dc(a.v, b.v)`,
    /// `h.f_low(3dB)`. `function` is as written, a method with its dot (`.at()`);
    /// `expected` says what it takes.
    BadArguments {
        function: &'static str,
        expected: &'static str,
    },
    /// `ac(…)` of anything but a net's voltage over an input port's: no source would be
    /// excited.
    NoExcitation,
    /// A measure defined in terms of itself, directly or through other measures:
    /// `cycle` is each measure on the way, from the one met again.
    MeasureCycle { cycle: Vec<String> },
    /// A spec whose measure isn't one number: `spec x: h within …` on a response, or
    /// `h.at(1kHz)`, a phasor.
    NotANumber { spec: String, on: MeasureType },
    /// `<` or `>` as a spec's limit (plan §2.7): read as `<=` or `>=`, its fix.
    StrictLimit,
    /// `within` with one value: one number (`within 5V`), or a spread that doesn't vary
    /// (`within 5V ± 0%`, `within 5V..=5V`), as an env's `EnvNeedsRange`.
    WithinNeedsRange,
    /// `within ..=6.5V`: an open range is a one-sided limit, written as one, its fix
    /// (`<= 6.5V`) when its end is sound.
    OpenRangeLimit,
    /// A `pub spec` that measures an internal net, directly or through a measure
    /// (language.md §8.3): a parent can't rely on it. The fix removes `pub`.
    PublishedInternal {
        spec: String,
        net: String,
        block: String,
    },
    /// A warning: a contract with specs for a block that has no circuit, which nothing
    /// can check (plan §3.1).
    UncheckedSpecs { block: String },
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
        "EnvNeedsRange",
        "TemperatureSpread",
        "PercentSpread",
        "TemperaturePoint",
        "WrongRole",
        "IncompleteSetup",
        "OpenRange",
        "NotAShape",
        "NoDefaultSetup",
        "NotAMeasure",
        "NeedsAnalysis",
        "WrongMeasureType",
        "BadArguments",
        "NoExcitation",
        "MeasureCycle",
        "NotANumber",
        "StrictLimit",
        "WithinNeedsRange",
        "OpenRangeLimit",
        "PublishedInternal",
        "UncheckedSpecs",
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
            EnvNeedsRange { .. } => "EnvNeedsRange",
            TemperatureSpread => "TemperatureSpread",
            PercentSpread { .. } => "PercentSpread",
            TemperaturePoint { .. } => "TemperaturePoint",
            WrongRole { .. } => "WrongRole",
            IncompleteSetup { .. } => "IncompleteSetup",
            OpenRange { .. } => "OpenRange",
            NotAShape { .. } => "NotAShape",
            NoDefaultSetup { .. } => "NoDefaultSetup",
            NotAMeasure { .. } => "NotAMeasure",
            NeedsAnalysis => "NeedsAnalysis",
            WrongMeasureType { .. } => "WrongMeasureType",
            BadArguments { .. } => "BadArguments",
            NoExcitation => "NoExcitation",
            MeasureCycle { .. } => "MeasureCycle",
            NotANumber { .. } => "NotANumber",
            StrictLimit => "StrictLimit",
            WithinNeedsRange => "WithinNeedsRange",
            OpenRangeLimit => "OpenRangeLimit",
            PublishedInternal { .. } => "PublishedInternal",
            UncheckedSpecs { .. } => "UncheckedSpecs",
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
            EnvNeedsRange { .. } => "E-value",
            TemperatureSpread | PercentSpread { .. } | TemperaturePoint { .. } => "E-unit",
            WrongRole { .. } => "E-role",
            IncompleteSetup { .. } | NotAShape { .. } => "E-setup",
            NoDefaultSetup { .. } | PublishedInternal { .. } => "E-contract",
            NotAMeasure { .. }
            | NeedsAnalysis
            | WrongMeasureType { .. }
            | BadArguments { .. }
            | NoExcitation
            | MeasureCycle { .. }
            | NotANumber { .. } => "E-measure",
            OpenRange { .. } => "E-value",
            StrictLimit | WithinNeedsRange | OpenRangeLimit => "E-limit",
            UncheckedSpecs { .. } => "W-unchecked",
        }
    }

    fn severity(&self) -> Severity {
        match self {
            ResolveErrorKind::UncheckedSpecs { .. } => Severity::Warning,
            _ => Severity::Error,
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
            ResolveErrorKind::PublishedInternal { .. } => "the internal net",
            ResolveErrorKind::UncheckedSpecs { .. } => "has no circuit",
            _ => "",
        }
    }

    fn text(&self, fix: Option<&Fix>) -> Text {
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
                    Namespace::Type => "type",
                    Namespace::Shape => "shape",
                    Namespace::Setup => "setup",
                    Namespace::Function => "function",
                    Namespace::Probe => "probe",
                    Namespace::Measure => "measure or const",
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
                NameKind::DefaultSetup => (
                    "the contract names its default setup twice".to_string(),
                    "second `setup = …;`".to_string(),
                    vec![],
                ),
                NameKind::Block | NameKind::Port | NameKind::Setup | NameKind::Spec => (
                    format!("{} `{name}` is defined twice", what.noun()),
                    "second definition".to_string(),
                    vec![],
                ),
                // Nets and instances share a namespace with ports, and envs with consts
                // (model.md E5).
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
                what,
                valid,
                unlisted,
                suggestion,
            } => {
                let mut notes = Vec::new();
                if let Some(s) = suggestion {
                    notes.push(format!("help: did you mean `{s}`?"));
                }
                let has = match unlisted {
                    0 => list(valid),
                    _ => format!("{}, … and {unlisted} more", list_quoted(valid)),
                };
                notes.push(format!("note: `{of}` has {has}"));
                (
                    format!("`{of}` has no {} `{field}`", what.noun()),
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
            EnvNeedsRange { name } => (
                format!("`env {name}` needs a range"),
                "a single value".to_string(),
                vec![
                    "help: an env is a range the engine searches, like `-10°C..=60°C`; a fixed value is a `const`"
                        .to_string(),
                ],
            ),
            TemperatureSpread => (
                "a temperature's spread is a difference, like `5K` or `5°C`".to_string(),
                "a temperature, which is a point".to_string(),
                vec![],
            ),
            PercentSpread {
                kind,
                nominal,
                percent,
            } => percent_spread(*kind, *nominal, *percent),
            TemperaturePoint { kelvin } => (
                "a temperature is written in `°C`".to_string(),
                match kelvin {
                    true => "this is in `K`, which is a temperature difference".to_string(),
                    false => "a plain number".to_string(),
                },
                write_fix(fix),
            ),
            WrongRole {
                port,
                signal,
                wrong,
            } => match wrong {
                RoleMismatch::Shape(written) => {
                    let takes = Shape::for_role(*signal);
                    // `Supply` and `Signal` are both sources: one is just another port
                    // type's, not the other direction's.
                    let label = match (written, takes) {
                        (Shape::Supply, Some(Shape::Signal)) => "a `Power<In>`'s shape",
                        (Shape::Signal, Some(Shape::Supply)) => "an `Analog<In>`'s shape",
                        _ => "the other direction's shape",
                    };
                    let takes = takes.map_or("nothing", Shape::name);
                    (
                        format!("`{port}: {signal}` takes a `{takes}`, not a `{}`", written.name()),
                        label.to_string(),
                        write_fix(fix),
                    )
                }
                RoleMismatch::Ground => (
                    format!("`{port}` is ground: a setup sets nothing on it"),
                    "the reference, 0 V by definition".to_string(),
                    vec![],
                ),
                RoleMismatch::Drives(field) => (
                    "a setup sets a quantity this block drives".to_string(),
                    format!(
                        "`{port}: {signal}`: this block drives `{port}.{field}`; a setup gives it a `Load {{ r, c, i }}`"
                    ),
                    vec![format!(
                        "help: a range on a quantity you drive belongs in a spec: `spec …: dc({port}.{field}) within …;`"
                    )],
                ),
            },
            IncompleteSetup { setup, missing } => (
                format!("setup `{setup}` leaves out {}", list(missing)),
                "incomplete".to_string(),
                vec![
                    "note: every port with a role is set, a source's voltage has no ideal default, and `temp` isn't assumed (language.md §8.2)"
                        .to_string(),
                ],
            ),
            OpenRange { field } => (
                format!("`{field}` needs both ends of its range"),
                "no lower end".to_string(),
                vec![
                    "note: only a resistance, an impedance or a capacitance starts at 0 when its lower end is left out"
                        .to_string(),
                ],
            ),
            NotAShape { port, takes } => {
                let first = takes.fields()[0].name;
                (
                    format!("`{port}` takes a shape, not a value"),
                    "a value".to_string(),
                    vec![format!(
                        "help: give it the whole shape, `{port}: {} {{ {first}: … }}`, or one field, `{port}.{first}: …`",
                        takes.name()
                    )],
                )
            }
            NoDefaultSetup { contract, setups } => {
                let help = match setups.as_slice() {
                    [] => format!(
                        "help: `{contract}` has no setup yet: write `setup S for {contract} {{ … }}`, then name it here, `setup = S;`"
                    ),
                    [setup] => {
                        format!("help: name the setup its specs are checked in: `setup = {setup};`")
                    }
                    _ => format!(
                        "help: name the setup its specs are checked in, one of {}",
                        list_quoted(setups)
                    ),
                };
                (
                    format!("contract `{contract}` has no default setup"),
                    "no `setup = …;`".to_string(),
                    vec![help],
                )
            }
            NotAMeasure { name, is } => (
                format!("`{name}` isn't a measure"),
                is.to_string(),
                vec![
                    "help: a contract's `let` measures the design, like `dc(out.v)` or `ac(out.v / in.v)`; parts go in the circuit"
                        .to_string(),
                ],
            ),
            NeedsAnalysis => (
                "a probe is measured in an analysis".to_string(),
                "a probe".to_string(),
                match fix {
                    Some(_) => write_fix(fix),
                    None => vec![
                        "help: `dc(…)` gives its operating point, and `ac(out.v / in.v)` its response to an input"
                            .to_string(),
                    ],
                },
            ),
            WrongMeasureType { method, on } => {
                let takes = match method {
                    Method::At { .. } => "a response, from `ac(…)`",
                    Method::Mag => "a phasor, from `.at(f)`",
                    Method::Db => "a phasor or a number with no unit",
                    Method::FLow { .. } | Method::FHigh { .. } => {
                        "a response with no unit, a gain from `ac(out.v / in.v)`"
                    }
                };
                let method = method.name();
                (
                    format!("`.{method}()` doesn't apply to {}", describe_measure(*on)),
                    describe_measure(*on),
                    vec![format!("note: `.{method}()` takes {takes}")],
                )
            }
            BadArguments { function, expected } => (
                format!("`{function}` takes {expected}"),
                "wrong arguments".to_string(),
                write_fix(fix),
            ),
            NoExcitation => (
                "`ac` measures a net's voltage over an input port's".to_string(),
                "nothing here is excited".to_string(),
                vec![
                    "help: write `ac(out.v / in.v)`, where `in` is an input port: the setup's source there gets the AC stimulus"
                        .to_string(),
                ],
            ),
            MeasureCycle { cycle } => {
                let name = &cycle[0];
                // `a` reads `b`, `b` reads `c`, and `c` reads `a`.
                let reads: Vec<String> = (cycle.iter().zip(cycle.iter().cycle().skip(1)))
                    .map(|(reader, read)| format!("`{reader}` reads `{read}`"))
                    .collect();
                let notes = match reads.as_slice() {
                    [_] => vec![],
                    [first @ .., last] => {
                        vec![format!("note: {}, and {last}", first.join(", "))]
                    }
                    [] => unreachable!("a cycle has a measure"),
                };
                (
                    format!("measure `{name}` is defined in terms of itself"),
                    format!("this leads back to `{name}`"),
                    notes,
                )
            }
            NotANumber { spec, on } => {
                let help = match on {
                    MeasureType::Phasor(_) => "help: take its magnitude: `.mag()`, or `.db()`",
                    _ => {
                        "help: reduce it to one number: `.at(1kHz).mag()`, `.at(1kHz).db()` or `.f_low(-3dB)`"
                    }
                };
                (
                    format!(
                        "spec `{spec}` checks one number, not {}",
                        describe_measure(*on)
                    ),
                    describe_measure(*on),
                    vec![help.to_string()],
                )
            }
            StrictLimit => (
                "a spec's limit is `<=` or `>=`".to_string(),
                "strict".to_string(),
                [
                    write_fix(fix),
                    vec![
                        "note: a value equal to the bound is inside the simulator's numerical noise, so a strict limit can't mean more"
                            .to_string(),
                    ],
                ]
                .concat(),
            ),
            WithinNeedsRange => (
                "`within` takes a range or a tolerance".to_string(),
                "one value".to_string(),
                vec![
                    "help: write a range, `4.5V..=6.5V`, or a tolerance, `5.5V ± 1V`".to_string(),
                ],
            ),
            OpenRangeLimit => (
                "an open range is a one-sided limit".to_string(),
                "open range".to_string(),
                match fix {
                    Some(_) => write_fix(fix),
                    None => vec!["help: write it with `<=` or `>=`".to_string()],
                },
            ),
            PublishedInternal { spec, net, block } => (
                format!("`{spec}` can't be `pub`"),
                "published".to_string(),
                vec![
                    format!(
                        "note: it measures `{net}`, a net of `circuit {block}`; a parent can't rely on it"
                    ),
                    "help: remove `pub`: an internal spec is still checked".to_string(),
                ],
            ),
            UncheckedSpecs { block } => (
                format!("`{block}`'s specs aren't checked: it has no circuit"),
                "not checked".to_string(),
                vec![
                    "note: a block with no circuit is an interface; nothing simulates it"
                        .to_string(),
                ],
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

/// The text of `PercentSpread`: which readings the percentage has, worked out on the
/// value written, and how to write each one so it's unambiguous.
fn percent_spread(kind: QKind, nominal: Option<f64>, percent: f64) -> Text {
    // A spread is a distance: `± -5%` reads as 5%, so no help suggests a negative one.
    let percent = percent.abs();
    let p = significant(percent, SHOWN_DIGITS);
    let of = |x: f64| significant(x.abs() * percent / 100.0, 3);
    let nominal = nominal.map(|x| significant(x, SHOWN_DIGITS));
    // `tolerance` reports a percentage on a temperature or a level, never on a plain
    // quantity, whose zero is zero.
    let what = match kind {
        QKind::TempPoint => "a temperature's",
        _ => "a level's",
    };
    let notes = match (kind, nominal) {
        (QKind::TempPoint, Some(kelvin)) => {
            let celsius = to_celsius(kelvin);
            let (reading, absolute) = (of(celsius), of(kelvin));
            // A reading whose base is 0 (0 °C, or 0 K) has no spread to offer: a spread
            // of 0 doesn't vary, which is an error of its own.
            let spreads: Vec<String> = [reading, absolute]
                .into_iter()
                .filter(|&spread| spread != 0.0)
                .map(|spread| format!("`± {spread}K`"))
                .collect();
            vec![
                format!(
                    "note: {p}% of {celsius} °C is {reading} K, but {p}% of {kelvin} K is {absolute} K: which one is meant isn't written"
                ),
                format!(
                    "help: write the spread as a difference, {}",
                    spreads.join(" or ")
                ),
            ]
        }
        (QKind::TempPoint, None) => vec![
            "note: a percentage of a temperature depends on where its zero is, 0 °C or 0 K"
                .to_string(),
            "help: write the spread as a difference, like `± 5K`".to_string(),
        ],
        (_, Some(db)) => {
            let mut notes = Vec::new();
            // A percentage of 0 dB is nothing: only the gain's reading is left.
            if db != 0.0 {
                notes.push(format!("help: for a spread in dB, write `{db}dB ± {}dB`", of(db)));
            }
            let gain = significant(10f64.powf(db / 20.0), 3);
            notes.push(format!(
                "help: for {p}% of the gain, write the spec on `.mag()`, the gain as a ratio: `within {gain} ± {p}%`"
            ));
            notes
        }
        (_, None) => vec![
            "help: write the spread in dB, like `± 1dB`, or the spec on `.mag()`, the gain as a ratio, with the percentage"
                .to_string(),
        ],
    };
    (
        format!("{what} spread can't be a percentage"),
        "a percentage".to_string(),
        notes,
    )
}

/// `names` quoted and separated by commas: `` `a`, `b` ``.
fn list_quoted(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    quoted.join(", ")
}

/// The help that shows a kind's fix, when it has one: "help: write `5K`".
fn write_fix(fix: Option<&Fix>) -> Vec<String> {
    fix.map(|fix| format!("help: write `{}`", fix.replacement))
        .into_iter()
        .collect()
}

/// What a measure is, for messages: "a response", "a number in `V`".
fn describe_measure(ty: MeasureType) -> String {
    match ty {
        MeasureType::Probe(_) => "a probe".to_string(),
        MeasureType::Number { dim, kind } => format!("a number, {}", describe(kind, dim)),
        MeasureType::Response(_) => "a response over frequency".to_string(),
        MeasureType::Phasor(_) => "a phasor (a response at one frequency)".to_string(),
    }
}

/// What a position expects, for `UnitMismatch`: "a temperature (in `°C`)".
fn describe_expected(t: FieldType) -> String {
    match t.kind {
        QKind::TempPoint => "a temperature (in `°C`)".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `PercentSpread`'s help gives spreads that are valid and mean one reading: no float
    /// noise (-40 °C is 233.14999999999998 K as a float), no negative spread for `± -5%`,
    /// and no spread of 0 for the reading whose zero the value is.
    #[test]
    fn percent_spread_helps_with_valid_spreads() {
        let notes = |kind, nominal, percent| percent_spread(kind, Some(nominal), percent).2;
        let celsius = |c| Quantity::celsius(c).si;
        assert_eq!(
            notes(QKind::TempPoint, celsius(-40.0), 5.0),
            [
                "note: 5% of -40 °C is 2 K, but 5% of 233.15 K is 11.7 K: which one is meant isn't written",
                "help: write the spread as a difference, `± 2K` or `± 11.7K`",
            ]
        );
        assert_eq!(
            notes(QKind::TempPoint, celsius(0.0), -5.0)[1],
            "help: write the spread as a difference, `± 13.7K`"
        );
        assert_eq!(
            notes(QKind::Db, 0.1 + 0.2, -10.0),
            [
                "help: for a spread in dB, write `0.3dB ± 0.03dB`",
                "help: for 10% of the gain, write the spec on `.mag()`, the gain as a ratio: `within 1.04 ± 10%`",
            ]
        );
    }
}
