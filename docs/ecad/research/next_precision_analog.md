# Next Circuits: Precision Analog and Op-Amp Stages

> Research report · 2026-09-28 · Stress-tests the **current** design (`engine.md` v4, `engine_plan.md`, `engine_types.md` M3a draft, `language.md` v0.1 + roadmap §4.1, `model.md`) against op-amp circuits. It builds on round 1's `engine_precision_analog.md`, which tested engine v2 with closed-form models. This round runs **real vendor models on ngspice-42**.
> **Where the numbers come from:** every number marked **[P]** was computed for this report by the stdlib-Python scripts in `/root/.claude/jobs/443154a8/tmp/engine3/precision/` (`e1.py` … `e5.py`, `e4lib.py`), using libngspice through round 3's `soundness/ng.py`, with `.options tnom=25 reltol=1e-6 vntol=1e-9 abstol=1e-15` unless stated otherwise. The vendor models are the TI OPA171 (Green-Williams-Lis architecture, 2017) and TL072 (1989 macromodel) that ship in ngspice's own `examples/` tree (`p-to-n-examples/OPA171.txt`, `probe/TL072.301`). Datasheet numbers come from TI SBOS516H (OPA171, §5.7) and SLOS080W (TL07x, §5.x, TL07xBC column). The scripts stay out of the repo, as decided.

---

## 0. Summary

The CE amp tests one transistor, three AC or DC measures and eight knobs. Op-amp circuits break the design in four places it hasn't looked at yet. **(1) Vendor models are the first wall, and not the one we expected.** TI's OPA171 model needs ngspice's PSpice mode, works only through `.include` (the same text inlined fails), and then finds **no DC solution at any nonzero input voltage**; it converges only at 0 V. It is also blind to temperature (output offset 2.4400 mV at −40, 25 and 125 °C), and a PSpice-only "noiseless resistor" parameter is dropped **silently**, so its noise comes out **26× too high** (408 vs 15.8 nV/√Hz). **(2) Stability specs give a false PASS (all corners).** Phase margin against a load capacitance has its worst value deep inside the range: 47.6° at 944 pF, against 81.9° and 83.9° at the ends of 0…100 µF. Because the knob map is linear, the tangent nudge lands at 500 nF and the 8 audit points miss the dip. **(3) Precision front-ends blow the 12-knob budget at once.** A bridge with a 3-op-amp in-amp has 33 knobs in the cone of its main spec, so the MVP says UNDECIDED (budget) on everything. **(4) Several things the plan filed under "later" turn out to be structural.** Calibration doesn't need affine forms; it needs a measure that reads **several runs that share one board**. CMRR, PSRR, loop gain and noise need **several excitations per run**. And the default bench silently measures the common-mode gain of any block with two analog inputs. Most of the fixes are cheap if made now: nine seams in the M3a types (§4) cost a field or an enum each. The circuit to build right after the MVP is a **Sallen-Key filter**, which needs an op-amp part kind and a subcircuit export but still enumerates (§5).

---

## 1. The circuits, and why each one

| # | Circuit | What it tests that the CE amp doesn't |
|---|---|---|
| **A** | Non-inverting G = 10 stage, a vendor op-amp (OPA171), driving a cable through R_iso | Vendor subcircuits in the export and the knob model; model parameters as knobs; an error shell; temperature coverage; a loop-gain probe; **phase margin against a range knob with an interior worst case** |
| **B** | Unity-gain Sallen-Key low-pass section, Q = 1.307, f0 = 1 kHz (the second section of a 4th-order Butterworth) | New AC measures (f0, peaking, f_c); what "−3 dB" means in a filter; the smallest op-amp circuit that still enumerates. **The ladder's next step** (§5) |
| **C** | 350 Ω load-cell bridge + 3-op-amp in-amp (G = 100) + ratiometric ADC, 2-point calibration | ppm bands; **33 knobs in one cone**; calibration; tempco links; a dual op-amp's correlated halves; CMRR and noise as measures; the default bench with two inputs |

A precision reference or ADC driver (settling, charge kickback) is left out on purpose: it is a transient-tier circuit (plan §9.3), and round 1 §3.5–3.6 already covers it.

---

## 2. Each circuit through the stack

### 2.A The vendor op-amp stage

```
              +15 V
                │
     in ──── +  │ OPA171 ──●── R_iso 50 Ω ──●── out (to a cable)
            ┌── −          │                │
            │              │               CL  0 … 100 µF   (range: the cable)
            ├── Rf 9k ─────┘                │
           Rg 1k                           RL 10k
            │                               │
           GND                             GND
```

