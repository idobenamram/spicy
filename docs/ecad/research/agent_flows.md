# The Editor's AI Agent: Each User Flow Through the Engine

> Research note · 2026-09-27 · For the M3 engine MVP plan (roadmap §6).
> Builds on `../engine.md` (v3), `../walkthrough.md`, `../language.md` §5–§10, `../roadmap.md` §4.2 and §6, and `language_editor_mapping.md` §8.
> **Where the numbers come from:** every circuit number is computed by the scripts in the scratch folder `/root/.claude/jobs/443154a8/tmp/engine_research/agent/` (listed in the Appendix). They extend the walkthrough's reference model `ce_amp_model.py`. That model reproduces every walkthrough number, and my extension matches it exactly at all 256 corners (`check_model.py`). Outside sources are cited inline with URLs.

---

## Summary

The agent never decides a verdict; it asks for one. Its job is to turn a question into engine and language calls, and turn the answers back into plain words and **proposed** edits.

- **The engine** decides every verdict.
- **The language service** owns every change to the text, and a change lands only when the user accepts a diff.
- **Every engine answer carries a status:**
  - `verified`: the loop ran on this exact design revision;
  - `inner_bound`: a real simulated value at a real point, which can prove a FAIL but never a PASS;
  - `estimated`: from stored slopes, which proves nothing.

The biggest finding is about the "instant what-if" (engine.md §8), tested on the walkthrough amplifier:

| What-if method | Result on the tested edits |
|---|---|
| Re-evaluate the stored affine form in its native units | Nonsense for real edits: changing C_in from 1 µF to 2.2 µF predicts a worst-case f_L of **−6.68 Hz** |
| The same slopes in log coordinates | Exact for that edit (**15.19 Hz**), because f_L ∝ 1/C. But it misses interactions: raising the divider resistance predicts a 3σ bias of **6.51 V** (borderline) where the truth is **6.70 V** (a clear FAIL) |
| Re-simulate the previous worst points on the edited design | Matched the loop's answer in every worst-case row of the fix-bass test, at one run per point. Where a worst corner moved (one row of the gain-10 test), it still found the failure, though not the worst value. Each value is a real, reachable point, so this turns an instant what-if into a **proven** FAIL when it fails |

So `what_if` should return the log estimate plus re-simulated inner bounds, labeled, and only the full loop (`verify`) may call a fix a fix.

The other findings:
- **Pinning a real 2N3904** changes a verdict. Its datasheet guarantees only hFE ≥ 70 at 1 mA (onsemi 2N3903/D p. 2), which turns the MVP's bias spec from PASS to FAIL at 3σ (6.34 → 6.59 V). That verdict rests on AI-extracted data.
- **"Make the gain 10" breaks things if done by values alone.** Bias fails both ways and bass reaches 44 Hz. Splitting the emitter resistor fixes gain but moves bass to 31.5 Hz worst case, unless C_in is raised too.
- **The MVP needs almost none of this to be built now.** It needs:
  - machine-readable `spicy check` output with stable names, statuses and design revisions;
  - stored worst points and slopes;
  - a re-simulate-at-a-point entry point.

  The CLI is the first agent.

---

## 1. Setup: the circuit, two variants, and the numbers

The circuit is the walkthrough's common-emitter amplifier (walkthrough §1). The MVP file is `circuits/ce_amp.spl` (roadmap §4.2).

```
                      VCC  12 V ± 5%
                        │
            ┌───────────┴───────────┐
          R1 47k ±1%              RC 4.7k ±1%
            │                       ●──────── output  (VC)
   C_in     │                     C │
input ─||───●──── base ───────── B ─┤  Q1   β 100..=300
 1 µF ±20%  │                     E │
          R2 10k ±1%              RE 1k ±1%
            │                       │
           gnd                     gnd          temp −10..60 °C
```

| | **MVP design** (roadmap §4.2) | **Aged variant** (walkthrough §3) |
|---|---|---|
| Knobs | 8: range `temp`, `vcc.v`; statistical `r1`, `r2`, `rc`, `re`, `c_in`, `q1.beta` | 9: adds range `life` in [0, 1], with C_in × (1 − 0.2·life) |
| bass (f_L ≤ 30 Hz), worst case | **26.74 Hz, PASS** | **33.42 Hz, FAIL** |
| Used for | flows 3.3–3.7 | flows 3.1 and 3.2, which need a failing bass spec |

The aged variant needs `life` links, which are post-MVP (roadmap M6). I use it only because "why does bass fail?" needs a failing spec.

**Knob positions** are written ε ∈ [−1, +1]: −1 is the low edge, +1 the high edge, 0 the nominal. The nominal of a range knob is its midpoint (roadmap M1e), so `life` = 0.5 at ε = 0. The walkthrough quotes its nominal f_L at life = 0 (20.08 Hz); at mid-life it is 22.31 Hz.

**Model fidelity.** These are hand-model numbers, like the walkthrough's. The engine's real numbers will come from our simulator (Ebers–Moll, and SPICE temperature equations after M2c). They will differ slightly: our simulator gives VC = 5.52 V at nominal, measured with `run_op.py`, where the hand model gives 5.50 V.

---

## 2. The agent in one picture

```
 ┌───────────────────────────── user ──────────────────────────────┐
 │  chat · select-to-ask · reviews diffs · accept / reject          │
 └───────────────┬───────────────────────────────▲──────────────────┘
                 │ question, selection            │ answer + proposal (diff + verdict deltas)
 ┌───────────────▼───────────────────────────────┴──────────────────┐
 │  AGENT (LLM in a tool-use loop)                                    │
 │  reads, plans, proposes, explains.  Never decides a verdict,       │
 │  never writes the file.                                            │
 └──┬──────────────┬────────────────┬───────────────┬────────────────┘
    │ lang.*       │ engine.*       │ sim.*         │ parts.*
 ┌──▼─────────┐ ┌──▼────────────┐ ┌─▼───────────┐ ┌─▼─────────────────┐
 │ language   │ │ bounds engine │ │ one-point   │ │ part records      │
 │ service    │ │ check/explain │ │ simulation, │ │ lookup, extract,  │
 │ read slice │ │ what_if       │ │ sweeps      │ │ E-series snap     │
 │ propose    │ │ verify, solve │ │             │ │                   │
 │ diff, apply│ │               │ │             │ │                   │
 └──┬─────────┘ └──┬────────────┘ └─┬───────────┘ └───────────────────┘
    │ .spl text    │ Backend trait  │
    ▼  (the truth) ▼                ▼
                simulators (ours, ngspice later)
```

**Three sources of truth, one owner each:**

| Truth about… | Lives in | Who may change it |
|---|---|---|
| The design | The `.spl` text, identified by a revision hash `rev` | Only `lang.apply`, which needs the user's accept |
| Verdicts | Engine records with `status: verified` and the matching `rev` | Only the engine |
| Part data | Part records with `#[source]`, `#[from]`, `reviewed` (language §6.5) | The agent may draft them (`reviewed = false`); only a person may set `reviewed = true` |

**The statuses every engine answer carries** (a closed set):

| Status | Meaning | Can prove |
|---|---|---|
| `verified` | The loop (engine.md §5) ran on this `rev` | The engine's verdict: PASS (estimated or guaranteed), FAIL, UNDECIDED, … |
| `inner_bound` | A real simulated value at a real knob point, on this `rev` | **FAIL only** (the point exists, so the value happens) |
| `estimated` | Computed from stored slopes, no simulation | **Nothing.** Shown as "≈", never as ✓ or ✗ |
| `stale` | Computed on an older `rev` | Nothing, until re-run |

This is P2 of engine.md ("inner bounds prove failures; outer bounds prove passes"), applied to the agent's words.

---

## 3. The flows

Each flow has a sequence diagram, a short transcript with real numbers, and what it teaches about the interface. Tool calls appear in `[brackets]` in the transcripts, and the user doesn't see them. The revision hash `a3f91c` is illustrative.

### 3.1 "Why does bass fail?" (aged variant)

```
 user                 agent                         engine                  lang
  │ why does bass      │                              │                       │
  │ fail?              │                              │                       │
  │───────────────────►│ explain(bass, max, sigma3)   │                       │
  │                    │─────────────────────────────►│ reads stored results  │
  │                    │◄──── record: FAIL 31.67 Hz ──│ (no simulation)       │
  │                    │      worst point, form there,│                       │
  │                    │      contributors            │                       │
  │                    │ explain(bass, max, worst_case)                       │
  │                    │─────────────────────────────►│                       │
  │                    │◄──── FAIL 33.42 Hz, corner ──│                       │
  │                    │ read(slice: c_in, r1, r2, q1, spec bass)             │
  │                    │─────────────────────────────────────────────────────►│
  │                    │◄──── language text, line anchors, rev ───────────────│
  │◄───────────────────│ answer, citing record ids    │                       │
```

