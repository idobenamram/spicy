# AI-Native ECAD Editor — Engine Design v4

> 2026-09-27 · Supersedes `archive/engine_v3.md`.
> Companion docs:
> - `walkthrough.md`: one amplifier through the whole engine, with real numbers. **Start here if you're new.**
> - `engine_plan.md`: the engine MVP plan on ngspice, accepted 2026-09-27: verdict tables, backend rules, test suite, milestones. Cited as "plan §n".
> - `language.md`: how circuits, parts and contracts are written. `specs.md`: spec concepts.
> - Research: round 3's position papers `research/engine_position_{minimal,soundness,agent,scale,alternatives}.md`, cited by short name ("minimal §4.7"); round 2's `research/engine_synthesis.md`, cited by its changes C1–C12; round 1's `research/synthesis.md`, behind v3.
> - `bibliography.md`: references (keys like **[AGW94]**).

---

## 0. What changed from v3

1. **The MVP check simulates every corner** (§5): per spec side, up to 2^12 corners, plus guards inside the box and a search for the worst 3σ board. CE amp: 579 runs, 0.4 s, every `worst_case` value equal to the brute-force answer key.
2. **The worst-point loop becomes the scale path** (§5.8), after M3. Its guards are updated (C1–C5, the splice, the PASS allowance by measure kind), and it's accepted only when it agrees with enumeration.
3. **No affine forms in the MVP** (§3.1, D-F). Measures are evaluated per run; contributors are the flip table and main effects.
4. **New verdict words** (§3.3, D-E): PASS (all corners), PASS (estimated), PASS (implied by worst case). Every UNDECIDED carries a reason and a `next` step; nothing is "verified".
5. **ngspice is the first backend** (§6.3, D-B), and stays L0 for good. Our simulator joins as the second.
6. **The engine runs when asked** (§8): a full cold check on `spicy check`, a re-check on save in the editor. No instant estimates.
7. **Defaults are written out** (§6.6, D-A): the default transistor model decides whether bias FAILs at worst case.
8. **Numbers come from the plan's reference configuration** on ngspice-42 (plan §2.1), not v3's simplified model.

---

## 1. Goal and principles

**Goal:** for every spec, a verdict the engineer can trust over every tolerance, temperature and supply condition, with its worst value, what uses up the margin, and a counterexample when it fails.

| # | Principle |
|---|---|
| P1 | **The simulator is the source of truth for anything nonlinear.** The engine asks it only for values (and slopes where available), so any simulator and vendor model can be used |
| P2 | **Simulated points prove failures. A PASS says what it rests on:** every corner, a search, or the worst case |
| P3 | **A cheap approximation is never the last word.** Every corner of a side's cone is simulated; past the budget, every predicted worst point is |
| P4 | **Every number has a provenance, and every verdict says what it rests on:** model, distribution, defaults, unreviewed data |
| P5 | **Sampling answers "how often", never "how bad".** It can witness a FAIL, never support a PASS |
| P6 | **One run table per check.** Every measure is evaluated per run; every run counts for every side |
| P7 | **Cost is bounded and stated:** at most 2^12 corners per side, the loop beyond; every report prints its runs and time |

### 1.1 The running example

The walkthrough's common-emitter amplifier (walkthrough §1), as `circuits/ce_amp.spl`:

| Knob | Kind | Range | | Spec side | Measure | Bound |
|---|---|---|---|---|---|---|
| `temp` | range | −10 … 60 °C | | bias max / min | `dc(output.v)` | ≤ 6.5 V / ≥ 4.5 V |
| `vcc.v` | range | 12 V ± 5% | | gain max / min | `h.at(1kHz).mag()` | ≤ 4.83 / ≥ 4.37 |
| `r1` `r2` `rc` `re` | statistical | 47k, 10k, 4.7k, 1k ± 1% | | bass | `h.f_low(-3dB)` | ≤ 30 Hz |
| `c_in` · `q1.beta` | statistical | 1 µF ± 20% · 100 ..= 300 | | | | |

Nominal: VC 5.5032 V, |H(1 kHz)| 4.5908, f_low 20.127 Hz. Q1 uses D-A's model. The specs' confidence is `sigma(3)`, the default.

---

<a name="knob-model"></a>
## 2. The knob model

A **knob** is one independent thing that can vary. It's named by instance path (`amp.r1.value`, `temp`) and has a range. The language creates knobs (language §5.4; M1d's `KnobTable`); part records supply their data (language §6.5).

### 2.1 Kinds

| Kind | Examples | How the engine treats it |
|---|---|---|
| **Range** | temperature, supply, lifetime, input amplitude/frequency, load | Must hold at **every** value, so always take the worst |
| **Statistical** | part tolerances, IC parameter spreads | Worst case, σ, or yield, depending on confidence (§3.6) |
| **Mode** (discrete) | power state; PWM / PFM / current limit; boot/reset state; bus driver; firmware setting | Enumerated; each simulation reports which mode it ended in; specs can be scoped to a mode |
| **Solver** (`?`) | values left open | Chosen so specs pass (§4.8) |
| **Calibration / trim** | offsets and gains removed by a calibration step | Subtracted exactly (§4.5) |
| **Layout-owned assumption** | bus capacitance, trace length, board thermal resistance θJA | A range the schematic can't know; the verdict becomes "PASS given …", and the assumption is exported as a layout constraint |
| **Firmware-owned assumption** | watchdog kick period, boot time, peripheral timing | Same, owned by firmware |

The MVP implements range and statistical knobs; the rest come with parts (M5) and the full knob model (M6).

