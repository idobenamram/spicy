# AST Design (M1c)

> 2026-09-26 · Design note for roadmap M1c. **Status:** first version implemented in `crates/spicy_lang/src/parser/`. Updated 2026-09-30 for the v5 syntax (`syntax_v5_plan.md`): block headers and circuits, setups, `env`, `const`, one range node, and the size rule (A12–A13).
> How `spicy_lang` turns tokens into a syntax tree: how the tree is stored, how the parser builds it, what broken code looks like, what the parser rejects, and how it's tested. The grammar is in `grammar.md`; why an AST and not a lossless tree is in roadmap §4.4.
> Research (2026-09-26), one report per reference, each claim checked against the source:
> - **Spade:** `externals/spade` at `177e5c4`
> - **atopile:** `externals/atopile`
> - **rustc** at `a22b02ea`, **rust-analyzer** at `86493cee`
> - **Zig:** Codeberg master

---

## 1. One statement, and one broken one

```rust
/// Divider holds the base near 2.1 V.
let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };
```

(a statement of `circuit CeAmp`) becomes a statement with its doc comment and span, holding an expression tree:

```
Let r1                                    (the whole statement's span)
  doc  "/// Divider holds the base near 2.1 V."
  StructLit Resistor
    field a      Path vcc
    field b      Path base
    field value  Binary Tol
                   Quantity 47000
                   Quantity 1 %
```

Every node carries its byte span, so the editor can replace exactly the bytes of `47k ± 1%`.

**Broken:** `let r2 = Resistor { a: base, b: gnd value: 10k };` (a missing `,`).
- The parser reports "expected `,` or `}`".
- It skips to the `;`.
- It records the statement as `Stmt::Error` **with the span of the broken text**, and goes on to the next statement.

The tree always exists. The broken statement still has a node the editor and the AI can point at and replace.

---

## 2. How the references do it

| | **Spade** | **atopile** | **rustc** | **rust-analyzer** | **Zig** |
|---|---|---|---|---|---|
| **Storage** | Owned enums; `Box<Loc<Expr>>` (`spade-ast/src/lib.rs:375`) | Two trees: ANTLR's parse tree, then its own AST as graph nodes in a typegraph (`antlr_visitor.py:40`) | Owned structs and enums; `Box<Expr>`. `Expr` is 64 bytes (`ast.rs:4480`) | Lossless CST (rowan) with typed wrappers generated from a grammar file | Flat arrays: `{tag, main_token, data: 8 bytes}`, lists in `extra_data` (`Ast.zig:2914`) |
| **Spans** | Inline `Loc { span, file_id }`, 16 bytes | Line/col plus a **copy of the node's text** on every node (`ast_types.py:105`) | Inline 8-byte `Span`, interned when it doesn't fit (`span_encoding.rs:82`) | Offsets computed while walking the tree | **None stored:** computed from the first and last token, and clamped to one line (`Ast.zig:3993`) |
| **How it's built** | Recursive descent builds nodes directly; `Result<Option<T>>`; Pratt loop (clones the left side, `expression.rs:244`) | ANTLR, then a 1,038-line visitor into the AST, which it calls "a stop-gap" (`antlr_visitor.py:26`) | Recursive descent; precedence climbing (`expr.rs:145`); a "no struct literal here" restriction (`mod.rs:70`) | The parser emits events; `precede` handles left recursion; the tree is built afterwards | Recursive descent writing into arrays; reserve a node before its children; a `scratch` stack for lists |
| **Broken statement** | **Skipped, no node** (`lib.rs:3029`). One error node, for `a.` | **First error only, no partial tree.** The LSP keeps the last good build | **`StmtKind::Err` / `ExprKind::Err`** placeholders (`stmt.rs:1182`) | **ERROR nodes**; missing children are `None` | **Skipped, no node** (`Parse.zig:1952`) |
| **Always a tree?** | At the top level | No | No (it can abort on "expected item") | Yes, and a test checks every byte is in it | Yes (fails only on out-of-memory) |
| **Tests** | Hand-built expected trees (spans ignored in comparisons), plus 85 error snapshots | 3 parse tests; round trip of the token stream | UI tests with `.stderr` files, `--bless`; pretty-print until it converges | `.rast` tree dumps, updated by an environment variable; inline `// test` cases | 265 parse → format == source tests; 44 error-tag tests; a fuzzer checked against a parser generated from the grammar |
| **Size** | AST 1.1k lines, parser 6.6k | Visitor 1k, AST types 1.3k | AST 4.5k, parser 25k | Parser 9.7k, generated nodes 11k | AST 4k, parser 3.4k |

