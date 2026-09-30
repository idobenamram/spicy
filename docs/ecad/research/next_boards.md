# Next Circuits: Hierarchy, Composition and Whole Boards

> 2026-09-28 · One of five "next circuits" reports. It tests the current design (engine.md v4, engine_plan.md, engine_types.md M3a draft, language.md v0.1 with roadmap §4.1, model.md E1–E26) on designs built from blocks, not on one block.
> **Where the numbers come from.** Every number marked **[B]** was computed on ngspice-42 (libngspice, KLU build) by the stdlib-Python scripts in `/root/.claude/jobs/443154a8/tmp/engine3/boards/` (`lib.py`, `e1_twostage.py`, `e2_compose.py`, `e3_arith.py`), using the plan's reference configuration (plan §2.1: IS 1e-14, XTB 1.5, TNOM 25 °C, engine tolerances). Other numbers are cited by doc and section. Round 1 already covered block composition in general (engine_method_redteam §2.13 "A13", §2.8 "A8") and hierarchical export of affine forms (engine_precision_analog §4.4). This report doesn't repeat them. It asks what breaks in *today's* design.

---

## Summary

I placed the MVP amplifier inside bigger designs and pushed each one through the language, elaboration, the export, the engine and the M3a types, on ngspice. Four designs: the CE amp placed twice as a two-stage amplifier; two stages fed by a real supply block; the stereo pair from model.md §1 with a channel-matching spec; and a 4-channel, 50-knob board. The language and flatten handle the structure well. Placement, paths, per-placement knobs and probes through placements all work as designed. The engine is where it breaks, in three ways.
1. **The M3 cone rule is only the capacitor rule.** Under it, every DC spec on a board has every non-capacitor knob in its cone: 42 of 50 on the board. Every board side then goes UNDECIDED (budget). The plan's claim that "a two-stage amplifier's local bias sides (7 knobs each) still enumerate" (plan §2.3) needs the pinned-node rule, which the plan defers to the loop (plan §6.3, §10.2). With that rule, local bias cones are 7 knobs on any board.
2. **Every AC spec of a two-stage amplifier is over budget.** Stage 2 loads stage 1 (its input is 7.4–8.1 kΩ **[B]**), so stage 1's in-context gain, the end-to-end gain and f_low all have 14 knobs: 16,384 corners, 11.9 s serial **[B]**. The exact answer took 14 s on one worker **[B]**. Under D-D that's UNDECIDED (budget) on the first circuit after the MVP.
3. **Contracts compose wrongly without loads, and aren't checked at all below the top.** Multiplying the two stages' own gain guarantees gives 19.1–23.3. The true end-to-end gain is 12.27–13.93 **[B]**, because each stage was checked unloaded. And model.md E24 analyzes only the top block's contract, so placing `CeAmp` in a parent silently stops checking `CeAmp`'s specs.

What helps is cheap if done now. Add the pinned-node rule to M3. Give requests a "care set" so disjoint local cones can share runs: 8× fewer runs on the board **[B]**. Make `FlatContract` a list of contract instances with placement paths. Make the default bench data the engine can see. Add a `rests_on` field to records and a cone fingerprint per side. Without affine forms (D-F), a **corner join** does the evidence ladder's middle rung. It joins per-block run tables on the shared range knobs, with the load as an interface knob. It enclosed the exact two-stage gain (12.2508–13.9730 vs 12.2681–13.9299) with 768 runs instead of 16,384 **[B]**.

---

## 1. The four circuits

