# Engine Position: Soundness First ("a verdict must never lie")

> 2026-09-27 · Round 3, one of five position papers. Builds on `engine_synthesis.md` (round 2: C1–C12, D1–D10) and the six reports next to it.
> Every number was computed on **ngspice-42** (libngspice, in process) by the stdlib-Python scripts in `/root/.claude/jobs/443154a8/tmp/engine2/soundness/` (called `$S` below; §0 lists them), or is cited as `file:line` in `/tmp/refs/ngspice`.
> That directory is scratch and goes away with the job. Copy it into the repo if these numbers must stay reproducible.

## Summary

The position: **the engine may say UNDECIDED as often as it needs to, but it may never be confidently wrong.** At ngspice speed this costs almost nothing. One run of the CE amplifier (operating point, the exact 1 kHz point, a 121-point sweep, the refined f_low, and a read-back of every knob) takes about **1 ms**, so a full check with both confidences costs **0.14–0.27 s**. I rebuilt round 2's adversarial suite on ngspice's real Gummel–Poon BJT and added four new traps, then built brute-force answer keys on ngspice. That gives 21 spec sides in 10 circuits, 11 of which truly fail. Results:
- **The unguarded loop** gives a confident false PASS on **5 of the 11** failing sides.
- **Round 2's guarded loop** does so on **2**:
  - a tuned stage whose gain peaks inside the box: it reports 6.68 where the truth is 24.4;
  - a netlist that silently drops the temperature knob: bias max 6.18 V, truth 6.56 V.
- **The configuration proposed here** gives **0**, on this seed and in 30 audit seeds. It is the guarded loop plus a tangent check at the final vertex, a 16-point seeded audit that can only witness a FAIL or refute the search, a read-back of every knob value from the simulator, and margins measured over the waveform for transient specs. On the MVP it costs 116 runs where round 2's loop costs 63.

Four findings go beyond round 2:
1. **Flips alone lose the tangent information that reveals an interior maximum.**
2. **The DC saturation margin can't see clipping during the signal swing.** At the failing THD corner the DC margin is 2.995 V, while the large-signal margin is −0.169 V.
3. **Under `sigma(3)`, two guards turned out to be necessary.** The margin guard over the σ-ball is needed (without it, a saturating stage gets a false PASS). So are per-band-edge sub-searches (without them, a band minimum gets a false PASS, and a 16-point audit misses it in 30 of 30 seeds).
4. **ngspice itself has silent traps.** With no DC solution it returns 999,999,999.99 V, return code 0 and no message. A netlist error kills the shared library. `option temp` and `option reltol` in control mode are undone by `reset`. The default temperature is 27 °C.

Where the loop is honest but pessimistic (UNDECIDED on a true PASS), enumerating all 256 corners decides the question in 0.3 s. So "never lie" and "usually decide" can both hold at MVP scale.

---

## 0. What was done, and how to re-run it

