//! Pass 1 for the file's setups (language.md §8.2, contracts_plan.md step 2): each
//! `setup S for X { … }` resolved against `X`'s ports, as passes in
//! [`setup`](Resolver::setup):
//! 1. **Keys:** each entry's key becomes a port (`vcc`), a field of what's on one
//!    (`vcc.v`), or `temp`.
//! 2. **Shapes and roles:** each port's shape, written (`vcc: Supply { … }`) or implied
//!    by its role (`vout.i: 5mA` alone). An input takes a source, an output a load, a
//!    ground nothing.
//! 3. **Fields:** the shape's fields, from the shape and from the paths into it, each
//!    given once and typed, by the helpers parts use (`resolve/fields.rs`).
//! 4. **`temp`:** an env, a const, a point or a range, as a temperature.
//! 5. **Complete:** every port with a role is set, a source's voltage is given, and so
//!    is `temp` (v5 rule 1.3.1). One error lists everything left out. A key that names
//!    nothing is reported here: one misspelled key and one key left out is one mistake
//!    (`vc` for `vcc`), reported once, with the rename.
//!
//! A setup's values may name a const, and its `temp` an env, so setups come after the
//! file's values.

use spicy_errors::Reported;
use spicy_model::design::{
    Block, BlockId, Env, FieldValue, PortId, PortSetup, PortSetupSpans, Setup, SetupId, SetupSpans,
    Temp,
};
use spicy_model::prelude::{FieldType, Shape, SignalType};
use spicy_model::units::{Dimension, Quantity, Value};
use spicy_span::Span;

use super::error::RoleMismatch;
use super::fields::{Given, UnboundSlots, left_out_required};
use super::value::unparen;
use super::{
    BlockBuilder, FileValue, NameKind, Namespace, PerBlock, Redefined, ResolveError,
    ResolveErrorKind, Resolver, Scope, Signature, suggest, unknown_name,
};
use crate::parser::ast::{
    Attribute, Body, Expr, ExprKind, Field, Ident, ItemKind, Key, SetupDecl, SetupEntry, StmtKind,
};

/// The values a setup field may not hold yet: a level that changes over a run.
const STIMULI: &[&str] = &["Step", "Sweep"];

/// A setup resolved in pass 1, with what resolve's pass 3 reads to taint it: the proof
/// if parsing it reported an error, and its whole item, where an error makes it broken.
pub(super) struct ResolvedSetup {
    pub setup: Setup,
    pub spans: SetupSpans,
    pub broken: Option<Reported>,
    pub item: Span,
}

/// The block a setup is for: its id, its ports by name and their types, and its
/// circuit, whose nets and instances only explain a key that isn't a port.
#[derive(Clone, Copy)]
struct ForBlock<'a, 'p, 'src> {
    id: BlockId,
    ports: &'a Scope<'src, PortId>,
    block: &'a Block,
    circuit: Option<&'p Body<'src>>,
}

impl ForBlock<'_, '_, '_> {
    /// Whether port `p` takes a shape: it has a role, so a setup must set it. Not a
    /// ground, a `Pin`, or a port whose type is wrong.
    fn takes_shape(self, p: usize) -> bool {
        let signal = self.block.ports[p].signal.ok();
        signal.and_then(Shape::for_role).is_some()
    }
}

/// What one entry sets.
#[derive(Clone, Copy)]
enum Target<'a, 'src> {
    /// `vcc: Supply { … }`: the port's whole shape.
    Shape(PortId),
    /// `vcc.v: 12V`: one field of what's on the port.
    Field(PortId, &'a Ident<'src>),
    /// `temp: ambient`.
    Temp,
    /// `vc: Supply { … }`: a name that's no port, reported in pass 5, once it's known
    /// what's left out.
    Unknown(&'a Ident<'src>),
}

impl Target<'_, '_> {
    /// The port the entry sets; `None` for `temp` and for a name that's no port.
    fn port(self) -> Option<PortId> {
        match self {
            Target::Shape(port) | Target::Field(port, _) => Some(port),
            Target::Temp | Target::Unknown(_) => None,
        }
    }
}

/// One entry, its key resolved.
#[derive(Clone, Copy)]
struct Entry<'a, 'src> {
    target: Target<'a, 'src>,
    key: &'a Key<'src>,
    value: &'a Expr<'src>,
    span: Span,
}

impl<'a, 'src> Entry<'a, 'src> {
    /// The entry as one binding, for the rules fields share (`resolve/fields.rs`): named
    /// by what it sets (`v` of `vcc.v`, `vcc` of `vcc: Supply { … }`), with its value.
    fn given(&self) -> Given<'a, 'src> {
        let name = match self.target {
            Target::Field(_, name) | Target::Unknown(name) => name,
            Target::Shape(_) | Target::Temp => &self.key.segments[0],
        };
        Given {
            name,
            value: Some(self.value),
            span: self.span,
        }
    }
}

/// Each port's entries, from `entries` sorted by port: one run for each port any entry
/// sets, in port order. `temp`'s entries aren't a port's.
fn port_runs<'e, 'a, 'src>(
    entries: &'e [Entry<'a, 'src>],
) -> impl Iterator<Item = (PortId, &'e [Entry<'a, 'src>])> {
    let runs = entries.chunk_by(|a, b| a.target.port() == b.target.port());
    runs.filter_map(|run| Some((run[0].target.port()?, run)))
}

