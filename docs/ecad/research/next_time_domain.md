# After the MVP: Nonlinear and Time-Domain Analog

> 2026-09-28 · One of five "next circuits" reports. Question: which nonlinear and time-domain circuits come after the CE amp, and where does the current design (engine.md v4, engine_plan.md, engine_types.md M3a draft, language.md v0.1, model.md) break or lack a feature for them?
> Builds on round 1 (`engine_mixed_signal.md` §3.9–3.11 and §4.2–4.4: Schmitt thresholds as folds, the 555 threshold ladder, latch history, the Backward-Euler artefact; `engine_method_redteam.md` §2.1 and `engine_position_alternatives.md` §1: the clipping-THD case). Those findings are not repeated; this report tests them against the **current** design on **ngspice-42**.
> **Where the numbers come from.** Every number marked **[T]** was computed for this report on ngspice-42 (libngspice through `soundness/ng.py`) by the stdlib-Python scripts in `/root/.claude/jobs/443154a8/tmp/engine3/timedomain/` (listed in Appendix A). Times are simulation time on one worker, machine load 0.6–0.8 from other jobs. Cited numbers name their doc and section.

---

## 0. Summary

Four circuits test what the CE amp never does: a **bridge rectifier with a reservoir capacitor** (diode, sine bench, steady state, a surge at switch-on), a **555 astable on TI's own TLC555 vendor model** (a PSpice subcircuit with behavioral sources, frequency and duty from a transient), a **discrete emitter-coupled Schmitt trigger** (three DC solutions, history, thresholds as folds), and a **discrete two-transistor astable multivibrator** (MVP parts only, but it must start oscillating). Run on ngspice, the design breaks in a few concrete places. (1) **The plan's engine tolerances abort transients.** `abstol=1e-15` gives "timestep too small" on the rectifier at 46.6 ms. The band stage's `reltol=1e-9` aborts at 6.9 ms, or costs 15× when it doesn't. So every transient side would end UNDECIDED (simulator). (2) **D-A's default transistor has no junction capacitance.** The multivibrator's transient aborts at engine tolerances, and takes 3.2 s per run at ngspice's defaults (81 ms once capacitances are added). (3) **ngspice's cold `op` inside a Schmitt's hysteresis band returns the unstable middle solution** (2.497 V where the stable states are 1.855 V and 5.000 V). That value varies smoothly with the knobs, so enumeration would print an exact-looking verdict about a state that doesn't exist. (4) **The worst surge sits inside the box:** 10.29 A at every corner of the turn-on phase, 30.71 A at 90°, against a 30 A rating. The audit catches it, but `sigma(3)` can only say UNDECIDED. (5) **The 3 s time clause** sends a 7-knob rectifier (3.11 s), a 5-knob 555 (≈ 14 s) and a 10-knob multivibrator (≈ 83 s) to UNDECIDED (budget). Because the clause uses measured wall time, a side near the limit can route differently from one check to the next. (6) **Transient measures carry step-control noise.** On the vendor 555, a 1e-7 change in RB moves the frequency by +0.64 Hz, while the true 1% nudge moves it by −0.084 Hz: the σ search would get slopes of the wrong sign. (7) **The vendor model stops oscillating at 60 °C**, and M3a has no verdict row for an `Undefined` measure. Most fixes are cheap now: a transient slot in `Run`, a bench per request, tolerance profiles per analysis, typed `Undefined`, a per-run timeout, and a run-count budget (§8). The ladder starts with the rectifier's steady-state specs (§9).

---

## 1. The four circuits, and what each one tests

| Circuit | What the CE amp never does | Parts beyond the MVP |
|---|---|---|
| **A. Bridge rectifier + reservoir** (`circuits/full_bridge_rectifier.spicy`, grounded and sized) | Diodes; a sine bench; a transient that must reach steady state; windowed measures (ripple, minimum, peak current); a surge whose worst case is **inside** a range knob (the turn-on phase); the initial condition as part of the bench | `Diode`, sine source, ESR |
| **B. 555 astable on TI's TLC555 model** (shipped with ngspice: `examples/p-to-n-examples/TLC555.LIB`, "(C) 2011 Texas Instruments", PSpice) | A vendor subcircuit: PSpice syntax, `.include`, behavioral `VALUE`/`IF` sources and `VSWITCH` models; no DC operating point; frequency and duty from crossings; a typical-only model | An IC `part` with `model: spice(…)` (language §6.8) |
| **C. Emitter-coupled Schmitt trigger** (2 NPN, 5 resistors) | Three DC solutions; history (which state is real); thresholds that are fold points; a DC sweep as the analysis | none: MVP parts only |
| **D. Astable multivibrator** (2 NPN, 4 R, 2 C) | An unstable DC point that must break into oscillation; start-up; a transient on MVP parts | none: MVP parts only |

The clipping stage from the brief isn't one of the four: its THD case is already in the suite (`thd`, alternatives §1: 3.657% worst, 24/256 corners fail, 10.6 ms per run). This report adds only a measurement of THD's numerical floor (§6.3).

---

## 2. Circuit A: bridge rectifier with a reservoir capacitor

### 2.1 The circuit

```
            R_s 0.5Ω ±10%          D1          out ──┬────────┬──── load
 ┌─(~)──acs──/\/\──acn──┬──►|──┐   ┌──►|──┐        │        │
 │  17 V pk ±10%        │  D2  ├───┤      ├────────┤      C_res 2200µF ±20%
 │  47…63 Hz            │      │   │  D1  │        │      + ESR 25…100 mΩ
 └────────────acp───────┴──|◄──┘   └──|◄──┘       R_l 100Ω ±5%
                         D3 (to gnd)   D4 (to gnd)   │
                                                    gnd
 12 Vrms secondary (17.0 V peak) · four 1N4007 · reservoir 2200 µF · 100 Ω load (≈ 143 mA)
```

The repo's `full_bridge_rectifier.spicy` has **no ground node**: every node floats. On ngspice-42 it spun at 100% CPU for over 3 minutes and was killed **[T]** (`fb0.cir`). Elaboration would refuse this design (model.md E18, tier 2: no `Ground` net). But the backend must survive decks that hang, not only ones that crash (issue I9).

The diode card is the widely circulated 1N4007 card (`IS=7.02767n RS=0.0341512 N=1.80803 … CJO=1e-11 TT=1e-07`). Its origin is unverified, just as the plan's Q2N3904 card was (plan §10.1 D-A).

### 2.2 Knobs and specs

| Knob | Kind | Range | In the cones of |
|---|---|---|---|
| `temp` | range | −10 … 60 °C | all |
| `ac.amp` | range | 17.0 V ± 10% (mains ± 10%) | all |
| `ac.freq` | range | 47 … 63 Hz (50 and 60 Hz mains) | all |
| `ac.phase` (turn-on phase) | range | 0 … 180° | surge only |
| `c_res.value` | statistical | 2200 µF ± 20% | all |
| `c_res.esr` | range (aging) | 25 … 100 mΩ | all |
| `r_s.value` (winding) | statistical | 0.5 Ω ± 10% | all |
| `r_l.value` | statistical | 100 Ω ± 5% | steady-state sides |

