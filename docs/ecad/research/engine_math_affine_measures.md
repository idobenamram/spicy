# The Engine's Numeric Core: Affine Forms, Measures, and Slopes by Nudging

> 2026-09-27 · Research note for roadmap M3a. Companion to `../engine.md` §3 and §6.2, and `../walkthrough.md` §5 and §8.
> Every number here was computed by a script in `/root/.claude/jobs/443154a8/tmp/engine_research/affine/` (scratch, not committed; the file name is given next to each result), or is cited (paper + section, or file:line in the reference simulators under `/tmp/refs`).
> Other notes cover the worst_case loop, the sigma(k) method, and the guards. This one specifies the numeric building blocks they all use.

---

## Summary

The engine's common currency is the **affine form**: a center, one slope per named knob, and a remainder. The remainder means three different things depending on where the form came from. A formula over exact inputs has a **proven** remainder (affine arithmetic, de Figueiredo & Stolfi). A simulator line has an **unknown** remainder, estimated only from simulated-vs-predicted misses. The simulator and the measurement also have a **numerical band**. The type must keep these three apart, because they lead to different verdicts. For the MVP, the engine builds every line by nudging whole measures through the simulator, so it needs only the exact affine operations. The Chebyshev and min-range operations (worked through below) wait for datasheet arithmetic. The measure `h.f_low(-3dB)` needs a precise definition that the reference simulators don't provide. We recommend: −3 dB literally, relative to the maximum of |H| over the band, taken at the crossing nearest below it, found by bracketing on a 20-points-per-decade grid and then secant iteration in ln f. That gives a smooth measure, accurate to about 1e-15. Plain interpolation (what ngspice and Xyce do) is off by up to 1.3% and makes nudged slopes wrong by ±20%. Slopes of f_low come cheaply from the implicit function theorem, **provided the reference level's own slope is included**: without it, RC's slope comes out −0.40 Hz instead of 0. Nudged slopes are only as good as the simulator's convergence. Our simulator today returns VC 19 µV off, and a forward difference is **26% wrong** when a nudge changes the Newton iteration count, and **25% wrong** when nominal and nudged runs start differently. Engine-grade settings (separate voltage and current tolerances, reltol 1e-6) cut the error to 1e-13 V at no extra cost for warm-started nudges.

---

## 0. The data flow on one page

```
 knob point ε  (8 numbers, each in [-1, 1])
      │   knob coordinates: x = mid + rad·ε      e.g. R1 = 47 kΩ + 470 Ω·ε_R1        (§1.3)
      ▼
 backend.run(point) ── DC operating point (+ AC at chosen frequencies) ──►  f64 results
      │                                                                     + Newton band (§5)
      ▼
 measure library:  dc(output.v)  │  h.at(1kHz).mag()  │  h.f_low(-3dB)                (§3)
      │   → one MeasureValue { value, band } per measure, per simulated point
      ▼
 slopes by nudging: 1 nominal + 8 nudged runs, h = 1e-3 in ε units                    (§4)
      │   one nudged run serves all three measures
      ▼
 affine form per measure:  center + Σ aᵢ·εᵢ,  remainder,  band                         (§1)
      ▼
 worst point · contributors (§2) · verdict   ← the loop, sigma(k) and guards notes
```

Everything below follows one amplifier, the walkthrough's CE stage, with its 8 MVP knobs (roadmap §4.2): range `temp` (−10…60 °C) and `vcc.v` (12 V ± 5%); statistical `r1`, `r2`, `rc`, `re` (±1%), `c_in` (±20%), `q1.beta` (100…300).

---

## 1. The affine form

### 1.1 What it is

The walkthrough's line for VC (walkthrough §5.3), recomputed with central differences on the reference model (`headroom_and_ops.py`):

```
VC ≈ 5.49991 − 0.35404·ε_T + 0.13537·ε_VCC − 0.14110·ε_β
             + 0.07706·ε_R1 − 0.07455·ε_R2 − 0.06500·ε_RC + 0.06135·ε_RE      [V]
```

That's all an affine form is: a center, plus one coefficient per named knob. Each coefficient is **the swing at the knob's edge**: ε_T = −1 is −10 °C, and it adds 0.354 V. The general shape is

```
x  =  c  +  Σᵢ aᵢ·εᵢ  +  remainder          εᵢ ∈ [−1, 1]
```

The name on each term is what makes forms combine correctly (§1.6). Two forms that both contain ε_VCC share that knob, and their VCC terms add or cancel.

### 1.2 Three kinds of error, three meanings

The "± err" in engine.md §3.1 hides three different things. They are not interchangeable.

| | (a) Proven remainder | (b) Linearization remainder | (c) Numerical band |
|---|---|---|---|
| **Comes from** | A formula over exact inputs, through affine arithmetic | A simulator line: tangent slopes at one point | The simulator's convergence tolerance, the linear solve, and the measurement's interpolation |
| **Example** | 1/C_in over 0.8…1.2 µF: the Chebyshev line misses the curve by at most **0.02105 /µF** (§1.5) | The VC line misses the simulated worst corner by **0.182 V** (walkthrough §5.5) | Our simulator returns VC **19 µV** off at default tolerances (§4.2) |
| **Known how** | Computed exactly with the operation (de Figueiredo & Stolfi) | Only *observed*: largest \|simulated − line\| seen at the points simulated so far | Estimated from the last Newton update, or the root finder's last step (§5) |
| **Is it a bound?** | Yes, once rounding errors are also folded into a new symbol ([Stolfi97] §3.6.2, p.51) | **No.** An unchecked corner can miss by more | An estimate, not a bound |
| **Best verdict it supports** | PASS (guaranteed) | PASS (estimated) | Decides UNDECIDED when a value is within the band of a spec edge |

**Why keep them apart.** Suppose the engine adds the VC line's observed miss (0.18 V) to a proven remainder and calls the sum "err". The result would look like a bound, and a PASS resting on it would be labelled "guaranteed". It isn't. P2 ("outer bounds prove passes; every result says which it has") requires the type to record which kind it holds.

**Precedent and departure.** Affine arithmetic has only kind (a). Its remainder is a new noise symbol, and it is rigorous by construction [Stolfi97]. The worst-case-distance literature has only kind (b): it linearizes at a point and re-checks by simulation [AGW94]. We need both in one format, because hand formulas (datasheet arithmetic, §4.1 of engine.md) and simulator lines meet in the same spec expressions.

### 1.3 Knob coordinates

**ε ∈ [−1, 1], mapped linearly onto the knob's range.**

```
x = mid + rad·ε      mid = (lo + hi)/2,  rad = (hi − lo)/2
```

| Knob | lo … hi | mid | rad | 1e-3 in ε means |
|---|---|---|---|---|
| temp | −10 … 60 °C | 25 °C | 35 K | 0.035 K |
| vcc.v | 11.4 … 12.6 V | 12 V | 0.6 V | 0.6 mV |
| r1 | 46.53 … 47.47 kΩ | 47 kΩ | 470 Ω | 0.47 Ω |
| c_in | 0.8 … 1.2 µF | 1 µF | 0.2 µF | 0.2 nF |
| q1.beta | 100 … 300 | 200 | 100 | 0.1 |

Why ε and not physical units:
- **Every coefficient is a swing in the measure's own unit.** The engine can compare, sort and sum contributions with no conversion (§2). A slope "per ohm" and one "per kelvin" can't be compared.
- **Knobs with a zero nominal work.** An offset voltage of 0 ± 1 mV has no relative step. ngspice needs a special case for exactly this (`Sens_Abs_Delta`, `cktsens.c:24-25, 577-580`), and Xyce too (`N_NLS_SensitivityResiduals.C:224-229`).
- **The statistics stay simple.** A tolerance edge at 3σ is σ = 1/3 in ε, for every knob. A linear map keeps a normal distribution normal.

**Asymmetric ranges.** The reference model maps ε piecewise: nominal at ε = 0, the two edges at ±1 (`ce_amp_model.py`, `value()`). With an asymmetric range, say an electrolytic's +80/−20%, that puts a kink at nominal. The left and right slopes then differ, and a forward difference sees only one of them. Use the linear map above instead. Then nominal is simply a point with ε ≠ 0 (for +80/−20%, ε_nom = −0.6). All 8 MVP knobs are symmetric about nominal, so the two maps agree today.

**Log coordinates.** For wide ranges (β ×3) and for positive measures that behave multiplicatively (f_L ∝ 1/C), lines drawn in log space fit much better. From `coords.py`: the nominal tangent's prediction at the corner it picks, versus that corner simulated:

| Spec side | knobs lin, measure lin | knobs log, measure lin | knobs lin, measure log | **both log** |
|---|---|---|---|---|
| VC max (true 6.5905) | 6.4084 (miss 0.182) | 6.4596 (0.131) | 6.4877 (0.103) | **6.5485 (0.042)** |
| VC min (true 4.6597) | 4.5914 (0.068) | 4.6146 (0.045) | 4.6625 (−0.003) | 4.6822 (−0.023) |
| f_low max (true 26.738 Hz) | 24.820 (1.918) | 25.435 (1.303) | 25.426 (1.312) | **26.217 (0.521)** |
| gain min (true 4.4626) | 4.4777 (−0.009) | 4.4725 (−0.004) | 4.4791 (−0.010) | 4.4741 (−0.005) |

