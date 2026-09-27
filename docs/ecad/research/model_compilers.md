# Research: Compilers (rustc, rust-analyzer, Spade) for the Elaboration Stage

> 2026-09-27 · Research report for `model.md` (M1d).
> Revisions: rustc `main` at `b373574e` (2026-09-27), rust-analyzer `master` at `86493cee` (2026-09-26), Spade at `177e5c46` (local, `externals/spade`).
> "Verified" means the cited line was read. **Opinion** is marked.

## 1. Stages and names

**rustc**
- **Parse:** the parser produces the AST.
- **Name resolution:**
  - First it builds the module structure ("early", `build_reduced_graph.rs:1-6`).
  - Then, with the module tree complete, it resolves every body ("late", `late.rs:1-7`).
- **AST → HIR lowering:** HIR is the "high-level IR": the AST with names resolved and some syntax desugared. Lowering is "mostly a simple procedure, much like a fold" (`rustc_ast_lowering/src/lib.rs:1-4`).
- **Later stages:** type checking, then THIR and MIR.
- Resolution writes its answers into a map keyed by AST node id (`partial_res_map`, `rustc_resolve/src/lib.rs:1384`); lowering consumes that map.

**rust-analyzer (RA)**: three things sit between syntax and types.
- **ItemTree** (`item_tree.rs:1-31`): a per-file summary of the file's items, without bodies.
- **DefMap** (`nameres.rs:1-9`): the module tree plus each module's visible names. It's the output of name resolution.
- **ExpressionStore/Body** (`expr_store.rs:1-2`): each function body, lowered into arenas. An arena is a `Vec` that hands out typed indices.

**Spade**
- The AST is lowered to HIR (`spade-ast-lowering`), and type inference follows.
- `spade-hir-lowering` produces MIR by monomorphising each unit, i.e. one copy per set of generic arguments (`monomorphisation.rs:34-44`).
- `#[inline]` units are inlined (`inline.rs:358`), and the result goes to Verilog codegen.
- **Spade never flattens the hierarchy.** An entity instance stays `Operator::Instance { name, argument_names, .. }` in MIR (`spade-mir/src/lib.rs:295-305`) and becomes a Verilog module instance.

## 2. Identity

**rustc**
- **`DefId`** identifies a definition: a crate number plus an index (`def_id.rs:229-241`). The index, `DefIndex`, is "an interned shorthand for a particular DefPath" (`def_id.rs:212-214`).
- **What stays stable across sessions** is the **`DefPathHash`**, a hash of the definition's path (`def_id.rs:54-97`). The index itself is not stable.
- **`HirId`** = `(owner, local_id)`. The owner is the enclosing item; the local id is dense within that owner.
  - The docs say this is stable: you can "move an item around … or add or remove stuff before it, without the local_id part changing" (`rustc_hir_id/src/lib.rs:86-95`).
  - Because the local ids are dense, a mapping from them "can be implemented by a `Vec`" (`:165-172`).
- **Side tables:** types live in `TypeckResults`, one per owner, keyed by `ItemLocalId`. For example `node_types: ItemLocalMap<Ty>` (`typeck_results.rs:31-51`).
- **Spans:**
  - They stay inline in HIR (`hir.rs:2219-2224`).
  - Under incremental compilation they're made relative to their owner (`SpanLowerer`, `ast_lowering/lib.rs:396-410`; `SpanData.parent`, `rustc_span/lib.rs:700`).
  - `hir_id` is excluded from hashing (`hir.rs:2220`).

**rust-analyzer**
- **`AstId`** names an item by its kind, a hash of its name, and an index to tell same-named items apart. A nested item's id also includes its parent.
  - It is deliberately **not** a position: "IDs don't change unless the set of items itself changes" (`span/src/ast_id.rs:1-21`; the bit packing is at `:178-195`).
  - The file explains why: a churning id "can cause *a lot* to be recomputed".
- **Semantic ids are interned locations:** `FunctionId` = intern(`ItemLoc { container: ModuleId, id: AstId }`) (`lib.rs:123-126, 258`).
- **Body ids are arena indices:** `ExprId = Idx<Expr>` (`hir.rs:45-47`).
- **Source locations live only in a separate `ExpressionStoreSourceMap`,** which maps both ways between `ExprId` and syntax pointer (`expr_store.rs:167-197, 232-245`).

**Spade**
- `NameID(u64, Path)` comes from a global counter. Equality and hashing use only the number; the path is kept for printing (`spade-common/src/name.rs:263-295`).
- `ExprID` also comes from a counter, taken once per visited expression (`ast-lowering/lib.rs:2767-2768`). Types are keyed by it (`typeinference/equation.rs:598-601`).
- **Neither id is stable across edits.**
- Spans are inline everywhere, as `Loc<T> { inner, span, file_id }` (`location_info.rs:117-122`).

## 3. Name resolution

