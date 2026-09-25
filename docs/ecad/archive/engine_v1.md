# AI-Native ECAD Editor — Engine, Language & Simulation Design Notes

> Working draft · Last updated 2026-09-24
> Current architecture thinking, the decisions made so far, and the reasoning behind each.

---

## 0. Context

The editor is KiCad-like and AI-native, focused on schematics and simulation. PCB layout and placement are out of scope for now. Ideas already in the spec that this document builds on:

- Hierarchical blocks with an I/O page per block
- Block-level and system-level simulations, with results shown on the schematic
- Parametric component values with equations and rationale, plus project-wide parameters
- AI extraction of manufacturer part data into simulation models
- Tight integration with our own SPICE simulator (KLU-based)

This document covers the engine underneath: how values, tolerances, constraints and simulation results fit together, and how circuits are described (code and schematic).

**Core goal:** run simulations, feed the results back into tolerance/range checks, and use that to help guide component values.

---

## 1. Decisions at a glance

| # | Decision | Choice | Main reason |
|---|---|---|---|
| D1 | Adopt an existing tool or build | Build our own engine in Rust | The math core is small, existing engines don't close the simulation loop, and it keeps everything in one stack |
| D2 | Core value representation | Affine forms with units | Tracks correlation automatically; coefficients equal sensitivity × tolerance, so simulation results plug in directly |
| D3 | How simulation feeds range checks | Metrics + sensitivities → affine forms | One representation for static equations and simulated metrics |
| D4 | Rigor of simulation-derived checks | Linearized, then verified with corners + Monte Carlo | Their error terms are estimated, not proven |
| D5 | Data model | One canonical semantic model; code and schematic are views of it | The engine sees one thing, and both editors stay consistent |
| D6 | Source-of-truth granularity | Per block: each block is either drawn or coded | Avoids the hardest round-tripping problems |
| D7 | Circuit language | Our own, Rust-like, Spade-inspired | Preference for Rust syntax, Starlark rejected, full control |
| D8 | External solvers (dReal, IBEX) | Deferred | Only needed for exists–forall synthesis with logic; C++/FFI cost |
| D9 | Build order | Language first, schematic editor later | Fastest way to drive the engine and simulator without a GUI; text is the AI's most reliable interface |

---

## 2. Landscape review

### atopile

- MIT-licensed language, compiler and toolchain. Declarative `.ato` files; KiCad for layout.
- **Engine:** its own *faebryk parameter solver*, not Z3 or SymPy. It does symbolic rewriting over interval sets: values are ranges, parameters are symbols, constraints are subset/equality relations. Rewrite passes (canonicalize, fold literals, merge duplicates, subsumption) run to a fixed point; an empty set means a contradiction. Graphs are append-only so failures can be traced to their source.
- **Checks:** `assert` with `<`, `>`, `within`, evaluated worst-case across tolerance ranges. Purely algebraic: no transient, no AC, no simulation.
- **Part picking:** passives auto-picked from the JLCPCB catalog within the solved bounds; ICs added by hand (LCSC number or MPN) or from a package registry. Picking is tied to JLC's catalog.
- **Weakness:** literal ranges are treated as uncorrelated even with themselves, so expressions overestimate (the dependency problem, §4.1) unless they happen to be rewritten well. Syntax has changed noticeably between versions.

### Zener (Diode Computers)

- Starlark-based language with a Rust toolchain (built on starlark-rust), MIT-licensed. Generates KiCad layout files.
- **Imperative:** you compute values in code. Its `PhysicalValue` type carries nominal, min, max, tolerance and unit, with unit-tracked arithmetic and `.within()` checks. No general constraint solver found.
- Code renders to graphical schematics; placement is persisted as metadata in trailing comment blocks. ngspice-backed testbenches.
- A useful precedent for code and schematic coexisting. Rejected as our language because it's Starlark.

### Others

- **SKiDL:** plain Python netlist generation with ERC and SPICE output; can generate KiCad schematics.
- **PolymorphicBlocks (Berkeley):** the strongest electrical model (voltage/current limits, signal-level compatibility); abstract parts refined into concrete ones; GUI actions generate code. Alpha.
- **tscircuit:** React/TypeScript; goes all the way through layout with its own autorouter.
- **JITX:** commercial; AI writes the design code. Criticized for unreadable generated schematics.

### Community feedback on code-first design

