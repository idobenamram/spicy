//! Flatten, the second step of elaboration (docs/ecad/model.md#flatten-instances and
//! docs/ecad/model.md#flatten-checks): a [`Design`], each block once, becomes one
//! [`FlatDesign`] per root, with every placement expanded, every net merged and named,
//! every part a device and every spread a knob.
//!
//! Read top to bottom in [`flatten`], one pass after another:
//! - **Pass 1, recursion**, once per design: every placement cycle, reported once with
//!   its chain (`recursion.rs`).
//! - **Pass 2, the roots:** every block nothing places that has a circuit, each
//!   flattened on its own by passes 3 to 8: **the instance tree**, **the joins**, **the
//!   nets** (merged and named), **the devices and knobs**, **the default setup**
//!   (`setup.rs`), **the ground nets**.
//! - **Pass 9, the block's own checks** (`checks.rs`) on every root: power sources
//!   (`power.rs`), shorted parts.
//! - Last, **each problem once:** the reports of one problem, from every placement and
//!   every root, grouped into one (model.md E23).
//!
//! The checks for simulating a root, exactly one ground net and no isolated nets, run
//! only when it's simulated ([`check_simulation`]): a library block needn't be a whole
//! circuit on its own.
//!
//! Errors never stop the stage (model.md E7): a placeholder left by resolve (an
//! unresolved instance, an unbound pin, a port with a wrong type, a field with a wrong
//! value) is skipped without a new error, and a root that places a block with an error
//! inside it isn't checked as a whole, since its circuit isn't complete.

mod checks;
mod error;
mod power;
mod recursion;
mod setup;

use checks::Circuit;

pub use error::{FlattenError, FlattenErrorKind, FlattenProblem, InBlock};

use spicy_index::fx::{FxHashMap, FxHashSet};
use std::hash::Hash;

use crate::design::{
    Block, BlockId, Design, DesignSourceMap, FieldValue, Instance, InstanceId, InstanceOf,
    InstanceSpans, NetId, PortId,
};
use crate::flat::{
    Flat, FlatDesign, FlatDevice, FlatDeviceId, FlatField, FlatInstance, FlatInstanceId, FlatNet,
    FlatNetId, Knob, KnobId, KnobSource, LocalInstance, LocalNet,
};
use crate::prelude::PartKind;
use crate::units::Value;
use spicy_errors::Reported;

/// Everything flattening produces: each root's flat design, and every problem found.
#[derive(Clone, Debug, PartialEq)]
pub struct Flattened {
    /// One per block nothing places that has a circuit, in name order; a root with more
    /// placements than [`MAX_PLACEMENTS`] is reported instead.
    pub roots: Vec<FlatDesign>,
    pub errors: Vec<FlattenError>,
}

/// Flattens every root of `design`. Always returns; broken parts are skipped, and every
/// problem is reported once.
pub fn flatten(design: &Design, map: &DesignSourceMap) -> Flattened {
    let mut errors = Vec::new();
    // The blocks, and each block's placements of blocks and its parts, in name order:
    // the order every pass takes them in, so none depends on statement or block order
    // (E4). What the passes read of a block is found here, once per design, not at
    // every placement of it.
    let blocks = blocks_in_name_order(design);
    let children: Vec<_> = design.blocks.iter().map(placed_blocks).collect();
    let parts: Vec<_> = design.blocks.iter().map(placed_parts).collect();
    let targets: Vec<_> = design.blocks.iter().map(merge_targets).collect();

    // Pass 1, recursion: every placement cycle in the design, reported once each. The
    // placements on a cycle are cut, so the passes below can't loop, and the blocks
    // with a cut are broken, like the blocks resolve marked.
    let cut = recursion::find_cycles(design, map, &blocks, &children, &mut errors);
    let broken = broken_blocks(design, &cut);

    // Pass 2, the roots: every block nothing places (docs/ecad/model.md#questions,
    // question 2) that has a circuit (contracts_plan.md §3.1: one with none is an
    // interface), each flattened on its own, in name order.
    let flattener = RootFlattener {
        design,
        map,
        children: &children,
        parts: &parts,
        targets: &targets,
        cut: &cut,
        broken: &broken,
    };
    let roots = roots(design, &blocks, &children);
    let mut flat_roots = Vec::with_capacity(roots.len());
    for root in roots {
        match flattener.flatten(root) {
            Ok(flat) => flat_roots.push(flat),
            // Too large to flatten: reported instead.
            Err(error) => errors.push(error),
        }
    }

    // Pass 9, the block's own checks (E18 tier 2), on each root with nothing broken in
    // it: a placeholder has been reported, and checking around it would only report
    // it again. A problem inside a block counts every placement of the block, in every
    // root checked.
    let checked: Vec<&FlatDesign> = flat_roots.iter().filter(|d| d.tainted.is_none()).collect();
    let placements = placements_per_block(design, checked.iter().copied());
    for &data in &checked {
        let flat = Flat { design, data };
        let circuit = Circuit {
            flat,
            map,
            placements: &placements,
        };
        checks::run(circuit, &mut errors);
    }

    // A problem inside a block is reported once, however many placements have it,
    // under one root or several (model.md E23).
    let errors = grouped(errors);
    Flattened {
        roots: flat_roots,
        errors,
    }
}