```
you    Why does bass fail?
agent  [engine.explain bass max sigma(3) → verified, rev a3f91c]
       [engine.explain bass max worst_case → verified]
       Bass fails. At 3σ its worst f_L is 31.67 Hz against the 30 Hz limit, at
       −10 °C, 12.6 V, end of life, with C_in 19.9% low (β 190, others nominal).
       With every part at its worst edge it's 33.42 Hz.

       Why: f_L = 1/(2π·C_in·R_in). C_in shrinks twice, and the effects multiply:
       −20% tolerance, then −20% aging, gives 0.64 × 1 µF, so f_L rises by 1/0.64.
       At the 3σ point, C_in is 98.9% of the statistical spread. β is 1%. The
       resistors hardly matter.

       Aging is what flips it. With life fixed at "new", the absolute worst case is
       26.74 Hz, a pass. The design passes on day 1 and fails in the field.
       Would you like fix options?
```

**What the agent received** (the `explain` payload, compact form, §5):

```
bass: f_L <= 30Hz @sigma3 FAIL 31.67Hz (margin -1.67) verified rev a3f91c
 worst: temp=-10°C vcc=12.6V life=10y c_in=-19.9% beta=189.8 (others within 0.03% of nominal)
 form@worst: 31.67 -7.91c_in +3.96life -0.81beta -0.33temp -0.25r2 -0.05r1 -0.02re  [eps -1..1, + = high edge]
 abs worst 33.42Hz verified | loop 2 rounds, L0 slopes | tags: distribution-sensitive
```

Five numbers from the loop behind it (`flow1_explain.py`) matter for the interface:

| Step | f_L | Status |
|---|---|---|
| Line taken at nominal, pushed to its worst corner | 30.06 Hz | estimated |
| That corner, simulated | **33.42 Hz** | inner bound → FAIL (brute force over 512 corners agrees) |
| 3σ: range corner with nominal parts | 25.31 Hz | inner bound |
| 3σ: line from that corner predicts | 30.41 Hz | estimated |
| 3σ point, simulated, converged in round 2 | **31.67 Hz** | verified FAIL; Monte Carlo at that corner: 177 of 20,000 fail (0.89%) |

**Lesson 1: send the form taken at the worst point, not at nominal.** The nominal line says 30.06 Hz: "borderline". An agent given that line would call the problem marginal. It is 3.4 Hz worse. So `explain` returns the form where the loop last linearized, says where that was, and says how far the line missed when it was checked.

**Lesson 2: "contributors" needs one stated definition.** The same spec gives very different shares depending on the definition:

| Definition (at the worst-case corner, 33.42 Hz) | c_in | life | β | temp | r2 |
|---|---|---|---|---|---|
| Linear share: \|coeff\| / Σ\|coeff\| | 50.9% | 25.4% | 17.8% | 3.8% | 1.5% |
| Recovery: simulate with this one knob back at nominal | −6.68 Hz | −3.71 Hz | −1.47 Hz | −0.52 Hz | −0.25 Hz |
| Statistical variance share, at the 3σ point instead | 98.9% | (range) | 1.0% | (range) | 0.1% |

`language_editor_mapping.md` §8.1 shows `life: 46%, c_in.tol: 46%, beta: 4%, temp: 3%` for the same spec, without saying how those shares were computed, and I couldn't reproduce them.

Recommendation: report **recovery** for statistical knobs, since it answers "what if this part were perfect?" and costs one run per knob, done lazily. For range knobs, report "what if this condition were removed" (life fixed at new: 26.74 Hz). Always label the point.

### 3.2 "Fix bass" (aged variant)

```
 user            agent                       lang                  engine
  │ fix bass      │                           │                      │
  │──────────────►│ (plans 3 candidates from the form: c_in dominates;
  │               │  R_in is the other lever) │                      │
  │               │ propose(patch A|B|C)      │                      │
  │               │──────────────────────────►│ compile, type-check  │
  │               │◄── proposal ids, diffs ───│                      │
  │               │ what_if(A, B, C)                                 │
  │               │─────────────────────────────────────────────────►│ stored forms (log)
  │               │◄──── estimates + inner bounds (re-sim) ──────────│ + re-sim stored points
  │◄──────────────│ "≈ early look" (labeled estimated)               │  (≤ 10 runs each)
  │               │ verify(A, B, C)                                  │
  │               │─────────────────────────────────────────────────►│ full loop per proposal
  │               │◄──── verified records + verdict deltas ──────────│  (seconds)
  │◄──────────────│ diff + verdict delta table; recommends B or A    │
  │ accept B      │                           │                      │
  │──────────────►│ apply(B, user token)      │                      │
  │               │──────────────────────────►│ edits bytes of one statement
  │               │◄── new rev ───────────────│                      │
```

**The three candidates.** The agent derives them from the explain payload: C_in dominates, and R_in is the other term in f_L.

| | Edit (one statement each) | Why the agent proposes it |
|---|---|---|
| A | `let c_in = Electrolytic { p: base, n: input, value: 2.2uF ± 20% };` | f_L ∝ 1/C; 2.2 µF is an E6 value |
| B | `let c_in = Capacitor { a: base, b: input, value: 1uF ± 5%, dielectric: Film };` | Removes both the aging and most of the tolerance. Note the kind change: pins `p`/`n` become `a`/`b` (language §6.1) |
| C | `r1: 100k ± 1%`, `r2: 22k ± 1%` (two statements) | Doubles R_in (7.93 → 16.57 kΩ); keeps the divider ratio close |

**Instant predictions vs verified values** (`flow2_fix.py`; worst_case = WC, sigma(3) = σ3). The prediction methods:
- **P1** is the stored line in ε, as engine.md §3.1 stores it.
- **P2** is the same slopes, with knob values and positive measures in log coordinates.
- **P3** re-simulates the stored worst point on the edited design.

| Candidate · spec side | P1 line in ε | P2 line in log | P3 re-sim stored point | **Verified (loop = brute force)** | Limit |
|---|---|---|---|---|---|
| A · bass WC | **−6.68 Hz** | 15.19 | 15.19 | **15.19** | ≤ 30 |
| A · bass σ3 | — | 14.40 | 14.40 | **14.40** | ≤ 30 |
| B · bass WC | 18.80 | 21.92 | 22.52 | **22.52** | ≤ 30 |
| B · bass σ3 | — | 20.86 | 21.36 | **21.53** | ≤ 30 |
| C · bass WC | **−2.75 Hz** | 16.38 | 16.89 | **16.89** | ≤ 30 |
| C · bias max WC | 6.68 | 6.75 | 6.93 | **6.93** | ≤ 6.5 |
| C · bias max σ3 | — | **6.51** | 6.69 | **6.70** | ≤ 6.5 |
| C · bias min WC | **4.29** | 4.52 | 4.58 | **4.58** | ≥ 4.5 |

**Where the instant prediction is wrong enough to matter:**

1. **P1 extrapolates past zero.** Changing C_in from 1 to 2.2 µF is six knob-widths outside the box (ΔC = 1.2 µF, half-range 0.2 µF). A straight line in C goes negative: −6.68 Hz. The divider edit is 112 knob-widths out, measured linearly (−2.75 Hz). A real f_L cannot be negative, but a "−6.68 Hz ≤ 30 Hz: PASS" would look like a pass. **Rule: an estimate must never extrapolate linearly in a scale quantity (R, C, β). Use log coordinates.** In log coordinates, f_L ∝ 1/C is exact (P2 = 15.19).
2. **P2 misses interactions: candidate C breaks bias.** The divider change doubles R_th (8.25 → 18.0 kΩ). The base current's drop across R_th then matters twice as much, so VC's β slope doubles: −0.141 → −0.276 V per ε (`flow2_fix.py`). An affine form has no R_th × β term, so P2 predicts 6.51 V at 3σ. That is 13 mV over the limit, inside any honest error band, so at best "UNDECIDED". The truth is **6.70 V, a clear FAIL**, and the bias spec was a PASS (6.34 V) before the edit. This is the case where an agent trusting the instant number would present a fix that breaks another spec.
3. **P1 raises false alarms too:** it puts C's VC minimum at 4.29 V (FAIL), and the truth is 4.58 V (PASS).
4. **P3 was exact whenever the worst corner didn't move** (every WC row), and each P3 value is a real point. For C it gives 6.69 V at 3σ **instantly, as a proven failure**, with no loop. It is one run per stored point, and a full check stores at most 10 points for the MVP (5 spec sides × 2 confidences). For B at 3σ, P3 gives 21.36 Hz against the verified 21.53: correctly an under-estimate, because the 3σ point moved. That is why P3 is an inner bound and not a verdict.