- **Positive:** reusable modules; AI-generated designs are surprisingly good.
- **Negative:** loses what a good schematic conveys (signal flow, readability when debugging); schematic capture is a small share of total effort; part-library coverage is the real bottleneck.
- **Takeaway:** keep graphical schematics first-class. Code complements them rather than replacing them.

---

## 3. D1 — Build our own engine

Reasons not to adopt atopile or Zener:

1. Both are code-first design toolchains whose math is embedded in Python or Starlark ecosystems. Our editor is schematic plus simulation, written in Rust.
2. Neither feeds simulation results back into range checks, which is our core goal.
3. atopile's interval approach overestimates on correlated expressions; Zener has no solver at all.
4. Their biggest value-adds (KiCad layout sync, JLC part picking) are outside our current scope.
5. The math we need is small and well understood (§4).

---

## 4. Math foundation

### 4.1 Interval arithmetic

Every value is a range, and operations produce ranges guaranteed to contain every true result.

- `[a,b] + [c,d] = [a+c, b+d]`; multiplication takes the min and max of the four corner products.
- **Sound:** if the output range meets the spec, every real combination of parts does too.
- **Dependency problem:** a variable that appears more than once is treated as independent copies, which inflates the result.

Example: 5 V into a divider, R1 = R2 = 10k ±1%.

| Method | Vout range |
|---|---|
| True range | 2.475 – 2.525 V |
| Naive interval, `Vin·R2/(R1+R2)` | 2.450 – 2.550 V (twice too wide) |
| Single-use rewrite, `Vin/(1+R1/R2)` | 2.475 – 2.525 V (exact) |

Rule: expressions where each variable appears once give exact bounds; repeats inflate them. A 2.5 V ±1.5% spec would falsely fail the naive form.

### 4.2 Affine arithmetic — the core representation (D2)

```
x = x₀ + x₁ε₁ + x₂ε₂ + … + xₙεₙ
```

- `x₀` is the central value.
- Each `εᵢ` is an unknown but fixed number in [−1, 1], one per independent source of uncertainty: a component's tolerance, temperature, supply voltage. **Noise symbols are shared project-wide.**
- Each `xᵢ` is how much `x` moves because of source *i*.
- Range: `x₀ ± Σ|xᵢ|`.
- Converting an interval `[a,b]`: `(a+b)/2 + (b−a)/2 · ε_new`.

**Operations**

- **Linear (±, scaling):** exact, coefficient by coefficient. `R1 − R1 = 0` exactly, because both copies share ε₁.
- **Nonlinear (×, ÷, exp, …):** best linear approximation plus one new noise term that bounds the approximation error. Results stay sound.

**The divider again**, with R1 = 10 + 0.1ε₁ and R2 = 10 + 0.1ε₂ (kΩ):

```
R2/(R1+R2) ≈ 0.5 − 0.0025ε₁ + 0.0025ε₂ + (tiny error term)
Vout ≈ 2.5 ± 0.025 V   — within ~0.5 mV of exact
```

**Why it fits this project**

- The coefficients `xᵢ` are exactly sensitivity × tolerance: a first-order Taylor expansion with a guaranteed error bound.
- A simulated metric with gradients `gᵢ` becomes `m₀ + Σ gᵢ·tolᵢ·εᵢ`. It shares the same `εᵢ` as the static equations, so correlations carry through when the two are combined.
- One form yields three answers:
  - worst case: `Σ|xᵢ|`
  - statistical spread (RSS): `√(Σxᵢ²)`
  - margin diagnosis: the largest `|xᵢ|` identifies the part to change

**Limits**

- First-order only: strong nonlinearity over wide ranges grows the error term. Fix with bisection (§4.3).
- Noise symbols accumulate (one per nonlinear operation). Periodically merge the small ones.
- If more accuracy is ever needed, Taylor models are the higher-order generalization.

**Versus atopile:** atopile tracks correlation *symbolically* (keep symbols, rewrite expressions to avoid repeats; exact when a rewrite succeeds). Affine arithmetic tracks it *numerically and automatically*, at the cost of first-order approximation.

### 4.3 Solving for unknowns: constraint propagation + branch-and-prune

Used for values marked `?` in the language (solver-chosen).

**Contractors (HC4):** evaluate the expression tree forward, then project backward, shaving impossible values off each variable's range. Cycle through all constraints until nothing shrinks.

