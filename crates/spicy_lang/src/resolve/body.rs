//! Pass 2 for one block: its body, resolved by a [`BodyResolver`] of its own (as
//! rust-analyzer lowers each body with its own `ExprCollector`). The body has two passes
//! of its own: every name it declares ([`BodyResolver::new`]), then every statement
//! ([`BodyResolver::resolve`]), so statement order never matters (model.md E4).
//!
//! Each statement is resolved into a value (an instance, a merge) and stored only if
//! the statement owns its name: a second `let` or `net` of a name is checked for its
//! own mistakes, then dropped.

use std::collections::{HashMap, HashSet};

use spicy_errors::Reported;
use spicy_model::design::{
    Block, BlockId, BlockSpans, FieldValue, Instance, InstanceOf, InstanceSpans, Merge, MergeSpans,
    NetId,
};
use spicy_model::prelude::{FieldSchema, PartKind, SignalType};
use spicy_span::Span;

use super::{
    BlockBuilder, NameKind, Namespace, ResolveError, ResolveErrorKind, Resolver, Scope, Signature,
    suggest, unknown_name,
};
use crate::parser::ast::{self, Body, Expr, ExprKind, Field, Ident, Stmt, StmtKind};

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

    /// Whether `field`'s value could go in slot `i`: a pin binds a net (a name), a
    /// field takes a number. A misspelled slot is one whose value fits it.
    fn fits(self, field: &Field, i: usize) -> bool {
        let is_name = field
            .value
            .as_ref()
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
    /// Each field's value, once given.
    fields: Vec<Option<FieldValue>>,
    /// The fields that name no slot.
    unknown: Vec<&'f Field<'f>>,
}

