//! Resolve, the first step of elaboration (model.md §0, §3.2–3.4): the syntax tree
//! becomes a [`Design`], each block once, with every name resolved and every value
//! typed.
//!
//! Two passes (model.md E4, as rustc, rust-analyzer, Spade and Modelica do), then a
//! third that marks what's broken, read top to bottom in [`resolve`]:
//! 1. **The file's names:** every block's name, its circuit and contract (each named
//!    after it) and its setups (each naming it after `for`), then every block's
//!    [`Signature`] (its ports). A block can be placed before its definition. And the
//!    file's values (`resolve/env.rs`): every `env` and `const`, typed against its
//!    declared type, the consts first, since an env's or a part's value may name one.
//! 2. **The circuits:** each block's circuit, by a [`BodyResolver`](body::BodyResolver)
//!    of its own, which has two passes of its own: every name the circuit declares, then
//!    every statement, so statement order never matters.
//! 3. **What's broken:** every block with an error inside it, or a read of a broken
//!    const, is tainted ([`Block::tainted`]), so flatten doesn't check a circuit with a
//!    part missing.
//!
//! Errors never stop the stage (E7): a name that doesn't resolve becomes a placeholder
//! holding the proof it was reported (an `InstanceOf::Error`, an `Err` binding), reported
//! once. A second definition of a name (a block, a circuit, a port, a `net` or `let`, an
//! `env` or `const`) is checked for its own mistakes, then dropped: it never enters the
//! design. Which one was meant isn't known, so the first of a block, an env or a const
//! defined twice is broken too: the block is tainted, the value is `Err`.
//!
//! Contracts and setups are only matched to their blocks here; their contents are
//! resolved in the next steps (contracts_plan.md steps 2–4).

mod body;
mod env;
mod error;
mod value;

pub use error::{NameKind, Namespace, ResolveError, ResolveErrorKind};

use std::collections::HashMap;

use spicy_errors::{Diag, DiagKind, Reported};
use spicy_model::design::{
    Block, BlockId, BlockSpans, ConstId, Contract, Design, DesignSourceMap, EnvId, Instance,
    InstanceSpans, Merge, MergeSpans, Net, NetId, Port, PortId,
};
use spicy_model::prelude::{PartKind, Role, SignalType, SignalTypeError};
use spicy_model::units::Quantity;
use spicy_span::Span;

use crate::edit_distance::edit_distance;
use crate::parser::Parsed;
use crate::parser::ast::{self, BlockDecl, BlockEntry, Body, Ident, ItemKind};
use body::BodyResolver;

/// Everything resolving produces: the design, where each part of it was written, and
/// every problem found.
#[derive(Clone, Debug)]
pub struct Resolved {
    pub design: Design,
    pub source_map: DesignSourceMap,
    pub errors: Vec<ResolveError>,
}

