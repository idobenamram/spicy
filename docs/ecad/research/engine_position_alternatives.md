# Round 3 Position: The Skeptic. Is the Worst-Point Loop the Right Core?

> 2026-09-27 · Round 3, one of five position papers. Reads with `engine_synthesis.md` (round 2's C1–C12 and D1–D10) and the six round-2 reports next to it.
> Every number below was computed on **ngspice-42** (libngspice driven in-process from stdlib Python), or it is cited. The scripts and their saved outputs are in `$R2/alternatives/` (R2 = `/root/.claude/jobs/443154a8/tmp/engine2`). The appendix maps each table to its script. That directory goes away with the job; copy it if the numbers must stay reproducible.

## Summary

I tried to replace the worst-point loop and could not. Round 2 did put its trust in the wrong place, though. I built five alternatives on ngspice: screening plus enumeration, two response surfaces, Latin-hypercube sampling and random vertices. I also ran exhaustive corner enumeration. All of them ran next to round 2's guarded loop on the CE amplifier, on six adversarial variants (12 spec sides) and on 40 random CE designs. The random designs give 240 spec sides, each judged at 8 bounds near its true worst value, so 1,920 verdicts. Each result is scored against a brute-force answer key built on ngspice. **As a search, the loop is the best of the lot.** It found the exact worst value on 231 of 240 random sides; the 9 misses are interior maxima of at most 0.020%. It was exact, or within 4 ppm, on all 12 adversarial sides once the pieces of a `min()` are searched separately. It needs 84 runs (median), and its cost stays linear up to 32 knobs. The alternatives lose. Sampling is disqualified: LHS with 70 points gave 949 false PASSes in 960 failing cases. Screening falls into the walkthrough's temperature trap, and its allowance explodes with the knob count. A quadratic response surface gives tighter brackets, but it costs at least (n+1)(n+2)/2 runs, and it is badly wrong outside the tolerance box: for C_in 1 → 2.2 µF it predicts f_low 42.9 Hz where the truth is 12.2 Hz. **The loop's weak point is its evidence for PASS, not its search.** Round 2's allowance, the largest observed line error (C6), is about 10% of the spread from nominal to worst. So 789 of 960 true PASSes near the bound came out UNDECIDED. A local 2-flip certificate costs 6 more runs per side and cuts the allowance by a factor of 50. It turns 81% of those UNDECIDEDs into correct PASSes, with no false PASS. It needs one condition: every kink must be declared. A hidden `min()` kink fools it, while round 2's allowance does not. **At ngspice speed, enumeration is affordable for small blocks.** All 256 corners got every adversarial verdict right, and with an interior check for power-type measures every bench verdict too. That takes 0.4 s on one core or 57 ms on 16. My recommendation is to **combine**: enumerate when 2^n_eff ≤ 1024 corners (the MVP is here); beyond that, run the loop with the 2-flip certificate; keep the k-σ search for `sigma(k)`; and drop the "instant tier". A full re-check costs 0.14–0.4 s, which is less than one agent turn.

---

## 1. The position, and how I tested it

**Position.** The worst-point loop is a local search with guards bolted on. Before we build it, we should know whether something simpler, cheaper or more honest does the job better. I checked four candidates: screening plus enumeration, response surfaces, sampling, and brute enumeration. I also asked what the tools and standards actually do. I scored every method on the same four things: worst-value accuracy, verdict correctness, run count and wall time.

**Test bed** (`ngamp.py`, `check_backend.py`, `noise_check.py`):

| Fact the engine relies on | Verified |
|---|---|
| `.temp {temp_c}` and `.options temp={temp_c}` both follow `alterparam` + `reset` | V(out) at −10 / 25 / 60 °C = 5.863924 / 5.541035 / 5.224354 V, both ways. That is identical to `ngspice -b` with a literal `.temp` |
| `reset` re-parses the deck | `com_rset` = `com_remcirc` + `inp_source_recent` (`runcoms2.c:175-186`). `alterparam` edits the stored deck and needs `reset` or `mc_source` (`inp.c:1818-1821`; manual v47 §13.5.5). So **every run is a cold start**, and D6 holds automatically |
| AC vectors are complex through `ngGet_Vec_Info` | `v_compdata` holds `{re, im}` (`sharedspice.h:162-181`). `ac lin 1 1k 1k` lands exactly on 1000 Hz; the `ac dec` grid lands on 1000.0000000000011 |
| ngspice's default TNOM is 27 °C | Manual v47 §11.1.1. I set `.options tnom=25` so that β = 200 at 25 °C. XTB = 1.5 gives β(T) |
| Cost per run, in-process | op 0.135 ms; op + one AC point 0.215 ms; op + 101-point sweep 0.289 ms. **A full MVP run** (op + exact 1 kHz + sweep + f_low refinement, all measures) **takes 1.25–1.5 ms**. A 10-period transient for THD takes 10.6 ms |
| Slopes by nudging are clean | The R1 slope of VC is 0.07700…0.07707 V per unit ε for h = 0.1 … 10⁻⁴, even at default tolerances. Engine tolerances (reltol 10⁻⁶) move nominal VC by 6·10⁻⁵ V. I used them anyway (C10) |

Measures follow round 2: `dc(output.v)`; |H| at exactly 1 kHz (C9); f_low as the literal −3 dB crossing below the peak (D5), bracketed on a 20-per-decade grid and then refined by false position in ln f to 10⁻¹⁴. ngspice's own `meas` gives 20.083 Hz here (half-power, linear interpolation). The refined value is 20.127 Hz, which is round 2's number.

**The answer key** (`key.py`): all 2ⁿ corners, 1,000 seeded interior points, and a coordinate search from the best point, for every side. That is 8,650 runs over the six designs, in 14 s, and **none of them failed** to converge.

| MVP side | Round-2 model (Python) | **ngspice key** | Verdict |
|---|---|---|---|
| nominal VC · gain · f_low | 5.4999 V · 4.5908 · 20.127 Hz | 5.5032 V · 4.5908 · 20.127 Hz | — |
| VC max (≤ 6.5) | 6.5905 | **6.5595** | FAIL |
| VC min (≥ 4.5) | 4.6597 | **4.6921** | PASS |
| gain max (≤ 4.83) | 4.7030 | **4.7028** | PASS |
| gain min (≥ 4.37) | 4.4626 | **4.4618** | PASS |
| f_low max (≤ 30 Hz) | 26.738 | **26.790** | PASS |

The numbers differ in the third digit and the verdicts are the same. No interior point beat the best corner on any MVP side. The walkthrough §5.7 trap is still there on ngspice. The nominal T slope of gain is −0.0017 per unit ε, which says "hot is worse". But at the worst corner, cold (4.4618) is worse than hot (4.4670).

**The adversarial variants** were rebuilt as ngspice decks. They reproduce round 2 closely:

| Variant | What it breaks | Round 2 (EM model) | ngspice |
|---|---|---|---|
| `hiZ`: R1 470k, R2 100k, RC 9.1k, C_in 100 nF | a limiter off at nominal (saturation) | gain 8.831 nominal; min 3.891; 62/256 corners saturate | 8.831; min **3.994**; **62/256** saturate |
| `pq`: RC 3.6k, P_Q ≤ 8.70 mW | an interior maximum | corners 8.7088, interior 8.7102 mW | corners 8.7069, interior **8.7097** mW |
| `band`: C_L 690 pF ± 10% (9th knob), \|H\| ≥ 4.33 over 100 Hz–10 kHz | a second extremum plus a below-noise slope | edges 4.5019 / 4.4993; min 4.3126 | 4.5019 / 4.4993; min **4.3135** |
| `swing`: RC 3.8k, min(VCC − VC, VC − VE − 0.2) ≥ 4.25 | a kink | 5.255 / 5.155; min 4.1757 | 5.253 / 5.158; min **4.2003** |
| `thd` (new): 0.62 V at 1 kHz in, amplitude ±10% as a range knob; THD ≤ 1% | a transient limiter (clipping) | — | nominal 0.223%; worst **3.657%**; 24/256 corners fail. **The DC saturation margin never drops below 3.0 V, so round 2's margin guard is blind to it** |

---

## 2. The contenders

Each method gets its own run cache, so its run count and wall time are its own (`methods.py`). Every run returns every measure, and every inner bound is pooled over all the runs of that method (C4).

| Method | What it does | Runs at n = 8 | Growth with n |
|---|---|---|---|
| **Loop (round 2)** | The synthesis §2 loop: nominal + nudges, 4 range corners, 2 interior temperatures, margin guard; jump, then flips at vertices, accept only improvements (D1, D2); below-noise knobs to an edge; lazy multistart; interior check for power; verdict row 6 with e_obs | 70 (MVP) | linear |
| **Loop + piecewise** | as above, but a `min(…)` measure is searched piece by piece (worst-case report §4.3) | 70 | linear |
| **Loop v2** | piecewise, plus a **2-flip certificate**: once no single flip improves, simulate the 6 pairwise flips of the 4 knobs with the smallest flip losses. An improving pair continues the walk. Otherwise the allowance is the largest non-additivity seen, \|f(ij) − f(i) − f(j) + f(★)\| | 96 | linear (+6 per side) |
| **Screen + enumerate** | Plackett–Burman 12 (12 corner runs) → main effects → keep the vital few (≥ 90% of Σ\|effect\|, at most 5), put the rest at their main-effect worst edge, enumerate 2^k corners. Allowance = Σ\|dropped effects\| + the main-effect model's largest miss | 100 | n + 2^k per side |
| **RSM 2FI** | 2^(8−2) resolution-V fraction (64 corners) + centre → linear + 2-factor-interaction model. Verify its top 3 predicted corners per side. Flags curvature (centre ≠ factorial mean) | 77 | ≥ 1 + n + n(n−1)/2 |
| **RSM quad** | face-centred CCD: the fraction + 16 axial + centre (81) → full quadratic by least squares. Maximize over the box, verify top 3. Allowance = max(leave-one-out residual, verification miss) | 93 | ≥ (n+1)(n+2)/2 |
| **LHS N** | Latin hypercube, N points; best value found | N | — |
| **Random vertices N** | N distinct random corners | N | — |
| **All vertices** | every one of the 2ⁿ corners | 256 | 2ⁿ |
| **All vertices + interior** | plus the loop's interior check (inward nudges, golden section) at the best corner of power-type measures | 256–287 | 2ⁿ |

---

## 3. Results

### 3.1 The MVP amplifier

All methods, both sides of each spec (`compare.py`, `compare_out.txt`):

| Side (truth) | Loop (r2) | Loop v2 | Screen | RSM quad | LHS 70 | 32 rand. vert. | All vertices |
|---|---|---|---|---|---|---|---|
| VC ≤ 6.5 (6.5595, FAIL) | FAIL, exact | FAIL, exact | FAIL, exact | FAIL, exact | **PASS ✗** (6.059) | **PASS ✗** (6.438) | FAIL, exact |
| VC ≥ 4.5 (4.6921) | PASS, e 0.066 | PASS, e 0.003 | UNDECIDED, e 0.382 | PASS, e 0.019 | PASS* | PASS* | PASS |
| gain ≤ 4.83 (4.7028) | PASS, e 0.0027 | PASS, e 0.0001 | PASS | PASS | PASS* | PASS* | PASS |
| gain ≥ 4.37 (4.4618) | PASS, e 0.009 | PASS, e 0.00007 | PASS, 4.4623. Its own search fell for the T trap (hot, 4.4678); the pooled value came from a bias run | PASS | PASS* | PASS* | PASS |
| f_low ≤ 30 (26.790) | PASS, e **1.92** | PASS, e **0.0002** | PASS, 26.784 | PASS, e 0.118 | PASS* | PASS* | PASS |
| **Runs · wall** | **70 · 0.10 s** | 96 · 0.13 s | 100 · 0.15 s | 93 · 0.14 s sim (+0.6 s Python fit) | 70 · 0.11 s | 32 · 0.05 s | **256 · 0.38 s** |

"e" is the method's allowance, the number a PASS must clear. PASS* means "no failing sample seen", which is no evidence at all. At n = 8, every method that searches or enumerates gets the MVP right. Only sampling fails, and it fails on the one side that matters.

### 3.2 The adversarial suite (12 sides)

| Side (truth) | Loop (r2) | Loop + pw | Loop v2 | Screen | RSM quad | LHS 70 | 32 r.v. | All vert. |
|---|---|---|---|---|---|---|---|---|
| MVP × 5 | 5 right | 5 | 5 | 4 + U | 5 | 4 + **✗** | 4 + **✗** | 5 |
| hiZ gain ≥ 8.36 (3.994, FAIL) | FAIL (margin guard) | FAIL | FAIL | FAIL (5.69) | FAIL (4.33) | FAIL (7.42) | FAIL | FAIL |
| hiZ gain ≤ 9.24 (9.065, PASS) | U-bracket (e 8.31) | U | **PASS** (e 0.0009) | U | U | PASS* | PASS* | PASS |
| hiZ VC ≥ 1.2 (1.287, PASS) | U-regime | U-regime | U-regime | U | U | PASS* | PASS* | PASS |
| pq P_Q ≤ 8.70 mW (8.7097, FAIL) | FAIL, exact (interior check) | FAIL | FAIL | FAIL (8.7009) | FAIL (corner) | **✗** | FAIL | FAIL (corner 8.7069) |
| band ≥ 4.33 (4.3135, FAIL) | FAIL | FAIL | FAIL | FAIL | FAIL | **✗** | FAIL | FAIL |
| swing ≥ 4.25 (4.2003, FAIL) | U-bracket (found 4.374) | **FAIL** | FAIL | U | FAIL | **✗** | **✗** | FAIL |
| THD ≤ 1% (3.657, FAIL) | FAIL | FAIL | FAIL | FAIL | FAIL | FAIL (1.02) | FAIL | FAIL |
| **right / UNDECIDED / wrong** | 9 / 3 / 0 | 10 / 2 / 0 | **11 / 1 / 0** | 8 / 4 / 0 | 10 / 2 / 0 | 8 / 0 / **4** | 10 / 0 / **2** | **12 / 0 / 0** |
| runs (MVP, hiZ, pq, band, swing, thd) | 70, 63, 46, 35, 24, 44 | 70, 63, 46, 35, 32, 44 | 96, 84, 48, 43, 44, 49 | 100, 90, 17, 44, 45, 43 | 93, 88, 84, 150, 84, 83 | 70 each | 32 each | 256, 256, 256, 512, 256, 256 |

Round 2's loop behaves on ngspice exactly as the guards report says: no wrong verdicts, the same two UNDECIDEDs on `hiZ`, and the margin guard catching the saturation. The one side it can't settle, `swing`, is fixed by the piecewise rule that round 2 already recommended. Enumeration needs none of the guards for these islands.

### 3.3 Random designs: the unbiased benchmark

Every adversarial case was built to break the loop. That makes the suite a fair test of the guards, but a biased test of the methods. So I drew 40 random CE designs (`bench.py`): R1 20k–600k, R2/R1 0.12–0.30, RC 2k–12k, RE 0.4k–2.5k, and C_in set for f_low of 8–60 Hz. Each design has a sane active nominal and a complete key. Each has six sides (VC max/min, gain max/min, f_low max, P_Q max), and the MVP's 8 knobs. **Bounds** are placed at bound = nominal + (truth − nominal)(1 + δ). δ < 0 makes a true FAIL and δ > 0 a true PASS, at 0.3% to 10% of the nominal-to-worst spread from the truth. Every method runs once per design; its verdict is then evaluated at all 8 bounds (`bench_eval.py`, `bench_eval.txt`).

Percent right per offset δ (240 sides each):

| Method | −0.10 | −0.03 | −0.01 | −0.003 | +0.003 | +0.01 | +0.03 | +0.10 | **False PASS** | UNDECIDED | Runs med / max |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Loop (round 2) | 100 | 100 | 100 | 100 | 0.0 | 1.7 | 19.2 | 50.4 | **0** | 789 | 84 / 118 |
| **Loop v2** (pw + 2-flip) | 100 | 100 | 100 | 100 | 65.4 | 84.6 | 92.9 | 96.2 | **0** | 146 | 116 / 146 |
| Loop v3 (sep. quadratic + 2-flip) | 100 | 100 | 100 | 100 | 0.0 | 0.0 | 6.7 | 67.9 | 0 | 781 | 132 / 162 |
| Loop + PB12 in the shared round | 100 | 100 | 100 | 100 | 0.0 | 0.0 | 1.7 | 12.5 | 0 | 926 | 95 / 129 |
| Screen + enumerate | 99.2 | 97.1 | 90.8 | 87.9 | 0.4 | 0.4 | 0.4 | 0.8 | **4** | 1,011 | 116 / 170 |
| RSM 2FI | 100 | 99.2 | 97.9 | 97.1 | 0.0 | 0.0 | 1.2 | 1.2 | 0 | 968 | 80 / 81 |
| RSM quad | 100 | 98.8 | 97.9 | 97.1 | 1.7 | 14.2 | 68.3 | 90.8 | 0 | 555 | 96 / 97 |
| LHS 70 | 4.6 | 0 | 0 | 0 | 100 | 100 | 100 | 100 | **949** | 0 | 70 |
| 32 random vertices | 87.5 | 39.2 | 33.8 | 28.7 | 100 | 100 | 100 | 100 | **506** | 0 | 32 |
| All vertices | 100 | 99.6 | 99.2 | 98.3 | 100 | 100 | 100 | 100 | **7** | 0 | 256 |
| **All vertices + interior** | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | **0** | **0** | 263 / 287 |

Worst-value accuracy over the 240 sides: the loop is exact on 231 (max miss 0.020%, all on P_Q max or gain max, which peak inside the box). Enumeration alone is exact on 226 and misses by up to **1.03%** on an interior P_Q peak; that miss causes its 7 false PASSes. With the interior check it matches the loop (231, 0.020%). RSM quad is exact on 223 and misses by up to 17.5% on a design whose corners saturate. Screening is exact on 163 and misses by up to 20%. Median wall time per check (6 sides): loop 157 ms, loop v2 172 ms, all vertices 400 ms.

**What the table says:**
- The FAIL side is easy for any method that searches. All the loops are 100% right there.
- The PASS side separates the methods. There **round 2's loop confirms far fewer PASSes than loop v2, RSM quad or enumeration**: 19% of true PASSes at a margin of 3% of the spread, and 50% at 10%. It is never wrong; it just can't say PASS.
- Sampling is wrong almost every time the truth is FAIL.

### 3.4 Why the loop's PASS stalls, and a better certificate

Round 2's allowance e_obs is the largest miss of any straight line it drew: the nominal line's misses at the range corners, and every jump. On the MVP, f_low's e_obs is 1.92 Hz. That is the 1/C_in curvature: the nominal line predicts 24.87 Hz at the worst corner and the simulation gives 26.79 Hz. But that miss says nothing about whether a *better corner* exists. The C_in edge is already certified by its flip. Over the 240 random sides, e_obs is a median **9.7% of the nominal-to-worst spread** (max 39× the spread). It covered the true miss on all 240 sides; it is safe and far too wide.

The question a PASS must answer is: could an unvisited point beat the incumbent? After the 1-flip certificate, the nearest candidates are 2-flips. For a smooth measure, a 2-flip beats the incumbent only if the pair's interaction exceeds the two single losses. So loop v2 measures that interaction directly, where it matters:

```
 at x★ (no single flip improves; the flip losses d_i are known, all ≥ 0)
   take the 4 knobs with the smallest d_i            → 6 pairs, 6 runs
   any pair better than x★?  → move there, keep walking (it's a real improvement)
   else  e₂ = max over pairs |f(ij) − f(i) − f(j) + f(★)|     (non-additivity)
   PASS (estimated) needs margin > e₂ + ε_num
```

| | e_obs (round 2) | e₂ (loop v2) |
|---|---|---|
| MVP VC · gain min · f_low | 0.066 V · 0.009 · 1.92 Hz | 0.003 V · 0.00007 · 0.0002 Hz |
| Median allowance / spread (240 random sides) | 9.7% | **0.19%** |
| Covers the true miss | 240 / 240 | 239 / 240 (the miss: an interior gain max, 0.0074%, short by 0.000505 against e₂ 0.000449; no bound tested fell in that window) |
| False PASS (1,920 verdicts + 12 adversarial) | 0 | 0 |
| True PASSes near the bound left UNDECIDED | 789 of 960 | 146 of 960 |

**The catch, measured** (`allow_test.py`). e₂ only sees interactions near x★. A kink whose other branch sits at the far vertex is invisible to it. On `swing` without the piecewise rule, loop + e₂ reports **PASS (est.)** at 4.374 V with e₂ = 0.0024 V. The truth is 4.2003 V: a **false PASS**. e_obs says UNDECIDED (0.746 V). With the piecewise rule, e₂ gives the right FAIL. I tried to get both properties at once. "Loop v3" adds the residual of a separable quadratic (16 axial runs) at every vertex the check simulated. It catches the hidden kink (UNDECIDED, 0.555 V), but it is nearly as wide as e_obs on smooth circuits (median 6.4% of the spread, 68% of PASSes confirmed at δ = +0.10). The interactions among T, β, R1 and R2 look just like a kink to a separable model.

So the allowance must depend on **what the engine knows about the measure's structure**:
- For library measures that are smooth in the knobs (`dc`, `at`, `f_low` with C9's refinement, device margins), and for `min`/`max` compositions whose pieces are searched separately: use **e₂**.
- For anything that can hide a kink (user expressions with `abs`, `clip`, undeclared `min`/`max`, transient-derived measures such as THD): keep **e_obs** until calibration says otherwise.

### 3.5 Scaling: a buffered cascade, n = 8, 20, 32

k CE stages with ideal unity buffers between them, so the stages are independent and the exact key decomposes: each stage is enumerated alone for every (T, VCC) on a grid, then T is refined by golden section (`scale.py`). The measure is total |H(1 kHz)|, max and min.

| n | Exact key (runs) | Loop r2 | Loop v2 | Both exact? | Screen allowance vs value | LHS (same runs as v2), gap | RSM quad, minimum runs | Enumeration |
|---|---|---|---|---|---|---|---|---|
| 8 | 3,520 | 42 | 51 | yes | 0.065 on 4.64 | 0.97% / 0.74% | 45 | 256 |
| 20 | 10,560 | 90 | 102 | yes | 27.5 on 105.9 | 4.2% / 5.4% | 231 | 2²⁰ ≈ 9 min at 0.5 ms/run |
| 32 | 17,600 | 138 | 150 | yes | 3,108 on 2,486 | 7.1% / 8.4% | 561 | impossible |

The loop's cost grows linearly: n + 1 runs per certification. At n = 32 it takes 0.13 s. This family is gentle (separable and monotone), so it proves scaling, not robustness. The alternatives fail here in the expected ways. Screening's allowance is the sum of every dropped effect, so it grows with n until it is useless. A full quadratic can't be fitted with fewer runs than it has coefficients. Sampling falls further behind as n grows.

### 3.6 Sampling and extreme values

The best of N uniform samples of VC max approaches the corner value hopelessly slowly (`extras.py`, 16,384 DC runs in 2.8 s):

| N | Best | Gap to 6.5595 | Robson–Whitlock endpoint (2x₍N₎ − x₍N−1₎) | Cooke endpoint (1979) |
|---|---|---|---|---|
| 64 | 6.1277 | 0.432 V | 6.210 | 6.176 |
| 1,024 | 6.1909 | 0.369 V | 6.209 | 6.204 |
| 16,384 | 6.3082 | 0.251 V | 6.313 | 6.315 |

The gap falls roughly as N^−0.1. For a smooth function whose maximum sits at a vertex with n_eff active knobs, the chance that a sample comes within t of the maximum shrinks like tⁿ⁽ᵉᶠᶠ⁾, so the best-of-N gap shrinks only like N^(−1/n_eff). Extreme-value endpoint estimators assume a tail they can see, and here the tail is a single corner. They add 0.004–0.08 V to the best sample and are still 0.24 V short at 16k runs. Sampling answers "how often" (yield), never "how bad at worst". No tool I know of uses it for a worst-case bound (§4).

**Covering arrays**, the structured cousin of sampling. PB12 is a strength-3 covering array for up to 11 knobs: I checked that every 3 columns contain all 8 sign patterns. So it must hit any failure island defined by ≤ 3 knobs. The table shows which islands each design hits:

| Side | Failing corners | Knobs that must be pinned | PB12 hits? | P(32 random vertices hit) |
|---|---|---|---|---|
| MVP VC ≤ 6.5 | 2 / 256 | 7 | no | 0.235 |
| hiZ gain ≥ 8.36 | 62 / 256 | 3 | **yes** | 1.000 |
| pq P_Q ≤ 8.70 mW | 8 / 256 | 6 | no | 0.662 |
| band ≥ 4.33 | 16 / 512 | 6 | no | 0.650 |
| swing ≥ 4.25 | 2 / 256 | 7 | no | 0.235 |
| THD ≤ 1% | 24 / 256 | 5 | **yes** | 0.966 |

Latent failures tend to be broad (hiZ, THD). Directed failures tend to be narrow (MVP bias, swing), and there the slopes point straight at them. The two failure modes complement each other. But the loop found every island in this suite without a covering array, and adding PB12 to the loop's shared round only inflated e_obs (§3.3 table, "Loop + PB12"). **Not worth it in the MVP.**

### 3.7 Response surfaces: accurate in the box, wrong outside it

Inside the box, the CCD quadratic is excellent. Its allowances are up to 16× tighter than e_obs (f_low 0.118 vs 1.92 Hz; median 2.5% of the spread vs 9.7% on the bench), and it predicts sub-box edits well (below). Its cost is the problem: at least (n+1)(n+2)/2 runs, which is 45, 231 and 561 at n = 8, 20 and 32. The worst-value search is also weaker than the loop's. It found 4.33 where the truth is 3.994 on `hiZ`, missed by 17.5% on one random design, and was exact on 223 of 240 sides against the loop's 231.

**What-if through the surface** (`agent_flows.py`). The same 81-run model from the MVP check is asked about edited designs:

| Edit | Where it sits | Worst-case side | Truth (256 corners) | RSM prediction | Error |
|---|---|---|---|---|---|
| C_in 1 → 2.2 µF | 6× outside the box on C_in | f_low max | 12.18 Hz | **42.85 Hz** | +30.7 Hz. It predicts a FAIL where the truth is a clear PASS |
| R1 47k → 51k | 8.5× outside on R1 | VC max | 7.150 V | 7.162 V | +0.012 V |
| pin 2N3904, β 70..300 | 1.3× on β's low side | VC max · f_low | 6.802 V · 27.78 Hz | 6.682 V · 27.17 Hz | −0.12 V · −0.61 Hz |
| RC tolerance 1% → 0.5% | inside | f_low · VC max | 26.790 Hz · 6.5290 V | 26.761 · 6.5282 | −0.028 Hz · −0.0009 V |
| temperature 0..50 °C | inside | VC max | 6.4555 V (**now PASS**) | 6.4566 V | +0.001 V: the verdict change is predicted correctly |

Round 2's straight line predicted −4.0 Hz for the C_in edit (synthesis C11). The quadratic predicts 42.9 Hz. **Both are nonsense, in opposite directions**, because neither function was ever sampled there. Meanwhile, re-checking the edited design takes **96 runs, 134–142 ms** (loop v2) or 256 runs, 385–445 ms (all corners).

### 3.8 `sigma(3)`: the k-σ search stays

The range knobs go anywhere in their box. The statistical knobs go in the ball ‖u‖ ≤ 3 through the exact truncated-normal map (D3). The key is a projected-gradient ascent on the sphere from 6 starts at each range corner, 1,081–1,728 runs per side (`sigma.py`).

| Side | Key | k-σ point (round 2 C7) | RSM quad + 1 run | Monte Carlo, 2,000 boards | worst_case |
|---|---|---|---|---|---|
| VC ≤ 6.5 | 6.2813 | **6.2813** (49 runs) | 6.2797 (predicted 6.300) | 6.257 | 6.5595 |
| VC ≥ 4.5 | 4.8842 | **4.8842** (66) | 4.8843 | 4.877 | 4.6921 |
| gain ≤ 4.83 | 4.6636 | **4.6636** (32) | 4.6635 | 4.669 | 4.7028 |
| gain ≥ 4.37 | 4.5176 | **4.5176** (39) | 4.5197 | 4.518 | 4.4618 |
| f_low ≤ 30 | 24.974 | **24.974** (32) | 24.970 (predicted 25.148) | 24.87 | 26.790 |

The k-σ point is exact to 5 digits on all five sides. My untuned version used 32–66 runs per side; round 2's tuned one uses 8–22 (statistical report §3.4). Once a CCD exists, the RSM point is within 0.05% after one verification run, but its unverified predictions miss by up to 0.18 Hz. Monte Carlo cannot resolve a 99.865% quantile with 2,000 boards: about 2.7 samples are expected beyond it. The nesting rule (C7) holds on ngspice: every `sigma(3)` value lies inside the `worst_case` range. The bias values replace round 2's model numbers (6.313 → 6.281 V) with the same verdict. **Enumeration has no role in `sigma(k)`**: the k-σ point is interior. So the engine needs the search machinery whatever path `worst_case` takes.

### 3.9 Enumeration at ngspice speed

`par.py`, one libngspice per process (the library keeps global state):

| Corners | 1 process | 4 processes | 16 processes |
|---|---|---|---|
| 256 (MVP, 8 knobs) | 398 ms | 156 ms | **57 ms** |
| 1,024 (10 knobs) | 2,472 ms | 622 ms | **231 ms** |

The worst-case report (§6.3) put the crossover at n ≤ 6, by run-count parity. On ngspice, the budget that matters is wall-clock time, not runs. That moves the crossover to about **n_eff ≤ 10**, where 2^n_eff corners take ≤ 2.5 s on one core or ≤ 0.25 s on 16 cores. n_eff is the number of knobs that can reach the measure structurally: VC has 7 on the MVP, because C_in can't enter DC. Enumeration also makes the rest of the engine simpler:
- **Explanation is free.** Every neighbour of the counterexample is a corner that was already simulated, so the flip table costs 0 runs (§5.2).
- **Adding a spec costs nothing** if its analysis already runs: 0 new runs for `VCE >= 3.9 V`. Loop v2 needs 3 new runs for the same spec (`extras.py`).
- **After enumeration, the loop's vertex runs are all cache hits.** Running it as a cross-check costs only its nudges and interior points.

---

## 4. What the tools and standards do (d)

Mostly from round 1's red team, which read the primary sources (`engine_method_redteam.md` §3, tagged [V] there). I added the Fast PVT patent.

| Approach | Who ships it | What it means for us |
|---|---|---|
| **Exhaustive corner enumeration** | LTspice users' `wc()` + `.step` idiom: all 2ᴺ + 1 vertices (red team §3.1). Cadence ADE Assembler corner sets; PrimeWave "thousands of corners". The red team's own advice: "enumerate range corners by default … 2^k ≤ 256" (§2.3) | The board-level practice for small N, and **what M3b builds anyway** as the answer key |
| **One jump along nominal sensitivities** | PSpice `.WCASE` and AA, Multisim, Micro-Cap EVA; ECSS extreme-value analysis, "the best initial approach" (ECSS-Q-HB-30-01A §5.3.2, via worst-case report §5.4) | The loop's first step. Vendors document the failure themselves: PSpice gives "no guarantee" unless monotone |
| **Iterative worst-case search** | Cadence VVO worst-case distance, "under 100 simulations for each spec … small number of specs/parameters"; MunEDA WiCkeD, whose vendor says still run brute-force MC for nonlinearity | The loop's lineage. **Nobody publishes their certificate or allowance** |
| **Box optimizer** | Micro-Cap "EVA (Optimizer)": "more reliable … substantially slower" | The loop is a cheaper, local version of this |
| **Surrogate ordering a finite corner set** | **Solido Fast PVT** (US8612908B2, read on Google Patents): Gaussian-process models of the output over the PVT corner set. The next corner to simulate maximizes a lower-confidence-bound criterion g(x) ± w·s(x). It stops on no improvement, an iteration cap, or a confidence bound. The patent's example needs 12.2× fewer simulations than full factorial. Every corner it reports has been simulated | "Models order, simulations decide": the same rule as Solido HSMC (US9483602B2) and Cadence FMC. It is **enumeration made cheaper**, over a set of hundreds to thousands of discrete corners. It does not replace the search over a continuous box |
| **Quadratic response surfaces** | Keysight ADS "Shadow Model" quadratic surrogates, used inside yield optimization (red team §3.1) | Precedent for RSMs as an *optimizer's* accelerator, not a verdict |
| **Monte Carlo** | everyone; ECSS MC at P ≥ 99.5% with 95% confidence; LTspice `mc()` | Yield and σ sanity checks, never a worst-case bound |

**Nobody I found uses space-filling sampling to bound a worst case over a box.** The board tools that promise a worst case either enumerate (LTspice) or jump once (PSpice). The loop sits between the two, and the IC tools that run such a search don't publish how they decide PASS.

---

## 5. Flow diagrams

### 5.1 The cold check (MVP) as I propose it

```
 spicy check circuits/ce_amp.spl
   │  M1f: SPICE export + knob table (8 knobs; n_eff: VC 7, gain 8, f_low 8)
   ▼
 load the deck into libngspice once (.param per knob; alterparam + reset per run)
   │
   ├─ R0  shared: nominal + 8 nudges                                           9 runs
   │       → nominal slopes (contributors, σ start), margin lines
   │
   ├─ route each side:  2^n_eff ≤ 1024 ?
   │     │ yes (MVP: 256)                           │ no (n_eff > 10)
   │     ▼                                          ▼
   │   ENUMERATE all corners, one batch           LOOP v2: jump → flips → 2-flip certificate
   │   every measure at every corner              (piecewise for min/max; margin guard;
   │   256 runs · 0.38 s (1 core) · 57 ms (16)     below-noise knobs to an edge; interior check)
   │   interior check at the worst corner          ≈ n + 7 runs per side, more if it moves
   │   of power-type measures (MVP: 0 runs)       allowance e₂ (smooth measures) or e_obs (others)
   │     │                                          │
   │     └────────────── pooled inner bounds, counterexamples ─┘
   │
   ├─ sigma(3): k-σ point at the side's worst range corner (exact map),
   │            cross-evaluated at the other range corners               8–66 runs per side
   │            (not needed for the verdict when worst_case passes: nesting)
   ▼
 verdict table → terminal + JSON
   bias   FAIL (worst_case: 6.5595 V at T− VCC+ R1+ R2− RC− RE+ β−)   PASS at 3σ (6.281 V)
   gain   PASS (all 256 corners)     bass  PASS (all 256 corners)
   cost: 256 + 9 + σ (40–330) runs ≈ 0.5–0.9 s on one core
```

### 5.2 The agent's main flows through this engine

Tool names follow `agent_flows.md` §4.1. Costs are measured on ngspice, single core. An LLM turn takes seconds, so **the engine is never the latency bottleneck at MVP scale**.

```
 "Why does bias fail?"
   engine.explain(bias, upper, worst_case)                      0 runs (stored)
     → counterexample corner, VC = 6.5595 V
     → flip table at that corner (all neighbours were simulated):
         T +0.702 · β +0.420 · VCC +0.355 · R1 +0.150 · R2 +0.139 · RC +0.122 · RE +0.109 · C_in 0
         (V drop when that one knob moves to its other edge; the overshoot is 0.0595 V)
     → sigma(3): 6.281 V, PASS
   Agent may claim: FAIL is proven (a real, reproducible run). "Moving any one of the
   seven DC knobs off its edge passes" is proven (7 real runs). So the failure needs
   every part at its edge at once, which is why 3σ passes.

 "Fix bias."
   lang.propose(R1 46.4k) · lang.propose(R2 10.2k) · lang.propose(R1 45.3k)   instant
   engine.verify(each)                         96 runs · 145–195 ms each (loop v2)
                                               or 256 runs · ~385 ms (all corners)
     R1 46.4k → bias 4.593 … 6.4635 V, every spec PASS
     R2 10.2k → bias 4.542 … 6.4218 V, every spec PASS
     R1 45.3k → VC min 4.406 V: FAIL (overshoot)
   Agent may claim: "verified" for each checked candidate, on all specs, not just bias.

 "What if C_in = 2.2 µF?"   (any edit to the design)
   engine.verify(proposal)                     ≈ 0.14–0.4 s: just re-check
     → f_low worst 12.18 Hz, PASS
   Never answered from a stored form or surface: those said −4.0 Hz and 42.9 Hz.
   Sub-box questions ("RC at 0.5%?", "temp 0..50 °C?") may get an RSM
   prediction first, labeled "predicted" (errors ≤ 0.03 Hz / 0.002 V here).

 "Add a spec: VCE >= 3.9 V."
   lang.propose(spec) → engine.check           all corners: 0 new runs · loop v2: 3 new runs
     → FAIL, worst 3.289 V
   Agent may claim: verified, same as any check.

 "Pin a real 2N3904."
   parts.extract → β 70..300 (reviewed = false) → lang.propose → engine.verify
     → C12's value-keyed cache keeps the 128 corners at the unchanged β = 300 edge;
       128 new runs (≈ 0.2 s)
     → bias max 6.802 V FAIL (it was 6.5595); f_low 27.78 Hz PASS
   Agent may claim: a verdict that rests on AI-extracted data, tagged unreviewed.
```

### 5.3 What each alternative would mean for the agent

| Engine core | "Why" | "Fix" | "What-if" | Honest? |
|---|---|---|---|---|
| Loop | Flip table at the worst corner (real runs) + nominal slopes | re-check each candidate | re-check | Yes, if the allowance fits the measure (§3.4) |
| All corners | Flip table free; "which corners fail" exact (2 of 256) | re-check | re-check | Yes, over corners; interior only where checked |
| Response surface | Interaction terms (the T × β that flips the gain trap) | fast candidate ranking | instant, but **only inside the box** | Only as "predicted", and never for a design edit |
| Sampling | Histograms | — | — | Not for worst case |

A response surface makes what-if cheap. It makes it honest only for questions about a sub-box. On ngspice the honest answer (re-check) costs less than a second, so the surface buys nothing in the agent flows the MVP needs.

---

## 6. Critique of round 2: keep, cut, change

| Item | Verdict | Why (evidence here) |
|---|---|---|
| **C1** accept only improvements, stop on optimality | **Keep** | Every walk ended with a 1-flip certificate inside the 4-round budget: no UNDECIDED (search) on the 12 adversarial or 240 random sides |
| **C2** / **D1** flips at vertices | **Keep, extend** | The 1-flip certificate is the right move. Add the 2-flip check (6 runs per side): it is what makes PASS confirmable (§3.4) |
| **C3** margin guard | **Keep, note its blind spot** | It caught `hiZ` in 1 run. It can't see dynamic limits: the THD corners clip with a DC margin ≥ 3.0 V. The loop found THD through its slopes because distortion already rises at nominal. A hard limiter would have no slope there |
| **C4** pool every run | **Keep** | Free. With enumeration it is automatic |
| **C5** below-noise knobs to an edge | **Keep** | On ngspice, slopes agree to 4–5 digits for h = 0.1 … 10⁻⁴, so noise sits far below every slope that matters and the rule rarely triggers (I set τ from σ_f = 10⁻⁹·\|f\|) |
| **C6** PASS needs margin > e_obs | **Change** | Safe (0 false PASS) but 789 of 960 near-bound true PASSes UNDECIDED. Use e₂ for smooth or declared measures and e_obs for the rest (§3.4) |
| **C7** / **D3** / **D4** exact map, k-σ point, per side | **Keep** | Exact on 5/5 sides on ngspice; nesting holds |
| **C8** three kinds of error | **Keep** | e₂ is an *observed* remainder (kind 2), never a proof |
| **C9** / **D5** measure definitions | **Keep** | ngspice's `meas` interpolates linearly (20.083 Hz); the refined crossing is 20.127 Hz, round 2's number |
| **C10** engine-grade tolerances | **Keep as a setting, drop as a blocker** | On ngspice, slopes are clean even at defaults. reltol 10⁻⁶ has no cost I could measure |
| **C11** instant tier | **Cut, for blocks this size** | A re-check is 0.14–0.4 s. Stored forms and surfaces are wrong for real edits (−4.0 Hz, 42.9 Hz vs 12.2 Hz) |
| **C12** content cache, rounds | **Keep** | With C12's value-based key, pinning β 70..300 keeps the 128 corners at β = 300 cached, by construction |
| **D2**, **D7**, **D8**, **D10** | **Keep** | — (D10: add a verdict kind "PASS (corners)") |
| **D6** cold runs | **Automatic** | `reset` re-parses the deck every run |
| **D9** function-backed test backend | **Change** | The adversarial variants run as ngspice decks and reproduce round 2 (hiZ 8.831, 62/256 saturated; band 4.5019/4.4993). The full keys take 14 s, so the regression suite can run on ngspice in CI. A function backend is still handy for unit-testing driver logic |
| Worst-case report §6.3, "enumeration wins for n ≤ 6" | **Change** | On ngspice the crossover is wall-clock time: n_eff ≤ 10 (≤ 2.5 s on one core, ≤ 0.25 s on 16) |

---

## 7. Recommendations for the final plan

1. **Keep the worst-point loop as the engine's scalable core; do not replace it with a response surface, screening or sampling.** *Reason:* it is the best search on every suite here. It was exact on 231/240 random sides (misses ≤ 0.020%, all interior), and exact or within 4 ppm on all 12 adversarial sides with the piecewise rule. Its cost is linear to n = 32 (150 runs, 0.13 s). RSM needs ≥ (n+1)(n+2)/2 runs and missed by up to 17.5%. Screening gave 4 false PASSes. LHS gave 949.
2. **Route `worst_case` by n_eff: enumerate every corner when 2^n_eff ≤ 1024, else run the loop.** Add the interior check (inward nudges, then golden section) at the worst corner of power-type measures. Consider triggering it for every side whose inward nudge improves: n runs per side, not tested here. *Reason:* 1,920 of 1,920 bench verdicts right with 0 UNDECIDED, and 12 of 12 adversarial verdicts even without the interior check. It takes 0.4 s on one core for the MVP, and M3b builds exactly this anyway. Without the interior check it gave 7 false PASSes (P_Q peaks inside the box by up to 1.03%).
3. **Report enumeration's PASS as its own kind, "PASS (corners)"**, next to FAIL, PASS (estimated) and PASS (guaranteed, corner theorem). *Reason:* it is exact over the vertex set and uses no model, but it is not a proof over the box.
4. **Replace C6's allowance: after the 1-flip certificate, flip the 6 pairs of the 4 lowest-loss knobs.** Use e₂ = the largest non-additivity as the PASS allowance for smooth library measures and for `min`/`max` searched piece by piece. Keep e_obs for measures that can hide a kink. *Reason:* e₂ confirms 96% of true PASSes at a margin of 10% of the spread (e_obs: 50%), with 0 false PASSes in 1,932 verdicts. Without the piecewise rule it gives a false PASS on `swing`, which is why the split by measure kind is required.
5. **Make piecewise search mandatory for any measure the language knows is a `min`/`max` of pieces.** *Reason:* it turns the kink case from UNDECIDED into a correct FAIL for 8 runs, and recommendation 4 depends on it.
6. **Run the loop on the MVP anyway, as a cross-check against enumeration, and fail the test suite on any disagreement.** *Reason:* after enumeration the loop's vertex runs are cache hits. It needs about 15 runs that aren't corners (9 of them shared with R0), and it validates the scalable path on every check.
7. **Keep round 2's k-σ search for `sigma(k)` (C7) unchanged.** *Reason:* exact on 5/5 sides. Enumeration can't answer `sigma(k)`, and Monte Carlo at 2,000 boards can't resolve 3σ.
8. **Cut the "instant tier" (C11) from the plan for block-sized circuits: `what_if` on a design edit = `verify`.** *Reason:* 0.14–0.4 s per re-check on ngspice. Stored forms and surfaces were wrong by 16–31 Hz on the C_in edit. Surface predictions may be shown only for sub-box questions, labeled "predicted".
9. **Do not add a covering array (PB12) or random vertices to the MVP's shared round.** *Reason:* the loop found every island without them, and they inflated e_obs (UNDECIDED 789 → 926). Revisit when behavioural models without device margins appear.
10. **Build the regression suite on ngspice decks: the MVP plus hiZ, pq, band, swing and THD, keys by enumeration plus refinement.** *Reason:* they reproduce round 2's nominal values to 4 digits and its failure structure (62/256 saturating corners on `hiZ`), and all six keys take 14 s. That is closer to the real product than a function backend (D9).
11. **Specify the loop's verdict rows per measure kind in M3a:** which measures are smooth, which are declared-piecewise, which are kink-capable. *Reason:* the allowance can only be tight where the structure is known (§3.4).

---

## 8. Open questions

1. **How far does e₂ generalize?** It was calibrated on one circuit family (240 random CE sides plus the variants). Op-amp stages, an LDO and a buck may have 3-knob interactions that 2-flips can't see. What is the smallest regression set that would convince us?
2. **The 1024-corner threshold.** It assumes ~1.5 ms per run. Transient specs (THD: 10.6 ms per run) would push the threshold down to about n_eff ≤ 7 on one core. Should the rule be a time budget ("enumerate if the estimate is < 2 s") rather than a corner count?
3. **Hidden kinks in transient measures.** THD clipping is exactly the regime e₂ can't see and the DC margin guard can't see either. Do we need a dynamic margin (for example the minimum VCE over the waveform) as a guard measure?
4. **Is "PASS (corners)" worth a separate word for the user**, or should it read "PASS (estimated)" with the method in the details?
5. **Joint range × statistical search past 4 range knobs** (synthesis question 7). Enumerating range corners × the k-σ point costs 2^R × (k-σ runs). Would a surrogate that only orders the range corners, in the Fast PVT style, be the right tool there?
6. **Parallel runs through libngspice.** The library holds global state, so parallelism means processes. Does the Rust engine drive a process pool of libngspice instances, or `ngspice -b` subprocesses at ~10–15 ms each?

---

## Appendix: scripts (`$R2/alternatives/`)

| Script | Produces | Output |
|---|---|---|
| `ngcore.py` | ctypes driver for libngspice | — |
| `check_backend.py` | §1 facts: temperature with `alterparam`, complex AC, timing | `out_check_backend.txt` |
| `noise_check.py` | §1: slopes vs nudge size, default vs engine tolerances | `out_noise_check.txt` |
| `ngamp.py` | the MVP and five variant decks, all measures, region and saturation margin | — |
| `key.py` | the answer key (§1) | `key.json` |
| `designs.py` | PB12, fractions, CCD, LHS, covering check, least squares with leave-one-out | — |
| `methods.py` | every method of §2 | — |
| `compare.py`, `summary.py` | §3.1, §3.2 | `compare.json`, `compare_out.txt`, `summary.txt` |
| `bench.py`, `bench_eval.py` | §3.3 (40 random designs, 1,920 verdicts) | `bench_raw.json`, `bench_eval.txt` |
| `allow_test.py` | §3.4 allowance variants, hidden kink | `out_allow_test.txt` |
| `scale.py` | §3.5 cascade n = 8, 20, 32 | `out_scale.txt`, `scale.json` |
| `extras.py` | §3.6 sampling and covering; §5.2 fix and add-a-spec flows | `out_extras.txt` |
| `agent_flows.py` | §3.7 what-if through the surface vs re-check | `out_agent_flows.txt` |
| `sigma.py` | §3.8 | `out_sigma.txt`, `sigma.json` |
| `par.py` | §3.9 parallel enumeration | `out_par.txt` |
