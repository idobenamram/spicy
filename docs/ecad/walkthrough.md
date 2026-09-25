# Engine Walkthrough: One Circuit, End to End

> Companion to `engine.md` · 2026-09-25
> Paper keys like **[AGW94]** point into `bibliography.md`.

This follows one small circuit from "the engineer picked some values" to "the engine gives a verdict". Every number here was computed from a simple model of the circuit (the Appendix lists what the model includes and leaves out).

Because the circuit is tiny (9 things that can vary, so 2⁹ = 512 extreme combinations), we can also **brute-force the true answers**. That's our answer key for checking how good each method is. The real engine can't afford brute force on real circuits, which is exactly why the loop exists.

---

## 1. The circuit and what the engineer typed

A common-emitter amplifier, the textbook one-transistor stage:

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
| R1, R2 | **Divider**: holds the base at about 2.1 V, which sets the transistor's current |
| RE | Sets the current (≈ (2.1 V − 0.65 V) / 1k) and makes the gain depend on resistors, not the transistor |
| RC | Turns the current into the output voltage; with RE it sets gain ≈ RC/RE |
| C_in | **Coupling capacitor**: passes the audio signal, blocks DC. With the input resistance it makes a high-pass filter |
| Q1 | **The transistor**: does the amplifying |

The engineer runs one simulation at the nominal values:

| Quantity | Nominal |
|---|---|
| Collector current IC | 1.38 mA |
| Collector voltage VC | 5.50 V |
| Gain at 1 kHz | 4.59 |
| Input resistance | 7.93 kΩ |
| Low cutoff frequency f_L | 20.1 Hz |

And writes three specs:

| Spec | Requirement | Why | Nominal |
|---|---|---|---|
| **S1 bias** | 4.5 V ≤ VC ≤ 6.5 V | Room for the output to swing both ways without clipping | 5.50 ✅ |
| **S2 gain** | 4.6 ± 5% (4.37 … 4.83) | The next stage expects this level | 4.59 ✅ |
| **S3 bass** | f_L ≤ 30 Hz | Don't cut the bass | 20.1 ✅ |

**Everything passes at nominal.** This is where most engineers stop today. The engine's job is the next question: *does it still pass on every board we build, at every temperature, with a sagging supply, and after years of use?*

---

## 2. When the engine runs

Like a type checker, in the background:

- **While editing:** each value change re-evaluates the checks from the last analysis's saved results (cheap straight-line estimates, section 5.3). Instant feedback.
- **In the background / on demand:** the full loop (section 5). A handful of simulations per spec.
- **Before sign-off:** a Monte Carlo confirmation (section 6).

What the engineer sees at the end is one row per spec: a verdict, a range, which parts use up the margin, and a concrete failing example if there is one (section 9).

---

## 3. Knobs: describing everything that can vary

A **knob** is one independent thing that can vary. Each knob has a range. There are two kinds, and the difference drives everything that follows.

### The two kinds

| | **Range knobs** | **Statistical knobs** |
|---|---|---|
| What | Operating conditions and slow changes | Manufacturing spread of each part |
| Examples here | temperature, supply voltage, aging | R1, R2, RC, RE, C_in, β |
| Who "chooses" the value | The world. **Every** board sees every temperature and eventually gets old | The factory. Each board rolls the dice **once** |
| The question | Must work at **every** value | What **fraction** of boards work? (yield) |
| How we treat it | Always take the worst value | Either take the worst (very conservative) or ask how rare failure is |

Why the difference matters: a board **will** see −10 °C if it's used outdoors in winter. But a board where all six parts landed at their worst tolerance edge at the same time is incredibly rare. Treating those two the same way is either reckless (averaging over temperature) or wasteful (assuming all parts are simultaneously worst).

### Our 9 knobs

