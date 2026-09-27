# Research: Hardware Elaborators (slang, Yosys, FIRRTL/Chisel/CIRCT) for the Elaboration Stage

> 2026-09-27 · Research report for `model.md` (M1d).
> Revisions: slang `97a2b64` (2026-09-26), Yosys `30d6257`, firrtl-spec `afee1ba`, CIRCT `048afe0`, Chisel `14b890c`.
> "Verified" means the cited line was read; the lessons at the end are opinion.

## 1. Stages and names

- **slang** (`docs/overview.dox:12-20, 62-90`): Lexer → Preprocessor → Parser → SyntaxTree → **Elaboration** (`Compilation`) → **Analysis** (`AnalysisManager`).
  - Elaboration resolves names, evaluates parameters, builds the hierarchy and checks types. Its output is a hierarchical "elaborated AST", reached through `getRoot()`.
  - Driver checks (several drivers, or none) are a separate later stage over the finished elaboration.
  - slang never produces a flat netlist.
- **Yosys:**
  - A frontend turns the syntax tree into **RTLIL**, Yosys's netlist format (`rtlil_rep.rst`).
  - **Passes** then rewrite that one `Design` in place:
    - `hierarchy`: finds the top module and builds parameterized copies;
    - `flatten`: inlines the hierarchy;
    - `opt_clean`: merges connected wires and removes dead ones;
    - `check`: reports driver problems.
- **FIRRTL:** spec 2.0.0 *removed* the old high/mid/low FIRRTL forms ("Remove FIRRTL forms and lowering", `revision-history.yaml:186`). CIRCT runs a pipeline of passes over one representation (`lib/Dialect/FIRRTL/Transforms/`): `InferWidths`, `Dedup`, `ModuleInliner`, `ExpandWhens`, `LowerTypes`, …
- **Chisel:** "elaboration" means *running the user's Scala program*. Each `Module(new X)` call is a constructor that emits FIRRTL; CIRCT then does the declarative work.

## 2. Definitions vs instances

- **slang** keeps three kinds of object (`InstanceSymbols.h:92-200`):
  - one `DefinitionSymbol` per module text;
  - one `InstanceSymbol` per placement (name, port connections, depth);
  - one `InstanceBodySymbol` per instance, holding the members.
- **slang deduplicates bodies** (`ElabVisitors.h:531-591`, `InstanceCacheKey.cpp:23-35`).
  - The cache key is the definition pointer, a hash of every parameter value, and the interface connections.
  - A matching instance gets `setCanonicalBody(first)`, the duplicate body is never visited, and downstream code follows the canonical one (`ASTVisitor.h:152`).
  - This works only because elaboration is lazy (`Scope::ensureElaborated`, `Scope.h:261`).
  - Caching is disabled when names reach upward out of the instance, and for bind, defparam and config (`InstanceCacheKey.cpp:18-21`, `ElabVisitors.h:568-573`).
- **Yosys** has no separate instance object.
  - A `Cell` is a primitive or an instance: its `type` names another module, and it maps port → `SigSpec` plus parameters (`rtlil.h:2495-2515`).
  - Parameterized modules are cloned from the kept original AST under the name `$paramod\Name\P=val…` (a SHA-1 if longer than 60 characters). An existing module with that name is reused, which is the whole dedup (`frontends/ast/ast.cc:1787-1834`).
- **FIRRTL targets** (`spec.md` §Targets, 3850-3918):
  - `~|Bar` means every instance of `Bar` (a "local" target); `~|Foo/a:Bar/c:Baz` means one instance path (a "non-local" target).
  - The design is stored "folded", one copy per module, and can be unfolded.
- **Chisel:** it used to emit one module per `Module(...)` call and relied on CIRCT's structural-hash `Dedup` pass. `Definition`/`Instance` now elaborate each parameterization once (`cookbooks/hierarchy.md:13-23`).
- **Instance paths:** slang joins with `.` from the parent scopes (`Symbol.cpp:93-130`). Unnamed generate blocks get positional names, `genblkN` (`BlockSymbols.cpp:673-688`).

