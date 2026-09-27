# Engine Position: The Minimalist

> 2026-09-27 · Round 3, one of five positions. The question: what is the smallest engine that ships a trustworthy `spicy check circuits/ce_amp.spl` on ngspice?
> Reads with: `engine_synthesis.md` (round 2: C1–C12, D1–D10) and the six reports next to it, `../engine.md`, `../walkthrough.md`, `../roadmap.md` §4.2 and §6.
> **Where the numbers come from.** Every circuit number was computed on ngspice-42 by a stdlib-Python script in the scratch folder `/root/.claude/jobs/443154a8/tmp/engine2/minimal/` (called `$M` below). The script is named next to each result, and §0 lists them all. Other numbers are cited (file:line or report section). The machine was shared with other jobs, so wall times are given as the range measured.

## Summary

At the MVP's size, `worst_case` doesn't need a loop. On ngspice, driven as one batch process per group of points, all 256 corners of `ce_amp.spl` take 0.26 s (0.10 s spread over 8 processes). Enumerating them gives the exact corner extreme of every spec side, and the 1-flip certificate comes free. It also simulates every corner where a limiter switches on. That makes the flip walk, the margin guard, the below-noise rule, lazy multistart, the rounds driver and the run cache unnecessary up to about 12 knobs. What corners can't show is an interior extremum. A first-order check at each side's worst corner (one inward nudge per knob) catches that and says UNDECIDED (interior). `sigma(3)` runs only where `worst_case` doesn't pass (nesting, C7), using round 2's k-σ point with the exact map.

The prototype is 270 lines of engine plus a 107-line ngspice backend. It matches an ngspice brute-force answer key exactly on all 8 `worst_case` sides of the MVP and three adversarial variants. It has **0 false PASSes and 0 bracket violations** across 12 `sigma(3)` results, checked against an independent σ key. That result needed one fix, which is new evidence for round 2's open question 4. As round 2 specified it, the σ search (start at the nominal-ranked range corner, then cross-check the other corners) gives a **false PASS** on the saturating variant: 8.66 reported, 5.48 true. On the two-band-edge variant its bracket misses the truth. Running the σ search a second time, starting from the `worst_case` counterexample, fixes both.

A full MVP check is 389 runs and takes 0.75–0.97 s serially. The Rust MVP comes to about 1,850 lines in ten modules. It keeps the seams the later pieces need: a batch `Backend`, one run table and a worst-case `Strategy`. The later pieces are the flip walk (past 12 knobs), the editor and the agent.

---

## 0. What was done, and how to re-run it

| Script (in `$M`) | What it does |
|---|---|
| `ng.py` | The backend. It writes one deck per batch: the circuit with every knob as a `.param`, plus a generated `.control` block (`alterparam` … `reset`, `op`, `print`, `ac lin 1 1k 1k`, `ac dec 50`, `wrdata`). It runs `ngspice -b` once and parses the output. It also holds the measures: `f_low` (literal −3 dB below max\|H\|, cubic interpolation in (ln f, dB)), \|H(1 kHz)\| and the BJT region |
| `ce.py`, `variants.py` | The MVP deck, standing in for the M1f export, plus three adversarial variants rebuilt from `engine_math_guards.md` §1.1: `hiz` (saturation off at nominal), `band` (a second band edge, 9 knobs), `pq` (an interior power maximum) |
| `calibrate.py` | Sets each variant's bound from ngspice's own numbers so that it keeps its trap (`calibration.json`) |
| `answer_key.py` | The brute-force key: all 2ⁿ corners, 1,000 seeded interior points, then a coordinate search (steps 0.25 → 10⁻³) from each side's best point. Same backend and measures as the engine (`key_*.json`) |
| `sigma_key.py`, `sigma_key_side.py` | An independent `sigma(3)` key: at every range corner, 2,000–3,000 random points on the 3-sphere in u-space, then a derivative-free pattern search on the sphere. Plus a 20,000-board Monte Carlo |
| `engine_min.py` | The minimal engine: run table, enumeration, interior check, `sigma(k)`, numerical band, verdicts, table |
| `walk.py`, `run_walk.py` | The flip walk (the path past 12 knobs), forced on the same cases, with and without its margin side |
| `suite.py` | The final evidence: engine vs both keys on all four circuits |
| `flows.py` | The agent flows of §6, with their costs |
| `t_temp.cir`, `t_ac.cir`, `t_speed.py`, `t_acc.py`, `t_tol.py`, `t_transport.py` | ngspice behaviour checks (§2) |

**The model.** The deck of roadmap §4.2 with `.model QNPN NPN(IS=1e-14 BF={beta} XTB=1.5)`, `.options tnom=25` and `.temp {temp_c}`. XTB = 1.5 gives β about +0.5 %/K, which is the walkthrough's tempco. TNOM = 25 °C makes β = 200 hold at the nominal 25 °C (ngspice's default TNOM is 27 °C). Knob positions are ε ∈ [−1, 1] and x = mid + rad·ε. The nominal of each knob is the middle of its range.

---

## 1. The position, and what it means for round 2

### 1.1 The position

> **Use the cheapest method that is exact for the design in front of you, and keep only the guards that method needs.**

Three claims, each tested below:

1. **For n ≤ 12, `worst_case` = enumerate the corners + a first-order interior check.** The search then has no error at the corners. Every corner-located limiter is found by construction, and the interior check turns the one remaining blind spot into UNDECIDED (interior). The answer key *is* the engine, so M3b and M3c merge.
2. **`sigma(k)` only where the verdict needs it.** A `worst_case` PASS implies a `sigma(k)` PASS (C7). On the MVP that leaves one σ search (bias ≤ 6.5 V) instead of five.
3. **Everything else waits for its first consumer.** That means the affine `Remainder` enum, the run cache, the rounds driver, JSON, the instant tier and the function backend. The seams that keep them cheap later are a batch `Backend`, one run table per check and a worst-case `Strategy`. They cost almost nothing now (§5).

### 1.2 Round 2's changes C1–C12, from this position