/// Resolves a parsed file. Always returns a design; broken parts are left out or marked
/// and reported.
pub fn resolve(parsed: &Parsed) -> Resolved {
    let mut r = Resolver {
        parsed,
        errors: Vec::new(),
        blocks: Scope::default(),
        values: Scope::default(),
        consts: None,
        syntax: ErrorStarts::default()
            .with(&parsed.lex_errors)
            .with(&parsed.errors),
        suggestions_left: SUGGESTION_BUDGET,
    };

    // Pass 1, the file's names: every block's name, its circuit, contract and setups,
    // then every block's ports. A block can be placed, and a port typed, before its
    // definition.
    let (decls, block_spans, second_decls) = r.declare_blocks();
    let (circuits, second_circuits) = r.match_to_blocks(NameKind::Circuit, decls.len());
    let (contracts, _) = r.match_to_blocks(NameKind::Contract, decls.len());
    r.check_setups(decls.len());
    let (signatures, starts): (Vec<Signature>, Vec<BlockBuilder>) = decls
        .iter()
        .zip(&circuits)
        .map(|(decl, circuit)| r.signature(decl, circuit.is_some()))
        .unzip();

    // …and the file's values: every env's and const's name, then their values. The
    // consts first, all typed before any can be named, since an env's value or a
    // part's may name one.
    let value_decls = r.declare_values();
    let (consts, const_spans) = r.const_values(&value_decls);
    r.consts = Some(consts.iter().map(|c| c.value).collect());
    let (envs, env_spans) = r.env_values(&value_decls);

    // A second circuit of a block is checked against the block's ports (see below).
    let second_starts: Vec<BlockBuilder> = second_circuits
        .iter()
        .map(|&(_, block)| starts[block.index()].clone())
        .collect();

    // Pass 2, the circuits: each block's nets, instances and merges.
    let mut design = Design {
        contracts: contracts.iter().map(|c| c.map(|_| Contract {})).collect(),
        envs,
        consts,
        ..Design::default()
    };
    let mut source_map = DesignSourceMap {
        envs: env_spans,
        consts: const_spans,
        ..DesignSourceMap::default()
    };
    for ((signature, start), circuit) in signatures.iter().zip(starts).zip(&circuits) {
        let stmts = circuit.map_or(&[][..], |(body, _)| &body.stmts);
        let body_resolver = BodyResolver::new(&mut r, &signatures, signature, start, stmts);
        let (block, spans) = body_resolver.resolve(stmts);
        design.blocks.push(block);
        source_map.blocks.push(spans);
    }
    // A second block or circuit of a name gets the same checks, for its own mistakes,
    // then is dropped: it isn't part of the design.
    for &(decl, _) in &second_decls {
        r.signature(decl, false);
    }
    for (&(body, block), start) in second_circuits.iter().zip(second_starts) {
        let signature = &signatures[block.index()];
        BodyResolver::new(&mut r, &signatures, signature, start, &body.stmts).resolve(&body.stmts);
    }

    // Pass 3, what's broken: every block with an error in it (see `Block::tainted`).
    // One its header's or circuit's parse reported, since an error found at the next
    // item (a missing `}`) is the open one's, not the next one's; or a lexer or resolve
    // error inside either. (A part's value that reads a broken const, whose error is
    // reported at the const, tainted its block in pass 2.)
    let inside = ErrorStarts::default()
        .with(&parsed.lex_errors)
        .with(&r.errors);
    let blocks = design
        .blocks
        .iter_mut()
        .zip(&decls)
        .zip(block_spans)
        .zip(&circuits);
    for (((block, decl), span), circuit) in blocks {
        let (circuit_broken, circuit_span) = match circuit {
            Some((body, span)) => (body.broken, Some(*span)),
            None => (None, None),
        };
        block.tainted = block
            .tainted
            .or(decl.broken)
            .or(circuit_broken)
            .or_else(|| inside.inside(span))
            .or_else(|| circuit_span.and_then(|s| inside.inside(s)));
    }
    // A block defined twice: which definition was meant isn't known, so the first,
    // the one the design keeps, is broken too.
    for (_, Redefined { first, reported }) in second_decls {
        design.blocks[first.index()].tainted.get_or_insert(reported);
    }

    Resolved {
        design,
        source_map,
        errors: r.errors,
    }
}

/// Where reported errors are, sorted, with the proof they were reported: what the
/// parser built around a syntax error is a guess, and a block with an error inside it
/// is tainted.
#[derive(Clone, Default)]
struct ErrorStarts {
    spans: Vec<Span>,
    reported: Option<Reported>,
}

impl ErrorStarts {
    /// These errors, and the errors among `diags`, one stage's problems.
    fn with<K: DiagKind>(mut self, diags: &[Diag<K>]) -> Self {
        let errors = diags.iter().filter(|e| e.is_error());
        self.spans.extend(errors.map(|e| e.span));
        self.spans.sort_unstable();
        self.reported = self.reported.or(Reported::among(diags));
        self
    }

    /// The proof, if one of the errors starts inside `s`.
    fn inside(&self, s: Span) -> Option<Reported> {
        let i = self.spans.partition_point(|e| e.start < s.start);
        let found = self.spans.get(i).is_some_and(|e| e.start < s.end);
        found.then(|| self.reported.expect("an error start is a reported error"))
    }
}

/// The file-wide state: the blocks' names, the file's values, and what every part of
/// the stage adds to (the errors, the suggestion budget). The blocks' signatures are a
/// separate, read-only table in pass 2 (as rust-analyzer keeps signatures apart from
/// bodies).
struct Resolver<'p, 'src> {
    parsed: &'p Parsed<'src>,
    errors: Vec<ResolveError>,
    /// The kind namespace's blocks (model.md E5).
    blocks: Scope<'src, BlockId>,
    /// The file's values: its envs and consts (model.md E5).
    values: Scope<'src, FileValue>,
    /// Each const's value, by `ConstId`. `None` while the consts are typed, so a const's
    /// value can't name another yet (see `named_value`).
    consts: Option<Vec<Result<Quantity, Reported>>>,
    /// Every lexer and parser error: an expression with one inside isn't typed (see
    /// `value`).
    syntax: ErrorStarts,
    suggestions_left: usize,
}

/// What a name in the file's values is.
#[derive(Clone, Copy)]
enum FileValue {
    Env(EnvId),
    Const(ConstId),
}

