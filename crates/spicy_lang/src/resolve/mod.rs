//! Resolve, the first step of elaboration (model.md §0, §3.2–3.4): the syntax tree
//! becomes a [`Design`], each block once, with every name resolved and every value
//! typed.
//!
//! Two passes (model.md E4, as rustc, rust-analyzer, Spade and Modelica do), read top
//! to bottom in [`resolve`]:
//! 1. **The file's names:** every block's name, then every block's [`Signature`] (its
//!    ports), then the contracts. A block can be placed before its definition.
//! 2. **The bodies:** each block's body, by a [`BodyResolver`](body::BodyResolver) of
//!    its own, which has two passes of its own: every name the body declares, then
//!    every statement, so statement order never matters.
//!
//! Errors never stop the stage (E7): a name that doesn't resolve becomes an
//! `InstanceOf::Error` or a missing binding, reported once. A second definition of a
//! name (a block, a port, a `net` or `let`) is checked for its own mistakes, then
//! dropped: it never enters the design.
//!
//! Contracts are only matched to their blocks here; their contents are resolved in the
//! next step (roadmap M1d-5).

mod body;
mod error;
mod value;

pub use error::{NameKind, Namespace, ResolveError, ResolveErrorKind};

use std::collections::HashMap;

use spicy_model::design::{
    Block, BlockId, BlockSpans, Contract, Design, DesignSourceMap, Instance, InstanceSpans, Merge,
    MergeSpans, Net, NetId, Port, PortId,
};
use spicy_model::prelude::{PartKind, Role, SignalType, SignalTypeError};
use spicy_model::span::Span;

use crate::edit_distance::edit_distance;
use crate::parser::Parsed;
use crate::parser::ast::{self, Body, Ident, ItemKind, StmtKind};
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
        syntax_errors: syntax_error_spans(parsed),
        suggestions_left: SUGGESTION_BUDGET,
    };

    // Pass 1, the file's names: every block's name, then every block's ports, then the
    // contracts. A block can be placed, and a port typed, before its definition.
    let (bodies, second_bodies) = r.declare_blocks();
    let (signatures, starts): (Vec<Signature>, Vec<BlockBuilder>) =
        bodies.iter().map(|body| r.signature(body)).unzip();
    let contracts = r.match_contracts(signatures.len());

    // Pass 2, the bodies: each block's nets, instances and merges.
    let mut design = Design {
        contracts,
        ..Design::default()
    };
    let mut source_map = DesignSourceMap::default();
    for ((signature, start), body) in signatures.iter().zip(starts).zip(&bodies) {
        let body_resolver = BodyResolver::new(&mut r, &signatures, signature, start, body);
        let (block, spans) = body_resolver.resolve(body);
        design.blocks.push(block);
        source_map.blocks.push(spans);
    }
    // A second block of a name gets the same checks, for its own mistakes, then is
    // dropped: it isn't part of the design.
    for body in second_bodies {
        let (signature, start) = r.signature(body);
        BodyResolver::new(&mut r, &signatures, &signature, start, body).resolve(body);
    }

    Resolved {
        design,
        source_map,
        errors: r.errors,
    }
}

/// Where every lexer and parser error starts, sorted.
fn syntax_error_spans(parsed: &Parsed) -> Vec<Span> {
    let lex = parsed.lex_errors.iter().map(|e| e.span);
    let parse = parsed.errors.iter().filter(|e| e.is_error());
    let mut spans: Vec<Span> = lex.chain(parse.map(|e| e.span)).collect();
    spans.sort_unstable();
    spans
}

/// The file-wide state: the blocks' names, and what every part of the stage adds to
/// (the errors, the suggestion budget). The blocks' signatures are a separate,
/// read-only table in pass 2 (as rust-analyzer keeps signatures apart from bodies).
struct Resolver<'p, 'src> {
    parsed: &'p Parsed<'src>,
    errors: Vec<ResolveError>,
    /// The kind namespace's blocks (model.md E5).
    blocks: Scope<'src, BlockId>,
    /// Every lexer and parser error, sorted: an expression with one inside isn't typed
    /// (see `value`).
    syntax_errors: Vec<Span>,
    suggestions_left: usize,
}

