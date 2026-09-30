# Next Circuits: Power

> 2026-09-28 · Round "next circuits", power domain. It tests the **current** design (engine.md v4, engine_plan.md, engine_types.md M3a draft, language.md v0.1 + roadmap §4.1, model.md E1–E26) on the power circuits that should come after the CE amp. It builds on round 1's `engine_power.md` (cited "power §n") and doesn't repeat it: round 1 asked whether the engine *math* survives power circuits; this asks where the *types, plan, language and export* break.
> **Where the numbers come from.** Every number marked **[P]** was computed for this report on ngspice-42 (libngspice through the round-2 ctypes driver `soundness/ng.py`) by the stdlib-Python scripts in `/root/.claude/jobs/443154a8/tmp/engine3/power/` (called `$P`; the appendix lists them). Timings are serial, one libngspice instance, machine load 0.7–1.5. Other numbers cite their source.

---

## 0. Summary

I walked three power circuits through the whole stack: a **discrete LDO** (TLV431-style reference + PNP pass, with sleep/run modes), a **synchronous buck** (at two levels, averaged and switching), and a **load switch** that starts a downstream converter. The design holds where the CE amp exercised it: enumeration found every worst case at a corner on all three, the audit found nothing inside the box, and the verdict tables need no new rows for FAIL or PASS (all corners). It breaks in five places. (1) **The time budget:** at ngspice speed a transient spec costs 14 ms (LDO load step), 19 ms (load switch) or 240 ms (switching buck) per run, so the plan's "fits 3 s" clause stops enumeration at 2^7, 2^7 and 2^3 corners. The buck's 11-knob load-step side took **551 s** to enumerate. (2) **Levels:** on the averaged buck model, enumeration says the load-step undershoot passes a 150 mV bound at all 1,024 corners (worst **143.1 mV**). The switching model fails it at **16 of 2,048** corners (worst **157.9 mV**). Re-running the averaged worst corner at the switching level, at both edges of f_sw (a knob the averaged model can't see), catches it: **157.5 mV**. So the switching run is a verification step one level up, not the level you enumerate on. (3) **The types assume one bench and a DC/AC run:** `AnalysisSet` has no transient, `Run` has no waveform, `DeviceOp` is BJT-only, `KnobBinding` has one target, and `Side` has no mode or condition. Each is cheap to widen now and costly to retrofit. (4) **Modes change the answer:** the LDO's phase margin is **92.9°** at 300 mA, **61.2°** at 1 mA, and **32.5°** at the 100 µA sleep load. A mode-dependent load range is not a product box. (5) **The export meets real power models:** a misspelled `VDMOS` line hangs ngspice for good, the self-heating VDMOS failed its operating point in 2 of 3 decks, and a thermal ambient must be a second binding of `temp`. The cheapest high-value seams: an `Analysis` list in `Needs` (with `Tran` and `DcSweep`), a `BenchId` on every `Request` and row key, `KnobKind::Mode`, a `Scenario` (a mode plus its sub-box), `Measured::Beyond`, a per-run timeout, multi-target bindings, a topological cone rule, and a `Level` on `Request`. Right after the MVP comes **the LDO**. It needs one IC part, current probes, benches and the first transient measure, and it proves loads, modes and loop gain without the analysis ladder.

---

## 1. The circuits, and why these three

| # | Circuit | What it tests that the CE amp doesn't | Chosen over |
|---|---|---|---|
| **P1** | Discrete LDO, 3.6–5.5 V → 3.3 V, 0–300 mA, TLV431-style reference/error amp, NPN driver, PNP pass; **modes Run/Sleep** | A load as a range knob; a mode that changes the load range; a feedback loop (loop gain, phase margin); PSRR (an AC source on a *supply*); a cheap transient (load step from the DC point); dropout as a device-region boundary; one IC part with a subcircuit model; self-heating as a measure | The battery rail with modes (folded in as P1's modes) |
| **P2** | Synchronous buck 12 V → 3.3 V, 2 A, 500 kHz, voltage mode, behavioral PWM, ideal switches; **two levels**: averaged and switching | The transient tier at real cost; the analysis ladder (a model the search runs on vs one that verifies); a knob only one level sees (f_sw); subharmonic behavior; a measure window that depends on a knob (whole periods of 1/f_sw); ε_num of a switching transient | A boost (round 1 covered its RHP zero: power §4) |
| **P3** | Load switch: PMOS + gate slew cap, 470 µF load, downstream converter as a constant-power load with UVLO (on 9.0 V, off 8.0 V), cable resistance, junction temperature from a thermal RC network | MOSFET models in the export; behavioral loads (B-source, switch with hysteresis); events (UVLO); a **missing crossing** that is a real failure; peak and junction-temperature measures; `temp` driving two targets | An eFuse IC (its current-limit and retry logic is a vendor model; round 1 did the arithmetic: power §6) |

---

## 2. P1: the discrete LDO

### 2.1 The circuit

```
 vin 3.6…5.5 V ─┬──────────────┬─────────────┐
                │              │             │ e
             R_bias 4.7k    R_be 470     ┌───┴──┐  Q1 PNP pass (β 100..=300)
                │              │         │ b    │
                ● db ───┐      ● pb ─────┘      │ c
                │       │      │                ●───────────────● vout ── I_load (0…300 mA Run, 100 µA Sleep)
     U1 TLV431  │     Q2 NPN   │c               │               │
     k=db  a=gnd│    b=db      │             R_top 16.5k      C_out 10 µF
     ref=fb     │    e ── R_e2 330 ── gnd       │               │
                │                               ● fb          R_esr 10 mΩ…1 Ω
       R_c 47k ─┴─ C_c 2.2n ── fb               │               │
                                              R_bot 10k        gnd
                                                │
                                               gnd
```

U1 sinks current from Q2's base when `fb` rises above V_ref (1.24 V ± 1%), so Q2 pulls less from Q1's base and vout falls: negative feedback. R_c + C_c compensate the loop. The TLV431 is a behavioral subcircuit (a 1 mA/V transconductor, a 60 dB internal node, a tanh-limited output), typical-only except V_ref, as vendor models are (power §9.4).

Nominal **[P]**: vout 3.2862 V; PSRR at 100 kHz −60.8 dB; I_q 1.85 mA (sleep, 3.6 V) to 2.25 mA (5.5 V); Q1 dissipates up to 0.664 W (5.5 V, 300 mA).

### 2.2 Knobs and specs

| Knob | Kind | Range | | Spec | Measure | Bound |
|---|---|---|---|---|---|---|
| `temp` | range | −20 … 60 °C | | `vout_dc` | `dc(vout.v)` | 3.3 V ± 3% |
| `vin.v` | range | 3.6 … 5.5 V | | `psrr` | \|vout/vin\| at 100 kHz | ≤ −40 dB |
| `vout.i` | range, **depends on mode** | Run 1 mA … 300 mA; Sleep 10 … 100 µA | | `pm` | phase margin of the loop | ≥ 45° |
| `mode` | **mode** | {Run, Sleep} | | `undershoot` | min vout after a 1 mA → 300 mA step | ≥ 3.1 V |
| `u1.v_ref` | statistical (part) | 1.24 V ± 1% | | `settle` | time back within 1% | ≤ 200 µs |
| `r_top`, `r_bot`, `r_be`, `r_bias`, `r_e2`, `r_c` | statistical | ± 1% | | `iq` (Sleep) | `dc(vin.i) − vout.i` | ≤ 3 mA |
| `c_out` · `c_c` | statistical | ± 20% · ± 10% | | `tj` | `temp + theta_ja · q1.power` | ≤ 125 °C |
| `c_out.esr` | statistical, typical-only | 10 mΩ … 1 Ω | | | | |
| `q1.beta`, `q2.beta` | statistical | 100 ..= 300 | | | | |
| `theta_ja` | **layout-owned** | 60 … 100 K/W | | | | |

That is 15 continuous knobs, 1 mode and 1 layout assumption: about twice the CE amp.

### 2.3 The contract, as far as v0.1 goes

`✓` = in the MVP subset (roadmap §4.1) · `▲` = designed in language.md but not in the MVP · `✗` = no syntax anywhere yet.

```rust
contract Ldo3v3 {
    assume temp in -20°C..=60°C;                                  // ✓
    assume vin.v in 3.6V..=5.5V;                                  // ✓
    assume mode in {Run, Sleep};                                  // ▲ language §8.2 (not in the grammar: no `{a, b}` set literal)
    assume vout.i in 1mA..=300mA where mode == Run;               // ✗ `where` exists only on specs (§8.3); a conditional assumption has no syntax
    assume vout.i in 10uA..=100uA where mode == Sleep;            // ✗
    assume theta_ja in 60K/W..=100K/W;                            // ✗ no place for a layout-owned quantity (engine.md §2.1 names the kind)

    spec vout_dc: dc(vout.v) in 3.3V ± 3%;                        // ✓ (vout.i as a knob needs M1e's load bench, §2.5)
    spec psrr: ac(vout.v / vin.v).at(100kHz).mag() <= -40dB on ripple;   // ▲ benches; ✗ a dB bound on a ratio (model.md E13 accepts dB only as a level argument)
    spec pm: ac(loop_gain(break: r_top.a)).phase_margin() >= 45°; // ✗ no way to say where the loop is broken
    spec undershoot: tran(vout.v).min() >= 3.1V on load_step;     // ▲ tran, benches; ✗ `.min()` isn't in §8.4's list (only .peak, .pp, .rms, .overshoot, .crossing, .thd)
    spec settle: tran(vout.v).settle(1%) <= 200us on load_step;   // ✗ no settling method (and it is non-smooth, §8.4)
    spec iq: dc(vin.i) - vout.i <= 3mA where mode == Sleep;       // ▲ current probes on ports, `where`
    spec tj: temp + theta_ja * q1.power <= 125°C;                 // ▲ `r1.power`-style probe; temperature point + difference (E13) ✓

    bench ripple    { vin: Dc(vin.v) + Ac(1), ..default }         // ▲ form bench; ✗ no AC-on-a-supply signal family
    bench load_step { vout: Load::Step { from: 1mA, to: vout.i, rise: 1us, at: 50us }, tran: 0us..=300us, ..default }   // ▲
}
```

**Where the syntax runs out:** conditional assumptions (`where` on `assume`), mode-set literals, layout-owned quantities, loop breaking, dB bounds, `min`/`settle`, and AC on a supply. Everything else is designed but outside the MVP subset.

### 2.4 Elaboration (model.md)

- **Parts.** `Tlv431` is an IC part (language §6.8) with a spread on `v_ref` and a `model: spice(…)` subcircuit. The MVP prelude has no IC parts, no part records and no `Zener` (roadmap §4.1). The smallest step is one prelude kind with a behavioral model, `ShuntRef { k, a, ref, v_ref }`, before part records arrive.
- **Knobs (E16).** `u1.v_ref` is a leaf field with a spread, so it becomes a knob like `r1.value`. `vout.i` becomes a range knob from `assume`. `mode` has no knob kind in `KnobTable` (range | statistical only).
- **Modes are not a product box.** `vout.i`'s range depends on `mode`. E16 makes one knob with one range, so the table can't express "1 mA … 300 mA in Run, 10 … 100 µA in Sleep". The honest shape is **one box per scenario**: `{mode = Run, vout.i ∈ [1 mA, 300 mA]}` and `{mode = Sleep, vout.i ∈ [10 µA, 100 µA]}`, the other knobs shared. Writing it as one range 10 µA … 300 mA would check sleep-mode loads with run-mode specs.
- **E24 (the contract)** needs three more measure functions (`min`, `settle`, `loop_gain`) and a current probe on a port (`vin.i`) and on a device (`q1.power`).

### 2.5 The M1f export (plan §5)

- **Load bench.** Plan §1.3's default bench leaves outputs unloaded. An `assume vout.i` on a `Power<Out>` port must become a current sink `I_load vout 0 DC {k_il}` bound to that knob. It's one lowering rule, and the language already implies it (language §3.2 item 3: "an `Analog<Out>` port gets the assumed load").
- **Benches are separate decks.** PSRR needs `AC 1` on `V_vin`, the gain spec none. The loop gain needs a 0 V `AC 1` source spliced into the net between `vout` and `R_top`, which splits one net into two. The load step needs `PULSE` on `I_load`. These are three netlists, not one with different analyses, so `EngineDeck` becomes one deck **per bench**, with a shared knob map.
- **Subcircuit knobs.** `X_u1 db 0 fb TLV431 vref={k_vref}` binds, and ngspice reads it back inside the subcircuit (`@v.x_u1.vr[dc]`-style paths; plan §12 q. 6 is answered for this form). A real vendor TLV431 model has no `vref` parameter, so the knob has nothing to bind to. That case needs engine.md §6.6's "standard wrapper" (a series trim source on `ref`), and the coverage map should report it.
- **Trap found:** the source in a `dc` sweep must be named in lower case in the control command (`dc v_vin …`). `dc V_vin …` produced no plot and no error message **[P]**.

### 2.6 The engine

**Costs per run [P]:**

| Analysis | ms per run | Notes |
|---|---|---|
| op | 0.65 | as the CE amp (0.71 ms, plan §2.3) |
| AC point (PSRR at 100 kHz) | 0.56 | |
| Loop-gain sweep, 10 Hz–100 MHz, 50/dec | 1.56 | its own deck (§2.5) |
| DC sweep of vin, 3.0–5.5 V in 5 mV (dropout) | ≈ 3 | a crossing measure on a sweep |
| Load step, 300 µs from the DC point | **14.3** | 3,018 time points; at reltol 1e-9: 15.3 ms, vmin equal to 1e-7 V |

**Cones and the budget.** The plan's cone rule is "C and L values never reach an op" (plan §6.2), a rule on *values*. It still leaves `r_c` (in series with `C_c`) and `c_out.esr` in the DC cone, which gives 13 knobs, 2^13 = 8,192 corners: **UNDECIDED (budget)** on the simplest spec. A **topological** rule would drop them: a resistor whose every DC path runs through a capacitor can't reach the op. With that rule the DC cone is 11 knobs. Enumerating it took **1.8 s for 2,048 corners, 0.88 ms per run [P]**: vout 3.2125 … 3.3609 V, inside 3.201 … 3.399 V, so PASS (all corners). The loop-gain cone has all 15 knobs: 32,768 × 1.56 ms = 51 s. The load-step cone has 15: 32,768 × 14.3 ms = 7.8 min. Both are over the budget, so they're UNDECIDED (budget) until the loop.

**Modes decide stability [P]:**

| Load | Phase margin | Crossover |
|---|---|---|
| 300 mA (Run, max) | 92.9° | 20.0 kHz |
| 1 mA (Run, min) | 61.2° | 2.7 kHz |
| 100 µA (Sleep, max) | **32.5°** (ESR 10 mΩ: 32.2°; 1 Ω: 35.6°) | 0.96 kHz |

`pm ≥ 45°` passes in Run and FAILs in Sleep. If sleep is folded into one load range, the verdict is right by accident. If sleep is forgotten, the report says PASS. The DC loop gain, by contrast, has an **interior maximum** in the load current: 64.8 dB at 100 µA, 92.4 dB at 30 mA, 78.6 dB at 300 mA **[P]**. A `≥` spec on it is safe at the corners, but a `≤` spec would need the audit to find the interior.

**Dropout and the region rule.** Dropout, measured as the vin where vout is 1% below regulation on a DC sweep, is **0.21 V at 300 mA** and 0.02 V at 1 mA **[P]**. At the 3.6 V, 300 mA corner (cold, β 100) the headroom is 0.239 V, and the PNP's gmu/gm is **4.4e-5**. That is under plan §4.6's 1e-4 threshold, but by a factor of only 2.3. At 3.5 V it flags. Then S7 ("some corner has a device outside its nominal region") makes **every** `sigma(3)` verdict whose cone holds Q1 UNDECIDED (device region changes), including specs that have nothing to do with dropout. For an LDO, dropout at the low-vin corner is the expected operating edge, not an anomaly. The rule needs scoping: to the measures whose value changes with the region, or to devices the part marks "must stay active".

**Load step [P].** 1 mA → 300 mA in 1 µs: vmin 3.057 V (−229 mV), back within 1% after 168 µs. The reverse step, 300 mA → 100 µA, peaks at 3.50 V (+6.5%) and is **still at 3.43 V after 3 ms**. The LDO can't sink current, so only the divider's 125 µA and the 100 µA load discharge C_out. A 1% settling time doesn't exist inside any practical window. The measure must return "beyond the window" (C9's `Beyond`), and that is a **FAIL** for `settle ≤ 200 µs`, not an `Undefined` that the tables would call UNDECIDED.

**Self-heating [P].** A fixed point on Q1's `dtemp` (ngspice's per-instance temperature offset) converges in **2 runs at θ_JA = 100 K/W and 3 at 250 K/W**. It moves vout by 30–60 µV, because the loop holds vout. T_J is 60 + 66.4 = **126.4 °C** at 100 K/W: FAIL against 125 °C. At 250 K/W it is 226 °C. So for an LDO, self-heating matters **as a measure** (`temp + theta_ja · q1.power`, per run, D-F), not as feedback into the circuit. θ_JA is a layout-owned knob that sets the verdict alone.

