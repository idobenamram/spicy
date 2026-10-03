//! Test support shared by the test suites and the fuzz targets (lexer.md §7, ast.md §5).

use std::collections::BTreeSet;
use std::fmt::Write;

use codespan_reporting::diagnostic::Diagnostic;
use spicy_span::Span;

use spicy_errors::{Diag, DiagKind, Render, render_plain};

use crate::lexer::{LexError, TokenKind, check, scan};
use crate::parser::ast::{
    Arg, Attribute, BlockDecl, BlockEntry, Expr, ExprKind, File, Ident, Item, ItemKind, Path,
    Relation, SetupDecl, SetupEntry, Stmt, StmtKind, Type, TypeKind, ValueDecl,
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
    errors.iter().map(Render::diagnostic).collect()
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
    check_flatten_invariants(src, &parsed);
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
/// formed, inside its parent's, and after the sibling before it (siblings come in
/// order, without overlapping). Parts without a line of their own (names, paths and
/// their segments) are checked the same way, in their place among the siblings.
struct Printer<'a> {
    src: &'a str,
    out: String,
    /// The nodes whose children are being checked, outermost first. Its length is the
    /// indent.
    parents: Vec<Parent>,
}

/// A node whose children are being checked.
struct Parent {
    span: Span,
    /// Where the next child may start: the end of the last one checked.
    next: u32,
}