| Spec | Measure | Bound | Worst over 128 corners **[T]** |
|---|---|---|---|
| `v_min` (a 9 V regulator needs headroom) | min of `out.v` over the last cycle | ≥ 12 V | **12.4195 V**: amp −10%, 47 Hz, C −20%, R_l 95 Ω, R_s 0.55 Ω, ESR 0.1 Ω, −10 °C |
| `ripple` | peak-to-peak of `out.v` over the last cycle | ≤ 1 V | **0.8253 V** |
| `d_peak` (repetitive diode current) | max of \|i\| over the last cycle | ≤ 2 A | **1.3072 A** |
| `surge` (switch-on, C discharged) | max of \|i\| over the first cycle | ≤ 30 A (1N4007 IFSM, per the common 1N4001–07 datasheets; to be confirmed on the part record) | corners: **10.287 A**; at 90°: **30.707 A, FAIL** |

As a check, nominal ripple is 0.5272 V, against the hand formula I/(2fC) = 0.659 V. The formula ignores the conduction time, so it overestimates.

### 2.3 The contract, as far as v0.1 goes

`⛔` marks where the syntax runs out, and each mark names what's missing.

```rust
block Rectifier {
    port ac_a: Analog<In>;          // ⛔ a floating AC pair: the default bench (language §8.5, plan §1.3)
    port ac_b: Analog<In>;          //    grounds each Analog<In> through its own source; a transformer
                                    //    secondary is ONE source across two ports, with neither side grounded
    port out: Power<Out>;
    port gnd: Ground;
    let d1 = Diode { a: ac_a, k: out };          // ⛔ Diode is language §6.1, not the MVP (roadmap §4.1)
    let d2 = Diode { a: ac_b, k: out };
    let d3 = Diode { a: gnd, k: ac_a };
    let d4 = Diode { a: gnd, k: ac_b };
    let c_res = Electrolytic { p: out, n: gnd, value: 2200uF ± 20%,
                               esr: 25mΩ..=100mΩ };  // ⛔ no `esr` field in the prelude schema (model.md E8)
    let r_l = Resistor { a: out, b: gnd, value: 100 ± 5% };   // load as a part: a load assumption is post-MVP
}

contract Rectifier {
    assume temp in -10°C..=60°C;
    assume ac in Sine { amp: 17V ± 10%, freq: 47Hz..=63Hz };   // ⛔ signal families are post-MVP (roadmap §4.1),
                                                               //    and `ac` names a port PAIR
    bench steady { tran: 8 cycles, ..default };                // ⛔ benches are post-MVP; "cycles" of a knob
                                                               //    frequency has no syntax (tran: 0ms..=20ms only)
    bench inrush { start: discharged, ac.phase: 0°..=180°, tran: 1 cycle, ..default };
                                                               // ⛔ no initial-condition or phase syntax
    let v = tran(out.v).window(last_cycle);                    // ⛔ tran is post-MVP; .window(1ms..=5ms) takes
                                                               //    absolute times (language §8.6), not cycles
    spec v_min:  v.min() >= 12V on steady;                     // ⛔ .min() isn't in the method list (§8.4)
    spec ripple: v.pp() <= 1V on steady;
    spec d_peak: tran(d1.a.i).window(last_cycle).peak() <= 2A on steady;   // ⛔ current probes are post-MVP
    spec surge:  tran(d1.a.i).peak() <= 30A on inrush;
}
```

Two gaps are deeper than syntax. First, the measurement window has to move with a knob: 8 cycles last 170 ms at 47 Hz but 127 ms at 63 Hz. Second, `surge` needs "the worst turn-on phase", which is really a maximum over an axis, not a knob (§2.6).

### 2.4 Elaboration (model.md)

- **Knobs:** `ac.amp`, `ac.freq` and `ac.phase` come from a signal-family `assume`. E16 creates knobs only from spreads in part fields and from `assume` on a port quantity (`vcc.v`). A `Sine { … }` pattern has two or three range fields, so each needs its own knob path (`ac.amp`, `ac.freq`). Language §5.4 already lists `amp.input.amp` and `amp.input.freq`, so this is an E16 extension, not a new idea.
- **Parts:** `Diode` needs a prelude schema (pins `a`, `k`, no required fields) and a **default model** in M1e. That default moves numbers as much as D-A does **[T]**, at plan-style tolerances with `rshunt=1e12`:

  | Diode model | `v_min` nominal | ripple | d_peak |
  |---|---|---|---|
  | 1N4007 card | 14.2599 V | 0.5272 V | 1.0789 A |
  | ngspice default `D` (IS 1e-14, N 1, no RS, no CJO) | 14.3612 V | 0.5439 V | 1.1601 A |
  | the repo's `DIO` (`n=2`) | 12.8260 V | 0.4786 V | 1.0248 A |

  At a 12 V bound, the gap between two defaults (1.43 V) decides the verdict. Worse, the ngspice default diode **aborts** ("timestep too small" at 13 ms) unless `rshunt` is set, even at ngspice's own tolerances **[T]**. It has no capacitance, so the bridge's AC nodes float whenever all four diodes are off.
- **"Isolated net" vs "floating while off":** model.md §7 q. 4's check is topological. The bridge's `ac_a`/`ac_b` nodes are connected, but only through diodes that are all reverse-biased for part of each cycle. That isn't a design error, but the deck needs `rshunt`, or diode capacitance, to simulate at all. It belongs in M1e/M1f's defaults, written into the report like TNOM (plan §1.3).

### 2.5 The M1f export contract (plan §5)