**rustc**
- **Namespaces:** types, values and macros are separate (`def.rs:619-631`), so one name can be both a type and a value.
- **Local scopes are "ribs":** a rib is a scope pushed onto a stack wherever the set of visible names changes. There is one stack per namespace, searched from the inside out (`late.rs:287-305`).
- **Items come before locals:** "A block's items are above its local variables in the scope hierarchy, regardless of where the items are defined" (`ident.rs:291-304`). That is what makes items order-independent and allows forward references.
- **Lookup order** (the `Scope` enum, `rustc_resolve/lib.rs:109-141`): the module's own names, then globs, then the preludes, with **built-in types last**.
- **Classification:** a lookup gives `Res::Def(DefKind, DefId)` or `Res::Err` (`def.rs:489-587`). The `DefKind` (struct, fn, …) says what the name is and implies its namespace (`def.rs:292-302`).
- **Collect, then bodies:**
  1. `DefCollector` assigns ids to every definition (`def_collector.rs`);
  2. imports are resolved;
  3. late resolution walks the bodies (`lib.rs:2091-2111`).

**rust-analyzer**
- **Collect, then bodies:** the DefMap is built by walking the modules, assigning item ids, and queuing the imports and macros that don't resolve yet.
  - A **fixed-point loop** then resolves as many as it can per round (`nameres.rs:11-49`; `collector.rs:456-478`).
  - Names that never resolve are recorded, and completion still works on them (`collector.rs:461-466`).
- Each name maps to a `PerNs { types, values, macros }` (`per_ns.rs:54-58`).
- **Inside bodies**, lookup goes through local bindings (`ExprScope`), then generic params, then block and module scopes (`resolver.rs:83-93`). The result is classified as `TypeNs` or `ValueNs` (`:96, :118`).
- The prelude is attached to the DefMap (`nameres.rs:181-187`).

**Spade**
- **Collect, in explicit passes, then bodies:**
  1. macros;
  2. external modules;
  3. traits and modules;
  4. types;
  5. `gather_symbols`, which records every unit's *head* (its signature) (`spade-compiler/lib.rs:174-231`; `global_symbols.rs:315-412`, where `visit_unit` adds `Thing::Unit(head)`).
  6. Only then are the bodies lowered (`lower_ast`, `lib.rs:696-719`).
- If a pass fails, compilation stops early (`lib.rs:196, 222, 234`).
- **The symbol table** is a stack of scopes keyed by *absolute* path (`symbol_table.rs:526-556`).
  - Types and non-types ("things") are kept in separate maps under the same `NameID`, so a struct's type and its constructor share one id (`:326-352`).
  - A lookup returns a `NameID`, and the caller asks for the kind of thing it wants. The typed errors are `NotAUnit`, `IsAType`, `NotAValue`, … (`:21-40`).

## 4. Definitions vs uses

- **rustc:**
  - Each definition is lowered once, as its own owner (`OwnerInfo`, `hir.rs:1377-1390`).
  - A use stores only its `Res`.
  - Generic code stays generic until codegen.
- **RA:**
  - A definition's signature (`StructSignature::of`, `signatures.rs:77-88`) is stored apart from its body.
  - A use is an `ExprId`, and its resolution is computed on demand.
- **Spade:**
  - HIR has one `Unit` per definition (`spade-hir/lib.rs:767-774`).
  - An instance is just a call expression: `ExprKind::Call { kind: CallKind::Entity(loc), callee: Loc<NameID>, args }` (`expression.rs:131-141, 237-244`).
  - Monomorphisation produces **one MIR unit per (definition, generic arguments)**, not one per instance. Requests are memoised in `translation: (NameID, Vec<KnownTypeVar>) → NameID` (`monomorphisation.rs:66-73`).

## 5. Lowering algorithm and error nodes

**rustc**
- Resolution runs once, eagerly, over the whole crate.
- Lowering is an on-demand query per owner (`queries.rs:237-240`; the per-owner lowering function is at `ast_lowering/lib.rs:704`).
- Every new HIR node takes the owner's next local id (`lib.rs:890-922`). A debug check ensures every AST node is lowered exactly once.
- **Error nodes:** `ExprKind::Err` / `TyKind::Err` carry an `ErrorGuaranteed`. It's a zero-sized value that can only be created by actually emitting an error (`rustc_span/lib.rs:2842-2849`). A placeholder therefore always proves that its diagnostic was already reported, and later stages stay quiet.

**rust-analyzer**
- Everything is lazy (salsa queries).
- Broken syntax lowers to `Expr::Missing` / `Pat::Missing` (`hir.rs:283, 757`).
- The stated rule: analyses return `(T, Vec<Error>)`, never `Result` (`architecture.md:456-457`).

**Spade**
- Lowering is eager and runs in several passes.
- `visit_expression` catches an error, pushes the diagnostic, and returns `ExprKind::Error` (`ast-lowering/lib.rs:2767-2779`). A failed statement becomes `Statement::Error` (`:2012-2019`).
- Codegen skips any unit that contains an error statement (`spade-compiler/lib.rs:743-749`).

## 6. Diagnostics

