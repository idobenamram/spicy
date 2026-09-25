# Engine Method Red-Team: assumptions, industry practice, missed methods

> 2026-09-25 · red-team of `../archive/engine_v2.md`, `../walkthrough.md`, `../specs.md`, `../bibliography.md`
> Scope: attack the **method** (not the language), survey what commercial tools actually do, find usable methods we missed, spot-check the bibliography, and look at how results should be presented.

**Evidence tags used throughout**

- **[V]**: read in a primary source (vendor manual, datasheet, patent, paper PDF, standard), or confirmed as a citation via DOI/Crossref.
- **[S]**: secondary (trade article, forum, search excerpt, author republication).
- **[U]**: unverified (from memory, or the source could not be reached).
- **[E]**: my own numerical experiment (Appendix A; pure Python, reproducible).

**Research limits.** The session's shared web-search budget ran out partway through. Later checks used direct fetches, the Crossref / OpenAlex / Semantic Scholar APIs, and PDF extraction. Several sources could not be reached at all:

- analog.com (timed out);
- cadence.com, community.cadence.com and Springer chapter bodies (blocked; Cadence pages were read via Wayback copies);
- EDAboard, Reddit and All About Circuits (blocked).

Forum evidence is therefore thin. Section 6 says where.

---

## 0. Summary

**Verdict.** The core of the v2 method is sound and matches industrial practice:

- The worst-point loop is the Hasofer–Lind / Rackwitz–Fiessler (HL-RF) iteration of structural reliability. Its answer is the worst-case distance.
- MunEDA WiCkeD ships worst-case distance (WCD) commercially, and so does Cadence's Variation Option.
- Cadence's "K-Sigma corners" algorithm is structurally the same loop: model, predicted corner, simulate, line search [V].
- On the walkthrough's amplifier the loop reproduces brute force, and FORM's Φ(−β) matches Monte Carlo: 0.98% vs 0.87–1.09% [E].

**The weaknesses are in the assumptions around the loop.** Ranked by how much damage they do on real boards:

1. **Confident false PASS from latent regimes (A3).** At nominal, a limiter that isn't active has zero gradient: headroom, dropout, current limit, UVLO, clipping. The loop never pushes the knob that activates it.
   - Its own check ("prediction ≈ simulation") then **passes**, because both agree on the wrong, benign point.
   - On a clipping-THD spec (the kind in `../specs.md` §12) the loop reports 0.0026% → PASS. The true worst corner is 3.29% → FAIL. Enumerating just the 8 range corners catches it [E].
   - PSpice's own manual documents the same failure for its `.WCASE`: a saturating BJT whose local gradient points the wrong way. Multisim calls its worst case "a predictor … not … a final say" [V].