**Lessons:**
- **Owned enums are the norm** (Spade, rustc). Zig's flat arrays save memory with untagged unions, which Rust can't do safely. They cost about 500 lines just to compute node spans (`Ast.zig:587-1100`), for memory our few-KB files don't need.
- **A broken statement needs a node** for us, because editor and AI edits replace a node's span. rustc's `Err` placeholders keep required fields required, so later stages match on every case without handling `None` everywhere (rust-analyzer's cost).
- **Never return `Result<File>`.** Always return the tree plus a list of errors (rust-analyzer, Zig).
- **Spans need tests.** Spade compares trees ignoring spans, so none of its parser tests check them.

---

## 3. Decisions

| # | Decision | Why |
|---|---|---|
| A1 | **Owned Rust enums and structs, `Box` for recursion** | rustc and Spade. Exhaustive `match` in every consumer, readable for us and the AI. Zig's layout would need hand-written accessors in Rust |
| A2 | **A `Span { start, end }` (8 bytes) on every node.** No file id: a parse is one file | Editor edits replace a node's bytes, and exact multi-line spans matter (unlike Zig's clamped ones) |
| A3 | **Names borrow from the source:** `Ident { text: &'src str, span }` | No copies and no interner in the parser. The AST lives next to `Tokens<'src>`. Interning belongs in `spicy_model` |
| A4 | **Placeholder error nodes:** `ItemKind::Error`, `StmtKind::Error`, `BlockEntry::Error` and `SetupEntry::Error`, each with the span of the broken text; `ExprKind::Error` and `TypeKind::Error` for a value or type that didn't parse (its statement or entry keeps its name), or a number or string the lexer rejected. Each holds the `Reported` proof that its error was reported (2026-09-29): a parse function returns `PResult<T> = Result<T, Reported>`, as rustc's `ExprKind::Err(ErrorGuaranteed)`. A `Body`, `BlockDecl` and `SetupDecl` also hold `broken: Option<Reported>`, set when parsing it reported an error, so an error found at the next item (a missing `}`) breaks the item left open, not the next one. Fields stay required | rustc's approach. Downstream matches stay total; the editor has a target; a later stage can't treat a placeholder as real |
| A5 | **`parse(src)` always returns a `Parsed`**: tokens, the tree, lexer errors, parser errors | rust-analyzer and Zig |
| A6 | **The parser builds nodes directly** (recursive descent + a Pratt loop), with no event stream and no clones | rust-analyzer's events exist to feed rowan, which we don't use |
| A7 | **The parser reads only significant tokens.** Trivia is skipped. A look-alike character the lexer already reported (`−`) is read as the character it looks like (`-`), as rustc does (`check_for_substitution`); other unknown characters are skipped. A parse error is dropped as a follow-on if it starts inside a lexer error, on the first token right after one, or after an unterminated `/*`; any other parse error is kept (a missing `;` after `1Meg` is a second, real mistake). Both lists are sorted, so the filter is linear | One mistake gives one error: `b − c`, `1.;`, `a % 3` each give only the lexer's |
| A8 | **Statement-level recovery:** on an error, scan again from the statement's start and stop after a `;` with no bracket open, before the closing `}`, before a statement or item start at or after the error, or before the next statement's doc comments and attributes. Brackets opened before the error are assumed broken (an unclosed `Resistor {` doesn't swallow the circuit); brackets opened after it are skipped whole (`for … { … }`). **Entry-level** (ports, setup entries; 2026-09-30): stop before the next `,` with no bracket open, the list's `}`, or the next item. **Item-level:** skip to the next item start, keeping its doc comments and attributes. `setup` and `pub` start an item only before an item's own next token (grammar.md §3). A missing `;` or `,` is a soft error when the next token starts a statement or entry, and so is a missing `{` before an item's contents. **Value-level** (2026-09-29): a right-hand side that doesn't parse (a `let`, `net x =`, `env` or `const` value, a port's type, a setup entry's value) becomes an `ExprKind::Error` / `TypeKind::Error` over the text up to its end, so what it belongs to keeps its name. Several unclosed constructs at the end of the file give one error | Spade's and Zig's sync points, plus rust-analyzer's "keep the skipped span" |
| A9 | **Shape rules live in the parser, meaning in elaboration.** Parser: chained comparisons, the `±` rule, spreads next to comparisons, a spec with no relation, a statement in the wrong body, `pub` where it means nothing. Elaboration: names, units, pins, which block a circuit, contract or setup is for | The split every reference makes (Spade vs its lowering, rustc vs `ast_validation`, Zig vs AstGen) |
| A10 | **Doc comments are data on the node** (`docs: Vec<Span>`); attributes are nodes (`attrs`) | rustc and Spade attach them the same way |
| A11 | **Two limits guard the stack:** nesting (parentheses, types, calls) up to 128, and tree height up to 1024, counting chained operators too (`a + a + … + a` is flat text but a deep tree, and dropping or cloning a tree recurses once per level). Height is counted bottom-up, each node one more than its tallest child, because a chain grows *above* operands already parsed: a count of the levels above the parse point missed `a + (…) + a + …` (a later review built a 48 000-level tree that way). Past either limit, `TooDeep` | The red team crashed the parser with 20 000 nested types and with a 100 000-term sum (on drop). A language server's worker threads have 2 MB stacks |
| A12 | **`Expr` stays 48 bytes,** checked at compile time. A large, rarer variant is boxed: `StructLit(Box<StructLit>)`, which also holds the generic arguments (2026-09-30). Boxing the spec's contents, which would shrink a statement from 200 to 136 bytes, was measured and not done: it saves ~0.5% of instructions and only moves glibc's trim threshold to other file sizes. Revisit when spec clauses make a spec much larger | rustc checks its own (`static_assert_size!`) and boxes its large kinds (`ExprKind::Struct(P<StructExpr>)`) |
| A13 | **One range node,** `Range { lo, hi }` with each end optional, for `a..=b`, `..=b` and `a..` (2026-09-30). Never both ends missing: `..` alone isn't a range | rustc's `ExprKind::Range(Option, Option, _)` and rust-analyzer's `RangeExpr`. Every range rule lives in one parse function, and resolve has one arm |

**Not needed yet:** node IDs (rustc assigns them later, not in the parser; our model gets its own), and the "no struct literal here" restriction (the MVP has no `for` loops or `if`, grammar.md §3).

---

## 4. The types

```rust
pub struct Parsed<'src> { tokens, file: File<'src>, lex_errors: Vec<LexError>, errors: Vec<ParseError> }

pub struct File<'src>     { items: Vec<Item<'src>>, span }
pub struct Node<'src, K>  { docs: Vec<Span>, attrs: Vec<Attribute<'src>>, kind: K, span }
pub type   Item<'src>     = Node<'src, ItemKind<'src>>;
pub enum   ItemKind       { Block(BlockDecl), Circuit(Body), Setup(SetupDecl), Contract(Body),
                            Env(ValueDecl), Const(ValueDecl), Error }

pub struct BlockDecl      { public: Option<Span>, name, ports: Vec<Node<BlockEntry>>, broken }
pub enum   BlockEntry     { Port { name, ty: Type }, Error }
pub struct Body           { name, stmts: Vec<Stmt>, broken }             // a circuit or a contract
pub struct SetupDecl      { name, block: Path, entries: Vec<Node<SetupEntry>>, broken }
pub enum   SetupEntry     { Set { key: Key, value: Expr }, Error }
pub struct Key            { segments: Vec<Ident>, span }                 // vcc, vin.v, temp
pub struct ValueDecl      { name, ty: Type, value: Expr }                // env (a range), const

pub type   Stmt<'src>     = Node<'src, StmtKind<'src>>;
pub enum   StmtKind       { Net { name, merge: Option<Expr> }, Let { name, value: Expr },
                            DefaultSetup { setup: Path },                 // setup = Operating;
                            Spec { public: Option<Span>, name, relation: Relation }, Error }
pub struct Relation       { lhs: Expr, op: RelOp, rhs: Expr }            // `x within a..=b`, `x <= b`

pub struct Expr<'src>     { kind: ExprKind<'src>, span }                 // 48 bytes (A12)
pub enum   ExprKind       { Quantity(QuantityLit), Str(&'src str), Path(Path),
                            StructLit(Box<StructLit>), Paren(Box<Expr>), Array(Vec<Expr>),
                            Field { base, name }, Call { callee, args: Vec<Arg> }, Index { base, index },
                            Neg(Box<Expr>), Binary { op: BinOp, lhs, rhs },
                            Range { lo: Option<Box<Expr>>, hi: Option<Box<Expr>> }, Error }
pub enum   BinOp          { Rel(RelOp), Tol, Add, Sub, Mul, Div }         // RelOp: Within, Lt, Le, Gt, Ge
pub struct StructLit      { path, generics: Vec<GenericArg>, fields: Vec<Field> }
pub struct GenericArg     { name, value: Expr, span }                    // A = Mcp6001
pub struct Field          { name, value: Option<Expr>, span }            // `gnd` alone: value is None
pub enum   Arg            { Positional(Expr), Named { name, value, span } }   // ref: dc
```

---

## 5. Testing

- **Case files,** the same layout as the lexer: `test_data/parser/{ok,err}/*.spl`. Each gets a snapshot of an indented tree dump showing every node's kind, span and source text, then the errors. Spans are visible in every snapshot (see Spade's gap).
- **`ce_amp.spl`:** a snapshot of its tree. It must parse with no errors: 5 items (env, block, circuit, setup, contract), 4 ports, 8 circuit statements, 4 setup entries, 5 contract statements. `v5_mvp.spl` parses the MVP subset of v5's example.
- **Coverage rule:** every `ParseErrorKind` appears in some `err/` case.
- **Every fix works:** each suggested fix, applied on its own, must remove the error it was offered for; both readings offered for an ambiguous `±` must parse cleanly. (The red team found fixes that produced new errors or silently changed a value.)
- **Regression tests for the red team's crashes:** 20 000 nested types, brackets, struct literals and minus signs, 100 000-term chains (parsed, cloned and dropped on a 2 MB test thread), and 40 000 errors in linear time.
- **Invariants, on any input.** Checked on every case file, on seeded random input in `cargo test`, and in a cargo-fuzz target:
  - no panic;
  - a tree always;
  - every node's span inside its parent's and inside the input;
  - error spans in bounds.
- **Later, with the formatter:** Zig's canonical test, parse → format == source.

---

## 6. Open questions

1. **Comment attachment for the formatter.** rust-analyzer attaches leading comments to the next item and stops at a blank line (`shortcuts.rs:240-270`). We'll decide when the formatter comes.
2. **Node IDs for side tables:** when elaboration needs them (pipeline.md decision 9).
