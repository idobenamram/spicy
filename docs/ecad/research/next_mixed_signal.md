# Next Circuits: Digital Interfaces and Mixed-Signal Boards

> 2026-09-28 · Research note for the "what comes after the MVP" round. It pushes three board-level circuits through the **current** design: `engine.md` v4, `engine_plan.md` (cited "plan §n"), `engine_types.md` (M3a, cited "T1–T11" or "types §n"), `language.md` v0.1 with roadmap §4.1, `model.md` (E1–E26) and `grammar.md`.
> It builds on round 1's `research/engine_mixed_signal.md` (cited "round 1 §n"), which did the datasheet arithmetic for these circuits against engine v2. That arithmetic isn't repeated here. This note asks where the *current* stack breaks.
> **Where the numbers come from.** Every number marked **[X]** was computed on ngspice-42 (libngspice, via the round-2 ctypes driver) by the scripts in `/root/.claude/jobs/443154a8/tmp/engine3/mixed/` (called `$M`; Appendix A lists them). They're scratch and stay out of the repo. The MOSFET is an illustrative level-1 card, not a vendor model, and it's marked that way wherever it's used.

---

## 0. Summary

The MVP pipeline is shaped around one fact of the CE amp: **one bench, one operating point, and a handful of continuous knobs, all measured from `op` and `ac`.** Board-level digital circuits break each part of that. An I2C bus needs a transient (a rise time between 0.3·VDD and 0.7·VDD, where VDD is itself a knob), two stimuli (release and pull-low), a knob that layout owns (trace capacitance), and a variant knob (a plug-in module with its own pull-ups). A 3.3 V ↔ 5 V shifter needs a MOSFET subcircuit, two directions, and a verdict for "the driver is outside the datasheet row the model came from". A power-on reset needs a supply ramp, an event-driven delay (XSPICE), and a delay measured *between two nets*.

ngspice handles all three, and cheaply: 0.16 ms per DC run, 1.4–8.5 ms per transient run **[X]**. So the MVP's enumeration strategy still fits. **What doesn't fit are the M3a types.** `AnalysisSet` is three booleans with no transient. `Run` has no waveform. `Request` can't name a bench. `KnobKind` has no discrete knobs. `Program` can't read a knob. `DeviceOp` is BJT-only. `Verdict` dropped the plan's `Unspecified`. And the band stage tightens `reltol`, while transient error comes from the time step. Each of these is a few lines to change in M3a today, and a retrofit through every stage later. Section 7 lists ten seams, with their cost now.

Four findings would give **silent wrong answers** unless designed for:
1. A failed ngspice `let` keeps the vector's previous value, so a read-back of a bad path returns a stale number.
2. A variant knob's dead branch reads back 1e15 Ω, which would stop every check as UNDECIDED (binding).
3. Transient values move with `tmax` far more than with `reltol`. POR release times move 60 µs, and not monotonically.
4. A linear driver model extrapolated past its datasheet test current gives a confident VOL that the datasheet doesn't cover.

---

## 1. The circuits, and why each one

| # | Circuit | What it tests that the CE amp doesn't |
|---|---|---|
| **C1** | **I2C bus on an MCU board**: 3.3 V, Fast mode, MCU + 2 sensors, 2.2 kΩ pull-ups, optional plug-in module with 4.7 kΩ pull-ups | A transient timing measure with **knob-dependent thresholds**. A **layout-owned** knob, and the question "how much can layout have?". A **variant** (discrete) knob that adds parts. Two benches. Arithmetic specs next to simulated ones. Interface types (`I2c`, `Logic<OpenDrain>`). Automatic checks |
| **C2** | **3.3 V MCU ↔ 5 V peripheral**, (a) wired directly, (b) through a BSS138-style bidirectional shifter | Datasheet **tables with gaps** by supply band. A **part choice** as a discrete knob (74HC / 74HCT / 74LVC). A **MOSFET inside a subcircuit**: read-back and device records. Two directions. Two rails. A verdict **outside the datasheet's region** |
| **C3** | **Power-on reset**: supervisor (VIT−, hysteresis, t_D 130–270 ms) + open-drain NRST with 47 kΩ / 10 nF, fed by a rail ramp | A **supply ramp** as a range knob. **Hysteresis**, where the state depends on history. An **event-driven XSPICE** delay. A **delay between events on two nets**. Time scales from µs to 300 ms. Firmware-owned assumptions (watchdog) |

**Not chosen, and why:**
- *ADC input settling* (source R, anti-alias RC, a sample-and-hold). It adds a periodic steady state and a ½-LSB accuracy requirement, which is the transient-band problem of C1 and C3 in a harder form. It's the rung after C3 in §8.
- *Crystal gm margin.* An AC measure at an impedance port with an interior hump (round 1 §3.8). It's closer to precision analog, and it stresses the inside-the-box guards the plan already tests with `pq` and `tuned`.

---

## 2. C1: the I2C bus

### 2.1 The circuit

```
            v3v3 (3.3 V ± 3%)
             │            │
          Rp 2.2k ± 1%  Rm 4.7k ± 5%   ← plug-in module: present or not (variant)
             │            │
 SDA ────────●────────────●──────────────────────────
             │     │      │      │        │
          C_trace C_mcu  C_t1   C_t2    C_mod        pins ≤ 10 pF each (UM10204 Ci);
          20..100  3..10  3..10  3..10   40..80 pF   C_trace owned by LAYOUT
          pF       pF                    (cable+pins)
             │
          S_drv  open-drain driver, Ron 40..133 Ω
             │   (133 Ω = VOL 0.4 V at 3 mA; the 40 Ω minimum is ASSUMED: no datasheet gives it)
            GND
```

