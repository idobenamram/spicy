# AI-Native ECAD Editor — Engine Design Notes v2

> **Superseded by `../engine.md` (2026-09-25).** Kept for history; corrections from the research round were applied here too. The `spicy_bounds` lesson crate this doc refers to has been removed; the numbers quoted from it stay here for reference.
> Working draft · 2026-09-24 · supersedes the engine parts of `engine_v1.md` (v1)
> Paper references like **[N&P07]** point into `../bibliography.md`.
> **New here? Start with `../walkthrough.md`**, which follows one amplifier through the whole engine with real numbers.
> How specs are defined (contracts, setups, automatic specs): `../specs.md`.

## What changed from v1, in five sentences

1. Affine arithmetic (AA) stays, but its job changes: it's the **common format** every check produces and reads, not the method that computes every bound.
2. For **linear DC circuits**, checking the corners of the tolerance box gives the **exact** answer (not a heuristic), so those bounds come from the circuit itself, not from hand-written equations.
3. Every check reports a **bracket** (an outer bound and an inner bound), and a failing check comes with a **concrete counterexample**.
4. Uncertainty comes in **two kinds** that must be combined differently: *range* (temperature, supply, aging: must work everywhere) and *statistical* (manufacturing spread: a yield question).
5. Simulation results are linearized **at the worst point, not at the nominal**, and the prediction is then checked by simulating that point.

Part A builds these ideas from scratch, one at a time, each with a small example. Parts B–F are the design itself.

---

# Part A — The ideas, step by step

Each section has a lesson in `crates/spicy_bounds/examples/` that prints the numbers used here.

## A1. What "a bound" means

Take the v1 example: 5 V into a divider, R1 = R2 = 10k ±1%. **What values can Vout take?**

There are two different kinds of answer, and we want both:

- **Outer bound:** "Vout is *definitely inside* [a, b]." Safe, but possibly too wide.
- **Inner bound:** "Vout *definitely reaches* every value in [c, d]." Proven reachable by some real combination of parts.

The true range is squeezed between them:

```
outer   [------------------------------]      ← nothing lies outside this
true        [----------------------]          ← what we actually want
inner          [----------------]             ← all of this really happens
```

Why both? Because a spec check then has **three** honest outcomes:

| Situation | Verdict | Meaning |
|---|---|---|
| Spec contains the whole outer bound | **PASS** | Nothing can violate it |
| Some reachable value is outside the spec | **FAIL** | And we can show the exact parts that cause it |
| The spec edge falls between inner and outer | **UNDECIDED** | Work harder (split, simulate more) |

v1 only had outer bounds, so it could never *prove* a failure, and it couldn't say how pessimistic a bound was. The gap between inner and outer **is** the pessimism.

## A2. Interval arithmetic and the dependency problem

Interval arithmetic replaces each number with a range and gives a guaranteed outer bound. Its flaw: it **forgets that two appearances of R1 are the same resistor**.

The smallest example (lesson 1, step 1):

```
R1 = 10k ±1% = [9900, 10100]
R1 − R1  →  interval: [−200, 200] Ω     (should be exactly 0)
```

Interval arithmetic pairs "R1 at its lowest" with "R1 at its highest", which can't happen. The divider formula `5·R2/(R1+R2)` has R2 twice, so the same thing inflates it (lesson 1, step 2):

| Method | Vout range | Width vs true |
|---|---|---|
| True range | 2.47500 – 2.52500 V | 1.0× |
| Interval, `5·R2/(R1+R2)` | 2.45050 – 2.55051 V | **2.0×** |
| Interval, rewritten `5/(1+R1/R2)` | 2.47500 – 2.52500 V | 1.0× |

The rewrite works because each variable then appears once, but that trick only goes so far. atopile has exactly this problem: its own test asserts the divider output is within [0.45, 50] when the true range is [0.82, 9.09] (`externals/atopile/test/core/solver/test_solver.py:915`), and its library parts hand-write every rearrangement "for the solver".

## A3. Affine arithmetic: remembering which part is which

The fix is to give every source of uncertainty a name, a **knob** ε that can sit anywhere in [−1, 1]:

```
R1 = 10000 + 100·ε[R1]        ε[R1] = −1 means R1 = 9900, +1 means 10100
R2 = 10000 + 100·ε[R2]
```

A value is then "a center, plus how much each knob moves it, plus an error bucket":

```
x = center + c₁·ε₁ + c₂·ε₂ + … ± err
```