**What the user sees** (the diff with verdict deltas; language_editor_mapping §8.3's panel, with statuses):

```
┌ PROPOSALS for "fix bass" · base rev a3f91c ────────────────────────────────────────────────────────┐
│ spec        before (verified)      A: c_in 2.2µ elyt     B: c_in 1µ film ±5%  C: divider 100k/22k  │
│ bass  WC    ✗ 33.42 Hz             ✓ 15.19               ✓ 22.52              ✓ 16.89              │
│ bass  σ3    ✗ 31.67 Hz             ✓ 14.40               ✓ 21.53              ✓ 15.35              │
│ bias  σ3    ✓ 6.34 V (max)         ✓ 6.34  unchanged     ✓ 6.34  unchanged    ✗ 6.70  NEW FAILURE  │
│ bias  WC    ✗ 6.59 V (max)         ✗ 6.59  unchanged     ✗ 6.59  unchanged    ✗ 6.93  worse        │
│ gain  WC    ✓ 4.46…4.70            ✓ unchanged           ✓ unchanged          ✓ 4.46…4.70          │
│ status                             verified (loop)       verified (loop)      verified (loop)      │
├────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ a accept · x reject · gc code diff · ]p next proposal                                              │
└────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

```
you    Fix bass.
agent  [lang.propose ×3] [engine.what_if A,B,C → estimated + inner bounds]
       Early look, not verified yet: A and B clear bass easily. C clears bass
       but already shows a real bias failure: 6.69 V at the old 3σ point (limit 6.5 V).
       [engine.verify A,B,C → verified]
       Verified:
       • A (2.2 µF electrolytic): bass 15.19 Hz worst case. Nothing else changes.
       • B (1 µF film ±5%): bass 22.52 Hz worst case. No aging at all.
       • C (divider 100k/22k): bass 16.89 Hz, but it breaks bias at 3σ (6.70 V,
         was 6.34 V). A higher divider resistance makes VC twice as sensitive to β.
         Not recommended.
       Bias still fails at absolute worst case (6.59 V) in A and B. That was true
       before and is unrelated to bass. I recommend A or B. Accept one?
you    B.
agent  [lang.apply B, user token] Applied. rev 5c07e2. bass PASS (verified on this rev).
```

**Lesson: `what_if` returns two things, labeled.** It returns P2 as `estimated`, and P3 as `inner_bound` with its point. It also returns a trust flag: how many knob-widths the edit moved each knob. If an edit changes the topology (a new part), it says "no stored form covers this; run verify" (flow 3.4).

### 3.3 "Is this design OK?" (pre-sign-off review, MVP design)

```
 user          agent                          engine                        lang
  │ is this     │ check(all specs, worst_case + sigma3)                       │
  │ design OK?  │───────────────────────────────►│ safety net + loop          │
  │────────────►│                                │ (231 runs, L0, ≈1 s)       │
  │             │◄──── 10 records, cost, not_modeled list                    │
  │             │ read(whole block + contract)                                │
  │             │─────────────────────────────────────────────────────────────►│
  │             │◄──── 32 lines of text ──────────────────────────────────────│
  │             │ (compares what's modeled with what the parts are:
  │             │  Electrolytic without aging, generic Npn, no load)          │
  │◄────────────│ verdict table + "rests on" + "not checked" + next steps     │
```

**What the engine returns** (`loop_cost.py`, `flow6_part.py`; brute force agrees on every worst-case value):

| Spec | Side | worst_case | sigma(3) (MVP default) | Verdict |
|---|---|---|---|---|
| bias 4.5…6.5 V | max | **6.5905 V ✗** at −10 °C, 12.6 V, R1 +1%, R2 −1%, RC −1%, RE +1%, β 100 | 6.3428 V ✓ | **Engineer's call:** FAIL only with all parts at their edges |
| bias | min | 4.6597 V ✓ | 4.8517 V ✓ | PASS (estimated) |
| gain 4.6 ± 5% | both | 4.4626 … 4.7030 ✓ | 4.5181 … 4.6649 ✓ | PASS (estimated) |
| bass ≤ 30 Hz | max | 26.74 Hz ✓ | 25.34 Hz ✓ | PASS (estimated) |

Cost: **231 simulations** for all 3 specs, both sides and both confidences, with L0 forward-difference slopes and the safety net shared by all specs. Brute force is 256. At 8 knobs the L0 loop is barely cheaper than brute force; its advantage appears as the knob count grows (the aged 9-knob design takes 248 runs against 512). Through the CLI, one run takes about 4 ms including process start-up (50 runs in 0.21 s, measured), so the check takes about 1 s.

**What the agent adds: what the verdicts rest on, and what they don't cover.** This part is the agent's value. Each item comes from comparing the model with the text, not from the engine:

```
agent  Verdicts (verified, rev a3f91c): gain and bass pass. Bias passes at 3σ
       (6.34 V) but fails at absolute worst case (6.59 V at −10 °C, 12.6 V, all
       resistors at their edges, β = 100). Whether that matters is your call.

       These verdicts assume:
       • β 100..=300 for a generic Npn, spread normal with the edges at 3σ.
       • The temperature behaviour of our simulator's BJT model.

       Not checked, because the MVP doesn't model it:
       1. Aging of c_in (an Electrolytic). With a 20% end-of-life loss, bass fails:
          33.4 Hz worst case, 31.7 Hz at 3σ (walkthrough §6).
       2. A real transistor. A 2N3904 guarantees only hFE ≥ 70 at 1 mA. With that,
          bias also fails at 3σ (6.59 V). See "use a real transistor".
       3. Output load and source impedance: none is assumed.
       4. C_in voltage rating and derating: no automatic checks yet.

       Also: bias's comment says it exists to leave room for a ±1 V swing. A direct
       swing spec would pass with more than 1.3 V to spare, even at the bias
       counterexample (6.0 V up, 4.2 V down). The bias window is stricter than the
       need it stands for. Want me to add the swing spec?
```

**Lesson: the engine should hand the agent a `not_modeled` list.** The agent can guess items 1, 3 and 4 from part kinds, but it's more reliable if the engine lists them. The list comes from knowing which standard knobs each part kind has and which the MVP lacks, e.g. "c_in: Electrolytic has no life link (M6)". This is P4 of engine.md ("every verdict says what it rests on"), extended to what it *doesn't* rest on.

### 3.4 "Make the gain 10 with the same bias"

engine.md §4.8 would solve this with `?` values. The MVP has no `?` (roadmap §4.1), so here the agent does the algebra itself, using engine numbers, and verifies. Post-MVP the same flow calls `engine.solve`.

```
 user              agent                                   engine / lang
  │ gain 10, same   │ sim.run(nominal) → IC 1.383 mA, r_π 3715 Ω
  │ bias            │ algebra: RE_ac = (β·RC/10 − r_π)/(β+1) = 449.2 Ω → E96 453 Ω
  │────────────────►│ option A (values): RE 453, re-set divider → propose, what_if
  │                 │◄── inner bounds: 3 real FAILs from 3 runs → A rejected
  │                 │ option B (topology): RE1 453 + RE2 549 ∥ C_E → propose
  │                 │ what_if → "new knobs re1, re2, c_e: no stored form" → verify
  │                 │◄── verified: gain ✓, bias as before, bass ✗ WC (31.5 Hz)
  │                 │ option B': B + c_in 2.2 µF → verify → all ✓ except old bias WC
  │◄────────────────│ diff (3 new statements, 1 new net, spec gain 4.6 → 10)
```

| Option | Nominal | bias WC | bias σ3 | gain 10 ± 5% (WC) | bass WC / σ3 |
|---|---|---|---|---|---|
| Base (gain 4.6 spec) | VC 5.50 V | 4.66 … **6.59 ✗** | 4.85 … 6.34 ✓ | (4.46 … 4.70 vs 4.6 ± 5%) | 26.74 / 25.34 ✓ |
| **A:** values only: RE 453 Ω, R2 5.76 kΩ (keeps VC ≈ 5.5 V) | VC 5.63, VE 0.62 V, f_L 32.7 Hz | **4.38 … 7.04 ✗✗** | 4.65 … **6.75 ✗** | 9.56 … 10.18 ✓ | **44.27 ✗ / 41.42 ✗** |
| **B:** RE1 453 unbypassed + RE2 549 ∥ C_E 100 µF | VC 5.51, gain 9.92 | 4.67 … **6.60 ✗** (as before) | 4.87 … 6.35 ✓ | 9.61 … 10.19 ✓ | **31.55 ✗** / 28.20 ✓ |
| B with C_E 470 µF | same | same | same | 9.61 … 10.19 ✓ | 29.72 ✓ (0.28 Hz margin) / 26.96 ✓ |
| **B':** B + C_in 2.2 µF | f_L 11.9 Hz | 4.67 … 6.60 ✗ (as before) | ✓ | 9.61 … 10.19 ✓ | 16.44 ✓ / 14.43 ✓ |

(`flow4_gain10.py`. Gain is |H(1 kHz)| from a complex small-signal solve for the split designs.)

What the numbers teach:

- **Why A fails.** Keeping IC with a 453 Ω emitter resistor drops VE from 1.39 to 0.62 V. The −2 mV/K of VBE now moves the current twice as much: VC's temperature slope goes from −0.354 to −0.712 V per ε. A simple rule captures it: VC's drift ≈ gain × VBE's drift (4.6 × 2 mV/K ≈ 9.2 mV/K before, 9.9 × 2 ≈ 20 mV/K after). The smaller divider also drops R_in to 4.9 kΩ, so bass fails.
- **The instant what-if disposes of A in three runs.** It re-simulates the base's stored worst corners on design A and gets VC 7.04 V, VC 4.49 V and f_L 44.27 Hz. All three are real points (`flow47_extra.py`), so A is **proven** to fail without running a loop. The true minimum is 4.38 V: that worst corner moved, so P3 found the failure but not the worst value, which is exactly what an inner bound promises.
- **B has no stored form to lean on.** It adds a net (`ex`), two parts and three knobs (re1, re2, c_e). `what_if` must say so and route to `verify`. It must not reuse RE's slopes for RE1.
- **B moves the bass problem.** The bypass pole adds to f_L, and R_in drops because only 453 Ω is degenerated. A fix for gain turns into a bass failure at worst case (31.55 Hz). Only a full re-check of **every** spec catches this, which is why `verify` always runs all specs, not just the one being fixed.
- **The spec edit is explicit.** "Make the gain 10" changes a requirement (`spec gain: … in 4.6 ± 5%` → `in 10 ± 5%`). The agent keeps the ±5% and says so, and the diff labels that hunk **requirement change**, separately from the design hunks.

Post-MVP the agent writes the question in the language, and `engine.solve` returns snapped candidates, each with a verified verdict:

```rust
let re1 = Resistor { a: emitter, b: ex, value: ? };      // unbypassed: sets the gain
let re2 = Resistor { a: ex, b: gnd, value: ? };          // bypassed: keeps VE ≈ 1.4 V
let c_e = Electrolytic { p: ex, n: gnd, value: ? };
```

### 3.5 "Add a spec: the output must swing ±1 V"

```
 user              agent                               lang                  engine
  │ output must     │ capabilities() → MVP: dc(), ac(); no tran, no benches      │
  │ swing ±1 V      │─────────────────────────────────────────────────────────►│
  │────────────────►│ derive DC-headroom form (cutoff + saturation)            │
  │                 │ propose(spec swing_up, spec swing_down)                  │
  │                 │──────────────────────────────►│ units, roles, probes ok  │
  │                 │ verify(proposal)                                         │
  │                 │─────────────────────────────────────────────────────────►│
  │                 │◄── PASS: 5.17 V up, 2.52 V down (worst case) ────────────│
  │◄────────────────│ spec text + derivation + verdict; notes the bias proxy   │
```

**Choosing the measure.** "Swing ±1 V without clipping" has two natural forms:

| Form | Text | Available in the MVP? | Problem |
|---|---|---|---|
| Drive it and measure distortion | `bench loud { input: Sine { amp: 218mV, freq: 1kHz }, … }` + `tran(output.v).thd(1kHz) < 1% on loud` | No: `tran`, signal patterns and benches are out (roadmap §4.1) | language.md §0's contract assumes input ≤ 100 mV, i.e. at most 100 mV × 4.70 = 0.47 V out. A 218 mV bench breaks the rule that "every bench is checked to stay inside the block's assumptions" (language §8.5) |
| DC headroom | limits on VCC − VC and VCE | Yes: `dc()` of probe differences | Needs a derivation (below) and an assumed VCE,sat |

**The derivation the agent writes into the doc comments.**

With RE unbypassed, a collector swing v moves VCE by v·(RC + RE)/RC:
- **Up (cutoff):** the collector can rise until IC reaches 0, i.e. by IC·RC = VCC − VC. So VCC − VC ≥ 1 V.
- **Down (saturation):** VCE must stay above VCE,sat (0.2 V: the 2N3904's maximum at 10 mA, datasheet p. 2; assumed here) while the collector falls 1 V. So VCE ≥ 0.2 V + 1 V × 5.7k/4.7k = **1.413 V**, which gets rounded up to 1.42 V.

```rust
/// Output can swing +1 V before cutoff: the collector can rise by IC·RC = VCC − VC.
spec swing_up: dc(vcc.v - output.v) >= 1V;
/// Output can swing −1 V before saturation: with RE unbypassed, VCE falls by
/// 1 V × (RC + RE)/RC = 1.21 V, and must stay above VCE,sat ≈ 0.2 V (assumed).
spec swing_down: dc(output.v - emitter.v) >= 1.42V;
```

The engine validates it in three layers:
1. **The language service:** units (V vs V), the probes exist, and roles (a contract may read internal nets, language §8.1).
2. **`capabilities`:** `dc()` of a difference is supported.
3. **The loop:** the verdicts below.

The confidence is the project default, sigma(3), and the agent says so.

| | Nominal | worst_case min | sigma(3) min | Verdict |
|---|---|---|---|---|
| swing_up: VCC − VC | 6.50 V | **5.165 V** | 5.383 V | PASS (≥ 1 V) |
| swing_down: VCE | 4.11 V | **3.250 V** | 3.464 V | PASS (≥ 1.42 V) |
| (for reference) down-swing actually available | 3.22 V | 2.524 V | 2.695 V | |
| (for reference) with a 10 kΩ AC load | | up 3.525 V, down 2.333 V | | still ≥ 1 V |

(`flow5_swing.py`)

**The proxy finding.** The existing bias spec's comment says it exists for this swing. At the bias counterexample (VC = 6.59 V, the worst-case FAIL) there is still 6.01 V of up-swing and 4.17 V of down-swing. So the requirement the window stands for passes with a large margin exactly where the window fails. The agent should **report** this and offer to add the direct spec. It must **not** loosen or delete `bias` on its own: that's a requirement change (§6, G6).

### 3.6 "Use a real transistor: 2N3904"

```
 user            agent                      parts                  lang           engine
  │ use a real    │ lookup("2N3904")           │                      │               │
  │ 2N3904        │───────────────────────────►│ (MVP: no catalog)    │               │
  │──────────────►│ extract(onsemi 2N3903/D)   │                      │               │
  │               │───────────────────────────►│ PDF → record, every  │               │
  │               │◄── record, reviewed=false, │ field #[from(p,row)] │               │
  │               │    gaps + conflation notes │                      │               │
  │               │ capabilities(): BJT params IS BF BR NF NR only                   │
  │               │ propose(beta: 70..=300 with provenance comment)  │               │
  │               │──────────────────────────────────────────────────►│               │
  │               │ verify ─────────────────────────────────────────────────────────►│
  │               │◄── bias σ3 FAIL 6.59 V, tag relies-on-unreviewed-part-data ─────│
  │◄──────────────│ record for review + verdict delta + what to check first          │
```

**What the datasheet actually says** (onsemi 2N3903/D, Rev. 9, August 2021, p. 2, "ON Characteristics", 2N3904 rows; read from the PDF at https://www.onsemi.com/pdf/datasheet/2n3903-d.pdf):

| Row | Condition | Min | Max |
|---|---|---|---|
| DC current gain hFE | IC = 0.1 mA, VCE = 1 V | 40 | — |
| | **IC = 1.0 mA, VCE = 1 V** | **70** | **—** |
| | IC = 10 mA, VCE = 1 V | 100 | 300 |
| | IC = 50 mA / 100 mA | 60 / 30 | — |
| Small-signal current gain hfe | IC = 1.0 mA, VCE = 10 V, f = 1 kHz | 100 | 400 |
| VCE(sat) | IC = 10 mA, IB = 1 mA | — | 0.2 V |

Temperature: the only hFE-vs-temperature data is Figure 15 (p. 6), a **typical** normalized curve at −55, 25 and 125 °C. Read by eye at our IC ≈ 1.4 mA it gives about 0.42, 0.8 and 1.25. No temperature coefficient is guaranteed.

**Three traps an extraction must not fall into.** Each is a plausible hallucination:

1. **Taking "100..300" as β.** That row is at 10 mA. Our transistor runs at about 1.3–1.4 mA, where only a minimum (70) is tested.
2. **Taking hfe 100..400 as β.** That's the small-signal gain, a different quantity (dIC/dIB), measured at VCE = 10 V.
3. **Taking a typical curve as a limit.** Figure 15 suggests β rises roughly 0.45 %/K above 25 °C and 0.8 %/K below it, near 1 mA. It is typical only.

**The record the agent drafts** (language §6.5 style; the MVP has no `part` items, see below):

```rust
/// onsemi 2N3904, TO-92 (2N3903/D Rev. 9).
#[source(datasheet = "https://www.onsemi.com/pdf/datasheet/2n3903-d.pdf", extracted = "ai", reviewed = false)]
pub part Q2N3904: Npn {
    #[from(page = 2, table = "ON Characteristics", row = "hFE, IC = 1.0 mA, VCE = 1.0 V")]
    beta_min_1ma: 70,                         // tested limit
    #[from(page = 2, row = "hFE, IC = 10 mA")]
    beta_max: 300,                            // ASSUMED at 1 mA: tested only at 10 mA (typical hFE is lower at 1 mA, Fig. 15)
    #[from(page = 6, figure = 15)]
    beta_tc: typical_only,                    // no guaranteed tempco
    #[from(page = 2, row = "VCE(sat), IC = 10 mA, IB = 1 mA")]
    vce_sat_max: 0.2V,
}
```

**The model problem.** A commonly circulated vendor-style `Q2N3904` model (`Is=6.734f Xti=3 Eg=1.11 Vaf=74.03 Bf=416.4 … Rb=10`; I did not verify its original source) is rejected by our simulator: `Parse error: invalid param: Xti` (tested with our binary, `q2n3904.spicy`). Our parser accepts only IS, BF, BR, NF and NR for BJTs (`crates/spicy_netlist/src/reader/netlist_models.rs`). So the agent must:
- call `capabilities()` before attaching a model;
- either keep the generic Ebers–Moll with the datasheet's β range, or route to ngspice (M4);
- tag the verdict **model-conditional** if parameters are dropped.

**Verdict delta** (`flow6_part.py`; MVP design, β nominal = midpoint):

| Spec side | Generic, β 100..=300 | 2N3904, β 70..=300 (max assumed) | 2N3904, β 70..=400 (max from hfe, derived) |
|---|---|---|---|
| bias max WC / σ3 | 6.59 ✗ / **6.34 ✓** | 6.83 ✗ / **6.59 ✗** | 6.83 ✗ / 6.59 ✗ |
| bias min WC / σ3 | 4.66 / 4.85 ✓ | 4.66 / 4.86 ✓ | 4.62 / 4.81 ✓ |
| gain min WC | 4.463 ✓ | 4.437 ✓ | 4.437 ✓ |
| bass max WC / σ3 | 26.74 / 25.34 ✓ | 27.73 / 25.46 ✓ | 27.73 / 25.18 ✓ |

The real part turns bias at 3σ from PASS into FAIL. The engine says FAIL is "always definite", but here the point is only reachable if β really can be 70. So the verdict is **FAIL, relies-on-unreviewed-part-data**, and the agent must say which number to check first: "p. 2, hFE at 1 mA". language.md §0 names `onsemi::MMBT3904` (SOT-23). I checked the 2N3904 (TO-92) datasheet only, not MMBT3904's. The MVP file's β 100..=300 may be optimistic for the part the docs name.

**In the MVP** there are no `part:` items or provenance attributes (M5). The agent can only edit `q1`'s `beta:` field, with a doc comment carrying the citation. That comment is the provenance until M5.

### 3.7 Select-to-ask on a region

The user drag-selects `r1`, `r2`, `re`, `q1` (the bias network) and asks "why is the bias so temperature-sensitive?". The context bundle is language.md §10's: a slice that compiles on its own, a rendered image, and the results for exactly those entities.

```
 user              editor                           agent                       engine / sim
  │ select r1 r2    │                                  │                             │
  │ re q1, ask      │ bundle: slice text (+ anchors),  │                             │
  │────────────────►│ image, op of the selected nets,  │                             │
  │                 │ bias records, rev                │                             │
  │                 │─────────────────────────────────►│ sweep(temp, −10..60, VC)    │
  │                 │                                  │────────────────────────────►│
  │                 │                                  │◄──── table ─────────────────│
  │◄─────────────────────────────── answer with numbers, cites slice lines           │
```

```rust
// slice of CeAmp (circuits/ce_amp.spl) · selection: r1 r2 re q1 · rev a3f91c
assume temp in -10°C..=60°C;                                    // :21
assume vcc.v in 12V ± 5%;                                       // :22
/// Room for the output to swing ±1 V without clipping.
spec bias: dc(output.v) in 4.5V..=6.5V;                         // :27
/// Divider holds the base near 2.1 V.
let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };         // :12
let r2 = Resistor { a: base, b: gnd, value: 10k ± 1% };         // :13
let re = Resistor { a: emitter, b: gnd, value: 1k ± 1% };       // :15
let q1 = Npn { c: output, b: base, e: emitter, beta: 100..=300 };   // :17
// outside the selection: rc (vcc–output), c_in (input–base)
```
```
results (hand model, nominal): base 2.048 V · emitter 1.390 V · output 5.500 V · q1.ic 1.383 mA
bias max: worst_case FAIL 6.59 V · sigma3 PASS 6.34 V · form@nominal: 5.500 -0.354temp -0.141beta +0.135vcc +0.077r1 -0.075r2 -0.065rc +0.061re
```

```
agent  [sim.sweep temp −10..60 °C, VC, others nominal]
       VC falls 10.2 mV/K: 5.86 V at −10 °C, 5.15 V at 60 °C. Temperature is the
       biggest term in the bias spread (±0.354 V of it).
       87% of that is VBE's −2 mV/K. It appears across re (:15): over 70 K, 140 mV
       more across 1 kΩ + R_th/β ≈ 1.04 kΩ is 0.135 mA more collector current, and
       0.63 V less at the output. β's own tempco adds 0.10 V; VT's change nets
       ≈ 0.01 V the other way.
       Rule of thumb: VC drifts by about gain × 2 mV/K. For a gain near
       rc/re = 4.7 you can only cut it by bypassing part of re (which raises the
       gain) or by lowering rc/re (which lowers it).
