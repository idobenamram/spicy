//! The `FlatDesign`: one root block with every placement expanded, every net merged
//! and named, every part a device and every value that varies a knob (model.md §9).
//! Built by [`crate::flatten`].
//!
//! It holds ids into the [`Design`], never names copied out of it: a name is looked up
//! when it's needed, through a [`Flat`], the flat design read together with its
//! `Design` (as rustc's `Instance` holds a `DefId`, and `tcx.def_path_str` names it).
//! Names are the stable identity (model.md E21): a [`HierPath`] from the root
//! (`left.r1`), kept as segments, never a dotted string parsed back (E14). The ids here
//! are dense indices, renumbered on every flatten; everything outside refers to paths.

use std::fmt;

use spicy_index::id;

use crate::design::{Block, BlockId, Design, Instance, InstanceId, NetId};
use crate::prelude::PartKind;
use crate::units::{Quantity, Value};
use spicy_errors::Reported;

id!(
    /// A placement, by its position in [`FlatDesign::instances`]. The root is 0.
    FlatInstanceId
);
id!(
    /// A merged net, by its position in [`FlatDesign::nets`].
    FlatNetId
);

impl FlatInstanceId {
    /// The root, which comes first.
    pub const ROOT: FlatInstanceId = FlatInstanceId(0);
}
id!(
    /// A leaf part, by its position in [`FlatDesign::devices`].
    FlatDeviceId
);
id!(
    /// A varying value, by its position in [`FlatDesign::knobs`].
    KnobId
);

/// A path down the hierarchy from the root, as segments: `left.r1` is `["left", "r1"]`.
/// The root itself is the empty path. (`ast::Path` is the other kind, a `::` path.)
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HierPath(pub Vec<String>);

impl HierPath {
    /// This path with `name` appended. Takes the path, so a chain of `child` calls
    /// copies no segment.
    pub fn child(mut self, name: &str) -> HierPath {
        self.0.push(name.to_string());
        self
    }
}

/// Segments joined with `.`, as people write them.
impl fmt::Display for HierPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.join("."))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlatDesign {
    /// Every placement, in tree order: the root first ([`root`](Self::root) is its
    /// block), then each placement's children, each with its own subtree, in name order.
    /// So among placements at the same depth, this order is the order of their paths.
    pub instances: Vec<FlatInstance>,
    /// Every net, in order of its name's rank (see [`FlatNet::names`]).
    pub nets: Vec<FlatNet>,
    /// Every leaf part: placement by placement in tree order, each one's parts in name
    /// order.
    pub devices: Vec<FlatDevice>,
    /// Every value that varies, one per placed field with a spread (E16), in the order
    /// of the devices.
    pub knobs: Vec<Knob>,
    /// The nets with a `Ground` port, in net order. Lowering makes the one ground net
    /// node 0 (E20); that there's exactly one is checked when the root is simulated
    /// ([`check_simulation`](crate::flatten::check_simulation)), since a library block
    /// needn't be a whole circuit.
    pub grounds: Vec<FlatNetId>,
    /// The proof that something in it is broken, if anything is (rustc's
    /// `tainted_by_errors`): a block in it with an error inside, or a placement cut for
    /// recursion. Then something written is missing from it, and it isn't checked or
    /// simulated as a whole (model.md E7).
    pub tainted: Option<Reported>,
}

impl FlatDesign {
    /// The block this design is the flattening of: the root placement's.
    pub fn root(&self) -> BlockId {
        self.instances[FlatInstanceId::ROOT.index()].block
    }

    /// The placement that placed `at`. `None` for the root.
    pub fn parent(&self, at: FlatInstanceId) -> Option<FlatInstanceId> {
        self.instances[at.index()].origin.map(|origin| origin.at)
    }

    /// `at`, then each placement it's inside, out to the root: the one walk up the tree
    /// (rowan's `SyntaxNode::ancestors`; rustc's `hir_parent_id_iter`, "prefer this over
    /// your own loop").
    pub fn ancestors(&self, at: FlatInstanceId) -> impl Iterator<Item = FlatInstanceId> + '_ {
        std::iter::successors(Some(at), |&at| self.parent(at))
    }
}

/// One placement of a block. Its path is its parent's, then the name of the `let` that
/// placed it: looked up on demand ([`Flat::path`]), as slang and CIRCT's `dbg.scope`
/// do, so a deep hierarchy doesn't store every prefix again at every level.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatInstance {
    pub block: BlockId,
    /// The `let` that placed it, in its parent. `None` for the root, which nothing
    /// placed.
    pub origin: Option<LocalInstance>,
}

/// A merged net: every local net, in every placement, that is joined into one.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatNet {
    /// Each local net it's made of, the one that names it first (E15), then the others,
    /// its aliases, in rank order.
    pub names: Vec<LocalNet>,
}

/// One local net inside a merged net: `left.vcc` is the net `vcc` of the placement
/// `left`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalNet {
    /// The placement it's in, and its net there (a port's net, for a port).
    pub at: FlatInstanceId,
    pub net: NetId,
}

/// One `let` inside a placement: `left`'s `r1` is the `let r1` of the block `left` is a
/// placement of (as rustc's `HirId` is an owner and a local id).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalInstance {
    /// The placement it's in, and its `let` in that placement's block.
    pub at: FlatInstanceId,
    pub instance: InstanceId,
}