- **Adding / subtracting** is exact, knob by knob. `R1 − R1 = 0` exactly, because both copies use the same ε[R1].
- **Multiplying / dividing** isn't a straight line, so AA draws the best straight line through the curve and puts the worst-case distance between line and curve into `err`. The result is still guaranteed; `err` is the price of the straight line.

The divider, computed by the engine (lesson 1, step 2):

```
Vout = 2.500125 − 0.012501·ε[R1] + 0.012500·ε[R2] ± 0.000376
```

Read it like this:

- **Each coefficient is "how much Vout moves when that part goes to the edge of its tolerance."** R1 at +1% moves Vout by −12.5 mV. That's sensitivity × tolerance, and it's why AA fits a simulator: the simulator can compute sensitivities (A8).
- **Worst case** = add up the absolute coefficients: 2.5 ± 0.025 V, plus the 0.38 mV error bucket.
- **Ranking**: the biggest coefficient is the part eating the most margin.

Add a 5 V ±2% supply and the ranking tells you where to spend money (lesson 1, step 3):

| Source | Moves Vout | Share of worst case |
|---|---|---|
| Vin (±2%) | ±50.0 mV | 65.5% |
| R1 (±1%) | ∓12.5 mV | 16.4% |
| R2 (±1%) | ±12.5 mV | 16.4% |
| approximation error | 1.4 mV | 1.8% |

Better resistors won't fix this divider; a better supply will.

**Inner bound for free.** Set every knob to the sign of its coefficient. That's a real combination of parts, and it pushes the straight-line part to its maximum. The true value there is at most `err` away. So:

```
inner = [center − Σ|cᵢ| + err,  center + Σ|cᵢ| − err]  =  [2.47550, 2.52475]
outer = [center − Σ|cᵢ| − err,  center + Σ|cᵢ| + err]  =  [2.47475, 2.52550]
true  =                                                    [2.47500, 2.52500]
```

The bracket is 0.75 mV wide on each side. That's A1's "how pessimistic is this" answered by one affine form.

**Which straight line?** For 1/x we use the Chebyshev line (the one with the smallest maximum error). An earlier version used the "min-range" line, which tilted the coefficients to −12.25 / +12.75 mV and doubled `err`. The choice matters, and it's something the engine will tune per operation [Stolfi97, R&K15].

## A4. Where affine arithmetic struggles

AA is excellent when the knobs only move things a little (tolerances of a few %). It gets bad in three situations:

**1. Wide ranges.** When the solver searches for a resistor anywhere in 1k–1M, AA has to fit a straight line to 1/R across three decades:

```
1/R for R in [1k, 1M]:
  true range     [0.000001, 0.001]  S
  affine range   [−0.000937, 0.001] S     ← allows *negative* conductance
  error bucket   ±0.000469              ← about half the answer
```

The fix is to split the range into pieces and work in **log space** (log R), since component values and E-series are spread logarithmically.

**2. Solving the circuit matrix with AA.** Every elimination step makes a straight-line approximation and adds error. Papers that did this found the solver only converged up to about 1–5% tolerance on transistor circuits unless the ranges were split [GOB08, Sch18].

**3. Humps.** A straight line can't follow a curve that goes up then down inside the range, e.g. a resonance landing on the test frequency. AA stays safe but gets loose [Din15].

**So AA becomes the common format, not the solver.** Each kind of question gets the method that suits it (Part B), and every method hands back its answer as an affine form so everything downstream (worst case, ranking, statistics, reports) works the same way.

## A5. The corner theorem for linear circuits

This is the most useful new idea, so here it is slowly.

**Claim.** In a *linear* circuit (resistors, sources, controlled sources, op-amp models; no diodes or transistors), if you change **one** resistor and hold everything else fixed, every node voltage moves **in one direction only**. It never goes up and then back down.

**Why.** Each resistor adds its conductance g into the MNA matrix in one small pattern (a "rank-1 stamp"). Working it through, any node voltage ends up as

```
V = (a + b·g) / (c + d·g)          a, b, c, d depend on the other parts
```

and a curve of that shape has no hump, as long as the circuit stays solvable (c + d·g ≠ 0) [PPC65]. The same holds for a resistor (through g = 1/R), a conductance, an independent source value, and a controlled-source gain, because each enters the matrix as **one rank-1 term**. It does **not** hold for a transformer turns ratio, a potentiometer wiper, or any knob shared by several elements (catch 2 below).

