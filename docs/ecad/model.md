# Elaboration: from Syntax Tree to Flat Design (M1d)

> 2026-09-27 · Design note for roadmap M1d. **Status:** agreed 2026-09-27; implementation starting with *resolve*.
> What happens between the parser's syntax tree and the simulator-ready circuit: the stages, every structure, the checks, and the tests. Every decision says where it comes from.
> Research round (2026-09-27), five reports, each claim checked against the source:
> - **Compilers:** rustc `b373574e`, rust-analyzer `86493cee`, Spade `177e5c4`
> - **Hardware elaborators:** slang `97a2b64`, Yosys `30d6257`, FIRRTL spec `afee1ba`, CIRCT `048afe0`, Chisel `14b890c`
> - **Modelica:** language spec `18f00b1` (3.7-dev), OpenModelica `41a658f`, Modelica Standard Library (MSL) `4c40388`
> - **Circuit tools:** atopile `619eda7`, ngspice `56a152c7`, Xyce `6243c628`, KiCad `cda6040f`, our `spicy_parser`
> Full reports: `research/model_compilers.md`, `model_hdl.md`, `model_modelica.md`, `model_circuit_tools.md`, `model_units.md`.
> - **Units:** uom `a465bcc`, Unitful.jl `550d5f1`, pint `e4042bb`, F# units of measure (Kennedy, "Types for Units-of-Measure"), OpenModelica's unit checker, atopile

---

## 0. What this stage is, and what we call it

The parser gives us a tree of what the text **says**: names are just text, and `47k` has no unit yet. The simulator needs a flat list of devices connected to numbered nodes. The stage in between answers what the text **means**.

**The name: elaboration.** It's what the hardware languages call this stage (slang, Yosys, Chisel), and "elaborated" is already the word in our roadmap (§2.4). Modelica calls the same thing *instantiation* and *flattening* (spec §5.6). rustc calls its version *name resolution* and *lowering*.

Elaboration has two steps, with two outputs:

```
 AST ───resolve───► Design ───flatten───► FlatDesign + KnobTable ───(M1e: lower)───► Lowered
 (spicy_lang)       (spicy_model)          (spicy_model)
```

| Step | Question it answers | Output | Like |
|---|---|---|---|
| **resolve** | What does each name refer to? Are the units right? Is every pin bound? | **`Design`**: each block **once**, as written, with names resolved and values typed | slang's elaborated definitions, Modelica's class tree, rustc's HIR |
| **flatten** | What does the whole circuit look like, with every placement expanded? | **`FlatDesign`**: every device by its path (`amp.r1`), nets merged, plus **`KnobTable`**: every toleranced value and assumption as a knob | Yosys `flatten`, Modelica's flat model, SPICE's subcircuit expansion |