### 2.7 The M3a types

| Type (engine_types) | What P1 needs | Status |
|---|---|---|
| `KnobKind { Range, Statistical }` (T1 §2.1) | `Mode(levels)`, layout-owned (a range with an owner tag) | missing |
| `Point`, ε ∈ [−1, 1] | a mode has no interior: no nudge, no σ, no audit coordinate | missing |
| `Side` (§2.3) | a scenario: fixed mode, narrowed ranges (`where`) | missing |
| `Needs`, `AnalysisSet { op, ac_points, sweep }` (T4) | `Tran`, `DcSweep`, loop-gain AC on another bench | missing |
| `Request` | `bench: BenchId` | missing |
| `RunTable` keys `(Key, Tolerance)` (T6) | `(BenchId, Tolerance, Level, Key)`; an op on `load_step` ≠ an op on the default bench | missing |
| `DeviceOp { ic, ib, vbe, vbc, gm, gmu }` | PNP works (the sign convention holds through gmu); power per device | BJT-only |
| `Measured { Value, Undefined }` | `Beyond` for the release-step settle | "later" |

### 2.8 What the engineer or agent asks

- "Why does `pm` fail?" The flip table must say **mode = Sleep** first. A mode flip is a flip like any other, but the table has to show it by name, not as ε.
- "What C_c fixes sleep-mode stability without slowing run-mode recovery?" A two-scenario what-if: one edit, both boxes re-checked.
- "Is the dropout a problem at 3.6 V?" The answer is the DC-sweep crossing (0.21 V) against the 0.239 V headroom, with its corner, and the region tag "q1 near saturation".
- "What θ_JA do I need?" The largest θ that passes: a solve on a layout knob (engine.md §4.8), exported as a layout constraint "θ_JA ≤ 98 K/W" (arithmetic: (125 − 60)/0.664).

