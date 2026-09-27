# `sigma(k)` Confidence: Range Knobs at Worst, Statistical Knobs at the k-σ Worst Point

> 2026-09-27 · Research for roadmap M3d (`spicy_engine`). This is one of six parallel engine-math notes; the `worst_case` corner search, finite differences and guards are covered by the others.
> **Model:** the walkthrough amplifier (`ce_amp_model.py`, which reproduces every walkthrough number), plus the walkthrough's aging knob (C_in × (1 − 0.2·life)) for the aged case. One call of the model counts as one simulation.
> **Scripts:** every number below comes from a stdlib-Python script in `/root/.claude/jobs/443154a8/tmp/engine_research/statistical/` (scratch, not in the repo). The appendix maps each table to its script.
> Keys like **[AGW94]** point into `../bibliography.md`; other references are listed in §9.

---

## Summary

`sigma(3)` asks one question per spec side: *at the worst range condition, does the spec still hold for every board whose statistical knobs lie within distance 3 of nominal, in a space where each knob is a standard normal?* On a straight-line spec that is the same as "at least 99.865% of boards pass this side". On the walkthrough amplifier the loop reproduces the walkthrough's rounds exactly: 6.225 → 6.297, 6.328 → 6.341, 6.343 → 6.343 V, and 25.3 → 30.4 → 31.7 Hz. That takes 19 and 15 simulations.

Four findings refine the docs:
1. **The walkthrough's numbers are for an untruncated normal.** The docs' default truncates each part at its tolerance. The exact map for that distribution gives 6.313 V and 31.15 Hz, which match the 200,000-board Monte Carlo quantiles (6.308 V, 31.13 Hz). The plain "u = 3ε" map is conservative by 30 mV and 0.5 Hz.
2. **Solve for the k-σ worst point, not for the worst-case distance.** The k-σ point costs 8–22 simulations per spec side whatever the margin. The worst-case distance costs 23–159, and whenever its design point falls outside the tolerance box (5 of the 6 spec sides here) it describes parts that can't exist.
3. **Take slopes in physical units and apply the distribution map exactly.** Under uniform knobs the plain loop falls into a two-point cycle and returns wrong, unsafe values (22.5 Hz instead of 26.3 Hz). Adding a safeguard fixes the values but costs about 90–280 simulations. Taking the slopes in physical units instead costs 8–15.
4. **The FORM yield Φ(−β_w) is within 6–9% of Monte Carlo.** Curvature (SORM) accounts for 4–5% of that. The truncation choice moves the yield by 11–13%, and the distribution assumption moves it 13×. So yield numbers must carry their assumption.

The worst range corner never moved when the statistical knobs moved, for any spec side that matters. One case still shows the risk: gain-min has two locally worst temperature corners, and its `worst_case` corner (cold) is not its `sigma(3)` corner (hot). For the MVP all three specs PASS at `sigma(3)`. Bias FAILs at `worst_case` (6.59 V), passes at `sigma(3)` under the default (6.31 V), and passes under uniform (6.50 V by FORM, 6.45 V by Monte Carlo).

---

## 1. What `sigma(k)` promises

### 1.1 One spec side, one tangent plane

A two-sided spec is two specs. `bias: dc(output.v) in 4.5V..=6.5V` is checked as "VC ≤ 6.5 V" and "VC ≥ 4.5 V", each with its own worst point.

For one side, the engine works in **u-space**: each statistical knob becomes a standard normal uᵢ (mean 0, σ 1). Nominal parts are at u = 0. A board "3σ out" is at distance ‖u‖ = 3.

```
                 u₂
                 ▲
        +3 ┌─────┼─────┐        □  the ±3σ box: every part within its tolerance
           │  .--┼--.  │        ○  the ball ‖u‖ ≤ 3
           │ /   │   \ │        ●  the 3σ worst point u*: the largest f on the ball
      ─────┼─┼───0───┼─┼────► u₁
           │ \   │   /●╲        ╲  the tangent plane at u*. On a straight-line spec
           │  '--┼--'  │╲          every failing board lies beyond it, and that side
        −3 └─────┼─────┘ ╲         holds Φ(−3) = 0.135% of boards in any dimension
```

**Why a ball gives a probability.** On a straight-line spec f = f₀ + g·u, the failing boards all lie beyond the tangent plane at u*. The standard normal looks the same in every direction. So the probability beyond a plane at distance 3 equals the 1-D tail Φ(−3) = 0.135%, whatever the number of knobs. Graeb calls this the *single-plane-bounded* tolerance region, and its yield partition is Φ(β_W) (Graeb07 §6.2.4, p. 121, eqs. 249–250; his Fig. 51 draws it next to a ±β_W box).

**What it does not mean.** It does not mean 99.865% of boards lie inside the ball. The ball holds much less:

| Knobs n | P(‖u‖ ≤ 3) (χ²ₙ ≤ 9) |
|---|---|
| 1 | 99.73% |
| 2 | 98.89% |
| 6 (our amplifier) | 82.64% |
| 8 | 65.77% |
| 20 | 1.71% |

The guarantee is per spec side, through its tangent plane. That's exactly the right shape for a spec. (Script: `extras.py`.)

**The numbers engineers mix up:**

| Claim | k | Probability |
|---|---|---|
| One side at 3σ | 3 | 99.865% pass that side |
| Both sides of a two-sided spec, 3σ each | 3 per side | ≥ 99.73% pass both (union bound) |
| `yield(99.9%)` for one side | 3.090 | 99.9% |
| `yield(99.9%)` for a two-sided spec as a whole, split evenly | 3.291 per side | ≥ 99.9% |

`engine.md` §3.6 says "3.09σ = 99.9%". That's right per side. For a two-sided spec as a whole it's 3.29σ per side (open question 1).

### 1.2 Inner and outer bounds, and how `sigma(k)` nests with `worst_case`

`sigma(k)` has the same inner/outer structure as `worst_case`, with the ball in place of the box:

| Bound | For `sigma(k)` | Proves |
|---|---|---|
| **Inner** | Any simulated board with ‖u‖ ≤ k that violates the spec | **FAIL**: a failing board exists within kσ. That's definite as a geometric statement; "≥ 0.135% of boards fail" is the first-order (FORM) reading of it |
| **Outer (estimated)** | The loop's converged maximum over the ball | **PASS (estimated)** |

**Nesting.** The 3-ball sits inside the ±3σ box (it touches the box only at the centres of its faces). So every `sigma(3)` point is a board with all parts in tolerance, and:

```
 σ(3) value  ≤  worst-case value           (max over the ball ≤ max over the box)

 worst_case PASS  ⇒  sigma(k) PASS          (no σ search needed for the verdict)
 sigma(k) FAIL    ⇒  worst_case FAIL        (the σ point is a counterexample for both)
```

With the exact distribution map (§2.2), this holds for every k, not only k ≤ 3.

In the MVP this settles four of the five spec sides for free, because each already passes `worst_case`:
- VC min: 4.660 V ≥ 4.5 V;
- gain: 4.463 … 4.703 inside 4.37 … 4.83;
- f_L: 26.74 Hz ≤ 30 Hz.

Only **VC max** (worst case 6.59 V > 6.5 V) needs the σ search to reach its verdict. The σ values of the other sides are still worth computing for the "realistic" column of the report (`engine.md` §3.6: "both are always shown when they disagree").

---

## 2. Standard-normal space

### 2.1 Mapping each knob

