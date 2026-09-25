# Research Synthesis: Round 1

> 2026-09-25 · Seven research agents stress-tested the engine and designed the language.
> The seven full reports sit next to this file. This document is the merged result: what holds, what changes, and what you need to decide.
> **Decisions (2026-09-25):** all recommendations in §4.3 accepted. Results: `../engine.md` and `../language.md`.

| Report | Focus |
|---|---|
| `engine_power.md` | Buck/boost, LDO, eFuse, ideal diode, charger (6 TI datasheets, averaged + switching + PSS models) |
| `engine_precision_analog.md` | Op-amp stages, filters, bridges, ADC drivers, references (8 experiments) |
| `engine_mixed_signal.md` | MCU boards: logic levels, I2C/SPI timing, crystals, latches, supervisors |
| `engine_method_redteam.md` | Every assumption attacked; commercial tools surveyed; 25 claims checked against sources |
| `language_core.md` | Items, ports, nets, units, knobs, generics, type system |
| `language_specs.md` | Contracts, assumptions, specs, benches, measures, automatic checks |
| `language_editor_mapping.md` | Canonical model, drawn vs coded, sidecar, auto-placement, KiCad, vim modes, AI |

---

## 1. The verdict in one paragraph

**The core of the engine holds.** The worst-point loop is the same method structural-reliability engineering calls FORM, and it's what MunEDA WiCkeD and Cadence's variation tools ship. On the walkthrough amplifier it matches brute force (it predicts 0.98% failing; Monte Carlo gives 0.87–1.09%). The range/statistical split is confirmed verbatim in Graeb's textbook.

What real circuits add is everything **around** the loop:
- circuits that switch **modes** (current limit, clipping, dropout);
- **knobs** with far more structure than "±tol";
- **models** that don't carry the knobs at all;
- circuits with **more than one DC solution**.

And one blocker dominates: **our simulator can't run most real circuits yet.** It has no controlled sources, MOSFETs, switches or op-amps, and no variable time step.

---

## 2. Corrections already made to the docs

The agents checked our claims against the sources. These are fixed in v2, the walkthrough, the spec design and the bibliography:

| Claim | What was wrong | Now |
|---|---|---|
| "3σ = 99.9% of boards" | One-sided 3σ is **99.865%** | Fixed everywhere |
| "The simulator is only good to 0.1%" | DC Newton converges quadratically, so it's good to about 1e-6–1e-5 | The real issues are `abs_tol` applied to currents and fixed-step transient (v2 Part D) |
| The corner theorem holds for "any element value" | Only for knobs entering as **one rank-1 term**, with the circuit solvable over the whole box. **Not** for power (P = V·I peaks inside the box), transformer ratios, pot wipers, loop gain, or state-dependent parts | Conditions added (v2 A5) |
| "Tempco, drift and DC bias are range terms" | Temperature is a range knob, but a part's tempco has a random sign (statistical). Full-strength stacking overstates budgets (2968 vs 1817 ppm). DC bias belongs in the device model, not a knob | Slicing rule (v2 A7) |
| ECSS states "biased linear + random RSS" | Not in the ECSS text; that's RAC WCCA practice. ECSS splits by *known direction*, not by *must hold everywhere* | "Similar to, not the same as" |
| Maxim "4.7 µF → 0.33 µF" as typical | An extreme case (Y5V 0603 at 5 V on a 6.3 V part) | Context added |
| Vishay "±3% at end of life" | 8000 h is a stress test; no 10-year figure exists | Reworded |
| "5–8 simulations per spec regardless" | Only with adjoint sensitivities, DC/AC | Qualified |
| N&P07 cost "20–25× one solve" | That figure is at 420 parameters, and it grows to ~1,200× at 10k; monotonicity proofs "typically fail in high dimensions" | Fixed |

The agents also caught what "yield" really depends on. The walkthrough's f_L spec fails **0.9%** under our default spread, **11.6%** uniform, **5%** with an offset reel, and **32%** with culled parts. That caution is now in walkthrough §6.

---

## 3. Engine: what changes