/// One port's shape as it's bound: the port's type and the shape its role takes, the
/// entry that writes its shape and the fields written in it (`Supply { v: 12V }`; none when it's implied, and `Err` with the proof when
/// what's written isn't a shape at all, `vcc: 12V`), each field so far (where it was
/// given, and its value: `Unset` until it is), and the fields it must have and doesn't.
struct PortFields<'a, 'src> {
    signal: SignalType,
    shape: Shape,
    written: Option<Span>,
    lit: Result<&'a [Field<'src>], Reported>,
    given: Vec<Option<Span>>,
    values: Vec<FieldValue>,
    missing: Vec<&'static str>,
}

impl PortFields<'_, '_> {
    fn new(signal: SignalType, shape: Shape) -> Self {
        let n = shape.fields().len();
        Self {
            signal,
            shape,
            written: None,
            lit: Ok(&[]),
            given: vec![None; n],
            values: vec![FieldValue::Unset; n],
            missing: Vec::new(),
        }
    }
}

impl<'p, 'src> Resolver<'p, 'src> {
    /// Every setup of a block the file has, in source order (a setup's id is its place
    /// here), each resolved against its block's ports. Setups are named per block, so
    /// every block can have its own `Operating`. A second setup of a name for one block
    /// is checked for its own mistakes, then dropped, and the first is broken (the
    /// second-definition rule): returned with the first's id.
    pub(super) fn setups(
        &mut self,
        signatures: &[Signature<'src>],
        blocks: &[BlockBuilder],
        circuits: &PerBlock<'p, 'src>,
        envs: &[Env],
    ) -> (Vec<ResolvedSetup>, Vec<Redefined<SetupId>>) {
        let mut names: Vec<Scope<SetupId>> = vec![Scope::default(); signatures.len()];
        let (mut setups, mut seconds) = (Vec::new(), Vec::new());
        for item in &self.parsed.file.items {
            let ItemKind::Setup(decl) = &item.kind else {
                continue;
            };
            let name = self.path_text(&decl.block);
            let Some(block) = self.blocks.get(name) else {
                let kind = ResolveErrorKind::UnknownBlock {
                    item: NameKind::Setup,
                    name: name.to_string(),
                };
                self.report(kind, decl.block.span);
                continue;
            };
            let id = SetupId::new(setups.len());
            let declared =
                names[block.index()].declare(&decl.name, id, NameKind::Setup, &mut self.errors);
            let of = ForBlock {
                id: block,
                ports: &signatures[block.index()].ports,
                block: &blocks[block.index()].block,
                circuit: circuits[block.index()].map(|(body, _)| body),
            };
            let (setup, spans) = self.setup(decl, &item.attrs, of, envs);
            match declared {
                Ok(()) => setups.push(ResolvedSetup {
                    setup,
                    spans,
                    broken: decl.broken,
                    item: item.span,
                }),
                Err(redefined) => seconds.push(redefined),
            }
        }
        (setups, seconds)
    }

    /// One setup against its block's ports, with where each part of it was written.
    fn setup(
        &mut self,
        decl: &SetupDecl<'src>,
        attrs: &[Attribute],
        of: ForBlock,
        envs: &[Env],
    ) -> (Setup, SetupSpans) {
        // Attributes aren't read on setups yet (plan §2.9 c): none is ignored silently.
        self.no_attributes(attrs);
        // Pass 1, keys: what each entry sets. An entry the parser couldn't read was
        // reported there, and it may be what the setup leaves out (`temp 25°C`), so
        // then nothing is known to be left out. A key that names nothing is reported
        // once every key is known: if it's the one slot left out, misspelled, its entry
        // sets that slot.
        let (mut entries, mut unreadable) = (Vec::new(), None);
        for entry in &decl.entries {
            self.no_attributes(&entry.attrs);
            match &entry.kind {
                SetupEntry::Set { key, value } => {
                    if let Some(target) = self.key(key, of) {
                        entries.push(Entry {
                            target,
                            key,
                            value,
                            span: entry.span,
                        });
                    }
                }
                SetupEntry::Error(reported) => {
                    unreadable.get_or_insert(*reported);
                }
            }
        }
        self.unknown_keys(&mut entries, of, unreadable.is_none());
        // Each port's entries are one run, in source order (a stable sort), so passes 2
        // and 3 visit each entry once, not once per port: a setup of a block with
        // thousands of ports stays linear.
        entries.sort_by_key(|entry| entry.target.port());
        // Pass 2, shapes and roles: each port's shape, written or implied by its role.
        let mut ports: Vec<Option<PortFields>> = std::iter::repeat_with(|| None)
            .take(of.block.ports.len())
            .collect();
        for (port, on_port) in port_runs(&entries) {
            ports[port.index()] = self.port_shape(port, on_port, of);
        }
        // Pass 3, fields: from the shape and from the paths into it, each once, typed.
        for (port, on_port) in port_runs(&entries) {
            if let Some(fields) = &mut ports[port.index()] {
                self.port_fields(port, fields, on_port, of);
            }
        }
        // Pass 4, `temp`.
        let temp = self.setup_temp(&entries, envs);
        // Pass 5, complete: every port with a role is set, with its source's voltage,
        // and so is `temp` (v5 rule 1.3.1), all listed in one error. Not when an entry
        // didn't parse: its syntax error is the one error (model.md E23), as a part's
        // broken binding's is, and what's left out holds its proof.
        let missing = left_out(of, &ports, temp.is_some());
        let incomplete = unreadable.or_else(|| {
            (!missing.is_empty()).then(|| {
                let kind = ResolveErrorKind::IncompleteSetup {
                    setup: decl.name.text.to_string(),
                    missing,
                };
                self.report(kind, decl.name.span)
            })
        });
        build_setup(decl, of, ports, temp, incomplete)
    }

    /// Reports each attribute: none is read on a setup or its entries yet.
    fn no_attributes(&mut self, attrs: &[Attribute]) {
        for attr in attrs {
            let what = "attributes on a setup";
            self.report(ResolveErrorKind::Unsupported { what }, attr.span);
        }
    }

    // --- Pass 1: keys ---------------------------------------------------------------

    /// What `key` sets: a port's shape, one field of it, `temp`, or nothing (`Unknown`).
    /// `temp` and `window` are keys, not ports. `None` if it's reported here: `window`, a
    /// key too deep, or a net or an instance of the circuit.
    fn key<'a>(&mut self, key: &'a Key<'src>, of: ForBlock) -> Option<Target<'a, 'src>> {
        let (first, rest) = key.segments.split_first()?;
        let unsupported = match (first.text, rest) {
            ("temp", []) => return Some(Target::Temp),
            ("window", []) => Some("`window`"),
            (_, [_, _, ..]) => Some("a key deeper than `port.field`"),
            _ => None,
        };
        if let Some(what) = unsupported {
            self.report(ResolveErrorKind::Unsupported { what }, key.span);
            return None;
        }
        let Some(port) = of.ports.get(first.text) else {
            return self.not_a_port(first, of).map(Target::Unknown);
        };
        match rest {
            [field] => Some(Target::Field(port, field)),
            _ => Some(Target::Shape(port)),
        }
    }