| Knob | Tolerance | Default distribution (`engine.md` §2.4) | σ | Exact map u → value |
|---|---|---|---|---|
| r1, r2, rc, re | ±1% | normal, σ = tol/3, truncated at ±tol | 0.333% | R = R₀·(1 + 1%·x/3) |
| c_in | ±20% | same | 6.67% | C = 1 µF·(1 + 20%·x/3) |
| q1.beta | 100..=300 | normal(200, 33.3), truncated to 100..300 | 33.3 | β = 200 + 33.3·x |

Here x = Φ⁻¹(c + (1 − 2c)·Φ(u)), with c = Φ(−3) = 0.00135, is the **truncated** normal in σ units. It is the per-knob Rosenblatt transform (u = Φ⁻¹ of the knob's own CDF). The simpler map drops the truncation: **x = u** (the identity map).

### 2.2 The identity map versus the exact map

Where the exact map puts a board:

| u | x (σ units of the part) | fraction of tolerance | slope dx/du |
|---|---|---|---|
| 1 | 0.996 | 33.2% | 0.99 |
| 2 | 1.977 | 65.9% | 0.95 |
| 2.5 | 2.430 | 81.0% | 0.84 |
| **3** | **2.783** | **92.8%** | 0.53 |
| 4 | 2.993 | 99.8% | 0.03 |

So under the exact map, a spec driven by a single knob is checked at 92.8% of that knob's tolerance, not at 100%. That's correct: 0.135% of truncated parts lie beyond 2.783σ. The identity map instead checks at the tolerance edge, and that edge has probability zero under truncation.

What the choice does to the answers, next to 200,000-board seeded Monte Carlo at each side's worst range corner (scripts: `summary_table.py`, `mc_all.py`):

| Spec side | Nominal parts at the range corner | σ(3), identity | σ(3), exact | MC 0.135% quantile, untruncated | MC 0.135% quantile, truncated (the default) | Worst case |
|---|---|---|---|---|---|---|
| VC max (V) | 6.0005 | 6.3428 | **6.3129** | 6.3462 | **6.3075** | 6.5905 |
| VC min (V) | 5.0183 | 4.8517 | 4.8525 | 4.8537 | 4.8573 | 4.6597 |
| gain max | 4.5979 | 4.6649 | 4.6640 | 4.6651 | 4.6617 | 4.7030 |
| gain min | 4.5836 | 4.5181 | 4.5189 | 4.5170 | 4.5194 | 4.4626 |
| f_L, day 1 (Hz) | 20.2454 | 25.3399 | **24.9206** | 25.3306 | **24.9035** | 26.7378 |
| f_L, aged (Hz) | 25.3067 | 31.6749 | **31.1507** | 31.6632 | **31.1294** | 33.4223 |

**The pattern:**
- The identity map answers for an **untruncated** normal. It matches that Monte Carlo within 0.0002–0.012.
- The exact map answers for the **truncated** normal we actually claim. It matches that Monte Carlo within 0.0005–0.021.
- The two maps differ only where one knob dominates and sits near its edge: β for VC max, C_in for f_L.

**Why it matters: a false FAIL.** Take a spec `f_L <= 31.5Hz` on the aged amplifier:
- the identity map says FAIL (31.67 Hz);
- the exact map says PASS (31.15 Hz);
- the truth under the stated distribution is PASS (quantile 31.13 Hz).

A FAIL that isn't true under the stated assumptions breaks P2 ("inner bounds prove failures"). With the solver of §3.3 both maps cost the same. **Recommendation: use the exact map.** The walkthrough's §5.6 and §6 numbers (6.34 V, 31.7 Hz) then become 6.31 V and 31.2 Hz; the verdicts don't change.

### 2.3 The box and the ball: can the worst point leave the box?

**The k-σ point, k ≤ 3: never.** The ball of radius 3 is inscribed in the ±3σ box, so every coordinate of a point on it is at most 3 in size. The point touches the box only when one knob carries the whole gradient. f_L comes closest: C_in at u = −2.98 (−19.9%), β at −0.31.

**The k-σ point, k > 3, identity map: yes.** `yield(99.9%)` needs k = 3.09; high-sigma work needs 4 or more. Clip by **water-filling**: fix the knobs that would pass ±3σ at the edge, and share the leftover radius among the rest. For f_L aged (`extras.py`):

| k | Clipped at the box (parts in tolerance) | Unclipped (C_in outside its tolerance) |
|---|---|---|
| 3.00 | 31.675 Hz | 31.675 Hz |
| 3.09 | 31.848 Hz (C_in −20%, β −0.72σ) | 31.915 Hz (C_in −20.5%) |
| 4.00 | 32.795 Hz (C_in −20%, β −2.63σ) | 34.566 Hz (C_in −26.5%) |

With the exact map no clipping is needed: every u lands inside the box by construction.

**The worst-case-distance point: often.** β_w is wherever the spec boundary happens to be. Under the identity map:

| Spec side | β_w | Design point | What that part would be |
|---|---|---|---|
| VC max | 3.63 | u_β = −3.59 | β = 80 (below the 100 minimum) |
| f_L day 1 | 4.86 | u_C = −4.84 | C_in −32% (tolerance ±20%) |
| gain min | 5.44 | u_β = −5.44 | β = 19 |

Φ(−β_w) then counts boards built from parts that would fail incoming inspection. The truncated truth is lower, and it is **exactly zero** whenever `worst_case` passes. For f_L day 1, FORM says 5.9·10⁻⁷ while the truth is 0: no in-tolerance board reaches 30 Hz.

### 2.4 β: normal, log-normal or uniform?

The datasheet gives only min 100 and max 300. A span of ×3 around no stated centre invites three readings:
- **normal(200, 33.3), truncated.** The docs' default, centred on the arithmetic middle.
- **Log-normal** with 100 and 300 at ±3σ, truncated: the median is √(100·300) = 173 and σ_ln = ln 3 / 6 = 0.183. It's natural for a quantity that spans a ratio.
- **Uniform** on 100..300: what selection (binning) produces.

Other knobs at the default; 200,000 seeded boards (`beta_alts.py`, `mc_all.py`):

| β reading | VC at the corner, median parts | VC max σ(3) | VC max MC quantile | f_L aged σ(3) | f_L aged MC quantile | f_L aged fail rate: FORM / MC |
|---|---|---|---|---|---|---|
| normal, truncated (default) | 6.0005 | 6.3129 | 6.3075 | 31.151 | 31.129 | 0.85% / **0.93%** |
| log-normal, truncated | 6.0534 | 6.3419 | 6.3354 | 31.406 | 31.356 | 1.14% / **1.19%** |
| uniform | 6.0005 | 6.4022 | 6.3893 | 31.568 | 31.357 | 1.12% / **1.13%** |

- **No verdict flips.** VC max passes and f_L aged fails under all three.
- **The log-normal moves the nominal itself.** The median board has β = 173, not 200, so VC at the corner rises 53 mV before any tolerance is applied.
- **For the MVP:** keep the default, and tag every verdict with the β reading it used. The honest remedy for not knowing is the uniform check (§8), not a better guess.

---

## 3. The method

### 3.1 Two dual questions

```
 worst-case distance  (FORM; Hasofer–Lind):   β_w  = min ‖u‖        subject to  f(θ*, u) = b
 k-σ worst point      (inverse FORM, "PMA"):  f*(k) = max f(θ*, u)  subject to  ‖u‖ ≤ k
```

θ* is the worst range corner and b is the spec bound. For a straight line f ≈ f₀ + g·u both have closed forms:

```
 β_w = (b − f₀) / ‖g‖          u* = k·g/‖g‖,   f*(k) = f₀ + k‖g‖

 verdict:   f*(k) ≤ b   ⇔   β_w ≥ k
```

On S1 at the cold, high-supply corner: f₀ = 6.0005 V and ‖g‖ = 0.0748 V/σ.

| | From the line at the corner | True |
|---|---|---|
| f*(3) | 6.225 V | 6.343 V |
| β_w | 6.68 | 3.63 |

The line is off by 0.12 V on f*, but it's off by nearly a factor of two on β_w. The reason: the k-σ point stays at distance k, where we care, while β_w travels to wherever the boundary is (9.8σ for VC min). There lines are worst and the parts don't exist.

### 3.2 The iterations and how they behave

| Name | Update | Solves | Known behaviour |
|---|---|---|---|
| **HL-RF** (Rackwitz–Fiessler) | u⁺ = [(∇G·u − G)/‖∇G‖²]·∇G | β_w | Full step to the nearest point of the linearized boundary. No convergence guarantee |
| **iHL-RF** (Zhang–Der Kiureghian) | Same direction; step length by an Armijo line search on m(u) = ½‖u‖² + c·\|G(u)\|, with c ≥ ‖u‖/‖∇G‖ | β_w | The safeguarded version. The merit function is as restated by Haukaas & Der Kiureghian (2006, eq. 20); the original's constants and convergence proof weren't read (§9) |
| **AMV** (advanced mean value) | u⁺ = k·∇f(u)/‖∇f(u)‖ | f*(k) | This is the walkthrough loop. It oscillates when f is concave in u; CMV/HMV are the published fixes (§9) |

Simulations to convergence at L0 (forward differences, identity map; `formrun.py`, `eps_solver.py`, `form_eps_run.py`):

| Spec side | β_w | HL-RF | iHL-RF | β_w, slopes in ε (§3.3) | **k-σ point (AMV)** |
|---|---|---|---|---|---|
| VC max | 3.63 | 63 | 59 | 37 | **19** |
| VC min | 9.77 | 147 | 159 | 51 | **13** |
| gain max | 10.27 | 77 | 81 | 23 | **8** |
| gain min | 5.44 | 56 | 133 | 37 | **8** |
| f_L day 1 | 4.86 | 49 | 52 | 30 | **15** |
| f_L aged | 2.33 | 35 | 36 | 23 | **15** |

- **The cost of β_w grows with the margin.** Most of it goes on sides nobody worries about: 147–159 simulations to learn that VC min has a failure probability of 10⁻²², for example. The k-σ point costs the same everywhere. That's the case for the performance measure approach made in reliability-based design (Tu, Choi & Park 1999, §9), and our numbers agree with it.
- **HL-RF overshoots and then settles.** On VC max: ‖u‖ = 6.68 → 4.44 → 3.88 → 3.66 → 3.63. iHL-RF halves its first step twice (1.67 → 3.40 → 3.78 → 3.63). Neither failed in the identity space; the model is only gently nonlinear.
- **In the exact truncated space, HL-RF fails.** On VC max it doesn't converge in 50 iterations and stops at 3.92 with the wrong design point (β at nominal); iHL-RF converges to 5.44. On the four sides whose worst case passes, both wander off (β_w from 10 to over 1,000), because the failure region there is empty or a sliver of the box. The saturating map has vanishing slope beyond ±3, which is where the safeguard earns its keep, and where §3.3's change fixes the problem outright.
- **Plain AMV under uniform knobs falls into a two-point cycle** on all six sides and returns whichever point it stops on. For f_L day 1 that's 22.45 Hz, when the true 3σ-uniform value is 26.28 Hz: an **unsafe** answer. Adding an ascent check (accept a step only if f improves; otherwise back off along the great circle) gives the right values, but costs about 90–280 simulations depending on the stopping tolerance (`sigma_robust.py`, `pma_solvers.py`). CMV/HMV reach the value but don't settle within 40 rounds.

### 3.3 Take slopes in physical units; apply the distribution map exactly

The loop needs a slope of f for each knob. There are two places to take it:

| | Slope in u (what AMV does) | Slope in ε, the knob's position in its tolerance (proposed) |
|---|---|---|
| What gets linearized | f ∘ (map u → ε) | f alone. Circuits are gently nonlinear in physical values |
| The map u → ε | Folded into the line, so its saturation looks like the circuit flattening | Kept exact |
| Sub-problem per round | u* = k·g/‖g‖ | max Σ aᵢ·εᵢ(uᵢ) subject to ‖u‖ ≤ k: separable; the optimum satisfies aᵢ·εᵢ′(uᵢ) = 2μ·uᵢ; bisection on μ, **no simulations** |
| Simulations per round | n nudges + 1 | n nudges + 1 (the same) |

For the identity map the two are identical: the same numbers as the walkthrough. For the other maps (`eps_solver.py`):

| Spec side | Uniform, slope in u, safeguarded | **Uniform, slope in ε** | Truncated normal, slope in ε |
|---|---|---|---|
| VC max | 6.4968 (245 sims) | **6.4967 (13)** | 6.3118 (13) |
| VC min | 4.7212 (89) | **4.7212 (13)** | 4.8527 (13) |
| gain max | 4.6949 (150) | **4.6946 (8)** | 4.6640 (8) |
| gain min | 4.4815 (195) | **4.4816 (15)** | 4.5189 (8) |
| f_L day 1 | 26.2802 (178) | **26.2798 (15)** | 24.9206 (15) |
| f_L aged | 32.8503 (178) | **32.8497 (15)** | 31.1507 (15) |

The same change fixes β_w in the truncated space. If even the tolerance edges can't reach the bound on the current line, the solver simulates that edge point before believing it. The line at the cold, high-supply corner says VC max tops out at 6.459 V with every part at its edge, but that edge point simulates to 6.59 V: the walkthrough §5.5 trap again. Only if the edge point also passes, and the line drawn there agrees, does it report "unreachable inside the box", i.e. β_w = ∞ and a failure probability of exactly 0 (`form_eps`). That's what happens for VC min, gain and f_L day 1 (16 simulations each). VC max gets 5.43 in 17 simulations, where HL-RF failed.

**Precedent and departure.** Taking gradients by the chain rule through the transform, ∇ᵤG = (∂x/∂u)ᵀ·∇ₓG, is standard FORM practice. Solving the transformed sub-problem exactly, instead of linearizing through the map, is our change. I didn't find this exact variant in the sources I checked. It's motivated by the saturation, and it costs nothing extra.

### 3.4 Stopping, early exits and cost

- **Round 1:** the line at the range corner, with nominal parts (1 + n simulations). It's shared by every spec side measured at that corner (one DC + AC run gives VC, gain and f_L), and by the `worst_case` loop. Then 1 simulation at the predicted point.
- **Each later round:** n nudges at the new point, then 1 simulation.
- **Stop** when |predicted − simulated| ≤ δ. I used δ = 10⁻³ of the spec window, or of the bound for a one-sided spec (2 mV for bias, 0.03 Hz for bass).
- **Early FAIL:** the first simulated point inside the ball that violates the spec proves FAIL. f_L aged is decided after 8 simulations.
- **Structural skip:** capacitors have zero slope for `dc()` measures, so bias needs 5 nudges, not 6. The analysis type tells the engine this without any screening run.
- **At L1** (adjoint slopes, M8), each round costs about 2 runs instead of n + 1. Nothing else in the method changes.

MVP cost at L0 (exact map, slopes in ε):

| Spec side | σ search | Cross-corner check (§6) | Uniform check (§8) | Total |
|---|---|---|---|---|
| VC max | 19 | 3 | 13 | 35 |
| VC min | 13 | 3 | — (worst case passes) | 16 |
| gain max | 8 | 3 | — | 11 |
| gain min | 8 | 3 | — | 11 |
| f_L | 15 | 3 | — | 18 |
| **All** | | | | **91**, or **70** after sharing |

The sharing: the first line at each corner serves every spec side measured there (7 runs instead of 6 + 7 + 7 at the cold corner, and 7 instead of 6 + 7 at the hot one), and the safety net has already run each corner with nominal parts. For comparison, the walkthrough's Monte Carlo check is 20,000 runs per range corner.

---

## 4. Worked example S1: VC max at `sigma(3)`

**Setup.** The range corner comes from the safety net's nominal-parts runs: cold (−10 °C) and high supply (12.6 V), where VC = 6.0005 V. The statistical knobs are R1, R2, RC, RE and β; C_in is structurally skipped.

**Identity map** (reproduces walkthrough §5.6; `s1.py`, `detail_logs.py`):

| Round | Line predicts | Simulation | Cumulative sims | The σ point in part values |
|---|---|---|---|---|
| 1 (line at the corner) | 6.2250 | 6.2965 | 7 | R1 +0.36%, R2 −0.34%, RC −0.29%, RE +0.28%, β 123 |
| 2 | 6.3279 | 6.3406 | 13 | R1 +0.18%, R2 −0.17%, RC −0.14%, RE +0.13%, β 105 |
| 3 | 6.3426 | **6.3428** ✓ | 19 | R1 +0.13%, R2 −0.12%, RC −0.11%, RE +0.10%, β 103 |

These are the walkthrough's numbers (6.225 → 6.297, 6.328 → 6.341, 6.343 → 6.343) to the last digit. The line misses in round 1 because VC bends in β (walkthrough §5.4). Re-linearizing at β ≈ 123 finds a steeper β slope, so β takes more of the 3σ budget: u_β goes −2.30 → −2.85 → −2.92.

**Exact map** (the recommendation):

| Round | Line predicts | Simulation | Cumulative sims | Parts |
|---|---|---|---|---|
| 1 | 6.2222 | 6.2846 | 7 | R1 +0.37%, …, β 127 |
| 2 | 6.3055 | 6.3118 | 13 | R1 +0.24%, …, β 113 |
| 3 | 6.3127 | **6.3129** ✓ | 19 | R1 +0.21%, R2 −0.20%, RC −0.17%, RE +0.15%, β 112 |

**Verdict:** 6.313 V ≤ 6.5 V, so **PASS (estimated)**.
- `worst_case` FAILs at 6.59 V, so this is the walkthrough's "engineer's call" row.
- The uniform check (§8) gives 6.497 V, also PASS, so the verdict isn't distribution-sensitive.

**Monte Carlo at the corner** (seed 12345, 200,000 boards):
- 0 failures under the default: 95% upper bound 0.0018%, far below 0.135%.
- Untruncated normal: 23 failures (0.0115%, CP95 0.0073–0.0173%), against FORM's Φ(−3.63) = 0.0142%.
- Uniform: 32 failures (0.016%).

---

## 5. Worked example S3: f_L at `sigma(3)`

### 5.1 The MVP, day 1 (no aging)

The range corner is cold and 12.6 V, with f_L = 20.245 Hz. Cold lowers β, which lowers the input resistance. The supply barely matters: the VCC slope is 0.001–0.002 Hz per unit of the knob.

| Map | Round 1: line → sim | Round 2: line → sim | σ(3) point | Sims |
|---|---|---|---|---|
| identity | 24.325 → 25.339 | 25.340 → **25.340** | C_in −19.9%, β 190 | 15 |
| exact | 24.054 → 24.920 | 24.921 → **24.921** | C_in −18.3%, β 181 | 15 |

- **Verdict: PASS.** It's guaranteed by the nesting anyway, since the worst case is 26.74 Hz.
- **FORM's number here is fiction.** In the identity map, FORM finds β_w = 4.86 with C_in at −32% and reports a 5.9·10⁻⁷ failure rate. In the exact map the solver reports "unreachable": no in-tolerance board reaches 30 Hz. Monte Carlo: 0 of 200,000.

### 5.2 The aged version (walkthrough §6)

Adding the walkthrough's aging knob gives a new range corner: cold, 12.6 V, end of life. There f_L = **25.307 Hz** (walkthrough: 25.3).

| Map | Round 1: line → sim | Round 2: line → sim | Verdict |
|---|---|---|---|
| identity | **30.406** → **31.674** | 31.675 → **31.675** | FAIL, proven at round 1 (8 sims) |
| exact | 30.068 → 31.150 | 31.151 → **31.151** | FAIL, proven at round 1 (8 sims) |

The walkthrough's 30.4 predicted and 31.7 simulated are reproduced exactly.

**Why the first line under-predicts.** f_L ∝ 1/C. Its slope in C_in's direction is 1.69 Hz/σ at the corner but 2.64 Hz/σ at the σ point. The second line, drawn at the σ point, is right to 0.1 mHz.

**The σ point is the counterexample:** C_in −18.3% (fewer than 0.2% of parts are that low), β 181, at −10 °C after 10 years: 31.15 Hz.

**A shortcut for testing aging in the MVP.** Aging scales f_L by exactly 1/0.8 = 1.25 at every point. So `f_L <= 24Hz` on day 1 is the same test as `f_L <= 30Hz` aged: β_w = 2.385 in both. That lets M3d exercise a real `sigma(3)` FAIL without links (§10).

### 5.3 Checking against Monte Carlo

At the aged corner, with 200,000 boards (seed 12345; `mc_all.py`, `sorm.py`, `sorm_tn.py`):

| Distribution | β_w | FORM Φ(−β_w) | SORM (Breitung) | Monte Carlo (CP95) |
|---|---|---|---|---|
| Truncated normal (default), exact map | 2.3846 | 0.855% | 0.894% | **0.933%** (0.891–0.976%) |
| Untruncated normal, identity map | 2.3333 | 0.982% | 1.026% | **1.047%** (1.003–1.093%) |
| Uniform, exact map | 1.2005 | 11.50% | 11.93% | **11.85%** (11.71–11.99%) |

The walkthrough's "177 of 20,000 (0.9%)" and the red-team's FORM "0.98%" are both consistent with these.

---

## 6. Range knobs: nested or joint?

**The worry** (`engine.md` §4.4, after Solido PVTMC): the worst range corner with nominal parts may not be the worst once the statistical knobs move.

**The test:** run the σ search at every range corner of every spec side, and compare the corner ranking with nominal parts (the safety net) against the ranking at σ(3) (`corners.py`, `crosscheck.py`):

| Spec side | Worst corner, nominal parts | Worst corner at σ(3) | Runner-up at σ(3) | Changed? |
|---|---|---|---|---|
| VC max | cold, 12.6 V (6.0005) | cold, 12.6 V (6.3428) | cold, 11.4 V (6.0170) | no |
| VC min | hot, 11.4 V (5.0183) | hot, 11.4 V (4.8520) | hot, 12.6 V (5.0957) | no |
| gain max | cold, 12.6 V | cold, 12.6 V (4.6649) | hot, 12.6 V (4.6620) | no |
| gain min | hot, 11.4 V (4.5836) | hot, 11.4 V (4.5181) | **cold, 11.4 V (4.5192)** | no, but close |
| f_L aged | cold, 12.6 V, end of life | same (31.675) | cold, 11.4 V, end of life (31.671) | no |

**What does move is gain min, between confidence classes:**
- At `worst_case` (all parts at their edges, β = 100), the worst corner is **cold**: 4.4626, against 4.4688 hot (walkthrough §5.7).
- At `sigma(3)`, β sits at only −0.51σ (β ≈ 183), and the worst corner is **hot**.
- Both temperature corners are local minima: the slope in T at each σ point points outward (+0.0008 per unit at cold, −0.0017 at hot).

So a σ search that reused the `worst_case` loop's corner would start in the wrong place. It would also stop there, because a local slope check at the cold corner says "cold is locally worst". Here the difference is only 1.3 mV/V, which changes nothing. The mechanism is exactly the PVTMC one.

**A cheap joint search for ≤ 8 range corners:**
1. Rank the range corners by their nominal-parts values from the safety net (already simulated, 0 extra sims).
2. Run the σ search at the top corner.
3. Evaluate the resulting u* at every other range corner: 2^r − 1 simulations, 3 in the MVP. On this circuit, u* at another corner reproduces that corner's own σ(3) value to within 1.5 mV and 4 mHz, because the σ direction barely depends on T or VCC.
4. If a corner beats the winner, switch to it and search again, warm-started from u*. If it doesn't beat the winner but could still change the **verdict** (its cross-evaluated value, pushed toward the bound by a slack of 10% of the winner's σ spread, crosses the bound), search it too. Otherwise stop.

