# The Data Pipeline: Where Each Struct Lives, and Why

> 2026-09-25 · Design note. **Status:** the direction is agreed. Step 1 (cleanup) is done and committed. Steps 2 and 4 are deferred until the engine runs many simulations; the others are discussed one at a time (§9).
> Based on two research reports: `research/staged_ir_survey.md` (how compilers and circuit simulators pass data between stages) and `research/circuit_redteam.md` (our code, measured). References like **[MLIR20]** point into `bibliography.md` §14.

This note answers one question: **as a circuit travels from text to simulation results, what data structure holds it at each stage, which crate owns that structure, when is it built, how long does it live, and why does it exist at all?**

---

## 1. The principle: split by vocabulary and by rate of change

Mature compilers and simulators don't keep one "circuit" or one "program" struct that everything reads and writes. They keep a chain of representations. A new representation gets its own type for one of two reasons:

1. **The vocabulary changes.**
   - rustc lowers source text through AST (what the user wrote) → HIR (names resolved) → THIR (fully typed) → MIR (a control-flow graph) → LLVM IR, and each level speaks a different language [RDG].
   - MLIR generalizes this as "progressive lowering": structure is dropped deliberately, "only where the structure is no longer needed", because "attempts to raise semantics once lowered are fragile" [MLIR20].
2. **The rate of change differs.** Data that changes at different times, or is owned by different code, goes in separate places.
   - rustc doesn't write types into HIR nodes; it keeps them in a side table keyed by node ID [RDG].
   - rust-analyzer keeps source positions apart from semantic bodies, so typing whitespace doesn't invalidate type information [RA-ARCH].
   - Cranelift gives every entity a small `u32` ID and keeps extra per-entity data in dense side tables indexed by that ID [CLIF].

Circuit simulators have converged on the same split for devices. Every one read (ngspice, Xyce, Gnucap, OSDI/VACASK) keeps five kinds of device data, separated by how often each changes:

| Device data | Changes | Example (ngspice) |
|---|---|---|
| As written, and which values were given | when the netlist is edited | `RESresist` + `RESresGiven` |
| Resolved (defaults, model → instance) | when the netlist is edited | `ressetup.c` |
| Derived from conditions (temperature) | per run / temperature | `restemp.c` → `RESconduct` |
| Matrix locations (where the device writes) | when the circuit's structure changes | `RESposPosPtr`, `bindCSC` |
| State (Newton linearization, integration history) | per iteration / time step | `CKTstates` |

- **ngspice** crams all five into one struct per device, which is why it needs "given" flags to re-run a stage safely [NGSPICE].
- **OSDI** (the modern Verilog-A model interface) makes each stage an explicit call: set parameters → `setup_model` / `setup_instance(temperature)` → write matrix locations → `eval` + `load` per iteration [OSDI03].

**Our simulator today merges resolved values and matrix locations** in one struct (e.g. `spicy_simulate::Resistor` holds `resistance` *and* its `stamp` indices), and rebuilds it at the start of every analysis.

Two more rules from the same sources:
- **One source of truth.** Only the top representation is ever edited; every lower one is recomputed from it.
- **Small handles for provenance.** A device carries a tiny ID that leads back to its source, and names are looked up only at the edges. rustc gives every MIR statement a 12-byte source-info record; Cranelift keeps source locations in a side table [RDG, CLIF].

---

## 2. The pipeline

```
          ┌──── spicy_lang ──────┐   ┌──────── spicy_model ─────────────┐
.spl ───► │ SyntaxTree (lossless) ├──►│ Design ──elaborate──► FlatDesign  │
          └───────────────────────┘   └───────────────────────┬──────────┘
                                                              │ spicy_backends::lower   (once per design)
                                                              ▼
SPICE ──spicy_parser──► Deck ──lower (SPICE rules)──► ┌── spicy_circuit ──────────────────┐
                                                      │ Circuit    wiring (+ names table)  │ once per design
                                                      │ Params     model + instance values │ nominal + one per run
                                                      │ Conditions, Analysis               │ per run / per request
                                                      └────────────────┬───────────────────┘
                                                                       ▼
                                   ┌──────────────── spicy_simulate ─────────────────┐
                                   │ Plan       stamp locations, patterns, KLU symbolic │ once per Circuit, shared
                                   │ Derived    temperature-adjusted values             │ per run  (added with temperature)
                                   │ Workspace  matrix values, KLU numeric, solution    │ one per thread, reused
                                   │ TranState  integration history                     │ per transient analysis
                                   │ Solution   results by index                        │ per analysis
                                   └───────────────────────────────────────────────────┘
```