/// The most placements one root may flatten to; past it, the root is reported and not
/// flattened. Recursion is found exactly (pass 1), so this is no depth limit (rustc's
/// `recursion_limit`, slang's `--max-hierarchy-depth`) but a size one: a block placed
/// twice in a block placed twice, 20 deep, is a million placements from 20 lines, and
/// the editor flattens on every edit.
pub const MAX_PLACEMENTS: usize = 1_000_000;

/// The checks for simulating one root (model.md E18 tier 2, E20), which lowering runs
/// first: its one ground net, node 0, returned, and the nets nothing connects to it
/// (warnings). `Err` if the root is broken or hasn't exactly one ground net; either way
/// it has been reported. Each problem is reported once, as in [`flatten`].
pub fn check_simulation(
    flat: Flat,
    map: &DesignSourceMap,
    errors: &mut Vec<FlattenError>,
) -> Result<FlatNetId, Reported> {
    // A broken root isn't simulated: something written is missing from it (E7).
    if let Some(reported) = flat.data.tainted {
        return Err(reported);
    }
    let placements = placements_per_block(flat.design, [flat.data]);
    let circuit = Circuit {
        flat,
        map,
        placements: &placements,
    };
    let mut found = Vec::new();
    let ground = checks::simulation(circuit, &mut found);
    errors.extend(grouped(found));
    ground
}

/// How many placements each block has in `roots`, indexed by `BlockId`: what a problem
/// inside a block counts against (E23).
fn placements_per_block<'f>(
    design: &Design,
    roots: impl IntoIterator<Item = &'f FlatDesign>,
) -> Vec<usize> {
    let mut placements = vec![0; design.blocks.len()];
    for instance in roots.into_iter().flat_map(|root| &root.instances) {
        placements[instance.block.index()] += 1;
    }
    placements
}

/// `errors` with each problem once. The checks report a problem per placement; the
/// same problem (where it's written, what it's related to, and what it says, with its
/// paths from the block it's in) in several placements is the first one found,
/// counting them all (slang's "in 2 of 3 instances"). As slang's `ASTDiagMap`, reports
/// are grouped by where they are, and compared by what they say only there, so none is
/// copied or hashed whole. The problems come in the order their places are first found.
fn grouped(errors: Vec<FlattenError>) -> Vec<FlattenError> {
    let by_place = errors.into_iter().map(|e| ((e.span, e.related), e));
    let mut problems: Vec<FlattenError> = Vec::new();
    for (_, here) in in_order_found(by_place) {
        // The problems in one place: almost always one.
        let first = problems.len();
        for error in here {
            let same = |p: &&mut FlattenError| p.kind.problem == error.kind.problem;
            match problems[first..].iter_mut().find(same) {
                Some(problem) => problem.kind.add_placement(),
                None => problems.push(error),
            }
        }
    }
    problems
}

/// `items` grouped by key: each group in the order of its items, and the groups in the
/// order their keys are first found, which is the order they're reported in.
fn in_order_found<K: Copy + Eq + Hash, T>(
    items: impl IntoIterator<Item = (K, T)>,
) -> Vec<(K, Vec<T>)> {
    let mut groups: Vec<(K, Vec<T>)> = Vec::new();
    let mut index: FxHashMap<K, usize> = FxHashMap::default();
    for (key, item) in items {
        let k = *index.entry(key).or_insert_with(|| {
            groups.push((key, Vec::new()));
            groups.len() - 1
        });
        groups[k].1.push(item);
    }
    groups
}