The simulated specs:
- `rise`: 0.3·VDD → 0.7·VDD, at most 300 ns (Fast mode; UM10204 figures as in round 1 §3.3);
- `fall_min`: 0.7 → 0.3·VDD, at least 20·(VDD/5.5 V) ns, which is 12 ns at 3.3 V;
- `vol`: the voltage with the driver on.

The arithmetic specs:
- the sink current stays within the driver's tested row (≤ 3 mA);
- the receivers' logic levels (VOL ≤ 0.3·VDD − 0.1·VDD margin).

### 2.2 The contract, as far as v0.1 goes

```rust
block I2cBus {
    port v3v3: Power<In>;
    port gnd: Ground;
    port sda: Pin;                 // ✗ wanted `Logic<OpenDrain>` inside `i2c: I2c<Controller>` (resolve: unknown signal type)
    let rp = Resistor { a: v3v3, b: sda, value: 2.2k ± 1% };
    let c_mcu = Capacitor { a: sda, b: gnd, value: 3pF..=10pF };   // stand-in for the MCU pin: no Logic part kinds
    let c_t1  = Capacitor { a: sda, b: gnd, value: 3pF..=10pF };
    let c_t2  = Capacitor { a: sda, b: gnd, value: 3pF..=10pF };
    let c_trace = Capacitor { a: sda, b: gnd, value: 20pF..=100pF };   // ✗ a layout assumption faked as a part
    // ✗ the driver: no switch / Nmos / open-drain part kind (roadmap §4.1: MOSFETs, IC parts are "not yet")
    // ✗ the module: a variant that adds Rm and C_mod. `if`/variants are not in v0.1 (language §12 q. 5)
}

contract I2cBus {
    assume temp in -40°C..=85°C;
    assume v3v3.v in 3.3V ± 3%;
    // ✗ #[owner(layout)] assume sda.c_bus in 20pF..=100pF;   attributes parse (grammar §3), but `c_bus` is no
    //   field of any signal type, and resolve has no owner attribute
    // ✗ assume mode in {Standard, Fast};                      a `{…}` set after `in` doesn't parse; enums are reserved
    // ✗ spec rise: tran(sda.v).rise_time(0.3 * v3v3.v, 0.7 * v3v3.v) <= 300ns on release;
    //   parses (calls, methods, `on` is contextual), but `tran`, `rise_time` and benches are not in the MVP subset
    // ✗ spec rise_by_mode: … <= mode.tr_max;                  a bound that depends on a knob (model.md E24: a constant bound)
    spec vol: dc(sda.v) <= 0.4V;                               // ✓ writable, but needs the driver on: a bench (✗)
}
```

**Where the syntax runs out:**
- *Written in v0.1 today:* the pull-up, the capacitors, the rail and temperature assumptions, and a DC spec.
- *Missing:*
  - a driver part;
  - the signal and interface types;
  - a variant;
  - a knob owned by someone other than the design;
  - a mode knob;
  - `tran` measures;
  - benches;
  - bounds that depend on knobs.

### 2.3 Elaboration (model.md)

- **Knobs (E16):** 8 statistical or range knobs come from the parts and assumptions. `c_trace` becomes a *statistical* knob, because it's a part-value spread, and that's wrong in two ways:
  - it's a condition that layout sets, so it's a range knob, owned by layout;
  - its verdict needs "PASS given C_trace ≤ …" (§2.6).

  The pin capacitances are datasheet maxima ("Ci ≤ 10 pF"). Their minimum of 3 pF is assumed, and `Knob` has no provenance to say so.
- **Variant:** E25 says structure depends only on `Exact` values, so a module that is sometimes fitted can't be a knob of one `FlatDesign`. It is either two elaborations checked together, or a knob that the exporter turns into a switched element (§2.4). model.md has no place for "a set of designs checked as one".
- **Tier-2 checks (E18):** "I2C lines need pull-ups" and "unique addresses" (language §3.4 `rules`) need `Logic<OpenDrain>` members and an interface net. The flat net is purely electrical, and C2's shifter will split one logical bus into two electrical segments (§3.3).

### 2.4 The engine deck (plan §5)

```
.param k_vdd=3.3 k_rp=2200 k_ctr=6e-11 k_cp1=6.5e-12 k_cp2=6.5e-12 k_cp3=6.5e-12 k_ron=86.5 k_mod=0 k_rpm=4700 k_cmod=6e-11
V_vdd vdd 0 DC {k_vdd}
R_rp vdd sda {k_rp}
C_ctr sda 0 {k_ctr}          (C_cp1..3 alike)
R_rpm vdd sda {k_mod > 0.5 ? k_rpm : 1e15}      ; variant as a ternary
C_cmod sda 0 {k_mod*k_cmod + 1e-18}
S_drv sda 0 ctl 0 SWOD
.model SWOD SW(RON={k_ron} ROFF=1e12 VT=0.5 VH=0)
V_ctl ctl 0 PULSE(1 0 100n 1n 1n 2u 10u)        ; BENCH: release at 100 ns, pull low at 2.1 us
```

What this shows about plan §5.1 (`EngineDeck`: "no analyses in the deck"):
- **The stimulus *is* in the deck.** `V_ctl` is the bench. VOL needs another one (the driver held on), which here is `alter v_ctl dc=1` + `op`. C2 needs two directions. So one check needs **several stimulus configurations**. Plan §5.1 has one deck per tolerance. T3's `prepare` takes one `Input`, and T3's `Request` has no bench.
- **The variant as a ternary works on ngspice.** But the read-back of `R_rpm` at `k_mod = 0` answers **1e15** where the knob asks for 4700 **[X]**. Plan §4.4's rule (a mismatch above 1e-9 → UNDECIDED (binding), check stops) would stop every check that has a variant.
- **Behavioral drivers are fine for ngspice.** `S` switch, PULSE, and ternaries all take `{param}`, so every knob reaches the simulator through `alterparam` (plan §4.2).

