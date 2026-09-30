# Contracts Across Hierarchy: When Can a Checked Block Be Reused?

> 2026-09-29 · Research report. The question, from the user: *"Can we make sure that a bench or a spec holds if a block uses another block? Is there a way to not need to simulate the full design in this case, by caching the underlying block's output?"*
> It builds on engine.md v4 (§3, §7), engine_plan.md (§1, §2, D-F), engine_types.md (T6 run table, §9.3 `rev`), language.md (§3, §8.2, §8.5, §8.8), model.md (E3, E16, E24) and research/next_boards.md (B1–B4, the "corner join").
> **Where the numbers come from.** Every number marked **[H]** was computed on ngspice-42 (libngspice, reference configuration of plan §2.1: IS 1e-14, XTB 1.5, TNOM 25 °C, engine tolerances) by stdlib-Python scripts in `/root/.claude/jobs/443154a8/tmp/engine4/hier/`: `hlib.py` (shared runner and the characterization cache), `h1_twostage.py`, `h2_amp_filter.py`, `h2_check_fhigh.py`, `h3_rail_amp.py`, `h4_threestage.py`, with their printed output in `h*.out` and `h*.json`. Numbers marked **[B]** come from next_boards.md and its scripts. Other claims are cited by document or reference.

---

## Summary

Yes, a parent can reuse a child's checked results instead of simulating the whole design, but only if the child's contract describes its **ports in both directions**. A port is not a one-way wire: the child's output impedance meets the neighbour's input impedance, and the current drawn flows back into the source. I tested four ways of reusing a child on four chains on ngspice. The chains were the two-stage CE amp, an amp driving an RC filter, a zener rail feeding the amp, and a three-stage amp. The four ways were (a) declared guarantees only, (b) guarantees checked over a range of load and source impedance ("interface knobs"), (c) cached characterization tables of the child, and (d) in-context simulation of the flat design, which is the truth. **(a) was unsound whenever loading mattered.** It gave a gain of 19.10–23.33 where the truth is 12.27–13.93, and a filter corner of ≥ 3040 Hz where the truth goes down to 2101 Hz. **(b) was sound when the parent checked that the real neighbours fall inside the child's interface ranges.** That check also catches the wrong assumption: with `z_load >= 10 kΩ` it FAILs, because the real load is 7.4–8.1 kΩ. But one measure of (b), the chain's −3 dB corner, still missed the truth by 1%, because a bandwidth doesn't compose from per-block bandwidths. **(c) matched the flat truth to 1e-14 on every linear AC measure.** A two-port table of the CE amp (512 runs, 0.38 s) served both placements of the two-stage amp, all three of the three-stage amp (1,048,576 corners composed in 0.3 s, against about 266 s of flat simulation), and the amp-plus-filter chain. There it also survived an edit to the filter with 8 new runs instead of 1,024. Shared knobs must be **joined**: every block is evaluated at the same temperature and supply. On the rail-plus-amp chain, composing without the join says the amp's bias reaches 6.5255 V, a false alarm against the 6.5 V limit. Joined on temperature, the answer is 6.3646 V, and the truth is 6.3640 V. This is how digital timing analysis works: cell libraries (Liberty) hold delay tables over input slew × output load, characterized at shared process-voltage-temperature corners, and the tool checks every real load against the cell's `max_capacitance`. The language therefore needs port-impedance and port-current quantities, interface assumptions that become knobs in the child's bench, and joins on environment knobs. The engine needs a characterization cache keyed by the child's definition and cone knobs, never by where the child is placed. In-context simulation stays required for FAILs, large-signal and nonlinear behaviour, coupling that doesn't go through a port (thermal, magnetic, shared ground), and as the regression check that keeps the tables honest.

---

## 0. The question in one picture

```
   TwoStage (parent)                              what the user wants
   ┌─────────────────────────────────────────┐
   │   ┌────────┐   mid    ┌────────┐         │    "s1 and s2 are CeAmp. CeAmp was already
   │in─┤ s1:    ├────●─────┤ s2:    ├──out    │     checked. Can TwoStage's specs reuse that,
   │   │ CeAmp  │          │ CeAmp  │         │     instead of simulating 2^14 corners?"
   │   └────────┘          └────────┘         │
   └─────────────────────────────────────────┘
```

The short answer: it depends on **what** was checked about CeAmp. If only "gain 4.6 ± 5%" was checked with nothing attached to the output, the answer is no. Once s2 is attached, s1's gain is 2.88, not 4.59 **[B]**. If CeAmp's port behaviour was checked or characterized, the answer is yes.

---

## 1. Theory, with numbers

### 1.1 Assume-guarantee: a promise with conditions

A contract is a pair: **assumptions** A (what the block needs from its surroundings) and **guarantees** G (what it promises when A holds). CeAmp's contract, from language §0 and next_boards §2.1:

```
  assume vcc.v in 12 V ± 5%        (the source must give this)
  assume temp  in −10..=60 °C
  assume output.z_load >= 10 kΩ    (what we drive must be at least this; post-MVP syntax)
  ───────────────────────────────
  spec   gain in 4.6 ± 5%          (holds only when all three assumptions hold)
```

The guarantee is **conditional**. "gain in 4.6 ± 5%" means "if the supply, the temperature and the load are as assumed, the gain is in this range". Nothing more.

**Composition rule.** When block U drives block D, the parent may use D's guarantee only if U's guarantees fall inside D's assumptions, for every quantity D assumes something about:

```
          U's guarantee   ⊆   D's assumption          (language §8.8)
   e.g.   rail.vout ∈ 11.60..12.51 V   ⊆   amp.vcc ∈ 11.4..12.6 V     ✓  [H3]
```

In contract theory (Benveniste et al., *Contracts for System Design*, 2018) this is **compatibility**: the environment each block sees must satisfy its assumptions. **Refinement** is the other half: a block that assumes less and guarantees more (A′ ⊇ A, G′ ⊆ G) may replace one that assumes more and guarantees less.

