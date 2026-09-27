# Research: Circuit Tools (atopile, ngspice/Xyce, KiCad, spicy_parser) for the Elaboration Stage

> 2026-09-27 · Research report for `model.md` (M1d).
> Revisions: atopile `619eda7f` (2026-03-11, local `externals/atopile`), ngspice `56a152c7` (2026-09-25), Xyce `6243c628` (2026-09-21), KiCad `cda6040f` (2026-09-27, GitHub mirror), spicy_parser at `101c711`.
> Verified facts in §1–7; lessons in §8 are opinion.

## 1. Stages and names

**atopile** (`atopile/compiler/README.md:9-28`)
- **Compile:**
  1. ANTLR parses the source into a graph AST.
  2. A visitor emits "make child" and "make link" actions into a **type graph** (`gentypegraph.py:592,620`).
  3. The linker resolves imports (`build.py:483`).
  4. A deferred executor handles inheritance, retyping and `for` loops (`deferred_executor.py:1-10`).
- **Build** (`build_steps.py:475-703`):
  1. instantiate;
  2. infer and check units (`:504-505`);
  3. graph and design checks;
  4. pick parts (this is where the parameter solver runs);
  5. prepare nets: designators, grouping pins into nets, naming nets.
- **Paths:** a path is the parent–child edge names joined by dots (`core/node.py:1520-1555`).

**ngspice** expands subcircuits as text (`frontend/subckt.c:209`):
1. numparam pass 1 replaces each `{expr}` with a placeholder;
2. global nodes are collected: `0` and `.global` (`subckt.c:141-152`);
3. `doit` copies each subcircuit body under its X line, repeating up to `MAXNEST=21` times (`:422-765`);
4. numparam pass 2 evaluates the values (`:373-381`).

Names after expansion:
- a device becomes `r.x1.r1`;
- an internal node becomes `x1.n`;
- a port is replaced by the node the parent connects to it (`translate_node_name`, `:1107-1128`);
- a model becomes `x1:name` (`:1761`).

**Xyce**
- Pass 1 builds one context per `.subckt` definition, nested lexically (`N_IO_CircuitContext.C:240-275`).
- Pass 2 re-reads the definition's text once per X line (`N_IO_DistToolDefault.C:970-1163`).
- Names use `:`, as in `X1:R1` (`N_IO_DistToolBase.C:822`).

**spicy_parser** runs its phases in order (`lib.rs:152-158`):
1. includes;
2. statements;
3. brace placeholders;
4. `collect_subckts`;
5. `expand_subckts`: recursive, with a child `Scope` per instance in an arena (`subcircuit_phase.rs:279-336`);
6. instance parsing, which evaluates values in that scope;
7. lowering.

Names: `X1.R1`, and `X1.mid` for an internal node (`expr.rs:247-271`).

**KiCad**
- A sheet definition is a "screen"; a **`SCH_SHEET_PATH`** is the chain of sheet placements from the root to one instance (`sch_sheet_path.h:240-250`).
- The connection graph builds **subgraphs**, the connected items within one sheet instance, and then merges them across sheets into nets.
- Netlist exporters walk every sheet path and write one flat netlist.

## 2. Definitions vs instances

- **atopile:**
  - `instantiate_node` creates a fresh subtree for each placement, instantiating children recursively and resolving each link against *this* instance (`faebryk/core/zig/.../typegraph.zig:1814-2040`). Two placements share only the type.
  - There are no instance arguments. `r1.resistance = 10kohm +/- 5%`, written in the parent, is a constraint on the parent's type and applies to each instance's `r1` (`ast_visitor.py:1224-1230`).
- **ngspice** deep-copies the body per instance (`subckt.c:607`). Identity is only the name prefix.
- **Xyce:**
  - resolves a definition's context once, and again per instance only when the subcircuit has instance parameters (`CircuitContext.C:1126-1132`);
  - a random parameter becomes a per-instance global named `X1:param` (`:1072-1108`), effectively our per-instance statistical knob.
- **Per-instance parameter precedence:**
  - **Xyce:** an X-line value always beats a `params:` default and a local `.param`. X-line expressions are evaluated in the caller's scope, and lookup follows the enclosing *definition*, i.e. lexically (`CircuitContext.C:1165-1180, 2618-2640`; `DistToolDefault.C:1140-1156`).
  - **ngspice:** walks a stack of scopes in expansion order, i.e. dynamically (`numparam/xpressn.c:377-395`).
  - **spicy_parser:** walks outward through the scopes that placed the instance, like ngspice (`expr.rs:416-443`).