impl<'a> Printer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            out: String::new(),
            parents: Vec::new(),
        }
    }

    /// Checks the span of the next child of the current parent: well formed, inside
    /// the parent, and after the child before it.
    fn check(&mut self, span: Span) {
        check_span(self.src, span, "node");
        let Some(parent) = self.parents.last_mut() else {
            return;
        };
        assert!(
            parent.span.start <= span.start && span.end <= parent.span.end,
            "span {span:?} inside its parent {:?}",
            parent.span
        );
        assert!(
            parent.next <= span.start,
            "span {span:?} after its previous sibling, which ends at {}",
            parent.next
        );
        parent.next = span.end;
    }

    /// Runs `children` with `span` as the parent: what they check lies inside it, in
    /// order.
    fn inside(&mut self, span: Span, children: impl FnOnce(&mut Self)) {
        self.parents.push(Parent {
            span,
            next: span.start,
        });
        children(self);
        self.parents.pop();
    }

    /// A path and its segments, as the next child.
    fn path(&mut self, path: &Path) {
        self.segments(path.span, &path.segments);
    }

    /// A dotted or `::` name (a setup's key, a path) over `span`, and its segments inside
    /// it, as the next child.
    fn segments(&mut self, span: Span, segments: &[Ident]) {
        self.check(span);
        self.inside(span, |p| {
            for segment in segments {
                p.check(segment.span);
            }
        });
    }

    /// One line, as the next child: `label start..end`, then the text if there is one.
    fn write(&mut self, label: &str, span: Span, text: Option<&str>) {
        self.check(span);
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

    /// A node's line, then its children, indented under it.
    fn node(&mut self, label: &str, span: Span, children: impl FnOnce(&mut Self)) {
        self.write(label, span, None);
        self.inside(span, children);
    }

    /// A line that also shows the node's text (leaves and errors).
    fn leaf(&mut self, label: &str, span: Span) {
        let src = self.src;
        self.write(label, span, Some(&src[span.range()]));
    }

    fn file(&mut self, file: &File) {
        self.node("File", file.span, |p| {
            for item in &file.items {
                p.item(item);
            }
        });
    }

    /// The doc comments and attributes before an item or a statement, in source order:
    /// the tree keeps them in two lists, and they may interleave (`/// a #[x] /// b`).
    fn docs_attrs(&mut self, docs: &[Span], attrs: &[Attribute]) {
        let mut docs = docs.iter().copied().peekable();
        for a in attrs {
            while let Some(d) = docs.next_if(|d| d.start < a.span.start) {
                self.leaf("Doc", d);
            }
            self.node(&format!("Attr {}", path_text(&a.path)), a.span, |p| {
                p.path(&a.path);
                for e in a.args.iter().flatten() {
                    p.expr(e);
                }
            });
        }
        for d in docs {
            self.leaf("Doc", d);
        }
    }

    fn item(&mut self, item: &Item) {
        let (label, body) = match &item.kind {
            ItemKind::Block(decl) => return self.block_decl(item, decl),
            ItemKind::Setup(decl) => return self.setup_decl(item, decl),
            ItemKind::Env(decl) => return self.value_decl(item, "Env", decl),
            ItemKind::Const(decl) => return self.value_decl(item, "Const", decl),
            ItemKind::Circuit(b) => ("Circuit", b),
            ItemKind::Contract(b) => ("Contract", b),
            ItemKind::Error(_) => {
                self.leaf("ItemError", item.span);
                return;
            }
        };
        self.node(&format!("{label} {}", body.name.text), item.span, |p| {
            p.docs_attrs(&item.docs, &item.attrs);
            p.check(body.name.span);
            for stmt in &body.stmts {
                p.stmt(stmt);
            }
        });
    }

    fn block_decl(&mut self, item: &Item, decl: &BlockDecl) {
        let label = format!("{}Block {}", pub_prefix(decl.public), decl.name.text);
        self.node(&label, item.span, |p| {
            p.docs_attrs(&item.docs, &item.attrs);
            if let Some(public) = decl.public {
                p.check(public);
            }
            p.check(decl.name.span);
            for port in &decl.ports {
                let BlockEntry::Port { name, ty } = &port.kind else {
                    p.leaf("PortError", port.span);
                    continue;
                };
                p.node(&format!("Port {}", name.text), port.span, |p| {
                    p.docs_attrs(&port.docs, &port.attrs);
                    p.check(name.span);
                    p.ty(ty);
                });
            }
        });
    }

    fn setup_decl(&mut self, item: &Item, decl: &SetupDecl) {
        let label = format!("Setup {} for {}", decl.name.text, path_text(&decl.block));
        self.node(&label, item.span, |p| {
            p.docs_attrs(&item.docs, &item.attrs);
            p.check(decl.name.span);
            p.path(&decl.block);
            for entry in &decl.entries {
                let SetupEntry::Set { key, value } = &entry.kind else {
                    p.leaf("EntryError", entry.span);
                    continue;
                };
                let names: Vec<&str> = key.segments.iter().map(|s| s.text).collect();
                p.node(&format!("Set {}", names.join(".")), entry.span, |p| {
                    p.docs_attrs(&entry.docs, &entry.attrs);
                    p.segments(key.span, &key.segments);
                    p.expr(value);
                });
            }
        });
    }

    /// An `env` or `const`: a name, a type and a value.
    fn value_decl(&mut self, item: &Item, label: &str, decl: &ValueDecl) {
        self.node(&format!("{label} {}", decl.name.text), item.span, |p| {
            p.docs_attrs(&item.docs, &item.attrs);
            p.check(decl.name.span);
            p.ty(&decl.ty);
            p.expr(&decl.value);
        });
    }

    fn stmt(&mut self, stmt: &Stmt) {
        let (label, public, name) = match &stmt.kind {
            StmtKind::Net { name, .. } => (format!("Net {}", name.text), None, Some(name)),
            StmtKind::Let { name, .. } => (format!("Let {}", name.text), None, Some(name)),
            StmtKind::DefaultSetup { setup } => {
                (format!("Setup = {}", path_text(setup)), None, None)
            }
            StmtKind::Spec {
                public,
                name,
                relation,
            } => {
                let label = format!(
                    "{}Spec {} {:?}",
                    pub_prefix(*public),
                    name.text,
                    relation.op
                );
                (label, *public, Some(name))
            }
            StmtKind::Error(_) => {
                self.leaf("StmtError", stmt.span);
                return;
            }
        };
        self.node(&label, stmt.span, |p| {
            p.docs_attrs(&stmt.docs, &stmt.attrs);
            if let Some(public) = public {
                p.check(public);
            }
            if let Some(name) = name {
                p.check(name.span);
            }
            match &stmt.kind {
                StmtKind::Net { merge, .. } => {
                    if let Some(e) = merge {
                        p.expr(e);
                    }
                }
                StmtKind::Let { value, .. } => p.expr(value),
                StmtKind::DefaultSetup { setup } => p.path(setup),
                StmtKind::Spec { relation, .. } => p.relation(relation),
                StmtKind::Error(_) => {}
            }
        });
    }

    /// The two sides, as children of the statement: a relation has no span of its own.
    fn relation(&mut self, r: &Relation) {
        self.expr(&r.lhs);
        self.expr(&r.rhs);
    }

    fn ty(&mut self, ty: &Type) {
        let TypeKind::Path { path, args } = &ty.kind else {
            self.leaf("TypeError", ty.span);
            return;
        };
        self.leaf(&format!("Type {}", path_text(path)), ty.span);
        self.inside(ty.span, |p| {
            p.path(path);
            for arg in args {
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
                self.inside(e.span, |p| p.path(path));
            }
            ExprKind::Str(_) => self.leaf("Str", e.span),
            ExprKind::Error(_) => self.leaf("ExprError", e.span),
            ExprKind::StructLit(lit) => {
                let label = format!("StructLit {}", path_text(&lit.path));
                self.node(&label, e.span, |p| {
                    p.path(&lit.path);
                    for g in &lit.generics {
                        p.node(&format!("Generic {}", g.name.text), g.span, |p| {
                            p.check(g.name.span);
                            p.expr(&g.value);
                        });
                    }
                    for f in &lit.fields {
                        let shorthand = if f.value.is_none() {
                            " (shorthand)"
                        } else {
                            ""
                        };
                        let label = format!("Field {}{shorthand}", f.name.text);
                        p.node(&label, f.span, |p| {
                            p.check(f.name.span);
                            if let Some(v) = &f.value {
                                p.expr(v);
                            }
                        });
                    }
                });
            }
            ExprKind::Paren(inner) => self.node("Paren", e.span, |p| p.expr(inner)),
            ExprKind::Neg(inner) => self.node("Neg", e.span, |p| p.expr(inner)),
            ExprKind::Array(items) => self.node("Array", e.span, |p| {
                for item in items {
                    p.expr(item);
                }
            }),
            ExprKind::Field { base, name } => {
                self.node(&format!("Field .{}", name.text), e.span, |p| {
                    p.expr(base);
                    p.check(name.span);
                });
            }
            ExprKind::Call { callee, args } => self.node("Call", e.span, |p| {
                p.expr(callee);
                for a in args {
                    match a {
                        Arg::Positional(v) => p.expr(v),
                        Arg::Named { name, value, span } => {
                            p.node(&format!("Arg {}", name.text), *span, |p| {
                                p.check(name.span);
                                p.expr(value);
                            });
                        }
                    }
                }
            }),
            ExprKind::Index { base, index } => self.node("Index", e.span, |p| {
                p.expr(base);
                p.expr(index);
            }),
            ExprKind::Binary { op, lhs, rhs } => {
                self.node(&format!("Binary {op:?}"), e.span, |p| {
                    p.expr(lhs);
                    p.expr(rhs);
                })
            }
            // The label shows which ends it has.
            ExprKind::Range { lo, hi } => {
                let label = match (lo, hi) {
                    (Some(_), Some(_)) => "Range a..=b",
                    (None, _) => "Range ..=b",
                    (_, None) => "Range a..",
                };
                self.node(label, e.span, |p| {
                    for end in lo.iter().chain(hi) {
                        p.expr(end);
                    }
                });
            }
        }
    }
}

/// `"pub "` before a label, for what's written `pub`.
fn pub_prefix(public: Option<Span>) -> &'static str {
    if public.is_some() { "pub " } else { "" }
}

