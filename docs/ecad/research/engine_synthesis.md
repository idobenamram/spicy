# Research Synthesis: Round 2, the Engine in Depth

> 2026-09-27 · Six research agents took the engine from "the loop we think works" to something specified enough to build.
> The six full reports sit next to this file. This document is the merged result: what holds, what changes, where the reports disagreed, what you need to decide, and the revised MVP plan (roadmap M3).
> Every number below is from those reports. Each report keeps its scripts (see §9), and all of them compute from one shared model of the walkthrough amplifier that reproduces every walkthrough number.

| Report | Focus |
|---|---|
| `engine_flows.md` | Every flow from an edit to a verdict on screen: stages, data, caching, run planning, cost per trigger |
| `agent_flows.md` | How the editor's AI agent handles each user request through the engine; the tool interface it needs |
| `engine_math_worst_case.md` | The worst-point loop for `worst_case`, as pseudocode, run against the 256-corner answer key |
| `engine_math_statistical.md` | `sigma(k)`: the k-σ worst point, distributions, yield, Monte Carlo checks |
| `engine_math_affine_measures.md` | Affine forms, contributors, measures (`f_low`), slopes by nudging and simulator noise |
| `engine_math_guards.md` | Adversarial circuits, the safety net, guards, the verdict table |

---

## 1. The verdict in one paragraph

**The loop holds, and it's cheap.** On the MVP amplifier (8 knobs, 3 specs, 5 spec sides), the specified `worst_case` loop finds **every true extreme exactly** (the same values as brute force over all 256 corners) in **about 70 simulations**. The `sigma(3)` loop reproduces the walkthrough's rounds exactly (6.225 → 6.297, 6.328 → 6.341, 6.343). A full check with both confidences takes **about 200 simulations and 8 ms** on one core, even with today's dense AC. So the MVP's problem isn't speed. It's trust: three of eleven adversarial spec sides got a **confident false PASS** from the loop as `engine.md` §5.1 describes it, and **none** did from the guarded loop, at the same cost. The biggest blockers are in the simulator: besides temperature (M2c) and BJT small-signal AC (M2d), the MVP now also needs engine-grade tolerances (M2e) and four fixes, one of them a real bug in our BJT equations.

---

## 2. The MVP check, end to end

This is the one flow the MVP needs (`spicy check`, no editor). The numbers are the CE amplifier's, at L0 (slopes by nudging).

```
 ce_amp.spl ─► parse ─► elaborate ─► knob table (8 knobs) ─► lower ONCE to Circuit
                                                               + a Binding (knob → Params field)
        │
        ▼
 SHARED ROUND (serves every spec)                                           15 runs
   nominal + 8 nudges      → a line for every measure and every device margin
   4 range corners         → temp × vcc, statistical knobs at nominal
   2 interior temperatures → catches temperature humps
   margin guard            → q1's saturation margin predicted over the box: 2.885 V ≫ its error, 0 runs
        │
        ▼
 PER SPEC SIDE, as independent state machines, merged into rounds by one driver
   worst_case: jump to the vertex the slopes point to → flip knobs → accept only improvements
               → stop when no flip improves                                ≈ 48 runs, 5 sides
   sigma(3):   at the worst range corner, the 3σ point in u-space           8–22 runs per side
               (skipped for the verdict when worst_case already passes)
        │
        ▼
 cold re-run of each decisive point at reltol/10 → numerical band ε_num      5 runs
        │
        ▼
 verdict table (§4.4) ─► terminal table + JSON, each with its cost ("68 runs, L0; brute force 256")
```

**What it prints for the MVP** (checked against the answer key):

| Spec side | `worst_case` | Verdict | `sigma(3)` (default map) | Verdict |
|---|---|---|---|---|
| bias: VC ≤ 6.5 V | 6.5905 | **FAIL**, with the corner as counterexample | 6.313 | PASS |
| bias: VC ≥ 4.5 V | 4.6597 | PASS (estimated) | 4.853 | PASS |
| gain: ≤ 4.83 | 4.7030 | PASS (estimated) | 4.664 | PASS |
| gain: ≥ 4.37 | 4.4626 | PASS (estimated) | 4.519 | PASS |
| bass: f_L ≤ 30 Hz | 26.738 | PASS (estimated) | 24.92 | PASS |