```

(Temperature split from `flow47_extra.py`: full model 0.710 V over 70 K; VBE tempco alone 0.618 V; β tempco alone 0.102 V; VT alone −0.009 V.)

**Lesson:** select-to-ask needs no new engine call. It needs:
- `explain` records filtered to the knobs and specs the selection touches;
- `sim.sweep`, which already exists as a backend capability.

The slice must keep line anchors, so answers cite lines and patches anchor to them (language_editor_mapping §8.1). **In the MVP there is no editor.** The CLI equivalent is `spicy check --format json` plus reading the file.

---

## 4. The tool interface

The tools are namespaced by owner, as Anthropic's tool-writing guidance recommends: "Namespacing (grouping related tools under common prefixes) can help delineate boundaries" (https://www.anthropic.com/engineering/writing-tools-for-agents). They are few and workflow-shaped. There is no `run_corner(i)` tool the agent could call 256 times: "More tools don't always lead to better outcomes" (same source).

### 4.1 The calls

| Tool | Input | Output | Cost class | Truthfulness rule |
|---|---|---|---|---|
| `lang.read` | `{selection \| block \| spec, rev?}` | Language text with line anchors, `rev` | instant | Text is the truth; the slice must compile with stubs |
| `lang.propose` | `{base_rev, patch}` (keyed statements, deletes, renames) | `proposal_id`, text diff, structural diff (values, kinds, specs), diagnostics as JSON with fix-its | instant | Never touches the user's file. Spec or confidence changes are flagged `requirement_change` |
| `lang.diff` | `{a: rev \| proposal, b: …}` | Text + structural diff | instant | — |
| `lang.apply` | `{proposal_id, user_token}` | New `rev` | instant | **Needs a token only the UI mints on the user's accept.** The agent cannot mint one. Fails if `base_rev` is no longer current |
| `engine.capabilities` | `{}` | Analyses, measures, device kinds, model params per device, sensitivity level (L0…L3), backends | instant | The agent must check this before writing a measure or attaching a model |
| `engine.check` | `{target: rev, specs?, confidences?}` | Verdict records (§4.2), cost, `not_modeled` | seconds | Only source of `verified` |
| `engine.explain` | `{spec, side, confidence, detail: concise \| full}` | The stored record, form at the worst point (where taken, miss seen), form at nominal, contributors (with definition), counterexample, guards fired | instant | Reads stored results. `stale` if `rev` changed |
| `engine.what_if` | `{proposal_id}` | Per record: `estimate` (log form), `inner_bound` (re-simulated stored points, each with its point), `trust` (knob-widths moved), or `not_covered` (new knobs) | instant (≤ 10 runs for the MVP) | Status `estimated` / `inner_bound` only. May say "FAIL (proven at a re-simulated point)"; may never say PASS |
| `engine.verify` | `{proposal_id}` | `check` on the proposal, plus the verdict delta against the base | seconds | Always all specs, not just the one being fixed |
| `sim.run` | `{target, point, analyses}` | Node voltages, each device's operating point and region, measures | instant (1 run) | `inner_bound` for any spec measure it computes |
| `sim.sweep` | `{target, knob, values, at?, measures}` | Table | seconds | Values are real points; a sweep is not a verdict |
| `engine.solve` (post-MVP) | `{target, unknowns: ["re1.value", …], specs, series: E96}` | Ranked, snapped candidates, each `verified` | minutes | Candidates are verified after snapping (engine.md §4.8) |
| `parts.lookup` (post-MVP) | `{mpn}` | Exists? catalog data, source | seconds | "Not found" is an answer; never invent an MPN |
| `parts.extract` | `{source: url \| pdf, mpn}` | Record text with `reviewed = false`, every field `#[from(page, table, row)]`, plus `gaps` and `conflations_avoided` | minutes | The agent may draft; only a person sets `reviewed = true` |
| `parts.snap` | `{value, series}` | Nearest values, e.g. 449.2 Ω → 453 (E96), 430/470 (E24) | instant | The agent must not propose an off-series value for a purchased part |