fn path_text(path: &Path) -> String {
    let names: Vec<&str> = path.segments.iter().map(|s| s.text).collect();
    names.join("::")
}

// --- Resolve -----------------------------------------------------------------------

use spicy_model::design::{
    Block, Design, DesignSourceMap, FieldValue, InstanceOf, Measure, PortId, Temp,
};
use spicy_model::measure::{ArithOp, MExpr, MeasureType, Method, Reference};
use spicy_model::units::QKind;

use spicy_model::flat::{
    Flat, FlatDeviceId, FlatField, FlatInstanceId, FlatNetId, KnobId, KnobSource,
};
use spicy_model::flatten::{FlattenError, FlattenProblem, check_simulation};
use spicy_model::prelude::{Shape, SignalType};

use crate::elaborate::{Elaborated, elaborate};
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

/// The flat dump used by the flatten case-file snapshots: each root's nets (name, then
/// aliases, and whether it's a ground net), devices (pins on nets, fields exact or
/// knobs) and knobs; then every problem from every stage, with every root simulated, as
/// data and as the user sees them.
pub fn dump_elaborate(file_name: &str, src: &str) -> String {
    let parsed = parse(src);
    let elaborated = elaborate(&parsed);
    let mut out = String::new();
    for flat in elaborated.roots() {
        out.push_str(&dump_flat(flat));
    }
    let problems = (parsed.lex_errors.iter().map(|e| problem(src, e)))
        .chain(parsed.errors.iter().map(|e| problem(src, e)))
        .chain(elaborated.resolved.errors.iter().map(|e| problem(src, e)))
        .chain(
            flatten_problems(&elaborated)
                .iter()
                .map(|e| problem(src, e)),
        )
        .collect();
    out.push_str(&problems_section(file_name, src, problems));
    out
}

