# Elaboration: from Syntax Tree to Flat Design (M1d)

> 2026-09-27 · Design note for roadmap M1d. **Status:** agreed 2026-09-27; implementation starting with *resolve*.
> 2026-09-30: examples and syntax updated to v5 (syntax_v5_plan.md).
> What happens between the parser's syntax tree and the simulator-ready circuit: the stages, every structure, the checks, and the tests. Every decision says where it comes from.
> Research round (2026-09-27), five reports, each claim checked against the source:
> - **Compilers:** rustc `b373574e`, rust-analyzer `86493cee`, Spade `177e5c4`
> - **Hardware elaborators:** slang `97a2b64`, Yosys `30d6257`, FIRRTL spec `afee1ba`, CIRCT `048afe0`, Chisel `14b890c`
> - **Modelica:** language spec `18f00b1` (3.7-dev), OpenModelica `41a658f`, Modelica Standard Library (MSL) `4c40388`
> - **Circuit tools:** atopile `619eda7`, ngspice `56a152c7`, Xyce `6243c628`, KiCad `cda6040f`, our `spicy_netlist`
> Full reports: `research/model_compilers.md`, `model_hdl.md`, `model_modelica.md`, `model_circuit_tools.md`, `model_units.md`.
> - **Units:** uom `a465bcc`, Unitful.jl `550d5f1`, pint `e4042bb`, F# units of measure (Kennedy, "Types for Units-of-Measure"), OpenModelica's unit checker, atopile

---

## 0. What this stage is, and what we call it

The parser gives us a tree of what the text **says**: names are just text, and `47k` has no unit yet. The simulator needs a flat list of devices connected to numbered nodes. The stage in between answers what the text **means**.

**The name: elaboration.** It's what the hardware languages call this stage (slang, Yosys, Chisel), and "elaborated" is already the word in our roadmap (§2.4). Modelica calls the same thing *instantiation* and *flattening* (spec §5.6). rustc calls its version *name resolution* and *lowering*.

Elaboration has two steps, with two outputs:

```
 AST ───resolve───► Design ───flatten───► FlatDesign (with its knobs) ───(M1e: lower)───► Lowered
 (spicy_lang)       (spicy_model)          (spicy_model)
```

| Step | Question it answers | Output | Like |
|---|---|---|---|
| **resolve** | What does each name refer to? Are the units right? Is every pin bound? | **`Design`**: each block **once**, as written, with names resolved and values typed | slang's elaborated definitions, Modelica's class tree, rustc's HIR |
| **flatten** | What does the whole circuit look like, with every placement expanded? | **`FlatDesign`**: every device by its path (`amp.r1`), nets merged, and every toleranced value and assumption as a knob | Yosys `flatten`, Modelica's flat model, SPICE's subcircuit expansion |

