# AI-Native ECAD Editor — Engine Design v3

> 2026-09-25 · Supersedes `archive/engine_v2.md`.
> Companion docs:
> - `walkthrough.md`: one amplifier through the whole loop, with real numbers. **Start here if you're new.**
> - `language.md`: how circuits, parts and contracts are written.
> - `specs.md`: spec concepts.
> - `research/synthesis.md` and the reports next to it: the stress tests behind this version.
> - `bibliography.md`: references (keys like **[AGW94]**).

---

## 0. What changed from v2

1. **A safety net around the worst-point loop.** v2's loop could report a confident PASS when a limiter (clipping, current limit, dropout) is off at nominal. v3 always also simulates the range-knob corners, and every device reports its operating region (§5).
2. **A rich knob model.** Kinds, provenance, shape, distribution, lot structure, constraints, discrete modes. Parameters that depend on the circuit's state live in device models, not in knobs (§2).
3. **Methods per question,** including a **datasheet-arithmetic engine** that needs no simulation and covers most digital specs exactly (§4).
4. **Honest verdicts.** More verdict types, tags for what a verdict rests on, yield that doesn't hide its distribution assumption, and board yield instead of per-spec yield (§3).
5. **Simulator-agnostic.** A backend interface with capability levels, so established simulators (ngspice, Xyce, LTspice, …) plug in today and ours grows into the same slot (§6).
6. **Corrections** from checking v2 against its sources (`research/synthesis.md` §2).

---

## 1. Goal and principles

**Goal:** for every spec, a verdict the engineer can trust. It comes with its range, what uses up the margin, and a concrete counterexample when it fails, and it's updated continuously while editing.

| # | Principle |
|---|---|
| P1 | **The simulator is the source of truth for anything nonlinear.** The engine asks it only for values, and slopes when available, so any simulator and any vendor model can be used |
| P2 | **Inner bounds prove failures; outer bounds prove passes.** Every result says which it has |
| P3 | **A cheap approximation is never the last word.** Every predicted worst point is simulated, and range corners are always checked |
| P4 | **Every number has a provenance, and every verdict says what it rests on:** which model, which distribution, which layout assumption, which unreviewed datasheet value |
| P5 | **Cost grows with the number of specs,** not with 2^(number of knobs) |
| P6 | **One common format:** affine forms in named knobs |

---

## 2. The knob model

A **knob** is one independent thing that can vary. It's named by instance path (`amp.r1.value`, `temp`) and has a range. The language creates knobs (language §5.4); part records supply their data (language §6.5).

### 2.1 Kinds

| Kind | Examples | How the engine treats it |
|---|---|---|
| **Range** | temperature, supply, lifetime, input amplitude/frequency, load | Must hold at **every** value, so always take the worst |
| **Statistical** | part tolerances, IC parameter spreads | Worst case, or yield, depending on confidence (§3.6) |
| **Mode** (discrete) | power state; PWM / PFM / current limit; boot/reset state; bus driver; firmware setting | Enumerated; each simulation reports which mode it ended in; specs can be scoped to a mode |
| **Solver** (`?`) | values left open | Chosen so specs pass (§4.8) |
| **Calibration / trim** | offsets and gains removed by a calibration step | Subtracted exactly (§4.5) |
| **Layout-owned assumption** | bus capacitance, trace length, board thermal resistance θJA | A range the schematic can't know; the verdict becomes "PASS given …", and the assumption is exported as a layout constraint |
| **Firmware-owned assumption** | watchdog kick period, boot time, peripheral timing | Same, owned by firmware |

### 2.2 Provenance

Every knob's range says where it came from:
- **Tested limit.** A production-tested min/max.
- **Guaranteed by design.** Not tested, but specified.
- **Typical only.** The datasheet gives no limit. There's no defensible distribution, so verdicts that depend on it are tagged.
- **Assumed.** Entered by the engineer.
- **Missing.** No data. A spec that needs it is UNSPECIFIED.