/// Every block, in name order: so moving a block in the file changes nothing (model.md
/// §9: a shuffle of blocks gives the same dump).
fn blocks_in_name_order(design: &Design) -> Vec<BlockId> {
    let mut blocks: Vec<BlockId> = (0..design.blocks.len()).map(BlockId::new).collect();
    blocks.sort_by(|&a, &b| design.block(a).name.cmp(&design.block(b).name));
    blocks
}

/// The placements of blocks in `block`, in name order.
fn placed_blocks(block: &Block) -> Vec<(InstanceId, BlockId)> {
    in_name_order(block, |instance| match instance.of {
        InstanceOf::Block(b) => Some(b),
        _ => None,
    })
}

/// The parts in `block`, in name order.
fn placed_parts(block: &Block) -> Vec<(InstanceId, PartKind)> {
    in_name_order(block, |instance| match instance.of {
        InstanceOf::Part(kind) => Some(kind),
        _ => None,
    })
}

/// The merge targets in `block`: the `x` of each `net x = [..]`, which rank for naming
/// the merged net ([`Rank`]).
fn merge_targets(block: &Block) -> FxHashSet<NetId> {
    block.merges.iter().map(|merge| merge.net).collect()
}

/// The instances of `block` that `pick` keeps, each with what it keeps of it, in name
/// order: so statement order doesn't change the ids (Yosys sorts by name the same way).
fn in_name_order<T>(block: &Block, pick: impl Fn(&Instance) -> Option<T>) -> Vec<(InstanceId, T)> {
    let mut picked: Vec<(InstanceId, T)> = block
        .instances
        .iter()
        .enumerate()
        .filter_map(|(i, instance)| Some((InstanceId::new(i), pick(instance)?)))
        .collect();
    let name = |i: InstanceId| &block.instances[i.index()].name;
    picked.sort_by(|(a, _), (b, _)| name(*a).cmp(name(*b)));
    picked
}

/// Each block's proof that something in it is broken (model.md E7): an error resolve
/// reported inside it, or a placement pass 1 cut.
fn broken_blocks(
    design: &Design,
    cut: &FxHashMap<(BlockId, InstanceId), Reported>,
) -> Vec<Option<Reported>> {
    let mut broken: Vec<Option<Reported>> = design.blocks.iter().map(|b| b.tainted).collect();
    for (&(block, _), &reported) in cut {
        broken[block.index()].get_or_insert(reported);
    }
    broken
}

/// The blocks nothing places that have a circuit, in the order of `blocks` (name
/// order).
fn roots(
    design: &Design,
    blocks: &[BlockId],
    children: &[Vec<(InstanceId, BlockId)>],
) -> Vec<BlockId> {
    let mut placed = vec![false; children.len()];
    for &(_, child) in children.iter().flatten() {
        placed[child.index()] = true;
    }
    let roots = blocks
        .iter()
        .filter(|&&b| !placed[b.index()] && design.block(b).has_circuit);
    roots.copied().collect()
}

/// Flattens each root: what every root's passes read, the design and what pass 1 found
/// in it. Each pass is a function that returns what it built.
struct RootFlattener<'d> {
    design: &'d Design,
    map: &'d DesignSourceMap,
    /// Each block's placements of blocks, in name order.
    children: &'d [Vec<(InstanceId, BlockId)>],
    /// Each block's parts, in name order.
    parts: &'d [Vec<(InstanceId, PartKind)>],
    /// Each block's merge targets ([`merge_targets`]).
    targets: &'d [FxHashSet<NetId>],
    /// The placements on a cycle, not expanded.
    cut: &'d FxHashMap<(BlockId, InstanceId), Reported>,
    /// Each block's proof that something in it is broken ([`broken_blocks`]).
    broken: &'d [Option<Reported>],
}

