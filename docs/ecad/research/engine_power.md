# The Bounds Engine Against Real Power Circuits

> Research report · 2026-09-25 · companion to `../archive/engine_v2.md`, `../walkthrough.md`, `../specs.md`
> Keys like **[Rid91]** point to §13 of this file. Keys already in `../bibliography.md` (e.g. **[AGW94]**, **[N&P07]**) keep their names.
> **Where the numbers come from.** Most numbers were computed for this report with small models (Appendix A): a Ridley current-mode small-signal model, a large-signal averaged model, and an idealized switching model with event location plus a shooting periodic-steady-state (PSS) solver. IC parameters come from real TI datasheets that I read (revision cited). Values marked *(rep.)* are representative, not taken from a specific datasheet. Values marked *(assumed)* are placeholders for spreads the datasheet does not give.

---

## 0. Summary: what power circuits do to the engine

The design (affine forms as the common format, exact corners for linear DC, the worst-point loop, range vs statistical knobs, Monte Carlo at sign-off) **still holds up for power circuits**, but only after a set of changes. The engine was designed around "one smooth metric from one cheap simulation of fixed parameter values". Power circuits break all three parts of that assumption. Findings, ranked by impact:

1. **Our simulator cannot simulate any of the six circuit classes today.** It has no MOSFET, switch, controlled or behavioral source, or comparator/logic element. It also has fixed-step transient with no breakpoints. This blocks power far more than any engine math. (§9.3, §11.6)
2. **Power specs need four analysis levels, and the loop must move between them.** The levels are formulas, the averaged model, PSS and full switching transient.
   - Ripple, peak current and efficiency don't exist in an averaged model.
   - Loop gain from the averaged model is wrong near fsw/2, which is exactly where the worst corners push crossover. At the buck's low-phase-margin corner, the averaged model predicted −168 mV of undershoot; switching simulation gave −218 mV.
   - PSS by shooting reached steady state in **21 simulated periods, vs ~1,160** for brute-force transient.
   - Recommendation: run the loop on the cheap level and **verify the predicted worst point one level up**. (§3.3, §9.2)
3. **Mode changes are the dominant real-world failure, and nothing in the design models them.** At the worst ripple corner (Vin max, L −20% and soft-saturated, fsw −20%, C_eff min), the peak inductor current at full load reaches **4.07 A**. The datasheet current limit is **4.0 A min**. A PSS with the limit at 4.0 A shows the converter current-limiting at 3 A and **V(out) collapsing to 2.92 V (−11%)**; the real part would then go into hiccup. At nominal the peak is 3.59 A, 0.4 A below the limit. Load-step undershoot vs step size **kinks** where the limit engages: its slope rises from about −140 mV/A to −580 mV/A. Modes must be a first-class output of every simulation, and the engine must check mode boundaries automatically. (§3.3 S3, S5)
4. **The knobs that decide loop stability are ones the datasheet doesn't bound.** For the TPS54335A, error-amp gm (1300 µS) and COMP-to-switch gain (8 A/V) are **typical only**, and slope compensation isn't given at all.
   - Worst-case phase margin: **14.7°** with ±30%/±20% placeholder spreads on these, vs **37.8°** with them at typical. Nominal is 60°.
   - The engine needs a knob kind for **"typical only, spread assumed"**, shown explicitly and ranked. It also needs a **"datasheet limit"** kind (bounded, distribution unknown, temperature already included), which is neither a range knob nor a statistical one. (§3.1, §11.2)
5. **Knobs are no longer independent of the circuit.**
   - MLCC capacitance depends on its DC voltage. Inductance depends on current. RDS(on) depends on junction temperature, and junction temperature depends on power dissipated.
   - The v2 notes list "DC bias" as a *range term added on top* (A7). That is the wrong kind of object: it is a state-dependent device model C(V, T), plus one knob for the uncertainty of the vendor curve.
   - Temperature splits into ambient (a range knob) and each part's own temperature (a solved state).
   - Self-heating raises the buck's junction-temperature swing by ×1.2–1.33. At 85 °C ambient it lands at **153–196 °C** depending on RDS(on) and board θJA, i.e. at or past thermal shutdown. (§3.3 S7, §9.5)
6. **The worst-point loop mostly works, but needs three guards for power.**
   - For load transient it worked well: the line predicted −211 mV, the averaged model gave −245 mV at the predicted corner, brute force confirmed that corner, and switching verification gave −238 mV.
   - For phase margin the line was 8° optimistic (buck) and **41° optimistic** (boost: predicted 45.5°, actual 4.0°).
   - On the buck, the true worst corner needed a **joint flip** of three knobs that act through one intermediate quantity (slope ratio mc). Flipping any one of them alone doesn't show it.
   - On the boost, re-linearizing **cycled** between two corners. The Vin slope flipped sign between nominal and the worst region, because of the RHP zero.
   - Guards: (a) enumerate corners exhaustively over the top-k knobs whenever the model is cheap (averaged AC takes about 1 ms per point); (b) remember visited corners and keep the best one; (c) test joint flips of knobs that share an intermediate (mc, fc, f_RHPZ, Qp). (§3.3 S4, §4)
7. **Stability specs need more than PM/GM.**
   - Use a modulus margin (min |1+T|), which stays continuous when crossovers multiply.
   - Check Floquet multipliers from the PSS. At the buck's low-PM corner the dominant oscillatory pair moves from 0.36±0.33j to **−0.42±0.46j**. With weak slope compensation the shooting method *converges to an unstable orbit* with a multiplier of **−1.18**, so PSS reports a steady state that the hardware never shows.
   - Add input-filter interaction (Middlebrook) as a composition spec. (§9.8)
8. **Block contracts for power interfaces must carry impedances, ripple spectra, transient envelopes and start-up behaviour, not only a DC range.** Examples:
   - LDO output ripple = buck ripple × PSRR: **0.10–2.27 mVpp** across corners and headroom.
   - eFuse current limit (1.80–2.20 A) vs a downstream converter that starts drawing 11 W at 4.5 V: 2.44 A, so the eFuse enters current limit with **16.5 W in its FET**.
   - UVLO hysteresis vs source impedance.
   - Converter negative input impedance vs input filter. (§6, §11.5)
9. **The exact methods earn their keep in power, at the "programming" parts.** Feedback dividers, EN/UVLO dividers, current-limit and charge-current set resistors, and NTC temperature windows are linear or single-use monotone chains, so corners are exact.
   - BQ24072 NTC cold cut-off: nominal +0.5 °C, exact range **−1.8…+2.9 °C**, so "no charging below 0 °C" fails.
   - Charge current with R_ISET = 890 Ω: **0.887–1.107 A**. The `?` solver picks 1.00 kΩ to stay ≤ 1.0 A. (§8)
10. **For power, "realistic" (3σ) and "absolute worst case" are close.** Most contributors are range knobs (Vin, load, ambient, aging, DC bias through the operating point) or datasheet limits. Buck DC accuracy: −3.22% realistic vs −3.61% worst case. Default power stability and protection specs to `worst_case`, and use yield mainly where passive tolerances dominate. (§3.3 S1)

Recommended changes are in §11. The biggest ones: simulator primitives plus variable-step events (§11.6); the analysis ladder with multi-fidelity verification (§11.1); mode-aware evaluation (§11.1); new knob kinds (§11.2); electrothermal coupling (§11.1); power auto-specs (§11.4); and richer contracts (§11.5).

---

## 1. What changes when the circuit is a power converter

The engine's implicit assumptions (from v2 A8, Part B and the walkthrough), checked against power circuits:

| Engine assumption | Power reality | Consequence |
|---|---|---|
| A spec is a smooth function of the knobs | Metrics change slope or jump at mode boundaries: CCM/DCM, pulse skipping, current limit, hiccup, dropout, thermal foldback, input-DPM. PM jumps when crossovers multiply | Linearization, and the loop's convergence, only hold **within a mode** |
| One simulation is cheap (ms to s) | Soft-start to steady state is ~10³ switching cycles; hiccup is 16,384 cycles per retry; a charge cycle is hours | You can't simulate every point at full fidelity. You need levels (§9.2) |
| Knobs are fixed element values | C(V_dc, T, age), L(I, T), RDS(on)(T_J, V_GS), T_J(P) | Knob effects go through the operating point. Corner exactness no longer applies (§9.6) |
| Temperature is one shared range knob | Each hot part has its own temperature, set by its own and its neighbours' dissipation | Temperature splits into ambient (knob) plus solved states (§9.5) |
| Part models exist and are open | Vendor models are often encrypted (PSpice/LTspice), SIMPLIS-only, typical-only, or missing | Datasheet-parameterized behavioural templates plus an external-simulator bridge (§9.4) |
| Tolerances are "±x%" with a distribution | IC limits are over-temperature guaranteed bands with unknown distribution; many loop-critical parameters are typical-only | New knob kinds (§11.2) |
| A spec is a number from one analysis | "Total output tolerance" combines DC, ripple and transient. Stability is a for-all over frequency and load. Protection specs are event sequences | Composite measures, for-all specs, and event/mode specs (§11.3) |
| Blocks connect through voltage ranges | Power interfaces interact through impedance, ripple and inrush | Contracts need impedance, spectrum and start-up behaviour (§11.5) |

Independent WCA practice agrees with this list. A 2014 AEi Systems talk at a NASA NEPP workshop [Hym14] reports that **"30–75% of the WC analyses are non-compliant"**. It lists where problems are found in power supplies:
- poor stability, including "filter instability due to multiple converters/source impedance" and "LDOs and voltage references – ESR impacted performance";
- "hybrids not analyzed to correct source/loading impedance";
- "startup and shutdown – inrush, overshoot";
- "cross-conduction, efficiency, current limit, UVLO, switching frequency".

It also passes on a NASA lesson that "worst case environment doesn't always occur at the bounds". The compliance-matrix template in that talk (following the Aerospace TOR [Len13]) lists, for a power converter:
- stability (gain and phase margin, and negative input impedance);
- input filter damping;
- input under-voltage on/off threshold with "no oscillations";
- output ripple;
- switching-frequency tolerance;
- efficiency.

Several of these are not expressible in our current spec design.

---

## 2. Method and test vehicles

**Datasheets read (limits with conditions):**

| Part | Role | Key numbers used | Source |
|---|---|---|---|
| TI TPS54335A | 3 A, 28 V synchronous buck, peak current mode, external comp | V_REF 0.7936–0.8064 V @25 °C, **0.788–0.812 V over T_J −40…150 °C**; EA gm **1300 µS typ only**; COMP→I_SW **8 A/V typ only**; HS current limit **4.0 / 4.9 / 6.5 A**; LS sourcing limit 3.5/4.7/6.1 A; LS sinking limit 0 A; f_sw 384/480/576 kHz (R_T 100k); t_on,min 94/145 ns; internal soft-start 2 ms typ only; R_DS(on) HS 128/230 mΩ, LS 84/170 mΩ; T_SD 160 min; hiccup after 512 cycles, restart after 16,384; θJA 42.1 °C/W (JEDEC) | SLVSCD5D (Feb 2016) |
| TI TPS54331 | 3 A asynchronous buck (contrast) | V_REF 0.772–0.828 V (±3.5%); **f_sw 456–684 kHz at 25 °C only**; t_on,min at 25 °C only; EA gm 92 µS typ only; current limit 3.5–5.8 A | SLVS839H (Oct 2023) |
| TI TPS7A20 | 300 mA LDO | ±1.5% over line/load/T_J; V_DO ≤ 140 mV @300 mA; PSRR **typ** 45 dB @1 MHz (20 mA), 40 dB (300 mA), at 1 V headroom; **C_eff 0.47–200 µF, ESR ≤ 100 mΩ**; θJA 166–201 °C/W | SBVS338H (Jul 2024) |
| TI TPS25947 | 5.5 A eFuse | I_LIM ±10% (e.g. 1.80/2.03/2.20 A at 1.65 kΩ); **I_dVdt 0.81 / 2.21 / 3.82 µA**; UVLO 1.183/1.20/1.223 V; SR[V/ms] = I_inrush[mA]/C_out[µF], C_dVdt[pF] = 2000/SR | SLVSFC9C (May 2026) |
| TI LM74700-Q1 | Ideal-diode controller | V_AK(REG) 13/20/29 mV; reverse threshold −17/−11/−2 mV; t_reverse 0.45/0.75 µs; −65 V rating | SNOSD17G (Dec 2020) |
| TI BQ24072 | Linear Li-ion charger, power path | V_BAT(REG) 4.16/4.20/4.23 V; K_ISET 797/890/975 AΩ; USB500 limit 450/475/500 mA; V_OVP 6.4/6.6/6.8 V; V_IN-DPM 4.35/4.5/4.63 V; NTC bias 72/75/78 µA; V_COLD 2.0/2.1/2.2 V; V_HOT 0.27/0.30/0.33 V; T_J(REG) 125 °C typ (all over T_J 0…125 °C) | SLUS810N (Oct 2021) |