### 2.2 Provenance

Every knob's range says where it came from:
- **Tested limit.** A production-tested min/max.
- **Guaranteed by design.** Not tested, but specified.
- **Typical only.** The datasheet gives no limit. There's no defensible distribution, so verdicts that depend on it are tagged.
- **Assumed.** Entered by the engineer.
- **Missing.** No data. A spec that needs it is UNSPECIFIED.

On top of that come the source reference (datasheet page and table) and whether the record was reviewed by a human.

### 2.3 Shape

| Shape | Example |
|---|---|
| **Slope** | A resistor tempco of ±25 ppm/K: a statistical coefficient × the temperature range knob |
| **Envelope** | An op-amp bias current of 300 pA at 25 °C, but 10 nA over −40…125 °C |
| **Box** | Reference tempco "box" specs, about 2× wider than a linear-tempco reading allows |
| **Aging law** | Linear, √t, ∛t: these differ by 20× over 10 years, so the law must be explicit |
| **Hysteresis** | A reference's thermal hysteresis |
| **Table** | Logic thresholds by supply band × temperature column, **with gaps** (a 74LVC1G04 guarantees nothing between 1.95 and 2.3 V) |

**The slicing rule.** For statistical × range products (tempco × temperature), go to the range extreme and treat the coefficient statistically there. Multiplying the two noise symbols would destroy tempco-tracking cancellation, and adding every tempco at full strength overstates budgets (2968 vs 1817 ppm in the precision study).

### 2.4 Distribution

- **Default** for a tested limit: normal with σ = tol/3, cut at the tolerance, through the exact per-knob map (round-2 D3). Datasheets often test much wider than the real spread: OPA189 Vos max is 7.5× typical.
- **Also checked under uniform,** the worst case among symmetric unimodal distributions [Birnbaum 1948; Barmish & Lagoa 1997 — to add to the bibliography], wherever it could change the verdict: where `worst_case` fails and the default passes (C7). If they disagree, the verdict is **UNDECIDED (distribution)**.
- **Typical-only values have no distribution.** A verdict that depends on one is tagged, never silently computed.

Example: the CE amp's bias max at `sigma(3)` is 6.2813 V under the default and 6.4657 V under uniform, a PASS either way. The suite's difference amplifier gives 1.81 vs 3.16 mV/V against ≤ 3 mV/V, so it's UNDECIDED. That's right: what decides it is information (a declared distribution, a matched network), not more simulation (plan §8.3).

### 2.5 Structure

- **Lot + per-part.** Parts from one reel or network share a lot knob and have small per-part knobs. Resistor networks match to ±0.01% even at ±0.1% absolute. A pinned network part supplies both numbers.
- **Constraints between limits.** A 74HC14's threshold box alone would allow −0.3 V hysteresis; treating the NE555's two thresholds as independent predicts a period from 0.11× to 2.17× nominal. Part records can state constraints (threshold ordering, minimum hysteresis, a ratio spec).

### 2.6 What is *not* a knob

**Parameters that depend on the circuit's own state** belong in the device model, evaluated at the operating point: MLCC capacitance vs DC voltage, inductance vs current, RDS(on) vs junction temperature. Treating them as range knobs (as v2 did for DC bias) is the wrong kind of object.

**Self-heating** needs an **electrothermal fixed point**: dissipated power → junction temperature → device parameters → power. The buck reaches 153–196 °C junction at 85 °C ambient, depending on θJA. No fixed point (thermal runaway) is a FAIL.

---

## 3. Results

### 3.1 Measures are evaluated per run (affine forms deferred)

v3 stored every result as an affine form `center + Σ coeffᵢ·εᵢ ± err`. v4 defers that (D-F). Instead, **every measure, derived ones included, is evaluated per run** from that run's values. `top_room = vcc.v − dc(output.v)` takes both from the same run, so a shared knob can't be counted twice, which is what forms were for. Treating them as two independent ranges gives 11.4 − 6.5595 = 4.84 V; per run, the true minimum over the corners is 5.1951 V. A side's inner bound is the best row of the whole table (**pooling**, C4). Each σ search's last slopes are kept in the record.

**Why.** Nothing in the MVP combines forms, and a simulator line has no proven remainder, so a form would only carry observed misses (C8). Stored forms are also wrong for edits: for C_in 1 → 2.2 µF a line predicts f_low = −4.0 Hz (C11) and a quadratic 42.9 Hz (alternatives §3.7). A re-check gives 12.177 Hz.

**When they return:** with hierarchy (§7), calibration (§4.5), error budgets, datasheet arithmetic (§4.1) and board yield (§4.4). Round 2's type is the plan for then: a `Remainder` that keeps exact, proven and observed errors apart, plus a separate band. Only exact or proven remainders may support PASS (guaranteed).

### 3.2 What a result carries

| Quantity | What it is | Proves |
|---|---|---|
| **Inner bound** I | The worst value actually simulated | Failures |
| **Bracket** | How far the true worst could lie beyond a search's result: 0 at enumerated corners; the last jump for the σ search | With I, PASS (estimated) |
| **Band** ε_num | The numerical uncertainty of a simulated value (§5.6) | Nothing; it widens every comparison |
| **Outer bound** (later) | Proven by exact methods (§4.1–4.2) | PASS (guaranteed) |

### 3.3 Verdicts and statuses

