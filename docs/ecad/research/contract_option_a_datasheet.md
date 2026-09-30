# Contract Option A: The Datasheet Is the Contract

> Design option · 2026-09-29 · One of three competing designs for the contract layer (specs, setups, conditions, loads and sources, faults, hierarchy). Philosophy A: **spec-centric, written as a datasheet**.
> Reads with: `research/contract_references.md` (§6 datasheets, §7 IBIS, §18 lessons), `research/contract_ee_practice.md` (§1 physics, §4 the reference board), `research/contract_hierarchy.md` (reuse, tables, joins), `research/contract_tool_gaps.md` (G1–G19, P1–P15), `language.md` v0.1 (§3, §5, §8), `engine_types.md` (M3a), `research/next_synthesis.md` (L1–L6, T1–T6, E1–E9).
> **Where the numbers come from.** **[S]** = `contract_ee_practice.md`, **[H]** = `contract_hierarchy.md`, **[DS]** = a datasheet or standard cited there. **[A]** = simulated for this report on ngspice-42 with the reference board's models (`contract_ee_practice.md` §4.3), by stdlib-Python scripts in `/root/.claude/jobs/443154a8/tmp/engine4/a/` (`fault.py`, `fault3.py`–`fault5.py`, `hold.py`, `rev.py`, `rev2.py`, `zout.py`, `ldoz.py`, `mid2.py`, `probe.py`). [A] numbers are at nominal parts unless a corner is named. Nothing in the repo was changed except this file.

---

## Summary

A datasheet is the most widely read spec format in electronics, and it already solves most of what we need. It has an **absolute maximum ratings** table (never exceed), a **recommended operating conditions** table (the range the part is designed for), and an **electrical characteristics** table. Each row of that table is one guaranteed number: a parameter, its **test conditions**, and **min / typ / max**. A header line says what holds "unless otherwise noted". Rows point at **test circuits** ("see Figure 30") when the setup is special. Option A makes that structure the language. A `contract` has sections named after the datasheet sections: `ratings`, `operating`, `conditions`, `specs`, `equivalent` (published port models) and `faults`, plus named `fixture` items (test circuits). **`operating` replaces `assume`**: it lists the ranges the block is designed for, and those ranges do two jobs. They become the range knobs every row is checked over, and they become the obligations a parent must meet wherever the block is placed. Inputs are described by their **source** (signal, impedance, disturbances). Outputs are described by their **load** (a range, a step, or another block). Disturbances are named **handles** (`vbus.ripple`, `vout.step`), and a row picks one by naming it in its measure (`ac(code / vbus.ripple)`, `tran(v).during(mcu.wake)`). Faults are declared events that use the same handles. Hierarchy works the way an engineer reads a part's datasheet: the parent checks that its real neighbours sit inside the child's operating conditions, in both directions (z_out against the load range, z_in against the source range), and then uses the child's rows without re-simulating them. When a check fails, the engine falls back to cached tables or to in-context simulation. The simple case stays simple: `ce_amp.spl` becomes two sections of the same length as today. The weak spots are real. Scalar operating ranges can be too pessimistic for frequency-dependent ports. Board-level rows with several stimuli strain the table form. Faults are the least datasheet-like part. And a datasheet habit of fixed test points can hide ranges unless the default stays "over the full operating range".

---

## 1. The philosophy, and the concepts it introduces

### 1.1 In one paragraph

**The contract is the block's datasheet, and the datasheet is checked.** Every guarantee is a **row**: a name, a measure, a limit (min / typ / max), and its test conditions. Rows are the centre. Everything else in the contract exists to give rows their meaning. The **operating** section says over which world the rows must hold. The **conditions** header says how the block is set up on the bench unless a row says otherwise. **Fixtures** are the named test circuits that a few rows need. **Ratings** are the limits nothing may ever cross, in any setup, including faults. **Equivalent** publishes what a port looks like to its neighbours. A parent uses a child the way a board designer uses an IC: it reads the child's operating conditions and checks its own circuit against them. It doesn't open the child. The same text renders as the datasheet table in the editor, and it can be printed as the block's real datasheet, so the verified numbers and the published numbers are the same numbers.

### 1.2 The sections

| Section | Datasheet name | What it holds | Becomes, in the engine |
|---|---|---|---|
| `ratings { }` | Absolute maximum ratings | limits on port quantities that must never be crossed | automatic sides, checked in every run of every fixture and every fault, at `worst_case` |
| `operating { }` | Recommended operating conditions | the environment, the **source** on every input, the **load** on every output | range knobs of the block's own fixtures; **containment checks** at every placement |
| `conditions { }` | "Test conditions unless otherwise noted", "typical values at…" | the default fixture's shape (ideal source? which test step?), the typical point, named condition sets, the default confidence | the header deck; the typical run; scenario defaults |
| `specs { }` | Electrical characteristics | the rows | one or two sides per row, each in a scenario |
| `equivalent { }` | "Equivalent input circuit" figures | a port model a neighbour may rely on (`SeriesRc { r, c }`) | auto-generated rows that check the model; the parent's containment uses its parameters |
| `faults { }` | Protection / fault behaviour | declared events: an open, a short, a swap, with a trigger and a duration | mode knobs and event parameters; fault scenarios; ratings rows in each |
| `fixture Name { }` | "See Figure 30" | a named test circuit: a form (header plus overrides) or a sheet (a circuit that places the block) | a scenario (form) or a separate deck (sheet) |

### 1.3 A row

```rust
/// Power-supply rejection at the regulator's crossover region.        ← the parameter's name and footnote
psrr: -ac(vout.v / vin.ripple).at(100kHz).db()   min 36dB  typ 40dB   with vin.v = 4.3V, vout.i = 50mA;
//    └──────────── measure ──────────────────┘  └─ limits ─┘ └ typ ┘   └──────── test conditions ──────┘
```

The columns, and their rules:

| Column | Written as | Rule |
|---|---|---|
| Name | `psrr:` | stable identity; with the placement path it's the `SpecPath` (P1) |
| Parameter | the `///` doc line | shown as the table's "Parameter" column |
| Measure | any measure expression (language §8.4), plus measure axes (`.at`, `.band`, `.sweep`, `.during`, `.after`) | a **measure axis** is part of the measure: "ΔVOUT as VIN goes from 4.4 to 5.25 V" is `.sweep(vin.v, 4.4V..=5.25V).span()`, not a condition |
| Min / Max | `min x`, `max y`, or `in lo..=hi`, or `in x ± t` | the limits; one side or two; `in` is shorthand the formatter keeps if written that way |
| Typ | `typ x` | **never a limit.** It is compared with the run at the `typical` point, printed next to it, and linted if it falls outside min/max (a wrong typ, or a model that disagrees with its datasheet) |
| Test conditions | `with a = v, b in r, set_name` | `=` pins a condition to a **fixed point**; `in` **narrows** a range (the row holds over the narrower range only); a name refers to a set from `conditions`. Anything not mentioned holds **over the full operating range** |
| Setup | `on Fixture` | only when the row needs a named test circuit |
| Confidence | `#[confidence(worst_case)]` above the row | default from `conditions { confidence: … }`, else `sigma(3)` (engine.md D11) |

That's the whole grammar of a row. It maps one to one onto the editor's spec table (§7.3).

**Three kinds of condition, kept visibly apart** (`contract_references.md` §18.1 lesson 5). A datasheet hides the difference in one column; the syntax doesn't:

| Kind | Datasheet example | Here |
|---|---|---|
| Fixed point | `IOUT = 50 mA` | `with vout.i = 50mA` |
| Quantified range ("for all") | `TJ = −40…125 °C` | the operating range, by default; `with temp in 0°C..=70°C` to narrow |
| Measure axis | `VOUT + 0.5 V ≤ VIN ≤ 5.5 V` for line regulation | `.sweep(vin.v, 3.8V..=5.5V)` inside the measure |

### 1.4 What replaces `assume`

`assume` did two jobs (`contract_references.md` §18.3): it defined range knobs, and it created obligations on neighbours. A datasheet puts both jobs in one table, "Recommended operating conditions", and every engineer knows how to read it. So:

```rust
// v0.1                                        // Option A
assume vcc.v in 12V ± 5%;                      operating {
assume temp in -10°C..=60°C;                       vcc.v: 12V ± 5%;
assume output.z_load >= 10kΩ;                      temp:  -10°C..=60°C;
                                                   output: Shunt { r: 10kΩ..=1MΩ };
                                               }
```

- There is no statement keyword. A line in `operating` is `quantity: range` or `port: source-or-load`.
- **The role rule stays** (language §5.3, §8.2). `operating` may name only what the *outside* sets: the environment, the source on an input (`vin.v`, `vin.z`, its ripple), the load on an output (`vout.i`, a load network). `specs` may name only what the block *produces* (its output voltage, its `z_in`, its `z_out`, its supply current). Writing `operating { output.v: … }` is the same `E-role` error as today, with the help text "a range on a quantity you drive is a row in `specs`".
- `ratings` is the other half of what people meant by "assume": limits the block can't survive past. It is not a knob. It's a check that runs everywhere.
- The CRML-style alternative (`when` / `given`) was considered. It keeps a keyword per statement, and it doesn't give the parent a table to read. The datasheet form wins on familiarity for the people who will read it: EEs, reviewers and the AI, which has read millions of datasheets.

### 1.5 Sources on inputs, loads on outputs

`contract_ee_practice.md` §1.6 settled the physics: **on an input you write a source; on an output you write a load.** "The previous stage misbehaves" is always a property of the source (its impedance, its noise, its ripple, its droop), because it describes what *arrives*. A load on an input would be an extra impedance across the block's own pins, which changes the block. The lead's question, "is an input's disturbance part of the source?", gets a plain **yes**.

The std library defines the vocabulary, in the language itself (a sketch):

```rust
// std/fixtures.spl (sketch)
/// What drives a power input: its voltage, its source impedance, and its disturbances.
pub struct Supply { v: Volt, z: Ohm, ripple: Ripple, droop: Droop, step: Step, ramp: Ramp }
/// What drives a signal input.
pub struct Signal { wave: Wave, z: Ohm, step: Step }
pub enum   Wave   { Dc(Volt), Sine { amp: Volt, freq: Hertz }, Step { by: Volt, edge: Time } }
/// What a power output feeds.
pub struct Load   { i: Amp, step: Step, c: Farad }
/// Signal-output loads.
pub struct Shunt    { r: Ohm, c: Farad }        // R ∥ C to ground
pub struct SeriesRc { r: Ohm, c: Farad }        // R in series, then C to ground (an RC filter's input)
/// Disturbances. Each field of this kind is a **handle** rows can name: `vin.ripple`, `vout.step`.
pub struct Ripple { pp: Volt, freq: Hertz }
pub struct Droop  { by: Volt, edge: Time, hold: Time }
pub struct Step   { from: Quantity, to: Quantity, by: Quantity, edge: Time }
pub struct Ramp   { from: Volt, time: Time }
```