---

## 3. P2: the synchronous buck, at two levels

### 3.1 The circuit

```
 vin 12 V ±10% ─── S_hs (30 mΩ) ──┬── L 6.8 µH ±20% ── DCR 20 mΩ ──┬── vout ── I_load 0.2 → 1…2 A step
                                  │ sw                                │
                   S_ls (30 mΩ) ──┘                            C 44 µF ±20%, ESR 2…10 mΩ
                   │                                                  │
                  gnd      PWM: comp vs ramp (1 V, f_sw 450…550 kHz)  R_top 10k / R_bot 3.2k → fb
                           type III: E_ea (gain 1e4), R2 4.7k, zeros at f_z 5.4…6.6 kHz, poles at 150 kHz
 averaged level: S_hs/S_ls/ramp replaced by  V(sw) = clamp(V(comp)/V_ramp, 0, 0.98) · V(vin)   (no f_sw at all)
```

### 3.2 Specs and knobs

| Spec | Measure | Level it needs | | Knob | Range |
|---|---|---|---|---|---|
| `vout_dc` 3.3 V ± 3% | `dc(vout.v)` | averaged op (or datasheet arithmetic: V_ref · (1 + R_top/R_bot)) | | `vin.v` | 10.8 … 13.2 V |
| `ripple` ≤ 30 mV | `tran(vout.v).pp()` over whole periods | switching (or a formula) | | `vout.i` | 1 … 2 A (the step's top) |
| `undershoot` ≤ 150 mV | pre-step mean − min, 0.2 A → `vout.i` | averaged; switching to verify | | `l1`, `c_out` | ± 20% |
| `il_peak` ≤ 4 A (current limit min) | `tran(l1.i).peak()` | switching (or I_out + ΔI/2) | | `c_out.esr` | 2 … 10 mΩ |
| `pm` ≥ 45° | loop gain | averaged AC | | `v_ref` | 0.8 V ± 1% |
| `eff` ≥ 90% at 2 A | P_out/P_in over whole periods | switching + loss models | | `r_top`, `r_bot`, `r2` | ± 1% |
| | | | | `f_z` (C2, C3) | ± 10% |
| | | | | `f_sw` | 450 … 550 kHz (switching level only) |

The contract is language §11.1's `Buck3v3` with one IC part. What runs out is the same as for P1, plus two things. There's no way to say **which model** a measure runs on (averaged vs switching), and none for a window in **whole periods of a knob** (`.window(periods: 10)`).

### 3.3 Costs, and what enumeration can afford [P]

| Run | Time | Points |
|---|---|---|
| Averaged, start-up 3 ms | 12 ms | 3,344 |
| Switching, start-up 3 ms | **330 ms** (27×) | 118,036 |
| Averaged loop-gain sweep, 100 Hz–1 MHz | 1.27 ms | |
| Averaged load step, 300 µs from the DC point | 9–16 ms (median 10) | ≈ 3,000 |
| Switching load step: start-up 1.5 ms, then the step (the op of a switching circuit is not its steady state) | **211–458 ms (median 240)** | ≈ 80,000 |
| Switching at reltol 1e-9 | 1,044 ms (3.1×) | 299,770 |

Plan §2.3 enumerates when 2^|cone| × (cost of the nominal run) fits 3 s. That allows **12 switching runs**, which is 2^3 corners. The averaged load step's 10-knob cone needs 1,024 × 10 ms = 11.5 s, also over budget on one worker, but 0.7 s on 16 (scale §3.3's 6–9× speed-up would put it at 1.3–1.9 s). **The time clause makes the verdict depend on the worker count**, so the same design can be PASS on one machine and UNDECIDED (budget) on another. The budget should be stated in runs per level (or by a declared worker count), so a verdict is reproducible, with time as a warning.

