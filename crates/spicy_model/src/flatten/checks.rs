//! The checks that need the whole circuit (model.md E18 tier 2), on one root's flat
//! nets, in two tiers, as Modelica checks every class on its own but balances only the
//! model it simulates (spec §4.8):
//! - **The block's own** ([`run`], pass 8 of flatten, on every root): power sources
//!   (`power.rs`) and shorted parts, true wherever the block is used.
//! - **For simulating it** ([`simulation`], when a root is simulated): exactly one
//!   ground net, node 0 (E20), and the nets nothing connects to it. A library block,
//!   which only a board places, needn't be a whole circuit on its own.
//!
//! One function per rule, each reporting what it finds, with its paths from the
//! placement it's in ([`Circuit::error`]).

use super::{
    FlattenError, FlattenErrorKind, FlattenProblem, InBlock, UnionFind, in_order_found, written,
};
use crate::design::{Design, DesignSourceMap, PortId};
use crate::flat::{Flat, FlatInstance, FlatInstanceId, FlatNet, FlatNetId, LocalNet};
use crate::prelude::SignalType;
use spicy_errors::Reported;
use spicy_span::Span;

/// What every check reads: one root's flat design, where each part of it was written,
/// and how many placements each block has, which a problem inside a block counts
/// against (E23). Built only for a root with nothing broken in it (`flatten`, and
/// `check_simulation`).
#[derive(Clone, Copy)]
pub(super) struct Circuit<'f> {
    pub(super) flat: Flat<'f>,
    pub(super) map: &'f DesignSourceMap,
    /// Indexed by `BlockId`.
    pub(super) placements: &'f [usize],
}

impl Circuit<'_> {
    /// `problem`, at `span`, with its paths from the placement `scope`: counted against
    /// the placements of `scope`'s block (E23), unless `scope` is the root.
    pub(super) fn error(
        self,
        problem: FlattenProblem,
        scope: FlatInstanceId,
        span: Span,
    ) -> FlattenError {
        let instance = &self.flat.data.instances[scope.index()];
        // Nothing placed the root.
        let in_block = instance.origin.map(|_| InBlock {
            block: self.flat.design.block(instance.block).name.clone(),
            found: 1,
            placements: self.placements[instance.block.index()],
            example: self.flat.path(scope).to_string(),
        });
        FlattenError::new(FlattenErrorKind { problem, in_block }, span)
    }
}

/// The block's own checks.
pub(super) fn run(c: Circuit, errors: &mut Vec<FlattenError>) {
    super::power::check(c, errors);
    shorted_parts(c, errors);
}

/// The checks for simulating a root: its ground net, node 0 (E20), and the nets nothing
/// connects to it. `Err` if it hasn't exactly one ground net, reported.
pub(super) fn simulation(
    c: Circuit,
    errors: &mut Vec<FlattenError>,
) -> Result<FlatNetId, Reported> {
    let ground = ground(c, errors)?;
    isolated_nets(c, ground, errors);
    Ok(ground)
}

/// The nets with a `Ground` port on them (E20), in net order: the facts, on every root.
/// Lowering needs exactly one; [`ground`] checks it.
pub(super) fn ground_nets(
    design: &Design,
    instances: &[FlatInstance],
    nets: &[FlatNet],
) -> Vec<FlatNetId> {
    let is_ground = |&name: &LocalNet| {
        matches!(
            port_signal(design, instances, name),
            Some((_, SignalType::Ground))
        )
    };
    let nets = nets.iter().enumerate();
    let grounded = nets.filter(|(_, net)| net.names.iter().any(is_ground));
    grounded.map(|(net, _)| FlatNetId::new(net)).collect()
}

/// The port the local net `name` is, and its signal. `None` for a net that isn't a
/// port, and for a port resolve left without a type.
pub(super) fn port_signal(
    design: &Design,
    instances: &[FlatInstance],
    name: LocalNet,
) -> Option<(PortId, SignalType)> {
    let block = design.block(instances[name.at.index()].block);
    let port = block.net_port(name.net)?;
    let signal = block.ports[port.index()].signal.ok()?;
    Some((port, signal))
}

/// Where the port `port`, the local net `name`, joins its net: its name at the root,
/// its binding (`vcc: v12`) in a placed block. `None` for a binding resolve left out.
pub(super) fn port_joins_at(
    map: &DesignSourceMap,
    instances: &[FlatInstance],
    name: LocalNet,
    port: PortId,
) -> Option<Span> {
    match instances[name.at.index()].origin {
        None => Some(declared_at(map, instances, name)),
        Some(origin) => written(map, instances, origin).pins[port.index()],
    }
}