/// Each block's circuit or contract, if it has one, with the span of its item.
type PerBlock<'p, 'src> = Vec<Option<(&'p Body<'src>, Span)>>;

/// The second definitions of a block name, each with the first it redefines.
type SecondBlocks<'p, 'src> = Vec<(&'p BlockDecl<'src>, Redefined<BlockId>)>;

/// What the rest of the file sees of a block: its ports, by name, which a placement
/// binds, and whether it has a circuit to place. Read-only in pass 2.
struct Signature<'src> {
    ports: Scope<'src, PortId>,
    has_circuit: bool,
}

/// A block being built, with its source map: each part is added together with where it
/// was written, so the two stay indexed alike (as rust-analyzer's `alloc_expr` records
/// a node and its source in one call).
#[derive(Clone, Default)]
struct BlockBuilder {
    block: Block,
    spans: BlockSpans,
}

impl BlockBuilder {
    fn new(name: &Ident) -> Self {
        Self {
            block: Block {
                name: name.text.to_string(),
                ..Block::default()
            },
            spans: BlockSpans {
                name: name.span,
                ..BlockSpans::default()
            },
        }
    }

    /// A port and its net, which is named after it.
    fn push_port(
        &mut self,
        name: &Ident,
        signal: Result<SignalType, Reported>,
        ty: Span,
    ) -> PortId {
        let port = PortId::new(self.block.ports.len());
        self.block.ports.push(Port { signal });
        self.spans.port_types.push(ty);
        self.push_net(name);
        port
    }

    fn push_net(&mut self, name: &Ident) -> NetId {
        let net = NetId::new(self.block.nets.len());
        self.block.nets.push(Net {
            name: name.text.to_string(),
        });
        self.spans.nets.push(name.span);
        net
    }

    /// Marks the block broken ([`Block::tainted`]), keeping the first proof.
    fn taint(&mut self, reported: Reported) {
        self.block.tainted.get_or_insert(reported);
    }

    fn push_instance(&mut self, (instance, spans): (Instance, InstanceSpans)) {
        self.block.instances.push(instance);
        self.spans.instances.push(spans);
    }

    fn push_merge(&mut self, (merge, spans): (Merge, MergeSpans)) {
        self.block.merges.push(merge);
        self.spans.merges.push(spans);
    }
}

/// The names declared in one scope (the file's blocks, a block's ports, a block's nets
/// and instances), each with where it was first declared. The one place a name given
/// twice is caught; a second declaration is reported and isn't entered (rustc's
/// `try_plant_decl_into_local_module`, Spade's `ensure_is_unique`).
#[derive(Clone)]
struct Scope<'src, T> {
    declared: HashMap<&'src str, (T, Span)>,
}

impl<T> Default for Scope<'_, T> {
    fn default() -> Self {
        Self {
            declared: HashMap::new(),
        }
    }
}

impl<'src, T: Copy> Scope<'src, T> {
    /// Declares `name` as `value`. If it's already declared, reports a duplicate of the
    /// first declaration (`what` is what kind of name it is), and returns the first
    /// (as rustc's `try_define` returns the old binding).
    fn declare(
        &mut self,
        name: &Ident<'src>,
        value: T,
        what: NameKind,
        errors: &mut Vec<ResolveError>,
    ) -> Result<(), Redefined<T>> {
        if let Some(&(first, at)) = self.declared.get(name.text) {
            let kind = ResolveErrorKind::Duplicate {
                name: name.text.to_string(),
                what,
            };
            let reported = ResolveError::new(kind, name.span)
                .with_related(at)
                .report(errors);
            return Err(Redefined { first, reported });
        }
        self.declared.insert(name.text, (value, name.span));
        Ok(())
    }

    fn get(&self, name: &str) -> Option<T> {
        self.declared.get(name).map(|&(value, _)| value)
    }

    /// Whether `name` is the declaration its name refers to, not a second one.
    fn is_first(&self, name: &Ident) -> bool {
        self.declared
            .get(name.text)
            .is_some_and(|&(_, at)| at == name.span)
    }