On top of that come the source reference (datasheet page and table) and whether the record was reviewed by a human.

### 2.3 Shape

How the knob acts on the circuit:

| Shape | Example |
|---|---|
| **Slope** | A resistor tempco of ±25 ppm/K: a statistical coefficient × the temperature range knob |
| **Envelope** | An op-amp bias current of 300 pA at 25 °C, but 10 nA over −40…125 °C |
| **Box** | Reference tempco "box" specs, about 2× wider than a linear-tempco reading allows |
| **Aging law** | Linear, √t, ∛t: these differ by 20× over 10 years, so the law must be explicit |
| **Hysteresis** | A reference's thermal hysteresis |
| **Table** | Logic thresholds by supply band × temperature column, **with gaps** (a 74LVC1G04 guarantees nothing between 1.95 and 2.3 V) |

**The slicing rule.** For statistical × range products (tempco × temperature), go to the range extreme and treat the coefficient statistically there. Multiplying the two noise symbols inside affine arithmetic would destroy tempco-tracking cancellation, and adding every tempco at full strength overstates budgets (2968 vs 1817 ppm in the precision study).

### 2.4 Distribution

- **Default** for a tested limit: truncated normal with σ = tol/3. Datasheets often test much wider than the real spread: OPA189 Vos max is 7.5× typical.
- **Always also reported under uniform,** the worst case among symmetric unimodal distributions [Birnbaum 1948; Barmish & Lagoa 1997 — to add to the bibliography]. If the two give different verdicts, the verdict is **UNDECIDED**.
- **Typical-only values have no distribution.** A yield verdict that depends on one is tagged, never silently computed.

Why this matters: the walkthrough's f_L spec fails 0.9% under the default, 11.6% uniform, 5% with an offset reel, and 32% with culled parts.

### 2.5 Structure

- **Lot + per-part.** Parts from one reel or network share a lot knob and have small per-part knobs. Resistor networks match to ±0.01% even at ±0.1% absolute. A pinned network part supplies both numbers.
- **Constraints between limits.** Datasheet min/max limits aren't always independent:
  - a 74HC14's threshold box alone would allow −0.3 V hysteresis;
  - treating the NE555's two thresholds as independent predicts a period anywhere from 0.11× to 2.17× nominal.

  Part records can state constraints (threshold ordering, minimum hysteresis, a ratio spec).

### 2.6 What is *not* a knob

**Parameters that depend on the circuit's own state** belong in the device model, evaluated at the operating point:
- MLCC capacitance vs DC voltage;
- inductance vs current (saturation);
- RDS(on) vs junction temperature.

Treating them as range knobs (as v2 did for DC bias) is the wrong kind of object.

**Self-heating** needs an **electrothermal fixed point**: dissipated power → junction temperature → device parameters → power. The buck reaches 153–196 °C junction at 85 °C ambient, depending on θJA. No fixed point (thermal runaway) is a FAIL.

---

## 3. Results

### 3.1 The common format: affine forms in named knobs

Every result is stored as `center + Σ coeffᵢ·εᵢ ± err`, where each εᵢ is a named knob. That's what makes these possible:

- **Combining without double-counting.** Headroom `vcc.v − dc(output.v)`: interval arithmetic gives 4.81 V, the truth is 5.17 V (walkthrough §8).
- **Calibration as an exact subtraction** (§4.5).
- **Hierarchy.** A block's characterized outputs are reused upstream, and shared knobs stay correlated.
- **Error budgets.** The familiar spreadsheet (one row per contributor) is generated from the coefficients.
- **Board yield** from the stored per-spec forms, at zero extra simulations (§4.4).

### 3.2 Brackets

| Bound | What it is | What it proves |
|---|---|---|
| **Inner** | Values really reached, at simulated or evaluated points | Failures |
| **Outer** | Guaranteed (formulas, corner theorem) or estimated (the loop) | Passes |

Also in every bracket: the **numerical error band** of the simulator and measurement.