- A field left out means "absent" for disturbances and "ideal" for impedances (`z` defaults to 0 Ω on a source; an output with no load is open, and gets a lint, see §2.9).
- A range in a field (`z: 0.1Ω..=0.5Ω`) is a range knob wherever the value is used in `operating`. The keyword position decides the meaning, as in language §5.3.
- **A load or a source can be another block.** `output: AdcInput` means "my load is that block's input". The fixture places the block there. This is how the ADC's sampling capacitor, a load that only exists in a transient with the sample clock, gets into its driver's checks (`contract_ee_practice.md` §2.6).
- **Two families are a union:** `output: SeriesRc { … } | Shunt { … }`. The union becomes a mode knob in the block's own checks.

**Handles are how rows choose a stimulus.** A disturbance declared on a source or load is inert until a row names it:

| Row says | The engine does |
|---|---|
| `ac(vout.v / vin.ripple)` | an AC run with the AC excitation on `vin`'s ripple slot only (every other AC source at 0) |
| `tran(vout.v).during(vout.step)` | a transient run that fires the load step, and measures inside its window |
| `tran(code).after(vbus.ramp)` | a transient from zero that fires the plug-in ramp, and measures after it |
| `tran(v3.v).during(cable_glitch)` | the same, for a declared fault (§2.8) |

This removes most fixtures. PSRR on a datasheet needs "Figure 5-1" because the AC source moves to VIN. Here the row names `vin.ripple` and the engine moves it (the CMRR/PSRR trap in `next_precision_analog.md`, where the default bench silently measured common-mode gain, can't happen: the row says what's excited).

---

## 2. The reference board, written in full

The board of `contract_ee_practice.md` §4: USB 5 V → 3.3 V LDO → op-amp gain stage → RC anti-alias filter → MCU ADC, with an MCU load on the rail and a 10 Ω + 10 µF filter for the analog rail.

```
                         ┌──────────────────────────── SensorBoard ───────────────────────────────┐
 USB2_LOW_POWER          │                                                                         │
 4.40–5.25 V ──[0.1–0.5Ω]┼●─vbus─┬─[ ldo: Ldo3v3 ]── v3 ─┬──[ vdda: VddaFilter ]── va ──┐         │
 ripple, droop, ramp     │       C_bus 4.7µ               │                               │         │
                         │                      [ mcu: McuLoad ] 5–30 mA, wake step       │ (vdd,   │
 sensor 100Ω–10kΩ        │                                                                │  vref)  │
 ≤100 mV, 20 Hz–1 kHz ───┼●─sensor─[ gain: GainStage (MidRef inside) ]── oa ──[ aa: AaFilter ]── ain ──[ adc: AdcInput ]
                         └─────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Shared definitions: the USB port and the parts

```rust
use std::prelude::*;
use std::fixtures::{Supply, Signal, Load, Shunt, SeriesRc, Ripple, Droop, Step, Ramp, Sine, Dc};

/// A USB 2.0 low-power host port seen through its cable (VBUS + GND wires and contacts).
/// This is the "USB source and cable": a source model, not a block. It lives outside the product.
pub const USB2_LOW_POWER: Supply = Supply {
    v:      4.40V..=5.25V,                                  // USB 2.0 §7.2.2, Table 7-7 [DS]
    z:      0.1Ω..=0.5Ω,                                    // cable + contacts [S §4.3]
    ripple: Ripple { pp: 100mV, freq: 100Hz..=1MHz },       // a design assumption: USB gives no number [S]
    droop:  Droop  { by: 330mV, edge: 1us, hold: 500us },   // USB 2.0 §7.2.4.2, hub port on attach [DS]
    ramp:   Ramp   { from: 0V, time: 100us },               // plug-in
};
```

The two ICs are subcircuit-backed part kinds (next_synthesis L5) whose models are the behavioural ones of `contract_ee_practice.md` §4.3. Their part records carry their own `absolute_max` and `operating` tables (language §6.5, §8.7), so the automatic checks see them. The records' fields used below: `Tlv755p.v_ref: 1.2025V ± 1%`, `Mcp6001.v_os: ±4.5mV`, and the abs-max rows `Tlv755p.vin: −0.3..=6.5 V` [DS], `Mcp6001.vdd: −0.3..=7.0 V`, `Mcp6001.inputs: vss − 1.0 V ..= vdd + 1.0 V or |i| ≤ 2 mA` [DS, from memory; to recheck against DS20001733L §1.0].

### 2.2 `Ldo3v3`

```rust
/// 5 V USB → 3.3 V, TLV755P-class LDO with its datasheet capacitors.
pub block Ldo3v3 {
    port vin: Power<In>;
    port vout: Power<Out>;
    port gnd: Ground;
    net co;

    let u1    = Tlv755p   { vin, vout, gnd, en: vin };
    let c_in  = Capacitor { a: vin, b: gnd, value: 1uF ± 10%, dielectric: X7R };
    /// Datasheet minimum 0.47 µF effective. ESR kept explicit: it shapes the loop.
    let c_out = Capacitor { a: vout, b: co, value: 1uF ± 10%, dielectric: X7R };
    let esr   = Resistor  { a: co, b: gnd, value: 5mΩ };
}

contract Ldo3v3 {
    ratings {
        vin.v: -0.3V..=6.5V;                      // TLV755P abs max [DS]
    }
    operating {                                   // recommended operating conditions
        vin:  Supply { v: 3.6V..=5.5V, z: ..=0.5Ω };
        vout: Load   { i: 0.1mA..=500mA, c: ..=10uF };
        temp: -40°C..=125°C;
    }
    conditions {                                  // unless otherwise noted
        vin:  Supply { z: 0Ω, step: Step { by: 1V, edge: 1us }, ramp: Ramp { from: 0V, time: 100us } };
        vout: Load   { step: Step { from: 5mA, to: 30mA, edge: 1us } };
        typical: temp = 25°C, vin.v = 5V, vout.i = 30mA;
    }
    specs {
        /// Output accuracy.
        vout_dc:   dc(vout.v)                                        in 3.3V ± 2%  typ 3.3V     with vout.i in 0.1mA..=50mA;
        /// Line regulation: output change over the USB input range.
        line_reg:  dc(vout.v).sweep(vin.v, 4.4V..=5.25V).span()      typ 1mV   max 5mV       with vout.i = 30mA;
        /// Load regulation.
        load_reg:  -dc(vout.v).sweep(vout.i, 0.1mA..=500mA).slope()  typ 56mV/A  max 0.1V/A;
        /// Dropout: input-output difference where the output has fallen 1%.
        dropout:   dc(vin.v - vout.v).sweep(vin.v, 3.6V..=3.2V).first(vout.v <= 0.99 * 3.3V)
                                                                     typ 215mV  max 250mV   with vout.i = 500mA;
        psrr:      -ac(vout.v / vin.ripple).at(100kHz).db()          min 36dB  typ 40dB     with vin.v = 4.3V, vout.i = 50mA;
        load_step: tran(vout.v).during(vout.step).deviation()        max 50mV  typ 20mV;
        line_step: tran(vout.v).during(vin.step).deviation()         max 20mV  typ 9mV      with vin.v = 4.3V, vout.i = 30mA;
        /// Start-up (see Figure "StartUp").
        t_start:   tran(vout.v).after(vin.ramp).crossing(3.2V)       max 1ms   typ 385us    on StartUp;
        start_pk:  tran(vout.v).after(vin.ramp).peak()               max 3.4V               on StartUp;
        /// Port characteristic: what the loads on the rail may rely on.
        z_out:     vout.z_out.band(10Hz..=1MHz).max()                max 2Ω                 with vout.i in 5mA..=500mA;
        i_gnd:     dc(vin.i - vout.i)                                typ 25uA  max 40uA     with vout.i = 0.1mA;
    }
    /// Figure "StartUp": the header circuit, started from zero.
    fixture StartUp { start: zero, tran: 0ms..=2ms }
}
```

Notes, with the numbers behind them:
- `vout_dc` is narrowed to 0.1–50 mA; the block is rated to 500 mA, where load regulation costs another 28 mV [S]. The simulated range at nominal V_ref is 3.2967–3.3013 V [S].
- `z_out` is where the datasheet habit and the physics meet. The first draft (`contract_ee_practice.md` §4.4) said "≤ 1 Ω for I_OUT ≥ 1 mA", and the peak is 3.2 Ω at 1 mA [S]. Narrowing to ≥ 5 mA isn't enough for 1 Ω either: the peak is **1.46 Ω at 5 mA** (33 kHz), and 0.62 Ω at 30 mA **[A]**. So the row says `max 2Ω` over 5–500 mA. **An LDO is worst at light load**, because its output pole moves down as the load current falls (§3.3 of the EE report). A datasheet row makes that visible: the condition is right next to the number.
- `psrr`, `line_step` and `dropout` pin conditions (`=`): they are datasheet-style fixed test points, used for comparison. Everything else holds over the operating range.
- `StartUp` is a **form fixture**: the header circuit, but started from all-zero instead of the operating point. It's named because two rows share it and because start-up is a setup people recognize.

### 2.3 `VddaFilter`

```rust
/// RC filter for the analog rail (ST AN2834). Cuts the MCU's load steps off the ADC reference.
pub block VddaFilter {
    port vin: Power<In>;
    port vout: Power<Out>;
    port gnd: Ground;
    let r = Resistor  { a: vin, b: vout, value: 10Ω ± 1% };
    let c = Capacitor { a: vout, b: gnd, value: 10uF ± 10%, dielectric: X7R };
}

contract VddaFilter {
    operating {
        vin:  Supply { v: 3.0V..=3.6V, z: ..=2Ω };
        vout: Load   { i: ..=1mA };
    }
    specs {
        /// A pass-through row: the bound is written in terms of the operating input.
        vout_v: dc(vout.v)                                    min vin.v - 12mV  max vin.v;
        atten:  -ac(vout.v / vin.ripple).at(10kHz).db()       min 14dB  typ 16dB;
        z_out:  vout.z_out.band(10Hz..=1MHz).max()            max 13Ω;
    }
}
```

`vout_v` is a **relative row**: its limits depend on the operating input. Passive and pass-through blocks need this, because they don't set a voltage, they pass one along. The parent evaluates the bound with the upstream block's real range (§3.3).

### 2.4 `MidRef` (placed inside `GainStage`)

```rust
/// Mid-rail bias: vmid = vdd / 2, heavily filtered.
pub block MidRef {
    port vdd: Power<In>;
    port gnd: Ground;
    port mid: Analog<Out>;
    let r_m1  = Resistor  { a: vdd, b: mid, value: 100k ± 1% };
    let r_m2  = Resistor  { a: mid, b: gnd, value: 100k ± 1% };
    let c_mid = Capacitor { a: mid, b: gnd, value: 1uF ± 10% };
}