/// Every flatten error of `src`, with every root simulated, for the coverage rule.
pub fn flatten_errors(src: &str) -> Vec<FlattenError> {
    flatten_problems(&elaborate(&parse(src)))
}

/// Flatten's problems, then each root's simulation checks' (`check_simulation`), as if
/// every root were simulated.
fn flatten_problems(elaborated: &Elaborated) -> Vec<FlattenError> {
    let mut errors = elaborated.flattened.errors.clone();
    for flat in elaborated.roots() {
        let _ = check_simulation(flat, &elaborated.resolved.source_map, &mut errors);
    }
    errors
}

/// One root's part of [`dump_elaborate`]: its nets, devices and knobs, by path.
pub fn dump_flat(flat: Flat) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "root {}", flat.design.block(flat.data.root()).name);
    let net = |n: FlatNetId| flat.net_path(n).to_string();
    for (id, n) in flat.data.nets.iter().enumerate() {
        let aliases: Vec<String> = n.names[1..]
            .iter()
            .map(|&a| flat.local_path(a).to_string())
            .collect();
        let ground = if flat.data.grounds.contains(&FlatNetId::new(id)) {
            " (ground)"
        } else {
            ""
        };
        let _ = write!(out, "  net {}{ground}", net(FlatNetId::new(id)));
        if !aliases.is_empty() {
            let _ = write!(out, "  aliases {}", aliases.join(", "));
        }
        out.push('\n');
    }
    for (id, device) in flat.data.devices.iter().enumerate() {
        let pins: Vec<String> = device
            .kind
            .pins()
            .iter()
            .zip(&device.pins)
            .map(|(pin, n)| format!("{pin}→{}", n.map_or("<unbound>".to_string(), net)))
            .collect();
        let _ = write!(
            out,
            "  {} {} {}",
            device.kind.name(),
            flat.device_path(FlatDeviceId::new(id)),
            pins.join(" ")
        );
        for (field, value) in device.kind.fields().iter().zip(&device.fields) {
            match value {
                FlatField::Unset => {}
                FlatField::Exact(q) => {
                    let _ = write!(out, " {}={q}", field.name);
                }
                FlatField::Knob(k) => {
                    let _ = write!(out, " {}=knob {}", field.name, flat.knob_path(*k));
                }
                FlatField::Invalid(_) => {
                    let _ = write!(out, " {}=<invalid>", field.name);
                }
            }
        }
        out.push('\n');
    }
    for (id, knob) in flat.data.knobs.iter().enumerate() {
        let path = flat.knob_path(KnobId::new(id));
        let _ = writeln!(out, "  knob {path} {:?} {}", knob.kind, knob.value);
    }
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
                InstanceOf::Error(_) => ("<error>".to_string(), Vec::new()),
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
                        FieldValue::Invalid(_) => {
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
        let Some(contract) = contract else {
            continue;
        };
        let _ = writeln!(out, "contract {}", block.name);
        let setup = match contract.default_setup {
            Ok(id) => design.setups[id.index()].name.as_str(),
            Err(_) => "<invalid>",
        };
        let _ = writeln!(out, "  setup = {setup}");
        for measure in &contract.measures {
            match &measure.value {
                Ok((expr, ty)) => {
                    let text = measure_text(expr, block, &contract.measures);
                    let _ = writeln!(out, "  let {}: {} = {text}", measure.name, type_text(*ty));
                }
                Err(_) => {
                    let _ = writeln!(out, "  let {} = <invalid>", measure.name);
                }
            }
        }
    }
    let invalid = |_| "<invalid>".to_string();
    for (env, span) in design.envs.iter().zip(&map.envs) {
        let value = env.value.map_or_else(invalid, |v| v.to_string());
        let _ = writeln!(out, "env {} {} = {value}", env.name, at(*span));
    }
    for (c, span) in design.consts.iter().zip(&map.consts) {
        let value = c.value.map_or_else(invalid, |q| q.to_string());
        let _ = writeln!(out, "const {} {} = {value}", c.name, at(*span));
    }
    for (setup, spans) in design.setups.iter().zip(&map.setups) {
        let block = design.block(setup.block);
        let _ = writeln!(
            out,
            "setup {} {} for {}",
            setup.name,
            at(spans.name),
            block.name
        );
        for (p, port) in setup.ports.iter().enumerate() {
            let Some(port) = port else {
                continue;
            };
            let mut line = format!(
                "  {}: {}",
                block.port_name(PortId::new(p)),
                port.shape.name()
            );
            for (field, value) in port.shape.fields().iter().zip(&port.fields) {
                match value {
                    FieldValue::Unset => {}
                    FieldValue::Given(v) => {
                        let _ = write!(line, " {} = {v}", field.name);
                    }
                    FieldValue::Invalid(_) => {
                        let _ = write!(line, " {} = <invalid>", field.name);
                    }
                }
            }
            let _ = writeln!(out, "{line}");
        }
        let temp = match setup.temp {
            Ok(Temp::Env(env)) => design.envs[env.index()].name.clone(),
            Ok(Temp::Value(v)) => v.to_string(),
            Err(_) => "<invalid>".to_string(),
        };
        let _ = writeln!(out, "  temp = {temp}");
    }
    out
}