| # | Round 2 says | Minimal MVP | Evidence |
|---|---|---|---|
| C1 | Stop on optimality; accept only improvements | **Moot** for enumeration. **Keep** for the walk (`walk.py`: only improving flips; stop when none improves) | Enumeration has no stop rule |
| C2 | Flips at vertices | **Subsumed.** Every 1-flip neighbour is a corner, so the flip table at the worst corner comes free (§6.2) | `flows.py`: 8 flips read from the table, 0 runs |
| C3 | Saturation-margin guard | **Cut** from the MVP. **Change its form** for the walk: a margin is an automatic spec side searched by the same code, not a separate mechanism | hiZ: enumeration sees 62/256 saturated corners and FAILs gain at 3.994 exactly. The walk without its margin side gives a **false PASS** (8.504 vs 3.994, §4.7) |
| C4 | Every run updates every spec | **Keep.** It's free: one table, each side's best row | Pooling is what lets the σ second start work (§4.5) |
| C5 | Below-noise knobs go to an edge | **Moot.** Enumeration visits both edges. The walk puts *every* knob at an edge, so no threshold τ exists | — |
| C6 | Verdict table (7 rows) | **Change:** enumeration needs FAIL / numerics / simulator / interior / PASS. `e_obs`, regime and search belong to the walk and to σ | §4.4 |
| C7 | σ: exact map, k-σ point, nesting, uniform only when needed, per side | **Keep all.** **Add:** a second start from the `worst_case` counterexample, a bracket from the last jump's miss (with convergence required), and a 0-run "regime reachable" rule | §4.5: two σ holes found and fixed |
| C8 | Affine form with a `Remainder` enum and a separate band | **Cut.** The MVP has no proven remainders and combines no forms. A derived measure is evaluated per run: `vcc.v − dc(output.v)` is exact over the corners by construction | `top_room` min 5.1951 V, 8 new runs (§6.2) |
| C9 | Exact measures | **Keep the definitions.** On ngspice, f_low is 20.12694 Hz (literal −3 dB below the max), and \|H(1 kHz)\| is taken exactly with `ac lin 1 1k 1k`. **Change the method:** cubic interpolation in (ln f, dB) at 50 points/decade (error 2.1e-7) instead of secant refinement, which in batch mode needs data-dependent AC points and a round trip. `Value \| Undefined` only; `Beyond` comes later, and `Undefined` never passes | `t_acc.py` |
| C10 | Engine-grade tolerances (M2e) | **Keep.** On ngspice it's one line: `.options reltol=1e-6 vntol=1e-9 abstol=1e-15` | VC error 1.1e-13 V (`t_tol.py`) |
| C11 | Instant re-evaluation needs a rework | **Defer.** A full check takes under 1 s, so the agent never needs an estimate (§6.3) | — |
| C12 | Content-keyed run cache; loops in rounds | **Cut.** A per-check dedup map is enough. The corner set is one batch, and σ is a few sequential batches for one side | 389 runs in 27 processes |

### 1.3 Decisions D1–D10, from this position

| # | Round 2's recommendation | This position | Why |
|---|---|---|---|
| D1 | Nudges at nominal, flips at vertices | **Moot for the MVP.** Nudges only in the interior check and σ. Flips return with the walk | Enumeration contains every flip |
| D2 | Optimality + accept only improvements | Keep, for the walk | — |
| D3 | Exact map | **Keep** | The σ key agrees: 6.28132 key vs 6.28129 engine |
| D4 | σ per side | Keep | — |
| D5 | Literal −3 dB below max\|H\| | Keep | 20.12694 Hz on ngspice |
| D6 | Cold start every run | **Free on ngspice.** `reset` throws away the circuit and rebuilds it from the altered deck (`runcoms2.c:173-185`, `inp.c:412-416`). Results are bit-identical at any batch position | `t_acc.py` |
| D7 | Both confidences always | **Disagree.** Compute σ only where `worst_case` isn't PASS. `--realistic` fills the column for every side | MVP: 389 runs / 0.75–0.97 s vs 541 runs / 1.5 s |
| D8 | Keep β 100..=300 | Keep | Pinning the 2N3904 makes bias UNDECIDED (distribution) at σ(3) on ngspice (§6.2) |
| D9 | Function-backed test backend first | **Cut.** The backend is ngspice, and the adversarial variants are real decks (`variants.py`), so there is nothing to build and later throw away | 4 decks, ~60 lines |
| D10 | Table + versioned JSON | **Defer JSON** to its first consumer (editor, MCP). The terminal table is the contract, snapshot-tested | No reader of the JSON exists yet |

---

## 2. Driving ngspice

### 2.1 Three transports, measured

256 corners of the MVP, each point an op + AC at exactly 1 kHz + a 50/decade sweep from 0.1 Hz to 100 kHz (`t_transport.py`, `t_speed.py`):

| Transport | 256 points | Per point | Notes |
|---|---|---|---|
| One process per point (netlist text per point) | 1.50–1.54 s | 5.9–6.0 ms | A single-point process: 7.2 ms |
| **One process per batch (`alterparam` loop)** | **0.26 s** | **1.0 ms** | op only: 0.25 ms/point; 20/decade sweep: 0.69 ms/point |
| libngspice in-process (ctypes, `alterparam`) | 0.16 s | 0.63 ms | Values equal the batch's to 1.9e-16 (VC), 1.2e-15 (\|H\|), 1.9e-15 (f_low) |
| Batch split over 4 / 8 / 16 processes | 125 / 101 / 91 ms | — | 20 cores |

**Recommendation: one `ngspice -b` process per batch.** It costs 1.6× libngspice, and it avoids a C FFI with callbacks, the library's global state (one circuit per loaded library) and a crash taking down `spicy`. Parallelism is simply more processes. libngspice stays an option behind the same trait if the overhead ever matters.

### 2.2 What was verified before relying on it

