# Position: Scale First. Real Boards Have 50–300 Knobs, Dozens of Specs, and Transients

> 2026-09-27 · Round 3, one of five position papers. Reads with `engine_synthesis.md` (round 2: C1–C12, D1–D10) and the six round-2 reports next to it, `../engine.md`, `../walkthrough.md` and `../roadmap.md` §3, §4.2 and §6.
> **Where the numbers come from.** Every number was computed on **ngspice-42** (KLU, `libngspice.so.0`) by the stdlib-Python scripts in `/root/.claude/jobs/443154a8/tmp/engine2/scale/` (§0 lists them; each writes a `.out` file next to it), or it is cited as `file:line` in `/tmp/refs/ngspice`. Machine: Intel i5-13600K (6 P-cores + 8 E-cores, 20 threads).

---

## Summary

The MVP has 8 knobs; a real board has 50–300 knobs and dozens of specs. So I built a small bounds engine on ngspice: round 2's guarded loop, sigma(3) with the exact map, a driver that runs in rounds, a run cache, and a pool of libngspice workers. I ran it on the MVP amplifier and on circuits up to **302 knobs and 103 spec sides**, each checked against an exact answer key. On the amplifier it finds every worst_case extreme exactly (65 runs, 3 rounds). sigma(3) agrees with an independent optimizer to 6e-5. A full check with both confidences takes 156 runs and 13 ms on 16 workers. Scale is where the architecture matters. Built as round 2 specifies it, every request is a full knob point and every side looks at every knob, and the cost grows as **N²**: 55,474 runs at 200 knobs. When each request covers only the knobs that can structurally reach its measure (its **cone**, read from the netlist graph, no simulation), the cost grows as **N**: 22.6 runs per knob for both confidences, from 8 knobs to 302. At 302 knobs that's 6,834 runs, 6 rounds and 3.0 s on 16 workers, exact on all 103 sides. The round count doesn't grow with size. A limiter hidden in one stage of a 300-knob chain beat round 2's guards: the counterexample was 2× off, and at 302 knobs a bass bound anywhere from 217 to 260 Hz would have given a false PASS. It took two changes: the margin guard's run has to compute every analysis, and its witness has to be *spliced* into each spec whose cone contains the device. Numeric screening saved only 4–5% of runs and broke exactness on all three end-to-end specs. ngspice's DC `.sens` is accurate but gives one output per call, so it loses to nudging when there are many outputs. AC `.sens` returns about 0 for everything. A transient run costs 35–42× an op+AC run, and a switching transient about 13,700× an op. The backend should be a pool of persistent worker processes, each with one libngspice and the circuit loaded once, with knobs set by `alter`. A process per run is 2.6–122× slower. The MVP must get the *types* right now: a request is a partial knob point with its own analyses, the cache is per measure and keyed by physical values, and each run returns every measure its analyses produce. Everything else can stay simple.

---

## 0. What was built, and how to re-run it

| Script | What it does | Output |
|---|---|---|
| `ng.py` | ctypes driver for libngspice: load a deck, send commands, read real and complex vectors, capture errors | — |
| `verify_ng.py` | Checks that `alterparam` + `reset` and `alter`/`altermod`/`option temp` both change the circuit, bit-identically to a fresh deck | stdout |
| `circuits.py` | Test circuits as knob tables + netlists + netlist graphs: the CE amp, a cascade of K buffered CE stages (with an optional "hiZ" stage), an RC ladder | — |
| `backend.py` | One libngspice `Instance` per worker (circuit loaded once, knobs by `alter`), our measurement functions, a `Pool` of P worker processes | — |
| `structure.py` | The structural influence pass: which knobs can reach which measure, per analysis | stdout |
| `engine.py` | The engine (475 lines): requests over cones, the driver in rounds, the per-measure cache, packing, the shared round, the margin guard + splice, the worst_case walk with flips, sigma(k), the verdict table | — |
| `check_meas.py`, `noise_check.py` | Nominal measures, f_low accuracy, slope noise at default vs engine tolerances | stdout |
| `bench_backend.py`, `bench_breakdown.py`, `bench_parallel.py`, `bench_tran.py` | Transport, per-run breakdown, parallel throughput, transient cost | `bench_*.out` |
| `run_ce.py`, `run_ce_engine.py`, `run_ce_spec.py` | CE amp: answer key, sigma key, the engine | `run_ce_*.out` |
| `cascade_key.py` | Exact answer key for the K-stage cascade | `cascade_key.out` |
| `run_scale.py` | The engine from K = 1 to 50 (8 to 302 knobs), three architectures, with and without the hidden limiter | `final_*.out`, `scale_*.out`, `*.json` |
| `diag_hiz.py`, `screen_test.py`, `sigma_scale_check.py`, `nested.py`, `edit_flows.py`, `run_ladder.py` | The experiments of §5–§7 | same names `.out` |
| `sens_check.py`, `sens_accuracy.py`, `sens_ac.py` | ngspice `.sens`: contents, accuracy, cost, AC | `sens_*.out` |
| `covering.py` | Range-knob corners vs covering arrays | `covering.out` |

**Conventions.** Knob positions ε ∈ [−1, +1] (low edge … high edge), as in round 2. A **run** is one knob point with its analyses: "op" (operating point only) or "op+AC" (operating point plus a 241-point sweep, 40 per decade, 1 Hz–1 MHz). A **side** is one direction of one spec. A **round** is one batch of independent runs. The engine uses ngspice `option reltol=1e-6 vntol=1e-9 abstol=1e-14`. The **wall** times include the Python engine's own bookkeeping; the **backend** times (ngspice plus inter-process transfer) are what a Rust engine would see.

---

## 1. The position, and round 2 seen from it

### 1.1 The position

1. **Cost must grow with the size of each spec's cone, not with knobs × specs.** On a board, most specs are local: a regulator's output, a stage's bias. A few are end-to-end. A loop in which every side looks at every knob is O(N·S), and since the specs grow with the board, that's O(N²).
2. **The cone is structural, exact and free.** It comes from the netlist graph, before any simulation. Numeric screening is not a substitute: it drops exactly the knobs that only matter inside a limiter.
3. **A run is a partial knob point with its own analyses.** It is cached per measure, keyed by the physical values of the knobs that can reach that measure. A transient never piggybacks on a DC request.
4. **ngspice is driven by a pool of persistent workers**, each with one libngspice instance, the circuit loaded once and knobs set by `alter`. Latency is set by rounds, and in these experiments the round count didn't depend on circuit size.
5. **Transient specs change the budget by one to four orders of magnitude.** They are the one place where L0 re-linearization at every vertex gets too expensive. They need their own scheduling tier and the analysis ladder (engine.md §5.3).

### 1.2 Keep, change, cut