For gain min, step 3 gives 4.5195 at the cold corner, against 4.5181 at hot. Hot stays the winner. The cold corner's own search would only lower it to 4.5192 (the cross-evaluation was within 0.0003 of it), and 4.5195 − 0.0066 is nowhere near 4.37. So the verdict can't change, and no extra search runs.

Had the search started from the `worst_case` corner (cold), step 3 would have found hot lower and switched. So the cross-evaluation catches the wrong start whichever corner it begins from. The cost is 2^r − 1 = 3 simulations per spec side. Full nesting would cost 4 × 8 to 8 × 15.

---

## 7. Yield from β_w

What the FORM number is worth, and what moves it, on the aged f_L spec (§5.3; `sorm.py`, `sorm_tn.py`, `mc_all.py`):

| Error source | Effect on the failure rate | Size |
|---|---|---|
| Curvature (FORM vs SORM) | Largest principal curvature κ = +0.036 in Breitung's convention (positive = curved toward the origin, so the failure side is slightly bigger than the half-space); factor Π(1 − β_w·κⱼ)^(−1/2) = 1.045 (Breitung 1984, eq. 17b) | **+4 to 5%** |
| FORM total vs Monte Carlo | 0.855% vs 0.933% (truncated); 0.982% vs 1.047% (untruncated) | **−8% and −6%** |
| Truncation choice | 0.933% (truncated) vs 1.047% (untruncated) | **−11%** |
| β reading | 0.93% (normal) vs 1.19% (log-normal) | **+28%** |
| Distribution shape | 0.93% (default) vs 11.85% (uniform) | **×13** |

