# Runs, Cost and Tiers: When Each Spec Gets Checked

> 2026-09-30 · Research report. Question: which specs are cheap enough to check live while editing, and which belong in the background, on commit, in CI, nightly or only on request? How does the engine decide before it runs anything, and what should the editor show for a verdict computed on an older revision?
> **Where the numbers come from.** Every number marked **[C]** was measured for this report on **ngspice-42** (libngspice, KLU) by the stdlib-Python scripts in `/root/.claude/jobs/443154a8/tmp/engine5/cost/` (called `$C`; §0 lists them, and each writes a `.json` or `.out` file next to it). The check itself is the canonical MVP pipeline of `engine_plan.md` §2, run by the round-3 prototype `engine2/synthesis/canon.py` unchanged, on a new backend: a pool of libngspice worker processes. Machine: Intel i5-13600K (6 P-cores + 8 E-cores, 20 threads), load 0.2–0.4 from other jobs. Other numbers cite their doc and section.

---

## Summary

**What a full check costs.** I ran the complete check (every corner, the inside-the-box guards, the `sigma(3)` search, the numerical band) on thirteen kinds of spec, from a DC operating point to a switching buck:

| Spec kind | Runs, both confidences | ngspice time on 1 worker | On 16 workers |
|---|---|---|---|
| DC op, AC point, AC sweep (CE amp, 1–5 sides) | 302–579 | 0.05–0.28 s | 0.03–0.15 s |
| Fault sweep (10 fault setups) · multi-setup CMRR | 1,596 · 163 | 0.39 s · 0.05 s | 0.55 s · 0.02 s |
| 50-knob board, 16 local bias sides | 2,416 | 0.86 s | 0.50 s |
| Short transient (LDO load step) · long transient (rectifier) · start-up with events (load switch) | 1,133 · 255 · 1,127 | 17 s · 6.5 s · 22 s | 2.1 s · 1.4 s · 5.1 s |
| Switching transient (buck start-up + load step) | 2,167 | ≈ 9 min | **61 s** |

**[C]** (§1). Three things decide the cost. The corners (2^n) are 42–95% of the runs. The analysis sets the price per run: 0.13 ms for an operating point, 15–26 ms for a transient, 252 ms for a switching transient (110× to 1,900× an op). And the edit decides how much of this re-runs.

**The estimator.** `runs = 1 + 2^n + Σ|cone| + 8 + sides` for `worst_case`, plus a closed form per side for `sigma(3)`, with one fitted constant (§2). Before any run it predicts:
- the `worst_case` run count **exactly in 25 of 28 checks**;
- the both-confidence count with a **mean error of 11%**, and within ±30% everywhere except one check (−39%) where the interior guard started an ascent;
- the per-run time from the netlist alone within −28% … +39% for transients;
- the wall time on 16 workers within ±10% for three of the four transients, once the nominal run has been timed (the fourth is the ascent case).

The ascent can't be predicted: it makes the estimate a lower bound, and the scheduler re-estimates after each round.

**The tiers.** Each spec side group gets the earliest tier whose budget covers its estimated re-check time:

| Tier | When it runs | Budget (estimate × safety) | What landed there |
|---|---|---|---|
| **live** | every edit, after a 100 ms pause in typing | 0.1 s | single-block DC and AC specs; any board edit that touches one block |
| **save** | on save or at the end of a gesture | 1 s | a whole CE amp; the fault sweep; the 50-knob board's local sides |
| **idle** | in the background, with progress | 10 s | single-block transients (1.4–5.1 s) |
| **commit** | pre-commit, on the local pool | 60 s | multi-block transients |
| **CI** | every push, on the CI pool | 15 min | switching transients (61 s here, 18 s on 64 workers) |
| **nightly** | once a day | 8 h | sign-off statistics, deep searches |
| **manual** | only on request, estimate shown first | none | anything over its run budget |

The user can move any spec with `#[when(ci)]` and the like. A tier changes *when* a check runs, never *which* verdict it gets: the method is routed by run counts, not by time (§3).

**Incremental.** Content-keyed runs over each group's cone make an edit outside a cone free, exactly **[C]** (§4):
- `c_in` 1 → 2.2 µF re-ran the AC group (428 runs) and none of the DC group's 302;
- a resistor edit on the 50-knob board re-ran 257 of its 2,416 runs (11%);
- a changed bound re-ran nothing.

**Staleness.** A verdict carries the fingerprint of its cone: the devices, values and models in the cone, the setup, the spec, the confidence, the engine options and the engine and backend versions. If the fingerprint is unchanged, the verdict is exact and is shown as current, whatever else changed. If it changed, the old verdict stays visible but dimmed, labelled with *when* it will be re-checked ("running", "on commit", "in CI", "manual: ≈ 9 min") (§5).

---

## 0. How it was measured