### 1.2 Why loading breaks naive reuse

**A port has two numbers, not one.** A voltage source with nothing attached shows its full voltage. Attach a load and current flows, and part of the voltage is lost inside the source. Every analog output behaves like this:

```
     s1 seen from its output port             s2 seen from its input port
   ┌──────────────────────────┐            ┌──────────────────┐
   │  A_oc · v_in   Z_out     │   mid      │                  │
   │  ( ~ )───────/\/\/\──────┼─────●──────┼──── Z_in  ───┐   │
   │                          │            │              │   │
   └────────────────────┬─────┘            └──────────────┴───┘
                        ⏚                                  ⏚
   A_oc  = open-circuit gain       = 4.4618 .. 4.7028   [H1]   (CeAmp with nothing attached)
   Z_out = output impedance        = 4653 .. 4747 Ω     [H1]   (= R_C ± 1%: the BJT model has no Early effect)
   Z_in  = s2's input impedance    = 7445 .. 8145 Ω     [H1]   (|Z| at 1 kHz; R1 ∥ R2 ∥ (β+1)·R_E plus C_in)

   v_mid / v_in  =  A_oc · Z_in / (Z_in + Z_out)                 ← the voltage divider
   nominal:        4.5908 · 7928 / (7928 + 4700)  =  2.882       (in-context gain 2.8823 [B])
```

So s1's gain in context is **37% lower** than the gain CeAmp guarantees on its own. The guarantee isn't wrong. It was checked with the output open, and in the two-stage amp the output isn't open. The contract's assumption `output.z_load >= 10 kΩ` is broken by the 7.4–8.1 kΩ load, and nothing checked it.

**Why this is two-way.** The divider has one number from each side: Z_out belongs to s1, Z_in belongs to s2. No property of s1 alone predicts v_mid. This is true of every analog connection:

| Connection | Upstream number | Downstream number | What goes wrong if one is missing |
|---|---|---|---|
| Stage → stage (H1) | Z_out 4.7 kΩ | Z_in 7.9 kΩ | Gain 37% low per stage |
| Amp → RC filter (H2) | Z_out 4.7 kΩ | the filter's R 10 kΩ, in series | Filter corner 2.1–2.6 kHz, not 3.0–3.8 kHz |
| Rail → amp (H3) | Rail z_out, V vs I | the amp's supply current, 1.35–1.82 mA | Rail voltage and amp bias both move |
| Logic driver → line | Driver I/V curve, edge rate | Receiver C_in, line impedance | Delay, overshoot, ringing (IBIS, §3.2) |

### 1.3 What a contract must carry for its results to be reusable

