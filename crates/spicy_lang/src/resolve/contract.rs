//! Pass 3 for each block's contract (contracts_plan.md step 3): its default setup and
//! its measures, against the block as its circuit left it, as passes in
//! [`contract`](Resolver::contract):
//! 1. **The default setup:** `setup = S;`, among the block's setups.
//! 2. **Names:** every `let`, declared in the block's value namespace with its ports,
//!    nets and instances (model.md E5): `let base = …` next to `net base;` is a
//!    `Duplicate`.
//! 3. **Measures:** each `let` typed against the measure table
//!    (`spicy_model::measure`), on demand, so a measure can use one written after it.
//!    The lets are taken in name order, so a cycle is reported once, and the same way
//!    whatever the statement order. Typing a measure types each one it reads inside it:
//!    a chain of about 1,000 lets overflows a 2 MB stack (parked, contracts_plan.md §3).
//!
//! Specs, which read the measures, arrive in step 4.

use spicy_errors::Reported;
use spicy_index::fx::FxHashMap;
use spicy_model::design::{
    Block, BlockSpans, Contract, ContractSpans, InstanceId, InstanceOf, Measure, MeasureId, NetId,
    PortId, SetupId,
};
use spicy_model::measure::{ArithOp, MExpr, MeasureType, Method, Reference};
use spicy_model::prelude::{FieldType, Role, SignalType};
use spicy_model::units::{Dimension, QKind};
use spicy_span::Span;

use super::error::describe;
use super::value::{literal_quantity, unparen};
use super::{
    FileValue, NameKind, Namespace, Redefined, ResolveError, ResolveErrorKind, Resolver, Scope,
    suggest, unknown_name,
};
use crate::parser::ast::{self, Arg, BinOp, Body, Expr, ExprKind, Ident, StmtKind};

/// v5's other analyses, which are "not supported yet" rather than unknown.
const LATER_FUNCTIONS: &[&str] = &["tran", "noise"];

/// v5's other methods.
const LATER_METHODS: &[&str] = &[
    "min",
    "max",
    "settle",
    "crossing",
    "deviation",
    "span",
    "during",
    "after",
    "phase",
];

/// The block a contract describes: its nets and instances as its circuit left them,
/// their spans, and its setups by name.
#[derive(Clone, Copy)]
pub(super) struct ForContract<'a, 'src> {
    pub block: &'a Block,
    pub spans: &'a BlockSpans,
    pub setups: &'a Scope<'src, SetupId>,
}

/// What a name in a contract's value namespace is (model.md E5).
#[derive(Clone, Copy)]
enum ContractValue {
    /// A net of the block; a port's net is its port.
    Net(NetId),
    Instance(InstanceId),
    Measure(MeasureId),
}

/// A contract's first `let` of a name, a measure, with the proof it was defined again
/// if it was: which one was meant isn't known, so this one is broken too.
#[derive(Clone, Copy)]
struct Let<'a, 'src> {
    name: &'a Ident<'src>,
    value: &'a Expr<'src>,
    again: Option<Reported>,
}

/// Where the typing of a measure is: the three states of a depth-first search, as
/// flatten's `Visit` for placements.
enum Visit {
    New,
    /// Being typed: meeting it again closes a cycle.
    OnPath,
    Done(Result<(MExpr, MeasureType), Reported>),
}

impl<'p, 'src> Resolver<'p, 'src> {
    /// The contract `body` of the block `of`, with where each part of it was written.
    pub(super) fn contract(
        &mut self,
        body: &Body<'src>,
        of: ForContract<'_, 'src>,
    ) -> (Contract, ContractSpans) {
        // Pass 1, the default setup.
        let (default_setup, default_span) = self.default_setup(body, of);
        // Pass 2, names: the block's, then each `let`'s. A second `let` of a name is
        // typed for its own mistakes, then dropped; a first one of a measure's name is
        // broken too, as a const's is (see `Redefined`).
        let mut names = self.block_names(of, body);
        let (mut lets, mut seconds) = (Vec::new(), Vec::new());
        for stmt in &body.stmts {
            let StmtKind::Let { name, value } = &stmt.kind else {
                continue;
            };
            let id = ContractValue::Measure(MeasureId::new(lets.len()));
            match names.declare(name, id, NameKind::Measure, &mut self.errors) {
                Ok(()) => lets.push(Let {
                    name,
                    value,
                    again: None,
                }),
                Err(Redefined { first, reported }) => {
                    seconds.push((name, value));
                    if let ContractValue::Measure(first) = first {
                        lets[first.index()].again.get_or_insert(reported);
                    }
                }
            }
        }
        // Pass 3, measures: each first `let`, on demand, then each second one.
        let visits = (0..lets.len()).map(|_| Visit::New).collect();
        let resolver = MeasureResolver {
            r: self,
            of,
            names: &names,
            lets: &lets,
            visits,
            path: Vec::new(),
        };
        let measures = resolver.resolve(&seconds);
        let spans = ContractSpans {
            name: body.name.span,
            default_setup: default_span,
            measures: lets.iter().map(|l| l.name.span).collect(),
        };
        let contract = Contract {
            default_setup,
            measures,
            tainted: body.broken,
        };
        (contract, spans)
    }