**Consequence.** Take any combination of parts inside the tolerance box. Push R1 toward whichever edge raises V; V can only go up. Then R2, then R3… You end at a **corner** of the box, and V went up at every step. So **the maximum is at a corner**, and so is the minimum [N&P07 Thm 5.1].

```
R2 ↑
    ●───────────●      ● = corners: the extremes are here
    │           │
    │   ·  ·    │      · = any interior combination:
    │     ·     │          never beats the best corner
    ●───────────●
                  → R1
```

For the divider: max Vout is at R1 low, R2 high → 5·10.1/(9.9+10.1) = 2.525 V. **Exact.**

**I checked it numerically:** 388 random resistor + controlled-source networks, ±20% on every part, 116,400 random interior points. **No interior point beat the corners.** The only exceptions were circuits that become unsolvable somewhere inside the box (the matrix goes singular: a gain stage turning unstable). That can be detected, and is worth flagging to the user on its own.

**Why this matters.** For linear DC we don't need AA's straight-line approximations at all: we can get the **exact** range, so inner = outer. v1 said corners "give a bound only if the metric is monotonic" (§5.3). For this class they always do.

**The catches:**

1. **Too many corners.** 20 parts = 2²⁰ ≈ 1M corners. The fix: if we can *prove* "V always rises when R3 rises, everywhere in the box", R3's edge is decided. For small circuits and moderate tolerances most directions can often be proven, and then about **2 solves** suffice. [N&P07] warn that such proofs typically fail in high dimensions, so large circuits fall back to enumeration or the loop.
   - ⚠️ The direction at the nominal point is **not** proof. It can point at the wrong corner [N&P07 Ex. 41, TS00]. The direction must be checked over the whole box [Kol14b, SH18].
2. **One knob moving several parts.** Temperature changes every resistor through its tempco; a "same reel" knob shifts several parts together. Then humps *can* happen. Example: a bridge where one temperature knob t raises R1 and lowers R4:
   ```
   Vout(t) = 1/(2+t) − (1−t)/(2−t)
   Vout(−1) = 0.333    Vout(+1) = 0.333    but Vout(0) = 0   ← minimum in the middle
   ```
   The fix: there are only a few such shared knobs (temperature, supply, lot), so split over them and use the corner method inside each slice.
3. **AC magnitude.** In AC, changing one part moves the complex output along a circle, and its *size* can peak in the middle (resonance). Corners aren't enough for |H|; AC needs a different method (B-table).
4. **Nonlinear parts** (diodes, BJTs) break the theorem entirely. That's the simulation side (A8).
5. **Power and derating quantities aren't covered.** The theorem is about node voltages and branch currents (and sums or ratios of them). A product like P = V·I can peak inside the box: maximum power transfer is exactly such an interior peak.

## A6. Specs should point at the circuit (the language consequence)

v1's sketch:

```rust
require vin.v * r_bot / (r_top + r_bot) in 3.3V +- 2%;
```

This **types the circuit a second time**, as an equation. If someone later adds a load resistor on `out`, the circuit changes but the equation doesn't. The check keeps passing while being wrong. The alternative:

```rust
require dc(out) in 3.3V +- 2%;
```

The engine gets Vout from the circuit (MNA), so it's always the real circuit, and A5 gives the exact answer. Hand-written equations stay for **design intent the circuit doesn't contain**: power budgets, derived parameters, "total divider current under 100 µA".

That's my main language point for now. The others are in Part E.

## A7. What "±1%" means: two kinds of uncertainty

v1 computed a "statistical" answer as √(sum of squares) of **all** coefficients. That mixes two different things:

| Kind | Examples | The question | How to combine |
|---|---|---|---|
| **Range** | temperature, supply voltage, load, aging, drift | Must work at **every** value | Add worst-case: Σ\|c\| |
| **Statistical** | each part's manufacturing spread | What fraction of boards work? (yield) | Root-sum-square: √Σc² |

You can't "average over temperature": the board has to work at 85 °C, full stop. So:

```
margin = nominal − Σ|range coefficients| − k·√Σ(statistical coefficients)²
```

This is the standard split in analog IC design [Graeb07]. Space-industry worst-case analysis [ECSS11] uses a similar but not identical split, by *known direction* ("biased") vs *random*. The rule "add biased terms linearly, combine random ones by RSS" is RAC worst-case circuit analysis practice; the ECSS text doesn't state it.

**It changes answers.** v1's overshoot example: 8% nominal, spec < 12%, terms C_out 2.5%, ESR 1.5%, L 0.5%. v1's RSS said 11% → pass. But electrolytic ESR is strongly temperature-driven, so the ESR term is a *range* term:

```
8 + 1.5 (range, added) + √(2.5² + 0.5²) (statistical) = 8 + 1.5 + 2.55 = 12.05%  → FAIL
```

**Defaults I'd use** (and always show to the user):

- ±tol is a **hard edge** at 25 °C, new part: parts outside it are rejected at the factory.
- For statistics: spread with σ = tol/3. Beware: PSpice's GAUSS option uses σ = tol, so imported models can silently disagree.
- Temperature, supply and lifetime are **range** knobs. But a part's *response* to them is often uncertain itself: a resistor's ±25 ppm/K tempco has a random sign from part to part. So the effect is (statistical coefficient) × (range variable). The engine handles this by **slicing**: go to the temperature (or lifetime) extreme, and treat the coefficient statistically there. Adding every tempco at full strength overstates precision budgets (2968 vs 1817 ppm in `../research/engine_precision_analog.md`).
- The yield answer depends heavily on the assumed distribution (for the walkthrough's f_L spec: 0.9% failing with σ = tol/3, 11.6% with a uniform spread). So the engine reports yield under both, and says UNDECIDED when they disagree (`../research/engine_method_redteam.md`).

**These real-world terms are usually bigger than the tolerance:**

- A Vishay ±1% thick-film resistor is allowed to drift up to 2% in an 8000 h full-power test at 70 °C [Vishay CRCW]. The datasheet gives no 10-year figure, so how much applies to a product's life and stress is a modeling choice.
- A ceramic capacitor loses capacitance under DC voltage. In the extreme case in [Maxim 5527] (a 6.3 V Y5V 0603 part at 5 V), 4.7 µF measures 0.33 µF. A sensible X7R 0805 at 12 V still keeps only about 1.5 µF. Either way the loss can dwarf a ±10% tolerance.
- Parts from one reel are narrow but offset together, so they're **not independent**. Model that as one shared "lot" knob plus a small "per-part" knob each.
- Resistor networks match to ±0.01% even when each is only ±0.1% absolute. Shared knobs express that directly.

"Really accurate bounds" is at least as much about modeling these as about the math.

## A8. Connecting to the simulator: linearize at the worst point

For things we can't compute exactly (transistors, transient, ringing), we use the simulator. The key tool is the **sensitivity**: how much a metric moves per unit change of each part. The simulator gets all of them for about the cost of **one extra solve**, by reusing the matrix factorization it already has. This is called the adjoint method [DR69], and it's also how SPICE computes noise [RNM71].

Sensitivity × tolerance = the AA coefficient, so a simulation turns into an affine form.

**v1's plan:** simulate at the nominal, draw the straight line there, trust it, then Monte Carlo.
**Problem:** the straight line is drawn at the center, but the question is about the edge.

**v2's loop** (the "worst-case distance" method [AGW94, Graeb07]):

```
1. Simulate the nominal design + sensitivities  →  affine form
2. The affine form predicts the worst point (which parts at which edges) and its value
3. SIMULATE THAT POINT  →  a real value (an inner bound: this really happens)
     - fails the spec?  → a definite FAIL with a concrete counterexample
4. Compare prediction vs simulation:
     - close?  → trust the linearization near the edge
     - far?    → re-linearize at the worst point, go to 2   (usually 2–4 rounds)
```

Step 3 is the big improvement: the line gets checked **where it matters**, and failures become concrete.

**Monte Carlo** then becomes a final confirmation, run near the worst point, with an honest statement of what it proves: "0 failures in 300 runs → failure rate < 1% at 95% confidence." Plain Monte Carlo can't prove "never fails".

**Warning signs that the straight line is lying:**

- Prediction and simulation at the worst point disagree.
- A sensitivity changes sign somewhere in the box.
- A device changes behavior between nominal and worst point (diode turns on, op-amp hits the rail, a different number of ringing peaks).

## A9. Choosing values (`?`) with tolerance included

v1's solver example finds R1/R2 so the **nominal** ratio lands in 3.3 V ±2%. But the chosen resistors have their own tolerance. With ±1% parts the ratio R2/(R1+R2) itself moves by about ±0.68%, so the nominal has to land within **±1.32%**, not ±2%:

```
ratio r ≈ 0.66
ratio moves by  (1 − r) × (1% + 1%) ≈ 0.34 × 2% = 0.68%
nominal budget  2% − 0.68% = 1.32%
```

The real question is "**find** values such that **every** tolerance combination passes". v1 said that needs dReal (§4.4). For linear circuits, A5 makes it easy: "every combination passes" = "every corner passes" = usually just 2 corners once directions are proven. That's a normal constraint the HC4 + splitting solver can handle, with **no dReal** needed for the common case. Search in log R, since E-series are log-spaced.

## A10. What the user sees

One row per spec. The rows below are made-up numbers, only to show the format:

| Spec | Nominal | Bracket (inner … outer) | Verdict | How it was computed | Top contributors | Counterexample |
|---|---|---|---|---|---|---|
| `dc(out) in 3.3V ±2%` | 3.30 V | 3.262 … 3.338 | PASS | exact (corners) | Vin 65%, R1 16% | — |
| `overshoot < 12%` | 8.0% | 11.8 … 12.3 | FAIL | worst-point loop, 3 sims | ESR (temp) 40%, C_out 35% | C_out −20%, ESR @ −40 °C → 12.1% |

The "how it was computed" column replaces v1's "guaranteed / linearized" label, and the counterexample column is exactly what the AI should be explaining.

---

# Part B — The engine: the right method per question

| Question | Method | Result | Guaranteed? |
|---|---|---|---|
| Hand-written equations (few variables) | AA + direction proofs + splitting | affine form | Yes |
| Linear DC from the netlist | Corner theorem + direction proofs; fallback: rank-1 iteration [N&P07] or "p-solution" [Kol14a] | exact range → affine form | Yes, usually exact |
| Linear AC (magnitude, phase) | Complex range methods + splitting [PKK10, Dre06] | affine form | Yes, looser at large tolerances |
| Nonlinear DC, small block | Interval Newton around the operating point [Neu89, Rum90] | affine form | Yes |
| Nonlinear DC, transient, ringing | Worst-point loop (A8) + Monte Carlo | affine form + counterexamples | No; failures are definite, passes are estimates |

Rules for the AA core (from [R&K15], [GGP09]):

- Carry an interval next to each affine form, and use whichever is tighter.
- **Never merge the input knobs** (part tolerances, temperature); only compress the internal error terms.
- Round outward before anything is labeled "guaranteed". (Lesson 1's crate doesn't yet; its `1/x` pads by a few ulps as a stopgap.)

Knobs (noise symbols) carry:

- **Kind:** range / statistical / solver-chosen (`?`).
- **Distribution** (statistical only): default truncated normal with σ = tol/3.
- **Name** by instance path (`adc.r1.tol`, `temp`), so two copies of a block get independent part knobs but share the temperature knob.
- **Structure:** lot knob + per-part knob for parts from one reel or network.

---

# Part C — Decisions, updated

| # | v1 | v2 |
|---|---|---|
| D1 | Build our own engine in Rust | Unchanged |
| D2 | Affine forms are the core representation | Affine forms are the **common format**; structure-aware methods produce them (Part B) |
| D3 | Metrics + sensitivities → affine forms | Unchanged, but linearize **at the worst point** and verify there (A8) |
| D4 | Label guaranteed vs linearized | **Bracket + verdict + method + counterexample** (A1, A10) |
| D5 | One canonical model | Unchanged |
| D6 | Per-block drawn or coded | Unchanged |
| D7 | Own Rust-like language | Unchanged, but see Part E |
| D8 | dReal deferred | Still deferred, and needed less than thought (A9) |
| D9 | Language first | **Engine first**, driven by existing `.spicy` netlists + a small spec file; design the language around what the engine needs |
| D10 | — | **Two kinds of knobs**, range and statistical, combined differently (A7) |
| D11 | — | **Specs point at the circuit** (`dc(out)`), not restated equations (A6) |
| D12 | — | **Linear DC checks are exact via corners** (A5) |

---

# Part D — Simulator work needed

The simulator must feed the engine; today it can't yet.

1. **Parameter layer.** Device values are baked into plain `f64`s when the netlist is loaded (`crates/spicy_simulate/src/devices/resistor.rs:50`). Devices need parameter IDs, so the engine can:
   - re-run at corners without re-parsing (reusing KLU's symbolic analysis; only the numbers change), and
   - ask "how does this stamp change per unit of this parameter" for sensitivities.
2. **Transposed solve.** The adjoint method needs to solve with the transposed matrix. Reference KLU has `klu_tsolve`; our port only has `solve` (`solver/klu/solve.rs:186`).
3. **Simulator accuracy.** Newton's convergence test (`lib.rs:47-48`, reltol 1e-3, abstol 1e-6) compares successive iterates. Newton converges quadratically, so the DC result is actually good to roughly 1e-6–1e-5 relative, as measured by two of the research agents. (An earlier version of this note wrongly said 0.1%.) The real problems:
   - `abs_tol = 1e-6` is applied to currents too. 1 µA is huge for µA-level circuits.
   - Transient uses fixed steps with no error control, and step error ends up inside measured metrics.
   - ppm-level precision specs need reltol ≈ 1e-6.

   So: separate voltage and current tolerances, tighter tolerances in engine mode, and analytic sensitivities from the converged matrix (finite-difference nudges must be sized well above the solver tolerance).
4. **AC on an operating point.** AC currently only stamps R, C, L and sources (`ac.rs:92-104`). Diodes and BJTs aren't linearized at a DC operating point, so AC of any transistor circuit isn't right yet.
5. **Transient step control.** Fixed time step, no error control (`trans.rs:14`). Step-size error ends up inside measured metrics.
6. **Temperature.** `tc1`/`tc2` are parsed but not used, so temperature can't be a knob yet.

---

# Part E — Language implications

Short, since the math comes first. Spade stays the model for the compiler pipeline (see the v1 notes).

1. **Specs point at the circuit** (A6): `require dc(out) in 3.3V ± 2%`, `require ac(out/in, 10kHz).gain_db > 20`.
2. **Declare the knob kind**, because the engine combines them differently (A7):
   ```rust
   env temp: -40°C ..= 85°C;          // range: must hold everywhere
   let r1 = Resistor(10k ± 1%);       // statistical: manufacturing spread
   let r_top: Ohm = ?;                // solver picks it
   matched(r1, r2, ratio: 0.01%);     // shared knob: same network
   ```
3. **Connect by named pins**, not chains. v1's `vin.pos -- r1 -- out` has two problems:
   - In Rust, `a -- b` already means `a - (-b)`. It breaks v1's own rule "if it looks like Rust, it behaves like Rust".
   - A chain hides which pin is which, and that matters for diodes, electrolytics and transistors.
4. **`require … else "message"`**, borrowed from Spade's `where … else "msg"`.

---

# Part F — What we learned from atopile

atopile does interval arithmetic plus symbolic rewriting, with no simulation.

- **Weak:**
  - No backward narrowing (users hand-write inverses).
  - The dependency problem is unsolved (A2): the single-use rewrite is commented out (`src/faebryk/core/solver/symbolic/expression_wise.py:359-392`).
  - No outward rounding; `>` silently becomes `>=`.
  - Greedy part picking with no backtracking.
- **Correction to v1 §4.2:** atopile does *not* rewrite expressions to avoid repeats; the user must.
- **Worth stealing:**
  - "Spec" vs "part" bounds as different relations (⊆ vs ⊇).
  - Three-valued results (holds / fails / undetermined), as in A1.
  - Tracing contradictions back to the source line.
  - Property-based fuzz tests of the solver against direct evaluation.

---

# Part G — Build order and learning path

We build and learn in the same steps. Each lesson is a runnable example in `crates/spicy_bounds`.

| Lesson | Idea | Status |
|---|---|---|
| 1 | Intervals, the dependency problem, affine forms, coefficients, the bracket (A1–A3) | ✅ `lesson01_divider` |
| 2 | Where AA breaks: wide ranges, humps; splitting and log space (A4) | next |
| 3 | A tiny MNA solver + the corner theorem, with a brute-force check (A5) | |
| 4 | Range vs statistical knobs; Monte Carlo on an affine form (A7) | |
| 5 | Simulator: parameter layer, transposed solve, DC sensitivities (Part D) | |
| 6 | The worst-point loop on a real `.spicy` circuit (A8) | |
| 7 | Solving for `?` with tolerances and snapping to E-series (A9) | |

Run a lesson: `cargo run -p spicy_bounds --example lesson01_divider`

---

# Open questions

- Default distribution for statistical knobs (truncated normal σ = tol/3 vs uniform), and how to show it.
- Beginning-of-life vs end-of-life checks: separate specs, or one spec with a "life" range knob?
- How far to take guaranteed AC (complex methods), vs treating AC like simulation.
- How the AI extracts drift / DC-bias / tempco data from datasheets into knobs.
- Switching converters: averaged models vs periodic steady state (unchanged from v1).
- Part catalog source (unchanged from v1).