| Need | Engine deck today (plan §1.4, §5.1) | Works on ngspice? **[T]** |
|---|---|---|
| Sine source with knob parameters | not in the deck: the default bench has DC and `AC 1` only | `V_ac acp acs SIN(0 {k_amp} {k_f} 0 0 {k_ph})` + `alterparam` + `reset`: yes |
| Read-back of source parameters | read-back paths for DC value, R, C, BF | `@v_ac[sin]` returns `[0, 18, 60, 0, 0, 90]`; `@v_ac[sin][1]` = 18: yes, with an index (plan App. B #11) |
| Initial condition "C discharged" | none: "the deck carries no analyses" | `tran … uic` (per request) or `.ic`: yes. It belongs to the bench, not the knob point |
| `rshunt` (or a diode with CJO) | `.options` holds only tolerances and TNOM | yes; it must be written and reported |
| Diode currents in transient | the probe map names op device records | `i(v_ac)` works; per-diode currents need `.save @d1[id]` before the run |
| Read-back of temperature | `@q_q1[temp]` after the run | a diode's `@d1[temp]` reads −273.15 before any analysis, so the read happens after the run, as plan §4.2 already orders |

### 2.6 Through the engine

**Analyses and cost.** Eight cycles with `maxstep` 20 µs take **25 ms per run** (10,615 time points) at reltol 1e-6 and abstol 1e-12 **[T]**. The start-up transient settles fast: cycle-by-cycle minimum 0 → 14.0553 → 14.2447 → 14.2586 → 14.2597 → 14.25983 V, then flat to 1e-5 V through cycle 40. So "the last of 8 cycles" is steady state here. At a slow corner it might not be, which is why a settle check is needed (I13).

**The steady-state cone** has 7 knobs, so 128 corners. Measured: **3.11 s simulated** (3.50 s wall) **[T]**, just over plan §2.3's 3 s clause. So `v_min`, `ripple` and `d_peak` would be **UNDECIDED (budget)** on one worker, with only 7 knobs. On 2 workers they enumerate. The verdict depends on the machine (I5).

**The surge has its worst case inside the box.** First-cycle peak current vs. turn-on phase, other knobs nominal **[T]**:

```
 phase  0°    15°   30°    45°    60°    75°    90°    105°   120°   150°   180°
 peak  7.88  8.06  10.61  16.23  20.55  23.28  24.21  23.28  20.55  10.61  7.88  A
         ▲ corner                        ▲ interior worst                    ▲ corner
```

With the other five knobs at their worst corner, the phase corners give **10.287 A** and 90° gives **30.707 A**. Against IFSM 30 A, every one of the 64 corners passes by 19.7 A, and the truth is a FAIL. How the plan's guards do (plan §2.5):

| Guard | Result **[T]** |
|---|---|
| Enumeration | 10.287 A at every phase corner: PASS on its own |
| Tangent check (1% inward at the worst corner) | 7.8802 → 7.8802 A: exactly flat, no ascent. Near 0° the peak still comes at the sine's crest, so a small phase change does nothing |
| Audit (8 Latin-hypercube points) | caught it with **100 of 100 seeds**; on average 5.35 of the 8 points beat the corner (`rect8.py`), so the ascent starts and finds 90° |
| `sigma(3)` | phase is a range knob; the σ search visits range knobs only at their edges, and step 4's inward nudge improves, so the result is **UNDECIDED (interior peak)** until `--deep` |

So the plan gets `worst_case` right, but by sampling rather than by structure. And `sigma(3)` can never decide a surge spec without the ball search. The real fix is to model the phase as an **axis of the measure** ("the worst over the turn-on phase", like "the maximum over time"), not as a knob (I4, seam S9).

**Numerical band.** The plan's band re-runs the decisive points at reltol 1e-9 (plan §2.7). On this circuit **[T]**:

| Options | Result | Time per run |
|---|---|---|
| reltol 1e-6, **abstol 1e-15** (plan §1.3, as written) | **aborts at 46.6 ms**, "timestep too small" | — |
| reltol 1e-9, abstol 1e-15 (the band deck) | **aborts at 6.86 ms** | — |
| reltol 1e-8, abstol 1e-15 | aborts at 6.88 ms | — |
| reltol 1e-6, abstol 1e-12 | v_min 14.259832, ripple 0.527245, d_peak 1.07895 | 26 ms |
| reltol 1e-7, abstol 1e-12 | 14.259822 · 0.527251 · 1.07891 | 36 ms |
| reltol 1e-9, abstol 1e-12 | 14.259844 · 0.527256 · 1.07892 | **383 ms (15×)** |
| reltol 1e-6, maxstep 5 µs / 2 µs | 14.259851 · 0.527252 / 14.259849 · 0.527256 | 71 / 174 ms |
| reltol 1e-3 (ngspice default) | 14.259862 · 0.527222 · 1.07891 | 18 ms |

abstol 1e-15 A asks for femtoamp accuracy on an amp-scale current; it's right for the CE amp's op (plan §4.5) and wrong for a rectifier's transient. Across every setting that completes, the measures agree to about 3e-5 V. That is the same size as the **run-to-run jitter** from a 1e-7 change in R_l: 8.0e-6 V (v_min), 2.8e-5 V (ripple), 9.8e-5 A (d_peak) **[T]**. The error comes from step control, not from reltol. A 1% nudge moves the three measures by 4.0e-4, 2.5e-4 and 4.5e-4, so the noise is **2–22% of the slope signal**. The σ search can live with that here; on the 555 it can't (§3.4).

### 2.7 The M3a types (engine_types.md)

| Type | What the rectifier needs | Today |
|---|---|---|
| `Run` (T4, §4.2) | a waveform: time + per-probe arrays | `readback, op, devices, ac_points, sweep`; no transient |
| `AnalysisSet` (§4) | `tran` with a stop rule in cycles of a knob, a `maxstep`, `uic` | three booleans |
| `Needs` (§2.3) | transient probes and device currents; per-bench stimuli | op probes, BJT records, AC points, one sweep |
| `DeviceOp` (§4.2) | diode `id`, `vd` | `ic, ib, vbe, vbc, gm, gmu`: BJT-only |
| `Tolerance` (§4) | a profile per analysis kind; a transient band by `maxstep`/jitter | `Engine` (1e-6/1e-15) and `Tight` (1e-9) |
| `Request` / `Prepared` | a bench per request (the steady deck vs. the inrush deck: another source, `uic`) | one deck per tolerance |
| `Cone` (§2.3) | C and ESR are in every transient cone | "the capacitor rule": correct only for op-only measures; must be keyed on the analysis |
| `Measured` (§7) | `Value` plus *where* (t = 7.3 ms); `Undefined` with a reason | `Value(f64)`, `Undefined(&'static str)` |

### 2.8 What the engineer or the agent would ask

- "At which time does the minimum happen, and what does the waveform look like at the failing corner?" The counterexample needs `t*` and a waveform, not only a knob point (I14).
- "Is 8 cycles enough? Show me it settled." That's a settle check whose result is printed.
- "Is the surge worst at the mains peak?" That's a phase sweep; `spicy sweep ac.phase 0°..180° 13` works once benches exist.
- "Which diode model is this?" `model_defaults` (T9) must list the diode default and `rshunt`.
- "What if I add an NTC?" A new part whose resistance falls as it heats up: an electrothermal state, not a knob (engine.md §2.6).

---

## 3. Circuit B: 555 astable on TI's TLC555 vendor model

### 3.1 The circuit

```
 vcc 5V ±5% ──┬──────────────┬────────── TLC555 (TI PSpice model, 437 lines:
              R_a 1k ±1%     │ VCC,RESET   MOSFETs, VALUE/IF sources, VSWITCH)
              ├── dis ───────┤ DISC
              R_b 6.8k ±1%   │
              ├── thr ───────┤ THRES, TRIG ── C_t 100nF ±5% ── gnd
              │              │ CONT ─ 10nF ─ gnd;   OUT ─ 10k pull-up
 f = 1.44/((R_a+2R_b)C) = 986.3 Hz (formula) · the model gives 970.83 Hz at 25 °C [T]
```

### 3.2 Knobs, specs, contract

Knobs: `temp` (−10…60 °C), `vcc.v` (5 V ± 5%), `r_a`, `r_b`, `c_t`: 5 knobs, 32 corners. The part's own spread (the threshold ladder, engine.md §2.5) is **not in the vendor model**. A typical-only model makes the frequency spread look like R and C alone.

```rust
use ti::Tlc555;                                   // ⛔ `part`, `use` of part libraries: post-MVP (roadmap §4.1)
block Astable {
    port vcc: Power<In>; port gnd: Ground; port out: Analog<Out>;
    net dis; net thr; net ctl;
    let u1 = Tlc555 { thres: thr, trig: thr, cont: ctl, reset: vcc, out, disc: dis, vcc, gnd };
    let r_a = Resistor { a: vcc, b: dis, value: 1k ± 1% };
    let r_b = Resistor { a: dis, b: thr, value: 6.8k ± 1% };
    let c_t = Capacitor { a: thr, b: gnd, value: 100nF ± 5%, dielectric: C0G };
    let c_ctl = Capacitor { a: ctl, b: gnd, value: 10nF };
    let r_pu = Resistor { a: vcc, b: out, value: 10k };
}
contract Astable {
    assume temp in -10°C..=60°C;
    assume vcc.v in 5V ± 5%;
    bench run { start: power_on, tran: 12 cycles, ..default };   // ⛔ benches, start-up, cycles
    let w = tran(out.v).after(cycles: 3);                         // ⛔ "skip the first cycles": no syntax
    spec freq: w.frequency(level: vcc.v / 2) in 1kHz ± 10% on run;   // ⛔ .frequency(), .duty(): not in §8.4;
    spec duty: w.duty(level: vcc.v / 2) <= 60% on run;              //    a level that depends on a knob
}
```

In the part record (language §6.8): `model: spice("models/TLC555.LIB", subckt: "TLC555", fidelity: typical_only, dialect: pspice)`. The `dialect` field is missing today and is needed (below).

### 3.3 The export contract and the backend

| Finding **[T]** | Consequence | Where it hits |
|---|---|---|
| Without `set ngbehavior=ps`, the library fails to load ("Formula() error" ×n; "there aren't any circuits loaded") | The deck must carry the simulator's compatibility mode and its `.include`s. In libngspice a netlist error kills the library (plan §4.3 rule 4), so this costs a worker restart per attempt | plan §5.1 `EngineDeck { text, knobs, probes }` has no includes and no mode |
| The DC operating point fails ("Transient op failed, timestep too small"). A relaxation oscillator has no stable op | The run must start with `uic` (every node at 0 V, as at power-on) | `AnalysisSet` has no start condition |
| Engine tolerances (reltol 1e-6, abstol 1e-12) abort at **5.11 ms** of 12 | Another tolerance profile is needed | plan §1.3 |
| reltol 1e-3 / 1e-4 / 1e-5: **317 / 440 / 790 ms per run**; f 971.91 / 970.83 / 970.29 Hz | 32 corners ≈ **14 s** at 1e-4: **UNDECIDED (budget) with 5 knobs**. The frequency moves 0.17% with the tolerance setting | plan §2.3 |
| The first period (edge to edge) is 1.0126 ms; the steady period is 1.0294 ms (1.6% longer). The first high phase ends at 0.61 ms | Period and duty must skip the first cycles (round 1 said this for the formula: `engine_mixed_signal.md` §3.10) | measure definition |
| Knob values persist across runs (`alterparam` is sticky) | The engine must set **every** knob on every run, as plan §4.2 does. A harness that sets only the changed ones silently inherits the last corner (it happened in this study's first script) | a backend-rule test |

### 3.4 Through the engine

**The model stops oscillating at 60 °C.** Frequency vs. temperature, other knobs nominal **[T]** (reltol 1e-4 and 1e-3 agree):

```
 −10°C   25°C    40°C    45°C    50°C    55°C    60°C
 972.46  970.83  970.04  969.63  968.78  959.35  none   Hz
                                          ▲ −1% step     ▲ threshold node reaches 5.000 V and never trips
```

At 60 °C the threshold node charges to VCC and the output stays high for 50 ms. Whether the real part does that is a datasheet question. The 1% step between 50 and 55 °C, followed by a cliff, looks like a model artifact (behavioral `IF` thresholds meeting level-1/3 MOSFET temperature dependence). What the design does with it today:
- At 60 °C with the other knobs nominal, `frequency = Undefined`; half of the 32 corners sit at 60 °C, and each one that also stops (not run here) is the same case. engine_types §7 says `Undefined` "can never pass", but the verdict tables (plan §3.1, T8) have **no row** for it. `SideEvidence.inner` is `Option<(f64, RowId)>`, so an `Undefined` row is either silently skipped (a false PASS risk) or has to become a FAIL whose counterexample is "60 °C: no oscillation" (possibly false). Neither is decided (I6).
- The claim a user needs is: *"freq FAILS at worst case: the simulator shows no oscillation at 60 °C within 12 ms (threshold node stuck at 5.000 V). The vendor model is typical-only; confirm the part's behavior at 60 °C."*

**Slopes are noise.** Frequency (from the last period) at reltol 1e-4 **[T]**:

| Change | Δf | What it should be |
|---|---|---|
| R_b × (1 + 1e-7) | **+0.642 Hz** | ≈ 0 |
| R_b × (1 + 2e-7) | +0.490 Hz | ≈ 0 |
| R_b + 0.68 Ω (the plan's nudge, h = 0.01) | **+0.336 Hz** | **−0.084 Hz** (the +1% run gives 962.40 Hz, so −8.43 Hz per 68 Ω) |

Cycle-to-cycle, the period wanders 1.0294–1.0302 ms (8e-4 relative). The plan's σ search takes one nudge per knob (plan §2.6), so here it would jump in the wrong direction. Averaging 4 periods cuts the noise (+0.015 Hz on the nudge) but doesn't remove it. The band rule `2·|f − f_tight|` can't measure this either, because the tight run aborts (I7).

### 3.5 What they'd ask

"Why does the frequency differ from the formula by 1.6%?" (the model's thresholds and discharge resistance, not R and C). "Does the real TLC555 work at 60 °C?" (read the part record's `operating { temp … }`). "What's the spread from the part itself?" (not in the model: a wrapper knob on the threshold ladder, engine.md §6.6 and §2.5).

---

## 4. Circuit C: emitter-coupled Schmitt trigger

### 4.1 The circuit and its three solutions

```
 vcc 5V ──┬─────────────┬
         R_rc1 2.2k    R_rc2 1k
          ├─ c1 ─ R_r1 10k ─ b2 ─ R_r2 10k ─ gnd
 in ── Q1 (b)          Q2 (b = b2)     out = Q2 collector
          └──── e ─────┴── R_re 470 ─ gnd       (D-A's Npn for both)
```

Up-sweep and down-sweep (`dc v_in 0 5 1m`, then `5 0 -1m`) **[T]**: **VT+ = 2.019–2.020 V**, **VT− = 1.209–1.210 V**, hysteresis 0.81 V. Each sweep has 5,001 points and takes 9.4–10.2 ms. The two stable outputs are 1.855 V (Q2 on) and 5.000 V (Q2 off).

**The cold operating point inside the band** (plan rule 10: every run cold) **[T]**:

| v_in | 1.0 | 1.5 | 1.8 | 1.9 | 2.0 | 2.1 |
|---|---|---|---|---|---|---|
| cold `op`: out | 1.855 | **3.902** | **2.860** | **2.497** | **2.088** | 5.000 |
| stable states | 1.855 | 1.855 / 5.000 | 1.855 / 5.000 | 1.855 / 5.000 | 1.855 / 5.000 | 5.000 |

Inside the band, ngspice's cold `op` lands on the **unstable middle solution**, every time and deterministically. It isn't an occasional misconvergence. `.nodeset` steers the op to either stable state (1.8551 V and 5.0000 V at v_in 1.9). A transient from the cold op with a 1 µV kick stays at 2.4971 V **[T]**. D-A's model has no junction capacitance, so the circuit has no dynamics that could reveal the instability.

This is round 1's latch finding (`engine_mixed_signal.md` §2.1), now on the actual backend and with the engine's default model. It is worse than "a second solution is never found" (plan §12 q. 5): the one solution that *is* found doesn't physically exist, and it varies smoothly with every knob. A spec like `dc(out.v) <= 0.5V` at an input in the band would enumerate cleanly into a precise, meaningless verdict.

### 4.2 Contract

```rust
contract Schmitt {
    assume temp in -10°C..=60°C;
    assume vcc.v in 5V ± 5%;
    let up   = dc_sweep(in.v, 0V..=5V).threshold(out.v, level: 3.5V);    // ⛔ a DC sweep as an analysis, the
    let down = dc_sweep(in.v, 5V..=0V).threshold(out.v, level: 3.5V);    //    sweep direction (= history), and
    spec vt_hi: up in 2.0V ± 0.15V;                                      //    .threshold(): none of them exist
    spec vt_lo: down in 1.2V ± 0.15V;
    spec hyst:  up - down >= 0.6V;          // per-run evaluation (D-F) works if both sweeps are in one run
}
```

**History lives inside a run.** Both sweeps of one knob point are one request, so `up − down` shares its knobs exactly (D-F's property), and every run stays cold and bit-identical (rule 10 holds). The history is the sweep's direction, part of the analysis, never carried from one run to the next. This is the principle that answers plan §12 q. 5 (seam S11).

### 4.3 Through the engine

- **Knobs:** temp, vcc, 5 resistors, 2 β = 9, so 512 corners. The full-resolution cost is 512 × 20 ms ≈ 10 s: **UNDECIDED (budget)**. A coarse sweep plus a fine window (≈ 300 points each way, ≈ 2 µs per point **[T]**) would be ≈ 1.2 ms per run and 0.6 s per check.
- **The threshold measure is a staircase:** it's quantized to the sweep step. A 1% nudge of R_rc1 (0.22 Ω) leaves VT+ in the same 1 mV bin (2.0190–2.0200 V); +1% moves it to 2.0170–2.0180 **[T]**. So slopes are 0 or one bin, and the σ search sees nothing. The fold needs a located crossing: bisection, or interpolation at the jump (round 1 §4.4 recommends event location). The measure library must report the threshold with its resolution as its measure band.
- **Verdict semantics:** VT± are smooth functions of the knobs (fold points move smoothly), so once located they suit enumeration. The danger is the plain `dc(…)`.

---

## 5. Circuit D: astable multivibrator (MVP parts, transient specs)

`Q1`/`Q2` D-A Npn, R_c 1 kΩ, R_b 10 kΩ, C 68 nF, VCC 5 V. The formula f = 1/(2·ln2·R_b·C) gives 1060.9 Hz.

| Setting **[T]** | Result | Time per run |
|---|---|---|
| **D-A model as accepted** (no CJE/CJC/TF), engine tolerances | **aborts** at 75–185 µs ("timestep too small", node c1): from the op, with `uic`, nominal and off-nominal alike | — |
| D-A model, reltol 1e-3 | completes: 996.64 Hz, duty 0.4556 | **3,258 ms** |
| D-A + the Q2N3904 card's capacitances (`CJE=4.493p CJC=3.638p TF=301.2p TR=239.5n`), engine tolerances | completes: **995.12 Hz**, duty 0.4556 | **81 ms** |

- **D-A is unfit for transient.** Without capacitances, a regenerative edge is instantaneous and the step controller shrinks to 1e-18 s. So D-A needs a transient-capable form before any transient spec (I2). Capacitances don't change the DC operating point; whether they move the 1 kHz gain or f_low is not measured here, and must be checked on the MVP key (§10 q. 7).
- **Start-up is noise-defined.** At perfectly symmetric nominal values the circuit does start from the op (ngspice's rounding breaks the symmetry). The first rising edge comes at 97 µs from the op and at 133 µs with `uic` **[T]**. Any spec tied to absolute time ("output high at t = 5 ms", "start-up time") is decided by numerical noise at symmetric points. Measures on autonomous circuits must be phase-invariant (period, duty, amplitude over the last N cycles), and a start-up spec needs a defined kick, such as a supply ramp (I15).
- **Noise is fine here:** a 1e-7 change moves f by 4.4e-4 Hz, while the nudge moves it by −0.0496 Hz (1% noise). Temperature −10 / 60 °C: 981.99 / 1008.37 Hz. So transient noise depends on the model: smooth device models are quiet, and behavioral switches (the 555) are loud.
- **Cost:** 10 knobs (4 R, 2 C, 2 β, temp, vcc), 1024 × 81 ms ≈ **83 s**. Even on 16 workers (scale §3.3's 6–9× speed-up) it's ≈ 10 s. Symmetric pairs could be exploited (swapping Q1's and Q2's parts mirrors the waveform, so it halves the corners for frequency), but that's a structural rule nobody has designed.

In the language this block is fully MVP (Npn, Resistor, Capacitor). Only the contract needs `tran(c2.v).after(cycles: 3).frequency(…)`, which makes it the cheapest transient circuit to *write*.

---

## 6. Across the four circuits

### 6.1 Cost against the enumeration budget (plan §2.3: ≤ 4096 corners and ≤ 3 s estimated)

| Circuit | Cone | Corners | Per run **[T]** | One worker | Routing today |
|---|---|---|---|---|---|
| CE amp (reference) | 8 | 256 | 0.71 ms (plan §2.3) | 0.18 s | enumerate |
| Rectifier, steady state | 7 | 128 | 25 ms | **3.11 s** (measured) | UNDECIDED (budget), by 0.11 s |
| Rectifier, surge (1 cycle) | 6 | 64 | 3.4 ms | 0.22 s | enumerate, but the worst case is inside the box |
| TLC555 astable | 5 | 32 | 440 ms (reltol 1e-4) | ≈ 14 s | UNDECIDED (budget) |
| Schmitt, full sweeps | 9 | 512 | ≈ 20 ms | ≈ 10 s | UNDECIDED (budget); ≈ 0.6 s with coarse+fine sweeps |
| Multivibrator | 10 | 1024 | 81 ms | ≈ 83 s | UNDECIDED (budget) |

The count limit (12 knobs) never binds in this domain; the time clause always does. D-D said "over budget is UNDECIDED (budget) until the loop". For transient specs, the loop isn't a way out either: it needs slopes (§3.4) and runs per knob. The practical path is a larger budget for the transient tier, parallel workers, and cheaper measures (coarse sweeps, PSS by shooting, engine.md §5.9).

### 6.2 The numerical band for transient measures

| Measure | Jitter (1e-7 knob change) | 1% nudge signal | Spread across tolerance settings | What the plan's band does |
|---|---|---|---|---|
| Rectifier v_min | 8.0e-6 V | 4.0e-4 V | ≈ 3e-5 V | tight re-run **aborts** |
| Rectifier ripple | 2.8e-5 V | 2.5e-4 V | ≈ 3e-5 V | aborts |
| Rectifier d_peak | 9.8e-5 A | 4.5e-4 A | ≈ 6e-4 A (maxstep 200 µs) | aborts |
| 555 frequency | **0.64 Hz** | **0.084 Hz** | 1.6 Hz (1e-3 vs 1e-5) | aborts |
| Multivibrator frequency | 4.4e-4 Hz | 0.050 Hz | — | not tried |
| Schmitt VT+ | step-quantized, 1 mV bins | < 1 bin | — | reltol doesn't change a bin |

For transients, the band has to be measured another way: re-run with half the `maxstep`, plus a jitter estimate from 2–3 runs perturbed by 1e-7. A measure whose jitter exceeds its nudge signal can't feed a slope-based search (σ, the loop). It gets UNDECIDED (noise) at `sigma(3)`, or larger nudges.

### 6.3 THD's floor (the clipping stage, briefly)

A THD measure by uniform resampling plus a DFT over the last 4 cycles, on a **linear** RC driven by a 1 kHz sine, where the truth is 0 **[T]** (`thd.py`):

| Options | Measured THD of a linear circuit |
|---|---|
| reltol 1e-6, maxstep 10 µs | 1.7e-5 (−95.5 dB) |
| reltol 1e-3, maxstep 10 µs | 1.1e-7 (−139 dB) |
| either reltol, maxstep 1 µs | 1.1e-9 (−179 dB) |

The floor is set by the time grid and the interpolation, not by reltol, and it isn't even monotone in reltol. A diode clipper reads 15.64–15.69% under all four settings. So large THD is robust, but a spec near −100 dB needs a measured floor (the maxstep re-run above). Otherwise it's engine.md's UNVERIFIABLE case.

### 6.4 What a counterexample means when the failure is a waveform

Today a counterexample is a knob point, pasteable into `spicy sim --at` (engine.md §3.5), and `sim` returns node voltages and measures. For these circuits the engineer needs three more things:
1. **Where on the axis:** "the minimum is 12.42 V at t = 164.3 ms, in cycle 8", or "surge 30.7 A at t = 5.1 ms after switch-on at 90°". The measure knows this; `Measured::Value` drops it.
2. **The waveform:** `spicy sim --at v_min.max --plot out.v,i(d1)` writes the trace (CSV or a `.raw` file), with the window and the violation marked.
3. **The kind of failure:** "no oscillation within 12 ms" is a different claim from "frequency 890 Hz". It needs its own words and a `next` step ("run 50 ms to rule out slow start-up"; "the vendor model is typical-only").

---

## 7. Issues, ranked

| # | Issue | Severity | Hits | Suggested fix |
|---|---|---|---|---|
| **I1** | The engine tolerances (`abstol=1e-15`) abort transients (rectifier at 46.6 ms), and the band's reltol 1e-9 deck aborts (6.86 ms) or costs 15× (383 vs 26 ms). Every transient side ends UNDECIDED (simulator), and the band is never measured | *blocks the circuit* | plan §1.3, §2.7, §4.5; engine_types `Tolerance` (§4), `Row::tolerance` (§6) | Tolerance **profiles per analysis kind**: op/AC as today; transient reltol 1e-6, abstol 1e-12, an explicit `maxstep` rule (e.g. period/1000). The transient band = a re-run at maxstep/2 plus a 1e-7 jitter estimate (§6.2) |
| **I2** | D-A's default Npn has no CJE/CJC/TF. Transient aborts (multivibrator), takes 3.2 s per run at ngspice defaults, and can't show a Schmitt's unstable point | *blocks the circuit* | plan §10.1 D-A, §5.2, §5.4; engine.md §6.6 | Extend D-A with the capacitances of the card it already cites (`CJE=4.493p CJC=3.638p TF=301.2p TR=239.5n`: 81 ms per run); confirm the MVP key doesn't move. Decide a **default `Diode`** the same way (the 1N4007 card vs `n=2`: 1.43 V apart), plus `rshunt` |
| **I3** | The cold `op` inside a hysteresis band returns the unstable middle solution (2.497 V; stable states 1.855 and 5.000 V), smoothly in every knob. Enumeration of a `dc()` measure then gives a precise verdict on a state that doesn't exist | *risks a wrong verdict* | engine.md §4.7; plan §12 q. 5, rule 10 | History lives **inside a run**: a bench states it (sweep from below or above, power-on ramp), and the run stays cold. At the nominal and decisive points, run the op from two nodesets (output high, output low); if they differ, a `dc()` measure without a history gets **UNDECIDED (multi-stable)**. 2–3 extra runs per side |
| **I4** | The worst surge is inside a range knob (the phase): corners 10.29 A, 90° 30.71 A vs 30 A. The tangent check is flat (7.8802 → 7.8802), so it rests on the audit (100/100 seeds, luck by design), and `sigma(3)` is always UNDECIDED (interior peak) | *risks a wrong verdict* | plan §2.5, §2.6 step 4, S8; engine.md §5.4 | Model the phase (and input level, load step time) as an **axis of the measure** ("max over phase", a dense 1-D sweep inside the bench), not a knob. Or give `KnobSpec` a shape `Interior`/`Periodic` that always gets a 1-D search |
| **I5** | The 3 s time clause sends 5–10-knob transient circuits to UNDECIDED (budget): rectifier 3.11 s at 7 knobs, 555 ≈ 14 s, multivibrator ≈ 83 s. Because it uses a measured run time, the routing, and so the verdict, depends on machine load and worker count, against plan §8.3 #6 (deterministic output) | *cost*, and *risks a wrong verdict* (nondeterminism) | plan §2.3, §12 q. 8, D-D | A budget in **runs × a deterministic cost weight per analysis** (or a time estimate frozen per `rev` in the store), a separate budget for the transient tier (tens of seconds, on demand, as §1.1 already implies), workers by default when a check exceeds 1 s (engine_types §13 q. 1) |
| **I6** | An `Undefined` measure (no oscillation at 60 °C in the vendor model; no crossing; not settled) has no verdict row. `SideEvidence.inner` skips it: a silent false PASS, or an unreasoned FAIL on a model artifact | *risks a wrong verdict* | engine_types §7, §8 (T8); plan §3.1 | `Undefined(kind)`: `NoEvent` (physical: FAIL with the waveform as counterexample, after a longer-window confirmation run), `NotSettled` and `WindowTooShort` (UNDECIDED, `next` = a longer run), `Numerical`. One row after W0, before W2 |
| **I7** | Transient measures carry step-control noise. On the 555, a 1e-7 change moves f by +0.64 Hz while the true nudge is −0.084 Hz, so the σ search's one-nudge slopes have the wrong sign. The rectifier's noise is 2–22% of its slope signal | *risks a wrong verdict* (at `sigma(3)`) | plan §2.6 (nudge h = 0.01), §3.1 S6/S11; engine.md §5.5 | Measure each measure's jitter at nominal (2–3 perturbed runs). If jitter > nudge signal/10, widen h or fit over several points; if still noisy, UNDECIDED (noise). Record the measure's noise floor in `SideEvidence` |
| **I8** | Vendor models need a PSpice mode (`ngbehavior=ps`; without it, "Formula() error" and no circuit), `.include`, and a `uic` start (the op fails). They are typical-only (no ladder spread) and can have cliffs (−1% at 55 °C, dead at 60 °C) | *blocks the circuit* | plan §5.1 `EngineDeck`; engine.md §6.6; language §6.8 | `EngineDeck { includes, dialect, start }`; a model-coverage tag "typical-only: the part's own spread isn't modeled" on every verdict that uses it; wrapper knobs (the 555 ladder, engine.md §2.5) in the part record |
| **I9** | A deck can hang the worker: the repo's floating rectifier spun for over 3 minutes. The worker is restarted only on a crash or a closed pipe | *blocks the circuit* (the check never returns) | engine_types T10 §10.3; plan §4.1, §4.3 rule 4 | A per-run wall-clock timeout (e.g. 100× the nominal run, at least 1 s) → kill and restart → `RunError::Timeout` → W4 |
| **I10** | The M3a types have no transient: `Run` has no waveform, `AnalysisSet` is three booleans, `Needs` has no transient probes or device currents, `DeviceOp` is BJT-only, one deck per tolerance (no bench) | *blocks the circuit* | engine_types T4 §4.2, §2.3, §4 | Seams S1–S4 below: cheap as types now, a retrofit through every stage later |
| **I11** | The language can't write these contracts: no `Diode` in the MVP prelude, no `Sine` assumption, no benches, no `tran` methods (`min`, `frequency`, `duty`, `threshold`), no cycle-based windows, no current probes, no ESR field, no floating source across a port pair | *blocks the circuit* (post-MVP by plan) | roadmap §4.1; language §8.2, §8.4–8.6; model.md E8, E16 | Stage them in §9's order. Two design points to settle before the syntax: windows **in cycles of a stimulus knob**, and a **differential source** in the default bench |
| **I12** | Steady state is assumed: measures on "the last cycle" need a settle check, and the stop time depends on the frequency knob (8 cycles = 127–170 ms) | *risks a wrong verdict* | language §8.6 (`.window` in absolute time); T7 | The window is defined in cycles; the measure compares the last two cycles and returns `Undefined(NotSettled)` beyond a tolerance |
| **I13** | The capacitor rule for cones must be keyed on the analysis: for a transient measure every C and ESR is in the cone | *risks a wrong verdict* if applied blindly | engine_types §2.3; plan §6.2 `Cone` comment | Compute the cone per measure from its `AnalysisSet`: op-only measures drop C/L, everything else keeps them. One `match` |
| **I14** | A waveform counterexample needs *where* (t*, cycle) and the trace; `Measured` and `Record` carry only a value | *ergonomics* | engine_types §7, §9; engine.md §3.5 | S5 below; `spicy sim --at … --plot` writes the trace |
| **I15** | Absolute-time measures on autonomous circuits are noise-defined: the first edge comes at 97 µs from the op and at 133 µs from `uic` on the same symmetric circuit | *risks a wrong verdict* | language §8.4 (`.crossing(…)`) | A lint: time measures on a circuit with no driving stimulus must be phase-invariant; start-up specs need a declared kick (supply ramp) |
| **I16** | THD's numerical floor is set by `maxstep` and resampling (−95 to −179 dB for a linear circuit), not by reltol | *cost* / *risks a wrong verdict* near a floor | plan §2.7; engine.md §3.3 "UNVERIFIABLE" | A band by maxstep re-run (I1); a THD bound within 10 dB of the measured floor gives UNDECIDED (numerics) |

---

## 8. Seams to add now

Each costs lines in the M3a note today, and saves a change through every stage later.

| # | Seam | Where | Cost now | What it avoids |
|---|---|---|---|---|
| **S1** | `Run.tran: Option<TranData>` (time + one array per transient probe) and `Needs.tran: Vec<TranNeed { bench, stop: Stop, maxstep, probes, currents }>`, with `Stop::Cycles { n, of: KnobId }`. `AnalysisSet` becomes a small list of `AnalysisId`s instead of three booleans | engine_types §2.3, §4, §4.2 | ≈ 30 lines of types; the worker ignores `tran` in M3 | Changing `Run`, `Needs`, the protocol frames (T10) and every stage's row reading when the transient tier arrives |
| **S2** | `Request.bench: BenchId`. `Prepared` holds one deck per `(bench, ToleranceProfile)`; `Tolerance { Engine, Tight }` becomes a profile chosen per analysis kind | engine_types §4; plan §4.2 | ≈ 20 lines; M3 has one bench and the two profiles it has today | The rectifier's steady and inrush decks, the Schmitt's two-history deck, and I1's transient profile all need it; today `Prepared` assumes one circuit |
| **S3** | `DeviceOp` → `DeviceRecord { kind: DeviceKind, values: Box<[f64]> }` with a per-kind field list (BJT: ic, ib, vbe, vbc, gm, gmu; diode: id, vd; MOSFET later) | engine_types §4.2 | ≈ 15 lines; the region rule reads the BJT fields by index | A breaking change to `Run`, the frames and the region tag the day a diode appears |
| **S4** | `Measured::Undefined(UndefinedKind)` with `NoEvent`, `NoCrossing`, `NotSettled`, `WindowTooShort`, `Numerical`; one verdict row for it in each table | engine_types §7, §8; plan §3.1 | one enum, two rows, two tests. The CE amp's f_low "no crossing" already needs it | I6: a silent skip or an unreasoned FAIL |
| **S5** | `Measured::Value { v, at: Option<AxisPoint> }` (time, frequency, sweep value, cycle) carried into `Record` | engine_types §7, §9 | one field; `f_low` already knows its frequency | I14; also answers "at which frequency is the worst gain" for AC |
| **S6** | `RunError::Timeout` and a per-run watchdog in the worker protocol | engine_types §4.1, §10.3 | one variant, a timer in the pool | I9: a check that never returns |
| **S7** | The enumeration budget in runs × a deterministic cost weight per analysis kind (or a nominal-run time frozen per `rev`), never live wall time | plan §2.3; engine_types `Policy.settings` | a decision plus a field | I5's nondeterminism; the budget stays meaningful when transient runs cost 30–500× an op |
| **S8** | `SideEvidence.noise: Option<f64>` (jitter from perturbed nominal runs) and `BandMethod { Reltol, MaxStep, Jitter }` per measure kind | engine_types §8; plan §2.7 | two fields; M3 fills `Reltol` | I1, I7, I16 without redoing the band |
| **S9** | Measure axes: a `Program` node `WorstOver { axis: Axis, grid }` (phase, input level, load-step time) that evaluates one bench sweep inside a run; or a `RangeShape { Box, Interior, Periodic }` on `KnobSpec` | engine_types §2.1, §7 | an enum variant, unused in M3 | I4: interior range knobs that neither corners nor the σ search can handle |
| **S10** | `EngineDeck { text, knobs, probes, includes: Vec<PathBuf>, dialect: Option<Dialect>, starts: Vec<Start> }` with `Start { Op, Uic, Nodeset(Vec<(NodeId, f64)>) }` | plan §5.1 (published in M3a) | three fields in a type the other session is about to implement; cheap only **before** M1f | I8, I3; changing M1f's output contract later needs both tracks |
| **S11** | History as bench data: `History { Cold, SweepFrom(Low \| High), PowerRamp { t_rise: KnobId } }`, and a `Nominal`-stage hook that runs two histories when the circuit has a positive-feedback structure | engine_types §5.1 (stages) | a type and an empty hook | I3: plan §12 q. 5 answered by structure instead of by a new stage later |
| **S12** | Record, as decisions next to D-A: the transient form of the default Npn (capacitances), the default `Diode`, `rshunt` | plan §10.1, §1.3; M1e | a decision, no code; `model_defaults` (T9) already prints them | I2: the first transient circuit otherwise reopens D-A |

S1–S6 and S10 are the ones worth doing in this M3a revision. S7, S8, S11 and S12 are decisions to write down now, so nothing in M3 assumes the opposite.

---

## 9. The ladder

**Right after the MVP: the rectifier's three steady-state specs** (`v_min`, `ripple`, `d_peak`; no surge yet).

| Why this one | |
|---|---|
| Smallest step into the time domain | One new part (`Diode`), one bench (sine), one analysis (`tran`), three plain measures (`min`, `pp`, `peak` over a window) |
| Well-behaved numerically | Jitter 8e-6–1e-4, 2–22% of the slope signal: both enumeration and the σ search work |
| A known answer | Every corner is cheap to key (128 corners, 3.5 s); a hand-formula sanity check (I/(2fC)) |
| Real value | Every mains-powered board has one, and ripple-vs-regulator-headroom is a spec people get wrong at low line |

**What it proves:** transient data in `Run` (S1), a bench per request (S2), transient tolerance profiles and a transient band (I1, S8), the diode default (S12), cycle windows with a settle check (I12), analysis-keyed cones (I13), and a budget that isn't machine-dependent (S7). **It depends on:** a `Diode` prelude kind and an M1e default diode model; `assume … in Sine { … }` for a floating port pair (or a bench written by hand in the suite format, T11, until the language has it); `tran` and three methods in the measure library; `rshunt` in the export.

**Then, in order:**
1. **Rectifier surge**: interior worst case on a phase axis (S9, I4); the `uic` start (S10).
2. **Multivibrator**: MVP parts, phase-invariant frequency and duty (I15); needs D-A with capacitances; stresses cost (1024 corners × 81 ms) and so parallel workers or the transient budget.
3. **Schmitt trigger**: DC sweeps with direction as history (S11), fold location and multi-stability detection (I3).
4. **Vendor 555**: dialect and includes (I8), `Undefined(NoEvent)` (I6), noisy measures (I7), typical-only coverage tags. Last, because it needs the most of everything, and its answers depend on the least trustworthy model.

---

## 10. Open questions

1. **A transient tolerance profile:** is `reltol 1e-6, abstol 1e-12, maxstep = period/1000` right beyond the rectifier? The multivibrator ran at abstol 1e-12; the 555 needed reltol 1e-4. Should the profile be per model (vendor models looser), with the looseness printed?
2. **Undefined as FAIL:** when is "no oscillation in simulation" a physical FAIL rather than a model artifact? A longer confirmation run rules out slow start-up, but not a model cliff (the TLC555 at 60 °C). Should any verdict that rests on a typical-only vendor model be capped at UNDECIDED unless it FAILs clearly?
3. **The transient budget:** the brief's circuits need 3–83 s per check on one worker. Is a transient tier "on demand, minutes allowed" (plan §1.1 "Later") acceptable to the user, or must enumeration give way to something cheaper (PSS by shooting, engine.md §5.9) before transient specs ship?
4. **Axes vs knobs:** is the turn-on phase an axis of the measure (the worst over a sweep inside one run) or a range knob with an interior shape? The axis is cheaper and exact on a grid, but hides the phase from the counterexample unless S5 carries it.
5. **History syntax:** `dc_sweep(in.v, 0V..=5V)` puts history in the measure; `bench rising { … }` puts it in the setup. Which reads better on the I/O page?
6. **Symmetry:** the multivibrator's mirror symmetry halves its corners. Is that worth a structural rule, or is it a one-circuit trick?
7. **Does adding capacitances to D-A move any MVP number?** Expected no for DC and within 1e-6 for the 1 kHz gain, but it must be measured on the key before S12 is accepted.

---

## Appendix A. Scripts (`/root/.claude/jobs/443154a8/tmp/engine3/timedomain/`)

| Script | What it computes | Section |
|---|---|---|
| `td.py` | libngspice transient driver (over `engine2/soundness/ng.py`), crossing interpolation | all |
| `fb0.cir` | the repo's floating bridge (hangs ngspice) | §2.1 |
| `rect1.py` | rectifier deck, 40-cycle settling | §2.6 |
| `rect2.py`, `rect2_main.py` | surge vs phase, jitter, nudge effect | §2.6 |
| `rect3.py`–`rect6.py` | tolerance and maxstep grid; diode defaults; `rshunt` | §2.4, §2.6 |
| `rect7.py`, `rect8.py` | surge at phase corners vs 90°; tangent nudge; audit catch rate over 100 seeds | §2.6 |
| `rect_enum.py` | 128 steady-state corners, timing and worst values | §2.2, §6.1 |
| `t555.py`, `t555b.py`–`t555e.py` | TLC555 astable: op failure, `uic`, tolerances, noise vs nudge, temperature cliff, PSpice mode | §3 |
| `schmitt.py` (+ inline runs) | sweeps, cold op in the band, nodesets, the staircase | §4 |
| `mv.py` (+ inline runs) | multivibrator with and without capacitances; jitter | §5 |
| `thd.py` | THD floor on a linear RC and a clipper | §6.3 |