### 3.3 Verdicts

| Verdict | Meaning |
|---|---|
| **PASS (guaranteed)** | A proven outer bound is inside the spec |
| **PASS (estimated)** | Estimated outer bound inside; worst points simulated; safety net run |
| **PASS (conditional)** | Passes *given* a stated layout or firmware assumption ("given bus capacitance ≤ 234 pF") |
| **FAIL** | A reachable point violates the spec. Always definite, with its counterexample |
| **UNDECIDED** | The spec edge is inside the bracket, or verdicts differ across distribution assumptions |
| **UNVERIFIABLE (model)** | No available model can speak to it (a settling tail at 18 bits, −115 dB THD, a typical-only macromodel) |
| **UNSPECIFIED** | The datasheet has no data where the design operates |

**For yield specs**, one counterexample doesn't fail the spec, because a yield spec allows some failures. The verdict comes from a statistical bound with its confidence (§4.4).

### 3.4 Tags

A verdict can carry any of these:
- **model-conditional:** a vendor model doesn't respond to some knobs (§6.6);
- **relies-on-typical;**
- **relies-on-unreviewed-part-data;**
- **distribution-sensitive;**
- **mode-dependent.**

### 3.5 Contributors and counterexamples

- **Contributors:** which knobs use up the margin, split into range and statistical, as shares. For a fit, also report how well the line explained the data (R²). This is what Cadence shows.
- **Counterexample:** the exact knob settings as a pasteable `corner`, one click from a simulation at that point.

### 3.6 Confidence classes

| Class | Meaning |
|---|---|
| `worst_case` | Every knob, statistical ones included, at its worst edge. This is ECSS's default extreme-value analysis |
| `sigma(k)` | Range knobs worst; statistical knobs at k σ worst-case distance. One-sided 3σ = 99.865%; 3.09σ = 99.9% |
| `yield(p)` | Range knobs worst; at least a fraction p of boards pass, proven with a confidence bound |
| `nominal` | Quick checks only |

**Defaults:** `sigma(3)` for user specs, `worst_case` for automatic checks. Both worst-case and realistic values are always shown when they disagree.

---

## 4. Methods: the right tool for each question

| The quantity depends on… | Method | Best verdict |
|---|---|---|
| Datasheet numbers and formulas only | §4.1 datasheet arithmetic / affine arithmetic | guaranteed, usually exact |
| A linear piece of the circuit, with the theorem's conditions met | §4.2 corner theorem | guaranteed, exact |
| Anything through a nonlinear device, modes, transient | §5 worst-point loop + safety net | estimated (FAIL always definite) |
| Yield | §4.4 statistics | statistical bound |

### 4.1 Datasheet arithmetic and hand formulas

About **two-thirds of board-level digital specs** are arithmetic on datasheet tables, with no simulation needed:
- logic-level margins;
- I2C pull-up and rise-time windows (UM10204);
- SPI/I2C timing budgets (e.g. the maximum SPI clock from MCU setup/hold plus flash timing);
- supervisor and watchdog windows;
- crystal gm_crit;
- ADC source-impedance limits;
- pin currents;
- charge-current programming;
- UVLO dividers.

These formulas are monotone in every knob, so evaluating corners is **exact in about 2 evaluations**. Where monotonicity can't be proven, affine arithmetic with splitting gives a guaranteed bracket.

This runs on **every edit.** Each supply rail is one shared named knob, so correlations are kept. Plain intervals lose that correlation: PolymorphicBlocks reports a false −1.35 V failure where the true margin is +0.036 V.

### 4.2 The corner theorem, with a strict applicability checker

For a linear circuit, extremes are at corners [N&P07 Thm 5.1, PPC65]. The engine applies this **only** when a checker confirms all of these:

1. Each knob enters the circuit matrix as **one rank-1 term**: R (via g = 1/R), a conductance, a source value, or a controlled-source gain. Not a turns ratio, pot wiper, or shared knob.
2. The knobs are independent (no lot knob spanning several elements).
3. The circuit stays solvable over the whole box (the determinant never changes sign; checkable, because the determinant is multilinear too).
4. The measured quantity is a node voltage, a branch current, or a sum or ratio of them. **Not** power, loop gain, or anything involving a state-dependent element.

Directions are proven over the box when possible, and corners enumerated otherwise. Proofs "typically fail in high dimensions" [N&P07], so large linear networks fall back to the loop.

### 4.3 The worst-point loop

Section 5.

### 4.4 Statistics

- **Board yield, not per-spec yield at one corner.** A board must pass every spec at every range condition. One test: 26.7% loss reported per spec vs 36.8% true.
  - First estimate: from the stored per-spec affine forms, at zero extra simulations (Schenkel et al., DAC 2001).
  - Confirmation: sampling in which each sample is checked across range corners and all specs.
- **Importance sampling** centred on the loop's worst point(s). 2,000 runs matched the precision of 20,000 plain Monte Carlo runs in the red-team test.
- **Clopper–Pearson stopping** with three-valued yield verdicts (pass / fail / keep sampling), as Cadence does.
- **Joint search over range and statistical knobs** (Solido PVTMC style). The worst corner with nominal parts isn't the worst at 3σ.
- **Distribution robustness:** default + uniform, as in §2.4.

### 4.5 Calibration

Precision specs are judged **after** calibration. A `calibrate` step in the contract (e.g. zero and gain at 25 °C) subtracts the calibrated knobs' contributions **exactly**, because they're named terms in the affine form. What's left is what calibration can't fix: drift over temperature and life.

Example: a load-cell front-end goes from all its error at 25 °C to **3479 ppm** worst case at 85 °C, dominated by gain-resistor tempco. Putting the gain resistors in one network cuts that term from 2968 to **237 ppm**. The engine's contributor ranking points straight at that fix.

### 4.6 Noise

Noise varies over time *within* one board, so it's neither a range nor a statistical knob.

- **A noise engine:** AC noise analysis at the operating point. Its sensitivities cost one extra solve per frequency (derived and checked in the precision study).
- **Specs get a per-reading form:** static error + z·σ_noise, which is distinct from the population RSS.

### 4.7 Circuits with several stable states

Latches, Schmitt triggers, hysteresis and start-up:
- **Classify** the stability of each DC solution.
- **Search** for multiple solutions, and trace hysteresis thresholds by continuation as fold points.
- **Declare history.** A spec on a multi-stable block must say which history it assumes. A latch's final state depends on the supply ramp: it holds against 20% mismatch at a 1 µs ramp, and loses at 5% mismatch at 1 ms.

Transient must guard against the Backward-Euler artefact that makes unstable points look stable when step × growth rate > 2.

### 4.8 Solving for `?` values

The question is "find values such that every tolerance passes".
- **For linear pieces** meeting §4.2, "every tolerance" becomes "every corner", so it's an ordinary constrained search: HC4 + branch-and-bound, in log space.
- **Otherwise**, design centering: maximize the smallest margin with the loop's gradients.

Then snap to E-series values or catalog parts (language §6.7), and verify the snapped design.

---

## 5. The worst-point loop, with its safety net

### 5.1 The steps

```
 edit
  │
  ▼
 [0] SAFETY NET  (shared by all specs)
     • simulate every range-knob corner, and 3–5 temperature points
     • every run logs each device's operating region and regime margins
       (headroom, dropout, VCE,sat, current-limit distance)
  │
 [1] nominal simulation (+ slopes, from the backend or by nudging, §6.3)
  │
 [2] per spec: affine form  →  predicted worst point
     (box-constrained, interior values allowed; both sides of two-sided specs)
  │
 [3] SIMULATE the predicted worst point  →  real value (inner bound)
     └── violates the spec?  →  FAIL + counterexample
  │
 [4] compare prediction vs simulation, and check the guards (below)
     • re-linearize at the worst point (trust region);
       remember visited corners; test joint flips of near-zero-slope knobs
     • statistical knobs: find the kσ worst point at the worst range slice; simulate it
     • repeat until prediction ≈ simulation AND no guard fires (typically 2–4 rounds)
  │
 [5] verdict + bracket + contributors + tags
  │
 [6] (sign-off) statistics: importance sampling, board yield, Clopper–Pearson (§4.4)
```