**Why f_L ∝ 1/C barely curves.** The 1/C bend lies along the direction of the worst point, and FORM handles that exactly: it only moves *where* along that direction the boundary sits. Curvature comes from how C_in and β interact (f_L ∝ 1/(C·R_in(β))), and that's small. VC max is similar: κ = +0.016 and a factor of 1.03 (1.47·10⁻⁴ SORM vs 1.15·10⁻⁴ Monte Carlo, CP95 0.73–1.73·10⁻⁴).

**Conclusions:**
- A SORM correction isn't worth building for the MVP. It's 5%, next to a 13× distribution uncertainty.
- Print Φ(−β_w) only with its assumption, e.g. "≈ 0.9% (FORM, truncated normal σ = tol/3)", and only for spec sides that fail or come near the bound. Yield verdicts are M7's job (importance sampling at the design point, Clopper–Pearson).
- Where β_w is infinite (the spec is unreachable inside the box), say "0 (no in-tolerance board fails)", not a tiny FORM number.

---

## 8. Distribution robustness

`engine.md` §2.4: always also report under uniform; if the verdicts differ, the verdict is UNDECIDED.

### 8.1 What "the 3σ point" means under uniform

| Reading | How | VC max | f_L day 1 | Verdict |
|---|---|---|---|---|
| **Quantile matching** (exact map ε = 2Φ(u) − 1) | Each knob sits at the same probability level as a normal knob at that u | 6.497 V | 26.28 Hz | This is the one that means "0.135% of uniform boards" (to first order) |
| Moment matching (σ = tol/√3, so "3σ" = 1.73·tol, clipped at the box) | Treat uniform as a normal with the same σ | 6.516 V (**false FAIL**) | 26.68 Hz | Degenerates into the worst case (6.59 V, 26.74 Hz) |
| Monte Carlo truth (0.135% quantile, 200,000 uniform boards) | — | 6.453 V | 26.09 Hz | — |