**Why two outputs and not one:** every reference keeps a *folded* form (one copy per definition) apart from the *unfolded* form (one copy per placement).
- Errors in a definition are found and reported **once**, with its source position, even if the block is placed ten times. ngspice and Xyce only check after expanding subcircuits into text, so one mistake in a subcircuit placed N times gives N errors with no position.
- The editor draws the folded form (a block's sheet). The simulator and engine need the unfolded form.
- A later edit to one block only redoes that block's resolve step.

---

## 1. One example through both steps

A board that places the amplifier twice shows everything flattening does. (`ce_amp.spl` alone is the special case with no placements.)

```rust
// CeAmp as in circuits/ce_amp.spl (§5): env ambient, block, circuit, setup Operating, contract.
pub block CeAmp { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }
circuit CeAmp { … }                                 // nets base, emitter; parts r1 … q1

pub block Stereo {
    v12: Power<In>,
    gnd: Ground,
    left_in: Analog<In>,
    right_in: Analog<In>,
    left_out: Analog<Out>,
    right_out: Analog<Out>,
}

circuit Stereo {
    let left  = CeAmp { vcc: v12, gnd, input: left_in,  output: left_out };
    let right = CeAmp { vcc: v12, gnd, input: right_in, output: right_out };
}
```

A block is two items: its header (the ports) and its `circuit` of the same name (the nets and parts).

### Step 1, resolve: each block once

```
Design
  block CeAmp                                   ports (header): vcc Power<In>, gnd Ground, input Analog<In>, output Analog<Out>
    nets     base, emitter                      (circuit)
    r1   part Resistor  a→vcc  b→base   value = 47 kΩ ± 1% (relative)
    …
    q1   part Npn       c→output b→base e→emitter   beta = 100..=300 (ratio)
  block Stereo
    left   block CeAmp  vcc→v12  gnd→gnd  input→left_in  output→left_out
    right  block CeAmp  vcc→v12  gnd→gnd  input→right_in output→right_out
  contract CeAmp                                matched to its block; contents in the next phase (§3.8)
```

Nothing is copied yet. `r1` exists once, inside `CeAmp`. Units are known: `47k` became 47 kΩ because `Resistor.value` expects ohms.

Each header and its circuit become one `Block`, so the `Design` is the same as before v5. `ce_amp.spl`'s `setup Operating for CeAmp` and its contract are matched to `CeAmp`. They, and its `env ambient`, aren't elaborated yet (§3.8).

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
    v12.v            range        (from Stereo's default setup; next phase)
    ambient          range        (the env that setup's `temp:` names; next phase)
    …
```

What happened, in terms of the references:
- **Paths:** each placement's name is prefixed (`left.r1`), as Yosys (`cellname.childname`), Modelica (`amp.r1.R`) and our own `spicy_netlist` (`X1.R1`) do.
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
- A file with `CeAmp` and a separate `PowerSupply` that nothing places has two roots, and each gets its own `FlatDesign`, checked standalone with its own default setup (`setup = …;` in its contract; before v5, a default bench derived from its assumptions, language §8.5). That's what "check this block's contract" means anyway.
- `--top X` narrows it to one.
- A file whose blocks all place each other has no root, which can only happen through recursion (E14): an error.

For `ce_amp.spl`, `CeAmp` is the only root, and paths are relative to it (`r1`, not `amp.r1`).
*From:* slang (definitions that nothing instantiates become tops; `NoTopModules`), Yosys ("Design has no top module"). Unlike them, we don't need a single top, because every block with a contract can be checked standalone.

### 3.2 Resolve: names

**E4. Two passes.**
1. *Collect:*
   - every block's name, and its **signature** (its ports and their types) from its header;
   - each `circuit`, `contract` and `setup` matched to its block by name: a circuit or contract by its own name, a setup by the name after `for`. Setup names are per block, so every block can have its own `Operating`.
2. *Bodies:* resolve every statement of every circuit.

Inside a circuit, collect all `net` and `let` names first too. The ports come from the header.

*Why:* it's what makes order irrelevant (language principle P4):
- a block can be placed above its definition;
- a circuit, contract or setup can come before its block's header;
- a net can be used before its `net` line;
- a feedback loop needs no forward declaration.

*From:* all three compilers (rustc's items-above-locals rule; Spade's explicit passes; RA's DefMap), and Modelica §4.3 ("used before they are declared").

**E5. Two namespaces.**
- **Kinds:** part kinds (`Resistor`), blocks (`CeAmp`), signal types (`Power`).
- **Values:** ports, nets, instances, measures, `env` quantities.

Setup names are apart from both, per block (E4).

Lookup goes from the block (its header's ports, its circuit's nets and instances), to the file's items, to the prelude last. A name found in the wrong namespace gets a typed error ("`vcc` is a port, not a part kind").
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
  - the bound of a relation (`… within 4.6 ± 5%` → the measure's unit).
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
  - a difference is written in K (`25°C + 15K`, `25°C ± 5K`). After `±` it may also be written in °C: `25°C ± 5°C` is ±5 K, since the position says it's a difference, and the SI and datasheets write `±0.5°C` (decided 2026-10-02);
  - a point is written in °C: a plain number (`25`) or a value in K (`300K`) where a temperature is expected is an error, with the same temperature in °C as the fix (`26.85°C`). In SPICE `K` means kilo (`.temp 300K` is 300 000 °C in ngspice), so K never reads as a point;
  - a spread is never a percentage (where zero is decides it) or a named temperature (a point).

  *From:* uom (separate point and interval types; an interval may be in degrees Celsius), pint and Unitful (their errors for offset units).
- **`%` and `ppm`:** after `±` they are always **relative** (`4.6 ± 5%` means ±0.23); anywhere else they are plain factors (0.01, 1e-6). So a plain-number const after `±` isn't supported yet: once named, `TOL = 1%` is the number 0.01, and whether it was relative can't be seen (decided 2026-10-02). *From:* atopile's tolerance rule, uom's and pint's `percent` = 0.01.
- **`dB`:** its own kind, never a dimension, accepted only where a level is expected (`f_low(-3dB)`). It means 20·log₁₀ of an amplitude ratio, the Bode and SPICE `vdb` convention, so −3 dB is a factor of 0.708. *From:* Unitful, which refuses to guess between 10·log and 20·log.

### 3.5 Flatten: instances, nets, knobs

**E14. Flatten eagerly, top-down, as a pure function.**
- Each placement's path is its parent's path plus its own name.
- Paths are stored as segments (`Vec` of names, or parent id + segment), never as a dotted string parsed back.
  - *Exception, for now* (decided 2026-09-29): a problem's data holds its paths as dotted strings (`SeveralSources.sources: ["dual.o1", "dual.reg.out"]`), because nothing reads them as data yet. When the editor does, they become segments or ids into the flat design, whichever it needs (`research/flatten_decisions_2.md` D2).
- A block that places itself, directly or through others, is an error showing the whole chain (`A → B → A`).

*Why:* designs are small. Segments avoid the dotted-name ambiguity Yosys had to patch with a separate attribute.

*From:*
- Yosys `flatten` (prefixing, refuses recursion);
- CIRCT's inliner (top-down, so each prefix is known at once);
- `spicy_netlist`'s cycle check, which already beats ngspice's depth limit of 21.

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
- *Superseded by setups (v5), and moved to the next phase:* the range knobs of `assume` (`vcc.v`) and the one `temp` knob. A range knob now comes from a setup field with a range or `±` (`vcc: Supply { v: 12V ± 5% }` gives `vcc.v`), and `temp` becomes the `env ambient` that a setup's `temp:` names (one knob, `ambient`). Designed in `research/contract_v4_review_implementation.md` §3.4–§3.5.
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
| **1. Per definition** (in resolve) | Once per block | Unknown names, wrong namespace, unit mismatches, pins bound exactly once, unknown or duplicate fields, duplicate names in a block, a statement's value of the wrong kind; a circuit, contract or setup for no block, a second circuit, a placed block with no circuit (§8) |
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
- `Design` and `FlatDesign` derive `PartialEq` without spans.
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

*Status (v5):* resolve today only matches each contract and setup to its block, and the `Contract` is empty. What they contain is resolved in the next phase.

**E24. The contract is resolved like a body, into typed expressions over probes.**
- *Superseded by setups (v5):* the `assume` statements. The environment is now a `setup S for X { … }`, and the contract names its default one (`setup = Operating;`). How a setup's fields and the `env`s it names become conditions and knobs is designed in `research/contract_v4_review_implementation.md` §3.4–§3.5, not here.
- A measure (`let h = ac(output.v / input.v)`) becomes a typed expression:
  - probes (`output.v` resolves to a net's voltage);
  - analysis calls (`ac`, `dc`);
  - methods (`.at`, `.mag`, `.f_low`) from a small table of built-in measure functions with declared argument and result units.
- A spec becomes its relation (`within`, `<=`, `>=`), bound and confidence attribute.

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
*Invariant to keep:* editing one block's circuit never changes another block's signature (its header).
*From:* rust-analyzer (salsa; "typing inside a body never invalidates global data"). Spade's language server does the whole-project collection each time.

---

## 4. The types (sketch)

```rust
// spicy_model

pub struct Design {
    pub blocks: Vec<Block>,           // BlockId = index; a header and its circuit, as one
    pub contracts: Vec<Option<Contract>>, // indexed like blocks
}
pub struct Block {
    pub name: Name,
    pub ports: Vec<Port>,             // PortId  (per block), from the header
    pub nets: Vec<Net>,               // NetId   (per block; ports are nets too), from the circuit
    pub instances: Vec<Instance>,     // InstanceId (per block), from the circuit
}
pub struct Port { pub signal: Result<SignalType, Reported> }  // name and span: its net's (port i is net i)
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

pub struct Contract {}                // empty until the next phase adds its default setup, measures and specs,
                                      // and setups, envs and consts to `Design` (contract_v4_review_implementation.md §3.1)
pub enum MExpr { Probe(Probe), Const(Quantity), Call(MeasureFn, Vec<MExpr>), Binary(Op, Box<MExpr>, Box<MExpr>), … }

pub struct FlatDesign {                // ids into the Design, never names copied out of it
    pub root: BlockId,
    pub instances: Vec<FlatInstance>, // the placement tree: block, (parent, the `let` in it)
    pub nets: Vec<FlatNet>,           // local nets (placement, NetId): the first names it, the rest are aliases
    pub devices: Vec<FlatDevice>,     // part kind, (placement, `let`), pins → Result<FlatNetId, Reported>, fields
    pub knobs: Vec<Knob>,             // source (a device's field), value, kind (Statistical | Range)
    pub grounds: Vec<FlatNetId>,      // the nets with a `Ground` port; lowering needs exactly one
    pub tainted: Option<Reported>,    // something in it is broken: not checked or simulated as a whole
    pub contract: FlatContract,       // (M1d-5) the top block's, with probes resolved to flat nets
}
pub struct Flat<'d> { pub design: &'d Design, pub data: &'d FlatDesign }  // names: path(), net_path(), knob_path()
pub struct DesignSourceMap { … }      // every id → span, and provenance for flat items
```

`Name` is an interned string; a `HierPath` is a list of names from the root (`left.r1`), printed with `.` only at the edges and built on demand from the ids, through `Flat` (rustc's `tcx.def_path_str(def_id)`).

---

## 5. `ce_amp.spl` through the stage (what the tests will pin)

`circuits/ce_amp.spl`, without its doc comments:

```rust
env ambient: Temperature in -10°C..=60°C;

pub block CeAmp { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }

circuit CeAmp {
    net base;
    net emitter;
    let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };
    let r2 = Resistor { a: base, b: gnd, value: 10k ± 1% };
    let rc = Resistor { a: vcc, b: output, value: 4.7k ± 1% };
    let re = Resistor { a: emitter, b: gnd, value: 1k ± 1% };
    let c_in = Electrolytic { p: base, n: input, value: 1uF ± 20% };
    let q1 = Npn { c: output, b: base, e: emitter, beta: 100..=300 };
}

setup Operating for CeAmp {
    vcc: Supply { v: 12V ± 5% },
    input: Signal { v: 0V },
    output: Load {},
    temp: ambient,
}

contract CeAmp {
    setup = Operating;
    let h = ac(output.v / input.v);
    spec bias: dc(output.v) within 4.5V..=6.5V;
    spec gain: h.at(1kHz).mag() within 4.6 ± 5%;
    spec bass: h.f_low(-3dB) <= 30Hz;
}
```

- **Resolve**, today:
  - 1 block (`CeAmp`): 4 ports from its header; 2 nets besides the port nets, and 6 part instances, from its circuit;
  - every pin bound once;
  - units: 4 in Ω, 1 in F, 1 ratio;
  - the setup and the contract matched to `CeAmp`, their contents not read yet.
- **Resolve**, next phase (`research/contract_v4_review_implementation.md` §4.2):
  - 1 env: `ambient`, 263.15 … 333.15 K;
  - 1 setup: `vcc.v` 12 V ± 5%, `input.v` 0 V (a point, so no knob), `output` an open load (unwritten is ideal), `temp: ambient`;
  - 1 contract: default setup `Operating`, 1 measure, 3 specs.
- **Flatten**, with `CeAmp` as the top:
  - 6 devices;
  - 6 nets: `vcc`, `gnd` (the Ground net), `input`, `output`, `base`, `emitter`;
  - tier-2 checks pass: `vcc` is a `Power<In>` port, so inside the block it's the source;
  - 6 knobs, one per part field with a spread (§9).
- **Knobs, 8,** once the setup is elaborated: flatten's 6, then the setup's `vcc.v` and the env's `ambient`, appended after them (`contract_v4_review_implementation.md` §3.5):

| Knob | Kind | Nominal | Spread |
|---|---|---|---|
| `r1.value` | statistical | 47 kΩ | ± 1% |
| `r2.value` | statistical | 10 kΩ | ± 1% |
| `rc.value` | statistical | 4.7 kΩ | ± 1% |
| `re.value` | statistical | 1 kΩ | ± 1% |
| `c_in.value` | statistical | 1 µF | ± 20% |
| `q1.beta` | statistical | 200 | 100 … 300 |
| `vcc.v` | range | 12 V | ± 5% |
| `ambient` | range | — | 263.15 … 333.15 K |

This is the "done when" of roadmap M1d: exactly these 8 knobs and 3 specs. The counts are those of v0.1; only `temp` became `ambient`.

---

## 6. Testing

- **Case files** `test_data/{resolve,flatten}/{ok,err}/*.spl`, the same layout as the lexer and parser. Snapshots of two dumps (`Design`, and `FlatDesign` with its knobs), then the rendered diagnostics.
- **A coverage rule:** every error kind has a case.
- **Two placements:** `Stereo` above. Separate `left.base`/`right.base`, a shared `v12`, separate knobs.
- **Determinism:** shuffling the statements of every circuit gives byte-identical dumps. KiCad and atopile both shipped bugs here.
- **Stability:** adding an unrelated part leaves every other knob path unchanged.
- **Invariants on random and fuzzed input:**
  - elaboration never panics;
  - every flat net has at least one member;
  - paths are unique;
  - every device pin points to an existing net;
  - every placeholder's block is tainted;
  - simulating a root gives its one ground net, or an error.
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
   - The check here is only the first, a topological one. An isolated net has no electrical meaning. In a simulation it gives a singular matrix. ngspice-42 prints "singular matrix: check node …", then goes on and reports the node at 0 V (measured 2026-09-28, `research/flatten_decisions.md` §C2), a silently wrong answer; our KLU solver would fail outright.
   - A warning, not an error, because while editing a half-finished schematic has isolated nets all the time. The design should still elaborate, and the editor still shows everything.
   - The engine (M3) refuses to *simulate* a design that still has one, with this warning as the reason, instead of a cryptic solver failure.
   - The no-DC-path case isn't a design error at all: the default setup drives `input` (`input: Signal { v: 0V }`), so the path exists in simulation. If a setup leaves such a node floating, that's for lowering to report (M1e).
5. **Stable ids for layout:** **deferred.** It needs its own research on how the layout editor works and how a project stores layout data. Paths remain the identity for the MVP (E21).

## 8. Implementation notes (resolve, M1d-3)

Where the first implementation differs from the text above, and why:
- **The prelude's data is in `spicy_model`** (`prelude.rs`), not in `spicy_lang` as roadmap §2.5 had it. Flatten needs the signal roles, and lowering needs the part kinds; neither may depend on the language.
- **How the code is laid out** (the standard for every stage from here on): `resolve()` reads as the passes, one comment each. Pass 1 builds each block's `Signature` (its ports by name, from its header), a read-only table in pass 2. Pass 2 gives each circuit a `BodyResolver` (as rust-analyzer gives each body an `ExprCollector`) that declares the circuit's names, then resolves each statement into a value (an instance, a merge) and stores it only if the statement owns its name, so a second definition, even a second block or circuit, is checked and dropped. Every name goes through one `Scope::declare` (rustc's `try_plant_decl`), and every part is added to the block together with its span (`BlockBuilder`, as rust-analyzer's `alloc_expr`).
- **One problem type for every stage:** `Diag<K> { kind, span, related, fix }` in the `spicy_errors` crate, with its rendering beside it (2026-09-29, as rustc's `rustc_errors` holds `Diag` and its emitters; it sat in `spicy_model::diagnostic` for a day, with the rendering in `spicy_lang`), generic over the stage's kind enum; `LexError`, `ParseError` and `ResolveError` are aliases (rustc has one `Diag`, Zig one `ErrorBundle`). Kinds carry typed data where there is some (`UnitMismatch` holds the expected `FieldType` and the found `Quantity`; the text is built when rendering).
- **The `Reported` token (E7), added 2026-09-29** once resolve and flatten both made placeholders. `Reported` is rustc's `ErrorGuaranteed`: a zero-size proof, made only by reporting an error (`Diag::report`) or by finding one a stage already reported (`Reported::among`, rustc's `DiagCtxt::has_errors`). The rule: `None` means absent, and "broken, and the user was told" holds the proof: `Port.signal: Result<SignalType, Reported>`, `Instance.pins: Vec<Result<NetId, Reported>>`, `FieldValue::Invalid(Reported)` (a wrong value, or a required field not written), `InstanceOf::Error(Reported)`, and `Block.tainted: Option<Reported>` (rustc's `tainted_by_errors`). A block is tainted when parsing its header or its circuit reported an error (the parser's `BlockDecl::broken`, `Body::broken`), when a lexer or resolve error lies inside either, when it's the first of two blocks with one name, and when a part's value reads a broken const (set where the field's placeholder is stored, as rustc's typeck taints a body that meets a type already holding an error). The first of two envs or consts with one name is `Err`, so its readers are broken too. Parse errors count by the body that reported them, not by position, as rustc taints the body whose checking reported one: a missing `}` is found at the next item but breaks the block left open. The parser's error nodes hold the proof too (`ExprKind::Error(Reported)`, `StmtKind::Error(Reported)`, `ItemKind::Error(Reported)`). A later stage has to meet `Err(Reported)` to get past a placeholder, so lowering can't quietly simulate a broken circuit.
- **The temperature errors (E13) came with `env` (contracts_plan.md step 1).** No part field takes a temperature, so `± 5%` on a temperature and a bare temperature can only occur in an `env`, a `const` or (from step 2) a setup's `temp:`.
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
- **Duplicates are checked, not merged (E7):** a second block, circuit, port, `net`, `let` or binding (`value: 1k, value: 5V`) of a name is reported and still checked for its own mistakes, but never enters the design or overwrites the first. A second contract, or a second setup of one name for one block, is a `Duplicate` too.
- **Items matched to blocks (v5, 2026-09-30).** Resolve reads each block's ports from its header and its statements from its `circuit`, and builds the same `Design` as before: every resolve and flatten dump stayed byte-identical (syntax_v5_plan.md step 3). The new errors:
  - `UnknownBlock` (`E-name`): a circuit, contract or setup for a block the file doesn't have, at the name (a setup's is the one after `for`);
  - `NoCircuit` (`E-circuit`): placing a block that has no circuit, at the placement. A block with no circuit that nothing places is an interface, and no error;
  - a second circuit for a block is a `Duplicate`, checked against the block's ports, then dropped.
- **An unknown field written twice** (`valu: 1k, valu: 2k`) is one misspelling: both are reported, the rename rule counts it once, and only the first gets the fix (renaming both would give `value` twice).
- **A port with a wrong type stays a port** (`signal: Err(Reported)`), so its uses resolve and aren't reported again.
- **A value the parser already flagged isn't typed** (`1k +- 1%`, `a ± b ± c`): what the parser built is a guess.
- **Spreads can be scaled** by a plain factor: `2 * (1k ± 1%)` and `(10k ± 500) / 2`, as the parser's help for `2 * 1k ± 1%` suggests. Adding to a spread, or dividing by one, is `SpreadInArithmetic`.
- **A tolerance is relative only when written with `%`:** `± 1%`, `± (1%)` and `± 2 * 0.5%` alike. Anything else is absolute, in the nominal's unit: `47k ± 50` is ±50 Ω, and `± (1V / 1V)` is a plain 1 (±1 on `beta`, a unit mismatch on a resistance), not ±100%.
- **Suggestions are budgeted** (64 per file): each looks at every name in scope, so thousands of unknown names would be quadratic. Names are looked up by hash.

## 9. Flatten: the plan (M1d-4, first version)

From the two reference reports, `research/flatten_hdl.md` (Yosys, CIRCT, slang, Verilator, rustc, Spade) and `research/flatten_circuit.md` (our `spicy_netlist`, atopile, KiCad, Modelica, Xyce, ngspice). Decided 2026-09-28.

**Where it lives.** Flatten is in `spicy_model` (`flat.rs` holds the types, `flatten/` the passes), so the engine and the editor can flatten a `Design` without the language. The problem type sits below both: `Diag<K>`, `DiagKind`, `Fix`, `Severity`, `Reported` and the rendering are the `spicy_errors` crate, `Span` is `spicy_span`, and the `id!` macro is `spicy_index`, small crates that `spicy_model` and `spicy_lang` depend on, as rustc has `rustc_errors`, `rustc_span` and `rustc_index` (decided 2026-09-29). `spicy_lang::elaborate` runs resolve, then flatten, and holds the case-file tests (`test_data/flatten/{ok,err}`).

**The passes**, one comment each in `flatten()`:
1. **Recursion, once per design.** A depth-first search over the `InstanceOf::Block` edges, with an on-stack set, so a diamond (A places B twice) isn't reported. Each cycle is reported once, with its chain (`A → B → A`), at the placement that closes it (CIRCT's `CheckRecursiveInstantiation`, Spade's traceback). It is a pass of its own because `A` and `B` placing each other leaves no root, so a walk from the roots would never see the cycle (`flatten_circuit.md` §3.1). Later passes skip a placement that is part of a cycle. The search takes blocks, and each block's placements, in name order: when cycles overlap, which placement closes one (what's reported, and what's cut) would otherwise depend on statement order.
2. **Roots.** Every unplaced block is a root (§7 question 2), flattened on its own, in name order.
3. **The instance tree.** Placements are expanded top-down, **children in name order**, so shuffling statements gives the same ids (Yosys sorts by name, Spade has `MonoKey`). `FlatInstance { block, origin: Option<LocalInstance> }`, `None` only for the root. A `LocalInstance { at, instance }` is one `let` inside a placement, as a `LocalNet { at, net }` is one net inside it (rustc's `HirId` is an owner and a local id). A name is looked up in the `Design` through the `let` it came from, and a path is rebuilt from parent pointers when something is reported or printed (slang, CIRCT's `dbg.scope`, rustc's `def_path_str`), never stored, and never a dotted string. (A first version stored every path, and took 20 s to flatten 10 000 nested levels; now it's 4 ms. Dropping the copied names, 2026-09-29, made flatten another 20–36% faster.) **A size limit:** past `MAX_PLACEMENTS` (a million), the tree stops growing and the root is reported (`E-size`, at the placement that went over) instead of flattened. Recursion is found exactly, so this is no depth limit (rustc's `recursion_limit`) but a size one: a block placed twice in a block placed twice, 20 deep, is a million placements from 20 lines, and the editor flattens on every edit.
4. **Joins.** A union-find over dense entries, `base[instance] + NetId` (Yosys `SigMap`), with one union per port binding and per `net x = [..]`.
5. **Naming.** In each group, the smallest key `(depth, rank, path)` wins. The rank is: port, then merge target (the `x` of `net x = […]`), then net. Every other entry is an alias. Depth comes first, as in KiCad's ranking test (`ZZZ_SHALLOW` beats `AAA_DEEP`) and Verilator's "the outer net survives". The path is compared without being built: placements are numbered in tree order, so at equal depth `(placement, local name)` orders exactly as the paths do. Nets are numbered in order of their winning key, so a shuffle of statements gives the same nets. (The `FlatDesign` keeps resolve's per-block ids for provenance, which follow declaration order, so the shuffle test compares the flat dump, which is everything by path.) The merge-target rank was decided 2026-09-28.
6. **Devices and knobs.** Devices are numbered placement by placement, in tree order, and each placement's parts in name order. Each field is `Exact`, `Knob`, `Unset` or `Invalid(Reported)`. A given value that varies becomes a knob; a spread of nothing (`± 0%`, `200..=200`) is `Exact`, as ngspice's `agauss` returns the nominal for one (decided 2026-09-29). A `Knob` holds where it comes from (`KnobSource::Field { device, field }`), its `Value` and its kind; its path, its identity (`left.r1.value`, Xyce's per-instance `X1:param`), is built from the source, not stored, so a deep hierarchy stays linear. The kind follows where the knob came from, not how its spread is written: a part's `± 1%` and its `100..=300` are both statistical (language.md §5.4). Range knobs come from setups and `env`s, in the next phase. The knobs are in the `FlatDesign`: the numbers that change per run are the engine's, not a second table's (decided 2026-09-29).
7. **The ground nets**, as their own step, on every root: every net with a `Ground` port, as plain facts. Lowering makes the one ground net node 0 (E20); that there's exactly one is checked when the root is simulated (below).
8. **The block's own checks** (E18 tier 2), after every root is flattened, on each root with nothing broken in it. One helper per rule:

   | Check | Computed as | Points at, and paths from |
   |---|---|---|
   | Several power sources (error) | 2 or more innermost sources meet (see *Innermost*) | the join where they meet (`o2: seven`), with where the first comes in as related; from the placement the join is in |
   | No power source (error) | sinks and no source on the net | where the first innermost sink comes in at the net's top (`vcc: rail`); from the net's top; only the innermost sinks listed |
   | Shorted part (warning) | a two-pin part with both pins on one net | its `let`; from its placement |

   **The checks for simulating a root** (decided 2026-09-29, as Modelica checks every class on its own but balances only the model it simulates, spec §4.8): exactly one ground net (0: the root's name; 2+: the second net, with the first as related), and isolated nets (a component of the device graph not connected to ground; the root's ports with a role count as connected, since the default setup drives its inputs and loads its outputs, `contract_syntax_v5.md` §1.3 rule 1; a warning at the component's first net, with paths from the placement the nets are all in). They run in `check_simulation`, which lowering calls first and which returns the ground net, and which the tests call on every root. A library block, which only a board places, needn't be a whole circuit on its own: a snubber across two `Pin`s, or an ADC with separate `agnd` and `dgnd`, has no error.

   **Faces:** a port seen from inside its block (`Inside`) exists only at the root. Every placed port is seen from outside. Roles: `Power<In>` inside and `Power<Out>` outside are sources, and `Power<In>` outside is a sink. If every block's own `Power<In>` counted as a source from inside, `Stereo`'s `v12` would get three sources (`flatten_circuit.md` §3.3). This is Modelica's rule that connection sets are formed per level.

   **Innermost:** a `Power<Out>` passed up a level is one source. A supply that exports its regulator's output puts `sup.v5` and `sup.reg.out` on one net. So only the innermost members of a net count: a placed block's port with another member inside the block is passing it on, as KiCad counts a net's pins and not its sheet pins. Each innermost port counts, as KiCad, Modelica and atopile count them: two outputs of one block on one rail are two sources (decided 2026-09-29), and so is an output with nothing behind it next to one fed by a regulator. Both rules come from one replay of the net's joins, each placement's inside before the placement (`power.rs`): a placed block's port is gathered when it's bound, and only if nothing inside the block is behind that port. Where sources meet in a placement is one problem, at the first join there that brings sources to where there already are some, listing every source that meets there: two regulators tied inside a block are reported there, two outputs of a block tied on a board at the board's binding. Outputs tied together inside their block with nothing behind them are one net there, so one source (KiCad's stacked pins). (Cutting the circuit into levels at the power ports, Modelica's connection sets, was tried and dropped: a supply that also ties a plain `sense` pin to its output would count its regulator twice.)

   **Each problem once** (E23): a check reports per placement, with its paths from the problem's scope. A problem is what it says (`FlattenProblem`) apart from where it is (`InBlock`), as slang keeps an instance and a count beside a diagnostic's arguments (`ASTDiagMap.cpp`). Flatten groups the reports of one problem (its span, related span and what it says) into one, and says where it is: "(in `Mid`)" when every placement of `Mid` has it, since it's then the block's own; "(in 2 of 3 placements of `Mid`, e.g. `a`)" when some do; nothing for a problem of the root itself. The count is over every root checked (over the one root simulated, for a simulation check): a placement in a broken root was never looked at.

   **Complete circuits only** (E7): a root isn't checked or simulated as a whole if it's tainted (`FlatDesign::tainted`): a block in it with an error inside (`Block::tainted`), or a placement cut for recursion. A statement the parser couldn't read, a second `let` or binding, or a merged net that doesn't resolve leaves no placeholder, and the checks would report the damage.

**Provenance** needs no side table in v1. A flat instance or device keeps `(parent, InstanceId)` and a net keeps its local nets `(FlatInstanceId, NetId)`, so every name comes from the `Design` and every span from the `DesignSourceMap` (rustc's `UsageMap` style: ids, not copied spans). A flat design is read through one `Copy` handle, `Flat { design, data }`, made once, so it's never read against another `Design` (rustc's `TyCtxt`, decided 2026-09-29). Recording the cause of each join, for "why are these one net?", comes with the editor.

**Not in this version:** the contract. Its range knobs (`vcc.v`, and `ambient`, which was `temp`) now come from setups and `env`s, not `assume`. They and the flat contract come in the next phase (M1d-5; `research/contract_v4_review_implementation.md` §3.4–§3.5, §3.9), so `ce_amp` flattens to its 6 part knobs.

**Tests:**
- `ce_amp` as the root: 6 devices, 6 nets, 6 knobs;
- `Stereo`: `v12` with aliases `left.vcc` and `right.vcc`, one source, separate `left.base` and `right.base`, separate knobs;
- the ranking cases: depth over name, port over net, the merge target, three levels deep;
- a shuffle of statements and blocks that must give `==` results and byte-identical dumps;
- stability: adding an unrelated part changes no other name;
- recursion: self-placement, `A ↔ B` with no root reported once, the diamond not reported;
- one error case per check, a shorted part placed twice giving one warning, and a problem in one of three placements saying which;
- resolve errors upstream add no flatten errors;
- fuzz invariants: no panic, unique paths, every pin on an existing net.

## 10. Found along the way

- **`spicy_netlist` subcircuit parameter precedence** (`subcircuit_phase.rs:321-325`): a subcircuit's local `.param` overrides the value given on the `X` line. Xyce does the opposite, and ngspice is unconfirmed. To check against ngspice when it's installed.