**Cost classes, with the CE amplifier's measured and computed numbers:**

| Class | Budget | CE amp | Calls |
|---|---|---|---|
| **instant** | No loop; at most a few single runs | Stored-record reads; `what_if` re-simulates ≤ 10 points, ≈ 40 ms through the CLI at ≈ 4 ms/run | read, propose, diff, capabilities, explain, what_if, sim.run, snap |
| **seconds** | One loop | `check`/`verify`: 231 runs ≈ 1 s through the CLI | check, verify, sweep |
| **minutes** | Sampling, search, I/O | Monte Carlo at 20,000 runs ≈ 80 s at CLI speed; `solve`; datasheet extraction | solve, extract, statistics |

The 4 ms per run is an upper bound, dominated by process start-up. In-process runs will be much faster. For big circuits the classes stay; the absolute times grow.

### 4.2 The verdict record (sketch)

One record per spec × side × confidence. `spicy check --format json` emits exactly this, so the CLI and the future editor share one contract.

```json
{
  "schema": "spicy.check/1",
  "rev": "a3f91c",
  "records": [
    {
      "id": "bias/max/worst_case",
      "spec": "bias", "side": "max", "confidence": "worst_case",
      "measure": "dc(output.v)", "unit": "V", "limit": 6.5,
      "status": "verified", "verdict": "FAIL",
      "value": 6.5905, "margin": -0.0905,
      "bracket": { "inner": 6.5905, "outer": null },
      "counterexample": { "temp": "-10°C", "vcc.v": "12.6V", "r1.value": "+1%", "r2.value": "-1%",
                          "rc.value": "-1%", "re.value": "+1%", "q1.beta": 100 },
      "form_at_worst": { "taken_at": "counterexample", "center": 6.5905,
                         "coeffs": { "temp": "…", "q1.beta": "…" }, "miss_seen": 0.182 },
      "method": { "name": "worst_point_loop", "rounds": 2, "slopes": "L0" },
      "tags": [], "guards_fired": []
    }
  ],
  "cost": { "runs": 231, "backend": "spicy", "level": "L0" },
  "not_modeled": ["c_in: Electrolytic without life link (M6)", "no load or source-impedance assumption"]
}
```

