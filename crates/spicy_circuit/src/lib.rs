//! The simulator-ready form of a circuit: what every front-end lowers to, and
//! what the simulator and exporters read.
//!
//! A lowered circuit is split by how often each part changes
//! (docs/ecad/circuit.md):
//! - [`Circuit`]: the structure. Which device connects to which nodes, and
//!   which model it uses. Built once per design and shared by every run.
//! - [`Params`]: the numbers. Fully resolved, one copy per run.
//! - [`CircuitNames`]: names for results and messages. The simulator never
//!   reads it.
//! - [`Analysis`]: what to run.
//!
//! No SPICE syntax or rules are left here: front-ends apply them while
//! lowering.

mod analysis;
mod params;

pub use analysis::{AcSpacing, AcSweep, Analysis, DcSweep, SourceRef, Transient};
pub use params::{
    BjtModel, BjtParams, CapacitorModel, CapacitorParams, DeviceTemperature, DiodeModel,
    DiodeParams, InductorModel, InductorParams, Params, Phasor, Polarity, ResistorModel,
    ResistorParams, SourceParams, Waveform,
};

use spicy_index::id;

id!(
    /// A node. Node 0 is ground.
    NodeId
);
id!(
    /// Index into [`Params::resistor_models`].
    ResistorModelId
);
id!(
    /// Index into [`Params::capacitor_models`].
    CapacitorModelId
);
id!(
    /// Index into [`Params::inductor_models`].
    InductorModelId
);
id!(
    /// Index into [`Params::diode_models`].
    DiodeModelId
);
id!(
    /// Index into [`Params::bjt_models`].
    BjtModelId
);
id!(
    /// Index into [`Circuit::inductors`].
    InductorId
);
id!(
    /// Index into [`Circuit::vsources`].
    VsourceId
);
id!(
    /// Index into [`Circuit::isources`].
    IsourceId
);

impl NodeId {
    pub const GROUND: NodeId = NodeId(0);

    pub fn is_ground(self) -> bool {
        self == Self::GROUND
    }
}

/// Everything a front-end produces for one circuit.
#[derive(Debug, Clone, PartialEq)]
pub struct Lowered {
    pub circuit: Circuit,
    pub params: Params,
    pub names: CircuitNames,
    pub analyses: Vec<Analysis>,
}

/// The circuit's structure. Device `i` of a kind is described by entry `i`
/// of that kind's list here, in [`Params`] and in [`CircuitNames`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Circuit {
    /// Number of nodes, ground included. Node ids run from 0 to `node_count - 1`.
    pub node_count: usize,
    pub resistors: Vec<Resistor>,
    pub capacitors: Vec<Capacitor>,
    pub inductors: Vec<Inductor>,
    pub diodes: Vec<Diode>,
    pub bjts: Vec<Bjt>,
    pub vsources: Vec<TwoTerminal>,
    pub isources: Vec<TwoTerminal>,
}

/// A source's terminals. Current is positive flowing from `positive`
/// through the source to `negative`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TwoTerminal {
    pub positive: NodeId,
    pub negative: NodeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resistor {
    pub positive: NodeId,
    pub negative: NodeId,
    pub model: ResistorModelId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capacitor {
    pub positive: NodeId,
    pub negative: NodeId,
    pub model: CapacitorModelId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inductor {
    pub positive: NodeId,
    pub negative: NodeId,
    pub model: InductorModelId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diode {
    /// Anode.
    pub positive: NodeId,
    /// Cathode.
    pub negative: NodeId,
    pub model: DiodeModelId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bjt {
    pub collector: NodeId,
    pub base: NodeId,
    pub emitter: NodeId,
    pub model: BjtModelId,
}

/// Names for results and messages, indexed like the [`Circuit`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CircuitNames {
    pub title: String,
    /// Indexed by node id. Node 0 is ground, named `"0"`.
    pub nodes: Vec<String>,
    pub resistors: Vec<String>,
    pub capacitors: Vec<String>,
    pub inductors: Vec<String>,
    pub diodes: Vec<String>,
    pub bjts: Vec<String>,
    pub vsources: Vec<String>,
    pub isources: Vec<String>,
}