    /// A key that isn't one of the block's ports: a net or an instance of its circuit,
    /// reported (`None`), or `name`, which names nothing.
    fn not_a_port<'a>(&mut self, name: &'a Ident<'src>, of: ForBlock) -> Option<&'a Ident<'src>> {
        let stmts = of.circuit.map_or(&[][..], |body| &body.stmts);
        let is = stmts.iter().find_map(|stmt| match &stmt.kind {
            StmtKind::Net { name: n, .. } if n.text == name.text => Some(NameKind::Net),
            StmtKind::Let { name: n, .. } if n.text == name.text => Some(NameKind::Instance),
            _ => None,
        });
        let Some(is) = is else {
            return Some(name);
        };
        let kind = ResolveErrorKind::WrongNamespace {
            name: name.text.to_string(),
            is,
            expected: "a port",
        };
        self.report(kind, name.span);
        None
    }

    /// Reports each key that names nothing (`Unknown`). A setup's slots are its block's
    /// ports, then `temp`, so one misspelled key and one slot left out that it could be
    /// is one mistake (`vc` for `vcc`): reported once, with the rename, and its entry
    /// then sets the slot, so the passes below check it as the slot (`vc: Supply {}`
    /// still lacks its `v`) and its fix uncovers nothing. Only its first entry: the fix
    /// renames only the first, since renaming both would give the slot twice. Nothing is
    /// renamed unless what's left out is `known`.
    fn unknown_keys(&mut self, entries: &mut [Entry<'_, 'src>], of: ForBlock, known: bool) {
        let is_unknown = |e: &Entry| matches!(e.target, Target::Unknown(_));
        // Names are built only when a key names nothing: a clean setup allocates none.
        let Some(first) = entries.iter().position(is_unknown) else {
            return;
        };
        let unknown: Vec<Entry> = entries.iter().copied().filter(is_unknown).collect();
        let temp = of.block.ports.len();
        let mut given: Vec<Option<Span>> = vec![None; temp + 1];
        for entry in entries.iter() {
            let slot = match entry.target {
                Target::Shape(port) | Target::Field(port, _) => port.index(),
                Target::Temp => temp,
                Target::Unknown(_) => continue,
            };
            given[slot].get_or_insert(entry.span);
        }
        let takes = |slot: usize| slot == temp || of.takes_shape(slot);
        let missing = (0..=temp).filter(|&s| known && given[s].is_none() && takes(s));
        let missing: Vec<usize> = missing.collect();
        let mut names: Vec<String> = of.block.port_names().map(str::to_string).collect();
        names.push("temp".to_string());
        let slots = UnboundSlots {
            names: &names,
            given: &given,
            missing: &missing,
        };
        let givens: Vec<Given> = unknown.iter().map(Entry::given).collect();
        // A key could be a port with a role, or `temp`, which has no fields and isn't a
        // shape: `tmp.v: …` and `tmp: Supply { … }` aren't `temp` misspelled.
        let fits = |u: usize, slot: usize| {
            if slot != temp {
                return of.takes_shape(slot);
            }
            let entry = unknown[u];
            let shape = matches!(unparen(entry.value).kind, ExprKind::StructLit(_));
            entry.key.segments.len() == 1 && !shape
        };
        let block = &of.block.name;
        let (_, renamed) =
            self.report_unknown(block, NameKind::Port, &givens, &slots, fits, |_| false);
        let Some(slot) = renamed else {
            return;
        };
        let key = entries[first].key;
        entries[first].target = match (slot == temp, &key.segments[1..]) {
            (true, _) => Target::Temp,
            (false, [field]) => Target::Field(PortId::new(slot), field),
            (false, _) => Target::Shape(PortId::new(slot)),
        };
    }

    // --- Pass 2: shapes and roles ---------------------------------------------------

    /// Port `port`'s shape, from `on_port`, the entries that set it (one at least): the
    /// one its role takes, written (`vcc: Supply { … }`, with its fields) or implied by a
    /// path into it (`vout.i: 5mA`). `None` when the port takes nothing: an entry on a
    /// ground or a `Pin` is reported, and one on a port whose type is wrong is skipped,
    /// since that was.
    fn port_shape<'a>(
        &mut self,
        port: PortId,
        on_port: &[Entry<'a, 'src>],
        of: ForBlock,
    ) -> Option<PortFields<'a, 'src>> {
        on_port.first()?;
        let name = of.block.port_name(port);
        let signal = of.block.ports[port.index()].signal.ok()?;
        let Some(takes) = Shape::for_role(signal) else {
            for entry in on_port {
                self.no_role(name, signal, entry.key.span);
            }
            return None;
        };
        // Whatever shape is written, its fields are bound to the one the port takes, so
        // fixing a wrong shape's name uncovers nothing.
        let mut fields = PortFields::new(signal, takes);
        let mut shape_given = None;
        for entry in on_port {
            let Target::Shape(_) = entry.target else {
                continue;
            };
            let lit = self.shape_written(name, signal, takes, entry.value);
            if self.given_once(&mut shape_given, entry.given()) {
                fields.written = Some(entry.span);
                fields.lit = lit;
            } else {
                // A second shape (the second-definition rule): bound as the first is,
                // for its own mistakes, then dropped. What it leaves out isn't listed:
                // the setup is the first's.
                let mut second = PortFields::new(signal, takes);
                second.lit = lit;
                self.port_fields(port, &mut second, &[], of);
            }
        }
        Some(fields)
    }

    /// Reports an entry on a port that takes nothing: a ground, or a `Pin`, which has
    /// no role.
    fn no_role(&mut self, port: &str, signal: SignalType, at: Span) {
        let kind = match signal {
            SignalType::Ground => ResolveErrorKind::WrongRole {
                port: port.to_string(),
                signal,
                wrong: RoleMismatch::Ground,
            },
            _ => ResolveErrorKind::Unsupported {
                what: "a setup entry on a `Pin` port, which has no role",
            },
        };
        self.report(kind, at);
    }

    /// The fields of the shape `e` writes for `port` (`Supply { v: 12V }`), whose role
    /// takes `takes`. Reports a shape for the other direction (fixed to `takes`) and a
    /// name that's no shape; the fields of either are bound to `takes`. `Err` if `e`
    /// isn't a shape at all (`12V`, `Resistor { … }`), reported.
    fn shape_written<'a>(
        &mut self,
        port: &str,
        signal: SignalType,
        takes: Shape,
        e: &'a Expr<'src>,
    ) -> Result<&'a [Field<'src>], Reported> {
        let e = unparen(e);
        let ExprKind::StructLit(lit) = &e.kind else {
            if let ExprKind::Error(reported) = e.kind {
                return Err(reported);
            }
            let kind = ResolveErrorKind::NotAShape {
                port: port.to_string(),
                takes,
            };
            return Err(self.report(kind, e.span));
        };
        if let (Some(first), Some(last)) = (lit.generics.first(), lit.generics.last()) {
            let what = "generic arguments";
            let at = Span::new(first.span.start, last.span.end);
            self.report(ResolveErrorKind::Unsupported { what }, at);
        }
        let name = self.path_text(&lit.path);
        match Shape::from_name(name) {
            Some(shape) if shape == takes => {}
            Some(shape) => {
                let kind = ResolveErrorKind::WrongRole {
                    port: port.to_string(),
                    signal,
                    wrong: RoleMismatch::Shape(shape),
                };
                let error =
                    ResolveError::new(kind, lit.path.span).with_fix(lit.path.span, takes.name());
                error.report(&mut self.errors);
            }
            None => self.not_a_shape(name, takes, lit.path.span)?,
        }
        Ok(&lit.fields)
    }

    /// Reports `name`, written where a shape goes, which isn't one: a step or a sweep,
    /// a block, a part kind or a signal type (`Err`: it's something else), or nothing,
    /// a misspelled shape. Only `takes` is suggested: it's the one shape that fits.
    fn not_a_shape(&mut self, name: &str, takes: Shape, at: Span) -> Result<(), Reported> {
        if STIMULI.contains(&name) {
            return Err(self.no_stimulus(at));
        }
        if let Some(is) = self.kind_of(name) {
            let kind = ResolveErrorKind::WrongNamespace {
                name: name.to_string(),
                is,
                expected: "a shape",
            };
            return Err(self.report(kind, at));
        }
        let suggestion = suggest(&mut self.suggestions_left, name, [takes.name()]);
        let error = unknown_name(name, Namespace::Shape, suggestion.clone(), suggestion, at);
        error.report(&mut self.errors);
        Ok(())
    }

    // --- Pass 3: fields -------------------------------------------------------------

    /// Port `port`'s fields: from its written shape's literal, then from each path into
    /// it among `on_port`, its entries, each given once. Reports a field the shape
    /// doesn't have; one it must have and doesn't is kept in `missing` (`v`), unless an
    /// unknown field is its misspelling.
    fn port_fields<'a>(
        &mut self,
        port: PortId,
        fields: &mut PortFields<'a, 'src>,
        on_port: &[Entry<'a, 'src>],
        of: ForBlock,
    ) {
        let name = of.block.port_name(port);
        let mut unknown = Vec::new();
        let lit = fields.lit.unwrap_or(&[]);
        for field in lit {
            self.bind_shape_field(fields, Given::from(field), name, &mut unknown);
        }
        for entry in on_port {
            if let Target::Field(..) = entry.target {
                self.bind_shape_field(fields, entry.given(), name, &mut unknown);
            }
        }
        let schemas = fields.shape.fields();
        let required =
            (0..schemas.len()).filter(|&i| schemas[i].required && fields.given[i].is_none());
        let required: Vec<usize> = required.collect();
        // Names are built only when something is unknown or left out.
        if unknown.is_empty() && required.is_empty() {
            return;
        }
        let names: Vec<String> = schemas.iter().map(|f| f.name.to_string()).collect();
        let slots = UnboundSlots {
            names: &names,
            given: &fields.given,
            missing: &required,
        };
        let shape = fields.shape.name();
        let (reported, renamed) = self.report_unknown(
            shape,
            NameKind::Field,
            &unknown,
            &slots,
            |_, _| true,
            |_| false,
        );
        // A field left out is missing, unless it's explained: misspelled (`vv` for `v`),
        // or the port's shape wasn't written as one (`vcc: 12V`), each reported.
        for &i in &required {
            let explained = if renamed == Some(i) {
                reported
            } else {
                fields.lit.err()
            };
            match explained {
                Some(reported) => fields.values[i] = FieldValue::Invalid(reported),
                None => fields.missing.push(schemas[i].name),
            }
        }
    }

    /// Binds `given` to its slot of `fields`' shape, typed by the setup's rules, unless
    /// the slot was given already. A source's field on a load (`output.v`) is what the
    /// block drives, reported; another field the shape doesn't have goes to `unknown`.
    fn bind_shape_field<'a>(
        &mut self,
        fields: &mut PortFields,
        given: Given<'a, 'src>,
        port: &str,
        unknown: &mut Vec<Given<'a, 'src>>,
    ) {
        let Some((i, schema)) = fields.shape.field(given.name.text) else {
            let driven = match fields.shape {
                Shape::Load => Shape::Supply.field(given.name.text),
                Shape::Supply | Shape::Signal => None,
            };
            match driven {
                Some((_, schema)) => {
                    let kind = ResolveErrorKind::WrongRole {
                        port: port.to_string(),
                        signal: fields.signal,
                        wrong: RoleMismatch::Drives(schema.name),
                    };
                    self.report(kind, given.span);
                }
                None => unknown.push(given),
            }
            return;
        };
        // Checked even when it's the second binding of the slot, for its own mistakes.
        let e = self.written_value(given);
        let value = e.and_then(|e| self.setup_value(e, schema.ty, given.name.text));
        let value = FieldValue::from(value);
        if fields.given[i].is_none() {
            fields.values[i] = value;
        }
        self.given_once(&mut fields.given[i], given);
    }

    /// A setup field's value, or `temp`'s, typed as `ty` the way a part's is, with the
    /// setup's own rules (plan §2.9 b): `..=b` starts at 0 on a field that can't be
    /// negative (`z: ..=0.5Ω`) and needs a lower end on one that can (`v`, `i`); `a..`
    /// and a step or a sweep aren't supported yet.
    fn setup_value(&mut self, e: &Expr, ty: FieldType, field: &str) -> Result<Value, Reported> {
        match &unparen(e).kind {
            ExprKind::Range {
                lo: None,
                hi: Some(hi),
            } if starts_at_zero(ty.dim) => {
                let exact = FieldType {
                    spread_allowed: false,
                    ..ty
                };
                let hi = self.value(hi, exact, field)?.nominal;
                if hi.si < 0.0 {
                    return Err(self.report(ResolveErrorKind::RangeReversed, e.span));
                }
                Ok(Value::range(Quantity { si: 0.0, ..hi }, hi))
            }
            ExprKind::Range {
                lo: None,
                hi: Some(_),
            } => {
                let kind = ResolveErrorKind::OpenRange {
                    field: field.to_string(),
                };
                Err(self.report(kind, e.span))
            }
            ExprKind::Range {
                lo: Some(_),
                hi: None,
            } => {
                let what = "a range with no upper end (`a..`)";
                Err(self.report(ResolveErrorKind::Unsupported { what }, e.span))
            }
            ExprKind::StructLit(lit) if STIMULI.contains(&self.path_text(&lit.path)) => {
                Err(self.no_stimulus(lit.path.span))
            }
            _ => self.value(e, ty, field),
        }
    }

    /// Reports a step or a sweep, written where a shape or a field's value goes: not
    /// supported yet.
    fn no_stimulus(&mut self, at: Span) -> Reported {
        let what = "`Step` and `Sweep`";
        self.report(ResolveErrorKind::Unsupported { what }, at)
    }

    // --- Pass 4: `temp` -------------------------------------------------------------

    /// The setup's temperature, with its entry's span; `None` if it isn't written. An
    /// env (`temp: ambient`, the one place an env's name is a value), or a value typed
    /// as a temperature: a const's, a point or a range. A second `temp` is checked for
    /// its own mistakes, then reported as given twice.
    fn setup_temp(
        &mut self,
        entries: &[Entry],
        envs: &[Env],
    ) -> Option<(Result<Temp, Reported>, Span)> {
        let mut temp = None;
        let mut given = None;
        for entry in entries {
            let Target::Temp = entry.target else {
                continue;
            };
            let value = self.temp_value(entry.value, envs);
            if self.given_once(&mut given, entry.given()) {
                temp = Some((value, entry.span));
            }
        }
        temp
    }

    /// What `temp: e` holds: the env `e` names, or else `e` typed as a temperature.
    fn temp_value(&mut self, e: &Expr, envs: &[Env]) -> Result<Temp, Reported> {
        if let ExprKind::Path(path) = &unparen(e).kind
            && let Some(FileValue::Env(id)) = self.values.get(self.path_text(path))
        {
            // A broken env was reported at its definition: its proof is the setup's.
            return envs[id.index()].value.map(|_| Temp::Env(id));
        }
        self.setup_value(e, FieldType::temperature(), "temp")
            .map(Temp::Value)
    }
}

