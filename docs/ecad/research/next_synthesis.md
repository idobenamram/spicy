# After the MVP: What the Next Circuits Need

> 2026-09-28 · Merges five reports that pushed post-MVP circuits through the current design (engine v4, `engine_plan.md`, `engine_types.md` M3a draft, language v0.1, `model.md`), on ngspice-42.
> Reports: `next_precision_analog.md` (**PA**), `next_power.md` (**PW**), `next_mixed_signal.md` (**MS**), `next_time_domain.md` (**TD**), `next_boards.md` (**BD**). Numbers come from those reports; each report has its scripts and citations.

## How to read this

The findings are split by the layer they hit: **language**, **elaboration and model**, **export**, **engine algorithm**, **engine types (M3a)**, **backend (ngspice)**, **report and agent**. Each entry has the same five parts:

- **Problem:** what's missing or wrong, in plain words.
- **Scenario:** a concrete circuit and spec where it bites, with numbers.
- **Impact:** *blocks* (the circuit can't be checked), *wrong verdict* (the engine could print a confident, false answer), *cost* (it works, too slowly), or *ergonomics*.
- **Where:** the doc section or type it hits.
- **Fix:** what to do. **Seam now** marks a change that is cheap in the M3a draft today and expensive to retrofit.

Findings that several reports hit are merged into one entry, with every scenario listed.

### The circuits behind the findings

| Report | Circuits |
|---|---|
| PA | OPA171 ×10 stage driving a cable (vendor model, phase margin vs load capacitance) · Sallen-Key low-pass, Q 1.3 · bridge + 3-op-amp in-amp with 2-point calibration (33 knobs) |
| PW | discrete LDO with Run/Sleep modes · synchronous buck (averaged and switching models) · PMOS load switch with inrush and a thermal network |
| MS | I2C bus with an optional plug-in module · 3.3 V ↔ 5 V logic (direct, and through a MOSFET shifter) · power-on reset with a supply ramp |
| TD | bridge rectifier + reservoir · 555 astable (TI vendor model) · discrete Schmitt trigger · two-transistor multivibrator |
| BD | CE amp placed twice (two-stage) · a supply block feeding it · a matched stereo pair · a 4-channel board (50 knobs) |

### The headline, in five lines

1. **Transient is everywhere after the MVP**, and the M3a types don't have it. Four of five reports were blocked by this.
2. **One test setup per check isn't enough.** CMRR, PSRR, loop gain, load steps and two-module I2C all need several benches per design.
3. **The engine can still print confident, false answers** in six new ways: interior dips, the wrong model level, an unstable DC solution, a missing crossing read as "unknown", a component that ignores temperature, and a knob with two targets.
4. **The 12-knob enumeration budget is hit fast.** Boards, in-amps, and anything transient need the pinned-node cone rule, a run-count budget, and eventually the loop.
5. **About fifteen seams are cheap now.** Four change a protocol or a stored format and shouldn't be deferred: the analysis list, a bench id, `Beyond`, and a run timeout.

---

## 1. Language

### L1. No way to write benches beyond the default one
- **Problem:** v0.1 has only the default bench: a DC source on each `Power<In>` and an AC 1 V source on each `Analog<In>`. There's no load, no second stimulus, no stimulus waveform, and no way to name a setup.
- **Scenarios:**
  - PA: an in-amp needs a differential and a common-mode stimulus. The default bench drives both inputs together and measures common-mode gain: 0.0005 instead of 2.0.
  - PW: LDO PSRR, loop gain and load-step each need their own setup.
  - MS: I2C with and without the plug-in module.
  - TD: the rectifier needs a mains sine source.
  - BD: each stage was checked unloaded. The product of the stage guarantees, 19.1–23.3, is far from the real two-stage gain of 12.27–13.93.
- **Impact:** blocks; wrong verdict (BD, PA).
- **Where:** language §8.5 (benches exist only as prose); roadmap §4.1 (not in the MVP); plan §1.3 (default bench).
- **Fix:** named benches in the language (the "form bench" of language §8.5); `spec … on <bench>`. Engine side: see T2.

### L2. No transient measures, and no measure axes
- **Problem:** v0.1 has `dc()` and `ac()` only. There's no `tran()`, no `.peak()`, `.settle()`, `.freq()` or `.duty()`, and no way to say "over turn-on phase" or "per cycle".
- **Scenarios:**
  - TD: rectifier ripple, and surge vs turn-on phase (the worst is at 90°, 30.71 A against a 30 A rating).
  - TD: 555 frequency and duty.
  - PW: load-step undershoot.
  - MS: I2C rise time and reset delay.
- **Impact:** blocks.
- **Where:** language §8.4; roadmap §4.1.
- **Fix:** a transient measure family, and measure axes (a knob or bench variable swept inside the measure, such as the turn-on phase).

### L3. No discrete choices: modes, variants, part options
- **Problem:** knobs are continuous ranges only.
- **Scenarios:**
  - PW: LDO phase margin is 92.9° at 300 mA and 32.5° in sleep, so the mode changes both the load range and the verdict.
  - MS: a plug-in module present or absent; 74HC vs 74HCT vs 74LVC.
- **Impact:** blocks; wrong verdict if a mode's range is folded into one knob.
- **Where:** language §5.4 (`assume mode in {Run, Sleep}` is sketched, not in v0.1); `model.md` E16.
- **Fix:** discrete knobs in the language, and conditional assumptions ("in Sleep, load ≤ 1 mA"). Engine side: E5.

### L4. Sub-block contracts are silently dropped
- **Problem:** only the top contract is analyzed. A placed block's specs and `assume`s vanish without a word.
- **Scenario:** BD, the two-stage amp: stage 1's bias spec and stage 2's input assumption are never checked.
- **Impact:** wrong verdict (a silent omission).
- **Where:** `model.md` E24; language §8.8.
- **Fix:** keep every contract instance with its placement path (M1). Until composition exists, at least warn "sub-contract not checked" (R3).

### L5. Part kinds beyond the MVP
- **Problem:** v0.1 has Resistor, Capacitor, Electrolytic, Npn, Pnp.
- **Scenarios:**
  - PA: op-amp (a subcircuit).
  - PW: MOSFET, inductor, switch, IC subcircuit.
  - MS: switch, logic gate, comparator.
  - TD: diode, 555.
- **Impact:** blocks.
- **Where:** roadmap §4.1; language §6.
- **Fix:** a subcircuit-backed part kind first (it covers op-amps, the 555 and ICs), then diode, MOSFET and inductor. Each needs a default model (see B4).

### L6. Current probes, loads and power are missing
- **Problem:** there are no current probes (`q1.c.i`) or power probes in v0.1, and no way to say "this output drives this load".
- **Scenarios:**
  - PW: inrush current and junction temperature.
  - TD: diode surge current.
  - BD: the stage-to-stage loading.
- **Impact:** blocks.
- **Where:** language §8.4 (designed, not in the MVP subset).
- **Fix:** add them with the transient family.

---

## 2. Elaboration and model

### M1. Contracts as instances with paths
- **Problem:** `FlatContract` is one flat list, so spec names collide when a block is placed twice (`bias` and `bias`).
- **Scenario:** BD, the 4-channel board: 36–76 sides with colliding names.
- **Impact:** ergonomics; wrong verdict if two sides merge.
- **Where:** `model.md` E24; `engine_types` §2.3 (`Side::spec` is a bare name).
- **Fix:** **seam now:** `SpecPath` (the placement path plus the spec name) in `Side` and `Record`.

### M2. Knob provenance, owner and lots
- **Problem:** a knob doesn't record where its range came from (tested limit, typical, assumed), who owns it (layout, firmware), or what group it belongs to.
- **Scenarios:**
  - MS: datasheet IC limits get the invented truncated normal, so `sigma(3)` can PASS on a distribution nobody measured.
  - MS: layout owns the bus capacitance, so the useful output is a constraint ("≤ 121.1 pF"), not a verdict.
  - PA/BD: matched networks and a stereo pair need lot knobs.
- **Impact:** wrong verdict (MS); blocks (lots).
- **Where:** `model.md` E16; `engine_types` T2 (`Policy`); engine.md §2.2, §2.5.
- **Fix:** **seam now:** `KnobSpec.origin` and `provenance` fields, and a `group` field for lots. `sigma(k)` refuses (UNSPECIFIED) on knobs without a tested two-sided limit.

### M3. A knob with several targets, or an expression target
- **Problem:** a knob binds to exactly one deck parameter.
- **Scenarios:**
  - PW: `temp` drives both the circuit temperature and the thermal network's ambient source. Read-back checks only one of them.
  - PA: tempco links (a resistor's value = nominal × (1 + tc × (T − 25))).
- **Impact:** wrong verdict (a binding that's half-checked).
- **Where:** plan §5.1 (`KnobBinding`); `engine_types` §4.
- **Fix:** **seam now:** a binding holds a list of targets, each with an expression over knobs; read-back checks every target.

### M4. Log-scaled knobs and nominals off the midpoint
- **Problem:** every knob maps as `x = mid + half·ε`.
- **Scenario:** PA: load capacitance over 0–100 µF. A linear midpoint of 50 µF is meaningless; the plan's inward nudge lands at 500 nF and misses the 944 pF dip.
- **Impact:** wrong verdict (feeds E1).
- **Where:** `engine_types` §2.1.
- **Fix:** **seam now:** `KnobSpec.scale: Linear | Log`, and a real nominal (the field exists; the map should use it).

---

## 3. Export (the M1f contract)

### X1. Vendor models: includes, dialect, start conditions
- **Problem:** the engine deck is one self-contained text with R, C, NPN and PNP.
- **Scenarios:**
  - PA: TI's OPA171 and TL072 need `.include` and PSpice compatibility mode.
  - TD: the TLC555 needs the same, plus a `uic` start.
- **Impact:** blocks.
- **Where:** plan §5.1 (`EngineDeck`).
- **Fix:** **seam now, before M1f is built:** `EngineDeck` gains `includes` (hashed into `rev`), `dialect`, and `start` (operating point or `uic` with initial conditions).

### X2. One deck per bench
- **Problem:** the deck has the default bench baked in.
- **Scenarios:** every L1 scenario.
- **Impact:** blocks.
- **Where:** plan §5.1; §1.4.
- **Fix:** M1f exports one deck per bench, all sharing the same knob map. See T2.

---

## 4. Engine algorithm

### E1. Interior worst cases that the guards miss
- **Problem:** the inside-the-box guards (a tangent nudge at the worst corner, 8 audit points, pooling) miss a narrow dip away from the worst corner, and a worst case that lives on a measure axis.
- **Scenarios:**
  - PA: phase margin vs load capacitance. The corners read 81.9° and 83.9°, the true worst is 47.6° at 944 pF, and the engine prints **PASS (all corners)**.
  - TD: inrush vs turn-on phase. The corners give 10.29 A, 90° gives 30.71 A over a 30 A rating. The audit caught this one (100 of 100 seeds), but `sigma(3)` can only say UNDECIDED.
- **Impact:** **wrong verdict.**
- **Where:** plan §2.5 (the "not covered" case it names), §12 q. 2.
- **Fix:** **seam now:** a measure kind (`smooth` / `kink` / `interior possible`). Measures of known risk (phase margin vs C, surge vs phase, resonances) declare `interior possible` and get a real 1-D search along the risky knob (E4) instead of a nudge. Log knobs (M4) make that search sensible.

### E2. Which model level a verdict rests on
- **Problem:** a verdict from a simplified model is printed as if it held for the real circuit.
- **Scenario:** PW, the buck's 150 mV undershoot. The averaged model says PASS (all corners), worst 143.1 mV over 1,024 corners. The switching model fails at 16 of 2,048 corners, worst 157.9 mV. Re-running the averaged worst corner at switching level, at both f_sw edges, catches it (157.5 mV).
- **Impact:** **wrong verdict.**
- **Where:** engine.md §5.9 (the analysis ladder); plan W6.
- **Fix:** **seam now:** a `Level` on requests and records. A PASS says which level it rests on, and the worst corners are re-verified one level up (engine.md §5.9's rule, made concrete).

### E3. An unstable DC solution, reported precisely
- **Problem:** a cold operating point can land on an unstable equilibrium, and the engine takes it as real.
- **Scenario:** TD, the Schmitt trigger. Inside the hysteresis band, `op` returns 2.497 V; the stable states are 1.855 V and 5.000 V. The value varies smoothly with the knobs, so enumeration prints a confident verdict about a state that doesn't exist.
- **Impact:** **wrong verdict.**
- **Where:** engine.md §4.7; plan §12 q. 5.
- **Fix:** history as bench data (a ramp that says which state the circuit is in), and a stability check or UNDECIDED (multi-stable) when positive feedback is detected. Soon after the MVP; flag it in the language for now.

### E4. The 12-knob budget, and what goes past it
- **Problem:** the budget is set per side on the cone size and on 3 s of measured wall time.
- **Scenarios:**
  - BD: every DC side of the 4-channel board has a 42-knob cone under the capacitor rule, so all are UNDECIDED (budget). The two-stage bias fits by 30 ms (2.97 s vs 3 s).
  - BD: the two-stage AC sides have 14 knobs: 16,384 corners, 11.9 s.
  - PA: the in-amp has 33 knobs; even a 12-knob subset is 12,288 runs. The slopes at nominal pick the wrong corner, and one flip fixes it.
  - TD/PW: transient sides of 5–10 knobs miss 3 s (rectifier 3.11 s, 555 ≈ 14 s, multivibrator ≈ 83 s, switching buck's key 551 s).
  - PW: resistors in series with capacitors stay in the DC cone: 13 knobs where 11 would enumerate in 1.8 s.
- **Impact:** blocks; also **nondeterminism**: a wall-time rule makes a verdict depend on the machine's speed and worker count (TD, PW).
- **Where:** plan §2.3; `engine_types` §2.3 (the cone rule).
- **Fix:**
  - **seam now:** the budget as a **run count** per analysis level, never wall time;
  - **seam now:** the pinned-node / topological cone rule (BD estimates ≈ 150 lines; it cut the board's cones to the local stage);
  - the loop comes right after M3 for boards, in-amps and transients. PA, BD and TD all put it first in their domain.

### E5. Modes and scenarios
- **Problem:** there's no place for "this spec, in this mode, over this range".
- **Scenario:** PW, the LDO: phase margin is 61.2° at 1 mA in Run and 32.5° in Sleep. One merged range would hide which mode fails.
- **Impact:** wrong verdict, blocks.
- **Where:** plan §2; `engine_types` §2.3.
- **Fix:** **seam now:** `KnobKind::Mode` (discrete levels, enumerated), and a `Scenario` in the plan: mode, range overrides, bench.

### E6. "Missing crossing" read as unknown when it's a FAIL
- **Problem:** `Undefined` always becomes UNDECIDED.
- **Scenarios:**
  - PW: the LDO output is still 3.43 V 3 ms after load release (it never settles); a load switch on a long cable never reaches 90%.
  - TD: the vendor 555 stops oscillating at 60 °C.
- **Impact:** **wrong verdict** in the soft direction: a real FAIL shows as UNDECIDED. There's also no verdict row for `Undefined` at all (TD).
- **Where:** `engine_types` T7/T8; plan §4.6; round 2's C9.
- **Fix:** **seam now:** `Measured::Beyond { side, limit }` ("didn't settle within the window" is Beyond on the failing side, so FAIL), a typed `Undefined { reason }`, and a verdict row for each.

### E7. Calibration: a measure over several runs
- **Problem:** `Program::eval(&Run)` reads one run.
- **Scenario:** PA, the bridge with 2-point calibration: the error after calibration needs the same board at zero load, at full load, and at the measured point.
- **Impact:** blocks. It turns out affine forms aren't needed for this (D-F stands).
- **Where:** `engine_types` T7.
- **Fix:** **seam now:** a measure can read several runs that share one board (same statistical knobs, different bench or range values).

### E8. Datasheet arithmetic: specs with no simulation
- **Problem:** every measure needs a simulator run, and measures can't read knob values directly.
- **Scenario:** MS, direct 3.3 V → 5 V logic: VOH(min) vs VIH(min) from datasheet tables with supply bands and gaps. No simulation at all.
- **Impact:** blocks.
- **Where:** engine.md §4.1 (planned for M5); `engine_types` T7, T3.
- **Fix:** **seam now:** a `Program` node that reads a knob, and runs that need no simulator. Tables with gaps are M5, but UNSPECIFIED must exist for them (T5).

### E9. Numerical band and noise for transients
- **Problem:** the band stage tightens reltol, but transient error comes from time-step control.
- **Scenarios:**
  - MS: the reset delay moves 60 µs, not monotonically, as the step changes.
  - TD: on the 555, the σ search's nudge gives +0.336 Hz where the true effect is −0.084 Hz: the wrong sign.
- **Impact:** wrong verdict (at `sigma(3)` especially).
- **Where:** plan §2.7, §1.3.
- **Fix:** **seam now:** tolerance profiles per analysis kind (transient: a step setting, not reltol 1e-9), and a noise floor per measure that sets the nudge size or forbids nudging.

---

## 5. Engine types (M3a)

### T1. Analyses: a list, with transient and DC sweep
- **Problem:** `AnalysisSet { op, ac_points, sweep }` and `Run` have no transient or DC sweep.
- **Scenarios:** every transient scenario above; PW's DC sweep for line regulation.
- **Impact:** blocks (in four of five reports).
- **Where:** `engine_types` §4, §4.2, T3/T4.
- **Fix:** **seam now:** `Needs.analyses: Vec<AnalysisSpec>` (`Op`, `Ac { excitation, points | sweep }`, `Noise`, `Tran { stop, step, window }`, `DcSweep`), and results per analysis in `Run`. A transient's stop time can be given in cycles of a knob (TD).

### T2. A bench id on every request
- **Problem:** nothing records which setup a run used, so two benches at the same knob point would dedup into one row.
- **Scenarios:** every L1 and X2 scenario.
- **Impact:** blocks; wrong verdict (rows from the wrong bench).
- **Where:** `engine_types` §4, §6 (the run-table key).
- **Fix:** **seam now:** `BenchId` in `Request` and in the run-table key. Each AC job carries its own excitation (PA).

### T3. Device records for every device kind
- **Problem:** `DeviceOp` has BJT fields only.
- **Scenarios:** PW (MOSFET margins, VDMOS thermal), TD (diode current), MS (the shifter's MOSFET).
- **Impact:** blocks the guards on those circuits.
- **Where:** `engine_types` §4.2.
- **Fix:** **seam now:** `DeviceOp` as an enum by device kind.

### T4. Where the extreme happened
- **Problem:** a measured value doesn't say at what time or frequency.
- **Scenario:** TD, "worst surge at t = 8.3 ms, at 90° turn-on". A waveform counterexample needs its time.
- **Impact:** ergonomics; it also feeds E1's axis search.
- **Where:** `engine_types` T7.
- **Fix:** `Measured` carries the location of an extremum.

### T5. Restore UNSPECIFIED
- **Problem:** the M3a verdict enum dropped it, as "later".
- **Scenario:** MS, the shifter: one driver sinks both pull-ups, 3.83 mA, where the datasheet row stops at 3 mA. The 0.477 V low level is outside what the datasheet covers, not a clean FAIL.
- **Impact:** wrong verdict (it becomes a FAIL or a PASS it isn't).
- **Where:** `engine_types` §8.
- **Fix:** **seam now:** `Verdict::Unspecified(reason)`, even if M3 never produces it.

### T6. Requests carry the cone they care about
- **Problem:** requests are full knob points, so runs can't be shared across sides later.
- **Scenario:** BD, the board's local sides: 8× the runs. One run served all 8 bias cones at once, 128 runs.
- **Impact:** cost at board scale.
- **Where:** plan §6.3 (the "scale disagrees" row); `engine_types` §4.
- **Fix:** **seam now:** `Request.care: ConeId`, unused in M3.

---

## 6. Backend (ngspice)

### B1. A per-run timeout
- **Problem:** the worker waits forever.
- **Scenarios:**
  - TD: the repo's floating rectifier deck hung ngspice for over 3 minutes.
  - PW: a misspelled VDMOS model line hangs it indefinitely.
- **Impact:** blocks (a hung check).
- **Where:** `engine_types` §4.1, §10.3.
- **Fix:** **seam now:** `RunError::Timeout`; the pool kills and restarts the worker.

### B2. The plan's tolerances break transients
- **Problem:** `abstol=1e-15` and the band's reltol 1e-9 were tuned on an operating point.
- **Scenario:** TD, the rectifier aborts at 46.6 ms with `abstol=1e-15`. The reltol 1e-9 re-run aborts, or costs 15× when it completes.
- **Impact:** blocks.
- **Where:** plan §1.3, §2.7, §4.5.
- **Fix:** the tolerance profiles of E9.

### B3. Read-back traps
- **Problem:** read-back assumes the value it reads is the one the knob set.
- **Scenarios** (all MS):
  - a failed ngspice `let` silently returns the previous value;
  - a variant's unused branch reads back 1e15 Ω and would stop every check;
  - a MOSFET's instance Vth is temperature-adjusted, so it never equals the knob.
- **Impact:** wrong verdict, silently; or a false binding stop.
- **Where:** plan §4.4; `engine_types` §5.3.
- **Fix:** **seam now:** a read-back kind per binding (exact, model parameter, temperature-adjusted, skipped with a reason); clear the vector before each `let`.

### B4. Default models don't fit transients
- **Problem:** the D-A default transistor has no junction capacitances, and the default diode is bare.
- **Scenario:** TD, the multivibrator: its transient aborts at engine tolerances, and takes 3.2 s per run at ngspice's defaults, against 81 ms with capacitances. The default diode moves the rectifier's minimum output by 1.43 V.
- **Impact:** blocks (transient), wrong verdict (diode).
- **Where:** plan §1.3, D-A.
- **Fix:** default models get junction capacitances before the transient tier; the report's `not_modeled` already names the gap.

### B5. Vendor models are hostile
- **Problem:** real vendor models misbehave in ways the engine can't see.
- **Scenarios:**
  - PA: the OPA171 finds no DC point at any input but 0 V (every convergence aid failed).
  - PA: it ignores temperature (offset 2.4400 mV at −40, 25 and 125 °C).
  - PA: its noise is 26× too high, because PSpice mode drops a parameter silently.
  - TD: the TLC555 stops oscillating at 60 °C.
  - PW: VDMOS self-heating failed its operating point in 2 of 3 decks.
- **Impact:** blocks; **wrong verdict** (temperature knobs that do nothing).
- **Where:** engine.md §6.6 (model coverage).
- **Fix:** **seam now:** a backend `qualify()` step run once per model (does it converge across the box, does it respond to temperature, is its noise sane), whose findings go into `not_modeled` and the tags.

---

## 7. Report and agent

### R1. What a verdict rests on
- **Problem:** a verdict doesn't say which assumptions, model level or stub it depended on.
- **Scenarios:**
  - BD: verdicts that use a supply block's guarantee instead of the real supply.
  - PW: the model level (E2).
  - MS: undischarged layout assumptions.
- **Impact:** wrong verdict by omission; the agent can't be honest without it.
- **Where:** `engine_types` §9.
- **Fix:** **seam now:** `Record.rests_on` (assumptions, level, stubs, waivers) and a per-side cone fingerprint.

### R2. Board-level reading of per-side verdicts
- **Problem:** 50 sides that each pass at 3σ don't bound the board.
- **Scenario:** BD, 50 sides at 3σ allow up to 6.75% of boards to fail (union bound).
- **Impact:** misread.
- **Where:** plan §3; engine.md §4.4 (board yield is M7).
- **Fix:** a board summary line with the union bound until M7.

### R3. Silent omissions
- **Problem:** sub-contracts not checked (L4), transient specs skipped, `not_modeled` gaps.
- **Scenario:** BD, the two-stage amp prints only top-level verdicts.
- **Impact:** wrong verdict by omission.
- **Fix:** the report lists everything it did **not** check.

### R4. Output the constraint, not only the verdict
- **Problem:** a layout-owned knob's useful answer is a limit.
- **Scenario:** MS, the I2C bus: layout may add up to 121.1 pF of trace capacitance with the module fitted.
- **Impact:** ergonomics (the engine knows it and doesn't say it).
- **Fix:** PASS (conditional) with the exported constraint (engine.md §3.3, already designed).

---

## 8. The seams to add to M3a now

Collected from the entries above. **Bold** = changes a protocol or a stored format, so it shouldn't be deferred (PW's and TD's point).

| Seam | Entry | Cost in the M3a draft |
|---|---|---|
| **`Needs.analyses: Vec<AnalysisSpec>` with Tran, DcSweep, Noise; results per analysis** | T1 | an enum and a `Vec` |
| **`BenchId` on requests and in the run-table key; an excitation per AC job** | T2, X2 | one field, one key part |
| **`Measured::Beyond` + typed `Undefined`, with verdict rows** | E6 | one variant, two rows |
| **`RunError::Timeout` and a per-run timeout in the protocol** | B1 | one variant, one frame field |
| **`EngineDeck` includes, dialect, start condition (before M1f)** | X1 | three fields |
| `SpecPath` in `Side` and `Record` | M1 | one type |
| `KnobSpec` origin, provenance, group; `scale: Linear \| Log` | M2, M4 | four fields |
| Knob bindings with several targets and a read-back kind | M3, B3 | a `Vec` and an enum |
| `KnobKind::Mode` and a `Scenario` in the plan | E5 | a variant and a struct |
| `Level` on requests and records | E2 | one field |
| Measure kind (smooth / kink / interior possible) | E1 | one enum |
| Multi-run measures; knob reads in `Program`; simulator-free runs | E7, E8 | two `Program` nodes |
| Tolerance profiles per analysis kind; noise floor per measure | E9, B2 | a map |
| `DeviceOp` by device kind; extremum location in `Measured` | T3, T4 | an enum, a field |
| `Verdict::Unspecified` restored | T5 | one variant |
| `Request.care: ConeId` | T6 | one field |
| Budget as a run count per level; the topological cone rule | E4 | a rule change; ≈ 150 lines |
| `Record.rests_on`; `qualify()` on the backend | R1, B5 | a field, a trait method |

---

## 9. The circuit ladder after the MVP

Each report proposed the first circuit in its domain. The order below puts them by what they need, cheapest first.

| # | Circuit | Proves | Needs |
|---|---|---|---|
| 1 | **Sallen-Key filter** (PA) | an op-amp as a subcircuit part, new AC measures, while staying under 12 knobs | L5 (subcircuit part), X1, a Q/f0 measure |
| 2 | **Two-stage CE amp** (BD) | block reuse, per-placement knobs, loading, sub-contracts | M1, L4, E4 (topological cones); an exact key in 14 s |
| 3 | **LDO** (PW) | benches, modes, a cheap transient from the DC point | L1, L3, T1, T2, E5 |
| 4 | **I2C bus** (MS) | the transient tier with a free exact answer key (the RC formula), variants, layout constraints | T1, T2, L3, R4 |
| 5 | **Rectifier steady state** (TD) | long transients, the transient band, the diode default, cycle windows | T1, B2, B4, E9 |
| then | the loop (in-amp, boards), the buck's two levels, the Schmitt trigger's history, vendor 555 | | E2, E3, E4, B5 |

---

## 10. Open questions for the walkthrough

1. **Which seams go into M3a now:** all of §8, or only the five bold ones?
2. **Should D5 (f_low = −3 dB below the peak) change?** PA's Sallen-Key gives 1188 Hz by D5 and 1389 Hz from the DC gain, with f0 at 1000 Hz. A named reference (`f_low(-3dB, ref: dc)`) may be needed.
3. **Should D-A's default transistor gain junction capacitances now,** so the MVP's model is the transient one too (B4)? It doesn't change the MVP's DC and mid-band numbers much, but that needs checking.
4. **The run-count budget (E4): what number?** A count per level replaces the 3 s wall-time clause.