impl RootFlattener<'_> {
    /// `root` flattened, or the error if it has more than [`MAX_PLACEMENTS`].
    #[expect(clippy::result_large_err, reason = "once per root")]
    fn flatten(&self, root: BlockId) -> Result<FlatDesign, FlattenError> {
        // Pass 3, the instance tree: every placement, top-down.
        let instances = self.instance_tree(root)?;

        // Pass 4, the joins: one union per port binding and per `net x = [..]`.
        let entries = Entries::new(self.design, &instances);
        let mut joins = self.joins(&instances, &entries);

        // Pass 5, the nets: one per group of joined entries, named by rank (E15). The
        // root's own nets are kept as flat nets too: where its probes and sources land.
        let (nets, net_of) = self.nets(&instances, &entries, &mut joins);
        // The root's entries come first (`Entries::new`).
        let root_nets = net_of[..self.design.block(root).nets.len()].to_vec();

        // Pass 6, the devices and the knobs.
        let (devices, mut knobs) = self.devices(&instances, &entries, &net_of);

        // Pass 7, the default setup: its fields fixed or Range knobs, after the part
        // knobs, so a setup renumbers no part knob.
        let setup = self.setup(root, &mut knobs);

        // Pass 8, the ground nets: every net with a `Ground` port. Lowering makes the
        // one ground net node 0 (E20), checked when the root is simulated.
        let grounds = checks::ground_nets(self.design, &instances, &nets);

        // Whether anything in it is broken: then it isn't checked as a whole (E7).
        let tainted = self.tainted(&instances);
        Ok(FlatDesign {
            instances,
            nets,
            devices,
            knobs: knobs.list,
            grounds,
            root_nets,
            setup,
            tainted,
        })
    }

    /// Every placement under `root`, in tree order: the root, then each child with its
    /// own subtree, in name order. Among placements at the same depth, this order is the
    /// order of their paths.
    ///
    /// Past [`MAX_PLACEMENTS`], the error, at the placement that went over, and the
    /// tree grows no further.
    #[expect(clippy::result_large_err, reason = "once per root")]
    fn instance_tree(&self, root: BlockId) -> Result<Vec<FlatInstance>, FlattenError> {
        let mut instances = Vec::new();
        let mut todo = vec![FlatInstance {
            block: root,
            origin: None,
        }];
        while let Some(instance) = todo.pop() {
            if instances.len() == MAX_PLACEMENTS {
                return Err(self.too_many(root, &instances, &instance));
            }
            let at = FlatInstanceId::new(instances.len());
            // Pushed in reverse, so they come off the stack in name order.
            for &(i, child) in self.children[instance.block.index()].iter().rev() {
                if self.cut.contains_key(&(instance.block, i)) {
                    continue;
                }
                todo.push(FlatInstance {
                    block: child,
                    origin: Some(LocalInstance { at, instance: i }),
                });
            }
            instances.push(instance);
        }
        Ok(instances)
    }

    /// `root` has more placements than [`MAX_PLACEMENTS`]: `over` is the first past it,
    /// under `instances`.
    fn too_many(
        &self,
        root: BlockId,
        instances: &[FlatInstance],
        over: &FlatInstance,
    ) -> FlattenError {
        let origin = over.origin.expect("the root is the first placement");
        let problem = FlattenProblem::TooManyPlacements {
            root: self.design.block(root).name.clone(),
            limit: MAX_PLACEMENTS,
        };
        let at = written(self.map, instances, origin).name;
        FlattenError::new(problem.into(), at)
    }

    /// Every entry joined to the ones it's the same net as.
    fn joins(&self, instances: &[FlatInstance], entries: &Entries) -> UnionFind {
        let mut joins = UnionFind::new(entries.len());
        for (at, instance) in instances.iter().enumerate() {
            let at = FlatInstanceId::new(at);
            let block = self.design.block(instance.block);
            // `vcc: v12`: the child's port net is the parent's net.
            if let Some(origin) = instance.origin {
                let parent_block = self.design.block(instances[origin.at.index()].block);
                let placement = &parent_block.instances[origin.instance.index()];
                for (port, net) in placement.pins.iter().enumerate() {
                    if let Ok(net) = *net {
                        let port = block.port_net(PortId::new(port));
                        joins.union(entries.of(at, port), entries.of(origin.at, net));
                    }
                }
            }
            // `net x = [a, b]`.
            for merge in &block.merges {
                for &other in &merge.with {
                    joins.union(entries.of(at, merge.net), entries.of(at, other));
                }
            }
        }
        joins
    }

    /// One net per group of joined entries. In each group the smallest key names the
    /// net: depth first (the outer net survives, as in Verilator and KiCad's ranking),
    /// then rank (port, merge target, net), then the path. Nets are numbered in order
    /// of their name, so a shuffle of statements gives the same nets.
    ///
    /// The path is compared without being built: placements come in tree order, so at
    /// equal depth `(placement, local name)` orders exactly as the paths do.
    fn nets(
        &self,
        instances: &[FlatInstance],
        entries: &Entries,
        joins: &mut UnionFind,
    ) -> (Vec<FlatNet>, Vec<FlatNetId>) {
        // Every local net's key, in the group of the net it's joined into.
        let depths = depths(instances);
        // Each group's keys; `group_of` finds a group by its union-find root.
        let mut groups: Vec<Vec<NameKey>> = Vec::new();
        let mut group_of: Vec<Option<usize>> = vec![None; entries.len()];
        for (at, instance) in instances.iter().enumerate() {
            let at = FlatInstanceId::new(at);
            let block = self.design.block(instance.block);
            let targets = &self.targets[instance.block.index()];
            for (net, local) in block.nets.iter().enumerate() {
                let net = NetId::new(net);
                let key = NameKey {
                    depth: depths[at.index()],
                    rank: Rank::of(block, net, targets),
                    at,
                    name: &local.name,
                    net,
                };
                let root = joins.find(entries.of(at, net));
                let group = *group_of[root].get_or_insert_with(|| {
                    groups.push(Vec::new());
                    groups.len() - 1
                });
                groups[group].push(key);
            }
        }
        // The first key of each group names its net, and the nets are numbered in order
        // of their names.
        for group in &mut groups {
            group.sort_unstable();
        }
        // In order of each group's name. The names are copied out once: comparing
        // `a[0]` with `b[0]` would reach into two groups per comparison, a cache miss
        // each on a big design.
        groups.sort_by_cached_key(|group| group[0]);

        let mut net_of = vec![FlatNetId::new(0); entries.len()];
        let nets = groups
            .into_iter()
            .enumerate()
            .map(|(id, group)| {
                let names = group
                    .into_iter()
                    .map(|key| {
                        net_of[entries.of(key.at, key.net)] = FlatNetId::new(id);
                        LocalNet {
                            at: key.at,
                            net: key.net,
                        }
                    })
                    .collect();
                FlatNet { names }
            })
            .collect();
        (nets, net_of)
    }

    /// Every leaf part, placement by placement in tree order and each one's parts in
    /// name order, with its pins on flat nets and its fields fixed or knobs, and the
    /// knobs. A given value with a spread becomes a knob of its own per placement
    /// (E16): two placements are two physical parts.
    fn devices(
        &self,
        instances: &[FlatInstance],
        entries: &Entries,
        net_of: &[FlatNetId],
    ) -> (Vec<FlatDevice>, Knobs) {
        let mut devices = Vec::new();
        let mut knobs = Knobs::default();
        for (at, instance) in instances.iter().enumerate() {
            let at = FlatInstanceId::new(at);
            let block = self.design.block(instance.block);
            for &(i, kind) in &self.parts[instance.block.index()] {
                let part = &block.instances[i.index()];
                let device = FlatDeviceId::new(devices.len());
                let pins = part
                    .pins
                    .iter()
                    .map(|net| net.map(|net| net_of[entries.of(at, net)]))
                    .collect();
                let fields = part
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(field, &value)| knobs.field(value, KnobSource::Field { device, field }))
                    .collect();
                devices.push(FlatDevice {
                    kind,
                    origin: LocalInstance { at, instance: i },
                    pins,
                    fields,
                });
            }
        }
        (devices, knobs)
    }

    /// The proof that something in the tree is broken, if anything is: then the circuit
    /// isn't complete, and isn't checked as a whole.
    fn tainted(&self, instances: &[FlatInstance]) -> Option<Reported> {
        instances.iter().find_map(|i| self.broken[i.block.index()])
    }
}