/// What the setup leaves out, in port order (`vcc.v`, `input`, then `temp`): each port
/// with a role that nothing sets, each field a port's shape must have and doesn't, and
/// `temp`.
fn left_out(of: ForBlock, ports: &[Option<PortFields>], temp_written: bool) -> Vec<String> {
    let mut list = Vec::new();
    for (p, port) in ports.iter().enumerate() {
        let name = of.block.port_name(PortId::new(p));
        match port {
            Some(port) => {
                for field in &port.missing {
                    list.push(format!("{name}.{field}"));
                }
            }
            None if of.takes_shape(p) => list.push(name.to_string()),
            None => {}
        }
    }
    if !temp_written {
        list.push("temp".to_string());
    }
    list
}

/// Whether a field of dimension `dim` can't be negative, so `..=b` starts at 0: a
/// resistance or an impedance, a capacitance ([R2] §3.1).
fn starts_at_zero(dim: Dimension) -> bool {
    dim == Dimension::OHM || dim == Dimension::FARAD
}

/// The setup and its spans, from what the passes built. A field or a `temp` left out
/// that must be written holds `incomplete`, the proof it was reported (left out, or a
/// misspelled key's rename); a placeholder taints the setup, wherever its error was
/// reported (a broken const or env).
fn build_setup(
    decl: &SetupDecl,
    of: ForBlock,
    ports: Vec<Option<PortFields>>,
    temp: Option<(Result<Temp, Reported>, Span)>,
    incomplete: Option<Reported>,
) -> (Setup, SetupSpans) {
    let left_out = || incomplete.expect("a field that must be written and wasn't is reported");
    let mut tainted = None;
    let (ports, port_spans) = ports
        .into_iter()
        .map(|port| {
            let Some(port) = port else {
                return (None, None);
            };
            // A field that must be written and is still unset was listed as left out.
            let mut values = port.values;
            left_out_required(&mut values, port.shape.fields(), left_out);
            for value in &values {
                if let FieldValue::Invalid(reported) = value {
                    tainted.get_or_insert(*reported);
                }
            }
            let spans = PortSetupSpans {
                shape: port.written,
                fields: port.given,
            };
            let setup = PortSetup {
                shape: port.shape,
                fields: values,
            };
            (Some(setup), Some(spans))
        })
        .unzip();
    let (temp, temp_span) = match temp {
        Some((temp, at)) => (temp, Some(at)),
        None => (Err(left_out()), None),
    };
    if let Err(reported) = temp {
        tainted.get_or_insert(reported);
    }
    let setup = Setup {
        name: decl.name.text.to_string(),
        block: of.id,
        ports,
        temp,
        tainted,
    };
    let spans = SetupSpans {
        name: decl.name.span,
        ports: port_spans,
        temp: temp_span,
    };
    (setup, spans)
}