## 3. Connectivity

- **Yosys:**
  - A `SigSpec` is a list of wire bits or constants. `Module::connections_` is a list of `(lhs, rhs)` pairs, and nothing is merged when a connection is added (`rtlil.h:2077`).
  - `SigMap` is a union-find over bits that maps every bit to a representative (`sigtools.h:276-339`; the `mfp` union-find, with path compression and "promote", is at `hashlib.h:1426-1472`).
  - Which name survives a merge is deterministic (`passes/opt/clean/wires.cc:93-143`): input ports, then public names, then output ports, then the most attributes, then the lowest name.
- **FIRRTL:**
  - Digital `connect` is *directed*. Every expression has a **flow** (source, sink or duplex), and an *input* port is a source inside its module (the flipped view, `spec.md` §Flow, 1517-1554). Initialization coverage makes an unconnected wire an error (`spec.md:2264-2285`).
  - **`Analog` + `attach`** is FIRRTL's undirected, commutative join for inout nets, with no driver rule (`spec.md:865-893, 1838-1854`). That is exactly our net model.
- **slang:** the `DriverTracker` analysis reports `MultipleContAssigns`, with an "also assigned here" note (`DriverTracker.cpp:303-344`).
- **Yosys `check`:** "multiple conflicting drivers", and "used but has no driver" (`passes/cmds/check.cc:390-399`).

## 4. Flattening

- **Yosys `flatten`** (`passes/hierarchy/flatten.cc`):
  - It sorts modules topologically, flattens the leaves first, and refuses recursion (420-441).
  - For each instance it copies the child's wires and cells as `cellname.childname` (`concat_name`, 44-62).
  - Port wires become plain wires, and each port binding becomes a `connect` pair (206-267). Merging is left to `SigMap` and `opt_clean`.
  - Because names can contain dots, the true path is also stored as a space-separated `hdlname` attribute (87-108).
  - A `$scopeinfo` cell keeps the removed instance's attributes (270-296). `keep_hierarchy` opts an instance out (318).
- **CIRCT `ModuleInliner`:** works top-down, so each cloned operation gets its full prefix exactly once, with `_` as the separator: `test1_test2_test_wire` (`ModuleInliner.cpp:1285-1294`, `test/Dialect/FIRRTL/inliner.mlir:49-52`). Ports become wires (`mapPortsToWires`). Only marked instances are inlined.
- **slang never flattens.** In all three, flattening is an explicit, late step, and the hierarchy is kept for everything else.

## 5. Inference passes

- **FIRRTL width inference** is a whole-circuit constraint problem.
  - Every unknown width is a variable with `x ≥ max(…)` constraints (`InferWidths.cpp:614-657`), and cycles get special handling (`x ≥ max(a·x+b, c)`, 395-412).
  - Errors: "uninferred width" (1147), and "constrained to be wider than itself" with a note per constraint (`infer-widths-errors.mlir:5-13`).
  - Instance ports are unified with the module's own port variables (`InferWidths.cpp:1616-1627`). So every instance shares one width: the "minimum width … for all instantiations of the module" (Chisel `width-inference.md`).
  - `attach` forces equal widths in both directions (1572-1586).
  - Chisel's docs still say "users are encouraged to manually specify widths … to prevent any surprises".
- **For us:** "`47k` takes its unit from the `value:` field" is *checking against an expected type*. The field schema already fixes the unit, so no solver is needed.

## 6. Identity and stability

- **Where identity comes from:**
  - Yosys: public `\name` vs generated `$name` (`rtlil_rep.rst`), plus `hdlname` for paths;
  - FIRRTL: targets;
  - slang: the hierarchical path.
- **Chisel is the cautionary case:**
  - names come from Scala `val`s through a compiler plugin, after years of "trouble reliably capturing the names" (`naming.md:8-13`);
  - temporaries are named `_…` (`naming.md:281`);
  - slang's `genblkN` shifts when a sibling is inserted.