- **Probable spicy_parser bug:** `subcircuit_phase.rs:321-325` merges the defaults, then the instance values, then the local `.param`s. So a local `.param R=2k` beats an instance's `R=1k`, the reverse of Xyce.
  - `tests/subcircuit_inputs/subcircuits.spicy` has exactly this case (`X2 … R=1k`, with a local `.param R=2k`).
  - ngspice's behavior here is unconfirmed.
  - Related: a `.model` inside a subcircuit goes into the global table (`:99-101`), and nested `.subckt` definitions aren't supported (`:92`).
- **KiCad:**
  - A placed part on a shared sheet is one `SCH_SYMBOL`. Its reference and flags are stored per instance in `m_instances`, keyed by the `KIID_PATH` of the placement (`sch_symbol.h:1148-1154`; `sch_sheet_path.h:112-136`). So one object is R1 on one path and R5 on another (`sch_symbol.cpp:1504-1534`).
  - KiCad calls its per-instance value and footprint fields a "dubious decision" and is replacing them with variants (`sch_sheet_path.h:120-123`).
  - Nets are per path too: the same wire on two placements gives two nets.

## 3. Connectivity

**atopile**
- `a ~ b` adds an undirected edge and refuses mismatched interface types (`interface.zig:25-75`). There is no union-find.
- Nets are found by a graph search that walks through the hierarchy, joining same-named children of connected interfaces (`pathfinder.zig:200-330`). `F.Net` objects are created only at build time (`libs/nets.py:100-147`).
- **Net naming** (`libs/net_naming.py:675-723`):
  - A name the user forces wins; two conflicting forced names are an error (`:264-281`).
  - Otherwise names are scored: generic ones (`net`, `p1`) rank lower, names higher in the hierarchy rank higher (`:56-158`).
  - Collisions get a hierarchy prefix, then `-1`/`-2` (`:434-655`).
  - The order is deterministic, by sorted full names. `gnd` and `vcc` are never decorated.
- There is no global ground. `ElectricPower` is an interface with `hv` and `lv`.
- **ERC** (electrical rules check, `libs/app/erc.py:374-549`):
  - shorted power pair;
  - a part shorted across both ends;
  - several sources on one rail ("Power sources shorted");
  - `requires_external_usage`.

  The unconnected-pin check is commented out (`:537-545`).

**SPICE**
- Ports map by position (`subckt.c:1595-1621`). ngspice needs node counts per device, with special cases for E, G, K, W and POLY lines (`:1673-1695`).
- **Ground:**
  - ngspice auto-inserts `.global gnd` and rewrites `gnd` to `0` (`inpcom.c:1941-1946, 2377`);
  - Xyce leaves `0`, `$G…` and `.GLOBAL` nodes unprefixed (`DistToolBase.C:760-802`);
  - spicy_parser keeps only `"0"` global (`netlist_types.rs:162-166`) and has no `.global`.
- Xyce keeps an **alias map** from a port path (`X1:IN`) to the outer node, so `V(X1:IN)` resolves after flattening (`DistToolDefault.C:1036-1054`).

**KiCad**
- **Driver priority.** A driver is an item allowed to name its net; lowest to highest: `PIN < SHEET_PIN < HIER_LABEL < LOCAL_LABEL < LOCAL_POWER_PIN < GLOBAL_POWER_PIN < GLOBAL` (`connection_graph.h:72-83`, `.cpp:659-701`).
- **Within a subgraph:** strong drivers (hierarchical label and above) beat weak ones. Ties go through `compareDrivers` and finally alphabetical order (`.cpp:180-373`).
- **Across sheets:**
  - a sheet pin matches a hierarchical label by name (`:3222-3229`);
  - for the merged chain: a strong driver, then higher priority, then the *shorter sheet path* (the name nearer the top), then alphabetical order (`:3479-3521`).
- **Net name forms:**
  - local names get a path prefix, `/amp/base`; global labels and power pins stay bare, `GND` (`sch_connection.cpp:404-440`);
  - unnamed nets default to `Net-(R1-Pad1)`;
  - duplicates get `_1`/`_2`.