| Word | Claims | Does not claim |
|---|---|---|
| **FAIL** | A reachable board violates the spec; its point is printed and reproduces at reltol 1e-9 | — |
| **PASS (all corners)** | Every corner of the cone simulated and passing; no inward nudge, audit point or pooled run beat the worst corner (§5.4) | That no small interior region away from the worst corner is worse |
| **PASS (estimated)** | A search's worst point passes by more than bracket + band | That no worse point exists (the problem is NP-hard) |
| **PASS (implied by worst case)** | `sigma(3)` only: the side passes at `worst_case`, and the σ region lies inside the box | More than that PASS claims |
| **UNDECIDED** (reason) | The engine can't decide; `next` says what would | "Probably passes" |

*Later:* **PASS (guaranteed)** from exact methods; **PASS (conditional)** given a layout or firmware assumption ("given bus capacitance ≤ 234 pF"); **UNVERIFIABLE (model)** when no model can speak to it (−115 dB THD); **UNSPECIFIED** (M5) when the data doesn't define the question (agent R9).

The first matching row of a verdict table picks the word (plan §3.1). A guard never blocks a FAIL, except a read-back mismatch: then the simulated point isn't the reported one, and even a FAIL is UNDECIDED (binding).

**UNDECIDED reasons, and `next`** (plan §3.3; `spicy check --deep` runs the automatic ones):

| Reason | `next` |
|---|---|
| **binding:** a knob didn't reach the simulator; the check stops after 1 run | None: an exporter bug, named in the message |
| **budget:** more than 2^12 corners | The loop (§5.8) |
| **numerics:** \|I − B\| ≤ ε_num | For f_low, exact refinement (3–5 runs) |
| **simulator:** a run failed or was implausible | Restart the worker and retry |
| **ascent budget** | A larger budget, more starts |
| **search · interior peak · bracket · uniform spread** (§5.5) | The **ball search** over the whole 3σ ball: 6,600–9,600 runs per side, ≈ 5–7 s |
| **device region changes** | A σ search on the device margin (after M3) |
| **distribution:** default passes, uniform fails | None: declare a distribution, or change the parts |