#[cfg(test)]
mod tests {
    use spicy_errors::DiagKind;
    use spicy_model::design::FieldValue;

    use crate::parser::parse;
    use crate::resolve::{NameKind, ResolveErrorKind, resolve};
    use crate::testing::Rng;

    /// A name written where a shape goes is looked up in the kind namespace's order
    /// (model.md E5), as a placement's and a port type's are: the file's block
    /// `Resistor` shadows the prelude's part kind, so it's "a block, not a shape".
    #[test]
    fn a_block_shadows_a_part_kind_where_a_shape_goes() {
        let src = "block Resistor { p: Pin }\n\nblock Ldo { vin: Power<In>, gnd: Ground }\n\n\
                   setup S for Ldo {\n    vin: Resistor { v: 5V },\n    temp: 25°C,\n}\n";
        let resolved = resolve(&parse(src));
        let kinds: Vec<_> = resolved.errors.iter().map(|e| &e.kind).collect();
        let is = |k: &&ResolveErrorKind| match k {
            ResolveErrorKind::WrongNamespace { is, .. } => Some(*is),
            _ => None,
        };
        assert_eq!(kinds.len(), 1, "{kinds:?}");
        assert_eq!(is(&kinds[0]), Some(NameKind::Block));
    }

    /// Entry order inside a setup doesn't change what it means (model.md E4): a path
    /// merges into its port's shape whether it's written before or after it.
    #[test]
    fn entry_order_does_not_matter() {
        let head = "env ambient: Temperature in -10°C..=60°C;\n\n\
                    block Ldo { vin: Power<In>, vout: Power<Out>, gnd: Ground }\n\n\
                    circuit Ldo {}\n\nsetup Operating for Ldo {\n";
        let entries = [
            "    vin: Supply { v: 4.3V..=5.5V },",
            "    vin.z: ..=0.5Ω,",
            "    vout: Load { c: ..=20uF },",
            "    vout.i: 5mA..=50mA,",
            "    temp: ambient,",
        ];
        let src = |entries: &[&str]| format!("{head}{}\n}}\n", entries.join("\n"));
        let reference = resolve(&parse(&src(&entries)));
        assert!(reference.errors.is_empty(), "{:#?}", reference.errors);
        let mut rng = Rng(0x5e7);
        for _ in 0..20 {
            let mut shuffled = entries;
            for i in (1..shuffled.len()).rev() {
                shuffled.swap(i, rng.below(i + 1));
            }
            let text = src(&shuffled);
            let resolved = resolve(&parse(&text));
            assert!(resolved.errors.is_empty(), "{text}");
            assert_eq!(resolved.design.setups, reference.design.setups, "{text}");
        }
    }

