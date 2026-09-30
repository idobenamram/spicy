# What Engineers Actually Set Up: Interfaces, Loads, Sources and Benches

> Research report · 2026-09-29 · Input for the contract redesign (specs, setups, conditions). Written for a reader who is not an electrical engineer.
> **Where the numbers come from.** Numbers marked **[S]** were simulated for this report on ngspice-42 (`/usr/bin/ngspice`, batch mode) by stdlib-Python scripts in `/root/.claude/jobs/443154a8/tmp/engine4/ee/` (`celib.py`, `ce2.py`, `parts.py`, `ldo.py`, `drop.py`, `stage.py`, `adc.py`, `board.py`, `corners.py`, `btran.py`, `fix.py`, `droop.py`, `final.py`). Numbers marked **[B]** come from `research/next_boards.md`, **[P]** from `next_power.md` / `next_precision_analog.md`. Numbers marked **[DS]** are from the datasheets and standards listed in §7. The scripts stay out of the repo.

---

## 0. Summary

Every connection between two blocks is a **voltage divider**. The driving side is a *source*: an ideal voltage behind an impedance (its Thévenin equivalent). The receiving side is a *load*: an impedance. So a block's output value depends on what it drives, and the signal that arrives at an input depends on what drives it. This is why a stage verified alone can fail in context: the walkthrough CE amp has a 4.70 kΩ output impedance, and the next CE stage's 7.93 kΩ input impedance divides its gain from 4.59 to **2.88** (the formula gives 4.5908 × 7.928/(7.928+4.700) = 2.882; the simulation gives 2.8823) **[S]**. The answer to the lead's question is: **on an output, you write a load. On an input, you write a source** (its signal, its impedance, and its disturbances: noise, ripple, droop, slow edges, overshoot). "The previous stage misbehaves" is always a description of the *source*, because it is about what arrives. What a block does to its neighbors (its own input impedance, its own output impedance, its supply current) is a *guarantee* of that block. Real practice confirms the rest of the design direction. Datasheets are written as **one header test condition plus per-row overrides** (the TLV755P LDO: "VIN = VOUT + 0.5 V, IOUT = 1 mA, CIN = COUT = 1 µF unless otherwise noted"). That is exactly a default bench plus form benches. A datasheet uses about 4 to 13 distinct setups; most specs share the header one. Verification plans differ from datasheets in one way: the datasheet tests at **fixed** values (RL = 10 kΩ, CL = 60 pF) so parts can be compared; a product must hold **for all** loads and sources in the ranges its neighbors really present. The worst case is often *not* the heaviest load: the LDO modeled here has its worst output-impedance peak (9.2 Ω) at 100 µA, not at 500 mA (0.18 Ω) **[S]**. Capacitive loads are the classic trap, because they can make a feedback loop ring or oscillate: 100 nF straight on the op-amp output gives 69% overshoot; the same capacitor behind 470 Ω gives 2.4% **[S]**. Section 4 defines a small shared reference board: USB 5 V → 3.3 V LDO → op-amp gain stage → RC anti-alias filter → MCU ADC, with an MCU load on the rail. It has exact values, ranges and specs, block-level and board-level. Two of its board-level specs exist only at board level and fail with a naive design: the MCU's load steps disturb the ADC reading by −10/+12 codes through the shared rail, which a 10 Ω + 10 µF filter reduces to −0.4/+2.1 codes **[S]**.

---

## 1. The physics of interfaces, from scratch

### 1.1 Every output is a source with an impedance

Take any output: an amplifier output, a regulator output, a sensor, a USB port. If you draw more current from it, its voltage changes a little. The simplest model that captures this is an **ideal voltage source in series with a resistor**:

```
      block output, seen from outside
   ┌────────────────────────┐
   │   Z_out                │
   │ ──/\/\/──┬─────────────┼──● out
   │          │             │
   │  (+) V_th (ideal)      │
   │          │             │
   └──────────┴─────────────┼──● gnd
                            │
```

- **V_th** is what you measure with nothing connected (the "open-circuit" voltage).
- **Z_out** (the source impedance, `z_src` in `language.md` §3.2) tells you how much the voltage drops per unit of current drawn: ΔV = I × Z_out.

This is the **Thévenin equivalent**. Every linear port has one, at every frequency. Some real values:

| Output | V_th | Z_out | How it was found |
|---|---|---|---|
| Walkthrough CE amp, collector | 4.59 × v_in (AC) | **4.70 kΩ** (= RC; no Early effect in the model) | inject 1 A AC into the output with the input shorted, read the voltage **[S]** |
| MCP6001 op-amp stage, G = 10 (closed loop) | 10.09 × v_in | **0.30 Ω at 100 Hz, 3.0 Ω at 1 kHz, 30 Ω at 10 kHz, 210 Ω at 100 kHz** | same method **[S]**. Feedback makes it tiny at low frequency, but it rises as the loop gain falls |
| TLV755P-class 3.3 V LDO at 30 mA | 3.30 V | **83 mΩ at 100 Hz, rising to 0.62 Ω at 50 kHz** | AC current into the output **[S]** |
| USB cable from the host | 5 V | about 0.1–0.5 Ω (VBUS + GND wires and contacts) | USB 2.0 budgets the cable drop, see §4 |
| A sensor (piezo, pH probe, thermistor bridge) | the measured quantity | 100 Ω … 10 MΩ, depending on type | its datasheet |

The last column matters: for an amplifier, Z_out is not a resistor you placed. It is a property that emerges from the circuit, and it depends on frequency.

**Norton** is the same port drawn the other way: an ideal current source I_N = V_th / Z_out in parallel with Z_out. Both are exact descriptions of the same linear port. Engineers pick whichever makes the arithmetic shorter: Thévenin for voltage outputs (amplifiers, regulators), Norton for current outputs (a photodiode, a transistor collector seen as a current, a 4–20 mA loop).

### 1.2 Every input is a load

An input draws some current from whatever drives it. The simplest model is an impedance to ground (or to a bias voltage):

```
                  block input, seen from outside
            ┌────────────────────────┐
 in ●───────┼──┬                     │
            │  Z_in                  │
 gnd ●──────┼──┴                     │
            └────────────────────────┘
```