**Statuses.** A number outside a verdict is **`simulated`** (one real run at a stated point: `spicy sim --at`, a sweep, the flip table), which may prove a FAIL but never a PASS, or **`stale`** (a record of another design revision, §8). `predicted` and `carried` come later. **No "verified":** an LLM reads it as "proven" (agent §3.3, §5.3 #8).

**`claim`.** The engine writes each verdict's sentence; the agent may paraphrase it but never strengthen it (agent R1). *"bias max FAILS at worst case: 6.5595 V > 6.5 V at temp −10 °C, vcc.v 12.6 V, r1 47.47k, r2 9.9k, rc 4.653k, re 1.01k, q1.beta 100 (c_in: any)."*

### 3.4 Tags

**model-conditional** (§6.6), **relies-on-typical**, **relies-on-unreviewed-part-data** (M5), **distribution-sensitive**, **mode-dependent**, and **region** ("q1 saturated at the worst point": informs, doesn't change the verdict). Every report also lists **`not_modeled`**: defaulted model parameters that matter, from the model cards.

### 3.5 Counterexamples and contributors

- **Counterexample:** pasteable knob settings (`spicy sim ce_amp.spl --at bias.max`). Knobs outside the measure's cone print as `any`, or an LLM blames C_in for a bias failure (agent §5.3 #3).
- **Contributors:** two exact secants from the run table, at no extra runs (plan §7.4):
  - **the flip table:** each knob moved to its other edge at the counterexample. For bias max (6.5595 V): temp → 5.8578 V, β → 6.1395, vcc → 6.2049, r1 → 6.4096, r2 → 6.4204, rc → 6.4375, re → 6.4502, c_in no effect. Every single flip passes: the failure needs every part at its edge at once;
  - **main effects** over all corners, V per half-range: temp −0.333, β −0.185, vcc +0.142, r1 +0.076, r2 −0.073, rc −0.064, re +0.060.

  Split into range and statistical: the largest, cold, is a range knob, so better parts won't remove it.

### 3.6 Confidence classes

| Class | Meaning |
|---|---|
| `worst_case` | Every knob, statistical ones included, anywhere in its range. ECSS's default extreme-value analysis |
| `sigma(k)` | Range knobs anywhere; statistical knobs together no less likely than a kσ event. **Per spec side:** 3σ = 99.865%; a two-sided 99.9% needs 3.29σ per side (round-2 D4) |
| `yield(p)` | Range knobs worst; at least a fraction p of boards pass, with a confidence bound |
| `nominal` | Quick checks only |

**Defaults:** `sigma(3)` for user specs, `worst_case` for automatic checks. Both confidences' values are shown on every side; the spec's confidence picks the headline.

---

## 4. Methods: the right tool for each question

| The quantity depends on… | Method | Best verdict |
|---|---|---|
| Datasheet numbers and formulas only | §4.1 datasheet arithmetic (M5) | guaranteed, usually exact |
| A linear piece, with the theorem's conditions met | §4.2 corner theorem (later) | guaranteed, exact |
| Anything simulated | §5 enumeration + guards + 3σ search; the loop past 12 knobs per side | all corners / estimated |
| Yield | §4.4 statistics (M7) | statistical bound |

### 4.1 Datasheet arithmetic and hand formulas

About **two-thirds of board-level digital specs** are arithmetic on datasheet tables: logic-level margins, I2C pull-up and rise-time windows (UM10204), SPI/I2C timing budgets, supervisor and watchdog windows, crystal gm_crit, ADC source-impedance limits, pin currents, charge-current programming, UVLO dividers.

These formulas are monotone in every knob, so evaluating corners is **exact in about 2 evaluations**. Where monotonicity can't be proven, affine arithmetic with splitting gives a guaranteed bracket: the first place forms return. Each supply rail is one shared knob, so correlations are kept. Plain intervals lose them: PolymorphicBlocks reports a false −1.35 V failure where the true margin is +0.036 V.

### 4.2 The corner theorem, with a strict applicability checker

For a linear circuit, extremes are at corners [N&P07 Thm 5.1, PPC65]. The engine applies this **only** when a checker confirms all of these:
1. Each knob enters the circuit matrix as **one rank-1 term**: R (via g = 1/R), a conductance, a source value, or a controlled-source gain. Not a turns ratio, pot wiper, or shared knob.
2. The knobs are independent (no lot knob spanning several elements).
3. The circuit stays solvable over the whole box (the determinant, multilinear too, never changes sign).
4. The measured quantity is a node voltage, a branch current, or a sum or ratio of them. **Not** power, loop gain, or anything involving a state-dependent element.

Enumeration already simulates every corner; the theorem adds the proof that nothing inside is worse, turning PASS (all corners) into PASS (guaranteed). Proofs "typically fail in high dimensions" [N&P07].

### 4.3 Simulated specs

Section 5.

### 4.4 Statistics (M7)

- **Board yield, not per-spec yield at one corner:** a board must pass every spec at every range condition (26.7% loss reported per spec vs 36.8% true). First estimate from stored forms (Schenkel et al., DAC 2001), once they return; confirmation by sampling across range corners and all specs.
- **Importance sampling** centred on the 3σ points: 2,000 runs matched 20,000 plain Monte Carlo runs in the red-team test.
- **Clopper–Pearson stopping** with three-valued yield verdicts, as Cadence does. One counterexample doesn't fail a yield spec.
- **Joint search over range and statistical knobs** (Solido PVTMC style), as the σ search does; **distribution robustness** as in §2.4.
- **Sampling never supports a PASS.** 20,000 boards at bias max's corner reach at most 6.4038 V, 0.156 V short of the worst corner (plan §2.2); a 70-point Latin hypercube gave 949 false PASSes in 960 failing cases (alternatives §3.3).

### 4.5 Calibration

Precision specs are judged **after** calibration. A `calibrate` step (e.g. zero and gain at 25 °C) subtracts the calibrated knobs' contributions **exactly**, as named terms of an affine form, so it waits for forms (D-F). What's left is drift over temperature and life. Example: a load-cell front-end has **3479 ppm** worst case at 85 °C, dominated by gain-resistor tempco; putting the gain resistors in one network cuts that term from 2968 to **237 ppm**. The contributor ranking points straight at that fix.

### 4.6 Noise

Noise varies over time *within* one board, so it's neither a range nor a statistical knob. A **noise engine** runs AC noise analysis at the operating point (sensitivities: one extra solve per frequency). **Specs get a per-reading form:** static error + z·σ_noise, distinct from the population RSS.

### 4.7 Circuits with several stable states

Latches, Schmitt triggers, hysteresis and start-up: **classify** each DC solution's stability; **search** for multiple solutions and trace hysteresis thresholds by continuation; **declare history** (a latch holds against 20% mismatch at a 1 µs supply ramp and loses at 5% at 1 ms). Transient must guard against the Backward-Euler artefact that makes unstable points look stable when step × growth rate > 2. On ngspice every run is cold, so a second DC solution is never found (open question).

### 4.8 Solving for `?` values

**For linear pieces** meeting §4.2, "every tolerance" becomes "every corner": HC4 + branch-and-bound, in log space. **Otherwise**, design centering with the σ searches' slopes. Then snap to E-series or catalog parts (language §6.7) and check the snapped design; no candidate is a fix before a check ran on it.

---

## 5. The check: enumeration, inside-the-box guards, the 3σ-point search

v3 made the worst-point loop the main method, with a safety net of corners. Round 3 found that at the MVP's size the net alone does the job, exactly (minimal §4.2, alternatives §3.9). The loop stays the way to scale (§5.8). Plan §2 walks every stage.

### 5.1 The steps

```
 spicy check circuits/ce_amp.spl
  │
 PLAN  8 knobs · 5 sides · a cone per measure (the knobs that can reach it: bias 7, gain/bass 8)
  │
 [0] nominal + read-back of every knob ...............................     1 run
  │    a knob that didn't reach the simulator stops the check
 [1] WORST_CASE: every corner of each side's cone ....................   256 runs
  │    each side's worst = the best row of the table
 [2] INSIDE THE BOX: nudges at each worst corner + 8 audit points ....    46 runs
  │    anything that beats the corner starts an ascent
 [3] SIGMA(3): the 3σ point per side, 2 starts .......................   265 runs
  │    + range nudges; + uniform spread where worst_case fails
 [4] BAND: decisive points re-run at reltol 1e-9 .....................    11 runs
  │
 verdict tables → records (claim, next) → table · JSON · .spicy/checks/<rev>.json
                                                             579 runs, 0.4 s
```

### 5.2 Routing

A side enumerates its cone when 2^|cone| ≤ 4096 **and** the estimated time fits 3 s (plan §2.3). Over budget, it goes to the loop (until then UNDECIDED (budget), D-D). The limit is per side, not in total: a two-stage amplifier's local bias sides (7 knobs each) still enumerate. **Cones come from the circuit, never from a slope** (the MVP rule: a capacitor can't reach an operating point). Dropping small-slope knobs saved under 5% at 302 knobs and broke exactness, because a limiter that's off at nominal has zero slope (scale §5.5).

### 5.3 `worst_case`: every corner

Every corner is one run, and every run returns every measure. That gives each side's exact corner extreme, and **every limiter that switches on at some corner** (on the hiZ variant 62 of 256 corners saturate). v3's guards (a zero-slope limiter, a region change, a slope that changes sign, a cycling search) are covered by a table of every corner. Enumeration even decides where the loop is pessimistic: hiZ's gain ≤ 9.273 and VC ≥ 1.197 are UNDECIDED by the loop and PASS (all corners) by enumeration (plan §2.4).

**CE amp:** bias max = **6.5595 V** at (−10 °C, 12.6 V, R1 +1%, R2 −1%, RC −1%, RE +1%, β 100). 2 of 256 corners fail: that DC corner with either C_in. The others: bias min 4.6921 V, gain 4.4618 … 4.7028, bass 26.790 Hz.

### 5.4 Inside the box

Corners can't show a maximum inside the box. Three guards look there; each feeds one **ascent** (line searches, then coordinate ascent; plan §2.5), whose result is FAIL, PASS (estimated) or UNDECIDED (ascent budget).

| Guard | What it does | CE amp | Catches |
|---|---|---|---|
| **Tangent check** | Nudge each cone knob 1% inward at each side's worst corner | 38 runs | pq's dissipation needle next to the worst corner (8.7097 mW inside vs 8.7069): only this guard sees it |
| **Audit** | 8 seeded Latin-hypercube points: witnesses and refuters, never an allowance | 8 runs | tuned's resonance on the test frequency, which beats the corners over 29% of the box |
| **Pooling** | Every in-box row, e.g. the σ search's, checked the same way | 0 runs | tuned, too |

None reliably finds a small interior region away from the worst corner (open question). On the CE amp nothing beats a worst corner.

### 5.5 `sigma(3)`: the 3σ-point search

**The question.** Temperature and supply take any value in their range; parts vary by their spreads (§2.4). The worst board no less likely than a 3σ event is the side's **3σ point**.

**The CE amp's answer.** For bias max: −10 °C, 12.6 V, β = 111.5, R1 47.10k, R2 9.980k, RC 4.692k, RE 1.0015k, giving VC = **6.2813 V**. A weak transistor (2.66σ below 200), resistors only 15–21% of the way to their edges. The worst corner needs all six at their edges at once, which is why bias fails at `worst_case` and passes at `sigma(3)`.

**How** (plan §2.6):
1. **Start A:** the worst temperature/supply corner, parts at nominal. Nudge the parts, jump to the 3σ board the line predicts is worst, simulate, repeat; never accept a worse point. Stop when prediction and simulation agree to 1e-4 of the bound.
2. **Start B:** the worst corner, scaled into the 3σ ball. One start missed a saturating corner and a second band edge (minimal §4.5).
3. **Other temperature/supply corners** are cross-checked, and temperature and supply are nudged inward (an improvement means UNDECIDED (interior peak)).
4. **Uniform spread** where `worst_case` fails and the default passes.

The search can't vouch for its answer if a corner has a device outside its nominal region, if `worst_case` needed an ascent here, or if the spreads disagree. **Nesting:** the σ region is inside the box, so a `worst_case` PASS implies a `sigma(3)` PASS (4 of the 5 CE-amp sides). **Accuracy:** all five sides within 4.1e-5 of an independent σ key. A `sigma(3)` PASS (estimated) rests on a local search, a weaker basis than enumeration, and says so.

### 5.6 The numerical band

Decisive points are re-run cold at reltol 1e-9: ε_num = max(2·|f − f_tight| + the measure's band, 1e-9·|f|). CE amp: ≈ 5e-9 for VC and gain, 2.7e-5 Hz for f_low. Always measured by re-running: a difference table reported 2e-13 V where the error at default tolerances is 9.5e-5 V (soundness §6).

### 5.7 What it prints

```
$ spicy check circuits/ce_amp.spl
 CeAmp · 8 knobs (2 range, 6 statistical) · ngspice-42 · q1: generic Npn (IS 1e-14, XTB 1.5, TNOM 25 °C) · rev 3f9a1c
 spec  side      sigma(3)  (the specs' confidence)              worst_case
 bias  ≤ 6.5 V   6.2813 V   PASS (estimated)                    6.5595 V   FAIL
                 uniform spread 6.4657 V PASS                   at temp -10°C vcc.v 12.6V r1 +1% r2 -1% rc -1% re +1% q1.beta 100 c_in any
 bias  ≥ 4.5 V   4.8843 V   PASS (implied by worst case)        4.6921 V   PASS (all corners)
 gain  ≤ 4.83    4.6635     PASS (implied by worst case)        4.7028     PASS (all corners)
 gain  ≥ 4.37    4.5177     PASS (implied by worst case)        4.4618     PASS (all corners)
 bass  ≤ 30 Hz   24.973 Hz  PASS (implied by worst case)        26.790 Hz  PASS (all corners)
 bias passes at sigma(3); it fails only with every part at its worst edge, cold, on a high supply. Your call.
 not modeled: Early effect (VAF), junction capacitances, base/collector/emitter resistances
 cost: 579 runs (256 corners, 46 interior, 265 sigma, 12 other) · 65 rounds · 0.4 s
```

### 5.8 At scale: the worst-point loop

A 300-knob board's end-to-end spec has all 302 knobs in its cone. The loop's cost grows linearly: with cones, 22.6 runs per knob for both confidences. At 302 knobs that's 6,834 runs in 6 rounds, 3.0 s on 16 workers, and all 103 sides equal the exact key (scale §5.2).

```
 [0] shared round: nominal + a nudge per cone knob + the range-corner net
     every run logs device regions and margins; each margin is a side of its own
 [1] per side: jump to the vertex the slopes point to
 [2] at a vertex: flip each cone knob to its other edge; move only if it simulates better
 [3] stop when no single flip improves; splice in margin witnesses, continue if better
 [4] PASS (estimated) needs margin > the allowance for this measure kind + ε_num
```

**Its guards, updated:**

| Guard | Why | Source |
|---|---|---|
| **C1** Accept only improvements; stop at optimality | "Prediction ≈ simulation" holds by construction; re-linearizing cycled on 2 of 6 cases (v3's boost) | C1 |
| **C2** Flip at vertices | A slope that changes sign across the box (gain vs temperature at β = 100); a region change | C2 |
| **C3** Margin sides | A limiter switched on by β at some corners: the loop said gain 6.46, truth 0 | C3 |
| **Splice**; witnesses run every analysis | A limiter in stage 17 of 33 was found but never combined with the other stages' worst edges: gain min 0.438 vs 0.215 | scale §5.4 |
| **C4** Pooling + final cross-check | Turned a false PASS into a FAIL | C4 |
| **C5** Below-noise knobs to an edge; exclude by structure only | A limiter off at nominal has zero slope | C5 |
| **Range-corner net:** 2^R to R = 8, a strength-3 covering array beyond | A THD spec the loop called 0.0026% was 3.29% at a corner | scale §6.6 |
| **Regime row** | A worst point in another region than nominal is UNDECIDED unless it FAILs | C6 |
| **Allowance by measure kind** | e₂ (non-additivity of the least-losing flip pairs) for declared-smooth measures: 0.19% of the spread vs 9.7% for e_obs. It gave a false PASS on an undeclared kink, so kink-capable measures (`abs`, `clip`, transient) keep e_obs | alternatives §3.4 |
| **Per-extremum sub-searches** | Several extrema over frequency (a Chebyshev ripple: 0.678 vs 1.113 dB) | round 2 §8 |
| **Audit, vertex half** | Failures shared by 25% of the corners | soundness §5.4 |

**Why not in M3:** without its margin side the walk gave a false PASS on hiZ; with its guards it left two true PASSes UNDECIDED that enumeration decides (minimal §4.7). **Accepted** (plan §9.3) as a second strategy over the same cones and run table, once it agrees with enumeration on every regression circuit of ≤ 12 knobs per side and is exact on a 50-stage cascade; CI then cross-checks the two. With L1 slopes a vertex needs 1 run plus flips of the ambiguous knobs; for transient specs the loop searches a cheaper level (§5.9).

### 5.9 The analysis ladder (with the transient tier)

Power circuits need four levels: formulas → averaged model → periodic steady state (PSS) → full switching transient. The search runs **on the cheapest adequate level**, **verifies the worst point one level up**, and carries the difference as a named model-gap term (averaged vs switching undershoot: −168 vs −218 mV). PSS by shooting reached steady state in 21 periods instead of about 1,160. Every PSS result is checked for **stability multipliers**; one run converged to an unstable orbit (multiplier −1.18).

---

## 6. Simulator backends

The engine talks to simulators only through a **backend interface**. ngspice comes first; our simulator joins as the second backend.

### 6.1 The interface

| Call | Purpose |
|---|---|
| `prepare(design, knobs, needs)` | ngspice gets M1f's **engine deck**: a netlist with a `.param` per knob, plus knob and probe maps (plan §1.4, §5). Our simulator will read the model natively, lowered to a `spicy_circuit::Circuit` |
| `run(prepared, batch)` | A batch of requests (knob point, analyses, tolerance), in parallel where possible. Each run returns node voltages, device records, AC data, and every knob **read back** |
| `capabilities()`, `sensitivities()` | Later: the level (§6.2) and batch limits |

- **Read-back:** a mismatch stops the check (UNDECIDED (binding)). It catches a dead temperature knob after 1 run; without it, every configuration tested gave a false PASS (soundness §3.2 d).
- **A failed run is data:** it can't support a PASS, and a FAIL elsewhere stands. With no DC solution, ngspice returns 999,999,999.99 V and exit code 0, so a voltage above 10× the largest source counts as a failed run.
- **Measurements are done by our own library** (language §8.4), never by a simulator's `.meas`, so a spec means the same on every backend. ngspice's `meas` puts f_low at 20.083–20.161 Hz; the answer is 20.127 Hz.

### 6.2 Capability levels

| Level | Provides | What the engine does | Cost per linearization |
|---|---|---|---|
| **L0** | Values only | Slopes by nudging each knob | N + 1 runs for N knobs |
| **L1** | DC/AC sensitivities | Adjoint or direct slopes | ≈ 1 run |
| **L2** | Transient and crossing-time sensitivities | Slopes for transient specs | ≈ 1 forward + 1 backward pass |
| **L3** | PSS with multipliers, noise, event location, operating regions | Power and precision specs at full strength | — |

The level changes cost, not soundness: enumeration needs only values; slopes serve the σ search, the loop and design centering.

### 6.3 ngspice, the first backend

**Transport** (D-B, plan §4.1). The same 579 runs take **0.39–0.42 s** through **libngspice in a worker process**, 0.73–0.75 s through an `ngspice -p` pipe, and 1.0–1.2 s with an `ngspice -b` process per round. The worker wins: binary vectors, and a crash kills only the worker (restart 18–19 ms). `ngspice -b` stays as the cross-check (§6.4).

**One run** is `alterparam` per knob, `reset` (every run cold and bit-identical), `op`, the AC analyses, reads, `destroy all`. Plan §4.3 lists the traps, each pinned by a test; three would give silent wrong answers: `reset` undoes control-mode `option temp`, TNOM defaults to 27 °C, and `ac dec` puts "1 kHz" at 1000.0000000000143 Hz.

**ngspice stays L0 for good.** Its AC `.sens` returns ≈ 0, and after a `sens` command the instance ignores temperature changes. DC `.sens` gives one output per call (scale §6.4). The engine never sends `sens`.

**Other backends** use the same interface: our simulator (§6.5); **Xyce**, an L2 backend (DC and transient sensitivities [Xyce16]; its AC sensitivity includes the operating-point shift); **LTspice** at L0 (batch mode + `.step`), where many vendor models ship; **SIMPLIS** for fast PSS in the power ladder, if licensing allows; commercial simulators (Spectre, HSPICE, PSpice) where customers have them. **Encrypted or vendor-only models** run on whichever backend supports them; enumeration needs only values.

### 6.4 Cross-checking

| Check | Must agree | When |
|---|---|---|
| **Worker vs `ngspice -b`** | Same deck, to 1e-13 on the MVP corners | Every test run |
| **Engine vs answer keys** | The adversarial suite as decks in the repo (7 circuits, 24 spec sides): no false PASS, `worst_case` = key, σ within 1e-4 (plan §8.2–8.3). The prototype: 0 false PASS, 0 wrong FAIL | CI |
| **ngspice builds** | Exact within one build, 1e-12 across builds; the backend identity is part of `rev` | CI |
| **Our simulator vs ngspice** | Within the band; beyond it the verdict stays UNDECIDED until resolved | Once ours joins; sign-off |
| **Loop vs enumeration** | Every enumerable case | Once the loop lands |

### 6.5 Our own simulator: the second backend

It joins once it passes the suite and cross-checks ngspice (plan §9.3), then takes over analyses as it outgrows L0:

| Phase | Adds | Unlocks |
|---|---|---|
| **Join** | BJT reciprocity fix; errors instead of panics and silent non-convergence; exact AC grid; device records; M2e tolerances; M2c temperature; M2d AC (`pipeline.md` §11) | The suite; the cross-check |
| **Grow** | Parameter layer, transposed KLU, DC and AC sensitivities (M8); controlled and B-sources; MOSFET, switch, logic; variable-step transient; electrothermal | L1; macromodels, power stages, self-heating |
| | Noise; transient sensitivities; PSS with multipliers | Precision specs; L2, L3 |

Until then, those analyses route to ngspice. A spec never fails to run because our simulator lacks a feature.

### 6.6 Model coverage and defaults

A vendor model that ignores temperature makes the temperature knob look irrelevant. So a **coverage map** records which knobs each model responds to (missing ones tag the verdict model-conditional); **standard wrappers** put knobs back (an op-amp error shell: Vos, Ib, CMRR, PSRR, Aol; MOSFET RDS(on) and Vth); and **sensitivity to un-toleranced model parameters** is reported (the buck's phase margin: 14.7° with assumed spreads vs 37.8° at typical). Region detection (gmu > 1e-4·gm) was checked only on a bare Gummel–Poon.

**Defaults the file doesn't write go into the export and the report** (plan §1.3). The model of a bare `Npn` (D-A) decides the CE amp's most informative verdict:

| `Npn` model (IS 1e-14, TNOM 25 °C) | bias max, `worst_case` | `sigma(3)` |
|---|---|---|
| **XTB 1.5** (D-A): β +0.5 %/K, as the walkthrough and the common Q2N3904 card | 6.5595 V: **FAIL** by 59.5 mV | 6.2813 V: PASS |
| XTB 0 (ngspice's default): β flat | 6.4572 V: PASS by 42.8 mV | 6.1839 V: PASS |

---

## 7. Hierarchy and composition

- **Contracts:** assumptions become range knobs; guarantees are specs (language §8). At every connection, upstream guarantee ⊆ downstream assumption.
- **Evidence ladder:** declared guarantees; characterized results (affine forms with shared knobs kept correlated: the first reason forms return, D-F); in-context simulation.
- **Cones keep blocks cheap:** a block's local specs enumerate their own cones inside a large design (§5.2).
- **Interface quantities go beyond a voltage range:** source impedance vs frequency, a ripple spectrum, start-up demand, and mode (a converter starting mid-ramp pushed the eFuse into current limit with 16.5 W in its FET).
- **Loading:** `z_src` / `z_load` assumptions, checked like any other.

---

## 8. When things run

| Era | Trigger | What runs | CE amp |
|---|---|---|---|
| **MVP** | `spicy check f.spl` (you or an agent) | The whole check of §5, cold. Report and run table stored under `.spicy/checks/<rev>.json` | 579 runs, 0.4 s |
| MVP | `--explain bias` | Reads the stored run table for this `rev` | < 10 ms |
| MVP | `spicy sim --at`, `spicy sweep` | A few real runs (`simulated`) | 1–5 ms |
| MVP | `--deep` | The check, then every UNDECIDED's `next` | seconds |
| **Editor** | Save or idle | The same full re-check; a what-if is a re-check | 0.4 s |
| Later | On demand | Transient specs, headline confidence only | 0.03 s per side here; minutes at 300 knobs |
| Later | Sign-off | Statistics (M7); the second backend | minutes |

**`rev`** hashes the elaborated design, knobs and contract, the engine settings and the backend identity; a record of another `rev` is `stale`.

**No instant tier.** v3 re-evaluated stored forms on every edit. A re-check costs less than an agent turn, and stored lines were badly wrong for real edits (§3.1). The editor may later replay stored worst points to show a FAIL at once, and carry verdicts whose cone an edit didn't touch (scale §7.2); neither is an estimate.

**Cost:** 579 runs on the CE amp (413 with `sigma(3)` only where it decides); at most 2^12 corners per side; the loop beyond, with no convergence guarantee.

---

## 9. Decisions

| # | Decision | Choice | Reason |
|---|---|---|---|
| D1 | Build or adopt | Build our own engine in Rust | Unchanged from v1 |
| D2 | Result format | **v4:** measures per run in one run table; affine forms deferred (D-F) | §3.1 |
| D3 | Main method for simulated specs | **v4:** enumeration per side (≤ 2^12 corners) + guards; the 3σ-point search; the loop past that | Exact at corners (§5) |
| D4 | Guards | **v4:** tangent + audit + pooling → ascent; the loop's own with the loop | §5.4, §5.8 |
| D5 | Knob kinds | range / statistical / mode / solver / calibration / layout- and firmware-owned | §2.1 |
| D6 | Provenance and shape | Required on every knob | §2.2–2.3 |
| D7 | State-dependent parameters | In the device model, not knobs | §2.6 |
| D8 | Exact methods | Datasheet arithmetic (M5) + corner theorem with checker | §4.1–4.2 |
| D9 | Verdicts | **v4:** D-E's words + reasons + tags | §3.3–3.4 |
| D10 | Yield | Board yield, distribution-robust, confidence-bounded (M7) | §4.4 |
| D11 | Default confidence | `sigma(3)` for user specs, `worst_case` for automatic checks | §3.6 |
| D12 | Simulators | **v4:** backend interface; ngspice first (D-B), L0; ours second | §6 |
| D13 | Measurements | Our own library, for every backend | §6.1 |
| D14 | External solvers (dReal, IBEX) | Still deferred | §4.8 |
| D15 | Build order | **v4:** plan §9 | §10 |
| **D-A** | Default model of a bare `Npn` | IS 1e-14, BF = β, XTB 1.5, XTI 3, EG 1.11, TNOM 25 °C, written out | §6.6 |
| **D-B** | Backend transport | libngspice in a worker process; `ngspice -b` as cross-check | §6.3 |
| **D-C** | Round 2's D1–D10 | As plan §10.1: exact map, σ per side, literal −3 dB, cold runs, both confidences, β 100..=300, no function backend, unstable JSON; nudges-vs-flips and the stop rule wait for the loop | plan §10.1 |
| **D-D** | M3's scope | No loop; over budget is UNDECIDED (budget) | §5.2 |
| **D-E** | The words | PASS (all corners / estimated / implied by worst case); UNDECIDED reasons; `simulated`, `stale`; no "verified" | §3.3 |
| **D-F** | Affine forms in M3 | None: per-run measures; contributors from the run table | §3.1 |

D-A to D-F were accepted on 2026-09-27 (plan §10).

---

## 10. Build order

Plan §9 has each step's done-when; M3a–M3e run on the suite's decks, so they don't wait for the language.

1. **M3a Design note:** types, the `Backend` trait, verdict tables, report schema, and the `EngineDeck` type M1f and M3b meet at. No `affine` module.
2. **M3b ngspice backend:** the worker, read-back, plausibility, restart; the `ngspice -b` path.
3. **M3c `worst_case`:** cones, measures, run table, enumeration, the inside-the-box guards, the band; the key harness.
4. **M3d `sigma(3)`:** the two-start search, range nudges, uniform spread, the ball search; the σ key harness.
5. **M3e Output:** records with `claim` and `next`; table and JSON; `--explain`, `--base`, `--at`, `--deep`; `sim`, `sweep`; the per-`rev` store.
6. **M3f End to end:** `spicy check circuits/ce_amp.spl` from the language front-end prints §5.7's table, equal to the key.

The language track owes D-A (M1e) and XTB/XTI/EG/TNOM, `.temp` and `.options` support before M1f (plan §5). **After M3** (plan §9.3): the loop, our simulator as second backend, the transient tier, statistics (M7), parts and datasheet arithmetic (M5), the editor and agent.

---

## 11. Open questions

From v3:
- The default distribution for "guaranteed by design" limits (untested limits don't screen parts: truncate them?); mode knobs × statistics (per-mode vs product yield).
- Electrothermal granularity (per part, or per board region with a θJA assumption); the default derating policy ("commercial" vs NASA / ECSS); how far guaranteed AC methods are worth taking.
- Catalog and part-data sources; reproducible builds (language §12).

From the plan (plan §12):
- Small interior regions away from the worst corner, and more than two basins at `sigma(3)`: should risky measures declare "interior possible"?
- Is the interior-peak rule worth its pessimism? It has never changed a verdict.
- Vendor models: region detection with terminal resistances; read-back inside subcircuits.
- Multi-stable circuits on ngspice: structural positive-feedback detection?
- The bracket rule rests on 20 σ results; re-check on op-amp and LDO circuits.
- The enumeration budget (4096 corners and 3 s, or time only?); automatic `--deep` under a second?
- Partial requests now (scale) or with packing (plan §6.3)?
