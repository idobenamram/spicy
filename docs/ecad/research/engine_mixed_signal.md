# Stress Test: Mixed-Signal, Digital Interfaces, Timing and Multi-Stable Circuits

> Research note · 2026-09-25 · companion to `../archive/engine_v2.md`, `../walkthrough.md`, `../specs.md`
> Reference keys in **[brackets]** are listed in §8. Keys already in `../bibliography.md` (e.g. [AGW94], [N&P07], [Galán99]) are used as-is.

**What this is.** The v2 engine was designed around, and demonstrated on, a single-transistor amplifier: continuous knobs, one DC operating point, smooth measures. This note pushes it against the circuits around a typical microcontroller: supervisors, sequencing, I2C, SPI, level shifters, GPIO loads, crystals, comparators, timers, latches, watchdogs and ADC inputs. For each circuit it lists what engineers actually spec, which knobs matter, how the engine would handle each spec, and where the engine breaks.

**Where the numbers come from.**
- Datasheet values were read from the vendor PDFs listed in §8.3, at the revisions given there.
- Every derived number was computed with short scripts: corner enumeration, a Newton solver that copies `spicy_simulate`'s DC algorithm, a backward-Euler transient, a Pierce small-signal model, and a square-law MOSFET model. The formulas are given inline so each number can be re-derived.
- Numbers marked *illustrative* are modelling assumptions, not datasheet values.

---

## 0. Top findings, ranked by impact

1. **Most board-level digital specs need no simulation at all. They are arithmetic on datasheet tables.**
   - Examples: logic-level margins, pull-up windows, rise time into a lumped C, SPI/I2C timing budgets, supervisor threshold windows, watchdog windows, crystal gm_crit, ADC source-impedance limits, pin current limits.
   - Almost every one of these formulas is monotone in each knob. So the corner/direction machinery of v2 §A5 gives **exact** answers in about two evaluations.
   - This is the largest and cheapest automatic-spec opportunity. Nobody in the open tools does it well:
     - atopile's logic signal type has no voltage thresholds.
     - PolymorphicBlocks uses plain intervals and loses correlations (§3.1).
     - KiCad and Altium ERC only check pin *types*.
2. **Knobs are not only continuous boxes.** Digital boards add four structures the engine must represent:
   - **Discrete modes:** power state, logic state, which device drives a bus, firmware configuration, IBIS corner.
   - **Constrained datasheet boxes:** e.g. VT+ − VT− ≥ ΔVT(min). Without the constraint, a 555's datasheet threshold boxes predict a period anywhere in 0.11×–2.17× nominal.
   - **Table-indexed parameters:** limits that change with the VCC band, the temperature column or the test current, with gaps where nothing is specified.
   - **Provenance:** tested, by design, typical-only, or missing.
3. **Multi-stable circuits break the "one DC solution" assumption, and our simulator falls into the classic trap.** On a cross-coupled latch, a Newton solver that copies `spicy_simulate` (zero initial guess, ±40·Vt clamp, 50 iterations):
   - fails to converge;
   - with more iterations, or with SPICE-style `pnjlim` limiting, converges to the **unstable metastable point**. That happens even with a 1 % mismatch.
   - A worst-point loop would linearize around a state that never physically exists.
4. **Which state is real depends on the circuit's history,** so specs on multi-stable blocks must declare a history (a setup) and use a smooth margin in place of the discrete outcome. Example: a 22 pF "power-on bias" capacitor on the latch
   - overcomes a 20 % adverse mismatch with a 1 µs supply ramp;
   - fails at 5 % mismatch with a 1 ms ramp.
   The supply ramp time is a range knob that decides the outcome.
5. **Backward Euler with a large step makes unstable equilibria look stable.** When h·λ > 2, a slowly ramped latch sits on the metastable branch. With h = 2 ns it resolves correctly. The fixed-step transient in `trans.rs` has exactly this hazard.
6. **Thresholds of hysteretic circuits are fold points, not DC values.** Our DC sweep follows one branch from the previous solution and **panics** at non-convergence (`dc.rs`, `.expect`). It cannot find thresholds. Two fixes:
   - arc-length continuation with fold detection;
   - for piecewise-linear models, *mode-conditioned linear analysis*, which is exact by corners.
7. **Crystal start-up is a linear AC measure, but it is not monotone.**
   - The Pierce negative resistance reproduces AN2867's gm_crit to 1.3 %.
   - It peaks at a finite gm: the most negative resistance achievable is −1021 Ω for an 8 MHz, CL 18 pF crystal. So a "safety factor 5" is impossible at ESR 200 Ω *regardless of gm*.
   - Corner checks are not enough here. Transient start-up simulation (10³–10⁵ cycles at Q ≈ 10⁴) is the wrong tool.
8. **Timing specs are threshold crossings.** Four things make them safe:
   - smooth sensitivities (dt*/dp = −(∂v/∂p)/v̇ [Galán99]);
   - event location with interpolation (our transient steps are fixed);
   - an input-slew automatic spec (74HC: 139 ns/V max at 4.5 V; 74LVC: 10 ns/V);
   - a separate monotonic-edge spec.
9. **Signal integrity without layout means conditional verdicts.** A schematic tool can bound static levels, lumped rise times and timing budgets for an *assumed* length, Z0 and bus capacitance. The honest output is "PASS given Cb ≤ 234 pF", plus exported layout constraints. SI is not the bottleneck for the example SPI pair: the MSPM0 controller's 29 ns input setup caps it at **13.8 MHz**, or 23.5 MHz with delayed sampling (then hold-limited).
10. **Some datasheet numbers the engine needs are typical-only or missing.** Examples:
    - MSPM0 pull-up "40 kΩ typ";
    - oscillation allowance "2 kΩ typ";
    - TLV755P start-up "550 µs typ" with no max;
    - LED Vf with no min;
    - BSS138 Vth only at 25 °C, and no Rds(on) below VGS = 2.5 V;
    - TI's current NE555 datasheet *deleted* its initial timing-error spec (rev K, 2026).

    The verdict needs a state for this: "UNSPECIFIED / relies on typ".

---

## 1. What board-level digital circuits do that the amplifier walkthrough never exercised

| Property | Amplifier walkthrough | MCU board circuits |
|---|---|---|
| Where the number comes from | Simulation (MNA) | Mostly datasheet tables plus simple formulas; simulation for a minority |
| Knobs | Continuous boxes: range + statistical | Also discrete modes, constrained boxes, table-indexed limits, typ-only values, layout and firmware assumptions |
| Operating point | Unique | Often several (latches, Schmitt, hysteresis, power latches, self-biased references) |
| Time behaviour | Smooth steady state / AC | Events: threshold crossings, resets, sequencing order, oscillator start, watchdog windows |
| Correlation | Shared temperature/supply knobs | Also shared **rails** (every pin on a rail sees the same VDD), IBIS min/max columns that move all of a chip's buffers together, and matched internal dividers (555 ladder) |
| Verdict | PASS/FAIL/UNDECIDED | Also "conditional on layout", "outside guaranteed region", "depends on history" |

---

## 2. Baseline: what `spicy_simulate` can and can't do today

Read from the source on branch `parametric_engine`:

| Area | Today | Why it matters here |
|---|---|---|
| Devices | R, C, L, V/I sources (DC, PULSE, SIN, EXP), diode, BJT (Ebers–Moll). Parser has subcircuits. | No MOSFET (level shifters, load switches, power latches); no switches, controlled or behavioral sources (comparators, logic, 555); no PWL (supply ramps) |
| DC | Plain Newton from a zero guess; junction voltage clamped to ±40·Vt (`bjt.rs`, "very bad limiting"); abs 1e-6 / rel 1e-3; 50 iterations | No gmin stepping, source stepping, `.nodeset` or `.ic`; picks an arbitrary solution of a multi-stable circuit, or none |
| DC sweep | Natural continuation from the previous point; `.expect(...)` **panics** on non-convergence | Cannot pass a fold, so it cannot trace hysteresis and crashes at the threshold |
| Transient | Fixed step, BE or trapezoidal; no breakpoints, events or LTE control; UIC is `unimplemented!` | Crossing times are quantized to the step; large BE steps make unstable equilibria look stable (§4.3); cannot start from a chosen state |
| Digital | None | No event-driven layer, no A/D bridges, no IBIS |
| AC | R, C, L and sources only (v2 Part D) | Crystal start-up is an AC measure at the operating point, so it needs linearized active devices |

### 2.1 The latch experiment

**Setup.** A symmetric two-NPN latch (VCC 5 V, RC = 1 kΩ, cross-coupled RB = 10 kΩ, IS = 1e-16, BF = 100). The solver copies `spicy_simulate`'s Newton scheme and BJT stamp.

**It has three DC solutions.** Stability is from the eigenvalues of −C⁻¹J, with Cπ = gm·0.3 ns + 2 pF, Cμ = 1 pF and 1 pF per node:

| Solution (V_c1, V_c2, V_b1, V_b2) | max Re(λ) | Status |
|---|---|---|
| 4.620, 0.056, 0.056, 0.819 | −2.5·10⁷ s⁻¹ | stable (Q2 on) |
| 0.056, 4.620, 0.819, 0.056 | −2.5·10⁷ s⁻¹ | stable (Q1 on) |
| 1.186, 1.186, 0.808, 0.808 | **+6.2·10⁷ s⁻¹** | **unstable (metastable)** |

**What the solver returns:**

| Solver | Symmetric | RC2 +1 % | RC2 −1 % |
|---|---|---|---|
| spicy settings (50 iterations) | no convergence | no convergence | no convergence |
| same, 200 iterations | **metastable point** (166 iterations) | **metastable point** (1.182, 1.187, …) | — |
| + SPICE3 `pnjlim` | **metastable point** (10 iterations) | **metastable point** | **metastable point** |

Better convergence aids make this *worse*: the solver then reliably returns the physically impossible point. ngspice's manual says the same thing: regenerative circuits "probably will not converge … unless the OFF option is used … or .nodeset" [ngspice47]. A tolerance engine that linearizes wherever DC lands would compute confident sensitivities around a state no board ever occupies.

---

## 3. Circuit by circuit