### 3.4 The level experiment: enumerate averaged, verify switching [P]

The full key: all 2^10 averaged corners (11.5 s) and all 2^11 switching corners, f_sw included (**551 s**).

| | Averaged (10 knobs) | Switching (11 knobs) |
|---|---|---|
| Worst undershoot | **143.08 mV** | **157.85 mV** |
| At the corner | vin low, i high, L high, C low, ESR low, V_ref low, R_top high, R_bot low, R2 low, f_z high | the same, but R_bot high, f_sw low |
| Corners over 150 mV | **0 of 1,024** → PASS (all corners) | **16 of 2,048** → FAIL |
| Switching at the averaged worst corner | — | f_sw low: **157.53 mV** (FAIL) · f_sw high: 146.32 mV |
| Rank of that corner in the switching table | — | 2nd, 0.32 mV below the true worst |
| Switching − averaged, per corner | — | **−0.03 … +26.8 mV** |

What this shows:
1. **The averaged model gives a false PASS here.** "PASS (all corners)" would be wrong if the corners are averaged-model corners. The word must name its level.
2. **Verifying one level up works, if the verification enumerates the knobs only the upper level has.** At the averaged worst corner with f_sw at nominal, the switching run would have landed between 146 and 158 mV. Only both f_sw edges find the FAIL. The rule: verify the lower level's worst corner × the 2^k corners of the knobs the lower level can't see.
3. **A constant model-gap term doesn't work.** The gap runs from −0.03 to +26.8 mV across corners. Round 1 proposed a "named model-form error term" (power §9.2). It has to be measured at the verified point, not carried from nominal (9.1 mV at nominal: 100.6 vs 109.7 mV).
4. **The step lands at a phase of the switching cycle** that depends on f_sw (the step time × f_sw cycles). That's a hidden knob, and part of why the gap varies. A switching-level load step should sweep the step phase (e.g. 4 phases) or report it.

### 3.5 What else the buck shows [P]

- **Subharmonics the averaged model can't see.** With R2 = 20 kΩ and the poles at 250 kHz, the switching model runs **period-2**: inductor peaks alternate 2.296 / 2.456 A at nominal. The averaged model has no way to show this: it has no switching cycle. With R2 = 20 kΩ and poles at 100 kHz the switching run isn't periodic at 3 ms at all. A measure over one period (`il_peak`) reads 2.30 or 2.46 A depending on which cycle it lands on. Switching measures need **at least two periods, and a periodicity check**: compare the last two periods and return "not periodic" when they differ.
- **ε_num of a switching transient.** Ripple is 5.19 mV at engine tolerances, 5.05 mV at reltol 1e-9, and 5.27 mV with a 10 ns maximum step: a band of ≈ 0.2 mV (4%), against 5e-9 V for the CE amp's DC. The band stage's "re-run at reltol 1e-9" (plan §2.7) misses the maximum-step half of it, and costs 3×. For transients the band needs both knobs (reltol and the maximum step).
- **Efficiency is 97.0% with ideal switches** (conduction losses only). Switching loss ≈ ½ · 12 V · 2 A · 20 ns · 500 kHz ≈ 0.24 W (a textbook estimate, t_r + t_f = 20 ns assumed), about 3.5 points on 6.6 W. The `eff ≥ 90%` verdict hinges on something not modeled. `not_modeled` must be able to list "switching losses", and a spec whose margin is smaller than an unmodeled term should carry the model-conditional tag automatically.
- **Every switching run printed "Dynamic gmin stepping failed / True gmin stepping failed"** and then completed. The backend must sort messages into fatal / warning / benign per analysis. Round 2's error filter (`ng.errors()`) treats both as failures.