| Behaviour | Result | Where |
|---|---|---|
| `.temp {temp_c}` follows `alterparam` + `reset` | Yes: 60 °C by `alterparam` gives VC = 5.182592 V, the same as a fresh deck at 60 °C. −10 °C: 5.830125 V both ways | `t_temp.cir`; `alterparam` edits the stored deck (`inp.c:1815-1823`), and `reset` rebuilds the circuit from it (`runcoms2.c:173-185`) |
| Every run is cold and pure | Yes. The nominal point gives bit-identical VC and H(1 kHz) at position 0 and at the end of a batch, and in a batch of one | `t_acc.py` |
| Full precision in text output | `set numdgt=15` gives 15 significant digits in `print` and `wrdata` | `t_ac.cir`; `options.c:356` |
| One file for all sweeps | `set appendwrite` appends each `wrdata`; complex data comes out as (f, re, im) | `t_ac.cir`; `wrdata` is `ft_writesimple`, `plotting/gnuplot.c:695-704` (format), `:723, :757` (append) |
| Device op data | `@q1[ic]`, `[ib]`, `[vbe]`, `[vbc]`, `[gm]`, `[gmu]` print after `op`. In forward active, gmu sits at GMIN = 1e-12 (`bjtload.c:490`) | `t_temp.cir`; `bjtask.c:116` |
| Tolerances | Default reltol 1e-3: VC 60 µV off, gain 4.3e-5 off. reltol 1e-6: 1.1e-13 V. Nudged slopes agree across h = 1e-2…1e-4 at both settings (β: −0.14035 / −0.14096 / −0.14102, which is curvature, not noise) | `t_tol.py` |
| f_low interpolation | Cubic in (ln f, dB), max relative error against a 2,000/decade reference: 1.3e-4 (10/dec), 8.2e-6 (20), **2.1e-7 (50)**, 1.3e-8 (100) | `t_acc.py` |

**The region rule on ngspice.** Round 2 defined the saturation edge as the BC current reaching 10⁻⁴ of IC, chosen from small-signal loading (`engine_math_guards.md` §2.4). ngspice reports conductances directly, so the same idea becomes **saturated when gmu > 10⁻⁴·gm**, and cutoff when IC ≤ 0. It needs no IS, BR or Vt(T) from the engine.

### 2.3 The deck, and what M1f must give

```
 <M1f export>                         .param temp_c=25 vcc=12 r1=47k … beta=200
                                      .options reltol=1e-6 vntol=1e-9 abstol=1e-15 tnom=25
                                      .temp {temp_c}
                                      <elements with {param} values> ; .model … BF={beta} XTB=1.5
 <generated per batch>                .control
                                      set numdgt=15 ; set appendwrite
   per point ──────────────────────►  alterparam temp_c=-10 … alterparam beta=100 ; reset
                                      op ; echo @P 17 ; print v(out) … @q1[gm] @q1[gmu]
                                      ac lin 1 1k 1k ; print v(out)          ← one per at(f)
                                      ac dec 50 0.1 1e5 ; wrdata sweep.txt v(out)
                                      destroy all
                                      .endc
```

So the M1f export needs three things beyond a plain netlist: every knob as a `.param`, `.temp {temp}` and a TNOM decision. The analyses and frequencies come from the contract. A sweep is 0.1 Hz … 100 kHz here: two decades below the bass bound and two above the 1 kHz point, as `engine_math_affine_measures.md` §3.4 recommends.

---

## 3. The answer key on ngspice

`answer_key.py`: 256 corners + 1,000 seeded interior points + a coordinate search per side, 1,576 runs in 2.1 s.

| Side | Corners | Best of 1,000 random | After search | Verdict | Round 2 (Python model) |
|---|---|---|---|---|---|
| bias: VC ≤ 6.5 V | **6.559537** | 6.236494 | 6.559537 | **FAIL** | 6.5905 |
| bias: VC ≥ 4.5 V | **4.692115** | 4.947826 | 4.692115 | PASS | 4.6597 |
| gain ≤ 4.83 | **4.702828** | 4.690906 | 4.702828 | PASS | 4.7030 |
| gain ≥ 4.37 | **4.461772** | 4.481471 | 4.461772 | PASS | 4.4626 |
| bass: f_L ≤ 30 Hz | **26.789724** | 25.787064 | 26.789724 | PASS | 26.738 |

- **Same verdicts as round 2, different numbers**, as the task expected (ngspice's Gummel–Poon with XTB and IS(T), vs the walkthrough's simplified model). Nominal: VC 5.503227 V, \|H(1 kHz)\| 4.590771, f_L 20.126944 Hz.
- **Every extreme is at a corner.** The coordinate search never beat the best corner, and random sampling is a poor worst-case finder (6.236 against 6.560).
- **All 256 corners are forward active.**

The variants, with bounds set by `calibrate.py` so that each keeps its trap:

| Variant | ngspice facts | Side (bound) | Key |
|---|---|---|---|
| `hiz`: R1 470k, R2 100k, RC 9.1k, C_in 100 nF | Nominal gain 8.8313, VCE 1.634 V, active. **62 of 256 corners saturate** (round 2's model also had 62) | gain ≥ 8.39 | **3.994366, FAIL** (saturated corner) |
| | | gain ≤ 9.273 | 9.065226, PASS |
| | | VC ≥ 1.197 V | 1.287051, PASS (worst point saturated) |
| `band`: C_L 690 pF ± 10% at the output (9 knobs) | Nominal band edges tie: \|H(100 Hz)\| 4.5018, band min 4.4992 (at 10 kHz). With C_in held at nominal the best is 4.3574 | min\|H\| over 100 Hz–10 kHz ≥ 4.3354 | **4.313479, FAIL** (C_in low, 100 Hz edge) |
| `pq`: RC 3.6k | P_Q nominal 7.79314 mW; the interior beats every corner | P_Q ≤ 8.70831 mW | corners **8.706944**, interior **8.709679** (β ε = −0.006): **FAIL**. A corners-only method would PASS |

---

## 4. The minimal engine

### 4.1 On one page

```
 spicy check ce_amp.spl
   │  knob table (8) · deck with .param per knob (M1f)
   ▼
 WORST_CASE  (n = 8 ≤ 12: enumerate)
   batch: nominal + 2^n corners ─────────────────────────────►  257 runs, 1 process
   every run ► every measure + q1 region ► one row of the run table
   per side: inner bound I = best row; counterexample = its corner          0 runs
   interior check at each side's worst corner: one inward nudge per knob
     (a capacitor can't move a dc() measure, so it's skipped there) ───►  ≤ 38 runs, 1 process
     a nudge that improves → 1 run at the parabola's peak (the far edge is
     already in the table) → FAIL if it violates, else UNDECIDED (interior)
   ▼
 SIGMA(3)  only for sides whose worst_case isn't PASS (nesting)
   4 range corners at nominal parts → rank → AMV k-σ search, slopes in ε, exact map,
   point + its nudges in one batch, ascent check, ≤ 10 rounds
   second start: the worst_case counterexample projected onto the 3-ball
   cross-evaluate u* at the other range corners; switch if one is worse
   uniform map: only when the default passes and worst_case fails ─────►  bias.max: 87 runs
   ▼
 ε_num: re-run each decisive point at reltol 1e-9, + the measure's own band ─►  ≤ 7 runs
   ▼
 verdicts (§4.4) ► terminal table
```

### 4.2 `worst_case`: what enumeration + interior check proves

| Claim | Enumeration + interior check | The walk (round 2's C1–C5) |
|---|---|---|
| Inner bound vs the best corner | **Equal**, by construction | Equal only if the walk reaches it |
| A limiter that switches on at some corner | **Always in the table** | Found only if a walk or a guard reaches it (margin guard, range corners, pooling) |
| 1-flip optimality of the answer | Free (every neighbour is a corner) | N runs per round |
| Joint flips (round 2 open question 2) | Free | Unsolved (the buck needed 3 knobs flipped together) |
| Interior extremum next to the worst corner | Detected to first order → UNDECIDED (interior), or FAIL if the parabola's peak violates | Needs its own trigger |
| Interior extremum not next to the worst corner | **Missed** (see §4.6) | Missed |
| Runs, MVP | 1 + 256 + 38 = **295** | 106 (§4.7) |

So for n ≤ 12 the only remaining assumption is "no hidden interior extremum away from the worst corner". The walk has that same assumption plus the search's own risk.

### 4.3 The verdict rows

For an upper bound B (a lower bound mirrors it). I is the inner bound and ε_num the numerical band. The first matching row wins.

| # | Condition | Verdict | Used by |
|---|---|---|---|
| 1 | I > B + ε_num | **FAIL**, with the counterexample | all |
| 2 | \|I − B\| ≤ ε_num | UNDECIDED (numerics) | all |
| 3 | a needed run failed | UNDECIDED (simulator) | all |
| 4 | the interior check found an inward improvement (and the parabola's peak didn't violate) | UNDECIDED (interior) | worst_case |
| 5 | σ: search not converged | UNDECIDED (search) | σ, walk |
| 6 | B − I > e + ε_num: e = 0 for enumeration; the last jump's miss for σ; round 2's `e_obs` for the walk | **PASS (estimated)** | all |
| 7 | otherwise | UNDECIDED (bracket) | σ, walk |
| + | σ PASS while the corner table shows a device outside its nominal region | UNDECIDED (regime reachable) | σ |
| + | σ PASS under the default map, FAIL under uniform | UNDECIDED (distribution) | σ |

ε_num = 2·\|f(reltol 1e-6) − f(reltol 1e-9)\| at the decisive point, plus the measure's interpolation band (10⁻⁶ relative for sweep-interpolated measures, from the 2.1e-7 measured). PASS (guaranteed) doesn't exist in the MVP.

### 4.4 Results against the keys

`suite.py` (the σ keys come from `sigma_key.py` and `sigma_key_side.py`):

| Circuit | Side | worst_case | Verdict | Key | σ(3) default | σ(3) uniform | σ key (def / uni) |
|---|---|---|---|---|---|---|---|
| MVP | VC ≤ 6.5 | 6.55954 | **FAIL** | 6.55954 FAIL | 6.28129 PASS | 6.46567 PASS | 6.28132 / 6.46567 |
| | VC ≥ 4.5 | 4.69211 | PASS (est.) | 4.69211 | implied PASS | — | 4.88424 |
| | gain ≤ 4.83 | 4.70283 | PASS (est.) | 4.70283 | implied PASS | — | 4.66355 |
| | gain ≥ 4.37 | 4.46177 | PASS (est.) | 4.46177 | implied PASS | — | 4.51762 |
| | f_L ≤ 30 Hz | 26.78972 | PASS (est.) | 26.78972 | implied PASS | — | 24.97348 |
| hiZ | gain ≥ 8.39 | 3.99437 | **FAIL** (saturated) | 3.99437 FAIL | 5.47748 **FAIL** | — | 5.47744 / 4.51548 |
| | gain ≤ 9.273 | 9.06523 | PASS (est.) | 9.06523 | implied PASS | — | — |
| | VC ≥ 1.197 | 1.28705 | PASS (est.) | 1.28705 | implied PASS | — | — |
| band | min\|H\| ≥ 4.3354 | 4.31348 | **FAIL** | 4.31348 FAIL | 4.41693 PASS | 4.34672 PASS | 4.41682 / 4.34658 |
| pq | P_Q ≤ 8.70831 | 8.70694 | **UNDECIDED (interior)**: β flagged; parabola peak 8.70799 | 8.70968 FAIL | 8.68486 PASS | 8.70165 PASS (e 4.5e-3) | 8.68498 / 8.70503 |

- **worst_case:** 8 of 8 values equal the key to every printed digit. 7 verdicts are right, and 1 is an honest UNDECIDED where a corners-only method would have given a false PASS (pq).
- **The MVP's `realistic` column** (`--realistic`) matches the σ key on every side, within 4e-5.
- **Every σ bracket [I, I + e] covers the σ key.** For pq uniform: I = 8.70165, e = 0.0045, truth 8.70503.
- **A 20,000-board Monte Carlo** at the cold/12.6 V corner (truncated normal): max VC 6.38735, 99.865% quantile 6.28261 (the σ(3) value is 6.28129), 0 boards above 6.5 V.
- **The MVP prints** bias = FAIL at `worst_case` (T −10 °C, VCC 12.6 V, R1 47.47k, R2 9.9k, RC 4.653k, RE 1.01k, β 100), and PASS at σ(3) (6.2813 V at R1 47.10k, R2 9.980k, RC 4.692k, RE 1.0016k, β 111.7; uniform 6.4657 V). Everything else is PASS. That's round 2's headline with ngspice's numbers.

**Cost** (serial, `flows.py` and the final timing): the MVP check is **389 runs in 27 processes, 0.75–0.97 s**. `worst_case` alone is 300 runs in 8 processes, 0.40–0.47 s. The worst-case part of the check is 295 runs in 7 batches; σ for bias.max, both maps and both starts, is 87 runs in 19 batches; ε_num is 7 runs in 1. The variants take 0.4–1.2 s each.

### 4.5 New: two holes in the σ search, and the fix

Round 2 never ran `sigma(k)` on an adversarial circuit (synthesis open question 4). My first σ version was round 2's method as specified: rank the range corners by the nominal-parts value, search the top one, cross-evaluate u* at the others (`engine_math_statistical.md` §6, rec. 8). It failed twice.

| Case | Round 2's σ method | Independent σ key | What went wrong |
|---|---|---|---|
| hiZ gain ≥ 8.39, default map | **8.6645, PASS** | **5.47744, FAIL** (saturated, at the hot / 12.6 V corner) | A zero-slope limiter again. The search starts at an active corner, and the line there points to low β, away from saturation (which needs high β and heat). The saturated σ point is at a range corner the cross-check never reaches, because u* there looks harmless |
| hiZ, uniform map | 8.5429, PASS | 4.51548, FAIL | Same |
| band, uniform map | 4.3793 (last-jump bracket 8e-4) | 4.34658 | A second extremum. At the 10 kHz edge C_in's slope is 0, so u(C_in) = 0 and the search never sees the 100 Hz edge. With a bound between 4.3466 and 4.3793 this would be a false PASS. The all-jumps `e_obs` (3.5e-3) doesn't cover it either |

Per range corner, the hiZ key is (T−, VCC−) 8.65588, (T−, VCC+) 8.69210, (T+, VCC−) 7.24688 saturated, (T+, VCC+) 5.47744 saturated. 2,077 of 14,076 sphere points saturate.

**The fix that suffices here: start the σ search a second time from the `worst_case` counterexample, projected onto the k-ball** (same range corner, equal |u| per knob, the corner's signs). It is generic, needs no device knowledge, and uses information the enumeration already paid for. The results are in the table of §4.4: hiZ becomes 5.47748 FAIL, band becomes 4.41693 / 4.34672, and the MVP doesn't change (+42 runs). Ablation (`engine_min.py` flags):

| σ guards | hiZ gain.min | band (def / uni) | MVP runs |
|---|---|---|---|
| Round 2's method | **false PASS** 8.6645 | 4.4262 / 4.3793, bracket misses the truth | 347 |
| + σ search on the q1 VCE margin (an automatic side; its in-ball runs pool) | FAIL 5.4916 | still misses | 347 (0 extra: no corner leaves forward active) |
| **+ second start from the counterexample** | **FAIL 5.4775** | **4.4169 / 4.3467 ✓** | 389 |

The second start alone fixes both. I also keep a **0-run rule**: if the corner table shows any device outside its nominal region, a σ PASS becomes UNDECIDED (regime reachable). A local search can't vouch for a kink it may not have crossed. It costs the MVP nothing (all 256 corners active). The margin search is its upgrade path, for when such an UNDECIDED needs resolving.

**The σ bracket.** Round 2 left two readings open. The guards report's `e_obs` takes the largest miss over all jumps. The statistical report's rule stops when the last jump's miss is within δ. On the MVP's uniform check the first jump missed by 0.102 V, larger than the 0.034 V margin, so the all-jumps rule says UNDECIDED, while the truth is PASS (key 6.46567 ≤ 6.5). **The last jump's miss, with convergence required, covered the key in 12 of 12 σ results**, and all-jumps added nothing on the one case that mattered (band, fixed by the second start). I use the last jump.

### 4.6 When each piece stops sufficing

| Piece | Stops sufficing when | Then | Evidence |
|---|---|---|---|
| Enumeration | 2ⁿ·t_run exceeds the budget (default 2ⁿ > 4096, n > 12; at 1 ms/run that's 4 s serial) | The flip walk with its guards (§4.7) | MVP walk 106 runs, exact |
| Interior check at the worst corner only | An interior extremum along an edge that doesn't touch the worst corner, higher than it (a knob moving a resonance through a fixed frequency; phase margin vs C_load) | Such measures declare "interior possible" and get a real interior search (golden section along the flagged knob, ~8 runs) | pq: flagged (β), parabola peak 8.70799 < true 8.70968 → UNDECIDED, not FAIL |
| Corner extremes for the key | Power-type or derived measures | The key keeps its interior search; it already has one | pq: corners 8.70694, interior 8.70968 |
| σ with two starts | A σ-reachable limiter that neither the nominal-parts line nor the counterexample direction points to | The regime rule gives UNDECIDED; the σ margin search resolves it | hiZ: the margin search alone gives FAIL 5.4916 |
| Min/max-over-axis measures (`.band()`, windows) at σ | More than two local extrema | Per-extremum sub-searches (Danskin, round 2 §1.1 e) | Not in the MVP language subset |
| `Value \| Undefined` | A DC-coupled design has no low cutoff | Add `Beyond` (C9) | Undefined never passes, so it's safe meanwhile |

### 4.7 Past 12 knobs: the flip walk, and why it needs its guards

`walk.py` is round 2's guarded core, trimmed. It takes nominal + n nudges + the 2^R range corners, sends every knob to the edge its slope picks, flips each knob (one batch), moves to the best improving flip, and stops when none improves. Device margins are automatic sides. Its verdict adds `e_obs`, regime and search rows. Forced on the same cases (`run_walk.py`):

| Case | Walk runs (processes) | Result | Key |
|---|---|---|---|
| MVP, 5 sides | 106 (17), 223 ms | all 5 exact; same verdicts | ✓ |
| hiZ, 3 sides | 78 (12) | gain FAIL 3.99437; gain ≤ and VC ≥ are **UNDECIDED (bracket)** (`e_obs` 4.54 and 1.37: the nominal line misses at saturated range corners) | FAIL / PASS / PASS |
| band | 50 (6) | FAIL 4.31348 | ✓ |
| pq | 47 (7) | UNDECIDED (interior) | FAIL |
| **hiZ, gain spec only, with the margin side** | 38 (5) | FAIL at 4.3284 (a saturated board, not the worst) | 3.994 FAIL |
| **hiZ, gain spec only, without the margin side** | 31 (4) | **PASS (estimated) 8.5039: false PASS** | 3.994 FAIL |

The last row reproduces round 2's case (a) on ngspice. With the full hiZ spec set, pooling hid it: the VC ≥ search visits a saturated corner anyway.

So the walk is cheaper (106 vs 295 runs on the MVP) but needs four extra mechanisms to be safe: margin sides, range corners, `e_obs` and the regime row. It also turns two true PASSes into UNDECIDED. Enumeration needs none of that. **Build the walk when the first design with more than 12 knobs arrives, and accept it only when it agrees with enumeration on every regression circuit of 12 knobs or fewer.**

---

## 5. Architecture

### 5.1 Crates and types

It follows roadmap §2.2: `spicy_engine` owns the math and the `Backend` trait, `spicy_backends` holds the ngspice adapter, and `spicy_cli` prints.

```rust
// spicy_engine::knob — from spicy_model's FlatDesign (M1d)
pub enum KnobKind { Range, Statistical }
pub enum Dist { TruncNormal3, Uniform }            // σ = tol/3 truncated at tol; uniform for the robustness check
pub struct Knob { pub path: Path, pub kind: KnobKind, pub lo: f64, pub hi: f64, pub dist: Dist }
pub struct KnobPoint(Box<[f64]>);                  // ε per knob in [-1, 1]; x = mid + rad·ε

// spicy_engine::backend — the seam for ngspice now, libngspice or our simulator later
pub trait Backend {
    type Prepared;
    fn prepare(&self, design: &FlatDesign, knobs: &[Knob], needs: &Needs) -> Result<Self::Prepared, BackendError>;
    /// A batch in, results in order. Parallelism is the backend's business (split into processes).
    fn run(&self, p: &Self::Prepared, points: &[KnobPoint]) -> Vec<Result<RunRecord, SimError>>;
}
pub struct Needs { pub op: bool, pub ac_at: Vec<f64>, pub sweep: Option<Sweep>, pub probes: Vec<Probe> }
pub struct RunRecord { pub scalars: Vec<(Probe, f64)>, pub devices: Vec<DeviceOp>, pub sweeps: Vec<Vec<(f64, Complex64)>> }
pub struct DeviceOp { pub path: Path, pub region: Region, pub values: Vec<(&'static str, f64)> }

// spicy_engine::measure — our library on returned data (engine.md D13)
pub enum Measured { Value(f64), Undefined(&'static str) }      // + Beyond later (C9)

// spicy_engine::runs — one table per check; pooling is "best row"
pub struct RunTable { rows: Vec<(KnobPoint, Result<Vec<Measured>, SimError>)>, index: HashMap<KnobPoint, usize> }

// spicy_engine::worst_case
pub enum Strategy { Enumerate, FlipWalk }                      // FlipWalk after the MVP
pub struct WcResult { pub inner: f64, pub at: KnobPoint, pub interior: Vec<Suspect> }

// spicy_engine::sigma
pub struct SigmaResult { pub inner: f64, pub at: KnobPoint, pub last_miss: f64, pub converged: bool }

// spicy_engine::verdict
pub enum Verdict { Fail { at: KnobPoint }, PassEstimated, Undecided(Reason) }
pub enum Reason { Numerics, Simulator, Interior, Search, Bracket, RegimeReachable, Distribution }
```

**Why these are enough for later features without a rewrite:**
- A **batch** `run` lets parallel processes, libngspice or our simulator slot in behind it.
- The **run table** is exactly what a persistent cache later stores (C12), keyed per revision. Its rows are what `--explain` and the agent read.
- **`Strategy`** is where the flip walk plugs in. Its tests are the enumeration's answers.
- **`KnobPoint` in ε** is round 2's `KnobCoord` minus the `Scale` field. A log scale can be added as a field later without changing callers.
- **`Verdict` with reasons** is the table's contract, and JSON can serialize it later.

### 5.2 What waits, and until when

| Deferred | Until | Why it can wait |
|---|---|---|
| Flip walk, margin sides, range-corner net, `e_obs`, regime row, walk budget | The first design with n > 12 | Enumeration is exact and needs none of them (§4.2) |
| Interior search (golden section along a flagged knob) | The first case whose UNDECIDED (interior) blocks a decision | The MVP never flags one |
| Affine type, `Remainder` enum, separate band (C8) | Datasheet arithmetic (M5), hierarchy | Per-run evaluation handles derived measures exactly |
| Content-keyed persistent cache, rounds driver, loop state machines (C12) | `spicy watch`, the editor | A one-shot check is 27 batches |
| Instant tier, log-unit forms (C11) | Editor, with slow circuits | The full check is under 1 s |
| JSON with a versioned schema (D10) | Its first reader | The table is snapshot-tested |
| `Beyond` measure value | The first DC-coupled `f_low` spec | `Undefined` is safe |
| σ margin search | A regime-reachable σ UNDECIDED that matters | The 0-run rule is safe |
| libngspice FFI | Process overhead dominating (larger decks re-parse per point) | 1.0 vs 0.63 ms/point |
| σ for every side (D7) | On demand (`--realistic`) | Nesting settles 4 of 5 MVP sides |
| Function-backed test backend (D9) | Never, if CI has ngspice | The variants are real decks |

### 5.3 Size

**The prototype, in code lines** (no blank, comment or docstring lines):

| Part | Lines |
|---|---|
| Run table, dedup, pooling | 30 |
| worst_case: enumeration + interior check | 33 |
| sigma(k): maps, k-point solver, AMV, two starts, cross-corners, ball | 94 |
| Numerical band | 12 |
| Verdict | 17 |
| Driver | 60 |
| Terminal table | 14 |
| **Engine (`engine_min.py`)** | **270** |
| ngspice backend + measures (`ng.py`) | 107 |
| Flip walk, for later (`walk.py`) | 36 |
| Answer key / σ key (tests) | 61 / 58 |

For comparison, round 2's `worst_case` loop alone is 286 non-blank lines (`worst_case/wcloop.py`), and its guarded loop 138 + 106 (`guards/guarded.py`, `guards/loop.py`).

**Rust estimate per module.** My assumption is about 4–5× the Python code lines, for types, error handling and parsing (and 10–15× for the thin ones), with tests excluded. These are estimates, not measurements.

| Module | Rust lines (est.) |
|---|---|
| `spicy_engine::knob` (from `FlatDesign`, ε↔value, the two maps) | 150 |
| `spicy_engine::backend` (trait, `RunRecord`, errors) | 80 |
| `spicy_backends::ngspice` (deck, control block, spawn, parse stdout + wrdata, batch split) | 300 |
| `spicy_engine::measure` (dc, at·mag, f_low, region) | 200 |
| `spicy_engine::runs` | 120 |
| `spicy_engine::worst_case` (enumerate + interior check) | 150 |
| `spicy_engine::sigma` | 350 |
| `spicy_engine::verdict` + ε_num | 150 |
| `spicy_engine::check` (driver) | 150 |
| `spicy_cli check` (table, `--at`, `--explain`, `--realistic`) | 200 |
| **Total** | **≈ 1,850**, plus ≈ 400 lines of answer-key test harness and 4 variant decks as fixtures |
| Later: flip walk + margin sides | ≈ 250 |

---

## 6. Flows

### 6.1 The cold check

```
 spicy check circuits/ce_amp.spl
 ─────────────────────────────────────────────────────────────────────────────────────
 parse · elaborate (M1d) · knob table: 8 knobs · SPICE export with .param per knob (M1f)
   │
   ├─ batch     nominal + 256 corners · op + AC(1 kHz) + sweep       257 runs  ≈ 0.26 s
   │            (split over 8 processes: ≈ 0.10 s)
   │            → run table: VC, |H(1k)|, f_L, q1 region per corner
   │            → bias.max I = 6.5595 V FAIL (1 of 128 DC corners fails)
   │              bias.min 4.6921 · gain 4.4618…4.7028 · bass 26.790 Hz
   ├─ batch     interior check: 38 inward nudges at the 5 worst corners     38 runs
   │            → no inward improvement anywhere: first-order optima
   ├─ 19 batches  σ(3) for bias.max only (the other 4 sides are implied PASS)
   │            4 range corners · AMV from 2 starts · default + uniform       87 runs
   │            → 6.2813 V PASS · uniform 6.4657 V PASS
   ├─ batch     ε_num: 7 decisive points at reltol 1e-9                        7 runs
   └─ verdict table                     total 389 runs, 27 ngspice processes, 0.75–0.97 s
      (the prototype sends the nominal and each side's nudges as separate batches: 27;
       batched as drawn, the same runs need 22)
```

What it prints (MVP; the columns are what the prototype computes):

```
 spec        worst_case                     sigma(3) (the specs' default)
 bias ≤ 6.5  6.5595 V  FAIL                 6.2813 V  PASS (est.)   uniform 6.4657 V PASS
             at temp=-10°C vcc.v=12.6V r1=+1% r2=-1% rc=-1% re=+1% q1.beta=100 (c_in: any)
 bias ≥ 4.5  4.6921 V  PASS (est.)          PASS (implied by worst_case)
 gain        4.4618 … 4.7028  PASS (est.)   PASS (implied by worst_case)
 bass        26.790 Hz PASS (est.)          PASS (implied by worst_case)
 cost: 389 runs (ngspice, 27 processes); worst_case by enumeration of 256 corners
```

### 6.2 The agent's flows

The MVP's agent is a CLI agent reading `spicy check` output. Each flow shows the calls, what runs, the measured cost, and what the agent may claim. The numbers come from `flows.py`.

```
 ─── "Why does bias fail?" ────────────────────────────────────────────────────────────
 agent ─► spicy check --explain bias           0 runs · reads the run table · < 10 ms
          worst corner 6.5595 V, q1 active
          1-flip table (every neighbour is a stored corner):
            temp → 5.8578 (−0.702)   beta → 6.1395 (−0.420)   vcc → 6.2049 (−0.355)
            r1 → 6.4096   r2 → 6.4204   rc → 6.4375   re → 6.4502   c_in → no effect
          main effects over 256 corners: temp −0.667 · beta −0.371 · vcc +0.285 V
            · r1 +0.152 · r2 −0.146 · rc −0.128 · re +0.120 · c_in 0
          corners failing: 2 of 256 (one DC corner, either c_in)
 agent may claim: "It fails at exactly one corner, with every part at its edge; any single
   part off its edge passes. Temperature is the biggest cause (a range knob, so better
   parts won't remove it). At 3σ it passes (6.28 V)." All values are simulated points.
 may NOT claim: any fix, before a check on the edited design.
```

```
 ─── "Fix bias" ───────────────────────────────────────────────────────────────────────
 agent proposes edits (text) ─► spicy check <candidate>   per candidate: 300 runs, 0.41–0.55 s
   R2 10k → 10.2k   VC 4.5421…6.4218  gain 4.4639…4.7045  bass 26.40   all PASS (est.)
   R1 47k → 46.4k   VC 4.5930…6.4635  gain 4.4632…4.7039  bass 26.85   all PASS (est.)
   RE 1k → 976      VC 4.5343…6.4265  gain 4.5713…4.8184  bass 26.85   all PASS (est.)
                                                          (gain max 0.012 from 4.83)
 3 candidates: ≈ 1.5 s serial, or one wall-clock check in parallel
 agent may claim: "verified on the candidate": the whole check ran on that exact edit,
   all specs, both sides. It should report each candidate's smallest margin
   (R2 10.2k: 42 mV at VC ≥ 4.5; R1 46.4k: 37 mV at VC ≤ 6.5).
 the user accepts one diff; nothing lands without that.
```

```
 ─── What-if on an edit: C_in 1 µF → 2.2 µF ───────────────────────────────────────────
 agent ─► spicy check <edit>                     389 runs, ≈ 1 s
          bass 26.790 → 12.177 Hz; bias still FAIL at worst_case, PASS at σ(3)
 There is no instant tier: the what-if IS a full check, so its answer is verified,
 not estimated (round 2 found the stored-line estimate gives −4.0 Hz for this edit, C11).
```

```
 ─── "Add a spec: top_room = vcc.v − dc(output.v) ≥ 5 V" ──────────────────────────────
 agent edits the contract ─► spicy check
   worst_case: the new measure is evaluated on the 256 stored corners (a check that
   keeps its run table), + 8 inward nudges for the interior check:     8 new runs, ≈ 20 ms
   → min 5.1951 V, PASS (est.). Exact over the corners: vcc.v and VC come from the
     same run, so the shared knob can't be counted twice (the job C8 gave the affine form).
   A plain cold re-check costs 389 runs, ≈ 1 s: also fine.
 agent may claim: "verified".
```

```
 ─── "Pin a real part: 2N3904" ────────────────────────────────────────────────────────
 agent drafts the part record (reviewed = false) · hFE ≥ 70 at 1 mA (onsemi 2N3903/D p.2,
 via agent_flows.md §3.6) ─► spicy check                              389 runs, ≈ 1 s
   bias.max: worst_case 6.8017 FAIL; σ(3) 6.4779 PASS default, 6.6926 FAIL uniform
             → UNDECIDED (distribution)          (was PASS with β 100..=300)
   gain min 4.4358, bass 27.78 Hz: PASS
 agent may claim: "with the 2N3904's datasheet minimum, bias depends on the β
   distribution assumption", and must say it rests on unreviewed, AI-extracted data.
```

### 6.3 What the agent may claim

| Status | Meaning | In the MVP |
|---|---|---|
| `verified` | The engine ran on this exact revision (the check, or `--explain` over its table) | Every answer |
| `stale` | Computed on an older revision | After any edit, until re-checked |
| `estimated`, `inner_bound` (round 2's agent statuses) | From stored slopes / re-simulated old points | **Not needed.** The check is under 1 s, so nothing is ever estimated. They return with the instant tier |

This is the minimalist's payoff for the agent. Honesty doesn't need a status taxonomy when every answer is a real check. The agent's one rule is "no fix is a fix before `spicy check` on it".

---

## 7. Recommendations for the final plan

1. **`worst_case` = enumerate the corners when 2ⁿ ≤ 4096, plus a first-order interior check at each side's worst corner. Merge M3b into M3c.** *Reason:* it's exact at the corners, needs no guards, and matched the ngspice key on 8 of 8 sides, including the three round-2 false-PASS shapes. Its one blind spot gives UNDECIDED (interior) (pq). 256 corners take 0.26 s.
2. **Schedule the flip walk as its own step after the MVP, triggered by the first design with more than 12 knobs.** It comes with margin sides, range corners, `e_obs` and the regime row, and it is accepted only when it agrees with enumeration on every regression circuit of 12 knobs or fewer. *Reason:* it's needed for scale (106 vs 295 runs on the MVP), and without its margin side it gives a false PASS (hiZ, 8.504 vs 3.994).
3. **Run `sigma(k)` only where `worst_case` isn't PASS** (C7's nesting), with round 2's exact map, slopes in ε and ascent check, **plus a second start from the `worst_case` counterexample projected onto the k-ball**. *Reason:* round 2's method gave a false PASS on hiZ (8.66 vs 5.48) and a bracket that missed the truth on band. The second start fixed both for +42 runs on the MVP.
4. **σ bracket = the last jump's miss, and the search must converge (else UNDECIDED (search)). If the corner table shows a device outside its nominal region, a σ PASS becomes UNDECIDED (regime reachable).** *Reason:* the last-jump bracket covered the independent key in 12 of 12. The all-jumps bracket turned a true PASS (MVP uniform, 34 mV margin) into UNDECIDED. The regime rule costs 0 runs.
5. **Transport: one `ngspice -b` process per batch.** The deck is the M1f export with every knob as a `.param`, `.temp {temp}` and `.options reltol=1e-6 vntol=1e-9 abstol=1e-15`, plus a generated `.control` block (`alterparam` … `reset`, `numdgt=15`, `ac lin 1 f f` per `at(f)`, one `wrdata` sweep with `appendwrite`). Large batches are split over processes. *Reason:* 1.0 ms/point against 0.63 for libngspice, with the same values to 2e-15, no FFI, no global state, and crash isolation. Every run is cold and pure for free.
6. **M1f must export knobs as `.param`, emit `.temp`, and decide TNOM.** *Reason:* the engine drives ngspice through them. TNOM 27 vs 25 °C, together with the XTB choice, moves nominal VC from 5.538 to 5.503 V.
7. **Measures: literal −3 dB below max\|H\| over the band, by cubic interpolation in (ln f, dB) at 50/decade; `at(f)` requested exactly; `Value | Undefined`.** *Reason:* 2.1e-7 accuracy in one batch, with no data-dependent round trip. `Undefined` never passes.
8. **The verdict rows of §4.3, and ε_num from re-running the decisive points at reltol 1e-9 plus each measure's interpolation band.** *Reason:* at reltol 1e-6 the solver band is 1e-13 V. The interpolation band is what's actually there.
9. **Fix these types now:** the batch `Backend::run`, `RunRecord` with named scalars, per-device op data and sweeps, one `RunTable` per check, `KnobPoint` in ε, a `Strategy` enum and a `Verdict` with reasons. **Defer** everything in §5.2. *Reason:* these are the seams the walk, the cache, the editor and JSON attach to. The rest has no consumer in the MVP.
10. **D7: don't compute both confidences always; `--realistic` fills the σ column for every side.** *Reason:* nesting settles 4 of 5 sides. That's 389 vs 541 runs and about 1 s vs 1.5 s.
11. **D9: no function backend. D10: JSON waits for its first reader.** *Reason:* the variants are real ngspice decks, and the table is snapshot-tested.
12. **Tests: the ngspice answer key (corners + 1,000 interior points + coordinate search) and an independent σ key (sphere sampling + pattern search). The variant decks (hiZ, band, pq) are the regression set. Assert the verdict, the value against the key, and bracket coverage. Skip when ngspice is absent.** *Reason:* this is the harness any later loop must pass. It ran in 2 s (worst_case key) and 91 s (MVP σ key with Monte Carlo).
13. **Output: the terminal table, `--at <corner>`, and `--explain <side>`** (1-flip table and main effects from the run table, 0 runs). *Reason:* "why does it fail" costs nothing with enumeration, and it's the agent's main read.

---

## 8. Open questions

1. **The enumeration budget.** A fixed 2¹², or a time budget from the nominal batch's measured cost? `reset` re-parses the deck for every point (`runcoms2.c:173-185`). At what circuit size does that dominate, and would `alter` on device values (no re-parse, but no `.temp`) be worth a second code path?
2. **The interior check looks only at the worst corner.** Should it also cover the top few corners, or every corner within the spec's margin? And which measure classes must declare "interior possible" (resonance at a fixed frequency, phase margin vs C_load)?
3. **The σ bracket rule rests on 12 results.** Last-jump miss vs all-jumps miss: re-check as the regression set grows, especially on op-amp and LDO circuits.
4. **The regime rule is blunt.** Once any corner saturates, every σ PASS becomes UNDECIDED. The σ margin search (tested: hiZ gives FAIL 5.4916) is the upgrade. Build it with the first such design, or with the walk?
5. **The region threshold on real models.** gmu > 10⁻⁴·gm was checked only on a bare Gummel–Poon (gmu floors at GMIN). Validate it with RB, RC, RE and quasi-saturation in vendor models.
6. **Should the σ column show for passing sides by default?** It costs +152 runs (about 0.6 s) on the MVP and nothing for the verdicts.
7. **Parallel processes and determinism.** Results are position-independent within a batch (verified). Is that also true across concurrent processes and ngspice builds, or should tests compare with a tolerance, as the numeric snapshots already do (roadmap §5)?
