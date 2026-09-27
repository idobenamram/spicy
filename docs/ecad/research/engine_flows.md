# Engine Flows: From an Edit to a Verdict on Screen

> 2026-09-27 · Research report for the M3 plan (the engine MVP, roadmap §6). Topic: every flow the engine runs, the data passed between its stages, and how to make it fast.
> Every number is either computed by the scripts in `$R/flows/` (R = `/root/.claude/jobs/443154a8/tmp/engine_research`; §0 and the appendix say how) or cited as a paper section or a `file:line` in a reference source. Reference sources: `/tmp/refs/{ngspice,xyce,gnucap,vacask,ra}`. Only this file was added to the repo.

## Summary

The engine is a chain of stages: parse → elaborate → knob table → lower → plan specs → runs → measure → affine forms → loop → verdicts → display. Almost all the cost is in one stage, the simulator runs. On the CE amplifier, a cold `spicy check` with both confidences needs **194 runs (167 operating points, 116 AC sweeps) in 7 rounds**, and on every worst_case side it lands on the 256-corner answer key. The whole job takes about 8 ms on one core, even with today's dense AC. So the MVP's question isn't speed. It's which structure makes the later flows cheap and trustworthy.

Four findings drive the recommendations:

1. **A content-addressed run cache gives exact early cutoff for free.** Each run is keyed by what the simulator actually reads, projected per analysis (an operating point never reads a capacitor's value). After `c_in` 1 µF → 2.2 µF, the bias spec replays all 92 of its runs from the cache and keeps bit-identical verdicts. No dependency bookkeeping is needed.
2. **Replay the loop's standard path; don't warm-start it from the last revision's worst points.**
   - Warm starts saved at most 7% of the solves.
   - They cost 14% *more* solves when only some specs changed.
   - They made sigma(3) values depend on the edit history (differences up to 2.8e-5 V).
   - Previous worst points should only decide which runs go first.
3. **The loop is bound by rounds, not runs.** At L0, sending each predicted worst point together with its re-linearization nudges costs no extra runs, and it cuts the rounds from 11 to 7. With 20 cores the CE amp's check would take under 1 ms.
4. **Instant predictions while editing need slopes in log units.** For `c_in` → 2.2 µF:
   - the stored straight line in ε predicts f_L = **−4.0 Hz**;
   - a line in ln f_L vs ln C predicts **9.128 Hz**;
   - the simulation gives **9.127 Hz**.

In the terms of *Build Systems à la Carte*, the engine is a suspending scheduler with constructive traces, the cell that paper calls "Cloud Shake".

The MVP needs only a small part of all this:
- a driver that runs the per-side loops in rounds;
- an in-memory run cache that lives for one check;
- a batch `Backend::run`.

Salsa, persistence, cancellation, scheduling tiers and instant predictions can wait for the editor.

---

## 0. What was computed, and how it was checked

| Script (`$R/flows/`) | What it does |
|---|---|
| `flows_model.py` | The walkthrough's equations (`ce_amp_model.py`), made editable (knob nominals and ranges change per revision, and an optional load capacitor can be added). Split into the two analyses the simulator runs: an operating point that reads no capacitor, and an AC sweep at that operating point. Gain is \|H(1 kHz)\| including C_in's high-pass; bass is f_low(−3 dB) |
| `engine_sim.py` | The run flow: which analyses each spec needs, the loops (worst_case and sigma(3)) written as coroutines that yield the knob points they need, a driver that merges every loop's requests into one batch per round, a run cache with per-analysis keys, and counters |
| `cold_check.py` | The cold check; costs per spec and per confidence |
| `triggers.py` | Every trigger of §3 |
| `cost_model.py` | Scaling to 30 and 50 knobs; time estimates from measured per-run costs |

**Checks:**
- **It reproduces the walkthrough.**
  - The bias 3σ loop goes predicted 6.2249 / simulated 6.2964 → 6.3278 / 6.3406 → 6.3425 / 6.3428 (walkthrough §5.6: 6.225/6.297, 6.328/6.341, 6.343/6.343).
  - The gain-minimum trap: the first corner (hot) gives 4.4673; re-linearizing flips temperature to cold, 4.4610 (walkthrough §5.7).
  - Headroom: interval 4.81 V, affine 5.26 V, true 5.17 V (walkthrough §8).
- **Every worst_case value equals the brute-force answer key** (all corners), on the base design and on four edited revisions (§3.1–3.4).
- **Limits.** The model is exact to double precision, so its nudged slopes carry no solver noise; the real simulator's will (roadmap M2e). The counts describe the flow. The real engine may need an extra round when noise flips a slope's sign. The MVP's own numbers will differ a little too: our Ebers–Moll simulator gives VC = 5.5216 V, the model 5.4999 V.

**Terms used below:**
- **Run:** one knob point, simulated with one set of analyses. An AC run always includes its operating point.
- **op / AC:** operating points and AC sweeps actually computed (an AC sweep is ≈ 50 frequencies).
- **Round:** a batch of runs that don't depend on each other, so they can run in parallel. The next round depends on this one's results.
- **Side:** one direction of one spec. bias has two (max ≤ 6.5 V, min ≥ 4.5 V), gain two, bass one: 5 sides. With 2 confidences that's 10 loops.
- **Hit:** a request answered from the run cache.

The model's gain is now \|H(1 kHz)\|, which is what the MVP spec measures, instead of the walkthrough's mid-band gain. C_in's high-pass costs 0.02% at 1 kHz, so the gain answer key is 4.4610 … 4.7024 instead of 4.4626 … 4.7030.

---

## 1. The stages, and the data between them

This extends `pipeline.md` §2–§4 downstream into the engine. It stays consistent with `circuit.md`: `Circuit` holds the wiring and is shared; `Params` holds the values, one per run; the `Binding` maps knobs to `Params` fields.

### 1.1 The whole chain

```
 ce_amp.spl (1,020 bytes)                                   crate          rebuilt
   │ lex + parse                                             spicy_lang     every edit
   ▼
 Tokens + Ast (spans)
   │ elaborate
   ▼
 Design ──► FlatDesign + KnobTable + Contract                spicy_model    per design revision
   │                                  │
   │ Backend::prepare (lower once)    │ plan_specs            spicy_engine   per (revision, contract)
   ▼                                  ▼
 Prepared                            SpecPlan ×3   measure, analyses, relevant knobs, sides, bound, confidence
 { Arc<Circuit>, Params₀,             │
   Binding, Arc<Plan> }               ▼
   ▲                                LoopState ×10  (5 sides × 2 confidences), each yields KnobPoints
   │                                  │
   │                                Driver: merge + dedup → RunBatch (one round), a RunKey per request
   │                                  ▼
   │                                RunCache ──hit──► Arc<RunResult>
   │                                  │ misses
   └────────── Backend::run ◄─────────┘   parallel inside the backend, one Workspace per thread
                                      │
                                      ▼
                                    RunResult ──► measurement library ──► Measured
                                      │
                                      ▼
                                    LoopState.resume(values) ──► AffineForm ──► next points, or done
                                      │
                                      ▼
                                    Verdict ×10 ──► spicy check table / --json   (later: badges, spec table, inlay hints)
```

### 1.2 Every struct: where, when, why

`KnobTable`, `Binding`, `Plan` and `Workspace` are already in `pipeline.md` §3. The rest are new.

| Struct | Crate | Built by, when | Lives | Its job |
|---|---|---|---|---|
| `KnobTable` | `spicy_model` | elaboration, per revision | per revision | Knob id → path (`amp.r1.value`), kind, nominal, lo, hi, distribution. Knob identity is the **path**, so results from an older revision can be matched after the table changes |
| `Contract` (resolved) | `spicy_model` | elaboration | per revision | Assumptions (range knobs), measures and specs, with probes resolved to nets |
| `Binding` | `spicy_backends` | lowering, once per revision | engine job | Knob → `Params` field, `Conditions.temp`, or a bench source (`vcc.v` → the default bench's DC source) |
| `Prepared` | `spicy_backends` | `Backend::prepare` | engine job | `Arc<Circuit>`, nominal `Params₀`, `Binding`, and our simulator's `Arc<Plan>` (once steps 2/4 exist). Lowered **once**; each run copies `Params₀` and applies the binding (`circuit.md` §5) |
| `Plan`, `Workspace` | `spicy_simulate` | `pipeline.md` §3 | shared / per thread | Unchanged |
| `SpecPlan` | `spicy_engine` | once per (revision, contract) | engine job | Per spec: the measure program; the analyses it needs (Op, or Ac with its frequencies); **the knobs that can reach it** (§4.6); its sides, bound and confidence |
| `KnobPoint` | `spicy_engine` | a loop, per request | until its results arrive, then in the loop's history | Dense `Box<[f64]>` of ε by `KnobId`. Irrelevant knobs sit at exactly 0.0, so points hash equal |
| `RunRequest`, `RunBatch` | `spicy_engine` | the driver, per round | one round | Point + analysis set (Op ⊂ Ac) + who asked; merged and deduped across loops |
| `RunKey` | computed by the backend | per request | as long as the result is kept | A hash of what the simulator reads for that analysis (§4.2) |
| `RunResult` | `spicy_backends` | per run | the run cache | Solutions by id (op vector; AC matrix of frequency × unknown), each device's operating region, Newton stats, or an error |
| `Measured` | `spicy_engine` (measurement library) | per run × measure | in the loop's history | The value of each measure, plus its numerical error band |
| `AffineForm` | `spicy_engine::affine` | per linearization | in the loop; the last one in the verdict | Center, a coefficient per knob, error term, **the point it was taken at** and the knob scales, so it can be re-expressed later (§4.7) |
| `LoopState` | `spicy_engine` | per (side, confidence), per check | one check | The loop as a state machine: phase, visited points and values, forms, guards fired, runs and rounds used |
| `Verdict` | `spicy_engine` | when its loop finishes | until the next revision; later persisted | Verdict kind, bracket, worst value, counterexample (a full knob point), contributors, tags, method and cost (runs, rounds, level L0) |
| `RunCache` | `spicy_engine` | per check (MVP); later per session and on disk | see §4.2 | `RunKey` → `Arc<RunResult>` |

### 1.3 One knob point through every stage

The bias.max worst_case corner, as the loop meets it in round 2:

```
KnobPoint  ε = [T −1, VCC +1, R1 +1, R2 −1, RC −1, RE +1, C_in 0, β −1]     asked for by bias.max / worst_case
Binding    Conditions.temp = −10 °C      vsources[0].dc = 12.6 V
           resistors: r1 47.47k, r2 9.9k, rc 4.653k, re 1.01k      bjt_models[q1].bf = 100
RunKey     Op: hash(structure key, Params without capacitor values or AC magnitudes, temp, solver config)
           → miss
Backend    Params_t = Params₀ + binding writes → Derived (β at −10 °C = 82.5) → Newton on the shared Plan
RunResult  IC 1.292 mA, V(out) 6.5905 V, VCE 5.27 V (forward active: no region change)
Measured   bias = 6.5905 V
LoopState  6.5905 > 6.5 → FAIL is now certain. The inward nudges (same round) keep every slope's sign → converged
Verdict    bias.max, worst_case: FAIL, 6.5905 V, counterexample = this point (C_in: any), 8 runs of its own
```

### 1.4 Where the chain can stop early

Each stage's output is compared with the previous revision's. When it's equal, everything below it is reused. This is "early cutoff" (*Build Systems à la Carte* §2.3; salsa calls it backdating, §6.1).

| Edit | Tokens/Ast | Design (no spans) | KnobTable | Circuit (wiring) | Params₀ | SpecPlan | Runs re-done |
|---|---|---|---|---|---|---|---|
| Comment or whitespace | changed | **equal: stop** | — | — | — | — | 0 |
| Rename net `base` → `b` | changed | changed | equal | equal (names are in `CircuitNames`) | equal | equal | 0 |
| `c_in` 1 µF → 2.2 µF | changed | changed | 1 nominal | equal → `Plan` kept | 1 field | equal | AC only (§3.1) |
| `r1` 47k → 51k | changed | changed | 1 nominal | equal → `Plan` kept | 1 field | equal | all (§3.2) |
| Add `c_load` | changed | changed | +1 knob | **changed** → new `Plan` | +1 device | equal | AC all, DC kept if the key drops capacitors (§3.3) |
| Change a spec bound | changed | changed | equal | equal | equal | 1 bound | 0 (§3.4) |

---

## 2. The cold check (`spicy check`), round by round

This is the MVP's only flow: one-shot, no editor, empty cache.

```
round 1  (18 requests → 17 runs)  SHARED by every spec
         safety net: 4 range corners (T × VCC) + temperature scan at 5 points (VCC nominal)
         nominal
         nudges: 8 knobs at +0.001 ε   (c_in's nudge needs AC only: its operating point = nominal's)
         → nominal VC 5.4999 V · |H(1k)| 4.5908 · f_L 20.079 Hz; first forms; predicted worst corners
           (bias max 6.4083 V predicted)

round 2  (76 requests)  every loop at once
         worst_case: each side's predicted corner + its 7–8 inward nudges
         sigma(3):   each side's worst range corner (from the safety net) + nudges of its statistical knobs
         → bias.max FAIL (6.5905 V, counterexample). bias.min, gain.max, bass.max converged.
           gain.min's slopes flip temperature → one more corner

round 3  (42)  gain.min's cold corner → 4.4610, converged · sigma(3) first kσ points (bias.max 6.2964 …)
round 4  (33)  sigma(3) iterations
round 5  (21)  gain and bass sigma(3) converged
round 6  (9)   bias.min sigma(3) converged
round 7  (3)   joint check: bias.max's 3σ point at the other 3 range corners → none worse → done
```

**Results** (the model, 8 knobs):

| Spec | worst_case | Answer key (corners) | sigma(3) | Loop rounds (wc / 3σ) |
|---|---|---|---|---|
| bias ∈ 4.5 … 6.5 V | 4.6597 … **6.5905** → **FAIL** | 4.6597 … 6.5905 | 4.8517 … 6.3428 → PASS | 2 / 7 |
| gain ∈ 4.37 … 4.83 | 4.4610 … 4.7024 → PASS | 4.4610 … 4.7024 | 4.5171 … 4.6639 → PASS | 3 / 5 |
| bass ≤ 30 Hz | 26.7378 → PASS | 26.7378 | 25.3399 → PASS | 2 / 5 |

These are the walkthrough's verdicts without the aging knob (post-MVP): bass passes because C_in can't age.

**Cost:**

| | Runs | op | AC | Rounds |
|---|---|---|---|---|
| Shared round | 17 | 16 | 17 | 1 |
| worst_case only | 69 | 64 | 53 | 3 |
| sigma(3) only | 142 | 119 | 80 | 7 |
| **Both** | **194** | **167** | **116** | **7** (11 without batching nudges with the corner, §4.3) |
| Brute force, worst_case only | 256 | 256 | 256 | 1 |

| Each spec checked on its own (both confidences) | Runs |
|---|---|
| bias (worst_case 32 + sigma(3) 76, sharing their first round) | 92 |
| gain (44 + 65) | 92 |
| bass (26 + 41) | 50 |
| **Sum if specs don't share** | **234**; checked together: **194** (−17%) |

At 8 knobs, L0's worst_case (69 runs) is only 3.7× cheaper than brute force (256). The loop's payoff starts around 10 knobs (§4.8). sigma(3) has no corner-based answer key; M3d checks it with seeded Monte Carlo, as the roadmap plans.

---

## 3. The triggers

Each trigger is shown from the edit to what the user sees. "T0" is synchronous work on the edit, and "T1" is background work (the tiers are defined in §5.1). The MVP has only the cold check (§2); the rest is for `spicy check` re-runs and, later, the editor. All numbers start from the cold check's cache.

### 3.1 Value edit: `c_in` 1 µF → 2.2 µF

```
 edit: c_in value 1uF ± 20% → 2.2uF ± 20%
   │
 T0 ─┬─ parse, elaborate ............................................ re-run (µs)
     ├─ KnobTable: amp.c_in.value nominal 1µ → 2.2µ ................... 1 knob changed
     ├─ lower: Circuit structure key equal ............................ cache → Plan kept
     │         Params₀.capacitors[c_in].c = 2.2µ ...................... changed
     ├─ SpecPlan ...................................................... cache
     ├─ which specs can change? (structure, before any run)
     │     bias reads the operating point; an op never reads a capacitor → UNAFFECTED, badge stays verified
     │     gain, bass read the AC sweep, which reads c_in ...................... affected
     └─ predictions from the stored forms ─────────► hints marked "predicted"
           bass   f_L ≈ 9.13 Hz nominal · worst ≈ 12.16 Hz (wc) / 11.52 Hz (3σ)
           gain   |H(1k)| 4.5908 → ≈ 4.5923
   │
 T1 ─┬─ run planning   bias:  its 92 runs all hit → verdict bits unchanged, 0 simulations
     │                 gain:  30 op + 90 AC new
     │                 bass:  15 op + 49 AC new    (the shared round is counted once)
     ├─ backend runs   45 op + 116 AC   (78 requests served by the cache)
     └─ measure → forms → loops → verdicts ─────────► hints turn "verified"
           bass   worst 12.1535 Hz (wc) / 11.5181 Hz (3σ): PASS, 17.8 Hz of margin
           gain   4.4622 … 4.7029 (wc): PASS
```

**How the engine knows bias is unaffected: two mechanisms.**
1. **Structure, known at T0:** the SpecPlan says bias depends only on the operating point, and an operating point reads no capacitor. So the UI doesn't even mark bias stale.
2. **Content, exact:** in T1 the bias loop replays, and every run key (its DC projection) is unchanged, so every run hits. That gives the same verdict, bit for bit, without the engine having to prove anything. Mechanism 2 is the ground truth; mechanism 1 is a UI shortcut.

**The instant prediction versus the verified value:**

| f_L prediction from the stored nominal form | Value |
|---|---|
| Straight line in ε (the form as stored today). The edit is ε = +6, far outside the box | **−4.011 Hz** |
| Straight line in ln C | 4.251 Hz |
| Line in ln f_L vs ln C (slope −0.9998) | **9.128 Hz** |
| Verified (simulated nominal) | **9.127 Hz** |
| worst_case: stored 26.7378 → log–log prediction / verified | 12.1555 / **12.1535** Hz |
| sigma(3): stored 25.3399 → log–log prediction / verified | 11.5199 / **11.5181** Hz |
| gain: 4.59081 → log–log prediction / verified | 4.59227 / **4.59155** |

f_L ∝ 1/C exactly in this circuit, so the log–log line is nearly exact. The walkthrough's "fixes the engine can check instantly" (§6: 2.2 µF → 15.2 Hz, with aging) relies on exactly this.

**Background re-check cost:** 45 op + 116 AC, against 167 + 116 for a cold check. Only AC is re-run. The 45 new operating points come from gain's sigma(3) path, which moved slightly because \|H(1 kHz)\| depends weakly on C_in; its new points no longer match the cached ones exactly.

### 3.2 Value edit: `r1` 47k → 51k

```
 edit: r1 value 47k ± 1% → 51k ± 1%
   │
 T0 ── KnobTable 1 nominal · Circuit equal (Plan kept) · Params₀ 1 field · all 3 specs affected (r1 reaches the op)
   │   predictions: VC ≈ 6.16 V nominal (line in ε) · gain ≈ 4.582 · f_L ≈ 19.79 Hz
   │
 T1 ── every run key changes (R1 is in every run) → 167 op + 116 AC, a full re-check
       → bias: 5.2696 … 7.1808 V (wc) FAIL · 5.4473 … 6.9520 V (3σ) FAIL
         gain 4.451 … 4.695 PASS · bass 26.40 Hz PASS      (all equal to the answer key)
```

| Nominal after the edit | Line in ε (ε = +8.51) | Power law (log–log) | Verified |
|---|---|---|---|
| bias VC | 6.1558 V | 6.1668 V | **6.1119 V** |
| gain | 4.5824 | 4.5828 | **4.5822** |
| bass f_L | 19.7896 Hz | 19.8032 Hz | **19.8122 Hz** |

VC isn't a power law in R1, so neither form is exact for a jump of 8.5 tolerance widths. They're off by 0.044–0.055 V. The preview still does its job: "VC ≈ 6.16 V, close to the 6.5 V limit" is visible at once, and the verified FAIL follows within milliseconds.

### 3.3 Topology edit: add a part

`let c_load = Capacitor { a: output, b: gnd, value: 100pF ± 10% };` limits the bandwidth. f_H = 1/(2π · 4.7k · 100 pF) = 338.6 kHz.

```
 edit: + c_load
   │
 T0 ── Design, FlatDesign: +1 instance · KnobTable 8 → 9 (amp.c_load.value, statistical)
   │   Circuit: +1 capacitor → NEW structure key → new Plan (pattern + KLU analyze ≈ 1.2 µs measured at this size)
   │   old knobs keep their paths, so old worst points still name valid knobs
   │   predictions: none for the new knob (no stored slope); every other form unchanged
   │
 T1 ── AC: every key changes (a new device) → 129 AC
       op: an op never reads a capacitor → if the op key hashes the circuit WITHOUT capacitors: 53 op new
                                           if it hashes the whole circuit:                  193 op new
       → |H(1k)| 4.590813 → 4.590793 (−4.4 ppm); verdicts unchanged; answer key (512 corners) matched
```

This shows what a projected key buys, and what it costs to get right. Dropping capacitors from the *structure* part of the op key is only sound if the DC solver really ignores them. It does: they're open circuits.

### 3.4 Contract edits

**(a) A new spec:** `spec top_room: vcc.v - dc(output.v) >= 5V;` (walkthrough §8).

```
 T0 ── SpecPlan +1 (analysis: Op; knobs: all but c_in) · everything upstream cached
   │   prediction from the stored VC form, with VCC as one shared knob: min ≈ 5.2624 V
   │   (plain intervals would say 4.8095 V: they pair "VCC low" with "VC high", which can't both happen)
 T1 ── 57 requests: 25 hits (safety net, nominal, nudges), 32 op new, 0 AC
       → worst_case 5.1647 V (true: 5.1647) PASS · sigma(3) 5.3830 V PASS
```

**(b) A changed bound:** `bias` 4.5 … 6.5 V → 4.0 … 7.0 V.

```
 T0 ── SpecPlan: 1 bound · verdict recomputed from the stored extremes: 0 runs
       worst_case max 6.5905 < 7.0 → PASS (was FAIL) · sigma(3) 6.3428 → PASS
```

This is free only if every loop converges its extreme, **even after a FAIL is already certain.** A loop that stops at the first violating point stores a value that isn't the extreme.

**(c) A changed assumption:** `vcc.v` 12 V ± 5% → ± 10%.

```
 T0 ── KnobTable: VCC range 11.4…12.6 → 10.8…13.2 · Circuit and Params₀ unchanged (nominal 12 V)
   │   prediction: double the VCC coefficient of each stored form
   │   bias.max 6.5905 → ≈ 6.7258 · gain.max ≈ 4.7084 · bass ≈ 26.7388
 T1 ── only runs with VCC at nominal hit (16) → 156 op + 104 AC new
       → bias: 4.5494 … 6.7673 (wc) FAIL · 4.7288 … 6.5051 (3σ) → FAIL (was PASS at 6.3428)
         gain 4.4532 … 4.7071 · bass 26.7404: PASS (all equal to the answer key)
```

The wider supply flips bias at 3σ, by 5 mV. That makes it a good test case.

These 16 hits exist only because the key is **physical** (the resolved values), not the knob position. With ε-keys tied to the revision, the same re-check gets 0 hits across revisions (4 within the check). The knob's meaning changed, while the physical nominal point didn't.

### 3.5 Confidence change: `#[confidence(worst_case)]` ↔ `sigma(3)`

```
 T0 ── SpecPlan: 1 confidence
       if both confidences are always computed: the headline flips, 0 runs
       if only the declared one is: 103 op + 63 AC new (21 hits) for sigma(3) after worst_case
```

engine.md §3.6 already says both values are shown whenever they disagree. So computing both is already implied. It costs 194 runs instead of 142 for sigma(3) alone (+37%), and it makes this trigger free.

### 3.6 The background check

This is T1 itself, for any of the edits above: the loops of every affected spec, from an up-to-date cache. §5 gives its ordering, cancellation and progress. What the user sees, in order:

```
edit ─► T0 hints "predicted" (µs) ─► round 1: nominal values, safety-net corners (inner bounds)
     ─► round 2: FAIL verdicts with counterexamples; most worst_case sides converged
     ─► rounds 3–7: sigma(3) sides converge; each hint turns "verified" as its loop finishes
```

### 3.7 Sign-off

```
 command ─► freeze the revision (design + contract hashes)
         ─► the standard check (§2), from the cache, with a random audit: re-simulate a few % of hits, compare bits
         ─► statistics (M7): board yield. Each sampled board is checked at every range point and every spec
               zero-failure Clopper–Pearson: yield ≥ 99.865% at 95% confidence → n = 2,218 boards
               × 7 range points (4 corners + 3 interior temperatures) = 15,526 runs
               (the CE amp: ~0.1 s of operating points + ~0.9 s of dense AC, single thread)
         ─► second backend cross-check (M4, ngspice)
         ─► signoff.lock: verdicts, methods, seeds, engine and backend versions (language_specs.md §11.3)
```

n is the smallest n with pⁿ ≤ 1 − confidence, where p = 0.99865 is the yield to prove (the zero-failure Clopper–Pearson bound), computed in `triggers.py`. Importance sampling (engine.md §4.4) reduces n. The red-team measured 2,000 importance-sampling runs matching the precision of 20,000 plain Monte Carlo runs.

### 3.8 Opening a project cold

```
 open ─► parse, elaborate, lower (µs–ms) ─► verdict keys = hash(design, contract, engine, backend versions)
      ├─ in .spicy/cache ─► show the verdicts at once as verified (they are exact for these inputs)
      ├─ not in the cache, but signoff.lock matches the design hash ─► show the signed-off verdicts
      └─ neither ─► T1 cold check (§2), with the run store serving whatever it still holds
```

### 3.9 "Simulate this counterexample"

```
 click ─► the counterexample is a full KnobPoint (irrelevant knobs shown as "any")
       ─► op: already in the cache (the loop simulated it) ─► node voltages at once, 0 runs
       ─► full view with AC plots: the bias loop ran only the op there ─► 1 AC run (op reused)
       ─► transient specs (later): the loop keeps measured values only ─► 1 run with waveforms saved
```

Keep the full `RunResult` for points the user is likely to open: worst points, counterexamples, safety-net corners. For the CE amp one result is ≈ 4 KB (5 unknowns × 50 frequencies, complex). For a 50-knob circuit it's ≈ 26 KB (32 unknowns), so keeping all ~3,500 results of a check would take ~90 MB. Keep measured values for every run and full results for the pinned ones.

### 3.10 Cost per trigger (CE amp, L0, both confidences)

| Trigger | New op | New AC | Hits | Rounds | Notes |
|---|---|---|---|---|---|
| Cold check | 167 | 116 | 4 | 7 | Brute force: 256 + 256, worst_case only |
| `c_in` → 2.2 µF | 45 | 116 | 78 | 7 | bias: 0 runs (92 hits) |
| `r1` → 51k | 167 | 116 | 4 | 7 | Everything reads R1 |
| Add `c_load` (9 knobs) | 53 | 129 | 78 | 7 | 193 op if the op key hashes capacitors |
| New spec `top_room` | 32 | 0 | 25 | 7 | |
| Changed bound | 0 | 0 | — | 0 | Needs converged extremes |
| `vcc` ±5% → ±10% | 156 | 104 | 16 | 7 | Hits only with physical keys |
| Confidence change | 0 (103) | 0 (63) | — | 0 (7) | Parentheses: if only one confidence is computed |
| Counterexample | 0 | 0–1 | 1 | 0–1 | |
| Cold open with a store | 0 | 0 | all | 0 | |
| Sign-off statistics | ~15.5k | ~15.5k | — | batches | Zero-failure 99.865% at 95% |

---

## 4. Run planning and efficiency

### 4.1 One run serves many measures

- **Analyses merge per point.** An AC run needs its operating point anyway, so {Op} ∪ {Ac} at one point is one job. The planner asks for the union of every spec's needs at that point: one AC sweep serves `gain` (1 kHz) and `bass` (the −3 dB search), and its operating point serves `bias`. The AC frequency list is the union too: 1 kHz plus the log sweep for f_low.
- **Every run is evidence for every measure.** A simulated point is reachable, so its values are inner bounds for *all* measures, not only for the spec that asked. Keeping each measure's running min and max over all runs is free, and it is a free guard: if any run beats a loop's answer, that loop isn't done. On the CE amp no run beat any loop: the min and max over all 283 cached solves equal the loop answers.
- **The shared round is shared.** The safety net, the nominal run and the nominal nudges serve every spec. Checking the three specs separately costs 234 runs; together, 194.

### 4.2 The run cache and its key

**What the key hashes.** The key covers what the simulator reads for that analysis:
- the circuit's structure key;
- the resolved `Params` fields the analysis reads;
- `Conditions` (temperature);
- the `Analysis` itself;
- the solver configuration;
- the backend's name and version.

Two choices matter:

| Choice | Option | CE amp evidence | Recommendation |
|---|---|---|---|
| Physical values or knob positions ε | ε + a revision hash | `vcc` ±10%: 0 hits across revisions | **Physical values.** A knob's ε means something different after a range edit; the simulator's input doesn't |
| | Physical values | `vcc` ±10%: 16 hits | |
| Per-analysis projection | The op key includes capacitor values | `c_in` edit: 180 new op; within one cold check: 180 op | **Project.** The op key skips capacitor and inductor values and AC source magnitudes |
| | Projected | `c_in` edit: 45 new op, bias fully cached; cold check: 167 op | |

**It must be pure.** A cached result has to be bit-identical to recomputing it. So a run must be a function of its key alone:
- no warm start from whatever the thread solved last;
- a warm start only from a seed that's part of the key (the nominal solution of the same DC projection), or a cold start.

`pipeline.md` §7 already asks for warm starts "from the nominal solution, never from whatever a thread solved last"; this sharpens it into a cache rule.

**Scope.** For the MVP, a `HashMap` that lives for one check is enough: it dedups within a round (8 duplicates) and across confidences (4 hits). An ε-key is even enough there, because the revision is fixed. Physical keys matter once the cache outlives a revision (the editor, `spicy watch`). A disk store follows rust-analyzer's bounded caches (LRU capacities in `crates/base-db/src/lib.rs:68-70`), with pinned entries for the results users open.

### 4.3 Rounds, not runs: batch the nudges with the worst point

At L0 the loop always re-linearizes at the corner it just predicted, even when that corner turns out to be the final answer. So the corner and its inward nudges can go in **the same round**:

| | Runs | Rounds (worst_case) | Rounds (both) |
|---|---|---|---|
| Corner first, then its nudges | 194 | 5 | 11 |
| Corner + nudges together | 194 | **3** | **7** |

The run count is identical, and the rounds drop by 40%. The only case where batching wastes runs is a loop that stops at the first FAIL, and §3.4(b) argues against stopping there. The rounds are the critical path; the runs are the work.

### 4.4 Parallelism

Rounds in the cold check hold 18, 76, 42, 33, 21, 9 and 3 requests. Runs within a round are independent.

- **One core:** ≈ 1.1 ms of operating points (167 × 6.6 µs) + 6.7 ms of dense AC (116 × 50 × 1.163 µs) ≈ 8 ms.
- **20 cores** (this machine): 14 run-slots in sequence × ≈ 65 µs ≈ 0.9 ms.

Parallel runs need per-thread `Params`, `Derived` and `Workspace` over a shared `Circuit` and `Plan` (`pipeline.md` §8). KLU's `factor` and `refactor` take the symbolic analysis as `&mut` (`crates/spicy_simulate/src/solver/klu/factor.rs:190`, `refactor.rs:21-26`), hence `pipeline.md` decision 8: clone it per thread.

**Process-level parallelism** (for external backends): our CLI takes a median **3.0 ms per process** for the CE amp's `.OP` (50 runs, measured here), against 6.6 µs in-process, about 450× more. That's the per-run price of the subprocess transport roadmap §3 plans for ngspice. The shared library avoids it (§6.3).

### 4.5 What the simulator reuses between runs, and when `pipeline.md` steps 2 and 4 pay

Within one run, the Newton loop already factors once and refactors after that (`crates/spicy_simulate/src/trans.rs:76-83`). Between runs, everything is rebuilt: `Layout`, `Devices`, the pattern, `analyze` (`dc.rs:74-91`). Measured per operating-point run (`research/circuit_redteam.md` §2):

| Per OP run | CE amp (dim 5) | ×10 stages (dim 32, ≈ 50 knobs) | ×100 (dim 302) |
|---|---|---|---|
| Today | 6.6 µs | 40.9 µs | 378 µs |
| With a shared `Plan` (steps 2/4: minus the measured topology total) | 5.1 µs | 31.2 µs | 276 µs |
| + warm start and refactor-only (step 4's per-thread workspace) | 0.45 µs | 2.9 µs | 27 µs |
| AC, one frequency, dense (today; no BJT stamps yet) | 1.16 µs | 19.7 µs | 1,814 µs |

**Refactor across runs.** KLU's refactor keeps the first factorization's pivot order (`refactor.rs:48-51`: "`numeric.pnum` … is the final pivot permutation"). The nominal run's factorization can therefore serve every run's first Newton iteration. The catch: pivots chosen at nominal can be poor at an extreme corner, and our KLU port has no pivot-growth or condition check yet (a `TODO` at `trans.rs:81`: "consider retrying full factorization on refactor failure"). Cross-run refactoring needs that fallback first.

**AC.** The AC matrix G + jωC has the same pattern at every frequency and in every run. That's one symbolic analysis per `Plan`, and a numeric refactor per frequency. Today's AC path is dense and allocates per frequency (`pipeline.md` §11).

**When it pays** (one full check, both confidences, single thread; run counts from §4.8; `cost_model.py`):

| Circuit | Solves (op / AC) | Ops today | Saved by steps 2/4 | Saved with warm start too | AC dense | AC sparse (est.) |
|---|---|---|---|---|---|---|
| CE amp | 167 / 116 | 1.1 ms | 0.25 ms | 1.0 ms | 6.7 ms | 3.3 ms |
| 50 knobs (×10 proxy) | 3,511 / 1,756 | 144 ms | 34 ms | 134 ms | 1,732 ms | 367 ms |
| ×100 stages | 3,511 / 1,756 | 1,328 ms | 358 ms | 1,232 ms | 159 s | 3.6 s |
| Sign-off, CE amp | 15.5k / 15.5k | 0.10 s | | 0.09 s | 0.9 s | |
| Sign-off, ×10 | 15.5k / 15.5k | 0.64 s | | 0.60 s | 15 s | |

"AC sparse (est.)" assumes one complex sparse refactor + solve costs 3× a real one (the measured 0.19 / 1.39 / 13.5 µs). That factor is an assumption, not a measurement.

The order this gives:
1. **Sparse AC with BJT stamps (M2d)** matters first, for any AC spec. The MVP needs it for correctness anyway.
2. **Steps 2/4 plus warm start** pay once interactive checks reach ≈ 50 knobs (144 → 10 ms for the DC side), or at sign-off. **Not for the MVP:** they would save 1.3 ms out of 8 on the CE amp.
3. **L1 sensitivities (M8)** cut the run count itself about 4× (§4.8).

### 4.6 Which knobs can reach which spec

- **By analysis** (exact, free). An operating point reads no capacitor value (open circuit), no inductor value (a short), and no AC source magnitude. Temperature reaches everything that has temperature models. A transient reads everything. On the CE amp this removes `c_in` from bias: 7 nudges instead of 8 per bias linearization, and the op-key projection of §4.2.
- **By connectivity.** A knob can only reach a probe if its device is in the probe's connected piece of the circuit. Nodes held by an ideal source (the default bench's supplies) and ground cut the graph, because an ideal source's voltage doesn't depend on what it drives. In the CE amp everything is connected, so this pruning buys nothing. On a board with several blocks behind ideal supply assumptions, it partitions the knobs by block.
- **By numbers** (screening). The nominal nudges measure each slope. A knob whose slope is tiny compared with the spec's margin (gain vs `c_in`: an elasticity of 4.0e-4) is still simulated, because structure says it *can* matter. But it tells T1 what to run first: order specs by |predicted change| / margin.

engine.md §6.2 already allows "known from the structure: a spec can only depend on knobs in its connected region". The analysis rule above is the part that's exact and free on day one.

### 4.7 Instant predictions: which form to store

engine.md §8 and walkthrough §2 promise "re-evaluation of stored affine forms for the changed values" on every edit. §3.1 and §3.2 show the catch: a value edit usually moves a knob far outside the box its form was drawn over (`c_in` +6 ε, `r1` +8.5 ε). A form in ε extrapolates badly (f_L −4.0 Hz).

| Store | Good for | c_in → 2.2 µF (true 9.127) | r1 → 51k, VC (true 6.112) |
|---|---|---|---|
| d measure / d ε (the form as defined today) | Knob moves inside the box: the loop itself | −4.011 Hz | 6.156 V |
| d measure / d ln(value) | Multiplicative knobs (R, C, β) | 4.251 Hz | — |
| d ln(measure) / d ln(value) | Positive measures near a power law (f_L, gain) | **9.128 Hz** | 6.167 V |

**Rule.**
- Store each form's raw derivatives together with its linearization point and the knob scales, so any of these can be derived.
- Predict value edits of multiplicative knobs in log–log, and of additive knobs (T, VCC) linearly.
- Always label the result **predicted** until T1 verifies it.
- Range edits are the easy case: rescaling a coefficient is exact for the line (`vcc` ±10%: 6.7258 predicted vs 6.7673 verified; the rest of the gap is curvature).

### 4.8 Cost at scale: CE amp vs 30 and 50 knobs

L0 run counts use a formula calibrated on the CE amp (it reproduces 202 requests for the CE amp): shared round (2^R + scan + 1 + N), plus per side 1.2 worst_case iterations × (1 + relevant knobs), plus sigma(3) (initial linearization + 3 iterations × (1 + relevant statistical knobs) + a joint check of 2^R − 1 range corners). L1 counts one run per linearization (plus an adjoint solve, not counted).

| Circuit | L0 worst_case | L0 sigma(3) | L0 both | L1 both | Brute force (corners) |
|---|---|---|---|---|---|
| CE amp: 8 knobs, R = 2, 5 sides | 69 (simulated) | 142 (simulated) | 194 (simulated) | ≈ 48 | 256 |
| 30 knobs: R = 5, 15 sides, 20 relevant | ≈ 445 | ≈ 1,537 | ≈ 1,915 | ≈ 580 | 1.1 × 10⁹ |
| 50 knobs: R = 5, 20 sides, 30 relevant | ≈ 831 | ≈ 2,767 | ≈ 3,511 | ≈ 761 | 1.1 × 10¹⁵ |

At R = 5, the sigma(3) joint check (31 runs per side) dominates L1: 465 of 580 runs at 30 knobs. Enumerating range corners at the final statistical point stops being cheap past about R = 4 (open question 3).

---

## 5. Scheduling

### 5.1 Tiers

| Tier | Trigger | Work | Budget (our choice) | Cancellable |
|---|---|---|---|---|
| **T0: every edit** | A text edit or gesture | Parse, elaborate, type and net checks, lower, SpecPlan; datasheet arithmetic (engine.md §4.1); predictions from stored forms | One frame (≈ 16 ms) | No: synchronous |
| **T1: background** | After T0 | Safety net + loops for DC/AC specs, both confidences, standard path, from the cache | ms to s | Yes |
| **T2: on demand** | A click or command | Transient/PSS specs, deep searches (multi-start, full enumeration), counterexample waveforms | s to min | Yes |
| **T3: sign-off** | A command | Statistics, a second backend, the cache audit, `signoff.lock` | min | Yes, resumable |

**Priority within T1:**
1. visible specs first;
2. then by |predicted change| / margin (the specs most likely to flip);
3. within a round, FAIL-seeking runs (predicted worst corners) before nudges;
4. the declared confidence before the other one.

History (the last revision's worst points) decides only **order**: simulate the old counterexample first to show "probably still FAIL" early. It never decides a verdict. That's the same line Solido draws: its model is used "to merely order the samples, rather than … to make a decision" (US9483602B2, via `engine_method_redteam.md` §3.2).

### 5.2 Cancelling stale work

- **A generation counter,** bumped on every accepted edit, like salsa's revision.
- Every queued `RunRequest` carries its generation. Requests from older generations are dropped when they are dequeued.
- A run in flight finishes. DC and AC runs take µs to ms; transient runs (T2) check the counter at each accepted time step. **Its result is still stored:** a content-addressed result is a true fact about its inputs, so a cancelled job never corrupts anything, and an **undo is instant** because every run of the previous revision is still cached.
- The loops of the old generation are dropped. The new generation's loops replay from the cache.

rust-analyzer's precedents, and where we depart:

| rust-analyzer | Where | Ours |
|---|---|---|
| Applying a change bumps salsa's revision counter; running queries that notice it unwind with `Cancelled` | `docs/book/src/contributing/architecture.md:377-388`; `crates/ide/src/lib.rs:198-201, 238-242` | No unwinding. Our unit of work is short (a run), so we check the generation between runs |
| Diagnostics carry a generation; older ones are cleared | `crates/rust-analyzer/src/main_loop.rs:668-670, 1256-1261` | Verdicts carry the generation they were computed at |
| `cargo check` restarts: kill the old process, wait 50 ms for more restarts, keep the last one's parameters | `crates/rust-analyzer/src/flycheck.rs:578-617` | Debounce keystroke edits ≈ 50–100 ms. Schematic gestures are discrete and need none |
| Diagnostics run on latency-sensitive threads, using at most a quarter of the pool | `main_loop.rs:690-709` | Keep some cores free for T0 when T1 floods the pool |

### 5.3 Progress

Each `LoopState` reports its phase, round, runs used and runs planned: "bass · 3σ loop · round 3 · 21 runs". The check reports runs queued and done per generation. Every verdict ends with its cost and the brute-force reference, as the red-team recommends ("report our simulation count *and* the brute-force reference", `engine_method_redteam.md` §3.2 item 8): "bias.max FAIL · worst-point loop, 8 runs + 17 shared, L0 · brute force 256".

### 5.4 Determinism: the same inputs give the same verdicts

A verdict must be a function of (design, contract, engine options, engine and backend versions) and nothing else:
- not of thread count;
- not of completion order;
- not of cache state;
- not of **edit history**.

| Source of drift | Rule |
|---|---|
| Completion order | The driver resumes loops only when the round's batch is complete, and hands results back in request order |
| Thread state | Each run is single-threaded and pure (§4.2). Newton starts cold, or from a seed that's part of the key |
| Cache state | A hit must equal a recomputation bit for bit. Sign-off audits a sample |
| Edit history | **Replay the loop's standard path.** Don't start it from the previous revision's worst points |

**The warm-start measurement behind the last rule.** Re-checks seeded from the previous revision's worst points, against the standard path with the same cache:

| Re-check | Standard path + cache | Warm-started loop + cache |
|---|---|---|
| `c_in` → 2.2 µF | 45 op + 116 AC, 7 rounds | 76 op + 108 AC, 5 rounds (**+14%** solves) |
| `r1` → 51k | 167 op + 116 AC, 7 rounds | 154 op + 108 AC, 5 rounds (−7%) |
| Values | Identical to a from-scratch run | worst_case identical (same corners); sigma(3) off by up to **2.8e-5 V** (bias.min, whose inputs didn't even change) |

A warm start helps only when everything changed, and even then by 7% of runs and 2 of 7 rounds. It hurts when little changed, because it abandons the cached path. And it makes a verdict depend on the edit history. The content-addressed cache already provides the reuse that "warm-start the loop from the last worst points" (red-team §3.2 item 7) was after.

### 5.5 Seeds for sampling (T3)

- **Key sampled values by knob path, sample index and a sign-off seed:** u = hash(seed, `amp.r1.value`, i).
  - An edit to r1's *value* then keeps r1's random draws; only their mapping to ohms changes.
  - The sampled boards change between revisions only where the design did (common random numbers).
  - The run cache stays valid for knobs the edit didn't touch.
- **Precedent:**
  - VACASK defaults to seed 0 and identifies each random draw by "instance/model/subcircuit-instance name, the parameter name, and the position of the call", stable across re-elaborations (`vacask/docs/cmd-analysis-mc.md:18, 61`).
  - Xyce draws a seed from `std::random_device` unless one is given (`xyce/src/AnalysisPKG/N_ANP_UQSupport.C:1243-1273`).
  - We follow VACASK and depart from Xyce's default.
- The seed goes into `signoff.lock`.

---

## 6. Grounding: what others do

### 6.1 Query-based incremental computation: salsa in rust-analyzer

- **Inputs, revisions, backdating.** Setting an input bumps salsa's single revision. A derived query checks whether its dependencies changed since it last ran, and re-executes if they did. If the new value equals the old, salsa can "backdate the result, meaning that, even though the inputs changed, the output didn't", which stops the invalidation from spreading ([salsa book, "The red-green algorithm"](https://salsa-rs.github.io/salsa/reference/algorithm.html)).
- **Durability.**
  - Library files are set with `HIGH` durability, their source roots `MEDIUM`, workspace files `LOW` (`crates/base-db/src/change.rs:93-99`).
  - After an edit, validation can skip queries that depend only on high-durability inputs.
  - Our analogue: the prelude and part records are high; the design and contract text are low.
- **Invalidation barriers.** The `ItemTree` exists so that "when typing inside an item body, the `ItemTree` of the modified file is typically unaffected" (`crates/hir-def/src/item_tree.rs:12-14`). Our barrier is the span-free `Design` (§1.4).
- **Snapshots and cancellation:** §5.2.

**What we take.** Early cutoff by comparing stage outputs; durability as a concept; snapshots that are cancelled rather than mutated.

**What we don't take yet: salsa itself.**
- Re-parsing and re-elaborating a 1 KB design is microsecond work, so equality checks at 4–5 stage boundaries give the same cutoffs.
- The expensive, fine-grained part is the runs, and the content-addressed run cache already handles those.
- Salsa pays when projects have many files and blocks. `language_editor_mapping.md` open question 11 asks where that line is; it can't be measured until elaboration exists.

### 6.2 *Build Systems à la Carte* (Mokhov, Mitchell, Peyton Jones, ICFP 2018)

The paper splits a build system into a **scheduler** (which task runs next) and a **rebuilder** (whether a task needs to run) (§4.1–4.2, Table 2). Our flows map onto it cleanly:

| Paper | Our engine |
|---|---|
| **Minimality:** run a task "at most once per build and only if [it] transitively depend[s] on inputs that changed" (Def. 2.1) | A run executes at most once per `RunKey`; unaffected specs cost 0 runs (§3.1) |
| **Early cutoff** (§2.3, Fig. 3) | `Design` equal after a comment edit; the op projection equal after a capacitor edit |
| **Static vs dynamic dependencies:** applicative vs monadic tasks (§3.4–3.5) | The front-end is static. The loop is **monadic**: which runs it needs depends on earlier results (slopes → worst point) |
| **Suspending scheduler:** build dependencies when requested, suspending the task (§4.1.3) | The loops are coroutines that yield `KnobPoint`s; the driver resumes them with results |
| **Verifying traces** (§4.2.2): store hashes, recheck | The front-end stages (compare outputs) |
| **Constructive traces** (§4.2.3): store the values too, shareable ("cloud") | The run cache: results keyed by their inputs' hash, shareable across revisions, machines, users |
| **Deep constructive traces** need deterministic tasks (§4.2.4) | Our keys hash terminal inputs (`Params`), so runs must be pure (§4.2, §5.4) |
| **Cloud Shake** = suspending + constructive traces, "minimal, and supports both early cutoff and monadic dependencies" (§4.4, §5.4); unfilled by any system in Table 2 | Exactly our combination |
| Parallel suspending: start static dependencies in parallel while dynamic ones resolve (§6.2) | The shared round (all static) goes first; loop rounds are dynamic |
| Iterative computations as a bounded series of steps (§6.6) | The loop needs a round budget and cycle detection (engine.md §5.2) |

### 6.3 How SPICE simulators support many runs

| Simulator | Mechanism | What it reuses between runs | Source |
|---|---|---|---|
| **ngspice** | No `.step` card: we found none in the input parser. Loops are written in the control language (`foreach`, `repeat`, `dowhile`) with `alter`/`altermod` on device and model parameters | Nothing structural: every `run` with reset does `CKTunsetup` + `CKTsetup` + `CKTtemp` | `src/frontend/commands.c:424-435, 633-641`; `src/spicelib/analysis/cktdojob.c:160-167` |
| | `alterparam` edits `.param`s in a stored deck and "has to be" followed by `mc_source`, which re-sources the deck: a full re-parse | Nothing | `src/frontend/inp.c:1817-1821` |
| | Shared library: load a circuit from memory (`ngSpice_Circ`), send commands (`ngSpice_Command`), read vectors (`ngGet_Vec_Info`), run in a background thread (`ngSpice_running`). Each loaded copy has its own `ident`, so parallel runs need separate library copies or processes | No process start per run | `src/include/ngspice/sharedspice.h:384-490`, `:391` |
| **Xyce** | `.STEP` is a plain loop: update the swept parameters, run the child analysis | Topology and matrix structure are built once; `setParam` + `processParams` per step | `src/AnalysisPKG/N_ANP_Step.C:205-238`; `data_model_survey.md` item 9 (`N_ANP_SweepParam.C:530-541`, `N_LOA_CktLoader.C:113-126`) |
| | `.SAMPLING`: MC or LHS with a `SEED` option; random seed by default | Same | `N_ANP_Sampling.C:468-489, 661-668`; `N_ANP_UQSupport.C:1243-1273` |
| **Gnucap** | A `sweep` command replays a group of commands for each value | If the structure is unchanged, only `precalc_first()` re-runs; partial LU driven by per-node `_changed` flags | `apps/c_sweep.cc:22`; `lib/u_sim_data.cc:264-266`; `include/m_matrix.h:118-137, 168` |
| **VACASK** | `sweep` before an `analysis`, nested; `mc` loops with LHS by default | "By default (`continuation=1`) the simulator uses the solution from the previous sweep point as the starting point"; setup re-runs only for flagged instances | `docs/cmd-sweep.md:124`; `docs/cmd-analysis-mc.md:18-25`; `circuit.md` §4.1 (`cirparams.cpp:255-512`) |

**What we follow and where we depart:**
- **Follow:** Xyce, Gnucap and VACASK build the structure once and re-run only value-dependent setup. That's `circuit.md`'s `Plan`, and `pipeline.md` steps 2/4.
- **Depart:** VACASK and Xyce continue from the previous sweep point. That's natural for a sequential sweep, but our runs are parallel and cached. We start from the nominal solution (or cold) so a result doesn't depend on which run a thread did before (§4.2).
- **Depart:** none of these simulators caches results across invocations. The run cache is the engine's job, because the engine knows which inputs a run reads.

### 6.4 How commercial analog tools organize runs

We couldn't reach vendor documentation in this session: cadence.com returned 403, the Wayback Machine was blocked, and the web-search budget was spent. What follows comes from `engine_method_redteam.md` §3, whose sources were verified by that agent (its tags: [V] primary, [S] secondary).

| Tool | How runs are organized |
|---|---|
| **Cadence ADE Explorer / Assembler + Variation Option** [V] | Corner sweeps; Monte Carlo with auto-stop on target yield; yield verification by reordering samples, with a Clopper–Pearson stop rule; WCD "under 100 simulations for each spec … small number of specs/parameters"; K-Sigma corners: ≤ 200 MC samples, then a model, a corner, and a line search ("up to 11 extra simulations, usually 1") |
| **Spectre FMC** [V] | A response surface orders pre-drawn MC samples worst-first and simulates in that order; its stopping rules are "kept as a trade secret" |
| **Siemens Solido** [V]/[S] | Fast PVT (a Gaussian-process model with active learning over PVT corners); PVTMC joint PVT × MC, because "worst case PVT at nominal may not be the worst case at the target sigma"; HSMC, where the model only orders samples; "Additive Learning … retains and reuses results and models from prior simulations" |
| **MunEDA WiCkeD** (Cadence since 2024) [V]/[S] | WCD by name, "simulator-true worst case"; the vendor still advises brute-force MC for strong non-linearity and low sigma |
| **LTspice** [V] | `.step` over a `wc()` idiom: exhaustive 2^N + 1 vertices |

**Unknown:**
- whether ADE or Solido cache results across design edits by content, or re-run a whole test;
- how they cancel stale jobs;
- how they keep runs deterministic under distributed execution.

The public claims ("additive learning", reuse "from prior simulations") suggest model reuse, not exact result reuse. Our design commits to exact reuse (§4.2) and to history used only for ordering (§5.1).

### 6.5 Follow or depart: summary

| Decision | Precedent | Us |
|---|---|---|
| Build structure once, re-run value setup per run | Xyce, Gnucap, VACASK (§6.3) | Follow (`Plan`; deferred to steps 2/4) |
| Early cutoff between stages | salsa, Shake (§6.1, §6.2) | Follow, with equality checks rather than salsa for now |
| Content-addressed, shareable result cache | Bazel, CloudBuild, Nix (à la carte Table 2) | Follow, at the run level; no simulator does this |
| Continue from the previous sweep point | VACASK, Xyce | **Depart:** start from nominal or cold (purity, parallelism) |
| Warm-start the next design iteration from the last results | Solido "Additive Learning" (red-team §3.2) | **Depart:** history orders runs; the verdict comes from the standard path (§5.4) |
| Random seed by default | Xyce | **Depart:** a seed per knob path, as VACASK does (§5.5) |
| Cancel by unwinding | rust-analyzer | **Depart:** check between short runs, keep completed results (§5.2) |
| Opaque stopping rules | Spectre FMC | **Depart:** report the runs, the rounds and why the loop stopped (§5.3) |

---

## 7. Recommendations for the MVP

The MVP has one consumer, `spicy check`: a one-shot run with no editor. Each item is decidable and gives its reason.

1. **One flow: the cold check of §2.** Pipeline: parse → elaborate → lower once → `SpecPlan` → shared round → loops (both confidences) → verdicts → table.
   - *Reason:* it is the only trigger without an editor. Every other flow reuses its pieces.
2. **Lower once and apply the `Binding` per run** (`circuit.md` §5, `pipeline.md` §6b). Don't use roadmap §2.5's shortcut of lowering a fresh `Circuit` per run.
   - *Reason:* the binding is needed anyway (knob → field), it costs no more, and it keeps `Circuit` shared, which steps 2/4 and parallel runs need later.
3. **Write each loop as a state machine that yields batches of `KnobPoint`s. A driver merges every loop's requests into one round, waits for the whole round, and resumes the loops in request order.**
   - *Reason:* this is the suspending scheduler of §6.2. It makes parallelism a backend detail, and it makes verdicts independent of completion order (§5.4).
4. **Send the predicted worst point and its re-linearization nudges in the same round.**
   - *Reason:* no extra runs at L0, and 7 rounds instead of 11 (§4.3).
5. **Converge each side's extreme even after a FAIL is certain,** and store the extreme.
   - *Reason:* the counterexample is the worst board, not the first failing one found. A later bound edit costs 0 runs (§3.4b). The FAIL itself can be printed as soon as it is certain.
6. **Compute both confidences for every spec; the spec's attribute picks the headline.**
   - *Reason:* engine.md §3.6 shows both when they disagree. It costs +37% runs on the CE amp (194 vs 142), and confidence edits become free (§3.5).
7. **An in-memory run cache for one check.** Key it by (analysis, ε-point with the irrelevant knobs dropped: capacitors for the op). Merge Op into Ac at the same point.
   - *Reason:* it dedups 8 requests per check, and 13 more operating points through the projection (180 → 167). It is also the seam for the later persistent, physically keyed store. Physical keys aren't needed while the revision is fixed.
8. **`Backend::run(&Prepared, &[RunRequest]) -> Vec<Result<RunResult, SimError>>`, batch in, results in order.** Serial inside for now.
   - *Reason:* engine.md §6.1's `run(points, analyses)` made concrete. Parallelism (rayon over the batch, per-thread workspaces) then changes nothing in the engine. The CE amp takes ≈ 8 ms serially, so it isn't needed yet (§4.4).
9. **Cold-start Newton in every run for the MVP.**
   - *Reason:* it makes every run a pure function of its inputs (§4.2). Warm starts save ≈ 1 ms per check on the CE amp (§4.5).
10. **Keep the full `RunResult` only for pinned points** (worst points, counterexamples, safety-net corners); keep `Measured` for all.
    - *Reason:* `spicy check --show <spec>` can print the counterexample's node voltages without re-running (§3.9), and memory stays bounded when circuits grow.
11. **Every verdict prints its cost:** runs (op / AC), rounds, level, and the brute-force reference.
    - *Reason:* engine.md §6.2 ("says what it cost") and the red-team (§5.3). It also makes the M3 tests readable.
12. **The M3b answer key goes through the same `Backend::run` and cache**, as a single 256-point batch.
    - *Reason:* the answer key and the loop then exercise one code path, and a test that runs both reuses the shared corners.
13. **Test determinism explicitly:** the same verdicts, bit for bit, with the cache on and off, and (once parallel) with 1 and N threads.
    - *Reason:* §5.4 depends on it, and it is cheap to pin now.

**What can wait, and until when:**

| Deferred | Until | Why it can wait |
|---|---|---|
| Physically keyed, projected, persistent run store (`.spicy/cache`) | `spicy watch` / the editor | Needed only across revisions (§3.1–3.4) |
| Salsa or any query framework | Projects with many files | Whole-file re-elaboration is microseconds; the run cache carries the cost (§6.1) |
| Generations, cancellation, debounce, tiers T0–T3 | The editor | A one-shot CLI has nothing to cancel |
| Instant predictions (log-form re-evaluation) | The editor | But store each form's raw derivatives, linearization point and knob scales now (the `AffineForm` row of §1.2), so nothing is lost |
| `pipeline.md` steps 2/4, warm start, cross-run refactor (with a pivot fallback) | ≈ 50 knobs interactive, or sign-off (§4.5) | 1.3 ms of 8 on the CE amp |
| Sparse AC | **Not deferrable**: M2d must stamp the BJT anyway; do it sparse | Dense AC is ≈ 86% of the CE amp's check time, and 1.7 s at 50 knobs |
| Connectivity pruning | Multi-block designs | Buys nothing on the CE amp (§4.6) |
| Sampling seeds, sign-off, audits | M7, M4 | |

---

## 8. Open questions

1. **Numerical noise and key purity.** With the real simulator, do nudged slopes flip sign near zero (the gain-vs-`c_in` elasticity is 4e-4)? That would add rounds, or make the loop take an unnecessary corner. M2e's two-step-size test should run on the CE amp's actual nudges.
2. **The op-key projection must never be wrong.** Dropping capacitor values from the operating-point key is sound only while the DC solver really ignores them. Capacitor-only nodes, `.ic`/UIC, and a future "DC with gmin to floating nodes" option all need care. Should the backend declare, per analysis, which fields it reads (generated from the same field tables as the `Binding`), instead of the engine assuming it?
3. **The joint range × statistical check past R = 4.** Enumerating 2^R − 1 range corners at each final statistical point costs 31 runs per side at R = 5, most of L1's budget. Use the range-slope form to pick candidates (Solido-style joint search), or only the corners the form can't rule out?
4. **When is "predicted" good enough to show as a verdict?** For `c_in` the log–log prediction was within 0.02%. For `r1` it was 0.05 V off, on a 6.5 V limit. Is a rule like "a verdict flips only after T1 confirms it" enough for the editor?
5. **Do FAILs converge in a bounded number of rounds?** On the CE amp every loop finished in ≤ 4 iterations. The engine needs a round budget and a stated fallback (engine.md §5.2 names enumeration). What budget, and how is it reported?
6. **How much of the result store to share.** Constructive traces can be shared across machines, as a team cache. Is that wanted, and what goes into the key for vendor models (file hashes)?
7. **Commercial practice we couldn't verify:** whether ADE or Solido reuse results across design edits exactly, and how they order and cancel jobs. Worth another pass when vendor docs are reachable.

---

## Appendix: reproducing the numbers

```
cd /root/.claude/jobs/443154a8/tmp/engine_research/flows
python3 flows_model.py     # nominal values, |H(1k)| vs mid-band gain
python3 cold_check.py      # §2: answer key, loop histories, costs per confidence and per spec
python3 triggers.py        # §3: every trigger, predictions vs verified, cache hits, warm start
python3 cost_model.py      # §4.5, §4.8: scaling, time estimates from circuit_redteam.md §2
```

- `engine_sim.py` holds the flow model:
  - `Engine.run_batch` is the round;
  - `op_key` / `ac_key` are the cache keys (physical by default, `key_mode="eps"` for comparison);
  - `wc_side` / `s3_side` are the loops;
  - `drive` is the suspending driver.
- The nudge step is 0.001 ε.
- sigma(3) uses σ = tol/3 per statistical knob and stops when successive values agree to 1e-4 relative.
- The safety net is the 4 range corners plus 5 temperature points at nominal VCC.
