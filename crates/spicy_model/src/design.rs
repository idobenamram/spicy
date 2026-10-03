//! The `Design`: each block once, as written, with names resolved and values typed
//! (model.md §4, the *folded* form). Nothing is copied per placement yet; that is
//! `flatten`'s job.
//!
//! Positions are not in here: they live in [`DesignSourceMap`], so two designs that
//! differ only in whitespace or comments compare equal (model.md E22, as
//! rust-analyzer keeps a body's data apart from its source map).

use spicy_index::id;

use crate::measure::{MExpr, MeasureType};
use crate::prelude::{PartKind, Shape, SignalType};
use crate::units::{Quantity, Value};
use spicy_errors::Reported;
use spicy_span::Span;

id!(
    /// A block, by its position in [`Design::blocks`] (source order).
    BlockId
);
id!(
    /// A port of one block. Numbered per block (model.md E21): an edit to one block
    /// can't renumber another's.
    PortId
);
id!(
    /// A net of one block, ports included. Numbered per block.
    NetId
);
id!(
    /// A part or placement in one block. Numbered per block.
    InstanceId
);
id!(
    /// An `env`, by its position in [`Design::envs`] (source order).
    EnvId
);
id!(
    /// A `const`, by its position in [`Design::consts`] (source order).
    ConstId
);
id!(
    /// A `setup`, by its position in [`Design::setups`] (source order).
    SetupId
);
id!(
    /// A contract's measure (`let h = …;`), by its position in
    /// [`Contract::measures`] (source order). Numbered per contract.
    MeasureId
);
id!(
    /// A contract's spec, by its position in [`Contract::specs`] (source order).
    /// Numbered per contract.
    SpecId
);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Design {
    pub blocks: Vec<Block>,
    /// Each block's contract, indexed like `blocks`: contracts are one-to-one with the
    /// blocks they describe. Kept apart from `blocks` so flattening depends on blocks only.
    pub contracts: Vec<Option<Contract>>,
    pub envs: Vec<Env>,
    pub consts: Vec<Const>,
    pub setups: Vec<Setup>,
}

/// `env ambient: Temperature in -10°C..=60°C;`: a condition of the whole project, a
/// range the engine searches. A setup names it (`temp: ambient`), and every root that
/// does gets its own knob for it.
#[derive(Clone, Debug, PartialEq)]
pub struct Env {
    pub name: String,
    /// Its range or tolerance, typed as its declared type (a range's nominal is its
    /// midpoint, model.md E16). `Err` when the type or the value was wrong, or the name
    /// is defined twice.
    pub value: Result<Value, Reported>,
}

/// `const R_TOP: Ohm = 10k;`: a fixed value of the whole project, usable where a number
/// is expected (`value: R_TOP`).
#[derive(Clone, Debug, PartialEq)]
pub struct Const {
    pub name: String,
    /// `Err` when the type or the value was wrong, or the name is defined twice.
    pub value: Result<Quantity, Reported>,
}

/// `setup Operating for CeAmp { … }`: the world around a block, what its specs are
/// checked in (language.md §8.2). Named per block: every block can have its own
/// `Operating`.
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    pub name: String,
    pub block: BlockId,
    /// What's on each of the block's ports, by `PortId`. `None` for a port that takes
    /// nothing (a `Ground`, a `Pin`, a port whose type is wrong) and for one the setup
    /// leaves out, which is reported: every port with a role must be written.
    pub ports: Vec<Option<PortSetup>>,
    /// The temperature. `Err` when it's wrong or not written.
    pub temp: Result<Temp, Reported>,
    /// As [`Block::tainted`]: whether an error was reported inside the setup, or a value
    /// it reads is broken, or it's the first of two setups of one name for one block.
    pub tainted: Option<Reported>,
}

/// What's on one port: its shape, and each of the shape's fields in the order of
/// [`Shape::fields`]. An optional field left out is `Unset`: ideal, so it gets no knob
/// (a 0 Ω source, nothing on the output).
#[derive(Clone, Debug, PartialEq)]
pub struct PortSetup {
    pub shape: Shape,
    pub fields: Vec<FieldValue>,
}

/// A setup's temperature.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Temp {
    /// `temp: ambient`: the env's range, one knob for every setup of a root that names it.
    Env(EnvId),
    /// `temp: 25°C`, `temp: -40°C..=125°C`, or a const's value.
    Value(Value),
}

/// A block's contract (model.md E24): the setup its specs are checked in, its
/// measures, and its specs.
#[derive(Clone, Debug, PartialEq)]
pub struct Contract {
    /// `setup = Operating;`: one of the block's setups. `Err` when it's left out or
    /// names none, reported.
    pub default_setup: Result<SetupId, Reported>,
    pub measures: Vec<Measure>,
    pub specs: Vec<Spec>,
    /// As [`Block::tainted`]: whether parsing the contract reported an error, so
    /// something written may be missing from it, or it's the first of two contracts for
    /// its block. A broken measure or spec doesn't taint it: only that spec, or what
    /// reads that measure, can't be checked.
    pub tainted: Option<Reported>,
}

