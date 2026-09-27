//! Test support shared by the test suites and the fuzz targets (lexer.md §7, ast.md §5).

use std::collections::BTreeSet;
use std::fmt::Write;

use codespan_reporting::diagnostic::Diagnostic;
use spicy_model::span::Span;

use crate::diagnostic::{Diag, DiagKind, render_plain};

use crate::lexer::{LexError, TokenKind, check, scan};
use crate::parser::ast::{
    Attribute, Expr, ExprKind, File, Item, ItemKind, Path, Relation, Stmt, StmtKind, Type,
};
use crate::parser::parse;

// --- Shared by the test suites ----------------------------------------------------

pub fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn file_name(path: &std::path::Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

/// The coverage rule (lexer.md §7.1, ast.md §5): every name in `all` is among the error
/// names that `names` gives for some `.spl` file in `test_data/<dir>`.
pub fn assert_every_kind_has_a_case(
    dir: &str,
    all: &[&str],
    names: impl Fn(&str) -> Vec<&'static str>,
) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test_data")
        .join(dir);
    let mut seen = BTreeSet::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "spl") {
            seen.extend(names(&read(&path)));
        }
    }
    let missing: Vec<_> = all.iter().filter(|name| !seen.contains(*name)).collect();
    assert!(
        missing.is_empty(),
        "error kinds without an err/ case: {missing:?}"
    );
}

/// A small deterministic generator (xorshift64), so the property tests need no crate.
pub struct Rng(pub u64);