| # | Circuit | Why the CE amp doesn't test it | Knobs | Sides |
|---|---|---|---|---|
| B1 | **Two-stage amplifier**: `CeAmp` placed twice, stage 2 AC-coupled onto stage 1's collector | Block reuse, per-placement knobs, one stage loading another, specs on both levels | 14 | 5 top + 2×5 in context |
| B2 | **Supply + consumers**: a capacitance-multiplier rail block feeding B1 | A `Power<Out>` block, a rail with source impedance, guarantee ⊆ assumption at a connection | 18 | 7 + the connection checks |
| B3 | **Stereo pair** (model.md §1's `Stereo`) with a channel-matching spec | A spec across two placements; matched parts (lots); correlation through shared temperature and supply | 14 (18 with lots) | 5 per channel + 2 matching |
| B4 | **4-channel board**: B2's rail feeding four B1 channels | 50 knobs, 40–76 sides, report and agent ergonomics, incremental re-checks | 50 (+3 rail) | 36 top-level, 76 with sub-block specs in context |

Each block is the MVP's `ce_amp.spl`, unchanged. That's deliberate: every difference in the verdicts comes from composition, not from a new device.

---

## 2. B1: the two-stage amplifier

### 2.1 In the language (v0.1)

```rust
block TwoStage {
    port v12: Power<In>;
    port gnd: Ground;
    port input: Analog<In>;
    port output: Analog<Out>;
    net mid;

    let s1 = CeAmp { vcc: v12, gnd, input, output: mid };
    let s2 = CeAmp { vcc: v12, gnd, input: mid, output };
}

contract TwoStage {
    assume temp in -10°C..=60°C;
    assume v12.v in 12V ± 5%;

    let h = ac(output.v / input.v);
    spec bias1: dc(s1.output.v) in 4.5V..=6.5V;      // a probe through a placement: works via aliases (E15)
    spec bias2: dc(s2.output.v) in 4.5V..=6.5V;
    spec gain:  h.at(1kHz).mag() in 13.2 ± 10%;
    spec bass:  h.f_low(-3dB) <= 40Hz;
}
```

**Where the syntax runs out:**
- **`CeAmp`'s own contract** (`bias`, `gain`, `bass`, and its two `assume`s) has no place here. The parent has to copy `bias` by hand as `bias1`/`bias2`. There's no way to say "check `s1` against its own contract, in context" (see §6.1).
- **No loads.** `assume output.z_load >= 10kΩ` (language §0) is post-MVP (roadmap §4.1). But it's exactly the assumption that stage 2 breaks (§2.4).
- **No block params** (`param`, language §7.1). A chain whose stages need different values means a copy of `CeAmp` per variant.
- **`s1.base.v` isn't reachable** from the parent: other blocks see only ports and `pub` items (language §8.1). Internal probes in context need the child's own contract.

### 2.2 Elaboration

Flatten does what model.md §1 and §9 say. There are 14 knobs: `temp`, `v12.v`, then `s1.r1.value` … `s1.q1.beta` and the same for `s2`. There are 8 nets, and `mid` has the alias `s1.output`/`s2.input`. One gap: `CeAmp`'s `assume vcc.v in 12V ± 5%` and `assume temp …` are resolved but dropped (E24). Nothing checks that the parent's `v12.v` range fits inside the child's assumption, or that the parent's temperature range fits the child's (language §8.2 "narrowing").

### 2.3 Export

It's flat, so the plan §5.1 deck carries it unchanged: 12 resistors and capacitors, 2 BJTs, each with its own `.model QM_s1_q1` / `QM_s2_q1`, and 14 `.param`s. The probe map must list aliases: `s1.output` and `s2.input` are both node `mid`. `ProbeMap` (plan §5.1) says "net path → node name", so it should map every alias, not only the winning name.

### 2.4 The engine

**Nominal [B]:** VC1 = VC2 = 5.5032 V (same as the MVP), |H(1 kHz)| = 13.2318, f_low = 25.729 Hz. Stage 1's in-context gain is **2.8823**, against **4.5908** standalone. Stage 2's input impedance at 1 kHz is **7,928 Ω** nominal, and **7,445–8,145 Ω** over its 256 corners.

**Exact extremes over all 16,384 corners [B]:** gain 12.2681–13.9299; f_low max 34.018 Hz; VC1 and VC2 each 4.6921–6.5595 V (exactly the MVP's bias key, so stage 1's bias doesn't see stage 2). Time: 14.0 s serial at 0.854 ms/run.

**Cones and routing** (plan §2.3's rule: 2^|cone| ≤ 4096 and 2^|cone| × the nominal run time ≤ 3 s; run costs measured **[B]**: op 0.153 ms, op + 1 kHz 0.214 ms, op + 1 kHz + sweep 0.725 ms):

| Side | Cone, capacitor rule (M3) | Cone, pinned-node rule (scale §6.1) | Routing in M3 |
|---|---|---|---|
| bias1, bias2 | 12 (every non-C knob): 4096 corners × 0.725 ms = **2.97 s** | 7 | Enumerates by 30 ms of margin; any slower machine or load and it's UNDECIDED (budget) |
| gain, bass (end to end) | 14 | 14 | **UNDECIDED (budget)**: 16,384 corners, 11.9 s |
| s1's gain in context (`mid/in`) | 14 | 14 (stage 2's input loads it) | UNDECIDED (budget) |
| s2's gain in context (`output/mid`) | 14 | 14 structurally, 8 in fact (`mid` is a cut node) | UNDECIDED (budget) |

So under D-D, the first circuit after the MVP gets UNDECIDED (budget) on every AC side. On 16 workers, 2^14 runs would take about 1.3 s (9.4× speed-up, scale §3.3), so the loop isn't the only way out (§7).

### 2.5 Composition: the evidence ladder on B1's gain

| Rung (engine.md §7) | How | End-to-end gain | Sound? | Runs |
|---|---|---|---|---|
| 1. Declared guarantees | `CeAmp.gain in 4.6 ± 5%`, squared | 19.10–23.33 | **No.** It's 40% high, because the guarantee was checked unloaded | 0 |
| 2a. Characterized, unloaded | Stage extremes over corners, squared | 19.91–22.12 **[B]** | **No**, for the same reason | 256 |
| 2b. **Corner join** with a load interface knob | Stage 1 characterized with `R_load ∈ [7,445, 8,145] Ω` (AC-coupled) as a range knob; per (temp, supply) corner, g1's extreme × g2's extreme; take the extreme over the corners | **12.2508–13.9730** **[B]** | Yes: it encloses the truth, 0.017 / 0.043 wide of it | 512 + 256 |
| 3. In-context simulation | All 2^14 corners | 12.2681–13.9299 **[B]** | Exact at corners | 16,384 |

What this shows:
- **Rung 1 is unsafe until contracts carry loads.** A composition check would have caught it: stage 2's input impedance is 7.4–8.1 kΩ, and language §0's `assume output.z_load >= 10kΩ` fails. But the MVP can write neither the assumption nor the input-impedance measure (no current probes, roadmap §4.1).
- **Rung 2 doesn't need affine forms.** The corner join reads the per-cone run tables the plan already builds (T6), keyed by physical values, and joins them on the shared range knobs, just like the cascade key (scale §2.2). It's sound when the composed measure is monotone in each block's result (a product of positive gains is), and when the interface knob's effect is monotone over its range. Here the load was modeled as a resistor, which is an approximation of the real (slightly complex) 1 kHz input impedance. Decorrelating `R_load` from g2 (both depend on stage 2's β) is what makes it slightly pessimistic. It proves PASSes. It never proves a FAIL (P2): a FAIL needs a rung-3 run.

---

## 3. B2: a supply block feeding consumers

### 3.1 The circuit and contract

A capacitance multiplier (the classic audio pre-regulator): R_b 10 kΩ ± 1% into C_b 100 µF, an NPN pass transistor (β 100..=300), and C_out 100 µF, fed from a raw 15 V ± 5%. It feeds B1.

```rust
block Rail { port raw: Power<In>; port vout: Power<Out>; port gnd: Ground; net pb;
    let rb = Resistor { a: raw, b: pb, value: 10k ± 1% };
    let cb = Electrolytic { p: pb, n: gnd, value: 100uF ± 20% };
    let qp = Npn { c: raw, b: pb, e: vout, beta: 100..=300 };
    let co = Electrolytic { p: vout, n: gnd, value: 100uF ± 20% };
}
contract Rail {
    assume raw.v in 15V ± 5%;
    // assume vout.i in 0mA..=5mA;           ← not writable: no current probes/assumptions in v0.1
    spec vout_dc: dc(vout.v) in 13V..=15V;   // the guarantee consumers rely on
}
```

**Where the syntax runs out:** `vout.i` (the sinks' summed current, language §3.2 `#[by(Sink, sum)]`), `i_max`, `z_src`. Without them, the power connection check "sum of sink currents ≤ the source's `i_max`" (language §3.2) can't be written.

### 3.2 What it does to cones

**[B]:** the nominal rail is 14.117 V with 3.94 mA drawn. It sits under the pass transistor's VBE and R_b's I_B drop, so its source impedance is tens of ohms. With s1 at nominal, **stage 2's five DC knobs alone move VC1 by 2.54 mV** (5.9767–5.9793 V).

That's small, but it's there, and the plan's rule is "cones come from the circuit, never from a slope" (engine.md §5.2). The rail is no longer an ideal source, so no node is pinned. Every consumer's bias cone now holds every other consumer's knobs plus the rail's: temp, raw, 5 + 5 stage knobs, R_b and the pass β = 14 for B1's bias, over budget. On B4 it's all 50+ knobs. **On a real board, the pinned-node rule only helps where an ideal source sits, and a real board has one: at its input connector.**

### 3.3 The contract cut

What makes cones local again is rung 1 used on purpose. Check each consumer with the rail **replaced by its guarantee**: an ideal source swept over `vout_dc`'s range as a range knob. That pins the node again, so B1's bias cones are 7. Separately, check the rail with its load replaced by the consumers' current range. The consumer's PASS then **rests on** `Rail.vout_dc` (and the rail's PASS rests on the consumers' current guarantee). This is circular assume-guarantee reasoning. For a static operating point it's sound when the operating point is unique, which the engine can't always show on ngspice (plan §12 q. 5). So the report must show the dependency, and the loop is closed by one in-context rung-3 run at the decisive points.

**Needed and missing:** a way for the exporter to emit a deck with a placement replaced by a stub (plan §5.1's `engine_deck` takes the whole `FlatDesign`); a knob whose range comes from another side's result (a new provenance in engine.md §2.2: "guaranteed by `rail.vout_dc`"); current probes; and a verdict record that says what it rests on (engine.md §3.3's "PASS (conditional)", later).

---

## 4. B3: the stereo pair and a matching spec

```rust
contract Stereo {
    assume temp in -10°C..=60°C;
    assume v12.v in 12V ± 5%;
    let hl = ac(left_out.v / left_in.v);
    let hr = ac(right_out.v / right_in.v);
    spec balance: hl.at(1kHz).mag() / hr.at(1kHz).mag() in 1 ± 2%;   // writable in v0.1
}
```

This is writable today: a ratio of two measures is ordinary arithmetic (grammar.md §3; model.md E12), and per-run evaluation keeps the shared `temp` and `v12.v` correlated (D-F). **Lots aren't writable** (`lot(…)`, language §5.4, post-MVP), and neither is per-placement temperature (`#[env(temp = temp + 15K)]`, language §8.2: it parses as an attribute, but has no meaning yet).

**Numbers [B]** (the channels only share temp and supply, so per (temp, supply) corner the worst ratio is max g / min g over one channel's corners, which is exact):

| Parts | Worst gL/gR | balance ≤ 2%? |
|---|---|---|
| Independent ±1% resistors, independent β 100..300, shared temp and supply | **1.05148** (5.15%) | FAIL |
| Same, but treating L and R as independent intervals (temp and supply not shared) | 1.05403 | — (5% pessimistic of the true 5.148%) |
| Resistors from one reel per value (lot ±1%, per part ±0.1%), β independent | **1.01352** (1.35%) | PASS |
| + a matched transistor pair (both β at one edge) | **1.00428** (0.43%) | PASS |

What this shows:
- **The physically right model has more knobs.** Without lots, the balance cone is 14 knobs. With them it's 4 lot knobs + 8 per-part knobs + 2 c_in + 2 β + 2 range = 18. Either way it's over budget in M3. The information that decides the verdict (a lot) makes enumeration harder, not easier.
- A lot knob touches both placements. So it must be a knob of the *parent* (it's created where the two placements meet), not of either child. E16 creates knobs "at leaf part fields, one per placement", so it has no place for a knob shared across placements.
- This is diffamp's lesson (plan §8.3) at board level: the verdict depends on declared correlation, not on more simulation.

---

## 5. B4: the 4-channel board

Rail (B2) → four B1 channels (8 `CeAmp` placements). 50 knobs on an ideal rail **[B]** (2 range + 8 × 6), 53 with the rail block. Sides: 8 × 2 bias, 4 × 2 gain, 4 bass, 3 × 2 adjacent-channel balance, 2 rail = **36**; with every placement's own `CeAmp` contract checked in context, **76**.

**Run cost [B]:** one full run (op + 1 kHz + sweep) is **1.694 ms**, and op only is **0.548 ms**, 2.3× the two-stage's. Every run simulates the whole board, even for a 7-knob side, because requests are full points (plan §6.3).

**Routing, by cone rule [B]** (`e3_arith.py`):

| Side | Capacitor rule (M3) | Pinned-node rule, ideal rail | Real rail (B2), no cut | With the contract cut at the rail |
|---|---|---|---|---|
| 8 local bias pairs | 42 knobs: UNDECIDED (budget) | 7: enumerate, 0.22 s each | ≥ 44: budget | 7: enumerate |
| 4 channel gain/bass | 50 (AC reaches every knob not blocked by a pinned node) | 14: budget (27.8 s) | ≥ 17: budget | 14: budget |
| 3 balance pairs | 50 | 26: budget | budget | 26: budget |

**Packing, measured [B]:** the 8 local bias cones are disjoint except for temp and supply. Setting every stage's 5 DC knobs to the same corner index in one run makes **128 runs** (0.08 s here) serve all 8 cones. Every stage's VC range came out as 4.6921–6.5595 V, equal to its unpacked key. Unpacked it's 8 × 128 = 1,024 runs, 1.73 s. This works only if a request says which knobs it cares about. The plan keeps full points (plan §6.3, Appendix C #5, the open disagreement with scale), and the driver dedups only identical points (engine_types §5.2).

**Board yield [B]:** per-side `sigma(3)` doesn't add up to a board. 50 sides each exactly at their 3σ point allow up to **6.75%** of boards to fail (union bound; 6.53% if independent). 76 sides allow 10.3%. Round 1 found the same number (redteam A8). What's new is that v4's report prints 76 lines of "PASS" and says nothing about the board.

**Incremental:** editing `c2s1.rc` under the pinned-node rule touches 2 bias sides, channel 2's gain and bass, and 2 balance pairs: 8 of 36 sides. The other 28 could be `carried` (scale §7.2). Under the capacitor rule, every DC side contains it, so every side re-runs. `rev` is one global hash (engine_types §9.3), so nothing can be carried without a per-side fingerprint.

---

## 6. Through the rest of the stack

### 6.1 Language and elaboration

| Need | v0.1 / model.md | Status |
|---|---|---|
| Place a block, bind ports, per-placement knobs, probes through placements | E14–E16, aliases | Works |
| Check a child's contract in context | E24: top contract only; E3: a placed block isn't a root | **Missing**: adding a parent stops the child's checks |
| Child `assume` ⊆ parent condition | language §8.2, §8.7 "Connections" | Missing (dropped in flatten) |
| Loads, source impedance, current | `z_load`, `z_src`, `.i` | Post-MVP (roadmap §4.1) |
| Block params | language §7.1 | Post-MVP: copy-paste per variant |
| Lots across placements | language §5.4 | Post-MVP; no home in E16 |
| Per-placement temperature | language §8.2 `#[env]` | Parses, no meaning; the export would need per-device `dtemp` |
| Spec names across placements | `Record.spec: String` (engine_types §9.1) | `bias` appears 8 times on B4 |

### 6.2 Export (plan §5)

Flat decks carry hierarchy fine. Three gaps: the probe map must include every alias (§2.3); the **default bench is built in M1e lowering** (plan §1.3), so the engine's adapter (`from_model`, engine_types T1) can't see which nodes are pinned by bench sources, and it needs to for cones; and there's no way to export a deck with a placement replaced by a contract stub (§3.3). Vendor subcircuits (a `part` with `model: spice(…)`, language §6.8) would be one device in the influence graph with every pin coupled (scale §6.1). That's conservative but fine.

### 6.3 M3a types

| Type | Problem at board scale |
|---|---|
| `Plan`, `Side { spec: SpecName }` (T1) | No placement path, no contract origin |
| `Cone` (§2.3) | "The capacitor rule" only |
| `Request { point: Point }` (T3) | Full points, so no packing |
| `KnobSpec` (§2.1) | No provenance (part, assume, guarantee, lot) |
| `Verdict`, `Record` (T8, T9) | No "rests on" list; counterexample lists 43 `any`s on a 7-knob side of B4 |
| `CheckReport` (T9) | Flat list; no grouping; no board-level line |
| `rev` (§9.3) | Global only |

### 6.4 What the engineer and the agent will ask

- "Stage 1 passes its gain spec alone. Why is the chain 13, not 21?" The answer needs the in-context gain of s1 (2.88) beside its standalone gain (4.59), and the loading cause (7.9 kΩ < 10 kΩ). That's a record per placement *and* per definition.
- "Which channel is worst?" A grouped report sorted by margin.
- "Did my edit to channel 2 change anything else?" `carried` with cone fingerprints.
- "Is the board OK?" A board line: "36 sides; 0 FAIL; 4 UNDECIDED (budget); each side at 3σ, which bounds board loss at ≤ 4.9% for 36 sides."
- "Can I trust the rail's number?" Show the `rests_on` chain: consumer PASS ← `rail.vout_dc` PASS (all corners).

---

## 7. Issues, ranked

| # | Issue | Severity | Hits | Fix |
|---|---|---|---|---|
| 1 | The capacitor rule makes every board DC cone "every non-C knob" (42/50 on B4), so every board side is UNDECIDED (budget). Plan §2.3's two-stage claim holds only by 30 ms of margin (2.97 s vs 3 s), and only for 2 stages | **Blocks the circuit** | plan §2.3 vs §6.3/§10.2; engine_types §2.3 | Build scale §6.1's pinned-node rule in M3 (~150 lines, µs). It needs the bench's sources (#6) |
| 2 | Every AC side of B1 has 14 knobs: 16,384 corners, UNDECIDED (budget) under D-D | **Blocks the circuit** | plan §2.3, D-D | Make the budget time-only and parallel (plan §12 q. 8): 1.3 s on 16 workers; the corner join (rung 2b) for PASSes; the loop later |
| 3 | Only the top contract is analyzed; placing a block silently drops its specs and its `assume` ⊆ checks | **Risks a wrong verdict** (the report looks clean) | model.md E24, E3 | `FlatContract` = contract instances with placement paths; check each block definition standalone (once) and each placement in context |
| 4 | Composing guarantees without loads is unsound (19.1–23.3 vs true 12.27–13.93) | **Risks a wrong verdict** (once rung 1 exists) | engine.md §7; language §8.8 | No rung-1/2 composition across a port whose load isn't assumed and checked; requires `z_load`/`z_src` and current probes |
| 5 | Real rails un-pin every node: cones cover the whole board | **Blocks the circuit** (B2, B4) | engine.md §5.2 "cones from the circuit" | The contract cut (§3.3): stub a `Power<Out>` block by its guarantee; record `rests_on` |
| 6 | The default bench lives in M1e lowering, out of the engine's sight | Blocks #1 | plan §1.3, §5.4; engine_types T1 | A `Bench` value in `spicy_model` (sources per port and knob), or bench metadata in `EngineDeck` |
| 7 | Full-point requests: 8× the runs on B4's local sides | **Cost** | plan §6.3; engine_types T3, §5.2 | `Request.care: ConeId`; the packing merge can wait, but not the field |
| 8 | Matching specs need lots, which add knobs, and lots have no home in E16 | Blocks B3 (lots); risks a wrong verdict (a FAIL that's really a PASS) | model.md E16; language §5.4 | Knobs created at the placement where parts meet (`KnobOrigin::Lot`); the engine's budget counts a lot as one knob |
| 9 | Per-side `sigma(3)` doesn't bound board yield | **Risks a wrong verdict** (misread) | engine.md §3.6, §4.4 | A board line with the union bound, free |
| 10 | Report: 76 sides × 2 confidences, flat, spec names collide, counterexamples list 40+ `any`s | **Ergonomics** | engine_types §9.1 | Group by placement, show FAIL/UNDECIDED first, print cone knobs only ("43 others: any") |
| 11 | `rev` is global: an editor re-check at board scale re-runs everything | **Cost** | engine_types §9.3 | Store a per-side cone fingerprint now; `carried` later |
| 12 | No block params: every variant is a copy | **Ergonomics** | roadmap §4.1 | Schedule `param` right after the MVP |

---

## 8. Seams to add now

Each costs little in the M3a draft and would otherwise mean a later retrofit of types, schema or tests.

| Seam | Where | Cost now | Avoids |
|---|---|---|---|
| **Pinned-node rule** in the cone pass (with the capacitor rule), fed by the bench sources | new `influence.rs` beside `plan.rs` | ~150 lines + tests (scale §6.1's pass was 143 lines of Python) | Every board side UNDECIDED (budget) |
| **Bench as data**: `Bench { sources: Vec<(NetPath, BenchSource)> }` produced from the contract, consumed by M1e and by the adapter | `spicy_model`; `EngineDeck` | One struct; M1e reads it instead of building it | The engine guessing which nodes are pinned; the contract cut later has the same shape (a stub is a bench source) |
| **`FlatContract` as a list of contract instances**: `{ placement: Path, contract: BlockId, specs, assumptions }` | model.md E24, `flat.rs` | A `Vec` instead of one; M1d-5 hasn't been built yet | Rewriting M1d-5 and the adapter when sub-block checks arrive |
| **`SpecPath`** (placement + name) in `Side` and `Record` | engine_types §2.3, §9.1 | A type change before the schema has readers | Colliding names; a `spicy.check/0` → `/1` break |
| **`Request.care: ConeId`** (the knobs this request depends on) | engine_types T3 | One field; the driver may ignore it in M3 | The plan/scale disagreement settled without a retrofit; packing becomes a driver change |
| **`KnobSpec.origin`**: `Part(path) \| Assume(side of contract) \| Env \| Guarantee(SideRef) \| Lot(parent path)` | engine_types §2.1, `Knob` in `flat.rs` | One enum; M3 fills `Part`, `Assume`, `Env` | The contract cut and lots needing a new knob model |
| **`Record.rests_on: Vec<SideRef>`** (empty in M3) | engine_types §9.1 | One field | Conditional verdicts needing a schema change |
| **Per-side cone fingerprint** (hash of the cone's knob specs, the devices in its influence region, the measure and bound) | engine_types §9.3 | One hash per side, computed with the cone | `carried` and editor re-checks needing a cache rework |
| **Budget by time with the worker count in the estimate** | plan §2.3 rule | Rewording + P in the estimate | 14-knob cones stuck at UNDECIDED on a 16-core machine |
| **A board line** in `CheckReport`: sides, verdict counts, union bound over the passing `sigma(k)` sides | engine_types §9.1 | Arithmetic | The "76 PASS lines = a good board" misreading |
| **Suite case `twostage`** with its exact 2^14 key (14 s) | engine_types T11, plan §8.2 | One deck + key | Hierarchy regressions going unseen |

---

## 9. The ladder: what comes right after the MVP

**B1, the two-stage amplifier.** It reuses the MVP block unchanged, so every difference is composition. It's the smallest design where:
- a child is placed (flatten, aliases, per-placement knobs);
- one block loads another (the in-context gain is 2.88 against 4.59 alone);
- local cones need the pinned-node rule;
- AC cones cross the 12-knob budget (14);
- its exact key is cheap (16,384 runs, 14 s **[B]**).

**What it proves:** local sides stay local; in-context checks of child contracts work; a 14-knob side is decidable (parallel enumeration now, the corner join as a PASS path, the loop later).

**It depends on:** the pinned-node rule and bench-as-data (#1, #6); `FlatContract` instances (#3); the time-only budget with a worker pool (#2); M1d-5 contracts; nothing new in M1f.

**Then:** B3 (matching, which needs lots, M6), then B2/B4 (the contract cut, current probes, `z_load`, `rests_on`).

---

## 10. Open questions

1. **In-context vs standalone as the headline** for a child spec: when they disagree (s1's gain 2.88 vs 4.59), which is the child's verdict? Probably both, with the standalone one tagged "unloaded".
2. **Cut-node reasoning for ratio measures:** `v(out2)/v(mid)` doesn't depend on stage 1 when `mid` is the only connection. Is it worth teaching the influence graph (cone 14 → 8)?
3. **Circular assume-guarantee** (rail ↔ load current): accept with a uniqueness caveat and one in-context run, or require a monotonicity argument?
4. **The corner join's soundness conditions:** how does the engine check that the composed measure is monotone in each block result and in the interface knob, rather than assume it?
5. **Lots and the budget:** should a lot knob plus its per-part knobs be enumerated as a polytope (|lot + part| ≤ tol, redteam §2.5 A5) rather than a box?
6. **Default for sub-block checks in the editor:** every definition standalone on every save, or only on the definitions whose cones an edit touched?