### 5.2 Guards: signs the line is lying

"Prediction ≈ simulation" is **never** treated as proof on its own. Any of these sends the loop back to step 4, or ends in UNDECIDED:

- A range corner from step 0 is worse than the loop's answer. **The zero-slope limiter case.** Example: a THD spec the loop called 0.0026% was really 3.29% at a corner.
- A device's operating region at the worst point differs from nominal (saturation, dropout, current limit).
- A regime margin is small or negative at any simulated point. Example: buck peak inductor current 4.07 A vs the 4.0 A minimum current limit.
- A slope changes sign across the box. Example: gain vs temperature flips at β = 100 (walkthrough §5.7).
- Re-linearization cycles between corners. Seen on the boost converter. Fall back to enumerating the involved knobs.
- A for-all-frequency spec has several local extrema. Each is tracked separately; linearizing at the nominal worst frequency was 5× too optimistic on a high-Q band-pass.

### 5.3 The analysis ladder

Power circuits need four levels: formulas → averaged model → periodic steady state (PSS) → full switching transient.

1. The loop **searches on the cheapest adequate level**.
2. It **verifies the worst point one level up**.
3. The difference is carried as a named model-gap term in the bracket. Averaged vs switching undershoot: −168 vs −218 mV.

PSS by shooting reached steady state in 21 periods instead of about 1,160. Every PSS result is checked for **stability multipliers**; one run converged to an unstable orbit with multiplier −1.18.

---

## 6. Simulator backends

Our simulator is young. The engine must work with **established simulators today**, and let ours take over analyses as it grows. So the engine talks to simulators only through a **backend interface**.

### 6.1 The interface

What the engine needs from any backend:

| Call | Purpose |
|---|---|
| `capabilities()` | Which analyses, devices and model formats it supports; its sensitivity level; batch and parallel limits |
| `prepare(model, knobs)` | Turn the design model into the backend's input. **Our simulator reads it natively,** lowered to a `spicy_circuit::Circuit` with no text in between (roadmap §2.1). External simulators get a netlist in their SPICE dialect, with every knob as a named parameter (`.param amp_r1_value = …`) |
| `run(points, analyses)` | Simulate a batch of knob settings (corners, worst points, samples), in parallel where possible |
| `results()` | Waveforms and operating-point data, plus per-device operating region where the backend exposes it |
| `sensitivities(metric)` | Optional, level-dependent (§6.2) |

**Measurements are always done by our own measurement library** (language §8.4) on the returned waveforms, never by each simulator's own `.meas`. That keeps a spec meaning the same thing on every backend.

### 6.2 Capability levels

| Level | Provides | What the engine does | Cost per linearization |
|---|---|---|---|
| **L0** | Values only | Slopes by nudging each knob (finite differences, step sized well above solver tolerance) | N + 1 runs for N knobs |
| **L1** | DC/AC sensitivities | Adjoint or direct slopes for DC and AC specs | ≈ 1 run |
| **L2** | Transient sensitivities, crossing-time sensitivities | Slopes for transient specs | ≈ 1 forward + 1 backward pass per metric |
| **L3** | PSS with multipliers, noise, event location, operating-region reporting | Power and precision specs at full strength | — |

The engine chooses its methods from the level, and **says what it cost** ("worst-point loop, 23 runs, L0 slopes"). At L0 the loop still works; it's just more expensive, so nudged slopes are limited to the knobs a spec is sensitive to. That's found cheaply from a first screening round, or known from the structure: a spec can only depend on knobs in its connected region.

### 6.3 External simulators