Moment matching puts the "3σ" point outside the box and then clips it back to nearly the worst case. That's `worst_case` in disguise (`moment_match.py`). **Use quantile matching.** It is the same exact-map machinery as the default, with a different marginal.

### 8.2 All spec sides, default vs uniform

| Spec side | σ(3), default | σ(3), uniform | MC quantile, uniform | Worst case | Default | Uniform |
|---|---|---|---|---|---|---|
| VC max ≤ 6.5 | 6.313 | **6.497** | 6.453 | 6.591 | PASS | PASS (by 3 mV) |
| VC min ≥ 4.5 | 4.853 | 4.721 | 4.758 | 4.660 | PASS | PASS |
| gain max ≤ 4.83 | 4.664 | 4.695 | 4.691 | 4.703 | PASS | PASS |
| gain min ≥ 4.37 | 4.519 | 4.482 | 4.488 | 4.463 | PASS | PASS |
| f_L ≤ 30, day 1 | 24.92 | 26.28 | 26.09 | 26.74 | PASS | PASS |
| f_L ≤ 30, aged | 31.15 | 32.85 | 32.62 | 33.42 | FAIL | FAIL |

**No MVP verdict becomes UNDECIDED.** VC max is the one to watch: it passes under uniform by 3 mV on the FORM value, or by 47 mV on the Monte Carlo value.

### 8.3 FORM under uniform: exact for one knob, conservative for sums

| Case | FORM (exact map) | Monte Carlo | Why |
|---|---|---|---|
| f_L aged (C_in dominates) | Φ(−1.20) = 11.5% | 11.85% | One knob: the exact map makes FORM exact for a monotone function of a single variable |
| VC max (five knobs share) | Φ(−3.04) = 0.118%; σ(3) 6.497 V | 0.016%; quantile 6.453 V | A sum of uniforms has lighter tails than the tangent plane suggests: the boundary curves away from nominal in the transformed space |

Conservative is the right direction for a check whose only job is to raise UNDECIDED. Why uniform is the robust choice:
- **Birnbaum's lemma** (1948, p. 77): for independent symmetric knobs with non-increasing densities (symmetric unimodal), summing more-peaked knobs gives a more-peaked sum.
- **Barmish & Lagoa's uniformity principle** (CDC 1996 version, §2.2): over that class, restricted to a box, the probability of landing in a closed, convex, symmetric set is smallest for uniform knobs.
- **For a one-sided spec on a straight-line metric S = a·ε,** the slab |S| ≤ t is such a set, and P(S > t) = ½·P(|S| > t). So uniform knobs give the heaviest one-sided tail too. The red-team report checked this numerically (A.4).
- The truncated-normal default is in that class, so the uniform answer covers it.
- For curved metrics this holds only approximately, and for acceptance sets that aren't symmetric about nominal it can fail. That's the truncation phenomenon of Winstead & Barmish; read it before M7.