/// A leaf part, placed.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatDevice {
    pub kind: PartKind,
    /// Its `let`, in the placement it's in.
    pub origin: LocalInstance,
    /// Each pin's net, in the part kind's pin order. `Err` if the binding was wrong.
    pub pins: Vec<Result<FlatNetId, Reported>>,
    /// Each field, in the part kind's field order.
    pub fields: Vec<FlatField>,
}

/// A device field: a fixed value, or a reference to a knob, never a knob's value
/// (E17): one `FlatDesign` serves every run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlatField {
    /// Not written: lowering applies the device's default.
    Unset,
    Exact(Quantity),
    Knob(KnobId),
    /// Written but wrong, or required and not written.
    Invalid(Reported),
}

/// A varying value: `left.r1.value`, `47 kΩ ± 1%`. Its path, its identity, is built
/// from where it comes from ([`Flat::knob_path`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Knob {
    pub source: KnobSource,
    pub value: Value,
    pub kind: KnobKind,
}

/// Where a knob comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnobSource {
    /// A device's field with a spread: `left.r1.value`. `field` is its index in the part
    /// kind's fields.
    Field { device: FlatDeviceId, field: usize },
}

/// How the engine treats a knob (engine.md §2). Decided by where the knob comes from,
/// not by how its spread is written: a part's `± 1%` and its `100..=300` are both
/// "the real part is anywhere in here" (language.md §5.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnobKind {
    /// A part's spread: a random variation across the parts built.
    Statistical,
    /// An operating condition the design must meet everywhere in (a setup's range, an
    /// `env` such as the ambient temperature). From setups, M1d-5.
    Range,
}

/// A flat design read together with the [`Design`] it's the flattening of, which names
/// everything in it: one handle, cheap to copy, instead of the two passed side by side
/// (as rustc reads everything through its `TyCtxt`).
#[derive(Clone, Copy, Debug)]
pub struct Flat<'d> {
    pub design: &'d Design,
    pub data: &'d FlatDesign,
}

impl<'d> Flat<'d> {
    /// The block a placement is of.
    pub fn block(self, at: FlatInstanceId) -> &'d Block {
        self.design.block(self.data.instances[at.index()].block)
    }

    /// The `let` that `origin` names.
    pub fn instance(self, origin: LocalInstance) -> &'d Instance {
        &self.block(origin.at).instances[origin.instance.index()]
    }

    /// The name of the `let` that placed `at`, which isn't the root.
    fn name(self, at: FlatInstanceId) -> &'d str {
        let origin = self.data.instances[at.index()].origin;
        &self.instance(origin.expect("the root has no `let`")).name
    }

    /// The path of a placement: its parents' names, then its own.
    pub fn path(self, at: FlatInstanceId) -> HierPath {
        self.path_from(FlatInstanceId::ROOT, at)
    }

    /// The path of a placement inside `scope`, from `scope`: `left.inner` is `inner`
    /// from `left`. Panics if `at` isn't inside `scope`.
    pub fn path_from(self, scope: FlatInstanceId, at: FlatInstanceId) -> HierPath {
        let inside = self.data.ancestors(at).take_while(|&at| at != scope);
        let mut names: Vec<String> = inside.map(|at| self.name(at).to_string()).collect();
        names.reverse();
        HierPath(names)
    }

    /// The path of a local net: `left.vcc`.
    pub fn local_path(self, name: LocalNet) -> HierPath {
        self.local_path_from(FlatInstanceId::ROOT, name)
    }

    /// The path of a local net inside `scope`, from `scope`.
    pub fn local_path_from(self, scope: FlatInstanceId, name: LocalNet) -> HierPath {
        let net = &self.block(name.at).net(name.net).name;
        self.path_from(scope, name.at).child(net)
    }

    /// A net's name: its first local net's path.
    pub fn net_path(self, net: FlatNetId) -> HierPath {
        self.local_path(self.data.nets[net.index()].names[0])
    }

    /// A device's path: `left.r1`.
    pub fn device_path(self, device: FlatDeviceId) -> HierPath {
        let origin = self.data.devices[device.index()].origin;
        self.path(origin.at).child(&self.instance(origin).name)
    }

    /// A knob's path, its identity: `left.r1.value`.
    pub fn knob_path(self, knob: KnobId) -> HierPath {
        match self.data.knobs[knob.index()].source {
            KnobSource::Field { device, field } => {
                let kind = self.data.devices[device.index()].kind;
                self.device_path(device).child(kind.fields()[field].name)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A root that places `a`, which places `b`: the walk up starts at the placement
    /// itself and ends at the root, which nothing placed.
    #[test]
    fn ancestors_walk_out_to_the_root() {
        let id = FlatInstanceId::new;
        let placed = |parent: usize| FlatInstance {
            block: BlockId::new(parent + 1),
            origin: Some(LocalInstance {
                at: id(parent),
                instance: InstanceId::new(0),
            }),
        };
        let root = FlatInstance {
            block: BlockId::new(0),
            origin: None,
        };
        let data = FlatDesign {
            instances: vec![root, placed(0), placed(1)],
            nets: vec![],
            devices: vec![],
            knobs: vec![],
            grounds: vec![],
            tainted: None,
        };
        let ancestors = |at| data.ancestors(at).collect::<Vec<_>>();
        assert_eq!(ancestors(id(2)), [id(2), id(1), id(0)]);
        assert_eq!(ancestors(FlatInstanceId::ROOT), [FlatInstanceId::ROOT]);
        assert_eq!(data.parent(id(1)), Some(FlatInstanceId::ROOT));
        assert_eq!(data.parent(FlatInstanceId::ROOT), None);
        assert_eq!(data.root(), BlockId::new(0));
    }
}