In log–log coordinates f_L's dependence on C_in is *exactly* linear (ln f_L = const − ln C). That's the walkthrough's "log space and splitting help" (§5.4), quantified. The price: a normal distribution in β isn't normal in ln β, and with log coordinates the nominal no longer sits at ε = 0 (for β, ε_nom = 0.262). The loop re-linearizes anyway, so the gain is fewer rounds, not a different answer. **MVP:** linear everywhere, but with a `scale` field in the knob coordinate so that log can be tried in M3c against the answer key.

### 1.4 Storage: dense or sparse

| | Dense `Vec<f64>`, one slot per knob | Sparse, sorted `(symbol, coeff)` pairs |
|---|---|---|
| MVP (8 knobs) | 64 bytes; trivial | ≤ 8 pairs; trivial |
| 8-channel bridge DAQ (≈450 knobs, `engine_precision_analog.md` §4.4) | 3.6 KB per form; 36 MB for 50 specs × 200 frequencies (that report's estimate) | only the knobs a spec depends on (a spec sees its connected region, engine.md §6.2) |
| add / sub | one loop over all knobs | merge of two sorted lists, O(n + m) |
| Remainder symbols from non-affine ops | don't fit (they aren't knobs) | fit naturally: another symbol kind |

**Recommendation:** sparse, sorted by symbol, inline storage for the first 8 terms (`SmallVec<[(Sym, f64); 8]>`). That's Stolfi's own layout: "we make sure that the terms of every affine form are always sorted in increasing order of their noise symbol indices" ([Stolfi97] §3.17.1, p.79). The symbol is either a knob or an anonymous remainder symbol. That second kind is how affine arithmetic keeps a reused intermediate correlated with itself: each non-affine operation creates a fresh symbol [Stolfi97]. The MVP never creates one, but reserving the variant now means the datasheet engine (M5) won't change the type.

### 1.5 Operations

**Exact operations** (no new error). With x = c_x + Σ xᵢεᵢ and y = c_y + Σ yᵢεᵢ:

```
x ± y  = (c_x ± c_y) + Σ (xᵢ ± yᵢ)·εᵢ
α·x    =  α·c_x      + Σ α·xᵢ·εᵢ
x + b  = (c_x + b)   + Σ xᵢ·εᵢ
```

**Non-affine operations** replace f(x) by a line α·x + ζ, plus a new symbol of radius δ that covers the gap ([Stolfi97] §3.7, pp.53–55). Only the line's choice differs:
- **Chebyshev (minimax):** the line whose worst miss over [lo, hi] is smallest. For f″ of one sign: α is the secant slope (f(hi) − f(lo))/(hi − lo). The miss is extreme at both ends and at the point u where f′(u) = α. ζ and δ put the line halfway between ([Stolfi97] §3.8.2, Theorem 2, p.57; the square root is worked in §3.9, eq. 3.8–3.11).
- **Min-range:** α = f′ at the end where |f′| is smallest. The result's range is then exactly [min f, max f], never wider. The miss is larger, but the range can't include impossible values (such as a negative 1/x) ([Stolfi97] §3.10, pp.64–66). For 1/x the authors recommend min-range, with α = −1/b² (§3.12, pp.69–70).
- **Multiplication:** x·y = c_x·c_y + Σ (c_x·yᵢ + c_y·xᵢ)·εᵢ, plus a new symbol of radius rad(x)·rad(y), where rad(x) = Σ|xᵢ| over all of x's symbols ([Stolfi97] eq. 3.18, p.72). The authors show this is "at most four times the error of the best affine approximation", which costs O(m log m) (§3.13, pp.71–72). Division is x·(1/y) (§3.14).

Each one below is worked on the amplifier (`headroom_and_ops.py`, library `aa.py`).

**1/x: the f_L factor 1/C_in.** C_in = 1 µF ± 20% is the form 1.0 + 0.2·ε_C (in µF).

| Line for 1/C | Form (1/µF) | Range | Covers the truth [0.8333, 1.2500]? |
|---|---|---|---|
| Tangent at 1 µF (what nudging gives) | 1 − 0.2·ε_C | [0.800, 1.200] | **No**: misses 1.25 by 0.05 at C = 0.8 µF (walkthrough §6: losing 20% of C raises f_L by 25%, not 20%) |
| Chebyshev | 1.02062 − 0.20833·ε_C ± 0.02105 | [0.7912, 1.2500] | Yes; miss checked on 4001 points: 0.02105 |
| Min-range | 1.04167 − 0.13889·ε_C ± 0.06944 | [0.8333, 1.2500] | Yes, and exactly the true range |

The monograph draws the Chebyshev line for 1/x (Fig. 3.8) without a formula. Theorem 2 gives it in closed form, here on [a, b] = [0.8, 1.2]: α = −1/(ab) = −1.0417 /µF² (−0.20833 per unit ε), and the extremes of the miss are at the ends and at u = √(ab) = 0.9798 µF. So δ = (1/√a − 1/√b)²/2 = 0.02105.

**Division, with shared knobs: the divider 12 V · R2/(R1 + R2)** (walkthrough §7):

| Method | Range (V) | Excess over the truth |
|---|---|---|
| Exact (the 4 corners; the corner theorem applies) | 2.0708 … 2.1402 | — |
| Affine: 12·R2·(1/(R1 + R2)) | 2.0703 … 2.1404 | 0.0007 V |
| Interval arithmetic | 2.0636 … 2.1478 | 0.0148 V |

Interval arithmetic treats the R1 inside the sum and the R2 on top as unrelated, so it pairs "R2 high on top" with "R2 low below". Affine arithmetic keeps the names and comes within 1% of exact.

**Multiplication: supply power P = VCC · I_supply** (walkthrough §8.3). I_supply is a simulator line (nudged slopes: 1.5947 mA + 0.1095 mA·ε_VCC + 0.0752 mA·ε_T + …). VCC appears in both factors.

| | P range (mW) |
|---|---|
| AA product (new symbol rad(VCC)·rad(I) = 0.6 V × 0.2608 mA = **0.1565 mW**) | 14.894 … 23.380 |
| P nudged directly as one measure (same slopes, no remainder) | 15.050 … 23.224 |
| True (256 corners) | 14.945 … 23.310 |

The linear coefficients of the AA product and of the directly nudged P are **identical** (to printed precision). That's the chain rule. The AA product adds only the product's own curvature term.

**sqrt, after a product: Johnson noise density √(4kT·R_in).** T is a knob, and R_in (a simulator line, 7926 Ω ± 286 Ω) also depends on T through β(T) and V_BE(T):