### 8.4 Which spec sides need which search

```
 worst_case PASS?  ── yes ──►  sigma(k) PASS under any in-tolerance distribution   (0 sims)
        │ no
        ▼
 default σ(k) point violates?  ── yes ──►  FAIL + counterexample (the σ point)
        │ no                                (uniform has heavier tails, so it would fail too)
        ▼
 uniform σ(k) point violates?  ── yes ──►  UNDECIDED (distribution-sensitive)
        │ no
        ▼
 PASS (estimated), both values shown
```

In the MVP only VC max goes past the first box: default 6.313 V, uniform 6.497 V, so PASS.

---

## 9. Grounding: where we follow precedent and where we depart

### 9.1 Decisions against their sources

| Decision | Precedent | What we do |
|---|---|---|
| `sigma(k)` = range knobs at worst, statistical knobs on the β-ball | Graeb07 §5.2 *Realistic Worst-Case Analysis* (linear model) and §5.4 *General Worst-Case Analysis* (nonlinear); §5.5.2 shows the realistic one is a special case of the general one; [AGW94] | Follow |
| The same question asked two ways: the k-σ point and the worst-case distance | Graeb07 Fig. 56 shows general worst-case analysis and geometric yield analysis (§6.3) as "inverse mappings exchanging input and output" | Follow; we compute the k-σ point for the verdict |
| Check each spec side separately, each at its own worst range condition | Schenkel et al. 2001, §2 eq. (2): θ_wc = argmin over θ of f, and "there is usually a unique worst-case operational parameter set … for each performance"; Graeb07 §6.3.5 *Worst-Case Range-Parameter Vector*. MunEDA's WCA documentation: the worst-case point "for every specification separately … under worst-case operating conditions … can only be obtained as solution of an optimization formulation over both, process parameters and operating parameters" | Follow, with range corners enumerated (§6) |
| Range conditions are part of the spec, not averaged | Schenkel 2001, eq. (5): "θ must be rigorously considered as part of the specification to avoid an illusively high yield estimate" | Follow (the range knobs are always at worst) |
| Per-spec probability from β_w | Graeb07 §4.8.3 *Yield Partitions*, §6.3.8 *Geometric Yield Partition*; §5.5.1 *Yield Approximation Accuracy*; Schenkel 2001 eq. (8) s_wc = argmin sᵀs subject to f = f_b | Follow, labelled as a FORM estimate; sampling at sign-off (M7) |
| The ball's meaning comes from the tangent plane | Graeb07 §6.2.4 *Single-Plane-Bounded* tolerance region, eqs. (249)–(250), vs §6.2.3 *Ellipsoid* | Follow |
| Solve for the k-σ point (PMA), not β_w (RIA) | Tu, Choi & Park 1999: PMA "is shown to be inherently robust and more efficient in evaluating inactive probabilistic constraints, while RIA is more efficient for violated [ones] … RIA yields singularity in some cases". Zhang & Der Kiureghian 1995 also give an improved *inverse* reliability algorithm | Follow. Here PMA was cheaper even on the violated side (15 vs 23–36) |
| Safeguard the iteration | iHL-RF (Zhang & Der Kiureghian 1995). Youn, Choi & Park 2003: AMV "behaves poorly for a concave performance function"; CMV/HMV fix it | Follow the principle. Our safeguard: slopes in ε with the map applied exactly, plus an ascent check (§3.3). It's our variant; I didn't find it in these sources |
| Non-normal knobs by an exact transform | Hohenbichler & Rackwitz 1981 (a general transformation reducing non-normal vectors to the standard first-order case); Graeb07 §3.7 *Transformation of Statistical Distributions*; Schenkel 2001 §2 (non-normal distributions "can be transformed into a normal" one) | Follow. The knobs are independent, so it's one CDF map per knob |
| Truncate at the tolerance | Board tools do truncate: PSpice's GAUSS draws only within ±3σ, and SIMetrix has a GAUSSTRUNC distribution (red-team A6 table and §3.1). The FORM and worst-case-distance papers cited here work with untruncated normals after transformation | Follow the board tools on the distribution. **Depart** from the WCD papers by using the exact truncated map: tested limits screen out-of-tolerance parts, and the identity map gives false FAILs (§2.2) |
| Range × statistical coupling | Solido: "The worst-case PVT condition at nominal is often not the worst-case PVT for your worst-case Monte Carlo samples" (Variation Designer technology page, archived 2017); MunEDA (above) | Enumerate range corners (≤ 8), search the worst, cross-evaluate u* at the others (§6). The corner didn't move here; the `worst_case` corner did |
| Curvature | Breitung 1984, eq. (17b) | Not in the MVP: 4–5% here |
| Uniform as the robust check | Birnbaum 1948 lemma; Barmish & Lagoa 1997 uniformity principle | Follow (`engine.md` §2.4), by quantile matching |
| Industry use of worst-case distance for high yield | Cadence (2014 blog): "ADE GXL selects WCD (worst case distance) metric-based method as its high yield solution … accuracy of WCD is impacted by nonlinearity of spec boundary" | Same caveat here: we verify every predicted point by simulation, and the yield number is labelled |

### 9.2 References

Status in brackets: **[read]** = text or abstract read by the verification agent; **[second-hand]** = through a citing work or an implementation; **[unverified]** = the detail wasn't reached.