| Script (`$S/`) | What it does |
|---|---|
| `ng.py` | ctypes driver for libngspice: load, command, read real or complex vectors (`sharedspice.h:162-183`), capture every output line |
| `backend.py` | One run = `alterparam` per knob, `reset`, `destroy all`, `op`, read-back of every knob value, q1's record (vbe, vbc, ic, ib, region, saturation margin), `ac lin 1 1k 1k`, `ac dec 20 0.3 100k` + f_low refinement, band or THD measures. Status `ok` / `failed` / `implausible` |
| `designs.py`, `cases.py` | The MVP amplifier and the variants below, every knob a `.param`, `.temp {temp_c}`, `.options tnom=25`. Q1: `IS=1e-14 BF={beta} XTB=1.5` (β rises about 0.5 %/K), XTI=3 and EG=1.11 by default (VBE about −2 mV/K) |
| `keys.py` | Answer keys: every corner + 1,000 seeded uniform points + coordinate search from the best point and the best corner, 16 worker processes |
| `engine.py` | The prototype engine: shared round, round 2's guarded vertex search, the unguarded loop, the tangent check, the audit, interior ascent, band re-runs, the verdict table |
| `sigma.py`, `sigma_keys.py` | `sigma(3)` with the exact truncated-normal map (D3), linearized in ε; its keys (sphere search, 1,500 directions per range corner + refinement; `ball_key` searches the ball's volume too) |
| `suite.py` | Every configuration × every case against the keys (`results_suite.txt`) |
| `exp_noise.py` | Numerical error at ngspice's default vs engine tolerances; slope errors; a noise-table estimate |
| `exp_audit*.py` | Audit detection: analytic from the keys, and end to end over 30 seeds |
| `exp_bjt_thd.py`, `exp_sigma_adv.py`, `exp_sigma_band.py` | The real-BJT clipping trace; `sigma(3)` on the adversarial cases; the band-edge `sigma(3)` trap |
| `check_mvp.py`, `flows.py`, `exp_escalate.py`, `exp_2n3904_sigma.py`, `exp_scale.py` | The MVP check, the agent flows, escalation, part pinning, cost vs circuit size |
| `t_*.py` | The ngspice behaviour checks of §2.2 |

**Conventions.**
- Knob positions are ε ∈ [−1, 1], linear between the edges. The MVP knobs follow roadmap §4.2: T −10…60 °C, VCC 12 V ± 5%, R ± 1%, C_in ± 20%, β 100…300.
- A point simulated twice counts once, because the engine caches runs.
- A "false PASS" is PASS (estimated) on a side whose key fails.

---

## 1. The position

### 1.1 What counts as a lie

| Lie | Example from this round | What prevents it |
|---|---|---|
| PASS (estimated) where some board in the region fails | tuned stage: 6.68 reported, 24.4 true (§3.2) | guards that look where the loop doesn't (§3, §4) |
| A number that isn't what it claims to be | the temperature knob silently dead: "bias worst case 6.18 V" was never simulated at −10 °C | read-back of every knob value from the simulator (§3.2 d) |
| A failed run treated as a value, or dropped | ngspice returns 1e9 V for a circuit with no DC solution, return code 0 (§2.2) | plausibility check; a failed run is data |
| A FAIL that doesn't reproduce | none seen. A FAIL rests on a simulated point, and the band re-run reproduces it to 7e-9 V | cold re-run at reltol 1e-9 (§6) |
| A bracket that doesn't contain the truth | `sigma(3)` band minimum: bracket [4.4259, 4.4262], truth 4.4170 (§4) | per-extremum sub-searches |

An UNDECIDED that a cheap step could have decided isn't a lie, but it is a failure of the product. §7.4 gives the cheap step.

### 1.2 Round 2 from this position: keep, cut, change

| Item | Verdict | Why (evidence in this report) |
|---|---|---|
| C1 stop on optimality, accept only improvements | **Keep** | — |
| C2 / D1 flips at vertices | **Keep, and add a tangent check** at the final vertex | Flips alone gave a false PASS on the tuned stage (§3.2 a). The tangent check fixed it, costing about N runs per side |
| C3 margin guard | **Keep, and extend** to large-signal margins (the minimum over the waveform) for transient specs, and to the σ-ball for `sigma(k)` | DC margin blind at the THD corner (2.995 V vs −0.169 V, §3.2 b); `sigma(3)` false PASS on `hiZ` without it (§4) |
| C4 pooled inner bound, final cross-check | **Keep, and filter by region** | Free. My prototype once pooled a nudge 0.1% outside the box, and only the key's A3 assertion caught it (§3.2 c) |
| C5 below-noise knobs to an edge | **Keep** | Removed from the soundness configuration: false PASS on `band` in 19 of 30 audit seeds (§3.3) |
| C6 PASS needs margin > e_obs + ε_num | **Keep the rule, pin its inputs.** e_obs = jumps + safety-net points only, never audit points. Say that it's conservative | With audit points in e_obs, the MVP's true PASS (VC ≥ 4.5) turns UNDECIDED in 21 of 30 seeds (§5.3). On fix candidates the loop's worst equals the truth, yet e_obs = 0.18 V leaves them UNDECIDED (§7.4) |
| C7 / D3 exact map, the k-σ point | **Keep, and add** the σ-ball margin guard and per-extremum sub-searches | §4 |
| C8 three kinds of error | **Keep, and add a validity flag**: did the simulator see the requested knob values? | Dead-temperature case (§3.2 d) |
| C9 / D5 measures | **Keep.** On ngspice, exact frequencies come from `ac lin 1 f f`; never use ngspice's `meas` for verdicts | `ac dec` puts 1 kHz at 999.999999999994 Hz (`acan.c:366`); `meas` f_low is 0.03–0.17% off (§2.2) |
| C10 engine tolerances | **Keep. On ngspice they're free,** but must be `.options` in the netlist | 0.794 vs 0.788 ms per run; control-mode `option` is undone by `reset` (§2.2) |
| C11 instant tier | **Change further.** At MVP scale a verified re-check costs 0.15–0.2 s, so the agent never shows a predicted verdict. What-if = re-simulated stored points, which can only prove FAIL | §9 |
| C12 content cache, rounds | **Keep.** The key must include the netlist text, the `.options` and the ngspice version | — |
| D2 stop rule | **Keep** | — |
| D4 σ per side | **Keep**, and print "per side" | — |
| D6 cold every run | **Keep.** On ngspice every `reset` + `op` is cold and bit-reproducible. So a cold re-run cannot detect a second DC solution | §10 |
| D7 both confidences | **Keep** | `sigma(3)` for all 5 sides costs 67–83 runs, about 60 ms |
| D8 β 100..300 | **Keep** | 2N3904 (β 70..300): bias max 6.80 V at `worst_case`; 6.478 V at `sigma(3)` (UNDECIDED by the loop, PASS after 10.5k runs, §9) |
| D9 function-backed test backend | **Change.** Build and test on ngspice directly; keep a function backend only for unit tests of the loop's state machines | The whole 21-side suite runs in about 1 s on 16 cores |
| D10 JSON | **Keep, and add** an `evidence` block (certificate, audit n/seed/result, escalation, binding, band) | §7.3 |

---

## 2. The ngspice bench

### 2.1 One run

```
 knob point ε (8 numbers)
   │  value = nominal + ε·(edge − nominal)          R1 = 47k + 470·ε ...
   ▼
 alterparam temp_c=… vcc=… r1=… … beta=…  ;  reset  ;  destroy all
   ▼
 op ──► status: error text?  missing vector?  |V| > 10 × max source?  → failed / implausible
   │   read-back: @q1[temp], @vcc[dc], @r1[resistance], …, @qnpn[bf]  → binding ok?
   │   q1: vbe, vbc, ic, ib → VCE, region, m_sat = VCE − Vt·ln(2·10⁴)   (round-2 edge, §2.4)
   ▼
 ac lin 1 1k 1k            → |H(1 kHz)| exactly
 ac dec 20 0.3 100k        → bracket f_low; Illinois steps in ln f, each an exact ac point
 (tran + linearize         → THD by DFT, and the minimum VCE over the swing)
   ▼
 RunResult { status, binding, measures, region, margins, band }     0.6–1.1 ms
```

Nominal on ngspice: VC 5.50329 V, |H(1 kHz)| 4.590814, f_low 20.12695 Hz. Round 2's Python model gave 5.4999 V, 4.590813 and 20.12695 Hz. The ngspice model differs mainly in temperature. At T = −10 °C, VC is 5.830 V on ngspice against 5.862 V in the model.

### 2.2 ngspice facts an engine must not get wrong

Each was checked in process (`$S/t_temp.py`, `t_tol.py`, `t_ac.py`, `t_rb.py`, `t_nosol.py`).

| # | Fact | Consequence |
|---|---|---|
| 1 | `option temp=60` and `option reltol=1e-9` typed as control commands are **silently undone by `reset`**. VC stays 5.522803 V at −10 and 60 °C | Temperature and tolerances go into the netlist: `.temp {temp_c}` + `alterparam`, and `.options reltol=1e-6 vntol=1e-9`. Verified: `.temp {temp_c}` follows `alterparam` to all printed digits of a batch run (5.863924 / 5.541035 / 5.224354 V) |
| 2 | Default TEMP = TNOM = 27 °C (`cktinit.c:69-70`; "Doing analysis at TEMP = 27.000000") | A netlist without `.temp` runs every point at 27 °C, and the temperature knob does nothing (§3.2 d). Set `tnom` deliberately; here 25 °C, so that β = 200 at the knob's nominal |
| 3 | AC sweeps accumulate `freq *= delta` (`acan.c:366`): `ac dec 20 0.1 100k` puts its "1 kHz" at 999.999999999994 Hz | Exact points come from `ac lin 1 f f` (0.085 ms each) |
| 4 | **With no DC solution**, 1 mA forced into a reverse diode, ngspice returns **V(b) = 999,999,999.99 V, return code 0, no message**. GMIN = 1e-12 S (`cktinit.c:47`) "solves" it | The plausibility check is non-negotiable on ngspice too (our simulator returned −7.3·10²⁶ V, round 2) |
| 5 | **A netlist error kills libngspice:** "ngspice.dll cannot recover and awaits to be detached". The next circuit loads, `op` returns 0, and no vector exists | ngspice runs in a worker process. On that message, or a missing vector, the worker is restarted and the run marked failed. Restart costs 18–19 ms |
| 6 | Plots accumulate: `op` takes 0.24 ms after 200 runs, 0.08 ms with `destroy all` before each | Destroy plots every run |
| 7 | `meas … when` interpolates linearly. Literal −3 dB via `meas` gives 20.161 / 20.142 / 20.132 Hz at 10 / 20 / 50 points per decade; refined, 20.12695 Hz. The round-2 demo's `gmax/sqrt(2)` is half power: 20.083 Hz | Our measure library, never `meas`, for verdicts (C9) |
| 8 | Every `reset` + `op` starts from the same initial guess. Repeated runs are bit-identical | Runs are pure (good for D6 and the cache). A cold re-run can't find a second solution (§10) |
| 9 | Vector parameters read back as vectors: `@vin[sine]` = [0, 0.175, 1000] | The read-back needs a per-parameter path. My first version compared the offset to the amplitude and raised a false binding alarm, which is the safe direction |

### 2.3 The suite and its answer keys

Keys at ngspice's default tolerances (the MVP's is also redone at engine tolerances for §8). All took 0.1–1.1 s on 16 workers.

