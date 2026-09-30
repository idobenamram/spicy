//! The power check (model.md E19), one of the block's own checks (`checks.rs`): on each
//! net, power sources that meet, or loads with no source at all.
//!
//! **Faces** (E19): a port is seen from *inside* its block only at the root. Every
//! placed port is seen from *outside*, by the parent that binds it. So a block's own
//! `Power<In>` is the source for the parts inside it only when that block is the root:
//! counting every placed block's `Power<In>` from inside too would give `Stereo`'s `v12`
//! three sources (`research/flatten_circuit.md` §3.3; Modelica forms connection sets
//! per level the same way).
//!
//! **Innermost:** a `Power<Out>` passed up a level is one source: a supply block that
//! exports its regulator's output (`let reg = Reg { out: v5, … }`, `port v5:
//! Power<Out>`) puts two `Power<Out>` members on the net, `sup.v5` and `sup.reg.out`.
//! So only the innermost members of a net count ([`Replay`]): a placed block's port
//! with a member behind it, inside the block, is passing that one on, as KiCad counts a
//! net's pins and not its sheet pins. Each innermost port counts, as KiCad, Modelica and
//! atopile count them: two outputs of one block on one rail are two sources.

use std::cmp::Reverse;
use std::collections::HashMap;

use super::checks::{Circuit, port_joins_at, port_signal};
use super::{FlattenError, FlattenProblem, UnionFind, in_order_found, written};
use crate::flat::{Flat, FlatInstanceId, FlatNetId, LocalNet};
use crate::prelude::{Role, SignalType};
use spicy_span::Span;

/// Each net's power: sources that meet, or loads with no source at all.
pub(super) fn check(c: Circuit, errors: &mut Vec<FlattenError>) {
    for (net, flat_net) in c.flat.data.nets.iter().enumerate() {
        let net = FlatNetId::new(net);
        let members = flat_net.names.iter();
        let members = members.filter_map(|&name| PowerMember::new(c, name));
        let (sources, sinks): (Vec<_>, Vec<_>) = members.partition(|m| m.role == PowerRole::Source);
        if sources.len() > 1 {
            several_sources(c, net, &sources, errors);
        }
        if sources.is_empty() && !sinks.is_empty() {
            no_source(c, net, &sinks, errors);
        }
    }
}

/// A port with a role for power on a flat net, as it's seen there (E19).
struct PowerMember {
    /// Its local net; the path is built only to report it.
    name: LocalNet,
    role: PowerRole,
    face: Face,
    /// Where it joins the net: the port's name at the root, the binding (`vcc: v12`)
    /// for a placed block.
    at: Span,
}

#[derive(Clone, Copy, PartialEq)]
enum PowerRole {
    Source,
    Sink,
}

#[derive(Clone, Copy, PartialEq)]
enum Face {
    /// The root's own port, seen by the parts inside it.
    Inside,
    /// A placed block's port, seen by the parent that binds it.
    Outside,
}

impl PowerMember {
    /// The local net `name` as a member of its net, if it's a port with a role for
    /// power: the root's own `Power<In>` and a placed block's `Power<Out>` give power, a
    /// placed block's `Power<In>` takes it. `None` for any other net, and for a port
    /// resolve left without a type or a binding.
    fn new(c: Circuit, name: LocalNet) -> Option<Self> {
        let instances = &c.flat.data.instances;
        let (port, signal) = port_signal(c.flat.design, instances, name)?;
        let face = match instances[name.at.index()].origin {
            None => Face::Inside,
            Some(_) => Face::Outside,
        };
        let role = match (signal, face) {
            (SignalType::Power(Role::In), Face::Inside) => PowerRole::Source,
            (SignalType::Power(Role::Out), Face::Outside) => PowerRole::Source,
            (SignalType::Power(Role::In), Face::Outside) => PowerRole::Sink,
            _ => return None,
        };
        let at = port_joins_at(c.map, instances, name, port)?;
        Some(PowerMember {
            name,
            role,
            face,
            at,
        })
    }
}