Read it top to bottom:
- **Above `spicy_circuit`**, things are about **what the designer wrote**: syntax, blocks, tolerances, knobs.
- **`spicy_circuit`** is the hand-off: **what to simulate**, in the simulator's vocabulary, from either front-end.
- **Below it**, things are about **how the simulator computes**: matrices, factorizations, time steps.

---

## 3. Every major struct: where, when, why

| Struct | Crate | Built by, when | Lives | Its job, which nothing else can do |
|---|---|---|---|---|
| `Deck` (`*Spec`, `Command`) | `spicy_parser` | parse, per SPICE file | until lowered (the CLI keeps it for error snippets) | What the netlist literally said. It holds SPICE's precedence ("instance beats `.model` beats default") and which values were given. **Only SPICE needs this, so SPICE-specific duplication lives here** |
| `SyntaxTree` | `spicy_lang` | parse, per edit | until the next edit | Reproduce the text exactly, for the formatter, editor/AI edits and error recovery |
| `Design` + `DesignSourceMap` | `spicy_model` | after parse, per edit | per design revision | Blocks as written (defined once, placed many times), values with spreads, names resolved, units checked. Spans go in a side table |
| `FlatDesign` + `KnobTable` | `spicy_model` | elaboration, per edit | per design revision | Hierarchy flattened. The one place that says which knob feeds which field. Holds no knob values |
| `Circuit` | `spicy_circuit` | lowering (either front-end), once per design | shared (`Arc`) by every run | The simulator's vocabulary: nodes, per-kind device wiring, each instance's model id. **Wiring only, no numbers.** Names and origins go in a `CircuitNames` side table the simulator never reads (`circuit.md` §4.9) |
| `CircuitNames` | `spicy_circuit` | lowering, beside the `Circuit` | as long as results are shown | Node names, device paths (`X1.R1`, `amp.r1`) and origins (spans). Only the edges read it: CLI, raw writer, engine, error messages |
| `Params` | `spicy_circuit` | lowering (nominal); backends (per run) | one run, reusable buffer | Resolved device numbers. Per kind, a model table shared by instances and an instance table (`circuit.md` §4.2): plain `Copy` structs, SI units, no strings, `Option` only for defaults that depend on another value. The unit a knob changes, and the level ngspice export works at |
| `Conditions` | `spicy_circuit` | backends / CLI, per run | one run | Temperature, later other global conditions |
| `Analysis` | `spicy_circuit` | SPICE lowering or the engine, per request | per request | "Run op / DC sweep / AC / tran". Lives here so an exporter doesn't import the simulator |
| `Binding` | `spicy_backends` | lowering, once per design | the engine job | Knob `amp.r1.value` → `Params.resistors[2].r`. Keeps knobs out of the circuit and the simulator |
| `Plan` | `spicy_simulate` | from the `Circuit`, once per design × solver | shared (`Arc`) by threads | Where each device writes (stamp arrays parallel to the device arrays), the sparsity patterns, and KLU's symbolic analysis. It allocates internal nodes and branch rows, and records the structure key it was built for (`circuit.md` §4.3). Replaces the `stamp` fields `setup_pattern` writes into devices today |
| `Derived` | `spicy_simulate` | from `Params` + `Conditions`, per run | one run | Temperature-adjusted values (like ngspice's temperature routine), computed once per run instead of in every Newton iteration. **Added only with temperature**; before that it would just copy `Params` |
| `Workspace` | `spicy_simulate` | per thread | reused across runs | Matrix values, right-hand side, KLU numeric factorization, solution buffers |
| `TranState` | `spicy_simulate` | per transient | one analysis | Integration history, indexed by device. Today it's a `HashMap` keyed by capacitor name |
| `Solution` → named results | `spicy_simulate` → backends / CLI | per analysis | caller | Results by index (fast); names attached only at the edge, for the raw writer, TUI and engine |

**Per-kind layout is kept.** `circuit.resistors[i]`, `params.resistors[i]`, `plan.resistor_stamps[i]` and `derived.resistors[i]` are parallel arrays with the same index `i`. That's today's struct-of-arrays across device kinds, plus side tables sharing the index, like Cranelift's `SecondaryMap`. The stamp loop zips them together.

Within one kind, a device's fields stay together in one struct (`Vec<ResistorParams>`), not one `Vec<f64>` per field, because a stamp reads all of a device's fields at once.

---

## 4. One resistor through every stage

```
.spl        let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };       ce_amp.spl:17
Design      Instance r1: Resistor, a→vcc, b→base, value = Tol(47kΩ, ±1%)
FlatDesign  device "amp.r1", value = Knob#2 "amp.r1.value" ∈ [46.53k, 47.47k]
Circuit     resistors[2] = { a: N1, b: N2 }  name "amp.r1"  origin #5        built once
Params      resistors[2] = { r: 47k, tc1: 0, tc2: 0 }                         nominal
Binding     Knob#2 → Params.resistors[2].r                                     built once
Plan        resistor_stamps[2] = { pp: 3, nn: 7, pn: 4, np: 6 }              built once, shared
─── run 17 (any thread) ───
Params      resistors[2].r = 47.47k        Conditions.temp = 60 °C
Derived     resistors[2].g = 21.07 µS                                          (once temperature exists)
Workspace   values[3] += g; values[7] += g; values[4] -= g; values[6] -= g
Solution    I(resistors[2]) → origin #5 → "amp.r1" → ce_amp.spl:17
```

The SPICE line `R1 vcc base 47k` becomes `ResistorSpec { resistance: Some(47k), model: None, … }` in the `Deck`. Its lowering applies the SPICE rules and produces the **same** `Circuit` + `Params` entry. Everything below `spicy_circuit` can't tell the two front-ends apart.

The node numbers and matrix indices above are illustrative.

---

## 5. How many definitions of a device, and why each exists

| Path | Representations of a resistor |
|---|---|
| SPICE | `ResistorSpec` → `ResistorParams` → `ResistorEval` (with temperature) |
| Language | a part-kind schema entry (data, not a Rust struct) → `ResistorParams` → `ResistorEval` |

- **`ResistorSpec`:** what a SPICE line said, before SPICE's rules are applied. It exists only on the SPICE path, so the secondary front-end carries the extra layer, not the core.
- **`ResistorParams`:** what the device *is*, after all defaults. It's the same for both front-ends, it's what a knob changes, and it's what gets exported to ngspice (which applies its own temperature equations, so derived values can't be exported).
- **`ResistorEval`:** how the device *behaves* in this run, at this temperature. Computed once per run; without it, temperature equations would run inside every Newton iteration.

