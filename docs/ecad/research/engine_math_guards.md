# Making the Loop's PASS Trustworthy: Safety Net, Guards, Verdict Logic, Adversarial Tests

> 2026-09-27 · Research for roadmap M3 (the `worst_case` loop, M3c, and its answer key, M3b).
> Reads with: `../engine.md` §3.2–3.4 and §5, `../walkthrough.md` §4–5 and §7.3, `engine_method_redteam.md` (A1–A3, Appendix A), `engine_power.md` (S3–S4, boost), `engine_precision_analog.md` §4.6–4.7.
> Every number below was computed by a script in the scratch directory `/root/.claude/jobs/443154a8/tmp/engine_research/guards/` (outputs in `results/`), or is cited. Section 0 lists the scripts.

## Summary

A linearized worst-case loop can report a confident PASS while a real board fails. We built three adversarial variants of the walkthrough amplifier (it saturates at some corners; its transistor dissipation peaks inside the box; a load capacitor adds a second band edge), and ran two loops on them, on the MVP amplifier itself and on the red team's clipping-THD stage: the loop as `engine.md` §5.1 describes it, and a guarded loop. The unguarded loop gives **three false PASSes** (saturation that the gain spec's slopes point away from; a band spec whose second extremum depends on a knob with a below-noise slope; the red team's clipping THD). The guarded loop gives **no wrong verdict** on the same eleven spec sides, and on the MVP amplifier it costs **61 simulations**, the same as the unguarded loop (63), because its main change (re-linearizing at a vertex by *flipping* each knob to its other edge instead of *nudging* it) costs the same N runs. Three guards were each necessary on this suite: **flips at vertices**, **never leaving a below-noise knob at nominal**, and a **saturation-margin guard** that searches the device's smooth margin with the lines the loop already has (0 extra runs on the MVP, 1 on the variant that saturates). "Prediction ≈ simulation" turns out to be vacuous at convergence (the final prediction is the line's value at its own anchor point), and no margin multiplier separates false PASSes from true ones (false PASSes had margin/error ratios of 1.85, 3.0 and 2,521; the MVP's true PASSes go down to 1.71). So the verdict rule stays simple (margin > largest observed line error + numerical band), and the guards carry the weight. Our simulator needs four fixes before the guards can work: per-device operating-point records, errors instead of panics in DC sweep and AC, detection of absurd solutions (it silently returns −7.3·10²⁶ V for a circuit with no DC solution), and the Ebers–Moll reciprocity bug in `bjt.rs` (VCE,sat 48 mV where ngspice's equations give 66 mV). The MVP's `worst_case` check then costs about **68 runs** in total against 256 for brute force.

---

## 0. What was done, and how to re-run it

| Script | What it does | Checked against |
|---|---|---|
| `ce_amp_model.py` | Copy of the shared reference model (walkthrough appendix) | Reproduces every walkthrough number |
| `em_amp.py` | The same amplifier with a full Ebers–Moll BJT: DC operating point by limited Newton (residual < 1e-15 A), region and margins, small-signal H(f) from the linearized junctions. Two current forms: `transport` (ngspice's Gummel–Poon at qb = 1, `bjtload.c:622-623`) and `ours` (`bjt.rs:133-135`). Temperature law of the walkthrough appendix | Transport form, forward active: VC = 5.499905 V, identical to the reference model. \|H(1 kHz)\| = 4.590813 vs the walkthrough's mid-band 4.591739 (the factor 1/√(1 + (f_L/1 kHz)²) = 0.99980) |
| `newton_replica.py` | Our DC Newton, copied: zero initial guess (`dc.rs:66`), ±40·n·Vt clamp (`bjt.rs:93-106`), step-size convergence test with abs 1e-6 / rel 1e-3 on every unknown (`trans.rs:46-59`, `lib.rs:39-47`), 50 iterations | Matches the real binary to 2.4e-7 V (its float32 output) at 128 corner points |
| `check_sim.py` | Real binary vs the `ours` form at the 64 DC corners + nominal | Max difference 4.5e-5 V: the binary's Newton error at rel_tol 1e-3 |
| `loop.py` | A minimal loop (nominal + nudges, jump to the line's vertex, re-linearize, stop when the vertex repeats) and the brute-force key (corners + random + coordinate refinement) | — |
| `guarded.py` | The guarded loop and verdict rule proposed in §2–§3 | — |
| `variants.py` | Knob mappings for the MVP and the variants | — |
| `exp0_mvp.py`, `exp_a.py` … `exp_g.py`, `exp_thd.py`, `exp_numerics.py`, `run_all.py`, `ablation.py` | One experiment each (§1–§3) | The brute-force key |

**Conventions.** Knob positions ε ∈ [−1, +1] (low edge … high edge), the 8 MVP knobs of roadmap §4.2. One model evaluation = one "simulation"; a point simulated twice is counted once (the engine will cache). Nudges are forward differences of 0.01 in ε. "The unguarded loop" below is `engine.md` §5.1 steps 1–4 without step 0 and without the §5.2 guards, with the L0 convention that a slope below 10⁻³ of the largest slope is noise, so that knob stays where it is (the red team's convention in its Appendix A.2).

---

## 1. How a linearized loop goes wrong

### 1.0 First, a trap in the stopping rule

On the MVP amplifier (reference model), the unguarded loop on VC max runs:

| Round | Line drawn at | Predicts | Simulated | Point |
|---|---|---|---|---|
| 1 | nominal | 6.4076 V | 6.5905 V | T− VCC+ R1+ R2− RC− RE+ β− |
| 2 | that corner | **6.5905** | **6.5905** | same corner |

Round 2's "agreement" is automatic: the new line is drawn *at* the corner, its best vertex is that same corner, and a line's value at its own anchor point is the simulated value. **In a vertex search, "prediction ≈ simulation" at convergence proves nothing.** The only informative comparisons are the *jumps*, where the point moved (round 1: 0.18 V off). `engine.md` §5.1 step 4 ("repeat until prediction ≈ simulation") should say "until the point stops moving", and the evidence for the verdict should be the errors of the jumps (§3.4).

### 1.1 The seven failure shapes

Each subsection: the circuit, what the unguarded loop reports, the truth (brute force), the guard that catches it, and the cost.

#### (a) A limiter that is off at nominal: transistor saturation

**Circuit.** Variant `hiZ`: the walkthrough amplifier with a high-impedance divider (R1 = 470k, R2 = 100k, same ratio) and RC = 9.1k for more gain; C_in = 100 nF. Base current now matters, so IC spreads with β. Nominal: VCE = 1.63 V, \|H(1 kHz)\| = 8.831, forward active. Spec: gain in 8.8 ± 5%, so gain ≥ 8.36.

```
   gain-min vertex (unguarded loop)            saturating corner (truth)
   T− VCC− R1+ R2− RC− RE+ Cin− β−             T+ VCC+ R1− R2+ RC+ RE− Cin− β+
   gain 8.501, VCE 5.14 V, active              gain 3.891, VCE 0.185 V, saturated
            └──────────── 7 of 8 knobs differ ────────────┘
```

The gain's own slopes point away from saturation: lower gain wants low β and a smaller IC, while saturation needs high β and a larger IC.

| Method | Sims | Gain min found | Verdict (spec ≥ 8.36) |
|---|---|---|---|
| Unguarded loop | 18 | 8.5011 (active at every simulated point) | **PASS (estimated)**: margin 0.141 > line error 0.076 |
| 4 range corners, nominal statistical knobs | +4 | 8.809 … 8.847, all active (VCE ≥ 0.481 V) | misses it |
| **Saturation-margin guard** (below) | **+1** | **4.2272**, q1 saturated | **FAIL** + counterexample |
| Guarded loop, full | +24 for this side | 3.8912 (= brute force) | FAIL |
| Brute force, 256 corners + 2,000 random | 2,256 | **3.8912**; 62 of 256 corners fail | **FAIL** |

**The guard.** q1's saturation margin m_sat (§2.4; > 0 means forward active) is an output of every run, so its line comes from the nominal and nudge runs the loop already made, **at no extra cost**. Its slopes (V per unit ε): β −1.49, T −1.04, VCC −0.14, R1 +0.13, R2 −0.10, RC −0.09, RE +0.06. The line predicts min m_sat = −1.678 V at T+ VCC+ R1− R2+ RC+ RE− β+. One simulation there: m_sat = −0.099 V (the margin itself stops falling once the device saturates, so the line's number is wrong, but its sign is right), region saturation, gain 4.2272. FAIL is proven.

**Is nominal-statistical enough at the range corners?** Not here. β (statistical) moves the margin more than temperature does. The margin guard covers the whole box for one run; evaluating each range corner at the margin-worst statistical setting would cost 4.

**Pooling helps for free.** If the design also has a bias spec (VC min), its own search visits the saturating corner in round 1 (VC min means IC max), and that run evaluates the gain too: 4.2272. Every simulated point should update every spec's inner bound.

#### (b) A slope that changes sign across the box

**Circuit.** The MVP amplifier (reference model), gain minimum (`walkthrough.md` §5.7).

| Round | Predicts | Simulated | Point |
|---|---|---|---|
| 1 (line at nominal) | 4.4778 | 4.4688 | **T+** VCC− R1+ R2− RC− RE+ β− |
| 2 (line at that corner) | 4.4666 | **4.4626** | **T−** VCC− R1+ R2− RC− RE+ β− |
| 3 | 4.4626 | 4.4626 | converged (27 sims) |

A one-jump tool (PSpice `.WCASE`, Multisim) stops at 4.4688. Re-linearizing already fixes it; the verdict (≥ 4.37) doesn't change. With flips (§2.6), the fix is visible in the flip table at the hot corner:

| Flip at T+ VCC− R1+ R2− RC− RE+ β− (gain 4.4688) | Gain | |
|---|---|---|
| T → T− | **4.4626** | better by 0.0062: move |
| β → β+ | 4.4981 | worse |
| RC → RC+ / RE → RE− | 4.5590 / 4.5589 | worse |
| VCC, R1, R2 | 4.4813, 4.4710, 4.4709 | worse |

**Guard:** none beyond re-linearizing at the vertex (8 runs, part of the loop). `engine.md` §5.2's "a slope changes sign across the box" doesn't need its own mechanism.

#### (c) An interior extremum and (d) cycling between vertices

**Circuit.** Variant `pq`: RC = 3.6k. Spec: q1's dissipation P_Q = IC·VCE + IB·VBE ≤ 8.70 mW (a derating-style spec; power is outside the corner theorem, `engine.md` §4.2 item 4). P_Q(IC) ≈ IC·(VCC − IC·(RC + RE)) is a parabola with its top at IC* = VCC / (2(RC + RE)) = 1.304 mA, while IC over the box spans about 1.29–1.63 mA.

```
 P_Q ▲            ● interior max 8.7102 mW (β = +0.19, IC 1.390 mA)
     │         ●     ●
     │      ●           ●            at the low-IC vertex the line says "raise IC";
     │   ●                 ●         at the high-IC vertex it says "lower it";
     │ ○ low-IC vertex        ○ high-IC vertex      so the loop jumps back and forth
     └──────────────────────────────────────► IC
```

| Method | Sims | P_Q max | Verdict (≤ 8.70) |
|---|---|---|---|
| Unguarded loop, nudges | 36 | 8.6849, **cycling** between a low-IC vertex (8.6849) and a high-IC vertex (8.4413); predictions 8.73–9.08 | UNDECIDED (bracket): jump errors up to 0.63 mW |
| Unguarded loop, flips | 27 | 8.6849, cycling | same |
| Guarded loop (flips, certificate) | +18 | 8.7088 (the best corner) | **FAIL** |
| Golden-section search along β at that corner | +8 | **8.7102** | FAIL |
| Brute force | 256 corners: 8.7088; + 2,000 random + refinement: **8.7102** | | FAIL |

What this shows:
- **Cycling is the signature of an interior extremum.** Full vertex jumps can't settle on an interior optimum (the red team's §4.2(a) point about SLP).
- **Smooth interior maxima are shallow over a many-knob box.** Among 256 corners there is almost always one near the top of the parabola: the interior beats the best corner by only 0.016%. The precision study found the same (+0.03% on an MFB filter, `engine_precision_analog.md` §3). Interior extrema matter a lot only when **one knob moves the evaluation point**: phase margin vs load capacitance (45.7° at 575 pF against 72° and 84.5° at the ends), a resonance on the test frequency, a ripple peak.
- **The answer key must search the interior for such measures.** 256 corners give the wrong key (8.7088) for P_Q.
- **Guard:** keep the best vertex seen and certify it by flips. Then check the interior when triggered: after a cycle, or when the "worst" is no worse than nominal (PSpice AA's sanity check, red team §3.3). The check is an inward nudge at the best vertex; a knob whose inward step improves the measure gets a golden-section search along it (8 runs here). Cadence's K-Sigma corners does a similar line search from nominal, "up to 11 extra simulations, usually 1" (red team §3.2).

#### (e) Several local extrema over frequency

**Circuit.** Variant `band`: C_L = 690 pF ± 10% at the output (a 9th knob). Spec: for all f in 100 Hz … 10 kHz, \|H(f)\| ≥ 4.33. The band minimum sits at one of two edges: at 100 Hz (set by C_in and R_in) or at 10 kHz (set by RC·C_L). At nominal they nearly tie: 4.5019 vs **4.4993**, so the loop linearizes at 10 kHz.

Slopes at nominal, per unit ε:

| | RC | RE | C_in | β | C_L |
|---|---|---|---|---|---|
| \|H(100 Hz)\| | +0.0450 | −0.0449 | **+0.0348** | +0.0161 | −0.00000 |
| \|H(10 kHz)\| | +0.0432 | −0.0449 | **+0.00000** | +0.0127 | −0.0179 |

At 10 kHz, C_in's slope is below any realistic noise floor.

| Method | Sims | Band min | Verdict (≥ 4.33) |
|---|---|---|---|
| Unguarded loop, exact slopes | 30 | 4.3126 | FAIL (lucky: C_in's slope, 3.6e-6, is 8e-5 of the largest, yet still has the right sign) |
| **Unguarded loop, L0 noise convention** | 30 | **4.3566**: C_in left at nominal | **PASS (estimated)**: margin 0.027 > line error 0.009 |
| Per-extremum loops (100 Hz edge, 10 kHz edge) | 50 | 4.3126 at the 100 Hz edge | FAIL |
| Guarded loop (below-noise knob goes to an edge; flips) | +18 | 4.3126 | FAIL |
| Brute force, 512 corners + 1,000 random | 1,512 | **4.3126** (C_in−, 100 Hz edge) | **FAIL** |

**Guards.** Two independent fixes:
1. Track each local extremum as its own sub-measure (Danskin 1966; precision study §4.7). Slopes are shared (one AC sweep gives every frequency), so the cost is one extra jump and its re-linearization per extra extremum.
2. Never leave a knob at nominal just because its slope is below noise: put it at an edge, and let the flip test decide.

The precision study's Chebyshev filter is the stronger case for (1): linearizing at the nominal argmax gives 0.678 dB, one loop step lands on the other ripple peak (0.949 dB), per-peak tracking gives 1.120 dB, truth 1.113 dB. The MVP has no for-all-frequency spec, so (1) waits until one appears; (2) costs nothing and is needed now.

**A correction to `engine.md` §5.2.** Its last bullet cites "linearizing at the nominal worst frequency was 5× too optimistic on a high-Q band-pass". That 5× figure (precision study §3, MFB table) is a *fixed-frequency* gain near a resonance: nominal sits on the peak, so the slope is about zero and the loss is quadratic. It isn't a multiple-extremum effect. The multiple-extremum example is the Chebyshev one above.

#### (f) A region change at the worst point

**Circuit.** Variant `hiZ` again, bias spec VC ≥ 1.2 V (truth: min 1.2897 V, so PASS).

| Round (nudges) | Predicts | Simulated | Region | Point |
|---|---|---|---|---|
| 1 | −0.0238 V | 1.3970 | saturation | T+ VCC+ R1− R2+ RC+ RE− β+ |
| 2 | 1.2862 | 2.3184 | active | T− VCC− … |
| 3 | 0.1461 | 1.3970 | saturation | back to round 1: **cycle** |

**Why it cycles.** In saturation VC ≈ VE + VCE,sat is flat, so local slopes are tiny and their signs are arbitrary. Nudged at the saturated corner, T's slope is +0.000 (a tiny positive number), so the vertex rule for a minimum sends T to T−, back into the active region. The flips there tell the truth (slopes as dVC/dε):

| Flip at the saturated corner (VC 1.3970) | VC after the flip | Secant slope | Local (nudged) slope |
|---|---|---|---|
| T+ → T− | 2.0620 (active) | −0.332 | **+0.000** |
| β+ → β− | 3.6077 (active) | −1.105 | −0.015 |
| VCC+ → VCC− | **1.2897** (saturated) | +0.054 | +0.055 |

VCC's slope also changes sign at the region boundary: −0.064 at nominal (active region: VC falls as VCC rises), +0.055 in saturation (VE, and so VC, rises with VCC).

With flips, the loop converges to the true 1.2897 V in 25 runs instead of cycling in 27. The verdict is still not PASS: the jumps missed by up to 1.42 V, so the margin (0.09 V) is inside the bracket, and the worst point is in a different region from nominal. So the verdict is **UNDECIDED (regime)**, which is conservative and correct: the bias spec "passes" only because the transistor is saturated there, and the gain spec at that point fails (4.227).

**Policy (§3.2):** a region change never blocks a FAIL. A PASS whose worst point lies in another region is UNDECIDED in the MVP. Lines drawn across a kink are meaningless, and fitting separate lines per region is a later refinement.

#### (g) Newton non-convergence, or a wrong solution, at a corner

What our simulator does today, checked on the real binary and on the replica:

| Situation | What happens | Where |
|---|---|---|
| CE amplifier, all 64 DC corners, and the saturating `hiZ` corners (25 °C) | Converges in 18–19 of 50 iterations; binary = replica to 2.4e-7 V | `exp_g.py` |
| No DC solution (0.1 mA forced into reverse-biased junctions, no GMIN) | **Returns V(b) = −7.27·10²⁶ V, exit code 0, no warning.** The relative step test accepts any iterate at that scale, and the clamped junction's tangent (g ≈ 1.6·10⁻³⁰ S) is what "solves" | `nosol.spicy`; `trans.rs:46-59`, `bjt.rs:97-104` |
| Cross-coupled latch (three DC solutions) | No convergence in 50 iterations; with 200 iterations or `pnjlim`, the **unstable metastable point** | `engine_mixed_signal.md` §2.1 |
| Newton fails in `.op` | `Err(SimulationError::NonConvergence)` | `error.rs:25-26`, `trans.rs:98-101` |
| Newton fails in a DC sweep; AC factorization fails | **Panics** (`.expect`) | `dc.rs:108,119`; `ac.rs:138-139` |

For the MVP amplifier, Newton is not the risk (no corner comes close to the iteration limit). But the engine needs a policy, because the corners are where devices go to extremes:
- **A failed run is not a value, and never a dropped point.** A dropped corner may be exactly the worst one. Retry: warm start from the nearest converged point, then parameter continuation from nominal. This is the idea behind ngspice's gmin and source stepping (`cktop.c:55-90`). If the point still fails, the spec is **UNDECIDED (simulator)** at that point. A FAIL found elsewhere still stands.
- **An absurd solution is a failed run.** Flag it if any junction ends beyond the ±40·n·Vt clamp (the solution then sits on the linear extrapolation, `bjt.rs:97-104`), or if any node voltage exceeds 10× the largest source voltage.
- **Multiple solutions:** re-run each spec side's decisive point from a cold start (§3.5). A different answer means "multi-stable" and UNDECIDED, unless the spec declares a history (`engine.md` §4.7).

### 1.2 Summary

| Case | Unguarded loop | Truth | Guard that catches it | Cost of the guard |
|---|---|---|---|---|
| (a) saturation off at nominal | PASS 8.501 | FAIL 3.891 | saturation-margin guard | 0 sims for the line + 1 to verify |
| (b) slope sign flip (MVP) | 4.4626, correct after re-linearizing | 4.4626 | re-linearize at the vertex | 8 (part of the loop) |
| (c/d) interior max, cycling | cycles, 8.6849 | 8.7102 | best-so-far + flip certificate; interior check when triggered | 0; interior check N + 8 |
| (e) second extremum + below-noise slope | PASS 4.3566 | FAIL 4.3126 | below-noise knobs to an edge + flips; per-extremum tracking | 0; one jump per extra extremum |
| (f) region change at the worst point | cycles, 1.3970 | 1.2897 (PASS, saturated) | flips (converge); region guard → UNDECIDED | 0 |
| (g) Newton | converges for the CE amp; absurd solution accepted elsewhere | — | result sanity + retry + cold-start re-run | 1 per spec side (merged with §3.5) |
| Red-team clipping THD (re-implemented from its A.2) | PASS 0.0026% (12 sims) | FAIL 3.2894% (4 of 32 corners fail) | range corners (2.5132%); flips + below-noise-to-edge also catch it | 8 corners, or 0 extra |

---

## 2. The safety net, precisely

### 2.1 The guarded loop on one page

```
 SHARED (once per design, serves every spec)
 [S1] nominal + N nudges ──► a line for every measure AND every device margin     N+1 runs
 [S2] the 2^R range corners, statistical knobs at nominal                         2^R runs
 [S3] 2 interior temperature points (other knobs nominal)                         2 runs
 [S4] margin guard: each margin's line → its minimum over the confidence region;
      if below the margin line's own observed error, simulate it                  0–1 run per margin
      every run: per-device region + margins + Newton status logged
 PER SPEC SIDE (min and max separately)
 [L1] start at nominal; jump to the line's vertex (below-noise knobs → an edge)    1 run
 [L2] at a vertex: FLIP each knob to its other edge (= re-linearize + test
      every 1-flip neighbour); move to the best improvement; repeat               N runs/round
      stop: no flip improves (1-flip certificate) · cycle · budget
 [L3] lazy multistart: restart only from a safety-net point that beats [L2]'s answer
 [L4] interior check, only if triggered (cycle, sanity check, power-type measure)  N + ~8
 [L5] inner bound = worst over EVERY run so far (pooled across specs)
 [L6] cold-start re-run of the decisive point at reltol/10 → ε_num, uniqueness    1 run
 [L7] verdict (§3)
```

### 2.2 Which points, and what each is for

| Points | MVP count | Catches | Evidence |
|---|---|---|---|
| Nominal + 8 nudges | 9 | Lines for every measure and every margin | — |
| 4 range corners (T × VCC), nominal statistical | 4 | Limiters driven by range knobs, including joint ones (the red team's THD: VCC low *and* cold). Needs nothing from device models, so it also covers behavioural models that report no margins. Free error samples for e_obs (§3.4) | THD 2.5132% from the corners alone |
| 2 interior temperatures (7.5, 42.5 °C) | 2 | An interior extremum along temperature at nominal parts (MOSFET zero-TC point, the P-FET cold-crank case in `engine_power.md`). Doubles as a sanity test of the new temperature code (M2c) | MVP: VC, gain and f_L are monotone in T (5.862 → 5.152 V; 4.5915 → 4.5891; 20.24 → 19.96 Hz) |
| Margin-guard points | 0 on the MVP (1 per crossing margin) | Regime changes that need many knobs at once, statistical ones included | `hiZ`: the only guard that caught (a) |

**Is nominal-statistical enough at the range corners?** For limiters driven mostly by range knobs, yes (THD). For limiters driven by statistical knobs (β in `hiZ`), no, and the margin guard is the cheap complement: one run instead of 2^R more.

**Margin guard on the MVP amplifier:** m_sat nominal 3.856 V; its line's minimum over the box is 2.885 V (at T+ VCC− R1− R2+ RC+ RE− β+); the line missed the range corners by at most 0.013 V. 2.885 V is far above 0.013 V, so no run is needed. (Simulated anyway for this report: 2.966 V, active.)

### 2.3 What every run logs

```
RunRecord {
    point:       knob values (and provenance: nominal | nudge | flip | jump | net | margin | key)
    status:      Ok { newton_iters } | Failed { kind, retries } | Implausible { why }
    measures:    every measure of every spec (one run serves all specs)
    devices:     per device: region, margins, clamped, and the op values below
}
```

### 2.4 BJT region and margins: definitions, with numbers

| Region | Condition (NPN; polarity-normalized) |
|---|---|
| forward active | BE on, BC off |
| saturation | BE on, BC on |
| reverse active | BE off, BC on |
| cutoff | BE off, BC off |

**BC "on" (the saturation edge).** The BC junction's transport current IS·e^(VBC/Vt)·(1 + 1/BR) reaches **10⁻⁴ of |IC|**. Then VBC,on = Vt·ln(10⁻⁴·|IC| / (IS·(1 + 1/BR))). At the MVP's nominal point (IC 1.38 mA, 25 °C) that gives VBC,on ≈ 0.40 V, the textbook edge of saturation. The margin **m_sat = VBC,on − VBC** equals VCE − VCE,sat(edge) in the active region; for the MVP, VCE,sat(edge) = 0.254 V.

**Why 10⁻⁴ and not 1%.** Our first definition used 1% of IC. On `hiZ`, the corner with VCE = 0.185 V was then still "active", yet its gain had collapsed from 8.8 to 3.9. The small-signal conductance of a barely-on BC junction loads the collector node long before it takes 1% of the DC current: at a 1% fraction it's 0.01·gm = 0.54 mS at the MVP's 1.38 mA, 2.5× the 4.7k load's 0.21 mS (and about 4× the 9.1k load at `hiZ`'s saturating corner). At 10⁻⁴ it's 1e-4·gm·RC ≈ 2.5% of the load. **Region thresholds must be set by the most sensitive measure (small-signal gain), not by DC currents.**

**BE "on" (cutoff).** IC ≥ 10⁻³ × IC of the nominal run; margin m_cut = Vt·ln(IC / (10⁻³·IC_nominal)). This is a current ratio, because the useful reference current is the design's own.

**Precedent.** ngspice and Xyce report the op values but no region for BJTs (ngspice `bjt.c:37-47`: ic, ib, ie, vbe, vbc, gm, gpi, gmu, go; no region field in `N_DEV_BJT.C`). ngspice's optional SOA check warns on absolute limits (`bjtsoachk.c:51-137`: Vbe, Vbc, Vce, Ic, Ib, Pd), not on regions. Gnucap's MOSFET model computes region flags and exposes them as probes (`d_mos.model:171-180`, flags at 277-282), and uses a region change to damp Newton (`d_mos.model:490-574`). We follow Gnucap: **the device computes its region and margins**, because only it knows IS, BR and Vt(T). Our thresholds are our own choice (a departure, justified above); Graeb's "sizing rules" are the IC precedent for constraints that keep each device in its intended region (Graeb et al., ICCAD 2001, via red team §2.3).

### 2.5 What our simulator must expose or fix

| # | Need | Today | Change |
|---|---|---|---|
| 1 | Per-device op record after convergence (vbe, vbc, vce, ic, ib, ie, gm, gpi, gmu, region, m_sat, m_cut, clamped) | `linearize()` computes i_f, i_r, g_f, g_r and the terminal currents (`bjt.rs:108-202`), and `stamp_nonlinear` discards them (`bjt.rs:205-253`). `OperatingPointResult` holds only `solution` (`dc.rs:9-12`) | Evaluate each device once at the final solution and return a record. **M2d needs the same record**: AC must stamp exactly these conductances, as ngspice's `bjtacld.c` reads them from the state vector |
| 2 | Errors, not panics | `simulate_dc` `.expect` (`dc.rs:108,119`); `simulate_ac` returns no `Result` and `.expect`s the factorization (`ac.rs:127-139`) | Return `Result<_, SimulationError>` |
| 3 | Detect absurd solutions | −7.3·10²⁶ V accepted silently (§1.1 g) | `Implausible` status: a junction beyond the clamp at the solution, or \|V\| > 10× max source |
| 4 | **Ebers–Moll reciprocity** | `bjt.rs:133-135` uses IES = ICS = IS, so αF·IES ≠ αR·ICS whenever BF ≠ BR | Use the transport form of ngspice (`bjtload.c:622-623`): IC = IS(e^(vbe/Vt) − e^(vbc/Vt)) − (IS/BR)(e^(vbc/Vt) − 1), IB = (IS/BF)(e^(vbe/Vt) − 1) + (IS/BR)(e^(vbc/Vt) − 1) |
| 5 | Initial guess | Always zero (`dc.rs:66`) | Accept a warm start (retries, nudges, the cold-vs-warm uniqueness check). ngspice starts "on" junctions at Vcrit (`bjtload.c:249-253`) |
| 6 | Newton iterations in the result | Returned by `newton_solve` and dropped (`dc.rs:67`) | Keep, for the run record |

**The reciprocity bug in numbers.** At forced β = 10 (0.1 mA into the base, 1 mA into the collector, `sat_forced.spicy`), our binary gives **VCE,sat = 47.77 mV**; the transport equations give **65.56 mV** (both computed in `em_amp.py`, and the binary matches the `ours` form exactly). In forward active the effect is small: VC differs from the transport form by ≤ 1.15 mV over the 65 test points. It matters exactly where the guards look (saturation), and it will show up as a mismatch in the M4 cross-check against ngspice.

### 2.6 From guard to action

| Guard | Signal | Cost | Loop action | If unresolved |
|---|---|---|---|---|
| Result sanity | `Err`, or `Implausible` | 0 | retry: warm start, then parameter continuation | UNDECIDED (simulator), naming the point |
| Region seen ≠ nominal | any run's region for a device the measure depends on | 0 | the point already counts for every spec (pooling); restart affected searches from it | if at the worst point and passing: UNDECIDED (regime); anywhere else: tag *regime change reachable* |
| Margin guard | the margin line's minimum < its observed error | 1 | simulate the predicted minimum; if it's still ambiguous, run the vertex search on the margin | as above |
| Net point beats the loop | a safety-net value worse than [L2]'s answer | 0 | lazy restart from it | — |
| 1-flip certificate | some flip improves | N per round (it *is* the re-linearization) | move | budget exhausted: UNDECIDED (search) |
| Below-noise slope | \|g_i\| below the FD noise floor | 0 | knob to an edge; the flips decide | — |
| Cycle | a vertex revisited | 0 | keep the best; certify it; interior check | UNDECIDED (search) if not certified |
| Sanity (PSpice AA) | the worst found is no worse than another simulated point | 0 | interior check | — |
| Interior temperature | T-scan point worse than both T corners of its slice | 2 (the scan) | golden section on T (~8), restart from there | — |
| Numerics | \|I − B\| ≤ ε_num | 1 (the [L6] re-run) | — | UNDECIDED (numerics) |
| Uniqueness | the cold-start re-run differs from the warm result | 0 (same re-run) | — | UNDECIDED (multi-stable), unless the spec declares a history |
| Several extrema (sweep measures) | more than one local extremum in the sweep | 1 jump + re-linearization per extra extremum | one sub-search per extremum | — |

### 2.7 Which guards were necessary: ablation

`ablation.py` runs the guarded loop with each guard removed, on eleven spec sides: MVP ×5, `hiZ` ×3, `pq`, `band`, THD.

| Configuration | Right | UNDECIDED | Wrong | Sims: MVP / hiZ / pq / band / THD |
|---|---|---|---|---|
| No guards (pooled inner bound only) | 5 | 3 | **3** (hiZ gain, band, THD) | 63 / 54 / 36 / 30 / 12 |
| **Recommended** | **9** | 2 | **0** | **61** / 51 / 31 / 32 / 28 |
| − flips (nudges at vertices) | 7 | 3 | 1 (band) | 67 / 75 / 40 / 34 / 26 |
| − below-noise knobs to an edge | 8 | 2 | 1 (band) | 65 / 61 / 31 / 32 / 30 |
| − margin guard | 8 | 2 | 1 (hiZ gain) | 61 / 45 / 31 / 32 / 28 |
| − range corners | 9 | 2 | 0 | 57 / 47 / 27 / 28 / 20 |
| − lazy multistart | 9 | 2 | 0 | 61 / 45 / 31 / 32 / 28 |
| + full multistart from the worst corner | 9 | 2 | 0 | 73 / 57 / 37 / 39 / 28 |

The two UNDECIDEDs are `hiZ` VC ≥ 1.2 V and `hiZ` gain ≤ 9.24 (both truly PASS). Both involve a reachable saturation region, which is the honest answer for a design that fails its gain minimum anyway.

**Reading it.** On this suite, flips, the below-noise rule and the margin guard are each necessary. Range corners and multistart are redundant *here*, because flips from the loop's vertex happen to reach the THD limiter. We still keep the 4 range corners: they need no cooperation from device models, they cover joint moves of range knobs that single flips don't, and they supply free error samples. Full multistart costs 12 more runs on the MVP for nothing; lazy multistart costs nothing.

---

## 3. Verdict and bracket logic

### 3.1 Inputs, per spec side

```
I       inner bound: the worst simulated value among points inside the confidence region
x_I     where I was simulated (the counterexample if the side fails)
e_obs   the largest |predicted − simulated| over every jump this side made, plus the nominal
        line's misses at the safety-net points (free samples)
ε_num   numerical band at x_I: 2·|f(reltol) − f(reltol/10)| from the [L6] re-run
B       the spec bound
flags   open guards (§2.6)
conf    worst_case | sigma(k)
```

The **estimated outer bound** is O = I + e_obs for an upper-bounded side (I − e_obs for a lower-bounded side). The bracket shown to the user is [I, O].

### 3.2 The decision table (upper-bounded side, value ≤ B; a lower bound mirrors it)

Rows are checked in order; the first match wins.

| # | Condition | Verdict | Report / tags |
|---|---|---|---|
| 1 | I > B + ε_num | **FAIL** | counterexample x_I; region at x_I if not nominal ("fails with q1 saturated"); σ-distance of x_I |
| 2 | \|I − B\| ≤ ε_num | UNDECIDED (numerics) | ε_num |
| 3 | a run this side needed failed or was implausible | UNDECIDED (simulator) | the point |
| 4 | region at x_I ≠ nominal region | UNDECIDED (regime) | device and region (policy for the MVP; per-region lines later) |
| 5 | search ended on a cycle or out of budget, without a 1-flip certificate | UNDECIDED (search) | the cycle |
| 6 | B − I > e_obs + ε_num | **PASS (estimated)** | bracket [I, I + e_obs]; *regime change reachable* if any run in the region changed region |
| 7 | otherwise | UNDECIDED (bracket) | bracket vs B |

**PASS (guaranteed)** comes only from exact methods (the corner theorem, datasheet arithmetic; `engine.md` §4.1–4.2), never from this loop. A FAIL is never blocked by a guard: it rests on a simulated point, and the only doubts about it are numerics (row 2) and the model (tags).

### 3.3 Two-sided specs and confidence classes

Each side is searched and judged on its own (the lower side of `bias` looks for VC min, the upper side for VC max).

| Lower side | Upper side | Spec |
|---|---|---|
| FAIL | anything | FAIL (both counterexamples if both sides fail) |
| anything | FAIL | FAIL |
| PASS | PASS | PASS (the weaker kind of the two; tags from both) |
| otherwise | | UNDECIDED, with the undecided side's reason |

| Confidence | Points that count for I and for FAIL | Where the margin guard searches |
|---|---|---|
| `worst_case` | anywhere in the box | the box |
| `sigma(k)` | range knobs anywhere; statistical knobs with ‖u‖₂ ≤ k in σ units (with the MVP default σ = tol/3: ‖ε_stat‖₂ ≤ k/3, i.e. ≤ 1 for k = 3) | range corners × the kσ ball |

Points outside a sigma region are reported as information ("fails at 3.4σ"), not as FAIL. Both classes share every run: a `worst_case` point inside the ball counts for `sigma(3)` too. Flips apply to range knobs only under `sigma(k)`: the kσ worst point is interior in the statistical knobs (the sigma loop itself belongs to another report).

### 3.4 How much margin makes an estimated PASS believable

**Definition.** e_obs = the largest miss of the linear model over every jump (the point moved) and every safety-net point the nominal line predicted. **Rule: PASS (estimated) needs B − I > e_obs + ε_num**, i.e. a multiplier k = 1 on e_obs.

**Why this e_obs.**
- The jumps are the only places where the line was used at box scale and then checked. The final "prediction" at convergence is its own anchor (§1.0), so it carries no information.
- The flip certificate has already simulated every 1-flip neighbour of the worst vertex. What e_obs guards against is a worse point *off* the visited path (interior, or several flips away). Its size scales with the nonlinearity seen at box scale.
- It is the walkthrough's rule ("0.16 V margin, comfortably more than the 0.07 V the line can be off by", §5.5). It is the same idea as a trust region's actual-vs-predicted ratio (Nocedal & Wright 2006, §4.1) and as Cadence's "conservative estimate of model accuracy" on the model-based side of its yield verification (red team §3.2).

**Tested** (margin / e_obs; truth from the key):

| Case | Loop | I | e_obs | Margin | Ratio | Verdict | Truth |
|---|---|---|---|---|---|---|---|
| MVP VC ≥ 4.5 | guarded | 4.6597 | 0.0675 | 0.160 | 2.37 | PASS | PASS |
| MVP VC ≤ 6.5 | guarded | 6.5905 | — | −0.091 | — | FAIL | FAIL |
| MVP gain ≥ 4.37 | guarded | 4.4610 | 0.0092 | 0.091 | 9.9 | PASS | PASS |
| MVP gain ≤ 4.83 | guarded | 4.7024 | 0.0028 | 0.128 | 46 | PASS | PASS |
| MVP f_L ≤ 30 Hz | guarded | 26.719 | 1.923 | 3.28 | **1.71** | PASS | PASS |
| hiZ gain ≥ 8.36 | unguarded | 8.5011 | 0.0764 | 0.141 | **1.85** | PASS | **FAIL** |
| band ≥ 4.33 | unguarded | 4.3566 | 0.0088 | 0.027 | **3.02** | PASS | **FAIL** |
| THD < 1% | unguarded | 0.0026% | 0.0004 | 0.997 | **2,521** | PASS | **FAIL** |
| pq ≤ 8.70 mW | unguarded | 8.6849 | 0.634 | 0.015 | 0.02 | UNDECIDED | FAIL |
| hiZ VC ≥ 1.2 | guarded | 1.2897 | 1.421 | 0.090 | 0.06 | UNDECIDED | PASS |

**Conclusion.** The false PASSes' ratios (1.85, 3.02, 2,521) sit inside the range of the true ones (1.71 … 46). **No multiplier k separates them.** Raising k to 3.1 would still pass THD and would turn the MVP's f_L and VC ≥ 4.5 into UNDECIDED. The margin rule protects against curvature the loop *has seen*: it caught the cycling P_Q case, and it keeps the region-change cases undecided. Regimes the loop *hasn't seen* are the guards' job. Keep k = 1 and track calibration in the answer-key suite (assertion A5, §4.2).

### 3.5 The numerical band, measured

| Point set (MVP, 25 °C) | True error of our Newton at reltol 1e-3 | Estimate \|f(1e-3) − f(1e-4)\| | Estimate / true |
|---|---|---|---|
| Nominal | 1.92e-5 V | 1.92e-5 V | 1.000 |
| 65 points (nominal + 64 DC corners) | ≤ 4.46e-5 V | ≤ 4.46e-5 V | ≥ 1.000 |

Newton converges quadratically, so the reltol/10 run is essentially exact, and the difference *is* the error (`exp_numerics.py`). With the factor 2, ε_num(VC) ≈ 1e-4 V, which is negligible against the MVP's margins (≥ 0.09 V). The same re-run, from a cold start, checks that the counterexample reproduces and that the solution is unique: **one run per spec side**. AC measures (gain, f_L) have no measured band yet: M2d must provide one.

---

## 4. Answer-key tests for M3b and M3c

### 4.1 The oracle

- **Points:** all 2⁸ = 256 corners, plus 1,000 seeded uniform interior points, plus a coordinate search (steps 0.25 → 10⁻⁴) from the best point. That's about 1,256 runs plus the refinement; each is a 5-node DC solve plus a short AC sweep, so it should take seconds at most (not measured yet). So it can run live in tests; snapshot its values too, so any change shows up in review.
- **Same code path:** the same backend, lowering and **measurement library** as the engine, never the walkthrough's formulas. They differ measurably: \|H(1 kHz)\| is 4.5908 where the walkthrough's mid-band gain is 4.5917; f_L as "−3 dB below \|H(1 kHz)\|" is 20.071 Hz where 1/(2π·C·R_in) gives 20.079 Hz. The worst values shift the same way (gain min 4.4610 vs 4.4626; f_L max 26.719 vs 26.738 Hz).
- **Completeness:** if the oracle can't simulate a point, the test fails. The key must be complete.
- **Corners are the key for the MVP's measures:** for all six sides (VC, gain and f_L, min and max), 2,000 random points plus refinement never beat the best corner. For power-type and derived measures they aren't (P_Q: 8.7088 at the corners vs 8.7102 in the interior), so the interior search is part of the oracle, not an option.

### 4.2 Assertions, per spec side and confidence class

| # | Assertion | Tolerance |
|---|---|---|
| A1 | Verdict = the key's verdict. A key worst within ε_num of B accepts UNDECIDED (numerics) | exact |
| A2 | The counterexample reproduces: a cold re-simulation of x_I gives I | ε_num |
| A3 | I is not beyond the key's worst. If it is, the oracle missed a point: **fail the test and fix the oracle** | ε_num |
| A4 | Worst value: \|I − key_worst\| small. Corner-extreme measures: equal. Measures marked "interior possible": relative 10⁻³ | ε_num / 10⁻³ |
| A5 | Bracket covers the truth: for every PASS (estimated), key_worst ∈ [I, I + e_obs]. Count the violations across the suite (must be 0); this is the calibration of k in §3.4 | — |
| A6 | Cost: simulation count equals a snapshotted number (MVP `worst_case`: 61 + T-scan + re-runs), so any cost change is visible | exact |
| A7 | Guards: on each adversarial case, the expected guard fires by name, and the verdict equals the key's or the expected UNDECIDED reason | exact |
| A8 | Determinism: two runs give identical traces | exact |

For `sigma(3)` (M3d) the key changes: a seeded Monte Carlo over the statistical knobs, taking each sample's worst over the range corners (board-level, `engine.md` §4.4), with a Clopper–Pearson interval. The assertion becomes agreement outside that interval.

### 4.3 Regression circuits, and what each needs

| Case | Circuit (from the MVP amplifier) | Spec | Expected | Needs in our simulator |
|---|---|---|---|---|
| MVP | as `ce_amp.spl` | the 3 specs | PASS, FAIL (VC ≤ 6.5), PASS, PASS, PASS | M2c temperature, M2d BJT AC |
| (a) | R1 470k, R2 100k, RC 9.1k, C_in 100 nF | gain 8.8 ± 5% | FAIL via the margin guard, counterexample 3.891; upper side UNDECIDED (the search cycles through the saturated region) | M2c, M2d, #1 op records, #4 reciprocity fix |
| (b) | MVP | gain ≥ 4.37 | T flips at the hot corner; worst at the cold corner | M2c, M2d |
| (c/d) | RC 3.6k | P_Q ≤ 8.70 mW | FAIL 8.7088; key 8.7102 interior; cycle detected | M2c, #1 (IC, IB, VBE, VCE) |
| (e) | C_L 690 pF ± 10% (9th knob) | \|H\| ≥ 4.33 over 100 Hz – 10 kHz | FAIL 4.3126 at the 100 Hz edge | M2d, a min-over-band measure |
| (f) | as (a) | VC ≥ 1.2 V | UNDECIDED (regime); worst 1.2897 found, no cycle | M2c, #1 |
| (g1) | two current sources into reverse junctions | any | UNDECIDED (simulator: implausible) | #3 |
| (g2) | latch (`engine_mixed_signal.md` §2.1) | any | UNDECIDED (multi-stable) | #2, #5 |
| THD | red-team op-amp clipping stage | THD < 1% | FAIL 3.2894% | op-amp models and transient: not ours; function backend only |

**Test the loop before the simulator is ready.** Give the `Backend` trait a function-backed test double: knob point → measures, regions and margins from a closed-form model. The models in this report translate directly: the Ebers–Moll amplifier in `em_amp.py` is under 200 lines. M3c's loop logic, guards and verdict table can then be tested against all nine cases while M2c and M2d are still in progress. The same cases then run on the real simulator as the features land.

---

## 5. Grounding

**Where we follow precedent.**
- *The loop is HL-RF / worst-case distance* (red team §0 and §4). Its known failure modes have 40 years of literature behind them. Graeb's textbook says "there is no investigation into when and how this iteration formula converges" (red team §4.2a).
- *Several design points (FORM).* Der Kiureghian & Dakessian, "Multiple design points in first and second-order reliability", *Structural Safety* 20(1):37–49, 1998, doi:10.1016/S0167-4730(97)00026-X (citation verified via Crossref). Their method, as summarized by papers citing it (I couldn't read the paper; ScienceDirect returned 403; secondary sources: the abstracts of doi:10.1177/0954410013499495 and doi:10.7843/kgs.2013.29.9.71): after each design point is found, **add a "bulge" to the limit-state function around it** (also called a barrier method), so the next search converges elsewhere. The failure probability then comes from the series system of all points found. Our analogues: per-extremum sub-searches (§1.1 e) and lazy restarts from safety-net points. In a box the natural "bulge" is simply "exclude visited vertices", which is our cycle memory. Safeguarded HL-RF (Zhang & Der Kiureghian 1995, merit-function line search) is the precedent for not trusting full jumps (red team §4.2a).
- *Analog tools use models to propose points and never to decide.* Solido HSMC uses its model "to merely order the samples, rather than … to make a decision" (US9483602B2). Cadence VVO quotes WCD at "under 100 simulations for each spec … small number of specs/parameters" and recommends scaled-sigma sampling for nonlinear behaviour. MunEDA tells users to still run brute-force Monte Carlo for non-linearity. PSpice `.WCASE`: "no guarantee" unless monotonic, with a BJT saturation example that fools "even an optimizer". PSpice AA's sanity check (a worst no worse than nominal signals trouble). All via red team §3.1–3.3 [V there].
- *Margin guards* = Graeb's sizing rules (ICCAD 2001; bipolar version Massier, Graeb & Schlichtmann, DATE 2008), via red team §2.3.
- *Max over frequency* = Danskin 1966, per-extremum tracking (precision study §4.7).
- *Region flags from the device model* = Gnucap (`d_mos.model:171-180, 490-574`).
- *Retry ladder* = ngspice's gmin and source stepping (`cktop.c:55-90`).

**Where we depart, and why.**
- **Flips instead of nudges at vertices.** WCD and HL-RF use local gradients. At L0 a flip costs the same as a nudge, the worst_case question is combinatorial (which vertex), and in a flat or saturated region local slopes have arbitrary signs (§1.1 f: T's nudged slope +0.000, its secant −0.332). A flip is the 1-exchange neighbourhood of local search over the vertices of the box (the standard move of local search over binary variables). With adjoint slopes (L1) the flips remain the certificate; only the first jump gets cheaper.
- **Below-noise knobs go to an edge.** The red team's own experiment left zero-slope knobs at nominal, and that's exactly how (e) fools the loop.
- **Region thresholds from small-signal loading**, not DC current fractions (§2.4).
- **Region change at the worst point → UNDECIDED**, which is stricter than `engine.md` §5.2's "back to step 4 *or* UNDECIDED" (no per-region lines in the MVP).

**Context only: guaranteed methods.** Interval Newton and verified solvers (Neumaier 1990; Rump's INTLAB; closest to us, Lemke et al. 2002 per the walkthrough §7.3) could turn some estimated PASSes into guaranteed ones for small transistor blocks. They don't change the MVP's guard design: every guard here protects an *estimated* verdict.

**Unknown.**
- How Cadence and MunEDA guard their WCD against latent regimes internally is not published (the red team found only the hedges above). The web search budget ran out in this session, so no new vendor documents were read.
- Whether the 10⁻⁴ saturation edge still predicts gain collapse with Early effect and quasi-saturation (Gummel–Poon RC, `bjt.c:33`).
- How well flips scale beyond about 20 knobs (§8).

---

## 6. What this refines or contradicts in the current docs

| Doc | Says | Refinement |
|---|---|---|
| `engine.md` §5.1 step 4, walkthrough §5.6 | "repeat until prediction ≈ simulation" | Vacuous at a vertex (§1.0). Stop when the point stops moving *and* no flip improves; the evidence is the jump errors |
| `engine.md` §5.1 step 4, roadmap M3c step 2 | re-linearize by slopes (nudging at L0) | At a vertex, re-linearize by **flips**: same cost, converges where nudges cycle (§1.1 f), certifies the vertex |
| `engine.md` §5.2, last bullet | "5× too optimistic on a high-Q band-pass" | That figure is a fixed-frequency near-resonance effect; the multiple-extremum example is the Chebyshev 0.678 vs 1.113 dB (§1.1 e) |
| `engine.md` §5.2 | "slope changes sign" as its own guard | Subsumed by flip re-linearization |
| `engine.md` §5.2 | "region differs from nominal → back to step 4, or UNDECIDED" | MVP: UNDECIDED (regime) unless FAIL; tag *regime change reachable* elsewhere |
| `engine.md` D4, red team §7 | multi-start | Lazy multistart suffices on every test case; full multistart cost 12 extra runs on the MVP for nothing |
| walkthrough §7.3 | "The engine checks for these at every simulated point" | Needs simulator op records that don't exist yet (§2.5 #1) |
| roadmap M3b | brute force over 256 corners | Plus interior points and refinement, through the engine's own measurement library (§4.1) |
| red team A.2 convention | zero-slope knobs stay put | They go to an edge (§1.1 e) |
| `bjt.rs` | Ebers–Moll with IES = ICS = IS | Non-reciprocal; VCE,sat 48 vs 66 mV (§2.5 #4) |

---

## 7. Recommendations for the MVP

Each is decidable, with its reason and its cost on `ce_amp.spl` (8 knobs, 5 spec sides, `worst_case`).

1. **Safety net = nominal + 8 nudges + 4 range corners + 2 interior temperatures, shared by all specs.** Cost 15 runs. *Reason:* the lines for every measure and margin come free; the corners need no model cooperation; the interior temperatures test the new temperature code and catch temperature humps.
2. **Every run returns a record with per-device operating-point data** (region, m_sat, m_cut, clamped, the ngspice op set). *Reason:* every guard reads it, and M2d needs the same conductances. Simulator work, §2.5 #1.
3. **Margin guard on q1's m_sat and m_cut:** predict each margin's minimum over the confidence region from the nominal lines; if it's below that margin line's observed error, simulate it. Cost 0 runs on the MVP (2.885 V against 0.013 V). *Reason:* the only guard that caught (a).
4. **Re-linearize at a vertex by flipping each knob to its other edge; stop when no flip improves.** Cost N = 8 per round, the same as nudging. *Reason:* fixes (b), (e) and (f); the 1-flip certificate is what makes a vertex claim credible.
5. **A knob whose slope is below the FD noise floor goes to an edge, never stays at nominal.** Cost 0. *Reason:* fixes (e); also makes the loop robust to FD noise on weak knobs.
6. **Pool the inner bound:** every run updates every spec. Cost 0.
7. **Lazy multistart and cycle memory:** restart only from a safety-net point that beats the loop's answer; on a cycle, keep the best and certify it. Cost 0 on the MVP.
8. **Interior check only when triggered** (a cycle, PSpice AA's sanity check, or a power-type measure): inward nudges at the best vertex, then golden section along each improving knob. Cost N + about 8 when triggered; 0 on the MVP.
9. **Verdict table of §3.2 with k = 1** (PASS needs margin > e_obs + ε_num); two-sided combination and confidence regions as in §3.3. *Reason:* no k separates false from true PASSes; the guards must.
10. **One cold-start re-run of each side's decisive point at reltol/10.** It gives ε_num, reproduces the counterexample, and checks uniqueness. Cost 5.
11. **Region change at the worst point → UNDECIDED (regime), unless FAIL.** *Reason:* lines across a kink carry no information; the MVP amplifier never leaves forward active, so this costs its verdicts nothing.
12. **A per-side budget of 4 rounds** (1 + 4·8 runs), then UNDECIDED (search). *Reason:* no convergence guarantee exists.
13. **Before M3, in the simulator:** the reciprocity fix, `Result` from DC sweep and AC, implausible-solution detection, and a warm-start argument (§2.5 #2–6). *Reason:* each one either corrupts the margins or turns an unknown into a panic or a silent wrong value.
14. **M3b oracle and the tests of §4:** 256 corners + 1,000 seeded interior points + refinement through the engine's own measurement library; assertions A1–A8; the nine regression cases of §4.3; a function-backed `Backend` test double so M3c can be tested before M2c/M2d land.

**Total for the MVP's `worst_case` check:** 15 (safety net) + 48 (the five spec sides) + 5 (re-runs) ≈ **68 runs**, against 256 for brute force. The measured guarded-loop part (61) equals the unguarded loop's (63): the guards cost about the 4 corners and 2 temperature points, and buy the removal of every false PASS in the suite.

---

## 8. Open questions

1. **Region thresholds with real BJT models.** Does a 10⁻⁴ BC-current edge still predict the gain collapse with Early effect, quasi-saturation and series resistances? Validate with ngspice (M4) on the `hiZ` variant.
2. **Joint flips.** The buck needed three knobs flipped together (`engine_power.md` S4); a 2-flip test at the final vertex costs C(N, 2) = 28 at N = 8, or 6 if restricted to the top four knobs by secant. No MVP or adversarial case needed it. Add it when a regression case does?
3. **Per-region lines** to recover PASS (estimated) when the worst point is certified but lies in another region (`hiZ` VC ≥ 1.2 V is a true PASS reported as UNDECIDED).
4. **Scaling the flips.** At N = 50, flipping every below-noise knob costs 50 runs a round. Screening can't drop them, because the zero-slope limiter lives exactly there, so the margin guard and range corners have to carry more. Where is the crossover?
5. **Calibrating k.** k = 1 has no A5 violations on this suite. Re-check as the regression set grows, especially with transient measures, where FD noise enters e_obs.
6. **`sigma(k)` guards:** flips don't apply to statistical knobs; is the margin guard over the kσ ball plus pooling enough? That belongs with the sigma report.
7. **Interior temperature points:** should the scan run at each VCC corner (4 runs) instead of at nominal VCC (2 runs)? No MVP data distinguishes the two.