| Case | Circuit | Spec side | Key worst | Corner worst | Truth | Failing corners | Failing volume |
|---|---|---|---|---|---|---|---|
| mvp | `ce_amp.spl` | VC ≥ 4.5 / ≤ 6.5 | 4.6921 / 6.5596 | same | PASS / **FAIL** | 0 / 2 of 256 | 0 / 0 |
| | | gain ≥ 4.37 / ≤ 4.83 | 4.4618 / 4.7028 | same | PASS / PASS | 0 | 0 |
| | | f_low ≤ 30 Hz | 26.790 | same | PASS | 0 | 0 |
| hiZ (a) | R1 470k, R2 100k, RC 9.1k, C_in 100n, VAF 100 | gain ≥ 8.36 | 4.0104 | same | **FAIL** | 61 / 256 | 3.9% |
| | | gain ≤ 9.24 | 9.0404 | same | PASS | 0 | 0 |
| | | VC ≥ 1.2 | 1.2875 (saturated) | same | PASS | 0 | 0 |
| | | f_low ≤ 35 Hz | 834.3 | same | **FAIL** | 127 / 256 | 17.7% |
| pq (c) | RC 3.6k | P_Q ≤ 8.70 mW | **8.7117** (interior) | 8.7088 | **FAIL** | 8 / 256 | 0 |
| band (e) | + C_L 690 pF ± 10% | min over 100 Hz…10 kHz of \|H\| ≥ 4.33 | 4.3136 | same | **FAIL** | 16 / 512 | 0 |
| opamp_thd | red team's clipping stage as a B-source | THD < 1% | 3.2893% | same | **FAIL** | 4 / 32 | 0.1% |
| **bjt_thd** (new) | MVP stage, 1 kHz sine, amplitude A 0.35…0.60 V a 9th knob, VAF 100 | THD < 1% | 1.4578% | same | **FAIL** | 4 / 512 | 0 |
| **tuned5** (new) | RE split 180 + 820 Ω; series LC across the 820 Ω, resonance 3% above 1 kHz, L and C ± 5%, Z₀ = 5 kΩ | gain ≤ 20 | **24.426** (interior) | 15.863 | **FAIL** | 0 / 1024 | 15.3% |
| **tuned20** (new) | same, Z₀ = 20 kΩ (sharper) | gain ≤ 10 | **24.426** (interior) | 6.683 | **FAIL** | 0 / 1024 | 14.5% |
| **diffamp** (new) | difference amplifier, R ± 0.1%, A_OL 50k…200k | \|A_cm\| ≤ 3 mV/V | 3.6428 | same | **FAIL** | 16 / 128 | 0 |
| **mvp_deadT** (new) | MVP netlist without `.temp` | same 5 sides | judged against the MVP key | | 1 FAIL, 4 PASS | | |

Round 2's traps reproduce on ngspice almost number for number: P_Q corner worst 8.7088 mW (round 2: 8.7088), band 4.3136 (4.3126), THD 3.2893% (3.2894%). Its hiZ variant still saturates at 62 of 256 corners.

---

## 3. False PASSes on ngspice (`worst_case`)

### 3.1 The headline table

21 spec sides, 11 truly failing. `$S/suite.py`, one seed for the audit (§5 repeats it over 30 seeds). Every configuration uses the same verdict table (§7.2), the same band re-runs and ngspice's default tolerances.

| Configuration | Right | UNDECIDED | **False PASS** | Which | Runs: mvp / hiZ / pq / band / thd(op-amp) / thd(BJT) / tuned5 / tuned20 / diffamp / deadT |
|---|---|---|---|---|---|
| (a) unguarded (engine.md §5.1, no net, no pooling) | 11 | 5 | **5** | hiZ gain, band, opamp_thd, bjt_thd, deadT VC max | 99 / 90 / 36 / 30 / 12 / 30 / 55 / 55 / 24 / 90 |
| (c1) unguarded + range corners | 12 | 5 | **4** | hiZ gain, band, bjt_thd, deadT | 67 / 67 / 40 / 34 / 31 / 38 / 59 / 59 / 28 / 58 |
| (c2) unguarded + margin guard | 13 | 6 | **2** | band, deadT | 63 / 63 / 36 / 30 / 13 / 31 / 55 / 55 / 24 / 54 |
| (c3) unguarded + 32-point audit | 16 | 3 | **2** | bjt_thd, deadT | 94 / 202 / 68 / 119 / 86 / 62 / 146 / 146 / 61 / 86 |
| **(b) round 2's guarded loop** | 15 | 4 | **2** | **tuned20, deadT** | 63 / 76 / 33 / 34 / 29 / 48 / 37 / 37 / 30 / 56 |
| guarded + tangent check | 18 | 2 | 1 | deadT | 103 / 100 / 105 / 43 / 34 / 57 / 127 / 125 / 88 / 96 |
| guarded + 32-point audit | 18 | 2 | 1 | deadT | 90 / 102 / 63 / 65 / 52 / 79 / 128 / 127 / 65 / 82 |
| **soundness** = guarded + tangent + 16-point audit (witness/refuter) + read-back | 14 | 7 | **0** (also 0 in 630 verdicts over 30 seeds) | — | **116** / 112 / 119 / 58 / 45 / 73 / 112 / 111 / 95 / 108 |
| soundness without the tangent check | 14 | 7 | 0 | — | 76 / 88 / 47 / 49 / 40 / 64 / 112 / 111 / 95 / 68 |

How to read the soundness row: 7 UNDECIDED = the 5 dead-temperature sides (binding) + `hiZ` gain ≤ 9.24 (bracket) + `hiZ` VC ≥ 1.2 (regime). The two `hiZ` ones are the same honest UNDECIDEDs as in round 2: a design whose gain collapses at some corners. **No configuration ever gave a false FAIL,** because a FAIL is a simulated point.

The false-PASS rate per failing side: unguarded **5/11**, guarded **2/11**, soundness **0/11**. The cost on the MVP: 99, 63 and 116 runs, which at about 1 ms per run is 0.1 s or less.

### 3.2 The four new traps

#### (a) A resonance on the test frequency: flips lose the tangent

The emitter resistor is split. A series LC across its lower part bypasses it at resonance and raises the gain. L and C (± 5% each) move the resonance across 1 kHz. So |H(1 kHz)| peaks in the **interior** of the box, where L·C tunes it exactly, and no corner comes close:

```
 gain at 1 kHz along L, C at its high edge (tuned20, other knobs nominal)
 L: -1.0  -0.6  -0.2  +0.0  +0.2  +0.4  +0.6  +1.0
     5.55  6.50  9.78  15.05 23.53 15.73 10.06  6.57      ← peak between the edges
 best of all 1024 corners: 6.683          true maximum: 24.426
```

Round 2's guarded loop on "gain ≤ 10":
- The nominal line points to L+ C+ (slopes +1.39 and +1.48 per unit ε). The first jump is predicted at 8.50 and simulates 6.68.
- One flip (R1) improves it by 0.003, and at that vertex every flip is worse. True: the other L and C edges are further off resonance.
- The certificate holds; I = 6.683, e_obs = 1.82, margin 3.32 > 1.82 → **PASS (estimated). Truth: 24.4.**
- The margin/e_obs ratio is 1.82, inside round 2's range of true PASSes (1.71…46). That confirms C6: no multiplier helps.

At the certified vertex, the **tangent** (a small inward nudge) says "inward is better" for L and C. That is exactly what the flip test can't say. A golden-section search along L, then an interior ascent, finds 24.42 → FAIL. It costs 8 nudges per side plus about 10 runs per improving knob. The 16-point audit also catches it, in 30 of 30 seeds (§5): 14.5% of the box fails outright, and 29% beats the loop's 6.68, which is enough to refute the certificate. With Z₀ = 5 kΩ (tuned5) the resonance is broad: e_obs = 9.55 covers it, and the guarded loop says UNDECIDED (bracket), which is honest but not decided.

#### (b) Clipping during the swing, on the real BJT