**Why two outputs and not one:** every reference keeps a *folded* form (one copy per definition) apart from the *unfolded* form (one copy per placement).
- Errors in a definition are found and reported **once**, with its source position, even if the block is placed ten times. ngspice and Xyce only check after expanding subcircuits into text, so one mistake in a subcircuit placed N times gives N errors with no position.
- The editor draws the folded form (a block's sheet). The simulator and engine need the unfolded form.
- A later edit to one block only redoes that block's resolve step.

---

## 1. One example through both steps

A board that places the amplifier twice shows everything flattening does. (`ce_amp.spl` alone is the special case with no placements.)

```rust
block CeAmp { … as in circuits/ce_amp.spl … }      // ports vcc, gnd, input, output; nets base, emitter

block Stereo {
    port v12: Power<In>;
    port gnd: Ground;
    port left_in: Analog<In>;
    port right_in: Analog<In>;
    port left_out: Analog<Out>;
    port right_out: Analog<Out>;
    let left  = CeAmp { vcc: v12, gnd, input: left_in,  output: left_out };
    let right = CeAmp { vcc: v12, gnd, input: right_in, output: right_out };
}
```

### Step 1, resolve: each block once

```
Design
  block CeAmp                                   ports: vcc Power<In>, gnd Ground, input Analog<In>, output Analog<Out>
    nets     base, emitter
    r1   part Resistor  a→vcc  b→base   value = 47 kΩ ± 1% (relative)
    …
    q1   part Npn       c→output b→base e→emitter   beta = 100..=300 (ratio)
    contract: assume temp ∈ 263.15 K..=333.15 K, assume vcc.v ∈ 12 V ± 5%, measure h, specs bias/gain/bass
  block Stereo
    left   block CeAmp  vcc→v12  gnd→gnd  input→left_in  output→left_out
    right  block CeAmp  vcc→v12  gnd→gnd  input→right_in output→right_out
```

Nothing is copied yet. `r1` exists once, inside `CeAmp`. Units are known: `47k` became 47 kΩ because `Resistor.value` expects ohms.

### Step 2, flatten: every placement expanded, nets merged

```
FlatDesign (top = Stereo)
  devices
    left.r1    Resistor  a→N1  b→N2   value → knob left.r1.value
    right.r1   Resistor  a→N1  b→N5   value → knob right.r1.value
    …
  nets
    N1  v12        aliases: left.vcc, right.vcc
    N0  gnd        aliases: left.gnd, right.gnd          (the Ground net)
    N2  left.base
    N5  right.base
    N3  left_in    aliases: left.input
    …
  knobs
    left.r1.value    statistical  47 kΩ ± 1%
    right.r1.value   statistical  47 kΩ ± 1%      ← its own knob: two real resistors
    v12.v            range        (from Stereo's contract)
    temp             range
    …
```

What happened, in terms of the references:
- **Paths:** each placement's name is prefixed (`left.r1`), as Yosys (`cellname.childname`), Modelica (`amp.r1.R`) and our own `spicy_parser` (`X1.R1`) do.
- **Nets:** a port binding (`vcc: v12`) *joins* the child's port net with the parent's net. All joins go into a union-find, the same structure as Yosys's `SigMap`, Modelica's connection sets and KiCad's new connectivity engine. Each resulting group is one net.
- **Net names:** the name declared highest in the hierarchy wins (`v12`, not `left.vcc`), as KiCad (shortest sheet path) and atopile (hierarchy ranking) do. Every other name stays an **alias**, so a probe like `left.vcc.v` still finds its net, as Xyce's alias map does.
- **Knobs:** each leaf part field with a spread becomes its **own** knob per placement (`left.r1.value`, `right.r1.value`), because they are two physical resistors.

---

## 2. What the references do

| | **Compilers** (rustc, rust-analyzer, Spade) | **Hardware** (slang, Yosys, FIRRTL) | **Modelica** (spec, OpenModelica) | **Circuit tools** (atopile, ngspice/Xyce, KiCad) |
|---|---|---|---|---|
| **Stages** | Collect every definition's name and signature first, then resolve bodies (rustc early/late resolution; Spade's explicit passes; RA's DefMap fixed point) | Elaboration (names, parameters, hierarchy), then a separate analysis for drivers (slang); passes over one netlist: `hierarchy`, `flatten`, `opt_clean` (Yosys) | Instantiate the class tree into an instance tree, then generate the flat system (§5.6). OpenModelica: expand → instantiate → type → flatten → unit check | atopile: type graph → instances → unit check → solve → nets. SPICE: textual expansion of subcircuits. KiCad: per-sheet connectivity, then nets across sheets |
| **Definition vs placement** | Once per definition; a use stores only what it resolved to | slang caches one body per (definition, parameter values); Yosys clones parameterized modules as `$paramod` | Class once, one subtree per component; a tool may reuse "as long as the flat result is identical" (§5.6.1.4) | KiCad: one symbol object, per-instance reference numbering by sheet path; Xyce: one context per definition |
| **Connections** | — | Undirected analog nets: FIRRTL `Analog` + `attach`. Union-find: Yosys `SigMap` | Connection sets by union-find; potentials equal, flows sum to zero (§9.2); inside/outside faces (§9.1.2) | KiCad and atopile rank names deterministically; KiCad's new engine uses union-find |
| **Identity** | rustc `HirId` = (owner, local index): stable when other items move. RA: ids by (kind, name, disambiguator), never by position. Spade's global counters give no reuse across edits | Paths as identity; Yosys stores the true path separately because dotted names became ambiguous. Chisel struggled for years to name things | Paths of component names from the root; OpenModelica shares prefixes between names | KiCad: random UUIDs per item, so renames keep layout. atopile matches layout by path, which breaks on rename |
| **Values and units** | — | FIRRTL infers widths with a whole-circuit solver, and Chisel still recommends writing widths explicitly | Units are string attributes. OpenModelica checks them after flattening, as warnings, and only if a flag is set. Parameters have no spread | atopile turns `10kohm ± 5%` into an interval right away and loses the nominal. Its solver treats every value as uncorrelated even with itself (X − X = [−10, 10]) |
| **Errors** | Never stop. rustc's `ErrorGuaranteed` proves an error was already reported, so later stages stay quiet; RA returns `(T, Vec<Error>)` | slang reports an error once for the definition, or "in 2 of 3 instances, e.g. …"; it also checks definitions nobody places | Every message has file:line:col; flat equations remember which `connect` produced them | Missing ground only shows up later as a singular matrix (Modelica, SPICE). KiCad's electrical rules check (ERC) has a pin-type conflict matrix |
| **Tests** | RA: fixture text → textual dump, as inline snapshots. rustc: expected `.stderr` files | Scripts asserting on the netlist; FileCheck | 1,344 files, each ending in an expected flat model or error text | KiCad: golden netlists, plus a determinism test across thread counts |

---

## 3. Decisions

Each decision has **why** and **from**. Items marked *(later)* are designed now but not built for the MVP.

### 3.1 Structure

**E1. Two steps, two outputs: `Design` (folded) and `FlatDesign` (unfolded).**
*Why:* errors in a definition are reported once, with a position. The editor needs the folded form and the simulator the unfolded one. *From:* all five: slang's definition vs instance bodies, Modelica's class tree vs flat model, Yosys, KiCad's per-sheet analysis, and the ngspice/Xyce counter-example.

**E2. Where the code lives.**
- *resolve* (AST → `Design`) is in **`spicy_lang`**, because it needs the syntax tree.
- *flatten* (`Design` → `FlatDesign`) is in **`spicy_model`**, because it needs no syntax.
- The types of both outputs are in **`spicy_model`**.

*Why:* roadmap §2.3. `spicy_model` must not know syntax trees, and the engine and the editor read `spicy_model` without the language.

**E3. Roots: every block that no other block places is flattened on its own.**
- In a file with `CeAmp` and `Stereo`, `Stereo` places `CeAmp`, so `Stereo` is the only root.
- A file with `CeAmp` and a separate `PowerSupply` that nothing places has two roots, and each gets its own `FlatDesign`, checked standalone with its own default bench (language §8.5). That's what "check this block's contract" means anyway.
- `--top X` narrows it to one.
- A file whose blocks all place each other has no root, which can only happen through recursion (E14): an error.

For `ce_amp.spl`, `CeAmp` is the only root, and paths are relative to it (`r1`, not `amp.r1`).
*From:* slang (definitions that nothing instantiates become tops; `NoTopModules`), Yosys ("Design has no top module"). Unlike them, we don't need a single top, because every block with a contract can be checked standalone.

### 3.2 Resolve: names

**E4. Two passes.**
1. *Collect:* every block's name and its **signature** (its ports and their types) from every item.
2. *Bodies:* resolve every statement.

Inside a body, collect all `port`, `net` and `let` names first too.

*Why:* it's what makes order irrelevant (language principle P4):
- a block can be placed above its definition;
- a net can be used before its `net` line;
- a feedback loop needs no forward declaration.

*From:* all three compilers (rustc's items-above-locals rule; Spade's explicit passes; RA's DefMap), and Modelica §4.3 ("used before they are declared").

**E5. Two namespaces.**
- **Kinds:** part kinds (`Resistor`), blocks (`CeAmp`), signal types (`Power`).
- **Values:** ports, nets, instances, measures, `env` quantities.

Lookup goes from the block's body, to the file's items, to the prelude last. A name found in the wrong namespace gets a typed error ("`vcc` is a port, not a part kind").
*Why:* a precise error for the most common confusion, and the prelude never shadows your own names. *From:* rustc (type/value namespaces, prelude searched last), Spade (`NotAUnit`, `IsAType` errors).

**E6. A `let` is classified by what its value resolves to.**
- `Resistor { … }` resolves to a part kind, so it's a **part instance**.
- `CeAmp { … }` resolves to a block, so it's a **placement**.
- `ac(…)` is a call to a measure function, so it's a **measure** (contract only).

*Why:* grammar.md §3 kept one `let` syntax for all three on purpose. *From:* Spade (an instance is an ordinary call expression, classified by its callee).

**E7. Errors never stop the stage.**
- `resolve` returns the design with its problems, `Vec<ResolveError>` (never a `Result`).
- A name that doesn't resolve becomes a placeholder holding a `Reported` token, which only the diagnostic sink can create. So a placeholder always proves its error was reported, and later steps skip it without adding cascades.
- An instance with one bad field keeps its other fields, so its knobs still appear.

*From:* rustc (`ErrorGuaranteed`), rust-analyzer (`(T, Vec<Error>)`, never `Result`), Spade (placeholder nodes; skip only the broken units).

### 3.3 Resolve: parts, fields and values

**E8. Part kinds are a schema.** Each part kind declares:
- its **pins** (`a`, `b`; `c`, `b`, `e`);
- its **fields**, each with a declared type (`value: Tol<Ohm>`, `beta: Tol<Ratio>`, `rating: Volt`) and whether it's required;
- nothing about simulation. Which device a part kind becomes is lowering's job (M1e).

For the MVP the prelude (`Resistor`, `Capacitor`, `Electrolytic`, `Npn`, `Pnp`, `Power`, `Ground`, `Analog`) is written in Rust, the shortcut in roadmap §2.5.
*Why:*
- the schema is what checks "every pin bound exactly once" and gives `47k` its unit;
- keeping devices out of it keeps `spicy_model` simulator-free (roadmap §2.3).

*From:* Modelica's standard library, where `Resistor` extends `TwoPin`: pins and parameters are the part; the equations come separately.

**E9. Field bindings.**
- Each field of an instance is a binding (the value, plus where it was written).
- A placement's value overrides the part kind's default.
- An unknown field is an error.
- A field given twice is an error with **both** positions.

*From:* Modelica modifiers (§7.2: outer overrides inner; unknown and duplicate modifiers are errors, the duplicate with both spans).

**E10. Values keep their nominal and their spread as written.**

```
Value { nominal: Quantity, spread: Exact | Rel(0.01) | Abs(0.05 V) | Range(lo, hi) }
```

*Why:*
- the budget check ("a ±5% part is looser than the ±1% budget", language §6.4) needs the relative form;
- the engine needs a nominal for every knob.

*From:* the units report. atopile converts `± 5%` to an interval right away and loses both; our knob model (engine.md §2) needs both.

### 3.4 Resolve: units

**E11. A dimension is a vector of exponents over six base units: s, m, kg, A, K, rad.**
- Two quantities are compatible exactly when their vectors are equal.
- Values are stored as `f64` in SI: `47k` → 47000.0 Ω, `-10°C` → 263.15 K.
- The way it was written (`4k7`) stays in the source text, reachable by span.

*Why these six:*
- every electrical unit is built from s, m, kg and A (V = kg·m²·s⁻³·A⁻¹);
- K is for temperature;
- `rad` keeps a phase from being compared with a gain by accident. Otherwise both are dimensionless, as in SI.

*Why checked while resolving, not by Rust's type system:* the checks are on user text and need readable errors. uom's compile-time errors are rustc type errors full of `typenum`.

*From:* pint, Unitful, OpenModelica and atopile all use run-time exponent vectors. atopile adds `rad` as a base, uom normalizes to SI.

*Integer exponents for the MVP.* Noise densities (V/√Hz) will need half-integer ones; the Modelica report recommends rational exponents. That's an internal change when noise arrives.

**E12. Unit inference: check against what the position expects. No solver.**
- A bare number takes the unit its position expects, but only in **value positions**:
  - a field (`value: 47k` → Ω);
  - a function argument (`at(1kHz)`);
  - an endpoint of `..=`;
  - the nominal of `±`;
  - the bound of a relation (`… in 4.6 ± 5%` → the measure's unit).
- Anywhere else a bare number is dimensionless.
- Computed values follow the same rule, by tracking whether a sub-expression is still unitless (refined while implementing):
  - If everything in it is unitless, the result is unitless and takes the expected unit at the end: `2 * 4.7k` and `1k + 2k` in a `value:` are 9.4 kΩ and 3 kΩ.
  - In `*` and `/`, a unit on one side makes the unitless side a plain factor: `1V / 1mA` is 1 kΩ, `2 * 1V` is 2 V.
  - In `+` and `-`, a unitless number next to one with a unit is an error, as in F#: `r + 5` where `5k` was meant.

*Why:* the field schema already fixes every unit, so there's nothing to solve.

*From:*
- F#: a bare literal is dimensionless.
- FIRRTL's counter-example: a whole-circuit width solver with its own error class, and Chisel still recommends writing widths explicitly.
- atopile: a range endpoint takes its partner's unit.

**E13. Three special cases.**
- **Temperature:**
  - a `°C` value is a *point* (an absolute temperature);
  - point − point is a *difference*, and point ± difference is a point;
  - point + point, and point × anything, are errors;
  - a difference is written in K (`temp + 15K`, `25°C ± 5K`); `± 5°C` gets an error with the fix `± 5K`.

  *From:* uom (separate point and interval types), pint and Unitful (their errors for offset units).
- **`%` and `ppm`:** after `±` they are always **relative** (`4.6 ± 5%` means ±0.23); anywhere else they are plain factors (0.01, 1e-6). *From:* atopile's tolerance rule, uom's and pint's `percent` = 0.01.
- **`dB`:** its own kind, never a dimension, accepted only where a level is expected (`f_low(-3dB)`). It means 20·log₁₀ of an amplitude ratio, the Bode and SPICE `vdb` convention, so −3 dB is a factor of 0.708. *From:* Unitful, which refuses to guess between 10·log and 20·log.

### 3.5 Flatten: instances, nets, knobs

**E14. Flatten eagerly, top-down, as a pure function.**
- Each placement's path is its parent's path plus its own name.
- Paths are stored as segments (`Vec` of names, or parent id + segment), never as a dotted string parsed back.
- A block that places itself, directly or through others, is an error showing the whole chain (`A → B → A`).

*Why:* designs are small. Segments avoid the dotted-name ambiguity Yosys had to patch with a separate attribute.

*From:*
- Yosys `flatten` (prefixing, refuses recursion);
- CIRCT's inliner (top-down, so each prefix is known at once);
- `spicy_parser`'s cycle check, which already beats ngspice's depth limit of 21.

**E15. Nets: a union-find, then naming.**
- Every (instance path, local net or port) is an entry.
- Each port binding and each `net x = [a, b]` is a union.
- Each resulting group is one `FlatNet`.
- **Naming** is a separate pass after merging:
  - the name declared highest in the hierarchy wins;
  - ties go to a port over an internal net, then to the name itself (alphabetical), never to declaration order: statement order must not change the meaning (E4, and the shuffle test below);
  - every other name stays an alias.

*Why:*
- net names must be deterministic, because knob names, probes and snapshots depend on them;
- aliases keep probes (`left.vcc.v`) working.

*From:* Yosys `SigMap` (union-find over bits; deterministic survivor choice), Modelica connection sets (OpenModelica's `DisjointSets`), KiCad (union-find, then the shorter sheet path wins), Xyce (the alias map from `X1:IN` to the outer node). KiCad and atopile both shipped non-deterministic net naming bugs, so a test shuffles statement order and requires identical names.

**E16. Knobs are created at leaf part fields, one per placement.**
- Every field whose value has a spread becomes a knob named by its flat field path (`left.r1.value`).
- Every `assume` on a top-level port quantity becomes a range knob (`vcc.v`).
- `temp` is the one environment knob, shared by everything.
- The knob records its nominal:
  - the written nominal for `±`;
  - the **midpoint** for a range. `beta: 100..=300` has nominal 200, as in the walkthrough.

*Why:*
- two placements are two physical boards' worth of parts, so they must vary independently;
- a knob's name must survive unrelated edits.

*From:*
- Xyce (a random parameter becomes a per-instance global, `X1:param`);
- the Modelica report's warning: a binding makes two values *identical*, i.e. fully correlated, which is wrong for part tolerances (language §7.1);
- engine.md §2.

The midpoint rule is the one choice here the references don't settle. It matches the walkthrough's numbers, and the engine can refine it later with a declared nominal.

**E17. The `FlatDesign` holds knob *references*, never knob values.**
- A device field is either `Exact(Quantity)` or `Knob(KnobId)`.
- Values at a knob point are applied in lowering (M1e).

*Why:* pipeline.md §3: one `FlatDesign` serves every run; each run only changes numbers. *From:* pipeline.md's `Binding` and circuit.md.

### 3.6 Flatten: roles and checks

**E18. Checks come in two tiers.**

| Tier | When | Checks |
|---|---|---|
| **1. Per definition** (in resolve) | Once per block | Unknown names, wrong namespace, unit mismatches, pins bound exactly once, unknown or duplicate fields, duplicate names in a block, a statement's value of the wrong kind |
| **2. Per flat net** (after flatten) | Once per design | Two `Power` sources on one net; a `Power<In>` with no source; no `Ground` net, or more than one; a two-terminal part with both pins on one net (warning); an isolated net, connected to nothing else through any part (warning; see §7, question 4) |

*Why:*
- tier 1 needs only the block's own text;
- tier 2 needs the whole connected circuit;
- language §9 already splits "shape" from "roles".

*From:*
- slang's split between elaboration and its later analysis of drivers;
- KiCad's ERC (power-out vs power-out, power pin not driven);
- atopile's ERC ("power sources shorted", both ends shorted);
- Modelica's weakness: a missing ground shows up only as a singular system later, with a hint string.

**E19. Roles use each net member's face.**
- A member of a net is either a port seen from *inside* its block, or a pin or port seen from *outside*.
- From inside, a block's own `Power<In>` port acts as the *source* for its parts (language §3.3). From outside it's a sink.
- The role check counts sources per net using these faces.

*From:* Modelica's inside/outside connectors (§9.1.2) and FIRRTL's port flip (an input is a source inside its module).

**E20. Ground.**
- There are no global nets.
- A `Ground` port is bound explicitly, like any other.
- Tier 2 requires exactly one net that carries a `Ground` member.
- Lowering (M1e) makes that net node 0.

*Why:* ngspice's automatic `.global gnd` and KiCad's forcing of `/amp/GND` to `GND` at export are patches for implicit global nets; the language decided against them (§4.1). *From:* the circuit-tools report, and MSL's `Ground` (an ordinary model with `p.v = 0`).

### 3.7 Identity, positions, diagnostics

**E21. Identity.**
- Inside `Design`: dense ids numbered **per block** (`NetId`, `InstanceId` within a `BlockId`), so an edit to one block can't renumber another.
- The **stable** identity everything outside refers to (knob names, probes, results, the editor) is the **path**.
- Ids may be renumbered on each revision; paths change only when you rename something.
- *(later)* A random stable id per instance, so layout survives a rename, is added before the editor needs it.

*From:*
- rustc `HirId` (owner, local index), stable when other items move;
- rust-analyzer (ids by name, never by position; "a churning id can cause a lot to be recomputed");
- KiCad (UUID paths survive renames; atopile's path matching doesn't);
- Spade (global counters: no reuse).

**E22. Positions live in a side table, `DesignSourceMap`, never in the data.**
- `Design`, `FlatDesign` and `KnobTable` derive `PartialEq` without spans.
- So "nothing changed" is a cheap equality test after a whitespace or comment edit.

*From:* rust-analyzer (a body's data and its source map are separate, so a whitespace edit changes only the map); pipeline.md decision 9.

**E23. Diagnostics.**
- A definition's errors are reported once, at the definition.
- An error that depends on placements gets the path and every binding position, grouped: "in 2 of 3 placements, e.g. `board.left`".
- Blocks nobody places are checked too.
- Every flat item keeps its provenance: its placement chain, and for a net, the bindings that merged it. That answers "why are these one net?".

*From:*
- slang (errors grouped across instances; uninstantiated definitions checked anyway);
- OpenModelica (each flat equation remembers the `connect`s that produced it);
- Yosys (flattening stacks the instance's source location onto copied items).

### 3.8 The contract

**E24. The contract is resolved like a body, into typed expressions over probes.**
- An `assume` becomes a condition on a port quantity or on `temp`, and a knob (E16).
- A measure (`let h = ac(output.v / input.v)`) becomes a typed expression:
  - probes (`output.v` resolves to a net's voltage);
  - analysis calls (`ac`, `dc`);
  - methods (`.at`, `.mag`, `.f_low`) from a small table of built-in measure functions with declared argument and result units.
- A spec becomes its relation, bound and confidence attribute.

In the MVP only the top block's contract is analyzed. Sub-block contracts are resolved and kept, for the composition checks later (language §8.8).
*Why:* the engine (M3) needs measures as values it can compute, with units checked (a spec comparing a gain with 30 Hz is an error now, not at run time). *From:* language §8.4, engine_flows.md. Modelica has no counterpart.

**E25. Exact vs design values** *(later, with `for` and `if`)*.
- Every value has a variability: `Exact < Design`.
- Anything that shapes the structure (array sizes, `for`, `if`) must be `Exact`.
- A binding may never be more variable than its slot.

The MVP has no structural constructs, so it's recorded only for now.
*From:* Modelica's variability order (§4.5) and its errors (`HIGHER_VARIABILITY_BINDING`, a non-evaluable `if` around a connect); language §5.5.

### 3.9 Incremental work

**E26. For the MVP, recompute everything on every edit,** which takes microseconds at this size. Each step is a pure function with outputs that implement `PartialEq`, so rust-analyzer-style caching can be added later without restructuring.
*Invariant to keep:* editing one block's body never changes another block's signature.
*From:* rust-analyzer (salsa; "typing inside a body never invalidates global data"). Spade's language server does the whole-project collection each time.

---

## 4. The types (sketch)

```rust
// spicy_model

pub struct Design {
    pub blocks: Vec<Block>,           // BlockId = index
    pub contracts: Vec<Contract>,     // one per block that has one
}
pub struct Block {
    pub name: Name,
    pub ports: Vec<Port>,             // PortId  (per block)
    pub nets: Vec<Net>,               // NetId   (per block; ports are nets too)
    pub instances: Vec<Instance>,     // InstanceId (per block)
}
pub struct Port { pub signal: Option<SignalType> }  // name and span: its net's (port i is net i)
pub enum SignalType { Pin, Ground, Power(Role), Analog(Role) }
pub struct Instance {
    pub name: Name,
    pub of: InstanceOf,               // Part(PartKindId) | Block(BlockId) | Error(Reported)
    pub pins: Vec<(PinOrPortId, NetId)>,
    pub fields: Vec<(FieldId, Value)>,
}
pub struct Value { pub nominal: Quantity, pub spread: Spread }
pub enum Spread { Exact, Rel(f64), Abs(f64), Range { lo: f64, hi: f64 } }
pub struct Quantity { pub si: f64, pub dim: Dimension, pub kind: QKind }
pub struct Dimension([i8; 6]);        // s m kg A K rad
pub enum QKind { Plain, TempPoint, Db }

pub struct Contract { pub assumptions: Vec<Assumption>, pub measures: Vec<Measure>, pub specs: Vec<Spec> }
pub enum MExpr { Probe(Probe), Const(Quantity), Call(MeasureFn, Vec<MExpr>), Binary(Op, Box<MExpr>, Box<MExpr>), … }

pub struct FlatDesign {
    pub instances: Vec<FlatInstance>, // the placement tree: path, block, parent
    pub nets: Vec<FlatNet>,           // name, aliases, members (with face)
    pub devices: Vec<FlatDevice>,     // path, part kind, pins → FlatNetId, fields → Exact | Knob(KnobId)
    pub ground: FlatNetId,
    pub contract: FlatContract,       // the top block's, with probes resolved to flat nets
}
pub struct KnobTable { pub knobs: Vec<Knob> }   // path, kind (Range | Statistical), nominal, lo, hi
pub struct DesignSourceMap { … }      // every id → span, and provenance for flat items
```

`Name` is an interned string; `Path` is a list of `Name`s, printed with `.` only at the edges.

---

## 5. `ce_amp.spl` through the stage (what the tests will pin)

- **Resolve:**
  - 1 block (`CeAmp`), with 4 ports, 2 nets and 6 part instances;
  - every pin bound once;
  - units: 4 in Ω, 1 in F, 1 ratio;
  - 1 contract, with 2 assumptions, 1 measure and 3 specs.
- **Flatten**, with `CeAmp` as the top:
  - 6 devices;
  - 6 nets: `vcc`, `gnd` (the Ground net), `input`, `output`, `base`, `emitter`;
  - tier-2 checks pass: `vcc` is a `Power<In>` port, so inside the block it's the source.
- **KnobTable, 8 knobs:**

| Knob | Kind | Nominal | Spread |
|---|---|---|---|
| `temp` | range | — | 263.15 … 333.15 K |
| `vcc.v` | range | 12 V | ± 5% |
| `r1.value` | statistical | 47 kΩ | ± 1% |
| `r2.value` | statistical | 10 kΩ | ± 1% |
| `rc.value` | statistical | 4.7 kΩ | ± 1% |
| `re.value` | statistical | 1 kΩ | ± 1% |
| `c_in.value` | statistical | 1 µF | ± 20% |
| `q1.beta` | statistical | 200 | 100 … 300 |

This is the "done when" of roadmap M1d: exactly these 8 knobs and 3 specs.

---

## 6. Testing

- **Case files** `test_data/model/{ok,err}/*.spl`, the same layout as the lexer and parser. Snapshots of three dumps (`Design`, `FlatDesign`, `KnobTable`), then the rendered diagnostics.
- **A coverage rule:** every error kind has a case.
- **Two placements:** `Stereo` above. Separate `left.base`/`right.base`, a shared `v12`, separate knobs.
- **Determinism:** shuffling the statements of every block gives byte-identical dumps. KiCad and atopile both shipped bugs here.
- **Stability:** adding an unrelated part leaves every other knob path unchanged.
- **Invariants on random and fuzzed input:**
  - elaboration never panics;
  - every flat net has at least one member;
  - paths are unique;
  - every device pin points to an existing net;
  - exactly one ground net, or an error.
- **End to end (M1e):** `ce_amp.spl` simulated through `Lowered` gives VC ≈ 5.52 V, as the equivalent SPICE deck does.

*From:* RA's fixture-and-dump tests, OpenModelica's expected flat models, KiCad's golden netlists and its determinism tests.

---

## 7. Questions: answered and open

1. **Names in code:** `elaborate` for the whole, `resolve` and `flatten` for the steps. **Agreed** 2026-09-27.
2. **Several unplaced blocks in one file:** **agreed** 2026-09-27: each one is a root and is flattened and checked on its own (E3), with `--top` to pick one. The earlier idea, "an error unless `--top`", is dropped: every block with a contract is meant to be checkable standalone.
3. **Nominal of a range knob:** the midpoint (β = 200). **Agreed** 2026-09-27. A way to declare a nominal gets added only if the midpoint turns out not to be enough.
4. **A net cut off from ground:** **agreed** 2026-09-27: a warning. What it means:
   - Two different things are easy to confuse:
     - an **isolated net**: nothing connects it to the rest of the circuit through any part;
     - a net that reaches ground only through capacitors, so it has **no DC path** (the input side of `c_in`, if nothing drives `input`).
   - The check here is only the first, a topological one. An isolated net has no electrical meaning. In a simulation it gives a singular matrix: ngspice fails with "singular matrix", and so would our KLU solver.
   - A warning, not an error, because while editing a half-finished schematic has isolated nets all the time. The design should still elaborate, and the editor still shows everything.
   - The engine (M3) refuses to *simulate* a design that still has one, with this warning as the reason, instead of a cryptic solver failure.
   - The no-DC-path case isn't a design error at all: the default bench drives `input`, so the path exists in simulation. If a bench leaves such a node floating, that's for lowering to report (M1e).
5. **Stable ids for layout:** **deferred.** It needs its own research on how the layout editor works and how a project stores layout data. Paths remain the identity for the MVP (E21).

## 8. Implementation notes (resolve, M1d-3)

Where the first implementation differs from the text above, and why:
- **The prelude's data is in `spicy_model`** (`prelude.rs`), not in `spicy_lang` as roadmap §2.5 had it. Flatten needs the signal roles, and lowering needs the part kinds; neither may depend on the language.
- **How the code is laid out** (the standard for every stage from here on): `resolve()` reads as the passes, one comment each. Pass 1 builds each block's `Signature` (its ports by name), a read-only table in pass 2. Pass 2 gives each body a `BodyResolver` (as rust-analyzer gives each body an `ExprCollector`) that declares the body's names, then resolves each statement into a value (an instance, a merge) and stores it only if the statement owns its name, so a second definition, even a second block, is checked and dropped. Every name goes through one `Scope::declare` (rustc's `try_plant_decl`), and every part is added to the block together with its span (`BlockBuilder`, as rust-analyzer's `alloc_expr`).
- **One problem type for every stage:** `Diag<K> { kind, span, related, fix }` in `spicy_lang::diagnostic`, generic over the stage's kind enum; `LexError`, `ParseError` and `ResolveError` are aliases (rustc has one `Diag`, Zig one `ErrorBundle`). Kinds carry typed data where there is some (`UnitMismatch` holds the expected `FieldType` and the found `Quantity`; the text is built when rendering).
- **No `Reported` token yet (E7):** a name that doesn't resolve becomes `InstanceOf::Error`, an unbound pin `None`. The rule "the error has already been reported" holds by construction in the one place these are created. A token becomes worth it when several stages create placeholders.
- **The temperature errors (E13) come with contracts (M1d-5).** No part field takes a temperature, so `± 5°C`, `± 5%` on a temperature and a bare temperature can only occur in `assume`. They're added and tested there.
- **One mistake, one error:**
  - a misnamed field (`resistance:` where `value:` is missing) is one error with the rename as its fix;
  - everything an instance is missing (pins, ports and required fields) is one error.
- **Fixes only when there's one right answer,** as in the lexer:
  - an unknown name with a single close match (`Resistr` → `Resistor`);
  - `47K` where ohms are expected → `47k`.

  A test applies every fix and checks that its error goes away and no other error appears (a fix that trades one error for another isn't the right answer). So:
  - the `k` fix is offered only for a lone literal whose whole suffix is `K` (`47K`), not for `5mK` or `1K + 1K`;
  - net suggestions list nets only (an instance would give "not a net"), and field suggestions only the pins and fields not given yet whose value fits (a name for a pin, a number for a field);
  - the shorthand `C { gnd }` is fixed as `gnd: gnd1`, keeping the pin.
- **Duplicates are checked, not merged (E7):** a second block, port, `net`, `let` or binding (`value: 1k, value: 5V`) of a name is reported and still checked for its own mistakes, but never enters the design or overwrites the first.
- **An unknown field written twice** (`valu: 1k, valu: 2k`) is one misspelling: both are reported, the rename rule counts it once, and only the first gets the fix (renaming both would give `value` twice).
- **A port with a wrong type stays a port** (`signal: None`), so its uses resolve and aren't reported again.
- **A value the parser already flagged isn't typed** (`1k +- 1%`, `a ± b ± c`): what the parser built is a guess.
- **Spreads can be scaled** by a plain factor: `2 * (1k ± 1%)` and `(10k ± 500) / 2`, as the parser's help for `2 * 1k ± 1%` suggests. Adding to a spread, or dividing by one, is `SpreadInArithmetic`.
- **A tolerance is relative only when written with `%`:** `± 1%`, `± (1%)` and `± 2 * 0.5%` alike. Anything else is absolute, in the nominal's unit: `47k ± 50` is ±50 Ω, and `± (1V / 1V)` is a plain 1 (±1 on `beta`, a unit mismatch on a resistance), not ±100%.
- **Suggestions are budgeted** (64 per file): each looks at every name in scope, so thousands of unknown names would be quadratic. Names are looked up by hash.

## 9. Found along the way

- **`spicy_parser` subcircuit parameter precedence** (`subcircuit_phase.rs:321-325`): a subcircuit's local `.param` overrides the value given on the `X` line. Xyce does the opposite, and ngspice is unconfirmed. To check against ngspice when it's installed.
