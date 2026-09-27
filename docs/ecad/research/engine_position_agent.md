# Position: Agent First — the Engine Is the Agent's Instrument, and the Engineer's

> Round 3 · 2026-09-27 · One of five position papers. It builds on round 2 (`engine_synthesis.md` and its six reports, mainly `agent_flows.md`) and makes the agent's side concrete on ngspice.
> **Where the numbers come from.** Every circuit number was computed on ngspice-42 (libngspice, in-process, through ctypes) by the scripts in the scratch folder `/root/.claude/jobs/443154a8/tmp/engine2/agent/` (listed in the Appendix). Round-2 numbers are cited by report and section. Source citations are `file:line` in `/tmp/refs/ngspice`. That scratch folder goes away with the job; copy the scripts into the repo if they should stay reproducible.
> **Not measured here:** how long an LLM takes per turn. Where a conclusion needs it, I assume only that one turn takes at least about a second.

---

## Summary

The engine has two readers: the engineer, and an AI agent working for the engineer. This position says the agent is the harder reader, so the engine should be designed for it: every verdict record carries the strongest sentence the engine allows (`claim`), what would settle it (`next`), a status (`predicted`, `simulated`, `estimated`, `exhaustive`, `proven`), and the design revision it belongs to. I built a small engine on ngspice (the round-2 loop with flips, the k-σ point with the exact map, the verdict table) and an agent API over it (check, explain, propose, what_if, verify, simulate_at, sweep, audit). Then I played the agent through eight flows. On the CE amp a full check with both confidences takes **223 runs and 0.13 s in-process** (0.18 s with everything `explain` needs stored), and its `worst_case` answers equal the 256-corner answer key on all five spec sides. So on this circuit a full re-check costs less than one LLM turn, and **cost should be counted in agent turns, not simulations**: `verify` should be the agent's default, and the instant "what-if" is an editor feature. Three findings matter most. **(1)** The MVP's most interesting verdict depends on a model default nobody wrote down. With ngspice's generic transistor (XTB = 0, so β doesn't change with temperature), bias max at worst case is 6.486 V: it passes at all 256 corners, by 14 mV. With XTB = 1.5 it fails at 6.594 V. The engine must list what the model leaves out. **(2)** Predictions mislead, and re-running the stored worst points doesn't. For "make the gain 10" by values alone, the log-unit prediction says bias max is 6.01 V; the truth is **6.92 V, a FAIL**. Re-running the stored worst point proves that failure in 6 ms. **(3)** Pinning a real 2N3904 shows that `sigma(3)` can come from a distribution nobody specified: bias at 3σ is FAIL (6.540 V) or PASS (6.182 V) depending on an upper β edge the agent has to invent. The worst-case FAIL (6.810 V) holds either way. A small claim lint, run on 18 agent answers, caught all three planted mistakes and passed all 15 honest answers. Its negation rule also let one cross-revision claim through.

---

## 1. The position

### 1.1 In one paragraph

The engine is an **instrument**: something you ask a question and get back a reading you can defend. The engineer reads the terminal table. The agent reads JSON, then writes sentences that the engineer will trust as much as the table. An LLM is fluent, has read a lot of textbooks, and is bad at hedging. So left alone it rounds UNDECIDED to "passes", quotes a prediction as a result, explains a failure from textbook physics the model doesn't contain, and names the part in a counterexample that had nothing to do with it. I saw each of these in the flows below. The position has three parts:
1. **The engine writes the verdict sentence.** It's deterministic and snapshot-tested. The agent may paraphrase it, never make it stronger.
2. **Every value says how it was obtained,** using a small fixed set of statuses, each with the words it allows.
3. **Latency is measured in agent turns.** A tool call costs one LLM turn, which takes seconds. On the CE amp, ngspice answers a full re-check in 0.2 s. So the engine should answer in few, complete calls, and prefer a real re-check over a cheap guess.

### 1.2 Three measurements that shape it

| Measurement | Number | Consequence for the design |
|---|---|---|
| Full check, both confidences, on libngspice in-process | 223 runs, **0.13 s** (0.18 s through the API, with the forms `explain` needs); 0.46 ms per run | `verify` costs less than an LLM turn. The agent should verify every candidate |
| The same verdict, two transistor temperature models | bias max `worst_case` **6.486 V** (XTB = 0) vs **6.594 V** (XTB = 1.5) | The engine must emit `not_modeled` and `coverage`, generated from the model card, not left to the agent |
| Log-unit prediction vs re-run of the stored worst point, "gain 10 by values" | predicted **6.01 V**, re-run **6.92 V** (FAIL; the full check agrees) | `what_if` returns re-runs, which can prove a FAIL. Predictions are labelled and suppressed where they can't hold |

### 1.3 Round 2, from this position: keep, cut, change