- **Two names on one net** raises `ERCE_DRIVER_CONFLICT` ("… will be used"); the netlist is still built (`:4069-4108`).
- **The new connectivity engine** uses union-find to build connected islands, then "claims" names by priority (`connectivity/conn_overview.h:32-220`).
- **Other ERC checks:**
  - a 12×12 matrix of pin-type conflicts (output–output is an error; `erc_settings.cpp:43-57`);
  - pin or power pin not driven (`erc.cpp:1640`);
  - pin not connected;
  - a sheet pin without a matching hierarchical label, and the reverse (`connection_graph.cpp:5087+`);
  - duplicate references;
  - a ground pin not on ground.
- **The SPICE exporter** makes any net whose last segment is `0` or `gnd` bare, so `/amp/GND` becomes `GND` (`netlist_exporter_spice.cpp:282-306`).

## 4. Values

- **atopile:**
  - A literal is a *set*: a union of intervals plus a unit, stored in base units (`Literals.py:3207-3290`). `10kohm +/- 5%` becomes [9.5k, 10.5k] (`ast_visitor.py:1481-1542`).
  - Tolerance units are checked (`DslTypeError`).
  - **No nominal or distribution is kept.**
  - `=` means ⊆ in a module and ⊇ in a component (`ast_visitor.py:846,860`), which is our "position decides meaning" rule (language §5.3).
  - The solver does interval arithmetic in which each literal is uncorrelated even with itself, so X − X = [−10, 10]. Correlation exists only through aliased parameters (`core/solver/README.md:60-200`).
  - Units are checked right after instantiation. The solver runs during picking, and a contradiction becomes `PickVerificationError` (`libs/picker/picker.py:575-589`).
- **SPICE:** plain numbers, evaluated after flattening:
  - ngspice: numparam pass 2;
  - Xyce: at context resolution;
  - spicy_parser: `ScopeRef::evaluate`, when the instance is parsed.

  Tolerances exist only as random-parameter functions (Xyce).
- **KiCad:** values are strings.

## 5. Identity and stability

- **SPICE:** mangled strings only; no stable id.
- **atopile:**
  - footprints are matched to the board through `atopile_address`, the instance path (`libs/part_lifecycle.py:687`; `build_steps.py:789-806`), so a rename breaks the match;
  - designators are assigned after picking and can be kept from the board (`libs/app/designators.py:24-140`).
- **KiCad:**
  - every item has a random UUID (`KIID`, `kiid.h:45,165`), which replaced creation timestamps;
  - paths are chains of sheet UUIDs (`/uuid/uuid/`), documented as unchanged when sheet parameters are edited (`sch_sheet_path.h:403-407`);
  - the netlist carries both `sheetpath names="/amp/"` and `tstamps="/uuid/"` (`netlist_exporter_xml.cpp:645-663`);
  - the board matches footprints by UUID path, falling back to the reference (`board_netlist_updater.cpp:2447-2450`), so renames and re-annotation don't lose layout;
  - the new engine gives a merged, split or renamed net its predecessor's net code.

## 6. Diagnostics found at this stage

- **ngspice:**
  - unknown subcircuit;
  - too few or too many nodes (`subckt.c:1166,1171`);
  - "infinite recursion", caught only by the depth limit;
  - formal/actual parameter count mismatch (`xpressn.c:1716`);
  - a redefined parameter is only a warning.
- **Xyce:**
  - undefined subcircuit;
  - wrong node count;
  - a duplicate port node mapped to different nodes;
  - a global in a port list that doesn't match;
  - model not found;
  - duplicate device names, checked *after* flattening, on the flat name (`N_IO_CircuitBlock.C:879`).
- **spicy_parser:**
  - `NotFound`, `ArityMismatch`, `NoNodes`, `PlacesItself` (`error.rs:212-237`), plus `CyclicParameter` and `UnknownIdentifier`;
  - recursion is found by cycle detection on the stack (`subcircuit_phase.rs:300`), which is better than ngspice;
  - no duplicate-device-name check, and `NotFound`/`ArityMismatch` carry no span.
- **atopile:**
  - redefinition and undefined symbol (`ast_visitor.py:223,363`);
  - unresolved types (`build.py:504`);
  - unresolved references at instantiation;
  - incompatible units;
  - ERC faults and design checks, accumulated per stage (`libs/app/checks.py:17-50`);
  - duplicate net names (`libs/app/pcb.py:94-107`).
- **KiCad:** the ERC list in §3 (`erc_settings.h:39-114`).

