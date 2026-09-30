# Research: Circuit Tools for Flatten (M1d-4)

> 2026-09-27 · Reference report for `model.md` §3.5–3.7 (E14–E23), the flatten step.
> Revisions: our `spicy_parser` at `3e8cedd`, atopile `619eda7` (`externals/atopile`), KiCad `cda6040f`, Xyce `6243c62`, ngspice `56a152c7`, Modelica spec `18f00b1`, OpenModelica `41a658f`.
> **Verified in source** unless marked *(recall)*. The companion report `model_circuit_tools.md` covers the same tools for the whole stage; this one is only about flatten.

## 1. What each tool does

**Our `spicy_parser`** (`subcircuit_phase.rs:279-336`, `expr.rs:247-271`)
- **Naming:** a device inside `X1` becomes `X1.R1`. An internal node becomes `X1.mid`. A port is *replaced* by the parent's node string (`node_mapping` lookup, `expr.rs:260-264`). So the name "highest in the hierarchy wins" by construction, and nothing records that `X1.in` existed. There is no union-find and no alias.
- **Ground:** `"0"` is never prefixed (`expr.rs:265`). This is the one global node.
- **Numbering:** nodes are numbered by first appearance (`node_mapping.rs:5-6`), so the numbers depend on statement order.
- **Cycles:** a stack of subcircuit names being expanded (`:296-306`). `PlacesItself` carries only the name and span, not the chain (`error.rs:230`). A cycle that no top-level X line reaches is never found.
- **Errors:** the first error stops the expansion (`?` everywhere). `NotFound` and `ArityMismatch` carry no span (`error.rs:68`).
- **Params:** defaults, then the X line, then the local `.param` (`:321-325`), so a local `.param` beats the instance's value. That is the reverse of Xyce.
- **Tests:** insta snapshots of the expanded deck (`:349`).

**atopile** (`src/faebryk/libs/`)
- **Naming** (`net_naming.py:675-723`):
  1. Forced names first; two different forced names are an error (`:264-281`).
  2. Then suggestions ranked by `(depth, name)` (`:250-253`).
  3. Then implicit names scored by depth × a "badness" table (`:56-101`).
  4. Collisions get prefixes, then the lowest common ancestor, then `-1`/`-2` (`:590-655`).
- **The non-determinism fix:** every step now runs over nets sorted by `_get_net_stable_key`, the sorted full names of each net's interfaces (`:104-120, 696-697`). "Skip the first one (highest in hierarchy)" relies on that order (`:621`). The history before the fix isn't in our shallow clone.
- **ERC** (`app/erc.py`):
  - **Shorted part:** any R, C, fuse or `ElectricPower` whose two ends are on one bus. The fault carries the path between them and a source chunk (`:413-462`).
  - **Power sources shorted:** more than one `ElectricPower` with the `is_source` trait on one bus. The message lists *every* source's full name (`:464-484`).
  - All faults are accumulated rather than stopping at the first (`:408-411`).
  - The unconnected-pin check is commented out (`:537-545`).

**KiCad** (`eeschema/`)
- **Driver priority:** `PIN < SHEET_PIN < HIER_LABEL < LOCAL_LABEL < … < GLOBAL` (`connection_graph.h:72-83`).
- **Across sheets** (`connection_graph.cpp:3478-3521`), a candidate replaces the current best when it is:
  1. strong while the best is weak;
  2. of higher priority;
  3. of the same priority with a **shorter sheet path**;
  4. equal in all of these and alphabetically lower.
- **The ranking test** (`qa/tests/eeschema/test_hierarchy_driver_ranking.cpp`): subgraphs sit in an unordered set, so a rule that doesn't compare against the incumbent "resolves these by heap address". The test pins `ZZZ_SHALLOW` beating `AAA_DEEP`, so depth beats alphabetical order.
- **ERC:**
  - a 12×12 pin matrix: `PwrO`–`PwrO` is an error, `PwrO`–`PwrI` is OK (`erc/erc_settings.cpp:44-57`);
  - only `PT_POWER_OUT` drives a power net (`erc.cpp:322-325`);
  - "power pin not driven" puts its marker on the **`PT_POWER_IN` pin**, the consumer, with that pin's sheet path (`erc.cpp:1957-2030`).

  KiCad has no inside/outside view. That's why every KiCad user has to add `PWR_FLAG` symbols (*recall*).

**Modelica / OpenModelica**
- **Connection sets:** each member is a tuple of variable and face (`connectors.tex:387-391`). In OpenModelica, `Connector = {name, ty, face, cty, source}`, where `source` is the `DAE.ElementSource` provenance (`NFConnector.mo:62-69`). Two members are equal only if their faces are equal too (`:145-149`). A set is a `DisjointSets` (`NFConnectionSets.mo:50, 108-122`).
- **Faces:** a connector is *outside* if its reference starts at a connector of the current model (`crefFace`, `NFConnector.mo:313-327`).
- **The one-source rule:** a set may contain at most one inside `output` or one outside `input`, i.e. "at most one source of a signal" (`connectors.tex:574-576`). A block's own input, seen from inside, is the source. That is E19.
- **Words:** Modelica's *outside* connector is what `model.md` calls a port "seen from inside its block". Code comments should say which convention they use.
- **Flat names** are dotted component paths (`amp.r1.R`) (*recall*, spec §5.6).