    /// Entries are grouped by port before passes 2 and 3, keeping source order within a
    /// port: of two shapes written for one port, the first is kept and the second is
    /// the duplicate, whatever is written between them.
    #[test]
    fn a_ports_first_shape_is_kept_across_other_entries() {
        let src = "block Amp { vcc: Power<In>, input: Analog<In> }\n\n\
                   circuit Amp {}\n\n\
                   setup S for Amp {\n    vcc: Supply { v: 12V },\n    input: Signal { v: 0V },\n    \
                   temp: 25°C,\n    vcc: Supply { v: 5V },\n}\n";
        let resolved = resolve(&parse(src));
        let [error] = &resolved.errors[..] else {
            panic!("{:#?}", resolved.errors);
        };
        let second = src.find("vcc: Supply { v: 5V }").unwrap();
        let first = src.find("vcc: Supply { v: 12V }").unwrap();
        assert_eq!(error.kind.name(), "Duplicate");
        assert_eq!(error.span.start as usize, second);
        assert_eq!(error.related.map(|at| at.start as usize), Some(first));
        let vcc = resolved.design.setups[0].ports[0].as_ref().unwrap();
        let FieldValue::Given(v) = vcc.fields[0] else {
            panic!("{:?}", vcc.fields[0]);
        };
        assert_eq!(v.nominal.si, 12.0);
    }