| | e_n (nV/√Hz) |
|---|---|
| AA: 4k·T·R_in, then sqrt | center 11.406, range 10.485 … 12.328 (sqrt's own Chebyshev miss: 0.018) |
| True (256 corners) | 10.399 … 12.239 |

The AA range misses the true minimum by 0.086 nV/√Hz. **AA is rigorous only if its inputs are.** R_in here is a tangent line with no remainder (the β curvature again), so everything computed from it inherits that gap.

**log: gain in dB.** (The monograph doesn't treat log; Theorem 2 applies because log is concave.) 20·log₁₀ of the gain line (4.4777 … 4.7058) gives 13.238 dB ± 0.217 dB, and the log's own Chebyshev miss is only **0.0013 dB**. Over a ±2.5% range, log is almost a straight line. The spec 4.6 ± 5% is 12.810 … 13.679 dB.

**abs: |gain − 4.6|.** The signed error spans −0.1223 … +0.1058 (from the line), so it straddles zero. The Chebyshev |·| gives 0.0573 ± 0.0567 plus small knob terms: range −0.0077 … 0.1223. The lower end is negative, which is meaningless for an absolute value. |·| through zero is AA's worst case (the precision study's CMRR had the same issue: `engine_precision_analog.md` §3.3). **Rule:** keep the signed form, and turn `|e| <= b` into the two one-sided checks `−b <= e <= b` before linearizing. Apply |·| and dB last.

**What the operations are for.** In the simulator world, the loop evaluates a measure expression *at each simulated point* (exactly) and nudges the whole expression to get its line. That equals composing the parts' lines with the operations above, to first order (the P example). So AA operations matter in three places:
1. **Formula-world specs** (datasheet arithmetic, engine.md §4.1), where the proven remainder gives PASS (guaranteed).
2. **Instant re-evaluation on edits** from stored forms (walkthrough §2).
3. **Hierarchy:** substituting a block's stored output forms into the parent (engine.md §7).

None of the three is in the MVP.

### 1.6 The headroom example, number by number

Walkthrough §8 claims: headroom `vcc.v − dc(output.v)` is 4.81…7.94 V by intervals, 5.26…7.74 V affine, and 5.17…7.72 V true. Reproduced in `headroom_and_ops.py`:

```
VCC form      12.00000                  + 0.60000·ε_VCC                       range 11.4 … 12.6
VC line        5.49991 − 0.35404·ε_T   + 0.13537·ε_VCC − 0.14110·ε_β + …      range 4.5914 … 6.4084
────────────────────────────────────────────────────────────────────────────────────────────────
headroom       6.50009 + 0.35404·ε_T   + 0.46463·ε_VCC + 0.14110·ε_β
                       − 0.07706·ε_R1  + 0.07455·ε_R2  + 0.06500·ε_RC − 0.06135·ε_RE
               radius = 0.35404 + 0.46463 + 0.14110 + 0.07706 + 0.07455 + 0.06500 + 0.06135 = 1.23773
```

| Number | How it arises | Is it right? |
|---|---|---|
| **4.8095 … 7.9403** (interval) | [11.4, 12.6] − [4.6597, 6.5905], using VC's *true* range from the 256 corners. 11.4 − 6.5905 = 4.8095 | Too wide. VC = 6.59 happens only at VCC = 12.6 V, never at 11.4 V. Intervals forget that |
| **5.2624 … 7.7378** (affine) | 6.50009 ∓ 1.23773. The VCC terms partly cancel: 0.600 − 0.135 = 0.465 | Correct correlation, **but not an enclosure**: the true minimum 5.1647 is below 5.2624. The VC line has no remainder, and β's curvature is missing (walkthrough §5.4) |
| **5.1647** (true min) | Corner T −10 °C, VCC 11.4 V, R1 +1%, R2 −1%, RC −1%, RE +1%, β 100 | Brute force, 256 corners |
| **7.7217** (true max) | The opposite corner | Brute force |
| 5.3446 … 7.5300 | 20,000 uniform random points inside the box | Random sampling misses both extremes by ~0.18 V: corners are rare |

Two lessons for the engine:
- **Run the loop on the derived measure itself.** The headroom line's predicted worst corners, simulated, give **5.1647 and 7.7217**, exactly the truth, for one simulation each. The slopes came free by the chain rule from VC's line.
- **Carry the observed miss as an *estimated* remainder.** Attaching VC's worst observed miss (0.1821 V) gives 5.0803 … 7.9199. That contains the truth, but it's an estimate (kind (b) in §1.2), not a proof.

---

## 2. Contributors

### 2.1 One definition for both confidence classes

**Contribution of knob i = its slope × its coordinate at the worst point:** cᵢ = aᵢ·(ε*ᵢ − ε⁰ᵢ).

Signs are taken toward the spec edge being checked (shown here for a maximum). For `worst_case`, the worst point is the corner ε*ᵢ = sign(aᵢ), so cᵢ = |aᵢ| and the share is |aᵢ| / Σ|aⱼ|.

For `sigma(k)`, the linear model's kσ worst point for the statistical knobs is ε*ᵢ = k·σᵢ²·aᵢ / √(Σⱼ σⱼ²aⱼ²) [AGW94]. Then cᵢ = k·σᵢ²aᵢ² / RSS, with RSS = √(Σ σⱼ²aⱼ²). These add up exactly to k·RSS, the statistical part of the spread. The share of each knob is its share of the variance, aᵢ²σᵢ² / Σ aⱼ²σⱼ². (Risk management calls this the Euler, or gradient, allocation.) Range knobs sit at full strength in both classes.

So `worst_case` shares are |aᵢ| over the sum, and `sigma` shares are aᵢ² over the sum of squares. They are one formula at two different worst points.

### 2.2 The gain spec's "RC + RE ≈ 80%" (verified)

From `contributors.py`, nominal slopes of the mid-band gain (|H(1 kHz)| is 0.02% lower, §3.2):

| Knob | Slope aᵢ (per unit ε) | worst_case share |
|---|---|---|
| RC | +0.04592 | 40.3% |
| RE | −0.04587 | 40.2% |
| β | +0.01302 | 11.4% |
| VCC | +0.00596 | 5.2% |
| T | −0.00132 | 1.2% |
| R1 | −0.00099 | 0.9% |
| R2 | +0.00096 | 0.8% |
| **RC + RE** | | **80.5%** |

The walkthrough's "~80%" (§7.3) holds. The same spec under `sigma(3)`:

```
gain, sigma(3), low side:   total spread 0.07349
  range knobs (full strength)     0.00728   ( 9.9%)    VCC 0.00596 (8.1%)   T 0.00132 (1.8%)
  statistical (3·RSS)             0.06621   (90.1%)    RC 0.03184 (43.3%)   RE 0.03178 (43.2%)
                                                       β  0.00256 ( 3.5%)   R1, R2 < 0.1%
  RC + RE = 96.1% of the statistical variance
```

### 2.3 How to present range vs statistical

One table, two groups, and each group adds up:

```
 S2 gain ≥ 4.37  (sigma(3), low side)   nominal 4.5917   line 4.5183   margin 0.148
 ─────────────────────────────────────────────────────────────────────────────
 range knobs        at full strength        0.0073   ██
   vcc.v            11.4 V                  0.0060
   temp             60 °C                   0.0013
 statistical        3σ point, RSS           0.0662   ████████████████
   rc               4.667 kΩ (−0.69%)       0.0318   (48% of variance)
   re               1.007 kΩ (+0.69%)       0.0318   (48%)
   q1.beta          180.3                   0.0026   (4%)
   r1, r2           ±0.015%                 0.0000
 nonlinearity       simulated − line       −0.0002   (simulated 4.5181 at this point)
```

The point column is ε*ᵢ converted back to physical units. It is the counterexample, so it's pasteable as a `corner`. (Here the line picks 60 °C. Walkthrough §5.7 shows that at β = 100 the temperature slope flips, which is the loop's business, not the table's.)

### 2.4 The nonlinearity row

Contributions from the nominal line don't add up to the simulated worst value. For VC max, worst_case: nominal 5.4999 V, simulated worst corner 6.5905 V, so the true move is 1.0906 V. Which slopes should the contributions use?

| Knob | nominal slope × ε* | worst-point slope × ε* | midpoint slope × ε* |
|---|---|---|---|
| T | 0.3540 | 0.4076 | 0.3712 |
| VCC | 0.1354 | 0.1771 | 0.1522 |
| β | 0.1411 | **0.5884** | 0.2602 |
| R1, R2, RC, RE | 0.0771, 0.0745, 0.0650, 0.0614 | 0.0737, 0.0699, 0.0607, 0.0534 | 0.0760, 0.0733, 0.0635, 0.0583 |
| **Sum** | **0.9085** | **1.4307** | **1.0547** |
| **Nonlinearity row** (true − sum) | **+0.1821** | **−0.3401** | **+0.0358** |

- **Nominal slopes** understate β (the curve is steep at β = 100).
- **Worst-point slopes** overstate it (the tangent there is the steepest part).
- **Midpoint slopes** (tangent halfway to the worst point) nearly add up: that's the midpoint rule for ∫ ∇f·dε along the path, which is exact up to third-order terms. They cost one more linearization (N more runs at L0).

**MVP rule:** use the loop's final linearization and print the nonlinearity row, so the table always adds up to the simulated value. A large row is itself a warning that the line is poor.

### 2.5 R²: only for fits, and not a safety measure

engine.md §3.5 asks for R² "for a fit". Nudged slopes are a tangent, not a fit, so R² doesn't apply to them. Where the engine does fit (e.g. a line through Monte Carlo samples at sign-off), R² can look excellent while the line misses by the whole margin (`contributors.py`, least-squares line in ε):

| Measure | Points | R² | Largest miss | Spread |
|---|---|---|---|---|
| VC | 256 corners | 0.9963 | 0.056 V | 1.93 V |
| VC | 2,000 MC samples (σ = tol/3) | 0.9925 | **0.090 V** | — |
| gain | 256 corners | 0.9980 | 0.0041 | 0.24 |
| f_low | 2,000 MC samples | 0.9915 | 0.71 Hz | — |

R² = 0.99 comes with a 0.09 V miss, the same size as S1's 0.09 V margin (walkthrough §5.3). **Report the largest miss, not R².** For nudged lines, report the observed miss at the checked point ("line 6.408 V, simulated 6.590 V").

---

## 3. Measures from simulator output

### 3.1 `dc(output.v)`

This is the operating-point node voltage, read directly. Its only error is the Newton band (§4.2, §5). Nothing to interpolate.

### 3.2 `h.at(1kHz).mag()`

**Ask the backend for exactly 1 kHz.** Don't interpolate from a sweep. An AC point at a known operating point is one complex linear solve, far cheaper than the operating point itself. Interpolating adds error for no gain: ngspice and Xyce both interpolate `FIND … AT=` linearly in linear frequency (`com_measure2.c:780-785`; `N_IO_MeasureFindWhen.C:408-414`), and Gnucap does the same (`m_interp.h:29-89`).

Two details the answer key (M3b) must match:
- **|H(1 kHz)| is not the mid-band gain.** C_in's high-pass is still 0.02% down at 1 kHz: |H(1 kHz)| = 4.59174 × 0.999798 = **4.59081** (`acmeas.py`). Over the 256 corners, |H(1 kHz)| spans 4.46099 … 4.70240, while the walkthrough's mid-band gain spans 4.46257 … 4.70305. A test that compares our simulator's |H(1 kHz)| with the walkthrough's gain formula must allow 4e-4, or compute the same quantity.
- **Grid points aren't exact.** `ac.rs` builds DEC grids by repeated multiplication, `f *= r` (`crates/spicy_simulate/src/ac.rs:26-31`). The 31st point of a 10-per-decade grid from 1 Hz comes out as 1000.000000000002, not 1000. Compute f_k = f_start·10^(k/N) directly, and add requested exact points to the grid.

### 3.3 `h.f_low(-3dB)`: defining it

"−3 dB" needs three choices that the spec text doesn't make.

**(1) Relative to what?**

| Reference level | Nominal f_low | Smooth in the knobs? | Precedent |
|---|---|---|---|
| **Maximum of \|H\| over the analysis band** | 20.12695 Hz | Yes, where the maximum is unique (envelope theorem) | The ngspice idiom: `meas ac fil1max max dbnode5`, `let fil1max3db = fil1max - 3`, then `TRIG … VAL=fil1max3db` (`examples/xspice/various/x_fer.cir:47-51`) |
| \|H\| at a stated reference frequency (1 kHz) | 20.11882 Hz | Always | none built in |
| The mid-band gain formula | 20.12695 Hz | — | Not measurable in general |

None of ngspice, Xyce or Gnucap has a built-in "−3 dB from the passband". ngspice's `WHEN` right-hand side must be a number or a vector name (`com_measure2.c:1891-1905`), hence the `.control`-block idiom above. Xyce qualifiers "can not depend on another measure's value" (`doc/Reference_Guide/MEASURE_Command.tex:797-804`), and its `FRAC_MAX` is transient-only (`N_IO_MeasureManager.C:254`). Gnucap's `.measure` stores results as parameters, so a later measure could reference a max (`apps/c_measure.cc:47`), but there's no example in its tree.

**(2) −3 dB, or half power?** Literally −3 dB is |H|/ref = 10^(−3/20) = 0.707946. Half power, 1/√2 = 0.707107, is −3.0103 dB. For a single pole, the half-power point is exactly 1/(2π·R_in·C_in) = **20.07921 Hz**, while −3 dB exactly is x/√(1+x²) = 0.707946 → x = 1.002377, so **20.12695 Hz**. That's 0.24% apart. Textbooks define the cutoff as the half-power point, "approximately −3.01 dB of the nominal passband value" (Van Valkenburg, *Network Analysis*, 3rd ed., 1974, pp.383–384, as quoted by Wikipedia's "Cutoff frequency"; not checked in the book). But `f_low(L)` takes a level, and −1 dB must mean −1 dB, so −3 dB should mean −3 dB too. That is also slightly conservative for a `<=` spec. A user who wants half power can write −3.01 dB. The walkthrough's f_L numbers are half-power (1/(2πRC)), so the answer key must convert. The spec margin (30 vs ~27 Hz) doesn't care, but a test pinned to 5 digits does.