/// One root's knobs, in id order, as passes 6 and 7 add them: a field refers to its
/// knob by id (E17). [`RootFlattener::flatten`] keeps the list as [`FlatDesign::knobs`].
#[derive(Default)]
struct Knobs {
    list: Vec<Knob>,
}

impl Knobs {
    /// `value` as a flat field: a knob from `source` if it's given and varies (E16).
    fn field(&mut self, value: FieldValue, source: KnobSource) -> FlatField {
        match value {
            FieldValue::Unset => FlatField::Unset,
            FieldValue::Invalid(reported) => FlatField::Invalid(reported),
            FieldValue::Given(value) => self.given(value, source),
        }
    }

    /// `value`, fixed, or a knob from `source` if it varies: a spread of nothing isn't a
    /// knob (`± 0%`; ngspice's `agauss` returns the nominal for one).
    fn given(&mut self, value: Value, source: KnobSource) -> FlatField {
        if !value.varies() {
            return FlatField::Exact(value.nominal);
        }
        self.list.push(Knob { source, value });
        FlatField::Knob(KnobId::new(self.list.len() - 1))
    }
}

/// How a local net ranks for naming its merged net, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Port,
    /// The `x` of `net x = [a, b]`: you named the merged net (decided 2026-09-28).
    MergeTarget,
    Net,
}