Example: need 3.3 V ±2% from 5 V, with R1 and R2 anywhere in 1k–1M.

1. From the spec, the ratio `R2/(R1+R2)` must lie in 0.647–0.673.
2. So `R1/R2` must lie in 0.485–0.546.
3. Back into the ranges: R2 ≥ ~1.83k, R1 ≤ ~546k.

**Branch and prune:** when contraction stalls, bisect one range, contract each half, discard empty halves, repeat to the target precision. The output is a set of boxes guaranteed to enclose every solution. Exponential in the number of variables, but subcircuit sizing usually involves 2–10. Splitting also reduces the dependency problem.

### 4.4 Logic and exists–forall (deferred, D8)

- **SMT** adds logic (if/else topologies, choosing among part options): a SAT solver picks which constraints are active, and a theory solver checks the math.
- **δ-completeness** (dReal): answers either "unsat" (provably no solution) or "δ-sat" (a solution exists if constraints are loosened by δ, default 0.001).
- **Exists–forall** is the true tolerance-design question: *do nominal values exist such that the spec holds for every tolerance deviation?* dReal4 solves it directly by combining interval propagation, counterexample-guided synthesis and numerical optimization.

### 4.5 Discrete part selection

- **Brute force first:** E96 from 1k to 1M is 288 values, so two resistors give ~83,000 pairs. At microseconds per exact check, the whole search takes under a second.
- **CP/MILP** only when optimizing board-wide cost or BOM consolidation.

---

## 5. Simulation in the loop (D3, D4)

### 5.1 Static vs dynamic

- **Static:** algebraic relations that hold at every instant (divider ratio, DC gain). The engine evaluates these directly.
- **Dynamic:** capacitors and inductors add state, so behavior depends on history and frequency: overshoot, settling, ripple, loop stability, inrush, filter response. Nonlinear devices remove closed-form answers, so these must be simulated.
- **Simulator analyses:** DC (Newton on the MNA system), AC (linearize at the operating point, solve `(G + jωC)x = b` per frequency), transient (time discretization with Newton at each step). Each run evaluates one point in parameter space.

### 5.2 Waveforms become metrics

Every spec needs a scalar measurement extracted from a waveform (like SPICE `.meas`): DC output, peak-to-peak ripple, overshoot %, settling time, phase margin, gain at a frequency, peak current.

Each metric is a function `m = f(p)`, where `p` covers component values, temperature and supply. The simulator is an expensive black box that computes `f`.

### 5.3 Bridging point results to ranges

| Method | Gives | Cost | Verdict |
|---|---|---|---|
| Corners | A bound, only if the metric is monotonic | 2ⁿ runs | Verification only; misses interior worst cases (e.g., an LC resonance landing on a fixed frequency) |
| Monte Carlo | Yield estimate, not a bound | Hundreds to thousands of runs, parallel | Final verification |
| **Sensitivities → affine forms** | Worst case, RSS, diagnosis | ~1 extra solve per parameter or per metric | **Primary method** |
| Surrogate models (quadratic, Gaussian process) | Cheap approximation | A few dozen runs | For noisy, non-smooth or slow metrics |
| Verified (interval/Taylor ODE) simulation | Guaranteed trajectory bounds | Bounds blow up (wrapping effect, stiffness) | Rejected as main path |

Worked example: overshoot 8%, spec < 12%, sensitivity terms C_out ±2.5%, ESR ±1.5%, L ±0.5%.

- Worst case: 8 + 4.5 = 12.5% → **fails**
- RSS: 8 + √(2.5² + 1.5² + 0.5²) ≈ 11% → **passes**
- Diagnosis: C_out's tolerance consumes most of the margin, so change that part

### 5.4 Computing sensitivities in our simulator

- **Direct method:** per parameter, one extra solve per converged step, reusing the existing KLU factorization (substitution only). Best with few parameters and many metrics.
- **Adjoint method:** one transposed solve gives one metric's gradient with respect to all parameters. KLU solves the transposed system with the same factors. Best with many parameters and few metrics.
- **DC and AC:** cheap and easy either way. Implement first.
- **Transient:** direct is straightforward; adjoint needs a backward-in-time pass over stored states. Later.
- **Device models:** use forward-mode automatic differentiation (dual numbers) on model equations instead of hand-derived derivatives.
- **Non-smooth metrics:** settling time jumps when a ringing peak crosses the band edge; switchers have discrete events. Use smooth metric definitions, and averaged models or periodic steady-state analysis for switching converters.
- **Labeling (D4):** static affine checks are shown as *guaranteed*; simulation-derived checks as *linearized, verified by simulation*.

