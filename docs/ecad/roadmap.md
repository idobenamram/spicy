# Roadmap: from the design docs to a working MVP

> 2026-09-25 · Living document. Based on `engine.md`, `language.md`, and a check of the current code (§1).
> **Status:** M0 (housekeeping) and simulator steps 1 and 3 (`pipeline.md` §9: bug fixes, ngspice defaults, CLI concerns out of the simulator; `spicy_circuit` as the simulator's input) are committed, as are three fixes to the SPICE parser: nested subcircuits, parameter scoping, and case-insensitivity. Steps 2 and 4 (reusing the simulator's setup across runs) are deferred until the engine runs many simulations. **Next:** the language front-end (M1, now six small steps ending in a SPICE export; the grammar note `grammar.md` awaits review); in parallel, the engine (M3) on **ngspice**, as planned in `engine_plan.md` (accepted 2026-09-27). Our simulator's follow-ups (temperature, AC at the operating point, accuracy, the bugs the engine research found) wait until after the MVP, when it joins as a second backend (M4).

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
| **Parser** (`spicy_netlist`) | SPICE netlists: R, C, L, D, Q, V, I, subcircuits, `.model`, `.param`, `{}` expressions; `.op .dc .ac .tran`, `.temp`, `.options` (TNOM and solver tolerances) |
| **Simulator** (`spicy_simulate`) | DC op and sweep, transient (fixed step), AC; KLU sparse solver |
| | BJT: Ebers–Moll (IS, BF, BR, NF, NR). **No temperature dependence.** No Early effect, no junction capacitances |
| | AC: dense; **stamps only R/C/L and sources**, so transistors are ignored |
| | Reads `spicy_netlist::reader::Deck` directly. Each device resolves its SPICE parameters and defaults itself (`from_spec`) |
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
SPICE ──spicy_netlist──►  spicy_circuit::Circuit  ──►  spicy_simulate  (op · dc · ac · tran)
                           │
                           └──►  SPICE netlist export  ──►  ngspice (the engine's first backend, M3) / a file to share or debug
```

**For the MVP (decided 2026-09-27)** the engine runs on ngspice through the export's engine deck: loaded once, with knob values set per run by `alterparam` (`engine_plan.md` §4–§5). The native path stays the architecture for our simulator, which joins as the second backend (M4).

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

**Cost:** a behavior-preserving refactor of existing code. The per-device parameter resolution moves out of `spicy_simulate` (`from_spec`) into a lowering step in `spicy_netlist` (Deck → Circuit). The now-stable snapshot tests must stay byte-identical through the refactor, which proves nothing changed. This is M2a.

### 2.2 Crates

```
crates/
  spicy_circuit/    (exists)  simulator-ready circuit form: nodes, devices, origins
  spicy_netlist/     (exists)  SPICE netlists → spicy_circuit (plus the analysis commands in the file)
  spicy_simulate/   (exists)  our simulator; reads spicy_circuit only
  spicy_cli/        (exists)  CLI + TUI; gains a `check` command

  spicy_span/       (new)     `Span`: where something was written, for every stage (rustc's `rustc_span`)
  spicy_index/      (new)     the `id!` macro: typed `u32` indices (rustc's `rustc_index`); used by spicy_model and spicy_circuit
  spicy_errors/     (new)     the problem type every stage reports (`Diag`, `Reported`) and its rendering (rustc's `rustc_errors`)
  spicy_model/      (new)     the design model (§2.4); shared by language, engine and (later) the editor
  spicy_lang/       (new)     language front-end: text → spicy_model, with diagnostics
  spicy_engine/     (new)     knob space, measures, corner enumeration + the 3σ-point search, verdicts; defines the `Backend` trait
  spicy_backends/   (new)     adapters: ngspice first (M3: libngspice in a worker process, engine_plan.md §4); our simulator later (M4)
```

Dependencies (arrows mean "depends on"; no cycles):

```
                          spicy_cli
                  ┌──────────┼──────────────┐
             spicy_lang   spicy_engine   spicy_backends ───► spicy_simulate ──► spicy_circuit
                  │        │      │            │                                   ▲
                  └──► spicy_model ◄┘                            spicy_netlist ─────┘
                           ▲                                (SPICE front-end, used by the CLI)
                           └────────────── spicy_backends ───► spicy_engine   (implements its Backend trait)
```

Below them, three small crates with no dependencies of their own apart from codespan (added 2026-09-29, the rustc way): `spicy_span` and `spicy_index`, which depend on nothing, and `spicy_errors`, which depends on `spicy_span` and `codespan-reporting`. `spicy_model` depends on all three, `spicy_lang` on `spicy_span` and `spicy_errors`, and `spicy_circuit` on `spicy_index`.

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
| ~~Each run lowers the model to a fresh `Circuit`~~ Superseded: lower **once**, then set knob values per run (on ngspice, the engine deck + `alterparam`; on our simulator, `Params` + the `Binding`, `circuit.md` §5) | It costs no more, and keeps the loaded circuit shared across runs (`research/engine_flows.md` §7) | — |
| Slopes by nudging each knob (L0) | Works with any backend | Sensitivities in our simulator (M8) |

### 2.6 Outside `crates/`

```
circuits/     example designs: existing .spicy netlists + new .spl designs (ce_amp.spl first)
docs/ecad/    design docs (README.md is the index), research/, archive/
externals/    reference repos (gitignored)
```

---

## 3. How the ngspice integration works (M3, decided 2026-09-27)

The engine MVP runs on ngspice (ngspice-42 with KLU, installed). The details are in `engine_plan.md` §4 (the backend) and §5 (what the export must emit); this section is the summary.

**Input.** ngspice only reads SPICE netlists, so this is where the netlist export lives. The exporter turns a flat design plus a knob point into ngspice's SPICE dialect:
- element names derived from model paths (`amp.r1` → `R.amp.r1`, ngspice's own scheme for flattened names; `netlist_writer.md`), with a table mapping them back;
- `.model` cards for transistors;
- `.temp`;
- the analyses the engine asked for.

**Transport**, two options, both behind the same `Backend` trait:

| | Subprocess (`ngspice -b -r out.raw deck.cir`) | Shared library (libngspice's C API) |
|---|---|---|
| How | Write the netlist, run ngspice in batch mode, read the raw output file | Load the circuit once from memory, change values with `alter`/`alterparam`, run, read result vectors through callbacks |
| For | Simple; a crash can't take down our process; parallel runs = parallel processes | No process start per run; the circuit stays loaded |
| Against | Process start and file I/O on every run | C FFI; global state (one circuit per loaded library); a system library dependency |

**Decided (D-B):** the shared library, in a **worker process**. The library is loaded once, and every run is `alterparam` + `reset` + the analyses, with each knob's value read back from ngspice. The separate process keeps the isolation: a netlist error kills libngspice, and it can't take our process down. On the CE amp the whole check takes 0.4 s this way, against 1.0–1.2 s with `ngspice -b` batches (`engine_plan.md` §4.1). The subprocess path stays as the tests' cross-check.

**Other pieces:**
- **Results:** a raw-file reader (we only have a writer today) maps vectors back to model paths. Measurements are done by our own library, as for every backend (engine v3 §6.1).
- **Capability:** L0 (values only), for good. Verified: `.sens` returns about 0 for AC and ignores later temperature changes; for DC it gives one output per call, so nudging is cheaper for many measures (`research/engine_position_scale.md`).
- **Traps, each pinned by a test** (`engine_plan.md` §4.3): `reset` undoes `option temp` and `option reltol` (use `.temp {param}` and `.options` in the deck); runs slow down without `destroy all`; a circuit with no DC solution returns 999,999,999.99 V with no error; TNOM defaults to 27 °C; `dec` sweeps miss 1 kHz exactly.
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
| Lexer | **Hand-written**, like `spicy_netlist`'s (std only, or the tiny `unscanny` it already uses). **No `logos`** | §4.4 |
| Parser | **Hand-written** recursive descent, with a Pratt loop for expressions. Design borrowed from Spade (§4.5), code written from scratch | §4.4, §4.5 |
| Syntax tree | **A typed AST with byte spans on every node, plus the lexer's full token list (whitespace and comments included).** No lossless tree, no `rowan` | §4.4 |
| Diagnostics | **`codespan-reporting`**, see the comparison below | The only new crate the front-end adds |
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

### 4.4 Why an AST and a hand-written lexer (decided 2026-09-26)

**The goals:** fast, modern, and as few crates as possible. The front-end ends up with **zero parsing dependencies**; its only new crate is `codespan-reporting`.

**The two kinds of syntax tree, on one line:**

```rust
    let r1 = Resistor { value: 47k ± 1% };  // top
```

- **AST (abstract syntax tree).** Keeps the meaning, with byte ranges ("spans") pointing back into the file:
  ```
  Let { name: "r1" @8..10,
        value: StructLit { path: "Resistor" @13..21,
                           fields: [value: Tol(47k, 1%) @31..40] },
        span: 4..43 }
  ```
  Spaces and the `// top` comment aren't in the tree. They stay in the token list.
- **CST (concrete syntax tree, "lossless").** Every character is a leaf: `WHITESPACE "    "`, `LET_KW "let"`, `WHITESPACE " "`, `IDENT "r1"`, … `SEMI ";"`, `WHITESPACE "  "`, `COMMENT "// top"`. Joining the leaves gives back the file. The nodes are untyped, so a typed layer (`LetStmt::name()`) is written on top. `rowan` is the library rust-analyzer uses for this.

**What we chose:** an AST with a span on every node, **plus the lexer's full token list**, whitespace and comments included, exactly as `spicy_netlist`'s lexer already keeps whitespace and newline tokens. The file is always reproducible byte for byte from the tokens. This is Zig's design: `zig fmt` formats from the AST plus the token list. Go's `gofmt` works from an AST plus a comment list.

`spicy_netlist` is the same kind: a hand-written lexer, then phases that build typed structures (`Deck`) directly. It has no syntax-tree layer that keeps the source.

| | **AST + spans + token list** (chosen) | **CST with `rowan`** |
|---|---|---|
| Crates | None | `rowan` 0.16 + 4 (countme, hashbrown, rustc-hash, text-size) |
| Code to write | The parser builds typed nodes directly | The parser emits start/finish events, plus a hand-written typed layer: roughly twice the code |
| Speed | Faster (no trivia nodes). Both are far faster than our file sizes need | Slower, but it wouldn't matter |
| Editor and AI edits ("change r1's value") | Replace the bytes at the node's span | Patch the tree |
| Canonical formatter | From AST + tokens (Zig, Go) | Easier comment placement |
| Formatting one statement without reflowing others (language_editor_mapping R14) | Format only the edited statement's span | Same |
| Moving code together with its comments, keeping odd hand formatting | Harder | Its real strength |
| Incremental reparsing on every keystroke | Not needed: a design file reparses from scratch in far less time than a keystroke | Its other strength, for very large codebases |
| Used by | rustc, Go, Zig, Spade, `spicy_netlist` | rust-analyzer, Roslyn (C#), Swift |

**Why this replaces the earlier "lossless from day one":** the three things lossless was meant for (precise edits, a formatter, error recovery) all work from spans plus the token list. The fear was that retrofitting would mean rewriting the parser. The MVP grammar is about 15 rules, so a rewrite would cost days, not a redesign. And if we ever need a CST, it doesn't require `rowan`: a plain `Node { kind, children }` over our tokens is a small amount of our own code.

**Why no `logos`.** logos (Spade's lexer) turns regexes on an enum into a lexer at compile time. It saves typing for many simple tokens, but:
- **Our hard tokens are the ones it doesn't help with.** Unit literals (`47k`, `4k7`, `10kΩ`, `1µF`, `5%`), `100..=300` (the lexer must not read `100.` as a decimal), `±` and `+/-`, and nested `/* */` comments all need hand-written code either way. Even Spade handles block comments outside logos, in its parser.
- **It's a proc-macro crate**, so it pulls a compile-time stack (syn, quote, regex-syntax, …) into the build.
- **A hand lexer for about 40 token kinds is a few hundred lines,** in the same style as `spicy_netlist`'s (`crates/spicy_netlist/src/reader/lexer.rs`).

### 4.5 What we take from Spade, and what we don't

Spade's parser (`externals/spade/spade-parser`) is the reference for *how* ours is structured. **Its code is not copied or ported:** Spade's compiler crates are EUPL-1.2, a copyleft licence that isn't compatible with our MIT licence, and its README explicitly refuses LLM-generated contributions.

**We take:**
- Parse functions return `Result<Option<T>>`: `Ok(None)` means "not mine, nothing consumed", `Ok(Some)` means parsed, `Err` is a diagnostic.
- A statement loop that dispatches on the leading keyword (`let`, `net`, `port`, `assume`, `spec`, `#[…]`).
- **Recovery:** after an error, skip to a token that can restart a statement, so one broken statement gives one error and the next parses normally. A missing `;` is reported with an insert-`;` fix and parsing just continues. Ours is tighter than Spade's in one way: since every statement ends in `;`, recovery also skips *past* the next `;`.
- A Pratt loop for expressions, with an ordered enum of binding powers.
- Brace-named arguments (`Resistor { a: vcc, … }`), with a flag that forbids them where a `{` opens a body (`for i in 0..N {`, later), as in Rust.
- A diagnostic builder (error, primary and secondary labels, help, suggested replacement), rendered by codespan-reporting.
- Snapshot tests of rendered errors (Spade has about 800).

**We leave:**
- Splitting `>>` into `> >` for nested generics. Spade needs it because it has shift operators. We have none, so there's no `>>` token and `Tol<Ohm>` inside `Foo<…>` just works.
- Pipelines, registers, macros, and the parse-trace machinery.

---

## 5. Testing strategy

| Layer | How it's tested |
|---|---|
| Lexer / parser | Snapshot of the syntax tree per construct; snapshot of rendered diagnostics per error; fuzzing (like the existing `spicy_netlist` fuzz target) |
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
| 6 | Crate scaffolding | → Moved to M1a. `spicy_circuit` got its own design note before anything changed: ✅ `circuit.md` |
| 7 | Lesson crate | ✅ `crates/spicy_bounds` removed; `Cargo.toml` and `Cargo.lock` are back to their committed state. The affine math will be written inside `spicy_engine` in M3 |
| — | Clippy warnings | Left for now: 86 warnings from the newer clippy (1.98) in existing code. CI reports them without failing |

### M1: Language front-end, up to a SPICE export

**Goal:** `spicy export circuits/ce_amp.spl` writes a SPICE netlist of the amplifier. That tests the whole block side of the language end to end, without the engine. (The contract side, specs and measures, is tested in M3.)

**Why this order** (agreed 2026-09-26):
- Each step has its own tests and review.
- The export gives two checks that need neither ngspice nor the engine:
  - the exported text, parsed back by `spicy_netlist`, must give the same `Circuit` + `Params`;
  - simulating it must give VC ≈ 5.52 V (§1).

**Steps:**

- **M1a: Grammar note.** `grammar.md`: tokens, keywords, EBNF, operator precedence (agreed), unit literals, the list of syntax errors. Drafted, awaiting review. 🔍
- **M1b: Lexer.** Creates `spicy_lang`.
  - Every token of `grammar.md` §2, whitespace and comments kept.
  - Unit suffixes split into prefix and unit (§5).
  - Lexer diagnostics through codespan-reporting.
  - **Done when:**
    - joining the tokens' text rebuilds every input byte for byte (`ce_amp.spl`, every test file, and a fuzz target);
    - a snapshot of `ce_amp.spl`'s token list;
    - a snapshot per lexer error (`grammar.md` §7: #2–3, #9–15, #22–24).
  - Design: `lexer.md` (two passes, data layout, errors, tests).
  - **Done 2026-09-26:** `crates/spicy_lang` (lexer), 17 case files plus `circuits/ce_amp.spl`, the fuzz target `fuzz_spicy_lang_lexer`. 🔍
- **M1c: Parser → AST.** Design: `ast.md`. **First version built 2026-09-26:** `crates/spicy_lang/src/parser`, 22 case files, the fuzz target `fuzz_spicy_lang_parser`.
  - The grammar of §3, the precedence and the `±` rule of §4, and recovery (§6).
  - **Done when:**
    - a snapshot of `ce_amp.spl`'s AST;
    - a snapshot per remaining syntax error in §7;
    - a fuzz target that never panics and always returns a tree or diagnostics. 🔍
- **M1d: Elaboration → `spicy_model`.** Creates `spicy_model`. Design: `model.md` (agreed 2026-09-27). **Resolve for blocks built 2026-09-27** (`spicy_model`, `spicy_lang::resolve`). **Flatten, first version, built 2026-09-28** (`spicy_model::flatten`, `spicy_lang::elaborate`; plan in `model.md` §9): every root, nets merged and named, per-placement knobs, recursion and the whole-net checks. **Reviewed 2026-09-29** (`research/flatten_decisions.md`): ids only, read through a `Flat` handle; a `Reported` proof token for everything broken; the checks in two tiers, the block's own and the ones for simulating it (`check_simulation`); sources counted innermost, as KiCad does; slang's "in 2 of 3 placements" wording; a size limit. Contracts (M1d-5) next, which add the `assume` knobs and the flat contract.
  - Name resolution, pin binding (every pin exactly once) and unit checking.
  - Role checks (one source per `Power` net).
  - Flattening the hierarchy, and extracting the knobs and the contract.
  - `spicy check --dump-model`.
  - **Done when:** `ce_amp.spl` elaborates to exactly the 8 knobs and 3 specs of §4.2, and every semantic error has a snapshot-tested diagnostic. 🔍
- **M1e: Lowering → `spicy_circuit`** (formerly M2b).
  - A flat design at a knob point becomes a `Circuit` + `Params`, with origins set to model paths.
  - The default bench (language §8.5): a DC source on each `Power<In>` port at its assumed nominal, and an AC source on `Analog<In>`.
  - Two decisions, in the step's design note:
    - what "nominal" means for a range knob (the midpoint gives β = 200, as in the walkthrough);
    - the default model for a bare `Npn`: **decided (D-A, 2026-09-27)**, `BjtModel { is: 1e-14, bf: ← q1.beta, xtb: 1.5, xti: 3.0, eg: 1.11, tnom: 25 °C }`, always written out in exports and reports (`engine_plan.md` §5.4). It needs the new `BjtModel` fields of `engine_plan.md` §5.2 (done).
  - **Done when:**
    - a snapshot of the lowered circuit;
    - simulated natively, the operating point gives VC ≈ 5.52 V;
    - on ngspice, with the D-A model at TNOM 25 °C, VC = 5.503227 V (`engine_plan.md` §2.1). 🔍
- **M1f: SPICE export** (formerly the first half of M4).
  - `Circuit` + `Params` + analyses → SPICE text, plus a name map (`amp.r1` ↔ `R.amp.r1`). Design: `netlist_writer.md` (decided 2026-09-30): `spicy_parser` became `spicy_netlist`, with a reader and a writer.
  - `spicy export`.
  - **Two export modes** (`engine_plan.md` §5, the engine's contract): the **numeric** export at one knob point, and the **engine deck** (`EngineDeck`: a `.param` per knob, `.temp {…}`, `.options` as an input, one `.model QM_<path>` per BJT named apart from its instance `Q_<path>`, no analyses, plus the knob and probe maps).
  - **Needs first, done 2026-09-30** (data only, no temperature physics): `Params.temp`, a `tnom` on every model, `BjtModel` `xtb`, `xti`, `eg`, `DiodeModel` `eg`, `xti`, and the solver options the source asked for (`engine_plan.md` §5.2); `spicy_netlist` accepts those model parameters, BJT `temp`/`dtemp`, `.temp <value>` and `.options` (`tnom`, `reltol`, `vntol`, `abstol`, anything else an error) (§5.3).
  - **Done when:**
    - round trip: `ce_amp.spl` → export → `spicy_netlist` → lower gives the same `Circuit` + `Params` + options;
    - the engine deck at nominal lowers to the same result as the numeric export, and ngspice reads back every knob's requested value at nominal and at one corner;
    - the same round trip for every `circuits/*.spicy` (SPICE → `Circuit` → export → SPICE → `Circuit`);
    - the exported amplifier simulates to the same VC. 🔍

  This step only depends on `spicy_circuit`, not on M1b–M1e, so it could also be built earlier or in parallel and tested on the existing `.spicy` files.

**Done when:** `spicy export circuits/ce_amp.spl` writes a netlist that round-trips and simulates to the walkthrough's operating point.

### M2: Native simulator input and simulator readiness

> **Superseded ordering:** `pipeline.md` §9 now defines the order of this work, as smaller steps each reviewed on its own: (1) cleanup, (2) stamp locations out of devices, (3) `spicy_circuit`, (4) one plan per circuit, (5) temperature. Steps 2 and 4 are deferred: they're a speed-up that matters once the engine runs many simulations. The items below stay as the list of what must eventually be done.

- **M2a: Extract `spicy_circuit`.** ✅ Done as pipeline step 3 (`circuit.md`); results stayed bit-identical.
  - Move parameter resolution from `spicy_simulate`'s `from_spec` into a Deck → Circuit lowering in `spicy_netlist`.
  - `spicy_simulate` then reads only `spicy_circuit`.
  - Behavior-preserving: **every existing snapshot stays byte-identical.** 🔍
- **M2b: Model → Circuit lowering.** Moved to M1e.
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

### Parser follow-ups (tracked, not scheduled)

Found while fixing the SPICE parser. None is needed for the MVP; each gets done when it starts to matter.

| Item | Why it matters | Where |
|---|---|---|
| **Keyword tables** instead of the 16-byte keyword buffer | Nothing checks the buffer's limit: a keyword longer than 16 characters, added later, would never match. Tables (name → meaning, matched case-insensitively) remove the limit and list every supported keyword in one place, the way ngspice declares device parameters (`bjt.c`, `BJTmPTable`) | TODO in `spicy_netlist/src/netlist_types.rs` (`Keyword`) |
| **Model coverage** | We accept 5 BJT and 3 diode model parameters; ngspice's parameter tables have 154 and 104 entries. Real vendor models (`VAF`, `IKF`, `CJE`, …) are rejected with `invalid param`. Support them, or accept and ignore them with a warning | `spicy_netlist/src/netlist_models.rs` |
| **Subcircuit scoping gaps** | `.model` cards inside a subcircuit are global; nested `.SUBCKT` definitions and `.global` aren't supported | `subcircuit_phase.rs`; `pipeline.md` §11 |
| **Parse allocations** | Parsing a 10,000-line netlist allocates 44.5 MB (about 4.4 KB per line): e.g. parameter lists rebuilt per device, a token vector per statement, subcircuit bodies cloned per instance. Profile before optimizing | parser |

### Simulator follow-ups (tracked, not scheduled)

**Decided 2026-09-27:** the engine MVP runs on **ngspice** (installed: ngspice-42, with KLU and the shared library). The engine assumes a working simulator at ngspice's speed; our simulator's issues wait, and it joins later as a second backend. The M1f SPICE export is what ngspice reads.

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

### M3: The engine MVP (`spicy_engine`), on ngspice

Planned in detail in `engine_plan.md` (accepted 2026-09-27; §9 is this list, §8 the tests). It replaces the earlier M3, which built the worst-point loop and affine forms first: with ≤ 12 knobs per spec, enumerating every corner is exact and cheap, so the loop comes after the MVP (decisions D-D, D-F).

M3a–M3e run on hand-written ngspice decks and hand-built contracts, so they don't wait for the language; M3f joins the two tracks.

- **M3a: Design note.** The plan's types (§6.2), the `Backend` trait, the two verdict tables, the report schema, and the `EngineDeck` type that M1f and M3b meet at (§5.1). Creates `spicy_engine` and `spicy_backends`. No `affine` module (D-F). 🔍
- **M3b: ngspice backend.** The libngspice worker process (D-B), request encoding, read-back of every knob, plausibility checks, restart after a crash; the `ngspice -b` cross-check path.
  - **Done when:** the backend-rule tests pass (plan §8.4), and the nominal CE amp gives VC 5.503227 V, |H(1 kHz)| 4.590771, f_low 20.126944 Hz (TNOM 25 °C). 🔍
- **M3c: `worst_case`.** The knob space, cones (the capacitor rule), the measures, the run table, enumeration per spec side, the inside-the-box guards (tangent check, 8 audit points, pooling, ascent), the numerical band, and the worst-case verdict table; the answer-key harness.
  - **Done when:** `worst_case` equals the answer key on every M3 case of plan §8.2; run counts pinned. 🔍
- **M3d: `sigma(3)`.** The exact distribution map (D3), the 3σ-point search from two starts, the other range corners, range nudges, the uniform-spread check, and the sigma verdict table; the σ answer-key harness.
  - **Done when:** σ values within 1e-4 of the σ key on the CE amp (both XTB settings), the plan's §8.3 UNDECIDEDs exactly, no false PASS on the adversarial suite; run counts pinned. 🔍
- **M3e: Output and store.** Records with `claim`, `next`, tags and `not_modeled`; the terminal table; `--format json` (unstable); `--explain`, `--at`, `--deep`; the per-revision store (`.spicy/checks/<rev>.json`) and `stale`.
  - **Done when:** snapshots of the CE amp and every suite case, as table and JSON. 🔍
- **M3f: End to end.** The flat design's knobs → knob space, `FlatContract` → plan, M1f's engine deck.
  - **Done when:** `spicy check circuits/ce_amp.spl` prints the plan's §2.8 table, equal to the answer key: bias FAIL at `worst_case` (6.5595 V) beside PASS at `sigma(3)` (6.2813 V); every other side PASS. Snapshot-tested. 🔍

**Acceptance for all of M3:** no false PASS on the adversarial suite (9 cases, kept in the repo as ngspice decks; plan §8.2).

### M4 and after (planned in detail once the MVP works)

- **M4: Our simulator as a second backend.** The simulator follow-ups above: the reciprocity fix, errors instead of panics and silent non-convergence, the exact AC grid, per-device records, then temperature (M2c), AC at the operating point (M2d) and engine tolerances (M2e). **Gate:** the adversarial suite passes on it, and it agrees with ngspice within the numerical band (engine v3 §6.4).
- **The loop, for more knobs:** the flip walk over cones, margin checks spliced into every side whose cone holds the device, the PASS allowance by measure kind (plan §9.3). **Gate:** it agrees with enumeration on every regression circuit of ≤ 12 knobs per side.
- **Transient tier:** THD and other `tran` measures, on demand, with large-signal margins.
- **M5:** Part records and `part:` pinning (knob identity kept); the datasheet-arithmetic engine, with affine forms (engine v3 §4.1).
- **M6:** Aging links and `life`; lots; the rest of the knob model (engine v3 §2).
- **M7:** Statistics: board yield, importance sampling (engine v3 §4.4).
- **M8:** Parameter layer and sensitivities in our simulator (transposed KLU solve, DC/AC adjoint).
- **Later:** language growth (generics, loops, interfaces, modules), automatic checks, the editor and the agent (plan §7, §9.3).
- **Later: our own temperature model.** The richest temperature behavior we have (datasheet curves, measured tempcos, later Verilog-A) lives in the part description, above `spicy_circuit`, which only carries SPICE-style parameters (TNOM, XTB, XTI, EG). Per part, the backend either **pre-applies** our law (hands the simulator values at the run's temperature, with TNOM = T) or **passes through** the SPICE parameters (vendor models it can't see into). Needs a research round first: VBIC/HICUM/BSIM temperature modeling, how Spectre and Xyce do it, fitting datasheet curves, and whether pre-applying covers internal effects such as junction capacitances. Not MVP.
- **Later: when specs run.** Tiers (live, save, idle, commit, CI, nightly, manual) placed by the engine's cost estimate, a `#[run(…)]` override, "gate by meaning, schedule by cost", and verdicts stored by cone fingerprint so fresh vs stale is exact. Designed in `research/runs_language.md`, measured in `research/runs_cost_tiers.md`. Not MVP: `spicy check` runs everything.

---

## 7. Decisions

| # | Decision | Status |
|---|---|---|
| 1 | Crate layout (§2.2), now with `spicy_circuit` for native simulator input | **Agreed:** the `spicy_circuit` data model is in `circuit.md` |
| 2 | ~~Our simulator for the MVP; ngspice right after~~ → **ngspice for the MVP; our simulator as the second backend (M4)** | **Agreed** 2026-09-27 |
| 3 | MVP language subset (§4.1) | To confirm |
| 4 | Syntax tree: typed AST with spans + the full token list; no lossless tree, no `rowan` (§4.4) | **Agreed** 2026-09-26 |
| 4b | Lexer and parser hand-written, no `logos`; Spade as a design reference only, never copied (§4.4, §4.5) | **Agreed** 2026-09-26 |
| 5 | Diagnostics: codespan-reporting | Agreed |
| 6 | Stale snapshot | Done: reverted to `V1` |
| 7 | Docs in `docs/ecad/` | Done |
| 8 | Commits | Only after your review of each milestone |
| 9 | One-time `cargo fmt --all` + fmt check in CI | Done |
| 10 | Clippy warnings in existing code | Left for now |
| D-A | Default model for a bare `Npn`: `IS=1e-14 BF={beta} XTB=1.5 XTI=3 EG=1.11`, TNOM 25 °C, written out (`engine_plan.md` §10) | **Agreed** 2026-09-27 |
| D-B | ngspice transport: libngspice in a worker process; `ngspice -b` as the tests' cross-check | **Agreed** 2026-09-27 |
| D-C | Round 2's D1–D10, as answered in `engine_plan.md` §10 | **Agreed** 2026-09-27 |
| D-D | No worst-point loop in M3: enumerate corners (≤ about 12 knobs per spec side); the loop comes after, gated on agreeing with enumeration | **Agreed** 2026-09-27 |
| D-E | Verdict words: PASS (all corners) / PASS (estimated) / PASS (implied by worst case); UNDECIDED with reasons and `next`; `simulated` / `stale`; no "verified" | **Agreed** 2026-09-27 |
| D-F | No affine forms in M3: measures evaluated per run; affine forms return with hierarchy, calibration, error budgets and datasheet arithmetic | **Agreed** 2026-09-27 |

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
| The `spicy_circuit` refactor subtly changes simulator behavior | Byte-identical snapshots as the gate (M2a); done as its own step. ✅ Passed: results stayed bit-identical |
| BJT convergence at extreme corners (no junction limiting) | The amplifier converged at the worst corner; add SPICE-style limiting if a corner fails |
| Nudged slopes drown in solver noise | M2e: tighter tolerances + a two-step-size consistency test |
| AC-at-operating-point bugs give plausible but wrong gains | Formula-based tests in M2d; cross-check with ngspice in M4 |
| The AST turns out too lossy for the editor (e.g. moving code with its comments) | The full token list keeps every character, so a small CST of our own can be added over the same lexer without changing the language (§4.4) |
| Scope creep | §4.1 is the contract for the MVP. Anything outside it goes to M4+ |