/// The ground net: exactly one (E20). There are no global nets, so the one net with a
/// `Ground` port is the one lowering makes node 0.
fn ground(c: Circuit, errors: &mut Vec<FlattenError>) -> Result<FlatNetId, Reported> {
    let flat = c.flat;
    let root = flat.data.root();
    match flat.data.grounds[..] {
        [ground] => Ok(ground),
        [] => {
            let problem = FlattenProblem::NoGround {
                root: flat.design.block(root).name.clone(),
            };
            let error = c.error(
                problem,
                FlatInstanceId::ROOT,
                c.map.blocks[root.index()].name,
            );
            Err(error.report(errors))
        }
        [first, second, ..] => {
            // Where each joins its net: its first `Ground` port.
            let instances = &flat.data.instances;
            let at = |net: FlatNetId| {
                let names = flat.data.nets[net.index()].names.iter();
                let mut grounds =
                    names.filter_map(|&name| match port_signal(flat.design, instances, name)? {
                        (port, SignalType::Ground) => port_joins_at(c.map, instances, name, port),
                        _ => None,
                    });
                grounds.next().expect("a ground net has a `Ground` port")
            };
            let grounds = flat.data.grounds.iter();
            let problem = FlattenProblem::SeveralGrounds {
                root: flat.design.block(root).name.clone(),
                nets: grounds.map(|&net| flat.net_path(net).to_string()).collect(),
            };
            let error = c.error(problem, FlatInstanceId::ROOT, at(second));
            Err(error.with_related(at(first)).report(errors))
        }
    }
}

/// A two-pin part with both pins on one net (a warning; atopile's shorted-part check),
/// at its `let`.
fn shorted_parts(c: Circuit, errors: &mut Vec<FlattenError>) {
    let flat = c.flat;
    for device in &flat.data.devices {
        let [Ok(a), Ok(b)] = device.pins.as_slice() else {
            continue;
        };
        if a != b {
            continue;
        }
        let problem = FlattenProblem::ShortedPart {
            part: flat.instance(device.origin).name.clone(),
        };
        let stmt = written(c.map, &flat.data.instances, device.origin).stmt;
        errors.push(c.error(problem, device.origin.at, stmt));
    }
}

/// Nets that no part connects to ground, one warning per group of them (model.md §7,
/// question 4), with paths from the placement they're all in.
fn isolated_nets(c: Circuit, ground: FlatNetId, errors: &mut Vec<FlattenError>) {
    let flat = c.flat;
    // The nets connected through a part (Xyce's topology check, with each part as one
    // group of its pins), and the nets the bench connects to ground.
    let mut connected = UnionFind::new(flat.data.nets.len());
    for device in &flat.data.devices {
        let mut pins = device.pins.iter().flatten();
        if let Some(first) = pins.next() {
            for pin in pins {
                connected.union(first.index(), pin.index());
            }
        }
    }
    for (net, flat_net) in flat.data.nets.iter().enumerate() {
        if flat_net.names.iter().any(|&name| on_the_bench(flat, name)) {
            connected.union(net, ground.index());
        }
    }
    // Every group apart from ground's, in the order of its first net.
    let ground = connected.find(ground.index());
    let groups = (0..flat.data.nets.len())
        .map(|net| (connected.find(net), FlatNetId::new(net)))
        .filter(|&(group, _)| group != ground);
    for (_, nets) in in_order_found(groups) {
        // Each net by its name, at its top. Nets are numbered shallowest first (E15), so
        // the first net's top is the placement they're all in: a part joins nets at its
        // own placement, and a net is all inside its top.
        let names: Vec<LocalNet> = nets
            .iter()
            .map(|n| flat.data.nets[n.index()].names[0])
            .collect();
        let scope = names[0].at;
        let path = |&n: &LocalNet| flat.local_path_from(scope, n).to_string();
        let problem = FlattenProblem::IsolatedNets {
            nets: names.iter().map(path).collect(),
        };
        let at = declared_at(c.map, &flat.data.instances, names[0]);
        errors.push(c.error(problem, scope, at));
    }
}

/// Whether the bench connects the local net `name` (language.md §8.5): one of the
/// root's own ports with a role, which the bench drives (an input) or loads (an output).
fn on_the_bench(flat: Flat, name: LocalNet) -> bool {
    name.at == FlatInstanceId::ROOT
        && matches!(
            port_signal(flat.design, &flat.data.instances, name),
            Some((_, SignalType::Power(_) | SignalType::Analog(_)))
        )
}

/// Where a local net is declared: its `net x;`, or its port's name.
fn declared_at(map: &DesignSourceMap, instances: &[FlatInstance], name: LocalNet) -> Span {
    let block = instances[name.at.index()].block;
    map.blocks[block.index()].nets[name.net.index()]
}