`miss_seen` = 0.182 V is the walkthrough's line-vs-simulation gap at that corner (6.408 predicted, 6.590 simulated, walkthrough §5.5). Knob names are the language's paths (`r1.value`, `q1.beta`), which are also the names in `lang.read` slices. No UUIDs: "eschew low-level technical identifiers" (Anthropic, same post).

### 4.3 The `what_if` result (sketch, candidate C of flow 3.2)

```json
{
  "proposal": "p/fix-bass/C", "base_rev": "a3f91c", "status": "estimated",
  "edits": [ { "knob": "r1.value", "from": "47k", "to": "100k", "knob_widths_moved": 76 },
             { "knob": "r2.value", "from": "10k", "to": "22k",  "knob_widths_moved": 79 } ],
  "trust": "extrapolated far outside the knob box: interactions not modeled",
  "records": [
    { "id": "bias/max/sigma3", "estimate": 6.513,
      "inner_bound": { "value": 6.693, "point": "stored 3σ point of rev a3f91c", "proves": "FAIL" } },
    { "id": "bass/max/worst_case", "estimate": 16.38, "inner_bound": { "value": 16.885, "proves": null } }
  ]
}
```

`knob_widths_moved` is |Δ ln value| divided by the knob's half-range in ln units: ln(100/47) / ln(1.01) ≈ 76. Above about 1, the estimate is an extrapolation and must be shown as such.

### 4.4 Mapping onto MCP, later

