# The Worst-Point Loop for `worst_case` Confidence, Specified

> 2026-09-27 · Research for roadmap M3c (`spicy_engine`). Companion to `../engine.md` §5 and `../walkthrough.md` §5.
> Every number here was computed with the scripts in `/root/.claude/jobs/443154a8/tmp/engine_research/worst_case/` (listed at the end), on the walkthrough amplifier's reference model, or is cited with its section.
> Sibling reports cover the statistical `sigma(k)` search, affine forms and step sizes, and guards and the safety net. This one covers only the loop.

---

## Summary

For a `worst_case` spec the engine must find the largest and smallest value of a measure over the whole tolerance box. It must report the best value it actually simulated (the **inner bound**, which proves failures) and an estimate of how far beyond that the truth could lie (the **estimated outer bound**, which supports passes). The loop specified here is a **monotone vertex walk**. It starts with the textbook one-jump worst case: push every knob to the edge its nominal slope points to (Graeb's "classical worst-case analysis"). It simulates that point, re-measures the slopes there, and flips every knob whose slope now points the other way. It only ever moves to a point that simulates *better*: a rejected proposal halves the number of knobs it may flip, and a single rejected flip triggers a search for an interior maximum. It stops when no slope points away from where its knob sits, which is the first-order optimality condition for a box. It then flips jointly the few knobs whose slopes it can't vouch for.

On the CE amplifier the loop finds all five extremes **exactly** (VC 4.6597…6.5905 V, gain 4.4626…4.7030, f_L max 26.738 Hz) in **71 simulations**, against 256 for brute force. That includes walkthrough §5.7's trap: step 1 lands on the hot corner (4.4688), the slopes there say "cold", and step 2 finds 4.4626. The estimated outer bound is "inner ± the largest prediction error the loop saw". It reproduces the walkthrough's 4.59 V, and it covered the true answer in all 75 cases of a noise stress test. But no local rule can see a limiter that is off at nominal. On a variant that saturates at some corners the loop alone reports a confident gain of 6.46 where the truth is 0. A regime-margin guard, searched like any spec, catches it with one extra run.

---

## 1. The problem in math

### 1.1 Setup

- **Knobs.** Each knob i has a position εᵢ ∈ [−1, 1]: −1 is its low edge, +1 its high edge, 0 nominal. The physical value is linear between the edges, as in walkthrough §5.3. For β, ε = −1, 0, +1 mean β = 100, 200, 300.
- **The box.** X = [−1, 1]ⁿ. For the MVP n = 8: temp and vcc.v (range), r1, r2, rc, re, c_in, q1.beta (statistical). Under `worst_case` all eight are treated the same: any value in the box can happen.
- **A run** is one simulation at one point of X. It returns *every* measure of the analyses it ran (for the MVP: DC operating point + AC sweep → VC, |H(1 kHz)|, f_L).
- **A spec side.** `f <= hi` needs M = max over X of f. `f >= lo` needs m = min over X of f. `f in lo..=hi` needs both. Below, everything is written for a maximum; a minimum is the maximum of −f.

### 1.2 The three numbers the engine reports per side

| Number | Definition | What it proves |
|---|---|---|
| **Inner bound** I | max f(s) over *every* point s simulated during the check, by any search, including nudges and safety-net corners | I ≤ M always. If I > hi + B, the spec **fails**, and s is the counterexample |
| **Estimated outer bound** U | U = I + E + R + B (defined in §2.8) | An estimate that M ≤ U. It is **not** a guarantee: it assumes the measure is smooth and has one worst region, which the guards check |
| **True extreme** M | max over X of f | Unknown in general; known here only by brute force |

B is the numerical error band of the simulator and measurement (the guards and step-size reports define it; it is 0 on the reference model).

**Verdict for an upper side** (a lower side mirrors it):

| Condition | Verdict |
|---|---|
| I > hi + B | **FAIL**, with the counterexample |
| U ≤ hi and no guard fires | **PASS (estimated)** |
| otherwise | **UNDECIDED** |

### 1.3 Why no algorithm can promise M cheaply

Finding the maximum of a smooth function over a box is NP-hard, even for a quadratic. Maximizing a convex quadratic xᵀLx over [−1, 1]ⁿ is MAX-CUT when L is a graph Laplacian (a convex function peaks at a vertex, so the box and the vertex set give the same answer). The related result that quadratic programming with a single negative eigenvalue is NP-hard is Pardalos & Vavasis, *J. Global Optim.* 1:15–22, 1991. So every practical method is local plus safeguards. That is exactly the split in `engine.md`: the loop is local, FAIL is definite, PASS is estimated, and the guards look elsewhere.

### 1.4 What "converged" means: the box optimality condition

Let gᵢ be the slope ∂f/∂εᵢ at a point x. x is a **first-order local maximum** of f over the box when, for every knob:

```
 knob at its high edge (εᵢ = +1):  gᵢ ≥ 0      the slope says "higher would be better", but the edge stops it
 knob at its low edge  (εᵢ = −1):  gᵢ ≤ 0
 knob inside           (−1 < εᵢ < 1): gᵢ = 0
```

These are the Karush–Kuhn–Tucker conditions for box constraints. For a *linear* f the only point meeting them is the vertex εᵢ = sign(gᵢ), which is the exact maximum. That is Graeb's classical worst-case analysis ([Graeb TUM] §4.4, eqs. 82 and 85; details in §5). For a nonlinear f the conditions hold at every local maximum, not only the global one. The loop stops exactly when they hold with the slopes measured *at the point itself*.

Example: VC max. At the vertex (T−, VCC+, R1+, R2−, RC−, RE+, β−) the slopes are T −0.407, VCC +0.177, R1 +0.074, R2 −0.070, RC −0.061, RE +0.053, β −0.583 V per unit ε. Every sign matches its edge, so the vertex is a local maximum. (C_in doesn't enter a DC analysis.) Brute force confirms it is also the global one: 6.5905 V.

---

## 2. The algorithm

### 2.1 The whole check on one page

```
 ┌───────────────────────── shared by every spec ─────────────────────────┐
 │ safety net: range corners + temperature points      (guards report)    │
 │ nominal run + one nudge per knob  →  nominal slopes of EVERY measure   │
 └─────────────────────────────────────────────────────────────────────────┘
        │  then, for each side of each spec (independent, can run in parallel):
        ▼
 start at the vertex the nominal slopes point to           ← Graeb's classical WCA
        │
        ▼
 ┌──► propose: every knob whose slope points away from where it sits moves
 │    to the other edge (at most `budget` knobs, largest predicted gain first)
 │    simulate ── better than the best so far? ── yes ──► accept; re-measure the
 │        │                                               slopes there; budget = all
 │        no
 │        ├── budget > 1:  halve it and propose again
 │        └── budget = 1:  that knob's slope points inward but its other edge is
 │                         worse → look for an interior maximum along it (1-D),
 │                         otherwise mark it "settled" here
 └─── repeat until no knob wants to move                   ← the box optimality condition
        │
        ▼
 ambiguity: flip jointly the knobs whose slopes can't be trusted (≤ 4 knobs → ≤ 15 runs)
        │
        ▼
 inner I = best run anywhere · estimated outer U = I + E + R + B · verdict
        │
        ▼
 cross-check: if any run of the whole check beats a side's answer, restart that side there
```

### 2.2 Pseudocode for one side

```
inputs   measure f, sense s (+1 for a maximum, −1 for a minimum)
         shared cache: knob vector → every measure; the nominal run x0 and nominal slopes g0
params   h      nudge size in ε units (0.01 here; from the step-size report)
         τ      "flat" slope threshold = 3·σ_f / h, with σ_f the simulator's noise on f
         K      max knobs flipped jointly in the ambiguity step (4)
         CAP    max proposals per side (12)
         δ      smallest improvement worth a run (1e-4 × the spec window)

φ(x) := s · f(x)                         # always maximize φ

# 1. start
x★, φ★, g := x0, φ(x0), s·g0            # x★ is the incumbent: the best point so far
g_prev := g
moves := improving_moves(x★, g)
         ∪ { i → edge picked by sign(gᵢ) : |gᵢ| ≤ τ and knob i is not structural }
budget := |moves|;  visited := {x0};  E := 0;  settled := ∅

# 2. walk
while moves ≠ ∅ and proposals < CAP:
    P    := the `budget` moves with the largest predicted gain gᵢ·(tᵢ − x★ᵢ)
    x    := x★ with the moves in P applied
    pred := φ★ + Σ_{i∈P} gᵢ·(tᵢ − x★ᵢ)  [+ cᵢ·(tᵢ − x★ᵢ)² for knobs with a known curvature cᵢ]
    if pred − φ★ ≤ δ: break                              # nothing left worth a run
    y := φ(x)                                             # a cache hit costs nothing
    if x ∉ visited: E := max(E, |y − pred|)               # the model's largest observed miss
    visited += {x}
    if y > φ★ + δ:                                        # ACCEPT: the only way x★ changes
        g_prev := g;  x★, φ★ := x, y
        g := slopes(x★)                                   # one nudge per relevant knob, inward at edges
        settled := ∅;  moves := improving_moves(x★, g);  budget := |moves|
    elif budget > 1:
        budget := ⌊budget / 2⌋                            # trust region shrinks: fewer flips
    else:                                                 # the single move of knob i failed
        fit q(t) through φ★, the slope gᵢ at x★, and y at the other edge
        if q has a maximum t* inside and φ(x★ with εᵢ = t*) > φ★:
            accept it; remember the curvature cᵢ; re-measure the slopes
        else:
            settled += {i}
        moves := improving_moves(x★, g) minus settled;  budget := |moves|

# 3. ambiguity: flips the slopes can't rule out
A := { knobs i at an edge, not structural : |gᵢ| ≤ τ  or  |gᵢ| ≤ |gᵢ − g_prev,ᵢ| }
if |A| ≤ K:  simulate the 2^|A| − 1 joint flips of A around x★;
             if one is better: accept it, re-measure the slopes, go back to step 2
else:        R := Σ_{i∈A} 2·|gᵢ|                          # lumped remainder, never dropped

# 4. report (after every side has run)
I := s · max φ over ALL runs of the check that lie in the box
U := s · (φ★ + E + R + B)

improving_moves(x, g) = for each knob i with |gᵢ| > τ, not settled:
    inside the box with a known curvature cᵢ < 0:  t = clip(xᵢ − gᵢ/(2cᵢ), −1, 1)   (1-D Newton step)
    gᵢ > 0 and xᵢ ≠ +1:  move to +1
    gᵢ < 0 and xᵢ ≠ −1:  move to −1
```

The reference implementation is `wcloop.py` (about 320 lines of Python). The rest of this section explains each rule and why it's there.

### 2.3 The start: Graeb's classical worst case

The first proposal is Graeb's classical worst-case analysis ([Graeb TUM] §4.4, eq. 85): each knob goes to the edge its nominal slope points to. It's also what PSpice's `.WCASE`, Multisim and ECSS extreme-value analysis do (§5). Two refinements:

- **Structural zeros stay nominal.** A knob that *cannot* enter the analysis is excluded, with no runs spent on it. For the MVP the only rule is "capacitor and inductor values don't enter a DC operating point", so C_in is skipped in the VC searches. This must come from the circuit, **not** from a bit-identical nudge. A limiter that is off at nominal also gives an exactly-zero slope (variant A, §4.1), and treating it as "irrelevant" is precisely the false-PASS mechanism.
- **Flat knobs go to an edge anyway.** A knob with a noise-level slope (|g| ≤ τ) goes to the edge its sign picks. Since its effect is unknown, the ambiguity step (§2.6) later tries the other edge.

### 2.4 The step, and why the incumbent never gets worse

**Propose.** Every knob whose slope points away from its current edge flips; interior knobs with a known curvature take a Newton step. For a linear model this jump is exact: it lands on the model's own optimum.

**Accept only improvements.** The best point so far (x★) changes only when a proposal simulates better by more than δ. This is the acceptance test of every trust-region method (the "ratio test": accept when the real improvement is positive).

It matters. The plain fixed-point loop re-linearizes wherever the model pointed, even when that point is worse, and it can cycle. `engine.md` §5.2 reports cycling on the boost converter. Measured here (`plain_fixed_point.py`):

| Case | Plain fixed-point loop | Monotone loop (this spec) | Brute force |
|---|---|---|---|
| CE amp VC max | fixed point, 6.5905 | 6.5905 | 6.5905 |
| CE amp gain min | fixed point, 4.4626 | 4.4626 | 4.4626 |
| Variant A, VC min (saturation) | **cycles** (period 2), best 1.8150 | 1.6508 | 1.6508 |
| Variant B, transistor power | **cycles** (period 2), best 9.2244 | 9.3156 (interior) | 9.3147 (corners) |

Accepting only improvements also gives **termination for free**: the incumbent strictly improves, vertices are finite, and a visited point is never proposed again. `engine.md`'s "cycle detection" is therefore not needed as a separate mechanism. The visited set plays its role: a proposal that was already simulated is known not to be better, and costs nothing. The cap (12 proposals) bounds the interior case.

**The flip budget.** When a jump fails, the model was wrong about at least one of the flipped knobs. Halving the number of flips (keeping the largest predicted gains) is a trust region measured in knobs instead of distance. At budget 1 it becomes Graeb's active-set rule ([Graeb TUM] §6.2): release the one constraint with the most negative Lagrange multiplier, which here is the knob with the largest predicted gain.

### 2.5 Interior points: only on evidence

The box maximum of a *linear* model is always a vertex. So the loop leaves the edges only when the simulations prove there is something inside.

**The trigger.** A single-knob move fails: knob i's slope points inward (moving inward improves f), but its other edge simulates worse. Then f's maximum along that edge-to-edge segment is strictly inside it. Just inside the current edge f is higher than at the edge, and the far edge is lower still, so neither end can be the maximum.

**How it's found cheaply.** Three numbers are already known: the value at the current edge, the slope there, and the value at the far edge. They fix a parabola q(t). Its peak t* is simulated (**one run**). If better, it's accepted, its curvature cᵢ is kept, and later steps along that knob are Newton steps t − gᵢ/(2cᵢ). If not, the knob is "settled" for this incumbent.

This is the one-dimensional core of a bound-constrained quasi-Newton method such as L-BFGS-B (Byrd, Lu, Nocedal & Zhu, *SIAM J. Sci. Comput.* 16:1190–1208, 1995). The full method is the upgrade path when interior worst cases become common: op-amp phase margin vs capacitive load, power, resonances (`synthesis.md` E10). For the MVP the CE amp needs no interior point (§3), and variant B (§4.2) shows the mechanism and its cost.

### 2.6 Ambiguity: which joint flips to test

`engine.md` §5.1 says "test joint flips of near-zero-slope knobs" without saying which. A knob at an edge is **ambiguous** when either:

1. its slope is at the noise floor (|gᵢ| ≤ τ), so its sign means nothing; or
2. **its slope changed by more than its own size** since the previous linearization: |gᵢ| ≤ |gᵢ − g_prev,ᵢ|. Such a slope could plausibly reverse somewhere between the two points, so the edge isn't secure. A slope that reversed sign always meets this test.

The ambiguous knobs (at most K = 4) are flipped jointly, which costs at most 15 runs. With more than 4, they are not enumerated; their linear worst effect Σ 2|gᵢ| is added to the outer bound as a lumped remainder R. That follows the red team's "bounded remainder term that is kept, never dropped" (`engine_method_redteam.md` §2.4).

**Worked example: S1 min.** Nominal β slope −0.140 V; at the VC-min vertex −0.057 V. The change, 0.083 V, is larger than the slope itself, because VC's dependence on β flattens as β grows. So β is ambiguous. One run flips it back to β = 100: VC 4.9856 V, not lower, so the vertex stands. Every other knob is secure: T goes from −0.354 to −0.341 (change 0.013 ≪ 0.341), and so on.

**Why not the obvious rule** ("flip every knob whose predicted loss 2|gᵢ| is below the model error E")? I tried it first. On S1 max it flagged R1, R2, RC and RE (2|g| ≈ 0.11–0.15 V < E = 0.18 V) and spent **15 wasted runs**. Over the whole CE amp it spent 29 joint-flip runs (96 in total instead of 71). On S3 it flagged six knobs, too many to enumerate, so it lumped them into R and pushed the outer bound to 30.21 Hz: a needless UNDECIDED. The whole-box error E is driven by β's curvature and says nothing about R1's edge. The slope-stability test is per knob and uses information already paid for. On the CE amp it flagged one knob (β in S1 min, 1 run). The flat rule added 3 runs for knobs this simplified model makes exactly flat (C_in for gain, RC for f_L); in the real circuit those slopes are small but non-zero.

### 2.7 "Prediction ≈ simulation": what it's for

`engine.md` §5.1 says to "repeat until prediction ≈ simulation". I recommend splitting that into three separate jobs, each with a precise rule:

| Job | Rule |
|---|---|
| **Termination** | The box optimality condition holds with the slopes at x★ (§1.4). Nothing about prediction error |
| **Accepting a step** | Simulated improvement > δ (the trust-region test) |
| **Tolerance on the model** | The model's largest observed miss E goes into the outer bound. The verdict then asks "is the margin larger than E?", which is the operational meaning of "prediction ≈ simulation within a tolerance", in spec units |

Why not terminate on agreement? Because agreement isn't the condition that matters. On S1 max the first jump missed by 0.18 V (prediction 6.4076, simulation 6.5905), yet the vertex it reached is the exact answer. On variant C (§4.3) the prediction matches to 0.07 V at a vertex that is 0.17 V away from the truth. "Prediction ≈ simulation" is never treated as proof (`engine.md` §5.2); it only measures how curved the measure is.

### 2.8 The estimated outer bound, numerically

```
 U = I + E + R + B            (for a lower side:  L = I − E − R − B)

 I  best simulated value (inner bound)
 E  largest |simulation − linear prediction| over this side's proposals
 R  lumped remainder of ambiguous knobs that were not enumerated (0 if ≤ K)
 B  numerical band of simulator + measurement
```

Properties:

- **Exact for linear measures.** A linear f is predicted perfectly, so E = 0 and U = I = M: inner = outer, as the corner theorem gives for linear pieces.
- **Reproduces the walkthrough.** For VC min the nominal line predicted 4.5922 V and the simulation gave 4.6597 V. So E = 0.0675 and L = 4.6597 − 0.0675 = 4.5922 V. That is walkthrough §4's "≈ 4.59 (estimated)".
- **Grows with nonlinearity.** For f_L, whose 1/C curve is strong, E = 1.93 Hz. For gain, which is ≈ RC/RE and nearly linear, E = 0.003–0.009.
- **Covered the truth in 75 of 75 noise cases** (§3.6), including the 10 where noise left the inner bound more than 0.1% away from the truth.
- **Not a guarantee.** It cannot cover a miss the local model never saw. In variant A the outer bound for gain min is 6.44 while the truth is 0; in variant C it is 4.27 against 4.18. Those are the guards' job (§4), and a verdict whose guards fire is at best UNDECIDED.

Precedent: neither Graeb, ECSS nor any tool surveyed reports an outer bound for the loop. They report the simulated worst case (§5). The quantity is ours, required by principle P2, and labeled "estimated" everywhere.

### 2.9 How sides and specs share runs

1. **Every run returns every measure** and goes into one cache keyed by the exact knob vector. No point is ever simulated twice.
2. **Shared up front:** the safety net and the nominal linearization. On the CE amp that's 6 + 9 = 15 of the 71 runs, serving all five searches.
3. **Min and max of one measure** share only that nominal linearization: their walks start at opposite vertices. A two-sided spec costs about twice a one-sided one.
4. **Different measures share whenever their worst vertices coincide.** Guards and the specs they protect often share one. In variant A the VCE-margin guard's worst vertex is the VC-min vertex, so the guard cost **1** new run instead of 9. In variant C, `down` min reused `swing` min entirely (0 new runs).
5. **The inner bound of each side is taken over all runs of the check**, not just its own walk. The final **cross-check** compares them. In variant A, gain min's walk stopped at 6.4565, but a run made for the VC-min search sits at gain 0.0000: a FAIL with a counterexample, found for free.
6. **Batching.** A linearization's n nudges are independent, so they go to the backend as one batch (`engine.md` §6.1 `run(points)`). A proposed vertex and its n nudges can be sent together speculatively; the nudges are wasted only if the vertex is rejected. With that, the CE amp's 71 runs need 4 sequential batches.

---

## 3. Worked example: the CE amplifier

The model is `ce_amp_model.py` (walkthrough appendix, 8 MVP knobs, no aging). Nudges are forward, h = 0.01 in ε units (1% of each half-range), inward at an edge. The stop threshold δ is 0 in the logged run; with δ = 1e-4 × the spec window every step, answer and run count below is unchanged. Script: `run_ce_amp.py`; full run log in `out_forward_L0.txt`.

### 3.1 Shared runs (15)

| Runs | What | Result |
|---|---|---|
| 1–4 | Range corners (T, VCC) ∈ {±1}², parts nominal | VC 5.02…6.00 V · gain 4.584…4.598 · f_L 19.96…20.25 Hz |
| 5–6 | Temperature points 7.5 °C and 42.5 °C | Nothing interior (VC 5.68, 5.32 V) |
| 7 | Nominal | VC 5.4999 V · gain 4.5917 · f_L 20.079 Hz |
| 8–15 | One nudge per knob | Slopes of all three measures, below |

Nominal slopes, change per unit ε (from −1 to +1 is twice this):

| | T | VCC | R1 | R2 | RC | RE | C_in | β |
|---|---|---|---|---|---|---|---|---|
| VC (V) | −0.354 | +0.135 | +0.077 | −0.075 | −0.065 | +0.061 | 0 | −0.140 |
| gain | −0.0013 | +0.0060 | −0.0010 | +0.0010 | +0.0459 | −0.0459 | 0 | +0.0130 |
| f_L (Hz) | −0.136 | +0.001 | −0.034 | −0.159 | 0 | −0.008 | −4.008 | −0.385 |

These match walkthrough §5.2 to its rounding (T 0.354 V; β 0.140 here vs 0.141 V there; …).

### 3.2 S1 bias: VC max and min

**VC max (8 new runs):**

| Step | Moves | Line predicts | Simulation | Action |
|---|---|---|---|---|
| 1 | T−, β−, VCC+, R1+, R2−, RC−, RE+ | 6.4076 V | **6.5905 V** | accept; 7 nudges there (C_in skipped: DC) |
| — | slopes at the vertex all agree with their edges | — | — | converged |
| ambiguity | slope stability: every knob secure (β −0.140 → −0.583: the change 0.443 < 0.583) | — | — | none |

Inner 6.5905 V = brute force. E = 0.1828, so U = 6.7733 V. **6.5905 > 6.5 → FAIL**, counterexample T = −10 °C, VCC = 12.6 V, R1 +1%, R2 −1%, RC −1%, RE +1%, β = 100. That's the walkthrough §5.5 board.

**VC min (9 new runs):** step 1 (T+, β+, VCC−, R1−, R2+, RC+, RE−) predicts 4.5922 V and simulates **4.6597 V**; converged. β is ambiguous (§2.6), so one joint-flip run gives 4.9856 V, not lower. Inner 4.6597 V = brute force; L = 4.5922 V ≥ 4.5 → the low side passes (estimated).

### 3.3 S2 gain: the trap of walkthrough §5.7

**Gain max (10 new runs):** step 1 predicts 4.7057 and simulates **4.7030** = brute force. C_in's gain slope is exactly 0 in this model (the mid-band formula ignores it), so C_in is "flat". One joint-flip run gives no change. U = 4.7057.

**Gain min (19 new runs):**

| Step | Moves | Line predicts | Simulation | Action |
|---|---|---|---|---|
| 1 | RC−, RE+, β−, VCC−, **T+**, R1+, R2− | 4.4778 | 4.4688 | accept; 8 nudges: **the T slope has flipped** |
| 2 | **T−** | 4.4666 | **4.4626** | accept; 8 nudges: all agree → converged |
| ambiguity | C_in flat → 1 run, no change | | | |

Why T flips, with the other knobs at the step-1 settings:

```
                         β = 100              β = 200           β = 300
   T = 60 °C  (hot)      4.4688  ◄─ step 1    4.4907            4.4981
   T = −10 °C (cold)     4.4626  ◄─ step 2    4.4933            4.5036
                         cold is worse        hot is worse      hot is worse
```

The gain slope for T is −0.0013 per unit at nominal, +0.0011 at step 1 and +0.0061 at step 2 (`out_slopes.txt`). The nominal line picks "hot" (4.4688, as in the walkthrough). One re-linearization at that corner reveals the flip, and the loop lands on the true worst corner, 4.4626. E = 0.0090, L = 4.4536. Both sides lie inside 4.37…4.83 → **PASS (estimated)**.

### 3.4 S3 bass: f_L max (10 new runs)

Step 1 (C−, β−, R2−, T−, R1−, RE−, VCC+) predicts 24.81 Hz and simulates **26.738 Hz** = brute force (walkthrough §6's "day 1, absolute worst 26.7 Hz"). The line was 1.93 Hz optimistic, the 1/C curve again. Converged at once. RC is flat for f_L (1 run). U = 26.738 + 1.928 = 28.67 Hz ≤ 30 → **PASS (estimated)**, with 1.3 Hz of margin beyond the allowance.

### 3.5 Scorecard

| Side | Loop inner | Est. outer | Brute force (256 corners) | New runs | Steps | Verdict |
|---|---|---|---|---|---|---|
| S1 VC max | 6.5905 | 6.7733 | 6.5905 | 8 | 1 | **FAIL** (counterexample) |
| S1 VC min | 4.6597 | 4.5922 | 4.6597 | 9 | 1 | low side passes |
| S2 gain max | 4.7030 | 4.7057 | 4.7030 | 10 | 1 | **PASS (est.)** |
| S2 gain min | 4.4626 | 4.4536 | 4.4626 | 19 | 2 | ″ |
| S3 f_L max | 26.738 | 28.666 | 26.738 | 10 | 1 | **PASS (est.)** |
| **Total** | | | | **71** incl. 15 shared | | agrees with brute force |

The three MVP measures are corner-dominated: 20,000 random interior points never beat the best corner. Random sampling is a poor worst-case finder here: its best VC was 6.338 V against 6.5905 V at the corner. The final cross-check found no run elsewhere that beat any side. An optional **flip certificate** (`flip_certificate.py`: simulate each single-knob flip at the answer, n runs) confirms all five answers are single-flip optimal by simulation, not just by slopes.

**Cost by slope method** (same answers in every case):

| Slopes | Total runs | Of which shared | Per side |
|---|---|---|---|
| L0 forward nudges (recommended) | **71** | 15 | 8–19 |
| L0 central nudges | 125 | 23 | 15–35 |
| L1 (slopes free, emulated) | 17 | 7 | 1–3 |

### 3.6 Sensitivity to simulator noise

A real simulator's outputs carry noise of about its tolerance. `noise_test.py` adds deterministic pseudo-noise of relative size 1e-7 … 1e-4 to every measure and reruns everything at three nudge sizes, with τ = 3σ_f/h:

"Right" means the loop reached the true worst vertex; its value then differs from the key only by the injected noise.

| Noise | h = 0.001 | h = 0.01 | h = 0.1 |
|---|---|---|---|
| 1e-7 | all 5 right | all 5 right | all 5 right |
| 1e-6 | gain max and min off (4.7013, 4.4709); outers cover the truth | all 5 right | all 5 right |
| 1e-5 | 3 off; outers wide (gain UNDECIDED) | all 5 right | all 5 right |
| 1e-4 | 4 off; outers very wide | 2 off (gain UNDECIDED) | all 5 right |

Of 75 side-cases, **the estimated outer bound covered the true value in all 75**. When noise hides the slopes the loop stops early, but the verdict widens to UNDECIDED instead of a false PASS. Without the noise-based τ, noise-level slopes get random signs and the loop wanders (up to 184 runs). The takeaway for the step-size report: nudges must be large enough that noise/h stays well below the smallest slope that matters (gain's T slope is 0.0013 per unit).

---

## 4. Where the loop alone fails

Three small variants of the same model (`variants.py`). Each is built to break one assumption.

### 4.1 A: a limiter that is off at nominal (saturation)

RC = 6.8k, and the model gets a soft VCE,sat knee (0.2 V, 20 mV wide). The small-signal gain scales with dVCE/dVCE,active, so it collapses in saturation. Nominal VCE = 1.21 V and gain = 6.64. The range corners (nominal parts) stay active, with VCE ≥ 0.45 V, but **10 of the 256 corners saturate**.

| Search | Loop alone | Brute force | Why |
|---|---|---|---|
| gain min | 6.4565, PASS (est.), outer 6.4434 | **0.0000** at T+ VCC+ R1− R2+ RC+ RE− β+ | Nominal gain slopes say "RC low, RE high", which is away from saturation. The collapse has zero slope at nominal |
| safety net | range-corner gains 6.63…6.65 | — | Saturation needs the statistical knobs too: **the safety net misses it** |
| VCE-margin guard (min of VCE,active − VCE,sat) | −0.3868 V: **saturation reachable**. 1 new run (its vertex was already simulated by VC min) | −0.3868 | A smooth margin keeps a slope at nominal, so the ordinary loop finds it |
| cross-check | gain at that guard's vertex = 0.0000 → **FAIL** with counterexample | | Every run serves every spec |

So the regime guard (red team §2.3, Graeb's "sizing rules") plus the cross-check turn a confident false PASS into a definite FAIL for the cost of one run. The walk on VC min in this variant also shows the outer bound doing its job honestly: the knee makes the line miss by up to 2.27 V, so E = 2.27 and the "outer bound" is meaningless (−2.16 V). The inner bound still matches the 256-corner brute force (1.6508 V). The cross-check even found an interior point slightly lower (1.6500 V, a nudge run), because the clamp makes VC non-monotone in VCC there.

### 4.2 B: an interior maximum (transistor power)

P = IC·VCE with RC = 3.3k. P(IC) is a parabola whose peak VCC²/(4(RC + RE(1+1/β))) sits inside the IC range the knobs produce. So the maximum is a ridge through the interior: any combination of T, R1 and R2 that puts IC at the peak. Two points on it are T ≈ 24 °C with R2 at its low edge, and T ≈ 9 °C with R2 at +0.78.

| Method | P max | Runs |
|---|---|---|
| True (grid over the interior knobs) | 9.3162 mW | — |
| **Loop**, stop rule δ = 1e-4 relative | 9.3156 mW, interior T = −0.47, R2 = +0.78 | 43 |
| Loop without a stop rule | 9.3157 mW, hits the cap zig-zagging along the ridge | 86 |
| Brute force, 256 corners | 9.3147 mW | 256 |
| 20,000 random points | 9.2910 mW | 20,000 |

The interior rule works: two single flips fail, two 1-D searches find the ridge. It costs 4–5× a vertex search, and the ridge (T and R2 both just set IC) makes coordinate steps zig-zag. Two lessons follow. For a measure like power, **256 corners are not the answer key**. And a joint quadratic model in the interior knobs (L-BFGS-B-like) is the upgrade when this becomes common. None of the MVP's three measures behaves like this.

### 4.3 C: a kink where slope signs lead to the wrong vertex

A "symmetric swing" measure: swing = min(VCC − VC, VC − VE − VCE,sat), with RC = 3.8k. At nominal the lower headroom is the smaller one (5.155 vs 5.255 V), so the swing's slopes are the lower headroom's slopes. The loop follows them to that piece's worst vertex, where that piece still binds. There the box optimality condition holds and the prediction agrees to 0.07 V.

| Search | Result | Brute force |
|---|---|---|
| swing min, loop alone | **4.3407 V**, outer 4.2725 | **4.1757 V** at the *opposite* vertex |
| each piece searched separately (8 more runs) | up: 4.1757 · down: 4.3407 → min = 4.1757 | ✓ |

A spec `swing >= 4.25V` would get a false PASS (estimated). The fix is structural, not numerical: a measure written as min/max of pieces is searched **piece by piece** (red team §2.1, "max-type measures as minimax"). The measurement library knows the pieces because the engineer wrote `min(...)`. The same holds for any measure with an internal max: over frequency, over time, over several peaks.

### 4.4 Summary

| Variant | Assumption broken | Loop alone | Caught by |
|---|---|---|---|
| A saturation | the slope sees every knob that matters | false PASS (6.46 vs 0) | regime-margin guard + cross-check (1 run). Safety net: **no** |
| B interior power | the worst case is a vertex | correct (9.3156), but 43–86 runs | interior rule; answer key needs an interior search |
| C kinked swing | the measure is smooth | false PASS (4.34 vs 4.18) | searching the pieces separately |

What I need from the guards report, in one line: the list of regime-margin measures per device (VCE − VCE,sat here), searched by this same loop as ordinary sides, and the verdict rule when one is reachable.

---

## 5. Grounding: what the sources say, and where we depart

### 5.1 Graeb: the formulation and the iteration

Sources: Graeb, *Analog Design Centering and Sizing*, Springer 2007, ch. 5 "Worst-Case Analysis", pp. 85–106 (chapter and pages confirmed via Crossref, doi:10.1007/978-1-4020-6004-5_5; body not read). Every statement below is read in Graeb's TUM textbook *Mathematical Methods of Circuit Design* (v3.3, May 2024), which says its presentation "follows" the 2007 book. It is cited as [Graeb TUM].

| Graeb | What it says | Our use |
|---|---|---|
| ch. 4 intro | "Individual worst-case analysis for specific performance features instead of one-fits-all worst-case parameter sets … is one of the main messages of this book" | One search per side of each spec |
| §4.1, eq. 69 | Worst-case analysis = min / max φ(x) subject to x in the tolerance region → worst-case parameter set and value | §1.2 |
| §4.2 | Boxes suit range parameters and the tolerances of discrete parameters ("their values are mainly at the end of the interval due to the manufacturing testing"); ellipsoids suit statistical ones | `worst_case` = box for all; `sigma(k)` = ellipsoid (sibling report) |
| §4.3, eqs. 70–74 | Linear model from forward finite differences. The step must not be "swallowed by the noise" nor "deviate significantly from the gradient". A deliberately wide step gives a "broader trend" | Nudges; τ from the noise (§3.6) |
| §4.4, eqs. 82, 85 | Classical WCA: the sign of each gradient component picks the bound; **a zero gradient leaves the component "undefined"**. Eqs. 84, 87: "simulation can be used to obtain the true performance value" there | Our start (§2.3). Graeb leaves g = 0 open; we treat it as the ambiguity case, because a latent limiter hides there |
| §4.6 | General WCA: nonlinear φ, solved iteratively (SQP). "The solution may also be inside … or more than one solution may exist" (mismatch pairs). The fixed-point "shoot from nominal with improved gradients" formula: **"there is no investigation into when and how this iteration formula converges"** | Why we add the acceptance test (§2.4) and interior rule (§2.5) |
| §4.6.3, eqs. 112–113 | General WCA over a **box** is stated only as a bound-constrained nonlinear program. No special algorithm is given | Our vertex walk is one: an active-set / trust-region SLP specialized to vertex-dominated answers |
| §4.7 | Each finite-difference sensitivity analysis costs n_x runs + the nominal. General WCA calls it once per iteration | Our L0 cost model (§6) |
| §6.2 | Active-set method: release the active constraint with the most negative Lagrange multiplier | Our loop at flip budget 1 |
| §9.8, eqs. 314–328 | Range parameters: the spec must hold for **all** of them. In practice worst-case range parameters are "determined once at the beginning … and verified by another analysis at the end" | Relevant to `sigma(k)`'s ordering. Under `worst_case` range and statistical knobs are one box |

### 5.2 Antreich, Graeb & Wieser, and their earlier papers

- **[AGW94]** "Circuit analysis and optimization driven by worst-case distances," *IEEE TCAD* 13(1):57–71, 1994, doi:10.1109/43.273749. From the abstract: "exact worst-case transistor model parameters and exact worst-case operating conditions"; uses "standard circuit simulators"; complexity "increases only linearly with the number of design variables". The body was not reachable, so I can't cite its iteration details.
- Graeb, Wieser & Antreich, DAC 1993, pp. 142–147, doi:10.1145/157485.164641. The abstract adds operating (range) tolerances: "unique and realistic worst-case manufacturing conditions and worst-case operating conditions".
- Graeb, Wieser & Antreich, EURO-DAC 1992, pp. 86–91, doi:10.1109/EURDAC.1992.246260. Abstract: exact worst-case parameter sets "for all specifications separately", computed "with a sequential quadratic programming method using standard simulation tools".

**Where we depart:** their worst case is an SQP solution on an ellipsoid (statistical) plus operating ranges. For the `worst_case` class all our knobs live in a box, and the answer is almost always a vertex (all five here). So a vertex walk, which costs one run per proposal and never needs a line search in the common case, is cheaper than a general SQP. It keeps SQP's two safeguards: accepting only improvements, and interior steps when evidence appears.

### 5.3 Spence & Soin

*Tolerance Design of Electronic Circuits*, Addison-Wesley 1988 (xiii + 215 pp., ISBN 0201182424; reprinted 1997). The Internet Archive copy is access-restricted, so **I could not read its vertex-analysis chapter**, and nothing here is cited from it. The closest verified statement of the same caveat is Tian & Shi, *IEEE TCAS-I* 47(8):1138–1145, 2000 (doi:10.1109/81.873869): vertex analysis with guessed directions can miss the worst case even in linear circuits. Walkthrough §5.7 shows it in ours.

### 5.4 ECSS-Q-HB-30-01A (14 January 2011)

- **§5.3.2 (p. 21):** extreme value analysis "is the best initial approach"; "if the circuit passes an EVA, it always functions properly"; EVA "uses the limits of variability and the circuit **directional sensitivities** to determine the worst case results". That is the classical one-jump method.
- **§4.4 (pp. 13–14):** "non-linear effects" must be considered, "otherwise the WCA should justify the absence of such effects".
- **Table 5-2 (p. 22):** EVA "results in pessimistic estimate"; "if circuit fails, there is insufficient data to assess risk".

**Where we depart:** ECSS's "always functions properly" holds only when the directional sensitivities stay valid over the whole range. That is exactly what re-linearization checks (§3.3's flip) and what the guards check (§4). ECSS names the risk in §4.4 but doesn't give an algorithm for it.

### 5.5 Board-level tools

The PSpice User's Guide (Product Version 10.2, ch. 13 "Monte Carlo and sensitivity/worst-case analyses", pp. 504–506) says: "Worst-case analysis is not an optimization process; it does not search for the set of parameter values that result in the worst result … It shows the true worst-case results when the collating function is monotonic within all tolerance combinations. Otherwise, there is no guarantee." Its own example is a BJT amplifier whose "gain increases with small increase in Rb2, but device saturates if Rb2 is maximized". That is variant A. Micro-Cap alone offers an optimizer-based "EVA (Optimizer)", "more reliable … substantially slower" (red team §3.3, [V] there).

### 5.6 Cadence and MunEDA: what is published

- **Cadence Virtuoso Variation Option datasheet** (cached copy): worst-case distance "defines the shortest distance from the nominal point to the specification boundary in the process/mismatch parameter space. WCD typically requires under 100 simulations for each spec and so is suitable for designs with a small number of specs/parameters". Also "one-step creation of worst-case corners from statistical data". ADE Assembler "can automatically identify worst-case and statistically derived corners". **Unknown:** the algorithm behind "worst-case corners", and whether any corner search over a box is gradient-guided or enumerative.
- **MunEDA WiCkeD** (cached product pages; MunEDA is now part of Cadence): "fast PVT corners analysis and worst-case corner generation for all kind of process and environmental conditions simultaneously", "simulator-true Worst Case Analysis", and "deterministic Worst-Case Methods" combinable with Monte Carlo. **Unknown:** the published algorithm. The company's TUM origin (red team §3.2) suggests the AGW94/SQP lineage, but that is inference, not a citation.

### 5.7 Follow vs depart, in one table

| Decision | Precedent | Here | Why |
|---|---|---|---|
| First jump by slope signs | Graeb eq. 85; PSpice `.WCASE`; ECSS EVA | Same | Exact for linear measures; 1 run |
| Always simulate the predicted point | Graeb eqs. 84/87; PSpice's worst-case run | Same, mandatory | It is the inner bound |
| Iterate by re-linearizing | Graeb §4.6 (SQP; no convergence study of the plain iteration); AGW94 | Monotone vertex walk with a flip budget | Terminates, never cycles (plain loop cycled on 2 of 6 cases), vertex answers are the norm |
| Trust region | Fletcher & Sainz de la Maza, SLP, *Math. Prog.* 43:235–256, 1989 | Counted in flipped knobs | Boxes make "distance" natural as a count of flips |
| Zero or near-zero slopes | Graeb: "undefined" | Flat → joint flips (≤ 4) or lumped remainder | A latent limiter looks exactly like this |
| Outer bound | None (tools report the simulated worst) | U = I + E + R + B, "estimated" | Needed by the verdict vocabulary (P2) |
| Non-monotone measures | PSpice: "no guarantee"; ECSS §4.4 | Guards, cross-check, piecewise measures | The loop alone can't see them (§4) |
| Cost per spec | Graeb §4.7; Cadence "under 100 simulations for each spec" | 8–19 per side at n = 8 (L0) | Consistent (§6) |

---

## 6. Efficiency: runs per spec as n grows

### 6.1 The cost model

With n knobs (n_eff of them able to affect the analysis), s accepted steps per side, and k ambiguous knobs:

| Slopes | Shared (once per check) | Per side |
|---|---|---|
| L0 forward | 1 + n | s·(1 + n_eff) + (2^k − 1) |
| L0 central | 1 + 2n | s·(1 + 2·n_eff) + (2^k − 1) |
| L1 adjoint | 1 run + 1 adjoint solve per measure | s runs + s adjoint solves + (2^k − 1) |

Plus the safety net (2^r range corners + temperature points), shared by everything. On gentle measures s = 1 (no sign change) or 2 (one sign change, like gain min).

### 6.2 Measured: a cascade of CE stages, n = 8, 32, 98

To measure scaling honestly I used k AC-coupled copies of the stage (nominals varied ±3% per stage) sharing T and VCC, so n = 2 + 6k. The measure is total gain, max and min. The answer key stays exact at any n: for fixed (T, VCC) the stages are independent, so each is enumerated alone, with T scanned on a 41-point grid (`scaling.py`).

| n | Brute force | L0 forward: shared + max + min | L0 central | L1 (value runs) | Steps max/min | Matches key |
|---|---|---|---|---|---|---|
| 8 | 256 | 9 + 8 + 16 = **33** | 62 | 4 | 1 / 2 | yes |
| 32 | 4.3·10⁹ | 33 + 28 + 56 = **117** | 230 | 4 | 1 / 2 | yes |
| 98 | 3.2·10²⁹ | 99 + 83 + 166 = **348** | 692 | 4 | 1 / 2 | yes |

(Plus 4 safety-net runs.) The number of steps did **not** grow with n; the cost did, linearly, because every linearization pays one run per knob. At L1 the cost is flat. So at n ≈ 30 an L0 two-sided spec costs ~120 runs, the same order as Cadence's "under 100 simulations for each spec". At n ≈ 100, L0 is ~350 runs per two-sided spec, and L1 (roadmap M8) is what makes the loop cheap.

### 6.3 What this means for the MVP and after

- **MVP (n = 8, L0):** 71 runs for the three specs vs 256 for brute force. That's only 3.6× cheaper, because at n = 8 enumeration is still affordable. The point of M3 is to build and validate the loop against brute force, not to beat it here.
- **When enumeration wins:** for 5 searches the loop costs roughly 5·(n + 1)·(1–2) runs. So for n ≤ 6 (2⁶ = 64 runs, which serve every spec) enumeration is as cheap and exact over vertices. A cost-based switch is a one-line rule.
- **Screening at large n:** a spec usually depends on 10–50 of hundreds of knobs (red team §2.4). Knobs whose nominal contribution is below 1% of Σ|gᵢ| can skip re-nudging at vertices and go into the lumped remainder R. That keeps L0 affordable until L1 lands.
- **Wall clock:** runs within a batch are parallel. The CE amp's 71 runs are 4 sequential batches with speculative nudging (§2.9).

---

## 7. Interfaces to the sibling reports (one line each)

- **Step sizes and affine forms:** I need h per knob and the noise σ_f per measure (for τ = 3σ_f/h and for B). The loop's final slopes at x★ are the affine form's coefficients for the contributor table, and E is a candidate for that form's "± err".
- **Guards and safety net:** I need the safety-net point list, the regime-margin measures (searched here as ordinary sides), B, and the verdict rule when a guard fires. The cross-check (§2.9) is theirs to own.
- **Statistical `sigma(k)`:** the k ≤ 3 ellipsoid (edges at 3σ) lies inside the `worst_case` box, so a `worst_case` PASS implies a `sigma(3)` PASS. S2 and S3 need no statistical search, and S1 can start from this loop's range signs and cache.

---

## 8. Recommendations for the MVP

1. **Implement the monotone vertex walk of §2.2, not the plain fixed-point loop.** *Reason:* it terminates by construction. The plain loop cycled on 2 of 6 test cases (§2.4) and needs cycle detection plus enumeration as a patch.
2. **Start from the classical one-jump worst case** (slope signs at nominal). *Reason:* Graeb eq. 85, PSpice, ECSS all do; it was already the exact answer for 4 of the 5 sides.
3. **Forward nudges, inward at an edge, h = 0.01 of the half-range until the step-size report says otherwise.** *Reason:* same answers as central nudges at 57% of the cost (71 vs 125 runs). h = 0.01 was robust up to 1e-5 relative noise (§3.6).
4. **Terminate on the box optimality condition with the slopes at the incumbent; accept a step only if it improves by more than δ = 1e-4 × the spec window; cap at 12 proposals per side.** *Reason:* separates termination from model accuracy (§2.7); δ stopped the ridge zig-zag in variant B (86 → 43 runs).
5. **Ambiguity = slope stability or flat; flip up to K = 4 ambiguous knobs jointly; lump the rest into R.** *Reason:* 4 joint-flip runs on the whole CE amp, vs 29 with the obvious E-threshold rule, which also made S3 needlessly UNDECIDED (§2.6).
6. **Estimated outer bound U = I + E + R + B, labeled "estimated".** *Reason:* exact for linear measures, reproduces walkthrough §4's 4.59 V, covered the truth in 75/75 noise cases (§2.8).
7. **Set the flat threshold from the noise: τ = 3σ_f/h.** *Reason:* without it, noise-level slopes send the loop wandering (up to 184 runs). With it the verdict widens to UNDECIDED instead of a false PASS.
8. **Structural zeros only from the circuit (MVP: C and L values don't enter DC); never from a bit-identical result.** *Reason:* a limiter that is off at nominal also has an exactly-zero slope (§4.1).
9. **One cache per check, every run returns every measure, each side's inner bound over all runs, and a final cross-check that restarts a side if another run beats it.** *Reason:* free, and it turned variant A's false PASS into a FAIL (§4.1).
10. **Interior points in the MVP: detection plus the one-run 1-D parabola step; flag any spec whose answer is interior.** *Reason:* the CE amp needs none; full bound-constrained quasi-Newton can wait until a spec needs it (variant B).
11. **Tests (M3b/M3c):** pin, per side, the trajectory (vertices visited), the run count (71 total at L0 forward), inner = brute force exactly, and U on the correct side of the truth. Add variants A–C as regression tests once the guards exist. *Reason:* the answer key is valid for the three MVP measures (interior sampling never beat a corner), and trajectories catch silent regressions that a final value alone would not.
12. **Report the cost with every verdict:** "worst-point loop, 71 runs, L0 slopes (brute force would be 256)". *Reason:* `engine.md` §6.2 asks for it, and speedups mean little without the reference (red team §3.2 item 8).

---

## 9. Open questions

1. **Is E the right allowance?** It is empirical. It was conservative on every smooth case here and useless on the knee (−2.16 V). Alternatives: a curvature bound from successive slopes, or the flip certificate (n runs, exact among single flips). It needs calibration on more circuits (op-amp stages, the LDO) before PASS (estimated) leans on it.
2. **What verdict ceiling after an interior step?** PASS (estimated) as usual, or a weaker "PASS (sampled)" as the red team suggests for non-smooth specs?
3. **Tangent nudges or full flips at vertices?** Flips give an exact single-flip certificate for the same n runs but lose the tangent slopes the affine form and `sigma(k)` want. Possibly both in a sign-off mode.
4. **Ridges** (several knobs that only act through one internal quantity, like T and R2 through IC): a joint quadratic model, or detection of collinear slopes?
5. **Multi-start policy.** When the best safety-net range corner disagrees with the start vertex's range signs, start a second walk from it. It never triggered on the CE amp; how often does it matter on real boards?
6. **Order of searches.** Running guards first maximizes run reuse (variant A: 1 run instead of 9). Is a static order enough, or should the engine schedule searches by expected overlap?
7. **Warm starts across edits.** Start each side from the previous edit's answer (Solido-style reuse, red team §3.2 item 7). The monotone walk makes this safe: a stale start can only cost runs, never a wrong answer.
8. **K and the cap.** K = 4 (≤ 15 runs) and 12 proposals are guesses sized for n ≈ 8–30.

---

## Scripts (in `/root/.claude/jobs/443154a8/tmp/engine_research/worst_case/`)

| File | Reproduces |
|---|---|
| `ce_amp_model.py` | The walkthrough model (unchanged copy) |
| `wcloop.py` | The loop of §2.2 |
| `run_ce_amp.py [forward\|central] [L0\|L1] [--log]` | §3; outputs `out_forward_L0.txt` (full run log), `out_central_L0.txt`, `out_forward_L1.txt` |
| `slope_tables.py` | §2.6 and §3.3 slope tables (`out_slopes.txt`) |
| `flip_certificate.py` | §3.5 flip certificate (`out_flip.txt`) |
| `noise_test.py [--tau]` | §3.6 (`out_noise_tau.txt`, `out_noise_notau.txt`) |
| `variants.py` | §4 (`out_variants.txt`) |
| `plain_fixed_point.py` | §2.4 comparison (`out_plain.txt`) |
| `scaling.py` | §6.2 (`out_scaling.txt`) |
