# Engine Plan: the Bounds Engine MVP on ngspice

> 2026-09-27 · Synthesis of round 3 (five position papers), revised after an independent review of the first draft (its re-runs are in `…/engine2/review/`). **Status:** accepted 2026-09-27: the user approved every recommendation in §10 (D-A to D-F, and round 2's D1–D10 as answered there).
> Reads with: `research/engine_position_{minimal,soundness,agent,scale,alternatives}.md` (round 3), `research/engine_synthesis.md` (round 2: C1–C12, D1–D10) and its six reports, `engine.md` (v3), `walkthrough.md`, `model.md` (M1d), `circuit.md`, `pipeline.md`.
> **Where the numbers come from.** A cited number names its paper and section ("minimal §4.5"). Numbers marked **[S]** were recomputed for this plan on ngspice-42 by the stdlib-Python scripts in `/root/.claude/jobs/443154a8/tmp/engine2/synthesis/` (called `$S`; Appendix F lists them). They are scratch and stay out of the repo, as decided.

---

## 0. Summary

**The plan in five sentences.**
1. `spicy check` runs a **fixed pipeline** on ngspice: simulate every corner of the knob box; check each side's worst corner for a maximum inside the box; search the 3σ point per side; re-run the decisive points at tighter tolerances; read the verdicts off one table.
2. On the CE amp that is **579 runs and 0.4 s** of simulation on libngspice (§2.8). Every `worst_case` value equals the brute-force answer key exactly. Every `sigma(3)` value is within 4.1e-5 of an independent σ key **[S]**.
3. The worst-point loop stays the engine's scalable core, but it is **not in the MVP**. Up to 12 knobs per spec side, enumeration is exact at the corners and needs none of the loop's guards (minimal §4.2, alternatives §3.9). The loop comes as its own step after M3, accepted only when it agrees with enumeration on every regression circuit.
4. The MVP's most informative verdict, **bias at worst case, depends on an unwritten model default**. With the transistor's β tempco (XTB = 1.5), bias max is 6.5595 V: FAIL. Without it (XTB = 0) it is 6.4572 V: PASS by 43 mV **[S]**. The headline (`sigma(3)`, the specs' confidence) is PASS either way (6.2813 V, 6.1839 V). Decision D-A (§10) settles it: the default model has XTB = 1.5, written into the export, so the report shows the worst-case FAIL beside that PASS.
5. On an adversarial suite of 7 circuits (10 cases, 24 spec sides), the planned engine gives **0 false PASS and 0 wrong FAIL** at both confidences, scored automatically against the keys **[S]** (§8.5).

**The three questions, answered plainly.**

| Question | Answer |
|---|---|
| When does the engine run? | In the MVP, only when asked: `spicy check file.spl`, typed by you or by an agent in a terminal. Every check is a full cold run (0.4 s here). There is no background run and no "instant" estimate. In the editor era: on save or idle, still a full re-check (§1.1) |
| Given the AST, what else does it need? | It never reads the AST. It reads the elaborated **`FlatDesign` + `KnobTable` + `FlatContract`** (M1d). It also needs the defaults the file doesn't write (device models, bench, distributions, engine settings), and the **engine deck** (the exported netlist with a knob map) from M1e/M1f (§1.2–1.4, §5) |
| What structures does it work with? | A knob space, a plan (sides, cones, needed analyses), requests and run records, one run table per check, per-side results, verdict records and the check report. The report and the run table are stored per design revision (§1.5, §6.2) |

**The plan on one page:**

```
 ce_amp.spl ─M1c─► AST ─M1d elaborate─► FlatDesign + KnobTable + FlatContract       rev = hash of these
                                            │ M1e: default bench, default device models (D-A)
                                            │ M1f: engine deck = netlist with a .param per knob + knob map + probe map
                                            ▼
 spicy check ─► PLAN  8 knobs · 5 spec sides · a cone per measure · needs: op, AC at 1 kHz, sweep 0.1 Hz–100 kHz
                 │
                 ▼  ngspice worker process (libngspice; per run: alterparam · reset · op · ac · read vectors)
   0  nominal + read-back of every knob .....................    1 run    binding self-test
   1  enumerate all 2^8 corners .............................  256 runs   worst_case value per side
   2  interior check at each worst corner + 8 audit points ..   46 runs   (+ ascent when something beats the corner)
   3  sigma(3) per side: 3σ-point search from 2 starts ......  265 runs   (incl. 44 uniform-spread runs where worst_case fails)
   4  band: decisive points re-run at reltol 1e-9 ...........   11 runs
                 │
                 ▼
   verdict tables (§3) ─► check report ─► terminal table · --format json · stored under .spicy/checks/<rev>.json
   CE amp: bias FAIL at worst_case (6.5595 V) beside PASS at sigma(3) (6.2813 V); every other side PASS.  579 runs, 0.4 s
```

## Glossary

| Term | Meaning |
|---|---|
| knob | One thing that varies, with a range: `temp`, `vcc.v`, `r1.value`, `q1.beta` |
| range / statistical knob | Must hold at every value (temperature, supply) / a manufacturing spread (tolerances, β) |
| ε | A knob's position: −1 at its low edge, 0 at nominal, +1 at its high edge |
| corner | Every knob at an edge: 2^8 = 256 corners for the CE amp |
| side | One direction of a spec: `bias` has two (≤ 6.5 V and ≥ 4.5 V); the CE amp has 5 |
| cone | The knobs that can reach a measure. A capacitor can't reach a DC voltage, so bias's cone has 7 knobs, gain's and bass's 8 |
| run / round | One simulation at one knob point / one batch of runs sent together (latency counts rounds) |
| inner bound | The worst value actually simulated. It proves a FAIL |
| nudge | A 1% step of one knob (h = 0.01 in ε) to see which way it moves a measure |
| `worst_case` / `sigma(3)` | The two confidences: every knob anywhere in its range / range knobs anywhere, statistical knobs within their 3σ spread (engine.md §3.6) |
| 3σ ball, 3σ point | The region `sigma(3)` allows for the statistical knobs, in standard-normal units / its worst point |
| decisive point | The point a verdict rests on: the side's worst corner, or its 3σ point |
| ε_num (band) | The numerical uncertainty of one simulated value |
| bracket | How far the true worst could lie beyond what a search found |
| pooling | Every run counts for every side: a side's inner bound is the best row of the whole table |

---

## 1. When the engine runs, what goes in, what comes out

### 1.1 When it runs

| Era | Trigger | What runs | CE amp |
|---|---|---|---|
| **MVP** | `spicy check file.spl` (you, or an agent in a terminal) | The whole check of §2, cold, every time. It stores the report and the run table under `.spicy/checks/<rev>.json` (≈ 0.25 MB) | 579 runs, 0.4 s |
| MVP | `spicy check --explain bias` | Reads the stored run table if the file's `rev` matches (0 runs); otherwise runs the check first | < 10 ms, or 0.4 s |
| MVP | `spicy sim --at <point>`, `spicy sweep <knob>` | One real run, or a few | 1–5 ms |
| MVP | `spicy check --deep` | The check, then every UNDECIDED's `next` step (§3.3) | seconds |
| Editor | On save or idle, debounced | The same full check; a what-if is a re-check (all five positions) | 0.4 s |
| Later | On demand | Transient specs (their own tier, §9.3) | 0.03 s per side on the CE amp, minutes on a 300-knob board (scale §6.7) |
| Later | Sign-off | Statistics (M7), a second backend | minutes |

**Why no instant tier.** A re-check costs less than one agent turn. Stored lines and surfaces were badly wrong for real edits. For C_in 1 → 2.2 µF the line gave −4.0 Hz (round 2, C11) and a quadratic surface 42.9 Hz, where the truth is 12.18 Hz (alternatives §3.7). A re-check gives 12.177 Hz **[S]**.

### 1.2 What goes in, beyond the AST

The engine sits below the language. It never sees syntax.

| Input | Made by | For the CE amp |
|---|---|---|
| **`FlatDesign`** | M1d flatten (`model.md` E14–E17) | 6 devices by path (`r1` … `q1`); 6 nets (`vcc`, `gnd` = ground, `input`, `output`, `base`, `emitter`); each device field is `Exact(value)` or `Knob(id)` |
| **`KnobTable`** | M1d (E16) | 8 knobs: `temp` (263.15–333.15 K), `vcc.v` (12 V ± 5%), `r1`/`r2`/`rc`/`re.value` (± 1%), `c_in.value` (± 20%), `q1.beta` (100..=300) |
| **`FlatContract`** | M1d (E24) | Measure `h = ac(output.v / input.v)`; specs `bias`, `gain`, `bass`, each with relation, bound and confidence (default `sigma(3)`) |
| **`rev`** | engine | A hash of the three above without spans (E22), the engine settings and the backend identity (ngspice version, options): soundness C12 |
| **Engine deck** | M1e lowering, M1f export | §1.4 and §5 |
| **Defaults the file doesn't write** | M1e and the engine | §1.3 |

### 1.3 Defaults not written in the file

Each one moves numbers, and each one is written into the check report, so nothing is silent.

| Default | Value in this plan | Owner | Evidence it matters |
|---|---|---|---|
| Device model of a bare `Npn` | `IS=1e-14 BF={beta} XTB=1.5 XTI=3 EG=1.11`, TNOM 25 °C (**decision D-A**) | M1e | XTB flips bias max at worst case from FAIL to PASS (§2.1) |
| TNOM | 25 °C, written out (ngspice's default is 27 °C) | M1f | TNOM 27 °C moves nominal VC 5.5032 → 5.5410 V under D-A's model **[S]** |
| Nominal of a knob given as `lo..=hi` | The midpoint (β = 200) | M1d, agreed (`model.md` §7 q. 3) | — |
| Default bench | A DC source on each `Power<In>` port at its knob; an `AC 1` source on each `Analog<In>`; outputs unloaded | M1e (language §8.5) | — |
| Distribution of a statistical knob | Truncated normal, σ = half-range/3, edges at ±3σ; uniform as the robustness check | engine (engine.md §2.4; D3) | Exact map vs identity: 6.343 → 6.313 V (round 2, C7) |
| Default confidence | `sigma(3)` for user specs | engine (engine.md D11) | — |
| Engine settings | `reltol=1e-6 vntol=1e-9 abstol=1e-15`; sweep at 50 points/decade, at least two decades beyond the contract's frequencies (CE amp: 0.1 Hz–100 kHz; affine report §3.4); nudge h = 0.01 in ε; enumeration budget 2^12 corners per side | engine | §4.5, §2.3 |

### 1.4 The engine deck

What the M1f exporter hands the engine for `ce_amp.spl`. Checked on ngspice-42 **[S]** (`t_engine_deck.cir`: nominal VC 5.503227 V; every read-back path answers).

```
* ce_amp.spl · CeAmp · engine deck (parametric export)
* k0 = temp (K → °C)   k1 = vcc.v   k2 = r1.value   k3 = r2.value   k4 = rc.value
* k5 = re.value        k6 = c_in.value              k7 = q1.beta
.param k0=25 k1=12 k2=47000 k3=10000 k4=4700 k5=1000 k6=1e-06 k7=200
.options tnom=25 reltol=1e-6 vntol=1e-9 abstol=1e-15
.temp {k0}
V_vcc   vcc   0 DC {k1}                 ; default bench: Power<In> port vcc
V_input input 0 DC 0 AC 1               ; default bench: Analog<In> port input
R_r1 vcc base {k2}
R_r2 base 0 {k3}
R_rc vcc output {k4}
R_re emitter 0 {k5}
C_c_in base input {k6}                  ; Electrolytic p = base, n = input
Q_q1 output base emitter QM_q1
.model QM_q1 NPN(IS=1e-14 BF={k7} XTB=1.5 XTI=3 EG=1.11)
```

| Knob | `.param` | Unit conversion | Target | Read-back vector |
|---|---|---|---|---|
| `temp` | `k0` | K → °C (−273.15) | circuit temperature | `@q_q1[temp]` |
| `vcc.v` | `k1` | — | `V_vcc` DC | `@v_vcc[dc]` |
| `r1.value` … `re.value` | `k2` … `k5` | — | `R_r1` … resistance | `@r_r1[resistance]` … |
| `c_in.value` | `k6` | — | `C_c_in` capacitance | `@c_c_in[capacitance]` |
| `q1.beta` | `k7` | — | model `QM_q1`, BF | `@qm_q1[bf]` |

- **The model name differs from the instance name.** With both named `Q_q1`, ngspice resolves `@q_q1[bf]` to the instance and answers "no such parameter" (found when the first draft's deck was re-run in review).
- **Parameters are knob ids** (`k0` … `k7`), with a comment giving each path. ngspice lowercases names, so path-derived names like `a.b_c` and `a_b.c` could collide.
- **The probe map** turns `output.v` into `v(output)`, and names the device records to read (`@q_q1[ic]`, `[ib]`, `[vbe]`, `[vbc]`, `[gm]`, `[gmu]`). The engine computes `h` as `v(output)/v(input)`; it never assumes the source is exactly 1 V.
- The deck carries **no analyses**; the worker sends them per request.

### 1.5 The structures, traced on the CE amp

```
 ce_amp.spl:12   let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };
   M1d           FlatDevice r1: Resistor a→vcc b→base, value = Knob#2
                 Knob#2 "r1.value": statistical, nominal 47 kΩ, 46.53k … 47.47k
   M1f           R_r1 vcc base {k2}   · knob map: r1.value → k2, read-back @r_r1[resistance]
   engine        KnobSpace: ε(r1) ∈ [-1, 1], x = 47k + 470·ε; in the cone of every measure
 ce_amp.spl:27   spec bias: dc(output.v) in 4.5V..=6.5V;
   M1d           Spec bias: Call(Dc, Probe(output, V)), In 4.5 V..=6.5 V, confidence sigma(3) (default)
   engine        Plan: sides bias.max (≤ 6.5 V), bias.min (≥ 4.5 V); needs op + v(output);
                 cone = 7 knobs (c_in can't reach an operating point)
 ── one run (corner 102 of 256) ───────────────────────────────────────────────────────────────
   Request       point ε = (-1, +1, +1, -1, -1, +1, +1, -1), analyses {op, ac@1kHz, sweep}
   worker        alterparam k0=-10 … k2=47470 … k7=100 · reset · op · read vectors · ac …
   Run           read-back 47470 ✓ (all 8) · v(output) 6.5595 V · h(1 kHz) · 301-point sweep · q1 active
   measures      vc 6.5595 V · gain 4.4748 · f_low 17.77 Hz             (our library, never ngspice's meas)
   RunTable      row 102, indexed once per cone (bias: 7 knobs; gain, bass: 8) by physical values
   SideResult    bias.max, worst_case: inner 6.5595 V at row 102 (enumeration), interior check clean
   Verdict       FAIL, counterexample: temp -10 °C, vcc.v 12.6 V, r1 47.47k, r2 9.9k, rc 4.653k,
                 re 1.01k, q1.beta 100, c_in any
   Record        claim + next + tags ─► CheckReport ─► terminal table / JSON / .spicy/checks/<rev>.json
```

| Structure | What it holds | Lives |
|---|---|---|
| `KnobSpace` | Per knob: kind, lo, hi, nominal; a distribution for statistical knobs; ε ↔ physical value | per check |
| `Plan` | Spec sides (sense, bound, confidence), each measure's `Cone`, the `Needs` (analyses, frequencies, probes, device records, read-backs) | per check |
| `Request` / `Run` | A knob point with its analysis set / everything one simulation returned, with a status | per run |
| `RunTable` | Every run of the check. One index per distinct cone, keyed by the physical values of the cone's knobs, so an edit to one knob's range keeps the other rows valid | per check; stored per `rev` |
| `SideResult` | Inner bound, its point, method, evidence (interior check, audit, search starts, convergence, bracket), ε_num | per side × confidence |
| `Record`, `CheckReport` | Verdict, value, margin, counterexample, tags, `claim`, `next`; plus `rev`, cost, `not_modeled` | stored per `rev` |

---

## 2. The check, step by step, on the CE amp

### 2.1 One reference configuration

The positions' numbers differ partly because their transistor models differ. Here are four settings of the same circuit **[S]** (`key.py`, `nominal.py`):

| Model setting | Nominal VC | hFE at −10 / 25 / 60 °C | bias max `worst_case` | Margin to 6.5 V | Used by |
|---|---|---|---|---|---|
| XTB 1.5, TNOM 25 | 5.5032 V | 165.8 / 200 / 236.2 | **6.5595 V** | **−59.5 mV, FAIL** | minimal, soundness, scale, alternatives |
| XTB 0, TNOM 25 | 5.5032 V | 200 / 200 / 200 | **6.4572 V** | **+42.8 mV, PASS** | (new here) |
| XTB 0, TNOM 27 (ngspice defaults) | 5.5381 V | 200 / 200 / 200 | 6.4863 V | +13.7 mV | agent §2.5 |
| XTB 1.5, TNOM 27 | 5.5410 V | 164.2 / 198.0 / 233.9 | 6.5940 V | −94.0 mV | agent §2.7 |

- **TNOM** is a constant of the model card: the temperature at which IS and BF are specified. It has nothing to do with the temperature knob's midpoint. With TNOM 27 °C the knob's 25 °C nominal runs 2 K below it: IS drops, VBE rises by 7.8 mV, and VC rises by 35 mV.
- **XTB** gives β a temperature coefficient. At 1.5, β rises about 0.5 %/K, which is exactly the walkthrough's model (walkthrough §3). At 0 (ngspice's default), β is flat.
- **The reference for this plan:** IS = 1e-14 (VBE ≈ 0.65 V at 1 mA, as in the walkthrough), TNOM = 25 °C, XTI = 3, EG = 1.11, engine tolerances. XTB is decision D-A. Where this plan shows one number it's XTB = 1.5; §2.8 shows XTB = 0 beside it.

### 2.2 The answer key

- **`worst_case` key:** all 256 corners, 1,000 seeded interior points, and a coordinate search from each side's best point (1,576 runs, 1.2 s on 16 processes).
- **σ key:** independent of the engine. At each of the 4 temperature/supply corners, 2,000 random directions on the 3σ sphere, then a pattern search (about 50,000 runs, 55 s). **[S]** (`key.py`, `sigma_key.py`)

| Side | XTB 1.5: `worst_case` | σ key | Verdicts | XTB 0: `worst_case` | σ key | Verdicts |
|---|---|---|---|---|---|---|
| bias ≤ 6.5 V | 6.559537 | 6.281320 | **FAIL** / PASS | 6.457245 | 6.183856 | PASS / PASS |
| bias ≥ 4.5 V | 4.692115 | 4.884237 | PASS / PASS | 4.722705 | 4.919197 | PASS / PASS |
| gain ≤ 4.83 | 4.702828 | 4.663552 | PASS / PASS | 4.706440 | 4.668468 | PASS / PASS |
| gain ≥ 4.37 | 4.461772 | 4.517621 | PASS / PASS | 4.459075 | 4.513042 | PASS / PASS |
| bass ≤ 30 Hz | 26.789724 | 24.973485 | PASS / PASS | 26.390928 | 24.759025 | PASS / PASS |

- **Every extreme sits at a corner:** no interior point and no coordinate step beat the best corner. All 256 corners are forward active.
- **The XTB 1.5 key matches the four positions** to the last printed digit (minimal §3, scale §4).
- **A ball search** that also samples the ball's volume and moves temperature and supply inside their range finds the same bias-max value, 6.281320, at the same corner (`deep.py`).
- **Monte Carlo, 20,000 boards** at bias max's corner (−10 °C, 12.6 V): XTB 1.5 gives a maximum of 6.4038 V and a 99.865% quantile of 6.2731 V; XTB 0 gives 6.3032 V and 6.1788 V. No board fails **[S]**.

### 2.3 Routing: enumerate or loop, per side

- **Rule.** Each side enumerates the corners of its own cone when 2^|cone| ≤ 4096 **and** the estimate 2^|cone| × (cost of the nominal run) fits 3 s on the worker pool. Sides share runs wherever their points coincide. A side over the budget goes to the loop (after M3; until then, UNDECIDED (budget)). So the limit is 12 knobs per side, not 12 in total: a two-stage amplifier's local bias sides (7 knobs each) still enumerate.
- **CE amp.** The cones have 7 and 8 knobs; the 256 corners of the 8 serve all five sides. 256 × 0.71 ms = 0.18 s **[S]**.
- **Why this threshold.** The positions agree on the idea and differ on the number: minimal and agent 4096; soundness 256 as a self-check and 4096 for escalation; alternatives 1024 at 1.25–1.5 ms per run. The time clause makes the rule follow the run cost: a 10.6 ms THD transient (alternatives §1) would stop at about 2^8 corners on one worker.

### 2.4 `worst_case`: enumerate every corner (256 runs)

Every corner is one run, and every run returns every measure. After one batch, each side's inner bound is the best row of the table.

| What enumeration gives | Why it matters | Source |
|---|---|---|
| The exact corner extreme of every side | No search error at corners | minimal §4.2 |
| Every limiter that switches on at some corner | Round 2's false PASSes need no guard (hiZ: 62 of 256 corners saturate) | minimal §4.4; alternatives §3.2 |
| The 1-flip table at the counterexample, free | "Why does it fail" needs no new runs (§7.4) | minimal §6.2 |
| A decision where the loop is pessimistic | hiZ gain ≤ 9.273 and VC ≥ 1.197: the loop says UNDECIDED; enumeration says PASS (all corners), which equals the key **[S]** | soundness §3.1; minimal §4.7 |

**CE amp:** bias max = 6.5595 V at (−10 °C, 12.6 V, R1 +1%, R2 −1%, RC −1%, RE +1%, β 100). **2 of 256 corners fail**: the same DC corner with either C_in.

### 2.5 Inside the box: the interior check, the audit, pooling (46 runs)

Corners can't show a maximum inside the box. Three cheap guards look there. Each one feeds the same **ascent**:
- a batched line search along a knob (9-point grid, then 6 zooms);
- a coordinate ascent over the knobs that improve;
- a tangent re-check after each sweep, at most 4 sweeps.

The ascent's result becomes the side's inner bound. It gives `FAIL` if it violates, `PASS (estimated)` if it passes and converged, and otherwise `UNDECIDED (ascent budget)`.

| Guard | What it does | CE amp cost | Source |
|---|---|---|---|
| **Tangent check** | At each side's worst corner, nudge each knob of the side's cone 1% inward. A nudge that improves starts the ascent | 38 runs (c_in skipped for the two DC sides) | soundness §3.2 a; minimal §4.2 |
| **Audit** | 8 seeded Latin-hypercube points inside the box, in the same batch. Each is a witness (a FAIL if it violates) and a refuter (an ascent starts if it beats the worst corner). Never part of any allowance | 8 runs | soundness §5.3 (its interior half) |
| **Pooling** | After `sigma(3)`, every in-box row (the σ search's points are interior points) is checked the same way | 0 runs | round 2 C4 |

**Which guard catches what** **[S]** (`ablation.py`):

| Case (bound) | Truth | All three | Tangent off | Tangent and audit off | Audit alone | None |
|---|---|---|---|---|---|---|
| pq (P_Q ≤ 8.70831 mW) | 8.709679 FAIL | **FAIL** | PASS ✗ | PASS ✗ | PASS ✗ | PASS ✗ |
| tuned5 (gain ≤ 20) | 24.4255 FAIL | **FAIL** | FAIL (audit) | FAIL (pooling) | FAIL | PASS ✗ (15.86) |
| tuned20 (gain ≤ 10) | 24.4253 FAIL | **FAIL** | FAIL (audit) | FAIL (pooling) | FAIL | PASS ✗ (6.683) |

- **The tangent check is necessary.** pq's peak is a needle next to its worst corner: 8.7097 inside against 8.7069 at the corner. Only the tangent sees it. Minimal's single parabola step found 8.70799, UNDECIDED; the ascent finds the key's 8.709679.
- **The audit and pooling cover broad regions away from the worst corner.** tuned's resonance beats the corners over 29% of the box (soundness §5.1). Each of the two guards alone catches it. 8 points hit a region filling 25% of the box with 90% probability.
- **What none of them covers:** a small interior region away from the worst corner (§3.2, §12). Not seen in any suite case.

CE amp: none of the 38 nudges and 8 audit points improves on its side's worst corner; pooling refutes nothing.

### 2.6 `sigma(3)`: finding the worst 3σ board (265 runs)

**What `sigma(3)` asks.** Temperature and supply may take any value in their range. Each part varies by its own spread (normal, σ = tolerance/3, cut at the tolerance). A board counts if its parts together are no less likely than a 3σ event. The worst such board is the side's **3σ point**.

**The CE amp's answer, in words.** For bias max, the 3σ point is −10 °C, 12.6 V, β = 111.5, R1 47.10k, R2 9.980k, RC 4.692k, RE 1.0015k, giving VC = 6.2813 V. That is a weak transistor (β 2.66σ below 200) with resistors only 15–21% of the way to their tolerance edges. The worst corner (6.5595 V) needs all six at their edges at once. That's why bias fails at worst case and passes at `sigma(3)`.

**How the engine finds it.**
1. **Start A:** the worst temperature/supply corner with parts at nominal. Measure how each part pushes the output (one nudge each), jump to the 3σ board the straight line says is worst, simulate it with its nudges, and repeat. Stop when prediction and simulation agree to 1e-4 of the bound: 3–7 rounds here. Never accept a worse point (round 2 C7; minimal §4.1).
2. **Start B:** the worst-case corner, scaled down into the 3σ ball, searched the same way. Round 2's single start missed a saturating corner (hiZ: 8.66 found, 5.48 true) and a second band edge. Start B fixed both (minimal §4.5), so it always runs.
3. **Cross-check:** the answer is re-simulated at the other temperature/supply corners, and the search moves if one is worse.
4. **Temperature and supply inside their range:** the search only visits them at their edges, so it nudges them inward at the 3σ point (2 runs per side). An improvement means UNDECIDED (interior peak).
5. **Uniform spread,** only where `worst_case` fails and the default passes: the same search with parts spread evenly across their tolerance (engine.md §2.4). The verdicts must agree.

**When the search may not vouch for its answer** (0-run rules):
- *device region changes:* a corner of the box has a device outside its nominal region (minimal §4.5);
- *interior peak:* this side's `worst_case` needed an ascent (the box has an interior maximum here), or step 4 flagged. A straight-line search always lands on the ball's surface, so it can't find a maximum inside. **This rule has not yet changed a verdict** **[S]** (`row7_scan.py`):
  - on tuned5, tuned20 and three more tuned variants with an interior peak, the search didn't converge, so S6 already said UNDECIDED;
  - on three variants without one, it converged and matched the ball search (22.8399 vs 22.8388);
  - on pq it converged correctly, because pq's 3σ worst lies on the surface (ball search: 8.684978). There the rule costs an UNDECIDED that `--deep` settles.

  It stays because non-convergence is an observation, not a guarantee;
- *distribution:* the default spread passes and the uniform one fails.

**A `worst_case` PASS implies a `sigma(3)` PASS** (C7): the σ region lies inside the box.

**Accuracy on the CE amp.** 28–57 runs per side, 2 starts each, all converged. The last-jump misses (the brackets) are 8.7e-5 to 1.03e-3, and every key lies inside its bracket **[S]**:

| Side | Engine | Key | Engine is optimistic by |
|---|---|---|---|
| bias ≤ 6.5 V | 6.281314 V | 6.281320 | 6e-6 |
| bias ≥ 4.5 V | 4.884252 V | 4.884237 | 1.5e-5 |
| gain ≤ 4.83 | 4.663511 | 4.663552 | 4.1e-5 |
| gain ≥ 4.37 | 4.517652 | 4.517621 | 3.1e-5 |
| bass ≤ 30 Hz | 24.973484 Hz | 24.973485 | 1e-6 |
| bias ≤ 6.5 V, uniform spread | 6.465673 V | 6.46567 (minimal §4.4) | 0 |

**What a `sigma(3)` PASS rests on.** Most `sigma(3)` verdicts are PASS (implied by worst case), which rests on enumeration. A `sigma(3)` PASS (estimated), like bias max's, rests on this local search and the rules above: a weaker basis, and the report says so (§3.2).

### 2.7 The numerical band (11 runs)

Each side's decisive points are re-run cold at reltol 1e-9. The band is ε_num = max(2·|f − f_tight| + the measure's own band, 1e-9·|f|). The measure band is 1e-6 relative for f_low, which is interpolated from the sweep (2.1e-7 measured at 50 points per decade: minimal §2.2; soundness §6.3).

CE amp: ε_num = 4.5–6.6e-9 for VC and gain, 2.7e-5 Hz for f_low **[S]**. These are exactly the floors: the tight re-runs agreed better than 1e-9. The closest call, bias max, is 0.0595 V past its bound, so the band decides nothing here. It is there for the edge case.

### 2.8 What it prints, and what it costs

```
$ spicy check circuits/ce_amp.spl
 CeAmp · 8 knobs (2 range, 6 statistical) · ngspice-42 · q1: generic Npn (IS 1e-14, XTB 1.5, TNOM 25 °C) · rev 3f9a1c
 spec  side      sigma(3)  (the specs' confidence)              worst_case
 bias  ≤ 6.5 V   6.2813 V   PASS (estimated)                    6.5595 V   FAIL
                 uniform spread 6.4657 V PASS                   at temp -10°C vcc.v 12.6V r1 +1% r2 -1% rc -1% re +1% q1.beta 100 c_in any
 bias  ≥ 4.5 V   4.8843 V   PASS (implied by worst case)        4.6921 V   PASS (all corners)
 gain  ≤ 4.83    4.6635     PASS (implied by worst case)        4.7028     PASS (all corners)
 gain  ≥ 4.37    4.5177     PASS (implied by worst case)        4.4618     PASS (all corners)
 bass  ≤ 30 Hz   24.973 Hz  PASS (implied by worst case)        26.790 Hz  PASS (all corners)
 bias passes at sigma(3); it fails only with every part at its worst edge, cold, on a high supply. Your call.
 not modeled: Early effect (VAF), junction capacitances, base/collector/emitter resistances
 cost: 579 runs (256 corners, 46 interior, 265 sigma, 12 other) · 65 rounds · 0.4 s
```

With XTB = 0, every side is PASS (implied by worst case) / PASS (all corners): bias 4.7227 … 6.4572 V, gain 4.4591 … 4.7064, bass 26.391 Hz; 518 runs **[S]**.

**Cost by stage** **[S]** (`canon.py`):

| Stage | Runs | Rounds |
|---|---|---|
| 0 nominal + read-back | 1 | 1 |
| 1 enumeration | 256 | 1 |
| 2 tangent check + audit | 38 + 8 | 1 |
| 3 `sigma(3)`: 5 sides × 2 starts, + 2 range nudges per side | 221 | 51 |
| 3 uniform spread, bias max | 44 | 10 |
| 4 band | 11 | 1 |
| **Total** | **579** | **65** |

- **Rounds:** 55 of the 65 are the 10 independent σ searches running one after another. Written as resumable state machines and merged into rounds by the driver, they need about as many rounds as the longest search: **≈ 19 in all**, computed from the measured per-search round counts **[S]**. This plan takes that (scale §8 item 5). It changes nothing on one libngspice worker, and it makes more workers and the `ngspice -b` fallback pay.
- **`sigma(3)` only where it decides the verdict:** 413 runs. Values on every side (D7) cost +166 runs and +0.12 s.

| Transport (same 579 runs; run values agree to 1.9e-15, search results to 1.5e-14) | Simulation time, serial |
|---|---|
| libngspice, deck loaded once, binary vectors | **0.39–0.42 s** |
| `ngspice -p` (pipe mode): one persistent process fed commands | 0.73–0.75 s |
| `ngspice -b`: one process per round | 1.0–1.2 s (0.75–1.0 s split over 8) |

These are ranges over several measurements under a machine load of 1.4–2.9 from other jobs; `-b` is the most load-sensitive, because it starts a process every round. The prototype's own Python arithmetic adds about 0.5 s, mostly the 3σ-point solve; a Rust engine removes nearly all of it. Appendix A explains why the positions' run counts (156, 223, 389) differ from 579.

---

## 3. Verdicts and statuses

### 3.1 The two verdict tables

For an upper bound B (a lower bound mirrors it). I is the inner bound, ε_num the band. **The first matching row wins.** Each row is a `Reason` or a PASS kind in the code (§6.2).

**`worst_case`:**

| # | Condition | Verdict |
|---|---|---|
| W0 | any run's read-back differs from the requested knob value | **UNDECIDED (binding)**; the check stops. It blocks even a FAIL, because the simulated point isn't the reported one (soundness §7.2) |
| W1 | 2^\|cone\| over the enumeration budget | UNDECIDED (budget) |
| W2 | I > B + ε_num | **FAIL**, with the counterexample |
| W3 | \|I − B\| ≤ ε_num | UNDECIDED (numerics) |
| W4 | a needed run failed or was implausible (after the retry ladder) | UNDECIDED (simulator) |
| W5 | an ascent didn't converge in its budget | UNDECIDED (ascent budget) |
| W6 | every corner passes, no nudge improves, nothing refutes the worst corner | **PASS (all corners)** |
| W7 | otherwise: a converged ascent's result passes, B − I > ε_num | **PASS (estimated)** |

**`sigma(3)`:**

| # | Condition | Verdict |
|---|---|---|
| S0 | as W0 | UNDECIDED (binding) |
| S1 | as W1 | UNDECIDED (budget) |
| S2 | the side's `worst_case` verdict is a PASS | **PASS (implied by worst case)** |
| S3 | I > B + ε_num | **FAIL** |
| S4 | \|I − B\| ≤ ε_num | UNDECIDED (numerics) |
| S5 | a needed run failed or was implausible | UNDECIDED (simulator) |
| S6 | a start didn't converge | UNDECIDED (search) |
| S7 | some corner has a device outside its nominal region | UNDECIDED (device region changes) |
| S8 | `worst_case` needed an ascent on this side, or temperature/supply improve inward at the 3σ point | UNDECIDED (interior peak) |
| S9 | the uniform spread FAILs | UNDECIDED (distribution) |
| S10 | the uniform spread is UNDECIDED for reason *r* | UNDECIDED (uniform spread: *r*) |
| S11 | B − I > bracket + ε_num | **PASS (estimated)** |
| S12 | otherwise | UNDECIDED (bracket) |

The uniform-spread search gets its own verdict from rows S3–S8, S11 and S12.

**Re-derived for the CE amp:**
- bias max is FAIL by W2. At `sigma(3)` it passes S3–S8; the uniform spread passes (0.034 V over its 5.9e-5 bracket), so not S9 or S10; S11 gives PASS (estimated), with 0.219 V against a bracket of 1.9e-4 V.
- The other four sides are PASS (all corners) by W6, and PASS (implied by worst case) by S2.

§2.8, §8.5 and the acceptance criteria (§8.3) are derived from these tables as written.

### 3.2 The words, and what each one claims

| Word | Claims | Does not claim |
|---|---|---|
| **FAIL** | A reachable board violates the spec; its point is printed and reproduces at reltol 1e-9 | — |
| **PASS (all corners)** | Every corner simulated and passing; no inward nudge at the worst corner improves; no audit or pooled point beats it | That no small region inside the box, away from the worst corner, is worse |
| **PASS (estimated)** | A search's worst point passes by more than its bracket and band. At `sigma(3)` it rests on the local search of §2.6 | That no worse point exists (the problem is NP-hard: worst-case report §1.3) |
| **PASS (implied by worst case)** | The same side passes at `worst_case`, and the `sigma(3)` region lies inside the box | Anything beyond that PASS's own claim |
| PASS (guaranteed) | Later: exact methods only (datasheet arithmetic, the corner theorem) | — |
| UNDECIDED (reason) | The engine can't decide; `next` says what would (§3.3) | "Probably passes" |
| UNSPECIFIED | Later (M5): the data doesn't define the question, for example `sigma(3)` with a one-sided tested limit (agent R9) | — |

- **Why "PASS (all corners)" gets its own word:** it rests on no model and no search, only on simulated corners (alternatives §3.9; agent's `exhaustive`).
- **Sampling never supports a PASS.** Monte Carlo, Latin-hypercube and audit points may witness a FAIL or refute a search, never support a `worst_case` or `sigma(3)` PASS. A 70-point LHS gave 949 false PASSes in 960 failing cases, and the best of 16,384 samples was still 0.25 V short of the worst corner (alternatives §3.3, §3.6). Sampling answers "how often", never "how bad".

### 3.3 UNDECIDED reasons, and `next`

Every UNDECIDED carries the step that would settle it, with its cost (agent R1; soundness §7.3). `spicy check --deep` runs them all.

| Reason | `next` | Automatic? | Cost at ngspice speed |
|---|---|---|---|
| binding | None: an exporter bug. The message names the knob ("asked −10 °C, ngspice ran 27 °C") | — | — |
| simulator | Retry ladder: restart the worker and retry the point; then report it with `spicy sim --at` | retry: yes | 1–10 runs |
| numerics | For f_low: refine the crossing with exact `ac lin 1 f f` points at the decisive point. Otherwise none | yes | 3–5 runs |
| ascent budget | A larger ascent budget, with more starts | `--deep` | 50–200 runs |
| search, interior peak, bracket, uniform spread: … | **Ball search:** per temperature/supply corner, 750 points on the 3σ sphere and 750 in its volume, then a pattern search that also moves temperature and supply inside their range; budget 4,000 pattern runs | `--deep` | 6,600–9,600 runs per side and spread **[S]**; ≈ 5–7 s on one worker, about 1 s on 16 |
| device region changes | A `sigma(3)` search on the device margin as its own side (minimal §4.5). After M3; until then it stays UNDECIDED | — | tens of runs |
| distribution | None: declare a distribution, or change the parts. A design decision | — | — |
| budget | The loop (after M3) | — | — |

**The ball search's verdict.** FAIL if its worst violates. PASS (estimated; ball search) if its pattern search converged within budget and B − I > ε_num. Otherwise UNDECIDED (search). It is still a search, not a proof, but it samples the whole ball and the whole temperature/supply range. On `pq` it gives 8.684978 mW (default spread) and 8.705026 mW (uniform), both converged, so PASS. On `tuned20` at bound 20 it hits its budget at 23.9976, a FAIL. Both are in `deep.py`.

### 3.4 Tags, `any` and `not_modeled`

- **`any`:** a knob outside the measure's cone prints as `any` in the counterexample. Otherwise an LLM reads `c_in = 1.2 µF` in the bias counterexample and blames C_in (agent §5.3 #3).
- **Region tag:** the worst point's device regions ("q1 saturated at the worst point"). Enumeration's verdict stands; the tag informs.
- **`not_modeled`:** generated from the model cards: defaulted parameters that matter (no VAF, no junction capacitances; XTB left at 0 if it is). The MVP's key verdict hinges on one (agent R5).
- **The existing tags stay** (engine.md §3.4): model-conditional, relies-on-typical, relies-on-unreviewed-part-data (M5), distribution-sensitive.

### 3.5 The claim text

The engine writes the sentence. The terminal and the agent both print it; the agent may paraphrase it but never strengthen it (agent R1). Examples for the CE amp:
- *FAIL:* "bias max FAILS at worst case: 6.5595 V > 6.5 V at temp −10 °C, vcc.v 12.6 V, r1 47.47k, r2 9.9k, rc 4.653k, re 1.01k, q1.beta 100 (c_in: any). Reproduces at reltol 1e-9 within 7e-9 V."
- *PASS (all corners):* "bias min passes at worst case at all 256 corners: worst 4.6921 V ≥ 4.5 V (margin 0.192 V); nothing inside the box beat the worst corner."
- *PASS (implied by worst case):* "gain passes at sigma(3) because it passes at worst case."
- *UNDECIDED:* "pq max is undecided at sigma(3): the box has an interior peak on this side. Do not call it a pass. Next: `spicy check --deep` (≈ 14,000 runs, ≈ 10 s)."

### 3.6 Statuses for numbers outside a verdict

| Status | What it is | May prove | In the MVP |
|---|---|---|---|
| (a verdict record) | The check's result on this `rev`; its words come from `claim` | its verdict | yes |
| `simulated` | One real run at a stated point (`sim --at`, `sweep`, the flip table) | FAIL, never PASS | yes |
| `stale` (modifier) | A stored record of another `rev`. The CLI marks it: it compares the file's current `rev` with the stored one | nothing about this design | yes |
| `predicted` | From stored slopes, no simulation | nothing | no (no instant tier) |
| `carried` | A verdict from an earlier `rev` whose cone the edit didn't touch (scale §7.2) | the old verdict, exactly | no (needs the cross-revision cache) |

**No "verified":** an LLM reads it as "proven" and writes "verified to pass" (agent §3.3, §5.3 #8). Round 2's `inner_bound` becomes `simulated`.

---

## 4. The ngspice backend

### 4.1 Shape

```
 spicy (engine, one process)                              worker process(es), P ≥ 1 (default 1)
 ┌──────────────────────────────────┐   batch of requests  ┌──────────────────────────────────────────┐
 │ driver: stages → rounds           │ ───────────────────► │ libngspice (dlopen), the deck loaded ONCE │
 │ run table · measures · verdicts   │                      │ per run: alterparam × n · reset · op ·    │
 │ restart a worker that died        │ ◄─────────────────── │ read vectors · ac … · destroy all         │
 └──────────────────────────────────┘   runs, in order     │ netlist error / crash → this worker dies  │
                                                            └──────────────────────────────────────────┘
 cross-check path (tests, no FFI): the same deck through `ngspice -b` with a generated .control block
```

| Option | Per MVP check | For | Against |
|---|---|---|---|
| **libngspice in a worker process** (scale §3.6) | **0.39–0.42 s** **[S]** | Fastest; binary vectors, no text parsing. A crash or netlist error kills only the worker (restart 18–19 ms, soundness §2.2 #5). The editor needs it anyway | A small FFI surface (5 C functions, 3 callbacks), a worker binary and a pipe protocol |
| `ngspice -p` pipe worker | 0.73–0.75 s **[S]** | Persistent, no FFI | Text output; file I/O per sweep |
| `ngspice -b` per round (minimal §2.1) | 1.0–1.2 s serial **[S]** | Simplest; no FFI | A process start per round |
| libngspice inside `spicy` (agent R11) | ≈ 0.4 s | No worker | A netlist error or a vendor-model crash takes down `spicy`; one circuit per library |

Recommended (decision D-B): the libngspice worker, one per core when batches are large, with `ngspice -b` kept in the test suite as a cross-check. The CE amp needs no parallelism (0.4 s serial). Scale measured 6× on 16 workers for the CE amp and 9.4× at 302 knobs (scale §3.3).

### 4.2 One run

```
 alterparam k0=-10 · … · alterparam k7=100                set every knob (the deck keeps a .param per knob)
 reset                                                      rebuild the circuit: every run is cold and pure
 op                                                         operating point
 read v(output) v(base) v(emitter) … @q_q1[ic|ib|vbe|vbc|gm|gmu] · read-backs
 ac lin 1 1000 1000 · read v(output), v(input)              one exact point per at(f)
 ac dec 50 0.1 1e5 · read frequency, v(output), v(input)    the sweep for f_low
 destroy all                                                drop the plots
```

The band stage loads a second copy of the deck with tight `.options` (tolerances must live in the deck: rule 2 below). Loading one takes ≈ 2 ms (agent §2.3).

### 4.3 Rules and traps

Each rule is pinned by a test (§8.4); Appendix B has the evidence for each.
1. `destroy all` after every run (plots accumulate; a run slows from 0.86 to 5.7 ms after 150).
2. Temperature by `.temp {k0}` + `alterparam`; tolerances as `.options` in the deck (control-mode `option temp`/`option reltol` are silently undone by `reset`).
3. A plausibility check on every run (no DC solution returns 999,999,999.99 V, return code 0, no message).
4. ngspice in a worker process, restarted on "cannot recover" or a missing vector (a netlist error kills libngspice).
5. The exporter writes `tnom` (the default is 27 °C).
6. `ac lin 1 f f` for every `at(f)` (`ac dec` puts "1 kHz" at 1000.0000000000143 Hz).
7. Never send `sens` to a worker (it freezes temperature in that instance; AC `.sens` returns ≈ 0, so ngspice stays L0).
8. Our measure library only, never `meas` (it interpolates linearly: 20.083–20.161 Hz vs 20.127 Hz).
9. One deck per worker (one circuit per loaded library).
10. Exact comparisons within one build (`reset` makes every run cold and bit-identical); a tolerance across builds (§8.1).
11. Read-back paths come from the knob map, and use distinct model names (§1.4).

### 4.4 Read-back, plausibility, failures

- **Read-back:** every run reads every knob back from the simulator. A mismatch above 1e-9 relative stops the check with UNDECIDED (binding). Cost: 8 extra vectors per run. It catches the dead temperature knob after **1 run** **[S]**. Without it, every configuration gave a false PASS on bias max (soundness §3.2 d). A test checks at the nominal run that every read-back path exists.
- **Plausibility:** any node voltage above 10 × the largest source is `implausible` (soundness §11 rec. 4).
- **A failed run is data,** never dropped: it can't support a PASS (W4, S5), and a FAIL elsewhere still stands.

### 4.5 Tolerances

`.options reltol=1e-6 vntol=1e-9 abstol=1e-15` go in the deck. They cost 0.8% of run time (0.794 vs 0.788 ms) and cut the error over the 256 corners from 9.5e-5 V to 6e-13 V (soundness §6). The band is measured by re-running, never by a difference table: a table reports 2e-13 V where the error at default tolerances is 9.5e-5 V (soundness §6).

### 4.6 Measures on ngspice

| Measure | Method | Accuracy |
|---|---|---|
| `dc(output.v)` | op vector | ε_num ≈ 5e-9 V **[S]** |
| `h.at(1kHz).mag()` | `ac lin 1 1k 1k`, \|v(output)/v(input)\| | exact frequency |
| `h.f_low(-3dB)` | Literal −3 dB below max\|H\| over the swept band (D5). Cubic interpolation in (ln f, dB) on the 50-per-decade sweep; exact refinement only at a decisive point inside its band | 2.1e-7 relative (minimal §2.2); 5e-7 at 40 per decade (scale §3.5) |
| Device region | Saturated when gmu > 1e-4·gm; cutoff when IC ≤ 0 | minimal §2.2 (checked on bare Gummel–Poon only) |
| Value kinds | `Value` or `Undefined` (never passes); `Beyond` later (C9) | — |

Round 2 refined f_low with extra AC solves (C9). On ngspice each extra frequency is a whole `ac` command with its own operating point (scale §3.2). At 2.1e-7 relative, interpolation matters only when a bound sits within 1e-6 of the value, and then the decisive point gets the exact refinement.

---

## 5. The M1f contract, and what the language track must provide

The engine meets the language at the engine deck. It needs three things from the other session. They are listed precisely enough to build: they add parameters, commands and one export mode, and M1f's round-trip done-when depends on them.

### 5.1 The engine deck, as a type (published in M3a)

```rust
// spicy_backends::ngspice (next to Binding, pipeline.md §3)
pub struct EngineDeck {
    pub text: String,                  // §1.4: .param k_i per knob, .temp {k_temp}, .options, elements, one .model per BJT
    pub knobs: Vec<KnobBinding>,       // KnobId → param name ("k2"), unit conversion, target (element, field), read-back vector
    pub probes: ProbeMap,              // net path → node name; device path → element name; the AC input source
}
pub fn engine_deck(design: &FlatDesign, knobs: &KnobTable, options: &SpiceOptions) -> Result<EngineDeck, ExportError>;
```

- **Names:**
  - elements follow roadmap §3 (`R_r1`, `Q_q1`), with collisions checked;
  - each BJT whose model field is a knob gets its own `.model QM_<path>`, named apart from the instance;
  - parameters are knob ids `k0 … kN` with a path comment.
- **Options are an input.** The engine asks for two decks, one with engine tolerances and one with tight tolerances.
- **No analyses** in the deck.
- **Ownership:** M1f (language track) writes both export modes, numeric and engine deck. The engine track (M3b) consumes `EngineDeck`, and uses hand-written decks until M1f lands.

### 5.2 `spicy_circuit`: what the model tables must carry for D-A

Following the existing pattern in `crates/spicy_circuit/src/params.rs`, every parameter the front-ends accept is carried, and the ones the simulator doesn't use yet say "Not simulated yet":

| Where | Field | Default | Doc |
|---|---|---|---|
| `BjtModel` | `xtb: f64` | 0.0 | forward/reverse β temperature exponent. Not simulated yet: temperature support (M2c) |
| `BjtModel` | `xti: f64` | 3.0 | saturation-current temperature exponent. Not simulated yet |
| `BjtModel` | `eg: f64` | 1.11 (eV) | energy gap for IS(T). Not simulated yet |
| `BjtModel` | `tnom: Option<f64>` | `None` = the circuit's nominal temperature | parameter-measurement temperature (K). `Option` because its default depends on another value, the rule params.rs already states. Not simulated yet |
| new `Conditions` (beside `Circuit` + `Params`; pipeline.md §3 names it, and it doesn't exist in code yet) | `temp: f64`, `tnom: f64` | 300.15 K each | the run's temperature and the circuit's nominal temperature. Not simulated yet |

### 5.3 `spicy_parser`: what it must accept

Today it rejects XTB, XTI and EG with `invalid param` (`netlist_models.rs`, `BjtModel::new`), and has no `.temp` or `.options` command (`netlist_types.rs`, `CommandType`). It must accept:
- **On `.model … NPN|PNP`:** `XTB`, `XTI`, `EG`, `TNOM` (in °C, stored in K), lowered into §5.2's fields.
- **`.temp <value>`:** one value, `{…}` expressions allowed (as in `.temp {k0}`), lowered into `Conditions.temp`. A list of temperatures is an error ("one value").
- **`.options`:** with `tnom` (→ `Conditions.tnom`, and the default for a model without its own TNOM), and `reltol`, `vntol`, `abstol` (→ a `SolverOptions` value in the lowered output next to the analyses, not in `Circuit` or `Params`). Any other key is an error that lists the accepted ones, as for model parameters today, so nothing is dropped silently.

### 5.4 M1e and M1f, and the round trip

- **M1e** lowers a bare `Npn` to `BjtModel { is: 1e-14, bf: ← q1.beta, xtb: 1.5, xti: 3.0, eg: 1.11, tnom: Some(298.15) }` (decision D-A), sets `Conditions.temp` from the `temp` knob, and builds the default bench.
- **M1f, numeric export** (a knob point): writes `.temp`, `.options tnom=…`, and full model cards. *Done when:* export → `spicy_parser` → lower gives the same `Circuit` + `Params` + `Conditions` (roadmap M1f, extended by `Conditions`).
- **M1f, engine deck** (§5.1). *Done when:* the deck at its nominal `.param` values, parsed by `spicy_parser` (which already evaluates `.param` and `{…}`), lowers to the same result as the numeric export at nominal; ngspice's read-back of every knob at nominal and at one corner equals the requested values.
- **Dependencies for the other session:** §5.2 and §5.3 come before M1f's done-when can hold. They are data-only (no temperature physics), so they don't pull M2c forward.

---

## 6. Architecture

### 6.1 Crates and modules

Roadmap §2.2 already has the crates. The engine never imports a simulator crate. `spicy_backends` implements the engine's `Backend` trait, so it depends on `spicy_engine`; roadmap §2.2's diagram should show that edge.

```
 spicy_cli ─── check · sim · sweep · export · the per-rev store (.spicy/checks/)
   │
   ├─► spicy_engine                                  ◄── spicy_model (FlatDesign, KnobTable, FlatContract)
   │     knob · structure (cones) · plan · backend (trait) · measure · runs
   │     worst_case (Enumerate; FlipWalk later) · sigma · deep (ball search) · band · verdict · report · check
   │
   └─► spicy_backends ──► spicy_engine (the trait)
         ngspice: engine deck (M1f), worker binary, pool, request encoding, read-back, plausibility
         spicy (later): model → Circuit → our simulator, same trait
```

### 6.2 Core types (sketches, not code)

```rust
// spicy_engine::knob — from KnobTable
pub struct KnobSpace { pub knobs: Vec<KnobSpec> }
pub struct KnobSpec { pub id: KnobId, pub path: Path, pub kind: KnobKind, pub lo: f64, pub hi: f64, pub nominal: f64 }
pub enum KnobKind { Range, Statistical(Dist) }
pub enum Dist { TruncNormal { edges_at_sigma: f64 }, Uniform }
pub struct Point(pub Box<[f64]>);                       // ε per knob; x = mid + half_range · ε

// spicy_engine::plan — from FlatContract
pub struct Plan { pub sides: Vec<Side>, pub measures: Vec<MeasureDef>, pub needs: Needs }
pub struct Side { pub spec: SpecId, pub sense: Sense, pub bound: f64, pub measure: MeasureId, pub confidence: Confidence }
pub struct MeasureDef { pub expr: MExpr, pub cone: ConeId, pub analyses: AnalysisSet }
pub struct Cone(pub Vec<KnobId>);                       // MVP rule: C and L values never reach an op

// spicy_engine::backend — ngspice now, our simulator later
pub trait Backend {
    type Prepared;
    fn prepare(&self, design: &FlatDesign, knobs: &KnobSpace, needs: &Needs) -> Result<Self::Prepared, BackendError>;
    fn run(&self, p: &Self::Prepared, batch: &[Request]) -> Vec<Result<Run, SimError>>;   // in order
}
pub struct Request { pub point: Point, pub analyses: AnalysisSet, pub tolerance: Tolerance }  // Engine | Tight
pub struct Run { pub readback: Vec<f64>, pub op: Vec<f64>, pub devices: Vec<DeviceOp>,
                 pub ac_at: Vec<Complex64>, pub sweep: Vec<(f64, Complex64)> }
pub enum SimError { Failed(String), Implausible { probe: Probe, value: f64 }, WorkerDied }  // binding: engine-side, from readback

// spicy_engine::measure, runs
pub enum Measured { Value(f64), Undefined(&'static str) }          // Beyond later
pub struct RunTable { rows: Vec<Row>, index: Vec<HashMap<Key, RowId>> }  // one index per distinct cone
pub struct Key(Box<[u64]>);   // the cone's physical values as f64 bits, after one canonical ε → x conversion
                              // (the prototype rounds ε to 12 decimals first)

// spicy_engine::worst_case, sigma
pub enum Strategy { Enumerate, FlipWalk }                          // FlipWalk after M3
pub struct SideResult { pub inner: Option<f64>, pub at: Option<Point>, pub method: Method, pub evidence: Evidence }

// spicy_engine::verdict, report
pub enum Verdict { Fail { at: Point }, Pass(PassKind), Undecided(Reason), Unspecified(String) }  // Unspecified: M5
pub enum PassKind { AllCorners, Estimated, ImpliedByWorstCase }     // Guaranteed later
pub enum Reason { Binding, Budget, Numerics, Simulator, AscentBudget, Search, DeviceRegionChanges,
                  InteriorPeak, Distribution, UniformSpread(Box<Reason>), Bracket }
pub struct Record { pub side: SideId, pub confidence: Confidence, pub verdict: Verdict, pub value: Option<f64>,
                    pub margin: Option<f64>, pub eps_num: Option<f64>, pub counterexample: Vec<(Path, KnobValue)>,
                    pub region: Vec<(Path, Region)>, pub method: Method, pub tags: Vec<Tag>,
                    pub claim: String, pub next: Option<Next> }
pub enum KnobValue { At(Quantity), Any }
pub struct CheckReport { pub rev: Rev, pub records: Vec<Record>, pub not_modeled: Vec<String>, pub cost: Cost }
```

### 6.3 In the MVP, and the seams that keep later work cheap

| Later feature | Seam built in M3 | Why it's enough |
|---|---|---|
| The loop past 12 knobs per side | `Strategy`; a `Cone` on every measure; the run table indexed per cone | The walk plugs in as a strategy. Scale's O(N) law comes from walking cones and keying the cache on them (scale §5.2) |
| Merging searches into rounds | The σ searches are resumable state machines from the start | ≈ 19 rounds instead of 65 (§2.8) |
| Packing requests | — | Adds partial points to `Request` later. **Scale disagrees** (it wants partial requests now, to avoid a retrofit); this plan keeps full points because enumeration asks for full points, and the costly part to retrofit, the per-cone index, is built now |
| Cross-revision cache, `carried` | Keys are physical values over each cone, plus a hash of fixed content | Physical keys survive edits: a β-edge edit kept 1,401 of 2,608 AC runs (scale §7.3) |
| Editor, MCP, a stable JSON schema | `CheckReport` with `serde`; `--format json` marked unstable (`spicy.check/0`); the per-`rev` store | The record already has `claim`, `next`, `rev` |
| Our simulator, Xyce, a libngspice pool | The batch `Backend` trait with per-request analysis sets | scale §8 items 3–4 |
| Transient tier | An `AnalysisSet` per request, never "every analysis of the check" | One transient spec would otherwise multiply a check's cost by about 38 (scale §6.2) |
| Affine forms, `Beyond`, PASS (guaranteed), UNSPECIFIED | Records keep each σ search's last slopes; `Measured`, `PassKind` and `Verdict` have room for the rest | D-F (§10) |

**Not in M3** (each waits for its first consumer): the flip walk and its guards (margin sides with splice, the range-corner net, the e_obs or e₂ allowance, the audit's vertex half), the influence graph beyond the capacitor rule, packing, the cross-revision cache, an instant tier, covering arrays, `.sens`, Monte Carlo as a command, affine forms (D-F).

---

## 7. Flow diagrams

### 7.1 The cold check

```
 spicy check circuits/ce_amp.spl
 ────────────────────────────────────────────────────────────────────────────────────────────────
 parse · elaborate (M1d) · lower + export (M1e/M1f) · rev                                    ms
   │
   ├─ plan: 8 knobs · 5 sides · cones (bias: 7 knobs, gain/bass: 8) · needs                  µs
   ├─ worker: load the deck once                                                              2 ms
   ├─ round 1      nominal + read-back                        1 run   → binding self-test
   ├─ round 2      256 corners                              256 runs  → table: VC, |H(1k)|, f_low, q1 region
   │               bias max 6.5595 V (2 of 256 corners fail) · bias min 4.6921 · gain 4.4618…4.7028 · bass 26.790
   ├─ round 3      38 nudges at the 5 worst corners + 8 audit points   46 runs  → nothing beats a worst corner
   ├─ rounds 4–64  sigma(3): 5 sides × 2 starts + uniform (≈ 15 rounds once merged)   265 runs
   │               → 6.2813 · 4.8843 · 4.6635 · 4.5177 · 24.973; pooling refutes nothing
   ├─ round 65     band: 11 decisive points at reltol 1e-9    11 runs  → ε_num 5e-9 V … 2.7e-5 Hz
   └─ verdict tables (§3.1) → report → .spicy/checks/<rev>.json          579 runs, 0.4 s
```

### 7.2 The commands the agent calls

The CLI is the first agent's interface (agent_flows §1); an MCP wrapper later maps one tool to one command (agent_flows §4.1).

| Call | MVP command | Runs (CE amp) | Latency | Returns, with status |
|---|---|---|---|---|
| `engine.check` | `spicy check f.spl [--format json]` | 579 | 0.4 s | records; stored per `rev` |
| `engine.explain` | `spicy check f.spl --explain bias` | 0 if stored for this `rev`, else 579 | < 10 ms / 0.4 s | the record, flip table, main effects (`simulated`) |
| `engine.verify([p…])` | `spicy check --base ce_amp.spl cand1.spl cand2.spl …` | 534–579 each | 0.38 s each; 0.4 s total on parallel workers | records + a delta per candidate against the base (same / FIXED / BROKEN / changed: agent §3.1) |
| `engine.what_if(p)` | = `verify` in the MVP | same | same | records. Replays of stored worst points (FAIL-only, 6–8 ms, agent R6) return with the editor |
| `sim.run` | `spicy sim f.spl --at bias.max` (a stored counterexample), `--at nominal`, or `--at "temp=-10°C vcc.v=12.6V q1.beta=100"` (unlisted knobs at nominal) | 1 | ≈ 1 ms | node voltages, device regions, measures (`simulated`) |
| `sim.sweep` | `spicy sweep f.spl temp -10°C..60°C 5` | 5 | ≈ 5 ms | a table (`simulated`): "a sweep is not a verdict" |
| `engine.deep` | `spicy check f.spl --deep` | + each `next`'s cost | seconds | records |
| `lang.propose`, `lang.apply` | post-MVP (the language service). In the MVP the agent edits copies of the file and the user reviews the diff | — | — | — |

### 7.3 The flows on one page

LLM turns are the agent's tool-calling turns for the flow (agent §4.9). Engine times use the recommended worker **[S]** (`flows.py`, `deep.py`).

| Flow | Calls, in order | LLM turns | Engine time | Strongest claim the flow earns |
|---|---|---|---|---|
| Why does bias fail? | `check --explain bias` | 1 | < 10 ms (stored) | FAIL + counterexample + exact flip effects |
| Fix bias | `--explain` → write 3 candidate copies → `check --base … c1 c2 c3` → the user picks one → the user applies it | 3 | 3 × 0.38 s | "passes every spec on this edit (all corners)", per candidate |
| What-if C_in 2.2 µF | edit a copy → `check --base` | 2 | 0.4 s | the check's words for that edit |
| Add a spec | edit the contract → `check` | 2 | 0.4 s | "top_room passes at all corners" |
| Pin a 2N3904 | draft β 70..300 → `check` | 2 | 0.4 s | FAIL at `worst_case`; `sigma(3)` UNDECIDED (distribution) |
| Through an UNDECIDED (`pq`) | `check` → read `next` → `check --deep` | 2 | 0.3 s + ≈ 10 s | `sigma(3)` PASS (estimated; ball search) |
| Make the gain 10 | `sim --at nominal` → algebra → 2 candidates → `check --base` → 3 more → `check --base` | 4–5 | ≈ 1.2 s (agent §4.6) | candidate B′ meets gain and bass |
| Select-to-ask | `sweep temp` | 1 | ≈ 5 ms | simulated values + "≈" arithmetic |
| Sign-off review | `check --deep` (+ optional Monte Carlo) | 1–2 | 0.4 s (+ ≈ 11 s) | the records + what isn't checked |

### 7.4 The flows, in detail

```
 ─── "Why does bias fail?" ───────────────────────────────────────── 1 turn, 0 runs (stored for this rev)
 agent ─► spicy check ce_amp.spl --explain bias
          counterexample 6.5595 V; flip table (each neighbour is a stored corner):
            temp → 5.8578 (−0.702)  beta → 6.1395 (−0.420)  vcc → 6.2049 (−0.355)
            r1 → 6.4096  r2 → 6.4204  rc → 6.4375  re → 6.4502  c_in → no effect
          main effects over 256 corners (V per half-range): temp −0.333 · beta −0.185 · vcc +0.142
            · r1 +0.076 · r2 −0.073 · rc −0.064 · re +0.060 · c_in 0
 may say: "At sigma(3) bias passes (6.28 V). At worst case it fails at one corner (2 of 256 runs,
   either C_in), with every part at its edge; moving any single knob off its edge passes. Cold is the
   largest effect, and it's a range knob, so better parts won't remove it."
 may not say: that any fix works, before a check on the edited design.
```

```
 ─── "Fix bias" ─────────────────────────────── 3 turns: explain · write candidates · check --base
 agent ─► spicy check --base ce_amp.spl r2_10k2.spl r1_46k4.spl re_976.spl      534 runs each
   R2 10k → 10.2k   VC 4.5421 … 6.4218   gain 4.4639 … 4.7045   bass 26.40    all PASS
   R1 47k → 46.4k   VC 4.5930 … 6.4635   gain 4.4632 … 4.7039   bass 26.85    all PASS
   RE 1k → 976      VC 4.5343 … 6.4265   gain 4.5713 … 4.8184   bass 26.85    all PASS (gain max 0.012 from 4.83)
 may say, per candidate: "passes every spec on this edit (all corners)", with its smallest margin
   (R2 10.2k: 42 mV at VC ≥ 4.5; R1 46.4k: 37 mV at VC ≤ 6.5). The user accepts one diff and applies it.
```

```
 ─── Through an UNDECIDED: "is the dissipation spec OK?" (pq: RC 3.6k, P_Q ≤ 8.70831 mW) ─── 2 turns
 agent ─► spicy check pq.spl                                                  393 runs, 0.3 s
          worst_case  8.7097 mW FAIL, inside the box (β ≈ 199.5, found by the ascent)
          sigma(3)    8.6849 mW UNDECIDED (interior peak) · next: spicy check --deep (≈ 14,000 runs, ≈ 10 s)
 agent ─► spicy check pq.spl --deep                                           ball search, both spreads
          sigma(3)    8.6850 mW default, 8.7050 mW uniform → PASS (estimated; ball search)
 may say: "P_Q fails at worst case (8.71 mW, with β near nominal) and passes at sigma(3) by a ball search
   (8.685 mW; 8.705 mW with a uniform spread)." Before --deep it may only say "undecided at sigma(3)".
 Another kind of UNDECIDED has no automatic next: diffamp's |A_cm| at sigma(3) passes with a normal spread
 (1.81 mV/V) and fails with a uniform one (3.16 mV/V). The agent asks the user which spread the parts
 really have, or proposes matched resistors (§8.3).
```

The other six flows (what-if, add a spec, pin a part, gain 10, select-to-ask, sign-off review) are drawn in full in Appendix E; §7.3 summarizes them.

### 7.5 What the agent may claim

- Verdict words only from records of the current `rev`, and at most as strong as their `claim` (agent R1; agent_flows G3).
- A `simulated` value may prove a FAIL, never a PASS. Sampling (Monte Carlo, LHS) never supports a PASS.
- One spec verdict per sentence; name both confidences when they disagree, the spec's confidence first (agent §3.4).
- No fix is a fix before a check ran on it (agent_flows G2). No requirement change is a fix (G6).
- A lint on the agent's text checks these rules. It caught 3 of 3 planted mistakes in 18 answers (agent §3.4). It is post-MVP; its corpus becomes a test of `claim`.

---

## 8. Testing and acceptance

### 8.1 Answer keys on ngspice

- **`worst_case` key:** all 2^n corners, 1,000 seeded interior points, and a coordinate search from each side's best point, through the same backend and measures as the engine. Round 2's assertion A3 holds (guards report §4.2): if the engine beats the key, fix the key. It happened on tuned20: 24.425515 against the key's 24.425279 **[S]**; soundness §4 saw it too.
- **σ key:** per temperature/supply corner, sphere sampling plus a pattern search (§2.2). The ball search of §3.3 adds the ball's volume and moves temperature and supply inside their range. On the MVP, pq, tuned20 and diffamp it ended at a temperature/supply corner (−10 °C, 12.6 V) every time, so no σ worst sat at an interior temperature or supply **[S]**.
- **Where they live:** keys are computed by `#[ignore]` tests (1–60 s each) and stored as snapshot files; the fast tests compare against them.
- **Across ngspice builds:** keys are pinned to the backend identity in `rev`. Within one build, comparisons are exact. Across builds they use 1e-12 relative (minimal §8 q. 7): CI's packaged ngspice may be another version or use another linear solver. This build is compiled with KLU but reports SPARSE 1.3 in both batch and shared-library mode **[S]**, so the solver belongs in the backend identity too.

### 8.2 The adversarial suite, as decks in the repo

**7 circuits, 10 cases, 24 sides.** `mvp_xtb0` and `deadT` are the MVP again (another model, a broken netlist); `tuned20_b20` is tuned20 with another bound.

| Case | From | Trap | Sides (bound) | Truth (key) | In M3? |
|---|---|---|---|---|---|
| `mvp` (XTB 1.5 and 0) | roadmap §4.2; agent §2.7 | a model default flips a verdict; gain's cold/hot slope flip (walkthrough §10.4) | 5 | §2.2 | yes |
| `hiz` | round 2 guards (a); minimal, soundness, alternatives | a limiter off at nominal: 62/256 corners saturate | gain ≥ 8.39, ≤ 9.273; VC ≥ 1.197 | FAIL 3.994 / PASS / PASS | yes |
| `band` | round 2 (e); minimal, soundness | a second band edge; C_in's slope is 0 at 10 kHz | min\|H\| over 100 Hz–10 kHz ≥ 4.3354 | FAIL 4.3135 | yes (engine-side measure) |
| `pq` | round 2 (c) | an interior power maximum next to the worst corner | P_Q ≤ 8.70831 mW | FAIL 8.7097 (interior) | yes (device-quantity measure) |
| `tuned5`, `tuned20` | soundness §3.2 a | a resonance on the test frequency; a broad interior peak | gain ≤ 20 / ≤ 10 | FAIL 24.43 | yes |
| `tuned20_b20` | this plan | the σ search far below the σ truth (15.8 vs 24.0) | gain ≤ 20 | FAIL at both | yes |
| `deadT` | soundness §3.2 d | the netlist forgets `.temp` | 5 | binding error | yes |
| `diffamp` | soundness §3.2 c | a kink at nominal (\|A_cm\| = 0) | \|A_cm\| ≤ 3 mV/V | FAIL 3.6428 at a corner; σ: normal PASS 1.8088, uniform FAIL 3.1552 | yes (deck + hand-built contract) |
| `bench40` | alternatives §3.3 | 40 random CE designs × 6 sides × 8 bounds near the truth | 1,920 verdicts | keys per design | nightly |

**Not covered yet:** soundness's hiZ `f_low ≤ 35 Hz` (127 of 256 corners fail); `swing` (a `min()` kink, alternatives §3.2: with `min`/`max` in the language); `bjt_thd`, `opamp_thd`, `thd` (with the transient tier); the `cascade` K = 8/50 (scale §2.2: with the loop). The measures the MVP language can't write yet (band minimum, P_Q, \|A_cm\|) are built into the test harness's contracts.

### 8.3 Acceptance criteria for M3

1. **No false PASS** on the suite, at either confidence, scored against the `worst_case` and σ keys (soundness §11 rec. 1).
2. **`worst_case` values equal the key:** corners exactly (same backend, same measures); interior maxima within 1e-6 relative, or better than the key.
3. **σ values within 1e-4 relative of the σ key,** and the key inside [I, I + bracket] for every σ PASS (minimal §7 rec. 12).
4. **Verdicts equal the keys' truth wherever the engine decides.** UNDECIDED only where listed, each with its reason:
   - `pq` at `sigma(3)`: interior peak;
   - `tuned20_b20` at `sigma(3)`: search;
   - `diffamp` at `sigma(3)`: distribution;
   - `deadT`: binding.

   `diffamp`'s UNDECIDED is the right answer, not a miss. At `sigma(3)` its common-mode gain passes with a normal spread (1.8088 mV/V, equal to its σ key) and fails with a uniform one (3.1552 mV/V, a simulated board). engine.md §2.4 says two spreads that disagree give UNDECIDED. What would decide it is information, a declared distribution or a matched resistor network (lots, M6), not more simulation.
5. **Run counts pinned** per stage for every case (MVP: 1 / 256 / 46 / 265 / 11).
6. **Deterministic:** the same check twice gives identical output on one build.
7. **The backend-rule tests of §8.4 pass.** Tests that need ngspice skip when it's absent, and CI installs it (roadmap §3).

### 8.4 Backend-rule tests

One test per rule of §4.3, in the module it guards (`#[cfg(test)] mod tests`), for example:
- *destroy all:* 300 runs keep a flat per-run time.
- *reset undoes option temp:* at ngspice's default tolerances, VC is 5.50329 V after `option temp=60` + `reset`, and 5.18259 V after `alterparam` of the temperature parameter.
- *no DC solution:* the diode deck returns 999,999,999.99 V, and the run is `implausible`.
- *netlist error:* a bad deck kills the worker; the run is `failed`; the next run succeeds after a restart.
- *exact 1 kHz:* the generator emits `ac lin 1 1000 1000`, and the returned frequency is exactly 1000.
- *no `sens`:* the generator never emits it.
- *read-back:* every knob's read-back path exists at the nominal run; `deadT` stops after 1 run with UNDECIDED (binding).
- *cold and pure:* the nominal point at batch position 0 and 200 is bit-identical.
- *cross-check:* the worker and `ngspice -b` agree to 1e-13 on the MVP corners.
- *pooling filters by region:* a nudge outside the box never enters a `worst_case` bound, and a point outside the ball never enters a σ bound (soundness §3.2 c).

### 8.5 The prototype on the suite

`$S/suite.py` runs the engine of §2 and scores both confidences against the keys **[S]**:

| Case | Side | `worst_case` (key) | `worst_case` verdict | `sigma(3)` | `sigma(3)` verdict |
|---|---|---|---|---|---|
| mvp (XTB 1.5) | bias ≤ 6.5 | 6.559537 (= key) | FAIL | 6.281314; uniform 6.465673 | PASS (estimated) |
| | other 4 | = key | PASS (all corners) | as §2.6 | PASS (implied by worst case) |
| mvp (XTB 0) | all 5 | = key | PASS (all corners) | as §2.2 | PASS (implied by worst case) |
| hiz | gain ≥ 8.39 | 3.994366 (= key) | FAIL | 5.477478 (key 5.47744) | FAIL |
| | gain ≤ 9.273 · VC ≥ 1.197 | = key | PASS (all corners) | — | PASS (implied by worst case) |
| band | min\|H\| ≥ 4.3354 | 4.313479 (= key) | FAIL | 4.416931; uniform 4.346584 (keys 4.41682, 4.34658) | PASS (estimated) |
| pq | P_Q ≤ 8.70831 | 8.709679 (= key) | FAIL (ascent) | 8.684944; uniform 8.697768 not converged (truth 8.684978 / 8.705026) | UNDECIDED (interior peak) |
| tuned5 · tuned20 | gain ≤ 20 · ≤ 10 | 24.4255 (≥ key) | FAIL (ascent) | 22.917 · 15.827 | FAIL · FAIL |
| tuned20_b20 | gain ≤ 20 | 24.425515 | FAIL | 15.827 (truth 23.998) | UNDECIDED (search) |
| deadT | all 5 | — | UNDECIDED (binding), 1 run | — | UNDECIDED (binding) |
| diffamp | \|A_cm\| ≤ 3 mV/V | 3.642784 (= key) | FAIL | 1.808785; uniform 3.155184 | UNDECIDED (distribution) |

**24 sides: 0 false PASS and 0 wrong FAIL at both confidences.** The UNDECIDEDs are exactly the four listed in §8.3.

---

## 9. Milestones

### 9.1 M3, step by step

Each step ends in a review (🔍), as roadmap §8 describes. M3a–M3e run on the round-3 decks and hand-built contracts, so they don't wait for the language. M3f joins the two tracks.

| Step | What | Done when |
|---|---|---|
| **M3a** Design note | This plan's types (§6.2), the `Backend` trait, the two verdict tables, the report schema, and the **`EngineDeck` type** (§5.1) that M1f and M3b meet at. Creates `spicy_engine` and `spicy_backends` | Reviewed 🔍 |
| **M3b** ngspice backend | The worker (D-B), request encoding, read-back, plausibility, restart; the `ngspice -b` cross-check path | The §8.4 tests pass; the nominal CE amp gives VC 5.503227 V, \|H(1 kHz)\| 4.590771, f_low 20.126944 Hz (TNOM 25) 🔍 |
| **M3c** `worst_case` | Knob space, cones (the capacitor rule), measures, the run table with per-cone indexes, enumeration per side, tangent check + audit + pooling + ascent, band, rows W0–W7; the key harness | `worst_case` = key on every M3 case of §8.2; `deadT` stops; counts pinned 🔍 |
| **M3d** `sigma(3)` | The exact map, the 3σ-point search from two starts as state machines merged into rounds, cross-corners, range nudges, uniform, rows S0–S12; the ball search for `--deep`; the σ key harness | σ within 1e-4 of the σ key on `mvp` (both XTB); hiz FAIL; band's bracket covers; the §8.3 UNDECIDEDs exactly; counts pinned 🔍 |
| **M3e** Output and store | Records with `claim`, `next`, `any`, tags, `not_modeled`; the terminal table; `--format json` (unstable); `--explain`, `--base`, `--at`, `--deep`; `spicy sim`, `spicy sweep`; the per-`rev` store (`.spicy/checks/<rev>.json`) and `stale` | Snapshots of the MVP and every suite case, in table and JSON 🔍 |
| **M3f** End to end | `KnobTable` → knob space, `FlatContract` → plan, M1f's engine deck | `spicy check circuits/ce_amp.spl` prints §2.8's table, equal to the key; snapshot-tested 🔍 |

### 9.2 Dependencies

```
 language track (another session)                       engine track (this plan)
 §5.2 spicy_circuit: BjtModel xtb/xti/eg/tnom, Conditions  M3a design note + EngineDeck type
 §5.3 spicy_parser: those params, .temp, .options                 │
 M1d elaborate: FlatDesign, KnobTable, FlatContract        M3b backend ─► M3c worst_case ─► M3d sigma(3)
 M1e lower: default bench, DEFAULT Npn MODEL ◄─ D-A                                     ─► M3e output
 M1f export: numeric + engine deck (§5.4)                         │
            └──────────────────────────┬──────────────────────────┘
                                       ▼
                        M3f: spicy check circuits/ce_amp.spl
```

- **D-A** (the default model) must be decided before M1e's note is final. The engine track can build and test both settings meanwhile.
- **§5.2 and §5.3** come before M1f's round-trip done-when can hold. They are data-only, so they don't pull temperature physics (M2c) forward.

### 9.3 After M3

| Next | What | Gate |
|---|---|---|
| **The loop, for more knobs** (engine.md §5) | Flip walk over cones (C1–C5). Margin sides that run every analysis, spliced into every side whose cone holds the device (scale §5.4). The PASS allowance by measure kind: e₂ for declared-smooth measures, e_obs for kink-capable ones (alternatives §3.4). The audit's vertex half (soundness §5.3); the influence graph (scale §6.1); packing; the cross-revision cache | Agrees with enumeration on every regression circuit of ≤ 12 knobs per side (minimal §4.7); exact on the cascade to K = 50 (scale §5.2); then a CI cross-check on every enumerable case (alternatives rec. 6) |
| **Our simulator as a second backend** (M4, reversed) | The simulator follow-ups (roadmap; `pipeline.md` §11): reciprocity fix, errors instead of panics and silent non-convergence, exact AC grid, device records, M2c/M2d/M2e | The suite passes on it, and it cross-checks ngspice within the band (engine.md §6.4) |
| **Transient tier** | THD and other `tran` measures, on demand, headline confidence only; large-signal margins over the waveform (soundness §3.2 b); the analysis ladder | `bjt_thd`, `opamp_thd`, `thd` with no false PASS |
| **Statistics (M7)** | Board yield (from affine forms, D-F), importance sampling, Clopper–Pearson; `monte_carlo` as an async command | engine.md §4.4 |
| **Parts (M5)** | Part records; knob identity kept on pinning (agent R12); `parts.bench`; UNSPECIFIED for one-sided limits (agent R9); datasheet arithmetic (affine, D-F) | — |
| **Editor and agent** | MCP over the CLI commands; the claim lint; `carried`; stored-point replays for `what_if`; `verify` in parallel workers | agent §6 |

---

## 10. Decisions (accepted 2026-09-27)

### 10.1 The decisions, each accepted as recommended

**D-A. The default model of a bare `Npn`** (M1e's decision).
- *Options:*
  1. `IS=1e-14 BF={beta} XTB=1.5 XTI=3 EG=1.11`, TNOM 25 °C, all written out;
  2. the same with XTB = 0 (ngspice's default);
  3. ngspice's defaults (IS = 1e-16, XTB = 0, TNOM = 27 °C).
- *What it decides:* whether the engineer sees a `worst_case` FAIL beside the `sigma(3)` PASS. With (1), bias max at worst case is 6.5595 V, FAIL; with (2), 6.4572 V, PASS by 43 mV. The headline at `sigma(3)` is PASS under both (6.2813 V, 6.1839 V). Option (3) moves the operating point itself: IS is 100× smaller, VBE is 0.783 V, and nominal VC is 6.060 V, 0.44 V from the bias limit before any tolerance **[S]**.
- **Recommendation: (1).** The widely circulated Q2N3904 Gummel–Poon card has `Xtb=1.5 Xti=3 Eg=1.11` (the card agent §4.5 benched, `agent/part_bench.py`). The walkthrough's physics agrees: β +0.5 %/K, VBE ≈ 0.65 V at 1 mA, VC 5.50 V. Whatever is chosen is written into the export and the report (§1.3).

**D-B. The backend transport for M3.**
- *Options:* libngspice in a worker process; `ngspice -p` pipe worker; `ngspice -b` per round.
- *Evidence:* §4.1. 0.39–0.42 / 0.73–0.75 / 1.0–1.2 s per MVP check, with identical values. Isolation is the same (a separate process).
- **Recommendation: libngspice in a worker process,** with `ngspice -b` kept as the test cross-check. It is the fastest, it avoids 15-digit text round trips, and the editor needs it anyway. If you'd rather have no FFI in M3, `ngspice -b` costs about 0.6–0.8 s more per check, and nothing else.

**D-C. Round 2's D1–D10, as this plan answers them.** Approve as a block, or pick out any.

| # | Round 2 asked | This plan |
|---|---|---|
| D1, D2 | Nudges vs flips; the stop rule | For the loop, after M3 (round 2's answers stand) |
| D3 | Exact distribution map | **Yes** (σ within 4.1e-5 of the key) |
| D4 | σ per side | **Yes**, printed "per side" |
| D5 | Literal −3 dB below max\|H\| | **Yes** |
| D6 | Cold runs | Settled: ngspice's `reset` makes every run cold and pure |
| D7 | Both confidences always | **Yes, values on every side:** +166 runs, +0.12 s. `worst_case` still decides the `sigma(3)` verdict of a side it passes |
| D8 | Keep β 100..=300 | **Yes**; the 2N3904 goes to M5 |
| D9 | Function-backed test backend | **Dropped:** the suite runs as ngspice decks |
| D10 | Table + JSON | The record now; `--format json` unstable in M3e; a stable schema when the editor or MCP reads it |

**D-D. M3's scope: no loop.** A side whose cone exceeds the enumeration budget (about 12 knobs) gets UNDECIDED (budget) until the loop step. **Recommendation: yes.** The MVP's cones have 7 and 8 knobs, and the loop needs guards that enumeration doesn't (§10.2).

**D-E. The words.** PASS (all corners) / PASS (estimated) / PASS (implied by worst case); UNDECIDED reasons as in §3.1; statuses `simulated` and `stale`; no "verified". **Recommendation: yes.**

**D-F. No affine forms in M3.** engine.md D2 makes affine forms "the common format", roadmap M3a plans an `affine` module, and language §8.4 says a measure "is carried by the engine as an affine form". This plan changes that for M3.
- *What M3 records instead:*
  - every measure, derived ones included, evaluated per run from that run's values. `top_room = vcc.v − dc(output.v)` takes vcc.v and VC from the same run, so a shared knob can't be counted twice, which is the property affine forms were for. Over the corners it's exact (5.1951 V);
  - contributors as exact secants: the flip table at the counterexample, and main effects over the corners, both read from the run table;
  - each σ search's last slopes, kept in the record for later use.
- *Why:* the MVP has no proven remainders and combines no forms, so the form would carry only observed misses (minimal §1.2 C8). Stored forms are wrong for edits: −4.0 Hz (C11) and 42.9 Hz (alternatives §3.7) where the truth is 12.18 Hz. A nominal form pushed to the worst corner reads 29.98 Hz where the truth is 32.99 Hz (agent §5.3 #1).
- *When affine forms return:* hierarchy (a block's characterized results reused upstream with shared knobs kept correlated, engine.md §7), calibration (exact subtraction, §4.5), error budgets (§3.1), datasheet arithmetic with proven remainders (M5), board yield from stored forms (M7). Round 2's design (C8: a `Remainder` enum and a separate band) is the plan for then.
- **Recommendation: yes.**

### 10.2 The conflicts that shape M3

The full record of 20 resolved conflicts, with who said what, is in Appendix C. These five change what M3 builds:

| Question | Who said what | Resolution | Evidence |
|---|---|---|---|
| The loop on the MVP | minimal: defer past 12 knobs · alternatives: run it as a cross-check · soundness: primary, enumeration as self-check · scale: the core | Not in M3; its own step after, gated on agreement with enumeration, then a CI cross-check | Enumeration decided hiz's two sides the loop leaves UNDECIDED **[S]**; the walk without its margin side gave a false PASS (minimal §4.7) |
| Inside the box | minimal: nudge + one parabola step · soundness: tangent + 16-point audit · alternatives: interior check on power measures | Tangent check on every side + 8 LHS audit points + pooling, all feeding one ascent | Only the tangent catches pq; any one guard catches tuned; none catches neither (§2.5) **[S]** |
| σ guards | minimal: second start · soundness: margin guard over the ball + band-edge sub-searches · agent: refuse one-sided limits | Two starts always; range nudges; rules S7–S10; the ball search as escalation; sub-searches with `.band()`; the refusal in M5 | Skipping start B misses band's uniform extremum (4.379 vs 4.347) **[S]**; start B fixed hiZ and band (minimal §4.5) |
| Backend | agent: in-process · minimal: batch per process · scale: worker pool · soundness: worker process | libngspice in worker processes; `ngspice -b` as the cross-check | §4.1 **[S]**; soundness §2.2 #5 |
| Cones | scale: first-class now · minimal: defer | A `Cone` on every measure, per-side enumeration, per-cone indexes now (capacitor rule only); the influence graph and packing with the loop | The N² vs 22.6 N law is a loop-at-scale law (scale §5.2); `any`, the nudges and per-side routing need cones now |

---

## 11. Doc changes to apply after approval

| Doc | Change |
|---|---|
| `engine.md` → **v4** | §0 what changed; §5 becomes "enumeration + inside-the-box guards + the 3σ-point search", with the loop moved to a scale section and its guard list updated (C1–C5, splice, the allowance by measure kind); §3.1, §3.5 and D2 (affine forms deferred, D-F; contributors as flip table and main effects); §3.3 the verdict words; §6.3 ngspice as the first backend, L0 for good; §6.4 the cross-check; §8 when things run (§1.1); §9 decisions; §10 build order = §9 here |
| `walkthrough.md` | **Restructure §5 and §9**: they teach the loop as how the engine works. Show enumeration → interior check → `sigma(3)` search on the CE amp, and move the loop to a "how it scales" section. New numbers from the reference configuration (§5.5 6.59 → 6.5595 V; §6's 6.34 → 6.28 V; §9's report table); §2 "when the engine runs"; the appendix model (IS, XTB, TNOM). §8's affine headroom range isn't an enclosure (round 2 §8) |
| `roadmap.md` | M3 replaced by §9.1 (M3a without an `affine` module); M4 becomes "our simulator as a second backend"; §3 (ngspice) moves into M3 with the worker; M1e names decision D-A; M1f gains §5; §2.2's diagram gains `spicy_backends → spicy_engine`; decision rows for D-A to D-F |
| `language.md` | §8.4: a measure is evaluated per run (same no-double-counting property), and `Beyond` for a crossing below the band (C9); §10: verdict deltas carry the verdict words and `simulated`/`stale` |
| `specs.md` | §9 verdict list; §11 "when it runs" (no per-edit estimate) |
| `pipeline.md` | §3 `Conditions` becomes concrete (§5.2); §6(b) per-run measures, no affine form; §6(c) the ngspice path uses the engine deck (`.param` per knob), not a netlist per point; §11 points to this plan |

---

## 12. Open questions

1. **The region threshold on real models.** gmu > 1e-4·gm was checked only on a bare Gummel–Poon (minimal §8 q. 5). Vendor models with RB, RC, RE and quasi-saturation may need another rule.
2. **Small interior regions away from the worst corner.** Neither the tangent check, 8 audit points nor the σ rows reliably find one (§2.5). Should measures of known risk (resonance at a fixed frequency, phase margin vs C_load) declare "interior possible" and get a real search?
3. **More than two basins at `sigma(3)`.** Starts A and B cover two local extrema. A smooth measure with three is uncovered unless a device changes region.
4. **Should the S8 rule stay?** It has never changed a verdict (§2.6). It costs pessimism on sides like pq, where the 3σ worst is on the ball's surface; `--deep` settles those in ≈ 10 s. A case where it decides (a converged search below an interior σ peak) is still to be found.
5. **Multi-stable circuits on ngspice.** Every run is cold and bit-identical, so a second DC solution is never found (soundness §10). Structural positive-feedback detection, plus UNDECIDED (multi-stable)?
6. **Read-back inside vendor subcircuits:** is the flattened instance enough (soundness §12 q. 7)?
7. **The bracket rule rests on few results:** 12 σ results in minimal, 20 here. Re-check with op-amp and LDO circuits.
8. **The enumeration budget:** 4096 corners and 3 s per side, or purely a time budget?
9. **Should `--deep` escalations run automatically** when they cost under a second?

---

## Appendix A. Why the positions' run counts differ

The difference is the algorithm and the σ policy, not the model.

| Engine | `worst_case` | `sigma(3)` | Band | Total |
|---|---|---|---|---|
| scale §4: round 2's loop; σ on all sides | 65 | 91 | — | 156 |
| soundness §8: loop + tangent + 16-point audit; σ only where `worst_case` fails | 116 | ≈ 35 | 5 | 156 (204 with σ on all sides) |
| agent §2.4: round 2's loop; σ untuned (re-linearizes 6 knobs per round, 3 cross-corners), all sides | 58 | 165 | not counted | 223 |
| minimal §4.4: enumeration + interior; σ only for bias max, both spreads × 2 starts | 295 | 87 | 7 | 389 |
| **this plan: enumeration + tangent + audit; σ on all sides, 2 starts, range nudges; uniform where needed** | **303** | **265** | **11** | **579** |
| this plan with σ only where it decides | 303 | 103 | 7 | 413 |

## Appendix B. ngspice rules and traps: the evidence

| # | Behaviour of ngspice | Rule | Source |
|---|---|---|---|
| 1 | Plots accumulate: a run slows from 0.86 to 5.7 ms after 150 runs | `destroy all` after every run | agent §2.2; soundness §2.2 #6 |
| 2 | `option temp=…` and `option reltol=…` in control mode are silently undone by `reset` | `.temp {param}` + `alterparam`; `.options` in the deck. Re-checked at default tolerances: VC stays 5.50329 V after `option temp=60` + `reset`; `alterparam` to 60 °C gives 5.18259 V **[S]** | soundness #1; agent §2.2 |
| 3 | No DC solution: V = 999,999,999.99 V, return code 0, no message. Re-checked **[S]** | Plausibility check | soundness #4 |
| 4 | A netlist error kills libngspice ("cannot recover") | Worker process, restarted; the run is `failed` | soundness #5 |
| 5 | TNOM defaults to 27 °C | The exporter writes `tnom` | all five |
| 6 | `ac dec` accumulates `f *= Δ`: point 200 of `dec 50` is 1000.0000000000143 Hz **[S]** (`acan.c:366`) | `ac lin 1 f f` for every `at(f)` | soundness #3 |
| 7 | After a `sens` command, temperature changes are ignored in that instance; AC `.sens` returns ≈ 0; DC `.sens` gives one output per call | Never send `sens`; ngspice is L0 | scale §3.5, §6.4 |
| 8 | `meas` interpolates linearly: f_low 20.083–20.161 Hz vs 20.127 Hz | Our measure library only | soundness #7; C9 |
| 9 | One circuit per loaded library; loading a second unloads the first | One deck per worker | agent §2.2; scale §3.3 |
| 10 | `reset` re-parses the stored deck, so every run is cold and bit-identical at any batch position | D6 holds for free; tests compare exactly on one build | minimal §2.2 |
| 11 | Model parameters read back as `@model[param]`, only if the model's name differs from the instance's; vector parameters need an index | Read-back paths from the knob map; distinct model names | soundness #9; `t_engine_deck.cir` **[S]** |
| 12 | In batch text output, 15 digits need `set numdgt=15` | Cross-check path only | minimal §2.2 |
| 13 | XTB defaults to 0 (β has no tempco) | Default model written out in full (D-A) | agent §2.7 |

## Appendix C. All conflicts resolved

| # | Question | Who said what | Resolution | Evidence |
|---|---|---|---|---|
| 1 | Enumerate at MVP size? | all: yes, thresholds 256–4096 | Yes, per side: 2^\|cone\| ≤ 4096 and ≤ 3 s estimated. The agent's `audit` tier becomes the default path | §2.3; = key on every side **[S]** |
| 2 | The loop on the MVP | see §10.2 | Not in M3 | §10.2 |
| 3 | The loop's PASS allowance | round 2: e_obs · alternatives: e₂ · soundness: pinned e_obs + audit | Not needed in M3 (enumeration's allowance is 0 at corners); for the loop, by measure kind | alternatives §3.4: e₂ turns 81% of e_obs's UNDECIDEDs on true PASSes into PASS, but gives a false PASS on an undeclared kink |
| 4 | Cones from the MVP | see §10.2 | Cones, per-side routing, per-cone indexes now | §10.2 |
| 5 | Partial requests | scale: now (retrofit cost) · this plan: with packing | Open disagreement, stated in §6.3 | enumeration uses full points |
| 6 | Loops as state machines merged into rounds | scale: now | Taken for the σ searches | 65 → ≈ 19 rounds **[S]** |
| 7 | Inside the box | see §10.2 | Tangent + audit + pooling + ascent | §2.5 **[S]** |
| 8 | σ guards | see §10.2 | Two starts, range nudges, S7–S10, ball search | §2.6, §3.3 **[S]** |
| 9 | σ bracket | minimal: last jump · soundness: all jumps | Last jump, with convergence required and S8 | All-jumps turns a true PASS UNDECIDED (minimal §4.5); tuned20's σ search sits 8 below the truth **[S]** |
| 10 | Margin guards | soundness, alternatives: the DC margin is blind to clipping · scale: run every analysis, splice | None in M3 (enumeration sees every corner); region recorded per run; large-signal margins with the transient tier; splice with the loop | soundness §3.2 b; scale §5.4 |
| 11 | Statuses | agent: predicted/simulated/estimated/exhaustive/proven · round 2: verified/inner_bound/estimated/stale · minimal: verified/stale · scale: + carried | Verdict words carry the evidence (§3.2); `simulated`, `stale` now; `predicted`, `carried` later; no "verified" | agent §3.3, §5.3 #8 |
| 12 | UNDECIDED (binding) | soundness | Adopted, as a check-level stop after 1 run | `deadT` **[S]** |
| 13 | `claim`, `next` | agent | Adopted | agent §4.8: `next` settled the review's only open side in one call |
| 14 | JSON | agent: now · minimal: when it has a reader | The record now; `--format json` unstable; a schema later | The CLI agent reads the table; tests read the JSON |
| 15 | Where `--explain` reads from | agent R13: a per-`rev` store | Store report + run table per `rev` in M3e (≈ 0.25 MB) | §1.1 **[S]** |
| 16 | Backend | see §10.2 | libngspice worker | §4.1 |
| 17 | f_low | C9: secant with extra AC solves · scale, minimal: cubic interpolation | Cubic at 50 per decade; exact refinement only inside the band at a decisive point | minimal §2.2; scale §3.2 |
| 18 | D7 | minimal: σ only where needed · the other four: both | Both, values on every side | +166 runs, +0.12 s **[S]** |
| 19 | `what_if` | agent: replays as a screen · minimal, alternatives: a re-check | A re-check in the MVP; replays with the editor's per-keystroke budget | 0.4 s per re-check **[S]**; replays fit 16 ms only in process (agent §3.5) |
| 20 | Instant tier, D9, D6, transient tier | all agree | No instant tier; no function backend; cold runs free; transient on demand | §1.1; §9.3 |

## Appendix D. Size

Minimal estimated its version at about 1,850 Rust lines in ten modules, plus 400 lines of test harness (minimal §5.3). This plan adds the read-back, the ascent, the audit and pooling, the second σ start on every side, the ball search, the verdict records with `claim`/`next`, the per-`rev` store, and a worker binary instead of stdout parsing. That makes roughly 2,500–2,900 lines; it's an estimate, not a measurement.

## Appendix E. The other agent flows, in detail

Numbers **[S]** (`flows.py`) unless cited; the reference configuration throughout, except the boxed gain-10 flow.

```
 ─── What-if on an edit ──────────────────────────────────────── 2 turns, a re-check each: 0.4 s
 C_in 1 → 2.2 µF    bass 26.790 → 12.177 Hz; bias still FAIL at worst case, PASS at sigma(3)
 R1 47k → 51k       VC max 7.1502 V: bias FAIL at both confidences (alternatives §3.7: 7.150)
 There is no prediction step: the answer is a check, so it carries the check's words.
```

```
 ─── "Add a spec: top_room = vcc.v − dc(output.v) ≥ 5 V" ─────────── 2 turns, a re-check: 0.4 s
 Evaluated per run (D-F): vcc.v and VC come from the same run, so the shared knob can't be counted twice.
 Over the 256 corners: min 5.1951 V, exact over the corners.
 may say: "top_room passes at all corners: 5.1951 V ≥ 5 V."
```

```
 ─── "Pin a real 2N3904" (β 70..300; hFE ≥ 70 at 1 mA is the tested limit) ─── 2 turns, 576 runs
   bias max   worst_case 6.8017 V FAIL · sigma(3) 6.4779 V default, 6.6927 V uniform → UNDECIDED (distribution)
   gain min 4.4358, bass 27.782 Hz: PASS                       (XTB 0: 6.6663 V; 6.3493 / 6.5587 V)
 may say: "with the 2N3904's tested minimum, bias fails at worst case (6.80 V); at sigma(3) it depends
   on the β distribution", tagged relies-on-unreviewed-part-data. From M5 on, sigma(3) is UNSPECIFIED
   when the upper β edge isn't a tested limit (agent R9): 6.182 vs 6.540 V with two invented edges (agent §4.5).
```

```
 ─── "Make the gain 10 with the same bias" ─────────────────────── 4–5 turns (agent §4.6)
 ┌ agent's numbers, on its model (XTB 0, TNOM 27): not recomputed on this plan's reference ┐
 │ simulate_at nominal (IC 1.375 mA, hFE 200) → r_π ≈ 3.7 kΩ → RE_ac ≈ 449 Ω → E96 453 Ω  │
 │ check [A: values only, B: split RE with C_E]    A breaks bias (6.917 V) and bass (43.5 Hz) │
 │ check [B + C_in 2.2 µF, …]                      B′ meets gain 9.586 … 10.203, bass 16.07 Hz │
 └───────────────────────────────────────────────────────────────────────────────────────────┘
 The spec edit 4.6 → 10 is a requirement change, shown apart from the design hunks (agent_flows G6).
 A stored-slope prediction said bias 6.01 V where the truth is 6.92 V: the reason there is no prediction tier.
```

```
 ─── Select-to-ask: "why is the bias so temperature-sensitive?" ─── 1 turn, sweep: 5 runs
 agent ─► spicy sweep ce_amp.spl temp -10°C..60°C 5 → VC, VBE, IC per temperature (simulated)
 answer from the sweep and not_modeled only: with XTB 1.5, VC 5.830 V at −10 °C, 5.183 V at 60 °C.
 An answer from textbook physics the model lacks is the trap (agent §4.7).
```

```
 ─── Sign-off review: "is this design OK?" ─────────────── 1–2 turns: check --deep (+ Monte Carlo)
 records: bias PASS at sigma(3), FAIL at worst case; the rest PASS (all corners) · nothing UNDECIDED
 optional: Monte Carlo at (−10 °C, 12.6 V), 20,000 boards: q99.865 6.2731 V, 0 fail (≈ 11 s, agent §3.5).
   It may be quoted as "how often" only; it never supports a PASS (§3.2).
 may say: the records; "bias fails only with every part at its edge: your call" (walkthrough §5.6, §6);
   what isn't checked (aging, a real transistor, loads), naming other revisions as such.
```

## Appendix F. The synthesis scripts (`$S` = `/root/.claude/jobs/443154a8/tmp/engine2/synthesis/`)

| Script | What it computes | Section |
|---|---|---|
| `ngb.py` | ngspice batch backend (one `ngspice -b` per chunk), read-back, plausibility, the f_low and region measures (after minimal's `ng.py`) | §2, §4 |
| `ngl.py`, `ngp.py` | The same interface through libngspice in process (soundness's ctypes driver) and through `ngspice -p` | §4.1 |
| `cases.py` | The reference deck and the variants (from minimal's `variants.py`, soundness's `designs.py`, the diffamp port from the review's `rr_diffamp.py`) | §2.1, §8.2 |
| `key.py`, `sigma_key.py`, `nominal.py`, `deep.py` | Answer keys, σ keys + Monte Carlo, the model-setting table, the ball search | §2.1, §2.2, §3.3 |
| `canon.py` (`canon_v1.py`: the first draft) | The engine of §2 and §3 | §2, §3 |
| `ablation.py`, `row7_scan.py`, `sigma_interior.py` | Inside-the-box guards; the S8 rule's scan | §2.5, §2.6 |
| `timing.py`, `flows.py`, `suite.py` | Transport timing, agent flows, the scored suite | §2.8, §7, §8.5 |
| `t_engine_deck.cir`, `t_nosol.cir`, `t_opttemp.cir`, `t_dec.cir`, `t_readback.cir` | The engine deck; re-checks of ngspice traps | §1.4, §4.3 |