    // --- Pass 1: the default setup ----------------------------------------------------

    /// `setup = S;`: one of the block's setups, with the statement's span. A second one
    /// is reported; none is `NoDefaultSetup`, unless the body didn't parse (that line may
    /// be the broken one).
    fn default_setup(
        &mut self,
        body: &Body<'src>,
        of: ForContract<'_, 'src>,
    ) -> (Result<SetupId, Reported>, Option<Span>) {
        let mut found: Option<(Result<SetupId, Reported>, Span)> = None;
        for stmt in &body.stmts {
            let StmtKind::DefaultSetup { setup } = &stmt.kind else {
                continue;
            };
            let id = self.setup_named(setup, of);
            match &mut found {
                None => found = Some((id, stmt.span)),
                Some((first_id, first)) => {
                    let kind = ResolveErrorKind::Duplicate {
                        name: "setup".to_string(),
                        what: NameKind::DefaultSetup,
                    };
                    let error = ResolveError::new(kind, stmt.span).with_related(*first);
                    let reported = error.report(&mut self.errors);
                    *first_id = first_id.and(Err(reported));
                }
            }
        }
        match found {
            Some((id, at)) => (id, Some(at)),
            None => {
                let missing = body.broken.unwrap_or_else(|| {
                    let kind = ResolveErrorKind::NoDefaultSetup {
                        contract: body.name.text.to_string(),
                        setups: of.setups.names_in_order(),
                    };
                    self.report(kind, body.name.span)
                });
                (Err(missing), None)
            }
        }
    }

    /// The setup `path` names among the block's, or an `UnknownName` with the one
    /// probably meant.
    fn setup_named(
        &mut self,
        path: &ast::Path,
        of: ForContract<'_, 'src>,
    ) -> Result<SetupId, Reported> {
        let name = self.path_text(path);
        if let Some(id) = of.setups.get(name) {
            return Ok(id);
        }
        let setups = of.setups.iter().map(|(setup, _)| setup);
        Err(self.report_unknown_name(name, Namespace::Setup, setups, path.span))
    }

    // --- Pass 2: names ----------------------------------------------------------------

    /// The block's value namespace as its circuit left it: its nets (a port's net first)
    /// and its instances, by the names written in the source. Sized once for them and
    /// every `let` of the contract `body`, so it never rehashes (as `BodyResolver::new`).
    fn block_names(&self, of: ForContract<'_, 'src>, body: &Body) -> Scope<'src, ContractValue> {
        let src = self.src();
        let lets = body.stmts.iter();
        let lets = lets.filter(|stmt| matches!(stmt.kind, StmtKind::Let { .. }));
        let count = of.spans.nets.len() + of.spans.instances.len() + lets.count();
        let mut declared = FxHashMap::with_capacity_and_hasher(count, Default::default());
        let nets = of.spans.nets.iter().enumerate();
        let nets = nets.map(|(n, &at)| (at, ContractValue::Net(NetId::new(n))));
        let instances = of.spans.instances.iter().enumerate();
        let instances =
            instances.map(|(i, s)| (s.name, ContractValue::Instance(InstanceId::new(i))));
        for (at, name) in nets.chain(instances) {
            declared.insert(&src[at.range()], (name, at));
        }
        Scope { declared }
    }
}