**(3) Which crossing?** For `f_low`, it's the crossing **nearest below the reference**: scan down in frequency from the maximum. ngspice's idiom uses `RISE=1`, the first rise from the sweep start (the lowest crossing). The two differ when the response dips below the level, recovers, and dips again below the passband, e.g. with a notch. "Nearest the passband" is what an engineer means by the band edge. If more than one crossing exists below the reference, the measure should warn: the spec is then ill-posed, and it jumps when crossings appear or disappear.

**Recommendation for the MVP:** f_low(L) = the frequency nearest below argmax|H| where |H| = max|H| · 10^(L/20), with L = −3 dB literally. Add an optional `ref:` frequency later. For this amplifier, the response rises monotonically (our BJT has no junction capacitances), so the maximum sits at the top of the band. It's then 1 − ½(f_L/f_top)² ≈ 1 − 2e-8 of the mid-band gain at f_top = 100 kHz: negligible.

### 3.4 The algorithm on a sampled sweep

```
 dB rel. max
   0 ┤                                    ●───●───●───●   ← reference: max |H| (refined if interior)
     │                            ●───●
  -3 ┤ - - - - - - - - - - - - ◆ - - - - - - - - - - -    ← level = max − 3 dB
     │                    ●  ↑ f_low
     │            ●───●      1. bracket: scan down the grid from argmax to the first sample below the level
     │    ●───●              2. refine: secant iteration on g(u) = dB(H(e^u)) − level, u = ln f,
     └──┬──────┬──────┬──        each step is one AC solve at the same operating point
       1 Hz  10 Hz  100 Hz
```

1. **Grid:** N points per decade, f_k = f_start·10^(k/N), over a band the engine chooses from the spec. For this file: two decades below the spec bound (30 Hz → 0.3 Hz) up to two decades above the highest `at` frequency (1 kHz → 100 kHz).
2. **Reference:** the largest sample. If it's interior (not at the band edge), refine it by a parabola in (ln f, dB) through the three samples around it.
3. **Bracket:** from argmax, walk down to the first k with |H(f_{k−1})| < level ≤ |H(f_k)|.
4. **Refine:** secant iteration in u = ln f from the bracket, with Illinois regula falsi as a fallback if a step leaves the bracket. Stop when |Δu| < 1e-9.

Accuracy vs cost on the nominal amplifier (relative error of f_low; `acmeas.py`):

| Points/decade | Linear in (f, \|H\|) | Linear in (f, dB), as ngspice `vdb` and Xyce | Linear in (ln f, dB) | **+ secant** | AC solves (with secant) |
|---|---|---|---|---|---|
| 5 | 3.9e-2 | 5.3e-2 | 2.6e-2 | **4e-16** | 32 |
| 10 | 1.7e-3 | 2.2e-3 | 1.1e-3 | **2e-16** | 56 |
| 20 | 7.5e-4 | 9.9e-4 | 4.9e-4 | **4e-16** | 106 |
| 50 | 2.5e-4 | 3.3e-4 | 1.7e-4 | 2e-16 | 256 |
| 100 | 9.4e-5 | 1.3e-4 | 6.3e-5 | 2e-16 | 505 |

Worst case over the 256 corners at 10 per decade: linear in (f, |H|) 1.0e-2; linear in (f, dB) 1.3e-2; linear in (ln f, dB) 6.7e-3; secant 1.3e-15. At 20 per decade the first three are 2.5e-3, 3.3e-3 and 1.7e-3. The secant needs 5–6 extra AC solves.

ngspice interpolates `WHEN` linearly in the raw scale, which is linear Hz for AC, on whatever quantity was asked for (dB for `vdb`) (`com_measure2.c:502-505, 664`). Xyce does the same (`N_IO_MeasureWhenAT.C:107-133`), and so does Gnucap (`measure_cross.cc:95-96`). None of them refines. **We depart:** interpolation error at 10 per decade (up to 1.3%, or 0.35 Hz near 27 Hz) is over a tenth of the gap between this design's worst case (26.7 Hz) and its limit (30 Hz), and it ruins slopes (next section). The refinement costs a few linear solves.

**Why 20 per decade, not 10:** the grid's job is to bracket and to *see* multiple crossings and peaks. The precision study found a Q = 10 peak under-read by up to 3.7 dB at 20 per decade (`engine_precision_analog.md` §4.7). For a first- or second-order high-pass, 10 per decade brackets fine. 20 per decade doubles a cheap cost for margin. Real peaks need local refinement (their note's fix).

### 3.5 Smoothness in the knobs

As a knob moves, the crossing slides across grid cells, and the interpolation error ripples with the grid's period. Forward-difference slope of f_low with respect to ε_C (h = 1e-3), evaluated at 200 base points between ε_C = 0 and 0.2 (exact slope at nominal: −4.0254 Hz; `acmeas.py`):

| Measure implementation | Slope error, min … max over the 200 points |
|---|---|
| Linear in (f, \|H\|), 10 per decade | −14.6% … +20.5% |
| Linear in (ln f, dB), 10 per decade | −10.3% … +12.9% |
| **+ secant refinement** | **−0.0200% … −0.0192%**: just the forward difference's own truncation error, h·f″/2f′ |

**A nudged slope of an unrefined crossing measure is noise.** Refinement is mandatory once slopes are taken by nudging.

### 3.6 When there is no crossing

If |H| stays within 3 dB of the reference all the way down to f_start, the crossing lies below the band. Sweeping from 25 Hz instead of 1 Hz (`acmeas.py`): at nominal there's no crossing ("below 25 Hz"), while the worst corner has one at 26.96 Hz. So the same measure is a number at one knob point and "below the band" at the next.