**Specs and knobs.**

| Spec | Measure | Bound |
|---|---|---|
| offset | `dc(output.v)` with the input at 0 V | in ±20 mV (RTO) |
| gain | `h.at(1kHz).mag()` | 10 ± 0.2% |
| bandwidth | −3 dB frequency above the passband | ≥ 200 kHz |
| stable | phase margin of the loop | ≥ 45° **for every CL in 0…100 µF** |

| Knob | Kind | Range | Source |
|---|---|---|---|
| `temp` | range | −40 … 125 °C | product |
| `vcc.v` | range | ±15 V ± 5% | product |
| `cl` (the cable) | range | 0 … 100 µF | assumption on the output port |
| `rf`, `rg`, `r_iso` | statistical | 0.1% | resistor |
| `u1.vos` | statistical | ±1.8 mV at 25 °C | SBOS516H §5.7 (max) |
| `u1.tcvos` | statistical | ±2 µV/°C | SBOS516H §5.7 (max, not 100% tested) |
| `u1.ib` | statistical, envelope | ±15 pA at 25 °C, **±3.5 nA** over −40 … 125 °C | SBOS516H §5.7 |
| `u1.gbw`, `u1.aol` | statistical? | 3 MHz **typical only**; Aol 110 dB min | SBOS516H §5.7 |

That's 11–12 knobs. The offset side's cone (no capacitors) has 9, which enumerates.

**The language.** Here is what the contract would look like. `▲` marks where v0.1 (roadmap §4.1) runs out.

```rust
block PrecisionStage {
    port vcc: Power<In>;  port vee: Power<In>;  port gnd: Ground;
    port input: Analog<In>;  port output: Analog<Out>;
    net fb; net pin;
    let u1 = OpAmp { inp: input, inn: fb, vp: vcc, vn: vee, out: pin,        // ▲ no op-amp part kind (§6.1 lists none)
                     part: ti::OPA171 };                                      // ▲ `part:` pinning is M5
    let rf = Resistor { a: pin, b: fb, value: 9k ± 0.1% };
    let rg = Resistor { a: fb, b: gnd, value: 1k ± 0.1% };
    let r_iso = Resistor { a: pin, b: output, value: 50 ± 0.1% };
}
contract PrecisionStage {
    assume temp in -40°C..=125°C;
    assume vcc.v in 15V ± 5%;
    assume vee.v in -15V ± 5%;
    assume output.c_load in 0F..=100uF;                                        // ▲ loads are post-MVP (§4.1)
    let h = ac(output.v / input.v);
    spec offset: dc(output.v) in ±20mV;                                        // ▲ `in ±x` with no nominal isn't in the grammar
    spec gain: h.at(1kHz).mag() in 10 ± 0.2%;
    spec bandwidth: h.f_high(-3dB) >= 200kHz;                                  // ▲ f_high isn't in the MVP measure set
    spec stable: ac(u1.loop_gain()).phase_margin() >= 45°;                     // ▲ loop_gain probe, phase_margin: language §11.1 only
}
```

**Elaboration (`model.md`).**
- The op-amp's datasheet spreads (Vos, Ib) aren't fields of any MVP part kind, so there are no knobs for them. E8's part schema needs an `OpAmp` kind whose fields are `vos`, `tcvos`, `ib`, `gbw` and so on.
- `u1.vos` and `u1.tcvos` drive **one** simulated quantity, Vos(T) = vos + tcvos·(T − 25). E17 allows a device field to be `Exact` or `Knob(KnobId)`, one knob per field. A link needs a field that depends on two knobs (issue I-12).
- `output.c_load` is an assumption on a port quantity that the default bench must turn into a capacitor. E16 already turns port assumptions into knobs; M1e's bench has to place the part.

**The export (plan §5).** The engine deck needs four things it doesn't have today [P, `e1.py`]:

1. **The vendor file, by `.include`.** The same model text inlined into the deck fails to parse (`no such function 'if'`, `Unknown model type vswitch`). Through `.include` it loads. `EngineDeck { text: String }` has nowhere to put the file, and `rev` (engine_types §9.3) hashes only the deck text, so an edited model file would leave a stale verdict looking current.
2. **PSpice mode**, `set ngbehavior=ps`, read from `.spiceinit` **before** libngspice initializes (a `set` sent afterwards had no effect). That is a new backend rule for plan §4.3.
3. **An error shell** for the knobs the model can't take. The exporter adds `Vvos in inp_i DC {k_vos + k_tcvos*(k_temp-25)}` and `Iib inp_i 0 DC {k_ib}` outside the subcircuit. Read-back works through the shell (`@vvos[dc]`), and even **inside** the subcircuit: `@v.xu1.v_os[dc]` = 214.023 µV and `@i.xu1.i_b[dc]` = 8 pA, the model's built-in values. That answers plan §12 q. 6: yes, flattened instance paths are enough.
4. **An offset the model already has.** The model carries a typical Vos of 214 µV. A symmetric ±1.8 mV shell on top of it spans −1.586 … +2.014 mV, which is 0.214 mV past the datasheet on one side. So the shell's range must be asymmetric (−2.014 … +1.586 mV), and `KnobSpec`'s midpoint rule (engine_types §2.1) doesn't hold.

**Simulating it.** On the nominal deck, with the input at 0 V [P, `e1.py`]:

| Quantity | Value |
|---|---|
| offset `dc(output.v)` | 2.4400 mV (10 × 214 µV, plus bias and PSRR terms) |
| the same at −40 / 25 / 125 °C | **2.4400 / 2.4400 / 2.4400 mV**. The model ignores temperature. The datasheet allows ±2 µV/°C of drift (±3.3 mV RTO over the range) and Ib up to 3.5 nA |
| \|H(1 kHz)\| | 9.99998 |
| f_high, −3 dB below DC | 320 kHz (≈ GBW / 10) |
| one run: `alterparam` + `reset` + `op` | 3.9 ms (1.3 ms `reset`, 2.4 ms `op`) |
| one run: + `ac lin 1 1k 1k` + a 351-point sweep | **14.3 ms**, 20× the CE amp's 0.71 ms |

At 14.3 ms per run, the plan's 3 s clause (plan §2.3) stops enumeration at about 210 runs, i.e. 2^7 corners on one worker, not 2^12.

**The trap: no DC solution away from 0 V.** With the input at 0.1, 0.25, 1 or 2.5 V, the OPA171 model finds **no operating point** on ngspice-42 ("Transient op failed, timestep too small"). These didn't help: `rshunt=1e12`, `gminsteps/srcsteps=100`, `itl1/itl6=1000`, `method=gear`, `noopiter`, a `.nodeset` at the right voltages, `optran 0 0 0 100n 10u 0`, and ngbehavior `ps`, `psa`, `lt`. TI's OPA1611 model (same architecture, also in ngspice's examples) fails the same way. The 1989 TL072 macromodel and Microchip's MCP6041 converge at every input tried [P, `t*.cir`, `tm.cir`]. ngspice's own `examples/optran/contents.txt` says of two other TI models that they "currently allow op calculation only with optran". So the engine can't assume that a vendor model which loads also runs. A bridge front-end on this op-amp would be UNDECIDED (simulator) on every run.

**Stability: the interior worst case** [P, `e2.py`, `e2b.py`]. The loop gain is taken by injecting a voltage at the op-amp's high-impedance − input (Middlebrook's method is accurate there). PM against CL, with R_iso = 52.5 Ω and −40 °C:

| CL | 0 | 100 p | 470 p | **944 p** | 2.2 n | 10 n | 100 n | 1 µ | 100 µ |
|---|---|---|---|---|---|---|---|---|---|
| PM | 81.9° | 66.7° | 49.4° | **47.6°** | 49.9° | 67.8° | 81.9° | 83.6° | 83.9° |

What the plan's guards do (plan §2.5), with CL a range knob and the map x = mid + half·ε (engine_types §2.1):

| CL range | Worst corner | Tangent nudge (h = 0.01) | 8 audit points | Verdict for PM ≥ 50° | Truth |
|---|---|---|---|---|---|
| 0 … 1 µF | 81.93° at CL = 0 | CL = 5 nF → 60.3°, "improves", starts the ascent; the ascent (9-point line search + 6 zooms, 63 runs) finds 47.614° | none below 10 nF; worst 79.8° | FAIL (ascent). Correct, **by luck**: the nudge happened to land in the dip | 47.614° FAIL |
| 0 … 100 µF | 81.93° at CL = 0 | CL = 500 nF → 83.57°, no improvement | chance that a point lands below 5 nF ≈ 5·10⁻⁵ each | **PASS (all corners): false** | 47.61° FAIL |

The same line search on a **log** axis (10 pF … 1 µF) finds 47.612° in 63 runs, whatever the range. Plan §12 q. 2 asked whether "phase margin vs C_load" should declare "interior possible". This says yes, and that range knobs spanning decades need a log map.