/// `spec gain: h.at(1kHz).mag() within 4.6 ± 5%;`: a number the design must keep
/// within a limit, in the contract's default setup.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub name: String,
    /// `pub spec`: one a parent may rely on, which names only the block's ports
    /// (language.md §8.3).
    pub public: bool,
    /// What it checks: one number. `Err` when it's wrong, reported.
    pub measure: Result<MExpr, Reported>,
    /// Which values pass, in the measure's unit. `Err` when it's wrong, when the
    /// measure is (its unit isn't known), or when it's the first of two specs of one
    /// name, reported.
    pub limit: Result<Limit, Reported>,
}

/// A spec's limit: `within 4.5V..=6.5V`, `<= 30Hz`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limit {
    pub op: LimitOp,
    /// A range or a tolerance for `within`; one exact number for `<=` and `>=`.
    pub bound: Value,
}

/// How a spec's value is compared with its bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitOp {
    /// `within`: inside the bound's range.
    Within,
    /// `<=`.
    AtMost,
    /// `>=`.
    AtLeast,
}

impl LimitOp {
    /// The relation it's written with.
    pub fn name(self) -> &'static str {
        match self {
            LimitOp::Within => "within",
            LimitOp::AtMost => "<=",
            LimitOp::AtLeast => ">=",
        }
    }
}

/// `let h = ac(output.v / input.v);` in a contract.
#[derive(Clone, Debug, PartialEq)]
pub struct Measure {
    pub name: String,
    /// What it computes, and what that is. `Err` when it's wrong, or it's the first of
    /// two of one name (which was meant isn't known), reported.
    pub value: Result<(MExpr, MeasureType), Reported>,
}

impl Design {
    pub fn block(&self, id: BlockId) -> &Block {
        &self.blocks[id.index()]
    }
}

/// One block as written: its ports, its nets and what it contains.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Block {
    pub name: String,
    pub ports: Vec<Port>,
    /// Every net of the block. A port is a net too (language §3.1): the first
    /// `ports.len()` nets are the ports' nets, in port order ([`Block::port_net`]).
    pub nets: Vec<Net>,
    pub instances: Vec<Instance>,
    /// `net x = [a, b];`: `x` and the listed nets are one net. Applied by flatten's
    /// union-find, with every name kept as an alias (model.md E15).
    pub merges: Vec<Merge>,
    /// Whether the block has a `circuit`, so a block with none differs from one with an
    /// empty `circuit A {}` (contracts_plan.md §3.1).
    pub has_circuit: bool,
    /// Whether an error was reported inside the block (by the lexer, the parser or
    /// resolve), or a value it reads is broken (a part's value naming a const whose own
    /// value is wrong), with the proof (rustc's `tainted_by_errors`). Every placeholder
    /// below has one, and so does what leaves none: a statement the parser couldn't
    /// read, a second `let` or binding of a name, a merged net that doesn't resolve. So
    /// flatten doesn't check a root that places a tainted block as a whole: the checks
    /// would report the damage (model.md E7).
    pub tainted: Option<Reported>,
}

impl Block {
    pub fn net(&self, id: NetId) -> &Net {
        &self.nets[id.index()]
    }

    /// The net a port is, inside the block.
    pub fn port_net(&self, id: PortId) -> NetId {
        NetId::new(id.index())
    }

    /// A port's name: its net's name, the one place it's kept.
    pub fn port_name(&self, id: PortId) -> &str {
        &self.net(self.port_net(id)).name
    }

    /// The ports' names, in port order.
    pub fn port_names(&self) -> impl Iterator<Item = &str> {
        self.nets[..self.ports.len()]
            .iter()
            .map(|n| n.name.as_str())
    }

    /// The port a net is, if it's a port's net.
    pub fn net_port(&self, id: NetId) -> Option<PortId> {
        (id.index() < self.ports.len()).then(|| PortId::new(id.index()))
    }
}

/// A connection point on the block's boundary: what a placement binds. Its name is its
/// net's ([`Block::port_name`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Port {
    /// `Err` when the written type was wrong. The port still exists, so its name
    /// resolves and placements can bind it (model.md E7).
    pub signal: Result<SignalType, Reported>,
}

/// A node inside the block. A port's net is named after the port.
#[derive(Clone, Debug, PartialEq)]
pub struct Net {
    pub name: String,
}

/// `net x = [a, b];`: `net` is `x`, `with` the nets listed.
#[derive(Clone, Debug, PartialEq)]
pub struct Merge {
    pub net: NetId,
    pub with: Vec<NetId>,
}

