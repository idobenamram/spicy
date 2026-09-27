# Engine Walkthrough: One Circuit, End to End

> Companion to `engine.md` and `engine_plan.md` · 2026-09-27 (rebuilt on the engine plan, numbers from ngspice)
> Paper keys like **[AGW94]** point into `bibliography.md`.

This follows one small circuit from "the engineer picked some values" to "the engine gives a verdict". Every number here comes from ngspice-42 running the plan's reference model (the Appendix lists what the model includes and leaves out).

The circuit is small: 8 things vary, so there are 2⁸ = 256 extreme combinations. That is few enough to **simulate them all**, and that is exactly what the MVP engine does. A real circuit can have too many knobs per spec for that, which is why the worst-point loop exists. It comes after the MVP (section 10).

---

## 1. The circuit and what the engineer typed

A common-emitter amplifier, the textbook one-transistor stage (`circuits/ce_amp.spl`):

```
                      VCC  12 V
                        │
            ┌───────────┴───────────┐
            │                       │
          R1 47k                  RC 4.7k
            │                       │
            │                       ●──────── out   (collector voltage VC)
   C_in     │                     C │
in ──||─────●──── base ───────── B ─┤  Q1   (NPN, current gain β)
   1 µF     │                     E │
            │                       │
          R2 10k                  RE 1k
            │                       │
           GND                     GND
```

| Part | Job |
|---|---|
| R1, R2 | **Divider**: holds the base at about 2.05 V, which sets the transistor's current |
| RE | Sets the current (≈ (2.05 V − 0.66 V) / 1k) and makes the gain depend on resistors, not the transistor |
| RC | Turns the current into the output voltage; with RE it sets gain ≈ RC/RE |
| C_in | **Coupling capacitor**: passes the audio signal, blocks DC. With the input resistance it makes a high-pass filter |
| Q1 | **The transistor**: does the amplifying |

The engineer runs one simulation at the nominal values:

| Quantity | Nominal |
|---|---|
| Collector current IC | 1.382 mA |
| Collector voltage VC | 5.503 V |
| Gain at 1 kHz | 4.591 |
| Input resistance | 7.93 kΩ |
| Low cutoff frequency f_L | 20.13 Hz |

And writes three specs:

| Spec | Requirement | Why | Nominal |
|---|---|---|---|
| **S1 bias** | 4.5 V ≤ VC ≤ 6.5 V | Room for the output to swing both ways without clipping | 5.503 ✅ |
| **S2 gain** | 4.6 ± 5% (4.37 … 4.83) | The next stage expects this level | 4.591 ✅ |
| **S3 bass** | f_L ≤ 30 Hz | Don't cut the bass | 20.13 ✅ |

**Everything passes at nominal.** This is where most engineers stop today. The engine's job is the next question: *does it still pass on every board we build, at every temperature, with a sagging supply, and (after the MVP) after years of use?*

---

## 2. When the engine runs

In the MVP, only when asked (engine plan §1.1):

| Trigger | What runs | CE amp |
|---|---|---|
| `spicy check ce_amp.spl`, typed by you or by an agent | The whole check of section 5, cold, every time. The report and every run are stored per design revision | 579 runs, 0.4 s |
| `spicy check --explain bias` | Reads the stored runs if the file hasn't changed | 0 runs, < 10 ms |
| `spicy sim --at <point>`, `spicy sweep <knob>` | One real run, or a few | 1–5 ms |
| `spicy check --deep` | The check, then whatever each UNDECIDED says would settle it | seconds |
| Editor era, on save or idle; later, on demand | The same full check; later, transient specs and statistics (yield) | 0.4 s; minutes |

**Why there's no "instant" estimate.** A full re-check costs less than one agent turn, and stored straight-line estimates were badly wrong for real edits. For C_in 1 → 2.2 µF, the stored line predicted f_L = −4.0 Hz. The re-check gives 12.177 Hz.

What the engineer sees is one row per spec side, at two confidences, with a verdict word and a concrete failing board if there is one (section 9).

---

## 3. Knobs: describing everything that can vary

A **knob** is one independent thing that can vary. Each knob has a range. There are two kinds, and the difference drives everything that follows.

### The two kinds

| | **Range knobs** | **Statistical knobs** |
|---|---|---|
| What | Operating conditions and slow changes | Manufacturing spread of each part |
| Examples here | temperature, supply voltage, aging | R1, R2, RC, RE, C_in, β |
| Who "chooses" the value | The world. **Every** board sees every temperature and eventually gets old | The factory. Each board rolls the dice **once** |
| The question | Must work at **every** value | How rare is a failing board? |
| How we treat it | Always take the worst value | Either take the worst (very conservative) or limit how unlikely the combination may be |

Why the difference matters: a board **will** see −10 °C if it's used outdoors in winter. But a board where all six parts landed at their worst tolerance edge at the same time is incredibly rare. Treating those two the same way is either reckless (averaging over temperature) or wasteful (assuming all parts are simultaneously worst).

### Our knobs

| Knob | Kind | Range | Where the number comes from |
|---|---|---|---|
| T | range | −10 … 60 °C | Product requirement |
| VCC | range | 12 V ± 5% (11.4 … 12.6 V) | Regulator spec over load and line |
| R1, R2, RC, RE | statistical | ±1% each | Resistor datasheet |
| C_in | statistical | ±20% | Electrolytic capacitor datasheet |
| β | statistical | 100 … 300 | Transistor datasheet (min/max hFE) |
| life | range | new … end of life (10 years) | Product lifetime. **Not in the MVP**; section 6 uses it |

The MVP has the first eight. The ninth, aging, comes later, but it is the best example of why the two kinds must be combined differently, so section 6 adds it.

### One part can depend on several knobs

Real part values are driven by their own statistical knob **and** shared range knobs:

```
C_in actual = 1 µF × (1 ± 20%)          ← its own statistical knob (factory)
                   × (1 − 0.2 · life)    ← shared "life" knob: electrolytics dry out (post-MVP)

β actual    = β_part × (T / 298.15 K)^1.5  ← the model's XTB = 1.5: about +0.5%/°C
              β_part = 200 gives 165.8 at −10 °C, 200 at 25 °C, 236.2 at 60 °C

VBE         = 0.659 V at 25 °C; 0.719 V at −10 °C; 0.598 V at 60 °C   ← same temperature knob
```

So the temperature knob moves **both** β and VBE, and it's the **same** knob for both. That's how correlation is represented: things driven by one knob move together.

### How the engineer sets them

Mostly they don't have to. **Range knobs** are declared once per project or block. **Statistical tolerances** come with each part: typed in, or pulled from the datasheet or part library. **Links** between parts and range knobs (tempco, aging) come from part models; for a bare `Npn`, the engine uses a default model and prints it in every report (the Appendix). From `ce_amp.spl`:

```
assume temp in -10°C..=60°C;                                        // range knob
assume vcc.v in 12V ± 5%;                                           // range knob
let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };             // statistical knob
let c_in = Electrolytic { p: base, n: input, value: 1uF ± 20% };    // (post-MVP: aging links it to life)
let q1 = Npn { c: output, b: base, e: emitter, beta: 100..=300 };   // tempco from the default model
```

### Two questions the engineer can ask

- **`worst_case`:** every knob, including every part, anywhere in its range. The most conservative answer. Used for safety-critical specs.
- **`sigma(3)`:** range knobs anywhere in their range; the parts together no less likely than a 3σ event. This is the default for specs. The answer is the value at the worst such board, per spec side.

The engine computes both for every spec side. Section 5 shows them giving different answers.

---

## 4. What the engine produces: inner, outer, and the true range

For each spec the engine wants the **true range**: every value the quantity can actually take, over all knob settings. In general you can't compute it: it's the extreme over an 8-dimensional box (or 50-dimensional, for a real circuit), and each point costs a simulation.

So the engine works with two things that squeeze the true range from opposite sides.

**An analogy.** You want to know the height of the tallest person in a country (the true maximum), to decide whether a 2.5 m door is tall enough.

- **Inner bound:** the tallest person you've actually **measured**, say 2.1 m. The true maximum is at least 2.1 m. You can't claim anything lower.
- **Outer bound:** a proof that **nobody** is taller than, say, 3 m. The true maximum is at most 3 m.
- The truth is somewhere between 2.1 and 3 m. The door (2.5 m) sits in that gap, so **you don't know yet.**
- If you measure someone at 2.6 m, the door definitely **fails**. You have a concrete example.
- If you prove nobody exceeds 2.4 m, the door definitely **passes**.

That's the whole idea:

> **Inner bounds prove failures. Outer bounds prove passes. The gap between them is how unsure we are.**

In the circuit:

- **Inner** comes from simulating concrete knob settings, i.e. specific "boards". Every simulated board can really be built, so its value really happens.
- **Outer** comes from math that covers the whole box at once. For purely linear pieces it can be exact (section 7). For anything through the transistor, the MVP has **no proof**. So each PASS word says what evidence stands behind it.

Three examples from our circuit:

| Quantity | Inner | Outer | True | Verdict |
|---|---|---|---|---|
| Divider voltage (VCC fixed at 12 V) | 2.0708 … 2.1402 | 2.0708 … 2.1402 | 2.0708 … 2.1402 | **exact** (section 7) |
| VC max vs 6.5 V | 6.5595 | — | 6.5595 | **FAIL, proven**: we simulated a board that hits 6.5595 V |
| VC min vs 4.5 V | 4.6921 (all 256 corners) | none proven | 4.6921 | **PASS (all corners)** |

The verdict words:

| Verdict | What we know |
|---|---|
| **FAIL** | A simulated, reachable board violates the spec. Always comes with that board's knob settings, re-run at tight tolerances |
| **PASS (all corners)** | Every corner simulated and passing, and nothing inside the box beat the worst corner (section 5.3). In the analogy: we measured every candidate on the shortlist of likeliest giants, and spot checks in between found nobody taller. Strong evidence, not a proof |
| **PASS (estimated)** | A search's worst board passes by more than the search's own uncertainty. Weaker: it rests on a search |
| **PASS (implied by worst case)** | For `sigma(3)`: the same side passes at `worst_case`, and every 3σ board lies inside the box |
| **PASS (guaranteed)** | Later: exact methods only (linear pieces, datasheet arithmetic) |
| **UNDECIDED (reason)** | The engine can't decide. It says why, and what would settle it |

---

## 5. The check, step by step (S1: collector voltage)

The MVP check is a fixed pipeline (engine plan §2). The whole thing, for all five spec sides at once:

```
 0  nominal + read-back of every knob ............    1 run
 1  simulate every corner (2^8) ...................  256 runs   → the worst_case value of every side
 2  look inside the box ...........................   46 runs   → does anything beat the worst corner?
 3  sigma(3): search for the worst 3σ board .......  265 runs   → the sigma(3) value of every side
 4  band: re-run the decisive boards, tighter .....   11 runs   → how exact is each number?
                                                     579 runs, 0.4 s
```

### 5.1 Step 0: nominal (1 run)

VC = 5.5032 V. The run also reads every knob's value **back** from ngspice and compares it with what was asked. A mismatch stops the check with UNDECIDED (binding). This isn't paranoia: a netlist that forgets the temperature line gives a dead temperature knob, and without the read-back that gave a false PASS on bias max in every configuration tried (engine plan §4.4). The read-back catches it after 1 run.

### 5.2 Step 1: every corner (256 runs)

A **corner** is a board with every knob at one of its edges: 2⁸ = 256 of them. They go to the simulator as one batch. Every run returns every measure (VC, gain, f_L), so the same 256 runs serve all five spec sides.

For S1:

| | VC | Board |
|---|---|---|
| Highest corner | **6.5595 V** | −10 °C, 12.6 V, R1 +1%, R2 −1%, RC −1%, RE +1%, β 100 (C_in either edge) |
| Lowest corner | 4.6921 V | 60 °C, 11.4 V, R1 −1%, R2 +1%, RC +1%, RE −1%, β 300 |

Only 2 of the 256 corners exceed 6.5 V: the same DC board with either C_in. 10 exceed 6.4 V; 50 exceed 6.0 V. **Bias max fails at `worst_case`**, with a board to show for it.

C_in can't reach a DC voltage (a capacitor is open at DC). So the report prints the counterexample with `c_in any`, otherwise a reader blames C_in.

**Which knobs matter?** The same table answers it, with no new runs. From the failing corner, flip one knob to its other edge (each neighbour is another corner that was already simulated):

| Flip | VC | Change |
|---|---|---|
| T → 60 °C | 5.8578 | −0.702 |
| β → 300 | 6.1395 | −0.420 |
| VCC → 11.4 V | 6.2049 | −0.355 |
| R1 → −1% | 6.4096 | −0.150 |
| R2 → +1% | 6.4204 | −0.139 |
| RC → +1% | 6.4375 | −0.122 |
| RE → −1% | 6.4502 | −0.109 |
| C_in | 6.5595 | 0 |

**Every single flip passes**, so this failure needs every knob at its edge at once. **Temperature is the biggest effect** (VBE drops as it warms, so the current rises). That's a range knob, so better resistors won't help with it.

### 5.3 Step 2: look inside the box (46 runs)

Corners can't show a maximum in the middle of the box. Three cheap guards look there:

- **Tangent check:** at each side's worst corner, nudge each knob 1% inward. For bias max, all 7 nudges lower VC (to 6.5537 … 6.5590 V). 38 runs over the five sides.
- **Audit:** 8 seeded random boards inside the box. Their VC is 5.23 … 5.99 V. Such points can show a FAIL or knock a corner off its throne; they never support a PASS.
- **Pooling:** every later run inside the box (the σ search's) is checked the same way, for free.

If anything beats a worst corner, a search (an "ascent") starts from it. On the CE amp nothing does. It matters elsewhere: in the plan's `pq` test circuit, a power peak of 8.7097 mW sits just inside the box, next to a worst corner at 8.7069 mW. Only the tangent check sees it.

So **bias min is PASS (all corners)**. The answer key agrees: all 256 corners, 1,000 random boards inside the box and a coordinate search from each side's best point never beat the best corner, on any side.

### 5.4 Step 3: `sigma(3)`, the worst 3σ board (265 runs)

**What `sigma(3)` asks.** Temperature and supply may take any value in their range. Each part varies by its own spread (normal, σ = tolerance/3, cut at the tolerance). A board counts if its parts together are no less likely than a 3σ event. The worst such board is the side's **3σ point**. In two of the knobs:

```
             R1 −1%                         R1 +1%
   β = 100   ┌───────────────◆──────────────● worst corner: 6.5595 V (every part at its edge)
             │        .·´´´´   ``·.         │
             │     .´                `.     │   ○ nominal parts
             │    :          ○         :    │   ◆ 3σ point: 6.2813 V
             │     `.                .´     │
             │        `·.,,,,,,,,.·´        │   the oval: every board at least as likely
   β = 300   └──────────────────────────────┘   as a 3σ event
```

The oval touches an edge only where one part alone sits at its tolerance edge. The corner, with every part at its edge, is far outside it.

**The answer for bias max:** −10 °C, 12.6 V, β = 111.5, R1 47.10k, R2 9.980k, RC 4.692k, RE 1.0015k, giving **VC = 6.2813 V**. That's a weak transistor (β 2.65σ below 200) with resistors only 15–21% of the way to their edges. The worst corner needs all six at their edges at once. That's why bias fails at `worst_case` and passes at `sigma(3)`.

**How the engine finds it.** It re-draws a straight line and jumps, until the line and the simulator agree:

1. **Start A:** the worst temperature/supply corner (−10 °C, 12.6 V) with parts at nominal: 5.9688 V. Nudge each of the five parts that reach VC (one run each), draw the line, jump to the 3σ board the line says is worst, simulate it with its nudges, and repeat:

   | Round | Line predicts | Simulation says |
   |---|---|---|
   | 1 (line from the range corner) | 6.1901 V | 6.2527 V |
   | 2 | 6.2734 V | 6.2802 V |
   | 3 | 6.2811 V | 6.2813 V ✓ |

   It stops when prediction and simulation agree to 1e-4 of the bound, and it never moves to a board that simulates worse. This is the "worst-case distance" idea from analog chip design [AGW94, Graeb07].
2. **Start B:** the worst corner, scaled down into the 3σ region (6.1899 V), searched the same way: 6.2733 → 6.2811 → **6.2813 V**. A second start always runs: a single start once missed a saturating corner (in the plan's `hiz` test circuit it found 8.66 where the truth was 5.48).
3. **Cross-check** at the other temperature/supply corners, with the same parts: 5.9607 V (−10 °C, 11.4 V), 5.5761 V (60 °C, 12.6 V), 5.2775 V (60 °C, 11.4 V). Cold and high supply stays the worst. Then temperature and supply are nudged inward at the 3σ point (2 runs): neither improves.
4. **Uniform spread**, because `worst_case` fails and the default passes: the same search with every part spread evenly over its tolerance gives **6.4657 V**. Still below 6.5 V, so the two spreads agree.

**Checked against keys:** an independent σ key (2,000 random directions per temperature/supply corner, then a pattern search) gives 6.281320 V; the engine gives 6.281314 V. A Monte Carlo of 20,000 boards at −10 °C, 12.6 V gives a maximum of 6.4038 V: no board fails.

The other four sides pass at `worst_case`, so their `sigma(3)` verdict is **PASS (implied by worst case)**: every 3σ board is inside the box. Their values are still searched and reported.

### 5.5 Step 4: the band (11 runs)

Each decisive board is re-run at a tolerance 1,000× tighter. The difference measures how exact each number is: 4.5–6.6e-9 V for VC and gain, 2.7e-5 Hz for f_L. Bias max is 0.0595 V past its bound, so the band decides nothing here. It's there for the edge case.

### 5.6 S1's verdicts

| Side | `sigma(3)` (the spec's confidence) | `worst_case` |
|---|---|---|
| VC ≤ 6.5 V | 6.2813 V, **PASS (estimated)**; uniform spread 6.4657 V | 6.5595 V, **FAIL** at −10 °C, 12.6 V, every part at its edge |
| VC ≥ 4.5 V | 4.8843 V, **PASS (implied by worst case)** | 4.6921 V, **PASS (all corners)** |

**The FAIL hangs on a model default.** A bare `Npn` gets a default model (the Appendix). Its XTB = 1.5 gives β its temperature coefficient. With XTB = 0 (ngspice's own default, β flat over temperature), bias max at `worst_case` is 6.4572 V: a PASS by 43 mV. The `sigma(3)` verdict is PASS either way (6.184 V). So the report always prints the model it used.

---

## 6. The two kinds of knobs in action (S3 bass, with aging)

S3 shows why range and statistical knobs must be combined differently. In the MVP, bass passes comfortably: 26.790 Hz at `worst_case`. But electrolytic capacitors dry out. Add the **life** knob (C_in × (1 − 0.2·life), a range knob; **not in the MVP**) and the story changes. Everything in this section was run on ngspice with that ninth knob: 2⁹ = 512 corners.

Nominal (a new board) f_L = 20.13 Hz; spec ≤ 30 Hz. The biggest effects are C_in's tolerance, C_in's aging, and β, which sets part of the input resistance. f_L is proportional to 1/C, so it curves: losing 20% of C raises f_L by 25%, not 20%.

| Knob goes to… | f_L moves: real | f_L moves: line (tangent at nominal) |
|---|---|---|
| C_in −20% (factory) | +5.03 Hz | +4.03 Hz |
| life = end (C_in loses 20%) | +5.03 Hz | +4.03 Hz |
| β = 100 | +0.77 Hz | +0.39 Hz |
| T = −10 °C | +0.16 Hz | +0.14 Hz |
| R2 −1% | +0.16 Hz | +0.16 Hz |

### Every method, side by side

| Method | f_L | Verdict | Right? |
|---|---|---|---|
| v1 "RSS": √(sum of squares) of **all** line moves, aging included | 25.8 Hz | PASS | ✗ treats aging as luck that averages out, but **every** board ages |
| v1 worst case from the line | 28.9 Hz | PASS | ✗ the line misses the 1/C curve |
| v2 realistic, but with the line at nominal | 28.3 Hz | PASS | ✗ right idea, slopes taken in the wrong place |
| **Engine `sigma(3)`**: worst range corner, then the 3σ-point search | **31.22 Hz** | **FAIL** | ✓ |
| Monte Carlo at the worst range corner (20,000 boards) | **1.1% fail** | FAIL | ✓ "how often" |
| **Engine `worst_case`**: all 512 corners simulated | 33.49 Hz | FAIL | ✓ |
| Day 1 only (the MVP's 8 knobs), `worst_case` | 26.79 Hz | PASS | ✓ |

### What the engine did

1. **Range knobs at their worst, always:** the σ search starts at the worst temperature/supply/life corner, cold (β drops, input resistance drops), 12.6 V and end of life (C_in −20%). With nominal parts that board gives **25.36 Hz**.
2. **Nudge the parts there.** At the aged corner C_in is smaller, and the 1/C curve is steeper there, so C_in's tolerance matters **more** than it did at the nominal. The new line predicts 30.12 Hz at the 3σ point.
3. **Simulate that board:** 31.217 Hz. One more round confirms it (predicted 31.2168, simulated 31.2169). **FAIL at `sigma(3)`.** Start B ends at 31.2166 Hz, and the independent σ key gives 31.216858 Hz.
4. **The 3σ board** is C_in 0.817 µF (−18%, 2.75σ low), β 181, and resistors less than 6% of the way to their edges. C_in carries it almost alone.
5. **`worst_case`:** 128 of the 512 corners fail. The worst is 33.487 Hz at −10 °C, 12.6 V, end of life, C_in −20%, β 100, R1 −1%, R2 −1%, RE −1%.

The whole aged check is 911 runs.

**A caution about "how often".** A Monte Carlo at that corner (a later feature, not in the MVP) finds 220 of 20,000 boards failing, about 1 in 91. That number rests on the assumed spread of the parts. Re-run with other realistic spreads:

| Assumed spread of the six parts | f_L failure rate |
|---|---|
| Our default (normal, σ = tol/3) | 1.1% |
| Uniform over the tolerance | 12.3% |
| C_in from an offset reel (centred at −12%, σ = 2%), others default | 5.8% |
| Parts culled from a wider batch (only 60–100% of the way to an edge) | 32.9% |

This is why, when `worst_case` fails and the default spread passes, `sigma(3)` repeats its search with a uniform spread and says UNDECIDED if the two disagree. (Here the default already fails; a uniform search gives 32.91 Hz.) The `worst_case` answer (33.49 Hz, FAIL) doesn't depend on any of this.

Notice step 1: aging is a **range** knob, so it's added at full strength and never averaged. That alone flips the verdict compared to v1's RSS. And the design **passes on day 1** (26.79 Hz at `worst_case`). Without the life knob you'd never see this failure; it only shows up years later, in the field.

### What the engineer sees

- **S3: FAIL at `sigma(3)`:** 31.22 Hz > 30 Hz at −10 °C, 12.6 V, end of life, C_in 0.817 µF. Worst example: C_in −20%, β 100 → 33.49 Hz.
- **Cause:** C_in (its ±20% tolerance plus its aging) is almost the entire spread.
- **Fixes, each a re-check:** C_in = 2.2 µF electrolytic → `worst_case` 15.22 Hz; C_in = 1 µF film (±5%, no aging) → 22.56 Hz.

### And S1 again

For S1 (VC max), `worst_case` **failed** (6.5595 V, section 5.2); `sigma(3)` passes (6.2813 V), and a 20,000-board Monte Carlo at the cold, high-supply corner finds no failing board. S1 fails only if **all six** parts land at their worst edge at once, in the cold, on a high supply. Whether that matters is the **engineer's decision** (a pacemaker: yes; a guitar pedal: no). The engine's job is to show both answers clearly.

---

## 7. Where exact math fits: the corner theorem, and what about transistors?

### 7.1 The corner idea, with the divider

The divider alone (R1, R2, VCC fixed at 12 V) is a purely **linear** circuit. Its output is `12 V × R2 / (R1 + R2)`. The tolerance box for two resistors is a rectangle with 4 corners:

```
 R2 +1% ●─────────────────●
        │                 │        each ● is a corner: both parts at an edge
        │     · · ·       │        · is any combination inside the box
        │   ·  nominal ·  │
 R2 −1% ●─────────────────●
      R1 −1%            R1 +1%
```

| R1 | R2 | Divider voltage |
|---|---|---|
| −1% | −1% | 2.10526 V |
| −1% | +1% | **2.14021 V** ← maximum |
| +1% | −1% | **2.07077 V** ← minimum |
| +1% | +1% | 2.10526 V |

**Why the extremes are at corners.** Raise R2 and the divider voltage always goes up, **no matter what R1 is**. Raise R1 and it always goes down. Neither part can make the output go up and then back down. So from any point inside the box, you can push R2 to its top edge and R1 to its bottom edge and only ever increase the output. You end at a corner, so the maximum is at a corner.

**Why this is true for linear circuits.** Pick one element in a linear circuit: a resistor, a source, or a controlled source (not a transformer turns ratio or a pot wiper, whose values appear in more than one place). Any node voltage, as a function of that element's value, has the shape

```
V = (a + b·value) / (c + d·value)        (a, b, c, d come from the rest of the circuit)
```

A curve of that shape never turns around. If you know Middlebrook's **Extra Element Theorem**, it's the same fact: any transfer function depends on one extra element in exactly this form. The math result is [PPC65] (1965, 3 pages), and the full theorem with its fine print is [N&P07] (Thm 5.1).

What corners buy us: **exact** answers, not estimates (inner = outer = true). A straight line through the nominal gives 2.0705 … 2.1400 here: close, but the corners give the exact 2.0708 … 2.1402.

### 7.2 Why it doesn't extend through the transistor

The transistor's current depends **exponentially** on VBE. It isn't a fixed element value that sits in the circuit. When you change R1, the transistor's operating point shifts, and with it the transistor's effective "values" (its gain and internal resistance). The neat `(a + b·value)/(c + d·value)` shape is gone, and nothing guarantees a quantity can't turn around inside the box.

### 7.3 So how does the engine handle transistor circuits (which is most circuits)?

1. **Simulate every corner, then look inside.** Up to about 12 knobs per spec side, this is the MVP's engine for every spec. It needs only the simulator, so it works for any circuit. Its FAILs are definite. Its PASS (all corners) rests on the corners plus the guards of section 5.3, not on a proof.
2. **Transistor circuits are usually "gently" nonlinear over a tolerance box.** In our amplifier, all five spec sides had their extremes at corners. That's why simulating corners works so well, but it's a pattern, not a proof. What breaks the gentleness: a transistor saturating or cutting off, a regulator dropping out, an op-amp hitting its rail, a resonance near the test frequency. The engine records each device's operating region at every simulated board.
3. **Exact math still earns its place.** Linear sub-blocks get exact answers (dividers, feedback networks, RC filters). Good designs push specs onto linear parts on purpose: our gain is ≈ RC/RE, and RC and RE cause **~78%** of its spread over the corners. That's what emitter degeneration is for. Guaranteed bounds for small transistor blocks are possible later (interval Newton methods [Neu89, Rum90]; closest to our engine: Lemke et al. 2002).

---

## 8. Derived measures, and where affine arithmetic fits

Suppose the engineer adds a spec on the top headroom, `VCC − VC` (room to swing up before hitting the supply). VCC appears **twice**: directly, and inside VC (which rises with VCC). Three ways to get its range:

| Method | Headroom range |
|---|---|
| Interval arithmetic: [11.4, 12.6] − [4.6921, 6.5595] | 4.8405 … 7.9079 V. Too wide: it pairs "VCC low" with "VC high", which can't happen together |
| Affine: the nominal line of VC, with the VCC knob shared, so it partly cancels | 5.2900 … 7.7036 V. Looks tight, but it is **not a bound**: the true minimum is lower |
| **MVP: evaluate `vcc.v − VC` on every run, from that run's own values** | **5.1951 … 7.6893 V**, exact over the corners |

Interval arithmetic has the "R1 − R1" problem (engine v2 §A2, in `archive/engine_v2.md`): it forgets that both VCCs are the same knob. The affine form fixes that because the knob has a **name**. But its line comes from slopes at the nominal, and its "± error" is only the miss someone happened to observe, not a proven remainder. So its minimum, 5.29 V, sits **above** the true 5.1951 V. A range that can miss the truth can't support a PASS.

**What the MVP does instead (decision D-F):** every measure, derived ones included, is computed per run from that run's values. `vcc.v` and VC come from the same simulated board, so a shared knob can't be counted twice. That was the property affine forms were for, and it comes for free.

**Where affine forms come back, after the MVP:** **hierarchy** (a block's characterized results stored as affine forms in its knobs, so the next level up reuses them without re-simulating, with shared knobs kept correlated); **calibration** and error budgets (exact subtraction, named contributions); **hand-written formulas** like datasheet arithmetic or `P = VCC × I_total`, with proven remainders from best-fit lines (section 10.3); and board yield from stored forms (M7). Only exact or proven remainders will ever support a PASS.

---

## 9. The whole check on one page

```
 spicy check ce_amp.spl            (you, or an agent)
  [0] nominal + read-back of every knob ................ 1 run    read-back differs? ─► UNDECIDED (binding)
  [1] every corner (2^8), one batch, every side ........ 256 runs  a corner violates? ─► FAIL + that board
  [2] 1% nudges at each worst corner + 8 audit boards .. 46 runs   something beats it? ─► ascent from there
  [3] sigma(3) per side: range knobs at their edges, two starts, re-linearize and
      simulate each predicted 3σ board until they agree (+ uniform spread if needed) .. 265 runs
  [4] band: the decisive boards re-run 1,000× tighter .. 11 runs
   └─► verdict words (section 4) + value + counterexample + flip table, stored per revision
```

The report for our amplifier:

| Spec side | Nominal | `sigma(3)` (the specs' confidence) | `worst_case` | Main causes |
|---|---|---|---|---|
| S1 VC ≤ 6.5 V | 5.503 | 6.2813 ✓ PASS (estimated); uniform 6.4657 | 6.5595 ✗ **FAIL** | temperature, β, VCC |
| S1 VC ≥ 4.5 V | 5.503 | 4.8843 ✓ PASS (implied by worst case) | 4.6921 ✓ PASS (all corners) | temperature, β |
| S2 gain ≤ 4.83 | 4.591 | 4.6635 ✓ PASS (implied by worst case) | 4.7028 ✓ PASS (all corners) | RC, RE (~78%) |
| S2 gain ≥ 4.37 | 4.591 | 4.5177 ✓ PASS (implied by worst case) | 4.4618 ✓ PASS (all corners) | RC, RE |
| S3 f_L ≤ 30 Hz | 20.13 | 24.973 ✓ PASS (implied by worst case) | 26.790 ✓ PASS (all corners) | C_in, β |
| *S3 with aging (post-MVP)* | *20.13* | *31.217 ✗ FAIL* | *33.487 ✗ FAIL* | *C_in tolerance + aging* |

The report's own summary line: "bias passes at sigma(3); it fails only with every part at its worst edge, cold, on a high supply. Your call."

**Cost.** 579 runs and 0.4 s of simulation: 1 + 256 + 46 + 265 + 11. Every `worst_case` value equals the brute-force answer key exactly, and every `sigma(3)` value is within 4.1e-5 of the independent σ key. Simulating every corner is affordable here because 256 is small. It doubles with every knob: at 12 knobs on one spec side it's 4,096 runs, about 3 s at 0.71 ms per run, and that's the MVP's budget. At 30 knobs it would be 2³⁰ ≈ a billion runs, about 9 days. Section 10 is what takes over there.

---

## 10. How it scales: the worst-point loop (after the MVP)

### 10.1 Why it exists

A spec side whose knobs don't fit the corner budget (about 12 knobs, counting only the knobs that can reach it) gets **UNDECIDED (budget)** in the MVP. The loop is what replaces enumeration there. Its cost grows with the number of knobs, not with 2 to that power. It comes as its own step after the MVP, and it is accepted only when it agrees with enumeration on every test circuit small enough to enumerate.

The CE amp is small enough to check the loop against the truth, so here it is on our circuit.

### 10.2 How it works

**Step 1: slopes.** A **sensitivity** is a slope: nudge one knob a little, see how much VC moves. On ngspice the engine nudges each knob by 1% of its half-range, one run per knob. (Our own simulator can later get all slopes for about the cost of **one** extra matrix solve, reusing the factorized matrix: the adjoint method [DR69, Xyce16].) Multiplied out to each knob's edge, they are **predictions**, not simulations: T moves VC by ±0.323 V, β by ±0.140 V, VCC by ±0.135 V, each resistor by 0.06–0.08 V, C_in not at all.

**Step 2: the straight line.** Put the slopes together:

```
VC ≈ 5.503 − 0.323·ε_T + 0.135·ε_VCC − 0.140·ε_β + 0.077·ε_R1 − 0.075·ε_R2 − 0.065·ε_RC + 0.061·ε_RE
```

Each ε is a knob position from −1 (its low edge) to +1 (its high edge). **This is the "affine form" from the v2 notes**: a straight-line estimate, with every term labeled by its knob. From the line, the worst case is easy: push each knob to whichever edge makes VC bigger.

```
VC max ≈ 5.503 + 0.323 + 0.140 + 0.135 + 0.077 + 0.075 + 0.065 + 0.061 ≈ 6.38 V
at: T = −10 °C, β = 100, VCC = 12.6 V, R1 +1%, R2 −1%, RC −1%, RE +1%
```

6.38 V < 6.5 V, so the line says **pass**. But we don't know how wrong the line is.

**Step 3: simulate the predicted worst board (1 run).** It gives **6.5595 V: FAIL.** The line was 0.18 V optimistic. It's the same corner enumeration found. This single extra simulation is the most important difference between engine v1 and v2: v1 trusted the line and reported a pass. The other direction: the line predicts VC min = 4.626 V, and the simulation at that corner gives 4.6921 V. There the line was 0.066 V pessimistic.

**Step 4: flip, and re-linearize.** At the predicted corner, flip each knob to its other edge (one real run each). If a flip makes VC worse (higher), move there and repeat. For bias max no flip improves (they're the flip table of section 5.2), so the corner stands. Off the corners, the loop re-draws its line at the predicted worst point and jumps again, exactly as the `sigma(3)` search does in section 5.4.

On the CE amp:

| Side | Line predicts | Simulated at the line's corner | After flips | Runs | Enumeration (256 runs) |
|---|---|---|---|---|---|
| VC max | 6.3801 | 6.5595 | 6.5595 | 16 | 6.5595 |
| VC min | 4.6264 | 4.6921 | 4.6921 | 16 | 4.6921 |
| gain max | 4.7055 | 4.7028 | 4.7028 | 18 | 4.7028 |
| gain min | 4.4761 | 4.4670 | **4.4618** (T flipped) | 25 | 4.4618 |
| f_L max | 24.8694 | 26.7897 | 26.7897 | 18 | 26.7897 |

Tens of runs per side instead of 2ⁿ. The price is that the loop's answers rest on its guards, not on having seen every corner. Its traps follow.

### 10.3 Trap 1: circuits aren't straight lines

Look at just one knob, β, with everything else nominal. Here are the real VC values next to the tangent line at β = 200:

| β | 100 | 150 | 200 | 250 | 300 |
|---|---|---|---|---|---|
| **Real VC** | 5.773 | 5.596 | 5.503 | 5.446 | 5.408 |
| **Straight line** (tangent at β = 200) | 5.644 | 5.574 | 5.503 | 5.433 | 5.362 |
| **Miss** | **0.129** | 0.022 | 0 | 0.014 | 0.046 |

The real curve **bends**. At low β the transistor draws more base current (IB = IC/β), which loads the divider more, so VC changes faster down there. The slope is the curve's steepness **at β = 200 only**. At β = 100 it's off by 0.13 V, more than the line's whole 0.12 V margin in step 2.

**Which straight line?** There's more than one way to draw a line through a curve:

| Line | How it's chosen | Worst miss over β = 100…300 |
|---|---|---|
| Tangent at β = 200 | Slope at the nominal point. What nudges or a simulator give us | 0.129 V |
| Best-fit ("Chebyshev") | The line whose worst miss is as small as possible | ±0.047 V |
| Tangent, measuring β in ×-steps (log β) | β = 100…300 is a factor of 3, so "per doubling" is more natural than "per unit" | 0.075 V |
| Two pieces in log β | Separate tangents for β = 100…173 and 173…300 | 0.014 V |

Take a line plus a band of ± its worst miss, and **the curve is inside the band**. That band is the "± err" part of an affine form. Three lessons: **choosing the line matters** (best-fit beats the tangent); **wide ranges hurt** (β varies by ×3); **log space and splitting help** a lot.

**How this connects to affine arithmetic.** When AA computes a *formula* like 1/x, it knows the formula, so it can draw the best-fit line over the whole range and compute the **exact** worst miss: a proven band. When the numbers come from a *simulation*, we only get the slope at the point we simulated, and we **don't know** the band until we check. That's why step 3 exists. (For the formula side: Stolfi's monograph [Stolfi97], §3.7 onward; the 2004 survey by the same authors is a gentler overview.)

### 10.4 Trap 2: the slope can point to the wrong corner

For the **gain minimum**, the slopes at the nominal say "hot is worse": the transistor's small internal emitter resistance (VT/IC) grows with temperature, which lowers the gain slightly. So the line picks T = 60 °C. But at the corner where β = 100, the slope for temperature **flips sign**. There, cold is worse, because β itself drops when cold, and at low β that effect wins.

| | Gain |
|---|---|
| Corner the nominal line picked (hot) | 4.4670 |
| True worst corner (cold) | 4.4618 |

It doesn't change the verdict here (the spec is 4.37), but it shows the direction at the nominal point is **not proof**. The flip in step 4 catches it: flipping T at the predicted corner gives the lower value, and the loop moves there. Papers show sign flips like this happen even in purely linear circuits [N&P07 Ex. 41, TS00]. Enumeration never has this problem: it simulates both.

### 10.5 Trap 3: a limit that's off at nominal has no slope

A transistor that saturates only at some corners, a regulator that drops out only at low supply: at the nominal the limiter is off, its slope is exactly zero, and the line never pushes toward it. "Prediction ≈ simulation" then agrees on the wrong, benign board. The plan's `hiz` test circuit (bigger resistors) has a spec gain ≥ 8.39. The loop of section 10.2 predicts 8.578 from the nominal line, simulates 8.504 at the line's corner, finds no better flip, and says **PASS**. Enumeration finds 62 of 256 corners saturated and a worst gain of 3.994: a **FAIL**. So before the loop ships it needs guards that enumeration doesn't: a margin for every device (VCE − VCE,sat for Q1) treated like a measure, flips at every vertex, and an allowance for the line's error that depends on the kind of measure (engine plan §9.3).

---

## 11. Every problem we hit, and its fix

| Problem | Where we saw it | Fix | Read |
|---|---|---|---|
| The line misses far from where it was drawn (curvature), worst over wide ranges | VC vs β (×3): 0.129 V; aged f_L vs C_in: +5.03 real vs +4.03 Hz line | Simulate every corner; the σ search simulates each point it predicts. For lines: log space, split ranges | [AGW94], [Graeb07], [Stolfi97] §3.7+ |
| Slope at nominal points to the wrong corner | Gain min: hot 4.4670 vs cold 4.4618 | Enumeration; in the loop, flips at every vertex | [N&P07] Ex. 41, [TS00] |
| A maximum inside the box | `pq`: 8.7097 mW inside vs 8.7069 mW at the worst corner | Tangent check + audit + pooling, then an ascent | engine plan §2.5 |
| A limiter off at nominal has zero slope | `hiz`: the plain loop says gain 8.504, PASS; 62 of 256 corners saturate, worst 3.994, FAIL | Enumeration sees it; the loop needs device-margin guards | `research/engine_method_redteam.md` |
| Range and statistical knobs mixed; aging ignored | Aged f_L: RSS says 25.8 Hz PASS, `sigma(3)` 31.22 Hz FAIL; day 1 passes (26.79 Hz) | Two kinds of knobs, combined differently; "life" as a range knob (post-MVP) | [ECSS11], [Graeb07], [Maxim 5527] |
| Failure rate depends on an unknown spread | Aged f_L: 1.1% default, 12.3% uniform, 32.9% culled | `sigma(3)` also under a uniform spread; UNDECIDED if they disagree | red-team report |
| Same knob counted twice; an observed line miss taken as a bound | Headroom: interval 4.8405 V, affine 5.29 V, true 5.1951 V | Derived measures evaluated per run (D-F); only proven remainders may support a PASS | [Stolfi97]; round 2 synthesis C8 |
| A model default flips a verdict | XTB 1.5 vs 0: bias max 6.5595 V FAIL vs 6.4572 V PASS | Write the default model out; print it in every report (D-A) | engine plan §2.1 |
| A knob that silently doesn't move | A netlist without `.temp`: a false PASS on bias max | Read back every knob on every run; stop after 1 run | engine plan §4.4 |
| Simulator tolerances | Default tolerances: 9.5e-5 V error over the 256 corners | Engine tolerances (6e-13 V) and a measured band | engine plan §4.5; [Xyce16] |
| Monte Carlo at one spec's worst corner isn't **board** yield | A board must pass every spec at every condition | Check each sample over conditions and all specs (statistics, M7) | Schenkel et al. DAC 2001 |

---

## 12. What to read, in order

You don't need all of these; this document is meant to stand on its own. If you want to go deeper:

1. **`engine_plan.md`**, the engine MVP plan this walkthrough follows: §2 is section 5 in full detail, §3 the verdict tables.
2. **[ECSS11] ECSS-Q-HB-30-01A, *Worst case analysis*** (free PDF). Written for engineers. It separates random tolerances from "biased" effects (drift, temperature, aging), close to our range/statistical split, and describes extreme value, RSS and Monte Carlo methods. The best match for sections 3 and 6.
3. **[Graeb07] Graeb, *Analog Design Centering and Sizing*,** the worst-case analysis chapters. The theory behind the `sigma(3)` search and the loop: range vs statistical parameters, worst-case points, and re-linearizing there. Heavier, but it's the backbone of sections 5.4 and 10.
4. **[Stolfi97] de Figueiredo & Stolfi,** the monograph, §3.7 onward, for how affine arithmetic chooses lines for formulas (section 10.3). Their 2004 survey is a shorter overview.
5. **For the corner theorem:** Middlebrook's Extra Element Theorem (the intuition you may already have as an EE), then [N&P07] §5 if you want the proof and the counterexamples. Optional.

Later, when our simulator becomes a second backend: **[Xyce16]** (how a real simulator computes sensitivities).

---

## Appendix: the model behind the numbers

- **Simulator:** ngspice-42, compiled with KLU, run in batch mode by stdlib-Python scripts (scratch, kept out of the repo, next to the plan's in `…/engine2/walkthrough/`). The engine will drive libngspice instead; the plan measured the two to agree to 1.9e-15.
- **Transistor:** ngspice's Gummel–Poon model with the plan's default card for a bare `Npn` (decision D-A): `.model QM_q1 NPN(IS=1e-14 BF={beta} XTB=1.5 XTI=3 EG=1.11)`, TNOM 25 °C written out (`.options tnom=25`; ngspice's own default is 27 °C). IS = 1e-14 puts VBE at 0.659 V at the nominal 1.38 mA. XTB = 1.5 gives β ∝ T^1.5 (about +0.5%/°C); XTI and EG give IS its temperature dependence, which moves VBE. Nothing else is set: no Early effect, no junction capacitances, no base/collector/emitter resistances, no high-current roll-off. So hFE equals BF at 25 °C, and no corner leaves the normal region (VCE ≥ 3.289 V at every corner).
- **Temperature** is the circuit temperature (`.temp`), set per run from the knob. Resistor tempco and drift, and C_in's tempco, are left out.
- **Default bench:** a DC source on `vcc` at its knob, an `AC 1` source on `input` (ideal, no source resistance), the output unloaded.
- **Engine tolerances:** `reltol=1e-6 vntol=1e-9 abstol=1e-15`. The band re-runs use 1e-9, 1e-12, 1e-18.
- **Measures**, computed by our own code from ngspice's vectors: VC = `v(output)` from the operating point; gain = |v(output)/v(input)| at exactly 1 kHz (`ac lin 1 1000 1000`); f_L = the literal −3 dB point below the peak of |H| over 0.1 Hz–100 kHz, 50 points per decade, interpolated as a cubic in (ln f, dB): 20.127 Hz at nominal, not the textbook 1/(2π·C_in·R_in) = 20.08 Hz. Input resistance (section 1) = |v(base)/i(C_in)| at 1 kHz.
- **Knobs:** as in section 3. The aged variant (section 6 only) adds `life` from 0 (new) to 1 (end of life), with C_in's value written as `{c_in*(1-0.2*life)}`: 9 knobs, 512 corners. Section 6's "nominal" is a new board.
- **Statistics:** each statistical knob is normal, with its edges at ±3σ, truncated at the edges, mapped exactly between standard-normal units and the part's value. β is normal(200, 33) truncated to 100…300; datasheets only give min/max, so the distribution is an assumption. It's exactly the kind the engine must show the user, which is why the uniform spread is the check.
- **Answer keys.** `worst_case`: all corners, 1,000 seeded random boards inside the box, and a coordinate search from each side's best point (MVP and aged variant); every extreme sat at a corner. `sigma(3)`: at each temperature/supply corner (at end of life, for the aged variant), 2,000 random directions on the 3σ sphere, then a pattern search; aged f_L gives 31.216858 Hz against the engine's 31.216857 Hz. Monte Carlo: 20,000 seeded boards at the side's worst range corner.
- **Slopes** come from nudges of 1% of a knob's half-range (the β tangent in section 10.3 from a central difference). The loop of section 10 was run for this document as a demonstration; it isn't part of the MVP.