**M3a types.** `Needs`/`AnalysisSet` (T4) have one implicit excitation, the default bench's AC 1 sources. The loop-gain run needs the input source at AC 0 and the probe source at AC 1. Each extra excitation is one more `ac` command, with its own operating point, on ngspice. A loop-gain run costs 8.6 ms here.

**What the engineer or agent would ask.** "Is it stable with the 2 m cable?" (PM against CL, the table above). "Why doesn't temperature matter?" The honest answer is that the model doesn't model it; `not_modeled` must say so, which needs a coverage probe (issue I-4). "What sets the offset?" Vos, from the shell. "Does a faster op-amp help?" GBW is typical-only and can't be spread on a black box.

### 2.B The Sallen-Key section

```
   in ── R1 10k ──●── R2 10k ──●──── + ┐
                  │            │       OPA171 ──●── out
                  C1 41.6 nF   C2 6.09 nF  ┌ − ─┘ (follower)
                  │            │           │
                 out          GND         out
```

**Specs and knobs.** f0 = 1 kHz ± 5%; peaking 3.0 dB ± 0.5 dB; stopband ≥ 38 dB at 10 kHz. The knobs are R1, R2 (±1%), C1, C2 (C0G ±5%), op-amp GBW (typical only), and temperature, whose effect on C0G is negligible. That's 4–6 knobs, so it enumerates.

**Numbers** [P, `e3.py`]: 7.9 ms per run (a 201-point sweep).

| Measure | Nominal | 16 corners | 200 interior points |
|---|---|---|---|
| peaking (max\|H\| / \|H(DC)\|) | 3.012 dB | 2.658 … 3.378 dB | 2.684 … 3.329 (inside the corners) |
| −3 dB below **max\|H\|** (D5, plan §4.6) | **1188 Hz** | 1120 … 1264 | inside |
| −3 dB below **\|H(DC)\|** | **1389 Hz** | 1310 … 1477 | inside |
| f0, the design parameter | 1000 Hz | — | — |

- **Enumeration is fine here.** Every extreme sat at a corner (with Q = 1.3 and ±5% parts; round 1 §4.6 shows Q ≥ 5 and fixed-frequency gains breaking this).
- **The measure is the problem.** D5 defines the cutoff as "−3 dB below max|H|". That suits the CE amp's band-pass, but for this section it is 1188 Hz, while a filter engineer means either the −3 dB point against the passband (1389 Hz) or f0 (1000 Hz). The three differ by 14% and 39%. A spec `f_high(-3dB) in 1kHz ± 5%` would get a confident verdict on a quantity nobody meant.

**Language.** `OpAmp` again (▲), plus `h.f_high(…)`, `.peak()` and a reference argument (▲). f0 and Q have no measure at all (▲; round 1 §6.2 item 6 proposed pole-based ones).
**Export.** A subcircuit plus `.include`, with the input at 0 V DC, so the OPA171 converges. Any follower with a DC input level would hit the §2.A trap; a TL072-class model or our own macromodel avoids it.
**The agent's questions.** "Which part moves the cutoff most?" That's the flip table, as for the CE amp. "Can I use ±10% caps?" A re-check.

### 2.C Bridge + in-amp + calibration

```
 Vexc 5 V ±5% ──┬─────────────┐
               Ra(1−x)       Rc(1+x)          A1 (TL072) ─ RF1 10k ─┐
                ├── inp ──►  +A1                          RG 202 Ω  ├─ diff stage A3 (4 × 10k) ── out ──► ADC
                ├── inn ──►  +A2                          RF2 10k ──┘       ratiometric: reading = v(out)/Vexc
               Rb(1+x)       Rd(1−x)
 GND ───────────┴─────────────┘          x = 0 … 0.004 (0 … full scale; FS out ≈ 2 V)
```

**Knobs: 33, all in the cone of the calibrated error** [P, `e4lib.py`]:

| Group | Knobs | Range | Source |
|---|---|---|---|
| range | `temp`, `vexc`, `x` (the load: the measurand) | −40…85 °C; 5 V ± 5%; 0…FS | product |
| bridge arms | 4 | ±0.1% | sensor |
| gain and diff resistors | 7 values + 7 TCRs | ±0.1%; ±25 ppm/K, random sign | thin film |
| A1, A2, A3 (TL072, TL07xBC grade) | 3 × (Vos, TCVos, Ib+, Ib−) = 12 | Vos 3 mV max; **TCVos 18 µV/K typical only**; Ib 200 pA max | SLOS080W |