Each subsection covers four things: **Specs** (with numbers), **Knobs**, **Engine walkthrough**, and **Breaks** (where it's weak). Methods and fixes are collected in §5–§6.

### 3.1 Logic-level compatibility across families and supplies

**Specs**

| Check | Formula (worst case over knobs) | Typical requirement |
|---|---|---|
| High level | VOH,min(I_load, VDD_tx) − VIH,min(VDD_rx) ≥ margin | ≥ 0 (guaranteed), often ≥ 0.1·VDD noise margin |
| Low level | VIL,max(VDD_rx) − VOL,max(I_load, VDD_tx) ≥ margin | same |
| Input abs-max | V_out,max ≤ V_I,absmax(rx) | often VDD_rx + 0.3 V unless the pin is "tolerant" |
| Powered-off receiver | V_pin ≤ limit when VDD_rx = 0; I_OFF / diode current | e.g. MSPM0 diode current ±2 mA; 74LVC I_OFF ±2 µA at 5.5 V |
| Input slew | edge rate ≥ datasheet Δt/ΔV limit (unless Schmitt) | 74LVC: 10 ns/V (VCC 2.7–5.5 V); 74HC: 139 ns/V at 4.5 V |
| Drive current | I_load ≤ the test current of the VOH/VOL row being used | beyond it the row doesn't apply |

**Real datasheet values**

| Part (source) | Thresholds |
|---|---|
| 74LVC1G04 (−40…85 °C) | VIH 2.0 V / VIL 0.8 V at VCC 2.7–3.6 V; 0.65/0.35·VCC at 1.65–1.95 V; 1.7/0.7 V at 2.3–2.7 V; **nothing specified for 1.95–2.3 V**. VOH ≥ 2.3 V at −24 mA / 3.0 V (**2.0 V** in the −40…125 °C column) |
| 74HC04 | VIH ≥ 3.15 V, VIL ≤ 1.35 V at 4.5 V; VOH ≥ 3.84 V at −4 mA (3.7 V at 125 °C) |
| 74HCT04 | VIH ≥ 2.0 V, VIL ≤ 0.8 V (4.5–5.5 V) |
| MSPM0G3507 SDIO | VIH 0.7·VDD, VIL 0.3·VDD, hysteresis ≥ 0.1·VDD; VOH ≥ VDD − 0.45 V at 6 mA (VDD ≥ 2.7 V, −40…125 °C); abs-max V_I ≤ VDD + 0.3 V (4.1 V max) on common pins, 5.5 V on ODIO pins |

**Worked checks** (MCU rail 3.3 V ±3 % = 3.201…3.399 V; 5 V rail ±5 %):
- MCU → 74HC04 at 5 V: 3.201 − 0.45 − 0.7·5.25 = **−0.92 V → FAIL**.
- MCU → 74HCT04: 3.201 − 0.45 − 2.0 = **+0.75 V → PASS**.
- 74HC04 at 5.25 V → MCU common pin: 5.25 V > VDD + 0.3 V → **abs-max FAIL**, unless it lands on an ODIO (5 V-tolerant) pin. The verdict depends on *which pin*.

**Knobs**
- Each rail's voltage is a **shared** knob: every pin on the rail sees the same value, and it comes from the power tree's affine form.
- The temperature range **selects the table column**; it isn't just a knob value.
- The load current comes from the rest of the netlist.
- Datasheet limits are **range** knobs. They are guaranteed bounds with no distribution attached.

**Engine walkthrough.** Every quantity is linear or monotone in each knob, so the direction proof (v2 §A5) is trivial and two corners give the exact range. The important part is keeping the rails as *named shared knobs*. The same-rail case shows why:

| Rail range | Interval check VOH,min − VIH,max (what an interval tool computes) | Shared-knob check min over VDD of (0.3·VDD − 0.45) |
|---|---|---|
| 3.0–3.6 V | +0.03 V | **+0.45 V** |
| 2.7–3.6 V | −0.27 V (false FAIL) | **+0.36 V** |
| 1.62–3.6 V | −1.35 V (false FAIL) | **+0.036 V** |

PolymorphicBlocks computes `input_thresholds = link.voltage × factor` and `output_thresholds = (gnd.upper, vdd.lower)` as independent intervals (`edg/electronics_interfaces/DigitalPorts.py`). That is exactly the v2 §A2 dependency problem, in its most common real-world form.

**Breaks**
- *Table semantics*, not the math:
  - VCC bands with gaps (1.95–2.3 V above);
  - temperature columns;
  - VOH given only at discrete test currents.

  VOH at another current is not guaranteed by interpolation. The only safe reading is "at most I_test, at least VOH_test", assuming a monotone driver.
- A rail range that straddles a band edge must be **split** at the edge. A range that touches a gap must report **UNSPECIFIED**, not PASS/FAIL.

### 3.2 A GPIO driving an LED or a MOSFET gate

**Specs**

| Spec | Numbers |
|---|---|
| LED current in window | I_min for visibility (e.g. ≥ 2 mA) ≤ I_LED ≤ min(pin rating, row test current) |
| Pin current abs-max | MSPM0: SDIO/HSIO **6 mA**, HDIO 20 mA; total VDD/VSS 80 mA (Tj ≤ 130 °C) |
| MOSFET enhancement | VOH,min ≥ the VGS at which Rds(on) is guaranteed (e.g. 2.5 V or 4.5 V row) |
| MOSFET dissipation | I²·Rds(on)(Tj) within derating |
| Load state during reset/boot | load OFF while the pin is Hi-Z, weakly pulled, or a boot strap |

**LED worked example** (MSPM0 SDIO; red Kingbright APT1608EC: Vf 2.0 typ / 2.5 max at 20 mA, −1.9 mV/°C, **no Vf min given**). The GPIO output resistance is taken from the table as ≤ 0.45 V / 6 mA = 75 Ω, a conservative linearization.

| R | I_LED range | Verdict |
|---|---|---|
| 100 Ω ±5 % | 3.2 … **16.8 mA** | **FAIL:** exceeds SDIO abs-max 6 mA and leaves the VOH row's 6 mA test region. Needs an HDIO pin with DRV=1, or a transistor |
| 330 Ω ±5 % | 1.37 … 5.10 mA | Passes the pin limits; may fail a brightness spec |

- The upper end assumes **Vf,min = 1.8 V**. That is an assumption, because the datasheet has no min.
- A blue APT1608QBC-D (Vf 3.3 typ / 4.0 max) on a 3.2 V rail has *negative* worst-case headroom. There is no guaranteed current at all.

**Knobs:** VDD (shared), R (statistical), LED Vf (range, partly missing), Vf tempco × temperature, GPIO drive row (a **firmware knob**: MSPM0 DRV=0/1 changes both the VOH row and the current rating).

**Engine walkthrough**
- The LED circuit is a 1-D nonlinear equation in the loop current, monotone in every knob, so it's exact at two corners.
- The GPIO model is the issue. There are three options, in increasing fidelity:
  1. linearized R_out from one table row;
  2. a piecewise envelope over all rows;
  3. IBIS [Pullup]/[Pulldown] I–V curves.
- For the MOSFET gate, the static check is arithmetic. Switching time is roughly Qg / I_drive, and needs the drive model.

**Breaks**
- *The operating point leaves the region the datasheet guarantees:* current above the test current, VGS below the lowest Rds(on) row. The engine must say so explicitly, rather than extrapolating a model.
- *Reset/boot state* is a discrete mode: Hi-Z, pull enabled or not, boot-strap semantics. Example: the MSPM0 BSL_invoke pin is sampled at boot. The check "the load is OFF in every pre-firmware state" is a mode enumeration, not a linearization.

### 3.3 The I2C bus

**Specs** [UM10204 Rev 7.0, Tables 10–11 and §7.1]:

| Parameter | Standard (100 kHz) | Fast (400 kHz) | Fast-mode Plus (1 MHz) |
|---|---|---|---|
| tr max (0.3→0.7·VDD) | 1000 ns | 300 ns | 120 ns |
| Cb max | 400 pF | 400 pF | 550 pF |
| VOL max | 0.4 V at 3 mA | 0.4 V at 3 mA | 0.4 V at 20 mA |
| VIL / VIH | 0.3 / 0.7·VDD | same | same |
| tLOW / tHIGH min | 4.7 / 4.0 µs | 1.3 / 0.6 µs | 0.5 / 0.26 µs |
| tSU;DAT min | 250 ns | 100 ns | 50 ns |
| Noise margin (low / high) | 0.1 / 0.2·VDD | same | same |

- Pull-up window: Rp,min = (VDD − VOL,max)/IOL and Rp,max = tr / (0.8473·Cb), where 0.8473 = ln(0.7/0.3).
- Fast mode also has a **minimum** fall time, 20·(VDD/5.5 V) ns. So the lower bound of Cb matters too.

**Worked numbers** (VDD 3.3 V ±5 %, Fast mode):

| Cb | Rp window | |
|---|---|---|
| 80 pF | 1022 … 4426 Ω | |
| 120 pF | 1022 … 2951 Ω | |
| 200 pF | 1022 … 1770 Ω | |

| Pull-up | tr at Cb = 200 pF | Max Cb for 300 ns |
|---|---|---|
| 1.5 kΩ ±1 % | 257 ns ✓ | 234 pF |
| 2.2 kΩ ±1 % | **377 ns ✗** (counterexample: Cb = 200 pF, R +1 %) | 159 pF |
| 4.7 kΩ ±5 % (the "default") | **836 ns ✗** | **72 pF** |

- **Hidden pull-ups:** a board 1.5 kΩ in parallel with three plug-in modules that each carry 4.7 kΩ gives Rp,min = 743 Ω. The sink current is then **4.12 mA > 3 mA → FAIL**. The VOL spec is no longer guaranteed.
- **The UM10204 timing table is itself a budget:**
  - Standard mode: tVD;DAT,max 3.45 + tSU;DAT 0.25 + tr 1.0 = 4.70 µs = tLOW,min.
  - Fast mode: 0.9 + 0.1 + 0.3 = 1.3 µs = tLOW,min.

  So with the actual tr and the controller's actual tLOW setting, the engine can re-derive functional slack. It can then separate "violates the standard's tr limit" from "actually fails".
- **Effective clock:** a controller that counts tHIGH from when it senses SCL high adds tr to the period. A 400 kHz setting with tr = 300 ns runs at **357 kHz**.

**Knobs**
- VDD: shared, and it is the *receivers'* reference, because VIH = 0.7·VDD of each receiver.
- **Cb:** a layout assumption. Pins ≤ 10 pF each by UM10204; trace roughly 0.8–1.3 pF/cm, from t_pd/Z0 on FR-4; plus connectors and cables.
- Rp tolerance; the number of pull-up sets; the controller's timing registers (firmware).

**Engine walkthrough**
- With every driver released, the bus is a **linear first-order RC**, and tr = 0.8473·Rp·Cb exactly.
- Monotone in every knob, so **PASS (guaranteed)** is possible, *conditional on the Cb assumption*.
- The engine can also run the constraint backwards and emit **"layout constraint: Cb ≤ 234 pF on SDA/SCL"**.

**Breaks**
- Cb is unknown until layout.
- Mixed-VDD buses need a level shifter (§3.4).
- Clock stretching.
- Buffers and multiplexers split the bus into segments, each with its own Cb and pull-ups.
- Neither atopile nor PolymorphicBlocks checks rise time against Cb and speed mode (§5.5).

### 3.4 Level shifters

**MOSFET bidirectional shifter** [AN10441]: gate at VDD1, source on the low side, drain on the high side. It needs VDD2 ≥ VDD1. When the high side pulls low, the low side is first pulled down through the drain–substrate (body) diode.

**Specs**
- The MOSFET must turn on when the low side is driven low: VDD1,min − VOL,1 > Vth,max(T_min) with margin.
- VOL on the far side at the **combined** sink current.
- tr on each side; I2C limits apply on both sides.
- Behaviour with either rail off.

**Worked numbers.**
- Device: BSS138BK. Vth 0.48 / 1.1 / 1.6 V (25 °C, 250 µA only); Rds(on) ≤ 6.5 Ω at VGS 2.5 V (25 °C); nothing specified below VGS 2.5 V.
- Model: square law with K calibrated so that Rds(2.5 V, Vth = 1.6 V) = 6.5 Ω. Vth tempco −3 mV/°C (*illustrative*).
- Circuit: Rp = 2.2 kΩ both sides; low-side driver Ron 133 Ω (0.4 V at 3 mA).

| Case | Vth (25 °C) | T | High side when low side pulls low | Verdict |
|---|---|---|---|---|
| 3.3 V ↔ 5 V | 0.48 … 1.6 V | −40 … 25 °C | 0.46 V; but **sink = 3.40 mA** | VOL spec (0.4 V at 3 mA) **violated**: the driver sinks both pull-ups |
| 1.8 V ↔ 5 V | 0.48 V | any | 0.39 V | OK |
| 1.8 V ↔ 5 V | 1.1 V (typ) | −40 °C | **1.99 V** (VIL = 1.5 V) | **FAIL** |
| 1.8 V ↔ 5 V | 1.6 V (max) | 25 °C | **5.23 V** (never goes low) | **FAIL** |

- The 5 V-side rise time (Rp 4.7 kΩ, CL 30 pF, CH 100 pF) is 396–398 ns for every Vth, ≈ 0.8473·Rp,H·CH. The upper part of the edge, where 0.7·VDD_H is crossed, happens with the MOSFET already off.
- Result: **a Fast-mode FAIL** at these values, and a plain RC formula predicts it.

**Dedicated translators.** Their key constraints live in *prose*, not in min/max tables:
- **TXS0102:**
  - internal 10 kΩ pull-ups on both sides;
  - one-shot edge accelerators (≈ 30 ns);
  - VCCA ≤ VCCB (A 1.65–3.6 V, B 2.3–5.5 V);
  - "adding lower value pull-up resistors will effect VOL".
- **TXB0104:**
  - loads ≤ 70 pF;
  - external pull-up/down resistors > 50 kΩ;
  - "must not be used in applications such as I2C or 1-Wire";
  - OE must be held low with a pulldown until both rails are ramped: a sequencing constraint;
  - the datasheet gives a *linear model* for the effect of external pulls, VOH = VCC·R_PD/(R_PD + 4.5 kΩ).

**Engine walkthrough**
- The MOSFET shifter is a genuine nonlinear DC problem, handled by the worst-point loop once MOSFETs exist.
- The dangerous knobs are Vth at cold (not in the datasheet) and the low-side rail minimum.
- The dedicated ICs are **typed constraints**: "no open-drain driver on this pin", "C_load ≤ 70 pF", "R_pull > 50 kΩ", "OE low until rails stable". These are a job for AI datasheet extraction, not simulation.

**Breaks**
- The 1.8 V case lies **outside the region the datasheet guarantees** (VGS < 2.5 V, T ≠ 25 °C). The verdict should be "FAIL at the datasheet corner; model-dependent elsewhere", not a confident number.
- The simulator has no MOSFET yet.

### 3.5 SPI at tens of MHz

**Specs**
- Setup/hold slack at the peripheral (MOSI) and at the controller (MISO, a round trip).
- CS lead and lag.
- A monotonic SCK edge at every receiver (no double clocking).
- Overshoot within abs-max: VDD + 0.3 V, MSPM0 diode current ±2 mA.

**Worked budget.** Controller MSPM0G3507, peripheral W25Q128JV flash, SPI mode 0, about 100 mm of trace (≈ 0.6 ns each way at ≈ 6 ps/mm):

| Datasheet value | Number |
|---|---|
| MSPM0 tSU.CI (MISO setup at controller), 2.7–3.6 V, no delayed sampling | 29 ns (37 ns at 1.62–2.7 V) |
| MSPM0 tSU.CI / tHD.CI with delayed sampling | 1 ns / **24 ns** |
| W25Q128JV tCLQV (clock low → output valid), max | 6 ns **at CL = 30 pF** |
| W25Q128JV tCLQX (output hold), min | 1.5 ns |

| Mode | Setup needs T/2 ≥ | Hold needs T/2 ≥ | f_max |
|---|---|---|---|
| No delayed sampling | 29 + 6 + 1.2 = 36.2 ns | — | **13.8 MHz** (setup-limited) |
| Delayed sampling (firmware knob) | 8.2 ns | 24 − 1.5 − 1.2 = 21.3 ns | **23.5 MHz** (hold-limited) |
| MSPM0's own footnote formula, max(t_VALID,CO + t_SU,PI, t_SU,CI + t_VALID,PO) | 35 ns | ignored | 14.3 MHz (ignores flight time and hold) |

So "40 MHz SPI" is infeasible for this pair before any signal-integrity question arises. The firmware switch moves the binding constraint from setup to hold.

**Knobs**
- Datasheet min/max: range.
- The VDD band: a table index, 29 vs 37 ns.
- Load capacitance against the datasheet's reference load (30 pF). Derating *flips direction* between setup and hold: a lighter load speeds the output up, which helps setup and hurts hold.
- Flight time: a layout assumption.
- Firmware: SPI mode, divider, delayed sampling. All discrete.

**Engine walkthrough.** This is interval/affine arithmetic over a timing graph, i.e. board-level static timing analysis (§5.2).
- Give each IC one correlated "speed corner" knob, like IBIS min/max columns (§5.1).
- Then the same chip's min and max are never combined. That is common-path pessimism removal, obtained for free from named shared knobs.

**Signal integrity: what can and can't be claimed without layout**

| Can claim (given stated assumptions) | Cannot claim |
|---|---|
| Lumped vs transmission-line class of each net from edge rate and an assumed length. Rule of thumb: distributed when one-way delay > t_r/6. At ≈ 6 ps/mm (microstrip) the critical length is ≈ 28 mm for t_r = 1 ns and ≈ 139 mm for 5 ns [Johnson93] | Crosstalk |
| Whether a series termination exists and whether its value fits Z0 ∈ [45, 55] Ω minus the driver R_out (from IBIS) | Return-path discontinuities, plane splits |
| Overshoot/ringing for an *assumed* point-to-point line, using IBIS clamp curves | Ground bounce / SSN magnitude (needs package and board PDN) |
| Topology warnings from the netlist (multi-drop SCK or CS) | EMI |
| Timing budgets with flight time as a range knob, and the *derived* max length that keeps the budget | Real reflections on branched topologies without geometry |

**Breaks.** Every SI verdict is conditional. The engine should export the assumptions as layout constraints (max length, matching, Z0) and mark the verdict "PASS (conditional)".

### 3.6 Supply supervisor, reset and power sequencing (2–3 rails)

**Supervisor specs.** A two-sided window, plus timing:

| Check | Formula |
|---|---|
| Resets before anything misbehaves | max over the rail's loads of VDD_min,i ≤ VIT−,min |
| No nuisance reset (steady state) | VIT+,max ≤ V_rail,min(dc), otherwise reset may never release |
| No nuisance reset (transients) | VIT−,max < V_rail,min(dc) − dip, where the dip comes from simulation or an assumption |
| Timeout | t_D,min ≥ rail settling + clock start-up (or firmware waits) |
| Output type | a push-pull supervisor must not drive a bidirectional open-drain NRST |
| Low-VDD behaviour | below VPOR the output is undefined (TLV809E: VPOR ≤ 700 mV) |

**Worked numbers**
- Rail: TLV755P 3.3 V, ±1.5 % over −40…125 °C, minus 18 mV of load regulation at 300 mA (typ-only), so V_rail,min(dc) = 3.232 V. With a 50 mV transient dip: 3.183 V.
- Supervisor: TLV809E, VIT− ±2 % over temperature, hysteresis 0.9–1.5 %, t_D (variant A) 130–270 ms.

| Part | VIT− range | VIT+ max | Margin to rail (dc) | Margin with dip | vs a 3.0 V-min device on the rail |
|---|---|---|---|---|---|
| TLV809E 2.93 V | 2.871 … 2.989 | 3.033 | +199 mV | +194 mV | **−129 mV → FAIL** (device can brown out without reset) |
| TLV809E 3.08 V | 3.018 … 3.142 | 3.189 | **+44 mV** | **+41 mV** | +18 mV |
| MSPM0 internal BOR level 3 | 2.85 … 3.01 falling | 3.04 rising | +192 mV | | configurable level: a firmware knob |

- The 3.08 V part is correct but has only ~40 mV of margin on *both* sides.
- The TLV809E is push-pull; the TLV803E is its open-drain twin. MSPM0 recommends a 47 kΩ pull-up and 10 nF on NRST, which suits an open-drain driver.

**Sequencing specs**
- Order (rail B starts only after rail A reaches 90 %).
- Delay windows.
- Monotonic ramps.
- Ramp-rate limits (MSPM0 lists dVDD/dt limits for POR/BOR).
- No I/O above its own VDD + 0.3 V in *any* power state. This is the **back-powering** check: a sensor on an always-on 3.3 V rail driving a pin of an unpowered IC forward-biases its ESD diode, against the MSPM0 limit of ±2 mA. It can also lift the unpowered rail.
- Translators disabled until both rails are up (TXB0104).

**Worked number: RC-delayed enable.** A 1.8 V LDO's EN is driven from the 3.3 V rail through 100 kΩ / 100 nF. The TLV755P EN thresholds are only bounded by VLO ≤ 0.3 V and VHI ≥ 1.0 V, so the switching point lies anywhere in [0.3, 1.0] V.
- Delay: **0.70 … 4.64 ms** (nominal ≈ 2.2 ms).
- Worst-min corner: EN switching at 0.3 V, C −25 % (X7R tolerance plus temperature), rail 3.35 V.
- A "≥ 1 ms after 3.3 V" requirement **fails**.
- The LDO's own start-up (550 µs **typ**, no max) cannot be bounded above.

**Knobs:** rail tolerances and dips (range), threshold spreads (range), timeout spread, the RC (statistical, plus capacitor DC bias and temperature), ramp shape (range: supply ramp time), **power state** (discrete), firmware BOR level (discrete).

**Engine walkthrough**
- Threshold windows are arithmetic and exact.
- Sequencing is a transient with events, but each segment is usually first-order (RC charging, a soft-start ramp). *Piecewise-analytic* crossing times are exact and monotone per segment (§4.4).
- The back-powering check is an **enumeration over power states** (2ⁿ rail on/off combinations plus the transitions allowed by the sequencer). In each state it runs the same abs-max arithmetic.

**Breaks:** typ-only start-up times; unknown ramp shapes; LDO in-rush and current limit; glitch immunity given as a typ curve (TLV809E: 10 µs at 5 % overdrive, typ).

### 3.7 Watchdog

**Specs**
- The kick period lies inside the window over clock accuracy and firmware jitter.
- The first timeout exceeds the boot time.
- The WDO pulse meets NRST's minimum width. MSPM0: 1.5 µs to generate BOOTRST; holding NRST low ≥ 1 s causes a POR.
- The WDO output type suits the NRST pin.

**Worked numbers** (TPS3430, CWD = NC, SET0/1 = 00): tWDL max 25.9 ms, tWDU min 46.8 ms, tRST 170–230 ms.

| MCU tick accuracy | Allowed nominal kick period | With 5 ms firmware loop jitter |
|---|---|---|
| ±3 % (MSPM0 LFOSC, −40…85 °C) | 26.70 … 45.44 ms | 26.70 … 40.44 ms |
| ±5 % (LFOSC over −40…125 °C) | 27.26 … 44.57 ms | 27.26 … 39.57 ms |

The datasheet also warns that at the 3/4 window ratio "tWDL(max) can overlap tWDU(min)". The guaranteed window can be **empty**, and a one-line check catches it.

**Knobs:** window limits (range), clock accuracy (range, temperature), and **firmware assumptions** (kick period, worst-case loop time, boot time). The firmware assumptions are a new knob family: they belong to the software, and the hardware spec must be *exported to* the software team as a requirement.

**Engine walkthrough.** Exact arithmetic. The useful output is the *derived firmware requirement*, "kick every 26.7–40.4 ms". That makes it an interface contract between firmware and hardware, handled like any other block interface.

### 3.8 Crystal oscillator

**Specs**

| Spec | Formula / numbers |
|---|---|
| Load capacitance | CL = C1·C2/(C1 + C2) + C_stray. With 27 pF caps and stray 3–7 pF: CL = 16.5–20.5 pF |
| Frequency budget (ppm) | Initial ±20; tuning fork −0.040 ppm/°C² about 25 ± 5 °C gives **−144 ppm at −40 °C and −169 ppm at 85 °C** (14.6 s/day); aging ±3 ppm per year; pulling |
| Pulling from stray uncertainty | S = C_m / (2(C0 + CL)²). With C_m = 3 fF (*illustrative*), C0 = 1.2 pF, CL = 12.5 pF: 8.0 ppm/pF, so ±2 pF of stray = **±16 ppm** (1.4 s/day) |
| Start-up margin | gm ≥ 5·gm_crit, gm_crit = 4·ESR·ω²·(C0 + CL)² [AN2867]; or a safety factor (R_added + ESR)/ESR ≥ 5 "excellent", 3 "good", < 3 "not recommended" [AN2648] |
| Drive level | ≤ crystal max (ABS07: 0.5 µW; ABM3B: 100 µW) |
| Start-up time | e.g. MSPM0 LFXT 1000 ms typ, HFXT 0.5 ms typ at 32 MHz |

**Worked numbers**
- ABM3B at 8 MHz: ESR ≤ 200 Ω, C0 ≤ 7 pF.
  - CL = 18 pF: gm_crit = **1.263 mA/V**, so margin 5 needs 6.3 mA/V.
  - CL = 8 pF: 0.455 mA/V.
- ABS07 (32.768 kHz): ESR ≤ 70 kΩ, CL 12.5 pF: gm_crit = 2.5 µA/V.

**Pierce small-signal check.** Model: an inverter as a transconductance gm, Rf = 1 MΩ, C1 = C2 = 2·CL, C0 across the crystal. We compute Z seen by the motional arm; oscillation starts when Re Z < −ESR.

| Crystal | gm giving Re Z = −ESR | AN2867 gm_crit | Re Z at gm = 0.5/1/2/5/10/20 mA/V | Most negative achievable |
|---|---|---|---|---|
| 8 MHz, CL 18 pF | 1.280 mA/V | 1.263 mA/V | −78 / −157 / −309 / −688 / −989 / −929 Ω | **−1021 Ω** |
| 8 MHz, CL 8 pF | 0.467 mA/V | 0.455 mA/V | −214 / −404 / −655 / −703 / −460 / −246 Ω | −754 Ω |

Three conclusions:
1. The formula is right, to 1.3 %.
2. |R_neg| is **non-monotone in gm**: too much gm *reduces* it, because C0 shunts the amplifier.
3. With ESR 200 Ω and CL 18 pF, the best possible safety factor is 1021/200 = 5.1, reached *only* at the optimum gm. A "margin 5" requirement is essentially unachievable with that crystal, whatever the MCU.

**Knobs**
- ESR and C0 (datasheet max: range).
- Cap tolerance (statistical).
- **C_stray:** a layout assumption.
- The MCU amplifier's gm. Often only typical: MSPM0 gives "oscillation allowance" OA = 2 kΩ typ for 4–8 MHz and 419 kΩ typ for LFXT. ST's newer parts give a gm_crit_max limit [AN2867]. The formats differ and must be normalized.
- Temperature.

**Engine walkthrough**
- CL, ppm and gm_crit budgets: exact arithmetic.
- Start-up margin: a **linear AC measure at the operating point**, either Re Z at the crystal port or the growth rate Re(pole) of the linearized circuit. It fits the engine's AC path and the worst-point loop.
- Because R_neg vs gm has a hump, corners alone are unsafe. Check both ends of the gm range *and* any interior stationary point, or use the v2 AC range methods with splitting.

**Breaks**
- *Transient start-up simulation is the wrong tool.* Q ≈ 10⁴–10⁵ (ABS07 Q ≥ 10,000).
  - Start-up takes 10³–10⁵ cycles (8 MHz × 0.5 ms = 4000 cycles; 32 kHz × 1 s = 32768 cycles) at tens of steps per cycle.
  - Fixed-step BE adds numerical damping that corrupts the growth rate.
  - Use eigenvalues for start-up. Use describing-function, harmonic-balance or PSS methods for amplitude and drive level.
- The MCU amplifier is a black box with a typ-only figure.
- C_stray depends on layout.

### 3.9 Comparators with hysteresis and Schmitt triggers (multiple operating points)

**Specs**
- VTH+ and VTH− windows.
- Hysteresis ≥ noise p-p + ripple (no chatter).
- Propagation delay.
- Output levels.
- For Schmitt ICs: datasheet VT+, VT− and ΔVT.

**Worked comparator.** Push-pull output to the rails. Vref = VDD·Rb/(Ra + Rb), 10 k/10 k. Input through R1 = 10 kΩ, feedback Rf = 100 kΩ, all 1 %. VDD 3.2–3.4 V; offset ±5 mV.

| Quantity | Exact (corners, shared knobs) | By subtracting separate intervals |
|---|---|---|
| VTH+ | 1.734 … 1.898 V | |
| VTH− | 1.414 … 1.558 V | |
| Hysteresis = VTH+ − VTH− | **313.7 … 346.9 mV** | 176.3 … 484.1 mV (4.6× too wide) |

The hysteresis is exactly VDD·R1/Rf: offset and Vref cancel. Only named shared knobs see that.

**Schmitt ICs: constrained boxes**

| Part | VT+ | VT− | ΔVT (VH) |
|---|---|---|---|
| 74HC14 at 4.5 V | 1.7 … 3.15 V | 0.9 … 2.0 V | 0.4 … 1.4 V |
| 74LVC1G17 at 3.0 V (−40…85 °C) | 1.29 … 1.71 V | 0.88 … 1.24 V | 0.31 … 0.64 V |

Treated as an independent box, the 74HC14 allows VT+ − VT− = 1.7 − 2.0 = **−0.3 V**, i.e. negative hysteresis. The datasheet's ΔVT row is a **linear constraint between two knobs**: 0.4 ≤ VT+ − VT− ≤ 1.4. The knob set is then a polytope, not a box.

**Engine walkthrough**
- A threshold is a **fold (saddle-node) point** of the DC transfer curve. Between VTH− and VTH+ there are three DC solutions: two stable, one unstable.
- With an ideal or piecewise-linear comparator model, each output state is a *linear* circuit. The threshold is then a linear DC quantity in that mode ("V+ = V− with the output at VDD"), so the corner theorem applies per mode and the result is **exact**. This is *mode-conditioned linear analysis*.
- With transistor-level models: arc-length continuation in Vin with fold detection (§5.3).

**Breaks**
- Our DC sweep follows one branch and panics at the fold.
- A plain DC op returns an arbitrary one of the three solutions.
- Chatter depends on noise, which is not in the knob set.

### 3.10 RC timers and the 555

**RC delay into a logic threshold.** 10 kΩ ±1 % × 100 nF, t = RC·ln(VCC/(VCC − VT+)):

| Receiver | C0G ±5 % | X7R ±10 % and ±15 % temperature |
|---|---|---|
| 74LVC1G17 at 3.0 V | 0.529 … 0.895 ms (1.69×) | 0.426 … 1.078 ms (2.53×) |
| 74HC14 at 4.5 V | 0.446 … 1.277 ms (2.86×) | 0.359 … 1.538 ms (4.28×) |

The **logic threshold spread dominates**, not R or C. The engine's contributor ranking will show this at once, even though the capacitor is the usual suspect.

**555 astable.** T = (RA + RB)·C·ln((VCC − VTL)/(VCC − VTH)) + RB·C·ln(VTH/VTL). Datasheet (TI NE555, SLFS022K) at VCC = 5 V:
- THRES level 2.4 / 3.3 / **4.2 V**;
- TRIG level 1.1 / 1.67 / 2.2 V;
- temperature drift 150 ppm/°C and supply drift 0.3 %/V (astable), both **typ only** and "specified by design or characterization, not production tested";
- revision K **deleted** the initial timing-error spec.

| Model of the thresholds | Period range (RA = RB) |
|---|---|
| Independent datasheet boxes | **0.11× … 2.17×** nominal (absurd) |
| Correlated ladder: VTH = 2·VTL·(1 + δ), δ ±1 %; ladder level ±10 % ratiometric | 0.85× … 1.20× |

Both thresholds come from one matched 3-resistor divider, so they move together. That internal structure must be in the part model, as a shared "ladder" knob plus a small ratio knob. Otherwise the arithmetic is useless.

**Engine walkthrough**
- Each phase of a 555 or an RC-plus-Schmitt timer is first-order linear.
- The crossing time is closed-form and monotone in every knob, so it's exact by corners (v2 §A5, applied per mode).
- Steady-state period needs the *second and later* cycles. The first high phase starts from 0 V, not VTL. So the measure must say "period of cycle ≥ 2", or use PSS/shooting.

**Breaks**
- Typ-only drift specs.
- Missing correlation structure.
- No behavioral comparator, flip-flop or switch in the simulator.

### 3.11 Latching circuits: SR latch from discretes, soft power latch

**SR latch specs**
- **Retention:** both states exist over all knobs, i.e. the fold points stay outside the operating range.
- **Set/reset:** minimum pulse amplitude and width.
- **Power-on state**, if one is required.
- **Noise immunity.**

**Power-on state results.** The §2.1 latch, simulated in transient with the supply ramped from 0 to 5 V:

| Supply ramp | Circuit | State reached |
|---|---|---|
| 1 µs | symmetric | stays metastable (the exactly symmetric sim never breaks symmetry) |
| 1 µs | RC2 +1 % / −1 % | Q2 on / Q1 on |
| 1 µs | 22 pF bias cap on b2, RC2 up to **+20 %** adverse | Q1 on (the bias wins) |
| 1 ms | RC2 −1 % | Q1 on (h = 10 ns); **metastable** with h = 2 µs (BE artefact, §4.3) |
| 1 ms | 22 pF bias cap, RC2 +1 % adverse | Q1 on |
| 1 ms | 22 pF bias cap, RC2 **+5 %** adverse | **Q2 on (bias defeated)** |

The capacitive bias injects C·dV/dt: 110 µA for a 1 µs ramp but only 0.11 µA for 1 ms. **The supply ramp time is a range knob that decides a discrete outcome.**

**Soft power latch** (P-MOSFET high-side switch, push-button to turn on, MCU GPIO holds it on).

Specs:
1. Turns on for a press ≥ t_press,min. This needs t_boot,max + t_GPIO-assert < t_press,min: a firmware assumption.
2. Turns off when the GPIO releases.
3. **Does not turn on at battery insertion.**
4. Off-state leakage ≤ a battery-life budget.
5. Holds through supply dips.

Spec 3 is capacitive: at a fast insertion step the gate is held by the Cgd/Cgs divider.
- Illustrative values: Ciss 500 pF, Crss 60 pF, VBAT 4.2 V gives Vgs ≈ −4.2·60/500 = −0.50 V. That is at the edge of |Vth,min| for small PMOS parts.
- A 100 µs insertion ramp with 100 kΩ gate pull-up (τ = 50 µs) gives only −0.22 V.
- Adding 10 nF gate–source cuts the step case to −25 mV.

The engine's margin for this spec is **min over t of (|Vth|,min − |Vgs(t)|)**, taken over the insertion rise time (a range knob, µs to ms).

**Engine walkthrough.** A spec like "latch comes up OFF" has a *discrete* outcome. It has no useful derivative, so the worst-point loop can't linearize it. Two fixes:
1. **A continuous margin proxy,** such as the peak Vgs margin above, the intentional bias minus the worst-case mismatch, or the distance to the separatrix. The loop runs on the proxy.
2. **Confirm the discrete outcome** by simulating the proxy's worst point.

**Breaks:** no MOSFET; no PWL ramp; no events; the BE artefact; and the MCU's boot time is a firmware quantity.

### 3.12 ADC input from a sensor, seen from the MCU

**Specs**
- Settling within ½ LSB in the sampling time.
- Static error budget: source-impedance times leakage, charge-injection average current, reference, gain/offset/INL.
- Input within the absolute range, including when unpowered.
- **The physical-unit error after the firmware conversion** (e.g. ±1 °C from an NTC).

**MSPM0 model** (datasheet): CS/H 3.3 pF, Rin 0.5 kΩ, CI 5 pF, leakage ≤ 50 nA (−40…125 °C).
- τ = (Rpar + Rin)·CS/H + Rpar·(Cpar + CI)
- K = ln(2ⁿ/ε) − ln((Cpar + CI)/CS/H)
- T_min = K·τ

| Source | τ | K (12-bit, ε = ½ LSB) | T_min |
|---|---|---|---|
| 10 kΩ, 10 pF trace | 185 ns | 7.50 | 1.38 µs |
| 50 kΩ (100 k NTC divider), 10 pF | 917 ns | 7.50 | **6.87 µs** |
| 50 kΩ with a 100 nF reservoir cap | — | < 0: charge sharing already below ½ LSB | minimum; but the average current 10.9 nA at 1 kS/s gives 0.54 mV = 0.68 LSB |

**Leakage** (1 LSB = 0.806 mV at 3.3 V):
- 50 nA × 10 kΩ = 0.62 LSB.
- 50 nA × 50 kΩ = **3.1 LSB**.
- A "±1 µA-class" MCU input (a common figure on older parts; *illustrative*) gives **62 LSB**.

For high-impedance sensors the **datasheet leakage row dominates** the error budget: an arithmetic spec that engineers often miss.

**Knobs**
- Source resistance (statistical and the sensor's own).
- Leakage (range, strongly temperature-dependent).
- Reference: ratiometric (VREF = VDD) cancels supply error, absolute does not. Shared knobs show the cancellation automatically.
- Sampling time and rate (firmware).
- Quantization (statistical, uniform).

**Engine walkthrough.** The measure extends through the *firmware transfer function*, e.g. an NTC Steinhart–Hart conversion. The spec is on the reported temperature, not on a voltage. Spec expressions must be able to include the conversion code as an expression, since the spec is really on firmware output.

---

## 4. Cross-cutting weaknesses

### 4.1 Digital parts in an analog simulator

Four fidelity levels, and when each is needed:

| Level | Model | Good for | Cost |
|---|---|---|---|
| 0 | Datasheet arithmetic (no simulation) | Levels, pull-ups, timing budgets, windows: most specs | ~0 |
| 1 | Smooth behavioral I/O: threshold input with tanh hysteresis; output as Thevenin R_out from the tables; delay | RC-into-logic timers, sequencing, power latches, supervisors; **differentiable**, so the worst-point loop works | low |
| 2 | IBIS buffers: I–V tables, V–t waveforms, C_comp, package | Edge rates, overshoot, flight-time correction, clamp currents | medium |
| 3 | Event-driven logic core + A/D bridges (XSPICE-style) + "firmware behavior" scripts | MCU in the loop (boot delay, then assert GPIO), watchdog kicks, latches with firmware | medium |

Transistor-level models of digital ICs are almost never available and not needed. Level 1 matters most for our engine: switch-like models (ideal switches, hard comparators) have discontinuous derivatives, and sensitivities die.

### 4.2 Multiple DC solutions: which operating point is the "real" one?

**Answer: none, from DC alone.** The realized state is a function of **history** (power-up, input sequence) and, for symmetric circuits, of **mismatch and noise**. Consequences for the engine:
1. **Detect** candidate multi-stable blocks:
   - topologically: a transistor circuit needs a particular feedback structure to have multiple equilibria [NW80]; comparators with positive feedback; cross-coupled pairs;
   - numerically: check the stability of every DC solution found (eigenvalues of the (G, C) pencil) [GW92].
2. **Find all solutions** for small blocks: homotopy with multiple-OP search [GG05], or interval all-solution methods (Yamamura, already in the bibliography).
3. **Never linearize at an unstable solution.** Classify first.
4. **Require a history** for specs on multi-stable blocks. Report either "for each stable state" (retention specs) or "in the state reached from setup H" (power-on specs).
5. **Margins are distances to folds:**
   - latch retention = how far VCC can dip before the ON state vanishes;
   - Schmitt threshold = the fold location;
   - unintended bistability = how close a circuit meant to be monostable is to having extra solutions. [GG05] explicitly offers a way to "gauge how close a particular circuit is to possessing multiple operating points".

### 4.3 Start-up behaviour and discrete events

- **Power-up** is the canonical history. It needs PWL ramps, ramp time as a range knob, and a proper transient.
- **BE super-stability:** for an unstable mode λ > 0, one backward-Euler step multiplies the perturbation by 1/(1 − hλ). For hλ > 2 its magnitude is below 1, so the unstable equilibrium *looks stable*.
  - Demonstrated in §3.11: h = 2 µs vs λ = 6.2·10⁷ s⁻¹, hλ ≈ 124.
  - Guards: limit h against the largest positive real eigenvalue near equilibria, and verify the final state's stability.
- **Discrete events** (reset asserted, oscillation started, latch flipped, watchdog fired): these outcomes aren't smooth. Every such spec needs a smooth margin proxy plus a discrete confirmation run.

### 4.4 Timing specs defined by threshold crossings

- **Smoothness.** t*(p) is smooth where the crossing is transversal, and dt*/dp = −(∂v/∂p)/(∂v/∂t) at t* [Galán99]. So crossing times fit the worst-point loop well *if* the simulator provides them.
- **When the slope goes to 0,** sensitivity blows up. Physically that is also a violation: slow edges into non-Schmitt CMOS inputs cause oscillation and shoot-through (TI SCEA046). This gives the automatic spec "input edge rate ≥ datasheet Δt/ΔV".
- **Crossing count changes** (ringing, glitches) make the measure discontinuous. Define measures as "last crossing before settling", and add a separate *monotonic-edge* spec.
- **Fixed steps quantize crossings.** A 1 ms RC with a 10 µs step has up to ±5 µs (0.7 %) error unless the crossing is located by interpolation [ST00].
- **Thresholds are themselves knobs:** VIH = 0.7·VDD, VT+ from a table. The measure is `crossing(v, level = affine form)`.
- **Piecewise-analytic shortcut.** For first-order segments (RC, 555, I2C rise, RC-delayed enables), t = τ·ln((V∞ − V0)/(V∞ − Vth)) is exact and monotone in each argument. No simulation is needed and the result is exact.

### 4.5 Specs that come from datasheet tables, not simulation

This is the biggest opportunity (§6.2 lists them). Four facts make it harder than "min/max arithmetic":
1. **Table indexing.** Rows depend on the VCC band, the temperature column and the test current. There are gaps (74LVC1G04: nothing for 1.95–2.3 V). Knob ranges must be split at band edges, and gaps must yield UNSPECIFIED.
2. **Correlation inside one part.**
   - Schmitt ΔVT;
   - the 555 ladder;
   - IBIS min/max columns ("min" = slow/weak, "max" = fast/strong, *consistently across all tables* [IBIS72]);
   - the same chip's tCO,min and tCO,max, which can't coexist. That is exactly common-path pessimism in timing analysis (§5.2).
3. **Provenance.**
   - guaranteed (production tested);
   - guaranteed by design or characterization (NE555 drift);
   - typ-only (MSPM0 RPU 40 kΩ, OA 2 kΩ; TLV755P start-up and load regulation; TLV809E glitch immunity);
   - missing (LED Vf,min; BSS138 at −40 °C).

   A PASS that relies on a typ-only number is a different claim.
4. **Datasheet revisions change the numbers.** The NE555 initial-error spec disappeared in rev K. Knob provenance must record the document revision.

**Default confidence** for datasheet-derived knobs should be **worst case (range)**. Vendors publish limits, not distributions, and the ECSS practice [ECSS11] treats such biased terms as worst case. This answers open decision 4 of `../specs.md` for digital parts.

### 4.6 Interface contracts between digital and analog blocks

- The I/O page's "GPIO logic levels" should be written as a **contract on levels at a load**: the block guarantees VOH/VOL at I_load and assumes VIH/VIL. Composition is the §3.1 arithmetic with shared rail knobs.
- **Contracts are mode-dependent.** A pin's contract differs in reset, boot, run, sleep and "rail off". The contract needs a mode index (assume–guarantee per mode, [Benv18]).
- **Firmware is on the other side of many contracts:** kick period, boot time, drive strength, pull enables, SPI delayed sampling, I2C timing registers, ADC sampling time, BOR level. These are design parameters owned by software. The engine should both *consume* them as knobs (possibly solver-chosen `?`) and *export* the derived constraints as firmware requirements.
- **Layout is the third party:** Cb, trace length, Z0, crystal stray. The same pattern applies: range knobs now, exported constraints later.

### 4.7 What a schematic-level tool can and cannot claim about signal integrity

See the table in §3.5. The rule: **every SI verdict is conditional on stated layout assumptions**, and the assumptions become layout constraints. Commercial flows push schematic-level constraints (max delay, matching) down to layout; we should export ours the same way. The tool can be quantitative about lumped behaviour and timing budgets. It must be silent, or clearly heuristic, about crosstalk, return paths, SSN and EMI.

---

## 5. Better methods: what literature and industry do

### 5.1 IBIS and IBIS-AMI
- **IBIS** [IBIS72; current version 8.0, ratified 5 Dec 2025] describes an I/O buffer behaviourally:
  - `[Pullup]`, `[Pulldown]`, `[GND Clamp]`, `[POWER Clamp]` I–V tables;
  - `[Ramp]` and `[Rising Waveform]` / `[Falling Waveform]` V–t tables into test fixtures;
  - `C_comp` and the package;
  - receiver thresholds `Vinl` / `Vinh`;
  - the vendor's **timing test load** `Cref` / `Rref` / `Vref` and measurement level `Vmeas`, included "to facilitate board-level timing simulation".
- Every table has typ/min/max columns, and IBIS defines "min" as slow/weak and "max" as fast/strong. That maps to **one discrete, correlated corner knob per IC**. It is not an independent box per parameter.
- **Flight-time correction:** board-level t_co = datasheet t_co + [t(actual load) − t(Cref/Rref)], both simulated with the IBIS buffer to Vmeas. This is how datasheet timing is transferred to a real load.
- KiCad's simulator reads IBIS through its KIBIS layer, which generates SPICE for ngspice (KiCad 7.x release notes and source, *partial*).
- **IBIS-AMI** (added in IBIS 5.0, Aug 2008) models SerDes equalization for multi-Gb/s links. It is **not relevant** to MCU-class boards. Note it only as the hook if that ever changes.

### 5.2 Board-level static timing analysis
- STA concepts [BC09] carry over directly:
  - setup/hold slack;
  - max delays for setup and min delays for hold;
  - common-clock vs source-synchronous budgets (SPI reads are common-clock round trips);
  - **clock reconvergence / common-path pessimism removal (CRPR/CPPR):** don't count the same physical path at both min and max.
- CPPR **is** the v2 §A2 dependency problem. With per-IC corner knobs and named path segments, affine forms remove the pessimism natively. A board STA built on our AA core gets CPPR for free.
- SI/timing references for board budgets: [HH08], [Johnson93].

### 5.3 Homotopy, continuation and multiple operating points
- **Artificial-parameter homotopy** for robust DC convergence [Mel93]; at scale for mixed-signal ICs [RM06]; survey [Traj99].
- **Finding multiple OPs automatically,** classifying their stability, gauging closeness to multistability, and choosing which OP is found first [GG05]. Built as an add-on to a SPICE-like simulator with augmenting resistors: a good fit for us.
- **Topology:** multiple equilibria require a feedback structure [NW80]. **Stability of OPs** [GW92].
- **Arc-length continuation and fold detection** for thresholds and retention margins [AG90]. Numerical techniques for hysteresis and S/N-shaped curves are surveyed alongside measurement analogues in [MG25].
- **Industry practice** (ngspice [ngspice47]):
  - gmin stepping, then source stepping, then a "transient operating point" (`optran`);
  - `.nodeset` / `OFF` for regenerative circuits.

  These *find a* solution. They don't tell you it's the right one.

### 5.4 Mixed-mode and event-driven simulation
- **XSPICE** (Georgia Tech Research Institute, 1992; now inside ngspice [ngspice47]): event-driven digital and user-defined nodes, code models, and `adc_bridge` / `dac_bridge` at the analog–digital boundary.
- **Verilog-AMS** [VAMS23; KZ04]: connect modules convert at domain boundaries.
- **The formal model** for "continuous dynamics plus discrete modes": hybrid automata [Hen96]. Precisely the structure of latches, supervisors, sequencers and firmware-in-the-loop.
- **Event location** [ST00] plus **sensitivities at events** [Galán99]: the numerical core for crossing-time measures and their gradients.

### 5.5 Interface-typed design tools

| Tool | What it models | What it checks | Gaps found |
|---|---|---|---|
| **atopile** (`externals/atopile`) | `ElectricLogic` = line + reference rail + push-pull/open-drain enum; `I2C` with speed-mode enum, address, frequency; `Crystal` with ESR, C0, CL, tolerance, aging | ERC: shorts, shorted power sources, incompatible interface types; `requires_pulls` (window 900 Ω – 11 kΩ, i.e. 1 k −10 % to 10 k +10 %) | **No voltage thresholds** on logic signals. The pull window ignores Cb and speed mode. `requires_pulls.MakeChild` has no `return` ("FIXME: broken"), and `I2C.requires_pulls()` passes an unsupported `interface_type` argument, so the check is effectively inert. `# TODO check multiple pulls per logic`. Crystal parameters exist but no gm_crit/start-up check; `Crystal_Oscillator.capacitance()` is a stub |
| **PolymorphicBlocks** [Lin20, Lin19] | `DigitalSource`: voltage, current limits, output thresholds, high/low driver, pull capable. `DigitalSink`: voltage limits, current draw, input thresholds | `DigitalLink`: overvoltage, overcurrent, "incompatible digital thresholds" (output range ⊇ input range), requires a driver or pull in each direction, conflicting drivers. `I2cLink`: a pull-up present (or controller has one), at most one pull-up, unique addresses, compatible frequencies | Interval semantics, so the shared-rail dependency problem gives false FAILs (§3.1). `I2cPullup` defaults to 4.7 kΩ ±5 % with "TODO restrictions on I2C voltage, current draw modeling": **no rise-time/Cb check**. No timing, modes or reset states |
| **KiCad ERC** [KiCad9] | Pin electrical type: input, output, bidirectional, tri-state, passive, free, unspecified, power input, power output, open collector, open emitter, not connected | A configurable **Pin Conflicts Map** (allowed / warning / error per type pair) | No voltages at all |
| **Altium** (*not verified this session*) | Similar pin types | A project "connection matrix" over pin types | Same class as KiCad |

The pattern: the open tools model *connectivity types* and occasionally intervals. None does quantitative, correlated, mode-aware interface arithmetic. That is the gap our engine fills.

Commercial "voltage-aware" schematic checkers exist (vendor claims *not verified here*), as does IC-level voltage propagation for ESD/EOS rule checks. They support the idea, not our design choices.

### 5.6 Contracts
Assume–guarantee contracts [Benv18] with **modes** (power state, reset state) and **third parties** (firmware, layout). Contract refinement ("upstream guarantee ⊆ downstream assumption") is already in `../specs.md` §2. What's new is indexing by mode and exporting unresolved assumptions to firmware and layout.

---

## 6. Recommended changes to our design

Ranked by impact per unit of effort.

### R1. An automatic-spec engine for datasheet interface arithmetic (highest impact, lowest cost)
- Evaluate on every edit. No simulation needed.
- Every formula below is monotone in each knob, so use the v2 direction proof plus two corners for **exact** results. Put rails in as **shared named knobs** from the power tree.

| Automatic spec | Formula (worst case) |
|---|---|
| Logic high/low margin | VOH,min(I) − VIH,min(VDD_rx) ≥ 0; VIL,max − VOL,max(I) ≥ 0 |
| Pin abs-max, powered and unpowered | V_pin,max ≤ min(V_absmax, VDD_rx + 0.3) in each power state; diode current ≤ limit |
| Pin and supply current | I_pin ≤ rating for the pin type and drive mode; Σ ≤ VDD/VSS limit |
| Input slew | edge rate ≥ Δt/ΔV limit unless the input is Schmitt |
| Open-drain bus | a pull-up exists; exactly one pull-up set; Rp,min ≤ Rp ≤ Rp,max(Cb, speed mode); VOL at total sink current |
| Drivers per net | no push-pull outputs contending; a push-pull output into a bidirectional open-drain pin (e.g. NRST) flagged |
| Timing budget | setup/hold slack over a timing graph with per-IC corner knobs (CPPR automatic) and flight time as a knob |
| Supervisor window | max VDD_min,i ≤ VIT−,min; VIT+,max ≤ V_rail,min |
| Watchdog window | window non-empty; firmware kick period derived |
| Crystal | CL match; ppm budget; gm_crit or safety factor; drive level |
| ADC input | R_src limit for the sampling time; leakage × R_src error; reservoir-cap current error |
| Translator constraints | typed constraints from datasheet prose (TXB: no open-drain, R_pull > 50 kΩ, C ≤ 70 pF) |

### R2. Extend the knob model
1. **Mode knobs (discrete):** power state, reset/boot state, logic state, bus driver identity, firmware configuration, IBIS corner {min, typ, max}. Evaluate by **enumeration with pruning**, never by linearization. Specs quantify over modes explicitly ("in every power state", "in run mode").
2. **Constrained knobs:** linear constraints among datasheet parameters (VT+ − VT− ∈ [ΔVT]; ladder ratios; "tco,min and tco,max are the same chip").
   - The knob set becomes a polytope. Corner methods still work: evaluate at the polytope's vertices.
   - This keeps v2 §A5's "exact" claim valid.
3. **Table-indexed parameters:** a parameter is a table keyed by (VCC band, temperature column, test current).
   - The engine splits the rail and temperature knobs at band edges.
   - A gap gives an **UNSPECIFIED** verdict.
4. **Provenance on every knob bound:** tested / by-design / typ-only / missing / assumed, plus the datasheet revision. It propagates into the verdict (R7).
5. **Assumption knobs owned by third parties:**
   - *layout:* Cb, length, Z0, stray C;
   - *firmware:* kick period, boot time, timing registers.

   Both are range knobs whose *derived* limits are exported as constraints or requirements.

### R3. What interface types should carry

A **pin electrical model**, filled by AI extraction from the datasheet:
- the supply-domain reference (a rail object);
- the drive type (push-pull / open-drain / tri-state / bidirectional) per mode;
- VIH/VIL, or Schmitt VT+/VT−/ΔVT, as a table vs VDD and temperature;
- VOH/VOL vs I (a table, or an IBIS reference);
- abs-max relative to its own VDD and absolutely, *including when unpowered* (fail-safe, IOFF);
- diode/injection current limit, C_in, leakage;
- internal pulls: value range, **when active** (reset, boot, firmware), provenance;
- **reset and boot state**, including strap semantics;
- edge rate / IBIS model; input slew limit;
- timing (tco min/max at Cref with Vmeas; tsu/th);
- firmware-configurable options (drive strength, slew, pull enable, delayed sampling) and how each changes the rows above.

**Bus types** (I2C, SPI, UART) compose pins with:
- the protocol timing table;
- bus-level knobs (speed mode, Cb, topology, length);
- structural rules (one pull-up set, unique addresses).

**The I/O page's "signal driving"** becomes event-timed stimuli with range-knob timing, e.g. "GPIO HOLD goes high 5–50 ms after NRST rises". This is the minimal *firmware behaviour model*.

### R4. Simulator: operating-point robustness and multi-stability

1. **Convergence aids:**
   - SPICE-style junction limiting (`pnjlim`) to replace the ±40·Vt clamp;
   - gmin stepping and source stepping;
   - `.nodeset` / `.ic`;
   - return an error instead of panicking in `simulate_dc`.
2. **Stability classification** of every DC solution: generalized eigenvalues of (G, C) at the solution [GW92]. Refuse to linearize at an unstable point.
3. **Multiple-OP search** for blocks flagged by topology (cross-coupled pairs, positive feedback) [NW80, GG05]:
   - homotopy with augmenting resistors;
   - for small blocks with guarantees, interval all-solution methods (bibliography §5).
4. **Arc-length continuation with fold detection.** Measures: `threshold(block, input)`, `fold_margin(knob)`. Replaces the natural-parameter DC sweep for hysteretic blocks.
5. **Mode-conditioned linear analysis:** for piecewise-linear devices (ideal comparators, switches), enumerate modes, solve each mode's linear circuit exactly with corners, and compute thresholds and crossing times in closed form where segments are first-order.

### R5. Simulator: transient and events

1. **Adaptive step size** (LTE control) with breakpoints at source corners (PULSE/PWL edges).
2. **Event location** with interpolation [ST00]. **Crossing-time sensitivities** [Galán99].
3. **Guard against BE super-stability:** cap h·λ near equilibria, or check the final state's stability.
4. **UIC / start from a given state**, for "which state after history H" and retention tests.
5. **PWL sources** and a supply-ramp setup whose ramp time is a range knob.
6. **Later:** PSS/shooting for timers and oscillator amplitude (v1 already lists PSS for switching converters).

### R6. Devices and a digital layer

- **MOSFET:** a level-1 square-law model is enough to start. Needed for level shifters, load switches, power latches.
- **Voltage-controlled switch, controlled sources (E/G/F/H), smooth behavioral sources** (tanh-based, differentiable).
- **Behavioral logic I/O** (level 1 in §4.1) and an **IBIS buffer element** (level 2).
- **A small event-driven layer** with A/D bridges and scripted firmware processes (level 3). Enough for "MCU boots in 5–50 ms, then asserts HOLD".

### R7. New measures and verdicts

**Measures**
- `rise_time(v, lo, hi)` with thresholds that are affine forms (0.3·VDD);
- `crossing(v, level, dir, nth | last)`;
- `max_over_t`;
- `monotonic(v, window)`;
- `port_impedance(node_a, node_b, f)` and `poles()` for oscillator start-up;
- `threshold` / `fold_margin`;
- `state_reached(setup)`, a discrete measure paired with a mandatory continuous proxy.

**Verdicts.** Add these to PASS / FAIL / UNDECIDED:
- **PASS (conditional: layout assumption X)**;
- **UNSPECIFIED** (the datasheet guarantees nothing in this region);
- **relies on typ-only parameter Y**.

**Rules**
- A multi-stable block always reports **all stable states**.
- A spec on it **must declare its history**.

### R8. Build-order additions (fits v2 Part G)

| Lesson | Idea |
|---|---|
| 3b | Datasheet arithmetic with shared rail knobs; the §3.1 interval-vs-shared table as the regression test |
| 4b | Mode knobs and constrained (polytope) knobs; the 74HC14 ΔVT and 555-ladder examples |
| 5b | Multi-OP latch: `pnjlim` finds the metastable point, then stability classification and homotopy find all three |
| 6b | Event location and crossing sensitivities on the RC-into-Schmitt timer (exact answer known in closed form) |

---

## 7. How each circuit maps to engine methods

| Circuit | Main spec type | Method | Exact? | Needs new simulator features? |
|---|---|---|---|---|
| Logic compatibility | Arithmetic | Corners over shared rails, table-indexed | Yes | No |
| GPIO → LED / MOSFET | 1-D nonlinear + arithmetic; modes for reset state | Corners; mode enumeration | Yes (within table region) | MOSFET for the gate case |
| I2C | Per-mode linear RC + arithmetic | Closed-form crossing + corners | Yes, conditional on Cb | No |
| MOSFET level shifter | Nonlinear DC + RC | Worst-point loop | No (model-dependent) | MOSFET |
| Dedicated translators | Typed constraints | Rule checks | Yes | No |
| SPI | Timing graph | Affine STA with corner knobs | Yes, conditional on flight time | No (IBIS later) |
| Supervisor | Arithmetic | Corners | Yes | No |
| Sequencing | Piecewise-first-order transient + power-state enumeration | Closed form; modes | Mostly | PWL, events |
| Watchdog | Arithmetic | Corners | Yes | No |
| Crystal | Arithmetic + AC port measure (non-monotone) | Corners + AC range / loop with interior check | Budgets yes; start-up estimated | Linearized AC of active devices |
| Comparator / Schmitt | Fold points | Mode-conditioned linear analysis or continuation | Yes for PWL models | Continuation |
| RC / 555 timers | Per-mode crossings | Closed form + constrained knobs | Yes | Behavioral comparator for simulation |
| Latches | Multi-stable + history | Stability classification, homotopy, transient with proxy margins | No | Homotopy, events, UIC, MOSFET |
| Watchdog / firmware contracts | Arithmetic + exported requirement | Corners | Yes | No |
| ADC input | Arithmetic through firmware conversion | Corners / AA | Yes | No |

About two thirds of the specs in this note are exact, simulation-free arithmetic once the knob model (R2) and the pin model (R3) exist.

---

## 8. References

**How these were checked.**
- **VERIFIED:** confirmed this session against a primary record (publisher/Crossref metadata, or the PDF itself).
- **(partial):** exists, but a detail was only seen in a secondary source.
- **(not verified):** could not be checked this session.

Keys already in `../bibliography.md` (e.g. [Galán99] = Galán, Feehery & Barton 1999; [ECSS11]; [AGW94]) are not repeated.

### 8.1 Papers and books

- **[NW80]** R. O. Nielsen, A. N. Willson, "A fundamental result concerning the topology of transistor circuits with multiple equilibria," *Proc. IEEE* 68(2):196–208, 1980. doi:10.1109/PROC.1980.11617. VERIFIED (Crossref).
- **[GW92]** M. M. Green, A. N. Willson, "How to identify unstable DC operating points," *IEEE TCAS-I* 39(10):820–832, 1992. doi:10.1109/81.199863. VERIFIED (Crossref).
- **[Mel93]** R. C. Melville, L. Trajković, S.-C. Fang, L. T. Watson, "Artificial parameter homotopy methods for the DC operating point problem," *IEEE TCAD* 12(6):861–877, 1993. doi:10.1109/43.229761. VERIFIED (Crossref).
- **[RM06]** J. Roychowdhury, R. Melville, "Delivering global DC convergence for large mixed-signal circuits via homotopy/continuation methods," *IEEE TCAD* 25(1):66–78, 2006. doi:10.1109/TCAD.2005.852461. VERIFIED (Crossref).
- **[GG05]** L. B. Goldgeisser, M. M. Green, "A method for automatically finding multiple operating points in nonlinear circuits," *IEEE TCAS-I* 52(4):776–784, 2005. doi:10.1109/TCSI.2005.844359. VERIFIED (Crossref and the PDF: stability detection, closeness to multiple OPs, choice of which OP is found first; examples include a Schmitt trigger, a voltage regulator with a latch-up state, and a circuit with nine DC operating points).
- **[MG25]** R. C. Melville, M. M. Green, "A continuation method-based approach for measuring the DC behavior of nonlinear circuits," *Int. J. Circuit Theory Appl.* 54(2):1135–1150, 2026 (online May 2025). doi:10.1002/cta.4602. VERIFIED (Crossref and abstract).
- **[Traj99]** L. Trajković, "Homotopy methods for computing DC operating points," *Wiley Encyclopedia of Electrical and Electronics Engineering*, 1999. doi:10.1002/047134608X.W2526. VERIFIED (Crossref).
- **[AG90]** E. L. Allgower, K. Georg, *Numerical Continuation Methods: An Introduction*, Springer, 1990; reprinted as *Introduction to Numerical Continuation Methods*, SIAM Classics, 2003, doi:10.1137/1.9780898719154. VERIFIED (Open Library, Crossref).
- **[Lin20]** R. Lin, R. Ramesh, C. Chi, N. Jain, R. Nuqui, P. Dutta, B. Hartmann, "Polymorphic Blocks: Unifying high-level specification and low-level control for circuit board design," *Proc. ACM UIST 2020*, pp. 529–540. doi:10.1145/3379337.3415860. VERIFIED (Crossref). Code: github.com/BerkeleyHCI/PolymorphicBlocks (`edg/electronics_interfaces/DigitalPorts.py`, `I2cPort.py`, `edg/circuits/I2cPullup.py`, read this session).
- **[Lin19]** R. Lin, R. Ramesh, A. Iannopollo, A. Sangiovanni-Vincentelli, P. Dutta, E. Alon, B. Hartmann, "Beyond schematic capture: Meaningful abstractions for better electronics design tools," *Proc. CHI 2019*, pp. 1–13. doi:10.1145/3290605.3300513. VERIFIED (Crossref).
- **[Vittoz88]** E. A. Vittoz, M. G. R. Degrauwe, S. Bitz, "High-performance crystal oscillator circuits: theory and application," *IEEE JSSC* 23(3):774–783, 1988. doi:10.1109/4.318. VERIFIED (Crossref).
- **[Benv18]** A. Benveniste et al., "Contracts for system design," *Foundations and Trends in EDA* 12(2–3):124–400, 2018. doi:10.1561/1000000053. VERIFIED (Crossref).
- **[Hen96]** T. A. Henzinger, "The theory of hybrid automata," *Proc. LICS 1996*, pp. 278–292. doi:10.1109/LICS.1996.561342. VERIFIED (Crossref).
- **[ST00]** L. F. Shampine, S. Thompson, "Event location for ordinary differential equations," *Comput. Math. Appl.* 39(5–6):43–54, 2000. doi:10.1016/S0898-1221(00)00045-6. VERIFIED (Crossref).
- **[BC09]** J. Bhasker, R. Chadha, *Static Timing Analysis for Nanometer Designs: A Practical Approach*, Springer, 2009. doi:10.1007/978-0-387-93820-2. VERIFIED (Crossref chapter records). The CPPR/CRPR discussion is standard STA practice (partial: chapter not read).
- **[HH08]** S. H. Hall, H. L. Heck, *Advanced Signal Integrity for High-Speed Digital Designs*, Wiley, 2008. doi:10.1002/9780470423899. VERIFIED (Crossref).
- **[Johnson93]** H. W. Johnson, M. Graham, *High-Speed Digital Design: A Handbook of Black Magic*, Prentice Hall, 1993. (partial: Open Library lists Johnson, 1993.)
- **[KZ04]** K. Kundert, O. Zinke, *The Designer's Guide to Verilog-AMS*, Kluwer/Springer, 2004. doi:10.1007/b117108. VERIFIED (Crossref).

### 8.2 Standards, specifications, tool documentation

- **[UM10204]** NXP, *I2C-bus specification and user manual*, UM10204 Rev. 7.0, 1 Oct 2021. VERIFIED (PDF): Tables 10–11; §7.1 pull-up sizing, including the 0.8473 derivation.
- **[IBIS72]** IBIS Open Forum, *IBIS Version 7.2*, ratified 27 Jan 2023. VERIFIED (spec text): `Cref` / `Rref` / `Vref` / `Vmeas` "to facilitate board-level timing simulation"; "min" = slow/weak and "max" = fast/strong; complete [Model] keyword list.
  - IBIS 8.0 ratified 5 Dec 2025; IBIS 5.0 Aug 2008. VERIFIED (ibis.org).
  - IBIS-AMI introduced with 5.0. (partial: secondary sources.)
- **[ngspice47]** H. Vogt, G. Atkinson, D. Warning, P. Nenzi, *Ngspice User's Manual*, version 47, 11 Aug 2026. VERIFIED (PDF): XSPICE from Georgia Tech Research Institute (1992); event-driven nodes; `adc_bridge`; gmin/source stepping; `optran`; the note on regenerative circuits and `.nodeset` / `OFF`.
- **[VAMS23]** Accellera, *Verilog-AMS Language Reference Manual*, 2023 release (published 2024-02); previous 2.4 (2014-06). VERIFIED (Accellera downloads page).
- **[KiCad9]** KiCad 9.0 Eeschema documentation: ERC Pin Conflicts Map and pin electrical types. VERIFIED (docs.kicad.org).
  - KiCad 7 IBIS/KIBIS: (partial: 7.0.x release notes mention IBIS fixes; `SIM_LIBRARY_IBIS` in the source docs).
- **JEDEC JESD8 series** (LVTTL/LVCMOS 3.3 V, 2.5 V, 1.8 V levels). (not verified: behind JEDEC login). The 74LVC values used here come from the verified datasheet instead.
- **Altium Designer** connection matrix. (not verified this session.)

### 8.3 Application notes and datasheets (all read this session unless marked)

| Key | Document |
|---|---|
| [AN10441] | NXP AN10441, *Level shifting techniques in I2C-bus design*, Rev. 01, 18 Jun 2007 (Rev. 2, 10 Feb 2020, hosted by Nexperia; not opened) |
| [SLVA689] | R. Arora, TI SLVA689, *I2C Bus Pullup Resistor Calculation*, Feb 2015 |
| [AN2867] | ST AN2867, *Oscillator design guide for STM8AF/AL/S, STM32 MCUs and MPUs*. **(not verified: st.com unreachable this session)**. Its gm_crit formula was cross-checked numerically (§3.8, 1.3 % agreement) |
| [AN2648] | Microchip AN2648 (AVR4100), *Selecting and Testing 32 KHz Crystal Oscillators for Microchip AVR Microcontrollers*, DS00002648A, 2018: safety factor table |
| [AN826] | S. Bible, Microchip AN826, *Crystal Oscillator Basics and Crystal Selection for rfPIC and PICmicro Devices*, DS00826A, 2002 |
| [SCEA043] | S. Curtis, D. Moon, TI SCEA043, *A Guide to Voltage Translation With TXB-Type Translators*, Mar 2010 |
| [SCEA044] | D. Moon, A. Sultana, TI SCEA044, *A Guide to Voltage Translation With TXS-Type Translators*, Jun 2010 |
| [SCEA035A] | P. Dhond, TI SCEA035A, *Selecting the Right Level-Translation Solution*, Jun 2004 |
| [SCEA046] | TI SCEA046B, *Understanding Schmitt Triggers*, Sep 2011 |
| Datasheets | Nexperia 74LVC1G04 Rev 18.1 (2024); 74HC_HCT04 Rev 10 (2024); 74HC_HCT14 Rev 10 (2024); 74LVC1G17 Rev 16.1 (2024); BSS138BK Rev 1 (2011) · TI NE555 SLFS022K (Mar 2026); TXS0102 SCES640L (Jan 2026); TXB0104 SCES650K (Mar 2025); MSPM0G350x SLASEX6C (Oct 2025); TLV809E SLVSES2J (May 2021); TPS3430 SBVS366A (Oct 2021); TLV755P SBVS320D (Sep 2024) · Winbond W25Q128JV Rev I (2021) · Abracon ABS07 (rev 08-10-22), ABM3B · Kingbright APT1608EC, APT1608SGC, APT1608QBC-D |

### 8.4 Source code read this session
- `externals/atopile`:
  - `src/faebryk/library/{requires_pulls,I2C,ElectricLogic,ElectricPower,can_be_pulled,Crystal,Crystal_Oscillator}.py`;
  - `src/faebryk/libs/app/erc.py`.
- `crates/spicy_simulate/src/{dc,trans,lib}.rs` and `devices/bjt.rs` in this repo.

---

## Appendix: formulas behind the numbers

| Number | Formula / model |
|---|---|
| Logic margins | VOH,min − VIH,min with VIH = 0.7·VDD_rx; shared VDD knob gives min over VDD of (0.3·VDD − 0.45) |
| I2C window | Rp,min = (VDD,max − 0.4)/3 mA; Rp,max = 300 ns/(0.8473·Cb) |
| SPI | T/2 ≥ tSU,ctrl + tCLQV + 2·t_flight (setup); T/2 ≥ tHD,ctrl − tCLQX − 2·t_flight (hold); t_flight = 0.6 ns |
| Supervisor | VIT± from the ±2 % accuracy and 0.9–1.5 % hysteresis; rail = 3.3·(1 − 0.015) − 0.06 V/A·0.3 A (− 50 mV dip) |
| RC enable delay | t = RC·ln(V/(V − Vth)); corners over R ±1 %, C ±25 %, Vth ∈ [0.3, 1.0] V, V ∈ [3.25, 3.35] V |
| 555 | T/(C·R) = 2·ln((5 − VTL)/(5 − VTH)) + ln(VTH/VTL) for RA = RB = R |
| Comparator | VTH = (Vref·(R1 + Rf) − Vout·R1)/Rf, Vout ∈ {0, VDD}, Vref = VDD·Rb/(Ra + Rb) + Vos |
| Crystal | gm_crit = 4·ESR·ω²·(C0 + CL)²; Pierce: 2-node admittance with gm, Rf = 1 MΩ, C1 = C2 = 2·CL, C0 across the crystal; Re of Z between the nodes |
| Latch | Ebers–Moll transport model (IS 1e-16, BF 100, BR 1), clamp ±40·Vt, Newton abs 1e-6 / rel 1e-3; stability from −C⁻¹J with Cπ = gm·0.3 ns + 2 pF, Cμ = 1 pF, 1 pF per node; transient: backward Euler with Cπ = 5 pF |
| Level shifter | Square-law NMOS, K = 1/(6.5·0.9) A/V², Vth tempco −3 mV/°C (*illustrative*), body diode source → drain; nested bisection for DC, forward-Euler RC transient for rise time |
| ADC | τ, K, T per the MSPM0 datasheet equations; leakage error = I_lkg·R_src; reservoir current = f_s·CS/H·V |