**Models (Appendix A):**
- L0: closed-form design equations.
- L1a: Ridley current-mode small-signal loop model [Rid91] with a gm Type-II compensator.
- L1b: large-signal averaged current-mode model with a current-limit clamp.
- L2: idealized switching model: synchronous switches, DCR, ESR, diode emulation, pulse skipping, cycle-by-cycle limit, soft-start, event-located turn-off. Plus shooting-Newton PSS with a finite-difference monodromy matrix.

These are what our engine would use at each level. They are not a substitute for vendor-validated models.

---

## 3. Synchronous buck: 12 V → 3.3 V at 3 A (TPS54335A-class)

### 3.1 The design and its knobs

**Design:**
- V_in 12 V ±10%; V_out 3.3 V; I_out 0–3 A; f_sw 480 kHz (R_T 100 kΩ).
- L 4.7 µH.
- C_out 3 × 22 µF 0805 6.3 V X5R.
- Feedback 31.6 kΩ / 10.2 kΩ (1%).
- Compensation R_c 3.57 kΩ, C_c 8.2 nF, C_hf 180 pF.

**Nominal:**
- V_out 3.278 V: **−0.65% from 3.3 V, just from snapping to E96**. At 3 A, EA finite gain costs another −5 mV.
- ΔI_L 1.11 A.
- f_c 45 kHz, PM 62°, GM 13 dB.

| Knob | Range | Kind (v2) | What it really is |
|---|---|---|---|
| V_in | 10.8–13.2 V | range | range; also **waveform** (line transients, hot-plug) |
| I_out | 0–3 A, steps 1.5→2.5 A @1 A/µs | range | **waveform**: step size, slew, repetition rate |
| T_ambient | −40…85 °C | range | range; part temperatures are **solved states** |
| life | 0–10 y | range | range (MLCC aging, resistor drift) |
| V_REF | 0.788–0.812 V | ? | **datasheet limit**: over T_J, distribution unknown. Must not also get a tempco |
| f_sw | 384–576 kHz | ? | datasheet limit |
| HS current limit | 4.0–6.5 A | ? | datasheet limit; also a **mode boundary** |
| EA gm, COMP→I_SW gain, slope comp, EA R_o | typ only / not given | ? | **typ-only**: spread *(assumed)* ±30%, ±20%, ±30%, 0.3–3 MΩ |
| R_DS(on) | 128/230 mΩ (HS) | ? | datasheet limit that **includes** the T_J dependence; use R_typ(T_J)·(1 + process) |
| L | 4.7 µH ±20% | statistical | statistical × **state-dependent** L(I): ×0.85–0.97 at 3–4 A *(rep.)* |
| C_out,eff | 17.3–39.2 µF (nominal 29.7) | statistical + range | **state-dependent**: DC-bias retention at 3.3 V 45% *(rep.)* ×(curve uncertainty ±10%) ×(tolerance ±20%) ×(X5R temp 0…−15%) ×(aging −5% at 10 y) |
| ESR | 0.5–3 mΩ | statistical | frequency-dependent |
| R_top, R_bot | ±1% | statistical | statistical + TCR (range) + drift (life) |
| θJA (board) | 35–55 °C/W *(assumed)* | — | **board assumption**: the PCB is out of scope, but the number dominates T_J. TI: applying JEDEC θJA to a real board "results in extremely erroneous values" [SPRA953] |

The C_out row is the clearest case of why DC bias is not a range term. One "22 µF" part becomes a product of five factors. Only one of them, the tolerance, is the kind of knob v2 imagined. The biggest factor, DC-bias retention, is fixed by the operating voltage. ADI tutorial 5527 [Fort5527] measured a 4.7 µF 6.3 V 0603 X5R at 5 V and 85 °C at **0.33 µF, −92.9%**; the bibliography's "4.7 µF becomes 0.33 µF" is that extreme case (79% of rated voltage, hot). Treating DC bias as a range term would take "worst = full derating" everywhere. That is wrong in both directions:
- it can't be more derated than the operating voltage implies;
- and "less C" is not always the worse case. Larger C is worse for inrush, for the LDO's 200 µF maximum, for input-filter resonance, and it lowers f_c.

### 3.2 What engineers actually spec for this rail

| # | Spec | Typical number | Level needed |
|---|---|---|---|
| S1 | DC output accuracy (line, load, temp, life) | 3.3 V ±2–3% | L0/L1 DC; exact corners on the divider |
| S2 | Output ripple (PWM mode) | ≤ 1% p-p (33 mV) | **L2 PSS** (the averaged model has no ripple) |
| S3 | Peak inductor current vs current limit and I_sat | I_pk < I_LIM,min and I_LIM,max < I_sat(hot) | **L2 PSS** + datasheet |
| S4 | Loop stability over line, load, temp, tolerances | PM ≥ 45°, GM ≥ 10 dB | L1a AC; L2 PSS + periodic AC near f_sw/2 |
| S5 | Load-step deviation (50% step, 1 A/µs) | ≤ ±5%, recovery ≤ 100 µs | L1b averaged transient; verify at L2 |
| S6 | Total tolerance, DC + ripple + transient | 3.135–3.465 V at all times | Composition + one L2 run at the joint worst point |
| S7 | Efficiency; junction temperature; no thermal shutdown | η ≥ 85% @3 A; T_J ≤ 125 °C @85 °C ambient | L2 PSS losses + electrothermal fixed point |
| S8 | Start-up: t_ss, monotonic, overshoot, pre-bias, UVLO/EN thresholds, PG, sequencing | t_ss 1–4 ms; overshoot ≤ 3%; no sink into 0–3.3 V pre-bias; start ≥ 9 V, stop ≤ 8.5 V | L3 transient; EN divider exact |
| S9 | Light-load mode and ripple; CCM boundary | ripple ≤ 50 mV in skip mode; CCM above X A | L2 PSS, mode-scoped |
| S10 | Short circuit: hiccup, survivability | I_pk < I_sat; T_J bounded | L3, many ms (hiccup = 16,384 cycles ≈ 34 ms per retry) |

### 3.3 Spec-by-spec walkthrough, with numbers

**S1: DC accuracy. Exact corners work, but knob classification decides the verdict.**

V_out = V_REF·(1 + R_top/R_bot) is linear in the divider conductances, so corners are exact (v2 A5).

| Treatment | V_out range (vs 3.300 V) | Against ±3% |
|---|---|---|
| V_REF band as worst case, R 1% (exact corners) | **−3.61% … +2.38%** | FAIL |
| V_REF band as worst case, R 0.1% | −2.29% … +0.99% | PASS |
| V_REF at 25 °C band only (±0.8%), R 1% | −2.92% … +1.67% | (misses temperature) |
| Realistic: V_REF band as range, R statistical σ = tol/3 | −3.22% … +1.92% | FAIL |
| V_REF treated as statistical (σ = 1.5%/3) | −2.50% … +1.19% | PASS |

The engine's math is fine here. The answer depends on **what kind of knob the V_REF band is**. The datasheet guarantees 0.788–0.812 V over the whole T_J range; it says nothing about the distribution, and nothing about how much of the band is temperature. Treating it as statistical is unjustified without vendor data. Adding a tempco on top of it double-counts. The −0.65% E96 snapping offset matters as much as either resistor. So the "nominal lands off-centre" item is a real budget line, and a job for the `?` solver (choose the pair that minimizes worst-case error, not nominal error).

**S2: Output ripple. It needs PSS; the worst case is 4× nominal.**

| | ΔI_L | V_out ripple | Source |
|---|---|---|---|
| Nominal | 1.11 A (PSS 1.17) | 10.9 mVpp (PSS **10.3**) | formula / L2 PSS |
| Worst corner: V_in 13.2, L ×0.8×0.85, f_sw 384 kHz, C_eff 17.3 µF, ESR 3 mΩ | 2.01 A (PSS 2.12) | 43.8 mVpp (PSS **40.3**) | formula / L2 PSS |

- Against ≤ 33 mVpp: nominal has 3× margin, the worst corner fails.
- Every factor is monotone, so a corner is right. But the factors are state-dependent: L(I) at the operating current, C(V) at the operating voltage.
- The averaged model has **no ripple at all**. The engine needs either a "ripple reconstruction" formula evaluated at the averaged operating point (L0), or PSS (L2).
- **Simulator accuracy:** Newton `reltol = 1e-3` (v2 Part D.3) means about 3 mV of error on a 3.3 V node, i.e. **30% of the 10 mV nominal ripple**. Ripple measures need reltol 1e-5 or better, or AC-coupled probes.

**S3: Peak current vs current limit. A mode change the averaged model cannot see.**

| | I_L,pk at 3 A | HS current limit (min / typ / max) |
|---|---|---|
| Nominal | 3.56 A (PSS 3.59) | 4.0 / 4.9 / 6.5 A |
| Worst ripple corner | 4.00 A (PSS **4.07**) | — |

PSS at the worst corner with the limit at its datasheet minimum (4.0 A): cycle-by-cycle limit engaged every cycle, average **V_out 2.921 V (−11%)**. The real IC then counts 512 limited cycles and enters hiccup, so the output switches off and retries.

Related checks at the same worst corner:
- The CCM boundary moves from 0.56 A (nominal) to 1.00 A.
- The inductor must not saturate at the *maximum* limit plus propagation-delay overshoot: 6.5 A + (13.2 − 3.28)/L_min × 100 ns *(assumed delay)* = **6.81 A**. That must be below I_sat at the inductor's hot temperature; ferrite I_sat drops with temperature *(rep.)*.
- Minimum on-time is fine here: t_on = 431 ns at 13.2 V and 576 kHz, vs 145 ns max. The same IC at 24 V → 1.0 V and 1.5 MHz +20% would need 23 ns, so it would pulse-skip.

This is the pattern that matters: **the spec fails by changing mode, not by drifting.** A DC or averaged analysis without the IC's limit logic reports a pass.

**S4: Loop stability. Mostly corners, but with a joint-flip trap and dominated by typ-only knobs.**

Knob box: 13 knobs (V_in, I_out 1–3 A, L, L_sat, C_eff, ESR, f_sw, gm, gcs, S_e, R_c, C_c, R_o).
- Nominal (geometric-centre C 26 µF): f_c 48 kHz, PM 59.7°, GM 12.9 dB.
- PM sensitivities (degrees over the full knob swing): C_eff +9.0, gm −8.6, gcs −5.6, f_sw +3.4, S_e −2.4, I_out +2.3, L −2.0; the rest < 1.

| Method | Min PM | Simulations |
|---|---|---|
| Line from nominal (predicted) | 24.0° | 27 (FD; adjoint would be 2) |
| Simulate the predicted corner (loop step 3) | **16.3°** (f_c 85 kHz, GM 4.0 dB) | +1 |
| Re-linearize there (one-at-a-time slopes) | no flip suggested, stays 16.3° | +13 |
| Brute force, 2⁸ corners over the top-8 knobs | **14.7°** (f_c 117 kHz, GM 2.0 dB, modulus margin 0.16) | 256 |
| 3,000 random interior points | 24.2° (never beats the corners) | 3,000 |
| f_c spread across the box | **21.7 – 117 kHz (5.4×)** | — |

**The trap.** The true worst corner differs from the predicted one in L, L_sat and S_e. All three act on the design **only through the slope ratio m_c = 1 + S_e/S_n** (S_n ∝ (V_in − V_out)/L), which sets the Q of the f_sw/2 sampling pole, Q_p = 1/(π(m_c·D′ − 0.5)) [Rid91].

