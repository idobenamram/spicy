//! The `Design`: each block once, as written, with names resolved and values typed
//! (model.md §4, the *folded* form). Nothing is copied per placement yet; that is
//! `flatten`'s job.
//!
//! Positions are not in here: they live in [`DesignSourceMap`], so two designs that
//! differ only in whitespace or comments compare equal (model.md E22, as
//! rust-analyzer keeps a body's data apart from its source map).

use spicy_index::id;

use crate::prelude::{PartKind, SignalType};
use crate::units::Value;
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

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Design {
    pub blocks: Vec<Block>,
    /// Each block's contract, indexed like `blocks`: contracts are one-to-one with the
    /// blocks they describe. Kept apart from `blocks` so flattening depends on blocks only.
    pub contracts: Vec<Option<Contract>>,
}

/// A block's contract. Its contents (assumptions, measures, specs) are resolved in the
/// next step (model.md E24, roadmap M1d-5).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Contract {}

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
    /// Whether an error was reported inside the block (by the lexer, the parser or
    /// resolve), with the proof (rustc's `tainted_by_errors`). Every placeholder below
    /// has one, and so does what leaves none: a statement the parser couldn't read, a
    /// second `let` or binding of a name, a merged net that doesn't resolve. So flatten
    /// doesn't check a root that places a tainted block as a whole: the checks would
    /// report the damage (model.md E7).
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

/// Where every part of the `Design` was written, indexed like the design itself.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DesignSourceMap {
    pub blocks: Vec<BlockSpans>,
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