/// A measure as written, with the block's and contract's names: `ac(output.v / input.v)`,
/// `h.at(1000 Hz).mag()`. Arithmetic is parenthesized, so the dump shows how it grouped.
fn measure_text(expr: &MExpr, block: &Block, measures: &[Measure]) -> String {
    let text = |e: &MExpr| measure_text(e, block, measures);
    match expr {
        MExpr::Const(q) => q.to_string(),
        MExpr::Voltage(net) => format!("{}.v", block.net(*net).name),
        // A binary's parentheses are the call's: `dc(vcc.v - output.v)`.
        MExpr::Dc(x) if matches!(**x, MExpr::Binary { .. }) => format!("dc{}", text(x)),
        MExpr::Dc(x) => format!("dc({})", text(x)),
        MExpr::Ac { out, input } => {
            format!(
                "ac({}.v / {}.v)",
                block.net(*out).name,
                block.port_name(*input)
            )
        }
        MExpr::Measure(id) => measures[id.index()].name.clone(),
        MExpr::Method { method, of } => {
            let args = match method {
                Method::At { hz } => format!("{hz} Hz"),
                Method::Mag | Method::Db => String::new(),
                Method::FLow { db } => format!("{db} dB"),
                Method::FHigh { db, reference } => match reference {
                    Reference::Peak => format!("{db} dB"),
                    Reference::Dc => format!("{db} dB, ref: dc"),
                },
            };
            format!("{}.{}({args})", text(of), method.name())
        }
        MExpr::Neg(x) => format!("-{}", text(x)),
        MExpr::Binary { op, lhs, rhs } => {
            let op = match op {
                ArithOp::Add => "+",
                ArithOp::Sub => "-",
                ArithOp::Mul => "*",
                ArithOp::Div => "/",
            };
            format!("({} {op} {})", text(lhs), text(rhs))
        }
    }
}