**Errors that belong to this stage (in all three):**
- unresolved or ambiguous names;
- the wrong kind of name (a type used as a value);
- duplicate definitions;
- missing or extra fields;
- misplaced attributes (Spade's `report_unused("trait")`, `global_symbols.rs:350`).

**Where they get their positions:**
- **rustc** reads the spans from HIR.
- **RA** stores lowering diagnostics *in the source map*, "since they're just as volatile" (`expr_store.rs:192-197`). DefMap diagnostics live in the DefMap (`collector.rs:497`).
- **Spade** collects them in a shared `DiagList`, and `Loc` supplies the span.

## 7. Incrementality: the editor vs the batch compiler

**rustc** re-runs resolution for the whole crate on every build (`eval_always`, `queries.rs:197-218`). After that it relies on per-owner hashing, relative spans and an on-disk cache.

**rust-analyzer**
- **The core invariant:** "typing inside a function's body never invalidates global derived data" (`architecture.md:173-175`).
- **The ItemTree is the invalidation barrier:** editing a body doesn't change the file's ItemTree, so name resolution isn't recomputed (`item_tree.rs:12-14`).
- **How salsa helps:** salsa is a memoising query engine. When a recomputed result equals the old one, the queries that depend on it don't re-run.
  - RA exploits this by computing `with_source_map`, which returns both data and source map, and exposing `of`, which returns only the data (`signatures.rs:79-88`).
  - A whitespace edit changes the map but not the data, so everything that depends on `of` survives.
- **Cancellation:** a new edit cancels in-flight work (`architecture.md:377-388`).

**Spade's language server** has no salsa. Its `LocalBuild` goal does the global symbol collection for the whole project, but lowers bodies only for the edited file (`spade-compiler/lib.rs:395-406`).

## 8. Testing

- **rustc:**
  - `tests/ui` holds `.rs` files with `//~ ERROR` annotations and blessed `.stderr` snapshots (e.g. `tests/ui/resolve/issue-2356.rs`);
  - HIR dumps from `-Zunpretty=hir` are compared against `.stdout` files (`tests/ui/unpretty/`).
- **RA:**
  - A test is a multi-file fixture string plus a text dump, compared with `expect![[…]]` inline snapshots (`nameres/tests.rs:15-80`).
  - Incrementality tests edit a file and assert which queries re-ran (`tests/incremental.rs:16-43, 129`).
  - The rule: tests are data-driven and don't test the API (`architecture.md:405-435`).
- **Spade:** `insta` snapshots, both of rendered diagnostics (`snapshot_error!`, `spade-tests/src/lib.rs:78`, about 800 `.snap` files) and of MIR (`snapshot_mir!`, `:186`).

## 9. Lessons (opinion unless marked)

**Verified facts behind the recommendations:**
- All three collect globals before bodies.
- rustc and RA give items order independence by putting them above locals.
- rustc and RA key ids by owner or by (kind, name, index), never by a global counter. Spade uses counters and so gets no incremental reuse.
- RA keeps the source map out of the data so that data equality gives early cutoff.
- All three use typed `Error` placeholders.
- Spade doesn't flatten.

**Recommendations:**
1. **Two passes.**
   - Pass A (collect): every item name → `BlockId` or contract, and every block's ports → its signature. This happens before any body is lowered.
   - Pass B (bodies): resolve pins, fields and measures.
   - Within a block body, collect all `port`, `net` and `let` names first too.
2. **Two namespaces:**
   - *kinds*: `Resistor`, `CeAmp`, signal types;
   - *values*: ports, nets, instances, measures, `env`.

   Classify `let x = P { … }` by what `P` resolves to. Return typed errors like Spade's. Look names up in the local scope, then the file, then `use`, then the prelude, as rustc's `Scope` order does.
3. **Ids and identity.**
   - In the Design, use dense per-block local indices owned by the `BlockId`, like `HirId`.
   - Stable identity = the path (`amp.r1`, `amp.r1.value`), the analogue of `DefPathHash` and RA's `AstId`.
   - Never key anything that must survive an edit by a global counter.
4. **Spans in a `DesignSourceMap` the data never contains.** Derive `PartialEq` without spans, following RA's `with_source_map`/`of` split, for early cutoff. This stage's diagnostics live beside the source map.
5. **Errors never stop the stage.**
   - Return `(Design, Vec<Diagnostic>)`.
   - Use placeholders carrying a `Reported` token that only the diagnostic sink can create (the `ErrorGuaranteed` pattern).
   - An instance with a bad field keeps its other fields.
   - FlatDesign skips only the contract items that touch an error.
6. **Define once, place many.** Elaboration is a separate walk that prefixes paths.
   - Memoise per-block work by `BlockId`; later, by `(BlockId, exact param values)`.
   - Unlike Spade, we must flatten.
   - Keep a map from each flat path back to its chain of placements.
7. **Incrementality.** For the MVP, recompute everything. Make each step a pure function with per-block `PartialEq` outputs, so salsa can be added later. Keep RA's invariant: editing a block body never invalidates other blocks' signatures.
8. **Tests.**
   - Fixture `.spl` → stable text dumps of Design, FlatDesign and KnobTable.
   - Rendered-diagnostic snapshots for every error.
   - Later: "which queries re-ran" tests, and a test that knob paths don't change when an unrelated part is added.