| Item | Verdict | Reason from the agent's side (numbers from §2 and §4) |
|---|---|---|
| **C1** stop on optimality, accept only improvements | **Keep** | Every record's value is a real simulated point, so every FAIL the agent reports is definite |
| **C2** flips at vertices | **Keep** | The one-flip certificate becomes a field (`one_flip_certificate`) the agent can cite. On ngspice: all 5 extremes exact in 43 runs after the shared round |
| **C3** saturation-margin guard | **Keep** | Cheap (0 runs on the CE amp: predicted VCE − 0.2 V ≥ 3.12 V over the box). Its line value is `predicted`, so the agent may not quote it as a fact |
| **C4** pooled inner bound, final cross-check | **Keep** | Free |
| **C5** below-noise knobs go to an edge | **Keep, and add:** counterexamples mark structural zeros as `any` | Otherwise the bias counterexample lists `c_in = 1.2 µF`, and an LLM attributes the bias failure to C_in (§5.3 #3) |
| **C6** PASS needs margin > e_obs + ε_num | **Keep the rule, change what UNDECIDED returns** | On ngspice the MVP's key side is UNDECIDED (margin 13.7 mV, e_obs 126 mV) although all 256 corners pass. UNDECIDED must carry a `next` the agent can take in one call: the corner `audit` (256 runs, 0.12 s) |
| **C7** exact map, decide from the k-σ point, per side | **Keep, and add a refusal:** `sigma(k)` is UNSPECIFIED when a statistical knob has no two-sided tested limit | 2N3904: bias at 3σ flips FAIL 6.540 V ↔ PASS 6.182 V with the agent's arbitrary upper β edge (§4.5) |
| **C8** three kinds of error kept apart | **Keep** | The API's statuses mirror it |
| **C9** measures defined exactly | **Keep; confirmed on ngspice** | ngspice's AC sweep accumulates `freq *= delta` (`acan.c:366`), and its `meas` gives 20.083 Hz (half power, linear interpolation) where our literal −3 dB gives 20.127 Hz |
| **C10** engine-grade tolerances | **Keep the options; no longer a blocker** | On ngspice, default and engine tolerances give the same nudged slopes to 4–5 digits, and the differences between step sizes shrink tenfold with h (curvature, not noise). reltol 1e-6 and 1e-9 give bit-identical results; the default 1e-3 is 32 µV off (§2.2) |
| **C11** instant re-evaluation → log prediction + re-run | **Change:** the instant tier is for the editor, not the agent | `verify` is 0.13–0.24 s. Predictions were off by 0.9 V (gain-10, option A) and by 25% (a part-kind change, fix-bass B). The agent gets re-runs; predictions only as `predicted`, and never after a kind or topology change |
| **C12** content-addressed cache, rounds | **Keep** | "Add a spec" replayed 641 cached solves and ran 205 new ones; `simulate_at` on any counterexample costs 0 runs |
| **D1**, **D2**, **D3**, **D4**, **D5** | **Keep** | — |
| **D6** cold Newton | **Moot on ngspice** | Every ngspice `op` and `ac` starts cold (`MODEINITJCT`: `dcop.c:65,77`, `acan.c:133-137`) |
| **D7** both confidences always | **Keep, strongly** | The lint's most common catch is a headline-only PASS that hides an UNDECIDED or FAIL at the other confidence |
| **D8** keep β 100..=300 | **Keep for the MVP, but record why** | On ngspice the verdict hinges on XTB; a real 2N3904 card gives hFE 132 at 1 mA and can't exceed 188 there (§4.5) |
| **D9** test backend first | **Moot** | ngspice is decided |
| **D10** table + JSON | **Keep and extend** | Add `status`, `claim`, `next`, `coverage`, `not_modeled`, `any`, `rev` (§6); plus a compact text form, 4.4× smaller |
| `agent_flows.md` statuses (`verified`, `inner_bound`, `estimated`, `stale`) | **Change the names** | "Verified" reads as "proven" to an LLM. Use `estimated` (engine.md's own "PASS (estimated)"), and add `simulated`, `exhaustive`, `predicted` (§3.3) |
| `agent_flows.md` `what_if` P1/P2/P3 | **Cut P1; keep P2 only as `predicted`; P3 always** | P1 goes negative (−6.68 Hz in round 2). P2 missed by 0.9 V here. P3 found every failure it could see |
| `agent_flows.md` contributors = recovery | **Keep, with a stated reference point per range knob** | Clear on ngspice: C_in 5.69 Hz, β 0.10 Hz at the aged 3σ point. But `life` to its midpoint recovers only 3.44 Hz where a new capacitor recovers 6.19 Hz (§4.1) |

---

## 2. The prototype on ngspice

### 2.1 What was built

```
 designs.py      the CE amp as a knob table + a SPICE template in which every value is a .param
      │          (MVP: 8 knobs; aged: + life; the edits the flows propose)
      ▼
 ngback.py       libngspice via ctypes: load once · per run: destroy all · alterparam × N · reset ·
      │          op (+ @q1[ic], @q1[ib]) · ac lin 1 1k 1k · ac dec 20 1 100k · f_low refinement
      │          + our measurement library (f_low: literal −3 dB, bracket + secant in ln f)
      ▼
 engine.py       Runner: content-addressed cache (op key skips capacitors) ·
      │          shared round · worst_case vertex walk with flips · sigma(3) k-σ point (exact map) ·
      │          band re-runs at reltol 1e-9 · verdict table (synthesis §4.4)
      ▼
 api.py          check · explain · propose · what_if · verify · simulate_at · sweep · audit · snap
      │          JSON "spicy.agent/0", statuses, claim + next, coverage, not_modeled, compact text
      ▼
 flow_*.py       the eight flows, played as the agent      lint.py   the claim lint
 answer_key.py   256/512 corners + interior samples; Monte Carlo (truncated normal)
```

About 1,300 lines of Python in the core modules, stdlib only.

### 2.2 ngspice facts I verified, and the traps among them

| Question | Answer | How checked |
|---|---|---|
| Does `.temp {temp_c}` follow `alterparam temp_c=…` + `reset`? | **Yes** (and `.options temp={temp_c}` too). VC 5.5382 / 5.2669 / 5.8057 V at 25 / 60 / −10 °C | `t_temp.py` |
| Does the `option temp=60` command work? | **Only until the next `reset`, which silently undoes it.** After `reset` the run is back at 25 °C with no message | `t_temp.py`: "option temp=60 + reset" gives the 25 °C VC |
| Does libngspice slow down over many runs? | **Yes, unless you `destroy all`.** Every analysis keeps a plot. A full run goes from 0.86 ms to 5.7 ms after 150 runs; with `destroy all` it stays at 0.44 ms | `t_timing2.py` |
| Does `ac` reuse the last operating point? | No: each `ac` solves its own operating point from a cold start (`acan.c:133-137`, `MODEINITJCT`). Every run is a pure function of its parameters, so D6 is automatic | source |
| Is 1 kHz on a `dec` grid exactly 1 kHz? | No: `freq *= ACfreqDelta` accumulates (`acan.c:366`); point 150 of `dec 50` is 1000.0000000000106 Hz. I use `ac lin 1 1000 1000` for `at(1kHz)` | `t_timing.py` |
| Complex AC vectors through `ngGet_Vec_Info`? | Yes: `v_compdata` points to `{re, im}` pairs (`sharedspice.h:176-183`) | `t_timing.py` |
| Numerical band | reltol 1e-6 and 1e-9 give **bit-identical** results; the default reltol 1e-3 is 32 µV off in VC at the worst corner | `engine.py` band re-runs; direct comparison |
| Slopes by nudging | dVC/dε_β = −0.13959 / −0.14019 / −0.14026 V at h = 1e-2 / 1e-3 / 1e-4: the differences shrink tenfold with h (curvature, not noise). Default vs engine tolerances at h = 1e-3: −0.140194 vs −0.140188 | `t_noise.py` |
| Nominal operating point | VC 5.538 V, IC 1.375 mA, hFE 200, \|H(1 kHz)\| 4.5903, f_low 20.127 Hz (TNOM is ngspice's default 27 °C) | `flow_f0.py` |
| One circuit at a time | Loading the 2N3904 bench unloaded the amplifier: "no vector _ic" until the bench was made to reload and invalidate the engine's circuit | `part_bench.py` |

The last row is roadmap §3's "global state (one circuit per loaded library)", met in practice. Any tool that needs its own circuit (a part bench, a second design) must go through the backend that owns the instance, or run in its own process.

### 2.3 Cost per run: in-process vs subprocess

| Transport | Operation | Median |
|---|---|---|
| libngspice in-process | load the netlist | 2.0 ms |
| | `destroy all` + 8 × `alterparam` + `reset` | 0.051 ms |
| | + `op` (3 nodes) | 0.077 ms |
| | + `op` + two device quantities (`@q1[ic]`, `@q1[ib]`) | 0.088 ms |
| | + one AC point at exactly 1 kHz | 0.083 ms |
| | + 101-point AC sweep | 0.210 ms |
| | **one full engine run:** op + devices + 1 kHz + sweep + f_low refinement (3–5 points) | **0.458 ms** |
| `ngspice -b` per run | op + 1 kHz + sweep, printed (no f_low refinement) | **5.0 ms** serial |
| | 256 corners on 1 / 20 worker processes | 1.29 s / **0.13 s** (0.51 ms per run effective) |

In-process is about 10× cheaper per run, and it's the only transport that fits the editor's one-frame budget (§3.5). For the agent both are fine on the CE amp.

### 2.4 The cold check, round by round

```
 spicy check circuits/ce_amp.spl --format json
   │  front-end (M1f): SPICE deck, every knob a .param, ".temp {temp_c}", engine options
   ▼
 libngspice ── load once (2 ms) ── one run = 0.46 ms
   │
   ├─ SHARED ROUND ............................................. 15 points
   │    nominal · 8 nudges (h = 1e-3) · 4 range corners (temp × vcc) · 2 interior temperatures
   │    saturation margin: its line predicts VCE − 0.2 V ≥ 3.12 V over the box → no extra run
   │
   ├─ worst_case, 5 sides ....................................... 43 points
   │    jump to the nominal line's vertex → flip each knob → accept only improvements →
   │    stop when no single flip improves (a certificate on every side)
   │
   ├─ sigma(3), 5 sides ......................................... 165 points
   │    at the worst range corner: the k-σ point in u-space, exact truncated-normal map,
   │    slopes in ε then chain rule; cross-checked at the other 3 range corners
   │
   ├─ band: 10 decisive points re-run at reltol 1e-9 → bit-identical
   │
   └─ for explain / what_if: the form at every record's worst point (≈ 80 nudged points)
   ▼
 10 records, each with status, claim, next · coverage · not_modeled
 engine 223 points, 130 ms · through the API 178 ms (one core, serial)
```

| Part of the check | Distinct points | Wall time |
|---|---|---|
| `worst_case` only (shared round + walk) | 58 | 40 ms |
| `sigma(3)` only | 180 | 103 ms |
| both | 223 | 130 ms |
| both, through the API (band re-runs + forms at the worst points) | — | 178 ms |
| **answer key:** all 256 corners | 256 | 118 ms |

At 8 knobs the loop's `worst_case` is 4.4× cheaper than brute force (58 vs 256). Both confidences together cost about as much as brute force alone. My `sigma(3)` spends 25–52 new runs per side, against round 2's 8–22 (synthesis C7). My loop re-linearizes all six statistical knobs every iteration and cross-checks three range corners; I didn't tune it, because on ngspice it doesn't matter for latency.

### 2.5 Results against the answer key (MVP, 8 knobs)

| Spec side | `worst_case` (engine) | Answer key: 256 corners | Best of 500 interior points | Verdict | `sigma(3)` (engine) | Monte Carlo, 20,000 boards, 99.865% quantile | Verdict |
|---|---|---|---|---|---|---|---|
| bias ≤ 6.5 V | **6.48628** | 6.48628 | 6.180 | **UNDECIDED (bracket)** | 6.2136 | 6.2138 | PASS (est.) |
| bias ≥ 4.5 V | 4.76300 | 4.76300 | 5.051 | PASS (est.) | 4.9588 | 4.9618 | PASS (est.) |
| gain ≤ 4.83 | 4.70612 | 4.70612 | 4.682 | PASS (est.) | 4.6681 | 4.6653 | PASS (est.) |
| gain ≥ 4.37 | 4.45845 | 4.45845 | 4.485 | PASS (est.) | 4.5125 | 4.5127 | PASS (est.) |
| bass ≤ 30 Hz | 26.3908 | 26.3908 | 26.037 | PASS (est.) | 24.759 | 24.729 | PASS (est.) |

- **`worst_case`:** the engine lands exactly on the corner answer key on every side, and no interior sample beats a corner.
- **`sigma(3)`:** the engine's k-σ values agree with the Monte Carlo quantile within its own scatter. Two 20,000-board runs with different seeds give 6.2138 and 6.2085 V for bias max.
- **Different from round 2.** Round 2's model gave bias max 6.5905 V at worst case (FAIL, synthesis §2). ngspice's generic model gives 6.4863 V. §2.7 explains why.
- **The one UNDECIDED.** bias max is 13.7 mV inside the limit, and the nominal line missed by 126 mV at that corner, so rule 7 of the verdict table applies. The corner audit settles it for corners: 256 corners in 0.12 s, worst 6.4863 V → "PASS (all corners)" (§4.8).

### 2.6 The aged variant (9 knobs: + life, C_in × (1 − 0.2·life))

| Spec side | `worst_case` | Answer key (512 corners) | `sigma(3)` | Monte Carlo at the worst corner (20,000 boards) |
|---|---|---|---|---|
| bass ≤ 30 Hz | **32.988 Hz, FAIL** | 32.988 Hz | **30.95 Hz, FAIL** | 99.865% quantile 30.91 Hz; 128 of 20,000 boards fail (0.64%) |
| the other 4 sides | as the MVP (gain min 4.4576) | equal on all 4 | as the MVP | — |

No interior sample (500) beats a corner on any side.

The check costs 273 points and 158 ms (212 ms through the API). Round 2's model gave 33.42 Hz (`agent_flows.md` §1) and 31.15 Hz (synthesis C7). Same verdicts; ngspice's transistor differs.

### 2.7 The MVP's verdict hinges on a model default

The generic `.model QNPN NPN(IS=1e-14 BF={beta})` leaves XTB at its default of 0, so β doesn't change with temperature. The walkthrough's model had β rising with temperature. Same design, same knobs, only XTB changed (`run_xtb.py`):

| | XTB = 0 (ngspice default) | XTB = 1.5 |
|---|---|---|
| hFE at −10 °C / 60 °C (nominal parts) | 200 / 200 | 164.2 / 233.9 |
| VC at −10 °C / 60 °C | 5.8057 / 5.2668 V | 5.8639 / 5.2243 V |
| bias max, `worst_case` | 6.4863 V → **UNDECIDED** (passes at all corners, by 13.7 mV) | **6.5940 V → FAIL** |
| bias max, `sigma(3)` | 6.2136 V, PASS | 6.3163 V, PASS |

So the most interesting verdict in the MVP rests on something nobody wrote into `ce_amp.spl`. An agent that says "bias passes at worst case" without "given a transistor model whose β has no temperature coefficient" is misleading the engineer. The engine can generate that caveat mechanically: it reads the model card, finds the temperature parameters left at their defaults (XTB = 0; resistors with no TC1/TC2), and lists them in `not_modeled`. This is engine.md §6.6's coverage map, applied to missing links between knobs rather than missing responses. A zero slope can't reveal this one: temp and β are separate knobs, and what's missing is the link between them.

---

## 3. The agent's instrument: API and contract

### 3.1 The calls

Namespaced by owner as in `agent_flows.md` §4.1. The `engine.*` calls are the ones built here. The rest (`lang.*`, `parts.*`) are as round 2 specified.

| Call | Input | Returns | Status of what it returns | CE amp cost, in-process |
|---|---|---|---|---|
| `engine.check` | `rev`, confidences | 10 records (claim, next, status), nominal, coverage, not_modeled, cost | `estimated` | 178 ms (MVP), 212 ms (aged) |
| `engine.explain` | record id | the record, form at the worst point, the nominal form's miss there, **recovery contributors** | slopes `simulated`; contributors `simulated` | 4–5 ms, 13–16 new solves (0 if asked again) |
| `lang.propose` | base rev, edits | proposal id, new rev, knobs moved (in knob widths), added, removed, topology changed?, requirement change? | — | instant |
| `engine.what_if` | proposal | per record: `replay` (stored worst point run on the new design; `proves: FAIL` if it violates), `predicted` or `not_covered`, the new design's nominal, trust | `simulated` + `predicted` | 6–8 ms (10 points) |
| `engine.verify` | proposal (**a list, recommended**) | a full check on the proposal + a delta against the base (same / FIXED / BROKEN / changed / new) | `estimated` | 65–237 ms each |
| `engine.simulate_at` | point | nodes, q1 (IC, hFE, VBE, VCE, region), every measure | `simulated` | 0 ms if cached, else 0.5 ms |
| `engine.sweep` | knob, values, `at` | a table | `simulated` ("a sweep is not a verdict") | 1.2 ms for 5 temperatures |
| `engine.audit` | rev, records | the worst corner of all 2^N, and whether the search missed it | `exhaustive` | 120 ms (256 corners), 541 ms (1024) |
| `parts.snap` | value, series | nearest series values | — | instant |
| *missing:* `parts.bench` | model card, test condition | e.g. hFE at IC = 1 mA, VCE = 1 V | `simulated` | 35 ms for a BF search (§4.5) |
| *missing:* `engine.monte_carlo` | rev, corner, n | quantile, failures, Clopper–Pearson bound | statistical | 11 s per range corner (20,000 boards): minutes class, async |

### 3.2 A record, as the agent receives it

The real output for the MVP's one interesting side (`out_check_mvp.json`):

```json
{
 "id": "bias/max/worst_case", "spec": "bias", "side": "max", "confidence": "worst_case",
 "headline": false, "measure": "dc(output.v)", "limit": 6.5, "unit": "V",
 "status": "estimated", "verdict": "UNDECIDED (bracket)",
 "value": 6.48628, "margin": 0.01372, "bracket": [6.48628, 6.61205], "e_obs": 0.126,
 "counterexample": { "temp": "-10.0°C", "vcc.v": "12.6V", "r1.value": "47.47kΩ", "r2.value": "9.9kΩ",
                     "rc.value": "4.653kΩ", "re.value": "1.01kΩ", "c_in.value": "any", "q1.beta": "100.0" },
 "region": "active", "rev": "036a1c",
 "method": { "name": "vertex walk + flips", "rounds": 2, "one_flip_certificate": true, "new_points": 9 },
 "tags": [],
 "claim": "bias max is UNDECIDED (every part at its worst edge): worst found 6.486V is 13.72mV inside 6.5V, but the search's line missed by up to 125.8mV. Do not call it a pass.",
 "next": { "call": "audit", "why": "simulate every corner of the box", "runs": 256, "est_ms": 118 }
}
```

Three fields are new compared with round 2's record (`agent_flows.md` §4.2), and each one answers a failure I saw:
- **`claim`**: the engine's own sentence. The terminal table prints the same template, so it is snapshot-tested once for both readers.
- **`next`**: for UNDECIDED, the call that would settle it, with its cost. Without it the agent can only say "not decided". With it, it settles the question in one more call (§4.8).
- **`"any"`**: capacitors can't move a DC measure. Without the marker, `c_in.value: "1.2µF"` appears in the bias counterexample, and an LLM reading it attributes the failure to C_in.

### 3.3 The honesty model: statuses, and what each lets the agent say

A closed set. Every number the API returns carries one of these statuses.

| Status | Produced by | What it is | Proves | The agent may say | The agent must not say |
|---|---|---|---|---|---|
| `predicted` | `what_if` (log-unit form at a stored worst point) | No simulation of this revision at this point | nothing | "≈ 14.99 Hz (predicted, not simulated)" | pass, fail, fixed, ✓, ✗ |
| `simulated` | `simulate_at`, `sweep`, `what_if` replays, any run | One real run of this revision at a stated point | **FAIL**, if it violates the limit | "at −10 °C, 12.6 V … VC is 6.742 V"; if over the limit: "fails: a reachable board gives 6.742 V" | pass, meets, OK |
| `estimated` | `check`, `verify` | The search's extreme over the whole box, plus guards and the verdict table | PASS (estimated), FAIL, UNDECIDED | "passes, estimated: worst found 22.22 Hz, 7.8 Hz inside"; "undecided: … would be settled by …" | guaranteed, proven, always, verified |
| `exhaustive` | `audit` | Every corner of the box simulated | PASS (all corners): complete over corners, not over the interior | "at all 256 corners the worst is 6.486 V" | guaranteed |
| `proven` | exact methods (datasheet arithmetic, corner theorem): post-MVP | A guaranteed bound | PASS (guaranteed) | "guaranteed" | — |
| `stale` (modifier) | any record whose `rev` ≠ the current revision | A fact about another design | nothing about this one | "on the previous revision, …" | anything present-tense |

**Tags** qualify the words, never remove them:
- `model-conditional`: "as far as the model can tell: its β has no temperature coefficient".
- `relies-on-unreviewed-part-data`: "given hFE ≥ 70 from 2N3903/D p. 2, extracted by AI and not yet reviewed".
- `distribution: …`: which distribution the σ figure assumes.

**Spec-level rules:**
- A sentence like "bias passes" needs every side at the headline confidence to pass.
- If the other confidence disagrees, the sentence must say so. That's walkthrough §6's "engineer's call".

**Why not "verified".** Round 2 used `verified` for "the loop ran on this revision" (`agent_flows.md` §2). An LLM reads "verified" as "proven" and will write "verified to pass", which the engineer reads as a guarantee. The loop's PASS is an estimate by the engine's own definition (engine.md §3.3), so the status should say so.

### 3.4 The claim lint

A deterministic check on the agent's message before it's shown, as `agent_flows.md` G3 proposed. It's made concrete in `lint.py` (68 lines). Sentence by sentence:
- a PASS word for a spec needs a fresh `estimated` or `exhaustive` PASS record at the confidence named, or at the headline confidence plus a mention of any disagreement;
- a FAIL word needs a FAIL record or a proven re-run;
- every quantity with a unit must appear in some tool result (rounding allowed), or be marked "≈" as the agent's own arithmetic.

Results on 18 answers (`flow_transcripts.py`):

| Answer | Lint |
|---|---|
| The 15 answers quoted in §4 | pass |
| "The design passes all specs." (MVP) | caught: bias max at `worst_case` is UNDECIDED |
| "Option C fixes bass." (after `what_if` only) | caught: the current revision's bass records are FAIL; nothing verified C |
| "C breaks bias at worst case: 6.81 V." | caught: 6.81 V appears in no tool result (the record says 6.742 V) |

Two phrasing rules were needed, and both go into the agent's instructions:
- **One spec verdict per sentence.** "C passes bass, but breaks bias" confused the sentence-level check, so I split it.
- **Name device quantities precisely.** "The transistor's gain is 62" matched the spec named `gain`; "hFE is 62" doesn't.

What the lint can't see. Its negation rule skips any sentence containing "not". So the review's "Not checked: … bass fails at 32.99 Hz" passed, although that FAIL belongs to another revision (the aged variant). Cross-revision claims need the revision named in the sentence and a lint that looks it up. A lint doesn't make the agent honest. It stops the most common slips before the engineer reads them.

### 3.5 Latency budget per interaction

| Interaction | Budget, and why | CE amp in-process (measured) | CE amp, subprocess | 50 knobs (estimate) |
|---|---|---|---|---|
| Editor keystroke (T0): `what_if` replay | one frame, 16 ms (`engine_flows.md` §5.1) | **6–8 ms** ✓ | 10 runs × 5 ms = 50 ms serial ✗; about 5 ms on 20 workers | 40 stored points (20 sides × 2 confidences) × ≥ 0.46 ms ≥ 18 ms: over budget unless parallel, or limited to the specs the edited knob reaches (`agent_flows.md` open question 1) |
| Agent: `check` / `verify` | below one LLM turn (≳ 1 s, assumed) | **0.13–0.24 s** ✓ | 223 × 5 ms ≈ 1.1 s serial; about 0.1–0.3 s on 20 workers (7 rounds, `engine_flows.md` §2) | 3,511 runs (`engine_flows.md` §4.8) × ≥ 0.46 ms ≥ 1.6 s serial: about one turn |
| Agent: `explain`, `simulate_at`, `sweep` | ≪ one turn | 0–5 ms | 5–50 ms | same order |
| Agent: `audit` | below one turn | 0.12 s (256), 0.54 s (1024) | 1.29 s serial, 0.13 s on 20 workers (256, measured) | not offered (2^50) |
| `monte_carlo` | minutes class; async, with progress | 11 s per range corner (20,000 boards) | ≈ 100 s serial | — |

What follows for the design:
1. **On the CE amp the engine is never the bottleneck.** Every flow in §4 spends under 1.3 s in the engine in total (not counting the optional Monte Carlo), and 1–5 LLM turns.
2. **So minimize turns.** `verify` takes a list, `check` returns everything the usual follow-ups need, and `explain` needs no loop. Fix-bass drops from 3 `what_if` + 3 `verify` calls to one `verify([A, B, C])`.
3. **The editor's instant tier needs in-process speed.** Replaying 10 stored points in one frame works in-process (6–8 ms) and not serially through subprocesses (50 ms).
4. **At 50 knobs `verify` costs about one turn.** Only there does `what_if` earn a place in the agent's loop, as a screen before `verify`: it can prove a FAIL, and it can never approve.

### 3.6 What the agent reads

| Encoding of the MVP `check` | Characters |
|---|---|
| JSON (`spicy.agent/0`, all 10 records) | 9,143 |
| Compact lines (one per record, `any` knobs dropped, `api.compact`) | **2,061** (4.4× smaller) |

The compact form goes into the standing context every turn. The JSON goes to the UI, the lint and tests. Example lines (real output, aged variant):

```
bass/max/worst_case    FAIL                 32.9885   lim 30     m -2.99 ±3.01 | temp=-10.0°C vcc.v=12.6V life=1.0×10y r1.value=46.53kΩ r2.value=9.9kΩ rc.value=4.747kΩ re.value=990.0Ω c_in.value=800.0nF q1.beta=100.0
bass/max/sigma3        FAIL                 30.9487   lim 30     m -0.949 ±0.00059 | temp=-10.0°C vcc.v=12.6V life=1.0×10y r1.value=46.99kΩ r2.value=9.994kΩ rc.value=4.7kΩ re.value=1000.0Ω c_in.value=816.1nF q1.beta=184.0
```

These lines still show `rc.value` in the bass counterexample, although RC has no effect on f_low in this model: its recovery is 1e-8 Hz. The fix is §5.3 #3: carry each knob's recovery, and drop knobs that don't move the measure.

---

## 4. The flows

Common notation:
- The columns are **engineer · agent · engine · ngspice**.
- Latencies are measured in-process on one core. Subprocess figures are about 10× the run cost.
- `rev` values are the prototype's real hashes: `036a1c` is the MVP, `b2aa7b` the aged variant.
- Transcripts are what the agent says. Tool calls in `[brackets]` aren't shown to the engineer. Every quoted answer was run through the lint and passed; §3.4 says what the lint can't see.

### 4.0 The cold check (`spicy check`)

```
 engineer            agent                 engine                              ngspice (libngspice)
    │ spicy check       │                     │                                     │
    │───────────────────────────────────────►│ parse → knobs → SPICE deck ─────────►│ load (2 ms)
    │                   │                     │ shared round (15)          ◄───────►│ 15 × 0.46 ms
    │                   │                     │ worst_case walks (43)      ◄───────►│
    │                   │                     │ sigma(3) points (165)      ◄───────►│
    │                   │                     │ band re-runs (10), forms (≈80)◄────►│
    │◄──────────────────────────────────────│ table + JSON + store[rev]           │   178 ms total
    │                   │ (later) reads JSON  │                                     │
```

- **What runs:** everything in §2.4.
- **Cached:** every run result (node voltages, device quantities, measures), keyed by the physical parameter values each analysis reads. Per revision: records, worst points, forms at the worst points, nominal forms.
- **What the agent may claim:** each record's `claim`, nothing stronger. For the MVP that's "gain and bass pass (estimated); bias passes at 3σ (6.214 V); bias max at worst case is undecided (6.486 V, 14 mV inside)", plus the `not_modeled` caveats.

### 4.1 "Why does bass fail?" (aged variant)

```
 engineer                 agent                               engine                    ngspice
    │ why does bass fail?    │                                    │                         │
    │───────────────────────►│ (standing context: compact records, rev b2aa7b)             │
    │                        │ explain([bass/max/sigma3,          │                         │
    │                        │          bass/max/worst_case])     │                         │
    │                        │───────────────────────────────────►│ record + form@worst     │
    │                        │                                    │ (stored, 0 runs)        │
    │                        │                                    │ recovery: 1 run / knob ─►│ 2 × 8 runs
    │                        │◄───────────────────────────────────│ 10.5 ms                 │
    │                        │ simulate_at(3σ point, life = new) ►│ 1 run ─────────────────►│ 0.5 ms
    │◄───────────────────────│ answer                              │                         │
```

What the engine returned (`out_f1_f2.txt`):

| | 3σ point | worst-case corner |
|---|---|---|
| Value | **30.95 Hz, FAIL** | **32.99 Hz, FAIL** |
| Point | −10 °C, 12.6 V, life 10 y, C_in 816.1 nF (−18%), β 184, others ≈ nominal | −10 °C, 12.6 V, life 10 y, C_in 800 nF, β 100, R2 −1%, R1 −1%, RE −1% |
| Recovery: C_in back to nominal | 5.692 Hz | 6.598 Hz |
| Recovery: life back to its midpoint (5 y, the range knob's nominal) | 3.439 Hz | 3.665 Hz |
| The same board with a new capacitor (life = 0 y, `simulate_at`) | 24.76 Hz (−6.19 Hz) | 26.39 Hz (−6.60 Hz) |
| Recovery: β | 0.103 Hz | 1.219 Hz |
| Recovery: R2 | 0.014 Hz | 0.251 Hz |
| The nominal line's prediction at this point | 29.04 Hz (miss 1.91 Hz) | **29.98 Hz (miss 3.01 Hz)** |

```
you    Why does bass fail?
agent  [engine.explain bass/max/sigma3, bass/max/worst_case → rev b2aa7b]
       [engine.simulate_at the 3σ point with life = new]
       Bass fails. With 3σ parts it reaches 30.95 Hz against the 30 Hz limit, at -10 °C,
       12.6 V and end of life, with C_in 18% low and β 184. With every part at its worst
       edge it reaches 32.99 Hz. C_in is the lever: putting it back to nominal recovers
       5.692 Hz. Aging is the other: with a new capacitor the same board gives 24.76 Hz.
       β recovers 0.103 Hz, the resistors almost nothing.
```

| Step | The agent knows | It may say | It must not say |
|---|---|---|---|
| Before any call | compact records (FAIL at both confidences) | "bass fails at both confidences", with the values | why (no contributors yet) |
| After `explain` | recovery at both points (`simulated`) | the ranking, in Hz, at a named point | "C_in causes 51% of the failure" (a share nobody computed); anything from the nominal line; "aging costs 3.44 Hz" (that's only half the aging, see below) |

**Lessons:**
- **Confirmed from round 2, on ngspice:** the nominal line predicts **29.98 Hz** at the worst corner, which reads as a pass. The truth is 32.99 Hz. The API gives the nominal form only as `form_at_nominal.miss_at_worst`, never as a value on its own.
- **New: recovery for a range knob needs the right reference point.** Moving `life` "back to nominal" means its midpoint, 5 years, and recovers 3.44 Hz. A new capacitor recovers 6.19 Hz. An agent quoting "aging costs 3.44 Hz" would understate aging by almost half. I caught it myself and asked `simulate_at` for life = 0. For range knobs whose natural reference is an edge (life = new), recovery should go to that edge (R4).

### 4.2 "Fix bass" (aged variant)

```
 engineer        agent                          lang                engine                   ngspice
    │ fix bass      │ (explain payload: C_in and aging dominate; R_in is the other lever)      │
    │──────────────►│ propose A, B, C ─────────────►│ 3 proposals, revs   │                      │
    │               │◄──────────────────────────────│ + knob widths moved │                      │
    │               │ verify([A, B, C]) ────────────────────────────────►│ 3 full checks ──────►│
    │               │                               │                    │ (cache: op runs of   │
    │               │                               │                    │  A hit the base's)   │
    │               │◄───────────────────────────────────────────────────│ records + deltas     │
    │               │                               │                    │ 0.63 s               │
    │◄──────────────│ diff + verdict-delta panel     │                    │                      │
    │ accept B      │ apply(B, user token) ─────────►│ new rev; old records → stale             │
```

The three candidates, as round 2 proposed them (`agent_flows.md` §3.2):
- **A:** C_in 2.2 µF ± 20%, still an electrolytic.
- **B:** C_in a 1 µF ± 5% film capacitor, so no aging link.
- **C:** divider 100k / 22k.

**`what_if` vs `verify`** (`out_f1_f2.txt`):

| Candidate · side | `predicted` (log form) | `replay` (stored point, simulated) | **`verify` (estimated)** | Limit |
|---|---|---|---|---|
| A · bass WC | 14.998 | 14.995 | **14.995, PASS** | ≤ 30 |
| A · bass σ3 | 14.07 | 14.068 | **14.068, PASS** | |
| B · bass WC | 27.78 → now `not_covered` | 22.224 | **22.224, PASS** | |
| B · bass σ3 | 26.48 → now `not_covered` | 21.179 | **21.301, PASS** | |
| C · bass WC | 15.99 | 16.413 | **16.413, PASS** | |
| C · bias max WC | **6.587** (reads as borderline) | **6.742, proves FAIL** | **6.742, FAIL (BROKEN)** | ≤ 6.5 |
| C · bias max σ3 | 6.286 | 6.437 | 6.447, PASS | ≤ 6.5 |
| Cost | — | 6–7 ms each | 195–230 ms each | |

- **B's prediction was 25% off** (27.78 vs 22.22 Hz). The edit removed the aging link, and the stored slopes still contained it. The API now returns `not_covered` whenever a part kind, a link or the topology changes.
- **C's re-run proved the bias failure** in 7 ms. The prediction called it borderline.
- **C at 3σ.** In round 2's model, C also broke bias at 3σ (6.70 V). On ngspice's XTB = 0 transistor it passes at 3σ (6.447 V). The model decides.

```
you    Fix bass.
agent  [lang.propose A, B, C] [engine.verify A, B, C → revs 121181, 8e01c3, bca70d]
       A passes bass, estimated: 14.99 Hz worst case, 14.07 Hz at 3σ.
       B passes bass, estimated: 22.22 Hz worst case, 21.3 Hz at 3σ. Bias max at worst
       case is still undecided: 6.486 V, 14 mV inside 6.5 V, as before.
       C passes bass at 16.41 Hz worst case. But it breaks bias at worst case: 6.742 V
       against 6.5 V.
       I recommend A or B. Accept one?
```

| Step | May say | Must not say |
|---|---|---|
| After `propose` | "three candidates", with the edits | anything about verdicts |
| After `what_if` (if used) | "C already fails bias at worst case: 6.742 V at the stored worst point (proven)"; "A and B show no failure at the stored points yet" | "A fixes bass" (the lint catches it) |
| After `verify` | "A and B fix bass (estimated)", "C breaks bias at worst case" | "A fixes bass for sure" |
| After `apply` | "applied; bass passes on rev …" (`verify` already checked that rev) | quote the old rev's records as current |

**Lesson:** with `verify` at 0.2 s, the agent needs no `what_if` step here. Two tool turns do it: `propose`, then `verify` with a list.

### 4.3 What-if on an edit: the engineer changes R1 from 47k to 51k

In the editor, this is two tiers. For the agent ("what happens if I use 51k?"), it's one `verify`.

```
 engineer                 editor / agent                   engine                             ngspice
    │ edits r1 → 51k         │                                  │                                   │
    │───────────────────────►│ T0: what_if(p/r1-51k) ──────────►│ replay 10 stored points ─────────►│ 7 ms
    │ inlay: bias ✗ 7.086 V  │◄─────────────────────────────────│ replays prove FAIL (bias max)     │
    │   "proven at a stored  │ T1: verify(p/r1-51k) ───────────►│ full check ───────────────────────►│ 181 ms
    │    worst point"        │◄─────────────────────────────────│ records + delta                   │
    │ table: bias ✗ both     │                                  │                                   │
```

| Side | `predicted` | `replay` | `verify` |
|---|---|---|---|
| bias max WC | 7.134 | **7.086, proves FAIL** | 7.086, FAIL (was UNDECIDED) |
| bias max σ3 | 6.887 | **6.835, proves FAIL** | 6.835, FAIL (was PASS) |
| bias min WC | 5.430 | 5.370 | 5.370, PASS |
| bass max WC | 26.04 | 26.05 | 26.05, PASS |

```
you    What if r1 is 51k?
agent  [lang.propose r1 → 51k ± 1%] [engine.verify → rev …]
       Checked: with 51k, bias fails at 3σ (6.835 V) and at worst case (7.086 V).
       Gain and bass pass.
```

**Cached:** nothing useful. Every run reads R1, as `engine_flows.md` §3.2 predicted. **What changed from round 2:** nothing. The replay's value in the editor is the instant, proven ✗.

### 4.4 "Add a spec: the output must swing ±1 V"

```
 engineer             agent                        lang                        engine                   ngspice
    │ output must      │ derive DC-headroom specs (agent_flows.md §3.5)          │                          │
    │ swing ±1 V       │ propose(add swing_up, swing_down) ─►│ flagged           │                          │
    │─────────────────►│                                     │ requirement_change │                          │
    │                  │ verify ────────────────────────────────────────────────►│ shared round: all hits   │
    │                  │                                     │                   │ new walks only ─────────►│ 96 op + 109 ac
    │                  │◄────────────────────────────────────────────────────────│ 65 ms, 641 cache hits    │
    │◄─────────────────│ spec text + derivation + verdicts; notes the bias proxy │                          │
```

| New spec | `worst_case` | `sigma(3)` | Verdict |
|---|---|---|---|
| `swing_up: dc(vcc.v − output.v) >= 1V` | 5.254 V | 5.493 V | PASS (estimated) |
| `swing_down: dc(output.v − emitter.v) >= 1.42V` | 3.374 V | 3.593 V | PASS (estimated) |

```
agent  Both new specs pass, estimated: swing_up needs 1 V and has 5.254 V at worst case;
       swing_down needs 1.42 V and has 3.374 V at worst case.
```

- **Cached:** because every run keeps all node voltages, the new DC specs replayed the whole shared round and every point the old walks had visited: 641 hits against 205 new solves.
- **One waste:** the new walks' fresh points also ran AC, because the design has AC specs. A planner that asks each point only for the analyses its requester needs would have run 96 operating points and no AC. That's `engine_flows.md` §4.1's union of needs, done per request.
- **What the agent may claim:** the verdicts above. That the bias window is stricter than the swing it stands for is a finding to report. Loosening `bias` would be a requirement change, proposed only if asked (round 2's G6).

### 4.5 "Use a real transistor: 2N3904"

```
 engineer          agent                                    parts / bench          engine                      ngspice
    │ use a real     │ extract 2N3903/D → hFE ≥ 70 at 1 mA, 1 V (tested);            │                            │
    │ 2N3904         │ 100–300 only at 10 mA (agent_flows.md §3.6)                    │                            │
    │───────────────►│ bench(card, IC = 1 mA, VCE = 1 V) ─────►│ BF search (35 ms) ──────────────────────────────────►│
    │                │◄─── BF 111.4 ↔ hFE 70; ceiling 187.6 ────│                     │                            │
    │                │ propose(q1: Q2N3904 card, BF 111.4..=?) ─────────────────────►│                            │
    │                │ verify ──────────────────────────────────────────────────────►│ full check (197 ms) ──────►│
    │                │◄─────── bias WC FAIL 6.810 V (tag: relies-on-unreviewed) ──────│                            │
    │                │ explain(bias/max/worst_case) ────────────────────────────────►│ hFE at counterexample 62.1 │
    │◄───────────────│ record for review + verdict delta + what to check first      │                            │
```

**What the bench found** (`part_bench.py`, on the commonly circulated Q2N3904 Gummel–Poon card, origin not verified):

| Question | Answer |
|---|---|
| hFE of the card as published (BF = 416.4) at 1 mA, 1 V, 25 °C | **132.0** (not 200, the MVP's nominal) |
| at 10 mA, 1 V | 158.7 (inside the datasheet's 100–300 at 10 mA) |
| BF that gives the datasheet's tested minimum, hFE = 70 at 1 mA | **BF = 111.4** (at −10 °C this gives hFE 58.0; at 60 °C, 82.7) |
| BF that gives hFE = 300 at 1 mA | **none:** at BF = 5000 hFE is only 187.6. The card's recombination current (ISE, NE) caps the gain at low current |

So "β 70..=300" can't be written as a BF range for this card. The agent has to choose an upper edge, and the engine's verdicts depend on that choice (`run_f5_alt.py`):

| BF range the agent picks | Nominal hFE at the op | bias max `worst_case` | bias max `sigma(3)` | bass max `worst_case` |
|---|---|---|---|---|
| 111.4 ..= 416.4 (tested min … card as published) | 119.2 | **6.810 V, FAIL** | **6.540 V, FAIL** | 27.68 Hz, PASS |
| 111.4 ..= 5000 (tested min … card's ceiling) | 198.0 | **6.810 V, FAIL** | **6.182 V, PASS** | 27.68 Hz, UNDECIDED |

- **The `worst_case` FAIL is the defensible verdict.** It depends only on the tested minimum. At the counterexample the transistor runs at hFE 62.1 and IC 1.244 mA (−10 °C, BF 111.4).
- **The `sigma(3)` verdict flips** on a choice the datasheet doesn't make. The engine should refuse to give one: "UNSPECIFIED (distribution): q1.beta has a one-sided tested limit" (recommendation R9).

**Two more things the flow exposed:**
- **The `what_if` replay was silently useless.** The knob's name changed (`q1.beta` → `q1.bf`), so the replay held it at nominal (BF 2556) and reported "no failure seen" (6.317 V). The API now flags `partial: no counterpart for q1.bf, held at nominal`. The real fix: when a part is pinned, keep the knob's identity (`q1.beta`) and change only its mapping to the model.
- **The engine can't say how BF was chosen.** It sees BF 111.4 ..= 5000. That this came from "hFE ≥ 70 at 1 mA, p. 2" lives only in the agent's provenance comment. M5's part records must carry the bench condition next to the knob.

```
agent  [parts.extract 2N3903/D] [parts.bench hFE @ 1 mA, 1 V] [lang.propose] [engine.verify]
       With the 2N3904, bias fails at worst case: 6.810 V at -10 °C, where the transistor's
       hFE is 62 (the datasheet's tested minimum is 70 at 1 mA; in the model hFE falls
       with cold). This rests on data extracted by AI and not yet reviewed: please
       check page 2, "hFE, IC = 1.0 mA". I can't give a 3σ verdict: the datasheet only
       guarantees a minimum, so any spread I'd assume above it is my invention.
```

### 4.6 "Make the gain 10 with the same bias"

```
 engineer        agent                                  engine                          ngspice
    │ gain 10,     │ simulate_at(nominal) ─────────────────►│ cached: 0 runs                │
    │ same bias    │◄── IC 1.375 mA, hFE 200 ──────────────│                               │
    │─────────────►│ algebra: RE_ac ≈ 449.1 Ω; snap → 453 (E96)                              │
    │              │ simulate_at × 4 (R2 candidates) ──────►│ 4 new runs, 2.3 ms ──────────►│
    │              │ propose A (values), B (split RE) ─────►│                               │
    │              │ verify([A, B]) ───────────────────────►│ 2 full checks, 0.45 s ───────►│
    │              │◄── A: bias ✗, bass ✗ · B: gain ✓, bass WC ✗ 30.69 Hz                  │
    │              │ propose B′ (B + C_in 2.2 µF), B-220µ, B-470µ; verify(list) ──► 0.69 s    │
    │◄─────────────│ diff (new net ex, 3 new parts, spec gain 4.6 → 10: requirement change)│
```

The agent's algebra, from engine numbers:
- VT = 25.69 mV at 25 °C, so r_π = β·VT/IC = 3737 Ω;
- RE_ac = (β·RC/10 − r_π)/(β + 1) = 449.1 Ω;
- `parts.snap` gives 453 Ω (E96).

| Option | Nominal (simulated) | bias max WC / σ3 | gain 10 ± 5% (WC) | bass WC / σ3 |
|---|---|---|---|---|
| **A:** RE 453, R2 5.76k (values only) | VC 5.707 V, gain 9.901, f_low 32.77 Hz | **6.917 FAIL / 6.595 FAIL** | 9.569 … 10.189 PASS | **43.49 FAIL / 40.37 FAIL** |
| **B:** RE1 453 unbypassed + RE2 549 ∥ C_E 100 µF | VC 5.550 V, gain 9.914, f_low 22.53 Hz | 6.497 UNDECIDED / 6.223 PASS | 9.583 … 10.202 PASS | **30.69 FAIL** / 27.40 PASS |
| B with C_E 220 µF | — | same | same | 29.27 UNDECIDED / 26.40 PASS |
| B with C_E 470 µF | — | same | same | 28.88 UNDECIDED / 26.15 PASS |
| **B′:** B + C_in 2.2 µF | — | 6.497 UNDECIDED → audit: **PASS at all 1024 corners by 2.6 mV** | 9.586 … 10.203 PASS | **16.07 PASS / 14.13 PASS** |

**`what_if` on A, the most misleading numbers in this study:**

| A · side | `predicted` (log form) | `replay` (simulated) | `verify` |
|---|---|---|---|
| bias max WC | **6.014** (looks like a comfortable PASS) | **6.917, proves FAIL** | 6.917 FAIL |
| bias max σ3 | 5.675 | 6.593, proves FAIL | 6.595 FAIL |
| bias min WC | **3.950** (looks like a FAIL) | 4.685 | 4.582 PASS |
| bass max WC | 42.61 | 43.49, proves FAIL | 43.49 FAIL |

The log form was 0.90 V too optimistic on one side and 0.63 V too pessimistic on the other. A 453 Ω emitter resistor with a smaller divider changes how temperature and β reach VC. That's an interaction the stored slopes can't carry (`agent_flows.md` §3.2 found the same effect with the divider, 0.19 V; here it's five times larger).

```
agent  [engine.simulate_at nominal] [parts.snap 449.1 Ω]
       At nominal the transistor runs at 1.375 mA with hFE 200, so r_π ≈ 3737 Ω and the
       unbypassed emitter resistance for a gain of 10 is ≈ 449 Ω, 453 Ω in E96.
       [engine.what_if A] Values alone don't work: at the stored worst points design A
       already fails bias (6.917 V) and bass (43.49 Hz), proven at re-simulated points.
       [engine.verify B, B′ …]
       B′ meets the new gain spec, estimated: 9.586 to 10.2 at worst case. Bass passes at
       16.07 Hz worst case. Bias max at worst case is undecided: 6.497 V, 2.6 mV inside
       the limit.
```

**Lessons:**
- **The spec edit is a requirement change.** The diff marks it separately from the design hunks.
- **`verify` always re-checks every spec.** B fixes gain and breaks bass at worst case, which only an all-spec check catches.
- **What's missing:** a way to sweep C_E inside one call, in the spirit of engine.md §4.8's `?`. The agent made three extra proposals by hand (B-220µ, B-470µ, B′).

### 4.7 Select-to-ask on a region: "why is the bias so temperature-sensitive?"

The engineer selects `r1`, `r2`, `re`, `q1`. The editor sends a slice of the text that compiles on its own, an image, the records that touch the selection, and `not_modeled` (language.md §10).

```
 engineer               editor                          agent                     engine            ngspice
    │ select r1 r2 re q1, │ bundle: slice + anchors,       │                         │                 │
    │ ask                 │ image, bias records, rev,      │                         │                 │
    │────────────────────►│ not_modeled ──────────────────►│ sweep(temp, 5 points) ─►│ 4 new runs ────►│ 1.2 ms
    │                     │                                │◄── VC, VBE, IC per T ───│                 │
    │◄──────────────────────────────────────────────────────│ answer, cites slice lines                │
```

| temp | VC | VBE | IC |
|---|---|---|---|
| −10 °C | 5.8057 V | 0.7264 V | 1.3179 mA |
| 25 °C | 5.5382 V | 0.6668 V | 1.3749 mA |
| 60 °C | 5.2668 V | 0.6064 V | 1.4326 mA |

```
agent  [engine.sweep temp, 5 points]
       VC falls from 5.806 V at -10 °C to 5.267 V at 60 °C, ≈ 7.7 mV/K. VBE falls ≈ 1.71 mV/K
       over the same span, and across RE plus the divider's share that accounts for
       ≈ 7.74 mV/K of VC drift, essentially all of it. In this model β does not change
       with temperature (XTB = 0).
```

- **The trap this flow avoids.** Round 2's answer to the same question (on its own model) said "β's own tempco adds 0.10 V" (`agent_flows.md` §3.7). On ngspice's generic model that's false: hFE is 200 at every temperature. An LLM that answers from textbook knowledge, or from round 2, would state a mechanism the model doesn't contain.
- **What the agent may claim:** the sweep values (`simulated`), its own arithmetic marked "≈", and the model's limits taken from `not_modeled`.
- **What's missing:** nothing new in the engine, as round 2 found. The standing context must include `not_modeled`.

### 4.8 The pre-sign-off review: "is this design OK?" (MVP)

```
 engineer           agent                                  engine                                  ngspice
    │ is this        │ (records in context; bias max WC     │                                        │
    │ design OK?     │  UNDECIDED, next = audit, 256 runs)  │                                        │
    │───────────────►│ audit(rev) ─────────────────────────►│ all 256 corners ──────────────────────►│ 120 ms
    │                │◄── every WC side: PASS (all corners); search never missed                     │
    │                │ (optional, async) monte_carlo(bias, −10 °C, 12.6 V, 20,000) ─────────────────►│ 11 s
    │                │◄── q99.865 = 6.2085 V, 0 of 20,000 fail                                       │
    │◄───────────────│ verdicts + what they rest on + what isn't checked                            │
```

```
agent  [engine.audit → exhaustive]
       Gain and bass pass. Bias passes at 3σ (6.214 V). At worst case bias is undecided by
       the search, but all 256 corners stay inside: 6.486 V against 6.5 V. That rests on
       a transistor model whose β has no temperature coefficient.
       Not checked: aging of the electrolytic C_in (with 20% end-of-life loss, bass fails
       at 32.99 Hz worst case), a real transistor (with a 2N3904, bias fails at worst
       case), and any output load or source impedance.
```

| Claim in the answer | Evidence and status |
|---|---|
| gain, bass pass | `estimated` records |
| bias passes at 3σ | `estimated`; Monte Carlo agrees (6.2085 V at 99.865%, 0 of 20,000 over 6.5 V) |
| all 256 corners inside | `exhaustive` (audit) |
| rests on XTB = 0 | `not_modeled`, generated from the model card |
| aging and 2N3904 results | `check`/`verify` records of other revisions (the aged variant, the 2N3904 proposal). They're facts about those designs, not about this one, and the sentence should name them as such. The lint let this through only because of its negation rule (§3.4) |

**Lesson:** the audit turned the only UNDECIDED into a claim the agent can make in one more call, and it cost less than the check. Past about 12 knobs (4,096 corners ≈ 1.9 s) the `next` for a `worst_case` UNDECIDED has to be something else: a larger search budget, joint flips, or honestly "undecided".

### 4.9 All flows on one page

| Flow | Tool turns (after `check`) | Engine time, in-process | Engine time, subprocess (≈ 10× per run) | Strongest claim the flow earns |
|---|---|---|---|---|
| 4.0 cold check | 0 (the CLI) | 178 ms | ≈ 1.1–2 s serial | each record's `claim` |
| 4.1 why bass fails | 2 (`explain` ×2, then `simulate_at` with life = new) | 11 ms | ≈ 0.1 s | FAIL (definite) + recovery ranking |
| 4.2 fix bass | 2 (`propose`, `verify` list) + apply | 0.63 s | ≈ 4–8 s serial | "A and B fix bass (estimated); C breaks bias WC" |
| 4.3 what-if on an edit | 1–2 | 7 ms + 181 ms | 50 ms + ≈ 1 s | "fails at 3σ and WC" |
| 4.4 add a spec | 2 | 65 ms (641 hits) | ≈ 0.5–1 s | PASS (estimated) |
| 4.5 pin 2N3904 | 4 (extract, bench, propose, verify) | 35 ms + 197 ms | ≈ 1–2 s | "FAIL at WC, given unreviewed hFE ≥ 70"; σ3 UNSPECIFIED |
| 4.6 gain 10 | 4–5 | ≈ 1.15 s | ≈ 6–12 s serial | "B′ meets gain; bass PASS; bias WC 2.6 mV inside at all corners" |
| 4.7 select-to-ask | 1 (`sweep`) | 1.2 ms | ≈ 25 ms | simulated values + "≈" arithmetic |
| 4.8 review | 1 (`audit`) (+ async Monte Carlo) | 120 ms (+ 11 s) | 1.3 s serial / 0.13 s parallel | PASS (all corners) + caveats |

---

## 5. Playing the agent: what worked, what was missing, where the engine misleads

### 5.1 What worked

- **Records as the only source of verdict words.** Every answer in §4 was written from `claim` fields and values, and passed the lint.
- **Re-runs of stored worst points.** They proved every failure that a stored point could reach: C's bias failure, 51k's bias failures, A's three failures, each in 6–8 ms. They never produced a false alarm.
- **Recovery contributors.** One run per knob, no definition to argue about, and they rank the levers the agent then proposes (C_in, then aging, then R_in).
- **The cache.** `simulate_at` on any visited point costs nothing; a new DC spec replayed 641 solves; `explain` asked a second time costs 0 runs.
- **`next` on UNDECIDED.** The review settled the MVP's open side in one call.

### 5.2 What was missing

| Gap | Where it bit | Fix |
|---|---|---|
| `verify` with a list of proposals | fix bass, gain 10: one turn per proposal | `verify([…])` returns every check and one delta table |
| A part bench | 2N3904: "β 70..=300" can't be a BF range; I wrote the bench by hand | `parts.bench(card, condition)`: M5, with the condition stored next to the knob |
| Knob identity across a part pin | the replay held the renamed knob at nominal and reported "no failure seen" | keep `q1.beta`; change only its mapping. Flag partial replays |
| Refusing `sigma(k)` without a distribution | 2N3904: σ3 flips PASS/FAIL with an invented edge | UNSPECIFIED (distribution) for one-sided or typical-only knobs |
| Recovery inside the counterexample | the compact line lists `rc.value` for bass, where its recovery is 1e-8 Hz | each counterexample knob carries its recovery; knobs below the noise are dropped from the compact form |
| Analyses per request, not per design | "add a spec" ran AC on 109 points no DC spec needed | the planner asks each point only for what its requester needs |
| A value search inside one call | gain 10: C_E 100/220/470 µF by hand | post-MVP `engine.solve` (engine.md §4.8) |
| A generated `not_modeled` | I wrote the XTB line by hand | read the model card's defaulted temperature parameters |
| Structural knowledge of links | fix-bass B: the stored slopes still contained the removed aging link | `what_if` returns `not_covered` after any kind, link or topology change |

### 5.3 Eleven ways the engine's answers could mislead an LLM

| # | What the engine could return | What an LLM would say | The truth | Fix in the contract |
|---|---|---|---|---|
| 1 | The nominal form, pushed to the worst corner (aged bass) | "borderline, 29.98 Hz" | 32.99 Hz, FAIL | Forms are always `form@worst`; the nominal form only as `miss_at_worst` |
| 2 | A log-unit prediction (gain 10, option A) | "bias fine at 6.01 V" | 6.92 V, FAIL | Predictions carry `predicted` and never go into standing context; replays go with them |
| 3 | A counterexample listing every knob (bias) | "C_in at +20% drives the bias failure" | C_in can't move DC | `any` for structural zeros; recovery per knob |
| 4 | `sigma(3)` for a one-sided part limit (2N3904) | "bias passes at 3σ with the real part" | depends on an invented edge (6.182 vs 6.540 V) | UNSPECIFIED (distribution) |
| 5 | A replay with a renamed knob held at nominal | "no failure at the old worst point" | the worst point wasn't reproduced; WC FAIL 6.810 V | `partial` flag; keep knob identity |
| 6 | The headline confidence only | "bias passes" | UNDECIDED at worst case | lint rule: say both when they disagree |
| 7 | Nothing about the model's missing links | "β's tempco adds to the drift" | hFE is 200 at every temperature here | `not_modeled` from the model card |
| 8 | The word "verified" | "verified to pass" | an estimate | status `estimated`; no "verified" anywhere |
| 9 | UNDECIDED with no way forward | "probably passes" | passes at all 256 corners by 13.7 mV, or might not | `next` with the call and its cost |
| 10 | Six-digit values and raw margins (`±3.275e-06`) | arithmetic slips; "6.81 V" | 6.742 V | round to 4 digits in the compact form; the lint checks every number |
| 11 | Recovery of a range knob measured to its midpoint (`life` → 5 y) | "aging costs 3.44 Hz" | a new capacitor recovers 6.19 Hz | recovery goes to the knob's reference (life = new) and says which point it used |

---

## 6. The minimum the MVP must expose

So the agent (and the editor) can be added later without redesign. The CLI is the first agent. None of this needs LLM code.

| # | Item | In the MVP as | Why now |
|---|---|---|---|
| 1 | **`spicy check --format json`**, schema `spicy.check/1` | the record of §3.2: id, spec, side, confidence, headline, status, verdict, value, margin, limit, unit, bracket, e_obs, ε_num, counterexample (knob paths, physical values with units, `any`), region, rev, method (rounds, certificate, runs), tags, **claim**, **next** | The seam for every consumer. `claim` is also the terminal text, so one template gets snapshot-tested |
| 2 | **`rev`** = a hash of the elaborated design (not the file bytes) | printed by `check` | Stale detection; comment edits don't invalidate results |
| 3 | **`coverage` and `not_modeled`** | structural zeros; knobs with exactly zero slope; defaulted model-card temperature parameters; part kinds without MVP links | The MVP's key verdict hinges on XTB (§2.7) |
| 4 | **A results store keyed by `rev`** | worst points (ε and physical), the form at each worst point with its linearization point and knob scales, all run results (node voltages, device quantities, measures) | `explain`, `what_if` and new DC specs need no loop |
| 5 | **`spicy sim --at <point>`** | node voltages, device quantities and region, every measure | `simulate_at`; "click the counterexample" (engine.md §3.5) |
| 6 | **`spicy sweep <knob>`** | a table of real points | select-to-ask |
| 7 | Library functions, no CLI needed yet | `explain` (recovery), `what_if` (replay; prediction only as `predicted`; `not_covered`; `partial`), `audit` (corners when 2^N ≤ 4096) | Cheap once 1–6 exist; they keep statuses honest from day one |
| 8 | The ngspice backend rules | `destroy all` per run; temperature only through `.temp {temp_c}`; `ac lin 1` at exact frequencies; our f_low; engine options; one owner per libngspice instance | Each rule is a measured trap (§2.2) |

**Not in the MVP:** chat, MCP, the lint (except as a test corpus for `claim`), `solve`, `monte_carlo` as a tool, `parts.*`.

---

## 7. Recommendations for the final plan

| # | Recommendation | Reason |
|---|---|---|
| **R1** | **The engine writes the verdict sentence.** Each record carries `claim` (the strongest allowed sentence) and `next` (for UNDECIDED: the call, its runs and time). The terminal table prints the same text | One deterministic, snapshot-tested template for both readers. The LLM can paraphrase but not strengthen it, and the lint can check against it. Without `next`, UNDECIDED leaves the agent stuck (§4.8) |
| **R2** | **Adopt the status set** `predicted` / `simulated` / `estimated` / `exhaustive` / `proven`, plus the `stale` modifier, with §3.3's word table. **Don't use "verified"** | Each status maps to allowed words. "Verified" reads as "proven" (§5.3 #8) |
| **R3** | **`spicy check --format json` (`spicy.check/1`) as in §6 item 1, plus a compact text form** in M3e | The seam for every later agent; the compact form is 4.4× smaller for the LLM's context |
| **R4** | **Counterexamples mark structural zeros as `any` and carry each knob's recovery** (1 run per knob, lazily or at check time). For a range knob, recovery goes to its declared reference (`life` → new), not to its midpoint, and says which point it used | Otherwise an LLM names C_in as a cause of the bias failure (§5.3 #3), or says aging costs 3.44 Hz when a new capacitor recovers 6.19 Hz (§4.1). Recovery costs 4–5 ms per record |
| **R5** | **Generate `not_modeled` and `coverage` from the model card and part kinds** in M3 | The MVP's bias max `worst_case` goes from UNDECIDED (6.486 V, passes at all corners) to FAIL (6.594 V) with XTB = 1.5 (§2.7) |
| **R6** | **`verify` is the agent's default; `what_if` is screening.** `what_if` returns replays (which can prove a FAIL), marks predictions `predicted`, returns `not_covered` after any kind, link or topology change, and flags `partial` replays | `verify` costs 0.13–0.24 s on the CE amp. Predictions missed by 0.90 V (gain-10 A) and 25% (fix-bass B). Replays proved every failure they could reach, with no false alarms |
| **R7** | **`verify` accepts a list of proposals** and returns one delta table | Agent turns dominate latency. Fix-bass goes from 6 engine calls to 1 |
| **R8** | **Add an `audit` tier:** every corner when 2^N ≤ 4096, status `exhaustive`, verdict "PASS (all corners)" or a FAIL the search missed | C6 leaves the MVP's key side UNDECIDED (13.7 mV inside, e_obs 126 mV). The audit settles it in 0.12 s (256 corners), and in 0.54 s for B′ (1024 corners, 2.6 mV inside) |
| **R9** | **`sigma(k)` returns UNSPECIFIED (distribution)** when a statistical knob's range isn't a two-sided tested limit, unless the design states a distribution | 2N3904: bias at 3σ is FAIL 6.540 V or PASS 6.182 V depending on an upper edge the datasheet doesn't give (§4.5) |
| **R10** | **ngspice backend rules** (§6 item 8), each pinned by a test: `destroy all` per run; temperature via `.temp {temp_c}` + `alterparam` only; exact AC frequencies by `ac lin 1`; our f_low; engine options; one owner per libngspice instance | Measured traps: 0.86 → 5.7 ms per run from accumulated plots; `option temp` silently undone by `reset`; `dec` grids accumulate (`acan.c:366`) |
| **R11** | **Use in-process libngspice for the MVP backend, behind the `Backend` trait,** and keep `ngspice -b` as the fallback and cross-check (reverses roadmap §3's "subprocess first") | 0.46 vs 5.0 ms per run. Only in-process fits the editor's 16 ms `what_if` budget; the agent is fine either way. The ctypes driver class is 79 lines |
| **R12** | **When a part is pinned, the knob keeps its identity** (`q1.beta`); only its mapping to the model changes, and the bench condition that produced the mapping (hFE ≥ 70 at 1 mA, 1 V) is stored with it. Specify it in M5's design note; the MVP needs nothing | Renaming broke the replay silently. The 2N3904 card can't reach hFE 300 at 1 mA at any BF, so the mapping is a modelling decision that must stay visible |
| **R13** | **Keep every run's full operating point in the per-`rev` store,** and plan analyses per request | "Add a spec" replayed 641 solves; with per-request analyses it would also have skipped 109 AC runs |
| **R14** | **Acceptance tests for the contract:** snapshot the `claim` strings of the MVP, the aged variant and the adversarial variants; keep the lint's 18 answers as a regression corpus | The contract is the product's seam to the agent; it deserves the same snapshot discipline as the simulator |

---

## 8. Open questions

1. **Should C6's allowance use the local error after a certificate?** After a one-flip certificate, every one-flip neighbour of the answer has been simulated. Is the nominal line's miss (126 mV at the MVP's bias corner) the right allowance, or should it be the secant error around the certified vertex? The current rule gives UNDECIDED on a side that passes at all 256 corners.
2. **What's `next` past 12 knobs?** Corners stop being affordable (2^13 = 8,192 runs, ≈ 3.8 s in-process). Options: a larger flip budget, joint flips of the top four knobs (round 2's open question 2), or a plain "undecided".
3. **Who decides the headline badge when the confidences disagree?** The spec's declared confidence, or the worse of the two, with the other shown on hover?
4. **Claim wording.** Fixed English templates, or structured claims (`{verdict, value, limit, point}`) that the UI and the agent each phrase? Structured is safer for the lint; text is what the terminal needs.
5. **How are `not_modeled` items ranked?** The MVP has four. A real board may have fifty. Which go into standing context: those that touch a spec near its limit?
6. **Async long jobs.** Monte Carlo is 11 s per range corner here and minutes on real circuits. Does the agent wait, poll, or get a notification in a later turn? (`agent_flows.md` open question 5.)
7. **Part-pin knob identity in the language.** Does `q1.beta` survive `part: onsemi::2N3904`, or does the part record define its own knobs (`hfe_1ma`) that `beta` maps to?
8. **Chats bound to revisions.** When the engineer reopens a chat whose records are stale, does the agent re-check silently (0.2 s here) before answering, or say the answers are old?
9. **Does the agent see `e_obs` at all?** It's needed to explain UNDECIDED, but "±3.3e-06" invites misreading. Show it only in `explain`?

---

## Appendix: scripts (`/root/.claude/jobs/443154a8/tmp/engine2/agent/`)

| Script | What it does |
|---|---|
| `ngback.py` | libngspice via ctypes (load once, alterparam + reset, op, exact AC points, sweeps, complex vectors), the f_low measure, and a subprocess runner |
| `t_temp.py` | Temperature follows `alterparam` through `.temp {temp_c}`; `option temp=` is undone by `reset` |
| `t_timing.py`, `t_timing2.py`, `t_timing3.py` | Per-piece run costs; the plot-accumulation slowdown and its fix |
| `t_noise.py` | Nudged slopes at three step sizes, default vs engine tolerances |
| `t_subproc.py` | `ngspice -b` per run, serial and on 20 workers |
| `engine.py` | Designs, cached Runner, shared round, `worst_case` walk with flips, `sigma(3)` with the exact map, band re-runs, verdict table |
| `designs.py` | MVP, aged variant, and the edits (value, film C_in, split emitter, 2N3904 card) |
| `answer_key.py`, `run_mvp.py`, `run_mc.py`, `run_mc_aged.py` | 256/512 corners + 500 interior points; Monte Carlo (truncated normal, 20,000 boards) |
| `run_mvp_cost.py`, `run_cost_final.py` | Run counts and wall times per confidence |
| `run_xtb.py` | The same MVP with XTB = 0 and 1.5 |
| `api.py` | The agent API, statuses, claims, compact encoding, `parts.snap` |
| `lint.py` | The claim lint |
| `flow_f0.py` | 4.0: cold check through the API (`out_check_mvp.json`, `out_check_aged.json`) |
| `flow_f1_f2.py` | 4.1 and 4.2 (`out_f1_f2.txt`, `out_whatif_C.json`, `out_verify_C.json`, `out_explain_bass_sigma3.json`) |
| `flow_f3_f4_f6.py` | 4.3, 4.4, 4.6 (`out_f3_f4_f6.txt`, `out_gain10.json`) |
| `part_bench.py`, `flow_f5.py`, `run_f5_alt.py` | 4.5: hFE bench, BF mapping, verify, sensitivity to the upper edge (`out_f5.txt`) |
| `flow_f7_f8.py`, `run_audit_bp.py` | 4.7, 4.8, and the audit on B′ (`out_f7_f8.txt`) |
| `flow_lint.py`, `flow_transcripts.py` | The lint on good and bad answers; every answer quoted in §4 |
