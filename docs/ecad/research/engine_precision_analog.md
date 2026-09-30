# The Bounds Engine vs. Precision Analog and Signal Chains

> Research report · 2026-09-25 · stress-tests `../archive/engine_v2.md`, `../walkthrough.md` and `../specs.md` against real precision circuits.
> Keys like **[N&P07]** point into `../bibliography.md`. New references are in §8, each with its verification status.
> **How the numbers were produced:** eight small pure-Python experiments (numpy isn't installed, so closed-form or tiny MNA models, brute-force corners, dense sampling, and finite differences as the answer key). Appendix A lists each experiment and its setup. Datasheet numbers marked ✓ were read from the manufacturer document during this session. Numbers marked *(rep.)* are representative placeholders, not tied to a verified part.

---

## 0. Executive summary

Precision analog is, in one sense, the **best case** for the v2 design. Tolerances are tiny (0.01–0.1%), most signal paths are linear, and the error terms are small and additive. The affine form *is* the error-budget spreadsheet engineers already keep. Pushing on real circuits still exposed gaps, ranked here by impact:

1. **Part models can't carry the knobs.** Vendor op-amp models "target typical electrical characteristics at room temperature" (TI SBOA338 ✓). They have no Vos/Ib spread, no temperature dependence, and no distortion or settling-tail fidelity. The OPA189 datasheet ✓ gives GBW, slew, noise density, settling and THD as **typical only**. Today the engine's temperature knob would be *silently blind* inside such a model: it would report "temperature doesn't matter". **Fix:** an error-source shell around vendor models, a parametric op-amp primitive whose parameters are knobs, per-model *fidelity metadata*, and detection of "zero sensitivity because unmodeled".
2. **The simulator can't run these circuits yet.** The parser knows only R, C, L, D, Q, V, I and X (`crates/spicy_netlist/src/reader/netlist_types.rs:130-139`). There are **no controlled sources (E/G/F/H), no behavioral sources, no noise analysis, no temperature, no switch**, and AC stamps only R/C/L/sources. Every op-amp circuit in this report is currently unsimulatable. The v2 corner theorem's own examples assume controlled sources.
3. **Calibration and trim are missing as a concept.** Almost every precision budget is judged *after* calibration. With named-knob affine forms, calibration becomes an exact subtraction, something only this representation can do well. In the bridge budget (§3.3) calibration removes the 25 °C error completely and leaves a tempco-dominated 3480 ppm FS worst case. Without it the engine answers the wrong question.
4. **"Tempco is a range term" (v2 §A7) is wrong for sign-random TCRs, and ECSS doesn't say it.** ECSS-Q-HB-30-01A Table 5-1 lists temperature variation as "Biased (**sometimes random**)", and §5.1.1.5b says "there can also be a random part" ✓. A ±25 ppm/K thin-film TCR has no preferred sign: a *statistical* knob multiplied by a *range* variable. In the bridge budget, the v2 rule gives 2968 ppm vs 1817 ppm (RSS). And multiplying ε_T·ε_TCR inside AA destroys TCR-tracking cancellation. **Fix:** slice over range knobs and never multiply range × statistical symbols in AA.
5. **AC, for-all-f and range-knob humps are common, and corner-snapping misses them.**
   - Op-amp with a capacitive load and isolation resistor: the worst phase margin is at **CL = 575 pF inside [0, 1 µF]**. PM there is 45.7°, versus 72° and 84.5° at the ends.
   - 0.5 dB Chebyshev: linearizing max-over-f at the nominal argmax predicts 0.68 dB. One loop step reaches 0.95 dB; the truth is **1.11 dB**. Tracking each ripple peak separately gives 1.12 dB.
   - RFI-filter CM→DM conversion peaks near 10 kHz.

   **Fix:** make the worst-point search a box-constrained optimization that allows interior knob values, and track each local extremum (Danskin).
6. **Noise is a third kind of uncertainty** (over time, within one board), not range or statistical. Its σ is a deterministic function of the knobs. Its sensitivities cost **two sparse solves per frequency for all parameters** (verified numerically against finite differences to about 1e-6). Specs need per-reading semantics (|static| + z·σ) that are distinct from population RSS.
7. **ppm accuracy.**
   - Linear op-amp circuits solve exactly (float error ≤ 1.4e-16 even with A_OL = 1e9).
   - Nonlinear circuits at Newton reltol 1e-3 are off by **up to 13 ppm** (median 0.46 ppm). That's the same size as the effect of a 0.01% resistor (4 ppm). At reltol 1e-6 the error is below 1e-4 ppm.
   - Fixed-step Backward Euler misstates a 13τ settling residual by 1.9× at h = τ/10.
8. **Datasheet semantics need first-class knob shapes:**
   - provenance (tested limit vs characterized vs typical-only);
   - envelopes over temperature (Ib max: 300 pA at 25 °C, but 10 nA over −40…125 °C ✓);
   - box-method tempco;
   - aging laws: linear (ECSS default), √t, or ∛t (Vishay thin film ✓). Over 10 years these differ by up to **20×**;
   - bias + random decomposition;
   - network ratio and TCR-tracking definitions.

   Default "σ = tol/3" is off by 2.5× for a guard-banded tested limit (OPA189 Vos: typ 0.4 µV, max 3 µV; TI says typ ≈ 1σ).

What *does* scale fine: an 8-channel bridge DAQ has about **450 knobs**. Adjoint sensitivities, affine forms and even Monte Carlo on it take well under a second. The scaling problems are knob authoring, presentation, and the combinatorics of setups × range corners × sweeps, not arithmetic.

---

## 1. What the v2 design already gets right here

- **Error budgets are affine forms.** An engineer's spreadsheet row ("RG TCR: 25 ppm/K × 60 K × sensitivity 1") is exactly `coefficient × ε`. Named knobs give correct cancellation for free in three places:
  - ratiometric excitation and reference;
  - network lot knobs, which cancel exactly in CMRR (§3.3);
  - channel-to-channel matching (shared reference and ADC gain cancel).

  No spreadsheet does that reliably.
- **Linearization is good.** With 0.1% parts, AA's error bucket is second order: about (10⁻³)² = 1 ppm. With 0.01% networks it's about 0.01 ppm. The worst-point loop converges in about one round for most DC specs.
- **Specs point at the circuit** (A6). In precision circuits the killer errors are interactions nobody writes equations for:
  - bias current through filter resistors;
  - burnout-detection current through the input network;
  - leakage into a high-Z node.

  The netlist contains them; hand equations don't.
- **Range vs statistical split** (A7), with the refinements in §4.9.

---

## 2. Reality check: what the simulator can do for these circuits today

| Needed for precision analog | Status in `crates/spicy_simulate` | Consequence |
|---|---|---|
| Controlled sources E/G/F/H | **Absent** (DeviceType has R, C, L, D, Q, V, I, X only) | No op-amp model of any kind, ideal or vendor |
| Behavioral sources (B, `TABLE`, `LIMIT`) | Absent | Vendor macromodels (GL/GWL style) won't parse |
| MOSFET/JFET | Absent | Transistor-level and many vendor models fail |
| AC on an operating point | Only R, C, L, sources (`ac.rs:92-104`) | No AC through any active device |
| AC solver | Dense 2n×2n real expansion, LU via `ndarray-linalg` (`ac.rs:76-126`, `ac.rs:141-142`), not KLU | O(n³) per frequency; needs sparse complex KLU for sweeps × corners |
| Noise analysis | Absent | No integrated noise, no SNR, no noise-free bits |
| Temperature | `tc1`/`tc2` parsed, unused | No drift, no tempco, no Ib(T) |
| Switch / time-varying element | Absent | Can't model SAR sampling kickback |
| Transient error control | Fixed step, BE or trapezoid (`trans.rs`) | Settling to ½ LSB not trustworthy (§4.8) |
| Newton tolerance | reltol 1e-3, abstol 1e-6 (`lib.rs:47-48`) | Nonlinear results to ~1–13 ppm (§4.8) |
| Pole-zero, loop-gain probe | Absent | No Q/f0 or phase-margin measures |

**Implication for build order:** Part D of v2 lists parameter layer, transposed solve, accuracy, AC on the operating point, step control and temperature. For this domain it must also include **controlled sources, an op-amp primitive, and noise**. They come before any precision spec can be checked.

---

## 3. Circuit by circuit

Each subsection gives realistic specs, the knobs that matter, a walkthrough of the engine, and where it breaks.

### 3.1 Op-amp gain stage with a realistic op-amp

**Circuit.** A non-inverting amplifier, G = 100 (Rf = 99 kΩ, Rg = 1 kΩ, 0.1%, ±25 ppm/K thin film), driving an 18-bit ADC. Op-amp: OPA189 ✓.

| OPA189 parameter ✓ | Value | Provenance |
|---|---|---|
| Vos | ±0.4 µV typ, ±3 µV max (25 °C); ±4 µV max (−40…125 °C) | tested / characterized |
| dVos/dT | ±0.005 typ, ±0.02 µV/°C max | characterized |
| Ib | ±70 pA typ, ±300 pA max @25 °C; ±1 nA max 0…85 °C; **±10 nA max −40…125 °C** | envelope over T |
| Ios | ±140 pA typ, ±600 pA max; ±1.6 nA 0…85 °C; ±3 nA −40…125 °C | envelope |
| A_OL | 150 dB min (140 dB min over T) | tested |
| CMRR | 146 dB min (±18 V) | tested |
| GBW / UGB / slew | 14 MHz / 8 MHz / 20 V/µs | **typical only** |
| en, 0.1–10 Hz, in | 5.2 nV/√Hz, 17 nVrms (0.1 µVpp), 165 fA/√Hz | **typical only** |
| Settling 0.01%, THD+N | 1.1 µs, 0.00006% | **typical only** |

**Specs engineers write.**

| Spec | Typical number |
|---|---|
| Gain error | ≤ 0.05% (or calibrated) |
| Gain drift | ≤ 5 ppm/K |
| Offset | ≤ 50 µV RTO after cal; drift ≤ 0.5 µV/K RTO |
| Noise | ≤ 1 µVrms RTI in 0.1 Hz–1 kHz |
| Nonlinearity | ≤ 5 ppm FS (to sit under the ADC's ±2 LSB = 7.6 ppm INL, ADS8881 ✓) |
| Swing / headroom | at every corner |

**Engine walkthrough.**

| Spec | Engine path | Numbers |
|---|---|---|
| Gain error (DC) | Linear → corners (exact) | Ratio of two 0.1% resistors: ±0.2% worst. A_OL term G/A_OL = 100/3.16e7 = **3.2 ppm**; needs A_OL as a knob (min spec) |
| Offset RTI | Linear in Vos, Ib, Ios (superposition) → exact | 3 µV (25 °C) + Ib·(Rf‖Rg = 990 Ω). At 25 °C: 0.3 µV. **At 125 °C: 9.9 µV, more than Vos.** Zero-drift Ib over temperature dominates |
| Gain drift | Slice over T; TCR is statistical in sign (§4.9) | Discrete Rf/Rg: 50 ppm/K worst, 35 ppm/K at 3σ (RSS). Same network (tracking ±2–5 ppm/K): 2–5 ppm/K |
| Nonlinearity (self-heating) | **Needs electro-thermal simulation**; value depends on its own power | At 10 V out, P(Rf) = 0.99 mW. At 393 K/W (0805, from TNPW derating ✓) ΔT = 0.39 °C → **9.7 ppm** gain shift at FS (∝ V²). With Rf = 9.9 kΩ (chosen for low noise): **97 ppm**. Either one is at or above the 18-bit INL |
| Noise | Noise analysis (absent) | 5.2 nV/√Hz ⊕ 4kTR of Rg (4.1 nV/√Hz for 1 kΩ): typ only |

**Where it breaks.**
- The vendor model has Vos and Ib at typ, 25 °C only (§4.1): the 125 °C Ib term, the biggest DC error, is invisible.
- Self-heating isn't modeled at all. It turns a linear circuit into a nonlinear, *signal-dependent* one, with a thermal settling tail on the order of the resistor's thermal time constant.
- A_OL enters gain error at ppm level but is only a min spec, with a large spread above it.

### 3.2 Active filters: Sallen-Key and MFB

**Specs.**
- f0 ±1–2%.
- Passband flatness or ripple (±0.05 dB for audio or anti-alias; 0.5 dB Chebyshev ripple ± 0.1 dB).
- Peaking ≤ 0.5 dB.
- Stopband ≥ 80–100 dB at fs − f_band.
- Group-delay flatness for data.
- Q within ±5%.
- Noise and distortion (C0G only in the signal path).

**Knobs.**
- R: 0.1–1%, TCR.
- C: C0G ±1–5%, 0 ± 30 ppm/K.
- Op-amp GBW (typ only), Zout(f) (typ curve), en.

**Experiments (Appendix A, E1).**

*MFB band-pass, Q = 10, f0 = 1 kHz, peak gain 1* (R1 = 159.2k, R2 = 800 Ω, R3 = 318.3k, C = 10 nF; R ±1%, C ±5%):

| Quantity | Range over the box |
|---|---|
| f0 | 943 … 1063 Hz (S(f0, C) = −½, S(f0, R2) = −0.50) |
| Q | 9.89 … 10.10 (MFB Q is insensitive) |
| \|H(1 kHz)\|, true (corners + edges + 200k samples) | **0.619 … 1.071** |
| \|H(1 kHz)\|, affine from nominal slopes | 0.930 … 1.070 (low side **5× too optimistic**) |

- **A fixed-frequency gain spec near a high-Q resonance is ill-conditioned.** f0 moves ±6% and Q ±1%, yet the gain at 1 kHz drops by up to 38%. The linearization misses it, because the detuning loss is *quadratic* around the peak. Express the intent as f0/Q specs, which are smooth and linearize well, or let the loop simulate.
- **Corners vs interior.** The max over the full 5-D box lies on an *edge*, not a vertex: 1.0712 vs the best corner's 1.0708 (+0.03%). Other knobs can retune the resonance, so the vertex error is small here. In a **1-D slice** (one knob moves the resonance: a drifting capacitor, or the test frequency itself) the extreme is interior: C1 at −5/−2.5/0/+2.5/+5% gives \|H\| = 0.913/0.982/**1.000**/0.959/0.877. This is the [Bartlett, Tesi & Vicino 1993] "edges, not corners" situation from the bibliography.

*Equal-component Sallen-Key, Q = 5 (K = 3 − 1/Q = 2.8, Rf/Rg = 1.8):* S_K^Q = K·Q = **14**.

| Gain-resistor tolerance | Q range |
|---|---|
| 1% | 4.24 … 6.11 (−15% / +22%) |
| 0.1% | ±1.8% |
| 0.01% | ±0.2% |

The engine should surface *sensitivity products* (Moschytz's gain-sensitivity product ✓) as design warnings, not just verdicts.

*Unity-gain SK, Q = 5, f0 = 10 kHz:* this needs a C1/C2 spread of 4Q² = 100×. Finite GBW raises the peak by +0.05 dB (10 MHz), +0.20 dB (2 MHz) and **+0.37 dB with a −4.6% peak-frequency shift (1 MHz)**. GBW is typical-only, so this term can't be bounded from the datasheet (§4.1).

*For-all-f flatness.* A 4th-order Butterworth (fc = 20 kHz, R 0.1%, C 2%) had its worst passband deviation at the band edge in **256/256 corners**: 0.414 dB nominal, 0.691 dB worst at 15 kHz. For monotone responses, for-all-f reduces to the band edge. The 0.5 dB Chebyshev is the opposite (§4.7).

**Where it breaks.** Corner snapping on AC magnitude; single-argmax linearization of max-over-f; ill-conditioned fixed-frequency specs near peaks; GBW and Zout typical-only. Also SK stopband "rebound": at high frequency the op-amp's closed-loop output impedance rises, and the signal feeds through C1. The stopband floor is then a hump set by the vendor Zout(f) curve, whose fidelity is unknown.

### 3.3 Bridge + instrumentation amplifier (load cell) front end

**Circuit.**
- 350 Ω bridge, 2 mV/V, 5 V excitation → FS = 10 mV.
- Input RFI filter with 1 kΩ per leg.
- 3-op-amp in-amp from OPA189: G1 = 100 (RF = 10k, RG = 202 Ω), diff stage G2 = 2 on a thin-film network.
- Ratiometric 24-bit ADC.
- −40…85 °C.

In this chain **1 µV RTI = 100 ppm FS**.

**Specs.** Total error ≤ 0.02–0.05% FS over temperature after calibration (weigh scales: OIML-style divisions); noise-free resolution ≥ 1/10,000–1/30,000 FS; CMRR ≥ 100 dB DC plus at 50/60 Hz and in the RFI band; zero stability (tare drift) ≤ 1 µV RTI.

**Budget produced by the engine-style affine form (E4).** Units: ppm of FS at full load. Each knob's edge is its ±3σ, so the RSS column is the 3σ value. ADC terms are *(rep.)*.

| Condition | Worst case | RSS (3σ) | Top contributors |
|---|---|---|---|
| 25 °C, no cal | ±2779 | ±1288 | RG tol 989, RF1/RF2 tol 495 each, A1 Vos 300 |
| 85 °C, no cal | ±6258 | ±2242 | RG TCR 1483, RG tol 989, RF TCR 742 each |
| 25 °C, after 2-pt cal | **0** | **0** | everything at the calibration condition cancels exactly |
| 85 °C, after 2-pt cal | **±3479** | ±1831 | RG TCR 1483, RF TCR 742 ×2, A1 237 (TCVos 120 + Ios(T)·1.175 kΩ 118) |

Grouped budget (the "spreadsheet") after calibration, at 85 °C:

| Group | Mechanism | WC | RSS |
|---|---|---|---|
| RG | tempco | 1483 | 1483 |
| RF1, RF2 | tempco | 743 each | 743 each |
| A1 (OPA189) | TCVos + Ios(T) | 238 | 168 |
| A2 | TCVos | 120 | 120 |
| diff-amp network | TCR tracking 2 ppm/K | 120 | 85 |
| ADC *(rep.)* | drift | 32 | 30 |
| Noise 0.1–10 Hz | 24.7 nVrms RTI (typ only) | 2.5 ppm rms, 16 ppm p-p | — |

**Engineering conclusion the engine should reach automatically.** Put RF1, RF2 and RG in one network with TCR tracking of 2 ppm/K. The gain-TCR contribution falls from 2968 ppm to **237 ppm**. That's the design fix, and it only shows up if matching is declared structurally (§4.3).

**CMRR (E3).** A difference amp with independent resistors of tolerance t has worst-case CMRR = (1 + G)/(4t). The engine's corners reproduce it exactly:

| G | t = 1% | t = 0.1% | t = 0.01% |
|---|---|---|---|
| 1 | 33.9 dB (99.9% of boards ≥ 39.5) | 54.0 (59.5) | 74.0 (79.3) |
| 10 | 48.6 (54.3) | 68.8 (74.4) | 88.8 (94.2) |

A network with 0.1% absolute and 0.01% ratio tolerance (modeled as lot knob + per-part knob) gives **80.0 dB**: the lot knob cancels exactly.

Two things break naively:
- **CMRR in dB has no affine form at nominal**, since Acm = 0 there and log\|0\| = −∞. The engine must work on the *signed* Acm, which is exactly linear (±0.05% on R4 gives Acm = ∓2.50e-4), and apply \|·\| and dB afterwards (§6.2).
- **AC CMRR is set by the input RFI filter**, not by the in-amp. RFI filter: R = 4.02k, Ccm = 1 nF, Cdiff = 10 nF.

  | Parts | 60 Hz | 1 kHz | 10 kHz | 100 kHz |
  |---|---|---|---|---|
  | R 1%, Ccm 5% | 74.8 dB | 51.4 dB | **45.3 dB** | 53.5 dB |
  | R 0.1%, Ccm 1% | 89.5 dB | 66.2 dB | **60.0 dB** | 68.3 dB |

  The worst frequency is interior (§4.7), and it's orders of magnitude worse than the in-amp's own ≥ 112 dB (INA821 at G = 10 ✓).

**Where it breaks.**
- No calibration concept (§4.9a).
- Ios(T) is an *envelope* with an unknown correlation between 25 °C and 85 °C (§4.9c). Assuming the same sign at both gives 1.0 nA residual after cal; assuming independence gives up to 2.2 nA.
- Sensor terms (zero balance, TC of span, creep, hysteresis) are datasheet rows, not netlist elements. The budget needs "hand rows" (§4.5).

### 3.4 Thermocouple and 4–20 mA inputs

**Thermocouple.** Type K is about 40.5 µV/°C near room temperature (NIST ITS-90 reference function; not re-verified this session). A ±1 °C system spec is therefore a budget of about **41 µV total**.

| Error source | Size | Engine handling |
|---|---|---|
| Amp Vos drift (zero-drift ~0.02 µV/K ✓ vs bipolar ~0.5 µV/K *(rep.)*) | 1.3 µV vs 32 µV over 65 K | slice over T |
| Burnout detection: 3.3 V / 50 MΩ = 66 nA through 2 × 1 kΩ filter + 50 Ω TC | **135 µV ≈ 3.3 °C** | exact from the netlist, *if* the pull-ups are drawn (classic missed error) |
| CJC sensor accuracy and **gradient** between terminal block and sensor | ±0.2–1 °C | needs a thermal gradient range knob (§4.9f) |
| Parasitic thermocouples (copper against leadframe/solder alloys) | tens of µV/K per junction pair × ΔT *(unverified)* | gradient knob × declared junctions |
| Leakage: 1 GΩ from a 5 V trace into a node with 1 kΩ source impedance | 5 µV ≈ 0.12 °C | auto leakage check (§6.4) |
| NIST polynomial / firmware linearization residual | spec'd by the fit | readout model (§6.2) |
| 50/60 Hz: ΣΔ sinc notches | set by digital filter and clock tolerance | readout model |

The spec is in **°C of the measurand**, not volts: T_read = f⁻¹(V_meas + f(T_cj,meas)). The engine needs a *readout model* to map voltage error to temperature error, plus a for-all over hot-junction temperature (0…1000 °C; the Seebeck coefficient varies with T).

**4–20 mA receiver** (250 Ω sense resistor; spec ±0.05–0.1% of span). Self-heating (E6), using thermal resistance implied by the Vishay TNPW derating table ✓ (1206: 0.27 W at 125 °C film → ≈ 204 K/W; 0805: ≈ 393 K/W):

| Package | TCR | ΔT at 20 mA | Raw error (ppm span) | After 2-pt cal at 4/20 mA |
|---|---|---|---|---|
| 1206 | 25 ppm/K | 20.4 °C | 638 | **186 ppm at 12.8 mA** |
| 1206 | 10 | 20.4 | 255 | 74 |
| 1206 | 5 | 20.4 | 128 | 37 |
| 0805 | 25 | 39.3 | 1228 | 358 |
| 0805 | 5 | 39.3 | 246 | 72 |

Self-heating is **signal-dependent**: error ∝ I³, so the post-calibration worst point sits *inside* the input range (12.8 mA). The spec is `for I in 4..20 mA`, with an interior worst case. It also has a thermal time constant (seconds), which makes it a dynamic error after a step. Protection-TVS leakage at hot (µA-class, datasheet) is a further knob: 1 µA / 16 mA = 62 ppm.

### 3.5 ADC driver + anti-alias/charge-bucket filter + SAR or ΣΔ ADC

**Part facts, ADS8881 ✓** (18-bit, 1 MSPS):
- t_ACQ = 290 ns; 55 pF sampling capacitors; input capacitance 59 pF.
- INL ±2 LSB (C grade); SNR 100 dB, THD −115 dB (typ).

The same datasheet gives the design rules:
- C_FLT ≥ 10× the sampling capacitance, and C0G only.
- R_FLT ≤ 22 Ω (distortion vs stability).
- Verify driver phase margin > 40° with the chosen filter "by SPICE simulation".
- "amplifier data sheets specify the output settling performance only up to 0.1% to 0.001%, which may not be sufficient for the desired 18-bit accuracy. Therefore, always verify the settling behavior of the input driver by TINA-SPICE simulations."

**The datasheet itself says the key spec can only be checked by simulation.** It also shows these rules could become automatic specs (§6.4).

**Specs.**
- Settle to ½ LSB (18 bit: 1.9 ppm FSR, ln(2¹⁹) = **13.2 τ** for a full step; 16 bit: 11.8 τ; 24 bit: 17.3 τ) within t_ACQ, including charge kickback.
- Driver PM > 40–45° with C_FLT.
- THD ≤ −110 dB at a stated f_in.
- Noise at the ADC ≤ ⅓ of the ADC noise.
- Anti-alias attenuation at fs − f_band.

**Experiment (E7).** Follower (single-pole GBW 180 MHz, Ro 10 Ω) → R 10 Ω → C_FLT 2.7 nF → R_sw 20 Ω → C_s 60 pF, discharged at t = 0; t_acq = 300 ns; 18-bit / 5 V.

| | Error at t_acq |
|---|---|
| Nominal | −0.076 LSB (integration converged to 4 digits at dt ≤ 0.2 ns) |
| True range over the 64 corners (GBW ±30%, Ro ±30%, R 1%, C_FLT 5%, C_s 10%, R_sw 30%) | −0.188 … −0.022 LSB |
| Worst corner | GBW **+30%**, Ro −30%, R +1%, C_FLT +5%, C_s +10%, R_sw +30% |
| Affine WC from nominal slopes, in error space | 0.155 LSB (optimistic) |
| Affine WC from nominal slopes, in log\|error\| space | 0.211 LSB (conservative) |

- **A faster op-amp settles worse** here: the capacitive load erodes phase margin. The nominal slope's sign isn't intuitive, and the loop finds it anyway.
- **Measure transforms matter.** Settling residuals are exponential in the knobs, so log-space linearization is better. When the error rings through zero across the box, only an *envelope* measure is smooth (§6.2).

**Where it breaks.**
- No switch element.
- Fixed-step BE overstates a 13τ residual by e^{(t/τ)(h/2τ)} = 1.9× at h = τ/10 (§4.8).
- THD at −115 dB needs a numerical noise floor below −130 dB (reltol ≲ 1e-7, coherent FFT), and a model whose distortion is characterized there. Vendor models aren't. The datasheet also says R_FLT adds distortion through the ADC's *nonlinear* input impedance, which a linear C_s model can't show.
- Noise must be integrated over the *full* RC bandwidth (sampling folds it), plus sampling kT/C: √(kT/55 pF) = 8.7 µVrms ≈ 0.46 LSB rms.
- For ΣΔ, the digital filter's noise bandwidth and notches belong in the readout model.

### 3.6 Voltage reference

**Numbers:** REF5050 ✓ (product page) high grade: 0.05% initial, 3 ppm/°C max, long-term stability 22 ppm over the first 1000 h (SOIC-8). Hysteresis and 0.1–10 Hz noise weren't verifiable from the page.

**Specs.** Initial accuracy (usually calibrated out); TC ≤ 2–5 ppm/K; long-term drift; thermal hysteresis; 0.1–10 Hz noise ≤ 1–3 ppm p-p; line/load regulation; buffer stability with the reference capacitor; ADC reference-input charge settling.

**Where it breaks.**

1. **Box method vs linear TC.** The box spec bounds the *whole curve* inside a box of height TC·ΔT_spec. It says nothing about where V(25 °C) sits in the box.
   - For −40…85 °C at 3 ppm/K, the box is **375 ppm** tall, so |V(T) − V(25)| ≤ 375 ppm.
   - A "TC × (T − 25)" knob gives at most 195 ppm (at −40 °C).
   - For a bow-shaped curve, the linear model underestimates by about 2× and also has the wrong shape (maximum at the ends, not wherever the bow is).
   - **Fix:** the Box knob shape (§6.1).
2. **Aging law.** ECSS §5.1.1.7.2 ✓ prescribes *linear* extrapolation as the conservative default ("other extrapolations may be adopted with adequate justification"). From 22 ppm/1000 h to 10 years (87,600 h):

   | Law | 10-year drift |
   |---|---|
   | Linear | 1928 ppm |
   | √t | 206 ppm |
   | ∛t (Vishay's thin-film law ✓) | 98 ppm |

   The law must be part of the knob, with its justification shown.
3. **Hysteresis** depends on history, is not removed by calibration (unless you recalibrate after thermal cycling), and has no natural place in range/statistical. Add a History shape: a ± band that is independent of the present T.
4. **Solder-reflow shift and humidity** (plastic packages): a per-board random shift, often with typical-only data. Usually calibrated out, so it's a statistical knob that calibration cancels.
5. **Dynamic behavior** is the part the simulator must do: reference buffer + 10–22 µF output cap (X5R/X7R → DC-bias derating [Maxim 5527]) + SAR reference-input current bursts.

### 3.7 Current-sense amplifier

**Part facts, INA240 ✓:** gain error 0.20% max, gain drift 2.5 ppm/°C max, Vos ±5 typ / ±25 µV max, drift ±50 typ / ±250 nV/°C max, CMRR 120 dB min, **input bias current 90 µA (typ only)**. The datasheet recommends external series resistors ≤ 10 Ω, because the bias currents mismatch with differential input and "tolerance can significantly impact the error".

**Spec (typical of motor or solenoid control).** |I_meas − I| ≤ 0.5% of reading + 20 mA, for I in 0.5…20 A, −40…85 °C, 10 years. The requirement **varies with the sweep variable**.

**Walkthrough with a 1 mΩ shunt (1%, TCR *(rep.)* 50 ppm/K):**

| Term | At 1 A (1 mV) | At 20 A (20 mV) |
|---|---|---|
| Vos 25 µV | 2.5% | 0.125% |
| Vos drift 250 nV/K × 60 K = 15 µV | 1.5% | 0.075% |
| 90 µA × 0.2 Ω mismatch (two 10 Ω ±1%) = 18 µV | 1.8% | 0.09% |
| Gain error 0.2% | 0.2% | 0.2% |
| Shunt tolerance 1% (calibratable) | 1% | 1% |
| Shunt self-heating: 0.4 W, ΔT 20–50 K × 50 ppm/K *(rep.)* | ~0 | 0.1–0.25% |

The worst I is at the **low end** for offset and at the **high end** for gain and self-heating. It's a for-all-x spec with a moving worst point, and the loop must search over x.

Also:
- PWM common-mode transients (AC CMRR, 93 dB at 50 kHz typ ✓) are a transient-glitch spec.
- A 2-terminal shunt symbol, or copper in series with the Kelvin sense (copper TCR ≈ 3900 ppm/K), should be a lint in precision current sensing.

### 3.8 Op-amp driving a capacitive load

**Specs.** PM ≥ 45° (or ≥ 40° per the ADS8881 rule), overshoot ≤ 20–25%, stable for CL anywhere in the declared load range (cables, ADC reference or input caps, piezo); settling as in §3.5.

**Knobs.** GBW (typ only), open-loop Zo(f) (typ curve only), CL (range knob), R_iso tolerance, feedback cap.

**Experiment (E2).** Follower, GBW 10 MHz, second pole 30 MHz, Ro = 50 Ω, R_iso = 20 Ω, feedback from the op-amp pin (in-loop isolation):

| CL | 0 | 100 pF | 220 pF | 470 pF | **575 pF** | 1 nF | 2.2 nF | 10 nF | 100 nF | 1 µF |
|---|---|---|---|---|---|---|---|---|---|---|
| PM | 72.4° | 58.3° | 50.3° | 45.9° | **45.7°** | 47.3° | 54.4° | 73.7° | 83.4° | 84.5° |

- Checking the ends of the CL range says PM ≥ 72°. **The truth is 45.7° at an interior CL.** With GBW and Ro at ±30%, the worst CL moves between 437 and 832 pF, and PM spans 42.1–50.5°.
- **The circuit is linear, but PM is not a network function at a fixed frequency.** It's evaluated at the crossover frequency, which itself moves with the knobs. So the corner theorem doesn't apply, even for the range knob CL.
- The 8° PM spread comes entirely from typical-only parameters.

**Simulator need.** A loop-gain probe that doesn't break the loop (Middlebrook 1975 ✓; Tian et al. 2001 ✓), plus PM/GM measures with crossover tracking. The for-all-CL search needs a 1-D optimizer per range corner.

---

## 4. Cross-cutting weaknesses

### 4.1 Op-amp (and IC) macromodels: how to put knobs on a model

**The facts.**
- TI: "Amplifier SPICE models are designed to target typical electrical characteristics at room temperature… Typical specifications cover 68.3% of all devices produced, or ±1-sigma" (SBOA338 ✓, the GWL model paper). The models reproduce Vos, Ib, en, A_OL, Zo, CMR and PSR *at typ*.
- Cadence PSpice Advanced Analysis ✓ handles tolerance by using its own **parameterized** library parts: tolerance parameters (POSTOL/NEGTOL), distribution parameters (default flat/uniform), and optimizable parameters like "GBW = 10 MHz" on an op-amp. It doesn't use vendor typ models for this.

**Consequences for our engine.**
1. The temperature knob has **zero sensitivity** through a typ-at-25 °C model. The adjoint would report "T contributes 0", which is false, and the verdict would be wrong in the dangerous direction.
2. Datasheet error terms (Vos, TCVos, Ib(T), Ios, CMRR, PSRR, A_OL min) must be *injected*.
3. Dynamic parameters (GBW, slew, Zo) can't be spread on a black box.

**Recommendation: two layers plus metadata.**

- **(a) Error-source shell** around any vendor model. All of it is linear (exact via superposition) except where noted:
  - Vos(T, life) in series with +IN: Vos0·ε + TCVos·η·(T − 25) + LTD(life).
  - Ib+ and Ib− current sources to ground, built from Ib and Ios knobs (Ib± = Ib ± Ios/2). They use an **envelope over T** (§4.9c) or a physics law (×2 per 10 K for FET/CMOS).
  - CMRR term Vcm·10^(−CMRR/20)·ε, and PSRR term.
  - Finite-A_OL spread as a VCVS from output to input, ΔV = Vout·(1/A_OL,min − 1/A_OL,typ)·ε.
  - Noise sources (en, in, 1/f corner) if the vendor model lacks them.
- **(b) A native parametric op-amp primitive** (our own device, in the spirit of Boyle et al. 1974 ✓ and TI's GL/GWL). Parameters are knobs: A_OL(s) poles/zeros, GBW, Zo(f), rails/headroom, slew, Ib/Vos, en/in, CMRR(f). Fit it to the vendor model *at typ* (AC gain/phase, Zo, noise, step). The fit residual becomes a reported *model-form error* term. Spreads then act on physical parameters, e.g. GBW ±30% with provenance "typical-only, default spread" (§4.9d).
- **(c) Fidelity metadata per model**, for example:
  `{dc: typ@25C, temperature: none, noise: typ, ac: good to 10×GBW, Zo: good, distortion: none, settling_tail: none}`.

  A spec whose measure depends on an unsupported aspect gets a new verdict, **UNVERIFIABLE (model)**, with a pointer to the datasheet number (e.g. "settling 0.01% in 1.1 µs, typ") as a hand row. That's the honest outcome for 18-bit settling tails and −115 dB THD.
- **(d) Blind-knob detection.** Before trusting a zero coefficient, check the model's declared sensitivities. "Not modeled" ≠ "doesn't matter".

### 4.2 Noise: a statistical quantity inside the statistics

**Three kinds of uncertainty, not two.**

| Kind | Varies over | Question | Combination |
|---|---|---|---|
| Range | conditions (T, supply, life, CL, f) | every value | worst case |
| Statistical | boards | yield | k·σ over boards |
| **Temporal (noise)** | time / readings on one board | per-reading coverage, SNR, noise-free bits | z·σ_n(θ) per reading |

σ_n is a **deterministic function of the other knobs** (4kTR, en(T), in(T), filter bandwidth). So noise is a *measure* with its own affine form in the knobs, not a knob:

- **Noise PSD via adjoint** [RNM71]: z = A⁻ᵀc, S_out = Σ_k \|z_k\|² S_k (one solve per frequency).
- **Noise sensitivities for every parameter at once.** Let w = Σ_k S_k·conj(z_k)·e_k and u = A⁻¹w. Then dS/dp = −2·Re(uᵀ (∂A/∂p)ᵀ z) + Σ_k \|z_k\|² ∂S_k/∂p. That's **one extra solve per frequency** for all p. I derived this and checked it numerically (E8): across 5 parameters × 3 frequencies it matches finite differences to 1e-7…1e-6. It's the classic adjoint-noise idea (Branin 1973 ✓) extended one order.
- **Spec semantics.**
  - (i) *Noise-only:* σ_n ≤ X, or noise-free bits ≥ N: for-all range, yield over statistical.
  - (ii) *Per-reading total:* P_t(\|e_static(θ) + n\| ≤ E) ≥ p. For Gaussian noise, roughly \|e_static\| + z(p)·σ_n ≤ E (z = 3.29 for 99.9%). This is **not** RSS(static, noise). RSS answers "fraction of all (board, reading) pairs", which spreadsheets silently assume.
  - (iii) *p-p conventions:* 0.1–10 Hz noise quoted p-p (≈ 6.6σ). This is 1/f and non-stationary over short windows, so keep it as a datasheet row with its convention.
- **Sampled and aliased noise.**
  - SAR without averaging: noise up to the full RC/sampling bandwidth folds into each sample.
  - ΣΔ: the digital filter's ENBW sets the band.
  - kT/C of the sampling cap.
  - Switched-capacitor front ends have cyclostationary noise. Yuan & Opal 2001 ✓ give an adjoint for periodically switched linear circuits; Demir & Sangiovanni-Vincentelli 1998 ✓ cover the general theory.

  The noise measure must carry the readout model (§6.2).
- **Where noise uncertainty really comes from.** Across the tolerance box, integrated noise barely moves: σ ∝ √f_c, so ±5% C gives about ±2.5%. The real uncertainty is **en and in being typical-only**, with 1/f corners that vary lot to lot. That's a provenance problem (§4.9d), not a sensitivity problem.
- **Magnitude check.** In the load-cell chain, noise was 2.5 ppm FS rms against a 3479 ppm static worst case. Per-reading vs RSS combination didn't change the verdict. It matters when static error has been calibrated down to the noise level (thermocouples, weigh scales after tare).

### 4.3 Correlated tolerances and matching: how to declare them

The datasheet vocabulary for resistor networks (Weihausen/Vishay 2013 ✓) is **absolute tolerance, tolerance matching (ratio tolerance), absolute TCR, TCR tracking, and relative drift**. Tracking works "because the individual resistor elements are all influenced by temperature to the same extent" on one substrate. That's a *thermal* correlation as well as a manufacturing one.

Proposed declaration (strawman):

```
network RN1 "thin-film array" {
    elements  r1 = 10k, r2 = 20k, r3 = 10k, r4 = 20k
    absolute  ±0.1%                 // shared lot knob  a·ε_lot
    matching  ±0.01% pairwise       // per-element m·ε_i, m = 0.005% (pairwise definition)
    tcr       ±25 ppm/K absolute, tracking ±2 ppm/K   // τ_lot·η_lot + τ_trk·η_i, both × (T − 25)
    drift     ±0.05% absolute, ±0.015% relative @ life  // bias + lot + per-element
    thermal   one node               // all elements at the same temperature (self-heating shared)
}
```

Engine translation:

R_i = R_i,nom·[1 + a·ε_lot + m·ε_i + (T − 25)(τ_lot·η_lot + τ_trk·η_i) + L(life)·(b + d_lot·ζ_lot + d_i·ζ_i)]

Traps to handle:
- **Ratio-tolerance definition** varies: pairwise, or relative to a reference element (then R_i/R_j can reach 2× the stated ratio). The part model must record which one applies.
- **The lot knob touches many elements**, so the corner theorem's rank-1 argument fails for it (v2 A5 catch 2). It's 1-D, so split or search it. In ratio circuits it cancels exactly, as in the 80 dB CMRR result.
- **Range × statistical products** (η·(T − 25)). Never form the product inside AA. It creates a fresh error symbol per resistor, so TCR tracking can't cancel between RF and RG, and the 237 ppm answer becomes ~3000 ppm. Instead, evaluate at a fixed T slice, where the coefficient of η is just (T − 25)·τ (§4.9b).
- **Cross-part correlations the netlist can't see:**
  - dual op-amps share die temperature (self-heating of one channel shifts the other's Vos);
  - multi-channel ADCs share reference and gain;
  - an in-amp's internal resistors vs the external RG (datasheet gain drift explicitly excludes RG's TCR).

  These should be declared in part models as shared knobs or shared thermal nodes.

### 4.4 The sheer number of knobs

Estimate for an 8-channel bridge DAQ:

| Per channel | Knobs |
|---|---|
| Sensor (datasheet rows): zero, span, TC zero, TC span, nonlinearity, hysteresis, creep | 7 |
| RFI filter (2 R, 3 C: tol + TC) | 10 |
| In-amp: Vos, TCVos, Ib, Ios, Ib(T) env, gain err, gain TC, CMRR, CMRR(f), PSRR, en, in, 1/f, GBW, Zo | 15 |
| RG: tol, TCR, drift, θ | 4 |
| Driver op-amp (~12) + R_FLT/C_FLT (4) | 16 |
| **Channel total** | **≈ 52 → × 8 = 416** |
| Shared: reference (7), ADC (8), supplies (3), T_amb + gradients (3), life (1), leakage per high-Z node (~16) | ≈ 38 |
| **Total** | **≈ 450** |

Compute cost is not the problem:
- Behavioral op-amps give an MNA of about 240 unknowns. One adjoint gives all ~450 coefficients per output per analysis point.
- A for-all-f noise-and-sensitivity sweep (200 frequencies × 3 solves × 16 range corners × 8 channels ≈ 77k sparse solves) takes seconds at most with KLU.
- Monte Carlo with 10⁴ DC solves takes well under a second.
- Affine storage: 450 knobs × 200 frequencies × 50 specs × 8 B ≈ 36 MB dense; sparse is far less.

The real problems:
1. **Authoring and provenance** of 450 knobs. It must come from part models and datasheet extraction (the AI-assisted part modeling in the vision).
2. **Presentation.** A 450-row contributor table needs grouping by block, part, mechanism, and "cancelled by calibration" (§4.5).
3. **Combinatorics.** Range knobs × setups × sweeps: T, supplies, life, CL, input level, source impedance, CM voltage, f is about 8–10 range dimensions, some with interior worst cases.
4. **Hierarchical export.** When a block exports its result, keep *shared* knobs explicit (T, life, reference, lot knobs spanning blocks). Collapse block-*private* statistical knobs into one aggregated independent symbol (σ = RSS; valid because private knobs are independent of everything outside the block), and private range knobs into a worst-case sum. This is zonotope order reduction [GGP09] with a correctness rule for shared symbols. It makes block contracts composable without losing RSS benefits or correlations.

### 4.5 Error budgets: replace or integrate the spreadsheet

Engineers' budgets (Kester's *Data Conversion Handbook* ✓ tradition; TI Precision Labs; ADI in-amp guide) have:
- rows = error sources;
- columns = value, unit, conversion to ppm FS (or % reading), at 25 °C, over temperature, end of life, after calibration;
- totals = worst case and RSS, often with a "bias vs random" split.

The engine should **generate exactly this view from the affine form**, not a new format:
- **Rows** = knobs grouped (part → mechanism). **Columns** = contribution at selected range slices (25 °C BOL, T_min, T_max, EOL) and after each declared calibration. **Totals** = WC, RSS at k, and the engine's verdict (from the loop or MC). §3.3 shows the table.
- **Hand rows** for terms that aren't simulated or not simulatable: ADC INL, reference hysteresis and long-term drift, sensor terms, thermal EMF, model-form error. Each has a coefficient, kind and provenance. They are still knobs in the same affine form, so they combine with simulated rows under the same rules.
- **Two-way spreadsheet exchange:** export with formulas, and import an existing budget as hand rows. Then show **reconciliation**: "your spreadsheet assumed RG tempco 25 ppm/K × 1; the netlist says RG's sensitivity is 1.98 because of the RF1 + RF2 structure".
- **Allocation.** A top-level budget (0.05% FS) split into block budgets becomes block guarantees with separate bias and random parts (§4.4, item 4).

### 4.6 AC humps and Q sensitivity (where the corner theorem fails)

From E1 and E2, the corner theorem needs three caveats for this domain:

1. **|H(f)| at fixed f:** extremes lie on box edges or in the interior along one knob that retunes a resonance. In the full box, edges beat corners by only 0.03% in the MFB case. In 1-D slices (a single drifting knob, or the test frequency as a knob) the interior extreme is the whole story.
2. **Derived AC measures** (PM, peaking, −3 dB frequency, max over f) aren't network functions at a fixed frequency. The evaluation frequency moves with the knobs, so *range* knobs (CL) have interior worst cases even in linear circuits (45.7° at 575 pF).
3. **Ill-conditioned formulations.** A fixed-frequency gain near a high-Q peak (−38% from ±6% f0 shift) linearizes badly. Pole-based measures (f0, Q from the dominant pole pair) are smooth and cheap: an eigenvalue derivative dλ/dp = −yᴴ(∂G/∂p + λ∂C/∂p)x / (yᴴCx) with left and right eigenvectors, the classic pole-sensitivity idea (Kuo 1958 ✓). Offer them as measures and warn when a spec is written the ill-conditioned way.

**Algorithmic fix:** the worst-point step solves a *box-constrained* problem. Each knob sits at a bound (gradient sign agrees) or is interior (gradient ≈ 0, checked by a 1-D search). Corners become the common special case, not an assumption.

### 4.7 Specs over a frequency band (for-all-f)

- **Monotone responses** (Butterworth passband): the worst frequency is the band edge in all 256 corners. That's cheap, but the engine should *prove* it per corner (sign of d|H|/df over the band), not assume it.
- **Equiripple responses** (Chebyshev, elliptic, many anti-alias designs): at nominal, several local maxima tie. In the 0.5 dB Chebyshev they're 0.500 dB at 7650 Hz and at 18480 Hz. Across the 256 corners the worst peak is the low one 134 times and the high one 122 times.

  | Method | Predicted max ripple | Truth |
  |---|---|---|
  | Linearize max-over-f at the nominal argmax | 0.678 dB | 1.113 dB |
  | One loop step from that prediction | 0.949 dB (lands on the *other* peak) | 1.113 dB |
  | **Track each local extremum as its own sub-spec** (Danskin 1966 ✓: derivative of a max = derivative at the maximizer, when unique) | 0.678 and **1.120** → max 1.120 dB | 1.113 dB |

  **Fix:** a for-all-f spec expands into one tracked sub-spec per local extremum, plus the band edges. Each sub-spec is smooth, and the verdict is the worst sub-spec.
- **Grid resolution.** At 20 points/decade (12% spacing), a Q = 10 peak can fall between grid points and be under-read by up to **3.7 dB**. Refine locally around maxima, or use the pole-based analytic peak.
- **Interior worst frequencies** also appear in non-filter specs: RFI-filter CM→DM peaks near 10 kHz, and the SK stopband rebounds.

### 4.8 Specs in ppm vs simulator numerical accuracy

| Finding (E5) | Number |
|---|---|
| Linear op-amp MNA (VCVS gain 1e5…1e9, gmin 1e-12): double vs exact rational | relative error 0 … **1.4e-16**; finite-A_OL gain error −999 / −10 / −0.1 ppm resolved exactly |
| Diode + R, Newton with the repo's criterion (reltol 1e-3, abstol 1e-6) | error **up to 13.2 ppm**, median 0.46 ppm over 200 circuits |
| Same, reltol 1e-6 | ≤ 7.4e-5 ppm |
| Physical effect of a 0.01% resistor change in that circuit | 4.06 ppm |
| Within one iteration count the error is smooth (4.94 → 4.92 ppm over a 0.1% sweep); it **jumps** when the count changes | finite differences between corner simulations corrupted at the ppm level |
| BE fixed step on a settling residual after t = 13τ | residual overstated by e^{(t/τ)(h/2τ)}: **1.9×** at h = τ/10, 1.07× at h = τ/100 |

Recommendations:
1. **Engine-mode Newton:** reltol ≤ 1e-6 plus one polishing iteration after convergence. That's cheap, since convergence is quadratic.
2. Sensitivities stay analytic (adjoint at the converged point), as v2 already says.
3. **Transient:** variable-step LTE control with settling-grade tolerances (reltol ~1e-6 to 1e-7). Use step alignment for FFT-based THD, and report the numerical noise floor next to any THD or settling result.
4. **gmin accounting:** gmin current (V·gmin) competes with fA–pA input bias currents (electrometer, photodiode and pH front ends). Remove gmin after homotopy and report its contribution.
5. Report a **numerical-accuracy bracket** alongside the tolerance bracket, so a "PASS by 2 ppm" is never within the solver's own error.

### 4.9 Other gaps found

- **(a) Calibration and trim (biggest missing concept).** Precision specs are judged after one-point (offset/tare), two-point (gain) or multi-point calibration, possibly at production temperature, sometimes re-run in the field (reference channels, current reversal, chopping in firmware). ECSS §5.1.1.3 ✓: "If the circuit compensates initial tolerance or environmental variations… the analysis report should include a justification for the residual variation."
  - *Formalization:* a calibration knob is chosen **per board, after** statistical knobs are rolled and **before** range knobs vary. The quantifier order is ∀stat ∃trim ∀range. It's distinct from `?` (∃ at design time).
  - *For a linear calibration* (offset + gain fitted at declared conditions), the post-cal affine form is exactly `form − [a(θ) + b(θ)·x]`, with a, b evaluated from the same form at the calibration points. In §3.3 this cancels everything at 25 °C and leaves tempco and drift.
  - *Trim with finite resolution or range* (a trim DAC step, select-on-test resistor from E96) leaves a residual uniform in ±½ step: a new statistical knob.
- **(b) Range × statistical products.** TCR × ΔT, drift-law(life) × random drift sign, Ib envelope(T) × sign. These are bilinear, not affine. Represent them as *affine in statistical knobs, parametrized by the range slice*. The loop already works slice by slice; the storage format must too.
- **(c) Envelope specs.** Most datasheet limits are "max over −40…125 °C" or "max over 0…85 °C" (OPA189 Ib and Ios ✓). They say nothing about correlation between temperatures, so calibration at 25 °C removes only p(25). A conservative residual is env(T) + env(25). Assuming "same sign" is a modeling assumption that must be shown.
- **(d) Provenance-aware distributions.**
  - *Tested limits* are guard-banded (OPA189 Vos max = 7.5 × typ; with TI's "typ ≈ 1σ", σ = tol/3 is 2.5× pessimistic).
  - *Characterized* limits (drift) aren't 100% tested.
  - *Typical-only* parameters (GBW, en, Zo, Ib of INA240) have no bound at all.

  Each gets a different default, and typical-only knobs get a visible "assumed spread" that the user can confirm. Several datasheets give **production histograms** (INA240 Vos and gain error ✓); use them when present.
- **(e) Self-heating and electro-thermal coupling.** Resistor value depends on its own dissipation (gain-stage nonlinearity 10–100 ppm, 4–20 mA INL 37–358 ppm, shunt 0.1–0.25%). Drift depends on film temperature: Vishay's thin-film law ΔR/R ∝ 2^{(ϑ−ϑ₀)/30 K}·(t/t₀)^{1/3} ✓. So aging is operating-point dependent too. ECSS §5.1.1.5d ✓ says the thermal analysis supplies each component's actual temperature.
- **(f) Spatial temperature.** Matched pairs only track at the same temperature. Thermal EMF needs *gradients*. CJC needs the terminal-to-sensor gradient. A single global T knob is insufficient. Add thermal nodes per part (or group) with an ambient + θ·P + declared-gradient model.
- **(g) Leakage.** PCB layout is out of scope, but high-impedance nodes are visible in the schematic. Inject declared leakage conductances (range knob, humidity-dependent) from each high-Z net to its worst-potential neighbors or rails. Flag nodes where the assumed leakage exceeds a budget fraction, and suggest guarding.

---

## 5. Better methods from literature and industry

| Problem | Method | Source (status in §8) |
|---|---|---|
| Error budgets | Row-per-source, bias/random split, WC + RSS, pre/post calibration | Kester (ed.), *Data Conversion Handbook* ✓; ECSS-Q-HB-30-01A ✓ (bias/random, compensation residuals, aging extrapolation) |
| Worst case with operating ranges | Range vs statistical parameters, worst-case distance, spec-wise linearization | [Graeb07], [AGW94], Schenkel et al. DAC 2001 (in bib) |
| Tolerance-annotated behavioral parts | Parameterized library parts with POSTOL/NEGTOL/DIST; sensitivity, MC, "Smoke" derating | Cadence PSpice Advanced Analysis User Guide ✓ |
| Op-amp macromodels | Boyle model; TI GL/GWL behavioral, modular models fitted to datasheet curves | Boyle et al. 1974 ✓; TI SBOA338 ✓ |
| Filter sensitivity | Gain-sensitivity product; pole-pair selection for minimum sensitivity; symbolic network functions | Moschytz 1970, 1971, Moschytz & Horn 1977 ✓; ISAAC (Gielen et al. 1989) ✓ |
| Pole/Q/f0 sensitivities | Pole-zero sensitivity of network functions; eigenvalue derivatives | Kuo 1958 ✓ |
| Loop gain / phase margin in simulation | Injection without breaking the loop; Tian's general method | Middlebrook 1975 ✓; Tian et al. 2001 ✓ |
| Noise and its sensitivities | Adjoint noise [RNM71]; sensitivity + noise via the adjoint network (Branin 1973 ✓); 2nd-solve noise sensitivities (§4.2, verified here); switched circuits (Yuan & Opal 2001 ✓); nonlinear/cyclostationary (Demir & Sangiovanni-Vincentelli 1998 ✓); practitioner method (Kay 2012 ✓) | |
| Max over a band | Danskin's theorem; per-extremum tracking; frequency-band guaranteed bounds [Ferber 2018 in bib] | Danskin 1966 ✓ |
| AC value sets | Edges (not corners) for frequency response | [Bartlett, Tesi & Vicino 1993] in bib |
| Simulation accuracy for distortion and settling | Tolerance and integration guidance for accurate Fourier and settling measures | Kundert, *Designer's Guide to SPICE and Spectre* ✓ |
| SAR driver design | Charge-bucket RC sizing, R_FLT limits, PM > 40° by simulation, settling to 18 bits | ADS8881 datasheet ✓; Walsh, Analog Dialogue 46-12 (unverified) |
| Resistor matching, drift | Tolerance matching, TCR tracking, relative drift; Arrhenius + ∛t thin-film drift | Weihausen/Vishay 2013 ✓; Vishay doc 28809 ✓; TNPW datasheet ✓ |

**Interval/affine AC methods from our bibliography** ([PKK10], [Dre06], [Din15], [SH18], [Ferber 2018]) are the right *guaranteed* upgrade for **low-Q** sections and small tolerance boxes. For Q ≳ 3 and flatness specs of 0.01–0.1 dB they need heavy splitting ([Din15]: "rough" near resonance; [SH18]: inner/outer ratio 0.28–0.54 at ±20%). Keep them optional and use the loop plus per-extremum tracking as the main path.

---

## 6. Recommended changes (ranked by impact)

### 6.1 Part-model and knob representation (highest impact)

```rust
enum KnobKind { Range, Statistical, Calibrated { at: CalPointId }, Trim { resolution: f64 }, Solver }
enum Provenance { TestedLimit, Characterized, TypicalOnly { assumed_spread: f64 }, Histogram(Dist), HandEntered }
enum KnobShape {
    Scalar,                                          // tol, Vos
    Slope    { over: RangeKnob, reference: f64 },    // statistical sign × (T − 25): TCR, TCVos
    Envelope { over: RangeKnob, env: Curve, corr: EnvCorr /* Same | Independent | Unknown */ },
    Box      { over: RangeKnob, height: f64 },       // reference box-method TC
    Life     { law: Linear | Sqrt | CubeRoot | Arrhenius { ea_ev: f64, t_ref_h: f64, temp: ThermalNode } },
    History  { band: f64 },                          // thermal hysteresis
}
struct Knob { id: KnobId, kind: KnobKind, shape: KnobShape, provenance: Provenance,
              dist: Dist, lot: Option<LotId>, thermal: Option<ThermalNodeId>, source: DatasheetRef }
```

- Every datasheet number carries provenance, so defaults differ (tested limit ≠ 3σ; typical-only gets an explicit assumed spread and a warning).
- Bias + random decomposition for aging and temperature, per ECSS Table 5-1.
- Network and matching declarations (§4.3) compile into lot + per-element knobs, with the ratio definition recorded.
- **Model fidelity metadata** and blind-knob detection (§4.1c–d).
- Per-technology defaults: thin film (TNPW ✓: 0.1%, ±25/15/10/5 ppm/K, ≤ 0.05% at 1000 h and 125 °C film), networks, foil, thick film ([Vishay CRCW]), C0G vs X7R.

### 6.2 Spec definitions

1. **Calibration blocks and after-cal requirements:**
   ```
   calibrate zero at (temp 25°C, life 0, input 0)
   calibrate gain at (temp 25°C, life 0, input FS)
   spec total = reading_error(%FS) in ±0.05% for input in 0..FS  after cal  yield 99.9%
   ```
2. **Readout model / measurand units:** specs in °C, % reading or ppm FS, through the ideal inverse transfer plus firmware operations (ratiometric divide, CJC, NIST polynomial, averaging, digital filter ENBW, current reversal). This needs **multi-setup measures**, e.g. `(V(+I) − V(−I))/2`.
3. **Requirements that vary with the sweep variable:** `for I in 0.5..20A: |err(I)| <= 0.5% * I + 20mA`.
4. **Noise measures and per-reading semantics:** `noise(V(out), 0.1Hz..10Hz).rms`, `noise_free_bits`, `error_per_reading(coverage 99.9%)` = static + z·σ_n, kept distinct from population RSS.
5. **Measure normalization.** The engine rewrites non-smooth measures into smooth signed ones before linearizing, and shows the rewrite:
   - `CMRR ≥ 80 dB` → `−1e-4 ≤ Acm/Ad ≤ 1e-4`;
   - `|x| ≤ a` → two one-sided specs;
   - settling → log residual, or envelope when ringing crosses zero;
   - phase margin → tracked crossover.
6. **Pole-based measures** (`f0`, `Q` of a named pole pair), and a warning when a fixed-frequency gain spec sits within f0/Q of a resonance.
7. **for-all-f** expands into tracked local-extremum sub-specs plus band edges (§4.7).

### 6.3 Engine algorithms

1. **Worst-point step = box-constrained optimization** with interior knobs allowed (KKT check plus 1-D searches). Use the corner theorem only for single-element DC knobs; lot, temperature and life knobs are multi-element, so slice or search them.
2. **Slice over range knobs.** Store affine forms per range slice, with statistical coefficients that are functions of the slice. Never multiply range × statistical symbols in AA.
3. **Calibration as exact affine subtraction** (§4.9a). Trim resolution becomes a residual knob.
4. **Noise engine:** PSD by adjoint, sensitivities by one extra solve (§4.2), noise affine forms in the knobs, and a per-source noise budget table.
5. **Error-budget view** generated from affine forms: grouping, slices, pre/post calibration, hand rows, spreadsheet import/export and reconciliation (§4.5).
6. **Hierarchical export** keeps shared knobs and aggregates private ones (§4.4, item 4).
7. **Verdicts:** add UNVERIFIABLE (model fidelity) and a numerical-accuracy bracket.

### 6.4 Automatic specs for precision analog (on by default in precision blocks)

| Auto spec | Source | Example |
|---|---|---|
| Op-amp input CM range, output swing and headroom at every corner, including rail-to-rail crossover regions | part model | — |
| **Stability with the actual load, for all CL in the declared range** | part model + setup | PM ≥ 45° (ADS8881 rule: > 40° with the RC) |
| **SAR driver rules** | ADC part model | C_FLT ≥ 10·C_s, C_FLT is C0G, R_FLT ≤ 22 Ω, settle to ½ LSB within t_ACQ |
| Bias current × source-impedance mismatch | part model | INA240: series R ≤ 10 Ω |
| Class II ceramic (X7R/X5R) in a filter or signal path | part model | lint: distortion, DC-bias, piezo |
| **High-Z node leakage sensitivity** | netlist + declared leakage | flag if 1 GΩ to a rail moves the reading by > 10% of budget |
| **Self-heating × TCR** of precision resistors | netlist + TCR + θ | flag if ΔT·TCR > 10% of budget (4–20 mA, gain resistors, shunts) |
| Ratiometricity check | netlist | excitation and ADC reference from one source, or budget both |
| Burnout / bias networks on sensor inputs | netlist | error through filter + sensor resistance |
| Reference buffer stability + reference-cap DC-bias derating | part models | — |
| Kelvin connection on shunts | symbol type | lint for 2-terminal shunts in precision sense paths |
| Model-fidelity coverage | fidelity metadata | every precision spec's measure is supported by its models |

### 6.5 Simulator features (in dependency order for this domain)

1. **E/G/F/H controlled sources**, then **B-sources** with `TABLE`/`LIMIT` (required for vendor macromodels).
2. **Native op-amp primitive** with knob parameters (§4.1b); error-source shell generator.
3. **AC on the operating point** for all devices; **sparse complex KLU** for AC (replace the dense 2n×2n expansion); AC transposed solve.
4. **Noise analysis** (adjoint) + noise sensitivities (second solve).
5. **Temperature**: T-dependent stamps (tc1/tc2, Ib(T) laws) + **electro-thermal self-heating** per thermal node.
6. **Pole-zero analysis** (generalized eigenproblem on G + sC) with eigenvalue sensitivities; **loop-gain probe** (Middlebrook/Tian) with PM/GM measures.
7. **Switch element** and **variable-step transient with LTE control** at settling-grade tolerances; coherent-FFT THD with a reported noise floor.
8. **Engine-mode accuracy**: reltol ≤ 1e-6 + polish; gmin removal and accounting.

---

## 7. Is anything in our bibliography misapplied?

| Item | How the v2 docs use it | Issue for precision analog | Correction |
|---|---|---|---|
| **[ECSS11]** | Summarized as "initial tolerance random; aging, temperature and radiation biased" (bib §12, v2 A7: "tempco… are range terms") | ECSS Table 5-1 ✓ says Aging and Temperature are "Biased (**sometimes random**)". §5.1.1.5b ✓: "There can also be a random part with respect to the bias value". §3.2.9 defines random as "no preferred direction or sign". A ±25 ppm/K TCR is random by that definition | Bias + random decomposition; TCR sign is statistical, × range variable (§4.9b). ECSS also covers compensation residuals (§5.1.1.3) and thermal-analysis temperatures (§5.1.1.5d), both relevant here |
| **[ECSS11] aging** | "life" as a range knob | ECSS's *linear* extrapolation (§5.1.1.7.2 ✓) is 20× more conservative than the ∛t thin-film law ✓ over 10 years. ECSS allows other laws "with adequate justification" | Make the law part of the knob and show the justification |
| **[N&P07] / [PPC65]** corner theorem | "Linear DC checks are exact via corners"; walkthrough extends the intuition to AC-ish cases | Holds for single-element knobs at DC. In precision circuits the dominant knobs are *shared* (T, lot, TCR-lot, life bias), and the key specs are AC/derived (CMRR(f), PM, ripple), where it doesn't apply. PM vs the range knob CL has an interior worst case in a *linear* circuit | Corners are a special case of the box-constrained step; slice shared knobs |
| **[RNM71]** | "Building the AC adjoint gets both [sensitivity and noise]" | True for noise PSD. Noise *tolerance analysis* also needs noise sensitivities, one more solve per frequency, not free. And noise isn't a range or statistical knob | §4.2 |
| **[AGW94] / [Graeb07]** WCD and yield ≈ Φ(β) | Yield numbers from the loop | Assumes Gaussian statistical parameters with known covariance. Datasheet limits are guard-banded tested limits or typical-only, and Φ(β) inherits that assumption | Provenance-aware distributions; show yield as conditional on assumed distributions |
| **v2 default σ = tol/3** | All statistical knobs | 2.5× pessimistic for guard-banded tested IC limits (OPA189: max = 7.5 × typ, TI typ ≈ 1σ ✓); undefined for typical-only | Per-provenance defaults |
| **[Vishay CRCW]** | Example of drift ("±1% becomes ±3% at end of life") | Thick film. Precision chains use thin film (TNPW ✓: ≤ 0.05% / 1000 h at 125 °C film, less when cooler per Arrhenius) and networks with *relative* drift | Per-technology defaults |
| **[Din15], [PKK10], [Dre06]** complex AA / parametric AC | Proposed for guaranteed AC (v2 Part B) | Loose near resonance; the flatness specs here are 0.01–0.1 dB | Optional guaranteed upgrade for low-Q sections; main path = loop + per-extremum tracking |

Nothing is *wrong* in the bibliography entries themselves. The issues are in how two of them (ECSS, the corner theorem) were generalized in the design text.

---

## 8. References introduced here

Status: **✓ verified this session** (primary record read: Crossref/DOI record, or the document itself). **(partial)**: exists, but a detail is unconfirmed. **(unverified)**: couldn't be checked this session (analog.com timed out; web-search budget exhausted).

**Papers and books**
- Boyle, G. R., Cohn, B. M., Pederson, D. O., Solomon, J. E., "Macromodeling of integrated circuit operational amplifiers," *IEEE JSSC* 9(6):353–364, 1974. doi:10.1109/JSSC.1974.1050528 ✓ (Crossref lists the authors in the order Boyle, Pederson, Cohn, Solomon).
- Tian, M., Visvanathan, V., Hantgan, J., Kundert, K., "Striving for small-signal stability," *IEEE Circuits and Devices Magazine* 17(1):31–41, 2001. doi:10.1109/101.900125 ✓
- Middlebrook, R. D., "Measurement of loop gain in feedback systems," *Int. J. Electronics* 38(4):485–512, 1975. doi:10.1080/00207217508920421 ✓
- Danskin, J. M., "The theory of max-min, with applications," *SIAM J. Appl. Math.* 14(4):641–664, 1966. doi:10.1137/0114053 ✓
- Moschytz, G. S., "Gain-sensitivity product—a figure of merit for hybrid-integrated filters using single operational amplifiers," *IEEE JSSC* 6(3):103–110, 1971. doi:10.1109/JSSC.1971.1049663 ✓
- Moschytz, G. S., Horn, P., "Reducing nonideal op-amp effects in active filters by minimizing the gain-sensitivity product (GSP)," *IEEE TCAS* 24(8):437–445, 1977. doi:10.1109/TCS.1977.1084363 ✓
- Moschytz, G. S., "Second-order pole-zero pair selection for nth-order minimum sensitivity networks," *IEEE TCT* 17(4):527–534, 1970. doi:10.1109/TCT.1970.1083158 ✓
- Gielen, G. G. E., Walscharts, H. C. C., Sansen, W. M. C., "ISAAC: a symbolic simulator for analog integrated circuits," *IEEE JSSC* 24(6):1587–1597, 1989. doi:10.1109/4.44994 ✓
- Kuo, F., "Pole-zero sensitivity in network functions," *IRE Trans. Circuit Theory* 5(4):372–373, 1958. doi:10.1109/TCT.1958.1086492 ✓
- Branin, F., "Network sensitivity and noise analysis simplified," *IEEE Trans. Circuit Theory* 20(3):285–288, 1973. doi:10.1109/TCT.1973.1083675 ✓
- Yuan, F., Opal, A., "Adjoint network of periodically switched linear circuits with applications to noise analysis," *IEEE TCAS-I* 48(2):139–151, 2001. doi:10.1109/81.904878 ✓
- Demir, A., Sangiovanni-Vincentelli, A., *Analysis and Simulation of Noise in Nonlinear Electronic Circuits and Systems*, Springer, 1998. doi:10.1007/978-1-4615-6063-0 ✓
- Kester, W. (ed.), *Data Conversion Handbook*, Analog Devices / Elsevier-Newnes, 2005, ISBN 978-0-7506-7841-4 ✓ (chapters by Kester confirmed via Crossref; editor role partial).
- Kay, A., *Operational Amplifier Noise: Techniques and Tips for Analyzing and Reducing Noise*, Elsevier/Newnes, 2012, ISBN 978-0-7506-8525-2 (partial: chapters confirmed via Crossref, full title from memory).
- Kundert, K., *The Designer's Guide to SPICE and Spectre*, Kluwer, ISBN 0-7923-9571-9, doi:10.1007/b101824 ✓ (Crossref lists the eBook as 2003; the print edition is 1995).
- Schaumann, R., Van Valkenburg, M. E., *Design of Analog Filters*, Oxford Univ. Press, 2001 (unverified).
- Vlach, J., Singhal, K., *Computer Methods for Circuit Analysis and Design*, 2nd ed., Van Nostrand Reinhold, 1994 (unverified).

**Standards, vendor documents, datasheets**
- ECSS-Q-HB-30-01A, *Worst case analysis*, 14 Jan 2011 ✓. Read: §3.2.2/3.2.9 definitions, §5.1.1.3 compensation, §5.1.1.5 temperature, §5.1.1.6 initial tolerance, §5.1.1.7.2 aging extrapolation, Table 5-1, §5.3.3–5.3.5. https://ecss.nl/wp-content/uploads/handbooks/ecss-q-hb/ECSS-Q-HB-30-01A14January2011.pdf
- Alani, T., Williams, I., "Green-Williams-Lis: Improved op amp spice model," TI SBOA338, Jan 2019 ✓.
- TI SLOA024B, "Analysis of the Sallen-Key Architecture," July 1999, rev. Sept 2002 ✓ (commonly attributed to J. Karki; author not confirmed this session).
- Cadence, *PSpice Advanced Analysis User Guide*, v17.2, 2016 ✓ (parameterized parts: POSTOL/NEGTOL, DIST default flat, GBW as optimizable parameter).
- Vishay, "Drift Calculation for Thin Film Resistors," Technical Note doc 28809, rev. 12-Apr-13 ✓.
- Vishay, TNPW e3 datasheet, doc 28758 ✓ (tolerances, TCRs, P70 per operating mode, ΔR/R at 1000/8000/225,000 h).
- Weihausen, S. (Vishay Draloric), "Taking a look at electrical properties of thin film chip resistor arrays," *Electronic Products*, 2013 ✓ (tolerance matching, TCR tracking, relative drift).
- TI OPA189/OPA2189/OPA4189 datasheet, SBOS830I, Oct 2021 ✓.
- TI INA240 datasheet, SBOS662C, Dec 2021 ✓.
- TI ADS8881 datasheet, SBAS547D, Aug 2015 ✓.
- TI INA821 product page ✓ (35 µV, 0.4 µV/°C, 0.15% gain error, 35 ppm/°C gain drift for G > 1, 112 dB CMRR at G = 10, 0.5 nA Ib, 7 nV/√Hz).
- TI REF50xx product page ✓ (0.025/0.05/0.1% initial; 2.5/3/8 ppm/°C; 22 ppm first 1000 h SOIC).
- Kitchin, C., Counts, L., *A Designer's Guide to Instrumentation Amplifiers*, 3rd ed., Analog Devices, 2006 (unverified).
- ADI MT-series tutorials on offset, bias current, noise, in-amps and references (unverified).
- Walsh, A., "Front-End Amplifier and RC Filter Design for a Precision SAR Analog-to-Digital Converter," *Analog Dialogue* 46-12, 2012 (unverified).
- NIST ITS-90 thermocouple reference functions (NIST Monograph 175, 1993) (unverified this session; type K Seebeck ≈ 40.5 µV/°C at 25 °C computed from memory of the coefficients).

---

## Appendix A: experiments

All scripts are pure Python 3 (no numpy), in `/tmp/pa/` (scratch, not committed). Each uses brute-force corners and/or dense sampling as the answer key.

| # | Script | Model | Key outputs |
|---|---|---|---|
| E1 | `filters.py`, `cheby.py` | Closed-form MFB BP (Q 10), equal-component SK (Q 5), unity-gain SK with integrator op-amp (finite GBW), 4th-order Butterworth and 0.5 dB Chebyshev from two SK sections | §3.2, §4.6, §4.7 |
| E2 | `capload.py` | 2-pole op-amp (A0 1e6, GBW, p2 = 3·GBW), Ro, R_iso, CL; loop gain from the pin; PM by bisection on the crossover plus a crossing count | §3.8 |
| E3 | `cmrr.py` | Difference amp Acm/Ad; lot + per-part network; 2-node RFI filter with Ccm and Cdiff | §3.3 |
| E4 | `budget.py` | Load cell + OPA189 3-op-amp in-amp + network diff stage + ratiometric ADC *(rep.)*; per-knob coefficients, 2-point calibration, grouping, noise | §3.3, §4.5 |
| E5 | `numerics.py` | Exact-rational vs float MNA (VCVS op-amp); diode + R Newton with the repo's convergence test and simplified pnjlim | §4.8 |
| E6 | `sar_4ma.py` (part 1) + inline | 4–20 mA self-heating V = IR₀(1 + TCR·θ·I²R₀), 2-point cal | §3.4 |
| E7 | `sar_4ma.py` (part 2) | Follower (single-pole) → R → C_FLT → R_sw → C_s, RK4, 64 corners | §3.5 |
| E8 | `noise_sens.py` | Non-inverting amp with integrator op-amp; thermal noise of Rs, Rg, Rf; adjoint noise + second-solve sensitivities vs finite differences | §4.2 |

Limitations: closed-form or small models, not the spicy simulator (which can't run op-amp circuits yet, §2). Knob ranges are stated in each section. ADC offset/gain numbers and shunt TCR are representative. The macromodels are idealized: single or two poles, no slew, no rails.