impl Rng {
    #[allow(clippy::should_implement_trait)] // not an `Iterator`: it never ends
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number in `0..n`.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// --- Lexer -------------------------------------------------------------------------

/// A span is ordered, inside `src`, and on character boundaries.
fn check_span(src: &str, span: Span, what: &str) {
    assert!(
        span.start <= span.end && span.end as usize <= src.len(),
        "{what} span {span:?} inside the source"
    );
    assert!(
        src.is_char_boundary(span.start as usize) && src.is_char_boundary(span.end as usize),
        "{what} span {span:?} on char boundaries"
    );
}

/// A problem's spans (its own, its fix's, its related one) lie inside the input, and it
/// renders. The same check for every stage.
fn check_problem<K: DiagKind>(src: &str, e: &Diag<K>) {
    check_span(src, e.span, "problem");
    if let Some(fix) = &e.fix {
        check_span(src, fix.span, "fix");
    }
    if let Some(related) = e.related {
        check_span(src, related, "related");
    }
    let _ = e.diagnostic();
}

/// One problem, for a snapshot: `code Name start..end "text"`, then its fix and related
/// span if it has them. The same line for every stage, so a lexer error reads alike in
/// the lexer's, the parser's and resolve's snapshots.
fn problem<K: DiagKind>(src: &str, e: &Diag<K>) -> (Span, String, Diagnostic<()>) {
    let (code, name) = (e.kind.code(), e.kind.name());
    let mut line = format!(
        "{code} {name} {}..{} {:?}",
        e.span.start,
        e.span.end,
        &src[e.span.range()]
    );
    if let Some(f) = &e.fix {
        let _ = write!(
            line,
            " fix {}..{} {:?}",
            f.span.start, f.span.end, f.replacement
        );
    }
    if let Some(r) = e.related {
        let _ = write!(line, " related {}..{}", r.start, r.end);
    }
    (e.span, line, e.diagnostic())
}

/// The errors section of a snapshot: every problem in source order, as data, then as
/// the user sees them. Empty when there are none.
fn problems_section(
    file_name: &str,
    src: &str,
    mut problems: Vec<(Span, String, Diagnostic<()>)>,
) -> String {
    if problems.is_empty() {
        return String::new();
    }
    problems.sort_by_key(|(span, _, _)| *span);
    let mut out = String::from("\n--- errors ---\n");
    for (_, line, _) in &problems {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("\n--- rendered ---\n");
    let diagnostics: Vec<Diagnostic<()>> = problems.into_iter().map(|(_, _, d)| d).collect();
    out.push_str(&render_plain(file_name, src, &diagnostics));
    out
}

/// Checks the lexer invariants of lexer.md §7.3 on `src`; panics with a description of
/// the first one that fails.
pub fn check_invariants(src: &str) {
    let tokens = scan(src);
    let errors = check(&tokens);

    // Tokens tile the input, end with one zero-length Eof, and only Eof is empty.
    let n = tokens.len();
    assert!(n >= 1, "at least the Eof token");
    assert_eq!(tokens.kind(n - 1), TokenKind::Eof, "last token is Eof");
    assert_eq!(tokens.span(0).start, 0, "first token starts at 0");
    for i in 0..n {
        let span = tokens.span(i);
        assert!(
            src.is_char_boundary(span.start as usize),
            "token {i} starts on a char boundary"
        );
        if i + 1 < n {
            assert_ne!(tokens.kind(i), TokenKind::Eof, "Eof only at the end");
            assert!(
                !span.is_empty(),
                "token {i} ({:?}) is empty",
                tokens.kind(i)
            );
            assert_eq!(span.end, tokens.span(i + 1).start, "tokens are contiguous");
        } else {
            assert!(span.is_empty(), "Eof is empty");
            assert_eq!(span.start as usize, src.len(), "Eof is at the end");
        }
    }

    // Round trip: joining the tokens gives back the input.
    let joined: String = (0..n).map(|i| tokens.text(i)).collect();
    assert_eq!(joined, src, "tokens rebuild the source");

    // Errors: inside the input, on char boundaries, non-empty, and not overlapping.
    for (k, e) in errors.iter().enumerate() {
        check_problem(src, e);
        assert!(!e.span.is_empty(), "error span is non-empty: {e:?}");
        if let Some(next) = errors.get(k + 1) {
            assert!(e.span.end <= next.span.start, "errors don't overlap");
        }
    }
    let _ = render_plain("input.spl", src, &diagnostics(&errors));

    // Appending a newline changes only the last significant token (and adds trivia).
    let with_newline = format!("{src}\n");
    let longer = scan(&with_newline);
    let last = n - 1; // Eof
    for i in 0..last.saturating_sub(1) {
        assert_eq!(
            (tokens.kind(i), tokens.span(i)),
            (longer.kind(i), longer.span(i)),
            "token {i} unchanged by a trailing newline"
        );
    }
    if last >= 1 {
        let i = last - 1;
        assert_eq!(tokens.kind(i), longer.kind(i), "last token keeps its kind");
        assert_eq!(
            tokens.span(i).start,
            longer.span(i).start,
            "last token keeps its start"
        );
    }
}

pub fn diagnostics(errors: &[LexError]) -> Vec<Diagnostic<()>> {
    errors.iter().map(LexError::diagnostic).collect()
}

/// The token dump used by the case-file snapshots: one `Kind "text"` line per token
/// (rust-analyzer's format), then the errors as data, then as the user sees them.
pub fn dump(file_name: &str, src: &str) -> String {
    let tokens = scan(src);
    let errors = check(&tokens);
    let mut out = String::new();
    for i in 0..tokens.len() {
        let _ = writeln!(out, "{:?} {:?}", tokens.kind(i), tokens.text(i));
    }
    let problems = errors.iter().map(|e| problem(src, e)).collect();
    out.push_str(&problems_section(file_name, src, problems));
    out
}

// --- Parser ------------------------------------------------------------------------

/// Checks the parser invariants of ast.md §5 on `src`; panics on the first one that
/// fails. Includes the lexer's.
pub fn check_parse_invariants(src: &str) {
    check_invariants(src);
    let parsed = parse(src);
    check_resolve_invariants(src, &parsed);
    let len = src.len() as u32;
    assert_eq!(
        parsed.file.span,
        Span::new(0, len),
        "the file spans the whole input"
    );
    // Printing the tree checks every node's span (see `Printer`).
    Printer::new(src).file(&parsed.file);
    for e in &parsed.errors {
        check_problem(src, e);
    }
    let _ = render_plain("input.spl", src, &parsed.diagnostics());
}

/// The tree dump used by the parser's case-file snapshots: one line per node with its
/// span (so spans are tested, unlike Spade's tests), the text of leaves and errors, then
/// the errors as data and as the user sees them.
pub fn dump_parse(file_name: &str, src: &str) -> String {
    let parsed = parse(src);
    let mut p = Printer::new(src);
    p.file(&parsed.file);
    let mut out = p.out;
    let lex = parsed.lex_errors.iter().map(|e| problem(src, e));
    let problems = lex
        .chain(parsed.errors.iter().map(|e| problem(src, e)))
        .collect();
    out.push_str(&problems_section(file_name, src, problems));
    out
}

/// Prints the tree, one line per node, and checks every node's span on the way: well
/// formed, and inside its parent's. Nodes without a line of their own (names, paths) are
/// checked too, and items and statements come in order without overlapping.
struct Printer<'a> {
    src: &'a str,
    out: String,
    /// The spans of the nodes whose children are being printed, outermost first. Its
    /// length is the indent.
    parents: Vec<Span>,
    /// The span of the last line printed: the parent of what `nested` prints.
    last: Span,
}

impl<'a> Printer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            out: String::new(),
            parents: Vec::new(),
            last: Span::new(0, 0),
        }
    }

    /// Checks a span that belongs inside `parent`.
    fn check(&self, span: Span, parent: Span) {
        check_span(self.src, span, "node");
        assert!(
            parent.start <= span.start && span.end <= parent.end,
            "span {span:?} inside its parent {parent:?}"
        );
    }

    /// Checks a path and its segments, inside `parent`.
    fn check_path(&self, path: &Path, parent: Span) {
        self.check(path.span, parent);
        for seg in &path.segments {
            self.check(seg.span, path.span);
        }
    }

    fn write(&mut self, label: &str, span: Span, text: Option<&str>) {
        if let Some(&parent) = self.parents.last() {
            self.check(span, parent);
        }
        self.last = span;
        let _ = write!(
            self.out,
            "{:indent$}{label} {}..{}",
            "",
            span.start,
            span.end,
            indent = self.parents.len() * 2
        );
        if let Some(text) = text {
            let _ = write!(self.out, " {text:?}");
        }
        self.out.push('\n');
    }

    fn line(&mut self, label: &str, span: Span) {
        self.write(label, span, None);
    }

    /// A line that also shows the node's text (leaves and errors).
    fn leaf(&mut self, label: &str, span: Span) {
        let src = self.src;
        self.write(label, span, Some(&src[span.range()]));
    }

    /// Prints `f`'s lines as children of the last line.
    fn nested(&mut self, f: impl FnOnce(&mut Self)) {
        self.parents.push(self.last);
        f(self);
        self.parents.pop();
    }

    fn file(&mut self, file: &File) {
        self.line("File", file.span);
        siblings(file.items.iter().map(|i| i.span));
        self.nested(|p| {
            for item in &file.items {
                p.item(item);
            }
        });
    }

    fn docs_attrs(&mut self, docs: &[Span], attrs: &[Attribute]) {
        for &d in docs {
            self.leaf("Doc", d);
        }
        for a in attrs {
            self.line(&format!("Attr {}", path_text(&a.path)), a.span);
            self.check_path(&a.path, a.span);
            self.nested(|p| {
                for e in a.args.iter().flatten() {
                    p.expr(e);
                }
            });
        }
    }

    fn item(&mut self, item: &Item) {
        let (label, body) = match &item.kind {
            ItemKind::Block(b) => ("Block", b),
            ItemKind::Contract(b) => ("Contract", b),
            ItemKind::Error => {
                self.leaf("ItemError", item.span);
                return;
            }
        };
        self.line(&format!("{label} {}", body.name.text), item.span);
        self.check(body.name.span, item.span);
        siblings(body.stmts.iter().map(|s| s.span));
        self.nested(|p| {
            p.docs_attrs(&item.docs, &item.attrs);
            for stmt in &body.stmts {
                p.stmt(stmt);
            }
        });
    }

    fn stmt(&mut self, stmt: &Stmt) {
        let (label, name) = match &stmt.kind {
            StmtKind::Port { name, .. } => (format!("Port {}", name.text), Some(name)),
            StmtKind::Net { name, .. } => (format!("Net {}", name.text), Some(name)),
            StmtKind::Let { name, .. } => (format!("Let {}", name.text), Some(name)),
            StmtKind::Assume { relation } => (format!("Assume {:?}", relation.op), None),
            StmtKind::Spec { name, relation } => {
                (format!("Spec {} {:?}", name.text, relation.op), Some(name))
            }
            StmtKind::Error => {
                self.leaf("StmtError", stmt.span);
                return;
            }
        };
        self.line(&label, stmt.span);
        if let Some(name) = name {
            self.check(name.span, stmt.span);
        }
        self.nested(|p| {
            p.docs_attrs(&stmt.docs, &stmt.attrs);
            match &stmt.kind {
                StmtKind::Port { ty, .. } => p.ty(ty),
                StmtKind::Net { merge, .. } => {
                    if let Some(e) = merge {
                        p.expr(e);
                    }
                }
                StmtKind::Let { value, .. } => p.expr(value),
                StmtKind::Assume { relation } | StmtKind::Spec { relation, .. } => {
                    p.relation(relation);
                }
                StmtKind::Error => {}
            }
        });
    }

    fn relation(&mut self, r: &Relation) {
        self.expr(&r.lhs);
        self.expr(&r.rhs);
    }

    fn ty(&mut self, ty: &Type) {
        self.leaf(&format!("Type {}", path_text(&ty.path)), ty.span);
        self.check_path(&ty.path, ty.span);
        self.nested(|p| {
            for arg in &ty.args {
                p.ty(arg);
            }
        });
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Quantity(q) => {
                let unit = q
                    .unit
                    .map(|u| format!(" {}", u.symbol()))
                    .unwrap_or_default();
                self.leaf(&format!("Quantity {:e}{unit}", q.value), e.span);
            }
            ExprKind::Path(path) => {
                self.leaf(&format!("Path {}", path_text(path)), e.span);
                self.check_path(path, e.span);
            }
            ExprKind::Error => self.leaf("ExprError", e.span),
            ExprKind::StructLit { path, fields } => {
                self.line(&format!("StructLit {}", path_text(path)), e.span);
                self.check_path(path, e.span);
                self.nested(|p| {
                    for f in fields {
                        let shorthand = if f.value.is_none() {
                            " (shorthand)"
                        } else {
                            ""
                        };
                        p.line(&format!("Field {}{shorthand}", f.name.text), f.span);
                        p.check(f.name.span, f.span);
                        if let Some(v) = &f.value {
                            p.nested(|p| p.expr(v));
                        }
                    }
                });
            }
            ExprKind::Paren(inner) => {
                self.line("Paren", e.span);
                self.nested(|p| p.expr(inner));
            }
            ExprKind::Neg(inner) => {
                self.line("Neg", e.span);
                self.nested(|p| p.expr(inner));
            }
            ExprKind::Array(items) => {
                self.line("Array", e.span);
                self.nested(|p| {
                    for item in items {
                        p.expr(item);
                    }
                });
            }
            ExprKind::Field { base, name } => {
                self.line(&format!("Field .{}", name.text), e.span);
                self.check(name.span, e.span);
                self.nested(|p| p.expr(base));
            }
            ExprKind::Call { callee, args } => {
                self.line("Call", e.span);
                self.nested(|p| {
                    p.expr(callee);
                    for a in args {
                        p.expr(a);
                    }
                });
            }
            ExprKind::Binary { op, lhs, rhs } => {
                self.line(&format!("Binary {op:?}"), e.span);
                self.nested(|p| {
                    p.expr(lhs);
                    p.expr(rhs);
                });
            }
        }
    }
}