/// A measure's type, for the dump: `response`, `number V`, `level dB`.
fn type_text(ty: MeasureType) -> String {
    match ty {
        MeasureType::Probe(_) => "probe".to_string(),
        MeasureType::Response(_) => "response".to_string(),
        MeasureType::Phasor(_) => "phasor".to_string(),
        MeasureType::Number {
            kind: QKind::Db, ..
        } => "level dB".to_string(),
        MeasureType::Number { dim, .. } if dim.is_none() => "number".to_string(),
        MeasureType::Number { dim, .. } => format!("number {}", dim.symbol()),
    }
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
    assert_eq!(
        map.contracts.len(),
        design.contracts.len(),
        "a span slot per contract"
    );
    for (b, (contract, spans)) in design.contracts.iter().zip(&map.contracts).enumerate() {
        let (Some(contract), Some(spans)) = (contract, spans) else {
            assert!(
                contract.is_none() && spans.is_none(),
                "a contract's spans match it"
            );
            continue;
        };
        check_span(src, spans.name, "contract name");
        spans
            .default_setup
            .iter()
            .for_each(|&s| check_span(src, s, "default setup"));
        if let Ok(id) = contract.default_setup {
            assert_eq!(
                design.setups[id.index()].block.index(),
                b,
                "the default setup is the block's"
            );
        }
        assert_eq!(
            contract.measures.len(),
            spans.measures.len(),
            "a span per measure"
        );
        for (measure, &at) in contract.measures.iter().zip(&spans.measures) {
            check_span(src, at, "measure name");
            assert_eq!(
                &src[at.range()],
                measure.name,
                "a measure's span is its name"
            );
        }
    }
    let in_src = |s: Span, what: &str| {
        check_span(src, s, what);
    };
    // The file's values: each at its own name, and a name at most once (a second
    // definition is dropped).
    assert_eq!(design.envs.len(), map.envs.len(), "a span per env");
    assert_eq!(design.consts.len(), map.consts.len(), "a span per const");
    let envs = design.envs.iter().map(|e| &e.name).zip(&map.envs);
    let consts = design.consts.iter().map(|c| &c.name).zip(&map.consts);
    let mut names = std::collections::HashSet::new();
    for (name, &s) in envs.chain(consts) {
        in_src(s, "env or const name");
        assert_eq!(
            &src[s.range()],
            name,
            "an env's or const's span is its name"
        );
        assert!(names.insert(name), "`{name}` is defined once in the design");
    }
    // The setups: each at its own name, once per block, a slot per port and per field
    // of its shape, with spans alike, and every placeholder taints its setup.
    assert_eq!(
        design.setups.len(),
        map.setups.len(),
        "a span table per setup"
    );
    let mut setups = std::collections::HashSet::new();
    for (setup, spans) in design.setups.iter().zip(&map.setups) {
        in_src(spans.name, "setup name");
        assert_eq!(
            &src[spans.name.range()],
            setup.name,
            "a setup's span is its name"
        );
        let once = setups.insert((setup.block, &setup.name));
        assert!(once, "setup `{}` is defined once per block", setup.name);
        let block = design.block(setup.block);
        assert_eq!(setup.ports.len(), block.ports.len(), "a slot per port");
        assert_eq!(spans.ports.len(), setup.ports.len(), "a span slot per port");
        for (port, port_spans) in setup.ports.iter().zip(&spans.ports) {
            let (Some(port), Some(port_spans)) = (port, port_spans) else {
                assert!(
                    port.is_none() && port_spans.is_none(),
                    "a port's spans match it"
                );
                continue;
            };
            assert_eq!(
                port.fields.len(),
                port.shape.fields().len(),
                "a slot per field"
            );
            assert_eq!(
                port_spans.fields.len(),
                port.fields.len(),
                "a span per field"
            );
            port_spans
                .shape
                .iter()
                .for_each(|&s| in_src(s, "setup shape"));
            for (value, s) in port.fields.iter().zip(&port_spans.fields) {
                s.iter().for_each(|&s| in_src(s, "setup field"));
                match value {
                    FieldValue::Unset => assert!(s.is_none(), "an unset field wasn't written"),
                    FieldValue::Given(_) => assert!(s.is_some(), "a given field was written"),
                    FieldValue::Invalid(_) => {}
                }
            }
        }
        spans.temp.iter().for_each(|&s| in_src(s, "setup temp"));
        assert!(
            spans.temp.is_some() || setup.temp.is_err(),
            "an unwritten `temp` is an error"
        );
        if let Ok(Temp::Env(env)) = setup.temp {
            assert!(
                design.envs[env.index()].value.is_ok(),
                "`temp` names a sound env"
            );
        }
        let fields = setup.ports.iter().flatten().flat_map(|p| &p.fields);
        // A port with a role that nothing sets was left out, which is reported.
        let left_out = setup.ports.iter().zip(&block.ports).any(|(set, port)| {
            let role = port.signal.ok().and_then(Shape::for_role);
            set.is_none() && role.is_some()
        });
        let placeholder = setup.temp.is_err()
            || left_out
            || fields
                .into_iter()
                .any(|f| matches!(f, FieldValue::Invalid(_)));
        if placeholder {
            assert!(
                setup.tainted.is_some(),
                "a setup with a placeholder is tainted"
            );
        }
    }
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
                InstanceOf::Error(_) => (0, 0),
            };
            assert_eq!(inst.pins.len(), n_pins, "a slot per pin");
            assert_eq!(inst.fields.len(), n_fields, "a slot per field");
            assert_eq!(inst.pins.len(), ispans.pins.len(), "a span slot per pin");
            assert_eq!(
                inst.fields.len(),
                ispans.fields.len(),
                "a span slot per field"
            );
            if !matches!(inst.of, InstanceOf::Error(_)) {
                inside(ispans.kind, ispans.stmt, "instance kind");
            }
            for s in ispans.pins.iter().chain(&ispans.fields).flatten() {
                inside(*s, ispans.stmt, "binding");
            }
            for (net, s) in inst.pins.iter().zip(&ispans.pins) {
                if let Ok(net) = net {
                    assert!(
                        net.index() < block.nets.len(),
                        "pins point at the block's nets"
                    );
                    assert!(s.is_some(), "a bound pin has a span");
                }
            }
            for (value, s) in inst.fields.iter().zip(&ispans.fields) {
                match value {
                    FieldValue::Unset => assert!(s.is_none(), "an unset field wasn't written"),
                    FieldValue::Given(_) => assert!(s.is_some(), "a given field was written"),
                    // Written but wrong, or required and not written.
                    FieldValue::Invalid(_) => {}
                }
            }
        }
        // Every placeholder taints its block: flatten relies on it to skip checking a
        // broken circuit.
        let placeholder = block.ports.iter().any(|p| p.signal.is_err())
            || block.instances.iter().any(|i| {
                matches!(i.of, InstanceOf::Error(_))
                    || i.pins.iter().any(Result::is_err)
                    || i.fields.iter().any(|f| matches!(f, FieldValue::Invalid(_)))
            });
        if placeholder {
            assert!(
                block.tainted.is_some(),
                "a block with a placeholder is tainted"
            );
        }
    }
    // Each problem is reported once (model.md E23). A resolve error holds an `f64`, so
    // it isn't hashed: the errors are compared pairwise.
    for (i, e) in resolved.errors.iter().enumerate() {
        let again = resolved.errors[..i]
            .iter()
            .any(|f| f.span == e.span && f.related == e.related && f.kind == e.kind);
        assert!(!again, "each problem is reported once: {e:?}");
        check_problem(src, e);
    }
}