### 5.5 Guiding values: the design-centering loop

The goal isn't values that pass at nominal. It's placing the nominal values so the entire tolerance box sits inside the spec with maximum margin.

1. **Static prune.** The affine/interval engine shrinks the design space using algebraic constraints. Cheap: thousands of evaluations per millisecond.
2. **Simulate** the current candidate and extract metrics.
3. **Sensitivities → affine forms →** worst-case margin for every spec.
4. **Optimizer step.** The gradient of the smallest margin gives the direction to move the nominal values (maximin).
5. **Snap to real parts.** Enumerate the few E-series or catalog neighbors around the continuous optimum.
6. **Verify** the final parts with the full nonlinear simulator: corners plus Monte Carlo. The linear model is trusted only after this passes.

Simulations are expensive and affine evaluations are nearly free, which is why step 1 exists.

### 5.6 Hierarchical blocks

- A block-level simulation characterizes the block's I/O page as affine forms (e.g., a regulator's output voltage with its tolerance terms, output impedance vs frequency).
- System-level checks consume those forms without re-simulating the block.
- **Caveat — loading:** blocks interact through port impedances. Either include source/load impedance in the characterization, or re-simulate at system level before sign-off.

---

## 6. Code + schematic (D5, D6)

### 6.1 One canonical model

The source of truth is a semantic model: components, nets, hierarchy, parameter expressions, constraints, and simulation testbenches with their metrics. The code editor and the schematic editor both edit this model. The engine only ever sees the model.

### 6.2 Per-block source of truth

- Each hierarchical block is either **drawn** or **coded**.
- **Coded blocks** render as auto-placed, read-only schematics. User placement tweaks are saved as metadata in a sidecar file.
- **Drawn blocks** can be exported to code.
- **Typical split:** top level and analog sections drawn; repetitive or parametric sections coded (sensor arrays, divider generators).
- Parameter expressions use one syntax in both views.
- The block boundary (the I/O page) is where the two views meet.

**Why per block:** generative code (loops, conditionals, computed values) can't be edited as a drawing without breaking the generator, and auto-drawn schematics tend to be unreadable. Full two-way editing of the same block is the single biggest time sink on the list, so it's deferred.

---

## 7. Language (D7)

### 7.1 Why our own

- Wanted: a simple-Rust look, Spade-like. Starlark rejected, which rules out Zener's language.
- The language describes a static graph, so its semantic surface is small enough to own.

### 7.2 Borrowed from Spade

| Spade idea | Our version |
|---|---|
| Standalone language, not embedded in a host (unlike Chisel) | Clean Rust-like syntax with no host-language baggage |
| Ports as bundles of directional wires; linear typing ensures every wire is driven exactly once | Typed interfaces (`Power`, `I2c`, `Analog`, …). ERC by the type system: one source per power net, no fighting outputs, every input driven |
| Type-level arithmetic to prevent truncation | Physical dimensions in the type system (V × A = W checked at compile time) |
| Expression-based, immutable variables | A static, analyzable graph, which makes code→schematic rendering tractable |
| Distinct unit kinds (fn / entity / pipeline), instantiated differently | `fn` for pure parameter math; `block` for a circuit with ports (drawable) |
| Attributes (`#[...]`) | Semantic metadata only: part number, simulation model |

### 7.3 New for circuits

- Units and tolerances in literals: `10k +- 1%`, `3.3V`, `100nF +- 10%`
- `?` marks a value the solver chooses
- `require` states a constraint — either a static equation or a simulated metric. Both become affine forms in the same checker.

### 7.4 Sketch

```rust
block AdcInput(vin: Power, out: Analog) {
    let r_top: Ohm = ?;                  // solver chooses
    let r_bot: Ohm = ?;

    let r1 = Resistor(r_top +- 1%, "0402");
    let r2 = Resistor(r_bot +- 1%, "0402");
    let c1 = Capacitor(100nF +- 10%, "0402");
    vin.pos -- r1 -- out -- r2 -- vin.gnd;
    out -- c1 -- vin.gnd;

    // static check, evaluated as an affine form
    require vin.v * r_bot / (r_top + r_bot) in 3.3V +- 2%;
    require r_top + r_bot in 10k..100k;

    // simulated metric, fed back through sensitivities
    require tran(step(vin)).settle(out, 0.1%) < 2ms;
}
```

