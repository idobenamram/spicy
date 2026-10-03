# Roadmap: from the design docs to a working MVP

> Live doc, updated 2026-10-03. It shows where the work is and what comes next. The design is in the docs that each section links to. Section numbers stay fixed, because other docs and code cite them. New links use the anchors.

**The MVP in one sentence:** spicy reads `circuits/ce_amp.spl`, checks its three specs on ngspice, and prints a verdict for each spec. Each verdict must match a brute-force answer key (every corner simulated).

**How we work:**
- One milestone at a time. Each starts with a short design note, is built in small steps, and ends with a [review](#reviews) before the next one starts.
- **Nothing is committed before you've reviewed it.** You review and clean up the code between milestones, then commit or ask for the commit.

---

<a name="status"></a>
## 1. Where the work is

| Area | State (2026-10-02) | Doc |
|---|---|---|
| Language front-end (M1a–M1d) | The lexer, the parser, resolve and flatten are committed, in the v5 syntax. The contracts phase runs now: steps 1 (env and const) and 2 (setups) are committed, steps 3–6 are next. | `contracts_plan.md` |
| Lowering (M1e) | Not started. It comes after the contracts phase. | `model.md` |
| SPICE export (M1f) | The writer is committed. `spicy export` waits for M1e. | `netlist_writer.md` |
| SPICE reader | Parked: it reads circuits mostly right. | `netlist_reader.md` |
| CLI | The redesign is partly decided. The CLI code gets a cleanup first, so that new commands fit. Then `spicy check` comes. | `cli_plan.md` |
| Engine (M3) | The plan is accepted. The type design is a draft. Implementation waits until the SPICE export works (2026-09-28). | `engine_plan.md`, `engine_types.md` |
| Our simulator (M2, M4) | Pipeline steps 1 and 3 are committed. Steps 2 and 4 wait until the engine runs many simulations. The rest comes after the MVP, in M4. | `pipeline.md` |

The check of the code at the start (2026-09-25) is in git: `git show 6f64e57:docs/ecad/roadmap.md`. The simulator limits it found are in [Simulator follow-ups](#simulator-follow-ups).

---

<a name="architecture"></a>
## 2. Architecture

Moved to [`ARCHITECTURE.md`](../../ARCHITECTURE.md) on 2026-10-03. Each section below gives a one-line summary and a link.

<a name="native-input"></a>
### 2.1 Our simulator reads our language natively; netlists are an export

Our simulator reads the lowered design (`spicy_circuit::Circuit`) directly. SPICE text is only an export ([data flow](../../ARCHITECTURE.md#data-flow)).

<a name="crates"></a>
### 2.2 Crates

The crates, their dependencies, and the folders outside `crates/`: [code map](../../ARCHITECTURE.md#code-map).

<a name="ownership"></a>
### 2.3 Who owns what

What each crate owns and must not know about, and the three import rules: [who owns what](../../ARCHITECTURE.md#ownership).

<a name="spicy-model"></a>
### 2.4 What `spicy_model` holds

The design as written, after name resolution and unit checks: [what `spicy_model` holds](../../ARCHITECTURE.md#spicy-model).

<a name="shortcuts"></a>
### 2.5 Deliberate MVP shortcuts

The prelude in Rust, and slopes by nudging: [MVP shortcuts](../../ARCHITECTURE.md#shortcuts).

<a name="outside-crates"></a>
### 2.6 Outside `crates/`

The folders outside `crates/`: [code map](../../ARCHITECTURE.md#code-map).

---

<a name="ngspice"></a>
## 3. How the ngspice integration works (M3, decided 2026-09-27)

The engine MVP runs on ngspice-42 (installed, with KLU and the shared library). The details are in `engine_plan.md` §4 (the backend) and §5 (what the export must emit).

- **Transport, decision D-B:** the engine loads libngspice once and changes the knob values for each run with `alterparam`. On the CE amp, a whole check takes 0.4 s this way, and 1.0–1.2 s with `ngspice -b` batches. `ngspice -b` stays as the cross-check in the tests.
- **Open again (2026-10-02):** D-B puts libngspice in a separate worker process, because a netlist error stops libngspice and would stop our process too. Whether the worker is a separate process comes back with the engine design (`cli_plan.md`, CLI-10).
- **Two export modes:** the numeric netlist at one knob point (M1f, `netlist_writer.md`), and the engine deck with a `.param` for each knob (later, in `spicy_backends`).
- **Tests:** the ngspice adapter sits behind a cargo feature. CI installs the Ubuntu `ngspice` package and runs the cross-check tests. Locally, they are skipped when ngspice is not installed.

---

<a name="mvp-language"></a>
## 4. The MVP language subset

<a name="mvp-subset"></a>
### 4.1 In and out

| In the MVP | Not yet (designed in language.md, built later) |
|---|---|
| **Items:** `block`, `contract`; comments; `///` doc comments (kept as rationale) | `mod`/`use` beyond the prelude, `fn`, `const`, `enum`, `signal`, `interface`, `part`, `family` |
| **Ports:** `port name: Type;` with `Power<In/Out>`, `Ground`, `Analog<In/Out>`, `Pin` | Interfaces (I2c, Spi), arrays and buses |
| **Nets and pin binding:** `net name;`, `let name = Kind { pin: net, …, field: value };`, `net x = [a, b];` | — |
| **Part kinds:** `Resistor`, `Capacitor`, `Electrolytic`, `Npn`, `Pnp` | Inductor, diodes, MOSFETs, IC parts, `part:` pinning, part records, `?` |
| **Hierarchy:** placing a block inside another (`let amp = CeAmp { … }`), flattened in elaboration | Contract composition checks (guarantee ⊆ assumption) |
| **Values:** SI literals with units (`47k`, `4.7kΩ`, `1uF`, `12V`, `-10°C`, `1kHz`, `-3dB`), `±` / `+/-` tolerances (relative and absolute), ranges `a..=b` (and open `..=b`, `a..` in setups), unit checking, arithmetic `+ - * /` | `from` derivations, `lot`, links (tempco, aging) |
| **Contracts (v5 syntax, `syntax_v5_plan.md`):** `env` and `const`; `setup S for X { … }` with a port's source or load (`vcc: Supply { v: 12V ± 5% }`) and `temp`; in the contract `setup = S;`, `let` measures, `[pub] spec name: measure within / <= / >= bound;`, `#[confidence(worst_case / sigma(3))]` | Modes, events, derived setups (`..Base`), spec clauses (`with`, `for`, `on`, `in`), function-form specs, `rated`, signal patterns (`Sine {…}`), `where`, lints and waivers, automatic checks |
| **Measures:** `dc(expr)`, `ac(expr)`, probes `net.v`, `port.v`; `.at(f)`, `.mag()`, `.f_low(-3dB)` | Transient, noise, current and power probes, the rest of the measurement library |
| **Generics and loops:** none | `<const N>`, `for`, `if`, `match`, indexed lets |

<a name="mvp-file"></a>
### 4.2 The file the MVP must handle

[`circuits/ce_amp.spl`](../../circuits/ce_amp.spl), in the v5 syntax: the common-emitter amplifier of the walkthrough, with one setup (`Operating`) and three specs (`bias`, `gain`, `bass`).

It has 8 knobs:
- **range:** `ambient` (the temperature, which the setup's `temp:` names), `vcc.v` (the setup's supply);
- **statistical:** the `r1`, `r2`, `rc`, `re` and `c_in` values, and `q1.beta`.

(The walkthrough's 9th knob, aging, needs links, which are post-MVP.)

<a name="front-end-choices"></a>
### 4.3 Front-end choices

Moved to [`ast.md`, front-end choices](ast.md#choices): a hand-written lexer and parser, a typed AST with spans plus the full token list, and `codespan-reporting` for diagnostics.

<a name="ast-choice"></a>
### 4.4 Why an AST and a hand-written lexer (decided 2026-09-26)

Moved to [`ast.md`, why an AST](ast.md#why-ast): an AST with a span on every node, plus the lexer's full token list, rebuilds the file byte for byte with no parsing crates. Neither `rowan` nor `logos` is used.

<a name="spade"></a>
### 4.5 What we take from Spade, and what we don't

Moved to [`ast.md`, Spade](ast.md#spade): Spade's parser is a design reference only. Its code is EUPL-1.2 and is never copied.

---

<a name="testing"></a>
## 5. Testing strategy

| Layer | How it's tested |
|---|---|
| Lexer / parser | Snapshot of the syntax tree per construct; snapshot of rendered diagnostics per error; fuzzing (like the existing `spicy_netlist` fuzz target) |
| Elaboration | Snapshot of the elaborated model; one test per semantic error (unbound pin, wrong unit, two sources on a rail) |
| Lowering | Snapshot of the lowered `Circuit` per knob point |
| Simulator features | Hand-calculated expectations (temperature equations, small-signal gain), with explicit tolerances |
| Engine | **The brute-force answer key is the oracle.** Loop verdicts must match it. Seeded Monte Carlo for sigma checks |
| Numbers in snapshots | Stored at full precision; compared with a 1e-12 relative tolerance (1e-14 absolute floor for round-off zeros) by `test_util::assert_numeric_snapshot`; the text around numbers must match exactly |

---

<a name="milestones"></a>
## 6. Milestones

**🔍 marks a review point:** we stop and go over the step together, and you review, clean up and commit before the next milestone.

<a name="m0"></a>
### M0: Housekeeping — done, committed

Done 2026-09-25: numeric snapshots compared with a tolerance (`crates/spicy_simulate/src/test_util.rs`), CI on `master` with a `cargo fmt --check`, and the docs moved to `docs/ecad/`. The clippy warnings in older code are left for now.

<a name="m1"></a>
### M1: Language front-end, up to a SPICE export

**Goal:** `spicy export circuits/ce_amp.spl` writes a SPICE netlist of the amplifier. That tests the whole block side of the language end to end, without the engine. (The contract side, specs and measures, is tested in M3.)

**Why this order** (agreed 2026-09-26):
- Each step has its own tests and review.
- The export gives two checks that need neither ngspice nor the engine:
  - the exported text, parsed back by `spicy_netlist`, must give the same `Circuit` + `Params`;
  - simulating it must give VC ≈ 5.52 V.

**Steps:**

- **M1a: Grammar note.** Done: `grammar.md` v0.2, the v5 syntax (2026-09-30).
- **M1b: Lexer.** Done 2026-09-26 (`lexer.md`): `crates/spicy_lang`, the fuzz target `fuzz_spicy_lang_lexer`. Moved to the v5 syntax on 2026-09-30.
- **M1c: Parser → AST.** Done 2026-09-27 (`ast.md`): `crates/spicy_lang/src/parser`, the fuzz target `fuzz_spicy_lang_parser`. Moved to the v5 syntax on 2026-09-30 (`syntax_v5_plan.md`).
- **M1d: Elaboration → `spicy_model`** (`model.md`).
  - Resolve (committed 2026-09-27) and flatten (committed 2026-09-30, reviewed in `research/flatten_decisions.md`) are done.
  - **Now:** the contracts phase, in six steps (`contracts_plan.md`). Steps 1 (env and const) and 2 (setups) are committed (2026-10-02). Then come the contract's default setup and measures, the specs and the `pub` rule, the flat default setup, and the design notes.
  - **Done when:** `ce_amp.spl` elaborates to exactly the 8 knobs and 3 specs of §4.2, and every semantic error has a snapshot-tested diagnostic. 🔍
- **M1e: Lowering → `spicy_circuit`.** To do, after the contracts phase.
  - A flat design at a knob point becomes a `Circuit` + `Params`, with origins set to model paths.
  - Each port gets the source or the load of the contract's default setup (v5).
  - Decided: a range knob's nominal is its midpoint (`model.md` E16, β = 200), and a bare `Npn` gets the default model of D-A (`engine_plan.md` §5.4).
  - **Done when:**
    - a snapshot of the lowered circuit;
    - simulated natively, the operating point gives VC ≈ 5.52 V;
    - on ngspice, with the D-A model at TNOM 25 °C, VC = 5.503227 V (`engine_plan.md` §2.1). 🔍
- **M1f: SPICE export** (`netlist_writer.md`).
  - The writer is committed (2026-09-30). It round-trips every `circuits/*.spicy` file, and each written netlist loads in ngspice-42 without a warning.
  - `spicy export` waits for M1e.
  - The engine deck (`engine_plan.md` §5.1) comes later, in `spicy_backends`, and uses the writer's line functions.
  - **Done when:**
    - round trip: `ce_amp.spl` → export → `spicy_netlist` → lower gives the same `Circuit` + `Params` + options;
    - the exported amplifier simulates to the same VC. 🔍

**Done when:** `spicy export circuits/ce_amp.spl` writes a netlist that round-trips and simulates to the walkthrough's operating point.

<a name="m2"></a>
### M2: Native simulator input and simulator readiness

`pipeline.md` §9 sets the order of this work. Step 1 (cleanup) and step 3 (`spicy_circuit` as the simulator's input, M2a, `circuit.md`) are done, and the results stayed bit-identical. Steps 2 and 4 wait until the engine runs many simulations. M2b moved to M1e. The rest is the work for M4:

- **M2c: Temperature.**
  - Standard SPICE temperature equations for the BJT (VT = kT/q; IS with XTI/EG; BF with XTB) and for resistors (TC1/TC2).
  - Temperature as a simulation setting. `spicy_circuit` and the reader already carry the temperatures and the model parameters (2026-09-30).
  - Tests against hand-calculated values. 🔍
- **M2d: AC at the operating point.**
  - Solve the operating point first, then stamp the linearized BJT and diode conductances into the AC matrix.
  - Tests: a CE stage's mid-band gain and input resistance against the small-signal formulas (walkthrough appendix). 🔍
- **M2e: Engine-grade accuracy.**
  - An "engine" Newton configuration with tighter tolerances.
  - Separate voltage and current tolerances.
  - A test that slopes agree at two step sizes. 🔍

<a name="parser-follow-ups"></a>
### Parser follow-ups (tracked, not scheduled)

Found while fixing the SPICE parser. None is needed for the MVP; each gets done when it starts to matter.

| Item | Why it matters | Where |
|---|---|---|
| **Keyword tables** instead of the 16-byte keyword buffer | Nothing checks the buffer's limit: a keyword longer than 16 characters, added later, would never match. Tables (name → meaning, matched case-insensitively) remove the limit and list every supported keyword in one place, the way ngspice declares device parameters (`bjt.c`, `BJTmPTable`) | TODO in `spicy_netlist/src/netlist_types.rs` (`Keyword`) |
| **Model coverage** | We accept 5 BJT and 3 diode model parameters; ngspice's parameter tables have 154 and 104 entries. Real vendor models (`VAF`, `IKF`, `CJE`, …) are rejected with `invalid param`. Support them, or accept and ignore them with a warning | `spicy_netlist/src/netlist_models.rs` |
| **Subcircuit scoping gaps** | `.model` cards inside a subcircuit are global; nested `.SUBCKT` definitions and `.global` aren't supported | `subcircuit_phase.rs`; `pipeline.md` §11 |
| **Parse allocations** | Parsing a 10,000-line netlist allocates 44.5 MB (about 4.4 KB per line): e.g. parameter lists rebuilt per device, a token vector per statement, subcircuit bodies cloned per instance. Profile before optimizing | parser |

<a name="simulator-follow-ups"></a>
### Simulator follow-ups (tracked, not scheduled)

**Decided 2026-09-27:** the engine MVP runs on **ngspice**. The engine assumes a working simulator at ngspice's speed; our simulator's issues wait, and it joins later as a second backend.

Found by the engine research (`research/engine_synthesis.md` §5), each with a row in `pipeline.md` §11:

| Item | Why it matters |
|---|---|
| BJT Ebers–Moll reciprocity (`bjt.rs:133-135`) | Wrong VCE,sat (47.8 vs 65.6 mV) and forward IC low by αF. Fixing it moves snapshots |
| Non-convergence returned as a solution | −7.3·10²⁶ V with exit code 0; an engine must see "run failed" |
| Panics in `simulate_dc` / `simulate_ac` | One failed run aborts a whole check |
| AC frequencies accumulated (`f *= r`) | `at(1kHz)` must mean exactly 1 kHz |
| No per-device operating-point records | The engine's guards read region and margins |
| Engine-grade tolerances (M2e) | Finite-difference slopes are 25–26% wrong at today's defaults |
| Temperature (M2c), BJT/diode AC at the operating point (M2d, sparse) | Needed before our simulator can run the MVP specs |

<a name="readme-follow-ups"></a>
### Simulator, KLU and visualization follow-ups (tracked, not scheduled)

Moved from the TODO list in `README.md` on 2026-10-03.

| Area | Item |
|---|---|
| Simulator | Support UIC again, as a simulation mode. Today `trans.rs` stops with "UIC is not supported yet" |
| Simulator | A validation step for netlists |
| Simulator | Gmin stepping |
| Simulator | Source stepping |
| KLU | Use KLU's statistics to decide when to factorize the matrix again |
| KLU | Benchmarks for each algorithm and for the full solve. The analyze step has one |
| KLU | Make singular matrices work when `halt_if_singular` is off |
| KLU | Refactor the functions and structs of KLU, mostly the numeric part |
| KLU | Complex numbers in KLU (to decide) |
| Speed | A `spicyVec` type for bounds checks |
| Visualizations | Merge the recorder macro |
| Visualizations | Visualizations for BTF and AMD |

<a name="m3"></a>
### M3: The engine MVP (`spicy_engine`), on ngspice

Planned in detail in `engine_plan.md` (accepted 2026-09-27; §9 is this list, §8 the tests). With ≤ 12 knobs per spec, enumerating every corner is exact and cheap, so the worst-point loop comes after the MVP (decisions D-D, D-F).

**Status:** the M3a design note (`engine_types.md`) is a draft for review. Implementation waits until the SPICE export works (2026-09-28).

M3a–M3e run on hand-written ngspice netlists and hand-built contracts, so they don't wait for the language; M3f joins the two tracks.

- **M3a: Design note.** The plan's types (§6.2), the `Backend` trait, the two verdict tables, the report schema, and the `EngineDeck` type that M1f and M3b meet at (§5.1). Creates `spicy_engine` and `spicy_backends`. No `affine` module (D-F). 🔍
- **M3b: ngspice backend.** The libngspice worker process (D-B), request encoding, read-back of every knob, plausibility checks, restart after a crash; the `ngspice -b` cross-check path.
  - **Done when:** the backend-rule tests pass (plan §8.4), and the nominal CE amp gives VC 5.503227 V, |H(1 kHz)| 4.590771, f_low 20.126944 Hz (TNOM 25 °C). 🔍
- **M3c: `worst_case`.** The knob space, cones (the capacitor rule), the measures, the run table, enumeration per spec side, the inside-the-box guards (tangent check, 8 audit points, pooling, ascent), the numerical band, and the worst-case verdict table; the answer-key harness.
  - **Done when:** `worst_case` equals the answer key on every M3 case of plan §8.2; run counts pinned. 🔍
- **M3d: `sigma(3)`.** The exact distribution map (D3), the 3σ-point search from two starts, the other range corners, range nudges, the uniform-spread check, and the sigma verdict table; the σ answer-key harness.
  - **Done when:** σ values within 1e-4 of the σ key on the CE amp (both XTB settings), the plan's §8.3 UNDECIDEDs exactly, no false PASS on the adversarial suite; run counts pinned. 🔍
- **M3e: Output and store.** Records with `claim`, `next`, tags and `not_modeled`; the terminal table; `--format json` (unstable); `--explain`, `--at`, `--deep`; the per-revision store (`.spicy/checks/<rev>.json`) and `stale`.
  - **Done when:** snapshots of the CE amp and every suite case, as table and JSON. 🔍
- **M3f: End to end.** The flat design's knobs → knob space, the contract → plan, the engine deck.
  - **Done when:** checking `circuits/ce_amp.spl` prints the plan's §2.8 table, equal to the answer key: bias FAIL at `worst_case` (6.5595 V) beside PASS at `sigma(3)` (6.2813 V); every other side PASS. Snapshot-tested. 🔍

**Acceptance for all of M3:** no false PASS on the adversarial suite (10 cases from 7 circuits, kept in the repo as ngspice netlists; plan §8.2).

<a name="m4"></a>
### M4 and after (planned in detail once the MVP works)

- **M4: Our simulator as a second backend.** The simulator follow-ups above: the reciprocity fix, errors instead of panics and silent non-convergence, the exact AC grid, per-device records, then temperature (M2c), AC at the operating point (M2d) and engine tolerances (M2e). **Gate:** the adversarial suite passes on it, and it agrees with ngspice within the numerical band (engine v3 §6.4).
- **The worst-point loop, for more knobs:** the loop on each cone, margin checks spliced into every side whose cone holds the device, and the PASS allowance by measure kind (plan §9.3). **Gate:** it agrees with enumeration on every regression circuit of ≤ 12 knobs per side.
- **Transient tier:** THD and other `tran` measures, on demand, with large-signal margins.
- **M5:** Part records and `part:` pinning (knob identity kept); the datasheet-arithmetic engine, with affine forms (engine v3 §4.1).
- **M6:** Aging links and `life`; lots; the rest of the knob model (engine v3 §2).
- **M7:** Statistics: board yield, importance sampling (engine v3 §4.4).
- **M8:** Parameter layer and sensitivities in our simulator (transposed KLU solve, DC/AC adjoint).
- **Later:** language growth (generics, loops, interfaces, modules), automatic checks, the editor and the agent (plan §7, §9.3).
- **Later: our own temperature model.** The richest temperature behavior we have (datasheet curves, measured tempcos, later Verilog-A) lives in the part description, above `spicy_circuit`, which only carries SPICE-style parameters (TNOM, XTB, XTI, EG). Per part, the backend either **pre-applies** our law (hands the simulator values at the run's temperature, with TNOM = T) or **passes through** the SPICE parameters (vendor models it can't see into). Needs a research round first: VBIC/HICUM/BSIM temperature modeling, how Spectre and Xyce do it, fitting datasheet curves, and whether pre-applying covers internal effects such as junction capacitances. Not MVP.
- **Later: when specs run.** Tiers (live, save, idle, commit, CI, nightly, manual) placed by the engine's cost estimate, a `#[run(…)]` override, "gate by meaning, schedule by cost", and verdicts stored by cone fingerprint so fresh vs stale is exact. Designed in `research/runs_language.md`, measured in `research/runs_cost_tiers.md`. Not MVP: the first check runs everything.

---

<a name="decisions"></a>
## 7. Decisions

| # | Decision | Status |
|---|---|---|
| 1 | Crate layout ([code map](../../ARCHITECTURE.md#code-map)), now with `spicy_circuit` for native simulator input | **Agreed:** the `spicy_circuit` data model is in `circuit.md` |
| 2 | ~~Our simulator for the MVP; ngspice right after~~ → **ngspice for the MVP; our simulator as the second backend (M4)** | **Agreed** 2026-09-27 |
| 3 | MVP language subset (§4.1) | To confirm |
| 4 | Syntax tree: typed AST with spans + the full token list; no lossless tree, no `rowan` ([`ast.md`](ast.md#why-ast)) | **Agreed** 2026-09-26 |
| 4b | Lexer and parser hand-written, no `logos`; Spade as a design reference only, never copied ([`ast.md`](ast.md#why-ast), [Spade](ast.md#spade)) | **Agreed** 2026-09-26 |
| 5 | Diagnostics: codespan-reporting | Agreed |
| 6 | Stale snapshot | Done: reverted to `V1` |
| 7 | Docs in `docs/ecad/` | Done |
| 8 | Commits | Only after your review of each milestone |
| 9 | One-time `cargo fmt --all` + fmt check in CI | Done |
| 10 | Clippy warnings in existing code | Left for now |
| D-A | Default model for a bare `Npn`: `IS=1e-14 BF={beta} XTB=1.5 XTI=3 EG=1.11`, TNOM 25 °C, written out (`engine_plan.md` §10) | **Agreed** 2026-09-27 |
| D-B | ngspice transport: libngspice in a worker process; `ngspice -b` as the tests' cross-check | **Agreed** 2026-09-27. The worker process is open again (§3) |
| D-C | Round 2's D1–D10, as answered in `engine_plan.md` §10 | **Agreed** 2026-09-27 |
| D-D | No worst-point loop in M3: enumerate corners (≤ about 12 knobs per spec side); the loop comes after, gated on agreeing with enumeration | **Agreed** 2026-09-27 |
| D-E | Verdict words: PASS (all corners) / PASS (estimated) / PASS (implied by worst case); UNDECIDED with reasons and `next`; `simulated` / `stale`; no "verified" | **Agreed** 2026-09-27 |
| D-F | No affine forms in M3: measures evaluated per run; affine forms return with hierarchy, calibration, error budgets and datasheet arithmetic | **Agreed** 2026-09-27 |

---

<a name="reviews"></a>
## 8. How each review works

At every 🔍:
1. **What changed:** a summary and the diff.
2. **The checks pass:** `cargo test --workspace`, `cargo clippy` with no new warnings, and `cargo fmt --all --check`. For a stage of the pipeline, also run [`/stage-review`](../../.claude/commands/stage-review.md): it adds fuzzing, a benchmark before and after, and the review subagents.
3. **A code review pass** for correctness, for fit with the [crate boundaries](../../ARCHITECTURE.md#ownership), and against [`docs/code_quality.md`](../code_quality.md).
4. **We go over it together.** Anything that feels wrong gets fixed before moving on.
5. **You review, clean up and commit.**
6. **This roadmap's status is updated** in the same commit, along with the design docs if a decision changed.

---

<a name="risks"></a>
## 9. Risks

| Risk | Mitigation |
|---|---|
| BJT convergence at extreme corners (no junction limiting) | The amplifier converged at the worst corner; add SPICE-style limiting if a corner fails |
| Nudged slopes drown in solver noise | M2e: tighter tolerances + a two-step-size consistency test |
| AC-at-operating-point bugs give plausible but wrong gains | Formula-based tests in M2d; cross-check with ngspice in M4 |
| The AST turns out too lossy for the editor (e.g. moving code with its comments) | The full token list keeps every character, so a small CST of our own can be added over the same lexer without changing the language ([`ast.md`](ast.md#why-ast)) |
| Scope creep | §4.1 is the contract for the MVP. Anything outside it goes to M4+ |