/// Siblings come in order, without overlapping.
fn siblings(spans: impl Iterator<Item = Span>) {
    let mut prev_end = 0;
    for s in spans {
        assert!(s.start >= prev_end, "siblings in order, without overlap");
        prev_end = s.end;
    }
}

fn path_text(path: &Path) -> String {
    let names: Vec<&str> = path.segments.iter().map(|s| s.text).collect();
    names.join("::")
}

// --- Resolve -----------------------------------------------------------------------

use spicy_model::design::{Design, DesignSourceMap, FieldValue, InstanceOf};

use crate::resolve::{ResolveError, resolve};

/// The `Design` dump used by the resolve case-file snapshots: every block, port, net
/// and instance with the span of its name (so the source map is tested too), pins with
/// the net they're bound to, fields with their typed values; then every problem from
/// lexing, parsing and resolving, as data and as the user sees them.
pub fn dump_resolve(file_name: &str, src: &str) -> String {
    let parsed = parse(src);
    let resolved = resolve(&parsed);
    let mut out = dump_design(&resolved.design, &resolved.source_map);
    let problems = (parsed.lex_errors.iter().map(|e| problem(src, e)))
        .chain(parsed.errors.iter().map(|e| problem(src, e)))
        .chain(resolved.errors.iter().map(|e| problem(src, e)))
        .collect();
    out.push_str(&problems_section(file_name, src, problems));
    out
}