### 2.5 The engine, on ngspice

**One run.** A 4.5 µs transient takes 4.5 ms at `tmax` 2 ns, or 8.5 ms at `tmax` 1 ns (4,569 points) **[X]**. The nominal rise time is **148.1920 ns**, against the exact RC formula 0.8473·Rp·Cb = **148.1924 ns**. With every driver off the bus is a linear RC, so the formula is an answer key for free.

**Enumeration.** Rise's cone by the circuit is VDD, Rp, C_trace, 3 pin capacitances and Ron: 7 continuous knobs. With the module come the variant and Rm, C_mod: 10 knobs, 3 of them dead when the module is absent.

| Variant | Corners run | tr max | At | tr min | Fails (> 300 ns) |
|---|---|---|---|---|---|
| no module | 128 | **244.749 ns** | Rp +1%, C_trace 100 pF, pins 10 pF | 53.513 ns | 0 |
| module | 512 | **272.618 ns** | + Rm +5%, C_mod 80 pF | 85.583 ns | 0 |

640 runs, **2.88 s** at 4.5 ms per run on one worker **[X]**. A naive 2^10 would be 1,024 runs, because the variant's dead knobs would be enumerated while absent. The run table's exact key (T6) can't tell they're duplicates: their physical values differ, even though they don't reach the circuit.

**The layout question: how much C_trace can layout have?** This is the number the engineer and layout actually need. It's a bisection on one knob's edge, with the other knobs at the side's worst corner. That's exact here because tr is monotone in every knob.

| Variant | C_trace limit, simulated (13 runs) | Formula |
|---|---|---|
| no module | **129.33 pF** | 129.35 pF |
| module | **121.08 pF** | 121.09 pF |

So the verdict the engineer wants is: *"rise PASSES (all corners) given C_trace ≤ 100 pF; it still passes up to C_trace ≤ 121.1 pF (module fitted) → layout constraint: C_trace ≤ 121 pF on SDA."* Nothing in the plan computes the second half. It is a new kind of question: a **slack in knob space**, not a value.

**VOL** (DC, driver on, VDD max, Rp −1%, Rm −5%, Ron 133 Ω) **[X]**:

| | VOL | Sink current |
|---|---|---|
| no module | 0.1956 V | 1.471 mA |
| module | 0.2831 V | 2.128 mA |

Both stay inside the 3 mA row the 133 Ω comes from, so the linear model is valid. Three modules would push the sink current past the row, as in round 1 §3.3 (4.12 mA). Then the simulated VOL is an extrapolation (§3.4).

**`fall_min`.** The enumerated tf minimum is **1.005 ns** (Ron 40 Ω, pins 3 pF, C_trace 20 pF) **[X]**, far below 12 ns. It rests on two things nobody specified:
- the assumed 40 Ω;
- the ideal switch's instant turn-on (real Fast-mode drivers limit their slope).