contract MidRef {
    operating {
        vdd: Supply { v: 3.0V..=3.6V, z: ..=20Ω };
        mid: Shunt  { r: 400kΩ.., c: ..=10nF };      // a bias resistor into a CMOS input
    }
    specs {
        ratio: dc(mid.v / vdd.v)                        in 0.5 ± 1.1%  typ 0.5;
        z_out: mid.z_out.at(1Hz)                        max 50kΩ;
        i_vdd: dc(vdd.i)                                max 20uA  typ 16.5uA;
    }
}
```

`ratio` is ±1.1%, not ±0.5%. The ratio R2/(R1+R2) of two equal ±1% resistors moves by (δ2 − δ1)/4 absolute, which is ±1% of 0.5. This matters for the board's `midscale` row (§2.10).

### 2.5 `GainStage`

```rust
/// Non-inverting AC gain of 10.09 around an MCP6001, biased at mid-rail.
pub block GainStage {
    port vdd: Power<In>;
    port gnd: Ground;
    port input: Analog<In>;
    port output: Analog<Out>;
    net inp;
    net inn;
    net ng;
    net vmid;

    let mid  = MidRef    { vdd, gnd, mid: vmid };
    let c_in = Capacitor { a: input, b: inp, value: 220nF ± 10% };
    /// Sets the bias and **is** the stage's input impedance.
    let r_b  = Resistor  { a: inp, b: vmid, value: 470k ± 1% };
    let u2   = Mcp6001   { inp, inn, vdd, vss: gnd, out: output };
    /// Gain = 1 + R_f/R_g = 10.09.
    let r_f  = Resistor  { a: output, b: inn, value: 9.09k ± 1% };
    let r_g  = Resistor  { a: inn, b: ng, value: 1k ± 1% };
    /// Makes the DC gain 1, so the output sits at vmid.
    let c_g  = Capacitor { a: ng, b: gnd, value: 47uF ± 20% };
}

contract GainStage {
    operating {
        vdd:    Supply   { v: 3.0V..=3.6V, z: ..=20Ω, ripple: Ripple { pp: 50mV, freq: 20Hz..=1MHz } };
        input:  Signal   { wave: Sine { amp: ..=100mV, freq: 20Hz..=1kHz }, z: ..=10kΩ };
        /// Designed to drive an RC anti-alias filter, or a light resistive load.
        output: SeriesRc { r: 390Ω..=1kΩ, c: 47nF..=220nF } | Shunt { r: 10kΩ..=1MΩ, c: ..=1nF };
        temp:   0°C..=70°C;
    }
    conditions {
        input: Signal { step: Step { by: 10mV, edge: 50ns } };
        typical: temp = 25°C, vdd.v = 3.3V, input.z = 0Ω, output = SeriesRc { r: 470Ω, c: 100nF };
    }
    specs {
        let h = ac(output.v / input.v);                       // pin to pin
        gain:   h.at(1kHz).mag()                                    in 10.09 ± 3%  typ 10.1;
        f_low:  h.f_low(-3dB)                                       max 5Hz        typ 3.7Hz;
        f_high: h.f_high(-3dB)                                      min 50kHz      typ 60.6kHz;
        /// No clipping at full amplitude on the lowest supply.
        clip:   tran(output.v).during(input.wave).thd(1kHz)         max 1%         with vdd.v = 3.0V, input.wave = Sine { amp: 100mV, freq: 1kHz };
        /// Stable into every allowed load. The worst load can be inside the range (§3.3 of the EE report).
        #[search(output.c)]
        stable: tran(output.v).during(input.step).overshoot()       max 10%        typ 2.4%;
        psrr:   -ac(output.v / vdd.ripple).band(20Hz..=1kHz).max().db()   min 60dB  typ 66dB;
        /// Port characteristics, for the neighbours.
        z_in:   input.z_in.band(20Hz..=1kHz).min()                  min 450kΩ      typ 470kΩ;
        z_out:  output.z_out.band(10Hz..=25kHz).max()               max 100Ω       typ 73.5Ω;
        i_vdd:  dc(vdd.i)                                           max 150uA      typ 117uA;
    }
}
```

- The `output` line is the whole loading story of `contract_ee_practice.md` §3.4 in one line. An op-amp drives a capacitor badly: 100 nF straight on the output rings with 69% overshoot; behind 470 Ω it's 2.4% [S]. So the allowed loads are "an RC with at least 390 Ω in front" or "a resistor with at most 1 nF". The union becomes a mode knob in the stage's own checks.
- `#[search(output.c)]` marks the measure as *interior possible* (next_synthesis E1): phase margin and overshoot can be worst at a middle capacitance, so the engine runs a real 1-D search along the load capacitance on a log axis (M4), not only the corners.
- `z_out` over 10 Hz–25 kHz: 0.30 Ω at 100 Hz, 3.0 Ω at 1 kHz, 10.7 Ω at 3.39 kHz, 30 Ω at 10 kHz, **73.5 Ω at 25 kHz** **[A]**. Feedback makes the output impedance tiny at low frequency, and it rises as the loop gain falls. This row decides a connection check in §3.

### 2.6 `AaFilter`

```rust
/// RC anti-alias filter, 3.39 kHz. R_aa is also the op-amp's isolation resistor.
pub block AaFilter {
    port input: Analog<In>;
    port output: Analog<Out>;
    port gnd: Ground;
    let r_aa = Resistor  { a: input, b: output, value: 470Ω ± 1% };
    let c_aa = Capacitor { a: output, b: gnd, value: 100nF ± 10%, dielectric: X7R };
}

contract AaFilter {
    operating {
        input:  Signal { z: ..=10Ω };               // as first written; §3.4 shows why this is too tight
        output: AdcInput;                           // the load is that block's input model
    }
    equivalent {
        /// What the driver sees. Checked by the engine, and read by the parent.
        input: SeriesRc { r: r_aa.value, c: c_aa.value };
    }
    specs {
        let h = ac(output.v / input.v);
        f_c:        h.f_high(-3dB)                                  in 3.39kHz ± 15%;
        alias:      (h.at(1kHz).mag() / h.at(25kHz).mag()).db()     min 15dB  typ 17.7dB;
        /// The ADC's kick must settle within 1 LSB by the end of each sample.
        drives_adc: tran(output.load.sample_error).after(input.wave).window(2ms..=3ms).absmax()
                                                                    max 1    typ 0.47   with input.wave = Dc(1.65V);
    }
}
```

`drives_adc` is the row an AC plot can't see: three filters with the same corner give −0.47, −4.6 and −45 LSB [S]. The load is `AdcInput`, so the header fixture places that block, sample clock included. `output.load.sample_error` is a measure the load block publishes (§2.7).

### 2.7 `AdcInput` and `McuLoad`: blocks that publish, but promise little

```rust
/// STM32F103 ADC input while sampling (DS5319 §5.3.18): switch + sample cap, reset to 0 V between channels.
pub block AdcInput {
    port input: Analog<In>;
    port vref: Power<In>;
    port gnd: Ground;
    net cs;
    let sw   = SampleSwitch { a: input, b: cs, r_on: 1kΩ, f_s: 50kHz, t_s: 1.125us, reset_to: gnd };  // R_ADC max
    let c_s  = Capacitor    { a: cs, b: gnd, value: 8pF };                                            // C_ADC max
    let leak = CurrentSource { p: input, n: gnd, value: -1uA..=1uA };                                 // injection/leakage
}

contract AdcInput {
    ratings {
        input.v: -0.3V..=(vref.v + 0.3V);
    }
    operating {
        /// The R_AIN rule: T_S / (f_ADC · C_ADC · ln(2^14)) − R_ADC = 13.5 kΩ.
        input: Signal { z: ..=13.5kΩ };
        vref:  Supply { v: 3.3V ± 3% };
    }
    specs {
        /// Published measures, for the blocks that use this one.
        pub let lsb = vref.v / 4096;
        pub let code = input.v / lsb;
        pub let sample_error = sw.end_of_sample(cs.v - input.v) / lsb;
        i_avg: dc(input.i)                  max 1.7uA  typ 0.66uA;       // C_s · V · f_s, plus leakage
    }
}

/// Stand-in for the MCU's digital supply current (its ADC is `AdcInput`).
pub block McuLoad {
    port vdd: Power<In>;
    port gnd: Ground;
    env i_run: Amp;                                        // set by the firmware, not by a part
    let i = CurrentSink { p: vdd, n: gnd, dc: i_run, wake: Step { from: 5mA, to: 30mA, edge: 1us } };
}

contract McuLoad {
    ratings   { vdd.v: -0.3V..=4.0V; }                      // STM32F103 abs max [DS]
    operating {
        #[owner(firmware)] i_run: 5mA..=30mA;
        vdd: Supply { v: 2.0V..=3.6V };
    }
    specs     { i_vdd: dc(vdd.i) max 30mA; }
}
```

- `wake` is a handle declared by a behavioural part (the current sink). Rows elsewhere can say `.during(mcu.wake)`.
- `i_run` is a firmware-owned range knob (engine.md §2.1). A verdict that rests on it says "PASS given the firmware keeps i_run in 5–30 mA".

### 2.8 The board