/// Every resolve error of `src`, for the coverage rule and the fix test.
pub fn resolve_errors(src: &str) -> Vec<ResolveError> {
    resolve(&parse(src)).errors
}

fn dump_design(design: &Design, map: &DesignSourceMap) -> String {
    let at = |s: Span| format!("{}..{}", s.start, s.end);
    let mut out = String::new();
    for (b, block) in design.blocks.iter().enumerate() {
        let spans = &map.blocks[b];
        let _ = writeln!(out, "block {} {}", block.name, at(spans.name));
        for (p, (name, port)) in block.port_names().zip(&block.ports).enumerate() {
            let signal = port.signal.map_or("<error>".to_string(), |s| s.to_string());
            let _ = writeln!(out, "  port {name}: {signal} {}", at(spans.nets[p]));
        }
        for (n, net) in block.nets.iter().enumerate().skip(block.ports.len()) {
            let _ = writeln!(out, "  net {} {}", net.name, at(spans.nets[n]));
        }
        for (i, inst) in block.instances.iter().enumerate() {
            let ispans = &spans.instances[i];
            let (kind, pins): (String, Vec<&str>) = match inst.of {
                InstanceOf::Part(k) => (k.name().to_string(), k.pins().to_vec()),
                InstanceOf::Block(id) => {
                    let b = design.block(id);
                    let ports = b.port_names().collect();
                    (format!("block {}", b.name), ports)
                }
                InstanceOf::Error => ("<error>".to_string(), Vec::new()),
            };
            let _ = writeln!(out, "  {} = {kind} {}", inst.name, at(ispans.name));
            for (pin, net) in pins.iter().zip(&inst.pins) {
                let net = net.map_or("<unbound>", |n| block.net(n).name.as_str());
                let _ = writeln!(out, "    {pin} → {net}");
            }
            if let InstanceOf::Part(k) = inst.of {
                for (field, value) in k.fields().iter().zip(&inst.fields) {
                    match value {
                        FieldValue::Unset => {}
                        FieldValue::Given(v) => {
                            let _ = writeln!(out, "    {} = {v}", field.name);
                        }
                        FieldValue::Invalid => {
                            let _ = writeln!(out, "    {} = <invalid>", field.name);
                        }
                    }
                }
            }
        }
        for merge in &block.merges {
            let with: Vec<&str> = merge
                .with
                .iter()
                .map(|n| block.net(*n).name.as_str())
                .collect();
            let _ = writeln!(
                out,
                "  merge {} = [{}]",
                block.net(merge.net).name,
                with.join(", ")
            );
        }
    }
    for (block, contract) in design.blocks.iter().zip(&design.contracts) {
        if contract.is_some() {
            let _ = writeln!(out, "contract {}", block.name);
        }
    }
    out
}