impl Rank {
    /// How `net` of `block` ranks; `targets` are the block's merge targets.
    fn of(block: &Block, net: NetId, targets: &FxHashSet<NetId>) -> Rank {
        if block.net_port(net).is_some() {
            Rank::Port
        } else if targets.contains(&net) {
            Rank::MergeTarget
        } else {
            Rank::Net
        }
    }
}

/// What orders the local nets of a merged net: the smallest names it. The fields are
/// compared in order; `net` only breaks the tie between two nets of one placement with
/// one name, which resolve doesn't allow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct NameKey<'d> {
    depth: usize,
    rank: Rank,
    /// The placement, in tree order: at equal depth, the order of the paths.
    at: FlatInstanceId,
    name: &'d str,
    net: NetId,
}

/// Where the `let` that `origin` names was written.
fn written<'m>(
    map: &'m DesignSourceMap,
    instances: &[FlatInstance],
    origin: LocalInstance,
) -> &'m InstanceSpans {
    let block = instances[origin.at.index()].block;
    &map.blocks[block.index()].instances[origin.instance.index()]
}

/// Each placement's depth below the root, from its parent's (parents come first).
fn depths(instances: &[FlatInstance]) -> Vec<usize> {
    let mut depths: Vec<usize> = Vec::with_capacity(instances.len());
    for instance in instances {
        let depth = instance
            .origin
            .map_or(0, |origin| depths[origin.at.index()] + 1);
        depths.push(depth);
    }
    depths
}

/// Every local net of every placement as one dense index, for the union-find: the
/// entry of `(at, net)` is `base[at] + net` (as Yosys's `SigMap` indexes bits).
struct Entries {
    base: Vec<usize>,
    len: usize,
}

impl Entries {
    fn new(design: &Design, instances: &[FlatInstance]) -> Self {
        let mut base = Vec::with_capacity(instances.len());
        let mut len = 0;
        for instance in instances {
            base.push(len);
            len += design.block(instance.block).nets.len();
        }
        Self { base, len }
    }

    fn of(&self, at: FlatInstanceId, net: NetId) -> usize {
        self.base[at.index()] + net.index()
    }

    fn len(&self) -> usize {
        self.len
    }
}

/// Disjoint sets over `0..n`, with path halving. Unions pick any root; naming is a
/// separate step (as Yosys's `SigMap`, then `opt_clean` choosing the name).
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        self.parent[a] = b;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spicy_span::Span;

    /// A shorted part at `span`, in the placement `example` of `Mid`'s 3.
    fn shorted(span: Span, example: &str) -> FlattenError {
        let kind = FlattenErrorKind {
            problem: FlattenProblem::ShortedPart {
                part: "r".to_string(),
            },
            in_block: Some(InBlock {
                block: "Mid".to_string(),
                found: 1,
                placements: 3,
                example: example.to_string(),
            }),
        };
        FlattenError::new(kind, span)
    }

    fn too_many(root: &str, span: Span) -> FlattenError {
        let problem = FlattenProblem::TooManyPlacements {
            root: root.to_string(),
            limit: MAX_PLACEMENTS,
        };
        FlattenError::new(problem.into(), span)
    }

    /// The same problem in two placements is one, the first found, counting both; one
    /// elsewhere is another.
    #[test]
    fn a_problem_in_several_placements_is_one() {
        let (here, there) = (Span::new(10, 20), Span::new(30, 40));
        let errors = grouped(vec![
            shorted(here, "a"),
            shorted(there, "a"),
            shorted(here, "b"),
        ]);
        let found: Vec<_> = errors
            .iter()
            .map(|e| {
                let in_block = e.kind.in_block.as_ref().unwrap();
                (e.span, in_block.found, in_block.example.as_str())
            })
            .collect();
        assert_eq!(found, [(here, 2, "a"), (there, 1, "a")]);
    }

    /// Two problems in one place that say different things stay two: two roots that
    /// both go over the limit inside one block they place.
    #[test]
    fn different_problems_in_one_place_stay_apart() {
        let here = Span::new(10, 20);
        let errors = grouped(vec![
            too_many("A", here),
            too_many("B", here),
            too_many("A", here),
        ]);
        let roots: Vec<_> = errors
            .iter()
            .map(|e| match &e.kind.problem {
                FlattenProblem::TooManyPlacements { root, .. } => root.as_str(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(roots, ["A", "B"]);
    }
}