## 7. Testing

- **ngspice:** golden-output circuits end to end (`tests/regression/subckt-processing/global-1.cir`, `lib-processing/scope-*.cir`). The flattened netlist itself is never compared.
- **Xyce:** in-tree GTest unit tests; the regression suite lives in a separate Xyce_Regression repository.
- **spicy_parser:**
  - insta JSON snapshots of the whole expanded deck per input (`subcircuit_phase.rs:348-381`);
  - unit tests of `Scope` naming (`expr.rs:590-623`).
- **atopile:**
  - `test/compiler/test_runtime.py` compiles snippets and asserts on connectivity and parameters, e.g. that interface children connect by name (`:82-119`);
  - inline pytest classes for net naming and ERC.
- **KiCad:**
  - golden netlists looked up by both reference and UUID path: complex hierarchy, hierarchical renaming, global promotion (`qa/tests/eeschema/test_netlist_exporter_kicad.cpp`);
  - a driver-ranking test written after ties had been broken by heap address;
  - an old-vs-new engine differential test (`test_connectivity_reference.cpp`);
  - results must be the same for any worker count;
  - exported SPICE is run in ngspice, including an `NpnCeAmp`.

## 8. Lessons (opinion)

**Verified facts behind them:** every tool keeps one definition and creates per-instance data by walking the hierarchy. The per-instance data is small:
- KiCad: a reference and flags;
- Xyce: a prefix, a node map and the instance parameter values.

Net naming is a real problem everywhere: KiCad and atopile use total, deterministic rankings and still shipped non-determinism bugs.

1. **`Design` is per definition and checked once:** names, units and duplicates per block, once, with spans. Xyce and ngspice check only flattened strings, so one mistake in a block placed N times gives N errors with no span. Per-instance data in `FlatDesign` is only the path, the port → net map and the instance's field values.
2. **Nets: union-find over (instance path, net) slots, then a separate naming pass.** Port binding and `net x = [a, b]` are unions (KiCad's new engine). atopile's search at build time is the slow, late alternative.
3. **Naming:**
   - The name declared highest in the hierarchy wins; ties go to declaration order (KiCad's shorter sheet path; atopile's hierarchy ranking).
   - A port always takes the parent's net name. In `CeAmp` placed as `amp`, `input` becomes the parent's `guitar`; the internal net stays `amp.base`.
   - Keep **aliases** from every port path to its merged net (Xyce), so probes like `input.v` still resolve.
   - `net x = [a, b]` inside one block: `x` names the result. Never warn about the usual port merges.
4. **Ground:** no global nets. Exactly one `Ground`-typed root net must exist, or it's an error; it lowers to node `0`. ngspice's automatic `.global gnd` and KiCad's forcing of `/amp/GND` to `GND` at export are both patches for implicit globals.
5. **Paths as structured segments, with a stable id beside them later.**
   - The path is the display name and the knob name.
   - Add an optional stable id per instance (KiCad-style) *before* the editor and layout sidecar need one: atopile's path matching breaks on rename, KiCad's UUID path survives it.
   - The MVP can skip it.
6. **Values: nominal + spread, with per-instance knobs** (`amp.r1.value` and `amp2.r1.value` are separate).
   - atopile's sets lose the nominal and treat every literal as uncorrelated, so they can't express a lot or tempco tracking.
   - Xyce's per-instance `X1:param` globals are the same idea as our knobs.
   - Check units in `Design`, and keep knob values out of `FlatDesign`.
7. **Block parameters (`r: Tol<Ohm>`):** the instance's argument wins, evaluated lexically in the caller (Xyce's precedence). Separately, fix or document spicy_parser's order at `subcircuit_phase.rs:321-325`.
8. **Cycle detection with the whole chain in the error** (`A → B → A`).
9. **Checks at elaboration:**
   - every pin bound exactly once;
   - two `Power<Out>` on one net (atopile, KiCad);
   - a `Power<In>` with no source;
   - a two-terminal part with both pins on one net;
   - exactly one ground net.
10. **Tests:**
    - `FlatDesign` snapshots for small blocks;
    - `CeAmp` placed twice, with distinct `amp1.base`/`amp2.base` and shared parent nets;
    - a determinism test that shuffles statement order;
    - an end-to-end check that the flat design simulates like the equivalent SPICE deck (as KiCad runs its `NpnCeAmp` in ngspice).