### 3.6 Is "every corner" the right question for a converter?

Mostly yes, **at the right level**:

| Spec | Right method | Why |
|---|---|---|
| `vout_dc`, UVLO, current-limit set points | Datasheet arithmetic (engine.md §4.1), exact in 2 evaluations | Monotone formulas over the part's limits |
| `pm`, `undershoot` | Enumerate the **averaged** model; verify the worst corners at switching level × the switching-only knobs | Enumeration on the averaged model also removes round 1's loop troubles: the joint three-knob flip and the cycling (power §0 item 6), because every corner is simulated |
| `ripple`, `il_peak` | A formula for the corners (monotone in CCM), verified at switching level, or PSS when a backend has it | ngspice has no shooting PSS for a driven circuit; the switching transient must run to steady state (1.5–3 ms here) |
| `eff` | Switching level with loss models at a few operating points × part corners | It's the smooth, expensive one; few knobs matter |
| Start-up, hiccup, faults | One-shot transient scenarios, not a box | Event sequences (power §9.3) |

What "every corner" misses in a converter is the **mode boundary** (CCM/DCM, current limit): a range knob such as the load can cross it inside the box. Enumeration catches a limit that engages *at* a corner (plan §2.4), and the audit catches a broad interior region. A spec on a mode ("never current-limit under the declared load") is the direct way: round 1's "mode-boundary auto-specs" (power §9.7 item 4).

---

## 4. P3: the load switch that starts a converter

### 4.1 The circuit

```
 12 V ±10% ── R_src 0.05…0.3 Ω ── 1 µH ──┬── vin ── Q1 PMOS (V_th −1.2…−2.4 V) ──┬── out ──┬── C_load 470 µF ±20% (+50 mΩ)
                                          C_in 10 µF       s│ g        d           │         ├── R_bleed 10k
                                                        R_gs 100k ● C_gd 22 nF ────┘         └── downstream converter:
                                                                  R_g 100k ── EN switch at 1 ms       I = P/V · on(UVLO 9.0/8.0 V), P 8…10 W
 thermal: B-source P = V(vin,out) · I_d → T_J node (R_jc 3 K/W ∥ 2 mJ/K) → case (R_ca 60 K/W ∥ 0.2 J/K) → V_ta = temp (°C as volts)
```

### 4.2 Specs, knobs, contract

Specs: `inrush` = `tran(vin.i).peak() <= 3A`; `t_start` = `tran(out.v).crossing(0.9 * vin.v)` ≤ 8 ms; `tj` = `tran(q1.tj).peak()` ≤ 125 °C; `no_restart` = out stays above 8.0 V once the converter is on. Knobs (9): `temp`, `vin.v`, `r_src` (a setup assumption), `c_load`, `load.p`, `c_gd`, `r_g`, `q1.vth`, `load.uvlo`.

The contract needs everything §2.3 lacked, plus four things with no syntax at all:
- a **behavioral load** in the language: the downstream converter as a constant-power sink with UVLO hysteresis. It's either a part kind (`PowerLoad { p, v_on, v_off }`) or a sheet bench (language §8.5);
- a **relative crossing threshold** (`0.9 * vin.v`, per run: fine under D-F);
- a **"from an event on"** window (`.window(from: out.v.crossing(9V))`);
- a **device thermal quantity** (`q1.tj`).

### 4.3 Engine results [P]

- **Cost:** 512 corners in **11.0 s** (17–25 ms per 40 ms transient). Over the 3 s budget on one worker, so all four sides are UNDECIDED (budget) in M3 as written.
- **Corner extremes:** inrush 3.715 A (nominal 2.477 A); T_J 79.3 °C; t_start 6.17 ms; minimum out after turn-on 8.835 V > 8.0 V, so no restart.
- **Inside the box:** the best of 64 random interior points is inrush 3.08 A and T_J 68.2 °C, both below the corners. 1-D scans at the worst corner are monotone in V_th (3.07 → 3.72 A), UVLO (3.715 → 3.674 A) and C_load (2.87 → 3.72 A). This circuit is corner-friendly.
- **A missing crossing that is a real failure:** with R_src = 2 Ω (a long cable), the rail settles at 9.97 V (V(12 − V)/2 = 10 W). It never reaches 90% of 12 V, and it never trips UVLO either. `t_start` has **no crossing**, and the right verdict is FAIL with this counterexample. Under the plan, `Undefined` "never passes" (T7), and the tables have no row that turns it into a FAIL. It would land in UNDECIDED at best. `Beyond { lo: window_end }` fails a `≤` bound decisively.
- **`temp` has two targets.** Its main effect on T_J is exactly +40 K, the half-range, through the thermal ambient source alone. Without that second binding, T_J would ignore the temperature knob. The circuit's `.temp` read-back would still match, so the check would pass silently. Read-back must cover **every** target of a knob.

### 4.4 The export and ngspice traps [P]

| Found | Consequence |
|---|---|
| `.model … VDMOS pchan (…)` (keyword outside the parentheses) makes ngspice **hang with no output**: killed at 10 s and 20 s timeouts, three times. `VDMOS (pchan …)` works (3.0 A, as expected) | The worker needs a **per-run wall-clock timeout** → `RunError::Timeout`, restart. T3/§4.1 have `WorkerDied` for a closed pipe, not for a live hang. The exporter's model cards need a round-trip test against ngspice's parser |
| VDMOS with `thermal` (self-heating nodes): the op failed ("timestep too small; trouble with m_q1") in 2 of 3 decks. It worked with the case node tied to a 25 V source (T_J 33.1 °C) | Built-in self-heating is fragile per model; an external thermal network driven by a power B-source worked every run. The thermal node's voltage **is** °C, so the ambient is a source bound to `temp` |
| A net held only by capacitors and an off FET: the op returned out = **−2.5 MV** with no error | Plausibility (plan §4.3 rule 3) caught it; M1e should add a bleeder or refuse (model.md §7 q. 4 names only isolated nets) |