The specs' default confidence is `sigma(3)`, so the headline is three PASSes, with bias shown as "fails only with every part at its edge": walkthrough §6's "engineer's call", now with exact numbers. The MVP has no aging knob, so no spec FAILs at `sigma(3)`. For a failing test, `bass: f_low <= 24Hz` on day 1 is exactly equivalent to the walkthrough's aged 30 Hz case (same worst-case distance, 2.385; Monte Carlo 0.84% failing).

---

## 3. What changes in the engine design

Ranked by impact.

### C1. Stop on optimality, and accept only improvements

`engine.md` §5.1 stops when "prediction ≈ simulation". Two reports independently found that this proves nothing: at convergence the last prediction is the line's value at its own anchor point, so the two agree by construction (guards §1.0, worst-case §2.7). And re-linearizing wherever the line points, even at a worse point, **cycled on 2 of 6 test cases** (worst-case §2.4).

New rule:
- The incumbent (best simulated point) only moves to a point that **simulates better**.
- The loop stops when no knob wants to move (the box's first-order optimality condition) or, with flips (C2), when no single flip improves.
- The prediction error is still recorded, but only to widen the outer bound.

This terminates by construction, so no cycle detection is needed.

### C2. At a vertex, flip knobs instead of nudging them

At a corner, the guarded loop re-linearizes by moving each knob **to its other edge** (a secant over the whole range) instead of nudging it (a tangent at the corner).
- It costs the same N runs.
- It **tests every one-flip neighbour for real**, so a corner's claim to be the worst comes with a certificate.
- It fixed three failure shapes: the slope that flips sign across the box (walkthrough §5.7), a band edge driven by a knob with a below-noise slope, and a region change (guards §1.1).

The worst-case report reached the same place from the other side. Its optional flip certificate confirmed all five MVP answers, and it lists "tangent nudges or full flips at vertices?" as an open question. The cost of flips: they don't give the tangent slopes that contributors and `sigma(k)` want. **Resolution (decision D1):** nudges at nominal, where the shared line is built; flips at vertices during the `worst_case` search.

### C3. A saturation-margin guard, because range corners aren't enough

A variant that saturates only at some corners fooled both the loop (it reported gain **6.46 where the truth is 0**) and the range-corner safety net: β drives the limiter, and the net only moves the range knobs.
- **The fix:** every run logs each device's operating region and margins. Each margin (for q1: VCE − VCE,sat) gets a line from the shared round, like a measure. If the line predicts the margin can get close to zero anywhere in the box, that point is simulated.
- **The cost:** 0 runs on the MVP (the margin stays ≥ 2.885 V, far above its 0.013 V error), and 1 run on the variant, which then FAILs correctly.

It needs per-device operating-point records from the simulator (§5).

### C4. Every run updates every spec, and every side is cross-checked at the end

- One cache per check. Every run returns every measure, and each side's inner bound is the best over all runs of the check, not only its own.
- If any run beats a side's answer, that side restarts from there.

This is free, and it turned one false PASS into a FAIL (worst-case §4.1).

### C5. Knobs whose slope is below noise go to an edge

A slope at the noise floor carries no sign information. Leaving the knob at nominal hid a band-edge failure (guards (e)).
- Such knobs go to an edge and are flipped like any other.
- The "flat" threshold is set from the measured noise: τ = 3σ_f/h.
- **Structural zeros** (a capacitor can't affect a DC measure) come from the circuit, never from a bit-identical result. A limiter that is off at nominal also gives an exactly-zero slope.

### C6. The verdict rule: margin beyond the observed error, and the guards carry the weight

- **No margin multiplier separates false PASSes from true ones.** The false PASSes had margin/error ratios of 1.85, 3.0 and 2,521; the MVP's true PASSes go down to 1.71.
- So PASS (estimated) needs **margin > largest observed line error + numerical band**, and the guards do the protecting.
- A worst point in a different operating region than nominal gives UNDECIDED unless it FAILs.

The full table is in §4.4.

### C7. `sigma(k)`: the exact distribution map, and the verdict from the k-σ point

- **Use the exact per-knob map.** The docs' default (normal, σ = tol/3, truncated at the tolerance) needs the exact per-knob map from u-space to the part value. The walkthrough used the untruncated map without saying so. With the exact map, S1 goes 6.343 → **6.313 V** and aged S3 goes 31.7 → **31.15 Hz**, matching a 200,000-board Monte Carlo (6.308 V, 31.13 Hz). Same verdicts; the old numbers were conservative and would give false FAILs near a bound.
- **Decide from the k-σ point, not the worst-case distance β_w.** The k-σ point costs 8–22 simulations per side whatever the margin. β_w costs 23–159, and on 5 of 6 sides its design point was a part outside its tolerance.
- **Nesting:** with truncation, every σ point lies inside the box, so **a `worst_case` PASS implies a `sigma(k)` PASS**. That settles 4 of the 5 MVP sides for free.
- **The uniform check** (engine.md §2.4) runs only when `worst_case` fails and the default passes; in the MVP only VC max.
- **Per side, not per spec.** "3.09σ = 99.9%" holds per side; a two-sided spec at 99.9% overall needs 3.29σ per side.

### C8. The affine form keeps three kinds of error apart

The `± err` in `center + Σ aᵢεᵢ ± err` meant three different things:
1. a **proven** remainder, from formula operations;
2. an **observed** miss, from a simulator line (the VC line misses by 0.182 V at the worst corner); this is not a bound;
3. the **numerical band** of simulator and measurement.

The type must keep them apart (a `Remainder` enum plus a separate `band`). Only exact or proven remainders may ever support PASS (guaranteed). A consequence for the walkthrough: its affine headroom range (5.26…7.74 V) is **not an enclosure**, because the true minimum is 5.165 V.

### C9. Measures are defined exactly, and never pass by accident

- **`f_low(-3dB)`** is the crossing nearest below the peak of |H| over the band, at max|H| − 3 dB (literally −3 dB).
  - The nominal is 20.127 Hz, not the textbook 1/(2πRC) = 20.079 Hz.
  - ngspice, Xyce and Gnucap all interpolate crossings linearly in linear frequency: up to 1.3% off, and nudged slopes come out ±20% wrong.
  - Bracketing plus a secant step in ln f is exact to 1e-15 for about 6 extra AC solves.
- **`at(1kHz)` requests exactly 1 kHz.** Our AC sweep accumulates `f *= r` and lands on 1000.000000000002. |H(1 kHz)| is 4.5908, not the mid-band 4.5917.
- **A measure returns Value, Beyond, or Undefined.**
  - A crossing shown to lie below the searched band is a one-sided bound (Beyond), not a missing crossing.
  - Undefined gives UNDECIDED, never a pass. Gnucap silently returns a huge number when there's no crossing.
- **The answer key must use the engine's own measurement functions.**

### C10. Slopes by nudging need engine-grade tolerances

A replica of our Newton loop matches the binary.
- **Today's tolerances make slopes wrong.** Both return VC 19 µV off, and forward slopes come out **25–26% wrong** in two ordinary situations: a nudge that changes the iteration count, and a cold nominal with warm nudges.
- **Engine settings fix it.** reltol 1e-6, vntol 1e-9 V and abstol 1e-12 A bring the error to 1e-13 V, for about +2 iterations cold and nothing warm.
- **M2e becomes an MVP prerequisite,** with the acceptance test from the affine report §7.7.
- **Nudging:** forward differences, stepping inward at an edge. One nudged run serves every measure.

### C11. "Instant re-evaluation on every edit" needs a rework (editor era)

`engine.md` §8 and walkthrough §2 promise re-evaluating the stored affine forms on every edit.
- **For edits outside the tolerance box, the stored line is nonsense.** For C_in 1 → 2.2 µF it predicts f_L = **−4.0 Hz** (simulated: 9.127 Hz). A line in log units predicts 9.128 Hz.
- **Log units still miss interactions.** A divider change predicted a borderline 6.51 V where the truth is a clear FAIL at 6.70 V.
- **What works:** re-simulating the stored worst points, at one run each, matched the loop in every worst-case row of the fix-bass test. It is always a real point, so it proves FAILs instantly.
- **The instant tier becomes** "log-unit prediction, labelled *predicted*, plus re-simulated inner bounds". Not in the MVP, but the MVP should store each form's raw derivatives, linearization point and knob scales so nothing is lost.

### C12. Runs are cached by content, and loops run in rounds

**Cache key.** Key each run by what the simulator reads, per analysis: an operating point never reads a capacitor.
- After `c_in` → 2.2 µF, bias replays all 92 of its runs from cache with bit-identical verdicts, with no dependency tracking.
- A design-hash key (`language_specs.md` §11.3) would re-run all 92.

**Latency.** It is set by rounds, not runs. Sending each predicted worst point together with its nudges costs no extra runs and cuts 11 rounds to 7.

**No warm start from last revision's worst points.**
- It saved at most 7% of the runs, and cost 14% more when only some specs changed.
- It made `sigma(3)` values depend on edit history.
- Old worst points only decide which runs go first. This refutes red-team §3.2 item 7.

---

## 4. Decisions for you

Each has a recommendation. The reports' evidence is in the section named.

| # | Decision | Options | Recommendation |
|---|---|---|---|
| **D1** | How the `worst_case` loop re-linearizes at a corner (C2) | tangent nudges · flips to the other edge · both | **Both:** nudges at nominal (the shared line: contributors, σ, margins), flips at vertices during the search |
| **D2** | Stop rule (C1) | "prediction ≈ simulation" (engine.md) · optimality + accept only improvements | **Optimality + accept only improvements** |
| **D3** | Distribution map for statistical knobs (C7) | identity (walkthrough's implicit choice) · exact map of the truncated normal | **Exact map.** The walkthrough's 6.34 V and 31.7 Hz become 6.31 V and 31.2 Hz |
| **D4** | What `sigma(3)` means on a two-sided spec | 3σ per side · 99.865% for the whole spec (3.2σ per side) | **Per side** for the MVP (what the docs imply); print it that way. Revisit with yield specs (M7) |
| **D5** | `f_low` definition (C9) | literal −3 dB below max\|H\| · half power (−3.01 dB) · relative to a named reference frequency | **Literal −3 dB below max\|H\| over the band.** The editor can hint the difference |
| **D6** | Newton start in the MVP | cold every run · warm nudges | **Cold** for the MVP: every run is a pure function of its inputs, and it costs about 1 ms per check. Warm once M2e's acceptance test passes |
| **D7** | Compute both confidences every time? | only the spec's own · both | **Both** (+37% runs on the CE amp). The spec's attribute picks the headline, and a confidence edit becomes free |
| **D8** | The MVP's β range | keep `beta: 100..=300` · use the 2N3904 datasheet | **Keep 100..=300** for the MVP (it's the walkthrough's generic `Npn`). Note: the 2N3904 guarantees only hFE ≥ 70 at 1 mA (100–300 is at 10 mA), and pinning it flips bias at 3σ from PASS (6.34 V) to FAIL (6.59 V). That's M5's job |
| **D9** | Build the engine against a function-backed test backend first? | wait for M2c/M2d/M2e · build in parallel on a test backend | **Parallel.** A `Backend` that evaluates the CE-amp formulas (plus the adversarial variants) lets M3b–M3d be built and tested now, and swapped to our simulator when M2c/M2d/M2e land |
| **D10** | Machine-readable output in the MVP | table only · table + `--format json` | **Both**, with a versioned schema: status, a hash of the elaborated model, margin, pasteable counterexample, method and runs, tags. The AI agent (and tests) read the JSON |

### 4.4 The verdict table (from `engine_math_guards.md` §3.2)

For an upper bound B (a lower bound mirrors it). I is the inner bound (the worst simulated value), e_obs the largest observed line error, ε_num the numerical band. The first matching row wins.

| # | Condition | Verdict |
|---|---|---|
| 1 | I > B + ε_num | **FAIL**, with the counterexample (and its operating region if it isn't nominal's) |
| 2 | \|I − B\| ≤ ε_num | UNDECIDED (numerics) |
| 3 | a needed run failed or was implausible | UNDECIDED (simulator) |
| 4 | the worst point is in a different operating region than nominal | UNDECIDED (regime) |
| 5 | the search hit its budget or a cycle, without a one-flip certificate | UNDECIDED (search) |
| 6 | B − I > e_obs + ε_num | **PASS (estimated)**, bracket [I, I + e_obs] |
| 7 | otherwise | UNDECIDED (bracket) |

PASS (guaranteed) never comes from the loop, only from exact methods (datasheet arithmetic, the corner theorem). A guard never blocks a FAIL.

---

## 5. Simulator work the MVP now needs

In order. The first four are fixes, each done test-first.

| # | Work | Why | Found by |
|---|---|---|---|
| 1 | **BJT Ebers–Moll reciprocity.** `bjt.rs:133-135` uses `IS` for both injection currents; reciprocity needs αF·I_ES = αR·I_CS = IS | VCE,sat comes out 47.8 mV where ngspice's equations give 65.6 mV; forward IC is low by αF (0.5% at β = 200). **A deliberate result change:** snapshots move | guards (verified: the equations at lines 133-135) |
| 2 | **Non-convergence must be an error.** A circuit with no DC solution returns −7.3·10²⁶ V with exit code 0 | A silent wrong value is the worst outcome for an engine that trusts runs | guards (verified: `nosol.spicy` reproduces it) |
| 3 | **`simulate_dc` and `simulate_ac` return `Result`** instead of panicking | A panic in one run kills the whole check; the loop needs "run failed" as data (verdict row 3) | guards |
| 4 | **AC frequency grid computed exactly** (`f_start·10^(k/N)`), not accumulated | `at(1kHz)` must mean 1 kHz | affine |
| 5 | **Per-device operating-point records:** region, VCE − VCE,sat and cutoff margins, Newton status | Every guard reads them (C3); M2d needs the same conductances | guards |
| 6 | **M2e engine mode:** reltol 1e-6, vntol 1e-9 V, abstol 1e-12 A, a residual check, the last update reported as the band, f64 through the API; plus its acceptance test | Slopes are 25–26% wrong today (C10) | affine |
| 7 | **M2c temperature** | The temperature knob must act | roadmap |
| 8 | **M2d AC at the operating point, sparse** | Gain and bass can't be simulated today; dense AC is 86% of the check's time | roadmap, flows |

Not needed for the MVP: `pipeline.md` §9 steps 2 and 4. They'd save 1.3 ms of 8 on the CE amp, and start to pay around 50 knobs or at sign-off.

---

## 6. The revised MVP plan (roadmap M3)

With D9, the engine and the simulator work proceed in parallel and meet at the end.

```
 simulator track (§5)                      engine track
 ─────────────────────                     ─────────────────────────────────────────────
 fixes 1–4 (test-first)                    M3a design note: types and the Backend trait
 operating-point records (5)               M3b answer key on the test backend
 M2e engine mode (6)                       M3c worst_case loop + guards on the test backend
 M2c temperature (7)                       M3d sigma(3) on the test backend
 M2d sparse AC at the op (8)               M3e output: table + JSON
            └──────────────────┬──────────────────┘
                               ▼
              M3f: swap in our simulator; answer key recomputed on it;
                   spicy check circuits/ce_amp.spl agrees with it
```

- **M3a: Design note.** Covers:
  - `KnobCoord` (ε ∈ [−1, 1], x = mid + rad·ε, a `Scale` field, Linear in the MVP);
  - `Affine` with sparse sorted terms, a `Remainder` enum and a separate `band`;
  - `MeasureValue` (Value / Beyond / Undefined, each with a band);
  - `RunRequest` / `RunResult`, with per-device operating-point data;
  - a batch `Backend::run(&Prepared, &[RunRequest]) -> Vec<Result<RunResult, SimError>>`;
  - loops as state machines that yield batches of knob points, and the driver that merges them into rounds;
  - the per-check run cache;
  - lowering once plus a `Binding` per run (`circuit.md` §5), not a fresh `Circuit` per run;
  - the verdict table.

  Creates `spicy_engine`. 🔍
- **M3b: Answer key.** Covers:
  - 256 corners, 1,000 seeded interior points, and refinement for power-type measures;
  - it runs through `Backend::run` and the engine's own measurement functions;
  - first on the test backend (the CE-amp formulas plus the adversarial variants: saturation, interior power maximum, second band edge, clipping THD). 🔍
- **M3c: `worst_case`.** Covers:
  - the shared round (15 runs), the monotone vertex walk with flips at vertices (D1, D2) and the margin guard;
  - the inner bound pooled over all runs, and the final cross-check;
  - the cold re-run for ε_num, and the verdict table.

  **Tests:** pin each side's trajectory and run count, inner bound = the answer key exactly, and the outer bound on the right side of the truth; the adversarial variants as regression cases. 🔍
- **M3d: `sigma(3)`.** Covers:
  - the exact map (D3), the k-σ point with an ascent check and a 10-round cap;
  - range corners ranked by the safety net, the σ point cross-evaluated at the other corners (3 runs), the nesting shortcut;
  - the uniform check only when `worst_case` fails.

  **Tests:** seeded Monte Carlo agreement, and `f_low <= 24Hz` as the failing case (0.84%). 🔍
- **M3e: Output.** Covers:
  - the terminal table (spec, verdict, worst value, top contributors, counterexample, cost) and `--format json` (D10);
  - `--at <corner>` to re-simulate a counterexample. 🔍
- **M3f: Our simulator.** Once §5 is done, the same tests run through the `spicy` backend, with the answer key recomputed on our simulator (its numbers differ from the model's: Ebers–Moll, and its own temperature equations). **Done when:** `spicy check circuits/ce_amp.spl` agrees with that answer key, and the tests pin it. 🔍

---

## 7. Where the reports disagreed, and how this resolves it

| Question | Reports | Resolution |
|---|---|---|
| Re-linearize at a vertex | worst-case: tangent nudges · guards: flips | D1: nudges at nominal, flips at vertices |
| Nudge size | worst-case: h = 0.01 of the half-range (robust to 1e-5 relative noise) · affine: 1e-3 in ε (at engine tolerances) | 1e-3 once M2e lands; M2e's acceptance test checks 1e-3 against 1e-4. Until then 0.01 |
| Newton start | affine: warm nudges (7× cheaper) · flows: cold every run (pure runs) | D6: cold in the MVP, warm after M2e's test |
| Warm-start the loop from the previous edit's worst points | worst-case: safe with the monotone walk · flows: doesn't pay, history-dependent | Only to order runs, never as the start |
| Runs for a full check (both confidences) | flows: 194 · agent: 231 | Different loop variants; same order. M3 tests pin the implemented count |
| Runs for `worst_case` | worst-case: 71 · flows: 69 · guards: 68 (with guards and the re-runs) | Consistent: about 70 against 256 |
| Knob coordinates | affine, statistical: linear ε · flows, agent: log units for predictions | Linear for the loop in the MVP; log-unit forms for editor-era predictions; try log–log in M3c against the answer key |

---

## 8. What changes in the current docs (to apply after your decisions)

| Doc | Change |
|---|---|
| `engine.md` §5.1 | Stop rule (C1), flips at vertices (C2), the shared round with margin guard (C3), pooled inner bound (C4) |
| `engine.md` §5.2 | The slope-sign guard is covered by flips. The "5× too optimistic" high-Q example is a fixed-frequency resonance effect; the multiple-extremum example should be the Chebyshev one (0.678 vs 1.113 dB) |
| `engine.md` §3.6, §4.4 | σ per side (3.29σ per side for a two-sided 99.9%); on the CE amp the nominal-parts worst corner *was* the worst at 3σ |
| `engine.md` §6.3 | ngspice's AC `.sens` holds the operating point fixed, so it misses VCC's, temperature's and β's effect on gain: AC specs stay at L0 on ngspice. Xyce's includes the shift |
| `engine.md` §8, walkthrough §2 | The instant tier becomes "predicted (log units) + re-simulated inner bounds" (C11) |
| walkthrough §5.6, §6, §9 | 6.34 V → 6.31 V and 31.7 Hz → 31.2 Hz under the stated default (same verdicts) |
| walkthrough §8 | The affine headroom range isn't an enclosure (true minimum 5.165 V < 5.26 V) |
| `language.md` §8.4 | A crossing below the searched band is a one-sided bound (Beyond), not "missing" |
| `language.md` §10 | Each value in a verdict delta carries its status (predicted / verified) |
| roadmap §2.5 | Lower once plus a `Binding` per run, not a fresh `Circuit` per run |
| roadmap M2, M3 | §5's simulator work (M2e before M3c, the four fixes, operating-point records); M3 replaced by §6 |
| `pipeline.md` §11 | New rows: BJT reciprocity, silent non-convergence, panics in DC sweep and AC, accumulated AC frequencies |

---

## 9. Checked by hand, and what isn't durable yet

**Spot checks** before this synthesis:
- `bjt.rs:133-135` does use `IS` for both injection currents (i_c0 = αF·i_F − i_R with i_F, i_R both scaled by `IS`).
- `nosol.spicy` returns V(b) = −7.27·10²⁶ V with exit code 0.
- Our current-source stamp matches ngspice's (`isrcload.c:460-461`).

**Scripts.** Every report's numbers come from stdlib-Python scripts in this job's temporary directory (`/root/.claude/jobs/443154a8/tmp/engine_research/`). The reports cite those paths. That directory is deleted with the job; move the scripts into the repo if they should stay reproducible.

**Unverified citations**, marked in each report:
- Graeb 2007 chapter 5 (the 2024 TUM textbook was read instead);
- the full Antreich–Graeb–Wieser paper (only its abstract);
- Der Kiureghian & Dakessian 1998 (only papers citing it);
- the iHL-RF constants;
- Cadence's "under 100 simulations per spec".

Cadence and MunEDA publish no worst-case-corner algorithm.

---

## 10. Open questions (merged)

1. **Is the observed line error a good enough allowance for PASS (estimated)?** It was conservative on every smooth case and useless on a knee. Calibrate on more circuits (op-amp stages, an LDO) before leaning on it.
2. **Joint flips.** The buck needed three knobs flipped together. No MVP or adversarial case did. Test pairs of the top four knobs (6 runs) only when a regression case needs it?
3. **Scaling flips.** At 50 knobs a round of flips costs 50 runs, and below-noise knobs can't be screened out, because that's exactly where zero-slope limiters live. Where's the crossover with the margin guard and range corners?
4. **`sigma(k)` guards.** Flips don't apply to statistical knobs; is the margin guard over the kσ ball plus pooling enough?
5. **Truncation for "guaranteed by design" limits.** Tested limits screen parts, so truncation is real; untested ones aren't screened. Untruncated for those?
6. **The β distribution.** Normal around 200 or log-normal around 173? It flips no MVP verdict, but moves yield 28%. Should part records carry a distribution field?
7. **The joint range × statistical check past 4 range knobs.** 2^R − 1 corners per σ point costs 31 runs per side at R = 5. Solido-style joint search, or only corners the form can't rule out?
8. **Warm starts and bit-exact reproducibility** once warm starts come back: compare engine tests with a tolerance, as `spicy_simulate`'s numeric snapshots already do?