- **Hasofer & Lind**, "Exact and invariant second-moment code format", *J. Eng. Mech. Div.* ASCE 100(EM1):111–121, 1974, doi:10.1061/JMCEA3.0001848. The reliability index as the minimum distance to the limit state in standard space [second-hand, via Liu & Der Kiureghian 1991]; section numbers [unverified].
- **Rackwitz & Fiessler**, "Structural reliability under combined random load sequences", *Computers & Structures* 9(5):489–494, 1978, doi:10.1016/0045-7949(78)90046-9. Tangent-hyperplane approximation, and normal approximations for non-normal loads [abstract read]; the HL-RF formula as implemented in OpenSees `HLRFSearchDirection.cpp` [second-hand]; its equation number [unverified].
- **Zhang & Der Kiureghian**, "Two improved algorithms for reliability analysis", in *Reliability and Optimization of Structural Systems*, pp. 297–304, 1995, doi:10.1007/978-0-387-34866-7_32. An improved algorithm for β, and one for inverse reliability [abstract read]; merit function and c condition [second-hand, via Haukaas & Der Kiureghian, *Prob. Eng. Mech.* 21:133–147, 2006, eq. 20]; Armijo constants and convergence proof [unverified].
- **Liu & Der Kiureghian**, "Optimization algorithms for structural reliability", *Structural Safety* 9(3):161–177, 1991. Five methods compared on four examples [abstract read]; their non-convergence examples [unverified].
- **[AGW94]** Antreich, Graeb & Wieser, *IEEE TCAD* 13(1):57–71, 1994, doi:10.1109/43.273749. Exact worst-case parameters and operating conditions; worst-case distances as the measure of performance and yield [abstract fragments read]; equation numbers [unverified].
- **[Graeb07]** Graeb, *Analog Design Centering and Sizing*, Springer 2007. The section numbers and titles above come from the table of contents and figure captions [read]. The closed form of the realistic worst-case point, s_WC = s₀ − β_W·C·g/√(gᵀCg), and the verbatim "Y = Φ(β_W)" [unverified: chapter 5 is paywalled].
- **Schenkel, Pronath, Zizala, Schwencker, Graeb & Antreich**, "Mismatch analysis and direct yield optimization by spec-wise linearization and feasibility-guided search", DAC 2001, pp. 858–863 [full text read]. Eqs. (2), (5), (8), (15)–(16); a yield from the spec-wise linear models "differing less than 1-2% from … Monte-Carlo analysis".
- **Tu, Choi & Park**, *J. Mech. Des.* 121(4):557–564, 1999 [abstract read]. **Youn, Choi & Park**, *J. Mech. Des.* 125(2):221–232, 2003 [abstract read]; the AMV formula's equation number [unverified]. **Der Kiureghian, Zhang & Li**, "Inverse reliability problem", *J. Eng. Mech.* 120(5):1154–1159, 1994 [abstract read].
- **Hohenbichler & Rackwitz**, *J. Eng. Mech. Div.* 107(6):1227–1238, 1981 [abstract read]. **Der Kiureghian & Liu**, *J. Eng. Mech.* 112(1):85–104, 1986 [abstract read; that it is Nataf-based is unverified from the text].
- **Breitung**, *J. Eng. Mech.* 110(3):357–366, 1984 [read]. Eq. (17b) and its sign convention, as in §7.
- **Birnbaum**, *Ann. Math. Stat.* 19(1):76–81, 1948 [read: the definition on p. 76, the lemma on p. 77]. **Barmish & Lagoa**, *MCSS* 10(3):203–222, 1997 [statement read in the CDC 1996 version, §2.2, p. 3421; journal section numbers unverified].
- **Solido** Variation Designer technology page (Wayback, 2017-10-20) [read]; sigma-driven corners patent US20120259446A1 (granted as US8494670B2) [abstract read]. **MunEDA** worst-case analysis page (Wayback, 2008) [read]. **Cadence** community blog, H. Liu, 2014-05-12 [read]. Cadence's "under 100 simulations for each spec" comes from the red-team report (§3.1) and wasn't re-found here.

---

## 10. Test plan for M3d

Tests live in `#[cfg(test)] mod tests` inside the `spicy_engine` source files. Anything stochastic is seeded, so counts are exact and get pinned as snapshots. The statistical tolerances below are applied **once**, when a snapshot is accepted, and are written next to it.

### 10.1 What each test pins

| # | Test | Oracle | Tolerance | Catches |
|---|---|---|---|---|
| T1 | **Linear closed form.** A synthetic f = f₀ + g·u (no simulator) | u* = k·g/‖g‖, f* = f₀ + k‖g‖, β_w = (b − f₀)/‖g‖ | 1e-12 relative | Wrong scaling (σ = tol/3), sign, norm |
| T2 | **Map round-trips.** Truncated normal, uniform, log-normal: u → ε → u, and ε at u = 3 | Closed forms: x(3) = 2.7826σ; uniform ε(3) = 0.99730 | 1e-12 | Wrong truncation constant, wrong CDF |
| T3 | **Solver vs brute force on the ball.** For each spec side: the σ(3) search against the best of 2,000 seeded random directions on the ball, refined locally | Brute-force value | 1e-3 of the spec window | A solver stuck in a cycle, or at a local maximum (the uniform AMV failure) |
| T4 | **Nesting invariants** | σ(3) value ≤ worst-case value; nominal ≤ σ(3) value (for a max); σ point inside the box; worst_case PASS ⇒ sigma PASS | exact | Map or clipping bugs |
| T5 | **Monotone in k.** σ(k) value at k = 1, 2, 3 | Strictly increasing (for a max) | exact | Sign and normalisation bugs |
| T6 | **Calibration.** Seeded Monte Carlo at the winning corner, N = 20,000: the fraction of boards beyond the σ(k) value | Φ(−k): 2.275% at k = 2 (455 expected, sd 21); 0.135% at k = 3 (27 expected, sd 5) | Ratio within [0.8, 1.25] at k = 2; count within [12, 50] at k = 3 | Wrong distribution in the search vs the sampler; wrong truncation |
| T7 | **Verdict vs Monte Carlo** on the MVP specs plus moved-bound test specs (below) | MC verdict: FAIL if the CP95 lower bound > 0.135%, PASS if the upper bound < 0.135% | Must agree wherever MC is decisive | The actual M3d claim |
| T8 | **Walkthrough regression.** Identity map, S1 | Rounds 6.2250/6.2965, 6.3279/6.3406, 6.3426/6.3428 | 1e-4 V (printed precision; runs on the reference-model backend, §10.3) | Loop logic |
| T9 | **Simulation count.** Per spec side | The costs in §3.4 | exact (snapshot) | Silent cost regressions |

**Calibration tolerance, measured** (`calib.py`). The ratio of MC count to expected ranged 0.98–1.02 at k = 2 with the exact map (1.02–1.06 at k = 1.5), and 1.00–1.17 with the identity map (gain min: 531 vs 455, a curvature effect). At k = 3 the count ranged 19–33 with the exact map and 15–30 with the identity map (expected 27). The bands in T6 include that FORM error plus 2 sd of sampling noise.

### 10.2 Test specs with known answers

These use the MVP circuit on day 1, with moved bounds (`test_specs.py`; 20,000 boards, seed 7, default distribution):

| Spec | σ(3) value | Verdict | β_w | MC failures | MC verdict |
|---|---|---|---|---|---|
| VC ≤ 6.25 V | 6.313 | FAIL | 2.52 | 130 (0.54–0.77%) | FAIL |
| VC ≤ 6.30 V | 6.313 | FAIL | 2.90 | 42 (0.15–0.28%) | FAIL |
| VC ≤ 6.35 V | 6.313 | PASS | 3.32 | 7 (0.014–0.072%) | PASS |
| **f_L ≤ 24 Hz** (≡ aged f_L ≤ 30 Hz) | 24.92 | FAIL | 2.385 | 168 (0.72–0.98%) | FAIL |
| f_L ≤ 25.0 Hz | 24.92 | PASS | 3.07 | 20 (0.061–0.154%) | **UNDECIDED** (dead band) |
| f_L ≤ 25.5 Hz | 24.92 | PASS | 3.62 | 1 (≤ 0.028%) | PASS |

The f_L ≤ 25.0 Hz row shows why T7 compares only where Monte Carlo is decisive. With β_w within about ±0.1 of k, a verdict disagreement is FORM error, not a bug.

### 10.3 The answer key on our own simulator