| Backend | Why | Notes |
|---|---|---|
| **ngspice** | Open source; broad device support; runs many vendor models (with PSpice/LTspice compatibility modes); batch mode and a shared-library API | Candidate for the **first external backend**. Its sensitivity analysis (`.sens`) covers DC and AC (to verify when implementing) |
| **Xyce** | Open source (Sandia); parallel; DC and transient sensitivities, direct and adjoint [Xyce16] | An L2 backend for transient slopes |
| **LTspice** | Very widely used; many vendor models ship for it | L0: batch mode + `.step`; no sensitivities |
| **SIMPLIS** | Piecewise-linear switching simulation; fast PSS for power | For the power analysis ladder, if licensing allows |
| **Commercial** (Spectre, HSPICE, PSpice) | Where customers already have them | Later, via the same interface |

**Encrypted or vendor-only models** run on whichever backend supports them. The engine only needs values, so they still participate at L0.

### 6.4 Cross-checking

- The same spec can run on **two backends**. Disagreement beyond the numerical error band flags a model or simulator issue, so the verdict stays UNDECIDED until resolved.
- A **golden test suite** (the walkthrough amplifier, the research circuits) runs on every backend in CI.

### 6.5 Our own simulator: roadmap and what each step unlocks

| Step | Adds | Unlocks |
|---|---|---|
| 1 | **Parameter layer** (devices reference knob IDs; re-stamp without re-parsing) | Fast corners and samples; the base for sensitivities |
| 2 | **Controlled sources (E/G/F/H), B-sources** | Op-amp macromodels, IC behavioral models |
| 3 | **Robust DC:** junction limiting, gmin/source stepping, errors instead of panics, separate V/I tolerances, reltol ≈ 1e-6 engine mode | Real circuits converge; ppm specs |
| 4 | **Transposed KLU solve + DC sensitivities** | L1 for DC |
| 5 | **MOSFET, switch, comparator/logic primitives** | Power stages, digital interfaces |
| 6 | **AC at the operating point** (sparse complex KLU), loop-gain probe, AC sensitivities | L1 for AC; stability specs |
| 7 | **Variable-step transient** with breakpoints, event location, PWL ramps | Correct timing and crossing measures |
| 8 | **Temperature** (tc1/tc2, device temperature models), then electrothermal | The temperature knob actually does something |
| 9 | **Noise analysis** | Precision specs |
| 10 | **Transient sensitivities,** then **PSS** with multipliers | L2, L3 |

Until a step lands, the corresponding analyses route to an external backend. A spec never fails to run just because our simulator lacks a feature, as long as some backend has it.

### 6.6 Model coverage

A vendor model that ignores temperature makes the temperature knob look irrelevant. So:

- **The coverage map** records, per model, which knobs it responds to (found from slopes, at any level). Missing responses tag the verdict model-conditional.
- **Standard wrappers** put the missing knobs back:
  - an op-amp error shell: Vos, Ib, CMRR, PSRR, Aol around a vendor model;
  - wrappers for a MOSFET's RDS(on) and Vth.
- **A native parametric op-amp,** fitted to the vendor model at typical.
- **Sensitivity to un-toleranced model parameters** is reported: "this spec is sensitive to `gm_ea`, which has no datasheet limit". This is exactly the buck case, where worst-case phase margin is 14.7° with assumed spreads vs 37.8° at typical.

---

## 7. Hierarchy and composition

- **Contracts:** assumptions become range knobs; guarantees are specs (language §8).
- **At every connection:** upstream guarantee ⊆ downstream assumption.
- **Evidence ladder:**
  1. declared guarantees;
  2. characterized results (affine forms, with the downstream forms substituted in, so shared knobs stay correlated);
  3. in-context simulation.
- **Interface quantities go beyond a voltage range.** The power study showed contracts also need:
  - source impedance vs frequency;
  - a ripple spectrum;
  - start-up demand (inrush);
  - mode (a downstream converter starting mid-ramp pushed the eFuse into current limit with 16.5 W in its FET).