/// Innermost sources that meet on `net` (KiCad's power-out conflict; atopile's "power
/// sources shorted", which lists every source). The net's joins are made again, a
/// placement at a time from the inside out ([`Replay`]). Where sources meet in a
/// placement is one problem: at the first join there that brings sources to where
/// there already are some, with where the first of those comes in as related, and
/// every source that meets there, by its path from the placement: `out: v5` inside a
/// block that ties two regulators, `o2: seven` for two outputs of one block tied on a
/// board.
fn several_sources(
    c: Circuit,
    net: FlatNetId,
    sources: &[PowerMember],
    errors: &mut Vec<FlattenError>,
) {
    let flat = c.flat;
    let mut replay = Replay::new(flat, net, sources);
    let joins = joins_on(c, net);
    for level in joins.chunk_by(|a, b| a.level == b.level) {
        // Each join where sources meet, with where the first of those comes in.
        let met: Vec<(&Join, Span)> = level
            .iter()
            .filter_map(|join| Some((join, replay.join(join)?)))
            .collect();
        // By the group they meet in, once the placement's joins are all made.
        let by_group = met
            .into_iter()
            .map(|(join, first)| (replay.group(join.outer), (join, first)));
        for (group, meetings) in in_order_found(by_group) {
            let (join, first) = meetings[0];
            let path = |m: &&PowerMember| flat.local_path_from(join.level, m.name).to_string();
            let problem = FlattenProblem::SeveralSources {
                net: flat.local_path_from(join.level, join.outer).to_string(),
                sources: replay.gathered(group).members.iter().map(path).collect(),
            };
            let error = c.error(problem, join.level, join.at);
            errors.push(error.with_related(first));
        }
    }
}

/// Loads on `net` with no source on it at all (KiCad's "power pin not driven"), at
/// where the first comes in at the net's top, with paths from there. Only the innermost
/// are listed ([`Replay`]): a port that passes power on to a load inside its block
/// (`s.v12` over `s.left.vcc`) isn't a load, as KiCad lists a net's pins and not its
/// sheet pins.
fn no_source(c: Circuit, net: FlatNetId, sinks: &[PowerMember], errors: &mut Vec<FlattenError>) {
    let flat = c.flat;
    let mut replay = Replay::new(flat, net, sinks);
    for join in joins_on(c, net) {
        replay.join(&join);
    }
    // The net is named at its top: the placement of its first local net (E15).
    let name = flat.data.nets[net.index()].names[0];
    let group = replay.group(name);
    let sinks = replay.gathered(group);
    let path = |m: &&PowerMember| flat.local_path_from(name.at, m.name).to_string();
    let problem = FlattenProblem::NoSource {
        net: flat.local_path_from(name.at, name).to_string(),
        sinks: sinks.members.iter().map(path).collect(),
    };
    errors.push(c.error(problem, name.at, sinks.comes_in));
}

/// A net's joins made again, one at a time, each placement's inside before the
/// placement ([`joins_on`]), gathering its members of one power role: in each group of
/// local nets joined so far, the innermost members, and where the first comes in.
///
/// Only the innermost count: a placed block's port is gathered when it's bound, and
/// only if nothing inside the block is behind it. One with a member behind it passes
/// that one on (`sup.v5` over `sup.reg.out`). The root's own ports are the innermost
/// of all: nothing places the root.
struct Replay<'m> {
    /// Each of the net's local nets, by its position in the net's names.
    index: HashMap<LocalNet, usize>,
    /// The member at each local net, by the same position, if it has one.
    member: Vec<Option<&'m PowerMember>>,
    joined: UnionFind,
    /// What each group has gathered, by its union-find root.
    gathered: Vec<Option<Gathered<'m>>>,
}

/// What one group of joined local nets has gathered.
struct Gathered<'m> {
    /// The innermost members, in the order they were joined.
    members: Vec<&'m PowerMember>,
    /// Where the first comes in, in the placement the group is joined up to so far.
    comes_in: Span,
    /// That placement.
    level: FlatInstanceId,
}

impl<'m> Replay<'m> {
    /// `members`, of one role on `net`, before any join is made: only the root's own
    /// ports are gathered yet.
    fn new(flat: Flat, net: FlatNetId, members: &'m [PowerMember]) -> Self {
        let names = &flat.data.nets[net.index()].names;
        let index: HashMap<LocalNet, usize> =
            names.iter().enumerate().map(|(i, &n)| (n, i)).collect();
        let mut member = vec![None; names.len()];
        let mut gathered: Vec<Option<Gathered>> = (0..names.len()).map(|_| None).collect();
        for m in members {
            let at = index[&m.name];
            member[at] = Some(m);
            if m.face == Face::Inside {
                gathered[at] = Some(Gathered {
                    members: vec![m],
                    comes_in: m.at,
                    level: m.name.at,
                });
            }
        }
        Replay {
            index,
            member,
            joined: UnionFind::new(names.len()),
            gathered,
        }
    }