```rust
/// USB-powered sensor front end (contract_ee_practice.md §4).
pub block SensorBoard {
    port vbus: Power<In>;                  // J1, USB
    port gnd: Ground;
    port sensor: Analog<In>;               // J2
    net v3;
    net va;
    net oa;
    net ain;

    /// USB 2.0 allows ≤ 10 µF on VBUS at attach.
    let c_bus = Capacitor  { a: vbus, b: gnd, value: 4.7uF ± 10% };
    let ldo   = Ldo3v3     { vin: vbus, vout: v3, gnd };
    let c_dec = Capacitor  { a: v3, b: gnd, value: 100nF ± 10% };
    let mcu   = McuLoad    { vdd: v3, gnd };
    let vdda  = VddaFilter { vin: v3, vout: va, gnd };
    let gain  = GainStage  { vdd: va, gnd, input: sensor, output: oa };
    let aa    = AaFilter   { input: oa, output: ain, gnd };
    let adc   = AdcInput   { input: ain, vref: va, gnd };
}

contract SensorBoard {
    ratings {
        vbus.v: -0.3V..=6.0V;                            // what J1 may see without damage
    }
    operating {
        vbus:   USB2_LOW_POWER;                          // source: voltage, cable, ripple, droop, plug-in
        sensor: Signal { wave: Sine { amp: ..=100mV, freq: 20Hz..=1kHz }, z: 100Ω..=10kΩ };
        temp:   0°C..=70°C;
    }
    conditions {
        confidence: sigma(3);
        typical: temp = 25°C, vbus.v = 5V, vbus.z = 0.3Ω, sensor.z = 1kΩ, mcu.i_run = 10mA;
    }
    faults {
        /// Contact bounce, or a brief pull on the cable on impact: VBUS opens, then returns.
        cable_glitch: open(vbus) at 1ms for 20us..=1ms;
        /// The cable is pulled out for good.
        unplug:       open(vbus) at 1ms;
        /// A miswired adapter swaps VBUS and GND at J1.
        reversed:     swap(vbus, gnd);
    }
    specs {
        let code = adc.code;
        /// Codes per millivolt of sensor EMF, for every sensor impedance (source loading included).
        code_gain:  ac(code / sensor.wave).band(20Hz..=1kHz).mag()          in (12.0 / 1mV) ± 7%   typ 12.01 / 1mV;
        /// No-signal reading. Ratiometric: V_REF's tolerance cancels.
        midscale:   dc(code)                                                in 2048 ± 20          typ 2047.6;
        /// VBUS ripple seen at the ADC, in codes peak to peak.
        ripple:     ac(code / vbus.ripple).band(vbus.ripple.freq).mag().max() * vbus.ripple.pp
                                                                            max 1                 typ 0.08;
        mcu_step:   tran(code - dc(code)).during(mcu.wake).absmax()         max 4                 typ 2.1;
        droop:      tran(code - dc(code)).during(vbus.droop).absmax()       max 4                 with vbus.v = 4.40V;
        alias:      aa.alias;                                               // the child's row, re-published
        /// Data valid after plug-in (see Figure "ColdPlug").
        t_valid:    tran(code).after(vbus.ramp).settle(2048 ± 4)            max 1.5s              typ 1.49s   on ColdPlug;
        #[confidence(worst_case)]
        usb_cap:    vbus.c_total                                            max 10uF;
        #[confidence(worst_case)]
        usb_current: dc(vbus.i)                                             max 100mA             typ 10.15mA;
        /// Informative only: no limit.
        inrush:     tran(vbus.i).after(vbus.ramp).peak()                    typ 424mA             on ColdPlug;

        /// Faults: hold-up through a short glitch, and recovery after a longer one.
        #[confidence(worst_case)]
        holdup:     tran(v3.v).during(cable_glitch).min()                   min 3.0V              with cable_glitch.length in ..=200us;
        #[confidence(worst_case)]
        recover:    tran(code).after(cable_glitch).settle(2048 ± 4)         max 200ms             with cable_glitch.length = 1ms;

        /// One-off: a scope probe on the op-amp output during bring-up must not make it ring.
        probe_ok:   tran(gain.output.v).during(sensor.step).overshoot()     max 10%               on ScopeProbe;
    }
    /// Figure "ColdPlug": header circuit from zero, 2 s.
    fixture ColdPlug { start: zero, tran: 0s..=2s }
}

/// One-off bring-up fixture: a 10× probe (10 MΩ ∥ 15 pF) on the op-amp output.
#[oneoff]
fixture ScopeProbe for SensorBoard {
    let dut   = SensorBoard { ..header };
    let probe = Shunt { a: dut.gain.output, b: dut.gnd, r: 10MΩ, c: 15pF ± 20% };
    sensor: Signal { step: Step { by: 10mV, edge: 50ns }, z: 100Ω };
}
```

What each board row tests, and what we know of it:

| Row | What it's about | Numbers |
|---|---|---|
| `code_gain` | source loading (R_b vs R_s), stage gain, filter droop, V_REF together | 11.35–12.71 codes/mV over 256 corners × 2 R_s × 2 f [S] ✅ |
| `midscale` | V_OS, divider mismatch, ADC leakage and average current | worst case **2020.9 … 2074.2 codes (−27 / +26)** **[A]** ❌ at `worst_case`; ≈ ±16 codes at `sigma(3)` by arithmetic ✅. See the note below |
| `ripple` | LDO PSRR → V_REF: no block row covers it | 0.08 LSB pp with the VDDA filter, 0.47 without [S] ✅ |
| `mcu_step` | shared rail → ADC reference | −0.4 / +2.1 codes with the filter; −10 / +12 without [S] ✅ |
| `droop` | USB droop through the LDO | 2047.9–2048.3 [S] ✅ |
| `t_valid` | every RC time constant together (C_g dominates, 0.47 s) | 2048.05 at 1.49 s [S] ✅ (thin) |
| `usb_cap` | structure | 4.7 µF + 1 µF, +10% each = 6.27 µF ✅ (arithmetic, 0 runs) |
| `usb_current` | USB unit load | ≈ 30.2 mA at 30 mA [S] ✅ |
| `holdup` | charge in C_bus + C_in + C_out keeps the rail up | worst **3.225 V** at 200 µs (caps −10%, VBUS 4.40 V, cable 0.5 Ω, 30 mA); 3.295 V at 100 µs **[A]** ✅ |
| `recover` | how fast the code returns after a 1 ms dropout | **104 ms** at VBUS 4.40 V, 93 ms at 5.25 V (30 mA, nominal parts) **[A]** ✅ |
| `probe_ok` | stability with a probe load | overshoot 1.51% without, 1.51% with 15 pF, 1.52% with 100 pF **[A]** ✅ |

**A finding from writing this out.** `contract_ee_practice.md` §4.5 budgets the MidRef mismatch at ±0.5% (±10.2 codes). The ratio of two ±1% resistors moves by ±1%, so the term is ±20.5 codes. A DC simulation at the aligned corner (R_m1 101 k, R_m2 99 k, V_OS −4.5 mV, leakage +1 µA) gives 2020.9, and the opposite corner 2074.2 **[A]**. So `midscale` FAILS at `worst_case` and passes at `sigma(3)` (σ from the ±1% edges at 3σ: about ±16 codes). That's the CE amp's `bias` story again, and the report says so. A 0.1% divider brings the worst case to about ±9 codes. A datasheet-style row made the error easy to find: the `ratio` row of `MidRef` states ±1.1%, and the board's budget has to use it.

**The hold-up, explained.** When the cable opens, nothing feeds the board, so the MCU's 30 mA comes out of the capacitors: C_bus, the LDO's C_in and C_out, about 6.8 µF. That's roughly 4.4 V/ms. The LDO keeps 3.3 V until its input falls to about 3.3 V plus its dropout, which takes a little over 200 µs. The rail falls to 3.2 V after 190 µs (without the LDO's C_in counted) and to 3.0 V after 270 µs at 30 mA; at 5 mA it holds for more than 1 ms **[A]**.

**The one-off.** `ScopeProbe` is used by one row. It's a real fixture (a sheet: it places the board and adds a part), checked every time like any other row, and folded away in the editor's table under "one-off checks". The probe changes nothing measurable, because R_aa already isolates the op-amp from capacitance. That's a useful answer for a quick question, and it costs 32 runs.

### 2.9 The faults, and the reversed connector

**What the declarations do.**
- `cable_glitch: open(vbus) at 1ms for 20us..=1ms` puts a smooth switch in series with the VBUS pin of the board's header fixture. It conducts in every normal run. In a fault run it opens at 1 ms for a time that is itself a range knob (`cable_glitch.length`). `holdup` narrows that knob to ≤ 200 µs; `recover` pins it to 1 ms.
- `unplug` is the same switch, opened for good.
- `reversed: swap(vbus, gnd)` can't be a switch in the same deck, because it re-routes the connector. It gets its own deck: the USB source is connected backwards.
- **Every declared fault gets the ratings checks,** whether or not a row mentions it. That's "no damage if reversed" with no new spec kind (P15 item 3).

**What the engine finds for `reversed`** **[A]**, and the physics. With VBUS backwards, the LDO's pass transistor has a built-in diode (its "body diode") from its output to its input. It conducts, and it pulls the whole 3.3 V rail negative. The model says the board draws **9.6 A** through the cable at 4.40 V (current limited only by the cable and the diodes), the LDO's VIN pin sits at **−1.53 V** and the 3.3 V rail at **−1.0 V**. A real host would trip its current limit; the LDO would likely be destroyed first. The ratings rows fail at once:

```
error[E-rating]: fault `reversed` drives `ldo.u1.vin` to −1.53 V; Tlv755p abs max is −0.3 V
 --> board.spl:31:9
   |
31 |         reversed:     swap(vbus, gnd);
   |         ^^^^^^^^ fault declared here
   = also: `mcu.vdd` at −1.0 V (abs max −0.3 V); `gain.u2.vdd` at −1.0 V (abs max −0.3 V)
   = note: 9.6 A flows through the cable at VBUS 4.40 V (the model has no current limit; not_modeled: host port limit)
   = help: add reverse protection: a series Schottky on VBUS (costs ≈ 0.3 V of headroom), or a P-MOSFET ideal diode
```

This verdict doesn't depend on the model's accuracy: the pin is below −0.3 V as soon as any diode conducts. With a Schottky in series (≈ 0.29 V at 30 mA), the reversed case draws 10 µA and the rail stays at 0.08 V: no rating is crossed **[A]**. The headroom cost is visible in the other rows: at VBUS 4.40 V through a 0.5 Ω cable the LDO input becomes 4.10 V, still far above 3.3 V plus 13 mV of dropout at 30 mA [S], and the rail still reads 3.296 V; hold-up to 3.2 V shortens from 190 to 150 µs **[A]**.

**What `unplug` finds.** When the rail collapses for good, the mid-rail bias keeps its charge (C_mid 1 µF behind 50 kΩ: a 50 ms time constant) while the op-amp's supply falls in about 2 ms. The op-amp's input then sits **1.56 V above its own supply** (30 mA case; 1.19 V at 5 mA) **[A]**. The MCP6001's input rating is "VDD + 1.0 V, *or* current limited to 2 mA" [DS, from memory]. The voltage part fails; the current part passes, because R_b (470 kΩ) limits the current to a few µA. So the ratings row must understand datasheet "or" clauses, and the model has no input clamp diodes (`not_modeled`, engine.md §3.4). The verdict is PASS on the current clause, tagged model-conditional. This is exactly the kind of case a hand-drawn switch at nominal never looks at (G19).

**The FMEA table the engine emits** (P15 item 6), one line per fault:

| Fault | Rows in scope | Ratings | Verdict |
|---|---|---|---|
| `cable_glitch` | `holdup` ✅ 3.225 V; `recover` ✅ 104 ms | all within | PASS |
| `unplug` | none | `gain.u2` input: voltage clause ✗, current clause ✅ (µA ≪ 2 mA), model has no clamp | PASS (model-conditional) |
| `reversed` | none | `ldo.u1.vin` ✗ −1.53 V; `mcu.vdd` ✗; `gain.u2.vdd` ✗ | **FAIL** → fixed with a Schottky |

