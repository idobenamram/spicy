# Research: How Hardware and Compiler Flatteners Work, for `flatten` (M1d-4)

> 2026-09-27 · Research report for `model.md` §3.5–3.7 (E14–E23).
> Revisions: Yosys `30d6257`, CIRCT `048afe0`, slang `97a2b64`, Verilator `43c3c88`, rustc `b373574`, Spade `177e5c4`.
> Every file:line below was read in the source at these revisions. The recommendations at the end are opinion.
> Correction to the brief: Yosys's flatten is `passes/hierarchy/flatten.cc`, not `passes/techmap/`.

## 1. Yosys `flatten`

- **Driver** (`flatten.cc:411-441`):
  - it builds a `TopoSort` of modules (children before parents) from a worklist of the used modules;
  - `if (!topo_modules.sort()) log_error("Cannot flatten a design containing recursive instantiations.")` (437-438). The message has **no chain**: `TopoSort` can record loops (`kernel/utils.h:151-157`), but flatten doesn't ask for them;
  - then `flatten_module` runs for each module in that order (440-441).
  - It flattens **bottom-up and in place**: each module inlines its already-flat children. Inside a module, a cell worklist (305-329) catches cells added during flattening.
- **Renaming:**
  - `concat_name` gives `cell.child` for public names and `$flattencell.child` for generated ones (45-57);
  - `map_name` then calls `module->uniquify` (62), which quietly adds a suffix on a collision;
  - port wires become plain wires (158-162).
- **Port merging** (215-268): each binding becomes a `connect(lhs, rhs)` pair plus `sigmap.add(...)`.
  - `SigMap` is an `mfp` union-find over bits (`sigtools.h:283-338`; `imerge` and `ipromote` at `hashlib.h:1452-1472`).
  - The union picks an arbitrary root. **The name is chosen later, in a separate pass:** `opt_clean`'s `compare_signals` ranks input ports, then public names, then output ports, then attribute count, then the name (`passes/opt/clean/wires.cc:93-143`).
- **Provenance:**
  - `hdlname` holds the true path as space-separated segments, because dotted names are ambiguous (87-107);
  - a `$scopeinfo` cell, renamed to the old cell's name, keeps the instance's and the module's attributes and `src` (270-296);
  - with `-noscopeinfo`, the cell's `src` is appended to every copied object instead (84-85).
  - `keep_hierarchy` opts an instance out (318-322).
- **Tests:** `.ys` scripts with `select -assert-count` (`tests/various/scopeinfo.ys:13-26`), plus a self-instantiating module (`tests/various/hierarchy_recursive.ys`).

## 2. CIRCT

- **`hw-flatten-modules`** (`lib/Dialect/HW/Transforms/FlattenModules.cpp`):
  - bottom-up, `llvm::post_order` over the instance graph (239);
  - it inlines each use through MLIR's `inlineRegion` (325-333), deciding by heuristics (172-203);
  - names become `inst/name` (130-134);
  - `hw.hierpath` ops (hierarchical paths to one target) that go through the inlined instance drop that hop (108-128).
- **FIRRTL `ModuleInliner`** (`lib/Dialect/FIRRTL/Transforms/ModuleInliner.cpp`):
  - **top-down**: "recursing from each module to clone" (15-16). Modules are visited parents-first (2086-2117);
  - `processInto` recurses with `nestedPrefix = prefix + instance.getName() + "_"` (2005-2014);
  - `mapPortsToWires` turns each child port into a wire named `prefix + port` (1803-1837), so each prefix is computed once, at clone time (1293-1294);
  - names are only prefixed, never uniqued. Only inner symbols go through a namespace (`uniqueInNamespace`, 1246, 1603-1637).
- **Provenance:** `createDebugScope` makes a `dbg.scope(instanceName, moduleName, parentScope)` for every inlined instance (2077-2084). That is an **instance tree with parent pointers**, kept after the hierarchy is gone (test: `test/Dialect/FIRRTL/inliner.mlir:846-849`). The inliner's own cycle check is debug-only (855-866).
- **Recursion:** a separate pass, `CheckRecursiveInstantiation.cpp:24-57`:
  - it finds strongly connected components (`scc_begin`) of the instance graph;
  - it reports one `"recursive instantiation"` error per cycle, with **one note per edge in it**: `"A instantiates B here"` at the instance's location.
  - Tests use `--verify-diagnostics` with `expected-note` (`check-recursive-instantiation-errors.mlir:3-37`), including two separate cycles giving two errors ("TwoLoops").

## 3. slang