Stamp locations (`Plan`) and history (`TranState`) are arrays of matrix positions and past values. They aren't parameter definitions.

**A part kind is not a device.** A part kind in the language (`Electrolytic`, a pinned `MMBT3904`) is a physical part: named pins, tolerances, ratings, datasheet data. A device is equations. The mapping is one-to-many:
- an electrolytic can lower to a capacitor, plus an ESR resistor on an internal node, plus leakage;
- a pinned transistor lowers to a BJT whose parameters come from its part record.

That's why the language doesn't reuse `ResistorParams` as its part type. The same distinction separates rustc's HIR from its desugared THIR [RDG]. The test: *does the solver need this number to compute currents?* If yes, it's a device parameter. Ratings stay in the model, and derating checks read results by path.

**Where defaults live:**

| Kind of default | Examples | Home |
|---|---|---|
| **Physics** (part of a device's equations) | Gummel–Poon `BR = 1`, `NF = 1` | `Default` impls on `spicy_circuit`'s parameter structs, shared by both front-ends |
| **SPICE dialect** | instance-over-model precedence; `scale` (`m` and `area` stay instance parameters, applied by the simulator: `circuit.md` §4.5); the fallback for a missing resistance (1 mΩ in ngspice, 1 kΩ in Xyce [XYCE]) | Only in `spicy_parser`'s lowering |
| **Depending on other values or the analysis** | an AC resistance that defaults to the DC one; a pulse rise time that defaults to the time step | Resolved per run, because a knob may change what they depend on |

**SPICE never lowers into `Design`.** Forcing the primary model to represent `.model` cards and subcircuit parameter semantics would push SPICE's duplication into the core. If people want SPICE designs in the editor, the right tool is a SPICE → `.spl` importer.

---

## 6. Data flow in the four main scenarios

**(a) CLI with a SPICE file**

```
f.spicy ─parse─► Deck ─lower─► Circuit + Params + [Analysis]
     Plan built once ─► Workspace ─► each analysis ─► Solution ─named─► raw file / TUI
```

**(b) The engine loop with `.spl`: many runs, same wiring, different numbers**

```
ce_amp.spl ─► Design ─► FlatDesign
  once:    lower ─► Arc<Circuit>, nominal Params, Binding ─► Arc<Plan>
  per run (thread t): Binding.apply(knob point) ─► Params_t ─► Workspace_t
           (warm start from the nominal solution)
           ─► Solution_t ─► measures by node id ─► affine form ─► verdict
```

**(c) ngspice export**

```
Circuit + Params(point) + Conditions + [Analysis] ─export─► SPICE text + name map
     ngspice -b ─► raw file ─read─► same ids ─► same measures
```

Exporting from `Circuit` means a part kind is expanded into devices in exactly one place (the language lowering). Exporting a SPICE file through `Deck → Circuit → export` also tests our own SPICE lowering against ngspice.

**(d) Back to the source (editor, AI, error messages)**

```
matrix row ─Plan─► device / node id ─Circuit─► origin #5 ─► "amp.r1" ─► ce_amp.spl:17
knob contributor ─Binding─► Params field ─► device ─► origin
```

Today's simulator errors carry no device or node, and one path panics (`dc.rs`). These chains fix that.

---

## 7. What the split buys: measured

The amplifier's operating point, release build, from `research/circuit_redteam.md`:

| Per run | Today | Planned, cold | Planned, warm start |
|---|---|---|---|
| Rebuild devices | 0.3 µs | ~0.1 µs (plain struct copy) | ~0.1 µs |
| Topology: pattern + KLU analyze | 1.2 µs | 0 (the `Plan` is built once) | 0 |
| Newton | 4.6 µs (18 iterations) | 4.6 µs | 0.45 µs (2 iterations) |
| **Total** | **~6.6 µs** | **~4.7 µs** | **~0.55 µs** |

- **Topology rework is 23–27% of every operating-point run today**, at 1, 10 and 100 copies of the stage.
- **Regenerating SPICE text and re-parsing it per run would add 72%.** That's why our simulator reads the language natively and SPICE text is only an export.
- **The biggest later levers:**
  - **Sparse AC:** the dense AC path spends 1.8 ms per frequency on a 100-stage circuit.
  - **Junction limiting:** cuts the 18 cold Newton iterations.
  - **Warm starts:** from the nominal solution, never from whatever a thread solved last, so results don't depend on scheduling.

---

## 8. Threads

- **Shared read-only** (behind `Arc`): `Circuit`, `Plan`, `FlatDesign`. They're plain data with no interior mutability, so `Send + Sync`.
- **One per worker thread:** `Params`, `Derived`, `Workspace`, `TranState`. Runs are then independent and can go in parallel.
- **Needed first:** KLU's `factor` currently takes the symbolic analysis as `&mut` (it revises a size hint), so either clone it per thread or change `factor` to take it by shared reference.
- **ngspice's shared library** holds one circuit per loaded library, so parallel ngspice runs need separate processes.

---

## 9. Steps

We go one step at a time and review each.

| Step | What | Status |
|---|---|---|
| **1. Cleanup** | Fix the subcircuit bug (§11), apply `m`/`scale`, align defaults with ngspice, and move CLI concerns (the `simulate()` dispatcher, raw-file config, AC printing) out of `spicy_simulate`, with tests guarding the behavior. Each fix starts from a failing test | ✅ Done, committed |
| 2. Stamp locations out of devices | Stamp indices move into parallel arrays owned by a `Plan`; transient history becomes indexed. No snapshot changes | Deferred: a speed-up that matters once the engine runs many simulations |
| 3. `spicy_circuit` | The crate with `Circuit`, `Params`, `Conditions`, `Analysis`; `spicy_parser::lower(&Deck)`; the simulator reads `&Circuit` and drops its parser dependency. No snapshot changes | ✅ Done (design: `circuit.md`): 3a `spicy_circuit` + lowering; 3b the simulator reads it (results bit-identical on every test netlist); 3c the parser no longer allocates branch rows |
| 4. One `Plan` + `Workspace` per circuit | Shared by all analyses of a circuit; reused across runs | Deferred, with step 2 |
| 5. Temperature | Adds `Derived`. The first deliberate result changes: thermal voltage becomes kT/q | To discuss |

These replace roadmap M2a–M2c's ordering. The roadmap links here.

---

## 10. Decisions

| # | Decision | Status |
|---|---|---|
| 1 | Several representations per device, each with its own job (§5) | **Agreed** |
| 2 | The SPICE front-end carries SPICE-specific layers; the core is designed for the language and the simulator | **Agreed** |
| 3 | Fix bugs and align defaults as their own step, test-first, with deliberate snapshot changes | **Agreed** |
| 4 | Per-run values as a full per-kind `Params` copy (vs a base circuit + a small "slot" vector) | **Agreed** (`circuit.md` §5) |
| 5 | `Analysis` types in `spicy_circuit` | **Agreed** (`circuit.md` §4.12) |
| 6 | ~~Keep the parser's MNA branch-row order for now~~ The simulator owns the order of unknowns; results print in circuit order | **Agreed, revised** (`circuit.md` §4.4): no simulator keeps netlist order, and KLU reorders anyway |
| 7 | ~~Fold `m`/`scale`/`area` into `Params` during SPICE lowering~~ Keep `m` and `area` as instance parameters; SPICE lowering folds only `scale` | **Agreed, revised** (`circuit.md` §4.5) |
| 8 | Clone KLU's symbolic analysis per thread for now | Proposed |
| 9 | Spans in side tables, not on every item (changes roadmap §2.4) | **Agreed** (`circuit.md` §4.9) |
| 10 | SPICE never lowers into `Design`; a SPICE → `.spl` importer later if needed | Proposed |

---

## 11. Problems the review found in today's code

| Problem | Where | Status |
|---|---|---|
| **Subcircuit instances shared internal nodes.** Two `DIV` instances solved to a single `mid` node at 3.5 V, instead of 5 V and 2 V | `expr.rs` (`Scope::get_node_name`), `instance_parser.rs` | ✅ Fixed in step 1: internal nodes are prefixed with the instance name (`X1.mid`, as in ngspice); ground stays global. Tests: `Scope` naming unit tests (`expr.rs`), `subcircuit_instances_get_their_own_internal_nodes` (parser), `subcircuit_instances_have_separate_internal_nodes` (simulator) |
| Subcircuit device names came out as `1_R1` (the `X` was stripped) | `statement_phase.rs`, `expr.rs` | ✅ Fixed in step 1: `X1.R1`, a dot path like the language's `amp.r1`. (ngspice writes `r.x1.r1` because it re-parses the text and needs the type letter first; Xyce writes `X1:R1`) |
| `m`, `scale` and `area` were parsed but never applied (`R 1k m=2` drew 1 mA, not 2 mA) | `devices/*.rs` `from_spec` | ✅ Fixed in step 1, following ngspice: R·scale/m, C·scale·m, L·scale/m, IS·area·m for the diode and BJT, folded into the resolved value once. Tests: each device's `from_spec` unit tests (`devices/*.rs`) |
| BJT default saturation current was `1e-14`; ngspice uses `1e-16` (`bjtsetup.c`) | `devices/bjt.rs` | ✅ Fixed in step 1. Tests: `defaults_match_ngspice` in `devices/bjt.rs` and `devices/diode.rs`. The other R/C/L/D/BJT and waveform defaults already matched |
| The simulator library owned CLI concerns (`simulate()` dispatcher, raw-file config, the raw writer) and AC printed to stdout on every frequency | `lib.rs`, `raw_writer.rs`, `ac.rs` | ✅ Fixed in step 1: all moved to `spicy_cli` (`batch.rs`, `raw.rs`); `SimulationConfig` only configures the solver; AC results have a named type (`AcResult`). Guarded by end-to-end tests of the binary written before the move, which passed unchanged after it. They now run `batch::run` directly: `mod tests` in `spicy_cli/src/batch.rs`, helpers in `spicy_cli/src/test_utils.rs` |
| The raw writer had no tests | `raw.rs` | ✅ Step 1: `batch::run` writes the raw file for op/DC/transient/AC and the test reads it back (header exactly; values exactly against the library run in-process) |
| Thermal voltage is a constant 0.02585 V; ngspice uses kT/q (0.025865 V at 27 °C) | `devices/diode.rs`, `devices/bjt.rs` | Step 5 (temperature), as the first deliberate result change |
| **New:** a node named `gnd` isn't ground. ngspice rewrites `gnd` to `0` everywhere (`inp_fix_gnd_name`, `inpcom.c`) | `spicy_parser` | Not planned: we don't need `gnd` support |
| **New:** with `--raw`, every analysis writes to the same `<file>.raw`, so a netlist with `.op` and `.dc` keeps only the last result | `spicy_cli/src/batch.rs` | Later: not important for now |
| **New:** the raw writer silently ignores write errors (`let _ = …`) | `spicy_cli/src/batch.rs` | Later: not important for now |
| **New:** a source's DC value came from evaluating its waveform at t = 0 with a zero step and stop time: a SIN without a frequency divided by zero and the operating point diverged; a PULSE with an explicit zero rise time gave V2 instead of V1 | `devices/waveform.rs` | ✅ Fixed in step 3b, test-first: the DC value follows ngspice (`vsrcload.c`): V1 for PULSE and EXP, VO + VA·sin(phase) for SIN |
| **New:** the transient waveforms differ from ngspice at the edges: `<` vs `≤` at the delay, a rise or fall time ≤ 0 isn't replaced by the step, a PULSE given only TR and TF gets PW = stop time instead of 0, and there are no breakpoints at pulse edges | `devices/waveform.rs` | Open |
| **New:** `.ac lin 1 1k 1k` (a single frequency) panics on an assertion that `fstop > fstart`; ngspice accepts it | `ac.rs` | Later, with the AC work (roadmap M2d) |
| `spicy_simulate` still depends on `spicy_parser` (it reads `Deck`), and on `clap` (for its two helper binaries) | `spicy_simulate/Cargo.toml` | ✅ Fixed. `clap` is optional, behind the `klu-tools` feature the KLU binaries require. Since step 3b the simulator reads `spicy_circuit`; the parser is only a test dependency |
| Topology is rebuilt for every analysis | `dc.rs`, `ac.rs`, `trans.rs` | Steps 2 and 4 (deferred) |
| AC is dense, allocates per frequency, and ignores transistors | `ac.rs` | Roadmap M2d |
| Transient history is keyed by device name | `trans.rs` | ✅ Fixed in step 3b: indexed by capacitor, since names left the simulator |