---

## 5. Cross-cutting findings

### 5.1 The transient tier against the plan's budget rule

| Circuit / side | Knobs in cone | ms per run [P] | All corners, serial | Plan §2.3 (3 s, 1 worker) |
|---|---|---|---|---|
| CE amp, all sides (plan §2.8) | 7–8 | 0.71 | 0.18 s | enumerate |
| P1 `vout_dc` (topological cone) | 11 | 0.88 | 1.8 s | enumerate |
| P1 `pm` | 15 | 1.56 | 51 s | budget |
| P1 `undershoot` | 15 | 14.3 | 7.8 min | budget |
| P3 every side | 9 | 19 | 11 s | budget |
| P2 `undershoot`, averaged | 10 | 10 | 11.5 s | budget |
| P2 `undershoot`, switching | 11 | 240 | 9.2 min | budget |

Three conclusions:
1. **With 3 s per side, every transient spec in this report is UNDECIDED (budget)** in M3, and its `next` is "the loop", which doesn't exist yet. The loop doesn't help much at L0 anyway: a linearization costs N + 1 runs, so 12 runs at 240 ms is 2.9 s per step, and power specs are kink-capable, so the loop keeps the pessimistic e_obs allowance (engine.md §5.8).
2. **The honest near-term path is a larger explicit budget for transient sides, run on demand** (plan §1.1's "later" row), with the time printed: 512 runs, 11 s for P3, is acceptable for `spicy check --deep`. The rule should be "≤ 4,096 corners, estimated time shown; above X s it runs only with `--deep` or `--transient`", not a silent UNDECIDED.
3. **Estimating from the nominal run is optimistic:** switching runs spread 211–458 ms (2.2×), and a failing corner is often the slowest. Use the maximum of the first round, or re-estimate after each batch.

### 5.2 Non-smooth and event measures

| Measure | What we saw | What the design says | Gap |
|---|---|---|---|
| Settling after release (P1) | Not settled in 3 ms | §8.4: "a missing crossing is never a pass" | It should be a **FAIL** (Beyond), not UNDECIDED |
| `t_start` with a long cable (P3) | No crossing: the rail stops at 9.97 V | T7: `Undefined` | Same |
| `il_peak` in period-2 (P2) | 2.30 or 2.46 A by cycle | — | a periodicity check; `Measured::NotPeriodic` → UNDECIDED (simulator), `next`: a longer run |
| Undershoot vs the step's phase (P2) | varies with f_sw | — | a hidden knob; sweep the phase |
| Load-step undershoot across a region change | smooth in all 2,048 corners here | engine.md §5.8: kink-capable measures keep e_obs | `MeasureDef` needs its smoothness class now |

### 5.3 Modes

A mode is a knob that is (a) discrete, (b) enumerated in full at both confidences (range-like), (c) never nudged, sampled or put into the σ ball, (d) often **the condition under which other ranges hold** (the load current), and (e) often a different **bench** (sleep drives `en` low and loads 100 µA). The plan's box model is a product of intervals. The shape that fits all of P1 is a **list of scenarios**, each a mode assignment + range overrides + a bench, over one shared knob table. A spec is checked in every scenario unless `where` narrows it. The cost is additive: P1's DC side is 2^11 corners in Run plus 2^11 in Sleep.

### 5.4 Loads as benches

- **Range loads** (`assume vout.i in …`) are a default-bench current sink bound to a knob: one M1e rule (§2.5).
- **Load steps** are form benches whose fields may be knobs (`to: vout.i`), which makes the step size a range knob of that bench only.
- **Behavioral loads** (constant power, UVLO, P3) are either a std part kind or a sheet bench. They need B-sources and switches with hysteresis in the export. Both worked on ngspice, but neither is in `spicy_circuit` or the prelude.
- **Benches change topology** (loop breaking, AC on a supply), so each bench is a **separate deck**. T3's `prepare` is per deck already. What's missing is a `BenchId` in `Request` and in the run-table key.

### 5.5 Thermal

Three cases, three treatments:
- **The LDO:** T_J is a derived measure (`temp + θ·P`); feedback into the circuit is negligible (60 µV). No fixed point needed.
- **The load switch:** a transient thermal network is the measure's own analysis. It's an external RC driven by a power B-source, so the export must be able to add behavioral nodes that aren't in the schematic.
- **The buck's R_DS(on)(T_J):** round 1's ×1.2–1.33 loop gain (power §9.5) needs a real fixed point. On ngspice it's either the device's own thermal model (fragile, §4.4) or an outer `dtemp` iteration (2–3 runs per point on P1 **[P]**). That's a **backend-level composite run**: the engine should see one `Run` with a `thermal: Converged(iters) | Runaway` status, and runaway is a FAIL (engine.md §2.6).

---

## 6. Issues, ranked

| # | Issue | Severity | Hits | Suggested fix |
|---|---|---|---|---|
| 1 | Enumerating the averaged model gives a false PASS (143.1 mV vs 157.9 mV at a 150 mV bound) if the word doesn't name its level, and if verification skips the switching-only knobs | **risks a wrong verdict** | engine.md §5.9; plan §3.1 W6; engine_types T8 | A `Level` on every request and result; "PASS (all corners, averaged)" is not a PASS of the spec. The verdict needs a verification row: the worst corner(s) re-run one level up × the corners of the knobs only that level has; the gap is measured there, never assumed |
| 2 | A missing crossing or unsettled measure is `Undefined` → UNDECIDED, when it is a FAIL (P1 release step, P3 long cable) | **risks a wrong verdict** (a real failure reported as "undecided") | engine_types T7 `Measured`; plan §3.1 (no row) | `Measured::Beyond { side, at }` now, with a W/S row: Beyond past a `≤` bound is FAIL with the counterexample; Beyond on the passing side supports a PASS (language §8.4, C9) |
| 3 | A mode-dependent load range can't be expressed; folding modes into one range checks sleep loads with run specs, and forgetting sleep hides a 32.5° PM | **risks a wrong verdict** | model.md E16; engine_types §2.1 `KnobKind`, §2.3 `Side`; language §8.2 (no `where` on `assume`) | `KnobKind::Mode`; `Scenario { mode assignment, range overrides, bench }` in the `Plan`; `Side.scenarios`; grammar: `assume … where mode == X` |
| 4 | Read-back checks one target per knob; `temp` also drives the thermal ambient (P3), so a dead second target passes silently | **risks a wrong verdict** | plan §1.4, §5.1 `KnobBinding`; T10 | `KnobBinding.targets: Vec<Target>`, a read-back per target |
| 5 | The region rule S7 fires for devices whose region change is normal (the LDO's pass PNP near dropout; every switch), turning unrelated `sigma(3)` verdicts UNDECIDED | risks spurious UNDECIDED (cost) | plan §3.1 S7, §4.6; engine_types `DeviceOp` | Scope S7 to devices in the side's cone *and* marked "must stay in region", or to measures that change with the region; `DeviceOp` per device kind |
| 6 | Every transient side is UNDECIDED (budget) at 3 s; the time clause depends on worker count and the nominal run's time | **cost** / reproducibility | plan §2.3; engine.md §5.2; plan §12 q. 8 | Budget in runs per level; a transient tier with an explicit, printed larger budget on demand; estimate from the slowest run so far |
| 7 | The value-based capacitor rule keeps resistors in series with capacitors in DC cones (13 vs 11 knobs: budget vs enumerate) | **cost** | plan §6.2; engine_types §2.3 | A topological rule: remove C, L, and any element with no DC path to the op except through a C, by union-find over DC-conducting elements |
| 8 | `AnalysisSet`, `Needs` and `Run` have no transient and no DC sweep; `Program` can't window a waveform | **blocks the circuit** (all three) | engine_types T4, T7 | `Needs.analyses: Vec<Analysis>` (`Op`, `AcPoint(f)`, `AcSweep`, `DcSweep { source, points }`, `Tran { stop, max_step, bench }`); `AnalysisSet` = a bit set over it; `Run.results: Vec<AnalysisData>`; waveform programs (`Min`, `Peak`, `Crossing`, `Window`, `Settle`, `PeriodMean`) |
| 9 | Benches are separate decks, but `Request` and the row key have no bench | **blocks the circuit** (PSRR, loop gain, load step) | engine_types T3, T6; plan §5.1 (one `EngineDeck`) | `BenchId` in `Request`, `Row`, `Key`; `EngineDeck` per bench with one shared knob map |
| 10 | A malformed model line hangs ngspice for good; no timeout in the worker | **blocks** (the check never returns) | plan §4.1, §4.3; engine_types §4.1 `RunError`, T10 | A per-run wall-clock limit in the pool, `RunError::Timeout`, kill and restart; a parser round-trip test of every emitted model card |
| 11 | The band re-runs at reltol 1e-9 only; for switching transients the max-step half dominates (5.05 / 5.19 / 5.27 mV) and costs 3× | risks a wrong verdict near the bound | plan §2.7, §4.5 | The transient band = spread over {reltol 1e-9, max step ÷ 4} at decisive points |
| 12 | Switching measures over one period read period-2 orbits as noise; windows must be whole periods of a knob (f_sw) | risks a wrong verdict | engine_types T7; language §8.6 `.window` | A periodicity check (last two periods agree, else `NotPeriodic`); windows in periods of a named knob |
| 13 | The M1e default bench leaves `Power<Out>` unloaded; `assume vout.i` needs a current sink | blocks (P1, P2) | plan §1.3; language §3.2 | One lowering rule |
| 14 | Only BJT device records; no device power or current probes | blocks (P1 `iq`, `tj`; P2 `il_peak`) | engine_types T4 `DeviceOp`; roadmap §4.1 | `DeviceOp` as an enum by device kind, plus terminal currents and power for every device; `V_sense` insertion or `@dev[i]` in the export |
| 15 | Warnings (gmin stepping) on every switching run would count as failures in a filter like round 2's | cost / wrong UNDECIDED | plan §4.4; T10 | Classify ngspice messages per analysis (fatal, warning, benign), with a test per message |
| 16 | The language lacks loop breaking, `min`, `settle`, dB bounds, set literals, conditional `assume`, layout-owned quantities, behavioral loads, a level selector | ergonomics (blocks writing the contract) | language §8.2–8.5; grammar.md; model.md E13 | In order of need: benches and `tran` measures (P1); `where` on `assume` + modes; `loop_gain(break: pin)`; a `#[level(averaged)]` attribute on measures or benches |
| 17 | `not_modeled` can't say "switching losses", and a spec whose margin is below an unmodeled term isn't tagged | ergonomics / honesty | plan §3.4; engine.md §3.4 | Model cards declare what they omit, per loss mechanism; tag model-conditional when the margin < a stated estimate |

---

## 7. Seams to add now (cost today)

Each is a type or plan change in the M3a draft, not an algorithm. The costs are rough estimates of Rust lines at today's draft size.

| # | Seam | Where | Cost now | What it avoids later |
|---|---|---|---|---|
| S1 | `Needs.analyses: Vec<Analysis>` with `Tran` and `DcSweep` variants (unimplemented in M3), `AnalysisSet` as a bit set over it, `Run.results: Vec<AnalysisData>` | engine_types T4 | ~40 lines; the MVP fills only `Op`/`AcPoint`/`AcSweep` | Rewriting `Run`, the protocol frames (T10 `Result` frame) and every stage's reads when the first transient lands |
| S2 | `BenchId` in `Request`, `Row` and the run-table key; `EngineDeck` per bench, one knob map | T3, T6; plan §5.1 | ~20 lines; the MVP has bench 0 only | Re-keying stored run tables (`.spicy/checks/<rev>.json`) and the cross-revision cache |
| S3 | `KnobKind::Mode { levels }`, and `Point` holding a level index for mode knobs (ε only for continuous ones) | T1 §2.1 | ~15 lines + a `debug_assert` that stages skip modes | Every stage's ε arithmetic assuming [−1, 1] |
| S4 | `Scenario { assignments, overrides, bench }` in `Plan`; `Side.scenarios`; the MVP has one scenario, the whole box | T1 §2.3 | ~25 lines | A second knob model for modes, or conditional ranges bolted into `KnobSpec` |
| S5 | `Measured::Beyond { toward: Sense, at: f64 }` plus rows W2b/S3b (Beyond past the bound → FAIL) | T7, T8; plan §3.1 | ~20 lines + 2 row tests | A wrong-verdict class the tables can't express; changing `Measured` after reports are stored |
| S6 | `KnobBinding.targets: Vec<Target>`, a read-back per target | plan §5.1; T10 | ~10 lines | A silent dead binding for every shared knob (temp → thermal ambient, supply → reference) |
| S7 | `RunError::Timeout` and a per-run limit in the pool (default 10× the median so far, floor 1 s) | T3 §4.1, T10 | ~30 lines | A hung check; the fix touches the protocol |
| S8 | `Level` in `Request` and `Method` (`Averaged`, `Switching`, …; MVP: `Circuit` only) and "level" on `Record` | T3, T9 | ~15 lines | Records that can't say which model a PASS rests on (issue 1) |
| S9 | `MeasureDef.smoothness: Smooth \| Kinked \| Discontinuous` (MVP: all `Smooth` or `Kinked` for `abs`) | T7 §2.3 | ~5 lines | The loop's allowance by measure kind (engine.md §5.8) needs it; settling must be flagged |
| S10 | The cone rule as a function on the netlist topology, not on part kinds | plan §6.2; T1 | ~40 lines (union-find, as flatten already does) | UNDECIDED (budget) on the LDO's simplest spec |
| S11 | `DeviceOp` as `enum { Bjt{…}, Mos{id, vgs, vds, gm, gds, vdsat}, Diode{…} }` + `power` for every device | T4 | ~20 lines | A breaking change to the frame format and the region rule |
| S12 | Plan §2.3: the budget in runs per level, time shown as an estimate; plan §1.3: `Power<Out>` + `assume i` → a current sink | plan text | a paragraph each | Verdicts that depend on the worker count; a missing M1e rule |

S1, S2, S5 and S7 are the ones I'd insist on. They all change the protocol or the stored format, and those are the expensive things to change once reports exist.

---

## 8. The ladder

```
 MVP  CE amp                  op + AC, BJTs, one bench, 8 knobs
  │
  ▼
 P1   LDO (Run/Sleep)         + a load knob, modes as scenarios, benches (PSRR, loop gain, load step),
  │                             the first transient measure (14 ms/run, from the DC point), Beyond,
  │                             one IC part (subcircuit + V_ref knob), T_J as a derived measure
  ▼
 P3   Load switch             + MOSFETs, behavioral loads (constant power, UVLO), events and crossings,
  │                             a thermal network, multi-target knobs, the run timeout; corner-friendly
  ▼
 P2   Buck                    + levels (averaged search, switching verification), periodicity,
                                transient band, a much larger budget; later PSS on a backend that has it
```

**Right after the MVP: P1, the LDO.**
- *Why:* it uses the MVP's own device (BJTs) and analyses (op, AC). Everything it adds is a seam listed above, and its transient starts from the DC point, so it's cheap (14 ms). It proves loads, modes and benches before the harder levels problem.
- *What it proves:* a mode-dependent verdict (PM 32.5° in sleep vs 61.2°+ in run); a load range as a knob; a loop-gain measure; the first transient spec with a real FAIL (settling after load release) via `Beyond`; a knob bound inside a subcircuit.
- *What it depends on:*
  - language: benches, `tran` + `min`/`settle`, current probes, `where` on `assume`, a mode literal, one IC part (or a `ShuntRef` prelude kind);
  - M1e: the load bench, AC on a supply, the loop-break splice;
  - engine: S1–S5, S10;
  - `spicy_circuit`/export: subcircuit instances with parameters.

**Then P3** (MOSFETs, behavioral loads, events, thermal, timeouts), **then P2** (levels). P2 before P3 would make the first transient circuit also the first multi-level one.

---

## 9. Open questions

1. **Who owns the averaged model?** Hand-written per IC class (round 1's templates, power §9.4), generated from the switching netlist, or supplied as a second `model:` in the part record? The engine only needs "two decks with the same knob map, one marked as the upper level".
2. **Is "verify the lower level's worst corner × the upper-only knobs" enough?** On the buck the averaged worst corner was 2nd of 2,048 at switching level, 0.32 mV short. Is that luck? A case where the averaged ranking is badly wrong (near f_sw/2) should be built before the rule is trusted.
3. **Mode × statistics:** is a mode a range knob for `sigma(3)` (every mode must pass, as here), or does a yield spec weight modes by duty (engine.md §11)?
4. **The step's phase in the switching cycle:** a knob, a sweep inside the measure, or a declared "worst over phase" measure?
5. **Transient budget:** how long may a `spicy check` take before transient sides need `--deep`? 11 s for P3's 512 runs is fine for sign-off, not for save-time re-checks.
6. **Self-heating:** outer `dtemp` iteration in the backend (2–3 runs per point on P1), or only device models with thermal nodes (fragile on ngspice-42, §4.4)?
7. **The region rule's scope** (§2.6): per device ("must stay active", from the part), per measure, or both?
8. **Behavioral loads:** std part kinds (`PowerLoad`, `ResistiveLoad`, `StepLoad`) or sheet benches only?

---

## Appendix: the scripts (`$P` = `/root/.claude/jobs/443154a8/tmp/engine3/power/`)

| Script / deck | Computes | Section |
|---|---|---|
| `ldo.cir`, `ldo_lg.cir`, `ldo_step.cir`, `ldo_unstep.cir`, `ldo_th.cir` | P1 decks: default bench, loop-break bench, load step, release step, `dtemp` | §2 |
| `run_ldo.py`, `pm.py`, `p1sum.py`, `tune.py` | Nominal, PSRR, per-analysis timing, phase margin vs load and ESR, compensation choice | §2.1, §2.6 |
| `enum_dc.py`, `drop.py`, `iq.py`, `th.py`, `step.py`, `settle.py`, `settle2.py` | 2,048 DC corners; dropout and region; I_q and P_Q1; electrothermal fixed point; load step and band; settling | §2.6 |
| `buck_common.inc`, `buck_sw*.cir`, `buck_avg*.cir` | P2 decks at both levels (start-up, load step, loop gain, tight tolerance) | §3 |
| `tbuck.py` … `tbuck4.py`, `band_buck.py` | Timing, subharmonic scan, averaged PM, model gap, ripple band, efficiency | §3.3, §3.5 |
| `enum_buck.py` → `enum_buck_avg.json`, `enum_buck_sw.json` | The two full keys (1,024 averaged, 2,048 switching corners) | §3.4 |
| `lsw2.cir`, `tlsw.py`, `enum_lsw.py`, `audit_lsw.py` | P3 deck, 512 corners, interior audit, 1-D scans, main effects | §4.3 |
| `t_sw.cir`, `t_vd*.cir`, `lsw.cir` | ngspice probes: switch hysteresis, B-source, `dtemp`, the VDMOS hang and thermal failures | §4.4 |