2. **Yield numbers depend on an unknowable input more than on anything the engine computes (A6).** The same design and the same spec (f_L ≤ 30 Hz) give:

   | Distribution assumption | Failure rate |
   |---|---|
   | Truncated normal, σ = tol/3 (the design's default) | 0.89% |
   | Uniform | 11.6% |
   | σ = tol/2 | 4.1% |
   | Offset reel | 5.0% |
   | Culled / bimodal | 32% [E] |

   Real datasheets back all of these shapes [V]:
   - TI's "typical" is µ+σ for Vos-type specs, with max at about 5σ.
   - Measured reels are tight (σ ≈ tol/7…tol/64) but offset by −0.26% to −1.68%.
   - Selection-made tolerances are uniform.
3. **"Yield at the worst range corner" is not board yield (A8).** A board must pass at *every* condition, and a board has many specs.
   - Two-sided window: MC at the single worst range corner says 26.7% loss; the true "fails anywhere" loss is 36.8% [E].
   - 50 specs at β = 3 each give up to 6.7% loss, against 0.135% for one spec.
   - Graeb's group evaluates yield over the intersection of specs, with each sample worst-cased over operating conditions (Schenkel et al., DAC 2001) [V].
4. **Vendor models are single typical points (A9).** They are typical-only, often without temperature, sometimes encrypted, and some effects are simply missing (Iq, Zout, IGSS, zener tolerance) [V].
   - Any PASS is model-conditional, and today nothing in the design says so.
5. **Several statements in our docs are wrong or overstated** (section 5):
   - The corner theorem's scope: it needs a single rank-one appearance per knob, a linear functional or ratio of solution components, and DC only. Power and derating metrics are **not** covered: maximum power transfer is an interior maximum.
   - "3σ (99.9%)": one-sided 3σ is 99.865%.
   - "The simulator is only good to 0.1%": wrong for DC Newton (≈2e-6 relative at reltol 1e-3 [E]); right for transient.
   - "FAIL is always definite": false for yield-type specs.

**What industry teaches** (section 3):

- **Board tools** (PSpice `.WCASE`, PSpice AA, Multisim, Micro-Cap EVA) all do *one* jump to the corner picked by nominal sensitivity signs [V]. Our loop is strictly better than every board-level tool.
- **IC tools** are robust for a different reason: they **use models to order or choose samples and never to decide**. Every verdict rests on real simulations (Solido High-Sigma Monte Carlo (HSMC), Cadence worst-samples and yield verification) [V]. They also search range and statistical knobs **jointly**: Solido PVTMC's premise is that the worst PVT at nominal is not the worst at the target sigma [S].
- **Missing everywhere:** a guaranteed bound, a three-valued verdict, and stress/derating checked **across tolerances**. Every Smoke/SOA feature checks the nominal only [V].
- **AI-EDA** (Flux, Circuit Mind, Quilter, atopile, JITX) does prompt-driven simulation, rule checks, or naive interval arithmetic. Nobody does simulation-backed worst case [V].

**Top design changes, ranked by impact** (details in section 7):

1. **Make the search non-local where it is cheap.**
   - Enumerate range corners (they are few, and one simulation serves every spec of that analysis).
   - Multi-start the loop, search both sides of two-sided specs, and add automatic **regime-margin guard metrics**.
   - Stop treating "prediction matched simulation" as evidence of correctness.
2. **A statistical verdict engine that fits what the loop produces.**
   - Importance sampling at the design point(s) instead of Φ(β) and plain MC.
   - Board-level yield: each sample worst-cased over range conditions, taken over all specs.
   - Clopper–Pearson confidence and sequential stopping.
   - Verdict semantics defined per confidence class.
3. **Distribution and data provenance on every knob.**
   - Record each knob's provenance, σ-multiplier and truncation, plus lot-offset knobs.
   - Always show yield under the default **and** under the distribution-robust bound. Uniform is the worst case among symmetric unimodal distributions for linear metrics: Birnbaum 1948 peakedness; Barmish & Lagoa 1997.
4. **Model-conditionality made visible.**
   - A per-model coverage map.
   - An adjoint "exposure" report for parameters that have no knob.
   - Standard knob wrappers (Vos, Ib, Rds(on), Vth).
5. **A robust local solver.** Trust-region / iHL-RF / SQP over range and statistical knobs **jointly**, in place of the bare fixed-point jump.
6. **A strict applicability checker for the corner theorem,** and derating/stress specs moved off it.
7. **Worst-case stress/derating as automatic specs.** This is the clearest market gap.

---

## 1. What was checked, and how

- **Documents read:** the vision, v2 notes, walkthrough, spec design and bibliography, plus the simulator's Newton loop (`crates/spicy_simulate/src/trans.rs:47-102`, `lib.rs:44-50`).
- **Five parallel research threads:**
  - IC variation tools;
  - board and AI-EDA tools;
  - alternative methods;
  - bibliography spot-check;
  - evidence on real-world distributions, models and regimes.
- **My own numerical experiments (Appendix A):**
  - I rebuilt the walkthrough amplifier's model exactly. Brute force reproduces the walkthrough's numbers: VC 4.6597…6.5905 V, gain 4.4626…4.703, f_L max 33.42 Hz, and 177/20,000 MC failures.
  - I used it, plus two small board-style models (LDO phase margin vs ESR; op-amp clipping THD), to test specific attacks.
- **References newly cited in this report** were checked as citations via Crossref or OpenAlex (Appendix B).

---

## 2. Assumption register

Every assumption the current design makes, stated or implied, with a board-level likelihood of breaking, the consequence, and the fix. "Where" points at the doc that states or relies on it (v2 = `../archive/engine_v2.md`, WT = walkthrough, SD = spec design).

| # | Assumption | Where | Breaks on real boards? | What goes wrong | Fix (priority) |
|---|---|---|---|---|---|
| A1 | The metric is smooth (C¹) in the knobs | v2 A8, SD §4 | **High** for transient and switching, medium for AC, low for linear-ish DC | Gradients mislead; loop oscillates or stops at a kink | Regime flags, gradient-consistency check, max-type measures handled as minimax, non-local fallback (P1) |
| A2 | One worst region; a local search from nominal finds it | v2 A8, WT §5 | **Medium** (stability windows, resonances, max-power-transfer) | Loop converges to one local worst and reports PASS (estimated); FORM under-counts failure | Multi-start from all range corners; mixture importance sampling at sign-off (P1) |
| A3 | The gradient at the current point "sees" every knob that matters | implicit in v2 A8 | **High** wherever a limiter is inactive at nominal (headroom, dropout, current limit, UVLO, clipping) | **Confident false PASS.** The loop's own "prediction matches simulation" check passes (experiment E2) | Enumerate range corners; auto-generated regime-margin guard metrics (P0) |
| A4 | Knob count is moderate | WT §9 ("5–8 sims regardless") | **High** at system level (hundreds–thousands); per spec usually 10–50 after locality | Finite-difference fallbacks cost N+1 sims; absolute worst case sits √N·3σ from nominal; reports drown in tiny contributors | Adjoint everywhere; sensitivity screening with bounded remainder; grouped reporting (P1) |
| A5 | Knobs are independent and the knob space is a box | v2 A7/B, WT §3 | **High** (reels, lots, matched networks, parameter correlation within one part, supply/temperature coupling) | Wrong corners; double counting (lot ±tol plus per-part ±tol exceeds the datasheet edge); infeasible corners | Structured knobs with constraints (common + differential, lot inside tolerance); polytope knob space (P1) |
| A6 | Distributions are known (truncated normal, σ = tol/3) | v2 A7, WT App. | **Very high.** Datasheets give min/max test limits or "typical" only | Yield numbers move 13–36× with the assumption (E1) | Show assumption; distribution-robust bound (uniform = worst symmetric unimodal); lot-offset range knob (P0) |
| A7 | Range knobs are few and every corner of their box is physical | v2 A7, SD §3 | Medium: 5–10 range knobs on real boards; some corners impossible | Pessimism at infeasible corners; with more than 10 knobs, gradient-picked corners can miss | Enumerate when 2^k ≤ budget (shared across specs); constraints to exclude infeasible corners (P0) |
| A8 | "Yield" = Monte Carlo at the worst range corner; per-spec yields stand alone | WT §6, §9 | **High** for two-sided specs and for many specs | Board yield overstated (E4: 26.7% vs 36.8% loss) | Per-sample worst over range corners; board-level union (P1) |
| A9 | The model is right | v2 D5, vision §5 | **Very high.** Vendor models are typical-only and behavioural | PASS is model-conditional; parameters with no knob are silently exact | Exposure report via adjoint to all un-knobbed model params; model wrappers; "model-conditional" tag (P0) |
| A10 | The simulator is accurate enough | v2 Part D.3 | Mixed. DC Newton is fine (E6); transient step control and current abs_tol are not | Noise in metrics near the spec edge; false FAIL/PASS at tiny margins | Separate V/I tolerances; numerical-error band in the bracket; frozen time grid (P1) |
| A11 | Adjoint sensitivities exist and are right for every analysis | v2 A8, Part D | Medium-high: AC of nonlinear circuits, transient, switching, behavioural models | Zero or wrong gradients in limiting models; costs higher than "one extra solve" | Autodiff device models; FD gradient checks in CI; direct method for transient (P1) |
| A12 | Corner-theorem preconditions hold for "linear DC" | v2 A5, WT §7 | Medium: shared knobs, power metrics, real macromodels | Wrong "exact" label | Tight applicability checker; exclude non-linear functionals (power) (P0 — correctness) |
| A13 | Blocks compose via affine forms and interval contracts | SD §2, WT §8 | **High**: loading, shared supplies, forms linearized at different points | Wrong system-level results | Store linearization point + validity radius; impedance and dynamics in contracts (P2) |
| A14 | The operating point is unique and stable | implicit | Medium: start-up circuits, feedback loops, references | DC/AC specs "pass" at an unstable or wrong equilibrium | Automatic stability and start-up specs (P1) |
| A15 | Temperature is one global knob | WT §3 | Medium-high for anything dissipating power | Self-heating and gradients break tracking | Per-part ΔT = θ·P on top of ambient (P2) |
| A16 | Aging is one linear "life" knob | WT §3 | Medium | Stress-dependent aging mis-scaled; direction sometimes unknown | Aging magnitude from per-part stress; sign as knob when unknown (P2) |
| A17 | An inner-bound point "really happens" | v2 A1, WT §4 | Medium | "Definite FAIL" rests on model plus numerics; statistically meaningless corners | Tag counterexamples with probability (σ-distance) and model-conditionality (P1) |
| A18 | FAIL is "always definite" for every confidence level | SD §9 | **Certain** for yield-type specs | A yield spec cannot be failed by one counterexample | Define verdicts per confidence class (P0 — semantics) |
| A19 | Affine forms are a safe common currency | v2 D2, SD §9 | Medium | Estimated `err` laundered into "guaranteed" when combined; forms reused far from their linearization point | Provenance and validity metadata on every form (P1) |
| A20 | Monte Carlo at sign-off proves what is claimed | v2 A8, WT §6 | Medium | 3σ targets need ~2,200 zero-fail runs; conditional on the chosen corner | Importance sampling at the worst point(s); board-level statement (P1) |
| A21 | Specs are scalar measures on static setups | SD §4–6 | Medium | Narrow high-Q peaks missed on grids; dynamic co-occurrence (supply droop during load step) ignored | Adaptive frequency refinement; dynamic range stimuli in setups (P2) |

### 2.1 A1: smoothness

**The design relies on it.** Sensitivities, the straight line, and the "3σ point on the ellipsoid" all assume f(ε) is differentiable. `../specs.md` §4 already notes that settling time jumps.

**How it breaks on boards:**

- **Max-type measures:** peak overshoot = max over time; "for all f in band" = max over frequency; worst over a sweep. By Danskin's theorem these are only directionally differentiable. The gradient jumps whenever the argmax changes from one ringing peak or one resonance to another.
- **Crossing measures.** Rise time and delay are smooth while the crossing is transversal (dt*/dp = −(∂v/∂p)/v̇). They **jump** when a crossing appears or disappears. Settling time is discontinuous whenever a ringing peak enters or leaves the band.
- **Regime changes** [V]. The boundaries themselves move with the knobs:
  - LDO dropout: dVout/dVin jumps from about the PSRR to about 1 (TI SLVA079).
  - CCM/DCM: dVout/dL and dVout/dR go from 0 to non-zero (TI SLVA057).
  - PWM vs power-save mode, which even changes the guaranteed DC accuracy (TPS62130: 785.6–814.4 mV in PWM, 781.6–822.4 mV in PSM).
  - The op-amp input-pair transition region, and phase reversal (OPA991 §7.3.5–7.3.6).
  - Comparator outputs, which are discrete.
- **Behavioural vendor macromodels** are full of IF/LIMIT/TABLE/ABS sources and switches. The gradient is piecewise constant, and often exactly zero in saturation.
- **Numerical noise.** Adaptive time steps and changing iteration counts make transient outputs piecewise-smooth functions of the parameters, with steps about the size of reltol. Moré & Wild (SIAM J. Sci. Comput. 33(3), 2011) [V]: "the effects of finite precision destroy smoothness of the simulation output".
- **Likelihood:** DC low–medium (rails, dropout); AC medium (phase margin with multiple crossovers, peaking); transient high; switching converters very high.

**Detection (cheap; adopt now):**

1. **Region signatures.** At every simulated point, record each device's region (diode on/off, BJT/MOS region, op-amp output clamped, switch states, current limit, and the index of the argmax peak). A signature that differs between nominal and worst point means "piecewise: re-linearize per region, or leave the local method".
2. **Gradient consistency.** Compare the simulated change f(x_w) − f(x_0) against the trapezoid ½(g₀ + g_w)·(x_w − x₀) computed from the two adjoint gradients. A large mismatch means curvature or a kink between the two points.
3. **Line check.** Add 1–2 simulations on the segment from nominal to worst point. This catches interior extrema and kinks on the path.
4. **Max-type measures as minimax.** Keep the top-k local maxima of the waveform or sweep as separate smooth pieces, and take the max over them in the optimizer. This is standard minimax, and SQP handles it.

**Fallback.** A spec flagged non-smooth gets range-corner enumeration, derivative-free local refinement, and sampling. Its best possible verdict is "PASS (sampled)", never "PASS (estimated)".

### 2.2 A2: one worst region

**Problem.** The loop is a local method. From nominal it converges to the nearest local worst point. Board examples with two or more separate worst regions:

- **The LDO ESR stability window.** A TPS76050 needs ESR "between 0.1 Ω and 20 Ω" (TI SLVA115) [V]. Both low and high ESR fail, the window depends on load, and electrolytic ESR moves 10–100× over temperature (CDE application guide) [V].
- Op-amp stability against capacitive load and closed-loop gain.
- A resonance landing on the test frequency.
- The bridge with a shared temperature knob in v2 A5, catch 2.
  - Its gradient at nominal is **exactly zero**: d/dt[1/(2+t)] = −0.25 and d/dt[−(1−t)/(2−t)] = +0.25.
  - So the loop has no direction for the maximum at all, and "finds" the minimum only because nominal happens to be the minimum.
- **Power in a load.** P(R_L) peaks at R_L = R_th. This is the maximum-power-transfer theorem: an interior maximum.

**In the statistical case** (FORM with several design points), Pf is under-counted: Der Kiureghian & Dakessian, Struct. Safety 20, 1998 [V].

**My LDO experiment [E]** (loop-gain model, PM vs ESR window, T, C and load as knobs). Here the loop found the true worst corner (hot, low ESR, −2.0° PM) in 3 rounds. **The loop is not always fooled; it is fooled when the benign direction is the one the nominal gradient points to.**

**Fix (now):**

- Multi-start from every range corner and from both signs of each dominant direction.
- For two-sided specs, run two searches (|offset| < X needs both sides).
- At sign-off, cluster the failing IS/MC samples by direction to discover extra design points (the HSCS idea: Wu, Bodapati & He, ISPD 2016 [V]). Use one mixture-IS component per design point.

### 2.3 A3: the gradient sees every knob that matters (the most dangerous one)

**Mechanism.** A limiter that is inactive at nominal contributes **zero** gradient. The loop never pushes the knob that would activate it. Re-linearizing at the predicted point doesn't help if that point is also benign. The loop converges, prediction and simulation agree, and it reports **PASS (estimated)**.

**Experiment E2 [E]: op-amp gain stage.**

- Setup: G = 1 + R2/R1 = 11 with ±1% resistors; input 0.10–0.20 V peak (range); VCC 5 V ±5% (range); T −40…85 °C. The output stage needs 0.25 V headroom at 25 °C, +1 mV/°C when cold. A small smooth distortion term is 0.002%, rising when hot.
- Spec: THD < 1%, exactly the form of `spec clip = tran(input).thd(V(out)) < 1%` in `../specs.md` §12.

| Method | Sims | Worst THD found | Verdict |
|---|---|---|---|
| Design's loop (nominal → gradient-sign corner → re-linearize) | 3 + gradients | 0.0026% at (A max, T hot, VCC nominal) | **PASS (estimated)**; prediction matched simulation |
| Enumerate the 8 range corners (A, VCC, T) | 8 | 2.51% at (A max, VCC low, T cold) | **FAIL** |
| Brute force, all 32 corners | 32 | 3.29% (A max, VCC low, T cold, R1 −1%, R2 +1%) | **FAIL**; 4 of 32 corners fail |

- VCC has zero gradient at nominal, so the loop leaves it at nominal. The smooth distortion term points to "hot", while clipping actually needs "cold".
- I constructed this to show the mechanism, but the pattern is generic: headroom, dropout, current limit, UVLO, saturation, comparator thresholds.
- The PSpice User's Guide documents the same failure for `.WCASE`: a BJT going into saturation fools the local gradient, and "even an optimizer, which checks the local gradients … is fooled by this circuit" [V].

**Fix (now):**

1. **Enumerate range corners by default.** Boards have about 5–10 range knobs: temperature, each supply rail, load(s), input level and frequency, source impedance, life. 2^k ≤ 256 simulations, and **one simulation evaluates every spec that uses that analysis**, so the cost is shared across specs. Beyond that, use model-guided corner search in the style of Solido's Fast PVT (a GP with a lower-confidence-bound infill criterion, patent US8612908B2 [V]).
2. **Regime-margin guard metrics, generated automatically from device models:**
   - op-amp output headroom (V_rail − |V_out| − V_sat) and input common-mode margin;
   - LDO dropout margin (Vin − Vout − V_drop(I));
   - BJT VCE − VCE,sat; MOSFET VDS − VDS,sat;
   - current-limit margin; comparator overdrive; UVLO margin.

   These are smooth and have non-zero gradients, so the loop can search them first. If any guard can be violated inside the box, every spec depending on that device is flagged "regime change reachable" and gets non-local treatment. This is the board analogue of Graeb's **sizing rules**: constraints that keep devices in their intended region (Graeb, Zizala, Eckmüller, Antreich, ICCAD 2001, pp. 343–349 [V]; bipolar version, Massier, Graeb, Schlichtmann, DATE 2008 [V]).
3. **Adopt PSpice AA's sanity check** [V]: a "worst" value that is not worse than nominal is a red flag.

### 2.4 A4: moderate knob count

**What scales well.** The adjoint gives every sensitivity for one extra solve per metric (DC/AC). Neither the loop's iteration count nor MC/IS cost grows with N.

**What doesn't:**

- **Finite-difference fallbacks.** Any analysis without adjoint or direct sensitivities costs N+1 simulations per point:
  - PSpice AA: 1 + N_params + 2·N_meas runs [V];
  - ngspice `.sens` perturbs "each parameter … independently" [V].
  - At N ≈ 1000 this is infeasible. Cadence's "WCD typically requires under 100 simulations for each spec … suitable for designs with a small number of specs/parameters" [V] is consistent with FD-based gradients (my inference).
- **Absolute worst case in high dimension.** The all-edges corner is √N·3σ from nominal: 30σ for N = 100. It is physically meaningless and it is exactly where linearization is worst. The statistical worst point stays at β ≈ 3 in σ units regardless of N, so its curvature error does not grow. **Absolute worst case should be reserved for specs with few knobs.**
- **Corner-theorem cost.** The direction proofs loosen as N grows. Neumaier–Pownuk measured about 20–25× one solve at mid size, rising to about 1,100–1,300× at around 10,000 parameters. The method is dense in m (the number of parameters) and ran out of 1 GB at about 14,500 [V, from the preprint]. "About 20 solves" is not a scaling law.
- **Reporting.** Hundreds of tiny contributors need grouping by block, part family and knob kind.
- **Scale in practice.** ECSS partitions large circuits into functional blocks and requires a sensitivity analysis to justify every neglected effect [V]. A board has hundreds to thousands of passives, each with 3–6 knobs, but a single spec usually depends on 10–50 of them.

**Fix:**

- Adjoint for every analysis kind.
- Screening from the adjoint gradient. Keep the knobs making up 99% of Σ|cᵢ| (worst case) or Σcᵢ² (statistical), and lump the rest into a **bounded remainder term** that is kept, never dropped.
- Grouped contributor reports.
- Later: DGSM (Sobol' & Kucherenko 2009 [V]) and active subspaces (Constantine, SIAM 2015 [V]) for nonlinear high-dimensional cases.

### 2.5 A5: independent knobs in a box

- **Lot structure.** A datasheet ±tol already includes lot-to-lot variation, so a "lot knob ±tol plus per-part knob ±tol" model double-counts, up to 2× the datasheet edge. The physically right region is |lot + local| ≤ tol, which is a polytope, not a box.
- **The evidence** [V] (lambdafox measurements; TI RES11A; Vishay arrays):
  - Within a lot, the offset dominates. Measured means were −0.26% and −1.68%, with σ = tol/7 and tol/64.
  - Correlation applies **separately** to initial value, TCR and drift. The RES11A has ±12% absolute tolerance but ±500 ppm ratio tolerance, and TCR ratio ±2 ppm/°C against 3.5–35 ppm/°C absolute.
  - Not everything correlates. In a dual op-amp, Vos is mismatch-dominated (independent per channel) while GBW and Iq are shared [U].
- **Range knobs are correlated too:** supply vs temperature, load vs supply, and aging vs temperature (Arrhenius). Some corners are physically impossible, which is pure pessimism.
- **Parameters within one part are correlated** (Vth, Rds(on) and gfs of a MOSFET; β and VBE). Vendors almost never publish this.
- **Fix:**
  - Knob *structures*: common plus differential, lot inside tolerance, with separate lot knobs for value, TCR and drift.
  - Linear constraints on the knob space. The worst corner of a linear model over a polytope is still a cheap LP; the corner theorem needs boxes, so apply it per slice.
  - For yield, a lot-mixture model, reported as "yield per lot": e.g. "90% of lots ≥ 99.9%; a reel offset at −0.6·tol gives 95%".

### 2.6 A6: known distributions

**What we assume.** Truncated normal, σ = tol/3, centred, with ±tol as a hard edge (v2 A7; walkthrough appendix).

**Evidence [V]:**

- **TI OPA991 §7.3.9.** For zero-mean specs, "the typical value is equal to the mean plus one standard deviation"; the Vos max is about 5σ. For specs with no max, TI suggests the 6σ value. "Unless there is a value in the minimum or maximum specification column, TI cannot assure the performance."
- **"Typical" means different things.** Steffes (Electronic Design): the industry practice is ±1σ. Test limits range from ±3.5σ to more than 8σ, and AC specs are not production-tested. Ganssle (Embedded.com) collects "typical" meaning the mode (TI E2E), the mean (Energy Micro), or "the average of our characterization data" (Microchip).
- **Resistor reels are tight but offset**, as in A5.
- **Tolerance created by selection gives a uniform distribution**, σ = tol/√3 (EDN, Desjardin 2016).
- **BJT hFE:** min/max only at one current, typical-only elsewhere. The graded bins (BC547 A/B/C) are culled, truncated segments with typicals off centre. The ungraded range 110–800 is 7:1, not a ± around a nominal (onsemi BC546/7/8).
- **Electrolytic ESR is often max-only,** and MLCC capacitance depends on time since the last reflow (aging resets above the Curie point) [V].

**Experiment E1 [E].** The walkthrough's S3 (f_L ≤ 30 Hz) at the cold, end-of-life corner, 20,000 samples each; everything identical except the statistical knobs' distribution:

| Distribution of the six statistical knobs | Failure rate |
|---|---|
| Truncated normal, σ = tol/3 (design default) | **0.89%** (matches walkthrough's 177/20,000) |
| Truncated normal, σ = tol/2 | 4.1% |
| C_in only: narrow reel offset at −0.6·tol (others default) | 5.0% |
| Uniform over ±tol | **11.6%** |
| Culled/bimodal (uniform in 0.6–1.0·tol, either sign) | 32% |

- For S1 (VC ≤ 6.5 V), "0 of 20,000 fail" under the default becomes 0.02% under uniform and 0.57% under culled.
- **The verdict "~1 in 110 boards" rests on an assumption no datasheet supports, and it moves 13–36×.** By comparison, all the linearization issues in the walkthrough were worth under 2 Hz (31.7 vs 33.4 Hz).

**A distribution-robust bound we can offer instead.**

- For a *linear* metric S = Σcᵢεᵢ with independent εᵢ, the only assumptions are that each εᵢ has a **symmetric unimodal** density inside [−1, 1].
- Every such density is "more peaked" than uniform on [−1, 1].
- Birnbaum's theorem: sums of independent, more-peaked symmetric unimodal variables are more peaked (Ann. Math. Stat. 19:76–81, 1948 [V citation; theorem statement from memory, PDF blocked]). Proschan's convex-combination version: AoMS 36:1703–1706, 1965 [V citation].
- So P(|S| > t) is **maximised by uniform knobs, for every t**.
- In robust control this is Barmish & Lagoa's "uniformity principle": "The uniform distribution: a rigorous justification for its use in robustness analysis", MCSS 10:203–222, 1997 [V citation].
- For resistive networks, Kettani & Barmish show that "performance bounds obtained via this new approach differ considerably from … conventional Monte Carlo" (IEEE TCAS-I 53:1289–1299, 2006) [V abstract].
- I checked it numerically [E]: 48 of 48 (coefficient set, threshold) cases had uniform tails ≥ triangular, truncated-normal and truncated-Laplace tails.
- A side result: the truncated-Laplace shape beat the truncated normal in the far tail. **σ = tol/3 is not conservative even among "reasonable" shapes.**
- **Caveats:**
  - It is exact only for linear metrics; for nonlinear ones, apply it to the linearization at the worst point, or just run IS under uniform.
  - It does not cover lot offsets, so keep those as range knobs.
  - It does not cover culled/bimodal distributions; their worst case is the two-point distribution, i.e. the absolute worst case.
  - Hoeffding's inequality (JASA 58:13–30, 1963 [V]) is distribution-free too. But for 1e-3 it needs t = 3.72·RSS, which is weaker than the absolute worst case unless there are more than about 14 comparable contributors. It is not useful for typical blocks.

**Fix (now):**

- Every knob carries **provenance** (guaranteed limit / characterization-only / typical-only / assumed multiplier), a **coverage multiplier** (how many σ the limit represents) and a **truncation**, all visible.
- Statistical verdicts are shown as a pair: "0.9% (assuming truncated normal σ = tol/3) · ≤ 11.6% (any symmetric unimodal within tolerance)".
- If the verdict flips between the two, it is **UNDECIDED (distribution-dependent)**. That is the honest answer.
- Tools disagree on what a Gaussian tolerance means [V], so store the limit, the σ-multiplier and the truncation explicitly:

  | Tool | Tolerance with a Gaussian distribution means |
  |---|---|
  | PSpice GAUSS | 1σ (yet `.WCASE` pushes Gaussian parameters 3σ) |
  | LTspice `gauss(x)` | x is σ |
  | SIMetrix, Multisim, Altium | 3σ |
  | Micro-Cap | user-set |
  | ngspice | rvar/sigma |

### 2.7 A7: range knobs are few, and every corner is real

- **Count.** Board range knobs are typically 5–10, and 2^k enumeration is affordable up to about 8. This is what industry does anyway: ADE Assembler corners, PrimeWave "thousands of corners" [V]. Above that, use model-guided search (Fast PVT) or the gradient with random restarts.
- **"Always worst" is right semantically, but the box is often too big.** Examples: minimum temperature with end of life when aging is thermally driven; maximum supply with maximum load when the regulator droops. Let users constrain the range box, e.g. "life at 85 °C mission profile" or "Vin and load linked by the source's output impedance".
- **Range knobs are not linear either** [V]:
  - Reference drift given by the **box method** "does not specify the exact shape and slope"; the drift is "expected to be non-linear" (TI REF35). The worst value can sit at an interior temperature.
  - Resistor TCR is piecewise (RES11A: 35 / 23 / 3.5 ppm/°C across three temperature bands).
  - X7R capacitance vs temperature is non-monotone.
  - **So temperature needs a 1-D scan (3–5 points), not just its two ends.**

### 2.8 A8: what "yield" means

**Two separate issues.**

**(a) Quantifier order.** A board must pass at *every* operating condition. True board yield is P_s[∀θ: f(s,θ) ok], which is ≤ min over θ of P_s[f(s,θ) ok]. The design computes the right-hand side (MC at the worst range corner). The two differ when different boards fail at different corners, which is typical for two-sided specs.

**Experiment E4 [E].** VC window 4.95…6.05 V, range corners (T, VCC):

| Quantity | Loss |
|---|---|
| Fail at (cold, high VCC) | 26.7% |
| Fail at (hot, low VCC) | 10.1% |
| Design's "MC at the worst range corner" | **26.7%** |
| Board fails at *any* condition (true) | **36.8%** |

**(b) Many specs.** Board yield is the probability that *all* specs pass. 50 specs at β = 3 each give up to 50 × 0.135% ≈ 6.7% loss by the union bound.

- Schenkel et al. (DAC 2001) define yield over the intersection of acceptance regions, with each sample evaluated at each spec's worst operating condition. Their example: a circuit that works over only 90% of the operating range counts as a failure [V].
- They estimate overall yield by MC on the **spec-wise linear models** taken at each worst-case point: 10,000 samples, **no extra simulations**, within 1–2% of real MC [V].

**Fix:**

- Board-level yield = MC or IS over statistical knobs, taking the worst over the (pruned) range corners per sample, over all specs.
- Screen first on the stored linear models at zero simulation cost (Schenkel), then confirm with IS.
- Report both "per-spec" and "board" yield.

### 2.9 A9: the model is right

**Evidence [V]:**

- **TI "Green-Williams-Lis" op-amp models** "target typical electrical characteristics at room temperature" (SBOA338).
- **Vos is a single fixed source.** The OPAx333 model has `DC=-3.00E-06` with typical drift, against a datasheet of 2 µV typical / 10 µV max.
- **Zout is "often not modeled correctly".** For the LMV844, "ZOUT does not match the data sheet" (TI "Trust but verify" blog).
- **TI power models** (TPS7A20, LMR33630A, LM5145): "Temperature effects are not modeled". TPS7A20: "Ground/quiescent current have not been modeled". The TPS40170 model is encrypted.
- **MOSFET models:**
  - Infineon models are "typical devices", with worst-case hooks dVth, dRDS(on) and dZth, and "no one-to-one relation between a deviation in the manufacturing process and the variation in a particular device parameter".
  - Nexperia: "always typical values and do not represent the limits of process variation". The model is the mean device; IGSS is not modelled.
  - onsemi offers corner models for a few families only.
- **Zener model** (Diodes Inc BZX84C51): a fixed 48.5 V battery.
- **Legacy Burr-Brown models:** "VALID ONLY AT AN AMBIENT TEMPERATURE OF 25 DEGREES".
- **No independent measurement-vs-model study was found** [gap].

**Consequences:**

- Every verdict is conditional on the model.
- A model parameter without a knob is silently treated as exact.
- Effects the model does not contain cannot be bounded by any knob range.

**Fix (now):**

1. **Per-model coverage map.** For each datasheet parameter: modelled and knobbed / modelled but typical only / not modelled. Filled in by the AI part-modelling pipeline (vision §5).
2. **Exposure report.** The adjoint gives ∂f/∂p for *every* model parameter at no extra cost, knobbed or not. Rank |∂f/∂p| × (plausible range) for parameters without a knob, and warn: "this spec is sensitive to R_out of U3, which has no tolerance data."
3. **Standard knob wrappers**, grafted externally in the way practitioners already do [V]: a series Vos source, input-bias current sources, a GBW or gain scale, Rds(on) scaling, a Vth shift, and vendor d-parameters and corner flags where they exist.
4. **Verdict tag "model-conditional",** listing the gaps from item 1. A missing limit gives UNDECIDED, not a guess.
   - The NASA TRMM worst-case analysis did exactly this by hand: "until radiation information for the 2N4405 is available, the analysis can not be completed" [V].

### 2.10 A10: simulator accuracy

**The design's claim** (v2 Part D.3): Newton rel_tol 1e-3 means results "only good to about 0.1%".

**Test [E].** I replicated the simulator's own acceptance rule (step-size test, accept the new iterate) on the CE amplifier. At rel_tol 1e-3 the final VC error is 1.2e-5 V (about 2e-6 relative). A 0.1% R1 change is resolved to 1.5e-5 relative error.

- Newton converges quadratically, so the accepted iterate's error is roughly the *square* of the last step.
- **For DC the claim is overstated.**
- (I could not build a Rust probe against the real crates: OpenBLAS pulls in `openssl-sys` and the machine has no OpenSSL headers. Worth re-running once it builds.)

**The real accuracy risks:**

- **One `abs_tol = 1e-6` for voltages and currents** (`lib.rs:47`). For branch currents that means 1 µA, so µA-level currents (quiescent current, bias current, leakage) are unresolved. SPICE uses separate VNTOL = 1 µV and ABSTOL = 1 pA [V ngspice manual].
- **Transient.** Today the time step is fixed with no truncation-error control (`trans.rs:14-16`). Once adaptive stepping arrives, metrics pick up reltol-sized, non-smooth step-control noise.
  - The evidence thread's toy test [E, theirs]: an adaptive trapezoid RLC at reltol 1e-3 had an overshoot error about 4× the metric's entire change over a 0.2% capacitor sweep. The FD sensitivity was 13% low.
  - Xyce: transient direct sensitivities use "the same time steps … It does not impose any additional error control" on ∂x/∂p [V].
- **Tools integrate differently.** LTspice's default trtol is 1 vs ngspice's 7 [V].

**Fix:**

- Separate V and I tolerances.
- An engine-grade preset (DC Newton is cheap to tighten).
- Discretize-then-differentiate adjoints on the exact step sequence.
- A **numerical-error band** in every bracket: re-run the decisive point at 10× tighter tolerances. Treat margins below the band as UNDECIDED (numerics). Never report FAIL or PASS inside the noise floor.

### 2.11 A11: adjoint sensitivities everywhere

- **AC of a nonlinear circuit is not "one extra solve".** Small-signal gm, rπ and C depend on the DC operating point, which depends on every knob. dH/dp therefore needs a DC adjoint whose source term contains **second derivatives** of the device equations, plus the AC adjoint. That is doable with autodiff (dual numbers), but it is a design requirement.
- **Derived AC metrics** (f₋₃dB, unity-gain frequency, phase margin) need implicit differentiation at the crossing. The phase margin **jumps** when the number of 0 dB crossings changes.
- **Transient.** Each scalar objective needs its own backward integration over the stored forward trajectory. Xyce: the transient adjoint "is best when the sensitivity of interest concerns only one or a handful of time points" [V]. Max-type metrics need the adjoint at t* (Danskin). Crossings need the event formula dt*/dp = −(∂v/∂p)/v̇ (Galán et al. 1999 [V]).
- **Switching converters** need saltation matrices at switching events, plus sensitivities of the periodic steady state (shooting method with a transposed monodromy solve).
- **Behavioural and limiting models** give zero or undefined gradients in saturation. A clipped op-amp reports "nothing upstream matters". **A zero sensitivity is not proof of insensitivity**, as with dVout/dL in CCM.
- **Fix:**
  - Autodiff device models from day one.
  - Gradient checks against high-precision FD in CI.
  - The direct method for transient with few knobs, the adjoint for many knobs and few objectives.
  - Region signatures (A1) to flag zero-gradient regimes.

### 2.12 A12: the corner theorem in practice

**What the theorem needs**, from the Neumaier–Pownuk text as read by two threads (details in section 5):

- (i) Every uncertain parameter enters as **one diagonal entry** in K + B·Diag(x)·A, i.e. one rank-one term, and the diagonal entries are independent (a box).
- (ii) The matrix is regular (non-singular) over the whole box.
- (iii) The quantity is a component of the solution. By Sherman–Morrison, any *linear functional* of the solution also works: node voltages, branch currents, differences.
- (iv) The theorem guarantees the extremum is at *some* vertex; finding which one needs directions proven over the whole box.

**In practice:**

- **Knobs shared across several stamps break (i):** temperature, lot, the common-mode knob of a matched network, one `.MODEL` parameter used by several instances, and supply only if it appears in the *matrix*. Supply in the RHS is fine, since x is linear in b.
- **Nonlinear functionals of the solution are not covered.** Ratios of two solution components *are* covered: every component shares the same Möbius denominator 1 + δ·aᵀc, so a ratio is again Möbius in each knob. Not covered:
  - **Power** (V·I, V²/R, I²R): maximum power transfer is an interior maximum.
  - Absolute values, and anything quadratic.
  - **The automatic derating specs in `../specs.md` §8 (resistor power ≤ 50% rated) are exactly this class.**
- **Some single parameters touch two rank-one terms** and so break (i) even without sharing: an ideal-transformer turns ratio (it appears in a row and a column), gyrators, potentiometer wipers. The bibliography thread checked V_L = Vs·n·R_L/(n²R_L + R_s): it peaks at n = √(R_s/R_L), an interior maximum.
- **N&P07 Ex. (41)** (nominal gradient picks the wrong vertex) only happens beyond ±25% uncertainty (checked numerically by the bibliography thread). For moderate tolerances the better citation is [TS00] (note Kolev's 2001 critical comment on it). Our own walkthrough §5.7 (gain vs temperature) remains the most convincing board-level example.
- **Only DC.** In AC the element admittances are complex and |H| is not monotone.
- **Real macromodels contain nonlinear elements.** A vendor op-amp model makes the circuit nonlinear unless the engine substitutes a linear op-amp. The "linear DC" class in practice is passive networks, sources and linear controlled sources.
- **The 388-network numerical check** illustrates the theorem; it does not extend it. Random resistor/controlled-source networks are not representative of op-amp circuits with feedback.

**Fix:** a strict applicability checker.

- It checks: one appearance per knob, a box, a linear functional or ratio, DC only, and regularity. Under the rank-one condition det(K + B·Diag(x)·A) is affine in each xⱼ, so its extremes over the box are at vertices. **The same sign at every vertex therefore proves nonsingularity** (2^k determinants; use an interval check when k is large). Flag singularity to the user on its own.
- The "exact" label is issued only when every check passes. Everything else goes to the loop.
- **Move power and derating specs to the loop with range-corner enumeration.**

### 2.13 A13: block composition

- **Linearization points differ.**
  - An affine form linearized at block A's own worst point is biased everywhere else.
  - The system worst point is usually different: block A is worst cold, block B worst hot.
  - So reusing block forms "without re-simulating" (walkthrough §8.4) is only valid near their linearization point.
- **Contracts must carry impedances and dynamics:**
  - source and load impedance over frequency;
  - capacitive-load range;
  - load-step profile;
  - start-up and sequencing states.

  The evidence:
  - The Middlebrook input-filter criterion: the filter's output impedance must stay "far below" the converter's input impedance; "the two curves should not overlap" (TI SNVA538) [V].
  - An LDO's stability depends on the downstream capacitor's ESR (SLVA115) [V].
  - ECSS §4.4 demands attention to "reciprocal interaction of the functional blocks (e.g. power supply variations induced by load changes …)" [V].
- **The theory base.** Benveniste et al., *Contracts for System Design* (FnT EDA 12(2–3), 2018) [V citation] gives the algebra, but its examples are not analog. Sun, Nuzzo, Wu & Sangiovanni-Vincentelli, "Contract-based system-level composition of analog circuits" (DAC 2009) [V citation; content U] is the closest analog work.
- **Fix:**
  - Every stored form carries {linearization point, validity radius, err provenance}.
  - Contracts include impedance ranges vs frequency.
  - System-level specs re-run the loop at system level; block forms serve as a warm start.

### 2.14 A14: a unique, stable operating point

- **Newton returns one solution.** Bandgaps, self-biased current sources, latches and Schmitt triggers have several. A corner can land in the wrong state, and so can real hardware at power-up, while the simulation stays in the good one through continuation.
- **DC and AC analyses happily compute specs at an unstable equilibrium.** An oscillating amplifier "passes" its DC bias and AC gain specs.
- **Fix: automatic specs.**
  - **Stability** at every simulated point: poles of the linearized (G, C) pencil, cheap at board scale, or loop-gain phase margin on marked loops.
  - **Start-up** from a zero or ramped state for circuits with feedback or self-bias.
  - Later, all-solutions methods for small blocks (Yamamura; already in the bibliography).

### 2.15 A15–A16: temperature and aging as single global knobs

- **Temperature.** Part temperature = ambient + θ·P.
  - Self-heating makes a power resistor, regulator or reference much hotter than its neighbours, which breaks the tempco-tracking assumptions behind matched pairs.
  - Vishay drift calculation: film temperature = θ_amb + R_th·P, and drift "doubles for every 30 K temperature rise" [V].
  - Use SPICE-style per-instance DTEMP from the operating-point power.
- **Aging depends on stress** [V]:
  - Arrhenius in each part's own temperature; ECSS defaults are 1.35 eV metal film, 1.67 eV ceramic, 0.43 eV tantalum.
  - Voltage (MLCC).
  - Ripple current (electrolytic ESR is "generally assumed to double" over life, and self-heating from ripple drives the aging).
  - ECSS: "ageing process cannot be assumed a priori to be linear"; aging is "biased (sometimes random)".
- **Fix:**
  - Derive per-part aging magnitudes from stress at the operating point.
  - Keep "life" as the range knob that scales them.
  - Add a sign knob where the drift direction is unknown.

### 2.16 A17–A18: what the verdicts mean

- **Inner bounds.** "Really happens" means "happens in the model, in the stated knob box, at this numerical accuracy". Tag each counterexample with:
  - its **probability** (σ-distance, or likelihood relative to nominal);
  - model-conditionality;
  - the numerical-error band.

  An all-parts-at-edge corner is reachable, but its probability may be ~1e-9.
- **FAIL is not uniformly definite.** For `worst_case` specs, one reachable violation is a definite FAIL. For `yield` specs, a violating point proves nothing: the spec is about a probability. A yield FAIL needs a statistical statement, e.g. "the Clopper–Pearson lower bound on the failure rate exceeds the target at 95%". **Define the verdict per confidence class:**

  | Confidence class | FAIL means | PASS means | UNDECIDED means |
  |---|---|---|---|
  | worst_case | A simulated point in the box violates | Proven outer bound inside spec (exact / AA / p-solution); or "estimated" if only the loop | Neither |
  | yield ≥ Y | Lower confidence bound on failure rate > 1−Y (IS or MC), or robust bound says so | Upper confidence bound < 1−Y **under the stated distributions**, plus robust-bound status | CI straddles the target, or verdict flips between default and robust distribution |
  | nominal | Nominal violates | Nominal satisfies | — |

- **Fix the sigma arithmetic** in `../specs.md` §7 and §10: "3σ (99.9%)" is inconsistent. One-sided 3σ is 99.865%; two-sided is 99.73%; 99.9% one-sided is 3.09σ.

### 2.17 A19–A20: affine forms as currency; Monte Carlo sign-off

- **Affine forms.** A simulation-derived form's `err` is an *estimate*. Combine it with a guaranteed form and the sum must be labelled estimated. Carry provenance per form (rigorous / estimated / unknown), plus the linearization point and validity radius, and apply a weakest-link rule on combination.
- **MC sign-off:**
  - The rule of three and "0 of 300 → < 1% at 95%" are correct: 1 − 0.05^(1/300) = 0.994%.
  - But a one-sided 3σ claim needs 2,218 zero-failure runs, and 99.9% needs 2,995. Anything past about 4σ is out of reach.
  - The result is also conditional on the chosen range corner (A8).
  - "Run near the worst point" (v2 A8) is only valid with importance weights.
- **Fix:** replace plain MC with IS centred at the design point(s) from the loop.
  - Experiment E7 [E], S3: IS with 2,000 runs gives 0.945% ± 0.078% (95% CI); plain MC needs 20,000 runs for ±0.13%. The advantage grows exponentially for rarer events.
  - At 200 runs the IS interval under-covered (0.60 ± 0.20% vs a true ~0.9%). **IS needs weight diagnostics** (effective sample size) and a minimum run count.
  - Use exact Clopper–Pearson bounds and sequential stopping. Cadence's yield-verification stop rule is a Clopper–Pearson test [V].

### 2.18 A21: specs as scalar measures on static setups

- **Frequency grids miss narrow high-Q peaks.** Refine adaptively near local maxima of |H|. Ferber et al. give guaranteed bounds over frequency *intervals* (IJNM 31(2), 2018) [V].
- **Static range knobs miss dynamic co-occurrence:** supply droop during a load step coinciding with a signal peak. Put these in setups as transient stimuli, not as static knobs.

---

## 3. What industry actually does

### 3.1 Summary table

| Tool | Worst-case / corner method | Statistical / yield method | High sigma | Stress / derating | Guarantee claimed | Tag |
|---|---|---|---|---|---|---|
| **Cadence ADE Explorer / Assembler + Variation Option (VVO)** | Corner sweeps; worst-case corners from statistics; **worst-case distance (WCD)** "under 100 simulations for each spec … small number of specs/parameters"; K-Sigma corners (model → corner → line search) | MC with auto-stop on target yield; yield verification by sample reordering with a Clopper–Pearson stop rule; mismatch contribution via variance-based global sensitivity analysis (sparse regression, R²) | Scaled-sigma sampling (SSS), "more accurate than WCD for nonlinear behavior"; WCD | — | Only the sampling side of yield verification is a statistical bound; the rest is empirical | [V] (datasheet, white papers, patents, via Wayback) |
| **Spectre FMC** | "Worst samples": ML response surface orders MC samples worst-first, simulates in that order, refits | Same, from a finite pre-drawn sample | Relative to the drawn sample space | — | Stopping rules "kept as a trade secret" | [V] white paper |
| **Spectre / HSPICE `dcmatch`, `acmatch`** | — | Linearized: mismatch sources → σ of outputs, sorted contributions, "calculus of probability instead of sampling" | — | — | Linear assumption stated | HSPICE [V]; Spectre [S] |
| **Siemens Solido** | **Fast PVT**: GP model + lower-confidence-bound infill (active learning) over PVT; sigma-driven corner = most probable point on the spec boundary via line search from nominal | PVTMC Verifier: joint PVT × MC, "worst case PVT at nominal may not be the worst case at the target sigma" | HSMC: models only **order** pre-drawn samples; "self-verifying" convergence curves | — | "Brute-force accurate" (empirical; relative to finite sample) | Patents [V]; PVTMC [S] |
| **MunEDA WiCkeD (Cadence since 2024)** | WCD by name; "simulator-true worst case"; deterministic and MC-based WCA combinable | MC at worst-case corners, contributor analysis, fast sensitivities, scaled / importance sampling | FORM through worst-case point; 110k mismatch parameters | — | Vendor itself: still run brute MC for "non-linearity, number of variables, complexity of test bench, low-sigma" | Site [V]; TUM origin (Antreich, Graeb guidance) [S] |
| **Synopsys HSPICE / PrimeSim / PrimeWave / ASO.ai** | Corners, `.DATA`, "thousands of corners", RL optimization across PVT | MC with LHS / low-discrepancy sequences; DCMatch / ACMatch | "Sigma amplification" (scaled sigma); PrimeSim HSMC/PYE | MOSRA aging | "All at 2–3σ … overly pessimistic" (their own guide) | [V] |
| **Keysight ADS** | — | MC yield, yield optimization (= design centering) with dynamic trial count; quadratic "Shadow Model" surrogates; yield-sensitivity histograms | — | — | Confidence tables (normal approximation) | [V] (archived docs) |
| **PSpice `.WCASE`** | Nominal + one +0.1% run per parameter (direction only) + **one** final run at the signed limits | `.MC` (UNIFORM default; GAUSS value = 1σ, truncated ±3σ) | — | — | "Shows the true worst-case results when the collating function is monotonic … Otherwise, there is no guarantee" | [V] |
| **PSpice Advanced Analysis** | Sensitivity: runs at 40% of tolerance, linear scaling, then worst-case min/max at signed limits (1 + N_p + 2·N_meas runs) | MC with draggable spec cursors, live yield, PDF/CDF | — | **Smoke**: avg/RMS/peak vs derated max operating conditions; red >100%, yellow 90–100%; **nominal transient only**; default "No Derating" | "Sensitivity assumes that the measured quantity varies monotonically" | [V] |
| **LTspice** | `wc()` idiom = exhaustive 2^N+1 vertex enumeration via `.step` | `mc()`, `flat()` uniform; `gauss(x)` x = σ; `.step` + `.meas` | — | — | None | [V] (help mirror; the ADI article via author republications) |
| **SIMetrix / SIMPLIS** | `WC()` random-endpoint distribution; `.SENS` DC-only linearized | GAUSS = 3σ; `GAUSSTRUNC`; lot / match; user distributions | — | SOA limits (`.SETSOA`) in DC and transient; no derating library | **DVM** testplans → automatic spec report with drill-down | [V] |
| **Micro-Cap 12** | **EVA** (sensitivity sign) *and* **"EVA (Optimizer)"** min/max over the box, "more reliable … substantially slower"; side-by-side RSS / MCA / EVA table | MC up to 100k, "Report When" fail expression | — | Smoke with derating libraries | Initial vs bias tolerances (Neg / Pos / Rnd) per ECSS / RAC style | [V] |
| **NI Multisim** | DC/AC only: sensitivity sign → one run; "a predictor of the true worst case, but not as a final say" | MC (tolerance = 3σ) | — | — | Candid limitations page | [V] |
| **Altium Mixed Simulation** | No directed WC | MC default **5 runs**, 10% default tolerances; sensitivity excludes the other analyses | — | — | — | [V] |
| **ngspice / KiCad** | `.sens` DC/AC by perturbation; KiCad GUI has no MC / WC | `agauss`, `limit()` random vertex; scripted | — | — | — | [V] |
| **TINA (full)** | MC / worst-case, negative tracking | Mean / sd / yield; click a curve → component values | — | Stress analysis colours parts red | — | [V] (marketing pages) |
| **Flux (Mar 2026)** | — | Prompt → netlist → SPICE → explanation; "evaluate results against your spec"; **no tolerance / MC / WC mentioned** | — | AI design review: resistor power, capacitor voltage margin (nominal rules) | — | [V] |
| **Circuit Mind, Celus, Diode Computers, Quilter** | — | — | — | Circuit Mind: "Stress & Derating" reports (method unknown) | — | [V] marketing; methods [U] |
| **atopile, JITX** | Interval arithmetic with no correlation (atopile: X − X gives [−10, 10]; JITX's own divider example is inflated) | — | — | — | — | [V] |

### 3.2 IC-grade tools: what matters for us

1. **WCD is real, industrial, and hedged by its own vendors.**
   - MunEDA was founded around 2001–2002 "under the guidance of … Prof. Kurt Antreich and Prof. Helmut Gräb (TUM)", and WiCkeD is named for the WCD algorithm [S]. muneda.com now redirects to Cadence (from late May 2024 per Wayback) [V].
   - Cadence VVO lists WCD as a high-yield method needing "under 100 simulations for each spec … suitable for designs with a small number of specs/parameters". It recommends SSS for nonlinear behaviour and many parameters [V].
   - MunEDA tells users to still run brute MC for non-linearity and low sigma [S].
   - **So neither vendor that ships WCD treats it as a proof.**
2. **Models order samples; they never decide.**
   - Solido HSMC: the model is used "to merely order the samples, rather than … to make a decision about whether a sample is feasible or infeasible" (US9483602B2) [V].
   - Cadence worst-samples and yield verification use the same principle, and state that the "higher than target" side depends on "a conservative estimate of model accuracy" [V].
   - **This is exactly how our linear model should be positioned.** It proposes points, and verdicts come from simulations.
   - Cadence's three-valued yield stop is already a PASS / FAIL / keep-sampling rule. **It reports which side is pure sampling and which is model-based.** We should do the same.
3. **Joint PVT × statistics.** Solido PVTMC exists because "worst case PVT at nominal may not be the worst case at the target sigma" [S]. Our "range corner first, then statistical" order has that exact blind spot. The walkthrough got lucky: the aged, cold corner was worst for both.
4. **Statistical corners at the target sigma, not all-knobs-at-3σ.**
   - Cadence K-Sigma: ≤ 200 MC samples, model, corner, then a line search on the segment from nominal ("up to 11 extra simulations, usually 1"). Claimed "10–40X speedup … within a 0.5-sigma difference" [V].
   - Solido sigma-driven corners: "the most probable process point giving the target specification value" [V].
   - Both are MC-seeded versions of our loop. HSPICE's own guide calls all-at-3σ "overly pessimistic" [V].
5. **Contribution tables are an expected output.**
   - Linear: DCMatch "calculus of probability".
   - Regression with R²: Cadence mismatch contribution, patent US10262092B1 [V].
   - Our adjoint gives the linear table almost for free. **Pair it with a linearity indicator** (R² or the loop's prediction mismatch).
6. **Non-Gaussian handling.** Cadence runs normality tests and fits an "extended from normal" family. Solido advertises no Gaussian assumption [V]. Board tolerances are often uniform, binned or offset (A6), so this matters more for us than for IC.
7. **Reuse across design iterations.** Solido "Additive Learning … retains and reuses results and models from prior simulations" [V]. For an interactive editor this is essential: warm-start the loop from the last worst points.
8. **Speedup claims depend on what they are compared against.** Spectre FMC: 1,340× against a 1M-sample space, but about 11× against 10k [V]. Report our simulation count *and* the brute-force reference.

### 3.3 Board-level tools: what matters for us

1. **Every board-level "worst case" is one jump along the nominal sensitivity signs** (PSpice `.WCASE`, PSpice AA worst-case min/max, Multisim, Micro-Cap EVA) [V].
   - The vendors document the failure themselves:
     - PSpice: "no guarantee" unless monotonic, with a BJT saturation example that fools "even an optimizer".
     - Multisim: "a predictor … not … a final say".
     - Micro-Cap: EVA "can be fooled by non-monotonic functions".
   - Micro-Cap alone offers an optimizer-based EVA fallback.
   - **Our predict → simulate → re-linearize loop is strictly better than all of them.** But A3 applies to all of them, and to us.
2. **PSpice AA's cheap sanity check** is worth copying: "a maximum worst-case value that is less than the original value, or a minimum value greater than the original value" signals non-monotonicity [V].
3. **Side-by-side estimates.** Micro-Cap reports RSS, MC (mean ± k·σ) and EVA low/high in one table. Its example reproduces the RAC 1993 guideline: 5.000 V nominal, RSS 4.939/5.061, MCA 4.940/5.061, EVA 4.901/5.100 [V]. That maps directly onto our bracket plus realistic/absolute columns.
4. **Biased vs random bookkeeping per part.** Micro-Cap separates initial (LOT) tolerance from bias tolerances (Neg / Pos / Rnd templates for drift/aging, temperature, voltage stress, radiation) in `.TLIB` libraries [V]. This is the ECSS Table 5-1 structure, and a good model for our part-knob schema.
5. **Stress/derating is always nominal.** PSpice AA Smoke (transient, nominal, default "No Derating", generic defaults such as 0.25 W / 50 V for unannotated parts), Micro-Cap, TINA and SIMetrix SOA [V].
   - ECSS and NASA EEE-INST-002 require the **worst-case combination** to stay under the **derated, temperature-dependent** limit, including composite rules: "applies to the sum of peak AC ripple and DC polarizing voltage"; tantalum ESR ≥ 0.1 Ω/V [V].
   - **Worst-case stress across tolerances is a clear gap we can fill** (note A12: power is not covered by the corner theorem).
6. **Sigma-convention chaos and silent defaults** (A6 table):
   - Altium: 10% on everything and 5 MC runs.
   - PSpice ignores op-amp Vos tolerances unless you own the AA licence: "PSpice quietly ignores this stuff" [S, newsgroup].
   - **Tag every limit as sourced or assumed.**
7. **SIMPLIS DVM** is the closest existing product to our spec table: testplans (line/load regulation, step load, start-up, short circuit, Bode, Zin/Zout, periodic operating point), then an auto-generated report against entered specs, with drill-down to waveforms [V]. It has no tolerance-aware verdicts.
8. **SIMPLIS POP** (periodic operating point) plus AC for switching converters is the practical answer to v2's open question "averaged models vs periodic steady state" [V].

### 3.4 Aerospace worst-case circuit analysis (WCCA) practice

- **ECSS-Q-HB-30-01A** [V] (ECSS has discontinued the handbook and moved its requirements to ECSS-Q-ST-30C).
  - **Four methods:** EVA, "EVA combined" (the random/biased split, **no formula given**), RSS (three-sigma limits), and MC (P ≥ 99.5% at 95% confidence, sample count from a Kolmogorov–Smirnov rule).
  - EVA is "the best initial approach … if the circuit passes an EVA, it always functions properly", but "if circuit fails, there is insufficient data to assess risk".
  - Analysis is at **end of life**; the reference is (22 ± 3) °C at beginning of life.
  - Aging: linear extrapolation (conservative) or Arrhenius.
  - Radiation margins: +20% if the flight part comes from the same manufacturer/process as the tested lot, >100% otherwise.
  - Partition into functional blocks; justify omitted effects by sensitivity analysis.
- **NASA TRMM Earth Sensor WCA, 1994** [V].
  - Hand analysis with RSS'd initial / temperature / age / radiation factors. hFE is modelled as spec × temperature factor × age factor, then radiation applied as a Δ(1/hFE) shift, i.e. drift transforms that are not simply ±%.
  - Explicit open items when data is missing. **This is a real-world UNDECIDED.**
- **Practice is spreadsheets and hand formulas plus targeted SPICE.** Our affine-arithmetic formula path (v2 Part B, row 1) matches this workflow well. Engineers already write these formulas; we would make them correlation-aware and guaranteed.

### 3.5 AI-EDA

- **Flux** (Mar 2026) runs SPICE from a prompt and explains results. Its design-review tab applies deterministic-plus-AI rating rules (resistor power, capacitor voltage margin) [V].
- **Circuit Mind** advertises stress/derating reports (method unknown) [V/U].
- **atopile and JITX** do uncorrelated interval arithmetic. JITX's own divider example is visibly inflated: +0.0466/−0.0678 vs exact +0.0460/−0.0674 [V].
- **Quilter** "Simulation exists to constrain uncertainty, not eliminate it" (blog) but does nothing on tolerances [V].
- **No AI-EDA product does simulation-backed worst case or yield with verdicts.** The white space is real.
- **Implication for the AI:** the most valuable thing it can explain is the *counterexample* and the *assumption set* behind a verdict. It should not generate tolerances silently. Every AI-extracted limit needs provenance (A6, A9).

---

## 4. Methods we may have missed

**Key framing.** Our worst-point loop *is* HL-RF from structural reliability (Hasofer & Lind 1974; Rackwitz & Fiessler 1978 [V]), and the worst-case distance *is* the FORM reliability index β. That literature has 40 years of results on exactly our failure modes: non-convergence, several design points, curvature, high dimension. We should import them rather than rediscover them.

### 4.1 Summary

| Method | Beats our loop when | Cost (simulations) | Fit with adjoint simulator | Adopt |
|---|---|---|---|---|
| **Range-corner enumeration** (shared across specs) + 1-D scans of temperature | Latent regimes (A3), interior temperature worst cases, several range corners | 2^k (k ≈ 5–8) + 3–5 per scanned knob, **shared by all specs of an analysis** | Independent | **NOW** |
| **Regime-margin guard metrics** (auto-generated from device models; "sizing rules" analogue) | Flat-then-cliff specs (A3) | Nearly 0: extra outputs of existing simulations | Native: guards are smooth | **NOW** |
| **Safeguarded local search**: iHL-RF (merit line search) / trust-region SQP, joint over range + statistical | Curved limit states, cycling, joint PVT × statistics | +0–30% vs the bare loop | Native | **NOW** |
| **Multi-start + two-sided search** + failure-direction clustering | Several worst regions (A2), two-sided specs | ×(number of starts), typically 2–6 | Native | **NOW** |
| **Importance sampling at the design point(s)** (min-norm / mixture IS) | Any yield claim; curvature where Φ(β) is off; rare events | 10²–10³ (est.); E7: 2,000 runs ≈ 20,000 MC for ±0.08% at 1%; 5k–9k runs for 1e-5…1e-6 at ±9% (Jonsson & Lelong, arXiv:2109.08393 [V]) | The loop supplies the shift points for free | **NOW** |
| **Board-level yield from stored spec-wise linear models** (MC on linear models; Genz multinormal; Ditlevsen bounds) | Many specs; quantifier order (A8) | **0 extra simulations** for screening | Uses our gradients | **NOW** |
| **Distribution-robust yield** (uniform as worst case among symmetric unimodal; Birnbaum 1948; Barmish & Lagoa 1997) | Distribution unknown (A6), which is almost always | Same as the yield run, with uniform inputs | Independent | **NOW** |
| **Exact Clopper–Pearson + sequential stopping (SPRT)** | Every MC/IS sign-off | Stops early | — | **NOW** |
| **Adjoint-based screening** (keep top contributors, bounded remainder); DGSM later | 100s–1000s of knobs | 1 gradient (DGSM: 20–200 gradient samples) | Excellent | **NOW** (screening); DGSM **LATER** |
| **Design-specific worst-case corners (export)** | Re-verification after small edits; handing corners to other simulators | 1 per spec per re-check | Loop output | **NOW** |
| SORM / quadratic worst case via Hessian-vector products | Strongly curved specs; better Φ(β) correction | 10–30 extra gradient evaluations (est.) | Good (finite differences of adjoint gradients) | LATER |
| Line sampling along the design-point direction | Moderate curvature, high dimension | 3–5 × 50–200 lines (est.) | Uses our direction | LATER |
| Active subspaces | Nonlinear worst case with 1000+ knobs | α·k·log m gradient samples [U formula] | Excellent | LATER |
| GP / Bayesian optimization over the range-knob subspace (Fast PVT-style) | k > 8 range knobs, non-monotone, no useful gradient | About 50–100 per objective (Dolatsara et al., TEMC 2021 [V]: 100 iterations) | Can use gradients | LATER |
| **LFT + μ upper bound for AC** | Guaranteed AC magnitude bounds, resonances | Ferber et al. TEMC 2015 [V]: 26 R/L/C, 17 min vs MC 3 h (1e4 samples) / 28 h (1e5); ">100 uncertain parameters" | Each element stamp maps to one real scalar block | LATER (**high value**: fills the "guaranteed AC" row) |
| p-solution / parametric interval systems (DC) | Rigorous affine outer bound when corners don't apply | About m solves + dense work | Output *is* an affine form | LATER |
| Critical-time / event transient adjoints, STL robustness margins | Overshoot, settling, delay specs | 1 forward + 1 backward per metric | Excellent | LATER (with transient) |
| PSS shooting sensitivities (saltation matrices) | Ripple / regulation in switching converters | 1 PSS + adjoint | Good | LATER |
| Sparse / gradient-enhanced PCE, Kriging, AK-MCS | Distributions, Sobol indices, expensive smooth metrics in ≤ 10–20 dimensions | Hundreds (est.) | Good | LATER |
| Design centering (spec-wise linearization + feasibility-guided search) | The `?` solver with tolerances | 627–689 simulations for two op-amps (Schenkel 2001 [V]) | Native | LATER |
| Subset simulation; scaled-sigma sampling; HSCS / REscope | Pf ≤ 1e-6, many modes, no design point | "A few thousand" | Weak | LATER / rare |
| Statistical blockade / GPD tail fitting | Tail diagnostics | 10–100× vs MC | None | Diagnostic only; **NEVER** as sign-off |
| CMA-ES / GA / DIRECT global search | Only tiny range spaces | 10³–10⁵ (est.) | Ignores gradients | **NEVER** over statistical space |
| Morris screening | Dominated by adjoint gradients | r(k+1) | — | **NEVER** |
| Kharitonov / edge theorem | Wrong uncertainty structure (circuit coefficients are multilinear) | — | — | **NEVER** |
| Intrusive stochastic Galerkin; zonotope / Taylor-model reachability; neural surrogates for sign-off | Research-grade or no guarantee | — | — | **NEVER** (board level) |

### 4.2 Notes on the ones that matter most

**(a) Safeguarded local search: replace the bare jump.**

- The v2 loop is a full-step fixed-point iteration: HL-RF for the statistical case, sequential linear programming (SLP) with full vertex jumps for the box case. Neither converges in general:
  - Liu & Der Kiureghian, Struct. Safety 1991 [V citation; convergence details U] compare HL-RF against safeguarded alternatives;
  - Graeb's own textbook says "there is no investigation into when and how this iteration formula converges" [V, bibliography thread];
  - SLP with full vertex jumps cycles between vertices whenever the true worst point is interior.
- Standard fixes:
  - iHL-RF: a merit-function line search (Zhang & Der Kiureghian 1995 [V]).
  - A trust-region SQP on the joint problem: min ‖u‖ s.t. f(u, θ) = spec, θ ∈ box. Schenkel et al. 2001 [V] use the linearized feasibility region as the trust region.
- A quadratic model over the β-ellipsoid is a trust-region subproblem, globally solvable even when indefinite (Moré & Sorensen 1983 [V]). Over a box it is NP-hard, which is one more reason to keep range knobs few and enumerable.
- **Solving the joint problem fixes the Solido-PVTMC blind spot** (worst range corner depends on the statistical point).

**(b) Importance sampling as the statistical verdict engine.**

- The loop's design point is exactly the IS shift point that min-norm IS (Dolecek et al. ICCAD 2008 [V]) finds by sampling.
- In whitened Gaussian coordinates, the minimum-norm failure point is Graeb's worst-case point. This equivalence is our observation, consistent with the large-deviations framing of the 2008 paper [bibliography thread].
- IS then gives an **unbiased** Pf with a confidence interval, correcting whatever curvature error Φ(β) has.
- Breitung's SORM shows how far off Φ(β) can be in high dimension. With 100 knobs, each with curvature β·κ = ±0.1, Pf moves by 117× or 194× (Breitung 1984 [V]; Valdebenito et al. 2010 [V]).
- For several design points, use a mixture proposal with one component per design point (Kanj et al. DAC 2006 [V]).
- Non-Gaussian knobs (uniform, truncated, binned) need a Nataf/Rosenblatt transform into standard-normal space (Liu & Der Kiureghian 1986 [V]). Alternatively, IS directly in knob space with the true density as p.
- **Weight diagnostics are mandatory**: E7 showed a bad interval at N = 200.

**(c) Board yield at zero extra simulations** (Schenkel 2001 [V]).

- Store each spec's linear model at its worst point.
- Run 10⁴–10⁵ MC samples through all the linear models, taking the worst over the pruned range corners per sample.
- That yields board yield and the quantifier-correct statistic. It was within 1–2% of full MC in their op-amp examples.
- Then confirm with IS. Report the union bound 1 − Σ(1−Yᵢ) ≤ Y ≤ min Yᵢ (Graeb textbook eqs. 312–313, per the bibliography thread) as a sanity bracket.

**(d) Distribution-robust yield: new, cheap, principled.**

- Section 2.6 (A6): uniform inputs give the worst-case two-sided tail among independent symmetric unimodal knobs for linear metrics (Birnbaum 1948; Barmish & Lagoa 1997 [V citations]; checked numerically [E]).
- Kettani & Barmish (TCAS-I 2006 [V abstract]) applied this "distributionally robust Monte Carlo" to resistive networks and found bounds that "differ considerably" from conventional MC.
- Winstead & Barmish, "Distributionally robust Monte Carlo analysis of circuits: the truncation phenomenon" [V citation] is the circuit-specific follow-up. **Read it before implementing**: for non-symmetric acceptance sets the worst case can be a uniform on a *narrower* interval (the "truncation principle") [U detail].

**(e) Regime-margin guard metrics: new, cheap, targeted at the most dangerous failure (A3).**

- Every device model exports the margins to its regime boundaries, as outputs of every simulation.
- The loop runs on the guards first. A guard that can be violated inside the box marks dependent specs "regime change reachable". Those specs get enumeration or multi-start and at most a "PASS (sampled)" verdict.
- IC precedent: sizing rules (Graeb et al. ICCAD 2001; Massier et al. DATE 2008 [V]).

**(f) Guaranteed AC via LFT + μ.**

- v2 Part B's "Linear AC → complex range methods + splitting" row can use μ-analysis, which has been demonstrated on circuits:
  - Ferber et al. IEEE TEMC 57(5):937–946, 2015 [V]: systematic LFT derivation for uncertain R/L/C networks, D-G scaling upper bound, 26 parameters, guaranteed upper and lower |H| bounds;
  - Ferber et al. IJNM 2018 [V]: bounds over frequency intervals, not just grid points.
- MNA stamps are rank one, so each element is one real scalar block. Shared knobs become repeated blocks, which is looser and more expensive.
- Real μ is NP-hard, so the upper/lower gap must be reported. **This is exactly our inner/outer bracket.**
- Operating-point dependence of gm etc. must be fed in from the DC affine forms.

**(g) What we should not do.**

- Global optimizers over the statistical space.
- Neural surrogates as evidence.
- GPD tail extrapolation as sign-off.
- Kharitonov-type results.
- Taylor-model reachability for board transient.

All are either unscalable or give no guarantee our verdict vocabulary can represent.

---

## 5. Bibliography spot-check

Checked against primary sources where reachable (author PDF of N&P07, the ECSS PDF, the PSpice Reference Guide, the Vishay datasheet, the Maxim note via Wayback, Wilks via Project Euclid, and Graeb's TUM lecture textbook, which states it "follows" Graeb07). Abstracts only for AGW94, TS00, Kolev 2002, Dolecek 2008 and PPC65.

| # | Claim in our docs (where) | Verdict | What the source says / corrected wording |
|---|---|---|---|
| 1 | **[N&P07] Thm 5.1: extremes at corners** (v2 A5; bib §3) | **CONFIRMED, conditions missing** | "If the linear system (K + B Diag(x) A) u = Fb, x ∈ **x**, b ∈ **b** is uniquely solvable for all x ∈ **x**, then the extremal values of any component u_k … are attained at vertices." Required: each parameter in **one rank-one term**; independent box; **nonsingular over the whole box**; per component (min/max may be at different vertices). Extension (ours, sound): linear functionals and ratios of components. **Not** power. |
| 2 | "The same holds for **any element value** (R, g, a source value, a controlled-source gain)" (v2 A5; WT §7.1) | **OVERSTATED** | True for R (via g = 1/R), g, VCCS/VCVS/CCCS/CCVS gains (rank-one), and RHS sources. **False** for transformer turns ratio, gyrators, pot wipers, and any shared knob. Counterexample: V_L = Vs·n·R_L/(n²R_L + R_s) peaks at n = √(R_s/R_L). |
| 3 | N&P07 Ex. (39) "interior max when a parameter appears twice" | CONFIRMED, wording | Precisely: a parameter multiplying a **rank-2** matrix (x₁ multiplies the identity). x₂ appears in 4 entries but as rank 1, which is fine. |
| 4 | N&P07 Ex. (41) "nominal gradient points to wrong corner" (v2 A5; WT §5.7, §10) | CONFIRMED, **large-tolerance only** | Wrong vertex only for δ > 0.5, i.e. beyond ±25%. Use [TS00] for moderate tolerances, and our own walkthrough §5.7. |
| 5 | "Truss, 101 parameters: 0.6% at 1%, 2.9% at 5%, **20–25× one solve**" (bib §3) | **PARTLY WRONG** | 0.6% / 2.9% (u_x; 0.7% / 3.3% u_y) are Ex. 7.1 (101 parameters), measured against an inner enclosure. "20–25×" is **Ex. 7.2 (420 parameters)**, growing to ~1,100–1,300× at 10,050 parameters (Ex. 7.3). Rounding was ignored (about 5× slower if rigorous). The paper also warns interval monotonicity proofs "typically fail in high dimensions", which contradicts our "usually almost all [directions] are decided, ~2 solves". |
| 6 | "k% means total width k%" | CONFIRMED | "[s − sk/200, s + sk/200] … width is k% of the nominal value." |
| 7 | [PPC65] "any network function is bilinear in any single element value" | CONFIRMED (abstract) | Wording: "bilinear in any element **immittance** (bilateral or unilateral)". Cite Middlebrook's N-EET (TCAS-I 45(9), 1998) for dependent sources. |
| 8 | Möbius V = (a+bg)/(c+dg) "has no hump" (v2 A5) | CONFIRMED with caveat | Monotone **while c + d·g ≠ 0 over the range**, i.e. the circuit stays solvable. This is Thm 5.1's hypothesis. |
| 9 | [Graeb07] design / range / statistical; classical / realistic / general WCA | **CONFIRMED** | Verbatim definitions in Graeb's TUM textbook. Range parameters: "only a range … is given … supply voltage and temperature … lifetime". Rectangles for range and ellipsoids for statistical. Acceptance "for all range parameters". Drop "(secondary sources)" from the bibliography. |
| 10 | [AGW94] "yield ≈ Φ(β) per spec" | CONFIRMED (via Graeb textbook eq. 296) | Φ(β_W) is the per-spec partition for a linearized spec with normal parameters. Overall: 1 − Σ(1−Yᵢ) ≤ Y ≤ min Yᵢ (eqs. 312–313). The AGW94 abstract also says cost grows **linearly in the number of design variables** (finite-difference sensitivities). |
| 11 | Rule of three; "0 fails in 300 → < 1% at 95%" (v2 A8) | CONFIRMED, **one-sided** | 1 − 0.05^(1/300) = 0.994%. Two-sided 95% Clopper–Pearson gives 1.22% (368 runs needed for < 1%). Valid only for i.i.d. samples from the assumed production distribution, not shifted "near the worst point" samples. |
| 12 | Wilks: "59 runs give a one-sided 95/95 limit" | CONFIRMED as consequence | 59 is not printed in the paper; it follows from eq. 6 (1 − 0.95^N ≥ 0.95). Two-sided min/max: 93. It says **nothing about the true worst case**. |
| 13 | Campi & Garatti 2008 as "what runs prove" | Citation CONFIRMED; **caveat missing** | For **convex** scenario programs. Checking a fixed design reduces it to (1−ε)^N, the rule of three. Non-convex: Campi, Garatti & Ramponi, TAC 2018. |
| 14 | [ECSS11] "biased terms add linearly, random by RSS; RSS means 3σ; MC ≥ 99.5% at 95%" (bib §12) | **MIXED** | Random / biased split, 3σ and MC "P = 99,5% … with a confidence level of 95%" (Kolmogorov–Smirnov sample rule) are CONFIRMED. **"Biased linear + random RSS" is NOT stated.** The "EVA combined approach" is named, "strictly valid only for Gaussian variables", with no formula. That rule is RAC CRTA-WCCA practice. There are **four** methods (EVA, EVA combined, RSS, MC); EVA is "the best initial approach"; analysis is **at end of life**. Aging and temperature are "biased (sometimes random)". |
| 15 | "This is the standard split in analog design [Graeb07] and space WCA [ECSS11]" (v2 A7; WT §11) | **OVERSTATED** | ECSS splits by *known direction* (biased vs random). Graeb splits by *must hold everywhere* vs *has a distribution*. Temperature agrees. **Supply/interface variation is "generally random" in ECSS** but a range knob in ours. ECSS's default method is EVA (everything at extremes). |
| 16 | PSpice GAUSS: tolerance = 1σ; default UNIFORM (v2 A7; bib §12) | CONFIRMED + detail | "GAUSS: … over the range ±3σ and <value> specifies the ±1σ deviation." Default UNIFORM. So a GAUSS 1% part reaches ±3%, and `.WCASE` pushes Gaussian parameters by 3σ. Newer versions may truncate at ±4σ [U]. |
| 17 | [Vishay CRCW] "±1% part may drift up to 2% after 8000 h, so closer to ±3% at end of life" (v2 A7) | Numbers CONFIRMED; **"end of life" OVERSTATED** | ±(1% R + 0.05 Ω) after 1000 h and ±(2% R + 0.1 Ω) after 8000 h, rated power at 70 °C, stability class 1. The 8000 h test is ~11 months at full power; the datasheet says there is no limited lifetime and gives **no 10-year figure**. "±3–3.6%" is our stack-up (with TCR), not a datasheet number. |
| 18 | [Maxim 5527] "4.7 µF becomes 0.33 µF"; "DC-bias loss usually dominates" | **OVERSTATED** | The 0.33 µF case is a **6.3 V Y5V 0603 at 5 V** (typical −92.9%), which the author "never use[s]". An X7R 0805 at 12 V gives 1.53 µF. "Usually dominates" is our reading. Author Mark Fortunato, Dec 2012. |
| 19 | Dolecek 2008: "IS center = worst-case point; one search serves both" | CONFIRMED in substance | The paper's shift is the minimum-norm failure point (found by sampling). The equivalence to WCD is our observation, valid per failure region. |
| 20 | [TS00] content | CONFIRMED | Title "Worst case tolerance analysis of linear analog circuits using sensitivity bands". Note Kolev's critical comment, TCAS-I 48(10):1265–1267, 2001. |
| 21 | [DR69], Kolev 2002, [Xyce16] adjoint claims, Hocevar 1985, Rohn–Kreinovich 1995, Mukherjee et al. 2000, N-EET 1998 | CONFIRMED | Xyce: "the adjoint method requires a matrix solve for every objective function"; "each local sensitivity … requires a separate reverse integration"; storage for long transients. Hocevar also appeared in IEEE T-ED 32(10). |
| 22 | [FS00] "Femia & Spagnuolo" | **Author order uncertain** | Crossref and OpenAlex list **Spagnuolo, Femia**; Semantic Scholar lists Femia, Spagnuolo. Check the PDF before citing. |
| 23 | "The loop needs roughly 5–8 simulations per spec **regardless** [of knob count]" (WT §9) | **OVERSTATED / unsupported** | Only true with adjoint sensitivities, and only for DC/AC. Transient needs one backward integration per metric. With finite differences (as the walkthrough appendix actually used) each round costs N+1 simulations. No published evidence for "2–4 rounds"; no convergence guarantee (Graeb textbook). Cadence VVO: "under 100 simulations for each spec". |
| 24 | "3σ (99.9% of boards)" (SD §7, §10) | **WRONG arithmetic** (my check) | One-sided 3σ = 99.865%; two-sided 3σ = 99.73%; 99.9% one-sided = 3.09σ. |
| 25 | "Newton … results are only good to about 0.1%" (v2 Part D.3) | **OVERSTATED for DC** (my check [E]) | Step-size acceptance with quadratic convergence leaves ≈2e-6 relative error at rel_tol 1e-3. The real issues are `abs_tol = 1e-6` used for currents, and transient truncation error. |

**Corrections to make in the docs** (short list; full wording is in the bibliography thread's notes, reproduced as rows 1–25 above):

- v2 A5: add the Thm 5.1 conditions, the rank-one qualifier and the singularity caveat. Say explicitly that power is not covered. Soften "~2 solves".
- Bibliography: move the 20–25× figure to Ex. 7.2, add the scaling numbers, and fix the ECSS row.
- v2 A7 and WT §11: "similar to, not the same as" ECSS.
- v2 A7: Vishay and Maxim phrasing.
- WT §9: the cost statement.
- SD §7: the sigma arithmetic.
- v2 Part D.3: the accuracy statement.
- Rule-of-three wording: one-sided, i.i.d. from the production distribution.

---

## 6. Results presentation

**Evidence base.** Vendor documentation of what tools display [V]. Only thin user evidence could be reached: the web-search budget ran out, and EDAboard, Reddit and All About Circuits blocked fetches. **I found no reachable paper on analog designers' workflow with variation tools**; treat this section's "users value" claims as inference unless tagged.

### 6.1 What existing tools show

- **Cadence ADE / VVO** [V]:
  - colour-coded pass/fail tables per spec and corner;
  - histograms with pass/fail regions; quantile plots with a normality test;
  - value / yield / mean / σ against iteration (convergence);
  - a correlation table;
  - a **sortable % contribution table per spec with R²**, hierarchical drill-down and schematic cross-probe;
  - "probability each spec already meets target".
- **Spectre FMC** [V]: each worst sample's σ, MC iteration number and value, i.e. a reproducible handle on the counterexample.
- **PSpice AA** [V]:
  - Sensitivity: a parameter table with @Min/@Max, absolute/relative sensitivity and linear/log bars, plus a spec table with worst-case min/max and "send to Optimizer".
  - MC: PDF/CDF with **draggable spec cursors that update yield live**, 10/50/90 percentiles, a raw table, and a start-run number to reproduce a run.
  - Smoke: bars as % of derated limit, red / yellow (90–100%) / green, and "Find in Design".
- **Micro-Cap** [V]: one table with RSS / MC / EVA low/high, in value, change and %, with Excel export.
- **SIMPLIS DVM** [V]: an auto-generated report against specs, with drill-down to waveforms.
- **TINA** [V]: hover over an MC curve to see the component values that produced it.
- **MunEDA** [S]: worst-case distance per spec and a binary PASS/FAIL. A customer quote values "binary PASS/FAIL".

### 6.2 User evidence (thin)

- **Interpreting MC statistics is hard enough to need an app note.** FlowCAD's PSpice note exists to answer "But how can we understand the statistical data?". The histogram statistics and the `.OUT` summary use different samples and quantities [S].
- **Silent behaviour destroys trust.** "PSpice quietly ignores this stuff unless you buy a license"; worst case "does not like math expressions … ABS(V(…)). It errors on that" [S, sci.electronics.design].
- **WCA's audience is reviewers.** "The task at hand is to provide proof that it won't be"; "they should see everything that is in the sims so they can talk it over" [S, same thread].
- **Speed of a single run is valued.** Designers value dcmatch because it gives σ "in a single simulation", and accept that it is "close but not quite identical" to MC [S, EDAboard excerpt].
- **Vendors warn against over-trusting worst case.** Multisim: use it "as a predictor … not as a final say"; HSPICE: all-at-3σ is "overly pessimistic" [V].

### 6.3 Recommendations for our result views

1. **Lead with margin, not only a verdict.**
   - Margin in spec units, and in σ-units (worst-case distance β_w, or Cadence's "sigma-to-target").
   - PASS/FAIL hides how close a spec is. β_w comes free from the loop.
2. **A verdict always carries its basis**, as compact badges:
   - method (exact / guaranteed / loop-N-sims / IS-N / MC-N);
   - distribution assumption (default vs robust);
   - model coverage (complete / model-conditional with k gaps);
   - numerics (error band vs margin).

   This answers the "show me everything so we can talk it over" reviewer need, and avoids PSpice-style silent behaviour.
3. **The counterexample is a first-class, reproducible object.** It has a knob vector, a seed/ID, its probability (σ-distance), and a one-click "simulate this board". This is what the AI should explain.
4. **Contributor Pareto** split by knob kind (range / statistical / model-exposure). Show a **linearity indicator** next to it: the loop's prediction mismatch or R². Group by block and part family when N is large.
5. **Side-by-side columns: nominal | absolute worst | realistic | robust-distribution.** This is Micro-Cap's RSS/MC/EVA idea mapped onto our bracket. The walkthrough §9 table already does most of this; add the robust column.
6. **Stress views at worst case, not nominal:** % of derated limit, with the 90% yellow band, avg/RMS/peak selection, schematic cross-probe and per-instance waiver with reason. PSpice AA Smoke's visual language is well known and worth reusing.
7. **Distributions on demand only**, always with spec lines and live yield (PSpice AA cursors). Never a bare histogram. Show per-lot yield when lot knobs exist.
8. **Transparent stopping.** For every run show "why it stopped" (converged / budget / CI decided) and the brute-force reference count. This is the opposite of Spectre FMC's trade-secret stopping rules, and a differentiator.
9. **UNDECIDED names its reason and the cheapest next action**, e.g.:
   - "distribution-dependent: provide lot data or accept robust bound";
   - "model-conditional: U3 Zout not modelled";
   - "numerics: margin 0.02% < noise 0.05%, tighten tolerances (auto)";
   - "needs 1,800 more IS runs".

---

## 7. Recommended design changes, ranked by impact

Impact = (how likely the problem is on real boards) × (how bad a wrong answer is) ÷ (cost to fix). "P0" means fix before the loop ships in any form.

| Rank | Change | Fixes | Cost | Priority |
|---|---|---|---|---|
| 1 | **Non-local safety net around the loop:** enumerate range corners by default (2^k ≤ 256, shared across specs); 1-D scans of temperature; multi-start; two-sided search for two-sided specs; demote "prediction ≈ simulation" to a necessary, not sufficient, check | A2, A3, A7 (confident false PASS) | Low: simulations shared across specs | **P0** |
| 2 | **Regime-margin guard metrics** exported by every device model (headroom, dropout, VCE,sat, current limit, comparator overdrive, UVLO), plus region signatures at every simulated point | A1, A3, A11 | Low–medium (device-model work) | **P0** |
| 3 | **Verdict semantics per confidence class** (worst_case vs yield vs nominal); yield FAIL/PASS by confidence bounds; fix "3σ (99.9%)"; counterexamples carry probability and conditions | A17, A18 | Low (spec/UI) | **P0** |
| 4 | **Distribution and data provenance on every knob** (guaranteed / characterization / typical-only / assumed; σ-multiplier; truncation; lot structure with abs(lot + local) ≤ tol); report yield under default **and** robust (uniform) distributions; UNDECIDED if they disagree | A5, A6 | Low–medium | **P0** |
| 5 | **Model coverage map + adjoint exposure report + standard knob wrappers**; "model-conditional" tag | A9 | Medium (ties into AI part modelling) | **P0** for the tag; P1 for the wrappers |
| 6 | **Strict corner-theorem applicability checker**; move power/derating specs off it; correct the v2 A5 text | A12 | Low | **P0** (correctness of the "exact" label) |
| 7 | **Statistical engine = IS at design point(s) + board-level yield** (per-sample worst over range corners, over all specs; zero-simulation screening on spec-wise linear models first); Clopper–Pearson and sequential stopping; replaces plain MC at sign-off | A8, A20 | Medium | P1 |
| 8 | **Safeguarded joint search** (trust-region SQP / iHL-RF over range + statistical knobs) in place of the full-step jump | A2, A8 (PVT × statistics), convergence | Medium | P1 |
| 9 | **Worst-case stress/derating** as automatic specs (derated, temperature-dependent limits; composite rules such as peak AC + DC; worst over tolerances): the clearest market gap | Market gap; SD §8 | Medium (uses 1, 2, 7) | P1 |
| 10 | **Simulator prerequisites, reprioritized:** separate V/I absolute tolerances; engine tolerance preset; numerical-error band in brackets; autodiff device derivatives *including second derivatives* (needed for AC of nonlinear circuits); FD gradient checks in CI; transient truncation-error control before any transient verdicts | A10, A11 | Medium–high | P1 |
| 11 | **Large-N handling:** adjoint screening with a bounded (never dropped) remainder; grouped contributor reports; restrict absolute worst case to low-N specs | A4 | Low | P1 |
| 12 | **Stability and start-up automatic specs** (pole check at every simulated point; start-up transient) | A14 | Low–medium | P1 |
| 13 | **Composition hygiene:** every affine form stores linearization point, validity radius and err provenance; contracts carry impedance vs frequency and load/step profiles; system specs re-run the loop with block forms as warm start | A13, A19 | Medium | P2 |
| 14 | **Temperature and aging from stress:** per-part ΔT = θ·P; aging magnitude from per-part stress (Arrhenius, voltage, ripple); unknown drift sign as a knob | A15, A16 | Medium | P2 |
| 15 | **Later methods:** LFT + μ for guaranteed AC; p-solution for rigorous DC affine enclosures; Fast-PVT-style GP when range knobs > 8; critical-time / event transient adjoints; PSS sensitivities for converters; active subspaces | Coverage | High | LATER |

### Concrete edits to the design documents

- **v2 A8 loop box.** Insert step 0, "enumerate range corners and evaluate guard metrics". Change step 4 to "trust-region step", and add "multi-start + two-sided". Replace "Monte Carlo … near the worst point" with "importance sampling centred at the design point(s), weighted".
- **v2 Part B table.**
  - Nonlinear row: verdict "PASS (estimated)" only if no regime change is reachable and the range corners are enumerated; otherwise "PASS (sampled)".
  - Linear AC row: add "LFT + μ (later)".
  - Linear DC row: add the applicability conditions.
- **v2 B knob properties.** Add provenance, σ-multiplier, truncation, lot structure with constraint, and an "unknown sign" flag.
- **Walkthrough §9.** Fix the cost claim. Add a "robust distribution" column. Change S3's verdict wording to "FAIL (≥ 0.9% under default distribution; up to 11.6% under any symmetric unimodal distribution within tolerance)".
- **Spec design §7.** Fix the sigma arithmetic. §9: per-class verdict table (2.16). §8: derating specs are evaluated at worst case (not by corners).
- **Bibliography.** Apply section 5's corrections and add Appendix B's references.

---

## Appendix A: experiments

Everything is pure Python 3 (no numpy available). Each experiment is small enough to re-type from this description. Only this report was written into the repo; the scratch scripts lived in `/tmp/redteam/`.

### A.1 Walkthrough amplifier model (reproduces the walkthrough exactly)

```python
K_Q = 8.617333e-5
def model(e):                       # every knob e[k] in [-1, 1]
    T = 25 + 35*e['T']; VCC = 12*(1+0.05*e['VCC']); life = (e['life']+1)/2
    R1 = 47e3*(1+0.01*e['R1']); R2 = 10e3*(1+0.01*e['R2'])
    RC = 4.7e3*(1+0.01*e['RC']); RE = 1e3*(1+0.01*e['RE'])
    C = 1e-6*(1+0.2*e['C'])*(1-0.2*life)
    beta = (200+100*e['beta'])*(1+0.005*(T-25)); VT = K_Q*(T+273.15)
    VTH = VCC*R2/(R1+R2); RTH = R1*R2/(R1+R2); IC = 1e-3
    for _ in range(200):            # damped fixed point on IC
        VBE = 0.65 + VT*math.log(IC/1e-3) - 0.002*(T-25)
        IC = 0.5*IC + 0.5*(VTH-VBE)/(RTH/beta + (beta+1)/beta*RE)
    rpi = beta*VT/IC
    Rin = 1/(1/R1 + 1/R2 + 1/(rpi+(beta+1)*RE))
    return dict(VC=VCC-IC*RC, gain=beta*RC/(rpi+(beta+1)*RE), fL=1/(2*math.pi*C*Rin))
```

- **Checks against the walkthrough.** Nominal (life = new): VC 5.49991, gain 4.59174, f_L 20.079 Hz. Brute force over 512 corners: VC 4.6597…6.5905, gain 4.4626…4.7030, f_L 16.29…33.42.
- **E1 (distributions).** S3 fails when f_L > 30 Hz, at corner T = −1, life = 1, VCC = 0; 20,000 samples of the six statistical knobs.
  - truncated normal σ = 1/3: 0.89%;
  - σ = 1/2: 4.07%;
  - uniform: 11.57%;
  - "culled" (uniform |ε| ∈ [0.6, 1], random sign): 31.8%;
  - C_in reel offset N(−0.6, 0.1): 5.0%.
  - Per range corner (truncated normal, 5,000 samples each): cold, aged ≈ 1.0%; hot, aged ≈ 0.5%; new ≈ 0%.
  - S1 (VC > 6.5 at T = −1, VCC = +1): 0 / 4 / 113 failures per 20,000 for truncated normal / uniform / culled.
- **E3 (FORM vs MC).** HL-RF in σ units on S3 converges to u* ≈ (C −2.32σ, β −0.25σ, R2 −0.08σ, …).
  - β = 2.334, Φ(−β) = 0.98%.
  - MC (40,000): untruncated normal 1.09%, truncated at ±3σ 0.87%.
- **E4 (quantifier order).** Window 4.95 ≤ VC ≤ 6.05; corners (T, VCC) ∈ {±1}², life new; 5,000 truncated-normal samples.
  - Per-corner loss 0% / 26.7% / 10.1% / 0%.
  - Any-corner loss 36.8%.
- **E7 (importance sampling).** Proposal N(u*, I) in σ units; target: truncated standard normal (|u| ≤ 3).
  - N = 200: 0.60 ± 0.20%;
  - N = 500: 0.80 ± 0.14%;
  - N = 2,000: 0.945 ± 0.078% (95% CI half-widths).
  - Plain MC half-width at the same p: ±1.30% / ±0.82% / ±0.41% / ±0.13% at N = 200 / 500 / 2,000 / 20,000.

### A.2 E2: clipping THD (latent regime)

- **Model.** Op-amp non-inverting stage.
  - G = 1 + R2/R1 (1k, 10k, ±1%); input amplitude A = 0.15 + 0.05·ε_A V.
  - VCC = 5(1 + 0.05·ε_V); T = 22.5 + 62.5·ε_T °C.
  - Output limit ±(VCC/2 − h), h = 0.25 − 0.001·(T − 25).
  - Smooth distortion 0.002%·(A/0.2)²·(1 + 0.3·(T − 25)/60).
- **THD** from a 2,048-point DFT of the clipped sine (harmonics 3, 5, 7, 9) combined in quadrature with the smooth term.
- **Loop.** Central-difference gradient, jump to the sign corner (zero-gradient knobs stay put), re-linearize, stop when the corner repeats.
  - Result: (A = +1, VCC = 0, T = +1) → 0.0026%, converged in 2 rounds with prediction ≈ simulation.
- **Brute force (32 corners):** 3.289% at (A +1, VCC −1, T −1, R1 −1, R2 +1); 4 corners > 1%.
- **Range corners only (8):** 2.513% at (A +1, VCC −1, T −1).

### A.3 E8: LDO phase margin vs ESR (loop not fooled)

- **Model.** L(s) = A₀(1 + s·ESR·C) / ((1 + s·R_L·C)(1 + s/ω_a)(1 + s/ω_g)), with A₀ = 2000, f_a = 3 kHz, f_g = 400 kHz. PM uses the unwrapped phase at the 0 dB crossing.
- **The window:** PM < 0 below about 0.15 Ω; peak ≈ 48° near 1.5–2 Ω; about 26° at 10 Ω.
- **Knobs:**
  - T −40…85 °C, with ESR ∝ 3^(−(T−25)/65);
  - ESR₂₅ = 1 Ω ± 50%;
  - C 2.2 µF ± 20%;
  - load 10–300 mA (log).
- **Result.** The loop reached the true worst corner (hot, low ESR, low C, heavy load; −2.0°) in 3 rounds. Random interior sampling found nothing worse (min 0.57°).

### A.4 E5: peakedness check (uniform is the worst symmetric unimodal)

- 12 random coefficient sets (n ∈ {2, 3, 5, 8, 20}), 60,000 samples per distribution.
- Thresholds at 30 / 50 / 70 / 85% of Σ|cᵢ|.
- In 48 of 48 cases, uniform gave P(|S| > t) ≥ triangular, truncated normal (σ = 1/3) and truncated Laplace, within MC error.
- Truncated Laplace exceeded truncated normal in the far tail. For example, with n = 3 at 70%: uniform 3.9%, triangular 0.26%, truncated Laplace 0.08%, truncated normal 0.03%.

### A.5 E6: Newton accuracy

- **Setup.** CE amplifier (IS = 1e-14, BF = 200, R1 = 47k, R2 = 10k, RC = 4.7k, RE = 1k, VCC = 12 V). Newton with a finite-difference Jacobian, ±0.1 V step limiting, and the simulator's acceptance rule (|x_new − x_old| ≤ abs + rel·max, accept x_new).
- **rel_tol 1e-3:** 17 iterations; VC error 1.2e-5 V; the change for R1 + 0.1% is accurate to 1.5e-5 relative.
- **rel_tol 1e-2:** VC error 1.7e-3 V.
- **abs_tol 1e-3 vs 1e-6:** no difference here. This circuit has no µA-level currents, so it does not exercise the `abs_tol`-on-currents issue.

---

## Appendix B: references added by this report (citation checked via Crossref / OpenAlex unless noted)

**Reliability and search**
- Hasofer & Lind, J. Eng. Mech. Div. ASCE 100:111–121, 1974, doi:10.1061/JMCEA3.0001848
- Rackwitz & Fiessler, Computers & Structures 9:489–494, 1978, doi:10.1016/0045-7949(78)90046-9
- Breitung, J. Eng. Mech. 110(3):357–366, 1984, doi:10.1061/(ASCE)0733-9399(1984)110:3(357)
- Zhang & Der Kiureghian, "Two improved algorithms for reliability analysis", IFIP 1995, doi:10.1007/978-0-387-34866-7_32
- Der Kiureghian & Dakessian, "Multiple design points in first and second-order reliability", Struct. Safety 20:37–49, 1998, doi:10.1016/S0167-4730(97)00026-X
- Valdebenito, Pradlwarter & Schuëller, Struct. Safety 32(2):101–111, 2010 (methods thread)
- Moré & Sorensen, "Computing a trust region step", SIAM J. Sci. Stat. Comput. 4:553–572, 1983 (methods thread)

**Sampling and yield**
- Dolecek, Qazi, Shah & Chandrakasan, ICCAD 2008, pp. 322–329, doi:10.1109/ICCAD.2008.4681593
- Kanj, Joshi & Nassif, DAC 2006, pp. 69–72, doi:10.1145/1146909.1146930
- Au & Beck, PEM 16:263–277, 2001, doi:10.1016/S0266-8920(01)00019-4
- Sun, Li, Liu, Luo & Gu, scaled-sigma sampling, ICCAD 2013, pp. 478–485, doi:10.1109/ICCAD.2013.6691160
- Singhee & Rutenbar, statistical blockade, DATE 2007 / TCAD 28(8), 2009
- Wu, Bodapati & He, HSCS, ISPD 2016, doi:10.1145/2872334.2872360 (methods thread)
- Jonsson & Lelong, "Rare event simulation for electronic circuit design", arXiv:2109.08393 (methods thread)
- Schenkel, Pronath, Zizala, Schwencker, Graeb & Antreich, "Mismatch analysis and direct yield optimization by spec-wise linearization and feasibility-guided search", DAC 2001, pp. 858–863, doi:10.1109/DAC.2001.935625
- Genz, JCGS 1:141–149, 1992 (methods thread)

**Scenario approach**
- Calafiore & Campi, IEEE TAC 51(5):742–753, 2006, doi:10.1109/TAC.2006.875041
- Campi & Garatti, "The exact feasibility of randomized solutions of uncertain convex programs", SIAM J. Optim. 19:1211–1230, 2008, doi:10.1137/07069821X
- Campi, Garatti & Ramponi, IEEE TAC 63:4067–4078, 2018, doi:10.1109/TAC.2018.2808446
- Tempo, Dabbene & Calafiore, *Randomized Algorithms for Analysis and Control of Uncertain Systems*, Springer 2005 (2nd ed. 2013), doi:10.1007/b137802

**Distribution-robust bounds**
- Birnbaum, "On random variables with comparable peakedness", Ann. Math. Stat. 19(1):76–81, 1948, doi:10.1214/aoms/1177730293 (citation [V]; theorem text not accessed)
- Proschan, "Peakedness of distributions of convex combinations", Ann. Math. Stat. 36:1703–1706, 1965, doi:10.1214/aoms/1177699798
- Barmish & Lagoa, "The uniform distribution: a rigorous justification for its use in robustness analysis", MCSS 10:203–222, 1997, doi:10.1007/BF01211503
- Kettani & Barmish, "A new Monte Carlo circuit simulation paradigm with specific results for resistive networks", IEEE TCAS-I 53:1289–1299, 2006, doi:10.1109/TCSI.2006.875183
- Winstead & Barmish, "Distributionally robust Monte Carlo analysis of circuits: the truncation phenomenon", LNCIS, doi:10.1007/BFb0110631
- Lagoa & Barmish, "Distributionally robust Monte Carlo simulation: a tutorial survey", IFAC 2002, doi:10.3182/20020721-6-ES-1901.00360
- Hoeffding, "Probability inequalities for sums of bounded random variables", JASA 58(301):13–30, 1963, doi:10.1080/01621459.1963.10500830

**Regime constraints**
- Graeb, Zizala, Eckmüller & Antreich, "The sizing rules method for analog integrated circuit design", ICCAD 2001, pp. 343–349, doi:10.1109/ICCAD.2001.968645
- Massier, Graeb & Schlichtmann, "Sizing rules for bipolar analog circuit design", DATE 2008, pp. 140–145, doi:10.1109/DATE.2008.4484676

**Screening and dimension reduction**
- Sobol' & Kucherenko, Math. Comput. Simul. 79:3009–3017, 2009, doi:10.1016/j.matcom.2009.01.023
- Constantine, *Active Subspaces*, SIAM 2015, doi:10.1137/1.9781611973860
- Li, "Finding deterministic solution from underdetermined equation", DAC 2009, pp. 364–369, doi:10.1145/1629911.1630009

**Surrogates and design of experiments**
- Jones, Schonlau & Welch, "Efficient global optimization of expensive black-box functions", J. Global Optim. 13:455–492, 1998, doi:10.1023/A:1008306431147
- Jones & Nachtsheim, "A class of three-level designs for definitive screening in the presence of second-order effects", J. Quality Tech. 43:1–15, 2011, doi:10.1080/00224065.2011.11917841 (curvature-detecting screening when gradients are unavailable)
- Keiter, Swiler & Wilcox, "Gradient-enhanced polynomial chaos methods for circuit simulation", Springer Math. in Industry, 2018, pp. 55–68, doi:10.1007/978-3-319-75538-0_6

**Robust AC bounds**
- Packard & Doyle, "The complex structured singular value", Automatica 29:71–109, 1993, doi:10.1016/0005-1098(93)90175-S
- Ferber et al., "Systematic LFT derivation of uncertain electrical circuits for the worst-case tolerance analysis", IEEE TEMC 57(5):937–946, 2015, doi:10.1109/TEMC.2015.2419455 (methods thread; text read)

**Industry**
- McConaghy, Breen, Dyck & Gupta, *Variation-Aware Design of Custom Integrated Circuits: A Hands-on Field Guide*, Springer 2013, doi:10.1007/978-1-4614-2269-3
- Solido patents: US8612908B2 (Fast PVT), US20120259446A1 (sigma-driven corners), US9483602B2 (HSMC). Cadence patent US10262092B1 (mismatch contribution). ProPlus US10339240B2. All read on Google Patents (IC-tools thread).

**Numerics and contracts**
- Moré & Wild, "Estimating computational noise", SIAM J. Sci. Comput. 33(3):1292–1314, 2011, doi:10.1137/100786125 (evidence thread)
- Benveniste et al., "Contracts for System Design", FnT EDA 12(2–3):124–400, 2018, doi:10.1561/1000000053 (evidence thread)
- Sun, Nuzzo, Wu & Sangiovanni-Vincentelli, "Contract-based system-level composition of analog circuits", DAC 2009, pp. 605–610, doi:10.1145/1629911.1630066 (evidence thread)

**Standards and application notes** (primary sources read by the threads)
- ECSS-Q-HB-30-01A (ecss.nl PDF)
- NASA EEE-INST-002
- NASA TRMM ESA WCA (NTRS 19940020281)
- TI OPA991, REF35, REF70, RES11A datasheets
- TI SLVA079, SLVA057, SLVA115, SNVA538, SNVA602, SBOA338
- Infineon MOSFET simulation-model AN 2014-02
- Nexperia AN90034
- onsemi TND6421/6422
- CDE aluminum electrolytic application guide
- Vishay tech note 28809 (drift calculation)