    fn iter(&self) -> impl Iterator<Item = (&'src str, T)> + '_ {
        self.declared
            .iter()
            .map(|(&name, &(value, _))| (name, value))
    }
}

/// The first declaration of a name declared again, and the proof the second was
/// reported. Which one was meant isn't known, so the first is broken too where it can
/// be: a block defined twice is tainted (pass 3), an env's or const's value is `Err`
/// (`resolve/env.rs`).
#[derive(Clone, Copy)]
struct Redefined<T> {
    first: T,
    reported: Reported,
}

/// How many "did you mean" searches one file gets. Each looks at every name in scope,
/// so a file of thousands of unknown names would take quadratic time; past this
/// budget, errors come without suggestions (rustc caps its suggestions the same way).
const SUGGESTION_BUDGET: usize = 64;

/// The candidate closest to `name`, while `left` (the file's remaining "did you mean"
/// searches) lasts. A free function over the one counter, so a search can borrow the
/// resolver's names while it spends the budget.
fn suggest<'c>(
    left: &mut usize,
    name: &str,
    candidates: impl IntoIterator<Item = &'c str>,
) -> Option<String> {
    *left = left.checked_sub(1)?;
    closest(name, candidates)
}

/// An unknown name, with the closest known one as a suggestion. `fix` is the edit that
/// applies it (a single choice, so the editor or the AI can apply it).
fn unknown_name(
    name: &str,
    namespace: Namespace,
    suggestion: Option<String>,
    fix: Option<String>,
    at: Span,
) -> ResolveError {
    let kind = ResolveErrorKind::UnknownName {
        name: name.to_string(),
        namespace,
        suggestion,
    };
    let error = ResolveError::new(kind, at);
    match fix {
        Some(fix) => error.with_fix(at, fix),
        None => error,
    }
}