What the references do:

| Simulator | No crossing |
|---|---|
| ngspice | `m_measured = NAN` (`com_measure2.c:689-690`), then the measure fails with "out of interval" (`:2299-2303`); a failed `.meas` creates no parameter (`measure.c:362-379`) |
| Xyce | Prints `name = FAILED` (MEASFAIL on by default, `N_IO_MeasureManager.C:106`); the value is DEFAULT_VAL = 0.0 (`N_IO_MeasureBase.C:148`) |
| Gnucap | Returns BIGBIG (≈ 0.92 × DBL_MAX) as an ordinary number, with no error (`measure_cross.cc:77, 105`; `include/constant.h:96`) |

Gnucap's choice is the dangerous one. For `f_low <= 30Hz`, a silent huge number fails correctly by luck, but the same trick for an `f_high >= …` spec would pass silently.

**Recommendation:** a measure returns one of three things:

```
MeasureValue::Value  { v, band }                 a crossing was found
MeasureValue::Beyond { side: Below | Above, limit } |H| stays on one side down to (or up to) `limit`
MeasureValue::Undefined { reason }               e.g. max |H| ≈ 0 (dead stage), NaN, several crossings
```

- On `Beyond`, the engine first **extends the band** by a decade at a time, up to a floor (say 1 mHz). A crossing found this way is a `Value`.
- A `Beyond` that survives the extension is a **one-sided bound**. It decides a spec only when the bound is on the passing side: `Below(1 mHz)` satisfies `f_low <= 30 Hz` (a DC-coupled amplifier has no low cutoff at all, and should pass).
- `Undefined` never passes. It gives UNDECIDED, with the reason.
- **A point that isn't a `Value` has no slope.** The loop can't linearize there, and must say so.

This refines language.md §8.4 ("a missing crossing is never treated as a pass"). A crossing shown to lie below a searched band is evidence, not a missing value. What must never pass is a crossing the engine *failed to look for*, or a meaningless one.

### 3.7 Slopes of f_low without re-running the sweep

At the crossing f*, the defining equation is

```
g(f, p) = dB(H(f, p)) − dB_ref(p) + 3 = 0,     dB_ref(p) = dB(H(f_ref, p))  or  max_f dB(H(f, p))
```

By the implicit function theorem,

```
df*/dp = − ( ∂dB(H(f*, p))/∂p  −  ∂dB_ref/∂p ) / ( ∂dB(H(f*, p))/∂f )
```

**The reference term matters.** If the reference is the maximum over the band, ∂dB_ref/∂p = ∂dB(H(f_max, p))/∂p by the envelope theorem (Danskin): the maximizer's own shift contributes nothing to first order. Checked on the nominal amplifier (reference |H(1 kHz)|; f* = 20.1188 Hz, ∂dB/∂f = 0.21544 dB/Hz, i.e. 9.98 dB/decade; `acmeas.py`):

| Knob | IFT with the reference term | IFT without it | Re-sweep + secant, central difference |
|---|---|---|---|
| T | −0.13643 | −0.12497 | −0.13643 |
| VCC | +0.00101 | −0.05129 | +0.00101 |
| R1 | −0.03407 | −0.02542 | −0.03407 |
| R2 | −0.15918 | −0.16770 | −0.15918 |
| **RC** | **0.00000** | **−0.40317** | **0.00000** |
| **RE** | **−0.00778** | **+0.39498** | **−0.00778** |
| C_in | −4.02051 | −4.02376 | −4.02051 |
| β | −0.38697 | −0.50163 | −0.38697 |

With the reference term, the IFT agrees with a full re-sweep to all five digits. Without it, RC gets a large slope it doesn't have (RC scales |H| at every frequency equally, so f_low can't depend on it), and RE's slope has the wrong sign.

**Cost.** For each nudged operating point, the IFT needs **2 AC solves** (at f* and at f_ref or f_max). A re-sweep needs about 111 (20 per decade over 5.5 decades) plus about 6 for the secant. ∂dB/∂f comes from the nominal run: analytically, dx/dω = −(G + jωC)⁻¹·(jC·x), one more solve with the matrix already factored at f*. At L0 the nudged operating point (a Newton solve) still dominates, so the saving is in AC work, which matters for large circuits and many frequencies. At L1 the same formula takes ∂H/∂p from AC sensitivities, provided they include the operating-point shift (§4.7).

---

## 4. Slopes by nudging

### 4.1 Our simulator's Newton loop today

| Setting | Ours (`crates/spicy_simulate`) | ngspice | Xyce (DC op) |
|---|---|---|---|
| reltol | 1e-3 (`lib.rs:44`) | 1e-3 (`cktntask.c:100`) | 1e-3 (`N_NLS_NOX_ParameterSet.C:161-171`) |
| Voltage tolerance | 1e-6, the same `abs_tol` as currents (`lib.rs:43`) | vntol 1e-6 V (`cktntask.c:102`) | ABSTOL 1e-12 in weights for all unknowns |
| Current tolerance | **1e-6 A** (`lib.rs:43`) | abstol 1e-12 A (`cktntask.c:99`) | ″ |
| Test | \|Δx\| ≤ abs_tol + rel_tol·max(\|x_k\|, \|x_{k+1}\|), every unknown (`trans.rs:47-59`) | voltages: reltol·max + vntol; currents: reltol·max + abstol (`niconv.c:55-58, 67-70`); **plus** each device's own current test (`bjtconv.c:45-70`) | weighted max-norm update < DELTAXTOL = 1 **and** ‖F‖∞ < RHSTOL = 1e-6 (`N_NLS_NOX_XyceTests.C:394-430`) |
| Residual checked? | No | Via device tests | Yes |
| Max iterations | 50 (`lib.rs:45`) | itl1 = 100 (`cktntask.c:110`) | — |
| Start | all zeros (`dc.rs:67`) | junction-voltage init (MODEINITJCT) | — |
| Junction limiting | clamp V/Vt at ±40 (`devices/bjt.rs:93-105`; "very bad limiting", its own TODO) | pnjlim (`devsup.c:49-84`) | — |

The difference that matters here: **1e-6 A as a current tolerance is a million times looser than SPICE's 1 pA**, and the same number also serves as the voltage tolerance.

### 4.2 What the tolerance does to the returned value

I rebuilt our DC Newton loop in Python, float64, line for line: same stamps, same Ebers–Moll model with Vt = 0.02585, same clamp, zero start, and the same convergence test (`newton_replica.py`). It reproduces our simulator's output on `ce_amp.spicy` (IS = 1e-14, BF = 200):

| | V(out) |
|---|---|
| Our simulator, `.raw` output (float32) | **5.52163839** V |
| Replica, default settings (18 iterations) | 5.5216384 V |
| Replica, converged to round-off | **5.52161922** V |

**Our simulator returns V(out) 19.2 µV (3.5 ppm) off,** and that's visible even through float32 (whose spacing at 5.5 V is 0.48 µV). Over 200 random points of the 6-knob box, the error reaches 34 µV, and I(VCC) is off by up to 5.0e-6 relative.

| Setting | V(out) error, max over the box | Iterations (cold start) |
|---|---|---|
| Default (reltol 1e-3, abs_tol 1e-6 for V and A) | 3.4e-5 V | 18 |
| Default + 1 polishing iteration | 5.1e-9 V | 19 |
| **reltol 1e-6, vntol 1e-9 V, abstol 1e-12 A** | **1.1e-13 V** | 20 |

Quadratic convergence makes accuracy cheap: two more iterations buy eight more digits.

### 4.3 Forward or central, and how big a step

Let each simulated value carry an error of at most δ. Then, with h the step (in ε units), for a forward difference

```
error ≈ h·|f″|/2  +  2δ/h      smallest at  h* = 2·√(δ/|f″|),   error* = 2·√(δ·|f″|)
```

and for a central difference

```
error ≈ h²·|f‴|/6  +  δ/h      smallest at  h* = (3δ/|f‴|)^(1/3),   error* ∝ δ^(2/3)
```

This is the classical trade-off. The first term is truncation (the curve bends within the step). The second is noise divided by the step. Where it comes from:
- Nocedal & Wright §8.1 (pp.195–197): forward difference (8.1), total error (8.5), and ε = √u (8.6) when the only noise is round-off u; central difference (8.7), with the optimal ε "about u^{1/3} and an error of about u^{2/3}". Their §9.1 (Lemma 9.1, eq. 9.5) covers our case, a *noisy* function: error ≤ L·ε² + η/ε for a central difference, with η the noise level.
- Gill, Murray & Wright §8.6: the forward interval h_F = 2√(ε_A/|Φ|), with Φ an estimate of f″ (eq. 8.44), and an automatic procedure that estimates Φ by a second difference (§8.6.2). The central interval (3ε_A/|f‴|)^{1/3} is theirs as quoted by Moré & Wild (2012, p.15); I could read GMW only through a garbled scan.
- Moré & Wild (2012), for noise given as a standard deviation ε_f: h = 8^{1/4}·√(ε_f/μ) ≈ 1.68·√(ε_f/μ) forward (eq. 8) and h = 3^{1/3}·(ε_f/μ)^{1/3} central (eq. 13), with μ a bound on |f″| or |f‴|.