/// Types one contract's measures, each on demand.
struct MeasureResolver<'r, 'p, 'src, 'a> {
    r: &'r mut Resolver<'p, 'src>,
    of: ForContract<'a, 'src>,
    names: &'a Scope<'src, ContractValue>,
    /// Each first `let` of a name, by `MeasureId`.
    lets: &'a [Let<'a, 'src>],
    /// Each measure's typing, by `MeasureId`.
    visits: Vec<Visit>,
    /// The measures being typed, each reading the next: a cycle's members.
    path: Vec<MeasureId>,
}

impl<'src> MeasureResolver<'_, '_, 'src, '_> {
    // --- Pass 3: measures -------------------------------------------------------------

    /// Every measure, typed: each first `let` in name order, so a cycle is reported
    /// once, and the same way whatever the statement order; then each second one,
    /// `seconds`, for its own mistakes.
    fn resolve(mut self, seconds: &[(&Ident<'src>, &Expr<'src>)]) -> Vec<Measure> {
        let mut order: Vec<MeasureId> = (0..self.lets.len()).map(MeasureId::new).collect();
        order.sort_by_key(|id| self.lets[id.index()].name.text);
        for id in order {
            let _ = self.measure(id);
        }
        for &(name, value) in seconds {
            let _ = self.let_value(name, value);
        }
        let typed = self.lets.iter().zip(self.visits);
        typed
            .map(|(l, slot)| Measure {
                name: l.name.text.to_string(),
                value: match slot {
                    Visit::Done(value) => value,
                    Visit::New | Visit::OnPath => unreachable!("every measure is typed"),
                },
            })
            .collect()
    }

    /// Measure `id`'s type, typed if it isn't yet; broken if it was defined again.
    /// Meeting a measure that is being typed is a cycle, reported where it's met.
    fn measure(&mut self, id: MeasureId) -> Result<MeasureType, Reported> {
        let i = id.index();
        if let Visit::New = self.visits[i] {
            self.visits[i] = Visit::OnPath;
            self.path.push(id);
            let Let { name, value, again } = self.lets[i];
            let typed = self.let_value(name, value);
            self.path.pop();
            self.visits[i] = Visit::Done(again.map_or(typed, Err));
        }
        match &self.visits[i] {
            Visit::Done(value) => value.as_ref().map(|&(_, ty)| ty).map_err(|&r| r),
            Visit::New | Visit::OnPath => unreachable!("a cycle is caught where it's met"),
        }
    }

    /// A `let`'s value, which must measure something: a part or a constant doesn't, and
    /// a probe needs an analysis (fixed to `dc(…)`).
    fn let_value(
        &mut self,
        name: &Ident,
        e: &Expr<'src>,
    ) -> Result<(MExpr, MeasureType), Reported> {
        if let ExprKind::StructLit(_) = unparen(e).kind {
            return Err(self.not_a_measure(name, e, "a part or a block"));
        }
        // What the parser built around a syntax error is a guess (as in `value`).
        if let Some(reported) = self.r.syntax.inside(e.span) {
            return Err(reported);
        }
        let (expr, ty) = self.expr(e)?;
        if let MeasureType::Probe(_) = ty {
            let mut error = ResolveError::new(ResolveErrorKind::NeedsAnalysis, e.span);
            // `output.v / input.v` is a gain for `ac(…)` as much as a ratio for `dc(…)`.
            let gain = match &expr {
                MExpr::Binary {
                    op: ArithOp::Div,
                    lhs,
                    rhs,
                } => matches!(self.excited(lhs, rhs), Ok(Some(_))),
                _ => false,
            };
            if !gain {
                let written = &self.r.src()[e.span.range()];
                error = error.with_fix(e.span, format!("dc({written})"));
            }
            return Err(error.report(&mut self.r.errors));
        }
        if !reads_the_design(&expr) {
            return Err(self.not_a_measure(name, e, "a constant"));
        }
        Ok((expr, ty))
    }

    fn not_a_measure(&mut self, name: &Ident, e: &Expr, is: &'static str) -> Reported {
        let kind = ResolveErrorKind::NotAMeasure {
            name: name.text.to_string(),
            is,
        };
        self.r.report(kind, e.span)
    }

    /// What `e` computes, and what that is.
    fn expr(&mut self, e: &Expr<'src>) -> Result<(MExpr, MeasureType), Reported> {
        match &e.kind {
            ExprKind::Quantity(lit) => {
                let q = literal_quantity(*lit);
                Ok((MExpr::Const(q), number(q.dim, q.kind)))
            }
            ExprKind::Paren(inner) => self.expr(inner),
            ExprKind::Path(path) => self.named(path, e.span),
            ExprKind::Field { base, name } => self.probe(base, name),
            ExprKind::Call { callee, args } => self.call(callee, args, e.span),
            ExprKind::Neg(inner) => {
                let inner = self.expr(inner)?;
                self.neg(inner, e.span)
            }
            ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs, e.span),
            ExprKind::Index { .. } => Err(self.r.unsupported("`m[s]`", e.span)),
            ExprKind::Error(reported) => Err(*reported),
            _ => Err(self.r.report(ResolveErrorKind::NotAValue, e.span)),
        }
    }

    /// A name where a measure goes: another measure, or a const's value. A net, a port
    /// or an instance is no number: it's probed (`output.v`).
    fn named(&mut self, path: &ast::Path, at: Span) -> Result<(MExpr, MeasureType), Reported> {
        let name = self.r.path_text(path);
        match self.names.get(name) {
            Some(ContractValue::Measure(id)) => {
                if let Visit::OnPath = self.visits[id.index()] {
                    return Err(self.cycle(id, at));
                }
                let ty = self.measure(id)?;
                return Ok((MExpr::Measure(id), ty));
            }
            Some(value @ (ContractValue::Net(_) | ContractValue::Instance(_))) => {
                let kind = ResolveErrorKind::WrongNamespace {
                    name: name.to_string(),
                    is: self.name_kind(value),
                    expected: "a measure",
                };
                return Err(self.r.report(kind, at));
            }
            None => {}
        }
        let Some(value) = self.r.values.get(name) else {
            return Err(self.unknown_value(name, at));
        };
        let q = self.r.file_value(value, at)?;
        Ok((MExpr::Const(q), number(q.dim, q.kind)))
    }

    /// `name` where a measure goes, which names nothing. Suggested another measure or a
    /// const, as a note, not a fix: a net or an instance is no number (`output` for
    /// `dc(outpt)`), a measure being typed would be a cycle (`h2` for `let h2 = h3;`),
    /// and typing goes bottom up, so whether a measure's type fits here isn't known
    /// (`dc(e)` with the response `b` suggested would be `BadArguments`).
    fn unknown_value(&mut self, name: &str, at: Span) -> Reported {
        let measures = self.names.iter().filter(|&(_, value)| {
            matches!(value, ContractValue::Measure(id)
                if !matches!(self.visits[id.index()], Visit::OnPath))
        });
        let consts = self.r.values.iter();
        let consts = consts.filter(|(_, value)| matches!(value, FileValue::Const(_)));
        let candidates = measures.map(|(n, _)| n).chain(consts.map(|(n, _)| n));
        let suggestion = suggest(&mut self.r.suggestions_left, name, candidates);
        let error = unknown_name(name, Namespace::Measure, suggestion, None, at);
        error.report(&mut self.r.errors)
    }

    /// The cycle closed at `at` by reading `id`, which is being typed: `id`, then each
    /// measure it reads on the way back to it.
    fn cycle(&mut self, id: MeasureId, at: Span) -> Reported {
        let start = self.path.iter().position(|&m| m == id);
        let members = &self.path[start.expect("a measure being typed is on the path")..];
        let cycle = members.iter().map(|m| self.lets[m.index()].name.text);
        let kind = ResolveErrorKind::MeasureCycle {
            cycle: cycle.map(str::to_string).collect(),
        };
        self.r.report(kind, at)
    }

    /// `output.v`: a net's or a port's voltage, a probe. Only `.v` is in the MVP.
    fn probe(&mut self, base: &Expr<'src>, name: &Ident) -> Result<(MExpr, MeasureType), Reported> {
        let ExprKind::Path(path) = &unparen(base).kind else {
            return Err(self.r.report(ResolveErrorKind::NotAValue, name.span));
        };
        let of = self.r.path_text(path);
        let net = match self.names.get(of) {
            Some(ContractValue::Net(net)) => net,
            Some(ContractValue::Instance(i)) => {
                return Err(self.instance_probe(i, of, path.span, name));
            }
            Some(ContractValue::Measure(_)) => {
                return Err(self.not_a_net(of, Some(NameKind::Measure), path.span));
            }
            None => {
                let is = match self.r.values.get(of) {
                    Some(FileValue::Const(_)) => Some(NameKind::Const),
                    Some(FileValue::Env(_)) => Some(NameKind::Env),
                    None => None,
                };
                return Err(self.not_a_net(of, is, path.span));
            }
        };
        match name.text {
            "v" => Ok((MExpr::Voltage(net), MeasureType::Probe(Dimension::VOLT))),
            "i" | "z" => Err(self.r.unsupported("the probes `.i` and `.z(f)`", name.span)),
            other => Err(self
                .r
                .report_unknown_name(other, Namespace::Probe, ["v"], name.span)),
        }
    }

    /// A probe on an instance: a part's pin (`q1.c`) isn't in the MVP; anything else
    /// on an instance isn't a net.
    fn instance_probe(&mut self, i: InstanceId, of: &str, at: Span, name: &Ident) -> Reported {
        let instance = &self.of.block.instances[i.index()];
        if let InstanceOf::Part(kind) = instance.of
            && kind.pins().contains(&name.text)
        {
            let pin = Span::new(at.start, name.span.end);
            return self.r.unsupported("part pins (`q1.c`)", pin);
        }
        self.not_a_net(of, Some(NameKind::Instance), at)
    }

    /// `name` before a probe, which isn't a net: something else (`is`), or nothing. An
    /// unknown name is suggested only a net, as a pin's is (`BodyResolver::net_named`):
    /// suggesting a measure would trade this error for another.
    fn not_a_net(&mut self, name: &str, is: Option<NameKind>, at: Span) -> Reported {
        if let Some(is) = is {
            let kind = ResolveErrorKind::WrongNamespace {
                name: name.to_string(),
                is,
                expected: "a net or a port",
            };
            return self.r.report(kind, at);
        }
        let nets = self.names.iter();
        let nets = nets.filter(|(_, v)| matches!(v, ContractValue::Net(_)));
        let nets = nets.map(|(n, _)| n);
        self.r.report_unknown_name(name, Namespace::Value, nets, at)
    }

    /// What kind of name `value` is, for a message: a port's net is its port.
    fn name_kind(&self, value: ContractValue) -> NameKind {
        match value {
            ContractValue::Net(net) if self.of.block.net_port(net).is_some() => NameKind::Port,
            ContractValue::Net(_) => NameKind::Net,
            ContractValue::Instance(_) => NameKind::Instance,
            ContractValue::Measure(_) => NameKind::Measure,
        }
    }

    /// `dc(…)`, `ac(…)`, or a method: `h.at(1kHz)`.
    fn call(
        &mut self,
        callee: &Expr<'src>,
        args: &[Arg<'src>],
        at: Span,
    ) -> Result<(MExpr, MeasureType), Reported> {
        match &callee.kind {
            ExprKind::Path(path) => match self.r.path_text(path) {
                "dc" => self.dc(args, at),
                "ac" => self.ac(args, at),
                name if LATER_FUNCTIONS.contains(&name) => {
                    Err(self.r.unsupported("`tran` and `noise`", path.span))
                }
                name => {
                    let functions = ["dc", "ac"];
                    Err(self
                        .r
                        .report_unknown_name(name, Namespace::Function, functions, path.span))
                }
            },
            // `.z(f)` is a probe that takes a frequency, not a method.
            ExprKind::Field { base, name } if name.text == "z" => self.probe(base, name),
            ExprKind::Field { base, name } => self.method(base, name, args, at),
            _ => Err(self.r.report(ResolveErrorKind::NotAValue, callee.span)),
        }
    }

    /// `dc(x)`: the operating point of a probe (or of a number, which it is already).
    fn dc(&mut self, args: &[Arg<'src>], at: Span) -> Result<(MExpr, MeasureType), Reported> {
        let [Arg::Positional(x)] = args else {
            return Err(self.bad_arguments("dc", "one probe, like `dc(out.v)`", at));
        };
        let (x, ty) = self.expr(x)?;
        let dim = match ty {
            MeasureType::Probe(dim)
            | MeasureType::Number {
                dim,
                kind: QKind::Plain,
            } => dim,
            _ => return Err(self.bad_arguments("dc", "a probe or a number", at)),
        };
        Ok((MExpr::Dc(Box::new(x)), number(dim, QKind::Plain)))
    }

    /// `ac(out.v / in.v)`: the response of a net to the source on an input port, the
    /// only form an analysis can excite (plan §2.5).
    fn ac(&mut self, args: &[Arg<'src>], at: Span) -> Result<(MExpr, MeasureType), Reported> {
        let [Arg::Positional(ratio)] = args else {
            return Err(self.bad_arguments("ac", "one ratio, like `ac(out.v / in.v)`", at));
        };
        if let ExprKind::Binary {
            op: BinOp::Div,
            lhs,
            rhs,
        } = &unparen(ratio).kind
        {
            let (out, _) = self.expr(lhs)?;
            let (input, _) = self.expr(rhs)?;
            if let Some((out, input)) = self.excited(&out, &input)? {
                let expr = MExpr::Ac { out, input };
                return Ok((expr, MeasureType::Response(Dimension::NONE)));
            }
        }
        Err(self.r.report(ResolveErrorKind::NoExcitation, ratio.span))
    }

    /// The net and the input port of `out / input`, if `ac(…)` excites it: a net's
    /// voltage over an input port's.
    fn excited(&self, out: &MExpr, input: &MExpr) -> Result<Option<(NetId, PortId)>, Reported> {
        let (&MExpr::Voltage(out), &MExpr::Voltage(input)) = (out, input) else {
            return Ok(None);
        };
        Ok(self.input_port(input)?.map(|port| (out, port)))
    }

    /// The input port net `net` is, if it's one: a port whose role is `In`, which every
    /// complete setup gives a source. `Err` for a port whose type is wrong, reported at
    /// the port: whether it's an input isn't known.
    fn input_port(&self, net: NetId) -> Result<Option<PortId>, Reported> {
        let Some(port) = self.of.block.net_port(net) else {
            return Ok(None);
        };
        let input = matches!(
            self.of.block.ports[port.index()].signal?,
            SignalType::Power(Role::In) | SignalType::Analog(Role::In)
        );
        Ok(input.then_some(port))
    }

    /// `of.name(args)`: a method of the measure table, on the measure it's called on.
    fn method(
        &mut self,
        of: &Expr<'src>,
        name: &Ident,
        args: &[Arg<'src>],
        at: Span,
    ) -> Result<(MExpr, MeasureType), Reported> {
        let (base, ty) = self.expr(of)?;
        let method = self.method_named(name, args, ty, at)?;
        if let MeasureType::Probe(_) = ty {
            return Err(self.r.report(ResolveErrorKind::NeedsAnalysis, of.span));
        }
        let Some(result) = method.result(ty) else {
            let kind = ResolveErrorKind::WrongMeasureType { method, on: ty };
            return Err(self.r.report(kind, name.span));
        };
        let expr = MExpr::Method {
            method,
            of: Box::new(base),
        };
        Ok((expr, result))
    }

    /// The method `name` names, with its arguments read: one arm each, the measure
    /// table's names (what each gives is [`Method::result`]). `on` is the type it's
    /// called on, for the suggestion when `name` is unknown.
    fn method_named(
        &mut self,
        name: &Ident,
        args: &[Arg<'src>],
        on: MeasureType,
        at: Span,
    ) -> Result<Method, Reported> {
        match name.text {
            "at" => Ok(Method::At {
                hz: self.positive_hz(args, at)?,
            }),
            "mag" => self.no_arguments(".mag()", args, at, Method::Mag),
            "db" => self.no_arguments(".db()", args, at, Method::Db),
            "f_low" => self.f_low(args, at),
            "f_high" => self.f_high(args, at),
            other if LATER_METHODS.contains(&other) => {
                Err(self.r.unsupported("this method", name.span))
            }
            other => {
                // Only a method that applies to `on`, whatever its arguments: `.mag()`
                // for `h.mg()` would trade this error for `WrongMeasureType`.
                let table = [
                    Method::At { hz: 1.0 },
                    Method::Mag,
                    Method::Db,
                    Method::FLow { db: -3.0 },
                    Method::FHigh {
                        db: -3.0,
                        reference: Reference::Peak,
                    },
                ];
                let methods = table.into_iter().filter(|m| m.result(on).is_some());
                let methods = methods.map(Method::name);
                Err(self
                    .r
                    .report_unknown_name(other, Namespace::Function, methods, name.span))
            }
        }
    }

    /// `.at(f)`'s frequency: in Hz, exact, above 0.
    fn positive_hz(&mut self, args: &[Arg<'src>], at: Span) -> Result<f64, Reported> {
        let expected = "one frequency above 0 Hz, like `.at(1kHz)`";
        let [Arg::Positional(f)] = args else {
            return Err(self.bad_arguments(".at()", expected, at));
        };
        let hz = self.constant(f, FieldType::exact(Dimension::HERTZ), "f")?;
        if hz <= 0.0 {
            return Err(self.bad_arguments(".at()", expected, f.span));
        }
        Ok(hz)
    }

    fn no_arguments(
        &mut self,
        function: &'static str,
        args: &[Arg],
        at: Span,
        method: Method,
    ) -> Result<Method, Reported> {
        match args {
            [] => Ok(method),
            _ => Err(self.bad_arguments(function, "no arguments", at)),
        }
    }

    /// `.f_low(L)`.
    fn f_low(&mut self, args: &[Arg<'src>], at: Span) -> Result<Method, Reported> {
        let [Arg::Positional(level)] = args else {
            let expected = "one level below 0 dB, like `.f_low(-3dB)`";
            return Err(self.bad_arguments(".f_low()", expected, at));
        };
        let db = self.negative_level(".f_low()", level)?;
        Ok(Method::FLow { db })
    }

    /// `.f_high(L)` or `.f_high(L, ref: dc)`.
    fn f_high(&mut self, args: &[Arg<'src>], at: Span) -> Result<Method, Reported> {
        let expected = "a level below 0 dB, and `ref: dc` to measure from the DC gain";
        let (level, reference) = match args {
            [Arg::Positional(level)] => (level, Reference::Peak),
            [Arg::Positional(level), Arg::Named { name, value, .. }]
                if name.text == "ref"
                    && matches!(&unparen(value).kind, ExprKind::Path(p) if self.r.path_text(p) == "dc") =>
            {
                (level, Reference::Dc)
            }
            _ => return Err(self.bad_arguments(".f_high()", expected, at)),
        };
        let db = self.negative_level(".f_high()", level)?;
        Ok(Method::FHigh { db, reference })
    }

    /// A level below 0 dB, `-3dB`: how far a response falls. One above 0 dB is fixed
    /// to its negative: `3dB` is how far it falls, written as some tools take it.
    fn negative_level(
        &mut self,
        function: &'static str,
        level: &Expr<'src>,
    ) -> Result<f64, Reported> {
        let db = self.constant(level, FieldType::level(), "level")?;
        if db >= 0.0 {
            let expected = "a level below 0 dB, like `-3dB`";
            let kind = ResolveErrorKind::BadArguments { function, expected };
            let mut error = ResolveError::new(kind, level.span);
            if db > 0.0 {
                let written = &self.r.src()[level.span.range()];
                error = error.with_fix(level.span, format!("-{written}"));
            }
            return Err(error.report(&mut self.r.errors));
        }
        Ok(db)
    }

    /// A method's argument: a number known before any analysis, typed as a part's value
    /// is (`Resolver::value`). A name of the contract isn't one: a measure is known only
    /// after an analysis, and a net isn't a number.
    fn constant(
        &mut self,
        e: &Expr<'src>,
        expected: FieldType,
        field: &str,
    ) -> Result<f64, Reported> {
        if let ExprKind::Path(path) = &unparen(e).kind
            && let name = self.r.path_text(path)
            && let Some(value) = self.names.get(name)
        {
            let kind = ResolveErrorKind::WrongNamespace {
                name: name.to_string(),
                is: self.name_kind(value),
                expected: "a constant",
            };
            return Err(self.r.report(kind, e.span));
        }
        Ok(self.r.value(e, expected, field)?.nominal.si)
    }

    fn bad_arguments(
        &mut self,
        function: &'static str,
        expected: &'static str,
        at: Span,
    ) -> Reported {
        let kind = ResolveErrorKind::BadArguments { function, expected };
        self.r.report(kind, at)
    }

    /// `lhs op rhs`: both sides, then `combine`. Only the recursion is here, so what
    /// each level of a deep sum keeps on the stack stays small, and a tree as deep as
    /// the parser allows fits a 2 MB stack (ast.md A11).
    fn binary(
        &mut self,
        op: BinOp,
        lhs: &Expr<'src>,
        rhs: &Expr<'src>,
        at: Span,
    ) -> Result<(MExpr, MeasureType), Reported> {
        let op = self.arith_op(op, at)?;
        let a = self.expr(lhs)?;
        let b = self.expr(rhs)?;
        self.combine(op, a, b, at, rhs.span)
    }

    /// One of `+ - * /`: a `±` or a comparison measures nothing.
    fn arith_op(&mut self, op: BinOp, at: Span) -> Result<ArithOp, Reported> {
        match op {
            BinOp::Add => Ok(ArithOp::Add),
            BinOp::Sub => Ok(ArithOp::Sub),
            BinOp::Mul => Ok(ArithOp::Mul),
            BinOp::Div => Ok(ArithOp::Div),
            _ => Err(self.r.report(ResolveErrorKind::NotAValue, at)),
        }
    }

    /// `-x`, of a linear quantity.
    fn neg(
        &mut self,
        (x, ty): (MExpr, MeasureType),
        at: Span,
    ) -> Result<(MExpr, MeasureType), Reported> {
        self.arithmetic_type(ty, at)?;
        Ok((MExpr::Neg(Box::new(x)), ty))
    }

    /// `a op b` over probes and numbers (model.md E12, as in a part's value): `+` and
    /// `-` need one unit, `*` and `/` combine them. A probe anywhere makes a probe, which
    /// an analysis then reads (`dc(vcc.v - 1V)`). Dividing by a written 0 is
    /// `DivisionByZero` at `divisor`, as in a value.
    fn combine(
        &mut self,
        op: ArithOp,
        (a, a_ty): (MExpr, MeasureType),
        (b, b_ty): (MExpr, MeasureType),
        at: Span,
        divisor: Span,
    ) -> Result<(MExpr, MeasureType), Reported> {
        let a_dim = self.arithmetic_type(a_ty, at)?;
        let b_dim = self.arithmetic_type(b_ty, at)?;
        let dim = match op {
            ArithOp::Add | ArithOp::Sub => {
                if a_dim != b_dim {
                    let kind = ResolveErrorKind::MixedUnits {
                        left: describe(QKind::Plain, a_dim),
                        right: describe(QKind::Plain, b_dim),
                    };
                    return Err(self.r.report(kind, at));
                }
                a_dim
            }
            ArithOp::Mul => a_dim * b_dim,
            ArithOp::Div => a_dim / b_dim,
        };
        if let (ArithOp::Div, MExpr::Const(q)) = (op, &b)
            && q.si == 0.0
        {
            return Err(self.r.report(ResolveErrorKind::DivisionByZero, divisor));
        }
        let probe = matches!(a_ty, MeasureType::Probe(_)) || matches!(b_ty, MeasureType::Probe(_));
        let ty = if probe {
            MeasureType::Probe(dim)
        } else {
            number(dim, QKind::Plain)
        };
        let expr = MExpr::Binary {
            op,
            lhs: Box::new(a),
            rhs: Box::new(b),
        };
        Ok((expr, ty))
    }

    /// The unit of `ty` in arithmetic: a probe's or a plain number's. A level, a
    /// temperature, a response or a phasor isn't a linear quantity.
    fn arithmetic_type(&mut self, ty: MeasureType, at: Span) -> Result<Dimension, Reported> {
        match ty {
            MeasureType::Probe(dim) => Ok(dim),
            MeasureType::Number { dim, kind } => self.r.linear(kind, at).map(|()| dim),
            MeasureType::Response(_) | MeasureType::Phasor(_) => {
                let what = "responses";
                Err(self.r.report(ResolveErrorKind::NotArithmetic { what }, at))
            }
        }
    }
}

/// A number of this unit and kind.
fn number(dim: Dimension, kind: QKind) -> MeasureType {
    MeasureType::Number { dim, kind }
}

/// Whether `expr` reads the design: a probe (only ever inside an analysis), a response,
/// or another measure. One that reads nothing is a constant, not a measure, even inside
/// `dc(…)`: `dc(5V)` is `5V`.
fn reads_the_design(expr: &MExpr) -> bool {
    match expr {
        MExpr::Voltage(_) | MExpr::Ac { .. } | MExpr::Measure(_) => true,
        MExpr::Const(_) => false,
        MExpr::Dc(of) | MExpr::Method { of, .. } | MExpr::Neg(of) => reads_the_design(of),
        MExpr::Binary { lhs, rhs, .. } => reads_the_design(lhs) || reads_the_design(rhs),
    }
}
