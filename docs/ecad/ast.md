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

<a name="decisions"></a>
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

<a name="types"></a>
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

<a name="testing"></a>
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

---

<a name="front-end-choices"></a>
## 7. Front-end choices

Moved here from roadmap §4.3–4.5 on 2026-10-03.

<a name="choices"></a>
### 7.1 The choices

| Choice | Recommendation | Why |
|---|---|---|
| Lexer | **Hand-written**, like `spicy_netlist`'s (std only, or the tiny `unscanny` it already uses). **No `logos`** | [7.2](#why-ast) |
| Parser | **Hand-written** recursive descent, with a Pratt loop for expressions. Design borrowed from Spade ([7.3](#spade)), code written from scratch | [7.2](#why-ast), [7.3](#spade) |
| Syntax tree | **A typed AST with byte spans on every node, plus the lexer's full token list (whitespace and comments included).** No lossless tree, no `rowan` | [7.2](#why-ast) |
| Diagnostics | **`codespan-reporting`**, see the comparison below | The only new crate the front-end adds |
| Tests | Snapshot tests: syntax trees, rendered diagnostics, the elaborated model, lowered circuits. Numeric outputs use the tolerance-based snapshots from M0 | The project's style; full precision is stored, but last-digit noise across machines doesn't fail tests |

**Diagnostics libraries compared** (current versions: codespan-reporting 0.13, ariadne 0.6, miette 7):

| | **codespan-reporting** | **ariadne** | **miette** |
|---|---|---|---|
| **Look** | rustc-style, which is what Rust users read every day | The prettiest: colored arrows, and it handles overlapping labels best | Fancy "graphical" or plain modes |
| **Maturity** | Long-lived and widely used in compilers (Spade uses a fork). Slow-moving, stable | Actively developed, but 0.x with breaking changes between versions | Stable 7.x; built mainly for application error chains |
| **How you use it** | Build `Diagnostic` values (labels, notes) and render against a file database | Builder API with a source cache | Derive macros on your error types (`#[derive(Diagnostic)]`) |
| **Spans** | Byte ranges, which match our spans | Character offsets by default (byte offsets configurable in recent versions; check when integrating) | Byte offsets |
| **Plain text for snapshot tests** | Yes | Yes | Yes |
| **Fit for us** | ✅ A compiler's diagnostics, rustc-familiar, low churn | Good if appearance matters most | Better suited to app errors than to a compiler |

**Recommendation: codespan-reporting.** Later, the SPICE parser's hand-rolled error snippets (`format_error_snippet` in the CLI) could move to it too, so both front-ends report errors the same way.

<a name="why-ast"></a>
### 7.2 Why an AST and a hand-written lexer (decided 2026-09-26)

**The goals:** fast, modern, and as few crates as possible. The front-end ends up with **zero parsing dependencies**; its only new crate is `codespan-reporting`.

**The two kinds of syntax tree, on one line:**

```rust
    let r1 = Resistor { value: 47k ± 1% };  // top
```

- **AST (abstract syntax tree).** Keeps the meaning, with byte ranges ("spans") pointing back into the file:
  ```
  Let { name: "r1" @8..10,
        value: StructLit { path: "Resistor" @13..21,
                           fields: [value: Tol(47k, 1%) @31..40] },
        span: 4..43 }
  ```
  Spaces and the `// top` comment aren't in the tree. They stay in the token list.
- **CST (concrete syntax tree, "lossless").** Every character is a leaf: `WHITESPACE "    "`, `LET_KW "let"`, `WHITESPACE " "`, `IDENT "r1"`, … `SEMI ";"`, `WHITESPACE "  "`, `COMMENT "// top"`. Joining the leaves gives back the file. The nodes are untyped, so a typed layer (`LetStmt::name()`) is written on top. `rowan` is the library rust-analyzer uses for this.

**What we chose:** an AST with a span on every node, **plus the lexer's full token list**, whitespace and comments included, exactly as `spicy_netlist`'s lexer already keeps whitespace and newline tokens. The file is always reproducible byte for byte from the tokens. This is Zig's design: `zig fmt` formats from the AST plus the token list. Go's `gofmt` works from an AST plus a comment list.

`spicy_netlist` is the same kind: a hand-written lexer, then phases that build typed structures (`Deck`) directly. It has no syntax-tree layer that keeps the source.

| | **AST + spans + token list** (chosen) | **CST with `rowan`** |
|---|---|---|
| Crates | None | `rowan` 0.16 + 4 (countme, hashbrown, rustc-hash, text-size) |
| Code to write | The parser builds typed nodes directly | The parser emits start/finish events, plus a hand-written typed layer: roughly twice the code |
| Speed | Faster (no trivia nodes). Both are far faster than our file sizes need | Slower, but it wouldn't matter |
| Editor and AI edits ("change r1's value") | Replace the bytes at the node's span | Patch the tree |
| Canonical formatter | From AST + tokens (Zig, Go) | Easier comment placement |
| Formatting one statement without reflowing others (language_editor_mapping R14) | Format only the edited statement's span | Same |
| Moving code together with its comments, keeping odd hand formatting | Harder | Its real strength |
| Incremental reparsing on every keystroke | Not needed: a design file reparses from scratch in far less time than a keystroke | Its other strength, for very large codebases |
| Used by | rustc, Go, Zig, Spade, `spicy_netlist` | rust-analyzer, Roslyn (C#), Swift |

**Why this replaces the earlier "lossless from day one":** the three things lossless was meant for (precise edits, a formatter, error recovery) all work from spans plus the token list. The fear was that retrofitting would mean rewriting the parser. The MVP grammar is about 15 rules, so a rewrite would cost days, not a redesign. And if we ever need a CST, it doesn't require `rowan`: a plain `Node { kind, children }` over our tokens is a small amount of our own code.

**Why no `logos`.** logos (Spade's lexer) turns regexes on an enum into a lexer at compile time. It saves typing for many simple tokens, but:
- **Our hard tokens are the ones it doesn't help with.** Unit literals (`47k`, `4k7`, `10kΩ`, `1µF`, `5%`), `100..=300` (the lexer must not read `100.` as a decimal), `±` and `+/-`, and nested `/* */` comments all need hand-written code either way. Even Spade handles block comments outside logos, in its parser.
- **It's a proc-macro crate**, so it pulls a compile-time stack (syn, quote, regex-syntax, …) into the build.
- **A hand lexer for about 40 token kinds is a few hundred lines,** in the same style as `spicy_netlist`'s (`crates/spicy_netlist/src/reader/lexer.rs`).

<a name="spade"></a>
### 7.3 What we take from Spade, and what we don't

Spade's parser (`externals/spade/spade-parser`) is the reference for *how* ours is structured. **Its code is not copied or ported:** Spade's compiler crates are EUPL-1.2, a copyleft licence that isn't compatible with our MIT licence, and its README explicitly refuses LLM-generated contributions.

**We take:**
- Parse functions return `Result<Option<T>>`: `Ok(None)` means "not mine, nothing consumed", `Ok(Some)` means parsed, `Err` is a diagnostic.
- A statement loop that dispatches on the leading keyword (`let`, `net`, `port`, `assume`, `spec`, `#[…]`).
- **Recovery:** after an error, skip to a token that can restart a statement, so one broken statement gives one error and the next parses normally. A missing `;` is reported with an insert-`;` fix and parsing just continues. Ours is tighter than Spade's in one way: since every statement ends in `;`, recovery also skips *past* the next `;`.
- A Pratt loop for expressions, with an ordered enum of binding powers.
- Brace-named arguments (`Resistor { a: vcc, … }`), with a flag that forbids them where a `{` opens a body (`for i in 0..N {`, later), as in Rust.
- A diagnostic builder (error, primary and secondary labels, help, suggested replacement), rendered by codespan-reporting.
- Snapshot tests of rendered errors (Spade has about 800).

**We leave:**
- Splitting `>>` into `> >` for nested generics. Spade needs it because it has shift operators. We have none, so there's no `>>` token and `Tol<Ohm>` inside `Foo<…>` just works.
- Pipelines, registers, macros, and the parse-trace machinery.