Checked on VC vs ε_β, the most curved knob (f′ = −0.14110, f″ = 0.13492, f‴ = −0.19354 V per unit ε^k), with random noise of amplitude δ added (`fd_study.py`):

| δ (V) | Forward: h* theory → error | measured best | Central: h* theory → error | measured best |
|---|---|---|---|---|
| 2e-5 (≈ our default error) | 2.4e-2 → 3.3e-3 (**2.3%**) | 2.9e-3 at h = 1e-2 | 6.8e-2 → 4.4e-4 (0.31%) | 4.7e-4 at h = 3e-2 |
| 2e-9 (≈ polished) | 2.4e-4 → 3.3e-5 (0.023%) | 3.0e-5 at 3e-4 | 3.1e-3 → 9.5e-7 | 8.1e-7 at 3e-3 |
| 1e-13 (≈ engine settings) | 1.7e-6 → 2.3e-7 | 2.4e-7 at 1e-6 | 1.2e-4 → 1.3e-9 | 6.8e-10 at 1e-4 |

The formulas predict the measured optimum well. With noise at our default error level, even the best forward step gives only 2.3%.

### 4.4 What Newton "noise" really looks like

Newton's error isn't random. At a fixed iteration count, the returned iterate is a smooth function of the parameters: a fixed number of smooth Newton maps from a fixed start. So the error is a smooth bias, and it **jumps** when the iteration count changes (the precision study saw the same: `engine_precision_analog.md` §4.8). Three cases, from the replica (`fd_study.py`, `newton_jumps.py`):

**Case 1, lucky: same count.** Inside the MVP box every cold run takes exactly 18 iterations, so the 19 µV bias barely changes between nominal and nudged runs. Forward-difference slopes at default settings are good to 1e-5 relative. That's luck, not design.

**Case 2: the count changes.** Sweeping VCC over a wide range, the count goes from 13 to 20, with 10 changes, and the error jumps at each (e.g. 1.9e-6 V → 2.4e-10 V). A forward difference that straddles one change:

| h = 1e-4 across the change at ε_VCC = −11.87 | Slope (V per unit ε) | Error |
|---|---|---|
| Default settings | 0.12782 | **−26%** |
| + 1 polishing iteration | 0.17326 | −9e-6 |
| Exact | 0.17326 | — |

**Case 3: mixed starts (the warm-start trap).** A warm-started nudge (from the nominal solution) converges in 2 iterations instead of 18, so it's tempting. But at default settings the cold nominal run is 19 µV off, while the warm nudged run is almost exact. The difference is a fake slope of 19 µV/h:

| Slope of V(out) wrt ε_R1, forward (exact 0.07705) | h = 1e-2 | 1e-3 | 1e-4 | 1e-5 |
|---|---|---|---|---|
| Default, cold nominal + warm nudge | 2.5% off | **25% off** | 250% | 2500% |
| Engine settings, cold nominal + warm nudge | 8.4e-5 | 8.4e-6 | 8.4e-7 | 8.0e-8 |

Worst over 30 random base points (knob β, forward, h = 1e-3): default cold/warm 38%. With engine settings, it's 7.7e-4, set by truncation alone.

**Costs of warm starts** (`warm_cost.py`): a nudge of h = 1e-3 warm-started from the nominal solution takes **2 iterations at engine settings** (the same as at default), against 20 cold. A warm jump straight to a corner takes 3. Warm starts cut the Newton work of L0 slopes by about 7×, and they're safe only at engine settings.

### 4.5 What engine-grade accuracy means (roadmap M2e)