Ranked by impact, merged across the four engine reports.

### E1. The simulator is the #1 blocker

All three circuit agents hit it. Merged order:

1. **Controlled sources (E/G/F/H) and behavioral B-sources.** They unlock op-amp models and IC behavioral models.
2. **MOSFET, voltage-controlled switch, comparator/logic primitives.**
3. **Robust DC:**
   - junction limiting;
   - gmin and source stepping;
   - errors instead of panics;
   - separate voltage and current tolerances;
   - an engine mode with reltol ≈ 1e-6.
4. **AC at the operating point**, sparse complex (KLU), with a loop-gain probe.
5. **Variable-step transient** with breakpoints and event location. The fixed-step Backward Euler we have can make unstable states look stable.
6. **Temperature** (tc1/tc2 are parsed but unused), then self-heating.
7. **Noise analysis.**
8. **Periodic steady state (shooting)** with stability multipliers, for switching converters.
9. **Sensitivities:** adjoint for DC/AC, and crossing-time sensitivities for transient.

### E2. A safety net against confident false PASSes

The red-team's most important finding. A limiter that's *off* at nominal (clipping, dropout, current limit) has zero slope, so the loop never pushes toward it. Then "prediction ≈ simulation" wrongly looks like proof.
- **Example:** a THD spec the loop reports as 0.0026% PASS is really 3.29% FAIL at a corner.
- **Power example:** the buck at its worst corner hits current limit and the output drops 11%. At nominal it's invisible.

Fixes:
- **Always** simulate every combination of the range knobs (one run serves every spec), and scan temperature at 3–5 points.
- Multi-start the loop. Remember visited corners, and test flipping several knobs together (the buck needed three flipped at once).
- **Regime guards:** every device model reports its margin to the next regime (headroom, dropout, VCE,sat, current limit), and every simulated point records each device's operating region.

### E3. Modes are first-class

Examples: power states, PWM/PFM/current-limit, boot and reset state, bus driver, firmware settings.
- They become **discrete knobs**.
- Every simulation reports which mode it ended in.
- Specs can be scoped to a mode.
- Automatic specs check the mode boundaries, e.g. "peak inductor current < minimum current limit".

### E4. A much richer knob model

This came from every engine agent. A knob carries:

| Field | Values |
|---|---|
| **Kind** | range · statistical · solver-chosen · **calibration/trim** · **layout-owned assumption** (bus capacitance, θJA) · **firmware-owned assumption** (watchdog kick period) |
| **Provenance** | tested limit · guaranteed by design · **typical-only** · missing |
| **Shape** | linear slope · envelope over temperature (datasheet "over temp" limits) · box (reference tempco specs) · aging law (linear / √t / ∛t) · hysteresis · table indexed by supply band and temperature, with gaps |
| **Distribution** | Default truncated normal. Yield is always also reported under uniform, and the verdict is UNDECIDED if the two disagree |
| **Structure** | lot knob + per-part knob; **constraints between limits** (a 74HC14's threshold box alone would allow −0.3 V hysteresis; the NE555's thresholds aren't independent) |

Tempco-like effects are **statistical coefficient × range variable**, handled by slicing over the range knob.

### E5. Parameters that depend on the circuit's state belong in the device model

Capacitance vs DC voltage, inductance vs current, and RDS(on) vs junction temperature are evaluated at the operating point inside the device model; they aren't knobs.

Self-heating needs an **electrothermal fixed point**. The buck reaches 153–196 °C junction at 85 °C ambient, depending on θJA. Thermal runaway is reported as FAIL.

### E6. Honesty about models

Vendor op-amp and IC models are usually typical-only at 25 °C. Through such a model the temperature knob has zero effect, and the engine would silently say "temperature doesn't matter".

- A **model coverage map**: flag every knob the model doesn't respond to.
- **Standard wrappers:** an op-amp error shell (Vos, Ib, CMRR, PSRR, Aol) around vendor models, and a native parametric op-amp.
- Every verdict is tagged **model-conditional**.
- A new verdict, **UNVERIFIABLE (model)**, for things like settling tails and −115 dB THD that no available model can speak to.

### E7. An analysis ladder

Power specs need four levels: formulas → averaged model → PSS → full switching transient.
- The loop **searches on the cheap level** and **verifies the worst point one level up**.
- The gap between levels is carried as a named error term. (Averaged vs switching undershoot: −168 vs −218 mV.)

### E8. A datasheet-arithmetic engine

About **two-thirds of board-level digital specs need no simulation**:
- logic levels;
- I2C pull-up and rise-time windows;
- SPI/I2C timing budgets;
- supervisor and watchdog windows;
- crystal gm_crit;
- ADC source impedance;
- pin currents.

Every one is monotone in every knob, so the corner method is **exact in about 2 evaluations**. This runs on every edit. It's where the open tools are weakest:
- atopile's logic type has no thresholds, and its I2C check is effectively inert.
- PolymorphicBlocks' plain intervals report a false −1.35 V failure where the true margin is +0.036 V, which is the dependency problem again.

### E9. Circuits with several stable states

Latches, hysteresis and start-up break "one DC solution". A copy of our Newton solver lands on the unstable midpoint of a latch, and the DC sweep panics at a hysteresis threshold.

- Classify the stability of each DC solution.
- Search for multiple solutions, and use continuation for thresholds.
- A spec on a multi-stable block must say which history it assumes (e.g. a supply ramp time).

### E10. Worst cases inside the box are common

Examples:
- op-amp phase margin vs capacitive load: worst at 575 pF, mid-range;
- Chebyshev ripple peaks;
- the crystal's negative resistance, which peaks at a finite gain.

So the worst-point step becomes a **box-constrained optimization** that allows interior values, and for-all-frequency specs track **each local extremum** separately.

### E11. Statistics done right

- **Board yield, not per-spec yield at one corner:** a board must pass every spec at every condition. One test: 26.7% loss reported vs 36.8% true. Board yield can be computed from the stored per-spec lines at zero extra simulations (Schenkel et al. 2001).
- **Importance sampling** centred on the loop's worst point: 2,000 runs gave the precision of 20,000 plain Monte Carlo runs.
- **Clopper–Pearson stopping**, with three-valued yield verdicts (Cadence does this).
- **Joint search** over range and statistical knobs (Solido PVTMC style). The worst corner at nominal parts isn't the worst at 3σ.

### E12. New concepts

- **Calibration.** Precision specs are judged after calibration. With named-knob affine forms, calibration is an **exact subtraction**. A load-cell budget goes from all error at 25 °C to 3479 ppm worst case at 85 °C, and putting the gain resistors in one network cuts the dominant term from 2968 to 237 ppm. This is a strong new argument for affine forms.
- **Noise** is its own kind: it varies over time *within* one board. Specs need a per-reading form (static error + z·σ_noise).
- **Error budgets:** generate the familiar spreadsheet from the affine forms, with hand-entered rows and import/export. Engineers already think this way.

### E13. More verdicts

- PASS (guaranteed)
- PASS (estimated)
- **PASS (conditional on a layout assumption)**, e.g. "PASS given bus capacitance ≤ 234 pF". Exported as a layout constraint.
- FAIL
- UNDECIDED
- **UNVERIFIABLE (model)**
- **UNSPECIFIED** (the datasheet has a gap there)
- A **relies-on-typical** flag on any of the above

### What stays as it was

- The worst-point loop core.
- Range vs statistical knobs.
- Affine forms as the common format (now with calibration as a second strong reason).
- Inner/outer brackets and counterexamples.
- Monte Carlo at sign-off.
- The corner theorem for linear pieces, with a strict applicability checker.

---

## 4. Language: the merged proposal

### 4.1 Where all three language agents agree

- **Standalone Rust-like language.** A few new keywords; everything else is Rust syntax with Rust meaning.
- **Every instance is named:** `let name = Type { … };`. Names are identity: paths like `board.amp.r1`, `prot.r_s[3]`. No UUIDs in code.
- **No chain syntax, no global nets, no action at a distance.** Ground is an explicit port.
- **Physical units are types**, checked at compile time (`Volt * Amp == Watt`).
  - Prefixes are case-sensitive: `m` is milli, `M` is mega, and `1Meg` gets a fix-it.
  - Tolerance is written `±` or `+/-`. `+-` is rejected, because in Rust it means `+ (-…)`.
  - Closed ranges are `..=`.
- **⊆ vs ⊇ is always visible.** A range after `in` / `<=` in `assume` or `spec` is a requirement. A range in a value position (`47k ± 1%`, `beta: 100..=300`) is a spread.
- **Order-independent block bodies**, with `///` doc comments as rationale (they show on the part's value card).
- **Build-time `for` / `if`** with indexed names (`let r_s[i] = …`). Structure may depend only on exact values (no `if` on a toleranced value). `where … else "msg"` comes from Spade.
- **Drawable = the "flat subset" of the language** plus a geometry-only sidecar. No graphics in the language. Delete the sidecar and nothing semantic changes.
- **Roles are typed**, e.g. `Power<In>`, `Logic<OpenDrain>`, `I2c<Controller>`. They map to KiCad pin types and to symbol sides.
- **Three tiers of checks:** the type checker (shape) → the net checker (roles: shorted supplies, unpowered ICs, missing pull-ups) → the engine (numbers: levels, derating).
- **A datasheet is a contract.** Absolute max → `absolute_max { }`, operating conditions → `operating { }`, electrical characteristics → spreads. Automatic checks are assumptions of parts and interfaces.
- **Waivers use Rust's lint model:** `#[allow(derating::capacitor::v_rated, reason = "…")]`.
- **Results never live in the source.** They show as inlay hints and diagnostics.
- **Default confidence:** 3σ for user specs, worst case for automatic checks.

### 4.2 The amplifier in the merged language

This uses my recommendation on each open decision (§4.3):

```rust
/// Common-emitter audio stage. Gain ≈ RC/RE ≈ 4.6, f_L ≈ 20 Hz.
pub block CeAmp {
    port vcc: Power<In>;
    port gnd: Ground;
    port input: Analog<In>;
    port output: Analog<Out>;

    net base;
    net emitter;

    /// Divider holds the base near 2.1 V, which sets IC ≈ 1.4 mA.
    let r1 = Resistor { p: vcc, n: base, value: 47k ± 1% };
    let r2 = Resistor { p: base, n: gnd, value: 10k ± 1% };

    /// Gain ≈ RC/RE. RC also puts VC near mid-supply.
    let rc = Resistor { p: vcc, n: output, value: 4.7k ± 1% };
    let re = Resistor { p: emitter, n: gnd, value: 1k ± 1% };

    /// f_L = 1/(2π·C_in·R_in) ≈ 20 Hz. Loses up to 20% by end of life.
    let c_in = Electrolytic { p: base, n: input, value: 1uF ± 20%, aging: -20% };

    let q1 = Npn { c: output, b: base, e: emitter, beta: 100..=300, beta_tc: 0.5%/K };
}

contract CeAmp {
    // assumptions → range knobs   (temp and life come from the project)
    assume vcc.v in 12V ± 5%;
    assume input in Sine { amp: ..=100mV, freq: 20Hz..=20kHz };
    assume input.z_src <= 1kΩ;
    assume output.z_load >= 10kΩ;

    let h = ac(output.v / input.v);

    /// Room for the output to swing ±1 V without clipping.
    spec bias: dc(output.v) in 4.5V..=6.5V;
    /// The next stage expects this level.
    spec gain: h.at(1kHz).mag() in 4.6 ± 5%;
    /// Don't cut the bass.
    #[confidence(yield(99.9%))]
    spec bass: h.f_low(-3dB) <= 30Hz;
}
```

```rust
contract Top {
    assume temp in -10°C..=60°C;
    assume life in 0y..=10y;
}
```

What the editor shows next to the specs (inlay hints, never written to the file):

```
spec bias: dc(output.v) in 4.5V..=6.5V;     ✓ max 6.34 V (3σ) · worst-case 6.59 ✗ · temp, β
spec gain: h.at(1kHz).mag() in 4.6 ± 5%;    ✓ 4.52…4.67 (3σ) · RC + RE ≈ 80%
spec bass: h.f_low(-3dB) <= 30Hz;           ✗ 31.7 Hz (3σ) · 0.9% of boards · C_in
```

Full worked examples for a TPS54302 buck, an ATtiny85 with two TMP117 sensors, and an 8-channel protection array are in `language_core.md` and `language_specs.md`.

### 4.3 Decisions for you

These are the points where the agents disagreed, or left the question open.

**L1. Where the contract lives**

| Option | For | Against |
|---|---|---|
| **A. Separate `contract CeAmp { … }` item** in the same file, like `impl` next to `struct` (specs agent) | One item = the I/O page. Circuit edits (edit mode) and spec edits (simulate mode) never touch the same item. Library blocks ship their contract. Familiar Rust pattern | Two items per block |
| **B. Inside the block body** (editor and core agents) | One self-contained item. An AI slice is one item | Circuit and I/O page mixed in long blocks |

*My recommendation: A.* It maps one-to-one onto the I/O page and onto simulate mode.

**L2. How connections are written**

| Option | Example | For | Against |
|---|---|---|---|
| **A. Element-centric:** each part binds its pins (core agent) | `let r1 = Resistor { p: vcc, n: base, value: 47k ± 1% };` | One statement per placed part. Reads like a SPICE line. Every pin bound exactly once by construction. Wins the common gestures (place, delete, change a part) | "What's on `base`?" needs the editor or LSP to answer |
| **B. Net-centric:** each net lists its pins (editor agent) | `net base = [c_in.n, r1.b, r2.a, q1.b];` | Reading a node at a glance; wire gestures | A part's connections are spread over several statements; a forgotten pin isn't caught by the syntax |

*My recommendation: A,* with `net x = [a, b];` kept for the rare net merge. Both agents suggested settling it with a small prototype measuring diff size and edit locality, which we can do.

**L3. Probe syntax.**
- Options: typed field accessors (`output.v`, `q1.c.i`, `r1.power`) or SPICE-style (`V(output)`, `I(q1.c)`).
- *My recommendation: field accessors.* They're more Rust-like, and the editor can offer the right measurement methods after a `.`. Note that `r1.p` is taken by the pin name, so power must be `r1.power`.

**L4. Port assumptions.**
- Options: attached to the port (`port vcc: Power<In> { v: 12V ± 5% };`) or as statements (`assume vcc.v in 12V ± 5%;`).
- *My recommendation: `assume`.* The attached form looks like a value, i.e. a spread, which is the opposite meaning.

**L5. Instance keyword.**
- Options: Spade-style `inst`, or plain `let name = Type { … }`.
- *My recommendation: no keyword.* Struct-literal syntax already looks different from a function call.

**L6. Small defaults.**
- `?` vs `_` for solver-chosen values. *Recommend `?`.*
- Does a bare `47k` mean ideal, or the project's default tolerance? *Recommend: ideal, plus a lint on physical parts without a tolerance.*

**L7. Name and file extension.** Candidates: `.spl`, `.spc`, `.ckt`. Only needed once we write a parser.

---

## 5. Suggested next steps

1. **You decide L1–L6.** Or tell me what feels wrong in the example above.
2. I write **engine design v3** (sections 3 and 2 folded into one coherent document) and **language design v0.1** (section 4 plus the three reports, merged into one spec).
3. **Building**, in an order that follows the findings:
   1. **Simulator prerequisites (E1):** controlled sources → parameter layer → sensitivities → robust DC. Nothing real runs without these.
   2. **The datasheet-arithmetic engine (E8) with the rich knob model (E4).** It needs no new simulator features, covers most digital specs exactly, and exercises the knob model.
   3. **The worst-point loop with the safety net (E2)** on the amplifier, then on an op-amp circuit.
   4. **A parser for the language subset** needed to express these examples.