| Flip at the predicted corner | PM | m_c | Q_p |
|---|---|---|---|
| none | 16.3° | 1.95 | 0.37 |
| L only | 17.0° (better) | 1.63 | 0.50 |
| S_e only | 15.9° | 1.51 | 0.58 |
| L + S_e | 15.2° | 1.34 | 0.74 |
| L + S_e + L_sat | **14.7°** | 1.30 | 0.79 |

At the predicted corner, PM vs S_e is flat (15.90, 16.11, 16.23, 16.26, 16.26 for ε = −1…+1). The one-sided slope at the corner is ≈ 0, yet the far edge is worse. Flipping L alone *raises* PM. No one-knob-at-a-time test finds this corner. It is not a big error in degrees, but GM goes from 4.0 dB to 2.0 dB.

**What decides the verdict:**

| Variant | Min PM over the box |
|---|---|
| All knobs | 14.7° |
| IC typ-only knobs (gm, gcs, S_e, R_o) fixed at typical | **37.8°** |
| … and C_eff also fixed at its centre value | 49.8° |
| "Naive" design compensated for 3 × 22 µF = 66 µF (no DC bias), real C_eff 29.7 µF | f_c 89 kHz, PM **34°** |
| Same naive design at C_eff 17.3 µF | f_c 130 kHz, PM **11.8°**, GM 2.0 dB |

So the stability verdict comes from (a) parameters the datasheet doesn't bound and (b) a capacitor effect the part number doesn't show. The engine's job is to make both visible:
- "about half of the PM loss comes from assumed IC spreads" (47% of the linear worst-case sum; 23° of the 45° drop);
- "C_eff is ~45% of the marked value at 3.3 V" *(rep.)*.

**Method note.** Each point of the averaged AC model costs about a millisecond. Brute-force corners over the top 8–12 knobs is affordable and **strictly better than the loop** here. The loop is for expensive metrics.

**S5: Load-step deviation. The loop works; verify at switching level.**

Step 1.5 → 2.5 A at 1 A/µs:
- Nominal: averaged model −133 mV; switching −130 mV raw, −122 mV cycle-averaged.
- Sensitivities (mV over the full swing): gm +24.8, C_eff +20.7, gcs +16.2, f_sw +6.1, S_e −4.3, L −4.0.
- The line predicts −211 mV. The averaged model at the predicted corner gives **−245 mV** (line 16% optimistic).
- Brute force over the top-6 knobs picks **the same corner** (86 averaged simulations in total).
- Switching verification at that corner: **−238 mV raw / −230 mV cycle-averaged**. That's 7.3% of V_out: a proven FAIL of a ±5% transient spec, with a counterexample.

Model-level mismatch shows up where it matters. For a 1.5 → 3.0 A step:
- at nominal, averaged −183 mV vs switching −176 mV (fine);
- at the low-PM corner (C 17.3 µF, gm +30%, gcs +20%, S_e −30%, f_sw 384 kHz, L min, V_in 10.8), averaged **−168 mV** vs switching **−218 mV raw**.

The averaged model leaves out the sampling pole, so it misses the ringing that low PM causes. **The averaged model is least accurate exactly at the corners the loop drives it to.** That's the argument for verifying one level up, and for carrying the L1/L2 gap as a model-form error term [RWBRB21b].

**Kink.** Worst ripple corner, current limit at 4.0 A, step from 1.0 A to I₁:

| I₁ (A) | 2.2 | 2.4 | 2.6 | 2.8 | 2.9 | 3.0 |
|---|---|---|---|---|---|---|
| Cycle-averaged undershoot (mV) | −177 | −205 | −232 | −258 | −270 | −329 |
| Current-limited cycles | 0 | 0 | 1 | 5 | 41 | 41 |

The metric is continuous, but its slope changes by about 4× at the current-limit boundary. A line drawn at 2.2 A predicts −289 mV at 3.0 A; the actual value is −329 mV, so the line is 40 mV optimistic, and the output has in fact collapsed into limit.

**S6: Total output tolerance. Where affine composition helps, and where it doesn't.**

Spec: V_out ≥ 3.135 V (−5%) at all times, including ripple and the 1 A step.
- DC depends on {V_REF, R_top, R_bot, EA gain}.
- Ripple depends on {V_in, L, L_sat, f_sw, C_eff, ESR}.
- Transient depends on {gm, gcs, C_eff, f_sw, S_e, L, V_in, R_c, …}.

Composed with shared knobs:

| | DC | − transient (cycle-avg) | − ripple/2 | = min V_out |
|---|---|---|---|---|
| Nominal | 3.273 V | 0.122 V | 0.005 V | **3.146 V (−4.7%)**: pass by 11 mV |
| Worst | 3.176 V | 0.230 V | 0.020 V | **2.926 V (−11.3%)**: FAIL |

- DC and transient share no knobs, so here the affine sum equals the naive sum of worst cases.
- Ripple and transient share C, f_sw and V_in (same worst sign) and L (**opposite** signs: ripple is worst at L_min, transient at L_max). Named knobs save a few mV there.
- The honest verdict still needs **one L2 run at the joint worst point**, because DC offset, ripple phase and the transient don't superpose exactly once a mode changes.
- The composition's value here is to find the right joint point cheaply and to rank contributors across all three sub-budgets.

**S7: Efficiency and junction temperature. Temperature is a solved state, not a knob.**

Loss model: conduction I_rms²·(D·R_HS + (1 − D)·R_LS) with R(T_J) = R₂₅(1 + 0.0045·(T_J − 25)) *(rep.)*; switching 0.5·V_in·I·(20 ns)·f_sw; dead time; gate/quiescent; inductor DCR and core. At 3 A and 25 °C: **η ≈ 86%**.

Electrothermal fixed point T_J = T_A + θJA·P(T_J):

| Case | T_J | Thermal loop gain → amplification |
|---|---|---|
| 25 °C, typical R_DS(on) | 81 °C | 0.17 → ×1.20 |
| 85 °C, typical R_DS(on) | **153 °C** | 0.17 → ×1.20 |
| 85 °C, R_DS(on) +15% (= datasheet max at hot) | **163.5 °C** (above T_SD min of 160 °C) | 0.19 → ×1.24 |
| 85 °C, board θJA 55 °C/W | 195 °C, i.e. no normal-mode solution: **thermal-shutdown mode** | 0.25 → ×1.33 |
| 60 °C, R_DS(on) +15% | 133 °C | ×1.24 |

- Fixed-point iteration converges in 7–10 steps.
- Runaway (loop gain ≥ 1) would need θJA ≈ 221 °C/W, which is unrealistic for this IC. It is very realistic for FETs in linear mode (§6).
- Two double-counting traps:
  - the datasheet's R_DS(on) max is at hot T_J, so applying a tempco on top of it double-counts;
  - the T_A sensitivity is ×1.2–1.33 larger than the "knob" view implies.
- The verdict at 85 °C depends mostly on **θJA, which belongs to the board**. That must be a declared assumption with a range, because JEDEC θJA is a comparison figure [SPRA953].