1. **Separate tolerances:** vntol for node voltages, abstol for branch currents, as ngspice does (`niconv.c:55-58, 67-70`). Engine mode: reltol 1e-6, vntol 1e-9 V, abstol 1e-12 A.
2. **A residual check** as well as the update check (Xyce's RHSTOL, `N_NLS_NOX_XyceTests.C:425-430`). A stalled iteration can take small steps without having converged.
3. **The last update is reported** as the operating point's numerical band (§5). It over-estimates the returned solution's error, which is safe.
4. **f64 end to end.** The CLI's `.raw` stores float32: 0.48 µV spacing at 5.5 V, which alone makes a forward slope wrong by 0.6% for R1 at h = 1e-3 (`band_numbers.py`). The engine reads results through the Rust API, never a raw file.
5. **Deterministic starts.** Within one linearization, all runs start the same way (all warm from the same nominal, or all cold). At engine settings mixing is harmless (case 3), but it costs nothing to keep.

Why these numbers: at reltol 1e-6 the replica's error drops from 3.4e-5 V to 1.1e-13 V, for +2 iterations cold and +0 warm. The precision study reached the same conclusion from a diode circuit (13 ppm at 1e-3, < 1e-4 ppm at 1e-6; `engine_precision_analog.md` §4.8).

**M2e's test, made concrete:** for all 8 knobs × 3 measures on the CE amplifier, forward slopes at h = 1e-3 and h = 1e-4 agree to 1e-3 relative. Also, a cold nominal with warm nudges gives the same slopes as all-cold, to 1e-3. Today's defaults fail the second check (25%).

### 4.6 Step units and size for the MVP

**Step in ε units,** i.e. a fixed fraction of each knob's half-range (the table in §1.3). ngspice steps 1e-6 *relative to the value* (`cktsens.c:577-582`), and Xyce steps √(machine ε)·|p| ≈ 1.5e-8·|p| (`N_NLS_SensitivityResiduals.C:224-229`, `N_DEV_DeviceInstance.C:623-632`). Their steps are tiny because they difference a closed-form device stamp, which is exact to round-off (§4.7). We difference a whole re-simulation, which carries Newton's band, so we need a much larger step. ε units also handle zero nominals, and they make the step proportional to the range the spec cares about.

**h = 1e-3, forward, stepping inward at the box edge** (at ε = +1, step to 1 − h). From the replica, worst relative slope error over 30 base points at engine settings (knob β, the most curved):

| h | Forward | Central |
|---|---|---|
| 1e-2 | 7.6e-3 | 5.9e-5 |
| **1e-3** | **7.7e-4** | 6.0e-7 |
| 1e-4 | 7.7e-5 | 4.4e-8 |

Why forward at 1e-3 is enough: the loop uses slopes to *pick a corner and predict*, then simulates the prediction. Its own error is curvature over the box, 0.18 V out of a 0.91 V swing (20%) for VC. A slope error of 8e-4 is 250× smaller. Forward costs N runs per linearization, central 2N.

**Upgrade to central** when a slope is numerically small, |aᵢ·h| < 1000·band, or when a guard asks for a precise slope. **A slope still below its noise after a step of 1e-1 is "numerically zero"**, and gets reported to the model-coverage map (engine.md §6.6: "not modeled" is not "doesn't matter").

**One nudged run serves every measure.** For this file, 1 nominal + 8 nudged runs, each an operating point and its AC points, give the lines for bias, gain and bass together.

### 4.7 Beyond nudging: what the references do (the L1 path)

| | How dx/dp is computed | Captures the bias shift in AC? |
|---|---|---|
| **ngspice `.sens`, DC** | Direct method: re-load *one device's stamp* at p and at p + 1e-6·p, at the converged solution; one solve with the already-factored matrix (`cktsens.c:554-624, 655-677, 701-702`) | n/a |
| **ngspice `.sens`, AC** | The same stamp difference, but only `DEVacLoad` of the perturbed device, with the DC operating point solved once at nominal (`cktsens.c:174, 923-924`) | **No.** And in this tree the per-frequency re-setup seems to linearize at 0 V instead of the operating point (`cktsens.c:373-399` with the MODEINITSMSIG assignment under `#ifdef notdef` at `:179-182`; read from the code, not run) |
| **Xyce `.SENS`** | Direct or adjoint (DC, transient, AC; `N_NLS_Sensitivity.C:883, 1100-1131`; `N_ANP_AC.C:1915, 1990`). dF/dp analytic (hand-written or Sacado AD) where available, else a device-level finite difference with √ε·\|p\| | **Yes.** AC first solves the DC dx/dp, then perturbs x and p together and re-loads G and C (`N_ANP_AC.C:1531-1570`) |
| **Gnucap** | None (only an unused transposed solve, `include/m_matrix.h:783-800`) | — |
| **Director & Rohrer [DR69]** | Adjoint: one transposed solve per *output* gives its slope to every parameter | depends on the implementation |

What a fixed-operating-point AC sensitivity would miss on our amplifier (`fixed_op.py`, the gain's slopes with IC held at nominal):

| Knob | Gain slope, full | At a fixed operating point |
|---|---|---|
| VCC | +0.00596 | **0** |
| R1 | −0.00099 | **0** |
| R2 | +0.00096 | **0** |
| RE | −0.04587 | −0.04508 |
| RC | +0.04592 | +0.04592 |

Temperature and β act on the gain *only* through the device model and the operating point, so a fixed-OP method can't see them at all. Emitter degeneration makes this amplifier forgiving. A stage without degeneration would not be.

Two conclusions:
- **engine.md §6.3 needs a correction:** ngspice `.sens` is usable for DC slopes (direct method), but not for AC specs whose slopes run through the bias. On ngspice, AC specs stay at L0 (nudging). Xyce's AC sensitivities include the shift.
- **A cheap middle step for our own simulator** is ngspice's DC method: difference one device's stamp at the converged solution, then one solve with the factored Jacobian. It has no Newton band at all and costs one linear solve per knob. For AC, take Xyce's approach: DC dx/dp first, then perturb x and p together. This fits roadmap M8 (the parameter layer and sensitivities), after the MVP.

---

## 5. The numerical error band

### 5.1 Where it comes from, per measure

| Measure | Source | Estimate | CE amp, default settings | CE amp, engine settings |
|---|---|---|---|---|
| `dc(output.v)` | Newton stopping early | Norm of the last Newton update | 1.9e-5 V | last update ≤ 4.2e-9 V; true error ≤ 1.1e-13 V (`band_numbers2.py`) |
| | Linear solve | Backward error from the residual (one mat-vec) | ~1e-15 relative | ″ |
| | Output format | float32 spacing, if a file is read | 4.8e-7 V | none (f64 API) |
| `h.at(f).mag()` | The operating point's error, through gm = IC/Vt | Propagate the DC band (or re-evaluate at the last two iterates) | IC off by 3.0e-6 relative → gain off by **5.4e-8** relative (`band_numbers.py`) | ~1e-15 |
| `h.f_low(-3dB)` | Interpolation | Last secant step \|Δ ln f\| | up to 1.3e-2 relative (linear interpolation, 10 per decade) | < 1e-9 (refined) |
| | Operating point | As for `at`, through the IFT | 2.1e-9 relative (∂ln f_L/∂ln IC = 7.0e-4; `band_numbers2.py`) | negligible |

**External backends hide their Newton loop.** On ngspice or LTspice the engine can't read the last update. It can still *measure* the noise: Moré & Wild's ECnoise estimates the noise level from a difference table of 6–8 evaluations along a short line through the point ("Estimating computational noise", 2011, §3, Thm 2.2). That's the M4 way to get δ for the step-size rule of §4.3, and to size the band.

Why the last update is a safe estimate: Newton converges quadratically, so the last step taken is about the error of the iterate it started from. The returned iterate is much better. Checked on the replica: one extra step from the default-converged solution has the same size as that solution's true error (ratio **0.9999–1.0000** over 100 points), and afterwards the true error is ≤ 4.2e-9 V while the step was ≥ 1.1e-5 V (`newton_jumps.py`).

### 5.2 How verdicts use it

1. **Inner bounds.** A simulated worst value w proves FAIL only if it violates the spec edge s by more than its band: |w − s| > k_band·band. Otherwise the verdict is UNDECIDED ("within numerical accuracy"). Take k_band = 10: the band is an estimate, not a bound.
2. **Passes.** The margin must exceed band + the estimated remainder (§1.2). At engine settings, bands are below 1e-9 of the values (VC: 4.2e-9 V out of 5.5 V), so they never decide a 5% spec. The precision study's ppm specs are where they start to matter (`engine_precision_analog.md` §4.8: "a PASS by 2 ppm is never within the solver's own error").
3. **Slopes.** A slope carries its own noise, about 2·band/h for a forward difference. A slope below 1000× that triggers the central-difference upgrade, and then the "numerically zero" flag (§4.6).
4. **The loop's convergence test** ("prediction ≈ simulation", engine.md §5.1 step 4) can't ask for better agreement than the band.
5. **Cross-backend checks** (M4, engine.md §6.4): two backends disagree only if the difference exceeds the sum of their bands. This is also why measurements stay in our library (engine.md D13). ngspice's own `.meas` would give f_low interpolated linearly in Hz, 1e-3 to 1e-2 off ours.

For the MVP, every measure carries a band field. Bands for AC measures come from the root finder and a documented relative floor of 1e-9, rather than full propagation. The machinery matters more than the numbers at this stage.

---

## 6. Where we follow precedent, and where we depart

| Topic | Precedent | This design | Why |
|---|---|---|---|
| Affine forms | de Figueiredo & Stolfi: center, noise symbols, fresh symbol per non-affine op, Chebyshev or min-range lines [Stolfi97] | Same for formula-world forms (M5) | Rigorous; proven on the divider (1% excess vs interval's 21%) |
| Remainder of simulator lines | Worst-case distance: linearize, simulate the worst point, re-linearize [AGW94, Graeb07] | A separate `Observed` remainder kind, never merged into a proven bound | P2: the verdict must say what it rests on |
| Knob coordinates | Xyce "scaled" sensitivity = dO/dp·p/100, per 1% (`N_NLS_Sensitivity.C:930-933`); ngspice absolute (`cktsens.c:747-752`) | ε ∈ [−1, 1] over the knob's range | Coefficient = swing at the edge; zero nominals work; σ = 1/3 for all |
| −3 dB reference | ngspice idiom: max sample − 3 (`x_fer.cir:47-51`); Xyce and Gnucap: nothing built in | max \|H\| over the band, −3 dB literally, nearest crossing below | Matches the idiom; smooth; answers what engineers mean |
| Crossing location | Linear interpolation in linear f, no refinement (ngspice, Xyce, Gnucap) | Bracket on the grid, then secant in ln f to 1e-9 | Interpolation error up to 1.3%; slopes ±20% wrong without refinement |
| No crossing | ngspice: fails, no value; Xyce: FAILED + 0.0; Gnucap: silent huge number | `Beyond` (a one-sided bound, after band extension) or `Undefined` | Never a silent pass; a searched band is evidence |
| Finite-difference slopes | ngspice: forward, 1e-6 relative, on device stamps at a fixed solution; Xyce: forward, √ε·\|p\| | Forward on whole re-simulations, h = 1e-3 in ε, engine-grade Newton | Backend-agnostic (L0 must work on any simulator); larger step because a re-simulation carries Newton's band |
| Newton tolerances | ngspice: reltol 1e-3, vntol 1e-6, abstol 1e-12 + device tests; Xyce: update and residual tests | Engine mode: reltol 1e-6, vntol 1e-9, abstol 1e-12, residual check | 19 µV → 1e-13 V; slopes free of the jump and mixed-start traps |
| AC sensitivity (later) | ngspice: fixed operating point; Xyce: includes the shift; [DR69]: adjoint | Xyce's way, then adjoint | Fixed-OP misses VCC's, T's and β's effect on the gain entirely |

---

## 7. Recommendations for the MVP

**Types** (Rust sketches, not code):

```rust
struct KnobId(u32);
enum Scale { Linear, Log }                       // MVP: Linear only
struct KnobCoord { lo: f64, hi: f64, nominal: f64, scale: Scale }
    // x(ε) = mid + rad·ε ;  ε(x) ;  ε_nominal need not be 0

enum Sym { Knob(KnobId), Anon(u32) }             // Anon: remainder symbols of non-affine ops (M5)

struct Affine {
    center: f64,
    terms: SmallVec<[(Sym, f64); 8]>,            // sorted by Sym, no zero coefficients
    remainder: Remainder,
    band: f64,                                   // numerical band, ≥ 0
}
enum Remainder {
    Exact,                                       // affine ops on exact inputs
    Bound(f64),                                  // proven (formula world, M5)
    Unchecked,                                   // simulator line, no check yet
    Observed { max_miss: f64, points: u32 },     // largest |simulated − line| so far; NOT a bound
}
    // ops: add, sub, scale, offset (exact); eval(point); range(); corner(side);
    //      contributions(worst_point) -> Vec<(KnobId, f64)> + nonlinearity row

enum MeasureValue {
    Value { v: f64, band: f64 },
    Beyond { side: Side, limit: f64 },           // one-sided bound, after band extension
    Undefined { reason: &'static str },
}

struct SlopeConfig { h: f64 /* 1e-3, ε units */, scheme: Forward | Central, warm_start: bool }
```

**Decisions**, each decidable on its own:

1. **Affine forms with sparse, sorted terms and a two-kind symbol (knob / anonymous).** *Reason:* scales to hundreds of knobs, and M5's AA operations then drop in without a type change.
2. **The remainder is an enum (Exact / Bound / Unchecked / Observed), and the numerical band is a separate field.** Only `Bound` or `Exact` may lead to PASS (guaranteed). *Reason:* §1.2. Mixing them would label estimates as proofs.
3. **The MVP implements only the exact operations.** Chebyshev and min-range (worked in §1.5) come with datasheet arithmetic in M5. *Reason:* the MVP's three specs are linearized by nudging whole measures, and AA ops on simulator lines add nothing to first order (§1.5, the P example).
4. **Knob coordinate ε ∈ [−1, 1] via x = mid + rad·ε, with a `Scale` field; Linear for all 8 MVP knobs.** Try log–log in M3c against the answer key. *Reason:* all MVP knobs are symmetric. Log–log cuts the nominal line's miss 4× on VC max and f_low (§1.3), which may save loop rounds, but it interacts with the sigma(k) distributions.
5. **Slopes in ε units, forward differences, h = 1e-3, stepping inward at box edges, warm-started from the nominal solution; one nudged run serves all measures.** Upgrade to central for small slopes; flag slopes that stay below noise as numerically zero. *Reason:* worst-case slope error 7.7e-4 at engine settings, 250× below the loop's own curvature error. Half the runs of central. Warm starts cut Newton work about 7× (§4.4–4.6).
6. **M2e engine mode before M3c:** separate vntol (1e-9 V) and abstol (1e-12 A), reltol 1e-6, a residual check, the last update reported as the band, f64 results through the API. *Reason:* today's defaults return VC 19 µV off and make slopes 25–26% wrong in two realistic situations (§4.4). Engine mode costs +2 iterations cold and nothing warm.
7. **M2e's acceptance test:** forward slopes at h = 1e-3 and 1e-4 agree to 1e-3, and cold-nominal-plus-warm-nudge slopes equal all-cold slopes to 1e-3, for all 8 knobs × 3 measures. *Reason:* it catches both traps. The second check fails today.
8. **`at(f)` requests the exact frequency; the AC grid is computed as f_start·10^(k/N).** *Reason:* no interpolation error. `ac.rs` accumulates `f *= r` today (1000.000000000002 at the 31st point).
9. **f_low(L) = the crossing nearest below argmax |H|, at max|H|·10^(L/20), with L = −3 dB literally.** *Reason:* it matches ngspice's idiom, stays smooth, and matches the engineer's meaning. The answer key must use the same definition: nominal 20.127 Hz, not 1/(2πRC) = 20.079 Hz.
10. **f_low algorithm:** a 20-per-decade grid over [spec bound/100, 100 × the highest `at` frequency], bracketing, then secant in ln f to |Δ ln f| < 1e-9 (Illinois fallback); warn on several crossings. *Reason:* 1e-15 accuracy for about 6 extra AC solves. Without refinement, nudged slopes are ±20% wrong (§3.5).
11. **Measures return Value / Beyond / Undefined.** Extend the band on Beyond. A surviving Beyond decides only from the passing side. Undefined gives UNDECIDED. Non-Value points have no slope. *Reason:* no silent passes (Gnucap's BIGBIG), and DC-coupled designs aren't failed by construction. This refines language.md §8.4.
12. **f_low slopes by the implicit function theorem, including the reference's slope (envelope theorem for the max): 2 AC solves per nudged operating point.** *Reason:* equals a full re-sweep to 5 digits. Without the reference term, RC gets −0.40 instead of 0 (§3.7).
13. **Contributors = slope × worst-point coordinate from the final linearization, grouped range / statistical, plus a nonlinearity row; no R² for nudged lines.** *Reason:* one formula covers worst_case (|a|/Σ|a|) and sigma (variance shares, adding up to k·RSS), and the table adds up to the simulated value. R² = 0.99 coexists with misses the size of the margin (§2.5).
14. **Every MeasureValue carries a band.** DC: the last Newton update. AC: the root finder's step plus a 1e-9 relative floor. Verdicts go UNDECIDED within 10 bands of an edge. *Reason:* costs nothing now, and is needed for ppm specs and for M4's cross-checks.
15. **Record for M4:** on ngspice, use `.sens` only for DC; AC specs stay at L0. *Reason:* ngspice's AC sensitivity holds the operating point fixed (and may linearize at 0 V in this tree), so it misses bias-driven slopes (§4.7). This corrects engine.md §6.3.

---

## 8. Open questions

1. **The f_low reference:** max over the band (recommended), or a reference frequency named in the spec (`f_low(-3dB, ref: 1kHz)`)? Here they differ by 0.04% (20.127 vs 20.119 Hz), but a response with peaking separates them.
2. **Log–log coordinates:** adopt in M3c if they reduce loop rounds on the answer key? How do they interact with sigma(k), where σ is defined in physical units?
3. **Midpoint contributions:** worth N extra runs for a table that nearly adds up (nonlinearity row 0.036 vs 0.182 V)? Or only on request?
4. **Warm starts and reproducibility:** with engine settings, results agree to ~1e-13 across start points, but not bit for bit. Do engine tests compare with a tolerance (as `spicy_simulate`'s numeric snapshots already do), or must every run be cold for bit-identical results?
5. **The band's safety factor** (10× proposed): fixed, or per measure kind?
6. **A cheap L0.5 in our simulator** (ngspice's DC stamp difference, §4.7) before M8? It removes Newton's band from slopes entirely, at one linear solve per knob.
7. **Literal −3 dB or half power?** Recommended: literal (20.127 Hz here), because the argument is a level. The textbook cutoff is half power (−3.01 dB, 20.079 Hz). Should `-3dB` be special-cased, or should the editor show a hint?
8. **Noise estimation on external backends (M4):** adopt Moré & Wild's ECnoise (6–8 extra runs per point) to set δ and the band, or run external backends at tight tolerances and trust a fixed floor?
9. **The analysis band for AC measures:** is [spec bound/100, 100 × highest `at` frequency] a safe default, and how does the engine choose it when a spec has neither (e.g. `bandwidth()`)?

---

## References

- **[Stolfi97]** J. Stolfi & L. H. de Figueiredo, *Self-Validated Numerical Methods and Applications*, 21st Brazilian Mathematics Colloquium, IMPA, 1997 (the title page lists Stolfi first; `bibliography.md` files it as de Figueiredo & Stolfi). Sections used: §3.1 affine forms (p.43); §3.4.1 conversion to intervals, eq. 3.1 (p.48); §3.6.2 rounding (p.51); §3.7 non-affine operations (pp.53–55); §3.8.2 Chebyshev approximations, Theorem 2 (pp.56–57); §3.9 square root (pp.57–63); §3.10 min-range (pp.64–66); §3.12 reciprocal (pp.69–70); §3.13 multiplication, eq. 3.18 (pp.70–74); §3.17.1 storage sorted by symbol index (p.79).
- de Figueiredo & Stolfi, "Affine arithmetic: concepts and applications", *Numer. Algorithms* 37:147–158, 2004, doi:10.1023/B:NUMA.0000049462.70970.b6 (§3.3 Chebyshev vs min-range; §3.4 multiplication). Also Stolfi & de Figueiredo, "An introduction to affine arithmetic", *TEMA* 4(3):297–312, 2003.
- **[AGW94]** Antreich, Graeb & Wieser, *IEEE TCAD* 13(1):57–71, 1994. The worst-case point; its linearized form ε* = k·σ²a/‖σa‖.
- **[Graeb07]** Graeb, *Analog Design Centering and Sizing*, Springer 2007.
- **[DR69]** Director & Rohrer, "The generalized adjoint network and network sensitivities", *IEEE Trans. Circuit Theory* 16(3):318–323, Aug. 1969, doi:10.1109/TCT.1969.1082965.
- **[Xyce16]** Keiter, Swiler, Russo & Wilcox, "Sensitivity Analysis in Xyce", SAND2016-9437, 2016 (OSTI 1562422): DC direct and adjoint (ch. 2), transient (ch. 3–4), device derivatives (ch. 5, p.41: finite differences are "the least desirable method"). Also the Xyce Users' Guide 7.9, §7.10.
- ngspice manual, version 47: §11.3.7 `.SENS` (pp.333–334) and §1.2.6 (pp.41–42: sensitivities by "perturbing each parameter of each device independently").
- Nocedal & Wright, *Numerical Optimization*, 2nd ed., Springer 2006: §8.1, eq. 8.1 and 8.5–8.7 (pp.194–197); §9.1, Lemma 9.1, eq. 9.5 (pp.221–222).
- Gill, Murray & Wright, *Practical Optimization*, Academic Press 1981: §4.6.1 and §8.6, eq. 8.44 (read from a garbled scan; see §4.3).
- Moré & Wild, "Estimating computational noise", *SIAM J. Sci. Comput.* 33(3):1292–1314, 2011; "Estimating derivatives of noisy simulations", *ACM TOMS* 38(3):19, 2012 (equation numbers from the preprints ANL/MCS-P1721 and P1785).
- Danskin, "The theory of max-min, with applications", *SIAM J. Appl. Math.* 14(4):641–664, 1966 (the envelope theorem used in §3.7).
- Reference sources (file:line as cited): ngspice (`/tmp/refs/ngspice`), Xyce (`/tmp/refs/xyce`), Gnucap (`/tmp/refs/gnucap`); our simulator `crates/spicy_simulate`.