/// A part or a placed block, with its pins bound and its fields given.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub name: String,
    pub of: InstanceOf,
    /// Pin (for a part) or port (for a block) → the net it's bound to, in the order of
    /// the part kind's pins or the block's ports. Every pin must be bound: `Err` when
    /// the binding was wrong or missing.
    pub pins: Vec<Result<NetId, Reported>>,
    /// Field values of a part, in the order of the part kind's fields. Empty for a
    /// placed block (blocks have no params yet).
    pub fields: Vec<FieldValue>,
}

/// One field of an instance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FieldValue {
    /// An optional field not written: lowering applies the device's default (M1e). It
    /// gets no knob.
    Unset,
    Given(Value),
    /// Written but wrong, or a required field not written. Later steps skip it.
    Invalid(Reported),
}

/// What an instance is an instance of (model.md E6: decided by what the name in
/// `let x = Name { … }` resolves to).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InstanceOf {
    Part(PartKind),
    Block(BlockId),
    /// The name didn't resolve, so later steps skip the instance without reporting
    /// again (model.md E7).
    Error(Reported),
}

/// A value as a field holds it: `Err` is a value that was wrong, reported.
impl From<Result<Value, Reported>> for FieldValue {
    fn from(value: Result<Value, Reported>) -> Self {
        match value {
            Ok(value) => FieldValue::Given(value),
            Err(reported) => FieldValue::Invalid(reported),
        }
    }
}

/// Where every part of the `Design` was written, indexed like the design itself.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DesignSourceMap {
    pub blocks: Vec<BlockSpans>,
    /// Each env's and const's name, indexed like [`Design::envs`] and [`Design::consts`].
    pub envs: Vec<Span>,
    pub consts: Vec<Span>,
    pub setups: Vec<SetupSpans>,
    /// Each contract's spans, indexed like [`Design::contracts`].
    pub contracts: Vec<Option<ContractSpans>>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ContractSpans {
    /// The contract's name in `contract Name`.
    pub name: Span,
    /// The `setup = S;` statement, if it's written.
    pub default_setup: Option<Span>,
    /// Each measure's name in `let name = …`, indexed like [`Contract::measures`].
    pub measures: Vec<Span>,
    /// Each spec's spans, indexed like [`Contract::specs`].
    pub specs: Vec<SpecSpans>,
}

/// Where one spec's name, measure and bound were written.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SpecSpans {
    /// The spec's name in `spec name: …`.
    pub name: Span,
    /// What it measures: `h.at(1kHz).mag()`.
    pub measure: Span,
    /// Its bound: `4.6 ± 5%`.
    pub bound: Span,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct SetupSpans {
    /// The setup's name in `setup Name for Block`.
    pub name: Span,
    /// Each port's entries, indexed like [`Setup::ports`].
    pub ports: Vec<Option<PortSetupSpans>>,
    /// The `temp: …` entry, if it's written.
    pub temp: Option<Span>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct PortSetupSpans {
    /// The `vcc: Supply { … }` entry; `None` when the shape is implied by the port's
    /// role (`vout.i: 5mA` alone).
    pub shape: Option<Span>,
    /// Where each field was given, in the shape (`v: 12V`) or by a path (`vcc.v: 12V`),
    /// in the order of [`PortSetup::fields`].
    pub fields: Vec<Option<Span>>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BlockSpans {
    /// The block's name in `block Name {`.
    pub name: Span,
    /// Each port's type (`Power<In>`), for errors about its role.
    pub port_types: Vec<Span>,
    /// Each net's name where it was declared. A port's net is declared by the port, so
    /// the first `ports.len()` are the ports' names.
    pub nets: Vec<Span>,
    pub instances: Vec<InstanceSpans>,
    /// Each `net x = [a, b];`, in the order of [`Block::merges`].
    pub merges: Vec<MergeSpans>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MergeSpans {
    /// The whole statement: "why are these one net?" points here (model.md E23).
    pub stmt: Span,
    /// Each merged net as written, in the order of [`Merge::with`].
    pub items: Vec<Span>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct InstanceSpans {
    /// The instance's name in `let name = …`.
    pub name: Span,
    /// The whole `let` statement.
    pub stmt: Span,
    /// The part kind or block as written (`Resistor`, `CeAmp`), for the chain of a
    /// recursive placement (model.md E14). Empty (`0..0`) when the value isn't
    /// `Kind { … }`.
    pub kind: Span,
    /// Each pin binding (`a: vcc`), in the order of [`Instance::pins`]. Like `pins` and
    /// `fields` there, empty when the instance is an [`InstanceOf::Error`].
    pub pins: Vec<Option<Span>>,
    /// Each field (`value: 47k ± 1%`), in the order of [`Instance::fields`].
    pub fields: Vec<Option<Span>>,
}