- **It never flattens.** It keeps an `InstanceSymbol`, and an `InstanceBodySymbol` with a `parentInstance` pointer (`InstanceSymbols.h:178-182`).
- **Paths are computed on demand,** walking parent pointers (`Symbol.cpp:93-125`). A revisit prints `<recursive>` (101-103).
- **Recursion:**
  - `activeInstanceBodies` is the set of bodies on the current descent (`ElabVisitors.h:329-338`);
  - going past `maxInstanceDepth` (default 128, `Compilation.h:163`) is `MaxInstanceDepthExceeded`;
  - a cached body that is already active is `InfinitelyRecursiveHierarchy` (343-351);
  - neither message has a chain: `"infinitely recursive instantiation of '{}'"` (`scripts/diagnostics.txt:1137-1138`).
  - It needs a depth limit only because SystemVerilog allows recursion that ends through parameters (`HierarchyTests.cpp:795-811`). **We don't:** with no parameters, any cycle is infinite.

## 4. Verilator `V3Inline`

- **Driver** (`V3Inline.cpp:707-773`):
  - it builds a module/cell graph and chooses which cells to inline by size (`--flatten` forces all of them);
  - then it inlines "bottom up (leaves into roots)" (662).
- **Renaming:**
  - the prefix is `cell + "__DOT__"` (587). Variables are renamed and their port direction is cleared (607-613);
  - `prettyName` turns `__DOT__` back into `.` for messages (`V3Ast.cpp:206`), the same dotted-name ambiguity as Yosys's.
- **What it keeps:** an `AstCellInline{cell name, original module name}` record per inlined cell, for hierarchical references and debugging (597-599).
- **Ports:**
  - `V3Inst` makes a pin bound to a plain variable an `AstAlias`, so both become one variable. The comment reads "The port is named first, so the net it connects to is the one that survives" (`V3Inst.cpp:55-98`, at 92): **the outer name wins**.
  - Everything else stays an assignment.
- **Recursion:** only a direct self-instantiation is allowed (it clones a `__Vrcm` copy, `V3LinkCells.cpp:610-630`). A longer cycle is "Unsupported: Recursive multiple modules" at a single module, with no chain (91-99).

## 5. rustc monomorphization collector

- **Driver:**
  - it collects the roots, then runs `collect_items_root` on each, in parallel (`collector.rs:1922-1937`);
  - a shared `visited` set dedups the work (361);
  - `collect_items_rec` is **plain recursion**, not a worklist (382, 603-611).
- **Recursion:** `recursion_depths: DefIdMap<usize>` counts how many times each *definition* is on the current path (655-686).
  - Past the limit, the error is `"reached the recursion limit while instantiating `{$instance}`"`, with a note: "`{def}` defined here" (`diagnostics.rs:5-14`; `tests/ui/recursion/infinite-function-recursion-error-8727.stderr`).
  - There is no chain, only the use site and the definition.
- **`UsageMap`:** `used_map` from each user to what it uses, and the inverse `user_map` (259-282). It's recorded once per item, which is a provenance table built during the walk.

## 6. Spade

- **Monomorphization** (`spade-hir-lowering/src/monomorphisation.rs`):
  - a `VecDeque` queue plus a `translation` map from (name, parameters) to the monomorphized unit, which dedups (67-69, 106-157);
  - it records a `request_points` entry from each item to its (parent item, request location).
  - **`add_mono_traceback` walks that parent chain, adding one "template traceback" line per hop** (186-207). That is the chain error we want, made from parent pointers.
  - `MonoKey` exists "to get deterministic ordering … to simplify MIR testing" (55-62).
- **Instance names** are `{name}_{counter}` (`spade-mir/src/unit_name.rs:81-104`): positional, so they shift when a sibling is added. **Don't copy that.**
- **`source_of_hierarchical_value`** resolves a path by walking segments through a per-unit map from instance name to unit (`spade-compiler/src/compiler_state.rs:338-397`). So a path stays segments all the way down.

## 7. What not to copy

- **In-place, bottom-up rewriting** (Yosys, Verilator, `hw-flatten-modules`). It exists because they mutate one netlist and may keep hierarchy. We build a new value.
- **`uniquify` / `_N` suffixes on collision** (Yosys 62, Spade). Segment paths under per-block unique names can't collide.
- **Dotted strings with a side attribute** (`hdlname`, `__DOT__`): store segments.
- **Depth limits** (slang, rustc): they exist for recursion that ends through parameters. With no parameters, a cycle check is exact.
- **Inlining heuristics, NLA (non-local annotation) rewriting, directed port flow** (bit widths, sign extension at 243-259): we have no partial flattening, no annotations, and our nets are undirected.

## 8. Recommendations

**`flatten(design, root) -> Flattened { flat, knobs, source_map, errors }`**, pure. Its top function reads as passes:

```text
// Pass 1, recursion: find each placement cycle in the Design's block graph, once per cycle, with its chain.
// Pass 2, the instance tree: expand placements top-down from the root; each FlatInstance gets its path.
// Pass 3, joins: one union per port binding and per `net x = [..]`, over (FlatInstanceId, NetId).
// Pass 4, nets: one FlatNet per group; its name and aliases chosen by rank.
// Pass 5, devices and knobs: every leaf part, with pins → FlatNetId and fields → Exact | Knob.
// Pass 6, tier-2 checks over the flat nets.
```

- **Pass 1: recursion, on `Design`, not on the flat tree.**
  - A DFS over `InstanceOf::Block` edges keeps a stack of `(BlockId, InstanceId)` pairs. A back-edge gives the chain `A → B → A`.
  - Report once per cycle, at the first placement, with one related span per hop ("`A` places `B` here", using `InstanceSpans.kind`). This follows CIRCT's SCC with a note per edge, with the chain shown the way Spade's traceback shows it.
  - Run it once for the whole design (a definition error, E23), before any root is expanded. Pass 2 then skips the placements that are part of a cycle, so it can't loop.
  - Test a diamond too (A places B twice, B places C), which must *not* be reported. A visited set in place of an on-stack set would report it; that is the difference between slang's `activeInstanceBodies` and a plain visited set.
- **Pass 2, `FlatInstance { parent: Option<FlatInstanceId>, name: Name, block: BlockId }`.**
  - The path is made of segments, rebuilt by walking parents, as slang (`Symbol.cpp:93`) and CIRCT's `dbg.scope` do.
  - Expand top-down (FIRRTL `ModuleInliner`) so each child knows its parent at creation.
  - **Expand children in name order, not statement order.** The shuffle test needs the same ids, not just the same names. Yosys sorts modules by name (`flatten.cc:423`) and Spade keeps `MonoKey` for the same reason.
- **Pass 3, union-find.**
  - One dense index: `base[FlatInstanceId] + NetId`.
  - A `Vec<u32>` of parents with path halving (Yosys `mfp`) is enough.
  - Unions pick any root. Naming is separate (Yosys: `SigMap`, then `compare_signals`).
  - Record each join's cause for provenance: `Binding { at, instance, pin }` or `Merge { at, merge }`.
- **Pass 4, naming.**
  - Each candidate is a (FlatInstanceId, NetId). The survivor is the smallest by: depth, then port over internal net, then path segment by segment.
  - This is Verilator's "outer net survives" (`V3Inst.cpp:92`) and Yosys's port-first rank, with the ties broken by name, never by order.
  - All other names are `aliases`, sorted.
  - `FlatNet { name: Path, aliases: Vec<Path>, members: Vec<Member> }`, with `Member { at: FlatInstanceId, net: NetId, face: Inside | Outside }` (E19).
  - **Pitfall:** a child's `Power<In>` port is on the merged net both from inside and from outside. Count it as a source only from the inside of the root. Otherwise each placement adds a phantom source (FIRRTL's port flip).
- **Pass 5:**
  - `FlatDevice { at: FlatInstanceId, name, kind: PartKind, pins: Vec<Option<FlatNetId>>, fields: Vec<FlatField> }`;
  - `KnobTable { knobs: Vec<Knob { path, field, kind, nominal, lo, hi }> }`, sorted by path;
  - `InstanceOf::Error`, `None` pins and `FieldValue::Invalid` are skipped silently (E7).
- **`FlatSourceMap`** (a side table, E22):
  - each FlatInstance and FlatDevice gets an origin `(BlockId, InstanceId)`, from which the span comes (`BlockSpans.instances[i].stmt`);
  - each FlatNet gets its joins.
  - That is an origin id plus a path, not copied spans: Yosys's `$scopeinfo` over its `src` stacking, and rustc's `UsageMap` as a table built during the walk.
- **Tests:** snapshot dumps (the Yosys `select -assert-count` and CIRCT `CHECK-NEXT` equivalent):
  1. `ce_amp`: 6 devices, 6 nets, 8 knobs (model §5).
  2. `Stereo`: `v12` with aliases `left.vcc` and `right.vcc`; `left.base` ≠ `right.base`; two `r1.value` knobs.
  3. Naming ties: a port vs an internal net at the same depth; alphabetical; 3 levels deep; a `net x = [a, b]` inside a child.
  4. Shuffle every block's statements: byte-identical dumps and an equal `FlatDesign` (`==`).
  5. Recursion: A→A; A→B→A (the chain and one span per hop); two separate cycles give two errors (CIRCT TwoLoops); a cycle nobody places is still reported; the diamond is *not* reported.
  6. Error placeholders produce no new errors.
  7. Stability: adding an unrelated part leaves every other path and knob unchanged.
  8. The invariants of model §6 (unique paths, pins point to existing nets, every net has a member).