impl<'p, 'src> Resolver<'p, 'src> {
    fn src(&self) -> &'src str {
        self.parsed.tokens.src()
    }

    /// A path as written. The MVP has no modules, so only a one-segment path can
    /// resolve: `a::b` is looked up as `"a::b"` and reported as unknown.
    fn path_text(&self, path: &ast::Path) -> &'src str {
        &self.src()[path.span.range()]
    }

    fn report(&mut self, kind: ResolveErrorKind, at: Span) -> Reported {
        ResolveError::new(kind, at).report(&mut self.errors)
    }

    // --- Pass 1: the file's names ---------------------------------------------------

    /// Every block's name, in `blocks`. Returns the blocks of the design, in source
    /// order (a block's id is its place here), with the span of each one's whole item,
    /// and the second definitions of a block name, each with the first it redefines.
    fn declare_blocks(&mut self) -> (Vec<&'p BlockDecl<'src>>, Vec<Span>, SecondBlocks<'p, 'src>) {
        let (mut first, mut spans, mut second) = (Vec::new(), Vec::new(), Vec::new());
        for item in &self.parsed.file.items {
            let ItemKind::Block(decl) = &item.kind else {
                continue;
            };
            let id = BlockId::new(first.len());
            let declared = self
                .blocks
                .declare(&decl.name, id, NameKind::Block, &mut self.errors);
            match declared {
                Ok(()) => {
                    first.push(decl);
                    spans.push(item.span);
                }
                Err(redefined) => second.push((decl, redefined)),
            }
        }
        (first, spans, second)
    }

    /// Each of the `blocks` blocks' circuit or contract (`what`), the item of that kind
    /// named after it, with the item's span. A second one of a name is a duplicate,
    /// returned with its block; one for no block is reported.
    fn match_to_blocks(
        &mut self,
        what: NameKind,
        blocks: usize,
    ) -> (PerBlock<'p, 'src>, Vec<(&'p Body<'src>, BlockId)>) {
        let (mut first, mut second) = (vec![None; blocks], Vec::new());
        let mut seen = Scope::default();
        for item in &self.parsed.file.items {
            let body = match (&item.kind, what) {
                (ItemKind::Circuit(body), NameKind::Circuit)
                | (ItemKind::Contract(body), NameKind::Contract) => body,
                _ => continue,
            };
            let declared = seen.declare(&body.name, (), what, &mut self.errors);
            match (self.blocks.get(body.name.text), declared) {
                (Some(block), Ok(())) => first[block.index()] = Some((body, item.span)),
                (Some(block), Err(_)) => second.push((body, block)),
                (None, Ok(())) => {
                    let kind = ResolveErrorKind::UnknownBlock {
                        item: what,
                        name: body.name.text.to_string(),
                    };
                    self.report(kind, body.name.span);
                }
                (None, Err(_)) => {}
            }
        }
        (first, second)
    }

    /// The block after each setup's `for` is one of the `blocks` blocks, and each block's
    /// setups have different names. Setups are named per block, so every block can have
    /// its own `Operating` (research/contract_v4_review_implementation.md §3.2).
    fn check_setups(&mut self, blocks: usize) {
        let mut seen: Vec<Scope<()>> = vec![Scope::default(); blocks];
        for item in &self.parsed.file.items {
            let ItemKind::Setup(setup) = &item.kind else {
                continue;
            };
            let name = self.path_text(&setup.block);
            match self.blocks.get(name) {
                Some(block) => {
                    let setups = &mut seen[block.index()];
                    let _ = setups.declare(&setup.name, (), NameKind::Setup, &mut self.errors);
                }
                None => {
                    let kind = ResolveErrorKind::UnknownBlock {
                        item: NameKind::Setup,
                        name: name.to_string(),
                    };
                    self.report(kind, setup.block.span);
                }
            }
        }
    }

    /// A block's signature, and the start of the block itself: its name, and its ports
    /// with their types and nets.
    fn signature(
        &mut self,
        decl: &BlockDecl<'src>,
        has_circuit: bool,
    ) -> (Signature<'src>, BlockBuilder) {
        let mut block = BlockBuilder::new(&decl.name);
        let mut ports = Scope::default();
        for port in &decl.ports {
            let BlockEntry::Port { name, ty } = &port.kind else {
                continue;
            };
            // Typed even when it's a second port of the name, for its own mistakes. A
            // port with a wrong type is still a port: its uses resolve (model.md E7).
            let signal = self.signal_type(ty);
            let port = PortId::new(block.block.ports.len());
            if ports
                .declare(name, port, NameKind::Port, &mut self.errors)
                .is_ok()
            {
                block.push_port(name, signal, ty.span);
            }
        }
        (Signature { ports, has_circuit }, block)
    }

    /// A port's type: `Pin`, `Power<In>`. `Err` if it's wrong, reported, or didn't
    /// parse.
    fn signal_type(&mut self, ty: &ast::Type) -> Result<SignalType, Reported> {
        match &ty.kind {
            ast::TypeKind::Path { path, args } => {
                let result = self.try_signal_type(path, args, ty.span);
                result.map_err(|e| e.report(&mut self.errors))
            }
            ast::TypeKind::Error(reported) => Err(*reported),
        }
    }

    #[expect(
        clippy::result_large_err,
        reason = "cold: the error is reported right away"
    )]
    fn try_signal_type(
        &self,
        path: &ast::Path,
        args: &[ast::Type],
        span: Span,
    ) -> Result<SignalType, ResolveError> {
        let name = self.path_text(path);
        if !SignalType::is_name(name) {
            // `v: Resistor` as a port: say what it is instead of "isn't a port type".
            let is = if self.blocks.get(name).is_some() {
                Some(NameKind::Block)
            } else {
                PartKind::from_name(name).map(|_| NameKind::PartKind)
            };
            let kind = match is {
                Some(is) => ResolveErrorKind::WrongNamespace {
                    name: name.to_string(),
                    is,
                    expected: "a port type",
                },
                None => ResolveErrorKind::BadSignalType {
                    name: name.to_string(),
                    problem: "isn't a port type",
                },
            };
            return Err(ResolveError::new(kind, path.span));
        }
        let role = match args {
            [] => Ok(None),
            [
                ast::Type {
                    kind: ast::TypeKind::Path { path, args },
                    ..
                },
            ] if args.is_empty() => Role::from_name(self.path_text(path))
                .map(Some)
                .ok_or("takes a role: `In` or `Out`"),
            _ => Err("takes one role: `In` or `Out`"),
        };
        let problem = match role.map(|role| SignalType::from_name(name, role)) {
            Ok(Ok(t)) => return Ok(t),
            Ok(Err(SignalTypeError::NeedsRole)) => "needs a role: `In` or `Out`",
            Ok(Err(SignalTypeError::TakesNoRole | SignalTypeError::Unknown)) => "takes no role",
            Err(problem) => problem,
        };
        let kind = ResolveErrorKind::BadSignalType {
            name: name.to_string(),
            problem,
        };
        Err(ResolveError::new(kind, span))
    }
}

/// The candidate closest to `name`: the same letters ignoring case, or else within a
/// third of its length in edits, at least one (rustc's name suggestions,
/// `find_best_match_for_name`). Ties go to the first in alphabetical order, so the
/// answer doesn't depend on the candidates' order.
fn closest<'c>(name: &str, candidates: impl IntoIterator<Item = &'c str>) -> Option<String> {
    let len = name.chars().count();
    let limit = len.max(3) / 3;
    candidates
        .into_iter()
        .filter_map(|c| {
            if c.eq_ignore_ascii_case(name) {
                return Some((0, c));
            }
            // Edit distance is at least the length difference: skip the far ones cheaply.
            if c.chars().count().abs_diff(len) > limit {
                return None;
            }
            let d = edit_distance(name, c);
            (d <= limit).then_some((d, c))
        })
        .min()
        .map(|(_, c)| c.to_string())
}