| Input | Z_in | Source |
|---|---|---|
| Walkthrough CE amp | **7.93 kΩ** at 1 kHz (R1 ∥ R2 ∥ the transistor's base, ≈ β·RE) | **[S]** |
| MCP6001 stage of §4 (AC-coupled, 470 kΩ bias resistor) | **470 kΩ** at 1 kHz, 473 kΩ at 20 Hz | **[S]** |
| MCP6001 pin itself | 10¹³ Ω ∥ 6 pF (common mode) | **[DS]** MCP6001 |
| MCU ADC pin (SAR, sampling) | a switch (≤ 1 kΩ) to an 8 pF capacitor that is connected only while sampling. On average it looks like R = 1/(f_s·C_s) = **2.5 MΩ** at 50 kSPS, plus a charge "kick" at every sample | **[DS]** STM32F103; §2.6 |
| 74LVC logic input | ≈ 5 pF, leakage ≤ ±1 µA | **[DS]** |

### 1.3 Connecting them: the divider

Connect an output to an input and you get this:

```
   source (stage 1)                 load (stage 2's input)
   Z_out = 4.70 kΩ
  ──/\/\/──────────┬──────────── v_in2
  (+) V_th         │
   = 4.59 · v_in1  Z_in = 7.93 kΩ
                   │
  ─────────────────┴────────────
```

The voltage that arrives is

```
   v_in2 = V_th · Z_in / (Z_in + Z_out)
         = 4.5908 · 7.928 / (7.928 + 4.700) = 2.882
```

Simulated, stage 1's in-context gain is **2.8823** **[S]**, against 4.5908 alone. That's the whole story of the two-stage amplifier in `next_boards.md`: the product of the two unloaded stage gains is 21.1 nominal; the real chain is 13.23 **[S][B]**. The same arithmetic, over load values **[S]**:

| Load on the CE amp output (through a large coupling cap) | Gain at 1 kHz | Loss |
|---|---|---|
| none (1 GΩ) | 4.590 | 0 |
| 100 kΩ | 4.384 | −4.5% |
| 10 kΩ | 3.123 | −32% |
| 7.93 kΩ (= the next CE stage) | 2.882 | −37% |
| 4.70 kΩ (= its own Z_out) | 2.295 | −50% (exactly half: that is how Z_out is measured on a bench) |
| 1 kΩ | 0.805 | −82% |

**The rule of thumb engineers use:** for a voltage signal, make Z_in at least **10× Z_out** (≈ 9% loss) and preferably **100×** (≈ 1% loss). This is called "bridging". The CE amp → CE amp connection violates it (7.93/4.70 = 1.7×). The op-amp stage → anything connection almost never does, because its Z_out is a few ohms.

### 1.4 The same on the input side: the source impedance

Now look at the CE amp's *input*. Drive it from a sensor with source impedance R_s. The sensor's own V_th is the "true" signal (the EMF). The amplifier's 7.93 kΩ input divides it **[S]**:

| Sensor source impedance R_s | Gain from the sensor EMF to the output | Low cutoff f_L |
|---|---|---|
| 50 Ω (a lab signal generator) | 4.562 | 19.95 Hz |
| 600 Ω (classic audio line) | 4.268 | 18.66 Hz |
| 2.2 kΩ (electret microphone) | 3.594 | 15.71 Hz |
| 10 kΩ (high-impedance sensor) | 2.030 | 8.88 Hz |

Two things change at once: the gain (the divider) and the low cutoff (R_s adds in series with C_in's resistance, so the RC corner moves down). Neither change is a property of the amplifier alone or of the sensor alone. **It exists only at the connection.**

**An important subtlety for specs: "gain from where?"**
- The **pin-to-pin** gain, v(out)/v(in), measured at the amplifier's own input pin, is **unchanged** by R_s for this circuit. The input pin voltage is lower, and the output is lower by the same ratio.
- The gain from the **sensor EMF** changes with R_s.
- The pin-to-pin gain **does** change with the load on the *output* (the table in §1.3), because the output divider is inside the "pin-to-pin" path.

So: *output loads change pin-to-pin specs; source impedances change source-referred specs.* A spec author must say which gain they mean. Datasheets do. An op-amp's "gain" is pin-to-pin with RL = 10 kΩ; an audio amp's "sensitivity" is referred to a 600 Ω source.

Source impedance matters for more than gain:
- **offset**: an op-amp input bias current I_B × R_s is a DC error (1 nA × 1 MΩ = 1 mV);
- **noise**: R_s has thermal noise, √(4kTR): 12.8 nV/√Hz for 10 kΩ;
- **bandwidth**: R_s with the input's capacitance makes a low-pass (10 kΩ × 10 pF → 1.6 MHz);
- **stability**: some inputs (a Sallen-Key filter, an emitter follower) behave differently with a high-impedance source.

### 1.5 Thévenin and Norton: how a port is characterized in practice

A linear port is fully described, at each frequency, by **two numbers**: V_th and Z_th (or I_N and Z_th). Engineers find them by measuring two conditions:

| Method | What you do | Used for |
|---|---|---|
| Open + short | V_th = open-circuit voltage, I_N = short-circuit current, Z = V_th / I_N | batteries, sensors, low-power sources |
| Two loads | measure V at two load values, solve the two divider equations | amplifier outputs on the bench; "load until the voltage halves" gives Z_out directly (row 4.70 kΩ above) |
| Injection | null the signal, push a small AC test current into the port, Z = v/i | simulation (what I did); regulator output impedance vs frequency |

**What this means for contracts:** a port's full interface description is small. An output guarantees `(V_th, Z_out)`; an input guarantees `Z_in`. For power, the rail guarantees `(V, Z_out(f), I_max)` and the sink guarantees `I` (its current draw). Everything in §1.3–1.4 follows from these by the divider formula. `language.md` §3.2 already has `z_src` (by the source) and `z_load` (by the sink) on `Analog`.

### 1.6 "The source misbehaves": why it belongs to the source, not to a load on the input

The lead asked whether loads belong on inputs too, "e.g. when the previous stage misbehaves". Here is how practice splits it.

**A load is something that takes current depending on voltage.** A resistor, a capacitor, a current sink, the ADC's sampling capacitor. It sits on the node and *responds* to what the driver puts there.

**A source is something that sets voltage (or current) and has an impedance.** It *initiates* the signal.

When "the previous stage misbehaves", what the next stage experiences is a different *arriving signal*. Every item in the usual list is a property of the source's V_th waveform or of its Z:

| Misbehavior | What it really is | How a test setup models it | Typical numbers |
|---|---|---|---|
| Weak drive | high source impedance | series R_s, as a range | sensor 100 Ω … 10 kΩ; USB cable 0.1–0.5 Ω |
| Wrong DC level / offset | V_th's DC part is off | DC source over a range | a previous op-amp's ±4.5 mV offset × its gain |
| Noise | V_th has a random part | a noise source (spectral density) on the source | op-amp 28 nV/√Hz **[DS]**; LDO 71.5 µV rms **[DS]** |
| Ripple / hum | V_th has a periodic part | AC or sine added to the DC source | USB VBUS from a switching host supply: tens of mV at 100 kHz–1 MHz (assumed; the USB spec gives no ripple number) |
| Droop | V_th steps down briefly | a step in the source | USB: ≤ 330 mV droop on a hub port when another device is plugged in **[DS]** USB 2.0 §7.2.4.2 |
| Slow edge | V_th ramps slowly | a PWL/pulse source with a rise time | logic input spec: rise ≤ 10 ns/V **[DS]** 74LVC1G04 Δt/Δv; a power rail that ramps over 100 ms |
| Overshoot / ringing | V_th rings after an edge | a source with overshoot (or its real driver) | a 5 V bus ringing to 6 V; abs-max checks care |
| Out-of-band content | V_th has content above the band | a sine or noise beyond the band | what the anti-alias filter must reject |

None of these is a load on the input. Modeling them as one would be wrong physics. A "load" on an input node would be an extra impedance in parallel with the block's own input, which *changes the block*. Misbehavior of the previous stage doesn't change the block; it changes what arrives.

**The input's own impedance is also not "a load on the input".** It *is* the load that this block puts on its source. That makes it a *guarantee of this block* (`spec z_in >= 450kΩ`) and an *assumption of the upstream block* (`assume output.z_load >= 450kΩ`), checked at the connection. Same for output impedance: a guarantee of the driver, an assumption of the receiver (`assume input.z_src <= 10Ω`).

**Three real exceptions, where an input's behavior matters to the source and must be a load in the *upstream* block's bench:**
1. **Nonlinear or dynamic input loads.** An ADC input kicks charge at every sample (§2.6). An input protection diode clamps and suddenly draws mA when the signal exceeds the rail. The ADC's input model belongs in the **driver's** setup as its load.
2. **Current flowing out of an input.** Op-amp bias current, a CE amp's base divider bias, the ADC's ±1 µA leakage. Still a property of the input, and still a load from the source's view (a current sink, not an impedance).
3. **Bidirectional ports** (I2C, a USB data pair, an `InOut` pin). Each end is sometimes the source and sometimes the load. Both descriptions apply, depending on the phase.

Power ports follow the same rule. A block's `vcc: Power<In>` has a **source** outside (the regulator, with its tolerance, Z_out, ripple, droop and ramp) and presents a **load** to it (its supply current, often with steps). A load step on your *own* supply input is your own current draw, a guarantee of yours (`spec vcc.i <= 30mA`). What the rail does in response is the regulator's source behavior, from its Z_out.

**The summary table the language can follow:**

| Port side | The outside world contributes | The block itself guarantees |
|---|---|---|
| signal input | a **source**: V_th waveform (signal family + disturbances), `z_src` range | `z_in` (and any kick, bias or leakage current) |
| signal output | a **load**: `z_load` / `r_load` range, `c_load` range, or a dynamic load (another block's input model) | V_th (the signal it produces), `z_out` |
| power input | a **source**: V range, `z_src(f)`, ripple, droop, ramp | its current draw `i` (static range, steps) |
| power output | a **load**: current range, steps, capacitance | V (accuracy), `z_out(f)`, `i_max`, transient response |

---

## 2. The standard setups, per block type

Each subsection lists the specs, each spec's setup (sources, loads, analysis) and which specs share a setup. The pattern that repeats everywhere: **one standard fixture (the datasheet's "test conditions unless otherwise noted") plus per-spec overrides.** Only a few specs change the circuit itself (loop breaking, a common-mode drive, a load-step element).

### 2.1 Discrete amplifier (e.g. the CE amp)

```
   Rs            C_in                      C_out
 ─/\/\/─┬─||─── [ AMP ] ───||──┬── out
 (~) vs │                      RL    CL
        │                      │     │
       gnd                    gnd   gnd
```

| Spec | Source | Load | Analysis | Shares setup with |
|---|---|---|---|---|
| Gain at f_ref (1 kHz audio) | AC, specified R_s (50 Ω lab, 600 Ω audio) | specified RL (10 kΩ line in, 600 Ω, 32 Ω headphone) | AC point | bandwidth, Z_out (same circuit) |
| Bandwidth (f_L, f_H at −3 dB) | same | same | AC sweep | gain |
| Output swing / max output before clipping | 1 kHz sine, rising amplitude | the heaviest RL | transient, THD ≤ 1% | THD |
| THD at a level | sine at the rated level | RL | transient + FFT | swing |
| Input impedance | AC; measure v and i at the input pin (or add a series R and find the halving value) | RL (Z_in can depend on the load, e.g. in an emitter follower) | AC | own bench (needs a current probe) |
| Output impedance | input shorted, inject current at the output, or two RL values | none / two values | AC | own bench |
| Bias point (V_C) | DC supply over its range | none needed | op | every DC spec |
| Supply rejection | AC on the supply, signal source at 0 | RL | AC | own bench (AC moved to the supply) |

**How many setups:** about 4–5 distinct circuits (small-signal, large-signal, Z_in, Z_out, supply AC). The gain, bandwidth and bias specs all share one.

### 2.2 Op-amp stage

The datasheet way is the MCP6001's Figure 1-1: one fixture for "most specifications" **[DS]**. Its header says: *"TA = +25°C, VDD = +1.8V to +5.5V, VSS = GND, VCM = VDD/2, VL = VDD/2, RL = 10 kΩ to VL, CL = 60 pF."* The fixture uses RG = RF = 100 kΩ, CF = 6.8 pF, and supply bypass caps of 100 nF and 1 µF.

| Spec | Setup (overrides of the fixture) | Analysis | Typical values [DS] |
|---|---|---|---|
| Offset V_OS | VCM = VSS | op | ±4.5 mV max |
| Open-loop gain | VOUT swept 0.3 V to VDD − 0.3 V | DC sweep | 112 dB typ |
| Output swing | 0.5 V input overdrive, RL to VDD/2 | op | within 25 mV of the rails |
| GBW, slew rate | fixture | AC; step transient | 1 MHz; 0.6 V/µs |
| Phase margin | **G = +1** (the worst gain for stability), CL = 60 pF | loop-gain AC | 90° (45° with 500 pF) |
| CMRR | VCM swept −0.3 V to 5.3 V | DC sweep (or AC on both inputs together) | 76 dB typ |
| PSRR | VCM = VSS; VDD changed (DC) or AC on VDD | DC or AC | 86 dB typ |
| Noise | fixture, 0.1–10 Hz and at 1 kHz | noise | 6.1 µVpp; 28 nV/√Hz |
| Short-circuit current | output shorted | op | ±23 mA at 5.5 V |
| Capacitive-load stability | R_ISO vs CL / G_N chart (Fig. 4-4) | AC/step | ">100 pF when G = +1" needs R_ISO |

**How many setups:** one fixture, about 6–8 variants of it (VCM, VDD, G, CL, shorted output, AC on the supply). Most AC specs share one run. CMRR and PSRR are the ones that *move the AC source* (onto both inputs, onto the supply). That's why `next_precision_analog.md` found that the default bench silently measures the common-mode gain of a two-input block **[P]**.

For an op-amp **stage in a design** (not the bare part), the verification plan adds: gain, bandwidth, Z_in, output swing *into the real load*, stability over the load range (§3.3), and noise referred to the input with the real source impedance.

### 2.3 Filter

| Spec | Source | Load | Analysis |
|---|---|---|---|
| Passband gain, ripple | AC, **source impedance specified** (for a passive RC, R_s adds to R: a 4.7 kΩ CE output in front of a 470 Ω / 100 nF RC moves f_c from 3.39 kHz to 308 Hz) | the real next input (ADC, next stage) | AC sweep |
| Corner f_c (−3 dB), f_0, Q, peaking | same | same | AC sweep |
| Stopband attenuation at f (e.g. at f_s/2 for anti-aliasing) | same | same | AC point |
| Group delay / step settling | step | same | AC phase or transient |
| Active filter: output swing, noise, stability | as §2.2 | real load | as §2.2 |

**How many setups:** usually 1 AC setup (all frequency-domain specs share it) plus 1 transient if settling matters. The Sallen-Key section in `next_precision_analog.md` (Q = 1.307, f_0 = 1 kHz) is this: one AC bench **[P]**. The catch is that the source and load impedances must be in it. A filter is a divider by construction, so its response is *defined* by what's around it.

### 2.4 LDO (linear regulator)

TLV755P, header **[DS]**: *"TJ = −40°C to 125°C, VIN = VOUT(NOM) + 0.5 V or 2.0 V (whichever is greater), IOUT = 1 mA, VEN = VIN, CIN = COUT = 1 µF (unless otherwise noted)."*

```
 VIN (DC + AC or step)          VOUT
 ──●──[C_IN 1µF]──[ LDO ]──●──[C_OUT 1µF, ESR]──[I_LOAD: DC, sweep or step]
```

| Spec | Override of the header | Analysis | TLV755P 3.3 V [DS] | Model of §4 [S] |
|---|---|---|---|---|
| Output accuracy | TJ range; also over VIN and IOUT | op, corners | ±1% (−40…85 °C) | 3.300 V nominal; set by V_ref ±1% |
| Line regulation | VIN swept VOUT + 0.5 V … 5.5 V | DC sweep | 2 mV | 1.0 mV (4.4 → 5.5 V, 30 mA) |
| Load regulation | IOUT swept 0.1 mA … 500 mA | DC sweep | 0.060 V/A | 56 mV/A (27.9 mV over 0.1 → 500 mA) |
| Dropout | IOUT = 500 mA; VIN lowered until VOUT falls | DC sweep + crossing | 150 mV typ, 215 mV max | 215 mV at 500 mA; 13 mV at 30 mA (1% drop) |
| PSRR | **VIN = VOUT + 1 V, IOUT = 50 mA**, AC on VIN at 1 kHz, 100 kHz, 1 MHz | AC | 52 / 46 / 52 dB | 56.7 / 40.0 / 50.0 dB |
| Output noise | VOUT = 1.2 V, 50 mA, 10 Hz–100 kHz | noise | 71.5 µV rms | not modeled |
| Ground current | IOUT = 0 | op | 25 µA typ | 25 µA |
| Current limit | VOUT forced to 0.9 × VOUT | op | 560–865 mA | not modeled |
| Load transient | Fig. 5-8: VIN = 5 V, **1 mA → 500 mA at 1 A/µs** | transient | figure | −150 mV / +109 mV |
| Line transient | Fig. 5-7: VIN step at **1 V/µs** | transient | figure | +9.3 / −8.3 mV (4.3 → 5.3 V, 30 mA) |
| Start-up | VIN = VEN power-up; EN start-up at VIN = 5 V, 100 mA | transient | t_STR 550 µs | 3.2 V at 385 µs |
| Stability | C_OUT ≥ 0.47 µF effective, X5R/X7R | loop gain or load step | "0.47 µF or larger" | Z_out peak vs load, below |

**How many setups:** one header plus about **12** overrides. Line regulation, load regulation and dropout are **sweeps** of the header (the same circuit, one source swept). PSRR moves the AC source to VIN, a different excitation. The load and line transients add a step element. So there are about **4 distinct circuits** (DC, AC-on-VIN, load step, line step) and about 12 *parameterizations*. Load regulation and dropout also show that "IOUT" is a range knob in the header: the spec holds *over* it.

### 2.5 Buck converter

TPS62160 **[DS]**: header TJ −40…125 °C, typical values at VIN = 12 V; typical application L = 2.2 µH, C_OUT = 22 µF, C_IN = 10 µF.

| Spec | Setup | Analysis | TPS62160 [DS] |
|---|---|---|---|
| Output accuracy | PWM mode, VIN ≥ VOUT + 1 V; PSM with C_OUT = 22 µF | op / averaged | ±3% (PWM), −3.5/+4% (PSM) |
| Load regulation | VIN = 12 V, VOUT = 3.3 V | DC sweep of I_OUT | 0.05 %/A |
| Line regulation | 3 V ≤ VIN ≤ 17 V, I_OUT = 0.5 A | DC sweep of VIN | 0.02 %/V |
| Efficiency | η = P_out/P_in vs I_OUT, one curve per VIN (Figs. 8–17) | switching transient to steady state, or averaged + loss model | figure family |
| Output ripple | fixed VIN, I_OUT; measured at C_OUT with a 20 MHz bandwidth limit | switching transient, steady state | Fig. 22 |
| Load transient | **500 mA → 1 A** in PWM; **100 mA → 500 mA** from power-save mode | transient | Figs. 26–29 |
| Start-up | I_OUT = 100 mA and 1 A | transient from 0 | Figs. 30–31 |
| Loop stability | inject a small signal across a 10–50 Ω resistor in the feedback path (Middlebrook's method) and measure loop gain | AC on the averaged model; frequency response analyzer on the bench | phase margin ≥ 45° is the common target |
| Mode transitions | PWM ↔ PSM thresholds vs I_OUT | slow load sweep | Figs. 24–25 |

**How many setups:** about 10. The efficiency family is *one* setup swept over two axes (VIN × I_OUT). The two load-step setups are different circuits, because the starting mode differs. `next_power.md` showed the other reason this block is hard: an averaged model and a switching model disagree on the load-step undershoot (143.1 vs 157.9 mV) **[P]**.

### 2.6 ADC driver

The job: charge the ADC's sampling capacitor to within ½ LSB during the acquisition time, every sample.

```
 op-amp ──R_aa──┬──●──── ADC pin ──[ R_sw ≤ 1kΩ ]──/ switch (closed for t_s)──┬── C_s 8 pF
                C_aa                                                          │
                │                                                            gnd
               gnd
```

What happens at each sample: the switch closes and C_s (holding the previous channel's voltage, worst case 0 V) grabs charge from C_aa. The voltage at the pin dips by ΔV × C_s/(C_s + C_aa). The driver must recover that before the switch opens, or on average between samples. The rules practice uses:
- **C_aa ≥ 20 × C_s**, so the kick is small (Walsh, ADI Analog Dialogue 46, 2012) **[DS]**;
- **settling to ½ LSB needs ln(2^(N+1)) time constants**: 9.0 τ for 12 bits;
- the MCU form: the STM32F103 datasheet (DS5319, §5.3.18) gives the maximum external resistance R_AIN < T_S / (f_ADC × C_ADC × ln(2^(N+2))) − R_ADC, with **C_ADC ≤ 8 pF, R_ADC ≤ 1 kΩ, leakage ±1 µA** **[DS]**;
- the op-amp must be stable driving C_aa, which is why R_aa is in series (§3.3).

| Spec | Source | Load | Analysis |
|---|---|---|---|
| Settling error at end of acquisition (LSB) | worst-case step (full scale, or the previous channel at 0 V) | the **ADC input model**: switch + C_s + sample clock | transient with the sample clock |
| DC error from ADC average current and leakage | DC | R_eq = 1/(f_s·C_s), plus ±1 µA | op |
| THD / SNR at the input frequency | full-scale sine | ADC model | transient + FFT |
| Driver stability | step | R_aa + C_aa + ADC | transient overshoot or loop gain |
| Anti-alias attenuation at f_s/2 | AC | ADC model | AC |

Simulated on the §4 stage (f_s = 50 kSPS, t_s = 1.125 µs, C_s reset to 0 V before each sample) **[S]**:

| R_aa / C_aa (same corner, 3.39 kHz) | Error at end of sample |
|---|---|
| 470 Ω / 100 nF | **−0.47 LSB** ✅ |
| 4.7 kΩ / 10 nF | −4.6 LSB ❌ |
| 47 kΩ / 1 nF | **−45 LSB** ❌ (−32 LSB even with a 17 µs sample time) |

All three filters are identical on an AC plot. They differ by 100× in the only spec that matters. The error is mostly the ADC's **average current** (C_s × V × f_s = 0.66 µA) through R_aa: 0.31 mV through 470 Ω and 31 mV through 47 kΩ. **This is a load that only shows up in a transient setup with the sample clock.** An AC bench with an "8 pF" load would call all three filters equal.

**How many setups:** 2–3 (settling transient, DC error, and FFT when dynamic specs matter).

### 2.7 Digital I/O

SN74LVC1G04 **[DS]**:

| Spec | Setup | Values at V_CC = 3 V [DS] |
|---|---|---|
| V_OH at a load current | output high, **current source drawing I_OH** = −100 µA, −16 mA, −24 mA | ≥ V_CC − 0.1 V; ≥ 2.4 V; ≥ 2.3 V |
| V_OL at a load current | output low, current source pushing I_OL = 100 µA, 16 mA, 24 mA | ≤ 0.1 V; ≤ 0.4 V; ≤ 0.55 V |
| Propagation delay t_pd | input pulse (t_r ≤ 2.5 ns, 0–3 V, measured at 1.5 V), **load: C_L = 15 pF ∥ R_L = 1 MΩ** | 0.7–3.3 ns |
| t_pd into a heavier load | **C_L = 50 pF ∥ R_L = 500 Ω** (§6 load circuit) | 1.0–4.2 ns (−40…85 °C) |
| Input slew requirement | the *source* (the driving gate) must be faster than 10 ns/V | Δt/Δv ≤ 10 ns/V |
| Input levels V_IH / V_IL, leakage, C_in | DC | datasheet table |

**How many setups:** about 4: a DC current-load family (one per I_OH/I_OL row), and two AC load circuits. The **load is the whole test**: V_OH has no meaning without I_OH. The DC numbers also give the output's Thévenin resistance: (3.0 − 2.3 V)/24 mA ≈ 29 Ω. So the rise time into a 50 pF bus is about 2.2 × 29 Ω × 50 pF ≈ 3.2 ns.

### 2.8 The common shape, and the counts

| Block | Distinct circuits | Parameterizations (header + overrides) | Specs sharing the main setup |
|---|---|---|---|
| Discrete amp | 4–5 | ~6 | gain, bandwidth, bias |
| Op-amp (datasheet) | 1 fixture + CMRR/PSRR/loop variants ≈ 4 | 6–8 | offset, gain, GBW, swing, noise |
| Filter | 1–2 | 1–2 | all frequency-domain specs |
| LDO (TLV755P) | 4 (DC, AC-on-VIN, load step, line step) | ~13 | accuracy, line/load regulation, dropout (sweeps of the header) |
| Buck (TPS62160) | 5–6 (DC, switching steady state, 2 load steps, start-up, loop injection) | ~10 | accuracy, regulation |
| ADC driver | 2–3 | 2–3 | settling and DC error share the ADC model |
| Digital output | 3 | ~4 per V_CC | V_OH/V_OL rows share one DC circuit |

Two kinds of "different setup" appear, and they cost different things:
1. **Same circuit, different values or sweeps** (IOUT = 50 mA instead of 1 mA; VIN swept). This is struct-update over a default: `language.md` §8.5's form bench.
2. **A different circuit**: the AC source moved to the supply, a loop broken for loop gain, a step element added, a load box replaced by the next block's input model. These are separate decks (`next_power.md` §5.4 **[P]**).

---

## 3. What's realistic: ranges, fixed values and worst cases

### 3.1 Fixed test values vs "for all in a range"

| Datasheet (a characterization) | Verification plan (a guarantee for your product) |
|---|---|
| RL = 10 kΩ, CL = 60 pF, IOUT = 1 mA: **fixed points** | "for every load the next block can present", **a range** |
| chosen so parts from different vendors can be compared, and so production test is quick | chosen from what is actually connected, with its tolerances |
| typical values at 25 °C plus min/max over temperature | worst case (or 3σ) over every knob |

Both kinds appear in real designs:
- **"At the test load"** is right when the load is *known and fixed by the design*. The ADC is always there; the RC filter is always 470 Ω / 100 nF (± tolerance). Then the load is a part of the circuit, not a condition.
- **"For all loads in a range"** is right at a **product boundary or a plug-in interface**: a USB port, a headphone jack (16–600 Ω), an output connector, a regulator whose downstream load current varies with the MCU's activity (5–30 mA).

A useful habit: a block-level spec is checked over the *range the block promises to tolerate* (its `assume`); at board level, the range shrinks to what the neighbors *actually* present, so the check is stronger and the margin is visible.

### 3.2 Which conditions vary over ranges

| Condition | Typical range | Why it varies |
|---|---|---|
| Load current on a rail | 10 µA (sleep) … full load; **modes** | the firmware's activity. `next_power.md`: PM 92.9° at 300 mA vs 32.5° at 100 µA **[P]** |
| Load resistance | headphones 16–600 Ω; line input ≥ 10 kΩ | what the user plugs in |
| Capacitive load | 0 … a cable (100 pF/m), a bypass cap, a MOSFET gate | layout, cable length, the user's scope probe (10–15 pF) |
| Source impedance | sensors 100 Ω … 10 MΩ; cables 0.1–0.5 Ω | sensor type, aging (electrodes), cable |
| Supply voltage | USB 4.40–5.25 V; Li-ion 3.0–4.2 V; 12 V ± 10% | the source, cable drop, battery state |
| Supply ripple / droop | a DC value plus an AC range (amplitude over frequency) | the source's regulator, other loads |
| Signal amplitude and frequency | a band (20 Hz–1 kHz, ≤ 100 mV) | the application |
| Temperature | −40…85 °C industrial, 0…70 °C commercial | environment and self-heating |

Values that are usually **fixed**: test frequencies (1 kHz, 100 kHz), a step's size and edge rate in a load-transient test (1 mA → 500 mA at 1 A/µs), a bandwidth limit on a noise or ripple measurement (10 Hz–100 kHz; 20 MHz). These define *how* to measure, not the environment.

### 3.3 Worst-case loads are not always the heaviest

| Circuit | Worst load for which spec | Why | Numbers |
|---|---|---|---|
| LDO | **lightest** load, for stability / output-impedance peaking | the output pole moves down as the load gets lighter | Z_out peak **9.2 Ω at 12.6 kHz at 100 µA**; 3.2 Ω at 1 mA; 0.62 Ω at 30 mA; 0.18 Ω at 500 mA **[S]** |
| LDO | **lightest** final load, for overshoot after load release | an LDO can't sink current; only the load discharges C_out | P1: still at 3.43 V after 3 ms **[P]** |
| LDO | heaviest load, for dropout and dip | pass device headroom | 215 mV at 500 mA vs 13 mV at 30 mA **[S]** |
| Op-amp with C_L | a **middle** capacitance | the phase dip is interior | 47.6° at 944 pF vs 81.9°/83.9° at 0 and 100 µF **[P]** |
| Amplifier | heaviest resistive load, for swing and gain | output divider, current limit | §1.3 |
| Logic output | largest I_OH for V_OH; largest C_L for rise time | ohmic drop; RC | §2.7 |

So a spec's worst case over a load range can be at either end or inside. For a bounds engine this means load ranges must be real knobs (not fixed at "full load"), sometimes with a log axis, and sometimes split by mode.

### 3.4 Why capacitive loads are dangerous

An op-amp (or a regulator) is a feedback loop. It is stable if, at the frequency where the loop gain falls to 1, the phase shift around the loop is comfortably less than 180°. The margin is the **phase margin**, and 45°–60° is the usual target.

The output has an internal resistance R_o (open-loop, before feedback; about 300 Ω in the macromodel below). A load capacitance C_L makes a new low-pass pole with it, at f_p = 1/(2π R_o C_L). If f_p falls near the loop's crossover frequency, it adds up to 90° of phase lag. The phase margin collapses, the response peaks, a step rings, and at worst it oscillates.

```
 op-amp                        op-amp
  ─[R_o]──●── out               ─[R_o]──●──[R_iso 470Ω]──●── out
          │                             │                 │
         C_L  ← pole R_o·C_L            (loop sees ≈ R_iso)  C_L
          │                                               │
```

The fix is a small series resistor R_iso (the MCP6001's Figure 4-3 **[DS]**). Above a few kHz the loop then sees a resistor instead of a capacitor. Simulated on the §4 stage (MCP6001-class model, G = 10) **[S]**:

| Load on the op-amp output | Peaking in the closed-loop response | Step overshoot |
|---|---|---|
| 60 pF (the datasheet's C_L) | 0 dB | 0.8% |
| 1 nF direct | 0 dB | 0.8% |
| 10 nF direct | 3.1 dB at 63 kHz | — |
| **100 nF direct** | **12.5 dB at 22 kHz** | **69%** ❌ |
| 470 Ω then 100 nF (the §4 design) | 0.09 dB | 2.4% ✅ |

Gain 10 is the *forgiving* case: the loop crosses over at about 100 kHz. At gain 1 (a buffer) it crosses at 1 MHz, and the datasheet says > 100 pF already needs R_iso **[DS]**. The model reproduces that roughly: at G = 1 the peaking is 0 dB with 60 pF, 1.1 dB with 500 pF and 9.9 dB with 5 nF **[S]**. This is why "for all C_L in 0…X" is one of the most common load specs, and why it needs a proper interior search (`next_precision_analog.md` §2.A).

---

## 4. The shared reference example: a USB-powered sensor front end

### 4.1 Why this board

It is small (one IC regulator, one op-amp, a handful of passives, an ADC input model) and runs in milliseconds on ngspice. It still has every interface kind from §1:
- a **power source with impedance and disturbances** (USB VBUS through a cable);
- a **rail shared** by an analog block and a digital load that steps (the MCU);
- a **signal source with a range of impedances** (the sensor);
- an **input that loads its source** (the gain stage's bias resistor);
- an **output that must be stable into a capacitive load** (the op-amp into the RC);
- a **dynamic, nonlinear load** (the ADC's sampling capacitor);
- **specs that exist only at board level** (codes per mV, rail disturbance to code, start-up to valid data, USB compliance).

It reuses real parts' datasheet numbers (TLV755P, MCP6001, STM32F103's ADC), so the specs are realistic. The CE amp is *not* reused: at 3.3 V it has almost no headroom, and a real 3.3 V board would use a rail-to-rail op-amp. The CE two-stage amplifier stays the canonical *loading* demo (§1.3; `next_boards.md`).

### 4.2 Structure: blocks placing blocks

```
                         ┌──────────────────────── Board (SensorBoard) ─────────────────────────────┐
  USB host               │                                                                           │
  VBUS 4.40–5.25 V ──Rcab┼─●─ vbus ─[Ldo3v3]── v3 ──┬──────────────[VddaFilter]── va ──┐            │
  (cable 0.1–0.5 Ω)      │ C_bus 4.7µF              │                                   │            │
                         │                     I_mcu 5–30 mA (McuLoad)                  │ (analog    │
                         │                                                              │  rail)     │
  Sensor                 │     ┌──────────── GainStage ─────────────┐                   │            │
  v_s ≤100 mV pk ─R_s────┼──●──┤ C_in ─● inp ─(+)MCP6001─● oa        ├─[AaFilter]─● adc ─[AdcInput]  │
  R_s 100 Ω–10 kΩ        │ sin │       R_b        (−)─R_f─┘          │  470Ω/100nF      VREF = va    │
                         │     │       │           R_g─C_g─gnd       │                              │
                         │     │   [MidRef] vmid = va/2               │                              │
                         │     └──────────────────────────────────────┘                              │
                         └───────────────────────────────────────────────────────────────────────────┘
```

| Block | Places | Ports |
|---|---|---|
| `SensorBoard` (top) | `Ldo3v3`, `VddaFilter`, `GainStage`, `AaFilter`, `AdcInput`, `McuLoad`, C_bus | `vbus: Power<In>`, `gnd`, `sensor: Analog<In>`; the output is the ADC code (a measure) |
| `Ldo3v3` | a TLV755P-class part, C_in 1 µF, C_out 1 µF | `vin: Power<In>`, `vout: Power<Out>`, `gnd` |
| `VddaFilter` | R 10 Ω, C 10 µF | `vin: Power<In>`, `vout: Power<Out>` |
| `GainStage` | `MidRef`, MCP6001, C_in, R_b, R_f, R_g, C_g | `vdd: Power<In>`, `input: Analog<In>`, `output: Analog<Out>`, `gnd` |
| `MidRef` | 100 kΩ / 100 kΩ, 1 µF | `vdd: Power<In>`, `mid: Analog<Out>` (a bias, high impedance) |
| `AaFilter` | R 470 Ω, C 100 nF | `input: Analog<In>`, `output: Analog<Out>` |
| `AdcInput` | switch model R_sw + C_s, leakage | `input: Analog<In>`, `vref: Power<In>` |
| `McuLoad` | a current sink with steps (stands in for the MCU's digital supply current) | `vdd: Power<In>` |

The MCU's ADC is inside the MCU. It appears as a block here so that its input model can be a load in other blocks' benches.

### 4.3 Exact values

**Parts and values** (the ones marked ± are statistical knobs):

| Ref | Block | Value | Tolerance | Note |
|---|---|---|---|---|
| R_cab | USB (bench/board boundary) | 0.3 Ω | range 0.1–0.5 Ω | VBUS + GND cable and contacts |
| C_bus | board | 4.7 µF | ±10% | USB 2.0 allows ≤ 10 µF on VBUS at attach (§7.2.4.1) |
| U1 | Ldo3v3 | TLV755P-class 3.3 V | V_ref ±1% | model below |
| C_ldo_out | Ldo3v3 | 1 µF, ESR 5 mΩ | ±10% | datasheet minimum 0.47 µF effective |
| C_dec | board | 100 nF at v3 | ±10% | op-amp/MCU bypass |
| I_mcu | McuLoad | 5 mA … 30 mA; steps 5 → 30 mA with 1 µs edges | range | MCU wake-up |
| R_fa, C_fa | VddaFilter | 10 Ω, 10 µF | ±1%, ±10% | ST AN2834 recommends filtering VDDA |
| R_m1, R_m2 | MidRef | 100 kΩ, 100 kΩ | ±1% each | vmid = va/2 |
| C_mid | MidRef | 1 µF | ±10% | |
| C_in | GainStage | 220 nF | ±10% | input coupling |
| R_b | GainStage | 470 kΩ | ±1% | bias to vmid; **this is the stage's input impedance** |
| U2 | GainStage | MCP6001-class op-amp | V_OS ±4.5 mV, GBW 1 MHz | model below |
| R_f | GainStage | 9.09 kΩ | ±1% | gain = 1 + R_f/R_g = 10.09 |
| R_g | GainStage | 1 kΩ | ±1% | |
| C_g | GainStage | 47 µF | ±20% | makes the DC gain 1, so the output sits at vmid |
| R_aa | AaFilter | 470 Ω | ±1% | also the op-amp's R_iso |
| C_aa | AaFilter | 100 nF | ±10% | f_c = 3.39 kHz; C_aa/C_s = 12,500 (≫ 20) |
| R_sw, C_s | AdcInput | 1 kΩ, 8 pF | max values | STM32F103 R_ADC, C_ADC |
| — | AdcInput | 12 bits, V_REF = va, f_s = 50 kSPS, t_s = 13.5 cycles at 12 MHz = 1.125 µs, leakage ±1 µA | | 1 LSB = 3.3 V/4096 = 0.806 mV |

**Environment and interface knobs (ranges):**

| Knob | Range | Where it comes from |
|---|---|---|
| `vbus.v` | 4.40 … 5.25 V | USB 2.0 §7.2.2: low-power port 4.40–5.25 V (high-power 4.75–5.25 V) |
| `vbus` droop | a 330 mV step down, 1 µs edges | USB 2.0 §7.2.4.2 |
| `vbus` ripple | 100 mVpp, 100 Hz … 1 MHz | a design assumption (USB gives none) |
| `r_cab` | 0.1 … 0.5 Ω | cable |
| `sensor.z_src` (R_s) | 100 Ω … 10 kΩ | sensor family |
| sensor signal | sine, ≤ 100 mV peak, 20 Hz … 1 kHz | application; 100 mV × 10.09 = 1.01 V fits 1.65 ± 1.6 V |
| `i_mcu` | 5 … 30 mA, with steps | firmware |
| `temp` | 0 … 70 °C | commercial (not modeled in the behavioral parts; resistor tempcos can be added) |

**Behavioral models** (as simulated; `parts.py`):

```spice
* TLV755P-class LDO: PMOS pass, PI error amp, soft start, 30 mOhm package resistance
.subckt ldo vin vout gnd
Vref r0 gnd DC {vref}            ; vref = 1.2025 V nominal, ±1% knob
Rss r0 ref 500k                  ; soft start, tau 110 us
Css ref gnd 220p
Rt vd fb 175k                    ; divider -> 3.3 V
Rb fb gnd 100k
Gea gnd ea ref fb 1m             ; error amp gm into a PI network (DC gain 220, HF gain 20)
Rea1 ea x 20k
Rea2 x gnd 200k
Cea x gnd 1n
Dc1 ea vin DCL                   ; anti-windup clamp
Dc2 gnd ea DCL
.model DCL D(IS=1e-12)
Bg gd gnd V = 0.93*v(vin) - max(0, min(v(vin), v(ea)))   ; 0.93 sets PSRR near the datasheet's
Rg gd g 100
Cg g vin 1n
Cds vin vd 20p
M1 vd g vin vin PP W=1 L=1
Rpkg vd vout 30m                 ; package + trace, sets load regulation near 0.06 V/A
.model PP PMOS(LEVEL=1 VTO=-0.8 KP=0.9 LAMBDA=0.02)
Ignd vin gnd 25u
.ends

* MCP6001-class op-amp: GBW 1 MHz, Aol 100 dB, Ro 300 ohm, RRO within 25 mV, PSRR 86 dB, Iq 100 uA
.subckt opamp inp inn vdd vss out
Bps ip vss V = v(inp) - v(vss) + 5e-5*(v(vdd)-v(vss)-3.3)
G1 vss n1 ip inn 1m
R1 n1 vss 1e8
C1 n1 vss 159p
Bo nx vss V = max(0.025, min(v(vdd)-v(vss)-0.025, v(n1)))
Ro nx out 300
Iq vdd vss 100u
.ends
```

The models are calibrated against the datasheets' headline numbers (§2.4 table: load regulation 56 vs 60 mV/A, dropout 215 vs 215 mV max, PSRR within 6 dB). They are not vendor models. They have no noise, no current limit, and no temperature dependence. Syntax designers can treat them as two `Part` kinds with subcircuit models.

**Board netlist** (`board.py` + the VDDA filter from `final.py`):

```spice
VUSB usb 0 DC {vbus} AC {vbus_ac}
Rcab usb vbus {r_cab}
Cbus vbus 0 4.7u
X1 vbus v3 0 ldo
Cout v3 co 1u
Resr co 0 5m
Cdec v3 0 100n
Imcu v3 0 DC {i_mcu}
Rfa v3 va 10
Cfa va 0 10u
Rm1 va vmid 100k
Rm2 vmid 0 100k
Cmid vmid 0 1u
VS s 0 DC 0 AC 1          ; the sensor's EMF
Rs s sin {r_s}
Cin sin inp 220n
Rb inp vmid 470k
X2 inp inn va 0 oa opamp
Rf oa inn 9.09k
Rg inn ng 1k
Cg ng 0 47u
Raa oa adc 470
Caa adc 0 100n
Cadc adc 0 8p             ; ADC pin, averaged (use the switch model of §2.6 for settling)
* ADC code = 4096 * v(adc) / v(va)
```

### 4.4 Block-level specs and their setups

Each block's specs are checked on its **own bench**: its ports driven by the sources and loaded by the loads its `assume`s declare. The "Setup" column is what that bench must contain.

**Ldo3v3**

| Spec | Bound | Setup | Simulated [S] |
|---|---|---|---|
| `vout_dc` | 3.3 V ± 2% | DC; VIN 4.40–5.25 V, I_OUT 0.1–50 mA (the ranges it promises), V_ref ±1% | 3.2967–3.3013 V (VIN 4.40–5.25 V, 0.1–30 mA, nominal V_ref) |
| `line_reg` | ≤ 5 mV over VIN | DC sweep of VIN at 30 mA | 1.0 mV |
| `load_reg` | ≤ 0.1 V/A | DC sweep of I_OUT 0.1 → 500 mA | 56 mV/A |
| `dropout` | ≤ 250 mV at 500 mA | DC sweep of VIN down, crossing at 1% | 215 mV |
| `psrr` | ≥ 36 dB at 100 kHz (the datasheet's 46 dB typical, derated) | AC 1 V on VIN, VIN = 4.3 V, I_OUT = 50 mA | 40.0 dB |
| `load_step` | dip ≤ 50 mV for 5 → 30 mA in 1 µs | transient, current-sink step | −19.9 mV / +16.4 mV |
| `line_step` | ≤ 20 mV for a 1 V step in 1 µs | transient, VIN step | +9.3 / −8.3 mV |
| `start` | V_OUT ≥ 3.2 V within 1 ms of VIN, no overshoot above 3.4 V | transient from 0, VIN ramp 100 µs | 385 µs, peak 3.299 V |
| `z_out` (guarantee) | ≤ 1 Ω, 10 Hz–1 MHz, for I_OUT ≥ 1 mA | AC current injection | 3.2 Ω peak at 1 mA ❌, 0.62 Ω at 30 mA ✅ |

The last row is instructive. As a *block* guarantee "for I_OUT ≥ 1 mA" it fails, because the light-load peak is high (§3.3). At board level the rail never carries less than 5 mA (the MCU), so the board-level bench passes. The block spec should be narrowed (`where i_out >= 5mA`), or the board relies on its actual load range. That is the "range shrinks in context" point of §3.1.

**GainStage** (standalone bench: ideal 3.3 V rail, ideal source, load as assumed)

| Spec | Bound | Setup | Simulated [S] |
|---|---|---|---|
| `gain` (pin-to-pin, input pin → output pin) | 10.09 ± 3% at 1 kHz | AC; source R_s = 0; **load: assume `output.z_load` = 470 Ω + ≥ 0 series C, or ≥ 10 kΩ; `output.c_load` ≤ 1 nF direct** | 10.089 (RL 10 kΩ ∥ 60 pF), 10.106 (470 Ω + 100 nF) |
| `f_low` | ≤ 5 Hz | AC sweep | 3.7 Hz (board, with R_s) |
| `f_high` | ≥ 50 kHz | AC sweep, the real load | 60.6 kHz (470 Ω + 100 nF); 94.6 kHz (10 kΩ) |
| `z_in` (guarantee) | ≥ 450 kΩ, 20 Hz–1 kHz | AC; measure the current into the input | 470–473 kΩ |
| `z_out` (guarantee) | ≤ 5 Ω at 1 kHz | AC injection | 3.0 Ω |
| `stable` | overshoot ≤ 10% for every allowed load | transient step, load range as a knob | 2.4% (470 Ω + 100 nF); 0.8% (1 nF); **69% (100 nF direct)**, outside the allowed load |
| `swing` | output within 0.1…3.2 V without clipping | transient, 1 kHz sine, heaviest load | the model swings to 25 mV from the rails |
| `psrr` | ≥ 60 dB | AC on the rail | 66 dB |

**AaFilter**

| Spec | Bound | Setup | Simulated [S] |
|---|---|---|---|
| `f_c` | 3.39 kHz ± 15% | AC; **assume `input.z_src` ≤ 10 Ω**; load = the ADC model | 3.73 kHz end to end (the op-amp's Z_out and GBW add) |
| `alias` | ≥ 15 dB attenuation at 25 kHz (f_s/2) relative to 1 kHz | AC | 17.7 dB |
| `drives_adc` | settling error ≤ 1 LSB at the end of the sample | transient with the sample clock, source = the op-amp stage | −0.47 LSB |

**AdcInput** is a part with a model and assumptions, not specs: `assume input.z_src` small enough (the R_AIN formula), `assume vref.v in 3.3V ± 3%`, and a guarantee of its input behavior (R_sw ≤ 1 kΩ, C_s ≤ 8 pF, ±1 µA leakage) that other blocks' benches use as their load.

### 4.5 Board-level specs and their setups

The board's input is the sensor EMF and USB power. Its output is the ADC **code**, code = 4096 × v(adc)/v(va).

| Spec | Bound | Setup | Simulated [S] | Board-only? |
|---|---|---|---|---|
| `code_gain` | 12.0 codes/mV ± 7%, 20 Hz–1 kHz, for all R_s in 100 Ω–10 kΩ | AC from the sensor EMF; all tolerances | nominal 12.01 (R_s 1 kΩ, 1 kHz); **11.35–12.71 over 256 corners × {100 Ω, 10 kΩ} × {20 Hz, 1 kHz}** ✅ (corners run without the VDDA filter; it changes V_REF by 1.2 mV, 0.04%) | **yes**: combines the source loading (R_b vs R_s), the stage gain, the filter droop at 1 kHz and V_REF |
| `midscale` | 2048 ± 20 codes with no signal | DC; V_OS, resistor tolerances, leakage | arithmetic below: ±16.8 codes worst case ✅ | yes (it's ratiometric: V_REF tolerance cancels) |
| `ripple_to_code` | ≤ 1 LSB pp for 100 mVpp VBUS ripple, 100 Hz–1 MHz | AC 1 V on VUSB, sensor AC 0; code = 1241·(v(adc) − ½·v(va)) linearized | peak 0.08 LSB (at 1–10 kHz) with the VDDA filter; 0.47 LSB without ✅ | **yes**: the path is LDO PSRR → V_REF, which no block spec covers |
| `mcu_step_to_code` | ≤ ±4 codes during a 5 → 30 mA MCU step | transient; step on `Imcu` | **with the VDDA filter: 2047.6–2050.0 (−0.4/+2.1)** ✅. Without it: **2038.0–2060.3 (−10/+12)** ❌ | **yes** |
| `droop_to_code` | ≤ ±4 codes during a 330 mV VBUS droop | transient on VUSB at VBUS = 4.40 V | 2047.9–2048.3 with the filter (2046.5–2049.7 without) ✅ | yes |
| `alias_rejection` | ≥ 15 dB at 25 kHz | AC | 17.7 dB ✅ | shared with AaFilter |
| `t_valid` | code within ±4 of midscale ≤ 1.5 s after plug-in | transient from 0; VBUS ramps 0 → 5 V in 100 µs | 2185 at 50 ms, 3959 at 100 ms, 2293 at 0.5 s, 2052.1 at 1.0 s, **2048.05 at 1.49 s** ✅ | yes (all the RC time constants together, dominated by C_g × (R_f + R_g) = 0.47 s) |
| `usb_cap` | C on VBUS ≤ 10 µF | structural (sum of caps on the net) | 4.7 µF + LDO C_in ✅ | yes |
| `usb_current` | ≤ 100 mA before configuration (1 unit load, USB 2.0 §7.2.1) | DC at max I_mcu | 10.15 mA at I_mcu = 10 mA; ≈ 30.2 mA at 30 mA ✅ | yes |
| `inrush` (informative) | peak current at plug-in | transient, VBUS ramp | 424 mA with a 100 µs ramp (a real hot-plug edge is faster; the cable's inductance limits it) | yes |

**`midscale` budget** (arithmetic from the datasheet numbers; 1 LSB = 0.806 mV):

| Source | Error at the ADC | Codes |
|---|---|---|
| Op-amp V_OS ±4.5 mV × DC gain 1 (C_g blocks DC gain) | ±4.5 mV | ±5.6 |
| MidRef mismatch, R_m1 and R_m2 ±1% | vmid ±0.5% = ±8.25 mV | ±10.2 |
| ADC leakage ±1 µA × R_aa 470 Ω | ±0.47 mV | ±0.6 |
| ADC average current 0.66 µA × 470 Ω | −0.31 mV | −0.4 |
| **Total (worst-case sum)** | | **±16.8** |

The V_REF tolerance doesn't appear: vmid comes from va, and the ADC measures against va, so it cancels (ratiometric design). It *does* appear in `code_gain` (a 1% higher V_REF means 1% fewer codes per mV).

### 4.6 Which block specs still hold in context, and which don't

| Block spec | In context | Why |
|---|---|---|
| `Ldo3v3.vout_dc`, `line_reg`, `load_reg` | **hold** | the board's load (5–30 mA) and VBUS range are inside the LDO's assumed ranges |
| `Ldo3v3.z_out ≤ 1 Ω for I ≥ 1 mA` | fails as written; **holds for the board's actual range** (≥ 5 mA) | the block's range was wider than needed; narrow it with `where` |
| `GainStage.gain` (pin-to-pin) | **holds** | its load (470 Ω + 100 nF) is inside its assumption; source impedance doesn't change pin-to-pin gain |
| gain from the **sensor EMF** | not a block spec; **changes with R_s** | R_b 470 kΩ vs R_s 10 kΩ: −2.1% (9.490 vs 9.690 at the ADC pin). With R_b = 100 kΩ: **−9.0%** (8.811). With 1 MΩ: −1.0% (9.596) |
| `GainStage.stable` | **holds** | the board connects 470 Ω + 100 nF, inside the allowed loads. Remove R_aa and it fails (69% overshoot) |
| `AaFilter.f_c` | **shifts** (3.39 → 3.73 kHz) but stays in ±15% | its source isn't ideal: the op-amp's GBW and Z_out |
| `AaFilter.drives_adc` | **holds only with the real driver and the real ADC model** | a DC or AC bench can't see it (§2.6) |
| `AdcInput`'s assumptions | **hold** | its source is 470 Ω with 100 nF (R_AIN rule met) |

**Specs that exist only at board level:** `code_gain` (from sensor EMF to code), `midscale`, `ripple_to_code`, `mcu_step_to_code`, `droop_to_code`, `t_valid`, `usb_cap`, `usb_current`. Every one of them is about **two or more blocks sharing something**: a node (loading), a rail (the MCU step), or a reference (ratiometry). This is the reason sub-contracts alone can't prove a board.

### 4.7 What the simulations show about setups

1. **Loading is a connection property.** The CE amp's gain drops from 4.59 to 2.88 in context. The formula V_th · Z_in/(Z_in + Z_out) predicts it to 4 digits.
2. **The input-side "misbehavior" is the source.** Sensor impedance is a source knob (R_s). It changes EMF-referred gain (−2.1% at 10 kΩ) and nothing pin-to-pin.
3. **The same AC response can be a pass or a 45-LSB fail.** Three filters with the same corner differ 100× in ADC settling. The ADC input is a dynamic load that only a transient bench with the sample clock can express.
4. **Capacitive load needs the load range, not a single value.** 1 nF passes, 10 nF peaks by 3 dB, 100 nF rings at 69%. With R_iso = 470 Ω, 100 nF is fine.
5. **Light load is the LDO's worst case for stability**: Z_out peaks at 9.2 Ω at 100 µA against 0.18 Ω at 500 mA.
6. **Shared rails create board-only failures.** An MCU load step moves the rail by 20 mV, which moves the ADC's reference but not the signal, which is 12 codes. A 10 Ω + 10 µF filter (1.6 kHz) fixes it. No block bench would find it, because no block owns both the MCU load and the ADC reference.
7. **Ratiometry changes which knobs matter.** V_REF's ±1% drops out of the midscale spec but stays in the gain spec.

---

## 5. What this suggests for the contract language

These are observations for the syntax designers. They are not decisions.

1. **On inputs, describe the source; on outputs, describe the load.** An input's condition is a *source*: a waveform family (DC, sine, band, step, ramp) plus disturbances (ripple, droop, noise, edge rate, overshoot) plus an impedance range. An output's condition is a *load*: R, C, current (static range, step), or another block's input model. The word "load" on an input is almost never what an engineer means.
2. **A port's own impedance is a guarantee, not a condition.** `z_in` on inputs and `z_out` on outputs are specs of the block and assumptions of the neighbor, checked at the connection. `language.md` §3.2 lists `z_load` and `z_src`, the *neighbor's* values. The block's own `z_in`/`z_out` need a home too, so that §1.3's divider can be checked without simulating the neighbor.
3. **A setup is "the header plus overrides".** This matches every datasheet read here (TLV755P, MCP6001, TPS62160, 74LVC1G04). Most specs use the header; a few override one field (IOUT = 50 mA); a few change the circuit (AC on a supply, a loop break, a step element, a real load model). Counting per block: 1 header, 2–12 overrides, 1–6 distinct circuits.
4. **Ranges vs points both occur.** Keep "for all in the assumed range" as the default (§8.6 already), and allow a spec to pin a condition to a test value (`where i_out == 50mA`), which is how datasheets define PSRR or noise.
5. **Loads are often other blocks.** The ADC input model is a load in the filter's bench; the next stage's `z_in` is a load in this stage's bench. A setup should be able to say "load = that block's input model" as well as "load = 10 kΩ".
6. **Board-level specs are about shared things** (a node, a rail, a reference). They need benches with several stimuli at once: sensor AC plus a VBUS disturbance plus an MCU step.
7. **Mode and range conditions go together.** `i_mcu` in 5–30 mA while awake; a sleep mode would change it and would change the LDO's stability (`next_power.md` P1).

---

## 6. Glossary (for this report)

| Term | Meaning |
|---|---|
| Source impedance, Z_out, `z_src` | how much a source's voltage drops per unit current drawn; the series resistor in its Thévenin model |
| Input impedance, Z_in | the impedance an input presents to whatever drives it |
| Load | whatever an output drives: a resistor, capacitor, current sink, or another block's input |
| Thévenin / Norton | two equivalent models of a linear port: voltage + series Z, or current + parallel Z |
| Bridging | Z_in ≫ Z_out (≥ 10×), so the connection barely divides the signal |
| Pin-to-pin gain | v(out)/v(in) measured at the block's own pins |
| EMF-referred gain | v(out)/(the source's open-circuit voltage) |
| Phase margin | how far a feedback loop is from oscillating; 45–60° is the usual target |
| R_iso | a small series resistor that isolates an amplifier from a capacitive load |
| PSRR | power-supply rejection: how much of the supply's ripple reaches the output (larger dB is better) |
| Line / load regulation | output change for a change in input voltage / load current |
| Dropout | the smallest VIN − VOUT at which a regulator still regulates |
| LSB | the smallest ADC step: V_REF / 2^N (0.806 mV here) |
| Ratiometric | the signal and the ADC reference come from the same rail, so the rail's error cancels |
| Kickback | the charge an ADC's sampling capacitor takes from its driver at each sample |

---

## 7. Sources

**Datasheets and standards read for this report:**
- TI **TLV755P** 500 mA LDO, SBVS320D (Nov 2017, rev. Sep 2024): §5.5 electrical characteristics and header conditions; Figs. 5-1…5-3 PSRR, 5-7 line transient (1 V/µs), 5-8 load transient (1 mA → 500 mA at 1 A/µs), 5-9/5-11 start-up; §7 output capacitor ≥ 0.47 µF. https://www.ti.com/lit/ds/symlink/tlv755p.pdf
- Microchip **MCP6001/1R/1U/2/4** 1 MHz op-amp, DS20001733L: DC/AC tables (header: RL = 10 kΩ to VDD/2, CL = 60 pF), Figure 1-1 test circuit, §4.3 capacitive loads, Figs. 4-3/4-4 R_ISO. https://ww1.microchip.com/downloads/en/DeviceDoc/MCP6001-1R-1U-2-4-1-MHz-Low-Power-Op-Amp-DS20001733L.pdf
- TI **TPS62160** 1 A step-down converter, SLVSAM2E: §7.5 electrical characteristics, Figs. 8–31 (efficiency, ripple, load transients 500 mA → 1 A and 100 → 500 mA, start-up), §9.2 typical application (2.2 µH, 22 µF). https://www.ti.com/lit/ds/symlink/tps62160.pdf
- TI **SN74LVC1G04**, SCES214AF: §5.5 V_OH/V_OL at I_OH/I_OL, §5.6–5.9 switching at C_L = 15 pF and 30/50 pF, §6 load circuits (15 pF ∥ 1 MΩ; 50 pF ∥ 500 Ω at 3.3 V). https://www.ti.com/lit/ds/symlink/sn74lvc1g04.pdf
- ST **STM32F103x8/xB** datasheet, DS5319, §5.3.18 ADC characteristics (R_ADC ≤ 1 kΩ, C_ADC ≤ 8 pF, sampling time, R_AIN equation, ±1 µA injection/leakage). https://www.st.com/resource/en/datasheet/stm32f103c8.pdf (the preliminary Rev. 1 read for this report lists C_ADC = 5 pF; current revisions give 8 pF, which is the value used).
- ST **AN2834**, "How to get the best ADC accuracy in STM32 microcontrollers" (VDDA/VREF+ filtering and decoupling).
- A. Walsh, "Front-End Amplifier and RC Filter Design for a Precision SAR Analog-to-Digital Converter", *Analog Dialogue* 46, 2012 (C_EXT ≥ 20 × C_DAC; settling to ½ LSB). https://www.analog.com/en/resources/analog-dialogue/articles/front-end-amp-and-rc-filter-design.html
- **USB 2.0 Specification**, §7.2.1 (unit load 100 mA, 500 mA max after configuration), §7.2.2 / Table 7-7 (VBUS 4.75–5.25 V high-power port, 4.40–5.25 V low-power port), §7.2.4.1 (≤ 10 µF at attach), §7.2.4.2 (droop ≤ 330 mV).
- R. D. Middlebrook, "Measurement of loop gain in feedback systems", *Int. J. Electronics* 38(4), 1975 (the injection method for loop gain).

**Earlier reports in this repo:** `research/next_boards.md` (two-stage CE: 2.8823 vs 4.5908, 12.27–13.93 vs 19.1–23.3), `research/next_power.md` (LDO modes, load steps, buck levels), `research/next_precision_analog.md` (phase margin vs C_L interior dip, CMRR on the default bench, Sallen-Key), `research/next_synthesis.md` (L1–L6).