**Xyce / ngspice** (the things **not** to copy)
- **Names:** Xyce builds `X1:R1` (`N_IO_DistToolBase.C:822`). ngspice builds `r.x1.r1` and `x1.n` (`subckt.c:1107-1131`). Both are mangled strings.
- **Globals:** ngspice inserts `.global gnd` automatically (`inpcom.c:1941`). Xyce leaves `0`, `$G*` and `.GLOBAL` nodes unprefixed (`DistToolBase.C:789-802`).
- **Recursion:** ngspice finds it only through `MAXNEST=21` (`subckt.c:422,763`).
- **Aliases:** Xyce maps `X1:IN` to the outer node, but only for names an output actually asks for (`N_IO_DistToolDefault.C:1036-1054`).
- **Per-instance random parameters:** a parameter whose expression is random becomes a per-instance global, `X1:param` (`N_IO_CircuitContext.C:1072-1108`, triggered at `:1283-1293`). This forces the whole subcircuit to be re-resolved for every instance. That is our per-placement knob, without the cost.
- **Topology warnings:**
  - "Voltage Node (n) connected to only 1 device Terminal";
  - "does not have a DC path to ground", computed by colouring nodes through each device's lead groups (`N_TOP_SerialLSUtil.C:222-455`; messages `N_TOP_Topology.C:1784-1811`).

  Both are collected in an `unordered_set`, so the warning order isn't deterministic.

## 2. Answers by question

| | spicy_parser | atopile | KiCad | Modelica | Xyce/ngspice |
|---|---|---|---|---|---|
| **1. Naming** | substitution: the parent's node wins | (forced, depth, name), sorted by stable key | (strength, priority, depth, alphabetical) | the path is the name | mangled prefix strings |
| **2. Members** | none kept | interfaces on a bus | pins and labels per subgraph | (variable, face, source) | nodes |
| **3. Rules** | none | shorted part, many sources | pin matrix, not driven | at most one source per set | 1-terminal, no DC path |
| **4. Ground** | `"0"` is global | `ElectricPower.lv`, not global | power symbols are global | an ordinary `Ground` model | `0`, `.global` |
| **5. Per-instance values** | X-line params (wrong order) | per-type constraints | per-path fields ("dubious") | bindings (fully correlated) | `X1:param` random globals |
| **6. Provenance** | spans on statements | source chunk per fault | sheet path per marker | `ElementSource` per connector | none |
| **7. Tests** | deck snapshots | inline pytest | golden netlists, ranking, worker count | expected flat models | golden simulation output |

## 3. Recommendations

**3.1 Entries and union-find.**
- Walk the placements top-down from the root, visiting each block's children **in name order** (names are unique per block).
- Each `FlatInstance` gets a base offset. The entry for `(instance, NetId)` is `base + net.index()`: dense, like Yosys `SigMap`.
- The unions are:
  - each placement's `pins[i] = Some(n)`, joining child `base + i` with parent `base + n`;
  - each merge's `net` with every net in `with`.
- Every union is recorded as `Binding{instance, port}` or `Merge{instance, index}`. That list answers "why are these one net?" (E23; OpenModelica's `source`).
- **Recursion is a separate pass over the whole `Design`**: a DFS over the edges of `InstanceOf::Block`, before any flattening.
  - It has to be separate: if `A` and `B` place each other, neither is unplaced, so there is no root, and a walk from roots would never report the cycle.
  - Report each cycle **once**, starting at its lowest `BlockId`, with the chain `A → B → A`.
  - The error points at the `InstanceSpans.kind` of the placement that closes the cycle, with the block's name span as `related`.
  - Flatten then skips any placement that is part of a cycle.
- Walk with an explicit stack, not recursion.

**3.2 Naming.**
- Each entry is a candidate with the key `(depth, kind, path segments)`. The smallest key wins. `kind` is 0 for a port and 1 for a net.
- Every other entry is an alias. Store the net's **entries**, sorted, rather than alias strings: the same list serves printing, probe lookup (`left.vcc.v` resolves to `left` and `vcc`, then to its entry, then to the flat net) and provenance. One home for the concept.
- `FlatNetId`s are numbered in sorted winner order, never by first appearance (`node_mapping.rs:5` is the counterexample). Then the `FlatDesign` itself is equal under a shuffle, not only its dump.

Worked examples:
- **A port bound to a parent net:** in `Stereo`, `left_in` (0, port) beats `left.input` (1, port), giving `left_in` with alias `left.input`. If `left.input` is bound to Stereo's internal `sig`, `sig` (0, net) still beats `left.input` (1, port), because depth comes before kind (KiCad's `ZZZ_SHALLOW`).
- **Two placements sharing a net:** `v12` has aliases `left.vcc` and `right.vcc`. The entries for `(left, base)` and `(right, base)` never union, so they stay two nets: `left.base` and `right.base`.
- **A merge `net x = [a, b]`** with all three internal at depth 0: E15 as written gives `a`, with aliases `b` and `x`.
  - **Decision needed:** a user who wrote `net x = …` probably expects `x`.
  - Option: add a kind between port and net for "merge target", like KiCad's label over pin.
  - `net x = [vcc, b]` gives `vcc` under either rule.