impl Bindings<'_> {
    /// The instance's pins and fields. A pin or a required field that wasn't given
    /// holds `unbound`, the proof it was reported; an optional one is `Unset`. (Each is
    /// collected in place: an element is the same size either way.)
    fn values(
        self,
        slots: Slots,
        unbound: Option<Reported>,
    ) -> (Vec<Result<NetId, Reported>>, Vec<FieldValue>) {
        let missing = || unbound.expect("a slot that must be given and wasn't is reported");
        let pins = self
            .pins
            .into_iter()
            .map(|pin| pin.unwrap_or_else(|| Err(missing())));
        let fields = self
            .fields
            .into_iter()
            .zip(slots.fields)
            .map(|(value, schema)| match value {
                Some(value) => value,
                None if schema.required => FieldValue::Invalid(missing()),
                None => FieldValue::Unset,
            });
        (pins.collect(), fields.collect())
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
    /// net and instance the body declares. A net is only its name, so declaring it adds
    /// it to the block (a net can be used before its line); an instance is added when
    /// its statement is resolved.
    pub(super) fn new(
        r: &'r mut Resolver<'p, 'src>,
        signatures: &'r [Signature<'src>],
        signature: &Signature<'src>,
        block: BlockBuilder,
        body: &Body<'src>,
    ) -> Self {
        // Sized once for every name the body declares, so it never rehashes.
        let declares =
            |stmt: &&Stmt| matches!(stmt.kind, StmtKind::Net { .. } | StmtKind::Let { .. });
        let count = signature.declared.len() + body.stmts.iter().filter(declares).count();
        let mut declared = HashMap::with_capacity(count);
        for (&name, &(port, at)) in &signature.declared {
            declared.insert(name, (ValueName::Net(block.block.port_net(port)), at));
        }
        let mut this = Self {
            names: Scope { declared },
            block,
            r,
            signatures,
        };
        for stmt in &body.stmts {
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
    pub(super) fn resolve(mut self, body: &Body<'src>) -> (Block, BlockSpans) {
        for stmt in &body.stmts {
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
            InstanceOf::Block(b) => self.signatures[b.index()].get(name).map(|p| p.index()),
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
                let mut ports: Vec<_> = self.signatures[b.index()].iter().collect();
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
        self.signatures[b.index()].declared.len()
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
        let ExprKind::StructLit { path, fields } = &value.kind else {
            let reported = match value.kind {
                ExprKind::Error(reported) => reported,
                _ => self.r.report(ResolveErrorKind::LetNotInstance, value.span),
            };
            let unresolved = instance(InstanceOf::Error(reported), Vec::new(), Vec::new());
            return (unresolved, spans);
        };
        spans.kind = path.span;
        let slots = match self.kind_named(path) {
            of @ InstanceOf::Part(kind) => Slots {
                of,
                pins: kind.pins().len(),
                fields: kind.fields(),
            },
            of @ InstanceOf::Block(b) => Slots {
                of,
                pins: self.port_count(b),
                fields: &[],
            },
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
        let is = if SignalType::is_name(name) {
            Some(NameKind::SignalType)
        } else {
            self.names.get(name).map(|v| self.name_kind(v))
        };
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
            fields: vec![None; slots.fields.len()],
            unknown: Vec::new(),
        };
        for field in fields {
            let name = field.name.text;
            let Some(i) = self.slot(slots.of, name) else {
                b.unknown.push(field);
                continue;
            };
            // Checked even when it's the second binding of the slot: a second definition
            // is checked for its own mistakes, just not stored.
            let first = b.given[i];
            if i < slots.pins {
                let net = match &field.value {
                    Some(e) => self.net_ref(e, NetUse::Pin(name)),
                    None => self.net_named(name, field.name.span, NetUse::Shorthand(name)),
                };
                if first.is_none() {
                    b.pins[i] = Some(net);
                }
            } else {
                let f = i - slots.pins;
                let value = match &field.value {
                    Some(e) => self.r.value(e, slots.fields[f].ty, name),
                    None => Err(self.r.report(ResolveErrorKind::NotAValue, field.span)),
                };
                if first.is_none() {
                    b.fields[f] = Some(match value {
                        Ok(value) => FieldValue::Given(value),
                        Err(reported) => FieldValue::Invalid(reported),
                    });
                }
            }
            // Slots, not names: a slot is given once, and its span is the whole binding.
            match first {
                Some(first) => {
                    let kind = ResolveErrorKind::Duplicate {
                        name: name.to_string(),
                        what: NameKind::Binding,
                    };
                    let error = ResolveError::new(kind, field.span).with_related(first);
                    error.report(&mut self.r.errors);
                }
                None => b.given[i] = Some(field.span),
            }
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
        let mut reported = None;
        let of = self.r.path_text(path);
        let slot_names = self.slot_names(slots.of);
        // Each unknown name once: `valu: 1k, valu: 2k` is one misspelling written twice.
        let mut seen: HashSet<&str> = HashSet::new();
        let first: Vec<bool> = b.unknown.iter().map(|f| seen.insert(f.name.text)).collect();
        // One unknown name and one missing slot that fits it is one mistake, a misnamed
        // pin or field (`resistance:` for `value:`): reported once, with the rename.
        let renamed = match (seen.len(), missing.as_slice()) {
            (1, &[i]) if slots.fits(b.unknown[0], i) => Some(i),
            _ => None,
        };
        for (field, &first) in b.unknown.iter().zip(&first) {
            let name = field.name.text;
            let suggestion = match renamed {
                Some(i) => Some(slot_names[i].clone()),
                None => {
                    let open = (0..slots.len())
                        .filter(|&i| b.given[i].is_none() && slots.fits(field, i))
                        .map(|i| slot_names[i].as_str());
                    suggest(&mut self.r.suggestions_left, name, open)
                }
            };
            // `C { gnd }` renamed to pin `b` is `b: gnd`, if `gnd` is a net;
            // otherwise there's no single fix. A repeat of an unknown name gets none:
            // renaming both would give the slot twice.
            let fix = match (&suggestion, &field.value) {
                _ if !first => None,
                (Some(s), Some(_)) => Some(s.clone()),
                (Some(s), None) if matches!(self.names.get(name), Some(ValueName::Net(_))) => {
                    Some(format!("{s}: {name}"))
                }
                _ => None,
            };
            let kind = ResolveErrorKind::UnknownField {
                field: name.to_string(),
                of: of.to_string(),
                valid: slot_names.clone(),
                suggestion,
            };
            let mut error = ResolveError::new(kind, field.name.span);
            if let Some(fix) = fix {
                error = error.with_fix(field.name.span, fix);
            }
            reported = Some(error.report(&mut self.r.errors));
        }
        if renamed.is_none() && !missing.is_empty() {
            let kind = ResolveErrorKind::Missing {
                of: of.to_string(),
                names: missing.iter().map(|&i| slot_names[i].clone()).collect(),
            };
            reported = Some(self.r.report(kind, path.span));
        }
        reported
    }
}