**Specs.** Total error after a 2-point calibration at 25 °C ≤ ±200 ppm FS for every load, temperature and excitation; CMRR ≥ 100 dB at DC and 60 Hz; 0.1–10 Hz noise ≤ 0.2 ppm FS rms.

**Language.** Most of it runs out (▲):

```rust
contract LoadCell {
    assume temp in -40°C..=85°C;
    assume vexc.v in 5V ± 5%;
    assume bridge.x in 0..=0.004;                         // ▲ E16: assumptions only on port quantities; `x` is the sensor's
    let reading = dc(out.v) / vexc.v;                     //   fine: a per-run derived measure (D-F)
    calibrate zero at { temp: 25°C, bridge.x: 0 };        // ▲ no calibration syntax anywhere (engine.md §4.5)
    calibrate span at { temp: 25°C, bridge.x: 0.004 };
    spec total: (reading.calibrated() - bridge.x) / 0.004 in ±200ppm;   // ▲ `in ±x`; calibrated readings
    spec cmrr: ac(out.v).on(cm).at(60Hz).mag() <= -100dB; // ▲ benches (§8.5) to set the excitation
    spec noise: noise(out.v).band(0.1Hz..=10Hz).rms() <= 0.4uV;   // ▲ noise(), V/√Hz units (E11)
}
// ▲ TCRs: `tc: ±25ppm/K` links (M6); `lot(...)` for the diff network (M6); OpAmp and part pinning (M5)
```

**Elaboration.** `tc` links need a field that depends on two knobs (E17, issue I-12). A dual op-amp (A1, A2 in one TL072) has two halves whose drifts are probably correlated. Nothing in `KnobTable` can say so (issue I-9).

**Export.** ngspice's resistor `tc1={k_trg}` carries each TCR as its own knob, so here one knob still drives one field. TCVos goes into the shell's expression, `{k_vos + k_tcvos*(k_temp-25)}` (§2.A). The deck with three TL072s loads and runs: **0.85 ms per `op` run** [P]. The same deck with the OPA171 finds no DC solution: the bridge puts both inputs at 2.5 V.

**Calibration without affine forms** [P, `e4.py`]. The calibrated error of one board is computed from **three runs that share its statistical knobs**: zero at (25 °C, x = 0), span at (25 °C, FS), and the reading at the range point. That's what the firmware does, and it is exact for a nonlinear circuit too. Affine subtraction would be exact only for a linear one.

| At 85 °C, full scale, per knob at its +edge | Uncalibrated, at 25 °C | Calibrated |
|---|---|---|
| A1 / A2 Vos (3 mV) | ∓150,000 ppm | **∓0.008 ppm** |
| bridge arms (0.1%) | ±62,468 ppm | ±0.18 ppm |
| RG value (0.1%) | −989 ppm | −0.000 ppm |
| **A1 / A2 TCVos (18 µV/K, typical only)** | 0 | **∓54,000 ppm** |
| RG TCR (25 ppm/K) | 0 | −1482 ppm |
| diff-network TCRs (R6, R7) | 0 | ∓1312 ppm |
| RF1, RF2 TCR | 0 | +742 ppm each |

- Calibration removes the 25 °C terms, as round 1 predicted (its 1483 / 742 ppm for RG / RF TCR reappear here as 1482 / 742 ppm), with no affine form involved.
- The two largest terms come from a **typical-only** number, and they cancel exactly if the two halves of the dual drift together. The verdict hangs on a correlation the datasheet doesn't give.
- **Cost:** 2 extra runs per distinct statistical point. They are shared by every range point of that board, so in a run table keyed per cone they deduplicate on their own.
- **Numerics aren't the problem here:** the calibrated error at nominal is 0.0564 ppm at reltol 1e-3, 1e-6 and 1e-9 alike, and every output agrees to 10 digits [P]. The band (plan §2.7) is far below any spec, even though the measure is a difference of three runs.
- **Ratiometric, with a residual:** with Vexc at +5%, the nominal board's error moves from 0.056 to −0.65 ppm, the excitation's common-mode path.