At the key's worst corner (hot, VCC low, R1−, R2+, RC+, RE−, β+, A = 0.60 V), q1 saturates on the negative output peaks: THD 1.4578%. The unguarded loop reports **0.274%, PASS**. The reason is physical. The smooth distortion of a degenerated BJT *falls* when IC rises (more loop gain g_m·R_E), while clipping *rises* with IC (less headroom). So the nominal THD slopes point away from the limiter:

| Knob | THD slope at nominal (%/ε) | What clipping needs |
|---|---|---|
| R1 | +0.0040 (loop picks R1+) | R1− |
| R2 | −0.0039 (loop picks R2−) | R2+ |
| β | −0.0031 (loop picks β−) | β+ |
| A | +0.0465 | A+ |

This is the red team's behavioural trap, now in a real transistor. **The DC margin can't see it:** q1's operating point at that corner has m_sat = 2.995 V. The large-signal margin (the minimum over the swing of VCE − Vt·ln(2·10⁴)) is nominally 1.217 V. Its line from the nominal nudges predicts −0.447 V at *exactly* the key's corner. One run there simulates −0.169 V, region saturated, THD 1.4578% → FAIL. Removing the margin guard from the guarded loop brings the false PASS back.

#### (c) A kink at nominal: the difference amplifier

|A_cm| is zero at nominal (R2/R1 = R4/R3) and grows with the mismatch in either direction. Every forward nudge raises it, so every nominal slope is positive, whatever the sign of the knob's real effect.
- **The unguarded loop** walks to the all-plus corner, where the mismatches cancel again (|A_cm| = 0.0009 mV/V), and cycles.
- **The guarded loop** reaches 1.82 and cycles.
- **Both** say UNDECIDED (search), which is honest.
- **The tangent check** finds 3.6428, the key's value → FAIL. In the full soundness configuration this side is FAIL in 26 of 30 audit seeds and UNDECIDED (search) in 4: an audit refutation moved the best point off a vertex, so the tangent check didn't run. That is sound but less decisive, and worth fixing in M3.

A lesson from building it: my first version reported 3.6436, *beyond* the key. A nudge taken from an interior point just below an edge had stepped 0.1% outside the box, and pooling took that run as the inner bound. The verdict (FAIL against 3.0) happened to be right, but the number came from a board that can't exist. Round 2's key assertion A3 ("the loop beats the key → investigate") caught it. Now nudges step inward whenever a forward step would leave the box, and pooling ignores every point outside the confidence region. **Pooling must filter by region**, and an M3 test should assert it.

This is a common spec (CMRR). A search that starts at a zero of the measure must expect cycles.

#### (d) A binding error: the temperature knob silently dead

The netlist lacks `.temp {temp_c}`. ngspice runs every point at its default 27 °C, and `alterparam temp_c=…` changes a parameter nobody reads.
- Every configuration without read-back reports bias max **6.18 V, PASS (estimated)**. The real circuit reaches 6.56 V at −10 °C: FAIL.
- The four "right" sides are right by accident. VC min is reported as 4.99 V; the truth is 4.69 V.
- **Read-back catches it at zero extra runs:** `@q1[temp]` reads 27 °C when −10 °C was asked for (mismatch 3.7), and every side becomes UNDECIDED (binding).

A "dead-knob" heuristic (all nudges bit-identical) would also flag it. But it false-alarms on knobs that are legitimately inert: T in the difference amplifier moves nothing. Read-back doesn't: `@r1[temp]` confirms that T was bound.

### 3.3 Which guards are non-negotiable

Each guard is removed alone from the **soundness configuration**, and all 21 sides are re-run over 30 audit seeds: 630 verdicts per row (`$S/exp_ablation_seeds.py`). The full configuration gives 0 false PASSes. The 214 UNDECIDED are the 7 per seed of §3.1, plus `diffamp` UNDECIDED (search) in 4 of 30 seeds (FAIL in the rest).