### 7.5 Design rules

1. **If it looks like Rust, it behaves like Rust.** You and the AI will both assume Rust semantics, so never reuse Rust syntax with a different meaning.
2. **Skip the hard Rust parts.** No borrow checker, no lifetimes, no traits at first. Generative constructs (`for`, `if`, const generics) run only at elaboration (compile time).
3. **Units in types, tolerances in values.**
4. **Placement stays out of the code.** Sidecar file keyed by instance path (e.g. `adc_input.r1`), so nudging a symbol never changes the code. Attributes only for semantic metadata.
5. **Generated blocks are read-only in the schematic.**

### 7.6 Implementation stack

| Need | Choice |
|---|---|
| Lexer | `logos` |
| Parser | `chumsky` or hand-written recursive descent |
| Diagnostics | `ariadne` or `miette` (rustc-quality errors) |
| Highlighting | tree-sitter grammar |
| Editor integration | LSP via `tower-lsp` |
| Incremental compilation | `salsa` (later, for large projects) |
| Interval arithmetic | `inari` (IEEE 1788) |

Reference implementation to study: Spade's compiler (open source on GitLab, Rust toolchain).

---

## 8. External engines and engine tiers (D8)

| Engine | What it is | License | Role for us |
|---|---|---|---|
| IBEX | C++ library for interval arithmetic and contractor programming; guaranteed solution enclosures and global optimization; affine arithmetic plugin | LGPL (dynamic linking if closed-source) | Possible test oracle, or a Tier B backend |
| dReal | SMT solver for nonlinear real arithmetic; δ-complete; exists–forall support; built on IBEX | Apache 2.0 | Only if we need exists–forall with logic and transcendental functions. Releases have slowed; check activity first |

Both are C++, so using them from Rust means FFI.

| Tier | Capability | Plan |
|---|---|---|
| A | Unit-checked tolerance arithmetic (Zener's level) | Build — small |
| B | Solve unknowns + worst-case checks (atopile's level, done better with affine forms) | Build — HC4 + bisection, a few hundred lines on top of an interval library |
| C | Exists–forall synthesis with logic and transcendental functions | Use dReal if and when needed |

---

## 9. Proposed build order

1. **Language front-end:** lexer, parser, types and units, elaborator → canonical model
2. **Affine forms + units;** static `require` checks (Tier A)
3. **Metrics + DC/AC sensitivities** in the KLU simulator
4. **Range-check display:** worst case, RSS, per-part margin ranking (also feeds AI explanations)
5. **Solve `?` unknowns:** HC4 + bisection (Tier B)
6. **Value guidance:** design centering + snap to real parts
7. **Verification:** corners + Monte Carlo; transient sensitivities
8. **Schematic editor** as a second front-end on the same model

---

## 10. Open questions

- Per-block edit mode vs editing the same block both ways (leaning per-block).
- Tolerance semantics for RSS: is ±tol a hard bound, 3σ, or something else? Per-part distributions?
- Switching converters: averaged models vs periodic steady-state analysis.
- Part catalog source for picking — avoid tying the tool to one distributor/assembler the way atopile ties to JLC.
- Connection syntax: `a -- r1 -- b` chains aren't Rust. Keep them, or find something more Rust-native?
- Temperature and aging as noise sources: when to include them.
- How AI-extracted part data maps into device models and their parameter tolerances.

---

## 11. References

- atopile — https://github.com/atopile/atopile
- Zener / pcb — https://github.com/diodeinc/pcb · language spec: https://docs.pcb.new/pages/spec
- Spade — https://spade-lang.org/
- IBEX — https://github.com/ibex-team/ibex-lib
- dReal4 — https://github.com/dreal/dreal4
- Kong, Solar-Lezama, Gao — *Delta-Decision Procedures for Exists-Forall Problems over the Reals* (CAV 2018) — https://arxiv.org/abs/1807.08137
- PolymorphicBlocks — https://github.com/BerkeleyHCI/PolymorphicBlocks
- SKiDL — https://github.com/devbisme/skidl
- tscircuit — https://github.com/tscircuit/tscircuit