Each row is a quantity that crosses a port. The owner is the side that sets it (language §3.2's `#[by(Source)]` / `#[by(Sink)]`).

| Quantity | Owner | What it's for | Example from the tests | In language v0.1? |
|---|---|---|---|---|
| **Voltage range** `v` | source | Upstream guarantee ⊆ downstream assumption | rail 11.60–12.51 V ⊆ 11.4–12.6 V **[H3]** | Yes |
| **Current range** `i` (summed over sinks) | sinks | The source's guarantee holds only up to a load current | amp 1.32–1.84 mA + others 20 mA ≤ the rail's checked 24 mA **[H3]** | Declared in `Power`; no current probes yet (roadmap §4.1) |
| **Source / output impedance** `z_src`, `z_out` (vs frequency) | source | The divider at every connection; a filter's corner | 4653–4747 Ω **[H1]** | `z_src` declared, no measure |
| **Input / load impedance** `z_in`, `z_load` (vs frequency) | sink | Same | 7445–8145 Ω **[H1]** | `z_load` declared, no measure |
| **Bandwidth and flatness** | source | Whether a downstream frequency measure composes (§2.3) | amp 0.51% below its plateau at 263 Hz **[H2]** | Only as specs on the whole block |
| **DC level of a DC-coupled port** | both | A DC-coupled neighbour moves the operating point | — (all our AC links are capacitor-coupled) | No |
| **Supply-side port** (ripple rejection, supply current vs frequency) | both | A real rail is a port too; the CE amp was characterized on an ideal VCC | — | No |
| **Large-signal I/V curve, swing, edge rate** | both | Clipping, slew and logic levels are nonlinear; impedance is a small-signal number | — | `Logic` has `voh/vil/c_in` only |
| **Shared environment** (temp, supplies from the same source) | the parent | Correlation between blocks (§1.4) | bias max 6.5255 vs 6.3646 V **[H3]** | `temp` shared (E16); `#[env]` parses, no meaning |

The first four are the minimum for a linear small-signal port. The last three are needed as soon as the port is DC-coupled, a supply, or driven hard.

### 1.4 Correlation: the second way naive composition goes wrong

Two blocks on one board see **the same temperature and, often, the same supply**. If the parent composes each child's range separately, it silently allows "stage 1 at −10 °C while stage 2 is at +60 °C", which no real board does. The composed range is then too wide. The engine already solves this inside one block by evaluating every measure per run (D-F: `top_room` 4.84 V as intervals, 5.1951 V per run). Across blocks, the same fix is a **join**: combine only child results computed at the same values of the shared knobs.

---

## 2. Four ways to reuse, measured

### 2.1 The methods and the test circuits

```
   (a) declared         (b) interface knobs            (c) characterized tables        (d) in context
   ────────────         ───────────────────            ────────────────────────        ──────────────
   use the child's      check the child over a         cache the child's port           simulate the flat
   guarantee as a       RANGE of load / source         behaviour at every point of      design at every
   range; no runs       impedance; the parent          its knobs (incl. the shared      corner of every
                        checks the real neighbours     ones); the parent evaluates      knob (the truth)
                        are inside that range          algebra, not the simulator
```

| # | Chain | Why it's here | Flat knobs | Truth |
|---|---|---|---|---|
| H1 | CeAmp → CeAmp (next_boards B1) | Load-side loading; one definition placed twice; f_low | 14 | 16,384 corners, 14.1 s |
| H2 | CeAmp → RC low-pass (R_f 10 kΩ ± 1%, C_f 4.7 nF ± 10%) | Source-side loading; a filter corner; an edit | 10 | 1,024 corners |
| H3 | Zener + emitter-follower rail → CeAmp (DC) | A two-way DC interface (V one way, I the other); correlation through temperature | 11 | 2,048 corners |
| H4 | CeAmp → CeAmp → CeAmp | Scale: flat is out of reach, tables aren't | 20 | 2^20 corners ≈ 266 s (estimated from 0.254 ms/run) |

**How (c) works for linear AC: the two-port table.** For each corner of CeAmp's 8 knobs, two runs give its **ABCD matrix** at every frequency. The ABCD matrix is the standard 2×2 description of a two-port: V1 = A·V2 + B·I2, I1 = C·V2 + D·I2. Run 1 drives the input with the output open, which gives A and C. Run 2 shorts the input and pushes 1 A into the output, which gives B and D. A cascade is a matrix product, so the gain of s1 → s2 is `1 / (A1·A2 + B1·C2)`. **The load is not an axis of the table.** It enters when the parent multiplies the matrices, which is why one table fits every neighbour. Z_in = A/C and Z_out = B/A come out of the same table (§1.2). This is exactly what a Touchstone S-parameter file holds for RF parts.

### 2.2 H1: the two-stage amp

End-to-end gain |H(1 kHz)| and f_low (spec `bass: f_low <= 40 Hz`):

| Method | How | Gain | f_low max | Encloses the truth? | Simulator runs |
|---|---|---|---|---|---|
| (a) declared | CeAmp's `gain in 4.6 ± 5%`, squared | 19.10–23.33 | not composable | **No**: 40% high | 0 |
| (a′) characterized, unloaded | CeAmp's own extremes, squared | 19.9074–22.1166 **[H]** | not composable | **No** | 256 |
| (b) interface knob, `z_load in 7.4k..=8.2k` | s1 with R_load a range knob; × s2's port gain | 12.2223–14.0076 **[H]** | not composable (a resistor is not Z_in(f)) | Yes (0.046 / 0.078 wide); check Z_in2 ⊆ range: ✓ | 512 + 256 |
| (b) `z_load in 5k..=1M` | same, looser | 10.3115–22.0121 **[H]** | — | Yes, but useless (spec is 13.2 ± 10%) | 512 + 256 |
| (b) `z_load >= 10k` (language §0's example) | same | 13.5859–22.0121 **[H]** | — | **No**, but the composition check FAILs: 7445–8145 Ω ⊄ ≥ 10 kΩ | 512 + 256 |
| (c) ABCD table, joined on (temp, vcc) | one CeAmp table for s1 **and** s2 | **12.2681–13.9299** **[H]** | **34.018 Hz** **[H]** | Exact: max per-corner error 2.8e-14 (gain), 5.2e-14 (f_low) | **512** (0.38 s) + 2.6 s of algebra |
| (c′) same table, **not** joined | each stage picks its own temp and vcc | 12.2681–13.9473 **[H]** | — | Yes, 0.017 too wide at the top | 512 |
| (d) flat | all 2^14 corners | 12.2681–13.9299 **[H]** | 34.018 Hz **[H]** | — | 16,384 (14.1 s) |

What this shows:
- **(a) is unsound here, and would PASS a design whose real gain is 12.3–13.9.** It isn't a small error: the guarantee answers a different question (unloaded gain).
- **(b) is sound exactly when its composition check passes.** With the wrong assumption (≥ 10 kΩ), the composed lower bound 13.59 is above the true minimum 12.27, and it's the check that saves us. So the check is not optional. It is what makes (b) sound.
- **(c) is not an approximation for linear small-signal measures.** The ABCD product is the same algebra ngspice does on the flat matrix, just in a different order. It even composes f_low, which (a) and (b) can't. f_low is a property of the whole frequency response, so a table over frequency is needed.
- **The correlation here is small** (13.9299 vs 13.9473), because the gain barely depends on temperature and supply. H3 is where it matters.
- next_boards' corner join (12.2508–13.9730 in 768 runs **[B]**) is method (b) joined on (temp, vcc). With my slightly wider range it's 12.2223–14.0076. Joining changed nothing here, because both stages hit their extremes at the same (temp, vcc) corner.

### 2.3 H2: amp → RC filter, and an edit

Here the loading comes from the **source** side. The filter's corner is 1/(2π·R·C) only if it's driven from zero impedance. The amp's 4.7 kΩ output impedance adds to R.

| Method | Chain f_high (−3 dB, upper) | Chain gain at 1 kHz | Encloses? | Runs |
|---|---|---|---|---|
| (a) filter checked alone on its default bench (ideal source); amp's `gain 4.6 ± 5%` | 3040.3–3791.3 Hz **[H]** | 4.152–4.671 **[H]** | **No** for both: the true corner goes down to 2100.6 Hz | 8 |
| (b) filter with `input.z_src <= 100 Ω` | 3010.7–3791.3 Hz | 4.2353–4.5480 | **No**, but the check FAILs: amp z_out 4653–4747 Ω ⊄ ≤ 100 Ω | 8 |
| (b) filter with `z_src in 0..=5k` | 2033.3–3791.3 Hz | 4.0058–4.5480 | Yes | 8 |
| (b) filter with `z_src in 4.5k..=5k` | 2033.3–**2606.2** Hz | 4.0058–4.3921 | **Gain yes; f_high no**: the truth reaches 2632.1 Hz (1.0% above) | 8 |
| (c) CeAmp table (the H1 table, reused) + filter table | **2100.6–2632.1 Hz** | **4.0236–4.3825** | Exact (error ≤ 3e-15) | 512 the first time, 0 once cached; + 8 |
| (d) flat | 2100.6–2632.1 Hz **[H]** | 4.0236–4.3825 **[H]** | — | 1,024 (0.67 s) |

**Why (b) misses f_high by 1%, with its check passing.** The chain's −3 dB point is measured from the chain's **own peak**. The peak sits at 263.0 Hz, where the amp's low-frequency roll-off and the filter's droop overlap. There, the amp is 0.51% below its plateau and the filter gives 0.9949 **[H]** (`h2_check_fhigh.py`). The chain's peak is lower than either block's own, so its −3 dB level is lower and its corner higher. **A bandwidth is not a function of the blocks' bandwidths.** Composing it needs an extra guarantee from the amp, "flat within x dB from f1 to f2", plus a rule that turns that flatness into a margin on f_high. Or it needs (c), which carries the whole response. This is the same trap as in timing analysis, where a path delay is not the sum of datasheet delays measured on each cell's own test load.

**The edit (C_f 4.7 nF → 3.3 nF).** The CeAmp table's key didn't change, so it was a cache hit (0 runs). Only the filter table was rebuilt (8 runs, 7 ms). The composed result 2977.8–3725.6 Hz matched a fresh flat run of 1,024 corners exactly **[H]**. A flat re-check costs 1,024 runs, and (c) cost 8.

### 2.4 H3: a rail feeding the amp (DC, two-way, correlated)

```
   raw 15 V ± 5% ──R_z 1k──●── zener 12.7 V ± 1%, +6 mV/K ── ⏚
                           │
                      ┌────┴────┐
                      │  pass   │ β 100..300
                      │  NPN    │
                      └────┬────┘
                           ●──── vout ───► CeAmp (draws 1.35–1.82 mA) ; other consumers: 0..20 mA
                        (rail: 11.60–12.48 V in context)
```

The rail's voltage **rises** with temperature: the zener's +6 mV/K beats the pass transistor's VBE drift. The amp's bias **falls** with temperature and rises with supply. Cold is where the bias is highest, and cold is also where the rail is lowest. The contract has to keep that correlation. The spec is the amp's `bias: dc(output.v) in 4.5V..=6.5V`, and its decisive side is the maximum.

| Method | How | Bias range | Bias max vs 6.5 V | Runs |
|---|---|---|---|---|
| (a) declared | Rail's `vout in 12 V ± 5%`, checked over load 0.5–24 mA (11.5960–12.5118 V ✓), and the amp's current + 20 mA ≤ 24 mA ✓. The amp's standalone check covers 11.4–12.6 V | 4.6921–6.5595 **[H]** | **Can't prove PASS** (> 6.5) | the blocks' own checks (here the rail table, 384, and the amp table, 448) |
| (b) characterized rail range | The rail's actual range 11.5987–12.4844 V as the amp's supply knob, independent of temperature | 4.7285–6.5255 **[H]** | **Can't prove PASS** | + 128 |
| (b) joined on temperature | Per temperature corner: rail 11.5987–11.9395 V at −10 °C, 12.1204–12.4844 V at 60 °C; the amp at that same temperature | 4.8236–**6.3646** **[H]** | **PASS**, by 135 mV | + 128 |
| (c) tables + fixed point | Rail table: V(vout) over its 5 knobs × 12 load currents. Amp table: bias and current over its 6 knobs × 7 supply voltages. For each flat corner, iterate "current → rail voltage → amp current" to a fixed point, then read the bias | 4.8236–6.3641 **[H]** | PASS | 384 + 448 (0 at composition; 0.01 s) |
| (d) flat | all 2^11 corners | 4.8236–**6.3640** **[H]** | PASS, by 136 mV | 2,048 (0.35 s) |

What this shows:
- **(a) is sound here**, unlike in H1 and H2, because this contract carries the two-way quantity: the rail's guarantee was checked over a load-current range, and the amp's current was checked to fit in it. **A declared guarantee is safe when the contract names every quantity that crosses the port.**
- **But (a) and unjoined (b) turn a PASS into a false alarm.** The standalone amp's worst case (cold with a 12.6 V supply) can't happen on this board, because a cold rail is at most 11.94 V. Composition bounds are one-sided. They can prove a PASS, never a FAIL (next_boards P2). So the honest verdict from (a) is UNDECIDED, and `next` would say "join on temperature" or "simulate in context".
- **Joining on the shared knob is what recovers the truth**: 6.3646 against 6.3640 V, 0.6 mV pessimistic.
- **The two-way loop is safe here, and the engine can show it.** The amp's current changes the rail's voltage, which changes the amp's current. Worst over all corners, the loop gain (rail output resistance × the amp's supply conductance) is **4.5e-3** **[H]**. It is far below 1, so the fixed point is unique and the iteration contracts (4.5 iterations on average). This is the static version of the circular assume-guarantee problem in next_boards §3.3. A loop gain below 1, computed from the two tables, is a checkable condition for accepting it.
- (c)'s error, 9.8e-5 V, comes from linear interpolation between table points in current and supply. It is not a bound. Section 4.4 gives the rule for this.

### 2.5 H4: three stages, where the tables stop being optional

| | Tables (c) | Flat (d) |
|---|---|---|
| New simulator runs | **0** (the cached CeAmp table: same key, cache hit) | 1,048,576 |
| Time | **0.3 s** of algebra | ≈ 266 s serial (0.254 ms/run × 2^20) **[H]** |
| Gain range | 33.7324–41.3124 **[H]** | — |
| Check | The two extreme corners re-simulated flat: 33.732379 and 41.312380, equal to 6 digits; 300 random corners: max relative error 3.9e-14 **[H]** | — |

This is the payoff the user asked about. The CeAmp was simulated once, 256 corners × 2 runs, and its table serves every placement in every design. The parent's work is matrix products. The **two re-simulated extreme corners** are the in-context confirmation. Any FAIL found by composition gets the same treatment: one flat run at the corner, which is the counterexample the report prints.

### 2.6 Scorecard

| | (a) declared | (b) interface knobs | (c) characterized tables | (d) in context |
|---|---|---|---|---|
| Sound? | Only if the contract carries every port quantity (H3 ✓; H1, H2 ✗) | Yes for scalar measures, **if** the neighbour-in-range check passes; no for composed shape measures (H2 f_high, −1.0%) | Linear AC: exact. DC with interpolation: estimate (9.8e-5 V) unless bracketed. Large-signal: no | The truth, at the corners |
| Correlation of shared knobs | Lost | Lost unless joined (H3: 6.5255 → 6.3646 V) | Kept by the join key | Kept |
| Tightness | Poor (H3: false alarm) | Depends on the range written | Exact / near exact | Exact |
| Runs, first time | 0 | child × 2^(interface knobs) | child × 2 per corner (AC), × grid (DC) | 2^(all knobs) |
| Runs after an edit elsewhere | 0 | 0 while the neighbour stays in range | 0 (H2 edit: 8 vs 1,024) | all |
| Can prove a FAIL? | No | No | No (a composed FAIL is confirmed by one (d) run) | Yes |

---

## 3. What the references do

### 3.1 Static timing analysis and Liberty: the closest analogy

Digital chips have billions of transistors, and nobody simulates them flat to check timing. Static timing analysis (STA) composes **pre-characterized cells**, and the scheme matches method (c) almost point for point.
- **The cell library (Liberty `.lib`)** stores, for each timing arc of each cell (for example, input A rising to output Y falling), a **delay table** and an **output transition table**. They are two-dimensional lookup tables whose axes are the **input transition time** (slew) and the **total output load capacitance** (the NLDM model: a `lu_table_template` with `index_1` = input_net_transition, `index_2` = total_output_net_capacitance; Liberty Reference Manual).
- **Each library is characterized at one PVT corner** (process, voltage, temperature: `nom_process`, `nom_voltage`, `nom_temperature`), for example slow-slow at 0.72 V and 125 °C. STA analyses the **whole chip at one corner at a time**. Every cell sees the same temperature and supply. That is our join on shared knobs.
- **STA composes along a path**: it looks up a cell's delay using the **actual** load on its output (the summed input capacitances of the cells it drives, plus the wire) and the **actual** slew arriving from the previous cell. It then passes the output slew on to the next cell. The load is not baked into the table. It enters at composition, like the ABCD load in §2.1.
- **Every table has a range, and STA checks that the design stays in it.** Output pins carry `max_capacitance` and `max_transition`. A net whose load or slew exceeds them is a design-rule violation, because the table would be extrapolating. That's the neighbour-in-range check of method (b).
- **Variation within a die.** One corner covers the shared knobs. Instance-to-instance variation (our per-placement knobs) is added on top by on-chip-variation derates, then depth-aware AOCV, then per-arc statistical sigma tables (POCV, Liberty Variation Format). That is the same split as ours: range knobs joined, statistical knobs per placement.
- **When a lumped capacitor isn't enough.** With resistive interconnect, the driver doesn't see its whole load. The tools compute an **effective capacitance** by iterating between the driver's table and the RC load (Qian, Pullela and Pillage, IEEE TCAD 1994). That's H3's fixed point in another form. Current-source models (CCS, ECSM) replace the delay number with the driver's current waveform, closer to a Thevenin/Norton port model.
- **Hierarchy.** A finished block is replaced by an **extracted timing model** (ETM: the whole block written as one Liberty cell, with tables over input slew and output load) or an interface logic model (only the logic near the ports is kept). This is method (c) one level up, and the block's own characterization cache.

| STA | Our engine |
|---|---|
| Cell | Block definition (CeAmp) |
| Timing arc table over (input slew × output load) | Two-port table over (knob corner × frequency); the load enters at composition |
| PVT corner, one per analysis | Shared range knobs (temp, supply), joined |
| OCV / AOCV / POCV / LVF | Per-placement statistical knobs |
| `max_capacitance`, `max_transition` | The interface assumptions `z_load`, `z_src`, and the neighbour-in-range check |
| Effective-capacitance iteration | The fixed point on a two-way DC interface (H3) |
| ETM / ILM | A block's cached table, reused by every parent |
| Sign-off SPICE on critical paths | In-context rung (d) at decisive corners |

**Why STA gets away with it** is also a lesson. Digital delay is **monotone** in slew and load, and a digital signal is **unidirectional** once it's restored to a logic level. Analog ports have neither property, so our tables must carry the impedances (two-way), and our compositions need the in-context confirmation.

### 3.2 IBIS: port models for board-level signal integrity

IBIS (the I/O Buffer Information Specification) describes a chip's I/O buffer **by its port behaviour, not its transistors**. It stores:
- the pull-up and pull-down I/V tables and the power- and ground-clamp I/V tables;
- the edge rate ([Ramp]) and V/t waveforms ([Rising Waveform], [Falling Waveform]) into a stated test fixture (R_fixture, V_fixture);
- the die capacitance C_comp, and the package R, L and C.

Each quantity has **typ / min / max** columns, and each column is a correlated corner (slow process with low voltage and high temperature together).

Why IBIS matters here: it stores a **relation between current and voltage** at the pin, not a voltage. The board simulator then solves the actual line and receiver **in context**, using the model. That is the two-way lesson of §1.2 applied to large signals. For our language, an IBIS-like **port I/V characterization** is the form (c) must take once a port is driven hard or is nonlinear (logic, clipping amplifiers, current-limited supplies).

### 3.3 Analog and mixed-signal flows

- **Mixed-level simulation.** Analog design environments let each instance use either its transistor-level schematic or a behavioural view (Verilog-A/AMS), chosen per instance (Cadence's config views and Hierarchy Editor). A behavioural model is only as good as its **port model**. A Verilog-A amplifier with no output resistance or input capacitance repeats exactly the error of method (a).
- **Real-number models and impedance.** Fast mixed-signal verification models analog signals as real numbers moving one way (Verilog-AMS `wreal`, SystemVerilog real-number modelling). That loses loading. The known fix is a net type that carries a Thevenin triple (voltage, current, resistance) with a resolution function, so connected models share a node the way circuits do (Cadence's EEnet package). It's the same two-way requirement.
- **Macromodels from characterization.** For linear ports: n-port tables over frequency (Touchstone `.sNp` files), fitted to rational models by vector fitting (Gustavsen and Semlyen, IEEE TPWRD 1999), with passivity enforced. For nonlinear blocks: behavioural models fitted to characterization runs, then **validated** against the transistor level in regression. Our H4 spot-check is that validation.
- **SPICE engines don't reuse sub-results.** ngspice expands every `.subckt` into a flat device list before analysis (`src/frontend/subckt.c`) and solves one matrix. Reuse has to come from the layer above the simulator, which is the bounds engine.

### 3.4 PolymorphicBlocks: parameter propagation

PolymorphicBlocks (Lin et al., UIST 2020) is a board-level HDL whose ports carry **interval-valued parameters**: a voltage source's output voltage and current limit, a sink's voltage limits and current draw. A **link** connects ports and computes the net's values: the voltage from the source, and the current as the **sum** of the sinks' draws. It then asserts containment: sink voltage limits ⊇ the link's voltage, source current limit ⊇ the summed draw. This is method (a) done properly for DC power, with the two-way quantity (current) included, exactly like H3's (a). It uses plain intervals, so it has H3's weakness: no correlation through shared knobs, and a false alarm where the truth passes. It checks connections. It doesn't simulate.

### 3.5 Contract theory and SysML v2

- **Assume-guarantee contracts** (Benveniste et al. 2018; Nuzzo et al., Proc. IEEE 2015, applied to aircraft electrical power systems). Composition takes the conjunction of the guarantees and discharges assumptions against the other components' guarantees. Refinement is A′ ⊇ A, G′ ⊆ G. Circular dependencies (A's assumption is B's guarantee and vice versa) need an extra argument: induction over time for dynamic systems (Abadi and Lamport, TOPLAS 1995; McMillan, CHARME 1999), or a unique fixed point for static ones (H3's loop gain < 1).
- **SysML v2** requirements have `assume constraint` and `require constraint` members and a `satisfy` relation from a part to a requirement. That's the same shape as our `assume`/`spec`, at the systems level, without circuit semantics.

---

## 4. What this means for the language and the engine

### 4.1 What a block's contract must declare so a parent can reuse it

1. **Its interface assumptions, as bounded ranges on both sides of every port.** For a signal output: `output.z_load in 5kΩ..=1MΩ`, not "≥ 10 kΩ" when the real load could be 7.4 kΩ. For a signal input: `input.z_src`. For a supply input: the supply range and, on the source side, the load current range. Each becomes a **range knob of the child's own default bench** (language §8.5: "outputs get the assumed loads"). So the child's standalone check covers every neighbour inside the range.
2. **Its port guarantees**: `z_in`, `z_out` (at the frequencies that matter), the supply current, the output range. These are the numbers a parent checks against the neighbours' assumptions.
3. **Its flatness, where a frequency measure is meant to compose** (§2.3).
4. **Which of its knobs are environment knobs** (temp, supplies) and so are shared with the parent, and which are its own parts' knobs.
5. Optionally, **how much it wants reused**: a block can mark itself `in_context` when its behaviour can't be captured at its ports (a thermal or magnetic neighbour, or a block that shares a ground return).

### 4.2 How a child's bench relates to the parent's context

```
                  child standalone                          parent context
   ┌──────────────────────────────────────┐    ┌──────────────────────────────────────┐
   │ default bench = the assumption box    │    │ the real neighbours                  │
   │   VCC  : DC source over 11.4..12.6 V   │    │   rail: 11.60..12.48 V  [H3]          │
   │   input: AC source, z_src over a range │ ⊇? │   s1's z_out: 4653..4747 Ω  [H1]      │
   │   output: z_load over 7.4k..8.2k       │    │   s2's z_in:  7445..8145 Ω  [H1]      │
   │   temp : −10..60 °C                    │    │   temp: the board's range             │
   └──────────────────────────────────────┘    └──────────────────────────────────────┘
      the child's specs hold over this box        reuse is sound iff context ⊆ box (every port, both ways)
```

**The bench is the envelope.** The child's verdict says "PASS for every neighbour inside this box". The parent's job is a **containment check per port quantity**, which is language §8.7's "Connections" check extended to impedances and currents. When containment passes, the child's verdict carries over and is recorded as resting on the check (next_boards' `rests_on`). When it fails, the child's verdict does not apply in this placement. The engine then checks the child's specs **in context** (method (d) or (c)), and the report shows both: "s1.gain: 4.46–4.70 standalone (z_load assumed ≥ 10 kΩ); 2.8x in context: z_load is 7.4–8.1 kΩ".

A sheet bench (language §8.5, `bench LoadStep for Buck3v3`) is the same idea: a stated context. Its results are reusable in a parent whose context the sheet bench encloses, and in no other.

### 4.3 How caching keys work

```
   table key  =  hash(  child definition after its own elaboration (params applied, device models, D-A defaults)
                      + its cone knobs: path-relative names, physical lo/hi (engine_types §2.1)
                      + interface-knob axes, if any (DC tables: supply grid, load-current grid)
                      + analysis: op | two-port over band B at d points per decade | …
                      + engine settings (tolerances) + backend identity )            ← like rev (§9.3), per block
   NOT in the key: the placement path, the parent, the neighbours, the other placements
   rows keyed by physical knob values (T6), so a range edit keeps the rows it still covers
```

What that gives, measured:
- **One table per definition, shared by placements.** H1's s1 and s2 used one CeAmp table (512 runs). H4's three stages read it from the cache (key `ceamp-0010241775b4`, 0 new runs).
- **Shared across designs.** H2's amp-plus-filter chain read the same cached table: 0 runs after its first build.
- **An edit invalidates only what it touches.** H2's filter edit rebuilt the 8-run filter table. The parent recomposed in milliseconds.
- **A per-placement override is a new key for that placement only.** If s2 had R_C = 5.6 kΩ, s2 would get its own 512-run table and s1 would keep the old one. A flat re-check would redo all 16,384 corners.
- **Join keys are the environment knobs**, stored as physical values in every row, so the parent can pick "all child rows at temp −10 °C, vcc 12.6 V".

**The per-placement knobs are not in the table's identity. They are its rows.** The table holds every corner of the child's own knobs. The parent picks one row per placement, independently, because two placements are two physical sets of parts (E16), and one row per shared-knob value, jointly.

**Limits of corner tables.** A table of corners serves `worst_case` enumeration. `sigma(3)`'s search visits interior points, which a corner table doesn't hold. For those, the child's runs are memoized by physical key on demand: the same T6 run table, scoped to the block definition instead of the whole design. The table's size is 2^(the child's cone), so a child with 20 knobs needs a million rows. Beyond that, the child's rows need the scale machinery (the loop, or a surrogate) exactly as a flat design would.

### 4.4 When in-context simulation is still required

| Situation | Why tables or guarantees can't decide it | H-evidence |
|---|---|---|
| **A FAIL** | Composition bounds prove PASSes only (next_boards P2). A composed worst corner becomes a real counterexample only after one flat run at that point | H4: both extreme corners re-simulated, equal to 6 digits |
| **A containment check fails** and there is no table | The child's verdict doesn't apply in this placement | H1 with `z_load >= 10k`; H2 with `z_src <= 100 Ω` |
| **Measures of the whole response** without a table over frequency | f_low, f_high and phase margin don't compose from scalars | H2 f_high: (b) 1.0% short |
| **Large-signal or nonlinear behaviour at a port**: clipping, slew, current limit, start-up, logic edges | A small-signal table is exact only around one operating point; large signals need I/V (IBIS-like) models or the circuit | — (not tested here) |
| **DC-coupled ports** without a DC interface table and fixed point | The neighbour moves the operating point, and the AC table depends on it | H3 shows the fixed point works when tables exist |
| **Coupling that doesn't go through a port**: self-heating of a neighbour, magnetic coupling, shared ground return, substrate | Not in any port quantity | — |
| **Interpolated DC tables near a spec limit** | The table value is an estimate (H3: 9.8e-5 V). Rule: if the margin is larger than a stated interpolation bound, PASS; otherwise confirm in context | H3: margin 136 mV ≫ 0.1 mV |
| **A circular interface whose loop gain isn't shown to be < 1** | The fixed point might not be unique | H3: 4.5e-3 |
| **Regression** of the cache itself | A table can go stale in ways the key doesn't see (a backend change) | Periodic in-context spot checks, like H4's 300 random corners |

### 4.5 Three kinds of knobs across a hierarchy

| Kind | Examples | In the child's table | In the parent | Correlation |
|---|---|---|---|---|
| **Per-placement** | `s1.r1.value`, `s2.q1.beta` (E16) | Rows (every corner) | Chosen independently per placement | Independent across placements; a `lot` knob (next_boards B3) is created in the parent and ties them |
| **Shared environment** | `temp`, a supply that every block sees | Rows, and the **join key** | One value for the whole board per corner, joined across children | Fully shared. Composing without the join loses it (H3: 6.5255 vs 6.3646 V) |
| **Interface** | `output.z_load`, `input.z_src`, the rail's load current, the amp's supply voltage in a DC table | An axis of the table (DC) or absent (linear AC: the ABCD load enters at composition) | **Bound** by the neighbour's actual value, or checked to lie in the child's assumed range | Determined by the neighbour's own knobs. Treating it as free decorrelates it: that's why next_boards' corner join was 0.017–0.043 too wide |

**How a knob changes kind across a level.** The child's `vcc.v` is a range knob (from its `assume`) when checked standalone. In the parent it is no longer a knob. It is **bound** to the rail's output, which is a function of the rail's knobs and of temperature. The child's `output.z_load` is a range knob standalone. In the parent it is bound to the neighbour's Z_in. And `temp` stays a knob at every level, the same knob, which is why it's the join key. So: **assumptions on ports become interface knobs in the child, then bindings in the parent; assumptions on the environment stay knobs all the way up.**

### 4.6 The evidence ladder, revised

engine.md §7 lists three rungs. The tests suggest four, each with its own rule:

| Rung | Evidence | Proves | Needs |
|---|---|---|---|
| 1 | Declared guarantees, with containment checked **on every port quantity, both ways** | PASS (conditional), resting on the child's verdict and the containment checks | The contract carries impedances and currents (§1.3) |
| 2 | Interface-knob verdicts (the child checked over its load/source range), joined on the shared knobs | PASS (conditional) for scalar measures | Bounded interface ranges; the join |
| 3 | Characterized tables composed in the parent (ABCD for linear AC; DC tables with a fixed point) | PASS (composed): exact for linear AC, estimated for interpolated DC | The characterization cache; loop gain < 1 for circular DC interfaces |
| 4 | In-context simulation | PASS and FAIL | The flat deck: the truth and the FAIL confirmation |

**On D-F (no affine forms in M3).** engine.md §7 and language §8.8 say the middle rung uses "affine forms, so shared knobs stay correlated". These tests suggest the middle rung doesn't need forms. **Per-run tables joined on the environment knobs keep the correlation**, exactly at the corners (H1, H2, H4), in the same way D-F's per-run evaluation does inside one block. Forms would add linearization error (D-F cites 29.98 Hz vs the true 32.99 Hz) that the ABCD product doesn't have. Forms may still earn a place for datasheet arithmetic and error budgets. For hierarchy, joined tables look like the better format. language §8.8's wording would need to change accordingly (not edited here).

### 4.7 What changes in the plan's types (small, and mostly already proposed)

- `KnobSpec.origin` (next_boards §8) gains `Interface(port quantity)`: a knob that exists in a child's bench and is bound in a parent.
- `FlatContract` as contract instances with placement paths (next_boards §8), plus a per-instance field saying how each instance's verdict was reached (rung 1–4).
- `Record.rests_on` (next_boards §8) lists the containment checks and child verdicts a composed PASS depends on.
- A **block-scoped run table**: engine_types T6 unchanged, but keyed by the block key of §4.3 instead of the global `rev`, stored under `.spicy/blocks/<key>`.
- A composition step in the engine that reads child tables and evaluates the parent's measure per joined row. For linear AC it is the ABCD product. For DC it is a fixed point.

None of this is M3 (D-D, D-F). Only the seams already listed in next_boards §8 are needed now.

---

## 5. What the contract syntax must support

A sketch, not a proposal of final syntax. It follows language v0.1's forms.

```rust
contract CeAmp {
    // environment: shared with the parent, joined across blocks
    assume temp in -10°C..=60°C;
    assume vcc.v in 11.4V..=12.6V;

    // interface assumptions: bounded, both ways; each is a knob of the default bench
    assume input.z_src  in 0Ω..=1kΩ;
    assume output.z_load in 5kΩ..=1MΩ;

    // port guarantees: what neighbours may rely on (checked on this block's own bench)
    spec z_in:  input.z_in.at(1kHz)  in 7kΩ..=8.5kΩ;
    spec z_out: output.z_out.at(1kHz) <= 5kΩ;
    spec i_vcc: dc(vcc.i) in 1mA..=2mA;
    spec flat:  ac(output.v / input.v).band(100Hz..=10kHz).ripple() <= 0.1dB;

    // behaviour, over every load in range
    spec gain: ac(output.v / input.v).at(1kHz).mag() in 2.5..=4.8;
}
```

The language must be able to say:
1. **Port impedances and currents as measures**: `port.z_in`, `port.z_out`, `port.i`, frequency-dependent with `.at(f)` and `.band(…)`. Current probes are the prerequisite (roadmap §4.1).
2. **Bounded interface assumptions on both sides of every port** (`z_load`, `z_src`, load current), which become bench knobs. An unbounded end (`>= 10kΩ`) needs a defined meaning: "up to open circuit", simulated as a limit point.
3. **Which knobs are environment knobs**: shared, inherited from the parent (language §8.2), and used as join keys. `#[env(temp = temp + 15K)]` at a placement must mean "this placement's temp is bound to the parent's temp + 15 K", which moves the join.
4. **The containment check, generated at every connection** for every port quantity both ways (language §8.7 "Connections"), with both lines shown on failure.
5. **Child specs in context**: a parent can ask for a placement's own contract to be checked where it's placed, and the report keeps both results (standalone and in context).
6. **A reuse level per placement or per block**: `#[evidence(declared | characterized | in_context)]`, defaulting to the lowest rung that is sound for the measure (§4.4), with `in_context` forced for blocks with non-port coupling.
7. **Flatness or response-shape guarantees**, so that frequency measures can compose without a full table.
8. **Large-signal port models** later (swing into load, I/V tables, edge rates for `Logic`): the IBIS-like form of characterization.
9. **Lots across placements** (next_boards B3), created in the parent, as the one way per-placement knobs become correlated.
10. **Rests-on in the output**: the report must say which child verdicts and containment checks a composed PASS depends on.

---

## 6. Open questions

1. **A new verdict word?** A rung-3 PASS on linear AC is as exact as enumeration (1e-14). On interpolated DC it's an estimate. Is it "PASS (all corners)", "PASS (estimated)", or a new "PASS (composed)" that names the tables it read?
2. **Interpolation bounds for DC tables.** H3's error was 9.8e-5 V with 7 supply points. Should the engine bracket instead, using monotonicity in the interface knob (evaluate both neighbouring grid points and take the worse)? That is sound only if monotonicity is shown, which is next_boards' open question 4 again.
3. **Unbounded interface ranges.** `z_load >= 10kΩ` is natural to write. Simulating "open" as an endpoint works for linear loads. Does it work for every measure (stability can get worse with a *lighter* load)?
4. **Tables for `sigma(3)`.** Corner tables serve `worst_case`. The σ search visits interior points of the parent's space, which map to interior points of the children. Is on-demand memoization enough, or does each child need a surrogate?
5. **Supply ports as ports.** The CeAmp table assumed an ideal VCC. With a real rail, the amp is a three-port (in, out, vcc). Should every `Power<In>` automatically join the characterization, at the cost of a 3×3 table?
6. **Where the join key comes from.** `temp` is obviously shared. Is a supply shared when two blocks sit on one rail through different filters? The correlation is real but partial.
7. **When to characterize.** On first use (lazily, when a parent needs it), or whenever a block with a contract is saved (eagerly, like a library cell)?
8. **Large blocks.** A child with a 20-knob cone has a million-row table. Should the table then hold only the port-parameter extremes per environment corner (losing internal correlation, like next_boards' corner join), or should it hold a fitted model?
9. **Revisit language §8.8 and engine.md §7.** They say characterized results are affine forms. These tests suggest joined per-run tables instead (§4.6). That's a design decision for the user, not made here.