The reference model and our simulator won't agree to the last digit: Ebers–Moll with IS = 1e-14 isn't the model's VBE law, and M2c/M2d decide temperature and AC. So:
- T3–T7 and T9 run on our simulator, with **its own** brute-force and Monte Carlo oracles.
- T8 runs on the reference-model backend (a test backend implementing the same `Backend` trait over `ce_amp_model`'s equations). It pins the loop logic independently of the simulator.

**Cost of the oracles.** 20,000 runs per spec side, at 50 µs to 1 ms per DC + AC run, is 1 to 20 s. Put them in a slow tier. The per-commit tier keeps T1, T2, T4, T5, T8 and T9, plus T3 with 200 directions.

---

## 11. Recommendations for the MVP

1. **Define `sigma(k)` per spec side, one-sided: "at the worst range corner, no board within distance k in u-space violates this bound".** *Reason:* it's the geometric statement the loop proves (FAIL definite, PASS estimated). Its probability reading, ≥ Φ(k) per side, is first order and gets printed as such.
2. **Use the exact per-knob map (Rosenblatt) for every statistical knob; the default is a normal with σ = tol/3 truncated at ±tol.** *Reason:* it answers for the distribution we claim and matches 200,000-board Monte Carlo within 0.02. The identity map is conservative by up to 0.5 Hz and can give false FAILs (§2.2). It costs the same.
3. **Compute the k-σ point (PMA) for the verdict; compute β_w only for display, for sides that fail or have β_w < k + 1.** *Reason:* 8–22 simulations whatever the margin, against 23–159 (§3.2). β_w is often ill-posed outside the box.
4. **Take slopes in ε (physical units) and solve the per-round sub-problem with the exact map.** *Reason:* one solver for every distribution. Under uniform it's 8–15 simulations and correct; the plain loop cycles to an unsafe value, and the safeguarded one costs about 90–280 (§3.3).
5. **Keep an ascent check anyway: accept a round only if f improved, else back off along the great circle, and cap the rounds at 10.** *Reason:* a cheap guarantee against cycling on circuits less gentle than this one.
6. **Stop when |predicted − simulated| ≤ 10⁻³ of the spec window; declare FAIL at the first simulated violation inside the ball.** *Reason:* it reproduces the walkthrough's 3 rounds, and decides f_L aged after 8 simulations.
7. **Skip capacitor nudges for `dc()` measures, and share each corner's first line across spec sides and with `worst_case`.** *Reason:* structurally zero slopes, and one run measures every spec (P5).
8. **Rank range corners by the safety net's nominal-parts values, search the top one, cross-evaluate u* at the other corners (2^r − 1 sims), switch if one is worse, and search a runner-up only if it could change the verdict.** *Reason:* it catches a wrong starting corner (gain min's second local corner) for 3 simulations instead of 4× nesting. Never reuse the `worst_case` loop's corner as final (§6).
9. **Use the nesting to skip work: `worst_case` PASS ⇒ `sigma(k)` PASS; compute the σ value for the report's "realistic" column only.** *Reason:* four of the five MVP sides are settled this way (§1.2).
10. **Run the uniform check (quantile matching, same solver) only when `worst_case` fails and the default σ point passes; UNDECIDED if it fails.** *Reason:* uniform is the heaviest-tailed symmetric unimodal case (§8.3); in the MVP only VC max needs it.
11. **Do not build SORM, importance sampling or Clopper–Pearson in M3d.** *Reason:* curvature is 4–5% (§7), and yield verdicts are M7.
12. **Tag every `sigma(k)` verdict with its distribution assumption (default, and the β reading), and print yields as "≈ p (FORM, assumption)"; print "0" when the failure region is unreachable inside the box.** *Reason:* the assumption moves the yield 13×; the truncation turns FORM's 5.9·10⁻⁷ into exactly 0 (§5.1).
13. **Test as in §10, with `f_L <= 24Hz` as the MVP stand-in for the aged failure.** *Reason:* it's the only exact way to test a `sigma(3)` FAIL without aging links, and it has a Monte Carlo answer (0.84%).

**What this refines in the current docs:**
- walkthrough §5.6 and §6: 6.34 V and 31.7 Hz become 6.31 V and 31.2 Hz under the stated truncated default (same verdicts);
- `engine.md` §3.6: "3.09σ = 99.9%" is per side; it's 3.29σ per side for a two-sided spec at 99.9% as a whole;
- `engine.md` §4.4: the worst range corner at nominal parts *was* the worst at 3σ on this circuit; the corner that differs is the `worst_case` one (gain min);
- roadmap M3d: "3σ worst point (σ = tol/3)" needs the map decision (item 2) and the per-side semantics (item 1).

---

## 12. Open questions

1. **Two-sided specs at `yield(p)`.** Split p evenly between the sides (3.29σ each for 99.9%), or require p per side (3.09σ)? The docs imply per side. Engineers probably read the whole spec.
2. **Truncation for "guaranteed by design" limits.** Tested limits screen parts, so truncation is real. For untested limits it isn't (`engine.md` §11 lists the related distribution question). Should those use the identity map, i.e. untruncated?
3. **The β reading.** Normal around 200, or log-normal around 173? The median board differs by 53 mV at the corner. Should part records carry a distribution field (part kinds default it), or stay with one global default plus the uniform check?
4. **Reporting the realistic value when `worst_case` passes.** It costs 8–15 simulations per side that the verdict doesn't need. Always, on demand, or only in the background?
5. **k > 3 with the identity map.** If we ever keep the identity map for some knobs, clipping at the box changes f_L aged at k = 4 from 34.6 to 32.8 Hz. Is that allowed to be silent?
6. **Accuracy of the uniform check near the bound.** VC max passes under uniform by 3 mV on FORM (47 mV on Monte Carlo). FORM is conservative for sums (§8.3). Should a near miss (say within 1% of the window) trigger a Monte Carlo confirmation rather than a verdict?
7. **Correlated knobs** (lots, networks; M6). The per-knob map assumes independence. Lots will need either a shared lot knob (already planned) or a Nataf correlation step; the former keeps the map per knob.
8. **Unverified citation details** (§9.2): Graeb's closed form for the realistic worst-case point and the verbatim Φ(β_W) statement (chapter 5 not read); iHL-RF's original constants and convergence proof; the AMV equation numbers; Cadence's "under 100 simulations per spec". None changes a recommendation; each is worth checking before the M3a design note cites it.

---

## Appendix: scripts

All in `/root/.claude/jobs/443154a8/tmp/engine_research/statistical/`, Python 3 standard library only:

| Script | What it computes | Used in |
|---|---|---|
| `ce_amp_model.py` | Copy of the shared reference model | everything |
| `statlib.py` | Maps (identity, truncated normal, uniform, log-normal), the σ loop (AMV, safeguarded, ε-linearized), FORM (HL-RF, iHL-RF, ε-linearized), seeded sampling, Clopper–Pearson | everything |
| `s1.py`, `s1b.py` | S1 rounds and costs | §4 |
| `s3.py` | S3 rounds, day 1 and aged | §5 |
| `detail_logs.py` | Round logs with part values, per map | §4, §5 |
| `corners.py`, `crosscheck.py` | σ(3) at every range corner; cross-corner evaluation | §6 |
| `formrun.py`, `form_eps_run.py` | β_w by HL-RF, iHL-RF, ε-linearized | §2.3, §3.2, §3.3 |
| `sigma_dists.py`, `sigma_robust.py`, `pma_solvers.py`, `eps_solver.py` | σ(3) under each map and solver; brute-force ball search | §3.2, §3.3 |
| `mc_all.py` → `mc_all_200000.json`, `mc_all_200k.log` | 200,000-board seeded MC per spec side and distribution; failure counts and 0.135% quantiles | §2, §5, §7, §8 |
| `beta_alts.py` | β readings | §2.4 |
| `sorm.py`, `sorm_tn.py` | Principal curvatures and Breitung factors | §5.3, §7 |
| `extras.py` | χ² ball content, truncation factors, map slopes, clipping at k > 3, cheap β estimates | §1, §2 |
| `moment_match.py` | Moment-matched uniform "3σ" point | §8.1 |
| `unif_linear.py` | Uniform quantile of the affine form (an alternative check, not recommended: under by 0.3–0.4 Hz on f_L) | — |
| `calib.py`, `test_specs.py` | Calibration counts and moved-bound test specs | §10 |