/// Each problem is reported once (model.md E23): no two alike, in the same place with
/// the same related place.
fn assert_reported_once(errors: &[FlattenError]) {
    let problems: std::collections::HashSet<_> = errors
        .iter()
        .map(|e| (e.span, e.related, &e.kind))
        .collect();
    assert_eq!(
        problems.len(),
        errors.len(),
        "each problem is reported once"
    );
}

/// Flattening any parsed input never panics, gives the same result every time, and
/// each root's flat design holds together: placements and devices point at what placed
/// them, paths unique, every local net of every placement in exactly one flat net,
/// every pin on an existing net, every knob referenced once, and the ground net found
/// exactly when the root was checked (model.md §6). Every problem is reported once.
fn check_flatten_invariants(src: &str, parsed: &crate::parser::Parsed) {
    let elaborated = elaborate(parsed);
    let design = &elaborated.resolved.design;
    // Nothing may depend on a `HashMap`'s order, which is seeded afresh each time.
    assert_eq!(
        format!("{:?}", elaborate(parsed).flattened),
        format!("{:?}", elaborated.flattened),
        "flattening is deterministic"
    );
    let errors = &elaborated.flattened.errors;
    assert_reported_once(errors);
    let map = &elaborated.resolved.source_map;
    let upstream = parsed.has_errors() || !elaborated.resolved.errors.is_empty();
    let recursion = errors
        .iter()
        .any(|e| matches!(e.kind.problem, FlattenProblem::RecursivePlacement { .. }));
    for view in elaborated.roots() {
        let flat = view.data;
        assert_eq!(flat.instances[0].origin, None, "the root comes first");
        for (at, instance) in flat.instances.iter().enumerate().skip(1) {
            let origin = instance.origin.expect("a placement has a parent");
            assert!(origin.at.index() < at, "parents come first");
            let placed = view.instance(origin);
            assert_eq!(placed.of, InstanceOf::Block(instance.block));
        }
        for device in &flat.devices {
            let part = view.instance(device.origin);
            assert_eq!(part.of, InstanceOf::Part(device.kind));
        }
        for (k, knob) in flat.knobs.iter().enumerate() {
            let KnobSource::Field { device, field } = knob.source;
            assert_eq!(
                flat.devices[device.index()].fields[field],
                FlatField::Knob(KnobId::new(k)),
                "a knob's source is the field that refers to it"
            );
        }
        if flat.tainted.is_some() {
            assert!(
                upstream || recursion,
                "a broken root has an error behind it"
            );
        }
        for ground in &flat.grounds {
            let grounded = flat.nets[ground.index()].names.iter().any(|name| {
                let block = view.block(name.at);
                block
                    .net_port(name.net)
                    .and_then(|p| block.ports[p.index()].signal.ok())
                    == Some(SignalType::Ground)
            });
            assert!(grounded, "a ground net has a `Ground` port");
        }
        // Simulating it gives its one ground net, or an error: its own, or the one
        // that broke it, and then nothing more is reported.
        let mut simulated = Vec::new();
        match check_simulation(view, map, &mut simulated) {
            Ok(ground) => assert_eq!(flat.grounds, [ground], "node 0 is the ground net"),
            Err(_) if flat.tainted.is_some() => {
                assert_eq!(simulated, vec![], "a broken root isn't checked")
            }
            Err(_) => assert!(
                simulated.iter().any(|e| matches!(
                    e.kind.problem,
                    FlattenProblem::NoGround { .. } | FlattenProblem::SeveralGrounds { .. }
                )),
                "no single ground net is reported"
            ),
        }
        assert_reported_once(&simulated);
        for e in &simulated {
            check_problem(src, e);
        }
        let unique = |paths: Vec<String>, what: &str| {
            let set: BTreeSet<&String> = paths.iter().collect();
            assert_eq!(set.len(), paths.len(), "{what} paths are unique");
        };
        let paths = |n: usize, path: &dyn Fn(usize) -> String| (0..n).map(path).collect();
        let instance = |i| view.path(FlatInstanceId::new(i)).to_string();
        unique(paths(flat.instances.len(), &instance), "instance");
        let device = |d| view.device_path(FlatDeviceId::new(d)).to_string();
        unique(paths(flat.devices.len(), &device), "device");
        let knob = |k| view.knob_path(KnobId::new(k)).to_string();
        unique(paths(flat.knobs.len(), &knob), "knob");
        let mut entries = BTreeSet::new();
        for net in &flat.nets {
            assert!(!net.names.is_empty(), "every net has a name");
            for name in &net.names {
                assert!(
                    entries.insert((name.at, name.net)),
                    "a local net is in one flat net"
                );
            }
        }
        let locals: usize = flat
            .instances
            .iter()
            .map(|i| design.block(i.block).nets.len())
            .sum();
        assert_eq!(entries.len(), locals, "every local net is in a flat net");
        let mut knobs = BTreeSet::new();
        for device in &flat.devices {
            for net in device.pins.iter().flatten() {
                assert!(net.index() < flat.nets.len(), "pins point at existing nets");
            }
            for field in &device.fields {
                if let FlatField::Knob(k) = field {
                    assert!(k.index() < flat.knobs.len(), "knobs exist");
                    assert!(knobs.insert(*k), "each knob belongs to one field");
                }
            }
        }
        assert_eq!(knobs.len(), flat.knobs.len(), "every knob is referenced");
    }
    for e in &elaborated.flattened.errors {
        check_problem(src, e);
    }
}