- **Loading:** `z_src` / `z_load` assumptions, checked like any other.

---

## 8. When things run

| Trigger | What runs |
|---|---|
| **Every edit** | Type and net checks; datasheet arithmetic (§4.1); corner-theorem checks (§4.2); re-evaluation of stored affine forms for the changed values |
| **Background** | The safety net (range corners); the loop for DC/AC specs |
| **On demand** | Transient and PSS specs; deep searches |
| **Sign-off** | Statistics (§4.4); cross-check on a second backend (§6.4) |

**Cost, honestly stated:**
- With L1 sensitivities, the loop needs roughly 5–8 simulations per DC/AC spec, plus the shared safety-net corners.
- At L0, each linearization costs one run per relevant knob.
- There is no convergence guarantee; Cadence quotes "under 100 simulations for each spec" for its commercial version.

---

## 9. Decisions

| # | Decision | Choice | Reason |
|---|---|---|---|
| D1 | Build or adopt | Build our own engine in Rust | Unchanged from v1 |
| D2 | Result format | Affine forms in named knobs | Combining, calibration, hierarchy, error budgets, board yield (§3.1) |
| D3 | Main method for nonlinear specs | Worst-point loop, linearized at the worst point | Industry-proven (FORM / WCD; MunEDA, Cadence) |
| D4 | Guard the loop | Range-corner enumeration, regime guards, multi-start | Zero-slope limiters give confident false PASSes (§5.2) |
| D5 | Knob kinds | range / statistical / mode / solver / calibration / layout- and firmware-owned | §2.1 |
| D6 | Provenance and shape on every knob | Required | Honest verdicts; typical-only values exist everywhere (§2.2–2.3) |
| D7 | State-dependent parameters | In the device model, not knobs | §2.6 |
| D8 | Exact methods | Datasheet arithmetic + corner theorem with applicability checker | Most digital specs; linear pieces (§4.1–4.2) |
| D9 | Verdicts | Seven verdicts + tags | §3.3–3.4 |
| D10 | Yield | Board yield, distribution-robust, confidence-bounded | §4.4 |
| D11 | Default confidence | `sigma(3)` for user specs, `worst_case` for automatic checks | §3.6 |
| D12 | Simulators | Backend interface with capability levels; ngspice as first external backend | §6 |
| D13 | Measurements | Our own library on returned waveforms, for every backend | §6.1 |
| D14 | External solvers (dReal, IBEX) | Still deferred; needed less than thought | §4.8 |
| D15 | Build order | Engine first, driven by `.spicy` netlists + the language subset needed | §10 |

---

## 10. Build order

1. **Simulator foundations** (§6.5 steps 1–4): parameter layer, controlled sources, robust DC, transposed solve + DC sensitivities.
2. **Backend interface + ngspice backend,** so real circuits (op-amps, MOSFETs, vendor models) run now, while step 1 matures.
3. **Knob model + part records + the datasheet-arithmetic engine** (§2, §4.1), with the language subset needed to write parts and contracts. This needs no new simulator features, and covers most digital specs exactly.
4. **The worst-point loop with its safety net** on the walkthrough amplifier. Cross-check our simulator against ngspice; compare with the answer key.
5. **Statistics:** board yield from affine forms, importance sampling, Clopper–Pearson.
6. **Our simulator's next steps** (§6.5 steps 5–10), each moving analyses from external backends to ours.

---

## 11. Open questions

- Default distribution for "guaranteed by design" limits (tested limits have a known over-guard; untested ones don't).
- How mode knobs interact with statistics (per-mode yield vs product yield).
- Electrothermal coupling granularity: per part, or per board region with a θJA assumption?
- Which derating policy is the default (a documented "commercial" set vs NASA / ECSS).
- How far guaranteed AC methods are worth taking, given the power study found them low-value for loops.
- Catalog and part-data sources, and how to keep builds reproducible (language §12).