| Knob | Kind | Range | Where the number comes from |
|---|---|---|---|
| T | range | −10 … 60 °C | Product requirement |
| VCC | range | 12 V ± 5% (11.4 … 12.6 V) | Regulator spec over load and line |
| life | range | new … end of life | Product lifetime (10 years) |
| R1, R2, RC, RE | statistical | ±1% each | Resistor datasheet |
| C_in | statistical | ±20% | Electrolytic capacitor datasheet |
| β | statistical | 100 … 300 | Transistor datasheet (min/max hFE) |

### One part can depend on several knobs

Real part values are driven by their own statistical knob **and** shared range knobs:

```
C_in actual = 1 µF  × (1 ± 20%)            ← its own statistical knob (factory)
                    × (1 − up to 20%)       ← shared "life" knob (electrolytics dry out)

β actual    = β_part × (1 + 0.5%/°C × (T − 25 °C))
              └ statistical (100…300)   └ shared temperature knob

VBE         = 0.65 V … − 2 mV/°C × (T − 25 °C)   ← temperature knob
```

So the temperature knob moves **both** β and VBE, and it's the **same** knob for both. That's how correlation is represented: things driven by one knob move together.

### How the engineer sets them

Mostly they don't have to:

- **Range knobs** are declared once per project or block: "operating temperature −10 … 60 °C", "supply 12 V ± 5%", "lifetime 10 years".
- **Statistical tolerances** come with each part: typed in, or pulled from the datasheet or part library.
- **Links** between parts and range knobs (tempco, aging, DC-bias curves) come from part models. That's where AI-assisted datasheet extraction fits.

Something like:

```
environment:  temp −10°C..60°C,  vcc 12V ±5%,  life 10 years
R1  47k ±1%                    (statistical)
C_in  1µF ±20%, aging −20% @ end of life   (statistical + linked to life)
Q1  β 100..300, tempco from model           (statistical + linked to temp)
```

### Two questions the engineer can ask

- **Absolute worst case:** every knob, including every part, at its worst edge. The most conservative answer. Used for safety-critical specs.
- **Realistic:** range knobs at their worst, statistical knobs treated statistically. The answer is a failure rate: "at −10 °C after 10 years, 1 board in 110 fails". Used for most specs, with a target like "99.9% of boards pass".

The engine computes both. Section 6 shows them giving different answers.

---

## 4. What the engine produces: inner, outer, and the true range

For each spec the engine wants the **true range**: every value the quantity can actually take, over all knob settings. For our circuit we know it only because we brute-forced it. In general you can't compute it: it's the extreme over a 9-dimensional box (or 50-dimensional, for a real circuit), and each point costs a simulation.

So the engine computes two things that squeeze the true range from opposite sides.

**An analogy.** You want to know the height of the tallest person in a country (the true maximum), to decide whether a 2.5 m door is tall enough.

- **Inner bound:** the tallest person you've actually **measured**, say 2.1 m. The true maximum is at least 2.1 m. You can't claim anything lower.
- **Outer bound:** a proof that **nobody** is taller than, say, 3 m. The true maximum is at most 3 m.
- The truth is somewhere between 2.1 and 3 m. The door (2.5 m) sits in that gap, so **you don't know yet.**
- If you measure someone at 2.6 m, the door definitely **fails**. You have a concrete example.
- If you prove nobody exceeds 2.4 m, the door definitely **passes**.

That's the whole idea:

> **Inner bounds prove failures. Outer bounds prove passes. The gap between them is how unsure we are.**

In the circuit:

- **Inner** comes from actually evaluating concrete knob settings, i.e. simulating specific "boards". Every simulated point is a combination that can really happen, so its value really happens.
- **Outer** comes from math that covers the whole box at once. For pieces of the circuit that are purely linear it can be exact (section 7). For anything through the transistor, we usually only have an **estimated** outer bound.

Three examples from our circuit, which show why all three matter:

| Quantity | Inner | Outer | True | Verdict |
|---|---|---|---|---|
| Divider voltage (VCC fixed at 12 V) | 2.0708 … 2.1402 | 2.0708 … 2.1402 | 2.0708 … 2.1402 | **exact** (section 7) |
| VC max vs 6.5 V, after simulating the predicted worst board | 6.59 | — | 6.59 | **FAIL, proven**: we have a board that hits 6.59 V |
| VC min vs 4.5 V | 4.66 | ≈ 4.59 (estimated) | 4.66 | **PASS, estimated** |

The possible verdicts:

| Verdict | What we know |
|---|---|
| **PASS (guaranteed)** | A proven outer bound is inside the spec. Only possible for linear pieces or with special math |
| **PASS (estimated)** | The estimated outer bound is inside the spec, and the estimate was checked by simulating the worst point |
| **FAIL** | A simulated, reachable point violates the spec. Always definite, always comes with the knob settings that cause it |
| **UNDECIDED** | The spec edge is inside the gap. The engine works harder (simulates more points) |

---

## 5. The loop, step by step (S1: collector voltage)

### 5.1 Step 1: nominal simulation (1 run)

VC = 5.50 V. The simulator's work (the factorized circuit matrix) is kept for the next step.

### 5.2 Step 2: sensitivities, meaning "how much does VC move per knob?"

A **sensitivity** is a slope: nudge one knob a tiny bit, see how much VC moves. Multiply by the knob's range and you get "how much VC moves if this knob goes to its edge".

The simulator can get **all** the slopes for about the cost of **one** extra matrix solve, not one simulation per knob. It already has the circuit's matrix factorized, and asking "how does VC respond to every part" is one more solve with that same factorization. That's the adjoint method [DR69]. Real simulators do it this way, and [Xyce16] describes how.

The engine now has this table. These are **predictions from the slopes**, not simulations:

| Knob | Knob goes to… | VC moves (predicted) |
|---|---|---|
| T | −10 °C / 60 °C | +0.354 / −0.354 V |
| β | 100 / 300 | +0.141 / −0.141 V |
| VCC | 11.4 / 12.6 V | −0.135 / +0.135 V |
| R1 | −1% / +1% | −0.077 / +0.077 V |
| R2 | −1% / +1% | +0.075 / −0.075 V |
| RC | −1% / +1% | +0.065 / −0.065 V |
| RE | −1% / +1% | −0.061 / +0.061 V |
| C_in, life | any | 0 (they don't affect DC) |

Already useful: **temperature is the biggest effect** (VBE drops 2 mV/°C, so the current rises when hot). That's a range knob, so better resistors won't help with it.

### 5.3 Step 3: the straight line

Put the slopes together and you get a straight-line estimate of VC for any knob setting:

```
VC ≈ 5.500 − 0.354·ε_T + 0.135·ε_VCC − 0.141·ε_β + 0.077·ε_R1 − 0.075·ε_R2 − 0.065·ε_RC + 0.061·ε_RE
```

Each ε is a knob position from −1 (its low edge) to +1 (its high edge). **This is the "affine form" from the v2 notes.** It's nothing more than this straight-line estimate, written so that every term is labeled with the knob it belongs to.

From the line, the worst case is easy: push each knob to whichever edge makes VC bigger.

```
VC max ≈ 5.500 + 0.354 + 0.141 + 0.135 + 0.077 + 0.075 + 0.065 + 0.061 = 6.408 V
at: T = −10 °C, β = 100, VCC = 12.6 V, R1 +1%, R2 −1%, RC −1%, RE +1%
```

6.408 V < 6.5 V, so the line says **pass**. The margin is only 0.09 V, though, and we don't know how wrong the line is. Honest status: **UNDECIDED**.

### 5.4 The problem: circuits aren't straight lines

What does "fitting a line" actually mean? Look at just one knob, β, with everything else nominal. Here are the real VC values (simulated) next to the straight line from step 3:

| β | 100 | 150 | 200 | 250 | 300 |
|---|---|---|---|---|---|
| **Real VC** | 5.770 | 5.593 | 5.500 | 5.443 | 5.404 |
| **Straight line** (tangent at β = 200) | 5.641 | 5.571 | 5.500 | 5.429 | 5.359 |
| **Miss** | **0.129** | 0.022 | 0 | 0.014 | 0.045 |

The real curve **bends**. At low β the transistor draws more base current (IB = IC/β), which loads the divider more, so VC changes faster down there. The slope from step 2 is the curve's steepness **at β = 200 only** (the tangent). It's perfect at 200 and gets worse the further you go. At β = 100 it's off by 0.13 V, more than our whole 0.09 V margin.

**Which straight line?** There's more than one way to draw a line through a curve:

| Line | How it's chosen | Worst miss over β = 100…300 |
|---|---|---|
| Tangent at β = 200 | Slope at the nominal point. What a simulator gives us | 0.129 V |
| Best-fit ("Chebyshev") | The line whose worst miss is as small as possible | ±0.047 V |
| Tangent, measuring β in ×-steps (log β) | β = 100…300 is a factor of 3, so "per doubling" is more natural than "per unit" | 0.075 V |
| Two pieces in log β | Separate lines for β = 100…173 and 173…300 | 0.014 V |

Take a line plus a band of ± its worst miss, and **the curve is guaranteed to be inside the band**. That band is the "± err" part of an affine form. This table has the three ideas the v2 notes kept coming back to:

1. **Choosing the line matters.** The best-fit line misses by less than the tangent.
2. **Wide ranges hurt.** β varies by ×3, and the curve bends a lot over that range.
3. **Log space and splitting help.** Measuring in ×-steps, and using several shorter lines, shrink the miss a lot.

**How this connects to affine arithmetic.** When AA computes a *formula* like 1/x, it knows the formula, so it can draw the best-fit line over the whole range and compute the **exact** worst miss. The band is guaranteed. That's what "AA fits a line" meant in v2. When the numbers come from a *simulation*, we only get the tangent at the point we simulated, and we **don't know** the band until we check. That's the next step.

(If you want the formula side in depth: Stolfi's monograph [Stolfi97], §3.7 onward, covers how to choose the line for each operation. The 2004 survey by the same authors is a gentler overview.)

### 5.5 Step 4: check the line where it matters (1 run)

The line predicted a worst board. **Simulate exactly that board:**

```
T = −10 °C, β = 100, VCC = 12.6 V, R1 +1%, R2 −1%, RC −1%, RE +1%
line predicted:  6.408 V   (pass)
simulation says: 6.590 V   (FAIL)
```

**The spec fails**, and we have the exact board that fails it. The line was 0.18 V optimistic, mostly because of the β curve above. The brute-force answer key agrees: out of all 512 corners, this one is the worst, at 6.590 V.

This single extra simulation is the most important difference between v1 and v2. v1 would have trusted the line and reported a pass.

It's also why the inner bound matters: this one simulation turned UNDECIDED into a **proven FAIL**, with a counterexample the engineer (or the AI) can look at.

The other direction: the line predicted VC min = 4.59 V at the opposite corner, and simulating that corner gives **4.66 V**. Here the line was 0.07 V *pessimistic*. The spec is 4.5 V, so the margin is 0.16 V, comfortably more than the 0.07 V we now know the line can be off by. Verdict: **PASS (estimated)**.

### 5.6 Step 5: re-linearize and repeat (when needed)

When the worst point isn't a plain corner (the statistical case, section 6), the engine:

1. Draws a new line (new slopes) **at the predicted worst point**, not at the nominal.
2. Predicts the worst point again from that new line.
3. Simulates it, and repeats until prediction and simulation agree.

For S1's realistic check (next section) it goes:

| Round | Line predicts | Simulation says |
|---|---|---|
| 1 (line from the range corner) | 6.225 V | 6.297 V |
| 2 | 6.328 V | 6.341 V |
| 3 | 6.343 V | 6.343 V ✓ |

Three rounds, three simulations. This is the "worst-case distance" method from analog chip design [AGW94, Graeb07].

### 5.7 A trap: the slope can point to the wrong corner

For the **gain minimum**, the slopes at the nominal say "hot is worse": the transistor's small internal emitter resistance (VT/IC) grows by about 0.2%/°C, which lowers the gain slightly. So the line picks T = 60 °C. But at the corner where β = 100, the slope for temperature **flips sign**. There, cold is worse, because β itself drops when cold, and at low β that effect wins.

| | Gain |
|---|---|
| Corner the nominal line picked (hot) | 4.4688 |
| True worst corner (cold) | 4.4626 |

It doesn't change the verdict here (the spec is 4.37), but it shows the direction at the nominal point is **not proof**. The fix is automatic in the loop: re-linearizing at the predicted corner reveals the flipped slope, and the engine flips that knob and checks again. Papers show this also happens in purely linear circuits [N&P07 Ex. 41, TS00].

---

## 6. The two kinds of knobs in action (S3: bass cutoff)

S3 shows why range and statistical knobs must be combined differently. Nominal f_L = 20.1 Hz; spec ≤ 30 Hz.

The biggest effects are C_in's tolerance, C_in's aging, and β, which sets part of the input resistance. f_L is proportional to 1/C, so it curves too: losing 20% of C raises f_L by 25%, not 20%.

| Knob goes to… | f_L moves: real | f_L moves: line |
|---|---|---|
| C_in −20% (factory) | +5.02 Hz | +4.02 Hz |
| life = end (C_in loses 20%) | +5.02 Hz | +4.02 Hz |
| β = 100 | +0.77 Hz | +0.39 Hz |
| T = −10 °C | +0.17 Hz | +0.14 Hz |
| R2 −1% | +0.16 Hz | +0.16 Hz |

### Every method, side by side

| Method | f_L | Verdict | Right? |
|---|---|---|---|
| v1 "RSS": √(sum of squares) of **all** moves, aging included | 25.8 Hz | PASS | ✗ treats aging as luck that averages out, but **every** board ages |
| v1 worst case from the line | 28.8 Hz | PASS | ✗ the line misses the 1/C curve |
| v2 realistic, but with the line at nominal | 28.3 Hz | PASS | ✗ right idea, slopes taken in the wrong place |
| **v2 loop**: worst range corner, re-linearize, simulate the 3σ point | **31.7 Hz** | **FAIL** | ✓ |
| Monte Carlo at the worst range corner (20,000 boards) | **0.9% fail** | **FAIL** | ✓ the answer key |
| Absolute worst (all knobs at worst edge, simulated) | 33.4 Hz | FAIL | ✓ |
| Day 1 only (no aging), absolute worst | 26.7 Hz | PASS | ✓ |

### What the v2 loop did

1. **Range knobs first, always at their worst:** cold (β drops, input resistance drops) and end of life (C_in −20%). Simulating that corner with nominal parts: **25.3 Hz**.
2. **Re-linearize there.** At the aged corner C_in is smaller, and the 1/C curve is steeper there, so C_in's tolerance matters **more** than it did at the nominal. The new line predicts 30.4 Hz at the 3σ point.
3. **Simulate that point:** 31.7 Hz. One more round confirms 31.7 Hz. **FAIL at 3σ.**
4. **Monte Carlo** at the cold, end-of-life corner: 177 of 20,000 boards fail, **about 1 in 110**.

**A caution about that 0.9%.** It rests on the assumed spread of the statistical parts: normal, with the tolerance edges at 3σ. The red-team agent re-ran the same spec under other realistic assumptions:

| Assumed spread | f_L failure rate |
|---|---|
| Our default (normal, σ = tol/3) | 0.9% |
| Uniform over the tolerance | 11.6% |
| An offset reel | 5% |
| Parts culled from a wider batch | 32% |

So the engine reports yield under the default **and** under a uniform spread, and says UNDECIDED if the two verdicts disagree. The absolute-worst-case answer (33.4 Hz, FAIL) doesn't depend on any of this.

Notice step 1: aging is a **range** knob, so it's added at full strength and never averaged. That alone flips the verdict compared to v1's RSS. And the design **passes on day 1** (26.7 Hz absolute worst). Without the life knob you'd never see this failure; it only shows up years later, in the field.

### What the engineer sees

- **S3: FAIL.** At −10 °C after 10 years, about 0.9% of boards have f_L above 30 Hz.
- **Worst example:** −10 °C, end of life, C_in −20%, β = 100 → 33.4 Hz.
- **Cause:** C_in (its ±20% tolerance plus its aging) is almost the entire spread.
- **Fixes the engine can check instantly:** f_L is proportional to 1/C, so
  - C_in = 2.2 µF electrolytic → worst case 15.2 Hz;
  - C_in = 1 µF film (±5%, no aging) → worst case 22.5 Hz.

### And S1 again, realistic

For S1 (VC max), the absolute worst case **failed** (6.59 V, section 5.5). The realistic check:

- Range corner (−10 °C, 12.6 V) → the loop's 3σ worst point → **6.34 V < 6.5 V**.
- Monte Carlo at that corner: **0 of 20,000** boards fail.

So S1 fails only if **all six** parts land at their worst edge at once, in the cold, on a high supply. Whether that matters is the **engineer's decision** (a pacemaker: yes; a guitar pedal: no). The engine's job is to show both answers clearly.

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

What corners buy us:

- **Exact** answers, not estimates: inner = outer = true. (The straight line from section 5.3 gives 2.0705 … 2.1400 here; close, but the corners give the exact 2.0708 … 2.1402.)
- **Cheap:** the loop's "go to the predicted corner" step is exact for linear pieces, provided the direction of each part is proven over the whole box (section 5.7's trap).

### 7.2 Why it doesn't extend through the transistor

The transistor's current depends **exponentially** on VBE. It isn't a fixed element value that sits in the circuit. When you change R1, the transistor's operating point shifts, and with it the transistor's effective "values" (its gain and internal resistance). The neat `(a + b·value)/(c + d·value)` shape is gone, and nothing guarantees a quantity can't turn around inside the box.

### 7.3 So how does the engine handle transistor circuits (which is most circuits)?

1. **The loop (section 5) is the main engine for anything that goes through a transistor**, which is most specs. It only needs the simulator, so it works for any circuit. Its failures are always definite; its passes are estimates that were checked at the worst point.

2. **Transistor circuits are usually "gently" nonlinear over a tolerance box.** In our amplifier, for all five checks (VC max/min, gain max/min, f_L), the extremes were at corners. 20,000 random points inside the box never beat the best corner. So "the worst case is at a corner" is a very good **guess** for transistor circuits, and the loop uses it. But it's not a proof, which is why the loop **simulates** the predicted corner instead of trusting it. What breaks the gentleness:
   - a transistor saturating or cutting off,
   - a regulator dropping out,
   - an op-amp hitting its rail.

   The engine checks for these at every simulated point and flags them.

3. **Exact math still earns its place:**
   - Linear sub-blocks get exact answers: dividers, feedback networks, RC filters, reference dividers.
   - Good designs push specs onto linear parts on purpose. Our gain is ≈ RC/RE, and RC and RE together cause **~80%** of its worst-case spread; the transistor barely matters. That's what emitter degeneration is for.
   - Combining results (next section).
   - Serving as the answer key for testing the loop.
   - Guaranteed bounds for small transistor blocks are possible (interval Newton methods [Neu89, Rum90]; closest to our engine: Lemke et al. 2002). That's a later, optional upgrade.

---

## 8. Where affine arithmetic fits

With the simulator doing most of the work, what is AA for?

**1. It's the format for the line.** The formula in section 5.3 *is* an affine form. Every method (simulation, corners, hand formulas) produces a line in the **same named knobs**.

**2. Combining results without double-counting.** Suppose the engineer adds a spec on the top headroom, `VCC − VC` (room to swing up before hitting the supply). VCC appears **twice**: directly, and inside VC (which rises with VCC).

| Method | Headroom range |
|---|---|
| Interval arithmetic: [11.4, 12.6] − [4.66, 6.59] | 4.81 … 7.94 V (pairs "VCC low" with "VC high", which can't happen together) |
| Affine: both terms share the VCC knob, so it partly cancels | 5.26 … 7.74 V |
| Loop checks the corner | **5.17 … 7.72 V** (true) |

That's the "R1 − R1" problem (engine v2 §A2, in `archive/engine_v2.md`), showing up in a real spec. Affine forms avoid it because the knob has a **name**.

**3. Hand-written formulas** (power budgets, derived values like `P = VCC × I_total`) get computed with a guaranteed band, using best-fit lines (section 5.4).

**4. Hierarchy.** A block's outputs are stored as affine forms, so the next level up can reuse them without re-simulating.

---

## 9. The whole loop on one page

```
 engineer edits a value
        │
        ▼
 [1] nominal simulation ───────────────────────────► nominal values
        │
 [2] sensitivities (≈ 1 extra solve per metric)
        │
 [3] the line: VC ≈ 5.50 − 0.354·ε_T + …   (affine form, shared named knobs)
        │
        ├── purely linear piece? ──► corners ──► EXACT (inner = outer)
        │
 [4] range knobs → worst corner (from the line)
        │
 [5] SIMULATE that point ─────► real value (inner bound)
        │                           └── violates spec? ──► FAIL + counterexample
        │
 [6] re-linearize there; statistical knobs → 3σ worst point;
     simulate; repeat until line and simulation agree (2–4 rounds)
        │
 [7] verdict + range + top contributors + failure rate estimate
        │
 [8] before sign-off: Monte Carlo at the worst range corner → "k of N fail"
```

The report for our amplifier:

| Spec | Nominal | Absolute worst | Realistic (worst conditions, 3σ) | Verdict | Main causes |
|---|---|---|---|---|---|
| S1 VC ≤ 6.5 V | 5.50 | 6.59 ✗ | 6.34 ✓ (0 / 20,000 fail) | **Engineer's call**: fails only with all parts at their edges | temperature, β |
| S1 VC ≥ 4.5 V | 5.50 | 4.66 ✓ | ✓ | PASS (estimated) | temperature, β |
| S2 gain 4.6 ± 5% | 4.59 | 4.46 … 4.70 ✓ | 4.52 … 4.67 ✓ | PASS (estimated) | RC, RE (~80%) |
| S3 f_L ≤ 30 Hz | 20.1 | 33.4 ✗ | 31.7 ✗ (0.9% fail) | **FAIL** | C_in tolerance + aging |

**Cost.** Brute force here is 512 simulations. With 30 knobs it would be 2³⁰ ≈ a billion.

When the simulator provides adjoint sensitivities (DC and AC), the loop needs roughly 5–8 simulations per spec almost regardless of knob count. With nudged slopes, each round costs one extra simulation per knob. Transient needs one backward pass per metric. There's no convergence guarantee; Cadence reports "under 100 simulations for each spec" for its commercial version. Monte Carlo (thousands of runs) happens only once, at the end.

---

## 10. Every problem we hit, and its fix

| Problem | Where we saw it | Fix | Read |
|---|---|---|---|
| The line misses far from where it was drawn (curvature) | VC vs β: 0.13 V; f_L vs C_in | Simulate the predicted worst point; re-linearize there | [AGW94], [Graeb07] worst-case chapters |
| Wide knob ranges bend the line a lot | β = 100…300 (×3) | Measure in log space; split the range | [Stolfi97] §3.7+; Boyd et al. GP tutorial (log space) |
| Slope at nominal points to the wrong corner | Gain min: hot vs cold | Re-check slopes at the corner; flip and re-simulate | [N&P07] Ex. 41, [TS00] |
| Range and statistical knobs mixed | f_L: RSS says pass, 0.9% really fail | Two kinds of knobs, combined differently | [ECSS11], [Graeb07] |
| Aging ignored | f_L passes on day 1, fails later | "life" as a range knob; aging data from datasheets | [ECSS11], [Maxim 5527] |
| Same knob counted twice | Headroom: 4.81 vs true 5.17 V | Affine forms with named, shared knobs | [Stolfi97] |
| Transistor changes region | Not in this design | Detect at every simulated point and flag | — |
| Simulator tolerances: abstol applied to currents; fixed-step transient | (engine prerequisite) | Separate voltage/current tolerances; step control; analytic sensitivities | [Xyce16] |
| A limiter that's **off** at nominal (clipping, dropout, current limit) has zero slope, so the loop never pushes toward it. "Prediction ≈ simulation" then wrongly looks like proof | A THD spec: the loop says 0.0026% PASS, the true worst corner is 3.29% FAIL (`research/engine_method_redteam.md`) | Always also simulate every range-knob corner (one run serves every spec); scan temperature; multi-start; log each device's operating region | red-team report |
| Monte Carlo at one spec's worst corner isn't **board** yield: a board must pass every spec at every condition | One test: 26.7% loss reported, 36.8% true | Check each sample over range corners and all specs; board yield from stored per-spec lines | Schenkel et al. DAC 2001 |

---

## 11. What to read, in order

You don't need all of these; this document is meant to stand on its own. If you want to go deeper:

1. **[ECSS11] ECSS-Q-HB-30-01A, *Worst case analysis*** (free PDF). Written for engineers.
   - It separates random tolerances from "biased" effects (drift, temperature, aging). That's similar to, but not the same as, our range/statistical split.
   - It describes four methods: extreme value (its recommended first approach), a combined variant, RSS, and Monte Carlo.
   - The best match for sections 3 and 6.
   - The exact "biased linear + random RSS" rule isn't in it; that's RAC worst-case circuit analysis practice.
2. **[Graeb07] Graeb, *Analog Design Centering and Sizing*,** the worst-case analysis chapters. The theory behind the loop: range vs statistical parameters, worst-case points, and re-linearizing there. Heavier, but it's the backbone of sections 5–6.
3. **[Stolfi97] de Figueiredo & Stolfi,** the monograph, §3.7 onward, for how affine arithmetic chooses lines for formulas (section 5.4). Their 2004 survey is a shorter overview.
4. **For the corner theorem:** Middlebrook's Extra Element Theorem (the intuition you may already have as an EE), then [N&P07] §5 if you want the proof and the counterexamples. Optional.

Later, when we build the simulator side: **[Xyce16]** (how a real simulator computes sensitivities).

---

## Appendix: the model behind the numbers

- **DC:** divider as a Thévenin source. BJT with VBE = 0.65 V at 1 mA, plus VT·ln(IC / 1 mA), minus 2 mV/°C·(T − 25 °C). β_actual = β·(1 + 0.5%/°C·(T − 25 °C)). No Early effect. No saturation modeled; VCE never drops below 3.25 V anywhere in the box, so the transistor stays in its normal region.
- **AC (mid-band, 1 kHz):** gain = β·RC / (rπ + (β + 1)·RE), rπ = β·VT/IC. f_L = 1 / (2π·C_in·R_in), R_in = R1 ∥ R2 ∥ (rπ + (β + 1)·RE). Ideal signal source.
- **Knobs:** as in section 3. C_in aging: capacitance × (1 − 0.2·life), life from 0 (new) to 1 (end of life). Resistor tempco and drift, and C_in tempco, are left out for simplicity.
- **Statistics:** each statistical knob is normal, with its edges at ±3σ, truncated at the edges. β is normal(200, 33) truncated to 100…300; datasheets only give min/max, so the distribution is an assumption. It's exactly the kind the engine must show the user.
- **Answer key:** all 512 corners, plus 20,000 random interior points per check.
- **Sensitivities** here were computed by finite differences on this small model. The real engine gets them analytically from the simulator (v2 notes, Part D).