The tools map one-to-one onto the Model Context Protocol, whose current spec is revision 2026-07-28 (https://modelcontextprotocol.io/specification/2026-07-28/server/tools):

- **Tools** are "model-controlled": all of §4.1 except `lang.apply`, which the host keeps behind the user's accept.
- **Resources** are "application-controlled": the design text and stored results.
- **Prompts** are "user-controlled": "review this design".

Each tool declares an `outputSchema`, and its `structuredContent` carries the JSON record. The spec also says servers "SHOULD also return the serialized JSON in a TextContent block". A second text block can carry the compact form of §5. Tool errors use `isError: true` with "actionable feedback that language models can use to self-correct", which fits our fix-it diagnostics.

**Not for the MVP:** the JSON contract is the seam, and an MCP server is a thin wrapper around it later.

---

## 5. What the agent reads: context and encodings

**Standing context** (sent every turn):
- The block and its contract as language text (the MVP file is 32 lines).
- One compact line per verdict record.
- The knob list with ranges.
- The project defaults (confidence, backend).
- The `not_modeled` list.

**On demand:**
- `explain` payloads;
- slices (select-to-ask);
- sweeps;
- datasheet pages.

**Encoding rules:**

1. **The design as language text.** It's what the agent edits, it carries the `///` rationale, and it compiles, so a malformed slice is caught (language_editor_mapping §8.1).
2. **Results as compact lines** in the model's context, with the JSON kept for machines and the UI. For the bass payload of §3.1 (`encoding_sizes.txt`):

   | Encoding | Characters |
   |---|---|
   | Compact lines (§3.1) | **364** |
   | Minified JSON, same content | 734 (2.0×) |
   | Pretty JSON | 904 (2.5×) |

3. **Names are language paths; units always.** Write `c_in=-19.9%`, `beta=189.8`, `31.67Hz`, never bare numbers.
4. **State the ε convention once** ("ε ∈ −1…1, + = high edge"), then write forms as `31.67 -7.91c_in +3.96life …`.
5. **Every form says where it was taken** (`form@worst`, `form@nominal`) and how far it missed when checked. §3.1 shows why: the nominal form misleads by 3.4 Hz.
6. **Round for reading** (3–4 significant digits). Ids, knob paths and `rev` stay exact.
7. **Never send the corner table.** Send the counterexample, the form, and the contributors. The 512-row brute force helps nobody reason.
8. **Offer `detail: concise | full`** rather than one size, following Anthropic's `response_format` suggestion. Their tools cap responses at 25,000 tokens by default (same post). Ours are far below that for the CE amp.

---

## 6. Guardrails

| # | Rule | Enforced by | CE-amp example |
|---|---|---|---|
| G1 | **No edit without a diff.** Every change is `propose` → the user reviews the diff and verdict deltas → `apply` with a user token | `lang.apply` needs the UI-minted token; `base_rev` must match | Flow 3.2: B is applied only after "B." |
| G2 | **No fix is called a fix until `verify` ran on that proposal.** Before that, only "≈ estimated" or "FAIL (proven at a re-simulated point)" | Statuses in every result, and the agent's output lint below | C's instant estimate looked borderline (6.51 V); verified, it's a clear FAIL (6.70 V) |
| G3 | **Verdict words come from records.** A lint on the agent's message: every PASS/FAIL/✓/✗ must match a `verified` record with the current `rev`; every number must appear in a tool result or be marked as the agent's own arithmetic | Post-processing of the agent's text, before display | "bass PASS" after applying B cites the record on rev 5c07e2 |
| G4 | **Units and E-series.** Values go through `lang.propose` (unit-checked, language §5.1) and `parts.snap` | The compiler; the snap tool | 449.2 Ω → 453 Ω (E96); 2.2 µF (E6), 100k/22k (E6) |
| G5 | **Part existence.** An MPN must come from `parts.lookup` or a document the user supplied. In the MVP (no catalog), the agent says "generic part, not checked against a catalog" | Tool result required for any MPN | — |
| G6 | **Requirement changes are never a fix.** Changing a limit, a confidence class, or deleting a spec is flagged `requirement_change` and proposed only when the user asks | `lang.propose` classifies hunks | Loosening bias to 6.6 V would "fix" bias: not allowed as a fix. Flow 3.5 reports the proxy and asks |
| G7 | **Provenance of AI-extracted data.** `reviewed = false`, `#[from(page, table, row)]` on every field; extraction notes list gaps and conflations avoided | Part-record schema; verdict tag relies-on-unreviewed-part-data | 2N3904: β ≥ 70 at 1 mA is tested; the max 300 is assumed from the 10 mA row |
| G8 | **Say what's missing.** For UNDECIDED, model-conditional, relies-on-typical and relies-on-unreviewed verdicts, the agent uses fixed phrasings (below) and never rounds to PASS | Tags in records; templates | Flow 3.6's bias FAIL rests on the unreviewed β = 70 |
| G9 | **Stale results are not results.** If `rev` changed, re-check before quoting | `stale` status | After applying B, every old record is stale |
| G10 | **Stop conditions.** Maximum proposals per request (e.g. 3), and a stop after 2 identical failing calls | Agent harness | atopile's circuit breaker does this (§7) |

**Fixed phrasings** (the agent fills the brackets from the record):

- **UNDECIDED:** "Not decided: the limit [6.5 V] lies between what was simulated ([x]) and what's estimated ([y]). I can't call this a pass. Running [method] would decide it."
- **model-conditional:** "PASS as far as the model can tell: [Q1's model] doesn't respond to [temp] for [β], so this verdict can't see that effect."
- **relies-on-unreviewed-part-data:** "FAIL, given [β ≥ 70] from [2N3903/D p. 2], extracted by AI and not yet reviewed. If that value is wrong, this verdict changes. Please check that row first."
- **worst_case FAIL, sigma(3) PASS:** "Fails only if [all six parts] sit at their worst edges together, [cold, on a high supply]. Whether that matters is your call." (walkthrough §6)

---

## 7. How others do it

**Sources.** Two research agents read primary pages for this note on 2026-09-27, by direct fetch (the web-search budget was spent). Numbers are as the sources state them. Pages were read through a fetch summarizer, not hand-checked against PDFs. Cadence's sites returned 403, so Cadence is thin here.

### 7.1 Industry

| Tool | How the AI touches the design | Simulation in the loop | Tolerance / worst case | Guardrails worth copying |
|---|---|---|---|---|
| **Flux** | Copilot "with your approval, make direct changes to your schematic", via an action button in chat (https://docs.flux.ai/reference/copilot.md) | ngspice; the AI generates a netlist, extracts metrics (−3 dB, ripple, gain), iterates on syntax/convergence errors (https://docs.flux.ai/reference/simulator-tool.md) | None mentioned | MCP server (Aug 2026): outside agents send requests to Flux's own agent; "Never assume a design change landed until a response confirms it" (https://docs.flux.ai/reference/flux-mcp-server.md). Grounding is "not bulletproof … always review its suggestions" (https://www.flux.ai/p/blog/flux-copilot-under-the-hood) |
| **atopile** | Sidebar agent; edits only through `project_edit_file` with `LINE:HASH` anchors from `project_read_file` | Build/solver checks; no simulation | Sets are "uncorrelated, even with themselves", so `X − X` isn't `{0}` | Skill file: "Verify after editing… Do not assume success", "Do not fabricate build results"; circuit breaker after 2 identical failures; the datasheet tool returns `source` and `page_range` (https://github.com/atopile/atopile, `.claude/skills/agent/SKILL.md`, `.claude/skills/solver/SKILL.md`) |
| **KiCad MCP** (mixelpixx) | 244 tools; validation "via `kicad-cli` on a copy" | None (SPICE netlist export only) | None | A hidden `execute_tool` dispatcher "was removed because the model then invented schemas it had never been shown"; a partial result "reported as success… is worse" than failing loudly (https://github.com/mixelpixx/KiCAD-MCP-Server) |
| **tscircuit** | Agent skill; loop "run tsci check placement … Run the build and checks again after each meaningful change" | `<analogsimulation>` via ngspice | None documented | Check after every change (https://docs.tscircuit.com/guides/circuit-generation/generating-circuit-boards-with-ai) |
| **circuit-synth** | Claude Code agents incl. `simulation-expert` | SPICE `.operating_point()`, `.ac_analysis()` | Nominal asserts ("3.3V ±5%") | Part-search results tagged by source (https://github.com/circuit-synth/circuit-synth) |
| **Synopsys ASO.ai** (IC) | RL optimizer | PrimeSim | "hundreds of PVT corners" (https://www.synopsys.com/ai/ai-powered-eda/aso-ai.html) | Not public |
| **Siemens Solido** (IC) | Assistant summarizes results | Solido engines | "self-verification technology" for high-sigma | Wiring not public |
| **JITX** | "Your approved AI edits that code" | "AI writes the code. HFSS validates." (https://www.jitx.com/) | — | A deterministic checker after the LLM |

**The gap:** no board-level AI tool we could read runs tolerance, corner or Monte Carlo analysis inside its loop, Flux's AI simulation included. That matches `engine_method_redteam.md` (§0 and the §3.1 summary table). Corner and statistical analysis appears only in IC tools (ASO.ai, Solido).

### 7.2 Academic LLM + simulator work

| Work | LLM does | Simulator / optimizer does | Corners / tolerance | Number |
|---|---|---|---|---|
| AnalogCoder, Lai et al., AAAI 2025, arXiv 2405.14918 | Writes PySpice code | ngspice; staged checks (operating point, DC sweep, function), errors fed back up to 3 times | None: "we prioritize the correct functionality" | Pass@1 66.1% on 24 circuits |
| ADO-LLM, Yin et al., ICCAD 2024, arXiv 2406.18770 | Proposes points, seeded by BO samples | GP-BO + HSPICE | Nominal only | 105 vs 405 simulations |
| AmpAgent, Liu et al., arXiv 2409.14739 | Derives formulas, initial W/L | ABC / TuRBO + Spectre | None | "1.32∼4×" fewer iterations |
| EEsizer, Liu & Chitnis, arXiv 2509.25510 | Calls ngspice through function calling | ngspice | Agent optimized nominal only; the authors' later 50-sample Monte Carlo: gain 78%, UGBW 76% pass | — |
| AutoSizer, Yu et al., arXiv 2602.02849 | Outer loop rebuilding the search space | Inner optimizer | None | Abstract: LLMs "are not suited for precise numerical optimization in AMS sizing" |
| White-box reasoning, Chen et al., arXiv 2508.13172 | Strategy with gm/Id tables | ngspice, SKY130 | TT corner, then all corners | TT specs "in 5 iterations" |
| **SABLE**, Li & Kim, arXiv 2607.03701 | Proposes changes through "a whitelist of 28 scoped SKILL entry points" and a JSON action contract with six stop conditions | Spectre, 3 corners, gated on the worst | Corners, no Monte Carlo | "The agent recomputes measurements and pass_fail authoritatively on the trusted side, so a model cannot talk its way into a pass." |
| D2S-FLOW, Chen et al., arXiv 2502.16540 | Extracts datasheet parameters into SPICE models | — | — | Exact match 0.86, F1 0.92: about 1 in 7 parameters not exactly right |

**Agent faithfulness:**
- An audit of a pre-release o3 found it "frequently fabricates actions it took to fulfill user requests" (Transluce, https://transluce.org/investigating-o3-truthfulness).
- A study of 1,600+ multi-agent traces lists "No or incomplete verification" and "Incorrect verification" as failure modes (Cemri et al., arXiv 2503.13657).
- Anthropic: agents must "gain 'ground truth' from the environment at each step", and "Poka-yoke your tools" (https://www.anthropic.com/engineering/building-effective-agents).

### 7.3 Where we follow precedent, and where we depart

| Decision | Precedent | Us | Why |
|---|---|---|---|
| A deterministic checker decides | SABLE, JITX/HFSS, atopile's "do not fabricate" | The engine decides; the agent's words are linted against records (G3) | Agents misreport tool results (Transluce, MAST) |
| Edits anchored to a revision | atopile `LINE:HASH`; Flux "confirm it landed" | Keyed statement patches (language_editor_mapping §8.2) + `base_rev` check | Names survive reformatting better than line hashes |
| Approve before apply | Flux (approve, then applied) | **Depart:** a diff **with verdict deltas** before accept | A fix that breaks another spec must show before acceptance (flow 3.2 C, flow 3.4 B) |
| Few, typed, workflow-shaped tools; fail loudly | Anthropic; KiCad MCP's dispatcher lesson | 15 tools in 4 namespaces; errors are fix-it diagnostics | Same |
| LLM proposes, optimizer computes numbers | ADO-LLM, AmpAgent, AutoSizer, LEDRO | The agent does back-of-envelope algebra; `solve` does search; the loop verifies | "not suited for precise numerical optimization" |
| Corners in the loop | SABLE (3 corners), ASO.ai | **Depart:** full range-corner safety net + worst-point loop + two confidence classes per spec | Our differentiator; nobody at board level does it |
| Uncorrelated ranges | atopile | **Depart:** named-knob affine forms keep correlation | The walkthrough §8 headroom example (4.81 vs 5.17 V) |
| Datasheet data | Flux: no public citation mechanism; atopile: source + page | `#[from(page, table, row)]` on every field, `reviewed = false`, verdict tags | D2S-FLOW's 14% non-exact rate; the 2N3904 traps |

---

## 8. What this changes in the current docs

1. **engine.md §8, "every edit: re-evaluation of stored affine forms for the changed values"** needs three refinements:
   - Plain forms extrapolate to nonsense for value edits (C_in 2.2 µF → −6.68 Hz). Scale knobs need log coordinates.
   - Forms without cross terms miss interactions: the divider edit turns a 3σ PASS into a FAIL the form calls borderline.
   - Re-simulating the stored worst points is cheap, was exact in every worst-case row of the fix-bass test, and proves failures even where a corner moved.

   The instant tier should be "log-form estimate + re-simulated inner bounds", labeled `estimated` / `inner_bound`.
2. **walkthrough §6, "fixes the engine can check instantly"** (2.2 µF → 15.2 Hz; film → 22.5 Hz): the numbers are confirmed (15.19, 22.52). But the first is instant only because f_L ∝ 1/C is exact in log coordinates, and the second needed re-simulation (the log form says 21.92).
3. **language.md §10 and language_editor_mapping §8.2–8.3** ("shown … together with how the verdicts would change, before you accept"): the verdict delta must carry a status. Before the loop finishes it's an estimate, and it must look different from a verified ✓.
4. **language_editor_mapping §8.1's contributors** (`life 46%, c_in 46%, beta 4%, temp 3%`) aren't reproducible without a stated definition (§3.1). Define contributors in M3a.
5. **roadmap M3e** prints a terminal table only. Add `--format json` with the §4.2 record (Recommendation 1).
6. **The part the docs name.** language.md §0 pins `onsemi::MMBT3904`, while the MVP file uses β 100..=300. The 2N3904 datasheet guarantees only ≥ 70 at 1 mA, and with that the MVP's bias spec fails at 3σ. (MMBT3904's datasheet not checked.)
7. **Cost at MVP size.** At 8 knobs the L0 loop costs 231 runs against 256 for brute force. The M3b answer key is cheap, and `verify` is seconds-class for the CE amp. At 9 knobs the loop already halves the cost (248 vs 512 runs), and brute force doubles with every added knob.

---

## 9. Recommendations for the MVP

Each item is small, decidable, and needed so an agent can be added later without redesign. The CLI is the first agent.

| # | Recommendation | Reason |
|---|---|---|
| 1 | **`spicy check --format json`** emitting the §4.2 record, versioned (`"schema": "spicy.check/1"`), snapshot-tested like the terminal table | The seam every later agent (CLI scripts, editor, MCP) uses. Costs a `serde` derive on the result types |
| 2 | **Every record carries** `status` (`verified` / `inner_bound` / `estimated`), `rev`, `side`, `limit`, `value`, `margin`, `counterexample` (pasteable, language knob paths), `method` + `runs`, `tags` | Agents do arithmetic badly and misreport. Give them the margin, not the parts to compute it (G3) |
| 3 | **`rev` = a hash of the elaborated model** (not the file bytes), printed by `check` | Results can be matched to designs; comment edits don't invalidate results; stale detection (G9) |
| 4 | **Persist the check's worst points and the forms taken there** (with each knob's physical value and half-range) to a results file keyed by `rev` | `explain` and `what_if` then need no re-run. Physical values let forms be re-expressed in log coordinates |
| 5 | **A re-simulate-at-a-point entry point** in `spicy_engine` (and `spicy check --at <corner>`), returning all measures and each device's region | The loop needs it anyway; it's `sim.run`, P3 of `what_if`, and "click the counterexample" (engine.md §3.5) |
| 6 | **Log coordinates for scale knobs** (R, C, β) in stored slopes, and when predicting worst points | P1 vs P2 in flow 3.2 (−6.68 vs 15.19 Hz). Also what walkthrough §5.4 recommends for β |
| 7 | **Define "contributors" in M3a** as recovery for statistical knobs (one run each, lazily) plus a range-knob removal line, and label the point | Flow 3.1: three definitions give 51%, 6.68 Hz or 98.9% for the same knob |
| 8 | **A `not_modeled` list in the output**, from part kinds vs MVP knobs (Electrolytic without life; no load assumption) | Flow 3.3: the review's most useful content; the engine knows it for sure, the agent only guesses |
| 9 | **Diagnostics as JSON too** (code, span, message, fix-it) | A broken patch gets a precise error to retry against (MCP's `isError` "actionable feedback") |
| 10 | **`what_if` as a library function** (P2 + P3, statuses, `knob_widths_moved`, `not_covered` for new knobs), CLI exposure optional | Cheap once 4–6 exist. Keeping its output statuses honest from day one prevents a UI from ever showing an estimate as a ✓ |
| 11 | **No LLM, MCP or chat code in M3** | The JSON contract is the seam; wrapping it as MCP later is mechanical (§4.4) |

---

## 10. Open questions

1. **P3's cost at scale.** With 50 specs × 2 sides × 2 confidences, re-simulating 200 stored points per keystroke is too much. Should P3 be limited to the specs whose forms depend on the edited knobs (structure tells us), and to on-demand runs otherwise?
2. **Out-of-box edits.** When `knob_widths_moved` is large, should `what_if` run one nominal simulation of the edited design as well? For the CE amp it's one 4 ms run, and it would have caught C's nominal shift.
3. **Who mints `user_token` in the CLI era?** An interactive `y/n` in `spicy apply`, or no apply at all until the editor exists?
4. **May the agent ever propose requirement changes unprompted?** Flow 3.5's proxy finding is valuable, but "loosen the spec" is the easiest fake fix. The current proposal: report, never propose, unless asked.
5. **Contract for long runs.** `verify` is seconds for the CE amp but minutes for a buck converter. Use an async handle, as MCP's non-normative "stateful tools" pattern suggests, and stream partial statuses (safety net done, spec 1 verified, …)?
6. **Distribution honesty in agent text.** engine.md §2.4 wants each σ3 verdict under both normal and uniform distributions. The MVP computes normal only, so should every σ3 record be tagged `distribution-sensitive` until M7?
7. **Where do extraction traps live?** (DC hFE vs small-signal hfe; test condition ≠ operating point; typical curves ≠ limits.) As a checklist the extraction tool enforces, or as reviewer prompts in the review view?
8. **How are chats bound to results?** Project-bound chats (vision §1) store record ids and `rev`s. What does the agent see when it reopens a chat whose results are stale?

---

## Appendix: scripts (scratch folder `…/engine_research/agent/`)

| Script | Computes |
|---|---|
| `agent_model.py` | Parametrized CE model (aging, split emitter, complex AC, swing limits) + a simplified loop (affine forms by nudging, worst-case loop, σ3 WCD loop, safety net, brute force, Monte Carlo) |
| `check_model.py` | Matches `ce_amp_model.py` at all 256 corners (max relative difference 0); complex solver = formulas |
| `flow1_explain.py`, `flow1_payload.py` | Flow 3.1: brute force, forms at nominal / worst / 3σ, contributors, Monte Carlo 177/20,000 |
| `flow2_fix.py` → `flow2_out.txt` | Flow 3.2: P1/P2/P3 vs verified for the three candidates |
| `loop_cost.py` | Flow 3.3: 231 runs (8 knobs) and 248 (9 knobs) for a full check; values = brute force |
| `flow4_gain10.py` → `flow4_out.txt`, `flow47_extra.py` | Flow 3.4: options A/B/B' verdicts; P3 for A; flow 3.7 temperature split |
| `flow5_swing.py` | Flow 3.5: headroom specs, loaded case, proxy check |
| `flow6_part.py` → `flow6_out.txt`, `q2n3904.spicy` | Flow 3.6: verdicts for β 70..300 / 70..400; the vendor model our parser rejects |
| `encoding_sizes.txt` | §5: compact vs JSON sizes |
