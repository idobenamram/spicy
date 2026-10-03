//! Pass 2 for one block: its circuit, resolved by a [`BodyResolver`] of its own (as
//! rust-analyzer lowers each body with its own `ExprCollector`). The body has two passes
//! of its own: every name it declares ([`BodyResolver::new`]), then every statement
//! ([`BodyResolver::resolve`]), so statement order never matters (model.md E4).
//!
//! Each statement is resolved into a value (an instance, a merge) and stored only if
//! the statement owns its name: a second `let` or `net` of a name is checked for its
//! own mistakes, then dropped.

use spicy_index::fx::FxHashMap;

use spicy_errors::Reported;
use spicy_model::design::{
    Block, BlockId, BlockSpans, FieldValue, Instance, InstanceOf, InstanceSpans, Merge, MergeSpans,
    NetId,
};
use spicy_model::prelude::{FieldSchema, PartKind};
use spicy_span::Span;

use super::fields::{Given, UnboundSlots, left_out_required};
use super::{
    BlockBuilder, NameKind, Namespace, ResolveError, ResolveErrorKind, Resolver, Scope, Signature,
    suggest, unknown_name,
};
use crate::parser::ast::{self, Expr, ExprKind, Field, Ident, Stmt, StmtKind, StructLit};

/// What a name in a block's value namespace is (model.md E5).
#[derive(Clone, Copy)]
enum ValueName {
    Net(NetId),
    /// Stored when its statement is resolved; nothing refers to it by id before then.
    Instance,
}

/// Resolves one block's body into the block its signature starts.
pub(super) struct BodyResolver<'r, 'p, 'src> {
    r: &'r mut Resolver<'p, 'src>,
    /// Every block's signature, indexed by `BlockId`: what a placement binds.
    signatures: &'r [Signature<'src>],
    /// The block's value namespace: its ports, nets and instances.
    names: Scope<'src, ValueName>,
    /// The block so far: the signature's ports, then what the body adds.
    block: BlockBuilder,
}

/// Where a net is named, which decides how an error about it reads and what its fix is.
#[derive(Clone, Copy)]
enum NetUse<'a> {
    /// An item of `net x = [a, b];`.
    Merge,
    /// Bound to a pin: `a: vcc`.
    Pin(&'a str),
    /// `C { gnd }`, short for `gnd: gnd`: the name written is both the pin and the net,
    /// so a fix must keep the pin (`gnd: gnd1`).
    Shorthand(&'a str),
}

impl NetUse<'_> {
    fn pin(self) -> Option<String> {
        match self {
            NetUse::Merge => None,
            NetUse::Pin(pin) | NetUse::Shorthand(pin) => Some(pin.to_string()),
        }
    }
}

/// What `Kind { … }` may bind, as numbered slots: the part's pins or the block's ports
/// first, then the part's fields.
#[derive(Clone, Copy)]
struct Slots {
    of: InstanceOf,
    pins: usize,
    fields: &'static [FieldSchema],
}

impl Slots {
    fn len(self) -> usize {
        self.pins + self.fields.len()
    }

    /// Whether slot `i` must be given: every pin or port, and a required field.
    fn required(self, i: usize) -> bool {
        i < self.pins || self.fields[i - self.pins].required
    }

    /// Whether `given`'s value could go in slot `i`: a pin binds a net (a name), a
    /// field takes a number. A misspelled slot is one whose value fits it.
    fn fits(self, given: Given, i: usize) -> bool {
        let is_name = given
            .value
            .is_none_or(|e| matches!(e.kind, ExprKind::Path(_)));
        is_name == (i < self.pins)
    }
}

/// What the fields of `Kind { … }` bound, by slot.
struct Bindings<'f> {
    /// Where each slot was given; `None` if it wasn't.
    given: Vec<Option<Span>>,
    /// Each pin's net, once given.
    pins: Vec<Option<Result<NetId, Reported>>>,
    /// Each field's value: `Unset` until it's given, as a setup's are.
    fields: Vec<FieldValue>,
    /// The fields that name no slot.
    unknown: Vec<Given<'f, 'f>>,
}