| Script (`$C/`) | What it does | Output |
|---|---|---|
| `pool.py` | Backend with `ngb.run`'s interface: a pool of P worker processes, each with one libngspice and one deck loaded once. Per run: `alterparam` for changed knobs, `reset`, the case's analyses and our measures. Each run is timed inside the worker, and ngspice's own command time is timed separately. A content cache keyed by the physical values of the requesting group's knobs sits in front | — |
| `circuits.py` | The decks (reused from earlier rounds, see below), one simulate function per spec kind, and the check cases in canon's format | — |
| `harness.py` | Runs `canon.check` on the pool; `worst_case` only (the σ stage switched off) or both confidences | — |
| `smoke.py` | One nominal run of every mode: the deck loads, the measures read, the serial cost per run | `smoke.out` |
| `checks.py` | Every full check: `worst_case` cold, then both confidences on top of that cache (so runs(both) = runs(wc) + the new runs). At P = 1 it also repeats both confidences fully cold to confirm the count | `checks_*.json`, `p1.log`, `p16.log`, `buck16.log` |
| `incr.py` | Edits and re-checks with the cache from the previous revision | `incr_P16.json`, `incr16.log` |
| `estimator.py` | The cost estimator of §2 | — |
| `validate.py`, `static_wall.py`, `summarize.py` | Estimate vs measurement; the tables of §1–§3 | `validate.out`, `static_wall.out`, `summary.out` |

**The circuits,** all reused unchanged:

| Kind | Circuit | Knobs (range + statistical) | Sides |
|---|---|---|---|
| DC op | CE amp, `bias` (engine_plan §2.1) | 7 (2 + 5; C_in isn't in a DC cone) | 2 |
| AC point | CE amp, `gain` at 1 kHz | 8 (2 + 6) | 2 |
| AC sweep | CE amp, `bass` (f_low, 0.1 Hz–100 kHz at 50/decade) | 8 | 1 |
| All five | CE amp, the MVP check | 8 | 5 |
| Multi-setup | Difference amplifier, `cmrr(dm: Diff, cm: Common)` as in contract_syntax_v5.md's InAmp, 60 Hz | 6 (1 + 5) | 1 |
| Fault sweep | CE amp with one part open or shorted (4 resistors × 2, C_in short, Q1 C–E short): 10 `#[fault]` setups, `rated` limits on Q1 and RC dissipation | 6–7 each | 2 each |
| Board | 4 channels × 2 CE stages on an ideal rail (next_boards.md B4), 50 knobs; each stage's bias with the pinned-node cone (2 range + 5) | 7 per group, 8 groups | 16 |
| Short transient | LDO load step, 300 µs from the DC point (next_power.md P1) | 10 (3 + 7) | 1 |
| Long transient | Bridge rectifier, 8 mains cycles to steady state (next_time_domain.md A); the window moves with the frequency knob | 7 (3 + 4) | 2 |
| Start-up, events | Load switch: enable at 1 ms, UVLO turn-on, 40 ms (next_power.md P3) | 9 (4 + 5) | 2 |
| Switching | Voltage-mode buck at 500 kHz, ideal switches: 1.5 ms start-up, then a load step, 1.8 ms (next_power.md P2) | 11 (2 + 9) | 1 |

**Conventions.**
- A **run** is one knob point, simulated with every analysis its group needs.
- **Both confidences** means `worst_case` and `sigma(3)` on every side (D7).
- **ngspice time** is the time spent inside ngspice's commands, summed over runs.
- **Sim-bound wall** is the wall time spent waiting on the pool. It is what a Rust engine would see, plus the Python pool's inter-process cost of about 1.5 ms per round.
- **Prototype wall** adds canon.py's own arithmetic, mostly the 3σ-point solve (0.3–0.6 s per check). A Rust engine removes nearly all of it (engine_plan §2.8).

**Correctness check.** The CE amp's check on this backend gives 579 runs and the plan's values: bias max 6.559537 V FAIL at `worst_case`, 6.281314 V PASS (estimated) at `sigma(3)`. That's the same 579 runs, the same stage split (1 + 256 + 46 + 265 + 11) and the same verdicts as engine_plan §2.8 **[C]** (`checks_p1.json`).

---

## 1. What each spec kind costs

### 1.1 Per run

The median over every run in the check, ngspice time only, one worker **[C]**:

| Analysis | ms per run | × an op | Time points |
|---|---|---|---|
| op (CE amp, 5 nodes, including the `reset` re-parse) | **0.13** | 1 | — |
| op, 50-knob board (33 nodes) | 0.32–0.34 | 2.5 | — |
| op + 1 AC point | 0.16 | 1.2 | — |
| op + AC sweep, 351 frequencies | 0.41 | 3.2 | — |
| op + AC point + sweep (the MVP check's run) | 0.46 | 3.6 | — |
| Two setups per point (Diff and Common: two op + AC runs) | 0.36 | 2.8 | — |
| The same two setups **packed** as two copies in one netlist | 0.27 | 2.1 | — |
| LDO load step, 300 µs | **14.6** | 112 | 3,020 |
| Load-switch start-up, 40 ms | **18.5** | 142 | 4,390 |
| Rectifier, 8 cycles | **25.7** | 197 | 10,005 |
| Switching buck, 1.8 ms | **252** (3 serial runs: 230–270) | 1,940 | 84,857 |

Earlier measurements agree: the CE amp's run is 0.71 ms in engine_plan §2.3 (op + point + sweep, with measure code), the LDO step 14.3 ms and the switching load step a median of 240 ms in next_power §2.3 and §3.

**Runs spread.** In one check:
- the rectifier's runs spread 24–47 ms and the LDO's 14–26 ms **[C]**;
- the buck's switching runs spread 211–458 ms (next_power §3);
- on 16 busy workers they spread 226–1,037 ms **[C]**.

A failing corner is often a slow one (next_power §5). So an estimate from the nominal run is optimistic by up to 2×.

**Multi-setup specs pay per setup.** A second setup costs 1.54× per point as two runs, or 1.16× when the two setups are packed into one netlist, one op + one AC solve **[C]**. Packing works when the setups differ only in sources, because rule 1.3.6 of contract_syntax_v5.md makes them the same board at the same moment.

### 1.2 Per full check

**[C]** (`summary.out`, `validate.out`):

| Spec kind | Runs, worst_case | Runs, both | Corners' share | ngspice time on 1 worker, wc / both | Sim-bound wall on 16 workers, wc / both | Prototype wall on 16, both |
|---|---|---|---|---|---|---|
| DC op: CE bias (2 sides) | 153 | 302 | 42% | 0.023 / 0.052 s | 0.038 / 0.065 s | 0.30 s |
| AC point: CE gain (2 sides) | 283 | 375 | 68% | 0.046 / 0.064 s | 0.010 / 0.028 s | 0.14 s |
| AC sweep: CE f_low (1 side) | 274 | 328 | 78% | 0.114 / 0.138 s | 0.028 / 0.052 s | 0.14 s |
| CE amp, all 5 sides | 308 | **579** | 44% | 0.145 / 0.277 s | 0.033 / 0.150 s | 0.61 s |
| Multi-setup CMRR, two runs per point | 80 | 163 | 39% | 0.026 / 0.053 s | 0.006 / 0.023 s | 0.22 s |
| Multi-setup CMRR, packed | 80 | 163 | 39% | 0.021 / 0.047 s | 0.028 / 0.048 s | 0.24 s |
| Fault sweep: 10 setups × 2 rated sides | 1,033 | 1,596 | 46% | 0.25 / 0.39 s | 0.42 / 0.55 s | 1.14 s |
| Board, 50 knobs: 16 local sides (8 groups) | 1,224 | 2,416 | 42% | 0.42 / 0.86 s | 0.14 / 0.50 s | 2.38 s |
| Short transient: LDO load step | 1,044 | 1,133 | 90% | 15.3 / 16.6 s | 1.74 / 2.08 s | 2.25 s |
| Long transient: rectifier | 153 | 255 | 50% | 3.8 / 6.5 s | 0.58 / 1.41 s | 1.53 s |
| Start-up with events: load switch | 929 | 1,127 | 45% | 18.0 / 21.8 s | 4.19 / 5.08 s | 5.38 s |
| Switching: buck | 2,069 | 2,167 | 95% | ≈ 8.7 / 9.1 min (2,167 × 252 ms) | 55.0 / **61.4 s** | 61.7 s |

The buck was run on 16 workers only. Its one-worker figure is its run count times the serial per-run median. next_power §3 measured the 2,048-corner enumeration alone at 551 s serially, consistent with this.

**What the table says.**
1. **The corners are the check.** Enumeration is 42–95% of the runs, and 90–95% for single-side transients. `sigma(3)` adds 20–97% on the DC and AC checks (two starts × several rounds per side; most where a side has few corners), but only 5–9% on the one-side transients, where 2^n dwarfs it. D7's "both confidences" is nearly free exactly where runs are expensive.
2. **The analysis sets the tier, not the knob count.** The 50-knob board's 16 local sides take 0.86 s of ngspice time. The LDO's one 10-knob load-step side takes 16.6 s, and the buck's one side 9 minutes.
3. **Parallelism pays only for expensive runs.**
   - On 16 workers, the LDO and the buck speed up 8.0× and 8.9×. This 6P + 8E CPU gives 9.4× at 16 (engine_position_scale §3.3).
   - The rectifier (4.6×) and the load switch (4.3×) speed up less, because their σ rounds of 5–10 runs, or the ascent's line searches, don't fill 16 workers.
   - Checks under 0.1 s don't speed up at all: the pool's ≈ 1.5 ms per round dominates.
4. **The interior guard can multiply the cost.** On the load switch, step-control noise in `t_start` made the tangent check suspect an interior peak. The ascent and pooled refutation then added 388 runs (+72% over the corners) **[C]**. The same happened on the re_short fault (+38 runs). No formula can see this before the corner round.
5. **Fault setups add up.** Each `#[fault]` setup is a full check of its own, with `rated` checked on it (contract_syntax_v5 rule 1.4.3). Ten DC faults cost 1,596 runs, 2.8× the healthy CE amp. A transient fault (like SensorBoard's `Unplug`) costs a transient check per fault setup.
6. **Cold 50-knob board.** With the pinned-node cone rule, each stage's bias side enumerates 7 knobs: 8 groups × 302 runs. The four end-to-end gain sides have 14-knob cones (next_boards §1 item 2). That's 2^14 = 16,384 corners, over the plan's 4,096 limit, so they are UNDECIDED (budget) until the loop exists. The estimator puts them at ≈ 16,750 runs, 11.7 s serial or ≈ 1.3 s on 16 workers (`static_wall.out`, run cost 0.70 ms measured by next_boards §1). If the limit were raised, they would fit the idle tier. At 302 knobs with cones, the scale study measured 6,834 runs and 3.0 s on 16 workers (engine_position_scale §5).

---

## 2. The estimator

### 2.1 What the engine knows before running

Everything the formula needs is in the plan (engine_types.md §2.3), before the first run:

| Input | Where it comes from | CE amp |
|---|---|---|
| n: knobs the group enumerates (the union of its sides' cones) | cone rules on the flattened netlist | 8 |
| per side: \|cone\|, and \|S\|, the statistical knobs in it | the same | bias (7, 5), gain and bass (8, 6) |
| R: range knobs | the knob table | 2 |
| analysis kind; AC points; transient window and max step; switching frequency | the spec's measure and setup | op + 1 point + 351-point sweep |
| setups per point | a multi-setup spec's signature | 1 |
| P: workers | the machine, or the CI's declared pool | 16 |

### 2.2 The formula

Stage by stage, following engine_plan §2 (`estimator.py`):

```
worst_case  = 1                      nominal
            + 2^n                    every corner
            + Σ_sides |cone|         tangent nudges at each side's worst corner
            + 8                      audit points
            + sides                  band re-runs

sigma(3)   += 2^R                    range corners at nominal parts (shared by all sides)
            + Σ_sides search(|S|, R) + sides (band)
  search(S, R) = 2 starts × [ (1 + IT)(1 + S) + (2^R − 1) ] + R
               (each start: linearize, IT more linearizations, cross-check the other range corners;
                then R inward nudges of the range knobs)
  + one more search per side that FAILs worst_case but passes sigma(3) (the uniform-spread check)

IT = 1.5  (fitted on ce_dc, ce_acpt, ce_sweep; the only fitted constant)

per-run time, static:   op    0.13 ms × (nodes / 5)^0.45
                        AC    op + 0.03 ms per exact point + 0.8 µs per swept frequency
                        tran  3.5 µs × (window / max_step) × {1.0 smooth, 1.4 diodes, 1.9 switching}
                        × setups per point (1.16 packed)
per-run time, measured: the nominal run (stage 0) or, better, the slowest run of the first round

wall on P workers = Σ_rounds [ ⌈round size / P⌉ × t × SLOW(P) + 1.5 ms ]
  rounds: nominal, corners, nudges + audit, band, then 9.5 rounds of (1 + |S|) runs per σ search
  SLOW(16) = 1.6 on this CPU (per-run median at P = 16 over P = 1: 1.56–1.68 [C]; 16 / 9.4 in scale §3.3)
```

The static transient constant is the measured ngspice time per accepted time point: 2.4 µs (rectifier), 3.0 (buck), 4.8 (LDO), 4.9 (load switch) **[C]**. ngspice takes max_step = min(tstep, span/50) when none is given; the LDO's 300 µs at 0.1 µs predicts 3,000 points and got 3,020. Diode commutations add 38% (rectifier: 10,005 points against 7,270 predicted). The buck's 1,800 switching edges add 90% (84,857 against 45,000).

### 2.3 Accuracy

**Run counts** **[C]** (`validate.out`; "before running" doesn't know the verdicts, "knowing the FAILs" adds the uniform search once the corner round has shown which sides FAIL):

| Check | worst_case: predicted / measured | Both: before running | Both: knowing the FAILs | Both: measured | Error |
|---|---|---|---|---|---|
| CE DC (fit) | 153 / 153 | 235 | 274 | 302 | −9% |
| CE AC point (fit) | 283 / 283 | 375 | 375 | 375 | 0% |
| CE sweep (fit) | 274 / 274 | 322 | 322 | 328 | −2% |
| CE all five | 308 / 308 | 522 | 566 | 579 | −2% |
| CMRR, single setup / two setups | 80 / 80 | 116 | 116 | 109 / 163 | +6% / −29% |
| 10 fault setups (each) | 87 / 80–125 | 159 | 159 | 137–192 | −17% … +16% |
| Board, each of 8 groups | 153 / 153 | 235 | 274 | 302 | −9% |
| Rectifier | 153 / 153 | 247 | 247 | 255 | −3% |
| LDO load step | 1,044 / 1,044 | 1,110 | 1,110 | 1,133 | −2% |
| Load switch (ascent) | 541 / **929** | 687 | 687 | 1,127 | **−39%** |
| Switching buck | 2,069 / 2,069 | 2,132 | 2,191 | 2,167 | +1% |

- **worst_case is exact in 25 of 28 checks.** The misses are the two ascents (load switch −42%, re_short −30%) and one shared worst corner that removed duplicate nudges (rc_short, +9%).
- **Both confidences:** mean \|error\| 11.4%, and within ±30% except the load switch.
- IT is not a constant of nature. On CMRR the σ search needed more rounds: its measure has a kink at nominal, where the common-mode gain is zero. So IT differs by spec, and the estimate is ±30% on the σ half.

**Per-run time and wall time** **[C]** (`validate.out`, `static_wall.out`):

| Check | Static t/run | Measured t/run | Wall on 16, static t | Wall on 16, measured t | Measured sim-bound wall |
|---|---|---|---|---|---|
| CE DC | 0.13 ms | 0.13 ms | 0.057 s | 0.058 s | 0.065 s |
| CE all five | 0.44 ms | 0.46 ms | 0.147 s | 0.152 s | 0.150 s |
| Board group | 0.30 ms | 0.33 ms | — | 0.024 s | 0.054 s |
| Rectifier | 35.6 ms (+39%) | 25.7 ms | 1.80 s (+28%) | 1.28 s (−9%) | 1.41 s |
| LDO | 10.5 ms (−28%) | 14.6 ms | 1.32 s (−37%) | 1.92 s (−8%) | 2.08 s |
| Load switch | 19.6 ms (+6%) | 18.5 ms | 1.76 s (−65%) | 1.62 s (−68%) | 5.08 s |
| Buck | 299 ms (+19%) | 252 ms serial | 67.8 s (+10%) | 62.8 s (+2%) | 61.4 s |

So there are two estimates, used at different moments:
- **Static,** before any run: good to about ±40% for transients and ±15% for op and AC. It is enough to pick a tier, with a safety factor.
- **After the nominal run:** good to ±10% for the transients here, except where an ascent starts.

### 2.4 Three rules the estimate must follow

1. **Re-estimate after every round.** A running check knows its corners are done and whether an ascent started. The load switch's ascent is visible after the second round. The scheduler then moves the remaining work if it no longer fits its tier (§3.3).
2. **Use the slowest run of the first round, not the nominal.** Run times spread up to 2.2× (§1.1), and the slow corners are often the failing ones.
3. **Report the prediction with the verdict.** engine_flows §5.3 already reports runs used against runs planned; the planned runs are this estimate. A check that ran more than twice its estimate says why ("ascent at t_start: interior peak suspected").

---

## 3. Tiers and budgets

### 3.1 The tiers

| Tier | Trigger | Budget | Where the budget comes from | Cancelled by a new edit? |
|---|---|---|---|---|
| **live** | a text edit or gesture, after 100 ms without another | **0.1 s** for all live work of one edit | "0.1 second is about the limit for having the user feel that the system is reacting instantaneously" (Nielsen, *Usability Engineering*, 1993, §5.5); debounce as rust-analyzer's flycheck restart (engine_flows §5.2) | yes, between runs (engine_flows §5.2) |
| **save** | save, or the end of a schematic gesture | **1 s** | "1.0 second is about the limit for the user's flow of thought to stay uninterrupted" (ibid.) | yes |
| **idle** | the editor has been idle 2 s | **10 s**, with progress | "10 seconds is about the limit for keeping the user's attention focused" (ibid.); a progress line per check (engine_flows §5.3) | yes; finished runs stay cached |
| **commit** | a pre-commit hook (`spicy check --through commit`), or "check all" in the editor | **60 s** on the local pool | our choice: about as long as a person waits for a commit | no: it gates the commit |
| **CI** | every push or pull request | **15 min** on the CI pool | Bazel's `size = "large"` test gets 900 s, `enormous` 3,600 s (Bazel Test Encyclopedia, "Test sizes"); the pull request is the gate | no |
| **nightly** | a scheduled job | **8 h** | a night | no; resumable |
| **manual** | a command or a click; the estimate is shown before it starts | none | Bazel's `tags = ["manual"]` keeps a target out of wildcard builds | yes |

The tiers generalize engine_flows §5.1's four (T0 every edit, T1 background, T2 on demand, T3 sign-off). That table put all DC and AC work in one background tier. The measurements split it: a single block's DC/AC check fits one frame's worth of attention (live), a board or a fault sweep fits save, and transients start at idle. The split mirrors rust-analyzer. Its native diagnostics run on every keystroke, while `cargo check` runs on save (the `checkOnSave` setting, engine_flows §5.2 for its `flycheck.rs` behavior), and tests run in CI.

### 3.2 How the engine assigns a tier

For each **side group** (the sides that share a cone and runs, as in §1):
1. **Estimate** its re-check wall time for the pool the tier runs on: the local workers for live, save, idle and commit, the CI's declared pool for CI and nightly. Use the static per-run time until the group has been run once; after that, use the stored slowest first-round time.
2. **Apply a safety factor:** 1.0 for op and AC, 2.0 for a static transient estimate, 1.5 for a measured one. The factors cover the ±40% static error, the 2.2× run spread and the ascent risk.
3. **Assign the earliest tier whose budget covers it.** Several live groups share the live budget in the order of engine_flows §5.1: visible specs first, then the specs most likely to flip. What doesn't fit moves to save.
4. **Hysteresis:** a group moves to another tier only when two consecutive estimates agree it should, so a spec near a boundary doesn't flap between save and idle.
5. **A spec that is over its run budget** (the plan's 4,096 corners, or a method that doesn't exist yet) is **manual**, with the reason shown: "UNDECIDED (budget): 16,384 corners; the loop is not built".

The estimate that matters **at edit time** is the incremental one (§4): the runs this edit invalidates, not the cold check. A board spec whose cold check is in save runs live when the edit touches one block, because its re-check is 11% of the runs.

**Where the measured kinds land** (cold check / typical edit; 16 local workers, 64 CI workers):

| Spec kind | Cold re-check (estimate × safety) | Tier | Typical edit | Tier for that edit |
|---|---|---|---|---|
| DC op, AC point, AC sweep, one block | 0.03–0.07 s | live | a part value: ≤ its group | live |
| CE amp, all five sides | 0.15 s | save | `c_in`: AC group only | live (0.06 s) |
| Multi-setup CMRR | 0.02–0.05 s | live | — | live |
| Fault sweep, 10 setups | 0.55 s | save | a part value: every fault setup whose cone holds it | save |
| 50-knob board, 16 local sides | 0.50 s | save | one resistor: one group (257 runs) | live (≈ 0.05 s) |
| Board end-to-end gain (14-knob cones) | ≈ 1.3 s estimated, over the 4,096 limit | manual (budget) today | — | — |
| Rectifier, LDO, load switch | 1.4–5.1 s measured (× 1.5 = 2–8 s) | idle | any part in the cone: the whole check | idle |
| Switching buck (11 knobs) | 61 s on 16 (× 1.5 = 92 s); ≈ 18 s on 64 | **CI** | any part in the cone | CI |
| Switching start-up, 3 ms, 12-knob cone (330 ms per run, next_power §3) | ≈ 142 s on 16 (estimated) | CI | — | CI |
| LDO load step, all 15 knobs (next_power §2.3) | 32,884 runs, ≈ 47 s on 16 (estimated), over the 4,096 limit | manual (budget) today | — | — |
| Sign-off statistics, CE amp: 15,526 runs (engine_flows §3.7) | ≈ 1 s single thread; minutes on a board | nightly, or manual at sign-off | — | — |

### 3.3 While a check runs

- **A check that overruns its tier keeps going,** but moves to the next tier's queue so it no longer blocks live work. The load switch would start in idle (its static estimate is 1.8 s × 2), see its ascent after round 2, and finish in idle anyway (5.1 s).
- **Live work gets reserved workers.** rust-analyzer keeps latency-sensitive work on at most a quarter of its pool (engine_flows §5.2). Here: when idle or commit work floods the pool, keep 4 of 16 workers for live.
- **Cancelled work is never wasted.** Every finished run is cached by content (engine_flows §5.2), so an undo, or an edit back to an earlier value, is free.

### 3.4 The user's override

A spec or a setup can move its tier with an attribute, in the style of contract_syntax_v5's existing `#[fault]` and `#[outside]`:

```rust
#[when(ci)]            spec dip: tran(vout.v).min() >= 3.25V on LoadStep in Run;
#[when(live)]          spec output: dc(vout.v) within 3.3V ± 2%;     // forced live
#[when(manual)]        setup DropoutRow for Ldo3v3 { … }              // never in CI
```

Project-level settings go with the other project constants: the workers per tier, the tier budgets, and the CI pool size.

| Override | Allowed? | What the editor shows |
|---|---|---|
| Later than the estimate (`live` → `ci`) | always | the tier, next to the verdict |
| Earlier than the estimate (`ci` → `save`) | yes, with a warning | "dip: ≈ 60 s per check, runs on every save" |
| `manual` | yes | excluded from `--through ci`, like Bazel's `manual` tag; shown as "not checked in CI" |
| `spicy check --through nightly`, or `--only dip` | a one-off run of more tiers | — |

`spicy check` in a terminal defaults to `--through ci`. The MVP runs everything on request (engine_plan §1.1), which is this with every spec in one tier.

### 3.5 Tiers never change verdicts

engine_plan §2.3 routes a side to enumeration only if the estimate fits **3 s** on the worker pool. Both next_power §3 and next_time_domain §0 item 5 found that this makes a verdict depend on the machine: the same buck is UNDECIDED (budget) on one machine and PASS or FAIL on another. With tiers, time no longer needs to be in the routing rule:
- **Routing** (the method, and so the verdict) is decided by **run counts** only: enumerate when 2^|cone| ≤ 4,096, otherwise loop or UNDECIDED (budget).
- **Scheduling** (the tier) is decided by **time**.

The buck's 2,048 corners are then enumerated, in CI, and the verdict is the same everywhere. This recommendation replaces the time clause of plan §2.3 with the run-count clause alone.

---

## 4. Incremental re-checking

### 4.1 What is affected by an edit

Two mechanisms, as in engine_flows §3.1:
1. **Structure, before any run:** a side is affected only if the edit touches its cone: a knob's value or range, a device in the cone, the setup, or the spec itself. The cone is recomputed on the new revision (the netlist graph with the pinned-node and analysis rules, engine_flows §4.6; next_boards §1). So a topology edit that connects a new part into a cone is caught.
2. **Content, exact:** every run is keyed by the physical values of its group's cone knobs plus the analysis, the options and the structure key (engine_flows §4.2). An unaffected group replays and hits every run. An affected group misses exactly the points whose values changed.

Mechanism 2 is the ground truth. Mechanism 1 lets the editor skip unaffected groups without replaying them, and it is what tiering and staleness need (§5).

### 4.2 What re-ran

**[C]** (`incr.py`, 16 workers). The CE amp is checked as two groups, a DC group (bias, 7 knobs, op only) and an AC group (gain and bass, 8 knobs):

| Edit | Groups re-run | Runs (hits) | ngspice time | Share of a cold check |
|---|---|---|---|---|
| CE cold: DC group + AC group | both | 730 (0) | 0.39 s | 100% |
| Re-open unchanged | none | 0 (730) | 0 | 0% |
| `c_in` 1 µF → 2.2 µF | AC | **428** (DC 302 hits) | 0.30 s | 59% |
| `r1` 47k → 51k | both | 685 (0) | 0.35 s | 94% (bias now FAILs: the σ path is shorter) |
| Bound `bias ≤ 6.5 V` → `6.6 V` | none | **0** | 0 | 0% |
| New spec `base_bias` (a DC node already simulated) | the DC group's new sides | **110** (302 hits) | 0.03 s | 15% |
| `vcc` ±5% → ±10% (a range knob, in every cone) | both | 728 (2) | 0.37 s | 100% |
| 50-knob board cold (8 groups) | all | 2,416 (0) | 1.49 s | 100% |
| Board: `c2s1.r1` 47k → 51k | one of 8 | **257** (2,114 hits) | 0.15 s | 11% |

**Findings.**
- **Splitting by analysis costs 26% more when cold** (730 runs against 579 for the joint check, because the DC group can't reuse the AC group's corners) **but halves a capacitor edit** (428 against 579). The planner should share runs where the points coincide (engine_flows §4.1) and key them per analysis. That gives both: the op part of an op+AC run serves the DC group. This prototype only keys per group.
- **Changing a bound costs 0 runs,** because the stored extremes answer it. Adding a spec on a node that is already simulated costs only its own nudges and σ search. Both need the run table to keep every probe of every run (engine_types.md §6).
- **A range edit on temperature or supply touches everything.** They are in every cone. That is the edit that sends a board back to its cold tier.
- **Replaying is not free in the prototype.** With 0 runs, replaying all groups through canon.py still took 0.37–0.43 s (CE) and 1.8 s (board) of Python arithmetic. The engine should skip an unaffected group by its **cone fingerprint** (§5.2) and not replay it. Replay is the correctness fallback, not the fast path.

### 4.3 What a typical edit costs, per tier

| Design | Typical edit | Re-runs | Time on 16 workers | Tier |
|---|---|---|---|---|
| CE amp | a part value outside some cone (`c_in`) | 428 | ≈ 0.06 s | live |
| CE amp | a part in every cone (`r1`) | 685 | ≈ 0.15 s | save |
| Any design | a bound, a confidence, a comment, a rename | 0 | 0 | live |
| 50-knob board | one part in one stage | 257 | ≈ 0.05 s | live |
| 50-knob board | supply or temperature range | 2,416 | ≈ 0.5 s | save |
| LDO / rectifier / load switch | any part in the transient's cone | the whole check | 1.4–5.1 s | idle |
| LDO | a part only in DC cones (not the step's) | the DC groups only | < 0.1 s | live |
| Buck (switching) | any part in its cone | 2,167 | ≈ 61 s (18 s on 64) | CI |

So a typical edit is almost always cheaper than its spec's cold tier. An edit rarely touches a range knob that sits in every cone.

---

## 5. Staleness

### 5.1 The states

Every verdict carries the revision it was computed at and its **cone fingerprint**. After an edit, the editor compares fingerprints. It never re-checks just to decide what to show.

| State | Condition | Display | Trusted? |
|---|---|---|---|
| **current** | computed at this revision | normal | yes |
| **carried** | computed at an older revision; cone fingerprint unchanged | normal; the hover says "checked at rev 3f9a1c; nothing in its cone changed since" | **yes, exactly** |
| **recomputed free** | only the bound changed (or the confidence, with both computed); stored extremes converged | normal, marked current | yes: 0 runs, same runs |
| **stale, running** | fingerprint changed; the re-check is in its tier and running | the old verdict dimmed, a "predicted" value if a stored form gives one (engine_flows §4.7), a progress line | no |
| **stale, deferred** | fingerprint changed; its tier hasn't come ("on commit", "in CI", "nightly") | the old verdict dimmed, labelled with when it will be re-checked and the estimate | no |
| **stale, manual** | fingerprint changed; the spec is manual | dimmed, "not re-checked: run now (≈ 9 min)" | no |
| **never checked** | new spec or new setup | an empty badge with its tier | — |

A stale **FAIL** stays red but dimmed. It must never disappear just because the design changed: the old counterexample is the first run of the re-check (engine_flows §5.1, "simulate the old counterexample first").

### 5.2 When an old verdict may be trusted exactly

**Exactly when its cone fingerprint is unchanged.** The fingerprint hashes everything the check reads:

| In the fingerprint | Why |
|---|---|
| The cone: every knob's path, nominal, range and distribution; every device, model card and fixed value inside it; the structure key of the cone's subgraph | these are the runs' inputs (engine_flows §4.2) |
| The cone rules' outputs (pinned nodes, analysis projection) | a topology edit can grow a cone; comparing the recomputed cone catches it |
| The setup and mode, resolved: sources, loads, windows, events, `env` ranges, project `const`s | a shared `ambient` range edit touches every cone |
| The spec: measure, bound, confidence | the bound alone is the "recomputed free" case |
| Engine options, the engine's version, the backend and its version, the default model library (D-A) | a new ngspice or a new default XTB changes every value |

**Where the rule needs care.**
1. **Pooling across groups.** canon.py's refutation reads every in-box row of its own check table. If rows were pooled across groups, a verdict would depend on runs outside its cone, and the fingerprint would no longer cover it. Pool within a group only.
2. **`rated` limits** are checked on every run of every setup (contract_syntax_v5 rule 1.4.3), so their fingerprint is the union of all setups' cones.
3. **Purity.** A carried verdict is exact only if the same inputs give the same bits: cold starts, fixed seeds, results in request order (engine_flows §5.4). Transient step-control noise (next_time_domain §0 item 6) is deterministic, so a carried transient verdict is still exact. It's the verdict's *meaning* that the noise weakens, not its reproducibility.

### 5.3 Verdicts from CI

A fingerprint-keyed verdict store works like a remote build cache:
- **Sharing:** CI writes its verdicts under their fingerprints. A developer who pulls sees every verdict whose fingerprint matches as **carried**, exact, with no local runs. That's the "cloud" case of constructive traces (*Build Systems à la Carte* §4.2.3 and Cloud Shake, engine_flows §6.2), and Bazel's remote cache.
- **The commit gate:** no spec in a tier up to `commit` is stale.
- **The merge gate:** no spec in a tier up to `CI` is stale, and none is FAIL unless waived.
- **Nightly and manual specs** report their age ("checked 3 revisions ago"), and a pull request that touches their cone lists them as "stale, deferred".

---

## 6. Grounding

| Decision | Precedent | Follow or depart |
|---|---|---|
| Tier budgets of 0.1 s, 1 s and 10 s | Nielsen, *Usability Engineering* (1993) §5.5, the three response-time limits | Follow |
| Live diagnostics on every edit, the expensive check on save, tests in CI | rust-analyzer: native diagnostics vs `cargo check` on save (flycheck, engine_flows §5.2) | Follow, with the split set by the estimate, not by the kind of check |
| A size class per test with a default timeout; manual targets | Bazel `size` (small/medium/large/enormous → 60/300/900/3,600 s) and `tags = ["manual"]` | Follow: a tier per spec, assigned automatically, overridable |
| Content-addressed results shared between machines | Bazel remote cache; constructive traces and Cloud Shake (*Build Systems à la Carte*, ICFP 2018, §4.2.3, §5.4) | Follow, at the level of runs and of verdicts |
| Skip validation of inputs that rarely change | salsa's durability levels (engine_flows §6.1) | Follow in spirit: the cone fingerprint is the barrier |
| Cancel stale work, keep finished results | rust-analyzer's revision counter (engine_flows §5.2) | Follow |
| Run corners and Monte Carlo when the user asks | Cadence ADE Explorer/Assembler run modes (engine_flows §6.4) | **Depart:** the engine schedules every spec itself, by cost |
| Route by measured time (plan §2.3's 3 s clause) | — | **Depart** (§3.5): route by runs, schedule by time |

---

## 7. Recommendations

1. **Add the estimator to the plan stage** (`Plan` in engine_types.md §2.3): planned runs and estimated time per side group, static first, refreshed after every round. It costs a few lines and needs no runs.
2. **Drop the time clause from plan §2.3.** Route by 2^|cone| ≤ 4,096 alone, and send long checks to a later tier instead of to UNDECIDED (budget).
3. **Give every verdict a cone fingerprint and a tier** in the report (engine_types.md §9), and store verdicts by fingerprint next to the run cache. The fingerprint is what makes "carried" exact and makes CI verdicts shareable.
4. **Key runs per analysis, and share them across groups** where points coincide (engine_flows §4.1–4.2). The measurement shows the trade: separate groups cost +26% cold and −26% on a capacitor edit; shared runs get both.
5. **The syntax:** a `#[when(…)]` attribute on specs and setups, and project-level tier budgets and worker counts. It's small and not needed for the MVP, where `spicy check` runs everything.
6. **The MVP needs none of the tiers** (engine_plan §1.1): every CE amp check fits `save`. The estimator and the fingerprint are worth putting in M3 anyway, as data in the report, because both are needed the moment the first transient spec (the LDO) arrives.

**Open questions.**
- How to predict IT per spec from its measure: kinks, as in CMRR, doubled the σ rounds.
- Whether a static test for step-control noise (measures on switching or event-driven waveforms) should forbid the tangent guard's 1% nudges before they trigger an ascent like the load switch's.
- The CI pool size: the buck needs 64 workers to finish in about 18 s.

---

## Appendix: reproducing the numbers

```
cd /root/.claude/jobs/443154a8/tmp/engine5/cost
python3 smoke.py > smoke.out                                  # per-run cost of every mode
python3 checks.py p1 1 ce_dc ce_acpt ce_sweep ce_full cmrr1 cmrr2 cmrrp fault_r1_open … rect lsw ldo
python3 checks.py boardP1 1 board_c0s1 … board_c3s2
python3 checks.py p16 16 <the same names>                     # sim-bound wall on 16 workers
python3 checks.py buck16 16 buck                              # ≈ 70 s on 16 workers
python3 incr.py 16                                            # §4.2
python3 summarize.py; python3 validate.py; python3 static_wall.py
```

`checks_p1.json` also holds a first board run with a cone bug: RC was left out of each stage's cone, giving 6 knobs. `summarize.py` and `validate.py` take the board from `checks_boardP1.json`, the corrected run. Timings vary about ±20% between repetitions under the machine's background load. Run counts and verdicts are deterministic.