    /// Makes `join`. If both sides had gathered members, they meet here: returns where
    /// the first of the outer side's comes in.
    fn join(&mut self, join: &Join) -> Option<Span> {
        let inner_at = self.index[&join.inner];
        let inner = self.joined.find(inner_at);
        let outer = self.joined.find(self.index[&join.outer]);
        if inner == outer {
            return None;
        }
        if join.kind == JoinKind::Binding {
            self.bring_up(inner, self.member[inner_at], join);
        }
        let coming = self.gathered[inner].take();
        let here = self.gathered[outer].take();
        let met = match (&here, &coming) {
            (Some(here), Some(_)) => Some(here.comes_in),
            _ => None,
        };
        self.joined.union(inner, outer);
        let root = self.joined.find(outer);
        self.gathered[root] = match (here, coming) {
            (Some(mut here), Some(coming)) => {
                here.members.extend(coming.members);
                Some(here)
            }
            (here, coming) => here.or(coming),
        };
        met
    }

    /// The group `inner`, joined up to `join.level` by binding `port`, a placed block's
    /// port on it (a member of the role, or `None`): what it has gathered comes in
    /// there, the first time it's joined up to that level. If it has gathered nothing,
    /// nothing inside the block is behind the port, so the port is innermost itself.
    fn bring_up(&mut self, inner: usize, port: Option<&'m PowerMember>, join: &Join) {
        match &mut self.gathered[inner] {
            Some(gathered) if gathered.level != join.level => {
                gathered.comes_in = join.at;
                gathered.level = join.level;
            }
            Some(_) => {}
            None => {
                self.gathered[inner] = port.map(|m| Gathered {
                    members: vec![m],
                    comes_in: join.at,
                    level: join.level,
                });
            }
        }
    }

    /// The group the local net `name` is in, by its root.
    fn group(&mut self, name: LocalNet) -> usize {
        self.joined.find(self.index[&name])
    }

    /// What the group `group` has gathered: something, for a group a join met in, or
    /// for the whole net once every join is made (its deepest member is innermost).
    fn gathered(&self, group: usize) -> &Gathered<'m> {
        let gathered = self.gathered[group].as_ref();
        gathered.expect("a group with members has an innermost one")
    }
}

/// One join that makes two local nets one: a port binding, or an item of a merged net.
struct Join {
    kind: JoinKind,
    /// The placement it's written in.
    level: FlatInstanceId,
    /// What it joins: a placed block's port, or a merged item.
    inner: LocalNet,
    /// What that's joined to, in `level`: the net bound, or the merged net.
    outer: LocalNet,
    at: Span,
}

#[derive(Clone, Copy, PartialEq)]
enum JoinKind {
    /// `vcc: v12`: a placed block's port bound to its parent's net, which brings what's
    /// inside the block up a level.
    Binding,
    /// An item of `net x = [..]`, joining two nets of one placement.
    Merge,
}

/// Every join on `net`: the binding of each placed block's port on it (`vcc: v12`), and
/// each item of a `net x = [..]` that names one of its local nets. Each placement's
/// inside comes before the placement, and a placement's joins come together:
/// placements are in tree order, so a later one is never around an earlier one.
fn joins_on(c: Circuit, net: FlatNetId) -> Vec<Join> {
    let flat = c.flat;
    let mut joins = Vec::new();
    for &name in &flat.data.nets[net.index()].names {
        let instance = &flat.data.instances[name.at.index()];
        let block = flat.block(name.at);
        if let (Some(port), Some(origin)) = (block.net_port(name.net), instance.origin) {
            let bound = flat.instance(origin).pins[port.index()];
            let at = written(c.map, &flat.data.instances, origin).pins[port.index()];
            if let (Ok(net), Some(at)) = (bound, at) {
                let outer = LocalNet { at: origin.at, net };
                joins.push(Join {
                    kind: JoinKind::Binding,
                    level: origin.at,
                    inner: name,
                    outer,
                    at,
                });
            }
        }
        let merges = block
            .merges
            .iter()
            .zip(&c.map.blocks[instance.block.index()].merges);
        for (merge, spans) in merges {
            let outer = LocalNet {
                at: name.at,
                net: merge.net,
            };
            for (&item, &at) in merge.with.iter().zip(&spans.items) {
                if item == name.net {
                    joins.push(Join {
                        kind: JoinKind::Merge,
                        level: name.at,
                        inner: name,
                        outer,
                        at,
                    });
                }
            }
        }
    }
    joins.sort_by_key(|join| Reverse(join.level));
    joins
}