### 2.10 `ce_amp.spl`, the simple case

```rust
contract CeAmp {
    operating {
        temp:  -10°C..=60°C;
        vcc.v: 12V ± 5%;
    }
    specs {
        let h = ac(output.v / input.v);
        /// Room for the output to swing ±1 V without clipping.
        bias: dc(output.v)       min 4.5V  max 6.5V;
        /// The next stage expects this level.
        gain: h.at(1kHz).mag()   in 4.6 ± 5%;
        /// Don't cut the bass.
        bass: h.f_low(-3dB)      max 30Hz;
    }
}
```

- Same length as today. It elaborates to the same **8 knobs** (2 range from `operating`, 6 statistical from the parts) and the same **3 rows / 5 sides**, which is the M1d acceptance test (roadmap §4.2). The engine deck is byte-identical to `engine_plan.md` §1.4: no load, an ideal AC source on `input`, a swept DC source on `vcc`.
- No `conditions` header means "the defaults": ideal sources, open outputs, typical point = nominal.
- **The lint that teaches the BD lesson.** `output` has no load in `operating`, so the rows hold unloaded only:
  ```
  warning[W-unloaded]: `output: Analog<Out>` has no load in `operating`; `gain` and `bass` hold with the output open
   = note: a parent that loads `output` can't reuse these rows (a next CE stage divides the gain to 2.88, [S] §1.3)
   = help: add e.g. `output: Shunt { r: 5kΩ..=1MΩ };` and a `z_out` row
  ```
- The grown version, once someone places two of them in a chain, adds two lines to `operating` (`input: Signal { z: ..=1kΩ }`, `output: Shunt { r: 5kΩ..=1MΩ }`) and two rows (`z_in`, `z_out`). That's the whole path from the MVP to a reusable block.
- **Migration is mechanical:** `assume x in r;` → `operating { x: r; }`, `spec n: m rel b;` → `specs { n: m limits; }`, `where temp == 25°C` → `with temp = 25°C`. The formatter can do it.

---

## 3. Hierarchy: reading the child's datasheet

### 3.1 How a child's rows and fixtures travel

```
   child GainStage (standalone)                      placed in SensorBoard as `gain`
   ─────────────────────────────                     ──────────────────────────────────────────────
   operating  ─── range knobs of its fixtures        operating ─── containment checks at each port
   conditions ─── its header fixture (stays home)    conditions ── not used (the board is the fixture)
   specs      ─── verdicts, cached per definition    specs ─────── rows travel with path `gain.…`:
   equivalent ─── checked port models                               carried if containment holds,
   ratings    ─── automatic sides                                   re-evaluated in context if not
                                                     ratings ───── checked in every board run and fault
```

This is the "checks travel, stimulus stays home" rule of `contract_references.md` §16, in datasheet words:
- **Rows travel.** Every child row appears in the parent's report under its placement path (`gain.gain`, `gain.z_out`, `gain.mid.ratio`). Nothing is dropped silently (L4).
- **Fixtures stay home.** The child's header fixture and its named fixtures are its unit tests. They run once per *definition*, not per placement.
- **Operating conditions turn into checks.** At each port of each placement, the parent checks that what it really connects fits the child's operating section.
- **Ratings go everywhere.** A child's ratings are checked in every parent run, every parent fixture and every fault.

### 3.2 The checks at each connection, both directions

Every connection is a voltage divider (`contract_ee_practice.md` §1.3): the driver's output impedance against the receiver's input impedance. So each connection has two checks, one per direction, and each check compares a **row** of one block with an **operating line** of the other:

| Direction | Published (a row) | Required (an operating line) | Example |
|---|---|---|---|
| forward: what arrives | `vout.v`, `z_out` | `vin: Supply { v, z }` / `input: Signal { z }` | LDO `vout_dc` ⊆ VddaFilter `vin.v` |
| backward: what's drawn | `i_vdd` row, `z_in` row, or an `equivalent` model (downstream) | `operating { vout: Load { i } }` / `output: SeriesRc { … }` (upstream) | AaFilter's `equivalent` vs GainStage's `output` family |
| sums on a rail | `Load { i }` range | Σ of every sink's `i` row | `mcu.i_vdd + vdda…` ⊆ LDO `vout.i` |
| environment | — | every child's `temp` line ⊇ the parent's | board 0–70 °C ⊆ LDO −40–125 °C |

**A row can be used only inside its own test conditions.** A datasheet number means nothing outside its conditions, and the check says so. The LDO's `z_out` row holds "with vout.i in 5 mA..=500 mA", so the parent must also show that the rail's real load stays ≥ 5 mA. It does: the MCU alone draws at least 5 mA.

### 3.3 The gain stage placed in the board, checked

The parent computes each quantity along the chain from the rows, then compares. This is edg's link arithmetic (`contract_references.md` §14.2) done on rows, with both directions:

| # | Connection | Guarantee (from rows) | Requirement (operating) | Result |
|---|---|---|---|---|
| 1 | `vbus` → `ldo.vin` | 4.40–5.25 V minus 0.5 Ω × 31 mA = 4.384–5.25 V; z 0.1–0.5 Ω | `3.6V..=5.5V`, `z ..=0.5Ω` | ✅ |
| 2 | `ldo.vout` → `vdda.vin` | `vout_dc` 3.234–3.366 V (holds for 0.1–50 mA; real load 5.1–31 mA ✅); `z_out` ≤ 2 Ω (holds ≥ 5 mA ✅) | `3.0V..=3.6V`, `z ..=2Ω` | ✅ |
| 3 | rail `v3` load | `mcu.i_vdd` ≤ 30 mA + VddaFilter ≤ 1 mA → ≤ 31 mA | `ldo.vout.i 0.1mA..=500mA` | ✅ |
| 4 | `vdda.vout` → `gain.vdd` | `vout_v` ∈ [vin − 12 mV, vin] → 3.222–3.366 V; `z_out` ≤ 13 Ω | `3.0V..=3.6V`, `z ..=20Ω` | ✅ |
| 5 | rail `va` load | `gain.i_vdd` ≤ 150 µA + `adc.i_avg` ≤ 1.7 µA | `vdda.vout.i ..=1mA` | ✅ |
| 6 | `sensor` → `gain.input` | board operating: z 100 Ω–10 kΩ, ≤ 100 mV, 20 Hz–1 kHz | `z ..=10kΩ`, `amp ..=100mV`, `freq 20Hz..=1kHz` | ✅ |
| 7 | `gain.output` → `aa.input` (backward) | `aa.equivalent`: SeriesRc r 465.3–474.7 Ω, c 90–110 nF | `SeriesRc { r: 390Ω..=1kΩ, c: 47nF..=220nF }` | ✅ |
| 8 | `gain.output` → `aa.input` (forward) | `gain.z_out` ≤ 100 Ω over 10 Hz–25 kHz (73.5 Ω at 25 kHz) | `aa.input: Signal { z: ..=10Ω }` | ❌ |
| 9 | `aa.output` → `adc.input` | `aa` output z ≤ 475 Ω (470 Ω ∥ 100 nF) | `z ..=13.5kΩ` | ✅ |
| 10 | `va` → `adc.vref` | 3.222–3.366 V | `3.3V ± 3%` = 3.201–3.399 V | ✅ |
| 11 | environment | board 0–70 °C | gain 0–70, LDO −40–125, … | ✅ (joined) |

**When all of a child's checks pass, its rows carry over with zero runs.** `gain.gain`, `gain.f_low`, `gain.f_high`, `gain.stable`, `gain.clip`, `gain.psrr`, `gain.z_in`, `gain.z_out` and `gain.i_vdd` are PASS in the board, resting on checks 4–7 and 11 and on GainStage's own cached verdicts (`Record.rests_on`, next_synthesis R1). Check 7 is the one that matters for stability: the board's real load (470 Ω + 100 nF) lies inside the family GainStage was checked over, so the 69%-overshoot case can't occur. Remove R_aa and check 7 fails with both lines shown.

**When a check fails (row 8), the child's rows don't apply in this placement,** and the report says why:

```
warning[W-operating]: `gain.output` drives `aa.input` outside AaFilter's operating conditions
  --> board.spl:14:9
   = aa requires: input.z ≤ 10 Ω                       (aa_filter.spl:6)
   = gain gives:  output.z_out ≤ 100 Ω, 10 Hz–25 kHz   (gain_stage.spl:39; 73.5 Ω at 25 kHz)
   = aa's rows f_c, alias, drives_adc are checked in context instead
   = help: widen AaFilter's `input.z`, or publish GainStage's output as an `equivalent`
```

Then the engine uses the lowest rung that is sound (`contract_hierarchy.md` §4.6): a cached two-port table of GainStage composed with AaFilter's (exact for the linear rows `f_c` and `alias`, 0 new runs once cached), and a small in-context run for `drives_adc` (a transient; tables can't carry it). In context, `f_c` is 3.73 kHz, inside ±15% [S]. So the row passes, but the *reason* changed from "carried" to "in context", and the report shows both.

### 3.4 The honest lesson of row 8

The obvious fix is to widen AaFilter's operating line to `z: ..=100Ω`. That makes the filter's own check fail: with a 100 Ω resistive source the corner is 1/(2π · 570 Ω · 100 nF) = 2.79 kHz, 18% low, outside ±15%. But the op-amp's output isn't a 100 Ω resistor. It's close to 0 Ω at low frequency and rises with frequency, like an inductor. A **scalar impedance range is pessimistic** for a port whose impedance depends on frequency. Three ways out, each a real design path:

1. **Publish the driver's port as a model:** `equivalent { output: OpampOut { z_dc: ..=1Ω, gbw: 0.9MHz..=1.1MHz, r_o: 300Ω } }`, and let AaFilter's operating line name that family. The containment check becomes parameter containment, like row 7. It's the IBIS lesson: the fixture is part of the data.
2. **Use the cached tables** (`contract_hierarchy.md` §2.3): exact for linear AC, no range needed.
3. **Live with in-context checks** for this pair. On this board it costs about 50 runs.

A datasheet has the same problem: an ADC datasheet says "drive with ≤ 50 Ω source impedance" and every applications engineer knows the real rule depends on frequency. Option A inherits that weakness and gives it a path (item 1), not a cure.

### 3.5 Cached child results

A child's results are cached per **definition**, never per placement (`contract_hierarchy.md` §4.3):

```
block key = hash( child definition after elaboration (params applied, part records, device models, D-A defaults)
                + its operating section (as physical knob ranges) + its conditions header + its fixtures
                + the measure definitions and analyses of its rows
                + engine settings + backend identity )
NOT in the key: the placement, the parent, the neighbours, the rows' limits and typ values
rows of the run table keyed by physical knob values (T6)
```