The honest verdict is UNSPECIFIED (the minimum drive isn't in the datasheet), not FAIL. The M3a `Verdict` can't say it (§6, issue 5).

**The band.** At `tmax` 2 ns the nominal tr is 148.190783 ns with engine tolerances, and 148.192157 ns with `reltol` 1e-9. At `tmax` 0.1 ns both give 148.192394, and the formula gives 148.192396 **[X]**. So the error is the time step (1.6e-3 ns, 1.1e-5 relative), and plan §2.7's reltol-only re-run removes most of it here only by accident. For C3 it doesn't (§4.5).

**Verdict semantics.** Every CE-amp verdict is already conditional on `vcc.v ∈ 12 V ± 5%`. What makes C_trace different is **who discharges the assumption**:
- `vcc.v` is discharged by composition (an upstream guarantee, language §8.8);
- C_trace is discharged by the layout team, outside the tool.

So "PASS (conditional)" (engine.md §3.3, "later") needs no new verdict word. It is PASS (all corners) plus a list of **undischarged assumptions, each with its owner and slack**.

### 2.6 Through the M3a types

| Type | What C1 needs | Today |
|---|---|---|
| `Needs`, `AnalysisSet` (types §2.3, §4) | `tran 1n 4.5u` for rise/fall; `op` with another stimulus for VOL | `{ op, ac_points, sweep }` booleans. No transient |
| `Run` (T4) | the waveform `sda(t)` | no waveform field |
| `Request` (T3) | which bench (release / hold-low) | `{ point, analyses, tolerance }` |
| `KnobKind` (types §2.1) | discrete `k_mod ∈ {0, 1}`; C_trace owned by layout; pin C "max only" | `Range | Statistical(Dist)`; no owner, no provenance |
| `Point`, enumeration (T6) | mixed radix: 2 levels for continuous knobs, n for discrete; dead knobs skipped | ε ∈ [−1, 1], 2^\|cone\| |
| `Program` (T7) | `rise_time(level_lo = 0.3·k_vdd, level_hi = 0.7·k_vdd)`: a crossing whose level reads a knob | no `Knob` read, no transient programs |
| `Side` (types §2.3) | bound 300 ns in Fast, 1000 ns in Standard (depends on the mode knob) | `bound: f64` |
| `KnobBinding` read-back (plan §5.1) | the param, not the element, for a variant | element vector only |
| `Record` (T9) | slack "C_trace ≤ 121.1 pF", owner "layout" | no field |

### 2.7 What the engineer and the agent ask

- "What's the most bus capacitance I can have?" → slack on C_trace (§2.5).
- "Is 4.7k OK?" → a re-check with Rp = 4.7k. Round 1 §3.3 has it failing at 72 pF.
- "Does the module break it?" → per-variant records. Both variants must pass: this knob is ∀, not a choice.
- "Standard or Fast mode?" → per-mode verdicts. The mode is a *choice* (firmware sets it), so the report lists which modes pass, not "the worst mode".
- "Why UNSPECIFIED on fall time?" → "the MCU datasheet gives no minimum drive; declare one, or accept the risk". `next` is a data request, not a run.

---

## 3. C2: 3.3 V MCU ↔ 5 V peripheral

### 3.1 (a) Wired directly: arithmetic on tables

The worked arithmetic is round 1 §3.1:
- MCU → 74HC04 at 5 V fails by −0.92 V;
- MCU → 74HCT04 passes by +0.75 V;
- 5 V → MCU common pin violates abs-max unless the pin is 5 V-tolerant.

What's new here is how the current stack would carry it:
- **Part choice is a discrete knob of kind *choice*.** The engineer picks one of HC / HCT / LVC. The report should read "HCT and LVC pass; HC fails at VIH", not a worst over the three.
- **Thresholds are functions of a shared rail knob:** VIH = 0.7·VCC (HC), or a band table (74LVC1G04: 0.65·VCC at 1.65–1.95 V, 1.7 V at 2.3–2.7 V, 2.0 V at 2.7–3.6 V, *nothing* at 1.95–2.3 V; round 1 §3.1).
  - A rail range that crosses a band edge must be **split at the edge**. That's the same mechanism as a discrete knob's levels: enumerate [lo, edge, hi] instead of [lo, hi].
  - A range that touches the gap must give **UNSPECIFIED**, a value kind `Measured` doesn't have.
- **No simulation at all.** `Program` would need to read knobs (`Program::Knob`). Every run would still go to ngspice (T5's stages always emit `Request`s). And the datasheet knobs (VIH of a receiver) aren't in the deck, so read-back (W0) has nothing to read.

### 3.2 (b) The BSS138 shifter

```
      v3v3 (3.3 V ± 3%)                  v5 (5 V ± 5%)
        │     │ gate                       │
      Rpl 2.2k└──────┐                   Rph 2.2k
        │            │ G                   │
  sl ───●──────── S ─┴─ D ─────────────────●─── sh
        │         (body diode S→D)         │
      S_l  MCU open-drain              S_h  5 V device open-drain
      (dir A)                          (dir B)
```

The model is **illustrative**: level-1 NMOS, VTO = the knob `k_vth` 0.5–1.5 V, KP 0.30, plus a body diode, in a subcircuit `BSS138_I`. Round 1 §3.4 has the BSS138BK's datasheet values (Vth 0.48 / 1.1 / 1.6 V at 25 °C only; RDS(on) specified only at VGS ≥ 2.5 V).

Knobs: temp −40…85 °C, VL, VH, Rpl, Rph, Vth, Ron. Seven knobs, 2 directions × 128 corners = **256 DC runs in 0.041 s, 0.16 ms per run** **[X]**.

| Direction | Far-side VOL max | At | Sink current max |
|---|---|---|---|
| A: 3.3 V side pulls low | **0.4768 V** | Vth 1.5 V, 85 °C, VL 3.399 V | **3.829 mA** |
| B: 5 V side pulls low | **0.4744 V** | same | 3.830 mA |

- **The driver sinks both pull-ups** (3.83 mA > the 3 mA row), as round 1 found with 3.40 mA. The simulated 0.477 V comes from a 133 Ω linearization of "0.4 V at 3 mA". Above 3 mA the datasheet guarantees nothing.
- So the right record is **UNSPECIFIED (outside the datasheet row: 3.83 mA > 3 mA)**, not "FAIL 0.477 V > 0.4 V". It becomes a FAIL only against a stated model of the driver beyond its row, tagged model-conditional.
- **Vth barely matters at a 3.3 V gate:** far-side VOL moves 0.4621 → 0.4670 V over Vth 0.5 → 2.0 V **[X]**. It matters at a 1.8 V low side (round 1: FAIL at Vth 1.1 V cold). That's also where VGS falls below the 2.5 V the datasheet specifies: the device region, not the driver, leaves the guaranteed region.

### 3.3 Elaboration, deck and read-back

- **Parts:** `Nmos` is "not yet" (roadmap §4.1). A vendor model means `part:` pinning and `model: spice(…, subckt: …)` (language §6.8), both M5.
- **The bus through the shifter:** `sl` and `sh` are two flat nets. The I2C rules must hold on each segment (pull-ups, rise time) and across both (unique addresses). model.md's `FlatNet` has no notion of a logical bus made of segments joined through a part. Language §9's "nets joined through an inductor count as one rail" is the same idea for power.
- **Read-back inside a subcircuit** (plan §12 q. 6), tested **[X]**:

| Path | Answer |
|---|---|
| `@x_q1:nm[vto]` (the model inside the subckt instance) | **1.3**, the knob exactly |
| `@m.x_q1.m1[von]` (the instance) | **1.3025**, Vth at 22.5 °C: temperature-adjusted, so it never equals the knob to 1e-9 |
| `@m.x_q1.nm[vto]`, `@m.x_q1.m1[vto]` | error |

  The model path works, and the exporter must emit that spelling. The instance value would trip W0 on every run.
- **A silent stale read-back.** When `let zz_ = <bad path>` fails, ngspice keeps `zz_`'s previous value. The first probe script read "22.5" for the model's VTO: the temperature read just before it **[X]**. A backend that reads a bad path into the same variable gets a plausible wrong number with no error. The rule: `unlet` before every read, and treat any error line during reads as a missing vector (a fifth trap for plan §4.3).
- **Device records:** the MOSFET answers `vgs`, `id`, `gm`, `gds`, `von`. T4's `DeviceOp { ic, ib, vbe, vbc, gm, gmu }` and plan §4.6's region rule (gmu > 1e-4·gm) are BJT-only.

### 3.4 The general lesson: validity regions

C1's three-module VOL, C2's shifter VOL, and VGS < 2.5 V are all the same event. **The operating point leaves the region where a model's numbers were tested.** The engine can check this per run, as an extra measure: the margin to the region's edge (I_sink ≤ I_row; VGS ≥ the lowest RDS(on) row). If the margin is negative at a side's decisive point, the side's verdict is UNSPECIFIED (region), unless another tested row covers it. It's cheap: one more measure per region. But it needs `Verdict::Unspecified` and part records that carry each row's test conditions (M5).

---

## 4. C3: power-on reset

### 4.1 The circuit and specs

```
 V_r: PWL 0 → 3.3 V ± 1.5% in t_ramp (0.1 … 50 ms, range)
   │
   ├── supervisor: comparator VIT− 3.08 V ± 2%, hysteresis 0.9–1.5%   (S switch with VT/VH)
   │      └── delay t_D 130–270 ms on release                         (XSPICE d_buffer, fall_delay = {k_td})
   │              └── open-drain NRST ── 47k ± 5% to VDD ── 10 nF ± 10% to GND
   └── MCU: needs VDD ≥ 3.0 V (its flash) and its clock started before NRST releases
```

- **release_delay:** NRST crossing 0.7·VDD rising, minus VDD crossing 3.0 V rising ≥ 1 ms. That's a delay between events on two nets.
- **Window specs:** VIT−,min ≥ the loads' VDD_min; VIT+,max ≤ V_rail,min. These are arithmetic, as in round 1 §3.6 (the TLV809E 3.08 V has +44 mV at DC).
- **Watchdog:** the kick period stays inside tWDL,max … tWDU,min. That's arithmetic, and the kick period is a **firmware-owned** assumption (round 1 §3.7).

### 4.2 The contract in v0.1

```rust
contract Por {
    assume temp in -40°C..=85°C;
    assume vdd.v in 3.3V ± 1.5%;
    // ✗ assume vdd in Ramp { from: 0V, time: 0.1ms..=50ms };   signal families (language §8.2) are not in the MVP
    // ✗ spec release_delay: tran(nrst.v).crossing(0.7 * vdd.v, rising) - tran(vdd.v).crossing(3.0V, rising) >= 1ms;
    //   parses; needs tran, crossing with a direction, and a difference of two event times
    // ✗ #[owner(firmware)] assume kick_period in 26.7ms..=40.4ms;   not a port quantity; no owner
    // ✗ spec window: sup.vit_minus.min() >= 3.0V;                  part-record fields and .min() are M5
}
```

### 4.3 Modeling the digital part for ngspice

Three options, in the order tried:
1. **An `S` switch with hysteresis** as the comparator (`VT = centre`, `VH = half-width`). Every threshold is a `{param}`, so it's knob-able.
2. **XSPICE for the digital delay:** `adc_bridge` → `d_buffer(fall_delay = {k_td})` → `dac_bridge`. libngspice loads the code models from `spinit`. `alterparam` + `reset` reaches `fall_delay`: the enumerated delays span 130.5 … 274.1 ms against t_D 130–270 ms **[X]**. The read-back works by **model** name (`@dbuf[fall_delay]` = 0.2), not by instance name (`@a_inv[fall_delay]` → error, and then the stale-value trap).
3. **The model's own correctness is the modeler's problem.** The first version of this model **released reset early** (−0.57 ms). The comparator's output pulled up to VDD, which is ≈ 0 V at power-up, so "below threshold" read as logic 0. That's exactly the real "undefined below VPOR" region (round 1 §3.6), in a model. The engine can't catch a wrong model. A bench-level sanity spec can ("NRST low while VDD < 2.5 V"): cheap to write, and it caught this bug.

### 4.4 The engine on ngspice

| | Value **[X]** |
|---|---|
| Nominal | VDD reaches 3.0 V at 4.5455 ms; NRST releases at 205.4296 ms; delay **200.884 ms** |
| One run, `tran 10u 0.35 0 1m` | 537 points, **1.4–2.0 ms** |
| Corners (t_ramp, VIT−, hysteresis, t_D, VDD, R_pu, C_r: 7 knobs) | 128 runs, **0.18 s**; delay **130.486 … 274.146 ms** |

Cheap. The hard part is accuracy, not cost.

### 4.5 The band for event-driven transients

| `tmax` | Release time **[X]** | Points | ms per run |
|---|---|---|---|
| 1 ms | 205.42964 ms | 537 | 1.4 |
| 0.1 ms | 205.47416 ms | 3,653 | 8.0 |
| 10 µs | 205.41378 ms | 35,098 | 74 |

- **Not monotone, and 60 µs wide:** 3e-4 of the value. It comes from how the analog step lands on the digital event, and `reltol` doesn't touch it.
- Plan §2.7's band ("re-run at reltol 1e-9") would report a band near zero here, and so would miss the error. That's the soundness §6 trap again (a difference table said 2e-13 where the true error was 9.5e-5).
- **Fix:** the tight run also cuts `tmax` (and `trtol`). ε_num comes from at least two step sizes. For event-driven models, an event-aligned breakpoint (`.tran` with `tmax` below the smallest delay's resolution) or a measure interpolated from the event queue.

### 4.6 History and multiple states

A hysteretic comparator has two DC solutions inside its window. ngspice's cold `op` picks one (plan §12 q. 5). A DC measure on `good` inside the hysteresis window is meaningless. **The bench's ramp is the declared history** (engine.md §4.7). So a spec on a hysteretic block must be a transient on a bench with a ramp. The plan could mark DC measures whose cone contains a hysteretic element (an `S` switch with VH > 0, an XSPICE digital block) as UNDECIDED (multi-stable). A structural check like that is cheap for behavioral models, because the hysteresis is a model parameter.

---

## 5. Specs that need no simulation

About two-thirds of board-level digital specs are datasheet arithmetic (engine.md §4.1, M5). Here, C1's row and level checks, C2(a) entirely, and C3's windows and watchdog. The M3a pipeline assumes every measure comes from a run:

| Step | What an arithmetic spec needs | M3a today |
|---|---|---|
| Program | read knobs: `Program::Knob(KnobId)` | T7 says a derived measure "reads vcc.v from the same point's knob value", but the listed enum has no `Knob` variant |
| Knobs | datasheet parameters with no simulator target (VIH, tWDL, VIT−) | every knob has a deck binding and a read-back (plan §4.4) |
| Stages | evaluate without a backend call | every stage emits `Request`s to the backend |
| Budget | 20 knobs × 2 evaluations, µs each | 2^\|cone\| ≤ 4096 **and** ≤ 3 s. The count clause refuses arithmetic that costs nothing; a time-only budget (plan §12 q. 8) wouldn't |
| Verdict | PASS (guaranteed) from monotonicity or intervals | `PassKind` "Guaranteed later" (fine: all corners is honest meanwhile) |
| Values | UNSPECIFIED inside a table gap | no `Measured::Unspecified`; M3a dropped plan §6.2's `Verdict::Unspecified` |

**The cheap way in:** a knob-only measure is a `Program` whose `AnalysisSet` is empty. The driver evaluates it on a "run" that never reaches the backend. Knobs get a `target: Sim | None`, and read-back skips `None`. Enumeration of arithmetic sides is limited by time, not by count. Then M5's datasheet engine adds exactness (monotonicity proofs or intervals) on the same records, without a new pipeline.

---

## 6. Issues, ranked

| # | Issue | Severity | Hits | Suggested fix |
|---|---|---|---|---|
| 1 | **No transient in the engine types:** no `tran` in `AnalysisSet`/`Needs`, no waveform in `Run`, no transient `Program`s (crossing with direction, rise time, delay between two nets, value at a time) | blocks C1, C3 | types §2.3, §4 (T3, T4), §7 (T7); plan §6.3 promises "an AnalysisSet per request" but the struct can't hold one | `Needs::analyses: Vec<AnalysisSpec>` (`Op`, `AcPoint(f)`, `AcSweep`, `Tran { tstop, tmax, bench }`); `AnalysisSet` = a bitset over them; `Run::waves` |
| 2 | **One bench per check:** a stimulus lives in the deck; C1 needs release and hold-low, C2 two directions, C3 a ramp | blocks C1–C3 | plan §5.1 (`EngineDeck`, one per tolerance); T3 `prepare(Input)`; `Request` | `BenchId` in `Request` and in the run table's exact key; `Prepared` holds one loaded deck per (bench, tolerance); plan §4.3 rule 9 (one deck per worker) becomes one worker per deck or a reload |
| 3 | **Transient band measures the wrong error:** reltol re-runs miss time-step error (C3: 60 µs, non-monotone in `tmax`; C1: 1.6e-3 ns) | risks a wrong verdict | plan §2.7, §4.5; engine.md §5.6; `Tolerance { Engine, Tight }` | `Tolerance` → numerics settings (`reltol`, `tmax` scale, `trtol`); ε_num from two step sizes for any `tran` measure |
| 4 | **Read-back traps:** a failed `let` returns the previous value; a variant's dead branch reads 1e15; a MOSFET's instance Vth is temperature-adjusted; XSPICE parameters read only by model name | risks a wrong verdict (silent), or blocks (false binding) | plan §4.3–4.4, §5.1 (`KnobBinding`), T10 | Per knob a `ReadBack { Element(path), Model(path), Param, None }`; `unlet` before each read and any error line = missing vector; read the param (not the element) for ternary-selected elements; a new trap row in plan §4.3 |
| 5 | **No UNSPECIFIED:** table gaps, missing minima (driver Ron), operating outside a datasheet row (3.83 mA > 3 mA) | risks a wrong verdict (a FAIL/PASS from an extrapolated model) | types §7–8 (`Measured`, `Verdict`) dropped plan §6.2's `Unspecified`; engine.md §3.3 | Restore `Verdict::Unspecified(reason)`; add `Measured::Unspecified`; validity-region margins as measures (§3.4) |
| 6 | **Discrete knobs:** variant (module fitted), mode (Standard/Fast), part choice (HC/HCT/LVC), firmware register; ∀ vs choice; dead knobs per level | blocks C1, C2(a) | types §2.1 `KnobKind`, `Point` (linear ε), T6 keys; plan §2.3 (2^\|cone\|) | `KnobKind::Discrete { levels, quantifier: ForAll | Choice }`; enumeration over per-knob level lists (mixed radix); a level may mark knobs dead (skipped, keyed as `Any`); per-level records for `Choice` |
| 7 | **Arithmetic specs have no path:** no `Program::Knob`, knobs without a simulator target, every stage calls the backend, the count budget | blocks C2(a) and most automatic checks | T7; plan §2.3, §4.4; engine.md §4.1 | §5's cheap way in |
| 8 | **Distributions for datasheet IC limits:** t_D, VIT−, Vth, pin C are limits with no distribution; T2 keys the policy on the knob kind only, so `sigma(3)` would PASS on an invented normal | risks a wrong verdict | T2 (`Policy`); engine.md §2.2, §2.4; agent R9 | `KnobSpec::provenance` now (`Tested`, `ByDesign`, `TypicalOnly`, `Assumed`, `MaxOnly`); the policy keyed on (kind, provenance); datasheet limits default to worst-case or UNSPECIFIED at `sigma(3)`, as round 1 §4.5 argued |
| 9 | **Bounds and levels depend on knobs:** tr ≤ tr_max(mode); crossings at 0.3·VDD; VOL ≤ 0.3·VDD − margin | blocks C1's mode spec; ergonomics otherwise | types §2.3 `Side::bound: f64`; model.md E24 (constant bound) | normalize every side to a margin program ≥ 0 at plan time (`bound − measure`), keeping the written form for display; allow knob reads in levels |
| 10 | **PASS (conditional) and slack:** layout- and firmware-owned knobs; "C_trace ≤ 121.1 pF" | ergonomics (the output the engineer wants is missing) | engine.md §2.1, §3.3 ("later"); T9 `Record` | `KnobSpec::owner: Design | Environment | Layout | Firmware`; `Record::rests_on` (undischarged assumptions with owners); a `slack` query: bisection on one knob's edge at the side's worst corner (13 runs here), exact when monotone, else stated as searched |
| 11 | **Transient cost vs the 3 s clause:** 4.5–8.5 ms per C1 run; 10 knobs → 1,024 corners = 4.6–8.7 s on one worker | cost | plan §2.3 (time clause), §12 q. 8 | a time budget over the worker pool (P > 1 for transients, types §13 q. 1); dead-knob skipping (640 instead of 1,024) |
| 12 | **Device records are BJT-only** | blocks region tags for MOSFETs, diodes, behavioral blocks | T4 `DeviceOp`; plan §4.6 | `enum DeviceOp { Bjt{…}, Mos{ id, vgs, vds, von, gm, gds }, Diode{…}, Other(Box<[f64]>) }`; the region rule per kind |
| 13 | **Signal and interface types:** `Logic<OpenDrain>`, `I2c<Controller>`; `Role` lacks `OpenDrain`/`InOut`; open-drain VOL is a row (VOL at IOL), not a scalar field; a logical bus spans electrical segments | blocks the automatic checks | language §3.3–3.4; model.md E18–E19; `prelude.rs` `Role { In, Out }` | reserve `OpenDrain`/`InOut` roles now; in language §3.3 make signal fields rows with test conditions; a model.md note: interface nets vs electrical segments |
| 14 | **Automatic checks in records:** origin, grouping, waivers; they default to `worst_case` | ergonomics (hundreds of records on an MCU board) | T9 `Record.spec: String`; language §8.7 | `SpecOrigin { User(name), Auto { rule, subject: Path } }`; `Record::waived: Option<String>` |
| 15 | **History and multi-stability:** DC on a hysteretic block picks one state | risks a wrong verdict | plan §12 q. 5; engine.md §4.7 | UNDECIDED (multi-stable) for DC measures whose cone holds a hysteretic element (visible in behavioral model parameters); hysteretic specs on ramp benches |
| 16 | **Behavioral models can be wrong** (the first POR model released reset early) | risks a wrong verdict (model-conditional) | engine.md §6.6 | bench sanity specs shipped with each behavioral part; model-conditional tag on every verdict through a behavioral IC |

---

## 7. Seams to add now

Each is cheap in M3a because M3a is still a draft (types §0: "draft for review"). Each would be a retrofit through the stages, the run table and the report schema later. "Cost now" is an estimate for the types and the MVP code paths, which each seam leaves otherwise unchanged.

| # | Seam | Where | Cost now | Avoids later |
|---|---|---|---|---|
| **S1** | `Needs::analyses: Vec<AnalysisSpec>` with a `Tran` variant (unused in M3); `AnalysisSet` = a bitset over it; `Run::waves: Box<[Wave]>` (empty in M3) | types §2.3, §4.2 | ≈ 30 lines; the worker's run sequence already takes analyses per request | reshaping `Request`, `Run` and the protocol frames (T10) for the transient tier |
| **S2** | `Request::bench: BenchId` (always 0 in M3); `Prepared` = a map (bench, tolerance) → loaded deck; bench in the exact dedup key | types §4, §6; plan §5.1 | ≈ 20 lines; the MVP has one bench | a key change in the run table and the per-`rev` store |
| **S3** | `KnobSpec` gains `levels` (enumeration points: `[lo, hi]` by default; breakpoints; discrete values), `provenance`, `owner`, `target: Sim | None`; `KnobKind::Discrete { quantifier }` reserved | types §2.1; `KnobTable` in model.md | ≈ 40 lines; `Enumerate` becomes mixed-radix (a loop over level indexes instead of bits) | a rewrite of enumeration, `Point` and the keys when modes, variants, part choices and table bands arrive |
| **S4** | `Program::Knob(KnobId)`; crossing levels and bounds as sub-programs; every `Side` normalized to a margin ≥ 0 | T7, types §2.3 | ≈ 20 lines | a second measure path for arithmetic and knob-dependent bounds |
| **S5** | Restore `Verdict::Unspecified(reason)` (plan §6.2 had it) and add `Measured::Unspecified` | types §7–8 | 2 variants, 1 verdict row | a schema break in `spicy.check/0`'s successor |
| **S6** | `KnobBinding::readback: ReadBack { Element, Model, Param, None }`; plan §4.3 gains the stale-`let` rule | plan §5.1, T10 | ≈ 15 lines + one backend-rule test | false binding stops, or silent stale read-backs, on the first vendor or behavioral model |
| **S7** | `DeviceOp` as an enum per device kind; the region rule as a function of the kind | T4, plan §4.6 | ≈ 15 lines | reworking the protocol frames and region tags for MOSFETs |
| **S8** | `Record::origin: SpecOrigin`, `Record::rests_on: Vec<(KnobId, Owner)>`, `Record::waived` | T9 | 3 fields | a report-schema break when automatic checks and conditional PASSes arrive |
| **S9** | `Tolerance` → a numerics setting with a time-step scale; the band stage re-runs with both tightened | types §4, plan §2.7 | ≈ 10 lines; no change for `op`/`ac` | a wrong ε_num on the first transient spec |
| **S10** | A stage may produce rows with an empty `AnalysisSet`, answered by the driver without the backend | T5 driver | ≈ 10 lines | a separate pipeline for datasheet arithmetic (M5) |

**Not worth doing now:** the slack query itself (it's a `--deep`-style step on top of S3/S8), interface types in the language, part records, XSPICE-specific code. The deck already carries XSPICE; only S6's read-back kinds are needed.

---

## 8. The ladder

**Right after the MVP: C1, the I2C rise time.** Minimal version: pull-up, capacitors, an open-drain switch, one variant.

What it proves:
- **The transient tier works end to end:** a waveform, a crossing whose level is 0.3·VDD from the same run, a band that refines the time step.
- **With a free answer key:** the exact RC formula (148.1924 ns) and the C_trace limits (129.35 / 121.09 pF) check the engine to 1e-4 **[X]**.
- **Two benches** (release, hold-low) in one check.
- **A discrete ∀ knob** (the module) with dead knobs.
- **A layout-owned knob,** with the slack output "C_trace ≤ 121 pF".

What it depends on:
- S1–S4, S6, S8, S9;
- in the language: a `Switch` or open-drain stand-in part kind, `tran(…)` with `.rise_time`/`.crossing`, `on bench`, and an `owner` attribute on `assume`.

Everything is still writable with `Pin` ports and `Capacitor` stand-ins, so interface types can come later.

**Then:**

| Rung | Circuit | Proves | Depends on |
|---|---|---|---|
| 2 | C3 power-on reset | ramp benches; XSPICE through the deck; delay between two nets; event-timing band; a firmware-owned knob; history | S1, S2, S6 (model read-back), S9 |
| 3 | C2(a) direct logic levels | arithmetic sides with no runs; table bands as levels; UNSPECIFIED in a gap; a part choice as a `Choice` knob | S3, S4, S5, S10; part records with tables (M5) |
| 4 | C2(b) BSS138 shifter | a vendor subcircuit; model read-back inside `x…:model`; MOSFET device records; validity regions | S5, S6, S7; `Nmos` and `part:`/`model:` (M5) |
| 5 | ADC input settling | ½-LSB accuracy (1.2e-4 of full scale at 12 bits) for a transient; a periodic steady state after many samples; a firmware sample-time `Choice` | the above, plus the analysis ladder (engine.md §5.9) |

---

## 9. Open questions

1. **Variants:** two elaborations checked as one (model.md E25 stays strict), or a discrete knob the exporter lowers to a switched element (ternaries work on ngspice, but break element read-back)? This note leans towards elaborations per variant sharing one knob space. That keeps P6 intact.
2. **Slack:** only for owner = layout/firmware knobs, or for every range knob ("vcc.v may drop to 11.2 V")? And how to state it when the side isn't monotone in that knob?
3. **Datasheet limits at `sigma(3)`:** worst-case by default (round 1 §4.5), or UNSPECIFIED until a distribution is declared (agent R9)? The first is more useful, the second more honest.
4. **Driver models past their row:** is UNSPECIFIED (region) the verdict, or FAIL tagged model-conditional? It changes how often the agent must say "the datasheet doesn't say".
5. **Where interface rules run:** as tier-2 net checks (model.md E18: no numbers), as automatic engine sides (numbers), or split (pull-up present = tier 2; rise time = engine)?
6. **Event timing:** is `tmax` refinement enough for XSPICE delays, or should crossing times of digital events come from the event queue (exact) rather than the analog waveform?
7. **Worker count for transients:** at 4.5–8.5 ms per run, should P default to the core count whenever a check has a `tran` side (types §13 q. 1)?

---

## Appendix A. The scripts (`$M` = `/root/.claude/jobs/443154a8/tmp/engine3/mixed/`)

| Script | What it computes | Section |
|---|---|---|
| `ngx.py` | libngspice harness over round 2's `soundness/ng.py`: load once; per run `alterparam` · `reset` · analyses · reads · `destroy all`; cubic crossing | all |
| `i2c.py` | C1 deck; nominal tr vs formula; module read-back; time-step sweep | §2.4–2.5 |
| `i2c_enum.py` | C1 enumeration, both variants (640 runs) | §2.5 |
| `i2c_limit.py` | C_trace limit by bisection; VOL; band (reltol vs `tmax`) | §2.5 |
| `shifter.py`, `rb.py` | C2(b) deck, 256 corners, Vth sweep; subcircuit read-back paths, the stale `let` | §3.2–3.3 |
| `por.py` | C3 deck with an `S` switch and XSPICE `d_buffer`; 128 corners; `tmax` sensitivity; XSPICE read-back | §4.3–4.5 |