/// The case-file suite (model.md §6).
#[cfg(test)]
mod tests {
    use std::path::Path;

    use spicy_model::design::{FieldValue, InstanceOf};

    use super::*;
    use crate::parser::parse;
    use crate::testing::{
        assert_every_kind_has_a_case, check_parse_invariants, dump_resolve, file_name, read,
        resolve_errors,
    };
    use spicy_errors::DiagKind;

    /// `ok/` cases: no problems at any stage, and a snapshot of the design.
    #[test]
    fn ok_cases() {
        insta::glob!("../../test_data/resolve", "ok/*.spl", |path| {
            let src = read(path);
            check_parse_invariants(&src);
            let parsed = parse(&src);
            let resolved = resolve(&parsed);
            assert!(
                !parsed.has_errors() && resolved.errors.is_empty(),
                "{}: unexpected errors {:#?}",
                path.display(),
                resolved.errors
            );
            insta::assert_snapshot!(dump_resolve(&file_name(path), &src));
        });
    }

    /// `err/` cases: at least one resolve error, and a snapshot of design and errors.
    #[test]
    fn err_cases() {
        insta::glob!("../../test_data/resolve", "err/*.spl", |path| {
            let src = read(path);
            check_parse_invariants(&src);
            assert!(
                !resolve_errors(&src).is_empty(),
                "{}: expected resolve errors",
                path.display()
            );
            insta::assert_snapshot!(dump_resolve(&file_name(path), &src));
        });
    }

    #[test]
    fn every_error_kind_has_a_case() {
        assert_every_kind_has_a_case("resolve/err", ResolveErrorKind::ALL_NAMES, |src| {
            resolve_errors(src).iter().map(|e| e.kind.name()).collect()
        });
    }

    /// The design the MVP is built around: 1 block with 4 ports, 2 nets and 6 parts, and
    /// its contract (model.md §5).
    #[test]
    fn ce_amp() {
        let src = include_str!("../../../../circuits/ce_amp.spl");
        let parsed = parse(src);
        let resolved = resolve(&parsed);
        assert!(resolved.errors.is_empty(), "{:#?}", resolved.errors);
        let design = &resolved.design;
        assert_eq!(design.blocks.len(), 1);
        let amp = &design.blocks[0];
        assert_eq!(amp.ports.len(), 4);
        assert_eq!(amp.nets.len(), 6, "4 port nets + base + emitter");
        assert_eq!(amp.instances.len(), 6);
        assert!(
            amp.instances
                .iter()
                .all(|i| i.pins.iter().all(Result::is_ok))
        );
        assert_eq!(design.contracts, [Some(Contract {})]);
        insta::assert_snapshot!(dump_resolve("ce_amp.spl", src));
    }

    /// Statement order inside a circuit doesn't change what it means (model.md E4): every
    /// order of `ce_amp.spl`'s circuit statements resolves to the same parts and bindings.
    #[test]
    fn statement_order_does_not_matter() {
        let src = include_str!("../../../../circuits/ce_amp.spl");
        let block_end = src.find("\n}\n").unwrap();
        let (head, rest) = src.split_at(block_end);
        let open = head.find("{\n").unwrap() + 2;
        let lines: Vec<&str> = head[open..]
            .lines()
            .filter(|l| l.trim_end().ends_with(';'))
            .collect();
        let reference = canonical(&resolve(&parse(src)).design);
        let mut rng = crate::testing::Rng(0x5eed);
        for _ in 0..20 {
            let mut shuffled = lines.clone();
            for i in (1..shuffled.len()).rev() {
                shuffled.swap(i, rng.below(i + 1));
            }
            let text = format!("{}{}\n{}", &head[..open], shuffled.join("\n"), rest);
            let resolved = resolve(&parse(&text));
            assert!(resolved.errors.is_empty(), "{text}");
            assert_eq!(canonical(&resolved.design), reference, "{text}");
        }
    }

    /// A design as a sorted list of facts, independent of declaration order.
    fn canonical(design: &Design) -> Vec<String> {
        let mut facts = Vec::new();
        for block in &design.blocks {
            for (name, port) in block.port_names().zip(&block.ports) {
                facts.push(format!("{} port {name} {:?}", block.name, port.signal));
            }
            for net in &block.nets {
                facts.push(format!("{} net {}", block.name, net.name));
            }
            for inst in &block.instances {
                let pins: Vec<&str> = inst
                    .pins
                    .iter()
                    .map(|p| p.map_or("?", |n| block.net(n).name.as_str()))
                    .collect();
                facts.push(format!(
                    "{} {} {:?} {:?} {:?}",
                    block.name, inst.name, inst.of, pins, inst.fields
                ));
            }
        }
        facts.sort();
        facts
    }