- **Source locations:**
  - slang uses an 8-byte `SourceLocation` (`overview.dox`);
  - Yosys uses a `src` attribute string, and without a scopeinfo cell, flattening *appends the instantiating cell's `src`* to every copied object (`flatten.cc:84-85`). So an origin is really a stack of locations;
  - Chisel prints `@[file line:col]`.

## 7. Diagnostics

- **slang coalesces** (`ASTDiagMap.cpp:25-110`, `TextDiagnosticClient.cpp:68-80`). Diagnostics are keyed by (code, location):
  - one raised in every instance prints once;
  - one raised in some instances prints "in N instances, e.g. top.a.b";
  - a single one prints "in instance: …".
- **slang checks definitions nobody places:** it instantiates them with invalid parameters (`Uninstantiated`) and checks them anyway (`Compilation.cpp:549-553`).
- **Top module:**
  - slang picks definitions that are never instantiated and have no parameters without defaults (`Compilation.cpp:343-373`, `NoTopModules`);
  - Yosys: "Design has no top module." (`hierarchy.cc:1110`).
- **Recursion:** slang has `MaxInstanceDepthExceeded` and `InfinitelyRecursiveHierarchy` (`ElabVisitors.h:329-350`); Yosys refuses to flatten recursive designs.
- **Unknown modules:** Yosys `hierarchy.cc:420`.
- **Which errors come when:** name, type and port errors belong to elaboration, per definition. Driver errors belong to a later analysis.

## 8. Testing

- **slang:** Catch2 unit tests compile an inline source string and assert on `diags[0].code`. There are dedicated caching regression tests (`HierarchyTests.cpp:2525`, `MultiAssignTests.cpp:735`).
- **Yosys:** `.ys` scripts run passes, then assert on the netlist with `select -assert-count` (`tests/various/scopeinfo.ys`, `hierarchy_param.ys`).
- **CIRCT:** lit + FileCheck `CHECK-NEXT:` on the printed IR, plus inline `// expected-error @+1 {{…}}` under `--verify-diagnostics` (`infer-widths-errors.mlir:1`).

## 9. Lessons (opinion)

1. **Keep two stages and name them the HDL way.**
   - `Design` is the folded form: one `Block` per definition, and each `Instance` refers to a `BlockId`.
   - `FlatDesign` is the unfolded form.
   - Driver and role checks run as a pass over the `FlatDesign`, like slang's analysis.
2. **Nets are undirected,** like FIRRTL's `Analog`/`attach`. There's no "one driver per net" rule; its analogue is role counting per merged net. Copy FIRRTL's port flip.
3. **Merge nets with a union-find over `NetId`s,** as Yosys's `SigMap` does.
   - Each port binding is a join. Collect all the joins, then resolve.
   - Choose the surviving name deterministically: the shortest (highest) path, then a port over an internal net, then declaration order.
   - Keep every alias.
4. **Store a path as interned segments, and print it with `.`.** Never parse a joined string back.
5. **Dedup key = `(BlockId, const-generic values)` only.** `param` values are per-instance data, so each resistor keeps its own knob.
6. **Flatten eagerly, top-down, as a pure function.** Keep an instance tree in `FlatDesign` (path → `BlockId`, parent), like Yosys's `$scopeinfo`, for sub-block contracts and composition checks.
7. **Don't build a unit solver.** Check each literal against the unit its field or port expects. A bare `47k` with no expected unit is an error, not an inference problem.
8. **Origins are a definition span id plus an instance path,** not a copied span.
9. **Diagnostics:**
   - definition-level errors once per `Block`;
   - instance-dependent errors with the path and the binding spans, coalesced slang-style;
   - check blocks nobody places;
   - detect recursive placement;
   - make the top explicit or unique.
10. **Require names written in the source;** never generate positional ones.
11. **Tests:**
    - snapshot a textual `FlatDesign` dump;
    - inline `//~ error` markers;
    - dedup and merge regressions, including two placements with different const generics.