/// Resolving any parsed input never panics, the source map matches the design entry
/// for entry, every span it records or reports lies inside the input, and every
/// binding's span lies inside its statement.
fn check_resolve_invariants(src: &str, parsed: &crate::parser::Parsed) {
    let resolved = resolve(parsed);
    let (design, map) = (&resolved.design, &resolved.source_map);
    assert_eq!(
        design.blocks.len(),
        map.blocks.len(),
        "one span table per block"
    );
    assert_eq!(
        design.blocks.len(),
        design.contracts.len(),
        "a contract slot per block"
    );
    let in_src = |s: Span, what: &str| {
        check_span(src, s, what);
    };
    let inside = |s: Span, outer: Span, what: &str| {
        assert!(
            outer.start <= s.start && s.end <= outer.end,
            "{what} {s:?} inside its statement {outer:?}"
        );
    };
    for (block, spans) in design.blocks.iter().zip(&map.blocks) {
        in_src(spans.name, "block name");
        assert_eq!(
            block.ports.len(),
            spans.port_types.len(),
            "a type span per port"
        );
        assert_eq!(block.nets.len(), spans.nets.len(), "a span per net");
        assert!(block.nets.len() >= block.ports.len(), "a net per port");
        assert_eq!(
            block.instances.len(),
            spans.instances.len(),
            "spans per instance"
        );
        assert_eq!(block.merges.len(), spans.merges.len(), "spans per merge");
        spans
            .port_types
            .iter()
            .chain(&spans.nets)
            .for_each(|&s| in_src(s, "port or net"));
        for (merge, mspans) in block.merges.iter().zip(&spans.merges) {
            in_src(mspans.stmt, "merge");
            assert_eq!(
                merge.with.len(),
                mspans.items.len(),
                "a span per merged net"
            );
            for &s in &mspans.items {
                inside(s, mspans.stmt, "merged net");
            }
            for net in merge.with.iter().chain([&merge.net]) {
                assert!(
                    net.index() < block.nets.len(),
                    "merges name the block's nets"
                );
            }
        }
        for (inst, ispans) in block.instances.iter().zip(&spans.instances) {
            in_src(ispans.stmt, "let");
            inside(ispans.name, ispans.stmt, "instance name");
            let (n_pins, n_fields) = match inst.of {
                InstanceOf::Part(k) => (k.pins().len(), k.fields().len()),
                InstanceOf::Block(b) => (design.block(b).ports.len(), 0),
                InstanceOf::Error => (0, 0),
            };
            assert_eq!(inst.pins.len(), n_pins, "a slot per pin");
            assert_eq!(inst.fields.len(), n_fields, "a slot per field");
            assert_eq!(inst.pins.len(), ispans.pins.len(), "a span slot per pin");
            assert_eq!(
                inst.fields.len(),
                ispans.fields.len(),
                "a span slot per field"
            );
            if inst.of != InstanceOf::Error {
                inside(ispans.kind, ispans.stmt, "instance kind");
            }
            for s in ispans.pins.iter().chain(&ispans.fields).flatten() {
                inside(*s, ispans.stmt, "binding");
            }
            for (net, s) in inst.pins.iter().zip(&ispans.pins) {
                if let Some(net) = net {
                    assert!(
                        net.index() < block.nets.len(),
                        "pins point at the block's nets"
                    );
                    assert!(s.is_some(), "a bound pin has a span");
                }
            }
            for (value, s) in inst.fields.iter().zip(&ispans.fields) {
                assert_eq!(
                    *value == FieldValue::Unset,
                    s.is_none(),
                    "a field has a span exactly when it was written"
                );
            }
        }
    }
    for e in &resolved.errors {
        check_problem(src, e);
    }
}
