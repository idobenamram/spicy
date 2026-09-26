# Roadmap: from the design docs to a working MVP

> 2026-09-25 · Living document. Based on `engine.md`, `language.md`, and a check of the current code (§1).
> **Status:** M0 (housekeeping) and simulator step 1 (`pipeline.md` §9: bug fixes, ngspice defaults, CLI concerns out of the simulator) are committed. Steps 2 and 4 (reusing the simulator's setup across runs) are deferred until the engine runs many simulations. **Next, to confirm:** the language front-end (M1); the simulator work the MVP needs (`spicy_circuit`, temperature, AC at the operating point, accuracy) can follow it.

**The MVP in one sentence:** `spicy check circuits/ce_amp.spl` parses the walkthrough amplifier written in our language, runs the worst-point loop on our own simulator, and prints a verdict for each of its three specs. Each verdict must match a brute-force answer key (every corner simulated).

**How we work:**
- One milestone at a time. Each starts with a short design note, is built in small steps, and ends with a review (§8) before the next one starts.
- **Nothing is committed before you've reviewed it.** You review and clean up the code between milestones, then commit or ask for the commit.

---

## 1. Where the code was at the start

A check of a clean checkout of `master` (2026-09-25):

| Area | Finding |
|---|---|
| **Build** | Builds once system OpenBLAS is installed (`libopenblas-dev`) |
| **Tests** | 5 of 42 simulator tests failed on master. **Fixed in M0.** |
| | 1 stale snapshot: commit `9ad507c` ("nvim support") changed `V1` → `V2` in `tests/op_dc/rc_op.spicy` without updating the snapshot |
| | 4 exact-float snapshots that differed in the last digit on a different OpenBLAS build |
| **CI** | Triggered on pushes to `main`, but the branch is `master`. **Fixed in M0** |
| **Formatting** | `cargo fmt --check` failed in 19 existing files. **Formatted in M0** |
| **Clippy** | The current clippy (1.98) reports ~90 warnings in existing code, mostly the KLU solver and the TUI. CI treats them as warnings |
| **Parser** (`spicy_parser`) | SPICE netlists: R, C, L, D, Q, V, I, subcircuits, `.model`, `.param`, `{}` expressions; `.op .dc .ac .tran`. No `.temp` |
| **Simulator** (`spicy_simulate`) | DC op and sweep, transient (fixed step), AC; KLU sparse solver |
| | BJT: Ebers–Moll (IS, BF, BR, NF, NR). **No temperature dependence.** No Early effect, no junction capacitances |
| | AC: dense; **stamps only R/C/L and sources**, so transistors are ignored |
| | Reads `spicy_parser::Deck` directly. Each device resolves its SPICE parameters and defaults itself (`from_spec`) |
| **Amplifier check** | The walkthrough amplifier converges: VC = 5.52 V nominal; 6.22 V at the worst corner (25 °C), in line with the walkthrough's model |

**Our simulator is enough for the MVP.** It needs two contained features: temperature, and AC at the operating point (§6, M2). ngspice comes right after the MVP (M4), as a cross-check and for devices we don't have yet.

---

## 2. Architecture

### 2.1 Our simulator reads our language natively; netlists are an export

Agreed: our simulator shouldn't need a netlist. The language produces a design model, and our simulator consumes it directly, through a small, simulator-ready circuit form. SPICE text is only an **export**, for ngspice, for sharing, and for debugging.

```
.spl  ──spicy_lang──►  spicy_model  (design: blocks, knobs, contracts)
                           │
                           │  lower(design, knob point)      per run, in memory, no text
                           ▼
SPICE ──spicy_parser──►  spicy_circuit::Circuit  ──►  spicy_simulate  (op · dc · ac · tran)
                           │
                           └──►  SPICE netlist export  ──►  ngspice (M4) / a file to share or debug
```

**`spicy_circuit::Circuit`** is flat, numeric and ready to simulate:
- named nodes;
- devices with **fully resolved** parameters (a resistance, a BJT's IS/BF/… and temperature coefficients);
- sources;
- for each device, its **origin** (a model path like `amp.r1`, or a SPICE line), so results and errors map back to what you wrote.

It has no knobs and no analyses. Analyses are requests to the simulator's API.

**Why native** (and not "generate a netlist and parse it again"):
- **Nothing is lost in translation.** Results are keyed by model paths (`amp.q1`), and errors point at `.spl` lines, not generated netlist lines.
- **Speed.** The loop runs dozens of simulations per spec, with no text generation or parsing per run. Later, the parameter layer (engine v3 §6.5, step 1) lets the simulator re-stamp new knob values without rebuilding anything.
- **Decoupling.** The simulator stops depending on the SPICE parser. Both front-ends (SPICE and our language) feed the same input type.

**Why not have the simulator parse `.spl` itself:** a simulator shouldn't contain a language front-end. "Natively" means it consumes the model's lowered circuit directly. The language's job ends at `spicy_model`.

**Cost:** a behavior-preserving refactor of existing code. The per-device parameter resolution moves out of `spicy_simulate` (`from_spec`) into a lowering step in `spicy_parser` (Deck → Circuit). The now-stable snapshot tests must stay byte-identical through the refactor, which proves nothing changed. This is M2a.

### 2.2 Crates

```
crates/
  spicy_circuit/    (new)     simulator-ready circuit form: nodes, devices, origins
  spicy_parser/     (exists)  SPICE netlists → spicy_circuit (plus the analysis commands in the file)
  spicy_simulate/   (exists)  our simulator; reads spicy_circuit only
  spicy_cli/        (exists)  CLI + TUI; gains a `check` command

  spicy_model/      (new)     the design model (§2.4); shared by language, engine and (later) the editor
  spicy_lang/       (new)     language front-end: text → spicy_model, with diagnostics
  spicy_engine/     (new)     knobs, measurements, the worst-point loop, verdicts; defines the `Backend` trait
  spicy_backends/   (new)     adapters: our simulator (model → circuit → simulate); ngspice (M4, behind a cargo feature)
```

Dependencies (arrows mean "depends on"; no cycles):

```
                          spicy_cli
                  ┌──────────┼──────────────┐
             spicy_lang   spicy_engine   spicy_backends ───► spicy_simulate ──► spicy_circuit
                  │        │      │            │                                   ▲
                  └──► spicy_model ◄┘                            spicy_parser ─────┘
                           ▲                                (SPICE front-end, used by the CLI)
                           └────────────── spicy_backends
```

### 2.3 Who owns what

| Crate | Owns | Must not know about |
|---|---|---|
| `spicy_circuit` | The simulator's input: one concrete circuit | Knobs, specs, syntax |
| `spicy_model` | The design as written: hierarchy, values with spreads, knobs, contracts (§2.4) | Syntax trees, simulators, affine math, results |
| `spicy_lang` | Text → `spicy_model`, with rustc-quality diagnostics | Simulators, the engine |
| `spicy_engine` | Design + contract → verdicts. Defines what it needs from a simulator (`Backend`, capability levels) and the result types | Any specific simulator, KLU, SPICE syntax |
| `spicy_backends` | Making simulators speak `Backend`: model → circuit for ours; model → SPICE text for ngspice | Specs, verdicts |

**The rules that keep this clean:**
- The engine never imports a simulator crate.
- The language never imports the engine.
- The simulator never imports a parser.

### 2.4 What `spicy_model` holds

The design as the engineer wrote it, after name resolution and unit checking, with no syntax left:

| Group | Types (sketch) |
|---|---|
| **Identity** | `BlockId`, `InstanceId`, `NetId`, `KnobId`; `Path` (`amp.r1`, stable, used everywhere) |
| **Units** | `Dimension` (SI exponent vector), `Quantity` (a number + its dimension) |
| **Values** | `Value { nominal: Quantity, spread: Exact \| Tol(rel or abs) \| Range(lo, hi) }` |
| **Signals** | `SignalType` (`Pin`, `Ground`, `Power`, `Analog`, `Logic`), `Role` (`In`, `Out`, …) |
| **Part kinds** (MVP prelude) | `PartKind` (`Resistor`, `Capacitor`, `Electrolytic`, `Npn`, `Pnp`), each with its pin names and field schema |
| **Design** (hierarchical, as written) | `Design { blocks, contracts }` |
| | `Block { name, doc, ports, nets, instances }` |
| | `Instance { name, of: PartKind or Block, pins: pin → net, fields: name → Value, doc }` |
| **Contract** | `Contract { assumptions, measures, specs }` |
| | `Assumption { target (env `temp`, or a port quantity like `vcc.v`), range }` |
| | `Spec { name, doc, measure: Expr, relation, bound, confidence }` |
| | `Expr`: measure expressions (analysis calls, probes, arithmetic, methods) |
| **Elaborated** (flat; what the engine and backends use) | `FlatDesign { nets, devices (path, kind, pin → net, parameters referring to knobs), knobs, contract with resolved probes }` |
| | `Knob { id, path, kind: Range \| Statistical, nominal, lo, hi, distribution }` |
| **Source map** | Every item carries a span (file + byte range), for diagnostics and the editor |

**Not in it:** syntax trees (those are `spicy_lang`'s), simulation results, affine forms and verdicts (`spicy_engine`'s), layout geometry (the editor's sidecar, later), and a simulator's circuit (`spicy_circuit`).

### 2.5 Deliberate MVP shortcuts

| Shortcut | Why | When it goes away |
|---|---|---|
| The prelude (`Resistor`, `Npn`, `Power`, …) is defined in Rust inside `spicy_lang` | The MVP language can't define parts or signals yet | Once `signal` / `part` items exist |
| Each run lowers the model to a fresh `Circuit` | Simple; lowering a 10-part design takes microseconds | The parameter layer (engine v3 §6.5, step 1) |
| Slopes by nudging each knob (L0) | Works with any backend | Sensitivities in our simulator (M8) |

### 2.6 Outside `crates/`

```
circuits/     example designs: existing .spicy netlists + new .spl designs (ce_amp.spl first)
docs/ecad/    design docs (README.md is the index), research/, archive/
externals/    reference repos (gitignored)
```

---

## 3. How the ngspice integration will work (M4)

**Input.** ngspice only reads SPICE netlists, so this is where the netlist export lives. The exporter turns a flat design plus a knob point into ngspice's SPICE dialect:
- element names derived from model paths (`amp.r1` → `R_amp_r1`), with a table mapping them back;
- `.model` cards for transistors;
- `.temp`;
- the analyses the engine asked for.

**Transport**, two options, both behind the same `Backend` trait:

| | Subprocess (`ngspice -b -r out.raw deck.cir`) | Shared library (libngspice's C API) |
|---|---|---|
| How | Write the netlist, run ngspice in batch mode, read the raw output file | Load the circuit once from memory, change values with `alter`/`alterparam`, run, read result vectors through callbacks |
| For | Simple; a crash can't take down our process; parallel runs = parallel processes | No process start per run; the circuit stays loaded |
| Against | Process start and file I/O on every run | C FFI; global state (one circuit per loaded library); a system library dependency |

**Plan:** start with the subprocess. Move to the shared library only if run overhead matters.

**Other pieces:**
- **Results:** a raw-file reader (we only have a writer today) maps vectors back to model paths. Measurements are done by our own library, as for every backend (engine v3 §6.1).
- **Capability:** L0 (values only) at first. ngspice's `.sens` may raise it to L1 later, to be verified.
- **Building without ngspice:** the adapter sits behind a cargo feature. CI installs the Ubuntu `ngspice` package and runs the cross-check tests; locally they're skipped when ngspice isn't installed.
- **Cross-checking:** every golden circuit runs on both simulators, and a difference beyond tolerance fails the test (engine v3 §6.4).

---

## 4. The MVP language subset

### 4.1 In and out

| In the MVP | Not yet (designed in language.md, built later) |
|---|---|
| **Items:** `block`, `contract`; comments; `///` doc comments (kept as rationale) | `mod`/`use` beyond the prelude, `fn`, `const`, `enum`, `signal`, `interface`, `part`, `family` |
| **Ports:** `port name: Type;` with `Power<In/Out>`, `Ground`, `Analog<In/Out>`, `Pin` | Interfaces (I2c, Spi), arrays and buses |
| **Nets and pin binding:** `net name;`, `let name = Kind { pin: net, …, field: value };`, `net x = [a, b];` | — |
| **Part kinds:** `Resistor`, `Capacitor`, `Electrolytic`, `Npn`, `Pnp` | Inductor, diodes, MOSFETs, IC parts, `part:` pinning, part records, `?` |
| **Hierarchy:** placing a block inside another (`let amp = CeAmp { … }`), flattened in elaboration | Contract composition checks (guarantee ⊆ assumption) |
| **Values:** SI literals with units (`47k`, `4.7kΩ`, `1uF`, `12V`, `-10°C`, `1kHz`, `-3dB`), `±` / `+/-` tolerances (relative and absolute), closed ranges `..=`, unit checking, arithmetic `+ - * /` | `from` derivations, `lot`, links (tempco, aging) |
| **Contracts:** `assume` on `temp` and on port quantities (`vcc.v`), `let` measures, `spec name: measure rel bound;`, `#[confidence(worst_case / sigma(3))]` | Signal patterns (`Sine {…}`), loads, benches beyond the default, `where`, lints and waivers, automatic checks |
| **Measures:** `dc(expr)`, `ac(expr)`, probes `net.v`, `port.v`; `.at(f)`, `.mag()`, `.f_low(-3dB)` | Transient, noise, current and power probes, the rest of the measurement library |
| **Generics and loops:** none | `<const N>`, `for`, `if`, `match`, indexed lets |

### 4.2 The file the MVP must handle

`circuits/ce_amp.spl`:

```rust
/// Common-emitter audio stage (walkthrough §1).
block CeAmp {
    port vcc: Power<In>;
    port gnd: Ground;
    port input: Analog<In>;
    port output: Analog<Out>;

    net base;
    net emitter;

    /// Divider holds the base near 2.1 V.
    let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };
    let r2 = Resistor { a: base, b: gnd, value: 10k ± 1% };
    let rc = Resistor { a: vcc, b: output, value: 4.7k ± 1% };
    let re = Resistor { a: emitter, b: gnd, value: 1k ± 1% };
    let c_in = Electrolytic { p: base, n: input, value: 1uF ± 20% };
    let q1 = Npn { c: output, b: base, e: emitter, beta: 100..=300 };
}

contract CeAmp {
    assume temp in -10°C..=60°C;
    assume vcc.v in 12V ± 5%;

    let h = ac(output.v / input.v);

    /// Room for the output to swing ±1 V without clipping.
    spec bias: dc(output.v) in 4.5V..=6.5V;
    /// The next stage expects this level.
    spec gain: h.at(1kHz).mag() in 4.6 ± 5%;
    /// Don't cut the bass.
    spec bass: h.f_low(-3dB) <= 30Hz;
}
```

It has 8 knobs:
- **range:** `temp`, `vcc.v`;
- **statistical:** the `r1`, `r2`, `rc`, `re` and `c_in` values, and `q1.beta`.

(The walkthrough's 9th knob, aging, needs links, which are post-MVP.)

### 4.3 Front-end choices

| Choice | Recommendation | Why |
|---|---|---|
| Lexer and parser | **Hand-written** lexer + recursive-descent/Pratt parser | Same style as `spicy_parser` and Spade. Full control over unit literals (`4k7`, `°C`, `±`) and error recovery |
| Syntax tree | **Lossless** (keeps every character, including comments and whitespace), with a typed layer on top. Probably via `rowan`, as in rust-analyzer | The editor and the AI will edit files as text. A formatter and precise edits need it, and retrofitting it means rewriting the parser |
| Diagnostics | **`codespan-reporting`**, see the comparison below | |
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

---

## 5. Testing strategy

| Layer | How it's tested |
|---|---|
| Lexer / parser | Snapshot of the syntax tree per construct; snapshot of rendered diagnostics per error; fuzzing (like the existing `spicy_parser` fuzz target) |
| Elaboration | Snapshot of the elaborated model; one test per semantic error (unbound pin, wrong unit, two sources on a rail) |
| `spicy_circuit` refactor | **Existing snapshots must stay byte-identical** |
| Lowering | Snapshot of the lowered `Circuit` per knob point |
| Simulator features | Hand-calculated expectations (temperature equations, small-signal gain), with explicit tolerances |
| Engine | **The brute-force answer key is the oracle.** Loop verdicts must match it. Seeded Monte Carlo for sigma checks |
| Numbers in snapshots | Stored at full precision; compared with a 1e-12 relative tolerance (1e-14 absolute floor for round-off zeros) by `test_util::assert_numeric_snapshot`; the text around numbers must match exactly |

---

## 6. Milestones

**🔍 marks a review point:** we stop and go over the step together, and you review, clean up and commit before the next milestone.

### M0: Housekeeping — done, committed

| # | Item | Status |
|---|---|---|
| 1 | Stale `rc_op` snapshot | ✅ Netlist reverted to `V1` |
| 2 | Robust numeric snapshots | ✅ Snapshots keep full precision (the original files, unchanged); a new `crates/spicy_simulate/src/test_util.rs` compares numbers with a 1e-12 relative tolerance and text exactly, writes `<name>.snap.new` on a mismatch, and accepts with `SPICY_UPDATE_SNAPSHOTS=1`. Checked both ways: last-digit noise passes, a 1e-11 change fails. 47/47 pass (5 new comparator tests) |
| 3 | CI + formatting | ✅ Push trigger `main` → `master`. `cargo fmt --all` run once (19 files, formatting only). CI now also runs `cargo fmt --all --check` |
| 4 | Docs | ✅ Moved to `docs/ecad/` (+ `research/`, `archive/`), renamed, 97 cross-references rewritten and verified, `README.md` index added |
| 5 | Commits | ✅ You commit |
| 6 | Crate scaffolding | → Moved to M1a. `spicy_circuit` gets its own detailed design note before anything changes |
| 7 | Lesson crate | ✅ `crates/spicy_bounds` removed; `Cargo.toml` and `Cargo.lock` are back to their committed state. The affine math will be written inside `spicy_engine` in M3 |
| — | Clippy warnings | Left for now: 86 warnings from the newer clippy (1.98) in existing code. CI reports them without failing |

### M1: Language front-end

- **M1a: Grammar note + scaffolding.**
  - One page: the exact MVP grammar (§4.1) in EBNF, the token list, and the keywords.
  - Create `spicy_model` and `spicy_lang`, empty, with READMEs stating their responsibility (§2.3). 🔍
- **M1b: Lexer + parser + diagnostics.**
  - Lossless syntax tree, error recovery, codespan-reporting.
  - Tests for every construct in §4.1, and ~20 error cases (missing `;`, `+-`, unknown unit prefix, …).
  - A fuzz target. 🔍
- **M1c: `spicy_model` + elaboration.**
  - Name resolution, unit checking, role checks (one source per `Power` net, every pin bound).
  - Flattening hierarchy, extracting knobs and the contract.
  - `spicy check --dump-model circuits/ce_amp.spl`. 🔍

**Done when:** `ce_amp.spl` elaborates to exactly the 8 knobs and 3 specs of §4.2, and every error case has a snapshot-tested diagnostic.

### M2: Native simulator input and simulator readiness

> **Superseded ordering:** `pipeline.md` §9 now defines the order of this work, as smaller steps each reviewed on its own: (1) cleanup, (2) stamp locations out of devices, (3) `spicy_circuit`, (4) one plan per circuit, (5) temperature. Steps 2 and 4 are deferred: they're a speed-up that matters once the engine runs many simulations. The items below stay as the list of what must eventually be done.

- **M2a: Extract `spicy_circuit`.**
  - Move parameter resolution from `spicy_simulate`'s `from_spec` into a Deck → Circuit lowering in `spicy_parser`.
  - `spicy_simulate` then reads only `spicy_circuit`.
  - Behavior-preserving: **every existing snapshot stays byte-identical.** 🔍
- **M2b: Model → Circuit lowering** (in `spicy_backends`). A flat design + a knob point → `Circuit`, with origins set to model paths. Snapshot tests per knob point. 🔍
- **M2c: Temperature.**
  - Standard SPICE temperature equations for the BJT (VT = kT/q; IS with XTI/EG; BF with XTB) and for resistors (TC1/TC2).
  - Temperature as a simulation setting, plus `.temp` in the SPICE parser.
  - Tests against hand-calculated values. 🔍
- **M2d: AC at the operating point.**
  - Solve the operating point first, then stamp the linearized BJT and diode conductances into the AC matrix.
  - Tests: a CE stage's mid-band gain and input resistance against the small-signal formulas (walkthrough appendix). 🔍
- **M2e: Engine-grade accuracy.**
  - An "engine" Newton configuration with tighter tolerances.
  - Separate voltage and current tolerances.
  - A test that slopes agree at two step sizes. 🔍

**Done when:** the amplifier's nominal VC, gain and f_L, run from the `.spl` file through our simulator natively, match the small-signal formulas and are reproducible to high precision.

**Decision point:** if M2c/M2d turn out much harder than expected, M4 (ngspice) moves ahead of M3, and our simulator catches up later. 🔍

### M3: The loop MVP (`spicy_engine`)

- **M3a: Design note.** The `Backend` trait and capability levels; knob representation; results as affine forms (an `affine` module inside `spicy_engine`); the MVP measurement functions (`dc`, `ac…at…mag`, `f_low`). Create `spicy_engine` and `spicy_backends`. 🔍
- **M3b: Answer key first.** A brute-force checker that simulates all 256 corners of the 8 knobs and reports each spec's true worst case. It's the test oracle for the rest. 🔍
- **M3c: `worst_case` confidence.**
  1. Safety net: every range corner.
  2. Nominal run, then slopes by nudging.
  3. Affine form, then the predicted worst corner.
  4. Simulate it, compare, re-linearize, and repeat until nothing changes.
  5. Verdict, bracket, contributors, counterexample.

  Tests: verdicts and worst values match the answer key. 🔍
- **M3d: `sigma(3)` confidence.** Range knobs at worst, statistical knobs at the 3σ worst point (σ = tol/3). Verified with a seeded Monte Carlo in tests. 🔍
- **M3e: `spicy check` output.** A verdict table in the terminal: spec, verdict, worst value, top contributors, counterexample. 🔍

**Done when:** `spicy check circuits/ce_amp.spl` prints verdicts that agree with the answer key, and tests pin them.

### M4 and after (planned in detail once the MVP works)

- **M4:** SPICE netlist export + ngspice backend + cross-checking (§3).
- **M5:** Part records and `part:` pinning; the datasheet-arithmetic engine (engine v3 §4.1).
- **M6:** Aging links and `life`; lots; the rest of the knob model (engine v3 §2).
- **M7:** Statistics: board yield, importance sampling (engine v3 §4.4).
- **M8:** Parameter layer and sensitivities in our simulator (transposed KLU solve, DC/AC adjoint).
- **Later:** language growth (generics, loops, interfaces, modules), automatic checks, the editor.

---

## 7. Decisions

| # | Decision | Status |
|---|---|---|
| 1 | Crate layout (§2.2), now with `spicy_circuit` for native simulator input | **Agreed:** the `spicy_circuit` data model is in `circuit.md` |
| 2 | Our simulator for the MVP; ngspice right after | Agreed |
| 3 | MVP language subset (§4.1) | To confirm |
| 4 | Lossless syntax tree from day one | To confirm |
| 5 | Diagnostics: codespan-reporting | Agreed |
| 6 | Stale snapshot | Done: reverted to `V1` |
| 7 | Docs in `docs/ecad/` | Done |
| 8 | Commits | Only after your review of each milestone |
| 9 | One-time `cargo fmt --all` + fmt check in CI | Done |
| 10 | Clippy warnings in existing code | Left for now |

---

## 8. How each review works

At every 🔍:
1. **What changed:** a summary and the diff.
2. **Checks green:** `cargo test --workspace`; `cargo clippy` with no new warnings; formatting (once decision 9 is made).
3. **A code review pass** for correctness and for fit with §2.3's boundaries.
4. **We go over it together.** Anything that feels wrong gets fixed before moving on.
5. **You review, clean up and commit.**
6. **This roadmap's status is updated,** along with the design docs if a decision changed.

---

## 9. Risks

| Risk | Mitigation |
|---|---|
| The `spicy_circuit` refactor subtly changes simulator behavior | Byte-identical snapshots as the gate (M2a); done as its own step |
| BJT convergence at extreme corners (no junction limiting) | The amplifier converged at the worst corner; add SPICE-style limiting if a corner fails |
| Nudged slopes drown in solver noise | M2e: tighter tolerances + a two-step-size consistency test |
| AC-at-operating-point bugs give plausible but wrong gains | Formula-based tests in M2d; cross-check with ngspice in M4 |
| The lossless syntax tree slows M1 down | Keep the typed layer thin; the MVP grammar is small |
| Scope creep | §4.1 is the contract for the MVP. Anything outside it goes to M4+ |