**The budget** [P, `e4.py`].
- **The MVP answer.** 33 knobs in the cone gives 2^33 corners, so every side is UNDECIDED (budget) (W1/S1, D-D). That's on the first real front-end, not a large board.
- **Enumerating only the 12 largest knobs** (the rest at nominal), at the corner 85 °C / FS, takes 4,096 boards × 3 runs = 12,288 runs and 12.4 s. The worst is 115,716.08 ppm.
- **The loop's first move** (jump to the vertex the nominal slopes point to) lands at 115,715.46 ppm, the wrong vertex. rf1's slope is +0.208 ppm at nominal but changes sign at the vertex, through its interaction with the drift terms. One flip reaches the key exactly, so a flip walk with guard C2 gets it (engine.md §5.8). The additive prediction (nominal + Σ|effect|) is 115,044.93 ppm, 671 ppm short: interactions matter at the ppm level.
- **So precision circuits need the loop first,** and the loop behaves well on them: near-affine, with one flip round.

**Default bench, CMRR, noise** [P, `e5.py`].
- **Default bench with two inputs.** A difference amplifier (R2 at 10.01k) has two `Analog<In>` ports. The default bench (plan §1.3) puts `AC 1` on **both**, so "the gain" `|v(out)/v(inp)|` is **0.000500**: the common-mode gain. Driven differentially (±½), it's 2.0015; with only `inp` driven, 1.0005. A max-side spec on that gain ("≤ 1.01") passes on a number that isn't the gain.
- **Noise.** One `noise v(out) vin dec 20 0.1 10k` on the OPA171 G = 100 stage takes 8.1 ms (101 points). The model's `R_NOISELESS` resistors use `T_ABS=-273.15`, a PSpice parameter that ngspice ignores. In PSpice mode it is dropped **without a warning** (without PSpice mode the warning prints). So the 1 GΩ internal resistors are noisy: **408 nV/√Hz RTI** at 1 kHz against 14 nV/√Hz typical (SBOS516H). With the model line replaced by `R(noisy=0)` it's **15.8 nV/√Hz**. That's a 26× error in the dangerous direction for a "noise ≥ …" side, and a false FAIL for "≤". Noise itself fits the per-run design (D-F): σ_noise is a deterministic function of the knobs. What's missing is an analysis kind, units (V/√Hz, E11's rational exponents) and a model check.

---

## 3. Issues, ranked

| # | Issue | Severity | Hits | Suggested fix |
|---|---|---|---|---|
| I-1 | **PM against CL: false PASS (all corners)** on a wide range knob. The worst is interior (47.6° vs 81.9°), the linear map puts the nudge past the dip, and 8 uniform audit points miss it | risks a wrong verdict | plan §2.5, §12 q. 2; engine_types §2.1 (`x = mid + half·ε`) | Log-scaled knobs; measures declared "interior possible" (PM, peaking, anything at a tracked frequency) get a 1-D log line search per range knob (63 runs here) before any PASS |
| I-2 | **Vendor models: no DC solution off 0 V** (OPA171, OPA1611 on ngspice-42), after every standard aid | blocks the circuit | plan §4.3 (no rule), §3.1 W4; engine.md §6.3 | A **model qualification** step when a part is added: op at 3 input levels × 3 temperatures, AC, noise sanity. A model that fails is refused with a message, not discovered as UNDECIDED (simulator) on corner 1 |
| I-3 | **PSpice mode + `.include` are required**; inlined text fails; the mode must be set before init | blocks the circuit | plan §4.3, §5.1 (`EngineDeck.text` only) | `EngineDeck { includes, compat }`; the worker writes `.spiceinit` or sets the mode before `ngSpice_Init`; a backend-rule test |
| I-4 | **Temperature-blind models**: offset identical at −40/25/125 °C (TL072: 80 nV over 125 K vs 18 µV/K typical) | risks a wrong verdict | engine.md §6.6 (coverage map, not in M3); engine_types T9 `not_modeled` | A coverage probe at the nominal run: move `temp` to both edges, and a measure that doesn't move gets `model-conditional` and a demand for a shell tempco knob. 2 runs |
| I-5 | **Silent PSpice-only parameters** (`T_ABS`): noise 26× off, no warning in PSpice mode | risks a wrong verdict | plan §4.3; engine_types §4.1 (warnings aren't captured) | Capture every loader message into the qualification record; a known-parameter list per ngspice version; our own rewrite of `T_ABS=-273.15` to `noisy=0` |
| I-6 | **Default bench with two `Analog<In>` ports measures the common-mode gain** (0.0005 vs 2.0015) | risks a wrong verdict | plan §1.3; language §8.5; engine_types `Needs` | Refuse a default AC bench with more than one analog input, or give `Diff` ports dm/cm quantities. Excitation becomes explicit (I-7) |
| I-7 | **One excitation per run.** CMRR, PSRR, loop gain (1–2 injections), noise and differential gain each need their own AC source settings | blocks the circuit | engine_types T3/T4 (`AnalysisSet { op, ac_points, sweep }`), §7 `Program::AcAt { num, den: ProbeId }` | `AnalysisSet { op, ac: Vec<AcJob { excitation, points, sweep }>, noise: Option<NoiseJob> }`; `Run` indexed by job; complex sub-expressions in `Program` (a differential denominator) |
| I-8 | **More than 12 knobs at once**: the in-amp has 33 in one cone, so UNDECIDED (budget) everywhere. The sign vertex missed; one flip fixed it | blocks the circuit | D-D; plan §2.3, §9.3 | Make the loop the first step after M3 (for this domain it's the MVP's successor); the budget reason reports cone size and estimated runs |
| I-9 | **Correlated halves of a dual op-amp** and a matched network decide the verdict (±54,000 ppm, cancelling if they track) | risks a wrong verdict | engine.md §2.5; model.md E16 (one knob per field); no lot in M3 | Shared or lot knobs on multi-unit parts; with unknown correlation, report both (tracked and independent), like the distribution rule (S9) |
| I-10 | **Calibration waits for affine forms**, but it only needs a multi-run measure | blocks the circuit | engine.md §4.5, D-F; engine_types T7 (`Program::eval(&Run)`) | Measures over *companion points* (same statistical point, pinned range values); the run table deduplicates them. §4 S3 |
| I-11 | **The cutoff's definition**: −3 dB below max 1188 Hz, below DC 1389 Hz, f0 1000 Hz | risks a wrong verdict (a wrong question) | plan §4.6 D5; language §8.4 | A required reference argument on crossings (`ref: dc | max | at(f)`); pole-based `f0()`, `q()` |
| I-12 | **Links**: Vos(T) = vos + tcvos·(T−25) is one field driven by two knobs | blocks the circuit | model.md E17 (`Exact | Knob`); plan §5.1 `KnobBinding` (one knob → one target) | `Field::Expr` over knobs (a small affine expression); read-back compares the element with the expression's value |
| I-13 | **Asymmetric knob around a model's built-in value** (214 µV + shell) | risks a wrong verdict (range too wide on one side) | engine_types §2.1 (midpoint nominal) | Use `KnobSpec::nominal` now; the shell's range is datasheet minus built-in |
| I-14 | **Run cost**: 14.3 ms with the OPA171 (20× the CE amp), mostly the 351-point sweep, so the 3 s clause allows ≈ 2^7 corners | cost | plan §2.3, §1.3 (sweep for every run) | A sweep per measure, not per check; `f_high` refined by bisection; the time budget counts workers |
| I-15 | **`rev` ignores included model files** | risks a wrong verdict (stale shown as current) | engine_types §9.3 | Hash each include's content into `rev` |
| I-16 | **DC verdicts on an unstable circuit.** `.op` returns a DC solution even when the loop oscillates. The CE amp can't oscillate; op-amp circuits can | risks a wrong verdict (reasoned, not measured here) | plan §3.1 (no row) | An automatic stability check on any block with an op-amp (loop gain or `.pz`), whose FAIL tags every other verdict of the block "conditional on stability" |
| I-17 | **Missing measures and units**: f_high, peak, phase margin, noise, CMRR; V/√Hz | ergonomics | roadmap §4.1; model.md E11; engine_types §7 | Add to the measure table as each circuit arrives. Rational exponents with noise |

---

## 4. Seams to add now (in M3a, before code)

Each one is a type change in `engine_types.md` or a line in the plan. None changes the MVP's behavior on the CE amp.

| # | Seam | Where | Cost now | Avoids later |
|---|---|---|---|---|
| **S1** | `KnobSpec.scale: Linear | Log { floor }`, and `nominal` honoured (not always the midpoint) | engine_types §2.1 | One enum; `KnobSpace::value` switches on it (≈ 20 lines) | I-1's false PASS; I-13. Every ε consumer (nudges, σ map, LHS) goes through the one conversion already |
| **S2** | `AnalysisSet { op, ac: Vec<AcJob>, noise: Option<NoiseJob> }`, with `AcJob { excitation: ExcitationId, points, sweep }`; `Run.ac` indexed by job | engine_types §4, §2.3 `Needs` | A `Vec` instead of two bools; the MVP has one job | I-6, I-7: rewriting `Run`, the protocol frames and every measure later |
| **S3** | Measures over several points: `MeasureDef { setups: Vec<Pinned> }`, where `Pinned` fixes some range knobs (calibration: temp 25 °C, x = 0 / FS), and `Program::eval(&[&Run])` | engine_types §7; plan §6.3 | The MVP has one setup (the point itself) | I-10: calibration, and later multi-setup readouts (current reversal) without affine forms |
| **S4** | `EngineDeck { text, includes: Vec<Include { path, sha }>, compat: Option<Compat> }`; `rev` hashes include content | plan §5.1; engine_types §9.3, §10 | Two fields | I-3, I-15 |
| **S5** | `MeasureKind { Smooth, Kink, InteriorPossible }` on `MeasureDef` | engine_types §2.3 | One enum, set by the measure table | I-1 routing; the loop's allowance by kind (engine.md §5.8) already needs it |
| **S6** | `KnobBinding.target` as an expression over knobs (`Σ cᵢ·kᵢ` or a product for links) with a computed read-back | plan §5.1; model.md E17 | Today's case is one term | I-12. Read-back is the binding test; a retrofit would touch the deck format and the W0 rule |
| **S7** | A `qualify()` step on `Backend` (loader messages, op at a few input points, temperature coverage per measure), feeding tags | engine_types §4 (T3) | A trait method with a default that does nothing | I-2, I-4, I-5 |
| **S8** | `KnobSpec.provenance` (Tested / Characterized / TypicalOnly / Assumed) and `group: Option<GroupId>` (lot, dual op-amp) | engine_types §2.1–2.2 | Two fields, ignored by M3's search, printed in the record | I-9, and the relies-on-typical tag; M5/M6 fill them |
| **S9** | `Reason::Budget { cone: u32, est_runs: u64 }` | engine_types §8 | A payload | The in-amp's message says "33 knobs, 2^33 corners, needs the loop", not just "budget" |

S1, S2 and S3 are the ones to take now. They change the shape of `Run`, `Point` and `Program`, which M3b–M3e would otherwise bake into the worker protocol and every stage.

---

## 5. The ladder

| Step | Circuit | Proves | Depends on |
|---|---|---|---|
| **1 (right after the MVP)** | **B: the Sallen-Key section**, on a TL072-class vendor model or on our own macromodel, input at 0 V DC | An `OpAmp` part kind; subcircuit + `.include` in the engine deck (S4); PSpice mode; `f_high` with a reference, `peak`, `f0` (I-11); still ≤ 12 knobs per side, so enumeration stays exact (16 corners bound 200 interior points here) | Language: `OpAmp` kind in the prelude (E8), lowered to `X…` + shell; M1f includes; measure table |
| 2 | **A: the vendor stage with a cable load** | Loop-gain probe; excitations per job (S2); log knobs and "interior possible" (S1, S5); error shell + qualification (S7); temperature coverage tag | Load assumptions (`c_load`), part pinning (M5-lite), S1/S2/S5/S7 |
| 3 | **C: the bridge front-end** | The loop at 33 knobs; calibration as multi-run measures (S3); links (S6); lots and dual op-amps (S8); CMRR, noise | The loop (plan §9.3), M6 links and lots, benches, `noise()` |

B comes first because it adds the op-amp and the export path while keeping enumeration exact. So a mismatch points at the new pieces, not at the search.

---

## 6. Open questions

1. **Vendor models that don't converge**: refuse them at qualification, or run them only through a backend where they converge (a PSpice or LTspice backend, engine.md §6.3)? And should our own parametric op-amp (round 1 §4.1 b) be the default, with vendor models as a cross-check?
2. **Log knobs**: is CL's lower edge really 0 (a floor like 1 pF, or a separate "no load" mode), and which quantities are log by default (C_load, R_load, frequency, current)?
3. **Correlation with no data** (the dual op-amp's drift): report both extremes as two verdicts, or UNDECIDED (correlation) the way S9 treats distribution?
4. **Typical-only knobs** (TCVos, GBW, en) dominate here. What range should they get: a declared assumption, a policy multiple of typical, or nothing plus a tag?
5. **Should a stability FAIL gate other verdicts** (I-16), and on which evidence: PM from a loop-gain probe, or right-half-plane poles from `.pz`?
6. **Calibration's quantifiers** (∀ board ∃ trim ∀ conditions): with S3, is a trim with finite resolution just another statistical knob added after the calibration runs?
7. **Where the error shell lives**: generated by M1f from the part record (so the deck isn't the vendor's text), or written into the part record as a small block the language can read?