**S8: Start-up, UVLO, pre-bias, sequencing.**
- EN/UVLO divider: linear, so corners are exact. But the EN pull-up (1.15 µA) and hysteresis current (3.3 µA) are **typ only**, and the rising threshold has only a max (1.28 V).
- UVLO start/stop windows are therefore partly assumed.
- The **automatic spec that matters**: UVLO hysteresis > I_in,start·R_source + input ripple. Otherwise the converter oscillates on and off during a slow input ramp. (That is the TOR's "no oscillations" line item.)
- Pre-bias: the TPS54335A's LS sink limit of 0 A means diode emulation, so start-up into a pre-biased output is monotonic. The engine needs **initial conditions as a range knob** (pre-bias 0…V_out) and an L3 transient with the IC's soft-start logic.
- Soft-start time is **typ only** (2 ms). A sequencing spec against another rail's power-good can only be verified with an assumed spread.

**S9: Light load.**
- Pulse skipping starts below a 0.5 A peak command (typ).
- DCM starts at I_out = ΔI_L/2: 0.56 A nominal, 1.00 A worst.
- Ripple at 0.5 / 0.3 / 0.1 A (PSS): 9.8 / 8.5 / 4.4 mVpp, all DCM.
- A ripple spec written for PWM mode is **undefined** below the mode boundary. The boundary moves with the knobs, so the spec must be scoped by mode, and the boundary becomes a spec of its own ("CCM for I_out ≥ 1.2 A at all corners").

### 3.4 Buck: what broke and what was weak

| Problem | Where | Severity |
|---|---|---|
| No simulator primitives (switch/MOSFET, comparator, controlled sources, events) | everything past S1 | blocking |
| Knob-kind ambiguity for IC limits and typ-only parameters | S1, S4, S5, S8 | decides verdicts |
| State-dependent C(V) and L(I) modelled as knobs | S2, S4, S5 | wrong kind; can also be wrong direction |
| Mode changes (current limit, DCM, skip, hiccup, TSD) | S3, S5, S7, S9 | the real failures |
| Averaged model inaccurate near f_sw/2 | S4, S5 at bad corners | optimistic exactly where it matters |
| Joint flips through a shared intermediate (m_c) | S4 | small here; see boost |
| Thermal coupling; board θJA dominates | S7 | decides verdicts |
| Solver tolerance vs mV ripple | S2 | 30% error |
| Long simulations (soft-start, hiccup) | S8, S10 | cost |

---

## 4. Boost: 5 V → 12 V at 1 A

**Design:**
- V_in 4.5–5.5 V; I_out 0.3–1 A; f_sw 600 kHz ±15%; L 4.7 µH ±20% (L_sat 0.85–0.97).
- C_out 3 × 10 µF 1206 25 V → **7–16 µF effective at 12 V** *(rep.)*.
- Peak current mode: 1/R_i = 5 A/V ±20%, S_e 1.5 A/µs ±30% *(assumed)*, gm 0.5 mS ±30% *(assumed)*.
- Compensation R_c 12 kΩ / C_c 6.8 nF / C_hf 120 pF.
- Model: current-mode boost with ideal inner loop and sampling double pole [EM20, Rid91].

**Specs engineers write:**
- V_out ±3%; ripple ≤ 1%.
- PM ≥ 45° at V_in,min and I_out,max.
- Peak switch current < I_LIM,min at V_in,min (input current ≈ 12·1/(0.9·4.5) = 2.96 A plus ΔI_L/2).
- C_out rms current (I_out·√(D/(1 − D)) = 1.40 A at D = 0.66) within MLCC rating.
- Slope-compensation adequacy at D_max.
- Plug-in inrush and overshoot.
- Output short behaviour. There is no inherent protection: the body diode connects input to output.

**Walkthrough, loop stability:**
- Nominal: f_c 15.8 kHz, PM 67.9°, f_RHPZ 96.6 kHz. Looks conservative (f_c = f_RHPZ/6).
- **f_RHPZ = R(1 − D)²/(2πL) spans 39.8–339 kHz across the corners.** The low end comes from V_in min, I_out max and L max, all range knobs or statistical, none exotic.
- Line from nominal predicts PM min **45.5°**. Simulating that corner gives **4.0°**. Loop step 3 catches it: a proven FAIL with a counterexample.
- Re-linearizing: round 1 → 4.0° (flip V_in, S_e); round 2 → −5.0°; round 3 → **−8.1° (unstable)**; round 4 goes back to −5.0°. **The loop cycles.**
- Brute force over 512 corners confirms **−8.1°** at V_in min, I_out max, L max, L_sat max, C min, f_sw min, gm max, gcs max, S_e max. The worst interior point is 34.3°.
- **Why V_in flips.** At nominal, higher V_in raises loop gain (larger D′), so the V_in slope says "high V_in is worse". Near the worst region, f_c is close to the RHP zero, and low V_in lowers the zero faster than it lowers the gain. This is the v2 "slope at nominal points to the wrong corner" trap, on a textbook power effect.
- **Slope compensation.** With an earlier S_e = 0.5 A/µs, the *nominal* Q_p was 9, next to subharmonic oscillation. Whether the part works at all is set by an internal IC parameter that datasheets usually don't specify.

**Plug-in inrush** (undamped L–C through the body diode when V_in steps):
- I_pk = V_in·√(C/L) = **8.0 A nominal, 11.3 A** at 5.5 V, C 16 µF, L 3.76 µH.
- V_out overshoots to about 2·V_in (10.5 V).
- Automatic specs: inductor I_sat, diode/FET surge rating, and downstream abs-max during plug-in.
- This is an L3 event transient, and it depends on source impedance: cable inductance is a range knob.

**Boost-specific lessons:**
1. The engine needs **loop memory** (visited corners, best-so-far, cycle detection).
2. Derived-quantity specs are more robust than PM alone, and cheap: f_c ≤ f_RHPZ,min/k, and Q_p ≤ Q_max at D_max.
3. DCM at light load changes the small-signal structure. The averaged model must be mode-aware (Vorpérian's PWM-switch model covers CCM and DCM [Vor90a, Vor90b]).

---

## 5. LDO (TPS7A20)

**Specs engineers write:**
- V_out ±1.5–3% (the datasheet guarantees ±1.5% over line/load/T_J).
- Dropout margin at V_in,min, I_max and hot.
- PSRR at the upstream converter's f_sw (≥ 40 dB at 500 kHz is a common ask).
- Output noise (7 µVrms typ).
- Load-transient deviation.
- Stability window: C_eff 0.47–200 µF, ESR ≤ 100 mΩ.
- T_J ≤ 125 °C; no thermal shutdown (165 °C typ).
- Foldback and short-circuit current.

**Walkthrough:**

*Thermal (L0, exact).* P = (V_in − V_out)·I is linear in the range knobs, so corners are exact. For 5 V → 3.3 V at 250 mA:

| V_in | θJA 166 °C/W | θJA 201 °C/W |
|---|---|---|
| 5.00 V | T_J 131 °C @60 °C ambient; max T_A for T_J ≤ 125: **54 °C** | 146 °C; max T_A **39 °C** |
| 5.25 V | 141 °C; 44 °C | 158 °C; 27 °C |
| 5.50 V | 151 °C; 34 °C | **171 °C** (thermal shutdown); **14 °C** |

The verdict flips on θJA (package *and* board) and on V_in max. The engine's math is trivial. The inputs are the problem: θJA is a board assumption, and V_in max should come from the upstream block's **guarantee** through the contract, not a guess.

*Stability window (automatic spec).* A "1 µF" 0402 6.3 V X5R at 3.3 V has C_eff **0.33–0.66 µF** *(rep.: DC-bias retention 45–60%; ±10% tolerance; X5R −15%; −5% aging)*. The datasheet minimum is 0.47 µF effective, so it FAILs at the worst corner. The same case size at 2.2 µF gives 0.72 µF min. This is the AEi "ESR/capacitor-impacted LDO stability" problem [Hym14]. It is invisible if DC bias isn't modelled as a function of V. An old PNP/PMOS LDO with an ESR stability window [SLVA115] adds an ESR *lower* bound as well, so the check becomes a 2-D region test on (C_eff, ESR).

*PSRR composition (contract check).* Buck ripple 10.3–40.3 mVpp at 384–576 kHz, times PSRR:
- 40 dB (datasheet typical at 1 V headroom) → 0.10–0.40 mVpp;
- 25 dB (plausible at 0.25 V headroom *(rep.)*) → 0.58–**2.27 mVpp**.

PSRR is typical only, and it falls with headroom, which depends on the upstream block's worst-case low output. So this spec **can't be guaranteed from datasheets at all**. The engine should say that ("depends on unguaranteed typical data"), not report a PASS.

**Where the LDO is weak for the engine:**
- LDO loop internals belong to the IC vendor. The board designer only sees a stability region, so the automatic spec is a *region membership* test.
- PSRR(f, headroom, I) is a typical curve, so a knob family with no guarantee.
- Dropout and ground current rise with T_J (a mild thermal loop).
- Dropout is itself a **mode**: in dropout the output follows the input, and all line/PSRR specs become meaningless.

---

## 6. Load switch / eFuse / inrush limiting (TPS25947)

**Specs engineers write:**
- Inrush ≤ X A (connector, upstream supply).
- Start-up time within a window (sequencing).
- FET stays inside SOA and T_J, including during start into a load that is *already drawing current*.
- Current-limit / circuit-breaker threshold vs normal peaks.
- Fast-trip response.
- UV/OV thresholds.
- Hot-plug overshoot below abs-max.
- Reverse-current blocking.

**Walkthrough: inrush and start-up time.** Design C_dVdt = 2.2 nF (SR_typ 0.91 V/ms from SR = 2000/C_dVdt[pF]). The datasheet's I_dVdt spread (0.81–3.82 µA around 2.21 typ) makes SR vary **×0.37 to ×1.73**. C_load is 400–600 µF (bulk electrolytic ±20% plus derated downstream MLCCs).

| C_load | I_dVdt min | typ | I_dVdt max |
|---|---|---|---|
| 400 µF | 133 mA, 36 ms | 364 mA, 13.2 ms | 629 mA, 7.6 ms |
| 600 µF | 200 mA, 36 ms | 545 mA, 13.2 ms | **943 mA**, 7.6 ms |

- Inrush and start-up time are monotone in both knobs, so corners are exact.
- The *design equation* in the datasheet uses typicals only. Engineers who use it as printed get a 0.55 A design that can draw 0.94 A.
- The FET absorbs ½CV² = 29–43 mJ **regardless of SR**.

**The real failure: the downstream converter starts during the ramp.**
- A downstream buck with UVLO at 4.5 V starts switching while the eFuse output is still ramping. Its constant-power 11 W load then draws 2.44 A, more than I_LIM (1.80–2.20 A).
- The eFuse enters current limit with (12 − 4.5)·2.2 = **16.5 W** in its FET, reaches thermal shutdown, and retries. The system may never start.
- With UVLO at 6.0 V: 1.83 A vs I_LIM,min 1.80 A, marginal.
- Evaluating this needs:
  - a system-level L3 transient with a **constant-power (negative-resistance) load model** of the downstream block;
  - the eFuse's mode logic (limit, thermal shutdown, retry);
  - transient thermal impedance Z_th(t);
  - a contract that tells the eFuse block what the downstream block demands during start-up.
- The correct automatic spec is a sequencing constraint: downstream enable ≥ eFuse PG, or downstream UVLO ≥ a threshold at which the demand stays below I_LIM,min.

**Thermal instability in linear mode.** While limiting, the FET runs in saturation at low current density. Below the zero-temperature-coefficient point, current rises with temperature, which is positive feedback. Nexperia AN11158 [AN11158] calls this out explicitly ("linear mode derating … positive feedback, and potential thermal runaway", "Spirito effect" [SBDR02]). TI's hot-swap note [SLVA673] warns that SOA curves are at T_C = 25 °C and must be derated, and that R_DS(on) is a function of T_J.

This is exactly the case where the electrothermal loop gain can reach 1. The engine's thermal fixed point must be able to report "**no fixed point: runaway**" as a FAIL with a counterexample, and the SOA check must use the transient thermal network, not a DC θ.

**Hot-plug.** With ceramic input capacitors and cable inductance, a hot-plug overshoots. LT AN88 [Per01] is the standard reference. I verified its title, author and date but not its exact overshoot figure; the physical upper bound for an undamped LC is 2×V_in.
- Knobs: cable inductance and resistance (range: a *setup* assumption), C_in(V), TVS clamp.
- Spec: V_in,peak < abs-max of every IC on the rail, which is automatic.
- This is an L3 event transient, and 2×V_in can be above the eFuse's 28 V abs-max for a 20 V USB-PD input.

---

## 7. Reverse-polarity protection (12 V, 3 A)

**Options and numbers:**
- Series Schottky: V_f 0.45 V → **1.35 W**.
- P-FET high side: 20 mΩ at V_GS = −10 V → 180 mW; at cold crank with V_GS ≈ −6 V, R_DS(on) 30–40 mΩ *(rep.)* → 270–360 mW.
- Ideal-diode controller LM74700-Q1 + N-FET: regulates V_AK to 13–29 mV. With a 5 mΩ FET at 3 A (15 mV) it is **in regulation mode**, the FET partly enhanced, P = 60 mW. With 10 mΩ (30 mV) it is in full conduction, 90 mW.

**Specs engineers write:**
- Forward dissipation and T_J at I_max and T_A,max.
- Reverse leakage at −V_batt, sustained (ISO 16750-2 defines reverse-voltage tests; exact levels not verified here).
- Gate–source voltage within rating at maximum input, including a clamped load dump: Zener 12 V ±5% plus tempco → **11.1–13.4 V** vs V_GS,max 20 V.
- Full enhancement at V_in,min (cold crank).
- Reverse-current turn-off time during input micro-cuts: t_reverse 0.45–0.75 µs plus gate discharge.
- Hold-up of downstream rails during input interruptions.

**Where it breaks:**
- **Modes.** The ideal diode has three regimes: regulation (linear), full conduction, and reverse blocking. The boundary (I·R_DS(on) = V_AK(REG)) moves with load, T_J and R_DS(on) spread. Dissipation is kinked at the boundary. The reverse threshold (−17…−2 mV) sets when it blocks, which matters with an AC ripple current on the input.
- **Temperature slope flips.** For a P-FET at high |V_GS|, R_DS(on) rises with T, so hot is worse. Near threshold at cold crank, drain current rises with T (below ZTC), so cold is worse. The worst temperature depends on V_in: the v2 A5 "shared-knob hump" case on a real part. The engine must split over (V_in, T), not assume a direction.
- **Events.** Reverse turn-off, ISO 7637-2 pulses and input micro-cuts are µs-scale events inside ms-scale waveforms. They need an L3 transient with a variable step and breakpoints. The bus waveforms are *standardized range knobs*, which argues for a library of standard stimuli (ISO 7637-2, ISO 16750-2, LV 124, USB-PD transitions) as named setups. Levels are to be verified from the standards themselves.

---

## 8. Li-ion charger front-end (BQ24072, USB input)

**Specs engineers write:**
- Charge voltage 4.20 V within the cell's limit (e.g. 4.15–4.25 V); safety-critical.
- Fast-charge current ≤ 1 C (cell) and ≥ a minimum (charge time).
- Input current ≤ USB limit (500 mA configured / 100 mA unconfigured; BC 1.2 DCP higher).
- Input OVP threshold and transient withstand at hot-plug.
- Temperature window: no charging below 0 °C, reduced or none above 45 °C (JEITA-style, not verified here).
- Termination current.
- Pre-charge threshold.
- Safety timers.
- Thermal regulation.
- System voltage during heavy load (power path).

**Walkthrough.**

*Charge voltage.* V_BAT(REG) 4.16–4.23 V over T_J 0–125 °C. This is a **datasheet limit**, so worst case applies directly: PASS against 4.15–4.25 V. The sense path (connector, protection FETs) adds I·R during constant current but not in constant voltage at termination. That needs a mode-scoped spec.

*Charge current* (the exact method plus the `?` solver). I_CHG = K_ISET/R_ISET is a single-use monotone chain, so corners are exact.
- R_ISET = 890 Ω (1%): **0.887–1.107 A**, so 1 C for a 1 Ah cell is exceeded.
- The solver finds R_ISET,nom ≥ 985 Ω and picks **1.00 kΩ**: 0.789–0.985 A.
- This is v2 A9 exactly, and it works unchanged.

*Actual charge current is a min() over modes.* I_bat = min(I_CHG, I_IN,lim − I_sys, thermally-regulated, V_IN-DPM-limited).
- At V_IN 5.25 V, V_BAT 3.0 V, 0.89 A: P = 2.0 W. With θJA 40 °C/W *(assumed)*, T_J(REG) = 125 °C is reached above **45 °C ambient**. The CC-phase current is then set by the thermal loop, not by R_ISET.
- With USB500 (450–500 mA), the input limit is the active constraint anyway.
- The measure is piecewise-smooth, and *which piece is active* depends on the knobs. Mode-aware evaluation is essential.
- "Charge time" is an integral over hours. It should be evaluated as a **quasi-static sweep over the slow state V_BAT** (∫dQ/I(V_BAT, mode)), not as a transient.

*NTC temperature window* (exact through monotone chains). V_TS = I_NTC·R_NTC(T). Knobs: V_COLD, I_NTC, R₂₅ ±1%, B ±1% (10 k, B = 3435 *(rep.)*). Each knob appears once and monotonically, so the 16 corners are exact.

| | Nominal | Exact range |
|---|---|---|
| Cold cut-off | +0.5 °C | **−1.8 … +2.9 °C**: FAIL against "no charging below 0 °C" |
| Hot cut-off | 50.8 °C | **46.1 … 55.9 °C** |

Add a range knob for the thermal offset between NTC and cell, since the NTC is not the cell.

*USB input current.* The 450–500 mA guarantee sits exactly at the USB limit: compliant by the datasheet, with zero margin and zero design freedom. The contract check is trivial but must exist.

*OVP and hot-plug.* V_OVP 6.4–6.8 V. A USB cable hot-plug rings, the same AN88 physics as §6. The 28 V input rating is the abs-max auto-spec.

**Where it breaks:** the circuit is a **state machine** (pre-charge → CC → CV → termination; plus DPM, thermal regulation, OVP, timers). Most specs are "in mode X, quantity Y", and the rest are "mode transitions happen at thresholds within Z". The engine needs a behavioural state-machine model of the IC, parameterized by datasheet limits, with mode as an output. A transistor-level model adds nothing here.

---

## 9. Cross-cutting: where the engine breaks and what to do

### 9.1 Switching makes metrics non-smooth and event-driven

| Metric | Smooth? | Why | Handling |
|---|---|---|---|
| Averaged DC and AC quantities | yes, within a mode | — | adjoint as planned |
| PSS ripple p-p, peak current | yes within a mode; kink when the argmax jumps | switching instants depend on parameters | event sensitivities: dt*/dp = −(∂g/∂p)/ġ at t* [GFB99]; PSS sensitivity via monodromy (below) |
| Efficiency | yes in CCM; kink at DCM/skip boundary | loss terms switch | mode-scoped |
| Peak undershoot with current limit | continuous, slope ×4 at onset (§3.3 S5) | mode change | detect and split |
| Settling time, number of skipped pulses, hiccup yes/no | discontinuous | integer or boolean | smooth surrogates (envelope at t), or derivative-free bracketing |
| PM | discontinuous when crossovers multiply | implicit definition | modulus margin; for-all over f (§9.8) |

**PSS sensitivities are cheap.** Shooting-Newton already builds the monodromy matrix Φ = ∂P/∂x₀. The steady-state sensitivity is dx*/dp = (I − Φ)⁻¹·∂P/∂p: one extra linear solve plus one forward-sensitivity period. On the buck, dx*/dC this way matched re-solving the PSS to 0.3% for the voltages and about 20% for the inductor current. The difference is finite-difference noise at events; analytic saltation matrices [LN04, BBS13] would remove it.

This gives the worst-point loop its sensitivities at PSS level for roughly **the cost of one extra period**, much cheaper than a transient adjoint over a full start-up ([Xyce16]: each point-in-time objective needs its own backward pass).

### 9.2 Averaged models vs PSS vs full transient: an analysis ladder

| Level | What | Good for | Fails for | Cost (this study) |
|---|---|---|---|---|
| L0 | Design equations (app-note formulas) | auto-specs, first bounds, ripple/peak estimates, thermal, inrush | anything dynamic or mode-dependent | µs |
| L1 | Averaged large-signal plus small-signal (state-space averaging [MC76], PWM switch [Vor90a, b], current-mode [Rid91, TM95, YLM12], COT [LL10, RS09]) | DC operating point, loop gain, load-step and soft-start envelopes, exhaustive corners | ripple, peaks, efficiency, anything near f_sw/2, event protection | ms per AC point; 0.1–0.5 s per transient in Python |
| L2 | PSS on the switching model (shooting [AT72a, TKW95]; SIMPLIS POP [SIMPLIS]) plus periodic small-signal (PAC/loop gain at the PSS [OTIS93, TKEW96]; SIMPLIS POP + AC) plus Floquet multipliers | ripple, peak current, current-limit engagement, efficiency, accurate loop gain up to f_sw/2, subharmonic stability, PSS sensitivities | start-up, faults, non-periodic events | **21 periods from a good guess** (vs ~1,160 for brute force); 31 with a 20-cycle warm start |
| L3 | Full switching transient, variable step, events (PLECS/SIMPLIS-style piecewise-linear [BV92, AH99] or SPICE) | start-up, pre-bias, hiccup, hot-plug, sequencing, fault response; verification of counterexamples | cost | ~300 steps/cycle × 10³–10⁵ cycles |

Two refinements from the literature:
- **Envelope following** [WL91] (and generalized averaging [SNLV91]) sits between L1 and L3 for slow transients like soft-start and thermal ramps.
- **Multi-fidelity rule.** The loop searches on the cheapest valid level and verifies the predicted worst point **one level up**. The discrepancy between levels at the same point goes into the affine form as a named model-form error term, not into the generic `err` bucket. Rashidi et al. treat model-form uncertainty as a first-class quantity in converter design optimization [RWBRB21b].

PSS robustness lessons from this study:
- **Warm start.** Shooting from a poor guess (compensator voltage too low, so current limit engages inside the one-period map) *failed*. 20 transient cycles first, then Newton, converged in 2 iterations. SIMPLIS POP likewise needs a trigger and a sensible start [SIMPLIS].
- **Existence is not stability.** With weak slope compensation, shooting converged to a period-1 orbit with Floquet multiplier **−1.18**: an *unstable* orbit that hardware never settles to. It shows up as subharmonic oscillation. Every PSS result must carry its multipliers.
- **Non-convergence can be information.** When slope compensation is removed at D ≈ 0.7, shooting doesn't converge because no stable period-1 orbit exists. The engine must tell "numerical failure" apart from "no period-1 steady state", and try a period-2 PSS or a transient.

### 9.3 Very long simulations
- Buck soft-start: 2 ms = 960 cycles; plus ~200 cycles to settle.
- Hiccup: 512 + 16,384 cycles per retry (≈ 35 ms at 480 kHz).
- eFuse slow corner: 36 ms of ramp with a switching load.
- Charge cycle: hours. Thermal: seconds to minutes.

**Our simulator today** is fixed-step (`trans.rs:14`) with no breakpoints. Resolving 5–10 ns switching edges means about 1 ns steps everywhere: 3 × 10⁶ steps for 3 ms. Trapezoidal integration also rings at switching discontinuities.

Fixes, roughly in order of value:
1. variable step with LTE control plus breakpoints at source edges and event location for internal thresholds;
2. PSS shooting for anything periodic;
3. averaged models for envelopes;
4. quasi-static sweeps for slow states (V_BAT, T);
5. an optional ideal-switch piecewise-linear mode (SIMPLIS/PLECS-like: exact exponentials between events, orders of magnitude faster);
6. thermal networks integrated at their own slow time scale (multirate).

### 9.4 Vendor models: encrypted, PSpice/SIMPLIS-only, typical-only

Industry practice (not verified item by item here): TI ships PSpice models (some encrypted, some averaged vs transient variants) and SIMPLIS models for some parts; ADI/LT ship LTspice models that run only in LTspice; IP encryption follows IEEE 1735 [IEEE1735]. Even open vendor models are **typical-only**: none carries the datasheet min/max for V_REF, gm, current limit or f_sw.

**Recommendation:**
1. **Datasheet-parameterized behavioural templates** per IC class (buck-PCM, buck-COT, buck-VM, boost-PCM, LDO, eFuse, ideal diode, linear/switching charger). Each parameter carries its guarantee type (tested limit / design limit / typ-only / curve) and conditions (T_J range, V_in). This is what vision §5 ("AI-assisted part modeling") should produce for power ICs.
2. **External-simulator bridge** (vision §3 already plans one) for encrypted models. Use it *only* for verification points (inner bounds and counterexamples), with finite-difference sensitivities if at all.
3. **Calibrate** each template against the vendor model or EVM data at nominal (loop-gain Bode, load step, efficiency curve). The residual becomes the model-form error term.

### 9.5 Thermal feedback

The fixed point T_J = T_A + Z_th·P(T_J, …) couples every temperature-dependent parameter to power:
- R_DS(on) and V_f;
- IC V_REF within its band, current limit, f_sw;
- inductor I_sat, which drops with temperature;
- MLCC C(T), electrolytic ESR(T) and life (Arrhenius);
- LDO dropout; charger thermal regulation.

The engine should:
- make **T_A the knob** and part temperatures **solved states** from a thermal RC network. Board θ comes from an assumption, because the PCB is out of scope, but it needs a range;
- solve the electrothermal system jointly: DC fixed point, transient multirate [WCSW97, MH97];
- compute sensitivities including the thermal loop: amplification 1/(1 − θ·∂P/∂T), ×1.2–1.33 for the buck;
- report runaway (no fixed point) as FAIL;
- link part tempcos to *local* temperature, e.g. an MLCC next to the inductor runs hotter than ambient.

### 9.6 Operating-point-dependent parameters

MLCC C(V_dc, T), inductor L(I), FET C_oss(V_ds) and R_DS(on)(V_GS, T_J), diode V_f(I, T):
- These are **device models in the simulator**, charge- or flux-based so charge is conserved, with a few knobs on the curves (vendor-curve uncertainty, tolerance, aging).
- The engine's chain rule then goes through the simulator's Jacobian, which the adjoint handles.
- They break the corner theorem's premise (a fixed element value). The **exactness** claims (A5) must be withdrawn for any quantity that depends on such an element's operating point. In power circuits that is most AC and transient quantities.

### 9.7 Multiple operating modes

Modes seen in this study:
- buck: PWM-CCM, DCM (diode emulation), pulse skip, cycle-by-cycle limit, hiccup, thermal shutdown, dropout (100% duty with boot refresh);
- LDO: regulation / dropout / current limit / foldback / thermal shutdown;
- eFuse: dV/dt ramp / current limit / circuit breaker / fast trip / OV clamp / thermal shutdown / retry;
- ideal diode: regulation / full conduction / blocking;
- charger: pre-charge / CC / CV / done, with overlays DPM, input limit, thermal regulation, OVP.

**Engine requirements:**
1. Every simulated point returns a **mode trace** (from template state variables and device regions).
2. The loop checks mode consistency between the linearization point and the verification point. If they differ, split the box along the mode boundary and linearize per mode.
3. Specs can be **scoped by mode**.
4. **Automatic mode-boundary specs** ("never current-limit under the declared load profile", "never thermal-shutdown at T_A,max", "no dropout at V_in,min", "CCM for I_out ≥ X").
5. The mode boundary is itself a smooth surface in knob space. The worst-case-distance method applies to "distance to the boundary" [AGW94].

### 9.8 Stability as a spec

1. **Loop gain source.** L1 averaged AC via Middlebrook/Tian injection [Mid75, TVHK01]; L2 periodic small-signal at the PSS for accuracy near f_sw/2 [OTIS93, SIMPLIS POP+AC].
2. **Measures.** PM, GM, *and* modulus margin min_ω|1 + T(jω)| (buck: 0.66 nominal → 0.16 worst). The modulus margin stays continuous when crossovers multiply, as in voltage-mode Type III with conditional stability.
3. **For-all structure.** Frequency is an internal for-all variable. Load, V_in and temperature are range knobs. The worst is usually at a corner (random interior never beat corners in either study), but joint flips are needed (§3.3 S4) and one-at-a-time loops can cycle (§4).
4. **Discrete-time stability.** Floquet multipliers from the PSS monodromy.
   - Buck nominal: 0.925 (slow compensator mode), 0.36±0.33j (the crossover pair: angle 42.5°/cycle ≈ 57 kHz), 0.01.
   - Low-PM corner: the pair moves to **−0.42±0.46j** (angle 132°/cycle ≈ 141 kHz, heading for −1).
   - Weak slope compensation: **−1.18**, unstable.
   - Note that the spectral radius alone (0.93 → 0.91) *hides* this, because the slow mode dominates. The spec must be on the **oscillatory multipliers** (e.g. |λ| ≤ 0.7 for complex/negative multipliers). This is the sampled-data view of Ridley's Q_p.
5. **Composition.** Input-filter interaction: Middlebrook's criterion |Z_out,filter(f)| ≪ |Z_in,conv(f)|, with Z_in ≈ −V_in²/P at low frequency [Mid76, Eri99]. This is a contract check across blocks, not a single-block spec. AEi lists it as a top WCA finding [Hym14].
6. **Time-domain cross-check.** Load-step ringing at the worst loop corner, verified at L2/L3.

Robust-control shortcuts don't transfer cleanly:
- Kharitonov and the edge theorem (bibliography §4) need characteristic-polynomial coefficients that are affine in independent parameters. Converter coefficients are multilinear in L, C, R, gm, so edges aren't sufficient and the mapping theorem gives only convex-hull bounds.
- μ-analysis/synthesis has been applied to switching regulators with component tolerances [WT00]. It's a possible later path for a *guaranteed* robust-stability certificate on the averaged model.
- I found no verified prior work in this session applying affine arithmetic or corner theorems to converter loop gain.

---

## 10. Better methods: what literature and industry use

| Problem | Method | Key references | What we should take |
|---|---|---|---|
| Averaged dynamics | State-space averaging; PWM-switch model (CCM/DCM); generalized averaging; rigor of averaging | [MC76], [Vor90a, b], [SNLV91], [KBBL90], [MSTV01], [EM20] | L1 template library: PWM switch covers CCM and DCM in one circuit model |
| Current-mode loop | Ridley sampled-data model (Q_p, f_sw/2 double pole); Tan–Middlebrook unified model; three-terminal CM switch; V²/COT describing functions | [Rid91], [TM95], [YLM12], [LL10], [RS09] | L1a models; Q_p and m_c as named intermediates for joint-flip search |
| Fast switching simulation | Ideal-switch piecewise-linear (SIMPLIS, PLECS) | [BV92], [AH99], [SIMPLIS] | Optional PWL mode; POP (PSS) as the workhorse |
| Periodic steady state | Shooting-Newton; extrapolation; matrix-free Krylov shooting; harmonic balance; autonomous-oscillator shooting (COT/hysteretic converters have no fixed period) | [AT72a], [AT72b], [Ske80], [TKW95], [NV76], [KWS90] | Shooting first (switching waveforms are not band-limited, so HB suits them poorly); Krylov for large netlists |
| Small-signal at PSS | PAC/PXF/PNOISE; SIMPLIS POP + AC | [OTIS93], [TKEW96], [Kun99], [SIMPLIS] | Loop gain near f_sw/2 at the verification level |
| Envelope transients | Envelope following | [WL91] | Soft-start, thermal ramps |
| Sensitivities through switching | Hybrid-system sensitivity functions; saltation matrices; periodic adjoint | [GFB99], [LN04], [BBS13], [BBS12], [SKD24], [SKD24b] | Event-aware direct sensitivities; PSS sensitivity via (I − Φ)⁻¹ |
| Subharmonics / fast-scale instability | Floquet / Poincaré-map analysis | [DH90], [BV01] | Multiplier-based stability spec |
| Loop-gain measurement | Voltage/current injection; null double injection / EET; Tian's method | [Mid75], [Mid89], [TVHK01] | Loop-break probes in the spec language |
| Input-filter interaction | Middlebrook criterion; optimal damping | [Mid76], [Eri99] | Impedance contracts |
| Robust stability under tolerances | μ-synthesis for regulators; UQ with model-form uncertainty | [WT00], [RWBRB21a, b] | Later: certificates on L1; now: model-form term |
| Electrothermal | Coupled electro-thermal simulation; MOSFET thermal instability | [MH97], [WCSW97], [SBDR02], [AN11158] | Thermal nodes, runaway detection, SOA with Z_th(t) |
| Core loss | iGSE | [VSAT02] | Inductor loss and temperature in PSS efficiency |
| WCA practice | ECSS WCA handbook; Aerospace TOR WCCA; JPL D-5703; derating (ECSS-Q-ST-30-11C, EEE-INST-002) | [ECSS11], [Len13], [Hym14], [JPL90], [ECSSder], [EEE-INST-002] | "Worst-case operating modes and conditions" as a plan section; BOL/EOL; compliance matrix per converter |
| Vendor practice | Resistor tolerance → accuracy; LDO ESR stability; thermal metrics; hot-swap SOA; reverse-battery circuits | [SLVA423], [SLVA115], [SPRA953], [SLVA673], [SLVA139] | Auto-spec formulas (L0) |

---

## 11. Recommended changes

### 11.1 Engine

1. **Analysis ladder with multi-fidelity verification** (§9.2).
   - Each spec's measure declares, or is assigned, the lowest level that can represent it.
   - The loop searches at level k and verifies predicted worst points at level k+1. Counterexamples are always from the highest level run.
   - A "method" column entry reads e.g. "loop on averaged (22 sims) + switching verification (1)".
   - The L1/L2 discrepancy at each verified point is a named model-form error term in the affine form.
2. **Cost-aware search.**
   - If a metric evaluation costs < ~10 ms (averaged AC), enumerate corners over the top-k knobs (k = 8–12) plus Latin-hypercube interior samples, and skip the loop.
   - For expensive metrics, run the loop with: visited-corner memory; best-so-far; cycle detection; **joint-flip moves** over knobs that share a declared intermediate (templates can declare m_c, f_c, f_RHPZ, Q_p, V_headroom).
3. **Mode awareness** (§9.7).
   - Mode traces from every simulation; mode-consistent linearization; splitting along mode boundaries; mode-scoped specs; automatic boundary specs.
   - A verdict is never PASS if the verification point's mode differs from the mode the spec was written for.
4. **Electrothermal coupling** (§9.5).
   - Thermal states; a fixed point with runaway detection; sensitivities through the loop.
   - Board thermal as an assumption knob; local temperature links for tempcos.
5. **Knob-kind-aware combination.**
   - Datasheet-limit and typ-only knobs are combined worst-case by default.
   - Statistical treatment of IC parameters only with vendor distribution data.
   - Always report "margin that depends on assumed spreads".
6. **Quasi-static sweeps** for slow states (battery voltage, temperature ramps, aging) instead of transients.
7. **Monte Carlo at sign-off, adapted for power.**
   - MC runs on L1/L2 with the averaged model as a control variate.
   - Switching-level MC only near the failure region (importance sampling centred on the worst point, which the bibliography notes is the same point [Dolecek 2008]).
   - Datasheet-limit and range knobs stay at their worst values during MC. MC only over genuinely statistical passives.
   - In power this often means MC answers a narrower question than in the amplifier walkthrough, and the report must say so.

### 11.2 Knob model

| New kind | Example | Default treatment |
|---|---|---|
| **datasheet limit** (bounded, distribution unknown, conditions attached: "over T_J −40…150 °C") | V_REF 0.788–0.812 V; current limit 4.0–6.5 A; I_dVdt 0.81–3.82 µA | worst case; never add a tempco on top; statistical only with vendor σ/C_pk |
| **typ-only** (no limits given) | EA gm, COMP→I_SW gain, soft-start time, EN currents, PSRR curves | require a spread (user or AI-proposed, flagged "assumed"); rank; show the verdict with and without |
| **state-dependent parameter function** | C(V_dc, T, age); L(I, T); R_DS(on)(V_GS, T_J) | device model in the simulator plus curve-uncertainty knobs |
| **solved state** (not a knob) | T_J, T_part | from the thermal network |
| **board / setup assumption** | θJA, cable L/R, source impedance, trace drop, pre-bias voltage | range knob declared in the block's assumptions |
| **waveform knob** | load-step size/slew/repetition; line transients; standard pulses (ISO 7637-2 etc.) | parameterized stimulus family; for-all over its parameters |
| **model-form error** | L1 vs L2 gap at a point | named error term, reported separately |

Resolve v2's open question on BOL vs EOL for power: a `life` range knob works for MLCC aging, resistor drift and electrolytic ESR growth. Electrolytic *life* itself depends on temperature and ripple current (Arrhenius), so it is an automatic spec, not a knob.

### 11.3 Spec definitions

Additions to `../specs.md`, strawman syntax:

```
block Buck3V3 {
  assume vin   in 10.8V..13.2V                       // plus: stimulus hot_plug(cable_L in 0.3uH..2uH)
  assume temp  in -40°C..85°C                        // ambient; part temperatures are solved
  assume board theta_ja(U1) in 35..55 °C/W           // PCB out of scope → declared assumption
  assume load  on out: I in 0A..3A,
               step(1.5A -> 2.5A, slew 1A/us, repeat 100Hz..20kHz)
  assume life  in 0..10 years

  U1 = TPS54335A(model: template buck_pcm, params: datasheet("SLVSCD5D"))
       spread gm_ea ±30% assumed "not in datasheet"   // typ-only knob, visibly flagged

  spec vdc    = op(V(out)) in 3.3V ± 3%                                   worst_case
  spec ripple = pss(V(out)).pp <= 33mV            when mode(U1) == PWM
  spec droop  = tran(load.step).min(cycle_avg(V(out))) >= 0.95*3.3V
  spec total  = tran(load.step).min(V(out)) >= 3.135V                    // engine composes DC+ripple+transient
  spec loop   = stable(U1.loop) { pm >= 45°, gm >= 10dB, modulus >= 0.5,
                                  floquet_osc <= 0.7 }  for all load, vin, temp
  spec nolim  = never(mode(U1) == current_limit) during load.step
  spec ccm    = mode(U1) == CCM for load >= 1.2A
  spec tj     = thermal(Tj(U1)) <= 125°C, never(mode(U1) == thermal_shutdown)
  spec start  = tran(power_on) { monotonic(V(out)), overshoot <= 3%,
                                 t_cross(0.9*3.3V) in 1ms..4ms } for prebias in 0V..3.3V
}
```

What's new, spec by spec:
- `pss(...)` measures: pp, max, min, avg, rms, efficiency, mode.
- `cycle_avg(...)`, to separate transient from ripple. Raw vs cycle-averaged differ by 8–34 mV here, and specs must say which one they mean.
- Mode predicates and `never(...) during ...`.
- `stable(...)` bundles, including modulus margin and Floquet.
- Initial conditions as for-all.
- Composite "total tolerance" measures that the engine decomposes.

**Default confidence for power:** `worst_case` for protection, stability, safety (charge voltage, NTC window) and abs-max. `sigma 3` for ripple, efficiency and accuracy where passives dominate.

### 11.4 Automatic specs for power

(Each can be waived with a reason, as in spec design §8.)

| Part / block | Auto-spec |
|---|---|
| MLCC | C_eff(V_dc, T, age, tol) ≥ the IC's minimum stable C; ≤ its maximum; voltage ≤ 50–80% of rated (derating per [ECSSder]/[EEE-INST-002]); ripple-current self-heating |
| Electrolytic | ripple-current rating at T; life ≥ mission at hot spot (Arrhenius; vendor data) |
| Inductor | I_pk,max(worst corner incl. limit + delay overshoot) < I_sat,min(T_hot); I_rms < rating; core + DCR loss in the thermal budget |
| Converter IC | I_LIM,min > I_out,max + ΔI_L,max/2 (+ transient margin); t_on ≥ t_on,min,max at V_in,max, f_sw,max; D ≤ D_max at V_in,min; slope compensation: Q_p ≤ 2 (or m_c·D′ − 0.5 ≥ margin) at D_max; boot/UVLO; T_J ≤ derated limit and < T_SD,min at T_A,max; f_sw window vs EMI plan |
| LDO | headroom ≥ V_DO,max(I, T_J) at V_in,min (from the upstream guarantee); (C_eff, ESR) inside the stability region; T_J |
| FET (load switch, eFuse, reverse protection) | V_DS, V_GS within derated ratings incl. clamp tolerances; SOA with Z_th(t) derated to T; linear-mode thermal stability |
| Interfaces | UVLO hysteresis > I_in·R_source + ripple; PG threshold vs worst transient droop; input-filter Middlebrook margin; downstream start-up demand < upstream limit; hot-plug peak < abs-max of all ICs on the rail; sequencing order and timing |
| Charger | V_BAT(REG) range within cell limits; I_CHG,max ≤ cell limit; NTC window thresholds (exact); USB input limit |

These are mostly L0 formulas and exact monotone chains. They are cheap, high-yield, and exactly what AEi finds missing [Hym14].

### 11.5 Contracts and composition for power interfaces

Extend the I/O page (vision §3, spec design §2):

**A power *source* guarantees:**
- DC range (affine form);
- ripple spectrum (amplitude at frequency bands, and mode-dependent);
- transient envelope for the declared load family;
- Z_out(f);
- start-up profile (ramp rate, PG timing);
- current capability and limit behaviour.

**A power *load* assumes / declares:**
- input voltage window, including transients;
- Z_in(f), including negative incremental resistance of downstream converters;
- start-up demand (inrush, UVLO threshold, constant-power behaviour);
- ripple tolerance (PSRR-weighted).

**Composition checks:**
- guarantee ⊆ assumption, per property;
- PSRR(f) × ripple(f) ≤ noise budget;
- Middlebrook impedance margin;
- start-up demand vs source limit over time.

At system level, reuse the upstream block's actual affine form with shared knobs (T_A, V_in), as spec design §2 already proposes.

### 11.6 Simulator features needed (priority order)

| # | Feature | Why |
|---|---|---|
| 1 | Voltage-/current-controlled switch with hysteresis; power MOSFET model (with body diode, C_oss(V)); E/G/F/H controlled sources; behavioural B-sources | cannot build any converter today |
| 2 | Variable time step with LTE control; breakpoints at source edges; **event location** for thresholds (comparators); BE/Gear at discontinuities (no trapezoidal ringing) | switching waveforms, cost, accuracy |
| 3 | Digital/behavioural primitives: comparator, SR latch, clock, one-shot, counter (for hiccup and timers), state machine | IC templates |
| 4 | Parameter layer with IDs (already v2 Part D.1) plus guarantee-type metadata | knobs |
| 5 | PSS shooting: warm start, damping, Floquet multipliers, period-k fallback; later matrix-free Krylov [TKW95]; autonomous variant for COT/hysteretic | ripple, peaks, efficiency, stability |
| 6 | AC at the PSS: periodic small-signal / loop gain; loop-break probes (Middlebrook/Tian) | stability near f_sw/2 |
| 7 | Averaged-model library (PWM switch CCM/DCM; current-mode Ridley/Tan–Middlebrook; COT) as templates, sharing parameters with the switching templates | L1 |
| 8 | Nonlinear charge-based C(V) and flux-based L(I) with curve data | state-dependent parameters |
| 9 | Thermal network nodes, electrothermal coupling, multirate | T_J, runaway, SOA |
| 10 | Event-aware direct sensitivities (saltation) and PSS sensitivity via (I − Φ)⁻¹; transposed solve (already D.2) for averaged AC | loop sensitivities |
| 11 | Tighter tolerances for PSS measures (reltol ≤ 1e-5); `.meas`-style measures over PSS windows and cycle averages | mV ripple |
| 12 | Optional ideal-switch piecewise-linear mode | speed for L3 |
| 13 | External-simulator bridge (ngspice/LTspice/PSpice/SIMPLIS CLI) for encrypted models | vendor models |

### 11.7 Effect on the build order (v2 Part G)

Add a power track after lesson 6:
- **P1:** L0 auto-specs plus exact chains (divider, NTC, R_ISET, EN/UVLO). Works today in `spicy_bounds` with no simulator changes.
- **P2:** averaged current-mode buck template; exhaustive-corner PM with joint-flip guards.
- **P3:** switch/MOSFET plus events plus variable step in `spicy_simulate`.
- **P4:** PSS shooting plus Floquet plus PSS sensitivities.
- **P5:** mode traces and mode-scoped specs; multi-fidelity loop.
- **P6:** electrothermal.
- **P7:** contracts with impedance, spectrum and start-up demand.

---

## 12. Bibliography check: what is misapplied or needs a caveat for power circuits

| Entry | How the design uses it | Issue for power | Fix |
|---|---|---|---|
| [Maxim 5527] "4.7 µF becomes 0.33 µF"; A7 "DC bias … range terms added on top" | DC bias as a range knob | The 0.33 µF figure is one extreme (0603, 6.3 V part at 5 V **and** 85 °C, −92.9%) [Fort5527]. DC bias is a deterministic function of the operating voltage, not a range; and "less C" is not always worse | model C(V, T) in the simulator; keep only curve uncertainty as a knob; don't quote the extreme as typical |
| [N&P07], [PPC65] corner theorem ("exact for linear circuits") | exact bounds from the netlist | Holds for dividers, EN/UVLO, current-limit and charge-current programming, NTC chains. Not for: loop gain or PM (AC and implicit); averaged models (duty cycle multiplies states); elements whose value depends on the operating point (MLCC, L(I), R_DS(on)(T_J)) | label exactness per measure; withdraw it when any element is state-dependent |
| [AGW94], [Graeb07] worst-case distance and yield ≈ Φ(β) | loop plus yield | Assumes smooth performances and a known distribution in the statistical space. Power contributors are mostly range, datasheet-limit or typ-only with unknown distributions, so Φ(β) is not meaningful for them. Non-smooth modes break convergence (boost loop cycled) | keep the loop within a mode; worst case for IC limits; guards (§11.1.2) |
| [ECSS11] | range vs statistical split | Consistent. But industry WCA plans treat "worst-case operating **modes** and conditions" as a first-class section [Hym14, Len13], which the design lacks | add modes (§9.7) |
| [DR69], [Xyce16] adjoint "≈ one extra solve" | sensitivities | True for DC and averaged AC. For transient metrics it's a backward pass per objective over a stored ms-long switching trajectory. For PSS use (I − Φ)⁻¹ (cheap). Event metrics need saltation or event terms [GFB99] | per-level sensitivity strategy |
| [GFB99] (listed as a side note) | — | **Central** for power: every switching instant is an event | promote to a primary reference |
| Aprille & Trick 1972 (PSS) | "later, for switching converters" | Correct method. But COT/hysteretic converters are *autonomous* (period unknown), so they need the oscillator variant [AT72b]. And PSS isn't "later" if power is in scope | move PSS to first-class |
| [Din15], [PKK10], [SH18] complex AA / parametric AC | "guaranteed AC" path | Converter loop uncertainties are large (C_eff ×2.3; IC gains ±20–30%), where [SH18] reports inner/outer ratios of 0.28–0.54 at ±20%. PM isn't a network-function value. Low value for power | deprioritize for power; sample the cheap averaged AC model instead |
| Edge/Kharitonov results (bibliography §4) | "AC needs edges, not corners" | Need coefficients affine in independent parameters. Converter polynomials are multilinear, so edges aren't sufficient. Empirically, corners plus joint flips worked here, but that's not a proof | treat as heuristics; certificates via μ on L1 later [WT00] |
| [GOB08], [Sch18] AA inside the solver | nonlinear DC | Transient was an open issue there. Not applicable to switching | none needed; just don't plan on it for power |
| [Hanley & Lippman-Hand], [Wilks] | MC confidence | Valid, but switching-level MC is costly; most power knobs aren't statistical | MC on L1/L2 with control variates |
| Walkthrough v1 overshoot example (electrolytic ESR as a range term) | range vs statistical | Correct: ESR(T) is range. The same logic makes T_J-dependent quantities solved states, not independent knobs | — |

---

## 13. References (verified in this session unless marked)

Status: **V** = title, authors, venue, year confirmed from a primary record (IEEE Xplore/Crossref/publisher/the document itself). **P** = partial (detail noted). **U** = not verified in this session.

**Power converter modeling and control**
- [MC76] R. D. Middlebrook, S. Ćuk, "A general unified approach to modelling switching-converter power stages," IEEE PESC 1976, pp. 18–34. doi:10.1109/PESC.1976.7072895. **V**
- [Vor90a] V. Vorpérian, "Simplified analysis of PWM converters using model of PWM switch. Part I: Continuous conduction mode," IEEE Trans. Aerosp. Electron. Syst. 26(3):490–496, 1990. doi:10.1109/7.106126. **V**
- [Vor90b] V. Vorpérian, "… Part II: Discontinuous conduction mode," IEEE TAES 26(3):497–505, 1990. doi:10.1109/7.106127. **V**
- [Rid91] R. B. Ridley, "A new, continuous-time model for current-mode control," IEEE Trans. Power Electron. 6(2):271–280, 1991. **V** (the Q_p = 1/(π(m_c·D′ − 0.5)) formula as commonly quoted from it; not checked against the paper text: **P**)
- [TM95] C. W. Tan, R. D. Middlebrook, "A unified model for current-programmed converters," IEEE TPEL 10(4):397–408, 1995. doi:10.1109/63.391937. **V**
- [YLM12] Y. Yan, F. C. Lee, P. Mattavelli, "Unified three-terminal switch model for current mode controls," IEEE TPEL 27(9):4060–4070, 2012. doi:10.1109/TPEL.2012.2188841. **V**
- [LL10] J. Li, F. C. Lee, "New modeling approach and equivalent circuit representation for current-mode control," IEEE TPEL 25:1218–1230, 2010. **P** (issue number not confirmed)
- [RS09] R. Redl, J. Sun, "Ripple-based control of switching regulators—an overview," IEEE TPEL 24:2669–2680, 2009. **P** (issue not confirmed)
- [SNLV91] S. R. Sanders, J. M. Noworolski, X. Z. Liu, G. C. Verghese, "Generalized averaging method for power conversion circuits," IEEE TPEL 6(2):251–259, 1991. doi:10.1109/63.76811. **V**
- [KBBL90] P. T. Krein, J. Bentsman, R. M. Bass, B. C. Lesieutre, "On the use of averaging for the analysis of power electronic systems," IEEE TPEL 5(2):182–190, 1990. doi:10.1109/63.53155. **V**
- [MSTV01] D. Maksimović, A. M. Stanković, V. J. Thottuvelil, G. C. Verghese, "Modeling and simulation of power electronic converters," Proc. IEEE 89(6):898–912, 2001. **V**
- [EM20] R. W. Erickson, D. Maksimović, *Fundamentals of Power Electronics*, 3rd ed., Springer, 2020. doi:10.1007/978-3-030-43881-4. **V**
- [Bas12] C. Basso, *Designing Control Loops for Linear and Switching Power Supplies: A Tutorial Guide*, Artech House, 2012, ISBN 978-1-60807-557-7. **V** (via a published review)
- [Bas16] C. Basso, *Linear Circuit Transfer Functions: An Introduction to Fast Analytical Techniques*, Wiley, 2016. doi:10.1002/9781119236344. **P** (author not in the Crossref record)
- [DH90] J. H. B. Deane, D. C. Hamill, "Instability, subharmonics, and chaos in power electronic systems," IEEE TPEL 5(3):260–267, 1990. **V**
- [BV01] S. Banerjee, G. C. Verghese (eds.), *Nonlinear Phenomena in Power Electronics*, IEEE Press, 2001. doi:10.1109/9780470545393. **V**

**Stability measurement, filters, robustness, uncertainty**
- [Mid75] R. D. Middlebrook, "Measurement of loop gain in feedback systems," Int. J. Electronics 38(4):485–512, 1975. **V**
- [Mid89] R. D. Middlebrook, "Null double injection and the extra element theorem," IEEE Trans. Educ. 32(3):167–180, 1989. doi:10.1109/13.34149. **V**
- [TVHK01] M. Tian, V. Visvanathan, J. Hantgan, K. Kundert, "Striving for small-signal stability," IEEE Circuits Devices Mag. 17(1):31–41, 2001. **V**
- [Mid76] R. D. Middlebrook, "Input filter considerations in design and application of switching regulators," IEEE IAS Annual Meeting Record, 1976, pp. 366–382. **V**
- [Eri99] R. W. Erickson, "Optimal single resistors damping of input filters," APEC 1999, pp. 1073–1079. doi:10.1109/APEC.1999.750502. **V**
- [WT00] G. F. Wallis, R. Tymerski, "Generalized approach for μ synthesis of robust switching regulators," IEEE TAES 36(2):422–431, 2000. doi:10.1109/7.845219. **V**
- [RWBRB21a] N. Rashidi, Q. Wang, R. Burgos, C. Roy, D. Boroyevich, "Multi-objective design and optimization of power electronics converters with uncertainty quantification—Part I: Parametric uncertainty," IEEE TPEL 36(2):1463–1474, 2021. doi:10.1109/TPEL.2020.3005456. **V**
- [RWBRB21b] same authors, "… Part II: Model-form uncertainty," IEEE TPEL 36(2):1441–1450, 2021. doi:10.1109/TPEL.2020.3007227. **V**

**Simulation methods**
- [BV92] D. Bedrosian, J. Vlach, "Time-domain analysis of networks with internally controlled switches," IEEE TCAS-I 39(3):199–212, 1992. **V** (first initial as listed by IEEE Xplore)
- [SIMPLIS] SIMPLIS Technologies / SIMetrix documentation, "Periodic Operating Point (POP)" and "POP and AC analysis" (simplistechnologies.com, help.simetrix.co.uk). **V** (documentation)
- [AH99] J. H. Allmeling, W. P. Hammer, "PLECS—piece-wise linear electrical circuit simulation for Simulink," IEEE PEDS 1999, pp. 355–360. doi:10.1109/PEDS.1999.794588. **V**
- [AT72a] T. J. Aprille, T. N. Trick, "Steady-state analysis of nonlinear circuits with periodic inputs," Proc. IEEE 60(1):108–114, 1972. doi:10.1109/PROC.1972.8563. **V**
- [AT72b] T. J. Aprille, T. N. Trick, "A computer algorithm to determine the steady-state response of nonlinear oscillators," IEEE Trans. Circuit Theory 19(4):354–360, 1972. doi:10.1109/TCT.1972.1083500. **V**
- [Ske80] S. Skelboe, "Computation of the periodic steady-state response of nonlinear networks by extrapolation methods," IEEE TCAS 27(3):161–175, 1980. **V**
- [TKW95] R. Telichevesky, K. S. Kundert, J. K. White, "Efficient steady-state analysis based on matrix-free Krylov-subspace methods," DAC 1995, pp. 480–484. **V**
- [KWS90] K. S. Kundert, J. K. White, A. Sangiovanni-Vincentelli, *Steady-State Methods for Simulating Analog and Microwave Circuits*, Kluwer, 1990. doi:10.1007/978-1-4757-2081-5. **V**
- [NV76] M. S. Nakhla, J. Vlach, "A piecewise harmonic balance technique for determination of periodic response of nonlinear systems," IEEE TCAS 23(2):85–91, 1976. **V**
- [OTIS93] M. Okumura, H. Tanimoto, T. Itakura, T. Sugawara, "Numerical noise analysis for nonlinear circuits with a periodic large signal excitation including cyclostationary noise sources," IEEE TCAS-I 40(9):581–590, 1993. **V**
- [TKEW96] R. Telichevesky, K. Kundert, I. Elfadel, J. White, "Fast simulation algorithms for RF circuits," CICC 1996, pp. 437–444. **V**
- [Kun99] K. S. Kundert, "Introduction to RF simulation and its application," IEEE JSSC 34(9):1298–1319, 1999. **V**
- [WL91] J. White, S. B. Leeb, "An envelope-following approach to switching power converter simulation," IEEE TPEL 6(2):303–307, 1991. **P** (end page not confirmed)
- [LN04] R. I. Leine, H. Nijmeijer, *Dynamics and Bifurcations of Non-Smooth Mechanical Systems*, Lecture Notes in Applied and Computational Mechanics, Springer, 2004. doi:10.1007/978-3-540-44398-8. **V**
- [BBS13] F. Bizzarri, A. Brambilla, G. Storti Gajani, "Extension of the variational equation to analog/digital circuits: numerical and experimental validation," Int. J. Circuit Theory Appl. 41(7):743–752, 2013 (online 2012). doi:10.1002/cta.1864. **V**
- [BBS12] F. Bizzarri, A. Brambilla, G. Storti Gajani, "Steady state computation and noise analysis of analog mixed signal circuits," IEEE TCAS-I 59(3):541–554, 2012. doi:10.1109/TCSI.2011.2167273. **V**
- [GFB99] S. Galán, W. F. Feehery, P. I. Barton, "Parametric sensitivity functions for hybrid discrete/continuous systems," Appl. Numer. Math. 31(1):17–47, 1999. doi:10.1016/S0168-9274(98)00125-1. **V**
- [SKD24] J. Sarpe, A. Klaedtke, H. De Gersem, "Periodic adjoint sensitivity analysis," arXiv:2405.19048, 2024 (preprint). **V** (periodic Parareal with a periodic coarse problem for adjoint sensitivities of the steady state of time-periodic nonlinear circuits)
- [SKD24b] J. Sarpe, A. Klaedtke, H. De Gersem, "Transient forward harmonic adjoint sensitivity analysis," Electrical Engineering 106(6):7831–7838, 2024. doi:10.1007/s00202-024-02463-z. **V**
- [IEEE1735] IEEE Std 1735, "Recommended Practice for Encryption and Management of Electronic Design Intellectual Property (IP)," 2014 (doi:10.1109/IEEESTD.2015.7274481) and 2023 (doi:10.1109/IEEESTD.2023.10328536). **V**

**Electrothermal, devices, magnetics**
- [MH97] H. A. Mantooth, A. R. Hefner, "Electrothermal simulation of an IGBT PWM inverter," IEEE TPEL 12(3):474–484, 1997. doi:10.1109/63.575675. **V**
- [WCSW97] S. Wünsche, C. Clauß, P. Schwarz, F. Winkler, "Electro-thermal circuit simulation using simulator coupling," IEEE Trans. VLSI Syst. 5(3):277–282, 1997. doi:10.1109/92.609870. **V**
- [SBDR02] P. Spirito, G. Breglio, V. d'Alessandro, N. Rinaldi, "Thermal instabilities in high current power MOS devices: experimental evidence, electro-thermal simulations and analytical modeling," MIEL 2002, vol. 1, pp. 23–30. doi:10.1109/MIEL.2002.1003144. **V**
- [VSAT02] K. Venkatachalam, C. R. Sullivan, T. Abdallah, H. Tacca, "Accurate prediction of ferrite core loss with nonsinusoidal waveforms using only Steinmetz parameters," IEEE COMPEL 2002, pp. 36–41. doi:10.1109/CIPE.2002.1196712. **V**
- [AN11158] Nexperia AN11158, "Understanding power MOSFET data sheet parameters," Rev. 7.0, 18 Feb 2025. **V** (read: linear-mode derating, thermal runaway, ZTC, Spirito effect)
- [Fort5527] M. Fortunato, Maxim (ADI) Tutorial 5527, "Temperature and voltage variation of ceramic capacitors, or why your 4.7µF capacitor becomes a 0.33µF capacitor." **P** (title and author confirmed; the 0603/6.3 V/5 V/85 °C/−92.9% example from ADI's page summary; date not confirmed)
- [Per01] G. Perica, Linear Technology Application Note 88, "Ceramic input capacitors can cause overvoltage transients," March 2001. **P** (title, author and date confirmed; the specific overshoot figure not read)

**Vendor application notes (read)**
- [SLVA423] M. Robertson, "Effect of Resistor Tolerances on Power Supply Accuracy," TI SLVA423, June 2010. **V** (gives ΔV_o/V_o = ±2T·(1 − V_ref/V_o))
- [SLVA115] J. Falin, J. Cummings, "ESR, Stability, and the LDO Regulator," TI SLVA115A, May 2002, rev. Feb 2020. **V**
- [SPRA953] D. Edwards, H. Nguyen, "Semiconductor and IC Package Thermal Metrics," TI SPRA953D, Dec 2003, rev. Mar 2024. **V** ("70%–95% of the power … dissipated from the test board"; applying θJA to a system board "results in extremely erroneous values")
- [SLVA673] A. Rogachev, "Robust Hot Swap Design," TI SLVA673A, 2014. **V**
- [SLVA139] J. Falin, "Reverse Current/Battery Protection Circuits," TI SLVA139, June 2003. **V** (header only)
- Datasheets: TI TPS54335A (SLVSCD5D), TPS54331 (SLVS839H), TPS7A20 (SBVS338H), TPS25947 (SLVSFC9C), LM74700-Q1 (SNOSD17G), BQ24072 (SLUS810N). **V** (revisions as listed in §2)

**WCA practice and standards**
- [ECSS11] ECSS-Q-HB-30-01A (in the main bibliography).
- [ECSSder] ECSS-Q-ST-30-11C Rev.2, "Derating – EEE components," 23 June 2021. **V**
- [EEE-INST-002] NASA GSFC EEE-INST-002, "Instructions for EEE Parts Selection, Screening, Qualification, and Derating" (NASA/TP-2003-212242). **V**
- [Len13] B. A. Lenertz, "Electrical Design Worst-Case Circuit Analysis: Guidelines and Draft Standard (REV A)," Aerospace Corp. TOR-2013-00297, June 2013. **P** (listing confirmed; document not read. The AEi talk cites a TOR-2012(8960)-4 Rev. A version)
- [Hym14] C. Hymowitz (AEi Systems), "WCCA: Tailoring TOR for Class D Missions," NASA NEPP EEE Parts for Small Missions Workshop, 11 Sep 2014. **V** (slides read)
- [JPL90] JPL D-5703, "Reliability Analyses Handbook," July 1990. **P** (listing confirmed)
- USB-IF, Battery Charging Specification Rev. 1.2, 15 Mar 2012. **P** (document confirmed; the DCP current value not checked against the text)
- ISO 16750-2 (reverse voltage, supply profiles), ISO 7637-2 (conducted transients), LV 124, JEITA/BAJ Li-ion charging guideline (2007). **U** (search budget exhausted; test levels and thresholds deliberately not quoted)

**Not found in this session:** no verified paper applying affine arithmetic or vertex/corner theorems to DC-DC converter loop gain or phase margin. Femia & Spagnuolo's GA+AA tolerance work ([FS00] in the main bibliography) is generic circuits.

---

## Appendix A: Models behind the numbers

- **Buck small-signal (L1a).** Ridley's current-mode control-to-output model: K-factor, output pole with the T_s/L·(m_c·D′ − 0.5) term, ESR zero, sampling double pole at f_sw/2 with Q_p = 1/(π(m_c·D′ − 0.5)). A gm Type-II compensator with finite R_o, feedback-divider gain H, and current-sense gain 1/R_i = 8 A/V. Slope compensation S_e = 1.0 A/µs equivalent *(assumed; not in the datasheet)*. PM and GM from 500–1,200 log-spaced points. Modulus margin = min|1+T|.
- **Buck large-signal averaged (L1b).** States i_L, v_C, v_Cc, v_comp. Duty from the averaged peak-current law d = (I_c − i_L)/(T_s·(S_e + S_n/2)), with the limit comparator (no slope comp) above I_LIM, clamped to [0, 0.93]. RK4, 10–40 ns steps.
- **Buck switching (L2).**
  - Ideal synchronous switches with R_DS(on) and DCR; ESR.
  - Peak current comparator with slope compensation; cycle-by-cycle limit.
  - Diode emulation (LS sink limit 0 A, so DCM); pulse skip below a 0.5 A command.
  - Internal 2 ms soft-start.
  - RK4 at T_s/300 with event location by interpolation.
  - Shooting-Newton on the one-period map with a finite-difference Jacobian (monodromy). Floquet multipliers from the characteristic polynomial of Φ.
  - Ideal switches have no switching loss. Efficiency used separate L0 loss formulas.
- **Boost small-signal.** Current-mode boost with ideal inner loop (Erickson ch. 18 form) and RHP zero R(1 − D)²/L, ESR zero, and the sampling double pole. Assumed IC parameters as in §4.
- **Thermal.** Single-node θJA fixed point, with R_DS(on)(T) = R₂₅(1 + 0.0045·(T − 25)) *(rep.)*.
- **Representative values (to be replaced by vendor curve data in the engine).** MLCC DC-bias retention (22 µF 0805 6.3 V at 3.3 V: 45%; 1 µF 0402 at 3.3 V: 45–60%; 10 µF 1206 25 V at 12 V: 7–16 µF for 3 parts); inductor soft-saturation factor 0.85–0.97; P-FET R_DS(on) at low V_GS; NTC B = 3435; charger θJA 40 °C/W.
- The Python scripts that produced these numbers were run in a scratch directory and are not part of the repo. Every model is fully specified above and in the tables.