    /// A second shape for one port is checked for its own mistakes, as a second `let` is
    /// (the second-definition rule), then dropped: the setup keeps the first.
    #[test]
    fn a_second_shape_is_checked_then_dropped() {
        let src = "block B { vcc: Power<In> }\n\ncircuit B {}\n\nsetup S for B {\n    \
                   vcc: Supply { v: 12V },\n    vcc: Supply { vv: 1V, z: 1Ω, z: 2Ω },\n    \
                   temp: 25°C,\n}\n";
        let resolved = resolve(&parse(src));
        let names: Vec<_> = resolved.errors.iter().map(|e| e.kind.name()).collect();
        assert_eq!(names, ["Duplicate", "Duplicate", "UnknownField"]);
        let vcc = resolved.design.setups[0].ports[0].as_ref().unwrap();
        assert_eq!(vcc.fields[1], FieldValue::Unset);
    }

    fn errors(entries: &str) -> Vec<&'static str> {
        let src = format!(
            "env ambient: Temperature in -10°C..=60°C;\n\n\
             block Ldo {{ vin: Power<In>, vout: Power<Out>, gnd: Ground }}\n\n\
             circuit Ldo {{}}\n\nsetup S for Ldo {{ {entries} }}\n"
        );
        let resolved = resolve(&parse(&src));
        resolved.errors.iter().map(|e| e.kind.name()).collect()
    }

    /// An entry the parser couldn't read may be what the setup leaves out (`temp 25°C`
    /// sets `temp`): its syntax error is the one error (model.md E23), as it is for a
    /// part with a broken binding, and the setup is still tainted (red team: it was also
    /// `IncompleteSetup`). A key that names nothing gets no rename either: the slot it
    /// would be renamed to may be the broken entry's.
    #[test]
    fn a_broken_entry_is_its_only_error() {
        let vout = "vout: Load {}";
        for entries in [
            format!("vin.v: 5V, {vout}, temp 25°C"),
            format!("vin Supply {{ v: 5V }}, {vout}, temp: 25°C"),
        ] {
            assert_eq!(errors(&entries), [] as [&str; 0], "{entries}");
        }
        let src = format!(
            "block Ldo {{ vin: Power<In>, vout: Power<Out> }}\n\n\
             setup S for Ldo {{ vin Supply {{ v: 5V }}, {vout}, temp: 25°C, x: 1 }}\n"
        );
        let parsed = parse(&src);
        assert_eq!(parsed.errors.len(), 1);
        let resolved = resolve(&parsed);
        let fixes: Vec<_> = resolved.errors.iter().map(|e| e.fix.is_some()).collect();
        assert_eq!(fixes, [false], "{:#?}", resolved.errors);
        assert!(resolved.design.setups[0].tainted.is_some());
    }

    /// A wrong shape's label says what it's for: a load is the other direction's, but a
    /// `Supply` on an `Analog<In>` is a source too, a `Power<In>`'s (red team: it was
    /// "the other direction's shape").
    #[test]
    fn a_wrong_shape_says_what_it_is_for() {
        let src = "block Amp { vcc: Power<In>, input: Analog<In>, output: Analog<Out> }\n\n\
                   circuit Amp {}\n\nsetup S for Amp {\n    vcc: Signal { v: 12V },\n    \
                   input: Supply { v: 0V },\n    output: Supply { },\n    temp: 25°C,\n}\n";
        let resolved = resolve(&parse(src));
        let labels: Vec<String> = resolved
            .errors
            .iter()
            .map(|e| e.kind.text(e.fix.as_ref()).1)
            .collect();
        assert_eq!(
            labels,
            [
                "an `Analog<In>`'s shape",
                "a `Power<In>`'s shape",
                "the other direction's shape"
            ]
        );
    }

    /// Parentheses change nothing (red team: `temp: (ambient)` was "an env's name
    /// anywhere but a setup's `temp:`", and `vin: (Supply { … })` "not a shape").
    #[test]
    fn parentheses_change_nothing() {
        let vout = "vout: Load {}";
        assert_eq!(
            errors(&format!("vin.v: 5V, {vout}, temp: (ambient)")),
            [] as [&str; 0]
        );
        assert_eq!(
            errors(&format!("vin: (Supply {{ v: 5V }}), {vout}, temp: 25°C")),
            [] as [&str; 0]
        );
    }

    /// A block with thousands of ports and as many misspelled keys stays linear: each
    /// error's note lists the first ten ports, then how many more (a first version
    /// copied every port into every error: 16 s for 20 000 keys on 20 000 ports).
    #[test]
    fn unknown_keys_on_a_wide_block_stay_linear() {
        use std::fmt::Write;
        let n = 4000;
        let mut src = String::from("block B { ");
        for i in 0..n {
            write!(src, "p{i:05}: Power<In>, ").unwrap();
        }
        src.push_str("}\n\ncircuit B {}\n\nsetup S for B {\n");
        for i in 0..n {
            writeln!(src, "    p{i:05}.v: 1V,").unwrap();
            writeln!(src, "    q{i:05}: Supply {{ v: 1V }},").unwrap();
        }
        src.push_str("    temp: 25°C,\n}\n");
        let parsed = parse(&src);
        let start = std::time::Instant::now();
        let resolved = resolve(&parsed);
        let elapsed = start.elapsed();
        assert_eq!(resolved.errors.len(), n);
        let ResolveErrorKind::UnknownField {
            valid, unlisted, ..
        } = &resolved.errors[0].kind
        else {
            panic!("{:?}", resolved.errors[0]);
        };
        // The block's ports, then `temp`.
        assert_eq!((valid.len(), *unlisted), (10, n + 1 - 10));
        assert!(elapsed.as_secs_f64() < 2.0, "took {elapsed:?}");
    }

    /// The note of an unknown key lists the first ten slots, then how many more.
    #[test]
    fn a_long_slot_list_is_cut_short() {
        let ports: Vec<String> = (0..12).map(|i| format!("p{i:02}: Power<In>")).collect();
        let paths: Vec<String> = (0..12).map(|i| format!("p{i:02}.v: 1V")).collect();
        let src = format!(
            "block B {{ {} }}\n\ncircuit B {{}}\n\nsetup S for B {{ {}, x: 1V, temp: 25°C }}\n",
            ports.join(", "),
            paths.join(", ")
        );
        let resolved = resolve(&parse(&src));
        let [error] = &resolved.errors[..] else {
            panic!("{:#?}", resolved.errors);
        };
        let notes = error.kind.text(None).2;
        let expected = "note: `B` has `p00`, `p01`, `p02`, `p03`, `p04`, `p05`, `p06`, `p07`, \
                        `p08`, `p09`, … and 3 more";
        assert_eq!(notes.last().map(String::as_str), Some(expected));
    }
}