**3.3 Members and faces.**
- A member is one of:
  - `Pin{device, pin}`;
  - `Port{instance, port, face}`. The face is `Inside` only for the **root's** ports and `Outside` for every placed port.
- **Why not "every block's own `Power<In>` faces inward"?** In `Stereo`, net `v12` would then have three sources: `Stereo.v12` inside, and `left.vcc` and `right.vcc` inside. A placed block's inward view is exactly the parent's net, which already has its own source. This follows Modelica's rule that sets are formed per level (`connectors.tex:397`), and language.md:148,659, where only the top's `Power<In>` ports get bench sources.
- The role is a single function, `role(signal, face)`:

  | Signal | Face | Role |
  |---|---|---|
  | `Power<In>` | Inside | Source |
  | `Power<Out>` | Outside | Source |
  | `Power<In>` | Outside | Sink |
  | anything else | any | none (for now) |

**3.4 Tier-2 checks.**
- One helper per rule, each returning `Vec<FlatError>`. Members and errors are sorted by path.
- Errors whose cause is inside a definition are grouped by `(block, instance)`, with the list of placement paths in the kind (E23).
- Broken input (`None` bindings, `InstanceOf::Error`) is skipped silently (E7).

| Check | Computed as | Span / related | From |
|---|---|---|---|
| **Two sources** (error) | a net with 2 or more Source members | the 2nd source's span / the 1st's; the kind lists every source path | atopile `erc.py:476-484`, KiCad `PwrO`×`PwrO` |
| **Unpowered** (error) | a net with at least 1 Sink and 0 Sources | the first sink's binding span (`vcc: rail`); the kind lists every sink | KiCad marks the `PwrI` pin, `erc.cpp:2003-2025` |
| **Ground** (error) | the ground nets are those with a member whose signal is `Ground`. 0 of them: the root's name span. 2 or more: the 2nd net's winning entry / the 1st's | — | E20, MSL `Ground` |
| **Shorted part** (warning) | a device with 2 pins, both `Some`, both on the same flat net | the part's `let` statement, grouped ("in 2 of 2 placements") | atopile `erc.py:413-462` |
| **Isolated net** (warning) | a second union-find over flat nets that joins all of each device's pins; each component without the ground net is isolated. One warning per component, listing its nets. Skipped when there's no ground net | the winning entry's `BlockSpans.nets` span | Xyce's colouring, with every device as one lead group |

- Xyce's "1 terminal" warning is left out: it would fire on every top-level input.

**3.5 Knobs.**
- Each `FieldValue::Given(v)` whose spread isn't `Exact` becomes `Knob(id)`, with path = device path + field name (`left.r1.value`). An exact value becomes `Exact(q)`. `Unset` and `Invalid` get no knob.
- Knobs are numbered in path order.
- This is Xyce's `X1:param` without the per-instance re-resolution, because values stay in the `KnobTable` (E17).
- When blocks get parameters, the instance's argument wins (Xyce), not `spicy_parser`'s order.

**3.6 What to share with `spicy_parser`.** No code: it is string-based, case-insensitive and stops at the first error. Keep only its conventions:
- `.`-joined display paths;
- the port takes the outer node;
- a stack for cycles, but report the chain it already holds.

Fix its `PlacesItself` chain and its param order separately.

**3.7 Tests** (`test_data/flatten/{ok,err}`, dumps as snapshots)
1. `ce_amp` as the top: 6 devices, 6 nets, `gnd` as ground, 6 part knobs.
2. `Stereo`: `v12` with aliases `left.vcc` and `right.vcc`, one source on `v12` (the three-source trap), separate `left.base` and `right.base`, separate knobs.
3. **Shuffle:** permute the statements (ports included) *and* the block order. `FlatDesign` and `KnobTable` must be `==`, and the dumps byte-identical (KiCad's ranking test, atopile's stable key).
4. **Ranking:** a deep name that sorts first alphabetically loses to a shallow one; port vs net at equal depth; an alphabetical tie; `net x = [a, b]`.
5. Three levels: `board.rail` with aliases `s.v12` and `s.left.vcc`.
6. **Stability:** adding an unrelated part leaves every other knob and net name unchanged.
7. **Recursion:** self-placement; `A ↔ B` with no root, reported exactly once, chain in the message.
8. One err case per tier-2 kind; a shorted part inside a block placed twice gives one grouped warning.
9. Resolve errors upstream add no flatten errors.
10. **Fuzz invariants:** no panic, every net has at least one entry, paths unique, every pin's net exists, exactly one ground net or an error.