/// What the rest of the file sees of a block: its ports, by name, which a placement
/// binds. Read-only in pass 2.
type Signature<'src> = Scope<'src, PortId>;

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
    fn push_port(&mut self, name: &Ident, signal: Option<SignalType>, ty: Span) -> PortId {
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
    /// first declaration (`what` is what kind of name it is) and returns false.
    fn declare(
        &mut self,
        name: &Ident<'src>,
        value: T,
        what: NameKind,
        errors: &mut Vec<ResolveError>,
    ) -> bool {
        if let Some(&(_, first)) = self.declared.get(name.text) {
            let kind = ResolveErrorKind::Duplicate {
                name: name.text.to_string(),
                what,
            };
            errors.push(ResolveError::new(kind, name.span).with_related(first));
            return false;
        }
        self.declared.insert(name.text, (value, name.span));
        true
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

    fn error(&mut self, kind: ResolveErrorKind, at: Span) {
        self.errors.push(ResolveError::new(kind, at));
    }

    /// Whether a lexer or parser error starts inside `s`.
    fn has_syntax_error(&self, s: Span) -> bool {
        let i = self.syntax_errors.partition_point(|e| e.start < s.start);
        self.syntax_errors.get(i).is_some_and(|e| e.start < s.end)
    }

    // --- Pass 1: the file's names ---------------------------------------------------

    /// Every block's name, in `blocks`. Returns the blocks of the design, in source
    /// order (a block's id is its place here), and the second definitions of a block
    /// name, already reported.
    fn declare_blocks(&mut self) -> (Vec<&'p Body<'src>>, Vec<&'p Body<'src>>) {
        let (mut first, mut second) = (Vec::new(), Vec::new());
        for item in &self.parsed.file.items {
            let ItemKind::Block(body) = &item.kind else {
                continue;
            };
            let id = BlockId::new(first.len());
            if self
                .blocks
                .declare(&body.name, id, NameKind::Block, &mut self.errors)
            {
                first.push(body);
            } else {
                second.push(body);
            }
        }
        (first, second)
    }

    /// A block's signature, and the start of the block itself: its name, and its ports
    /// with their types and nets.
    fn signature(&mut self, body: &Body<'src>) -> (Signature<'src>, BlockBuilder) {
        let mut block = BlockBuilder::new(&body.name);
        let mut ports = Scope::default();
        for stmt in &body.stmts {
            let StmtKind::Port { name, ty } = &stmt.kind else {
                continue;
            };
            // Typed even when it's a second port of the name, for its own mistakes. A
            // port with a wrong type is still a port: its uses resolve (model.md E7).
            let signal = self.signal_type(ty);
            let port = PortId::new(block.block.ports.len());
            if ports.declare(name, port, NameKind::Port, &mut self.errors) {
                block.push_port(name, signal, ty.span);
            }
        }
        (ports, block)
    }

    /// Each of the `blocks` blocks' contract: the `contract` of its name.
    fn match_contracts(&mut self, blocks: usize) -> Vec<Option<Contract>> {
        let mut contracts = vec![None; blocks];
        let mut seen = Scope::default();
        for item in &self.parsed.file.items {
            let ItemKind::Contract(body) = &item.kind else {
                continue;
            };
            if !seen.declare(&body.name, (), NameKind::Contract, &mut self.errors) {
                continue;
            }
            match self.blocks.get(body.name.text) {
                Some(block) => contracts[block.index()] = Some(Contract {}),
                None => {
                    let kind = ResolveErrorKind::ContractWithoutBlock {
                        name: body.name.text.to_string(),
                    };
                    self.error(kind, body.name.span);
                }
            }
        }
        contracts
    }

    /// A port's type: `Pin`, `Power<In>`. `None` if it's wrong, reported.
    fn signal_type(&mut self, ty: &ast::Type) -> Option<SignalType> {
        let result = self.try_signal_type(ty);
        result.map_err(|e| self.errors.push(e)).ok()
    }

    #[expect(
        clippy::result_large_err,
        reason = "cold: the error is reported right away"
    )]
    fn try_signal_type(&self, ty: &ast::Type) -> Result<SignalType, ResolveError> {
        let name = self.path_text(&ty.path);
        if !SignalType::is_name(name) {
            // `port v: Resistor;`: say what it is instead of "isn't a port type".
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
            return Err(ResolveError::new(kind, ty.path.span));
        }
        let role = match ty.args.as_slice() {
            [] => Ok(None),
            [arg] if arg.args.is_empty() => Role::from_name(self.path_text(&arg.path))
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
        Err(ResolveError::new(kind, ty.span))
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
    use crate::diagnostic::DiagKind;
    use crate::parser::parse;
    use crate::testing::{
        assert_every_kind_has_a_case, dump_resolve, file_name, read, resolve_errors,
    };

    /// `ok/` cases: no problems at any stage, and a snapshot of the design.
    #[test]
    fn ok_cases() {
        insta::glob!("../../test_data/resolve", "ok/*.spl", |path| {
            let src = read(path);
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
                .all(|i| i.pins.iter().all(Option::is_some))
        );
        assert_eq!(design.contracts, [Some(Contract {})]);
        insta::assert_snapshot!(dump_resolve("ce_amp.spl", src));
    }

    /// Statement order inside a block doesn't change what it means (model.md E4): every
    /// order of `ce_amp.spl`'s block statements resolves to the same parts and bindings.
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
        let src = "block B {\n    port v: Pin;\n    port g: Ground;\n    net x = [v];\n    net x = [g];\n    let r1 = Resistor { a: v, b: g, value: 1k };\n    let r1 = Resistor { a: g, b: v, value: 2k, bad: v };\n}\n";
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
        let mut src = String::from("block B {\n    port v: Pin;\n");
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

    /// A second block of a name is resolved for its own errors, after the design's
    /// blocks, then dropped: the design, its source map and its contracts hold the
    /// first only, and placements bind the first's ports.
    #[test]
    fn a_second_block_is_checked_then_dropped() {
        let src = "block A {\n    port p: Pin;\n}\nblock A {\n    port q: Bus;\n    let r = Resistor { a: q, b: nowhere, value: 1k };\n}\nblock B {\n    port x: Pin;\n    let a = A { p: x };\n}\ncontract A {}\n";
        let resolved = resolve(&parse(src));
        let design = &resolved.design;
        let names: Vec<&str> = design.blocks.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["A", "B"]);
        assert_eq!(resolved.source_map.blocks.len(), 2);
        assert_eq!(design.contracts, [Some(Contract {}), None]);
        let a = &design.blocks[1].instances[0];
        assert_eq!(a.of, InstanceOf::Block(BlockId::new(0)));
        assert_eq!(a.pins, [Some(NetId::new(0))]);
        let errors: Vec<_> = resolved.errors.iter().map(|e| e.kind.name()).collect();
        assert_eq!(errors, ["Duplicate", "BadSignalType", "UnknownName"]);
    }

    /// `C { a }` where `a` is an instance: the shorthand names pin `a`, so the error
    /// says which pin, as `a: r1` does.
    #[test]
    fn shorthand_bound_to_an_instance_names_its_pin() {
        let src = "block A {\n    port b: Pin;\n    let a = Resistor { a, b, value: 1k };\n}\n";
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
        let mut src = String::from("block B {\n    port base: Pin;\n");
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