| Removed | False PASSes in 630 | Where | MVP runs | Verdict |
|---|---|---|---|---|
| nothing | **0** | — | 117 | — |
| flips at vertices (C2) | 19 | band, 19 of 30 seeds | 85 | **non-negotiable** |
| below-noise knobs to an edge (C5) | 19 | band, 19 of 30 seeds | 100 | **non-negotiable** |
| margin guard (C3), large-signal for transient specs | 27 | bjt_thd 24, hiZ gain 3 | 117 | **non-negotiable** |
| read-back of knob values | 30 | deadT bias max, every seed (no point of that netlist exceeds 6.18 V, so the audit can't help) | 117 | **non-negotiable** |
| tangent check | 0 | the audit covers the tuned stage | 77 | keep one of the two |
| audit | 0 | the tangent check covers it | 103 | keep one of the two |
| tangent check **and** audit | 30 | tuned20, every seed | 63 | **one is non-negotiable** |
| range corners + interior temperatures | 0 | — | 111 | cheap; keep for models that report no margins |
| plausibility check, worker isolation | (no case trips them; §2.2 #4–5 shows ngspice needs them) | | 0 | **non-negotiable** |

Why keep both the tangent check and the audit although either suffices here: they cover different shapes (§5.4), and together they add about 50 runs, or 50 ms, to the MVP.

---

## 4. `sigma(3)` needs its own guards

`$S/exp_sigma_adv.py`, `exp_sigma_band.py`. Keys search range corners × the whole 3σ ball (1,500 points per corner, half on the sphere, half in the volume, then a capped coordinate search). The loop is round 2's k-σ point with the exact map. The worst range corner comes from the shared round, and the answer is cross-evaluated at the other corners.

| Case, side | Key at `sigma(3)` | Loop alone | + σ-ball margin guard | + 16-point σ audit (10 seeds) |
|---|---|---|---|---|
| hiZ gain ≥ 8.36 | **5.497 FAIL** (saturates at T+, VCC+, β ≈ 288) | **8.639 PASS: false** | 5.541 FAIL | FAIL ×10 |
| hiZ f_low ≤ 35 | 508.9 FAIL | 39.99 FAIL | 485.7 FAIL | FAIL ×10 |
| bjt_thd < 1% | 0.2579 PASS (clipping needs parts at their edges, outside the ball) | 0.2579 PASS | PASS | PASS ×10 |
| tuned20 ≤ 10 | 23.97 FAIL | 17.55 FAIL | FAIL | FAIL ×10 |
| opamp_thd < 1% | 3.055 FAIL | 3.055 FAIL | FAIL | FAIL ×10 |
| band ≥ 4.33 | 4.4170 PASS | 4.4262 PASS | PASS | PASS ×10 |
| pq ≤ 8.70 mW | 8.6869 PASS | 8.6868 PASS | PASS | PASS ×10 |

Two findings:
1. **The margin guard over the σ-ball is non-negotiable.** It minimizes q1's margin line over the range corners × the 3σ ball, using the same KKT solve as the k-σ point, and simulates the minimum. This answers round 2's open question 4. Flips don't exist for statistical knobs, so nothing else looks at the saturating direction.
2. **Band minima need one sub-search per band edge.** On `band`, the loop's `sigma(3)` answer 4.4262 misses the true 4.4170, and its bracket [4.4259, 4.4262] doesn't contain the truth. With a bound of 4.42 that is a **false PASS**, and a 16-point σ audit misses it in **30 of 30** seeds. Searching |H(100 Hz)| and |H(10 kHz)| separately (round-2 guards §1.1 e, Danskin) finds 4.4169 → FAIL, in 55 runs for both. Under `worst_case`, flips reached the other edge, so the guarded loop didn't need it there. Under `sigma(k)` it's mandatory.

One honest caveat: on `bjt_thd` the loop found 0.25792 where my ball key says 0.25788. **The key is itself a search** and missed a slightly worse point. Round 2's assertion A3 ("the loop beats the key → fix the key") fired, as designed.

---

## 5. The audit sample

**What it is.** A seeded sample drawn once per check: half random vertices, half a Latin hypercube of the interior (16 points in all). Under `sigma(k)`: random range corners × random points on the k-sphere. It is used for exactly two things:
- **witness:** an audit point that fails the spec is a counterexample (FAIL);
- **refuter:** an audit point worse than the search's answer refutes the search's certificate. The search restarts from it with an interior ascent, and the side can't be PASS unless that ascent's result still passes and nothing else is open.

### 5.1 What it can find: analytic, from the keys

The probability that the audit contains a failing point. The vertex part is drawn without replacement from the key's corner table. The interior part hits the key's failing volume.

| Side | Failing corners | Failing volume | n=16: vertices / interior / mixed | n=32 mixed | n=64 mixed |
|---|---|---|---|---|---|
| mvp VC ≤ 6.5 | 2/256 | 0 | 0.12 / 0.00 / 0.06 | 0.12 | 0.23 |
| hiZ gain ≥ 8.36 | 61/256 | 3.9% | 0.99 / 0.47 / 0.92 | 0.99 | 1.00 |
| pq P_Q ≤ 8.70 | 8/256 | 0 | 0.41 / 0.00 / 0.23 | 0.41 | 0.66 |
| band ≥ 4.33 | 16/512 | 0 | 0.40 / 0.00 / 0.23 | 0.40 | 0.65 |
| opamp THD | 4/32 | 0.1% | 0.95 / 0.02 / 0.71 | 0.95 | 1.00 |
| bjt THD | 4/512 | 0 | 0.12 / 0.00 / 0.06 | 0.12 | 0.23 |
| tuned20 ≤ 10 | 0/1024 | 14.5% | 0.00 / 0.92 / 0.71 | 0.92 | 0.99 |

- **Interior-only samples never see a corner failure.** Four of the nine failing sides have zero failing volume, and a fifth has 0.1%.
- **Vertex-only samples never see an interior one:** tuned has no failing corner at all.
- **Mixed is the only sensible design.**
- **Needles stay needles.** Bias max (2 failing corners of 256) and the BJT THD (4 of 512) are found 6–12% of the time. Only structure finds those: flips, margins, corners.

### 5.2 End to end over 30 seeds

Audit points used as witnesses and refuters only (15 sides × 30 seeds = 450 verdicts per row, dead-temperature case excluded):

| Configuration | Right | UNDECIDED | False PASS |
|---|---|---|---|
| unguarded + audit 32 | 368 | 53 | **29** (band 10 of 30, bjt_thd 19 of 30) |
| guarded + audit 16 | 390 | 60 | **0** |
| guarded + audit 32 | 390 | 60 | 0 |
| guarded + audit 64 | 390 | 60 | 0 |

The 60 UNDECIDED are the two `hiZ` sides in every seed. In every seed the audit refuted the tuned-stage search and the ascent found the peak (FAIL), even at n = 16: a refutation needs only a point better than 6.68, and 29% of the box is.

### 5.3 Witness and refuter, never an error sample

My first version also used audit points as free error samples of the nominal line (e_obs). The audit then made true PASSes UNDECIDED:
- MVP VC ≥ 4.5 went UNDECIDED (bracket) in **12 of 30** seeds at n = 16 and **21 of 30** at n = 32;
- the reason is the nominal line's miss at far corners (up to 0.16 V), against a 0.19 V margin.

e_obs then depends on which points happen to be drawn, and the verdict isn't stable across seeds. **The audit must never enter e_obs.** Its evidence is binary: it refutes or witnesses, or it doesn't.

### 5.4 Cost, and why keep both the tangent check and the audit

- **Cost.** The audit costs 16 runs (about 16 ms), shared by every spec side, plus an interior ascent (about 30–60 runs) only when it refutes.
- **What the tangent check covers.** It is deterministic, but local: interior maxima next to the certified vertex, along single knobs (pq: 8.7117; tuned; diffamp).
- **What the audit covers.** It is global, but probabilistic. Its 8 interior points hit a region filling 25% of the box with 90% probability; the tuned stage's refutation region is 29%. Its 8 vertices do the same for failures shared by 25% of the corners.
- **Neither catches** one bad corner in 512. The margin guard did that (bjt_thd).

---

## 6. The numerical band on ngspice

`$S/exp_noise.py`. Truth = the same run at reltol 1e-9, vntol 1e-12, abstol 1e-15.

| Quantity | ngspice defaults (reltol 1e-3, vntol 1e-6, abstol 1e-12; `cktinit.c:49-52`) | Engine settings (reltol 1e-6, vntol 1e-9) |
|---|---|---|
| Max error over 256 corners: VC / \|H(1k)\| / f_low | 9.5e-5 V / 7.7e-5 / 3.9e-5 Hz | 6.0e-13 V / 6.6e-14 / 6.0e-14 Hz |
| Time per run (op + 1 kHz + sweep + refinement) | 0.788 ms | **0.794 ms** |
| Forward-difference slope error, h = 1e-3: gain vs T / gain vs VCC | 1.2% / 0.36% | 0.02% / 0.007% |
| Noise-table estimate (7th differences, 9 points 1e-4 apart; the ECnoise idea) | **2e-13 V** | 2e-13 V |

Three consequences:
1. **Engine settings are free on ngspice. Use them always.** ngspice's test is on the Newton update (`niconv.c:56-69`: |new − old| ≤ reltol·max + vntol). Newton converges quadratically, so tightening costs one or two iterations out of a 0.8 ms run.
2. **Solver error isn't noise.** At default settings the error is a smooth bias of up to 9.5e-5 V that a local difference table can't see: it estimates 2e-13. Round 2 proposed ECnoise for external backends (affine §5.1). On ngspice it would report a band 10⁸ times too small. **Estimate the band by re-running, not by differencing.**
3. **The band recipe:** ε_num = max(2·|f(engine) − f(reltol 1e-9)| + the measure's own band, 1e-9·|f|), from one cold re-run of each decisive point.
   - The measure's own band: the last secant step for f_low. For transient measures, also re-run at half the time step, because reltol doesn't cover time discretization.
   - Measured values: 6.8e-5 V at the VC max counterexample at default tolerances, 6.6e-9 V (the floor) at engine settings.
   - The MVP's closest call is bias max, 0.0595 V beyond its bound. Against that, the band decides nothing at engine settings. It matters only when a spec edge is within about 1e-4 of a worst value at default settings. That is exactly the case the numerics row exists for.

---

## 7. What each verdict may claim

### 7.1 PASS (estimated): what it claims, and what it doesn't

A PASS (estimated) on one spec side claims all of the following, and the JSON carries each as a checkable field:

| # | Claim | Checked by |
|---|---|---|
| P1 | Every run it rests on succeeded, was plausible, and **saw the requested knob values** | status + plausibility + read-back |
| P2 | The worst value found, I, was **simulated** at x_I and reproduces cold at reltol 1e-9 within ε_num | band re-run |
| P3 | x_I is **locally certified**: no single-knob flip and no small single-knob inward move makes it worse | 1-flip certificate + tangent check |
| P4 | **No independent point contradicts it**: no safety-net, margin-guard or audit point is worse than I | pooling + refutation |
| P5 | No device changes regime at x_I, and no margin line predicts a crossing that wasn't simulated | region at x_I + margin guard |
| P6 | The margin exceeds the largest line error the check saw, plus ε_num | the C6 rule |

It **does not** claim:
- that no worse point exists (the problem is NP-hard: worst-case report §1.3);
- any probability, under `worst_case`;
- anything about effects the device model lacks.

That's why it stays "estimated" and never becomes "guaranteed" (round 2: guaranteed only from exact methods).

**Should a PASS that survives escalation get its own verdict word?** No. It is still PASS (estimated), with a stronger evidence line (§7.3).

### 7.2 The verdict table, revised

For an upper bound B (a lower bound mirrors it). The first matching row wins.

| # | Condition | Verdict | Change vs round 2 |
|---|---|---|---|
| 0 | a run's read-back differs from the requested knob value | **UNDECIDED (binding)**: nothing stands, not even a FAIL (we don't know which point was simulated) | **new** |
| 1 | I > B + ε_num | **FAIL** + counterexample | — |
| 2 | \|I − B\| ≤ ε_num | UNDECIDED (numerics) | — |
| 3 | a needed run failed or was implausible (after the retry ladder) | UNDECIDED (simulator) | — |
| 4 | the worst point's region ≠ nominal's | UNDECIDED (regime) | — |
| 5 | cycle, budget, **or a net/audit point refuted the search** and the ascent didn't settle it | UNDECIDED (search) | refutation **new** |
| 6 | B − I > e_obs + ε_num, with e_obs from jumps and safety-net points only | **PASS (estimated)** | e_obs inputs pinned |
| 7 | otherwise | UNDECIDED (bracket) | — |

Row 0 is the one place where a guard blocks a FAIL. The simulated point may not be the reported one (with a dead knob, even the corner coordinates are wrong), so the counterexample can't be pasted.

### 7.3 What the output says

**The six UNDECIDED reasons.** Each comes with the step that would decide it, so an UNDECIDED is never a dead end:

| Reason | The engine says | Escalation | Cost at ngspice speed (MVP) |
|---|---|---|---|
| binding | "ngspice ran `temp` at 27 °C when −10 °C was asked for. The netlist doesn't bind this knob" | fix the export (a bug, never a user problem) | — |
| simulator | "the run at ⟨point⟩ failed: ⟨ngspice text⟩" | retry ladder (restart the worker, then gmin/source stepping, then continuation from nominal) | 1–10 runs |
| numerics | "6.5000 V vs 6.5 V: inside the solver's band of 7e-9 V" | tighter tolerances; else stays | 1 run |
| regime | "the worst point (1.2875 V) has q1 saturated; lines across that kink mean nothing" | per-region search (later); the user decides | — |
| search | "a point the search didn't reach is worse: ⟨point, value⟩" | exhaustive corners + ascent (§7.4) | ~520 runs, 0.3 s |
| bracket | "worst found 6.4635 V at ⟨corner⟩, certified locally; the line missed by up to 0.18 V elsewhere, so 6.5 V is inside the bracket" | exhaustive corners + ascent (`worst_case`); ball search (`sigma(k)`) | 0.3 s / 6.7 s |

**Tags and the evidence line.** The existing tags (model-conditional, relies-on-…) stay. Every verdict also carries an evidence line, and the JSON a matching `evidence` block:

```
bias ≥ 4.5 V   PASS (estimated)   worst 4.6921 V at T+ VCC- R1- R2+ RC+ RE- beta+   bracket [4.6257, 4.6921]
               evidence: 1-flip + tangent certificate · audit 16/16 clean (seed 1) · q1 margin predicted ≥ 2.93 V
                         · knobs bound (read-back) · band 5e-9 V · 116 runs
bias ≤ 6.5 V   FAIL               6.5595 V at T- VCC+ R1+ R2- RC- RE+ beta-  (reproduces at reltol 1e-9 within 7e-9 V)
```

### 7.4 Escalation: exhaustive corners decide the pessimistic UNDECIDEDs

On the MVP the loop's inner bound **equals the key on every side**. So its UNDECIDEDs on smooth designs come from e_obs, which is set by the nominal jump, not from a missed point. The fix candidates of §9 show it (`$S/exp_escalate.py`):

| Candidate, side | Loop | Escalation: 256 corners + ascent from the best 3 | Result |
|---|---|---|---|
| R1 46.4k, VC ≤ 6.5 | 6.4635, e_obs 0.18 → UNDECIDED (bracket) | 6.46347 (= the loop's), 520 runs, 330 ms | PASS (estimated; all corners) |
| R2 10.2k, VC ≤ 6.5 / ≥ 4.5 | 6.4218 / 4.5421 → both UNDECIDED | 6.42180 / 4.54215, 520 runs each | PASS / PASS |
| 2N3904, f_low ≤ 30 | 27.78 → UNDECIDED | 27.78234, 592 runs, 361 ms | PASS |
| 2N3904, bias max at `sigma(3)` | 6.4779 → UNDECIDED | sphere search, 10,524 runs, 6.7 s: 6.47793 | PASS |

Exhaustive corners are affordable up to about 2¹² corners (≈ 4 s on one core, < 0.3 s on 16). Beyond that, UNDECIDED stays UNDECIDED unless the user asks for sampling.

At MVP scale the 256 corners cost about as much as the check itself (256 runs against 156–204). So a soundness-first MVP should also run them as a **self-check** whenever 2^N ≤ 256, and report any disagreement with the loop as an engine bug. That puts the answer key in production.

---

## 8. The MVP check on ngspice

`$S/check_mvp.py`, engine tolerances, keys recomputed at engine tolerances (`mvp_eng`).

```
$ spicy check circuits/ce_amp.spl                      (prototype output, soundness configuration)
 spec   side        worst_case                          sigma(3)
 bias   ≥ 4.5 V     4.6921 V  PASS (estimated)          4.8843 V  PASS (nested)
 bias   ≤ 6.5 V     6.5595 V  FAIL                      6.2813 V  PASS (estimated)
        counterexample: temp=-10°C vcc.v=12.6V r1=+1% r2=-1% rc=-1% re=+1% q1.beta=100
 gain   ≥ 4.37      4.4618    PASS (estimated)          4.5177    PASS (nested)
 gain   ≤ 4.83      4.7028    PASS (estimated)          4.6635    PASS (nested)
 bass   ≤ 30 Hz     26.790 Hz PASS (estimated)          24.973 Hz PASS (nested)
 headline (sigma(3)): 3 PASS · bias fails only with every part at its edge (worst_case FAIL)
 199 runs + 5 band re-runs · 0.27 s (ngspice 0.13 s) · knobs bound · no regime change · audit clean
```

| Side | Engine `worst_case` | Key (256 corners + 1,000 points + search) | Engine `sigma(3)` | Key (sphere search, ~10k runs) |
|---|---|---|---|---|
| VC ≥ 4.5 | 4.69211 | 4.69211 | 4.88426 | 4.88424 |
| VC ≤ 6.5 | 6.55954 | 6.55954 | 6.28130 | 6.28132 |
| gain ≥ 4.37 | 4.46177 | 4.46177 | 4.51767 | 4.51762 |
| gain ≤ 4.83 | 4.70283 | 4.70283 | 4.66349 | 4.66355 |
| f_low ≤ 30 | 26.78972 | 26.7897 | 24.97348 | 24.97348 |

- **The verdicts match round 2's.** The numbers are ngspice's: bias max 6.5595 V here, 6.5905 V in the Python model.
- **`sigma(3)` values sit within 6e-5 of the key.** The k-σ loop stops when prediction and simulation agree to 1e-4 relative, and e_obs covers the gap.

**Cost by stage:**

| Stage | Round 2's guarded loop | Soundness configuration |
|---|---|---|
| Shared round (nominal, 8 nudges, 4 range corners, 2 temperatures, margin guard) | 15 | 15 |
| Audit | — | 16 |
| `worst_case`, 5 sides | 48 | 85 (tangent checks: +37) |
| `sigma(3)`, 5 sides (D7) | 67 | 83 (σ audit: +16) |
| Band re-runs | 5 | 5 |
| **Total** | **135 runs, 0.22 s** | **204 runs, 0.27 s** |

With the nesting shortcut (σ only where `worst_case` fails), the soundness check is 156 runs in 0.14–0.19 s (`flows.py`, two runs). Wall time beyond the runs is Python arithmetic, mostly the KKT solve of the k-σ point.

---

## 9. Flows

Measured costs are on this CE amp, in process, one core (`$S/flows.py`, two runs). Per run: 0.6–1.1 ms in process, 4.9–5.6 ms as an `ngspice -b` subprocess, 18–19 ms for a fresh worker. At 1,000 nodes, op + a 111-point AC sweep takes 10.7 ms (`exp_scale.py`), so the same check takes about 2 s.

### 9.1 The cold check

```
 spicy check ce_amp.spl
   │ front-end (M1f) → netlist: knobs as .param, .temp {temp_c}, .options tnom reltol vntol
   │                 + knob table (ε ↔ value) + read-back paths (@q1[temp], @r1[resistance], …)
   ▼
 worker process: libngspice, circuit loaded once                        19 ms, once
   ▼
 SHARED ROUND  nominal + 8 nudges · 4 range corners · 2 temperatures      15 runs
               margin guard: q1 margin line → min over box (and σ-ball)    0–2 runs
 AUDIT         8 random vertices + 8 LHS points, seeded                   16 runs
   │           every run: status · plausibility · read-back · region · margins
   ▼
 worst_case, per side (state machines, merged into rounds)
   jump → flips at vertex → move while better → 1-flip certificate
   → tangent check at the final vertex → refuted by a net/audit point? → ascent      ~85 runs
   ▼
 sigma(3), per side: worst range corner → k-σ point (exact map), per band edge
   → cross-check other corners · nested sides skip (worst_case PASS ⇒ σ PASS)        ~15–83 runs
   ▼
 band: cold re-run of each decisive point at reltol 1e-9                              5 runs
   ▼
 verdict table (§7.2) ── UNDECIDED (bracket|search) and 2^N ≤ 4096? ── escalate: all corners + ascent
   ▼                                                                    (+520 runs, +0.3 s)
 table + JSON { verdict, value, bracket, counterexample, evidence, tags, cost }
                                               total 156–204 runs · 0.14–0.27 s
```

### 9.2 The agent's flows

Every agent statement is one of: **verified** (a check on this revision), **point** (a real simulated value at a real point, which may prove FAIL, never PASS), or **arithmetic** (the agent's own, labelled). There are no "predicted" verdicts at this scale.

**"Why does bias fail?"** (`engine.explain`, 0 runs)

```
 agent ──explain(bias, max, worst_case)──► engine: stored record (no runs)
       ◄── counterexample + flip table at x_I (cached by the certificate)
 flip T:    VC 5.8578 (+0.702 V, 35% of the excursion)     flip beta: 6.1395 (+0.420, 21%)
 flip VCC:  6.2049 (+0.355, 18%)   R1 7.5%   R2 7.0%   RC 6.1%   RE 5.5%   C_in 0
 agent may say: "fails at 6.56 V when cold, on a high supply, β = 100, with the divider and
   resistors at their edges (verified). Cold is the largest single contributor: 35% of the summed one-flip effects."
```

The flip effects are exact secants (real runs), not slopes. They are the contributors this engine can stand behind.

**What-if on an edit, then "fix bias"** (`what_if` → `verify` → maybe escalate)

```
 agent ──propose(R1 47k → 46.4k)──► lang ──► proposal p1
 agent ──what_if(p1)──► engine: re-simulate the 5 stored decisive points      5 runs, 4–6 ms
       ◄── bias max at the old worst corner 6.4635 V (point)
 agent may say: "at the old worst corner, bias max is now 6.46 V (one simulation, not a verdict)"
 agent ──verify(p1)──► engine: full check                              156 runs, 0.14–0.21 s
       ◄── bias max UNDECIDED (bracket): 6.4635, line error 0.18 V
 agent ──verify(p1, escalate)──► all 256 corners + ascent                    +520 runs, +0.33 s
       ◄── PASS (estimated; all corners), all specs, both confidences
 agent may say: "bias passes at worst case with 37 mV to spare (all 256 corners simulated);
   the other specs are unchanged" → shows the diff; the user applies it
```

The other candidates, same flow:
- **R1 45.3k.** The what-if already shows bias min 4.4057 V at a stored point. The agent may say "this breaks the lower bias limit: 4.41 V at ⟨corner⟩" before any verify. That is a proven FAIL.
- **R2 10.2k.** Both bias sides are UNDECIDED (bracket), and escalation gives PASS / PASS.

**"Add a spec: swing ≥ 1 V"** (`propose` → `verify`)

```
 spec swing: q1 saturation margin >= 1 V     (the measure every run already records)
 engine: new side searches the cache first → 0 new runs, 1 ms → PASS (estimated), worst 3.005 V
 agent may say: "passes with 2 V to spare (verified)"
```

The cache serves a new spec only if its measure is already recorded by every run. A spec that needs a new analysis (a transient THD) re-runs everything.

**"Use a real transistor: 2N3904"** (`parts.extract` → `propose` → `verify`)

```
 β 70..300 (hFE ≥ 70 at 1 mA; 300 from the 10 mA row: relies-on-unreviewed-part-data)
 verify: 167 runs, 0.18 s
   bias max   worst_case 6.8017 V FAIL · sigma(3) 6.4779 V UNDECIDED (bracket)
   bass       worst_case 27.78 Hz UNDECIDED (bracket) → escalate (592 runs, 0.36 s) → PASS
 escalate sigma(3) bias: sphere search 10,524 runs, 6.7 s → 6.47793 V → PASS, 22 mV margin
 agent may say: "with the 2N3904's datasheet β, bias fails at worst case (6.80 V) and passes at
   3σ by only 22 mV, given β ≥ 70 extracted by AI and not yet reviewed"
```

### 9.3 Cost and latency per flow (CE amp, ngspice in process, one core)

| Flow | Runs | Latency | May claim |
|---|---|---|---|
| Cold check, both confidences | 156–204 | 0.14–0.27 s | verified verdicts |
| Escalation, `worst_case` side | ~520 | 0.33 s | PASS (estimated; all corners) or FAIL |
| Escalation, `sigma(3)` side | ~10,500 | 6.7 s (< 0.5 s on 16 cores) | same, for σ |
| Explain | 0 | ms | verified record contents |
| What-if (stored points) | 5 | 4–6 ms | FAIL proven at a point; values at points |
| Verify a proposal | 156–180 | 0.14–0.21 s | verified verdicts, full delta |
| Add a spec on recorded measures | 0–20 | ms | verified |
| Worker restart after a netlist error | — | 18–19 ms | the run is `failed`, never a value |

---

## 10. Where soundness bends, and how the output says so

| Bend | Why it can't be avoided | What the output says |
|---|---|---|
| **e_obs is not a bound** (C6) | Finding the maximum over a box is NP-hard; a bound needs exact methods | "PASS (estimated)", always with the bracket and the evidence line; never "guaranteed" from the loop |
| **Needles** | The BJT THD's 4 failing corners of 512 turn up in a 16-point audit only 6% of the time. Only structure finds it: flips, margins, corners. A limiter in a behavioural model with no margin output, reached only by a joint move of 2+ statistical knobs, would be missed | the evidence line lists which structural guards applied. Tag *no margins reported by ⟨model⟩* |
| **Exhaustive escalation stops at ~2¹² corners** | 2^N | above that, UNDECIDED (bracket) stays, with the sampling command to run |
| **The audit is random** | A deterministic grid in 8–20 dimensions is either huge or blind | the seed is fixed per check, so it's reproducible. The line says "audit 16/16 clean (seed s)", never a probability |
| **The model is the model** | ngspice's Gummel–Poon with these parameters has no quasi-saturation (RC = 0), no self-heating, no junction capacitances | tag *model-conditional* with the missing effects. The 10⁻⁴ saturation edge was validated only on this model |
| **Multiple DC solutions** | On ngspice every run is cold and bit-identical (§2.2 #8). A cold re-run can't find a second solution, and ngspice picks one silently | MVP: no feedback in the CE amp. Later: a structural positive-feedback check on the netlist, then UNDECIDED (multi-stable) unless the spec declares a history (open question) |
| **Runs cost more** | +53 runs on the MVP vs round 2 (116 vs 63), about +50 ms | the cost is printed with every verdict. At 1,000 nodes it's about 2 s per check |
| **Pessimism** | a true PASS reported as UNDECIDED (the fix candidates, `hiZ` gain ≤ 9.24) | every UNDECIDED names its escalation and its cost (§7.3) |

---

## 11. Recommendations for the final plan

1. **Adopt "no false PASS on the adversarial suite" as M3's acceptance criterion, with the suite in the repo.** That means the 10 circuits and 21 sides of §2.3, answer keys on ngspice, and the MVP check agreeing with its key. *Reason:* on ngspice the suite runs in about 1 s on 16 cores. Without it, this round's two false PASSes of round 2's loop would have shipped.
2. **Build M3 on ngspice directly (change D9).** Keep a function backend only for unit tests of the loop's state machines. *Reason:* the MVP backend is decided, and the traps that matter here (tuned, clipping, binding) came from the real simulator and the real netlist path, not from formulas.
3. **Backend contract for ngspice:**
   - knobs bound through `.param` + `alterparam` + `reset`;
   - temperature via `.temp {temp_c}`;
   - `.options tnom=… reltol=1e-6 vntol=1e-9` in the netlist, never as control commands;
   - `destroy all` every run; exact frequencies by `ac lin 1 f f`;
   - one worker process per core, restarted on "cannot recover" or a missing vector.

   *Reason:* §2.2 facts 1–6, each verified; engine tolerances cost 0.8% of run time.
4. **Every run returns status, plausibility, binding and margins.** In detail:
   - plausibility: |V| > 10 × max source is implausible;
   - binding: a read-back of every knob value, where a mismatch > 1e-9 relative makes every verdict of the check UNDECIDED (binding);
   - margins: q1's region and margins, and for transient specs the minimum over the waveform.

   *Reason:* the 1e9 V silent solution, the dead-temperature false PASS, and the DC-blind THD corner.
5. **`worst_case` = round 2's guarded loop plus a tangent check at the final vertex.** At the certified vertex, nudge each knob inward. Any knob that improves gets a golden-section search, then an interior ascent. *Reason:* flips alone gave a false PASS on the tuned stage. Cost: about N runs per side (+37 on the MVP).
6. **A 16-point seeded audit per check (8 random vertices + 8 LHS interior; under `sigma(k)`, range corners × the k-sphere), used as witness and refuter only, never inside e_obs.** *Reason:* 0 false PASSes in 30 seeds with the guarded loop. With audit points in e_obs, a true PASS flips to UNDECIDED in up to 21 of 30 seeds. Cost: 16 runs.
7. **`sigma(k)` gets the σ-ball margin guard and one sub-search per band edge (per local extremum).** *Reason:* a false PASS on `hiZ` without the first, and on `band` without the second (the audit missed it in 30 of 30 seeds).
8. **The verdict table of §7.2:** row 0 (binding) added, refutation added to row 5, e_obs fed by jumps and safety-net points only. *Reason:* §3.2 d and §5.3.
9. **The band from one cold re-run per decisive point at reltol 1e-9 (half the time step for transient measures), with a 1e-9 relative floor.** Never from a noise table. *Reason:* the noise table reports 2e-13 V where the default-tolerance error is 9.5e-5 V.
10. **Every UNDECIDED names its escalation and its cost.** At MVP scale the engine runs all 2^N corners as a **self-check** whenever 2^N ≤ 256, and reports any disagreement with the loop as an engine bug. *Reason:* 0.3 s. It decided all 4 bracket UNDECIDEDs it was tried on, and the key shows it would decide `hiZ` gain ≤ 9.24 too (corner worst 9.0404). It also keeps the answer key in production.
11. **JSON (D10) gains an `evidence` block:** certificate kind, audit n / seed / result, escalation, binding, band, runs. The agent's verdict words must cite it. *Reason:* the agent must be able to say what a PASS rests on (P1–P6).
12. **The agent never shows a predicted verdict at MVP scale.** What-if = re-simulated stored points (5 runs, FAIL-only), then `verify` (0.2 s). *Reason:* a verified answer costs less than a person's reaction time, so prediction only adds risk (C11).

---

## 12. Open questions

1. **Tangent checks at 50 knobs.** N extra runs per side per certified vertex is 50 × sides. Is the audit enough past some N, or can the tangent come from adjoint AC/DC sensitivities (L1) for free?
2. **Large-signal margins in general.** For the THD stage the margin was "minimum VCE over the swing". What is the general rule for transient specs (op-amp output headroom, LDO dropout during a load step)? Should the device model report margins over time, like ngspice's SOA check (`bjtsoachk.c`), but for regions?
3. **Multi-stable circuits on ngspice.** Every run is cold and bit-identical, so how does the engine find a second DC solution? Candidates: `.nodeset` from both rails, source-stepping from both ends. Or a structural positive-feedback detector plus UNDECIDED (multi-stable).
4. **Keys are searches too.** On `bjt_thd` the loop beat the ball key by 4e-5. What budget makes a `sigma(k)` key trustworthy for tests: more directions, or Monte Carlo with Clopper–Pearson (round 2 §4.2)?
5. **Is e_obs worth keeping?** On the MVP and every fix candidate, the loop's worst equalled the truth while e_obs (0.18 V) blocked PASS. The alternative: PASS (estimated) from the certificates, with refutation and the escalation self-check, and the bracket shown only as information. That is more decisive, but it rests entirely on the guards. Needs a larger circuit set before deciding.
6. **Audit size vs knob count.** 16 points detected the tuned stage's 14.5% failing volume. At 50 knobs, interior failure regions shrink in volume. Should n grow with N, or should the audit concentrate on the directions the loop didn't visit?
7. **Read-back coverage.** Every knob needs a simulator-side path (`@dev[param]`, vector index). For vendor subcircuits the bound parameter may live inside a `.subckt`. Is reading the flattened instance enough, or does the export need to emit a read-back map?