impl Bindings<'_> {
    /// The instance's pins and fields. A pin or a required field that wasn't given
    /// holds `unbound`, the proof it was reported; an optional one is `Unset`. (The pins
    /// are collected in place: an element is the same size either way.)
    fn values(
        mut self,
        slots: Slots,
        unbound: Option<Reported>,
    ) -> (Vec<Result<NetId, Reported>>, Vec<FieldValue>) {
        let missing = || unbound.expect("a slot that must be given and wasn't is reported");
        let pins = self
            .pins
            .into_iter()
            .map(|pin| pin.unwrap_or_else(|| Err(missing())));
        left_out_required(&mut self.fields, slots.fields, missing);
        (pins.collect(), self.fields)
    }

    /// Where each pin, then each field, was given: taken out, before the values.
    fn spans(&mut self, slots: Slots) -> (Vec<Option<Span>>, Vec<Option<Span>>) {
        let mut pins = std::mem::take(&mut self.given);
        let fields = pins.split_off(slots.pins);
        (pins, fields)
    }
}

impl<'r, 'p, 'src> BodyResolver<'r, 'p, 'src> {
    /// The body's first pass: its value namespace, from the signature's ports and every
    /// net and instance the circuit's statements (`stmts`, none without a circuit)
    /// declare. A net is only its name, so declaring it adds it to the block (a net can
    /// be used before its line); an instance is added when its statement is resolved.
    pub(super) fn new(
        r: &'r mut Resolver<'p, 'src>,
        signatures: &'r [Signature<'src>],
        signature: &Signature<'src>,
        block: BlockBuilder,
        stmts: &[Stmt<'src>],
    ) -> Self {
        // Sized once for every name the body declares, so it never rehashes.
        let declares =
            |stmt: &&Stmt| matches!(stmt.kind, StmtKind::Net { .. } | StmtKind::Let { .. });
        let count = signature.ports.declared.len() + stmts.iter().filter(declares).count();
        let mut declared = FxHashMap::with_capacity_and_hasher(count, Default::default());
        for (&name, &(port, at)) in &signature.ports.declared {
            declared.insert(name, (ValueName::Net(block.block.port_net(port)), at));
        }
        let mut this = Self {
            names: Scope { declared },
            block,
            r,
            signatures,
        };
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Net { name, .. } => {
                    let net = NetId::new(this.block.block.nets.len());
                    let value = ValueName::Net(net);
                    let errors = &mut this.r.errors;
                    if this
                        .names
                        .declare(name, value, NameKind::Net, errors)
                        .is_ok()
                    {
                        this.block.push_net(name);
                    }
                }
                StmtKind::Let { name, .. } => {
                    let errors = &mut this.r.errors;
                    let _ =
                        this.names
                            .declare(name, ValueName::Instance, NameKind::Instance, errors);
                }
                _ => {}
            }
        }
        this
    }

    /// The body's second pass: every statement, each stored if it owns its name.
    /// Returns the finished block and where each part of it was written.
    pub(super) fn resolve(mut self, stmts: &[Stmt<'src>]) -> (Block, BlockSpans) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Net {
                    name,
                    merge: Some(list),
                } => {
                    let merge = self.merge(name, stmt, list);
                    if let Some(merge) = merge
                        && self.names.is_first(name)
                    {
                        self.block.push_merge(merge);
                    }
                }
                StmtKind::Let { name, value } => {
                    let instance = self.instance(name, stmt, value);
                    if self.names.is_first(name) {
                        self.block.push_instance(instance);
                    }
                }
                _ => {}
            }
        }
        (self.block.block, self.block.spans)
    }

    // --- Other blocks: what a placement binds ---------------------------------------

    /// The slot a field named `name` binds on `of`: a pin or port first, then a field.
    fn slot(&self, of: InstanceOf, name: &str) -> Option<usize> {
        match of {
            InstanceOf::Block(b) => self.signatures[b.index()]
                .ports
                .get(name)
                .map(|p| p.index()),
            InstanceOf::Part(kind) => {
                let pins = kind.pins();
                let pin = pins.iter().position(|p| *p == name);
                pin.or_else(|| kind.field(name).map(|(f, _)| pins.len() + f))
            }
            InstanceOf::Error(_) => None,
        }
    }

    /// Every slot's name on `of`, in slot order, for messages.
    fn slot_names(&self, of: InstanceOf) -> Vec<String> {
        match of {
            InstanceOf::Block(b) => {
                let mut ports: Vec<_> = self.signatures[b.index()].ports.iter().collect();
                ports.sort_unstable_by_key(|&(_, port)| port);
                ports
                    .into_iter()
                    .map(|(name, _)| name.to_string())
                    .collect()
            }
            InstanceOf::Part(kind) => {
                let fields = kind.fields().iter().map(|f| f.name);
                kind.pins()
                    .iter()
                    .copied()
                    .chain(fields)
                    .map(str::to_string)
                    .collect()
            }
            InstanceOf::Error(_) => Vec::new(),
        }
    }

    /// How many ports block `b` has.
    fn port_count(&self, b: BlockId) -> usize {
        self.signatures[b.index()].ports.declared.len()
    }

    /// What a value name is, for messages: a port's net is "a port" (model.md E5).
    fn name_kind(&self, name: ValueName) -> NameKind {
        match name {
            ValueName::Net(n) if self.block.block.net_port(n).is_some() => NameKind::Port,
            ValueName::Net(_) => NameKind::Net,
            ValueName::Instance => NameKind::Instance,
        }
    }

    // --- Merges and nets ------------------------------------------------------------

    /// `net x = [a, b];`: `x` merged with each listed net. `None` if the value isn't a
    /// list, reported.
    fn merge(&mut self, name: &Ident, stmt: &Stmt, list: &Expr) -> Option<(Merge, MergeSpans)> {
        let ExprKind::Array(items) = &list.kind else {
            if !matches!(list.kind, ExprKind::Error(_)) {
                self.r.report(ResolveErrorKind::BadMerge, list.span);
            }
            return None;
        };
        let mut with = Vec::new();
        let mut item_spans = Vec::new();
        for item in items {
            if let Ok(other) = self.net_ref(item, NetUse::Merge) {
                with.push(other);
                item_spans.push(item.span);
            }
        }
        let Some(ValueName::Net(net)) = self.names.get(name.text) else {
            return None; // a second `net x` of an instance's name, reported
        };
        let merge = Merge { net, with };
        let spans = MergeSpans {
            stmt: stmt.span,
            items: item_spans,
        };
        Some((merge, spans))
    }

    /// An expression that must name a net or port of the block.
    fn net_ref(&mut self, e: &Expr, used: NetUse) -> Result<NetId, Reported> {
        match &e.kind {
            ExprKind::Path(path) => {
                let name = self.r.path_text(path);
                self.net_named(name, e.span, used)
            }
            ExprKind::Error(reported) => Err(*reported),
            _ => Err(self.not_a_net(used, NameKind::Value, e.span)),
        }
    }

    /// The net `name`, written at `at`.
    fn net_named(&mut self, name: &str, at: Span, used: NetUse) -> Result<NetId, Reported> {
        match self.names.get(name) {
            Some(ValueName::Net(net)) => Ok(net),
            Some(ValueName::Instance) => Err(self.not_a_net(used, NameKind::Instance, at)),
            None => {
                // Only nets: suggesting an instance would trade this error for another.
                let nets = self
                    .names
                    .iter()
                    .filter(|(_, v)| matches!(v, ValueName::Net(_)))
                    .map(|(k, _)| k);
                let suggestion = suggest(&mut self.r.suggestions_left, name, nets);
                let fix = suggestion.as_ref().map(|s| match used {
                    NetUse::Shorthand(pin) => format!("{pin}: {s}"),
                    NetUse::Merge | NetUse::Pin(_) => s.clone(),
                });
                let error = unknown_name(name, Namespace::Value, suggestion, fix, at);
                Err(error.report(&mut self.r.errors))
            }
        }
    }

    fn not_a_net(&mut self, used: NetUse, found: NameKind, at: Span) -> Reported {
        let kind = ResolveErrorKind::NotANet {
            pin: used.pin(),
            found,
        };
        self.r.report(kind, at)
    }

    // --- Instances ------------------------------------------------------------------

    /// `let name = Kind { fields };`: a part or a placed block (model.md E6), with where
    /// each part of it was written. A value that isn't `Kind { … }`, or a kind that
    /// doesn't resolve, gives an `InstanceOf::Error` instance, reported.
    fn instance(&mut self, name: &Ident, stmt: &Stmt, value: &Expr) -> (Instance, InstanceSpans) {
        let mut spans = InstanceSpans {
            name: name.span,
            stmt: stmt.span,
            ..InstanceSpans::default()
        };
        let instance = |of, pins, fields| Instance {
            name: name.text.to_string(),
            of,
            pins,
            fields,
        };
        let ExprKind::StructLit(lit) = &value.kind else {
            let reported = match value.kind {
                ExprKind::Error(reported) => reported,
                _ => self.r.report(ResolveErrorKind::LetNotInstance, value.span),
            };
            let unresolved = instance(InstanceOf::Error(reported), Vec::new(), Vec::new());
            return (unresolved, spans);
        };
        let StructLit {
            path,
            generics,
            fields,
        } = &**lit;
        spans.kind = path.span;
        if let (Some(first), Some(last)) = (generics.first(), generics.last()) {
            let at = Span::new(first.span.start, last.span.end);
            let what = "generic arguments";
            self.r.report(ResolveErrorKind::Unsupported { what }, at);
        }
        let slots = match self.kind_named(path) {
            of @ InstanceOf::Part(kind) => Slots {
                of,
                pins: kind.pins().len(),
                fields: kind.fields(),
            },
            of @ InstanceOf::Block(b) => {
                // Its ports are still bound: the placement is checked as written.
                if !self.signatures[b.index()].has_circuit {
                    let block = self.r.path_text(path).to_string();
                    self.r
                        .report(ResolveErrorKind::NoCircuit { block }, path.span);
                }
                Slots {
                    of,
                    pins: self.port_count(b),
                    fields: &[],
                }
            }
            // Reported; its fields aren't checked.
            of @ InstanceOf::Error(_) => return (instance(of, Vec::new(), Vec::new()), spans),
        };
        let mut bound = self.bind(slots, fields);
        let unbound = self.report_unbound(slots, path, &bound);

        (spans.pins, spans.fields) = bound.spans(slots);
        let (pins, fields) = bound.values(slots, unbound);
        (instance(slots.of, pins, fields), spans)
    }

    /// The part kind or block `path` names, in the kind namespace: the file's blocks,
    /// then the prelude, searched last so it never shadows your own names (model.md E5,
    /// rustc's order). `InstanceOf::Error` if it names neither, reported.
    fn kind_named(&mut self, path: &ast::Path) -> InstanceOf {
        let name = self.r.path_text(path);
        if let Some(block) = self.r.blocks.get(name) {
            return InstanceOf::Block(block);
        }
        if let Some(kind) = PartKind::from_name(name) {
            return InstanceOf::Part(kind);
        }
        let error = self.not_a_kind(name, path.span);
        InstanceOf::Error(error.report(&mut self.r.errors))
    }

    /// Why `name` isn't a part kind or block: it's something else, or nothing.
    fn not_a_kind(&mut self, name: &str, at: Span) -> ResolveError {
        // Another kind (a signal type: blocks and part kinds were looked up), or one of
        // the block's values (`vcc`).
        let value = |name| self.names.get(name).map(|v| self.name_kind(v));
        let is = self.r.kind_of(name).or_else(|| value(name));
        if let Some(is) = is {
            let kind = ResolveErrorKind::WrongNamespace {
                name: name.to_string(),
                is,
                expected: "a part kind or block",
            };
            return ResolveError::new(kind, at);
        }
        let blocks = self.r.blocks.iter().map(|(name, _)| name);
        let parts = PartKind::ALL.iter().map(|k| k.name());
        let suggestion = suggest(&mut self.r.suggestions_left, name, blocks.chain(parts));
        unknown_name(name, Namespace::Kind, suggestion.clone(), suggestion, at)
    }

    /// Binds each of `fields` to the slot of its name: a pin to a net, a field to a
    /// typed value. A slot given twice keeps its first binding.
    fn bind<'f>(&mut self, slots: Slots, fields: &'f [Field<'f>]) -> Bindings<'f> {
        let mut b = Bindings {
            given: vec![None; slots.len()],
            pins: vec![None; slots.pins],
            fields: vec![FieldValue::Unset; slots.fields.len()],
            unknown: Vec::new(),
        };
        for field in fields {
            let given = Given::from(field);
            let name = field.name.text;
            let Some(i) = self.slot(slots.of, name) else {
                b.unknown.push(given);
                continue;
            };
            // Checked even when it's the second binding of the slot: a second definition
            // is checked for its own mistakes, just not stored.
            let first = b.given[i].is_none();
            if i < slots.pins {
                let net = match &field.value {
                    Some(e) => self.net_ref(e, NetUse::Pin(name)),
                    None => self.net_named(name, field.name.span, NetUse::Shorthand(name)),
                };
                if first {
                    b.pins[i] = Some(net);
                }
            } else {
                let f = i - slots.pins;
                let e = self.r.written_value(given);
                let value = e.and_then(|e| self.r.value(e, slots.fields[f].ty, name));
                let value = FieldValue::from(value);
                if first {
                    // Its error may be a const's, reported at the const: the block is
                    // broken either way (as rustc's typeck taints a body that meets a
                    // type already holding an error).
                    if let FieldValue::Invalid(reported) = value {
                        self.block.taint(reported);
                    }
                    b.fields[f] = value;
                }
            }
            self.r.given_once(&mut b.given[i], given);
        }
        b
    }

    /// Reports the fields that name no slot, and every slot that must be given and
    /// wasn't: unbound pins or ports, and unset required fields. Returns the proof, if
    /// it reported any.
    fn report_unbound(&mut self, slots: Slots, path: &ast::Path, b: &Bindings) -> Option<Reported> {
        let missing: Vec<usize> = (0..slots.len())
            .filter(|&i| b.given[i].is_none() && slots.required(i))
            .collect();
        if b.unknown.is_empty() && missing.is_empty() {
            return None;
        }
        let of = self.r.path_text(path);
        let names = self.slot_names(slots.of);
        let unbound = UnboundSlots {
            names: &names,
            given: &b.given,
            missing: &missing,
        };
        let is_net = |name: &str| matches!(self.names.get(name), Some(ValueName::Net(_)));
        let (mut reported, renamed) = self.r.report_unknown(
            of,
            NameKind::Binding,
            &b.unknown,
            &unbound,
            |u, i| slots.fits(b.unknown[u], i),
            is_net,
        );
        if renamed.is_none() && !missing.is_empty() {
            let kind = ResolveErrorKind::Missing {
                of: of.to_string(),
                names: missing.iter().map(|&i| names[i].clone()).collect(),
            };
            reported = Some(self.r.report(kind, path.span));
        }
        reported
    }
}
