//! The unknowns of the MNA system, and the row each one gets.
//!
//! Node voltages come first (ground has no row), then branch currents:
//! voltage sources, then inductors. Every solution vector the simulator
//! returns follows this order; [`unknowns`] describes it.

use spicy_circuit::{Circuit, CircuitNames, InductorId, NodeId, VsourceId};

/// What one entry of a solution vector holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unknown {
    /// A node voltage (never ground).
    Voltage(NodeId),
    /// A branch current, flowing from the device's positive terminal through it.
    Current(Branch),
}

/// A device whose current is an unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Branch {
    Vsource(VsourceId),
    Inductor(InductorId),
}

impl Unknown {
    /// The node's or the device's name.
    pub fn name<'a>(&self, names: &'a CircuitNames) -> &'a str {
        match *self {
            Unknown::Voltage(node) => &names.nodes[node.index()],
            Unknown::Current(Branch::Vsource(v)) => &names.vsources[v.index()],
            Unknown::Current(Branch::Inductor(l)) => &names.inductors[l.index()],
        }
    }
}

/// The unknowns in solution-vector order.
pub fn unknowns(circuit: &Circuit) -> Vec<Unknown> {
    let voltages = (1..circuit.node_count).map(|n| Unknown::Voltage(NodeId::new(n)));
    let vsources = (0..circuit.vsources.len()).map(|i| Branch::Vsource(VsourceId::new(i)));
    let inductors = (0..circuit.inductors.len()).map(|i| Branch::Inductor(InductorId::new(i)));
    voltages
        .chain(vsources.chain(inductors).map(Unknown::Current))
        .collect()
}

/// The MNA row of each unknown.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Layout {
    nodes: usize,
    vsources: usize,
    inductors: usize,
}

impl Layout {
    pub(crate) fn new(circuit: &Circuit) -> Self {
        Self {
            nodes: circuit.node_count - 1,
            vsources: circuit.vsources.len(),
            inductors: circuit.inductors.len(),
        }
    }

    /// Size of the MNA system.
    pub(crate) fn dim(&self) -> usize {
        self.nodes + self.branches()
    }

    /// Number of node-voltage rows.
    pub(crate) fn nodes(&self) -> usize {
        self.nodes
    }

    /// Number of branch-current rows.
    pub(crate) fn branches(&self) -> usize {
        self.vsources + self.inductors
    }

    /// A node's row, or `None` for ground.
    pub(crate) fn node(&self, node: NodeId) -> Option<usize> {
        node.index().checked_sub(1)
    }

    pub(crate) fn vsource(&self, index: usize) -> usize {
        self.nodes + index
    }

    pub(crate) fn inductor(&self, index: usize) -> usize {
        self.nodes + self.vsources + index
    }
}