| Item | Verdict | Reason (evidence in the section named) |
|---|---|---|
| **C1** stop on optimality, accept only improvements | **Keep** | Terminated with a 1-flip certificate on every one of the 103 sides at 302 knobs (§5.2) |
| **C2** flips at vertices | **Keep, scoped to the cone** | Flips cost \|cone\| runs per vertex, the same as nudges. At scale the lever is cone size, not flips vs nudges (§6.5) |
| **C3** margin guard | **Keep, and extend** | In a 300-knob chain the guard alone didn't fix the end-to-end sides. Its run must compute every analysis, and its witness must be spliced into every side whose cone holds the device (§5.4) |
| **C4** every run updates every spec | **Change** | "Every measure its analyses produce", not "every measure". The restart rule (a single pooled run beats the side) is not enough at scale: the worst board combined runs no single run had (§5.4) |
| **C5** below-noise knobs to an edge; structural zeros from the circuit | **Keep, and generalize** | "A capacitor can't reach an op" becomes the full influence graph: pinned nodes, one-way controlled sources, capacitors open in DC (§6.1). It turns O(N²) into O(N) |
| **C6** verdict table | **Keep** | The regime row fired correctly at scale (bass.max's worst point is in saturation, §5.4) |
| **C7** sigma(k): exact map, k-σ point, nesting | **Keep; prune the joint corners past R ≈ 5** | Exact map verified against an independent optimizer (§4, §5.6). The 2^R − 1 cross-corner runs per side need pruning past R ≈ 5 (§6.6) |
| **C8** three kinds of error | Keep | Scale-neutral |
| **C9** exact measures | **Keep; change the refinement** | On ngspice every extra AC frequency is a new `ac` command, which recomputes the op. Cubic interpolation on a 40-per-decade sweep is within 5e-7 at no extra runs (§3.5) |
| **C10** engine-grade tolerances | Keep, not a blocker on ngspice | ngspice's default slopes already agree to 4.5e-4 between h = 1e-3 and 1e-4 (truncation). Tight options still move a β slope by 5e-4 relative (§3.5) |
| **C11** instant tier = log estimate + re-simulated inner bounds | **Keep, and add "carried"** | With cones, a side whose cone the edit doesn't touch keeps its verdict *exactly*. At 200 knobs, a one-block edit leaves 64 of 69 sides untouched (§7.3) |
| **C12** content cache, rounds | **Keep, and generalize** | Key per measure = physical values over its cone (round 2's per-analysis projection is the special case). Rounds stayed at 3 / 6 from 8 to 302 knobs (§5.2) |
| **D1** nudges at nominal, flips at vertices | Agree | Scoped to the cone. It gets too expensive only for dense transient specs (§6.5) |
| **D2** optimality stop | Agree | — |
| **D3** exact map | Agree | Engine vs independent optimizer: ≤ 6.4e-5 on the CE amp, ≤ 2.5e-5 V on 32 bias sides at 98 knobs |
| **D4** σ per side | Neutral | — |
| **D5** literal −3 dB | Agree | Implemented by cubic interpolation (above) |
| **D6** cold start | Agree | Automatic on ngspice: every `op` starts from its junction initialization |
| **D7** both confidences | **Agree for DC/AC; not for transient** | sigma(3) nearly triples the runs at 302 knobs (2,319 → 6,834), still 3 s. Nesting saves only 15% here (§5.6). For a transient spec, compute the headline confidence only |
| **D8** β 100..=300 | Neutral | — |
| **D9** function-backed test backend | **Superseded** | The engine runs on ngspice directly. The cascade with its exact product key is a better scale fixture (§2.2) |
| **D10** JSON output | Agree; add the cone and the cost | The agent's "carried" claim needs the cone (§7.2) |
| engine_flows §4.6, §7: connectivity pruning deferred ("buys nothing on the CE amp") | **Disagree** | It is the difference between O(N) and O(N²). The *types* must carry cones from the MVP, even while the cone pass implements only the analysis rule |
| engine.md §6.3 / roadmap §3: ngspice `.sens` "may raise it to L1" | **Disagree** | DC: accurate, but one output per call (§6.4). AC: returns ≈ 0. ngspice stays L0 |
| roadmap §3: "start with the subprocess" | **Disagree** | A process per run is 9–122× slower for op and 2.6–14× for op+AC than a persistent in-process instance (§3.1) |

---

## 2. Test circuits and their answer keys

### 2.1 The MVP amplifier on ngspice

`ce_amp` is `circuits/ce_amp.spl` as a netlist: the 8 knobs of roadmap §4.2, IS = 1e-14, BF = β, and two settings the round-2 deck left out:
- `.options tnom=25`, because ngspice's default is 27 °C (it prints "TEMP = 25 and TNOM = 27" for `$R2/ce_amp_ng.cir`);
- `XTB=1.5`, so β varies with temperature (the default XTB = 0 gives none).

**Nominal:** VC 5.503287 V, |H(1 kHz)| 4.590814 (the round-2 model: 4.590813), f_low 20.126954 Hz with the literal −3 dB definition, saturation margin 3.860 V (round 2: 3.856).

**Answer key** (`run_ce.py`): all 256 corners plus 1,000 seeded interior points, through the engine's own measurement functions. No interior point beat a corner. The sigma(3) key is independent: projected gradient ascent on the sphere ‖u‖ = 3, central differences, 4 random starts at each of the 4 range corners (23,918 runs).

### 2.2 A cascade of K buffered CE stages, with an exact key at any size

```
 VIN ─► [stage 1] ─► E1 (×1/4.6) ─► [stage 2] ─► E2 ─► … ─► [stage K] ─► EK ─► b_K
          │                            │                        │
        out1                         out2                     outK           VCC shared, temp shared
 stage k = C_in,k → base_k; R1,k R2,k divider; RC,k; RE,k; Q_k (own .model, BF = β_k)
```

- **Knobs:** N = 6K + 2. Each stage has `r1 r2 rc re` (±1%), `c_in` (±20%) and `β` (100..300). The range knobs `temp` and `vcc` are shared.
- **Variation:** seven stage variants (E24/E96 neighbours) repeat along the chain.
- **Spec sides:** 2K + 3. Each stage has bias max ≤ 6.5 V and min ≥ 4.5 V. End to end: gain |H(1 kHz)| in G₀(1 ± 0.05√K), and f_low ≤ 1.3 × nominal. Each stage's saturation margin is also recorded, and guarded.
- **Sizes:** K = 1, 2, 4, 8, 16, 33, 50 gives N = 8, 14, 26, 50, 98, 200, 302.

**Why the key is exact.** Each stage is driven by an ideal source and loaded only by an ideal buffer's control input. So H_total(f) = Π H_k(f) exactly, and stage k's bias depends only on its own knobs plus temp and vcc. The key is built from single-stage brute force: 8 stage types × 1,152 runs (9 temperatures × 2 supplies × 64 statistical corners), 5.7 s.
- **bias:** the extremes over the stage's corners.
- **gain:** at each (temp, vcc), the product of each stage's own extreme, then the extreme over (temp, vcc).
- **f_low max:** each stage has one capacitor, so its normalized response is ordered by its pole. At each (temp, vcc), take each stage's max-f_low corner, multiply the H_k(f) and measure.

Checked against a real 3-stage cascade at 20 random corners: largest difference **5.9e-13**. No interior temperature beat the corners at any K (`cascade_key.out`).

**The hiZ variant** puts RC = 6.8 kΩ in the middle stage (k = K/2 + 1). It is active at nominal (margin 0.957 V) and saturates at 10 of its 256 corners, where its gain collapses from 1.44 to 0.61. That's round 2's "limiter off at nominal", hidden inside a big circuit.

### 2.3 An RC ladder: a spec that every knob reaches

50 RC sections: each R is 1 kΩ ±1% with tc1 = 100 ppm/K, each C is 10 nF ±5%, and RL is 100 kΩ ±1%. With temp, that's 102 knobs. Specs: f_high(−3 dB) and the DC gain. Every knob reaches f_high, so structure can't prune anything. The key is the 16 "group corners" (all R at one edge, all C at one edge, RL, temp) plus 400 random vertices.

---

## 3. Driving ngspice at scale: measured

### 3.1 Four transports

Per run, in ms. Every run moves every knob to a random point in the box and reads back what the engine needs (`bench_backend.out`).

| Circuit | Knobs | Analyses | A: process per run | B: one batch process | C: in-process, `alterparam`+`reset` | D: in-process, `alter` |
|---|---|---|---|---|---|---|
| CE amp | 8 | op | 6.98 | 0.26 | 0.14 | **0.057** |
| | | op+AC | 7.57 | 0.70 | 0.60 | **0.53** |
| cascade K=8 | 50 | op | 8.93 | 0.72 | 0.65 | **0.28** |
| | | op+AC | 10.61 | 1.97 | 1.80 | **1.39** |
| cascade K=16 | 98 | op | 9.92 | 1.43 | 1.49 | **0.54** |
| | | op+AC | 12.55 | 3.89 | 3.44 | **2.40** |
| cascade K=33 | 200 | op | 10.69 | 3.25 | 4.21 | **1.14** |
| | | op+AC | 17.89 | 8.02 | 8.25 | **5.08** |
| cascade K=50 | 302 | op | 15.95 | 5.45 | 7.58 | **1.70** |
| | | op+AC | 20.62 | 12.34 | 14.25 | **7.81** |
| RC ladder | 102 | op | 9.11 | 1.14 | 1.26 | **0.41** |
| | | op+AC | 10.22 | 2.34 | 2.30 | **1.42** |

- **A** writes a deck and runs `ngspice -b` per run. Process start-up alone is about 7 ms, so it's 122× slower than D for the CE amp's op and 2.6× for a 302-knob op+AC.
- **B** is one `ngspice -b` process running a `.control` script with `alter` + `op` + `ac` + `print` per run. It needs no FFI, and it's within 1.3–4.6× of D.
- **C** re-parses the whole deck on every `reset` (`com_rset` → `com_remcirc` + `inp_source_recent`, `runcoms2.c:175-185`; the deck copy is reloaded in `inp.c:560-600`). That's 2.5–4.5× D for an op.
- **D** changes instance and model parameters in place. Every analysis still does `CKTunsetup` + `CKTsetup` + `CKTtemp` (`cktdojob.c:160-167`), so each run is still a clean start.

`alter` and `option temp` gave results **bit-identical** to a freshly loaded deck (`verify_ng.py`: VC 5.1825918324421245 V at 60 °C both ways, and so on).

### 3.2 Where one run's time goes (mode D)

| Circuit | `alter`, per knob changed | `op` | AC, per frequency | AC fixed cost (setup + its own op) | Reading vectors in Python |
|---|---|---|---|---|---|
| CE amp | 2.2 µs | 0.065 ms | 1.0 µs | ≈ 0.09 ms | 0.14 ms |
| 302 knobs | 2.1 µs | 0.72 ms | 17.6 µs | ≈ 2.0 ms | 0.25 ms |

- An `ac` command always recomputes the operating point.
- An extra frequency is only cheap *inside* one sweep. That's why C9's refinement by extra AC solves is expensive on ngspice (§3.5).

### 3.3 Parallel: processes, or library copies in threads

ngspice keeps one active circuit per loaded library: `ft_curckt` (`src/frontend/circuits.c:16`) and `ng_ident` (`src/sharedspice.c:420`) are globals. Several circuits can be loaded and switched, but only one runs at a time. `ngSpice_running` (`sharedspice.h:486`) runs that one in a background thread. The `ident` argument of `ngSpice_Init_Sync` (`sharedspice.h:391`) exists so that callbacks can tell *copies* of the library apart. So parallel work needs separate copies: separate processes, or copies of the `.so` file loaded with `RTLD_LOCAL` (dlopen of the same path returns the same handle).

| Throughput, op+AC, ms per run | P = 1 | 2 | 4 | 8 | 12 | 16 | 20 |
|---|---|---|---|---|---|---|---|
| CE amp, worker processes | 0.434 | 0.293 | 0.169 | 0.110 | 0.081 | **0.073** (6.0×) | 0.076 |
| 98 knobs, worker processes | 2.534 | 1.365 | 0.703 | 0.440 | 0.345 | **0.297** (8.5×) | 0.286 |
| 302 knobs, worker processes | 8.223 | 4.185 | 2.204 | 1.389 | 1.075 | **0.877** (9.4×) | 0.854 |
| 98 knobs, library copies in Python threads | 2.511 | — | 0.878 | 0.767 | — | — | — |

- **Copies in threads work:** up to 24 copies were loaded in one process, and every result was correct. They are GIL-bound in Python (3.3× at 8 threads). From Rust they would scale like processes, but a segfault in any copy takes down the engine.
- **Processes scale to 9.4×** on this 6P + 8E-core CPU. The small CE amp saturates at 6× because each run is shorter than the transfer overhead.

### 3.4 Transient against op and AC

Best of three, in-process (`bench_tran.out`):

| Circuit | op | op+AC (241 pts) | Transient | Transient / op+AC | Transient / op |
|---|---|---|---|---|---|
| CE amp, 1 kHz sine, 5 ms, max step 1 µs (≈ 1,000 points per period, THD grade) | 0.023 ms | 0.25 ms | **10.5 ms** (5,008 pts) | 42× | 460× |
| 98 knobs, same | 0.106 ms | 1.87 ms | **67.5 ms** | 36× | 640× |
| 302 knobs, same | 0.336 ms | 5.66 ms | **200 ms** | 35× | 600× |
| Open-loop buck, 500 kHz, 2 ms of start-up (1,000 periods), max step 20 ns | 0.023 ms | — | **312 ms** (138,473 pts) | — | 13,700× |

The same 5 ms at the default step (508 points) costs a tenth as much, but can't resolve −60 dB distortion. The buck is an ideal-switch toy. A vendor controller model with soft-start logic will cost more. engine_power.md §0 item 2 found PSS by shooting reached steady state in 21 periods instead of about 1,160.

### 3.5 ngspice facts the engine must know

| Fact | Evidence | Consequence |
|---|---|---|
| Default `tnom` is 27 °C | ngspice prints it for `$R2/ce_amp_ng.cir` | The exporter (M1f) must write `.options tnom=` explicitly |
| XTB defaults to 0: β has no temperature coefficient | Gummel–Poon notes | Part records must carry XTB, or the temperature knob misses β |
| `alterparam` takes effect only through `reset` (a full re-parse) | `inp.c:1817-1821`, `runcoms2.c:175-185`; 2.5–4.5× slower per op run (§3.1) | Export each knob as an instance or model parameter, with a knob → `alter` target table. Use `.param` + `alterparam` only for knobs inside expressions |
| After a `sens` command, `option temp` is silently ignored in that instance | `sens_accuracy.py`: the temperature nudge came back exactly 0; `verify_ng`-style check: VC stays 5.50322723 V at 60 °C after `sens` | Never mix `.sens` and `option temp` in one instance, or reload after `.sens` |
| AC `.sens` returns ≈ 0 for every parameter | `sens_ac.py`: dH/dRC = 1.39e-12 per Ω, where the true value is about 9.8e-4 per Ω (the gain slope is 0.0459 per ε over a 47 Ω half-range); VCC and β give exactly 0 | Confirms by running what round 2 read in the code (`cktsens.c:179-182`, MODEINITSMSIG under `#ifdef notdef`). AC specs stay at L0 on ngspice |
| `ac dec` accumulates frequencies (1 kHz lands on 1000.000000000002) | `verify_ng.py` | Harmless for \|H\| (2e-15 relative in f) |
| Each `op`/`ac` creates a new plot, kept in memory | — | `destroy all` after reading each run |
| An extra frequency outside the sweep costs a whole `ac` command, including a new op | §3.2 | Measure f_low by cubic interpolation of dB vs ln f on the 40-per-decade sweep: within **2.5e-7, 1.0e-7 and 4.6e-7** of a 4,001-point reference at three points (`check_meas.py`) |
| Default tolerances already give good slopes | `noise_check.py`: h = 1e-3 vs 1e-4 agree to 4.3–4.5e-4 on β (truncation, as the formula predicts). With engine options, β's gain slope moves by 5e-4 relative | Use the engine options; they cost little. Slopes such as rc → f_low (7e-9 Hz per ε) are pure noise: C5's "below noise" case |

### 3.6 The backend shape I recommend

```
 engine (one process)                                 worker processes (P of them)
 ┌──────────────────────────────┐   batch of runs    ┌────────────────────────────────────────┐
 │ driver: one round = one batch│ ─────────────────► │ worker i: libngspice, circuit loaded    │
 │ requests → cache → pack      │   (point, analyses)│ ONCE; per run: alter changed knobs,     │
 │ results → per-measure caches │ ◄───────────────── │ `op` [+ `ac` sweep] [+ `tran`], read    │
 └──────────────────────────────┘   all op vectors,  │ vectors, `destroy all`                  │
                                    device records,  │ crash → that run is "failed" (row 3),   │
                                    probed AC/tran   │ the worker is restarted                 │
                                                     └────────────────────────────────────────┘
 fallback without FFI: the same workers as `ngspice -b` processes fed by a generated .control script (mode B)
```

**Why processes and not threads with copies:**
- A crash in a vendor model then fails one run instead of the check.
- They scale the same.
- They are the natural unit for a remote or cloud farm later.

---

## 4. The prototype on the MVP amplifier

`run_ce_spec.py`, structure on (for the CE amp that's only the analysis rule: `c_in` can't reach an op), speculative batching on.

| Side | worst_case | Answer key | Verdict | sigma(3) | Independent sigma key | Verdict |
|---|---|---|---|---|---|---|
| bias: VC ≤ 6.5 V | **6.5595** | 6.559537 | **FAIL**, counterexample −10 °C, 12.6 V, R1 +1%, R2 −1%, RC −1%, RE +1%, β 100 | 6.2813 | 6.281320 | PASS (est.) |
| bias: VC ≥ 4.5 V | 4.6921 | 4.692115 | PASS (est.) | 4.8843 | 4.884237 | PASS (est.) |
| gain ≤ 4.83 | 4.7028 | 4.702828 | PASS (est.) | 4.6635 | 4.663552 | PASS (est.) |
| gain ≥ 4.37 | 4.4618 | 4.461772 | PASS (est.) | 4.5177 | 4.517621 | PASS (est.) |
| bass: f_low ≤ 30 Hz | 26.790 | 26.789725 | PASS (est.) | 24.973 | 24.973489 | PASS (est.) |

- **Exact:** every worst_case value equals the 256-corner key. Every sigma(3) value is within 6.4e-5 of the independent optimizer (the loop stops at 1e-4 relative).
- **Same verdicts as round 2.** The values differ slightly because ngspice's Gummel–Poon model isn't the walkthrough formula: 6.5595 vs 6.5905 V, 6.281 vs 6.313 V.
- **Same counterexample corner** as round 2's JSON sketch.

| | Runs (op + op+AC) | Rounds | Backend, P = 1 | Backend, P = 16 |
|---|---|---|---|---|
| worst_case only | 65 (16 + 49) | 3 | 21 ms | 6.4 ms |
| both confidences | 156 (58 + 98) | 6 | 46 ms | 12.7 ms |
| both, without speculative batching | 128 | 8 | 38 ms | 11.8 ms |
| round 2 on its model (for reference) | 68–71 / 194 | — / 7 | — | — |
| through a process per run (mode A, estimated) | 156 × ≈ 7.4 ms | | ≈ 1.15 s | |

**The MVP needs no parallelism:** 46 ms serially. It does need the batch interface, because the same code at 302 knobs needs the pool.

---

## 5. How the loop's cost grows from 8 to 302 knobs

### 5.1 Three architectures

- **Dense** is round 2 as specified. Every side's relevant set is every knob. A request is a full knob point. Runs are still split into op-only (DC sides) and op+AC, and identical points are merged.
- **Structure.** Each measure has a **cone**: the knobs that can reach it, from the influence graph (§6.1, `structure.py`). A request is a partial point over the cone. The cache for that measure is keyed by the values on the cone, so any run that agrees on the cone answers it.
- **Packed** is structure plus packing. Requests that agree on every knob they share go into one simulation. This is Curtis–Powell–Reid column grouping for sparse Jacobians (J. Inst. Math. Appl. 13, 1974; Coleman & Moré, SIAM J. Numer. Anal. 20(1), 1983), applied to the loop's requests, not just to its nudges.

```
 dense request (bias of stage 7):  {temp:-1, vcc:+1, r1_1:0, …, r1_7:+1, r2_7:-1, …, beta_50:0}   302 values
 structural request (same):        {temp:-1, vcc:+1, r1_7:+1, r2_7:-1, rc_7:-1, re_7:+1, beta_7:-1}  7 values
 packing: stage 7's request and stage 12's request agree on temp and vcc → one simulation serves both
```

### 5.2 Runs, rounds and time against the exact key

Both confidences (worst_case + sigma(3)), P = 16 (`final_scale.out`, `final_dense.out`, `scale_dense33.out`). "Serial" is runs × the mode-D cost of §3.1.

| K | Knobs N | Sides | Dense: runs | Structure: runs (op + op+AC) | Packed: runs | Rounds | Structure: backend P=16 | Structure: serial (est.) | Sides = key |
|---|---|---|---|---|---|---|---|---|---|
| 1 | 8 | 5 | 162 | 156 (58 + 98) | 152 | 6 | 0.01 s | 0.06 s | 5/5 |
| 2 | 14 | 7 | 418 | 306 (116 + 190) | 244 | 6 | 0.02 s | — | 7/7 |
| 4 | 26 | 11 | 1,186 | 578 (232 + 346) | 400 | 6 | 0.05 s | — | 11/11 |
| 8 | 50 | 19 | 3,874 | 1,122 (464 + 658) | 712 | 6 | 0.12 s | 1.0 s | 19/19 |
| 16 | 98 | 35 | 13,858 | 2,210 (928 + 1,282) | 1,336 | 6 | 0.36 s | 3.6 s | 35/35 |
| 33 | 200 | 69 | **55,474** | 4,522 (1,914 + 2,608) | 2,662 | 6 | 1.36 s | 15 s | 69/69 |
| 50 | 302 | 103 | ≈ 125,000 (N² trend, not run) | **6,834** (2,900 + 3,934) | 3,988 | **6** | **3.0 s** | 36 s | **103/103** |

worst_case alone: 65, 111, 203, 387, 755, 1,537 and 2,319 runs in **3 rounds** at every size. Dense: 13,266 op + 1,009 op+AC at K = 33.

**Laws.**
- **Structure:** runs = **7.7 N** (worst_case) and **22.6 N** (both). The ratio is 7.7–8.1 and 19.5–22.6 at every K from 1 to 50.
- **Dense:** the op runs grow as ≈ 1.3 N² (52,866 at N = 200).
- **Rounds** don't grow. With unlimited parallelism the check at 302 knobs would take 6 rounds × 7.8 ms ≈ 50 ms.
- **Every one of the 103 sides** equals the exact key, in every architecture that was run (dense up to K = 33). All 103 end with a 1-flip certificate (`cert_check.py`).
- **Python's own cost** (the prototype's, not the design's) shows how dense bookkeeping hurts: dense at K = 33 spent 172 s of wall time on 4.7 s of simulation, because every request was a 200-entry vector. A Rust engine must keep requests sparse too.

**Batch sizes** at K = 50, both confidences: 309, 2,309, 1,804, 1,503, 609 and 300 runs in the 6 rounds. The first round is the shared round (N + 7). The second holds every side's first vertex together with its speculative flips.

### 5.3 Where the runs go

At 302 knobs, the 100 local bias sides cost 2,900 cheap op runs, whose serial cost is 4.9 s. The **3 end-to-end AC sides** cost the rest:
- 3,934 op+AC runs, 31 s serial;
- about 1.3 N each for worst_case and 4 N each with sigma(3).

The dense spec sets the bill, not the number of specs. Packing removes almost all the op runs (2,900 → 54: they ride on AC runs or merge across stages), but it saves only 14% of the time, because they were cheap.

### 5.4 A limiter hidden inside a 300-knob chain

The hiZ stage saturates at 10 of its 256 corners (§2.2). worst_case only, P = 16 (`scale_hiz.out`, `diag_hiz.out`):

| K | Guard | gain.min: engine / key | bass.max: engine / key | Runs | Rounds |
|---|---|---|---|---|---|
| 1 | margin guard | 0.60757 / 0.60757 | exact | 83 | 4 |
| 8 | margin guard + splice | 0.48954 / 0.48954 | exact | 543 | 5 |
| 33 | none | **0.43813** / 0.21532 | **177.03** / 228.41 Hz | 1,542 | 3 |
| 33 | margin guard, no splice (first version) | **0.43813** / 0.21532 | exact | 1,743 | 5 |
| 33 | margin guard + splice | 0.21532 / 0.21532 | 228.41 / 228.41 | 2,143 | 5 |
| 50 | none | **0.23502** / 0.12355 | **216.98** / 260.46 Hz | 2,324 | 3 |
| 50 | margin guard + splice | 0.12355 / 0.12355 | 260.46 / 260.46 | 3,231 | 5 |

**What happened:**
- **Without the guard** at K ≥ 33, the walk certifies a local optimum. The gain counterexample is 2× off, and the reported bass worst case is 17% low. Here both sides FAIL anyway (bass bound 172.9 Hz at K = 33). With a bass bound between 177 and 228 Hz (K = 33) or 217 and 260 Hz (K = 50), it would have been a **false PASS**.
- **The guard finds the saturation.** Stage 17's margin line predicts it can go negative, and the guard simulates that predicted minimum. The stage saturates there (the worst margin found is −0.119 V, at +60 °C and 12.6 V).
- **But the end-to-end gain side still missed it.** Its worst board needs stage 17 saturated *and* the 32 other stages at their low-gain edges, and no single run had both. So C4's rule ("restart if a pooled run beats the side") never fired.
- **Two changes fixed it:**
  1. The guard's run computes **every analysis**. My first version ran the margin's own analysis (op only), so the saturated point never reached the AC sides.
  2. **Splice.** After a side certifies, each guard witness whose device lies in the side's cone is written into the side's incumbent on the device's own knobs (stage 17's knobs, plus temp and vcc). That's 1 run, and the walk continues if it's better.
- **Cost:** +908 op+AC runs at K = 50 (one splice plus its flips per end-to-end side).
- **Verdict:** bass.max's worst point is in saturation, so the verdict table's regime row applies. It gives FAIL here, and would give UNDECIDED (regime) if the side passed.

### 5.5 Screening: what it saves and what it loses

`screen_test.py` drops, per measure, every knob whose nominal slope is under 10⁻³ of that measure's largest slope. It keeps them at nominal, then runs the guarded engine on the hiZ chain.

| K | Knobs dropped from f_low | from gain | Runs: structure → screened | gain.max | gain.min | bass.max: screened / key |
|---|---|---|---|---|---|---|
| 33 | 37 (every rc_k, including rc_17) | 4 (c_in) | 2,143 → 2,049 (−4.4%) | miss (2.48216 / 2.48242) | miss (0.21537 / 0.21532) | **226.35 / 228.41** |
| 50 | 57 (including rc_26) | 7 | 3,231 → 3,082 (−4.6%) | miss | miss | **257.48 / 260.46** |

- **rc_17 is the textbook case.** Its slope on f_low is 7e-9 Hz per ε at nominal, because the collector resistor doesn't move the input pole while the stage is active. In saturation it sets how deep the stage saturates, which moves the pole.
- **Screening saved under 5%** and made the inner bound wrong on all three end-to-end sides.
- **Structure keeps rc_17 at zero cost.** It sees that rc_17 can reach f_low through Q17's operating point.

This answers round 2's open question 3: exclude by structure, never by slope. Slopes only order the work.

### 5.6 sigma(3) at scale, and D7

- **Agreement across architectures:** at K = 16, the end-to-end sigma(3) values agree bit-for-bit (gain.max 0.961909, gain.min 0.813778, bass.max 97.155036).
- **Against an independent key:** all 32 bias sides are within **2.5e-5 V** of a per-stage-type optimizer (`sigma_scale_check.out`).
- **Cost:** sigma(3) is the bigger half of the check, 4,515 of 6,834 runs at 302 knobs. Each side costs (cone statistical knobs + 1) per iteration, and the 3 end-to-end sides dominate.
- **Nesting** (search sigma only where worst_case doesn't PASS) saves only 15% here: 6,834 → 5,784 runs at K = 50, 4,522 → 3,829 at K = 33. It adds 2 rounds. The expensive sigma sides are the end-to-end ones, and those are the failing ones.
- **So D7 holds for DC/AC:** both confidences at 302 knobs cost 3 s on 16 workers.

### 5.7 The RC ladder

The 102-knob ladder (`run_ladder.out`) has cones f_high = 102 knobs and DC gain = 52 knobs (capacitors can't reach an op). The engine used 419 runs (106 op + 313 op+AC) in 2 rounds. All four sides equal the best of the 16 group corners and 416 random vertices (f_high 19.389989–21.968888 Hz, DC gain 0.661425–0.671870), each with a 1-flip certificate. When every knob reaches a spec, it costs about 3 N runs. Structure can't help, but it doesn't have to: 0.04 s on 16 workers.

---

## 6. Cutting the cost: what works on ngspice

### 6.1 Structure: the influence graph

A knob perturbs the circuit at its element's terminals. The perturbation spreads to a node through any element that couples the two nodes, with these exceptions:

| Rule | Why | Effect in the cascade |
|---|---|---|
| A node held by an independent voltage source to ground (and ground itself) is **pinned**: nothing passes through it, unless the knob *is* that source | Its voltage can't change | VCC isolates the stages in DC |
| **DC:** a capacitor is open. **AC:** it couples | Round 2's analysis rule | c_in reaches no bias |
| A VCVS (E) is **one-way**: its control drives its output; nothing flows back, and its output is pinned for arrivals from its other neighbours | An ideal source's output doesn't depend on its load | Downstream stages can't reach upstream biases |
| **AC** also depends on the op: a knob that reaches a nonlinear device in DC injects at that device's terminals in AC | The small-signal stamp moves with the op | rc_17 reaches f_low (§5.5) |
| Temperature injects at every temperature-dependent element | BJTs, resistors with tc1 | temp is in every cone |

- **No simulation, no thresholds.** It never drops a knob that can matter. The only error is numerical cancellation, where the engine still spends a few runs.
- **The cones it found in the cascade:** bias_k ← {temp, vcc, r1_k, r2_k, rc_k, re_k, β_k}, and gain, f_low ← all 6K + 2 knobs.
- **Size:** 143 lines of Python, microseconds per circuit.
- **Precedent:** engine.md §6.2 ("a spec can only depend on knobs in its connected region"), made exact.

**What it doesn't know yet** (open questions): current-controlled sources (F, H), B-sources (all of their inputs count as controls), subcircuits from vendors (treat them as one device: every pin is coupled to every other), and a model shared by many instances (its parameters are one knob that touches every instance).

### 6.2 Analysis projection

A request carries the analyses its measure needs. A run does the union of its packed requests' analyses, never "all analyses of the check".

At 302 knobs, running the 2,900 op-only runs with the AC sweep would add 2,900 × 6.1 ms = 18 s of serial time. Adding a 200 ms transient to each of the 6,834 runs would take **1,367 s instead of 36 s**.

Pooling (C4) still holds per analysis: an op+AC run updates every DC and AC measure.

### 6.3 Packing

Packing cut the op runs from 2,900 to 54 at 302 knobs, and total runs from 6,834 to 3,988 (−42%). The time saving was only 14%, because the runs it removed were cheap.

It pays when the local specs need expensive analyses: many blocks with AC or transient specs of their own. **It is not needed in the MVP**, but it drops in for free once requests are partial points.

### 6.4 ngspice DC `.sens`: accurate, but one output per call

`sens v(out1)` returns d(output)/d(parameter) for every instance and model parameter (81 vectors on the CE amp, 3,952 at 302 knobs).

**Accuracy** against central nudges (h = 1e-4 in ε, `sens_accuracy.out`):

| Knob | Nudge (V per ε) | `.sens` × half-range | Relative difference |
|---|---|---|---|
| vcc | 0.1353776528 | 0.1353776547 | 1.4e-8 |
| r1, r2, rc, re | 0.0770632354 … | 0.0770631567 … | 1.0e-6 |
| β (`q1:bf`) | −0.1410260226 | −0.1410259987 | 1.7e-7 |
| temp (`q1_temp`, the device's own temperature) | −0.32325 (forward, clean instance) | −0.3232541 | 3.7e-6 |

**Cost:**

| Circuit | One `.sens` (1 output) | Nudging (N + 1 ops, every output at once) | `.sens` for K outputs |
|---|---|---|---|
| CE amp (N = 8) | 0.22 ms (8 op) | 0.23 ms | 0.22 ms (K = 1) |
| 50 knobs (K = 8) | 1.7 ms (23 op) | 3.7 ms | 13.5 ms |
| 302 knobs (K = 50) | 19–21 ms (33–47 op) | 124–194 ms | **970–1,070 ms** |

- **`.sens` wins only for few outputs** and many knobs. One nudged run gives every measure, while `.sens` gives one output per call, although it computes the whole dx/dp internally.
- **Temperature comes per device** (`q1_temp`), so the circuit's temperature slope is a sum over devices.
- **AC `.sens` is broken** (§3.5).
- **Verdict:** ngspice is an L0 backend. L1 belongs to our simulator (M8) and to Xyce.

### 6.5 When flips per round (D1) get too expensive

A vertex costs |cone| runs per side per round, whether by flips or by tangent nudges. What matters is |cone| × the run cost.

| Side | \|cone\| | Run | One round of flips, serial | At P = 16 |
|---|---|---|---|---|
| Local DC spec | 7 | op, 1.7 ms | 12 ms | ≈ 2 ms (one run's time) |
| End-to-end AC, 302 knobs | 302 | op+AC, 7.8 ms | 2.4 s | 0.25 s |
| End-to-end audio transient, 302 knobs | 302 | 200 ms | **60 s** | **6.4 s** |
| Buck transient, 50 knobs | 50 | 312 ms | 16 s | 1.7 s |

- **Flips are fine for every DC and AC case measured.**
- **They get too expensive for dense transient specs.** There the loop should run on the cheaper level and flip there (AC, averaged or PSS), and verify the worst point on the transient (engine.md §5.3).
- **Where the backend has L1** (our simulator after M8, Xyce), take tangent slopes in 1 run and flip only the ambiguous knobs (round 2 worst-case report §2.6: at most 4 knobs → at most 15 runs).

### 6.6 How many range knobs can 2^R corners afford?

**Cost of the full corner set at P = 16** (9.4× speedup), from measured run costs (`covering.out`):

| R | 2^R | op+AC, CE amp | op+AC, 302 knobs | Audio transient, 302 knobs | Buck transient |
|---|---|---|---|---|---|
| 2 | 4 | 0.2 ms | 3 ms | 0.09 s | 0.13 s |
| 5 | 32 | 2 ms | 27 ms | 0.68 s | 1.1 s |
| 6 | 64 | 4 ms | 53 ms | 1.4 s | 2.1 s |
| 8 | 256 | 14 ms | 0.21 s | 5.5 s | 8.5 s |
| 10 | 1,024 | 58 ms | 0.85 s | 22 s | 34 s |
| 12 | 4,096 | 0.23 s | 3.4 s | 87 s | 136 s |

**A covering array** of strength t contains every t-way combination of range-knob edges in at least one row. It catches every limiter that needs at most t range knobs at their edges together. The red team's THD case needed 2 (VCC low and cold).

**Size by greedy AETG-style construction** (Cohen et al., IEEE TSE 23(7), 1997):

| R | 2^R | Strength 2 | Strength 3 |
|---|---|---|---|
| 4 | 16 | 6 | 8 |
| 6 | 64 | 7 | 14 |
| 8 | 256 | 8 | 17 |
| 10 | 1,024 | 8 | 19 |
| 16 | 65,536 | 10 | 24 |

**The sigma(k) cross-corner check** (2^R − 1 runs per side) is the bigger problem. At R = 8 with 100 sides that's 25,500 runs. Use the range line of each side to keep only the corners it can't rule out, plus the covering-array rows.

**Recommendation:** enumerate the corners for R ≤ 8 on DC/AC and for R ≤ 5 when a transient spec needs them. Beyond that, use a strength-3 covering array (≤ 24 rows up to R = 16), plus the walk's own flips of range knobs, plus the margin guard.

### 6.7 Transient specs: the budget

Per transient side, from the measured per-side laws (§5.3: about 1.3 N runs for worst_case and 4 N with sigma, plus the shared N + 7):

| Case | Runs | Serial | P = 16 |
|---|---|---|---|
| CE amp THD-grade transient, worst_case | ≈ 15 + 10 = 25 | 0.26 s | ≈ 0.03 s |
| 302-knob chain, audio transient, worst_case | ≈ 700 | 140 s | 15 s |
| same, both confidences | ≈ 1,500 | 300 s | 32 s |
| 50-knob buck, 2 ms start-up, worst_case | ≈ 124 | 39 s | 4 s |
| same, both confidences | ≈ 257 | 80 s | 8.5 s |

The shared round's transient runs serve every transient side. Transient specs belong in engine.md §8's "on demand" tier, with the headline confidence only, and the analysis ladder when a cheaper level exists.

---

## 7. Flows

### 7.1 The cold check

```
 .spl ─► parse, elaborate ─► knob table ─► M1f export: netlist + knob map (knob → alter target) + element graph
                                               │
            structure pass (µs, no simulation) ┴► cone(measure), analyses(measure), device → margins
                                               │
 ┌───────────────────── driver: rounds; every round = one batch to the worker pool ─────────────────────┐
 │ R1 shared: nominal + N nudges + 2^R range corners (or covering rows) + 2 interior temps   N + 7 runs   │
 │    margin guard: each device margin's line → predicted minimum over its cone;                         │
 │    if close to 0 → simulate it with ALL analyses; keep it as a witness                     0–1 each   │
 │ R2… per side, as state machines over the side's cone:                                                  │
 │    worst_case: jump to the line's vertex + speculative flips → accept only improvements →            │
 │                1-flip certificate → splice each witness whose device is in the cone → C4 restart      │
 │    sigma(k):   worst range corner → k-σ point (exact map) with its nudges in the same round →         │
 │                cross-evaluate at the other corners                                                    │
 │    request = (measure, partial point on its cone, analyses) → per-measure cache → pack → batch        │
 └────────────────────────────────────────────────────────────────────────────────────────────────────────┘
                                               │
             pooled inner bound per measure ─► verdict table (C6 + regime row) ─► table + JSON (cone, cost)

 CE amp:     156 runs, 6 rounds, 46 ms serial, 13 ms on 16 workers
 302 knobs:  6,834 runs, 6 rounds, 36 s serial, 3.0 s on 16 workers (worst_case only: 2,319 runs, 3 rounds, 1.1 s)
```

### 7.2 The agent's main flows through this engine

The statuses are round 2's (agent_flows §2: `verified`, `inner_bound`, `estimated`, `stale`) plus one: **`carried`**. It means the verdict was verified on an earlier revision, and the edit touched nothing in that measure's cone. That's exact, not an estimate.

```
 "Why does gain fail?"                                                 cost: 0 runs (+1 optional)
   engine.explain(gain.min) ─► stored record: counterexample, form at the worst point, contributors,
                               guards fired (e.g. "q17 saturates here: regime")
   [sim.run(counterexample)] ─► node voltages + device regions            1 run: 0.5 ms / 7.8 ms
   agent may say: "FAILS at <corner>: q17 saturated, stage gain 0.61" (verified + inner_bound)

 "Fix bass"                                                            cost: ≤ 10 runs, then 1 re-check
   explain ─► contributors ─► agent drafts 2–3 edits (lang.propose)
   engine.what_if(p) ─► cone check: which sides can the edit reach? (e.g. 5 of 69)
                    ─► re-simulate those sides' stored worst points (≤ 10 runs, 1 round)
                    ─► log-unit estimates for them; the other 64 sides: carried
   engine.verify(p) ─► re-check with the carried cache: only runs whose cone key changed
   agent may say: what_if: "still FAILS at <point>" (inner_bound) or "≈ 24 Hz (estimated)";
                  only after verify: "PASS (est.)" (verified); unaffected sides: "unchanged" (carried)

 What-if on an edit (while typing)                                     cost: ≤ 1 round
   same as what_if; never shows PASS without verify

 "Add a spec"                                                          cost: 0 … a few runs
   new bound on a stored measure ─► verdict from the stored extreme      0 runs
   new measure on recorded quantities (a margin, a node already in every run's op vector)
                                  ─► shared round cached; only its walk  21 runs (§7.3)
   new analysis (first transient spec) ─► its own shared round + walk    §6.7

 "Pin a real part" (2N3904 on q17)                                     cost: that part's cone
   parts.extract ─► record (reviewed = false) ─► knob ranges (β 70..300) + model parameters
   cones reached: bias17, msat17, gain, f_low ─► re-check those          1,430 runs at 200 knobs
   a .model shared by all 50 stages is one knob in every cone ─► everything re-runs
   agent may say: verdict + tag relies-on-unreviewed-part-data
```

| Flow | Calls | Runs: CE amp | Runs: 200–302 knobs | Latency at P = 16 | Strongest claim |
|---|---|---|---|---|---|
| Why does X fail | explain (+ sim.run) | 0 (+1) | 0 (+1) | < 10 ms | verified FAIL + counterexample |
| Fix X: candidates | explain, propose, what_if × 3 | ≤ 10 each | ≤ 10 each | ≈ 1 round each | FAIL proven (inner_bound); estimated; carried |
| Fix X: verify | verify | ≤ 156 | 2,666 (one-block edit, 200 knobs) | 1.3 s | verified |
| What-if on typing | what_if | ≤ 10 | ≤ 10 | ≈ 1 round | same as above, never PASS |
| Add a spec | lang.propose, check | 0–20 | 0–21 | ≤ 1 round | verified |
| Pin a part | parts.extract, propose, verify | ≤ 156 | 1,430 | 0.7 s | verified + tag |

### 7.3 What an edit costs with the carried cache

Measured at 200 knobs and 69 sides, both confidences (`edit_flows.out`). The cold check is 4,522 runs, 6 rounds, 1.45 s.

| Edit | op runs | op+AC runs | Total | Backend | Sides whose values changed |
|---|---|---|---|---|---|
| Value edit in one block: rc_17 × 1.1 | 58 | 2,608 | 2,666 | 1.34 s | 5 (bias17 × 2, gain × 2, bass) |
| Pin a part: β_17 100..300 → 70..300 | 29 | 1,401 | 1,430 | 0.74 s | 3 |
| Range edit on a shared knob: vcc ±5% → ±10% | 1,914 | 2,406 | 4,320 | 1.38 s | 69 |
| New bound on a stored measure: bias17 ≤ 6.0 V | 0 | 0 | **0** | 0 | 0 |
| New spec on a recorded measure: msat17 ≥ 1 V | 21 | 0 | 21 | 0.01 s | 0 |

- **The 64 untouched bias-side pairs replay from cache.** The end-to-end AC sides re-run in full, because rc_17 is in their cone.
- **Physical keys pay off:** changing β's low edge keeps every point with β_17 ≥ its nominal, so only 1,401 of 2,608 AC runs re-ran.
- **For round 2's C11** (edits make stored lines nonsense): the instant tier should re-simulate the stored worst points only for the sides in the edit's cone, and carry the rest.

---

## 8. What the MVP must get right now, and what can stay simple

**Get right now.** Each is cheap on the CE amp and expensive to retrofit.

| # | Choice | Why now |
|---|---|---|
| 1 | **A request is (measure, partial point over the measure's cone, analyses).** The MVP's cone pass can start as the analysis rule, but the types carry cones | Every loop, cache and test is written against the request type. Changing it later touches everything |
| 2 | **The cache is per measure, keyed by physical values over the cone** (+ a hash of the cone's fixed content) | Round 2's C12 key is the special case. Physical keys survive edits (§7.3) |
| 3 | **A run returns every measure its analyses produce:** the full op vector, device records, probed AC/tran vectors. A new spec on recorded quantities then reuses old runs | Round 2's "every measure" breaks as soon as there's a transient spec (§6.2) |
| 4 | **The batch Backend takes per-run analysis sets, and the M1f exporter emits a knob → `alter` target map** (+ `tnom`) | The worker pool and `alter` depend on the map. Without it, runs fall back to `alterparam` + `reset` (2.5–4.5×) or a process per run (9–122×) |
| 5 | **Loops are state machines, and a driver merges them into rounds** (round 2, engine_flows §7 item 3), with speculative batching | Rounds didn't grow with size here; latency = rounds |
| 6 | **Guard witnesses run with every analysis, and are spliced into every side whose cone holds the device** | The only thing that fixed the 300-knob hidden limiter (§5.4) |
| 7 | **Never exclude a structurally reachable knob by its slope** | Screening saved < 5% and broke exactness (§5.5) |

**Can stay simple:**

| Item | Until | Why it can wait |
|---|---|---|
| The worker pool (serial is fine) | Past about 30 knobs, or the editor | The CE amp takes 46 ms serially |
| The full influence graph (beyond "caps can't reach an op") | Multi-block circuits | Every CE-amp knob reaches every AC measure. The graph pass is ~150 lines when needed |
| Packing | Many blocks with AC or transient specs of their own | −14% time at 302 knobs, all of it on cheap op runs |
| A persistent cache across revisions | `spicy watch`, the editor | Only needed across edits. But key by physical values now |
| Covering arrays | R > 8 (R > 5 for transient) | The MVP has R = 2 |
| `.sens` | Never on ngspice | §6.4 |
| The nesting shortcut for sigma(k) | When sigma dominates a budget | Saves 15% here |

---

## 9. Recommendations for the final plan

1. **Adopt cones as a first-class engine concept:** the request type, the cache key and the verdict record (which knobs can reach this spec).
   - *Reason:* runs grow as 7.7 N (worst_case) and 22.6 N (both) with cones, and as ≈ 1.3 N² without. That's 6,834 vs about 125,000 runs at 302 knobs, with identical, exact verdicts (§5.2).
2. **Compute cones from the netlist graph, never from slopes.** The rules: pinned nodes, capacitors open in DC, one-way controlled sources, op-dependence of AC, temperature to every temperature-dependent element.
   - *Reason:* exact and free. Screening by slope saved 4–5% and missed on 3 of 3 end-to-end sides (§5.5).
3. **Change C4:** a run returns every measure its analyses produce, and requests carry their analyses.
   - *Reason:* one transient spec would otherwise multiply the whole check's cost about 38× (§6.2).
4. **Extend C3:** margin-guard points run with every analysis, and their witnesses are spliced into every side whose cone contains the device.
   - *Reason:* without it the 300-knob hidden limiter gave a counterexample 2× off. At 302 knobs, a bass bound between 217 and 260 Hz would have been a false PASS (§5.4).
5. **Backend:** a pool of persistent worker processes, each hosting one libngspice with the circuit loaded once, knobs set by `alter`/`altermod`/`option temp`, and `destroy all` after each run. Fallback: batch-mode `ngspice -b` workers fed by `.control` scripts.
   - *Reason:* a process per run is 9–122× slower for op and 2.6–14× for op+AC. Workers scale 9.4× on 16 cores. A crash costs one run (§3).
6. **M1f exporter requirements:** a knob → alter-target table, `.options tnom=` written out, XTB carried from part records, and `.param` only for knobs used inside expressions.
   - *Reason:* §3.5. The defaults silently change the circuit: tnom 27 °C, no β tempco.
7. **Treat ngspice as L0 permanently.** Don't plan `.sens`.
   - *Reason:* DC `.sens` is one output per call (50 outputs: about 1 s vs 0.12–0.19 s by nudging). AC `.sens` returns ≈ 0. `sens` also disables `option temp` (§6.4, §3.5).
8. **Measure f_low by cubic interpolation of dB vs ln f on a 40-per-decade sweep,** not by extra AC solves.
   - *Reason:* within 5e-7 at 0 extra runs. On ngspice each extra frequency is a whole `ac` command with a new op (§3.5).
9. **Range corners:** enumerate for R ≤ 8 (DC/AC) and R ≤ 5 (transient); beyond that, a strength-3 covering array. Prune sigma's cross-corner runs with each side's range line.
   - *Reason:* at 302 knobs, 2^10 corners cost 0.85 s on AC but 22–34 s on transient. A strength-3 array needs ≤ 24 rows up to R = 16 (§6.6).
10. **Transient specs get their own tier:** on demand, headline confidence only, and the analysis ladder when a cheaper level exists.
    - *Reason:* a transient side at 302 knobs costs about 140 s serial for worst_case (15 s at P = 16); a buck at 50 knobs about 39 s (§6.7).
11. **Keep D1, D2, D3, D6 and D7 for DC/AC.**
    - *Reason:* all were exact at scale. D7 costs 3× the runs and is still 3 s at 302 knobs (§5.6).
12. **Add the cascade with its exact product key to the M3 test suite:** K = 8 in CI, K = 50 nightly, and the hiZ variant as the hidden-limiter regression.
    - *Reason:* the only fixture found with an exact key at any size. Building it takes 9,216 single-stage runs (§2.2).
13. **Add the `carried` status to the agent's vocabulary** (agent_flows §2).
    - *Reason:* after a one-block edit at 200 knobs, 64 of 69 sides are exactly unchanged by structure. Saying so is a proof, not an estimate (§7.3).

---

## 10. Open questions

1. **Cones for real models.** Current-controlled sources, B-sources, vendor subcircuits as black boxes, and `.model` cards shared by many instances. How conservative must the rules be, and how much of the O(N) law survives a board full of vendor op-amp macromodels?
2. **Dense end-to-end specs are the bill.** Can hierarchy make them sparse? The cascade's own key shows what composing per-block transfer functions does: 9,216 single-stage runs gave an exact key at any K. But that needs unilateral interfaces. With real loading, is composition through port impedances (engine.md §7, `z_src`/`z_load`) accurate enough to *search* on, and verify end to end?
3. **Splice generality.** It fixed a single saturating stage. Does it hold for two interacting limiters, or for a limiter driven by a joint move of knobs in two cones?
4. **Rounds with real noise.** Rounds stayed at 3/6 here. Do solver noise or interior optima (power, phase margin) add rounds at scale? What round budget should the engine state?
5. **Transient with ngspice at L0.** Is the ladder (search on AC or averaged models, verify on transient) available for most board transients, or do we need an L2 backend (Xyce's transient sensitivities) sooner than planned?
6. **The cache key's "fixed content" hash.** Which fields of a vendor model, subcircuit and analysis settings must go into it so that a carried verdict can never be stale?
7. **Remote workers.** A 64-core box would cut the 302-knob check below 1 s. Is a shared worker farm, and a shared result cache keyed as in §8, in scope for the editor?
8. **Covering arrays in the guard set.** They catch every t-way edge combination of range knobs, but not a limiter at an interior range value (the P-FET cold-crank case in engine_power.md). What finds those cheaply at R ≥ 8?