    /// Every suggested fix, applied on its own, removes the error it was offered for
    /// and adds none: a fix that trades one error for another isn't the right answer.
    #[test]
    fn fixes_remove_their_error() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data/resolve/err");
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "spl") {
                continue;
            }
            let src = read(&path);
            let before = problem_names(&src);
            for e in resolve_errors(&src) {
                let Some(fix) = e.fix else { continue };
                let mut fixed = src.clone();
                fixed.replace_range(fix.span.range(), &fix.replacement);
                let mut expected = before.clone();
                let at = expected.iter().position(|&n| n == e.kind.name()).unwrap();
                expected.remove(at);
                assert_eq!(
                    problem_names(&fixed),
                    expected,
                    "{}: fix {:?} for {} at {}..{}",
                    path.display(),
                    fix.replacement,
                    e.kind.name(),
                    fix.span.start,
                    fix.span.end,
                );
                checked += 1;
            }
        }
        assert!(checked >= 8, "only {checked} fixes checked");
    }

    /// Every lexer, parser and resolve problem of `src`, by name, sorted.
    fn problem_names(src: &str) -> Vec<&'static str> {
        let parsed = parse(src);
        let mut names: Vec<&'static str> =
            parsed.lex_errors.iter().map(|e| e.kind.name()).collect();
        names.extend(parsed.errors.iter().map(|e| e.kind.name()));
        names.extend(resolve(&parsed).errors.iter().map(|e| e.kind.name()));
        names.sort_unstable();
        names
    }

    /// A second `let` or `net` of a name is checked but never lands in the first one's
    /// place (red team B1: it used to overwrite the first instance's pins and value).
    #[test]
    fn a_duplicate_never_replaces_the_first() {
        let src = "block B { v: Pin, g: Ground }\n\ncircuit B {\n    net x = [v];\n    net x = [g];\n    let r1 = Resistor { a: v, b: g, value: 1k };\n    let r1 = Resistor { a: g, b: v, value: 2k, bad: v };\n}\n";
        let resolved = resolve(&parse(src));
        let b = &resolved.design.blocks[0];
        assert_eq!(b.instances.len(), 1);
        let r1 = &b.instances[0];
        assert_eq!(b.net(r1.pins[0].unwrap()).name, "v");
        let FieldValue::Given(value) = r1.fields[0] else {
            panic!("{:?}", r1.fields)
        };
        assert_eq!(value.nominal.si, 1000.0);
        assert_eq!(b.merges.len(), 1, "the second `net x` isn't merged");
        assert_eq!(b.net(b.merges[0].with[0]).name, "v");
        let names: Vec<_> = resolved.errors.iter().map(|e| e.kind.name()).collect();
        // Two duplicates, and the second `let`'s own mistake is still found.
        assert_eq!(names, ["Duplicate", "Duplicate", "UnknownField"]);
    }

    /// Thousands of unknown names stay fast: suggestions are budgeted, and lookups are
    /// by hash, not by scanning (red team B9: 5000 unknowns took 20 s).
    #[test]
    fn many_unknown_names_stay_linear() {
        let mut src = String::from("block B { v: Pin }\n\ncircuit B {\n");
        for i in 0..4000 {
            src.push_str(&format!(
                "    net n{i};\n    let r{i} = Resistor {{ a: nope{i}, b: v, value: 1k }};\n"
            ));
        }
        src.push_str("}\n");
        let parsed = parse(&src);
        let start = std::time::Instant::now();
        let resolved = resolve(&parsed);
        assert_eq!(resolved.errors.len(), 4000);
        let elapsed = start.elapsed();
        assert!(elapsed.as_secs_f64() < 2.0, "took {elapsed:?}");
    }

    /// A statement whose value or a port whose type doesn't parse keeps its name, and a
    /// block or circuit whose `{` is missing keeps its contents, so nothing that uses them
    /// is reported again: only the syntax error remains (rustc keeps a field whose
    /// expression didn't parse).
    #[test]
    fn a_broken_value_adds_no_resolve_errors() {
        let sources = [
            // `base` is still a net of `A`.
            "block A { g: Ground }\n\ncircuit A {\n    net base = [g g];\n    \
             let r = Resistor { a: base, b: g, value: 1k };\n}\n",
            // `a` is still a port of `Child`.
            "block Child { a: , g: Ground }\n\ncircuit Child {}\n\n\
             block Top { g: Ground }\n\ncircuit Top {\n    let c = Child { a: g, g };\n}\n",
            // `Child` is still a block, with its port.
            "block Child\n    a: Pin }\n\ncircuit Child {}\n\n\
             block Top { a: Pin }\n\ncircuit Top {\n    let c = Child { a };\n}\n",
            // `Child` still has its circuit.
            "block Child { a: Pin }\n\ncircuit Child\n    net n;\n}\n\n\
             block Top { a: Pin }\n\ncircuit Top {\n    let c = Child { a };\n}\n",
        ];
        for src in sources {
            assert!(parse(src).has_errors(), "{src}");
            assert_eq!(resolve_errors(src), vec![], "{src}");
        }
    }

    /// A second block or circuit of a name is resolved for its own errors, after the
    /// design's blocks, then dropped: the design, its source map and its contracts hold
    /// the first only, and placements bind the first's ports.
    #[test]
    fn a_second_block_is_checked_then_dropped() {
        let src = "block A { p: Pin }\n\ncircuit A {}\n\nblock A { q: Bus }\n\n\
                   circuit A {\n    let r = Resistor { a: p, b: nowhere, value: 1k };\n}\n\n\
                   block B { x: Pin }\n\ncircuit B {\n    let a = A { p: x };\n}\n\ncontract A {}\n";
        let resolved = resolve(&parse(src));
        let design = &resolved.design;
        let names: Vec<&str> = design.blocks.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["A", "B"]);
        assert_eq!(resolved.source_map.blocks.len(), 2);
        assert_eq!(design.contracts, [Some(Contract {}), None]);
        let a = &design.blocks[1].instances[0];
        assert_eq!(a.of, InstanceOf::Block(BlockId::new(0)));
        assert_eq!(a.pins, [Ok(NetId::new(0))]);
        let errors: Vec<_> = resolved.errors.iter().map(|e| e.kind.name()).collect();
        assert_eq!(
            errors,
            ["Duplicate", "Duplicate", "BadSignalType", "UnknownName"]
        );
    }

    /// `C { a }` where `a` is an instance: the shorthand names pin `a`, so the error
    /// says which pin, as `a: r1` does.
    #[test]
    fn shorthand_bound_to_an_instance_names_its_pin() {
        let src =
            "block A { b: Pin }\n\ncircuit A {\n    let a = Resistor { a, b, value: 1k };\n}\n";
        let errors = resolve_errors(src);
        let kinds: Vec<_> = errors.iter().map(|e| &e.kind).collect();
        assert_eq!(
            kinds,
            [&ResolveErrorKind::NotANet {
                pin: Some("a".to_string()),
                found: NameKind::Instance,
            }]
        );
    }

    /// One budget for the file, spent by net and kind searches alike; a rename (one
    /// unknown field, one missing slot) isn't a search, so it's offered past the budget.
    #[test]
    fn the_suggestion_budget_is_shared_and_renames_are_free() {
        let mut src = String::from("block B { base: Pin }\n\ncircuit B {\n");
        for i in 0..40 {
            src.push_str(&format!(
                "    let k{i} = Resistr {{}};\n    let r{i} = Resistor {{ a: bas, b: base, value: 1k }};\n"
            ));
        }
        src.push_str("    let late = Resistor { a: base, b: base, resistance: 1k };\n}\n");
        let errors = resolve_errors(&src);
        let suggested = |e: &ResolveError| match &e.kind {
            ResolveErrorKind::UnknownName { suggestion, .. }
            | ResolveErrorKind::UnknownField { suggestion, .. } => suggestion.clone(),
            _ => panic!("{e:?}"),
        };
        let (searched, rename) = errors.split_at(80);
        let with: Vec<bool> = searched.iter().map(|e| suggested(e).is_some()).collect();
        assert_eq!(
            with,
            [vec![true; SUGGESTION_BUDGET], vec![false; 16]].concat()
        );
        assert_eq!(suggested(&rename[0]).as_deref(), Some("value"));
    }

    /// `closest`: a case-only difference beats any edit, the limit is a third of the
    /// length (at least one), and ties go to the alphabetically first.
    #[test]
    fn closest_name_rules() {
        assert_eq!(closest("vcc", ["vc", "VCC"]).as_deref(), Some("VCC"));
        assert_eq!(closest("ab", ["abc"]).as_deref(), Some("abc"));
        assert_eq!(closest("abcdef", ["abcd"]).as_deref(), Some("abcd"));
        assert_eq!(closest("abcdef", ["abc"]), None);
        assert_eq!(closest("vlaue", ["value"]), None, "a swap is two edits");
        assert_eq!(closest("nx", ["nb", "na"]).as_deref(), Some("na"));
        assert_eq!(closest("x", std::iter::empty()), None);
    }
}