| Edit | What it invalidates |
|---|---|
| a row's **limit** or `typ` (`max 100Ω` → `max 80Ω`) | nothing. The measured values are in the run table; the verdict is recomputed with **0 runs** |
| a new row using an existing scenario | only the new measure's evaluation on existing runs (0 runs if its `Needs` are already served) |
| a new row with a new pin or excitation | the runs for that scenario only |
| narrowing an operating range | nothing: every kept row lies inside (T6's physical keys) |
| widening an operating range | the new corners only |
| an edit inside the child's body (a value, a part) | this child's cache; every parent's carried verdicts become `stale` until re-checked |
| a part-record or model change | every block that uses the part (P9: the part-data hash is in the key) |
| an edit in the parent or a sibling | nothing in this child; the parent re-runs its containment checks (microseconds) |

Two placements of one definition share one cache (the four-channel board, or H4's three stages: 0 new runs [H]). A per-placement override (`gain2 = GainStage { r_f: 20k … }`) is a new key for that placement only.

### 3.6 When flat simulation is still required

| Case | Why | On this board |
|---|---|---|
| **Board-only rows** | they're about something two blocks share: a node, a rail, a reference | `code_gain`, `midscale`, `ripple`, `mcu_step`, `droop`, `t_valid`, `usb_current`, `holdup`, `recover` |
| **A failed containment check without a table** | the child's rows don't apply here | `aa.drives_adc` (row 8) |
| **A FAIL found by composition** | composed bounds prove PASS only; one flat run at the corner is the counterexample | — |
| **Large-signal or nonlinear port behaviour** | clipping, slew, the ADC kick, start-up, faults | `drives_adc`, `t_valid`, every fault |
| **Coupling that isn't at a port** | self-heating, a shared ground return, magnetics | — (none modeled here) |
| **Regression of the cache** | a table can go stale in ways the key doesn't see | periodic in-context spot checks |

A block can force it: `#[in_context]` on a placement makes its rows always evaluated in the parent (for a block with a thermal neighbour).

---

## 4. What the engine does with it

### 4.1 Lowering: sections to engine structures

```
 contract text                          spicy_model (flatten)               spicy_engine
 ─────────────                          ─────────────────────               ────────────
 operating ──────────────────────────► KnobTable (Range, origin Env|Interface) ─► KnobSpace
 part values ────────────────────────► KnobTable (Statistical, origin Part) ───► KnobSpace
 faults ─────────────────────────────► KnobTable (Mode, event params) ──────────► KnobSpace
 conditions + fixtures ──────────────► Bench list (header, sheets) ───► M1f: one EngineDeck per bench
 specs rows ─────────────────────────► FlatContract rows with SpecPath ──► Plan: Scenario + Side + MeasureDef
 ratings (block + part records) ─────► automatic rows ───────────────────► Plan: Side (worst_case) per scenario
 placements ─────────────────────────► Containment checks ───────────────► Record.rests_on / fallback
```

**Which setups become which decks.** The rule: **one deck per distinct circuit**, and everything that only changes values, excitation, start or events is a *scenario* on the same deck.

| Written | Becomes | Why |
|---|---|---|
| the block's header (`conditions`, or the defaults) | **one deck** with every source and load slot: DC value params, one AC slot per excitation handle, PWL params per event, one gate per switchable fault | one knob map, one `prepare`; the worker picks the slot per request |
| `with x = v` / `with x in r` | a scenario on the header deck: pins and narrowed ranges | no new circuit |
| a disturbance handle in a measure (`vin.ripple`, `mcu.wake`) | the scenario's excitation or event | `alter @v_vin[acmag]=1`, or event params |
| a form fixture (`StartUp`, `ColdPlug`) | a scenario with `start: zero` (next_synthesis X1) | same netlist |
| a sheet fixture (`ScopeProbe`) | **a separate deck** | the circuit changes |
| `open` / `short` faults on existing branches | a gate in the header deck, off by default | a smooth switch conducts in normal runs |
| `swap` faults | **a separate deck** | re-routing can't be a gate |
| a raw SPICE fixture (§5.2) | **a separate deck**, DUT substituted | — |

**Which analyses.** A row's measure picks them: `dc(…)` → `Op`; `.sweep(k, r)` → `DcSweep` over a source or load knob; `ac(… / handle)` → `Ac { excitation }`; `tran(…).during/after(event)` → `Tran { events, window, start }`; `port.z_out` / `port.z_in` → `Ac { excitation: Inject(port) }` (a current injected at the port, the injection method of `contract_ee_practice.md` §1.5). `ac()` of a derived expression (the ratiometric `code`) is linearized at the operating point by the measure compiler.

**For the reference board:**

| Block | Decks | Scenarios (distinct pin/excitation/event sets) | Analyses |
|---|---|---|---|
| `Ldo3v3` | 1 | 8: default, line_reg pin, dropout pin, psrr pin + vin.ripple, load step, line step + pin, StartUp, z_out inject | Op, DcSweep ×3, Ac ×2, Tran ×3 |
| `VddaFilter` | 1 | 3 | Op, Ac ×2 |
| `MidRef` | 1 | 2 | Op, Ac |
| `GainStage` | 1 | 6: default, clip pin, stable step, vdd.ripple, z_in inject, z_out inject | Op, Ac ×3, Tran ×2 |
| `AaFilter` | 1 | 2 | Ac, Tran (sample clock) |
| `SensorBoard` | 3: header, `ScopeProbe`, `reversed` | 12 | Op, Ac ×2, Tran ×7 |

### 4.2 How knobs are affected

| Knob kind | Written in | Example on the board | Standalone (child) | In the parent |
|---|---|---|---|---|
| **Environment** | `operating { temp: … }` at every level | `temp` | a range knob | **the same knob**, shared by every block; the join key for cached rows [H §1.4] |
| **Per-placement part** | part values in bodies | `gain.r_f.value`, `ldo.u1.v_ref` | rows of the child's run table | picked independently per placement; tied only by a `lot` |
| **Interface** | `operating` port lines | `gain.output.r` (the SeriesRc r), `gain.vdd.v`, `ldo.vout.i` | range knobs of the child's header deck | **bound** to the neighbour: `gain.output.r` *is* `aa.r_aa.value`; `gain.vdd.v` is the LDO's output minus the filter drop. No longer free, so correlation is kept |
| **Top-level interface** | the top block's `operating` | `vbus.v`, `vbus.z`, `sensor.z` | — | stay range knobs: nobody above binds them |
| **Firmware / layout owned** | `#[owner(firmware)]` | `mcu.i_run` | a range knob | a range knob; the verdict says "given" |
| **Bench-local** | `conditions`, `with =`, fixtures | step 5→30 mA in 1 µs, `vin.v = 4.3V` in psrr | **constants** (fixed test values) or pins, never knobs | unchanged; they belong to the child's rows |
| **Measure axis** | `.sweep(vin.v, …)`, `.band(…)` | line regulation's VIN sweep | not a knob: an analysis variable | — |
| **Mode (union)** | `A | B` in `operating` | `gain.output.family ∈ {SeriesRc, Shunt}` | `KnobKind::Mode`, enumerated | bound to the neighbour's family (SeriesRc) |
| **Fault** | `faults { }` | `fault ∈ {off, cable_glitch, unplug}`, `cable_glitch.length` | — | the mode is fixed per scenario, never searched jointly; `length` is a range knob narrowed by the row |

**The rule behind the table:** operating lines on **ports** become interface knobs in the child and bindings in the parent; operating lines on the **environment** stay knobs all the way up (`contract_hierarchy.md` §4.5). Conditions and fixtures never create knobs unless written as ranges.

### 4.3 How many runs, roughly

Estimates from cone sizes and the per-analysis costs measured elsewhere (CE amp: 579 runs, 0.4 s; transients 10–30 ms each on this board). Not measured end to end.

| Scope | Rows | Largest cone | ≈ runs | Cost driver |
|---|---|---|---|---|
| `Ldo3v3` (cached) | 11 | 6 (temp, v_ref, c_in, c_out, vin.v, vout.i log) | ~600 | 3 transient scenarios × 64 corners |
| `VddaFilter`, `MidRef` (cached) | 3 + 3 | 4 | ~70 | — |
| `GainStage` (cached) | 9 | ~10 (c_in, r_b, r_f, r_g, c_g, v_os, r_m1, r_m2, input.z, load r/c/family) | ~1,300 | AC enumeration; the 1-D load search on `stable` |
| `AaFilter` (cached) | 3 | 3 | ~50 | the sample-clock transient |
| Board: carried child rows | 28 | — | **0** | containment only |
| Board: `aa` rows in context | 3 | 5 | ~50 (0 for `f_c`/`alias` once GainStage's table is cached) | — |
| Board: AC rows (`code_gain`, `ripple`) | 2 | 9 and 12 | ~600 + ~4,100 | `ripple`'s cone is at the 2^12 budget |
| Board: DC rows (`midscale`, `usb_current`) | 2 | 6 and 2 | ~70 | — |
| Board: short transients (`mcu_step`, `droop`, `holdup`, `probe_ok`) | 4 | 8–13 | 1,000–8,000 | `mcu_step`'s cone (≈ 13) is over budget: UNDECIDED (budget) in M3, the loop after |
| Board: long transients (`t_valid`, `inrush`, `recover`, `unplug`) | 4 | 6–9 | ~1,000, but 0.1–1 s each | simulated seconds; step profiles (E9) |
| Board: `reversed` | ratings | 2 | 4 | op only |

**About 10,000 runs for the whole board, minutes of serial time, dominated by the long transients.** The child blocks cost about 2,000 runs once, then nothing until they change. What datasheet style adds is *rows*; what costs is *scenarios* and *cones*. Rows that share a scenario share runs (the `Needs` union, engine_types §2.3), and the header makes that sharing visible: most rows use it.

### 4.4 What changes in `engine_types.md`

Most of it is the seams next_synthesis §8 already lists. Option A adds the scenario and the containment record.

```rust
// plan.rs
pub struct SpecPath { pub placement: Path, pub row: String }          // "gain" + "z_out" (M1, P1)
pub struct BenchId(u32);                                               // one per deck (T2, X2)
pub struct ScenarioId(u32);
pub struct Scenario {                                                  // one per distinct (bench, pins, excitation, events)
    pub bench: BenchId,
    pub pins: Vec<(KnobId, f64)>,                                      // `with x = v`
    pub narrows: Vec<(KnobId, f64, f64)>,                              // `with x in lo..=hi`
    pub excitation: Option<Excitation>,                                // `ac(… / vin.ripple)`, z_out injection
    pub events: Vec<Event>,                                            // disturbances and faults that fire
    pub start: Start,                                                  // Op | Zero (form fixtures)
}
pub enum Excitation { Slot(SlotId), Inject { port: Path } }
pub struct Event { pub handle: String, pub at: f64, pub length: EventLen, pub gate: GateId }
pub enum EventLen { Fixed(f64), Knob(KnobId), Forever }

pub struct Side {
    pub id: SideId, pub spec: SpecPath,                                // was a bare name
    pub sense: Sense, pub bound: Bound,                                // Bound::Const(f64) | Bound::Expr(Program) for relative rows
    pub measure: MeasureId, pub scenario: ScenarioId,
    pub confidence: Confidence,
    pub typ: Option<f64>,                                              // compared at the typical point, never a limit
    pub origin: SideOrigin,                                            // Row | Rating { part, pin, clause } | Equivalent { port }
}
pub struct Plan {
    pub benches: Vec<BenchNeeds>,                                      // Needs per bench, not one global Needs
    pub scenarios: Vec<Scenario>,
    pub sides: Vec<Side>, pub measures: Vec<MeasureDef>, pub cones: Vec<Cone>,
    pub typical: Vec<(ScenarioId, Point)>,                             // one typical run per scenario that has typ rows
}
pub struct BenchNeeds { pub bench: BenchId, pub probes: Vec<Probe>, pub devices: Vec<DeviceNeed>,
                        pub analyses: Vec<AnalysisSpec> }              // T1
pub enum AnalysisSpec {
    Op, DcSweep { knob: KnobId, from: f64, to: f64, points: u32 },
    Ac { excitation: Excitation, points: Vec<f64>, sweep: Option<Sweep> },
    Tran { stop: f64, profile: StepProfile, window: Window },          // E9
}

// knob.rs
pub enum KnobKind { Range, Statistical(Dist), Mode { levels: Vec<String> } }   // E5
pub enum Origin { Env { owner: Owner }, Part { placement: Path }, Interface { port: Path, field: String },
                  Fault { event: String } }                            // M2, contract_hierarchy §4.7
pub struct KnobSpec { /* as today */ pub origin: Origin, pub scale: Scale }    // M4: Linear | Log

// backend.rs
pub struct Request { pub point: Point, pub scenario: ScenarioId, pub analyses: Vec<AnalysisId>,
                     pub tolerance: Tolerance, pub care: ConeId }      // bench via scenario (T2, T6)

// hierarchy.rs (new, after M3)
pub struct Containment { pub at: (Path, Path), pub quantity: String,
                         pub guarantee: Interval, pub from_row: SpecPath,
                         pub requirement: Interval, pub from_operating: Span, pub ok: bool }
pub enum Evidence { Own, Carried { checks: Vec<ContainmentId>, child: BlockKey }, Composed { tables: Vec<BlockKey> }, InContext }

// report.rs
pub struct Record { /* as today */ pub spec: SpecPath, pub typ: Option<(f64, f64)>,   // (declared, measured at typical)
                    pub evidence: Evidence, pub rests_on: Vec<ContainmentId>, pub fault: Option<String> }
pub enum Measured { Value(f64), Undefined(&'static str), Beyond { side: Sense, limit: f64 } }   // E6
pub enum Verdict { Fail { at: RowId }, Pass(PassKind), Undecided(Reason), Unspecified(String) } // T5
// runs.rs: the run table keyed by (BlockKey, ScenarioId, cone key), stored under .spicy/blocks/<key>
```

### 4.5 Two rows, filled in

**Row 1: `Ldo3v3.psrr`** (child scope, cached under the LDO's block key).

```rust
// KnobSpace (block scope Ldo3v3)
k0  temp         Range            233.15 ..= 398.15 K     origin Env                              scale Linear
k1  vin.v        Range            3.6 ..= 5.5 V           origin Interface { port: vin,  field: "v" }
k2  vin.z        Range            0 ..= 0.5 Ω             origin Interface { port: vin,  field: "z" }
k3  vout.i       Range            1e-4 ..= 0.5 A          origin Interface { port: vout, field: "i" }  scale Log
k4  u1.v_ref     Statistical(TN3) 1.1905 ..= 1.2145 V     origin Part { placement: u1 }
k5  c_in.value   Statistical(TN3) 0.9 ..= 1.1 µF          origin Part { placement: c_in }
k6  c_out.value  Statistical(TN3) 0.9 ..= 1.1 µF          origin Part { placement: c_out }

// Scenario
s3 = Scenario { bench: b0 /* Ldo3v3 header deck */,
                pins: [(k1, 4.3), (k2, 0.0) /* header: z 0 Ω */, (k3, 0.05)],
                narrows: [], excitation: Some(Slot(vin.ripple)), events: [], start: Op }

// Measure and side
m3 = MeasureDef { name: "psrr", unit: dB, cone: c2 = [k0, k4, k5, k6],
                  program: Neg(Db(AcAt { num: Probe(v(vout)), den: Slot(vin.ripple), point: 0 })) }
Side { id: 7, spec: SpecPath { placement: "", row: "psrr" }, sense: Min, bound: Const(36.0),
       measure: m3, scenario: s3, confidence: Sigma(3.0), typ: Some(40.0), origin: Row }

// Needs of b0 (excerpt)
analyses[4] = Ac { excitation: Slot(vin.ripple), points: [1e5], sweep: None }

// One request, and what the worker sends
Request { point: ε(k0=−1, k4=+1, k5=−1, k6=+1), scenario: s3, analyses: [4], tolerance: Engine, care: c2 }
  alterparam k0=-40 k1=4.3 k2=0 k3=0.05 k4=1.2145 k5=9e-7 k6=1.1e-6
  alter @v_vin[acmag]=1 ; alter @i_vout[acmag]=0 ; reset ; ac lin 1 100k 100k
```

2^4 = 16 corners (the pinned knobs leave the cone) plus the typical run. [S] gives 40.0 dB at the nominal point.

**Row 2: `SensorBoard.holdup`** (board scope, flat deck, a fault).

```rust
// KnobSpace (board scope, knobs in this row's cone)
k1   vbus.v                Range            4.40 ..= 5.25 V     origin Interface { port: vbus, field: "v" }   // top level: stays free
k2   vbus.z                Range            0.1 ..= 0.5 Ω       origin Interface { port: vbus, field: "z" }
k3   mcu.i_run             Range            5 ..= 30 mA         origin Env { owner: Firmware }
k7   c_bus.value           Statistical(TN3) 4.23 ..= 5.17 µF    origin Part { placement: c_bus }
k8   ldo.c_in.value        Statistical(TN3) 0.9 ..= 1.1 µF      origin Part { placement: ldo.c_in }
k9   ldo.c_out.value       Statistical(TN3) 0.9 ..= 1.1 µF      origin Part { placement: ldo.c_out }
k10  c_dec.value           Statistical(TN3) 90 ..= 110 nF       origin Part { placement: c_dec }
k30  fault                 Mode { levels: [off, cable_glitch, unplug] }   origin Fault { event: "*" }
k31  cable_glitch.length   Range            20e-6 ..= 1e-3 s    origin Fault { event: "cable_glitch" }

// Scenario
s9 = Scenario { bench: b7 /* SensorBoard header deck, gate g0 on the VBUS pin */,
                pins: [(k30, cable_glitch)],
                narrows: [(k31, 20e-6, 200e-6)],                                  // `with cable_glitch.length in ..=200us`
                excitation: None,
                events: [Event { handle: "cable_glitch", at: 1e-3, length: Knob(k31), gate: g0 }],
                start: Op }

// Measure and side
m21 = MeasureDef { name: "holdup", unit: V, cone: c9 = [k1, k2, k3, k7, k8, k9, k10, k31],
                   program: Min(Window(Probe(v(v3)), During("cable_glitch"))) }
Side { id: 40, spec: SpecPath { placement: "", row: "holdup" }, sense: Min, bound: Const(3.0),
       measure: m21, scenario: s9, confidence: WorstCase, typ: None, origin: Row }
// plus, automatically, one ratings Side per rated pin in s9 (origin: Rating { … })

// Needs of b7 (excerpt)
analyses[6] = Tran { stop: 1.4e-3, profile: StepProfile { max_step: 1e-6 }, window: During("cable_glitch") }

// One request
Request { point: ε(k1=−1, k2=+1, k3=+1, k7..k10=−1, k31=+1), scenario: s9, analyses: [6], tolerance: Engine, care: c9 }
  alterparam k1=4.40 k2=0.5 k3=0.03 k7=4.23e-6 k8=9e-7 k9=9e-7 k10=9e-8 g0_at=1e-3 g0_len=2e-4 ; reset ; tran 1u 1.4m
```

2^8 = 256 corners. The worst corner found for this report is 3.225 V (low VBUS, long cable, 30 mA, caps −10%, 200 µs) **[A]**. `length` is on the passing side's edge here (the rail only falls with time), so the engine's monotonicity tag can later prune it.

---

## 5. Escape hatches

### 5.1 One-offs

Three sizes, all checked:

| Need | How | Checked how |
|---|---|---|
| a row at one odd condition | `with sensor.z = 47kΩ` on the row | the pin must lie inside `operating`, or the row gets `#[outside(operating, reason = "…")]` and a tag; G4 stays enforced |
| a small circuit change | `#[oneoff] fixture ScopeProbe for SensorBoard { … }` (§2.8) | a sheet fixture like any other; `#[oneoff]` only folds it in the editor and marks it in the report |
| a what-if that isn't a spec | `spicy check board.spl --with 'sensor.z = 47kΩ'` or the agent's `sim --at` | not stored in the file; results are `simulated`, never a verdict about the design |

A one-off row isn't weaker than a normal row. It's a normal row that the editor groups apart so the main table stays a datasheet.

### 5.2 A raw SPICE fixture

For a vendor's test bench, or one imported from LTspice:

```rust
/// TI's PSRR test bench for the TLV755P, used as is.
fixture VendorPsrr for Ldo3v3 = spice("benches/tlv755p_psrr.cir") {
    dut: "X1",                                          // the subcircuit call replaced by our Ldo3v3
    ports: { vin: "IN", vout: "OUT", gnd: "0" },
    params: { "VIN_DC": vin.v, "ILOAD": vout.i },       // deck params bound to our knobs
    excite: { vin.ripple: "VIN" },                       // which source carries the AC slot
}
psrr_ti: -ac(vout.v / vin.ripple).at(100kHz).db()   min 36dB   on VendorPsrr;
```

How it stays checked:
1. **The DUT is ours.** The `X1` line is replaced by the exported `Ldo3v3` subcircuit, so our parts, our knobs and our read-backs are used. A fixture that doesn't contain the named DUT is an error.
2. **Ports are bound by node name.** A missing node is an error at `prepare`, on the nominal run (T4).
3. **The bench is compared with `operating`.** The parser (spicy_parser already reads SPICE) finds the elements on DUT ports. Recognized ones (V, I, R, C, a PWL) are checked against the operating ranges: `VIN DC 4.3` ∈ 3.6–5.5 V. Anything it can't interpret (a B-source, a vendor macromodel) tags the rows `fixture-unchecked`, and their PASS reads "PASS (raw fixture)".
4. **No analyses or `.meas` from the deck.** The worker sends analyses per request (engine_plan §1.4), and measures are written in the row. A deck with `.control`, analyses or `.meas` is rejected with a fix-it. This keeps "never `meas`" (plan §4.3) and makes the measure visible in the table.
5. **The deck text is hashed into the block key** (and `rev`), so an edit to the file makes verdicts `stale`.

### 5.3 How escape hatches stay honest

Every escape hatch carries its tag into the report and the JSON: `oneoff`, `outside-operating`, `fixture-unchecked`, `raw-fixture`. A PASS resting on one of them says so in its `claim` sentence, and the agent may not drop the qualifier (engine_plan §3.5, §7.5).

---

## 6. Gap coverage (G1–G19)

**Improves** = this option makes it better than the tools in `contract_tool_gaps.md`. **Path** = it leaves room, nothing built. **Doesn't** = not this layer's job, or not solved.

| Gap | Option A | How |
|---|---|---|
| G1 no spec home | **Improves** | rows in the block's file, in the one format every EE reads; the table is a view |
| G2 limit stored twice | **Improves** | one row home; the block's printed datasheet is *generated* from the rows, so the published and verified numbers can't drift. `#[req("SYS-12")]` for external requirements (P1) is a path |
| G3 benches don't travel | **Improves** | the header fixture is derived from `operating`, so it follows edits; handles remove most fixtures. Interface-typed std fixtures reused across blocks (`std::fixtures::LoadStep for Power<Out>`) are a path |
| G4 bench drift from assumptions | **Improves** | every `with`, fixture and raw deck is checked against `operating`; a row's condition outside the operating range is an error unless waived |
| G5 corners by hand | Improves (engine) | conditions never list corners; the engine enumerates the box |
| G6 worst case is a guess | Improves (engine) | not this layer's work. What A adds: `typ` can never become a limit, and `#[search(…)]` marks interior worst cases |
| G7 unchecked distributions | Path | per-row provenance, like datasheet footnotes ("guaranteed by design", "by characterization") can feed `KnobSpec` provenance; typ-only data has no limit |
| G8 no correlation / lots | Improves (engine) | the environment is one knob at every level, joined; interface knobs are bound, not free. Lots are M6 |
| G9 MC cost | Doesn't (engine) | — |
| G10 sub-blocks trusted | **Improves** | containment in both directions at every port, plus "a row applies only inside its conditions"; failures fall back to tables or in-context runs, and the report shows both |
| G11 behavioural models drift | Path | a behavioural block that claims a contract is checked with the *same rows*; `equivalent` models are checked rows. "This model implements that block" needs P4 |
| G12 IP handoff is paper | **Improves** | the contract *is* a checkable datasheet; a block ships with its rows, verdicts and cache. Encrypted bodies are a path (P5) |
| G13 traceability | Partly | `SpecPath` identity, `#[req]`, and row-level diffs; "why did this verdict move" needs the engine |
| G14 no "not tested" | **Improves** | every row has a verdict or UNSPECIFIED; rows narrowed with `with … in` show which part of the operating range is covered, which gives a coverage map per row for free |
| G15 vendor models | **Improves** | a vendor part's own datasheet, written as rows, becomes its model's qualification bench: simulate the vendor model against its datasheet's min/max *and* typ. TI's "trust but verify" becomes a check |
| G16 datasheet data unreliable | **Improves** | the row format mirrors the datasheet table column for column, including test conditions and the typ/limit split: the structured-extraction case (97.5% vs 14.9% [contract_tool_gaps §1.12]) |
| G17 no diffs / CI | **Improves** | one row per line; the formatter does **not** align columns in text (the editor does), so an edit touches one line; `spicy check --base` (P11) |
| G18 AI checks single points | **Improves** | rows are small, uniform records an agent can read and write; every row gets an engine verdict |
| G19 faults hand-wired | Improves / path | faults are declared, scoped rows use `.during/.after`, ratings run in every fault, an FMEA table is emitted. Deriving the fault list from connector structure, and triggers on conditions, are paths (P15) |

**Where Option A is weak:**
- **Frequency-dependent ports** (§3.4): scalar operating ranges are pessimistic; the fix needs published port models or tables.
- **Board-level rows with several stimuli** fit the table less well. `ripple` needs an expression in its measure column; a row with a sensor tone *and* a droop *and* a load step would be long.
- **Faults are the least datasheet-like part.** Datasheets list protections, not scenarios; the `faults` section is our invention in datasheet clothing.
- **The fixed-test-point habit.** Copying a datasheet pins everything (`with vout.i = 1mA`), which checks a point, not the product. The default "over the operating range" protects against it, and a lint flags a pin on a quantity the row's measure is sensitive to ("psrr is pinned at 50 mA; the rail really sees 5–31 mA").

---

## 7. Strengths and weaknesses

### 7.1 By situation

| Situation | How Option A does | Why |
|---|---|---|
| **Big boards** | good for reuse, heavy for board rows | each child is a datasheet the board checks against, so 28 of this board's rows carry over with 0 runs; but board-only rows (shared rails, faults) are flat, and transients dominate the cost (§4.3) |
| **Quick experiments** | fair | a new row is one line; `--with` gives what-ifs without editing; but sections add ceremony to a scratch file |
| **Library IP** | **excellent** | this is what datasheets are for: ratings, operating conditions, characteristics, equivalent circuits; a vendor can ship a block that a customer's parent checks against automatically |
| **AI editing** | **excellent** | the agent has read countless datasheets; rows are uniform, one per line, with named columns; the engine's verdicts are per row |
| **A novice user** | good | the form is familiar from every part they've used; the lints teach the physics (unloaded outputs, pins that hide ranges); the risk is copying fixed test points |
| **The editor's form UI** | **excellent** | the text *is* the table: spec table (Name · Parameter · Measure · Conditions · Min · Typ · Max · Setup · Confidence · Verdict · Value), operating table, ratings table, faults table with the FMEA view |
| **Diffs** | good | one row per line, no alignment in text; a limit change is a one-token diff and costs 0 runs |
| **Multi-stimulus board specs** | weak | the measure column becomes code; the table form strains |
| **Frequency-dependent interfaces** | weak | scalar ranges are pessimistic (§3.4) until port models or tables are used |

### 7.2 Where it shines

- **Composition feels like normal engineering.** "Is my load inside the part's recommended conditions?" is a question every EE asks daily. The engine now asks it at every port, both ways, with numbers.
- **The typ column is honest.** It records the nominal claim, gets compared with the model, and can never pass a spec. That turns a model/datasheet disagreement into a visible lint (G15).
- **Specs are central, literally.** Operating conditions, headers and fixtures all exist to qualify rows, and the file reads in that order.
- **Setups are first-class without being everywhere.** The header is one setup shared by most rows, as in real datasheets (about 4–13 setups per datasheet, most rows on the header [S]); named fixtures appear only when the circuit changes.
- **Published and verified numbers are one text.** Print the contract and you have the block's datasheet.

### 7.3 Where it hurts

- **Two spellings of a limit** (`in a..=b` and `min a max b`). The formatter keeps whichever was written; a linter could pick one.
- **More sections than v0.1.** Small blocks pay a few braces.
- **Faults and events are grafted on.** They work, but the datasheet metaphor offers little guidance for them.
- **Interface ranges need care.** A range too tight fails containment (row 8); a range too loose fails the child's own check (§3.4). The engine can suggest a range from the neighbour's actual values ("gain gives 0.3–73.5 Ω; widen to ..=80Ω?"), but the author still has to understand the divider.
- **Rows multiply.** A thorough datasheet has 30–60 rows. Each is cheap to write and to diff, but the run count follows the number of distinct scenarios, and a careless `with` makes a new one.

---

## 8. Grammar sketch

Additions to `grammar.md` §3. Section names and row words are **contextual** (recognized only in their positions), so no new reserved words are needed; `assume` and `spec` leave the MVP keyword list after migration.

```ebnf
contract      = "contract" IDENT "{" { section } "}" ;
section       = { DOC } { attribute }
                ( "ratings"    "{" { DOC* target ":" expr ";" } "}"
                | "operating"  "{" { DOC* attribute* target ":" expr ";" } "}"      (* expr: range | family literal | path | expr "|" expr *)
                | "conditions" "{" { cond_stmt } "}"
                | "specs"      "{" { DOC* attribute* ( let | row ) } "}"
                | "equivalent" "{" { DOC* IDENT ":" expr ";" } "}"
                | "faults"     "{" { DOC* IDENT ":" fault ";" } "}"
                | fixture ) ;
cond_stmt     = target ":" expr ";"
              | "typical" ":" pins ";"
              | "confidence" ":" expr ";"
              | "set" IDENT "=" "{" pins "}" ";" ;
row           = IDENT ":" expr limits [ "with" conds ] [ "on" path ] ";"
              | IDENT ":" path ";" ;                                       (* re-publish a child's row: alias: aa.alias; *)
limits        = "in" range [ "typ" sum ]
              | [ "min" sum ] [ "typ" sum ] [ "max" sum ] ;               (* at least one; typ alone = informative row *)
conds         = cond { "," cond } ;
cond          = target ( "=" expr | "in" range ) | IDENT ;                 (* IDENT: a named set *)
pins          = target "=" expr { "," target "=" expr } ;
fault         = fault_op [ "at" sum ] [ "for" range ] [ "when" relation ] ;
fault_op      = ( "open" | "short" | "swap" | "inject" ) "(" list ")" ;
fixture       = "fixture" IDENT [ "for" path ]
                ( "{" { fixture_stmt } "}"                                 (* form: `field: value` overrides; sheet: `let` placements *)
                | "=" "spice" "(" STRING ")" "{" fields "}" ) ;
target        = path { "." IDENT } ;                                       (* vin.v, vout.i, cable_glitch.length *)
```

---

## 9. Open questions for the comparison

1. **Section order and names.** `specs` or `characteristics`? Keep `equivalent` separate, or make port models rows?
2. **Relative rows** (`min vin.v - 12mV`): enough for pass-through blocks, or do they need a general transfer form?
3. **Datasheet "or" clauses in ratings** (voltage *or* current limited): part-record syntax, or rows with `|`?
4. **Printing a real datasheet** from the contract: which rows are public, and who owns the typ column when the model and the part disagree?
5. **Port-model families** for frequency-dependent ports (§3.4): a std set (`OpampOut`, `LdoOut`, `SeriesRc`), or fitted tables from the child's cache?
6. **Fault-list generation** from connector structure (P15 item 2): declared by the author, proposed by the agent, or derived by the engine?
