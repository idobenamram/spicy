# Spec Design v0.1: How Specs Are Defined

> Working draft · 2026-09-25
> Builds on `vision.md` (the I/O page), `engine.md` (the engine) and `walkthrough.md` (the worked example).
> The concepts here still hold. The syntax is superseded by `language.md` §8 (contracts), and the engine side by `engine.md`.

---

## 1. What a spec is

A spec answers five questions:

| Part | Question | Amplifier example (walkthrough §1) |
|---|---|---|
| **Measure** | What number? | V(out) / V(in) at 1 kHz |
| **Setup** | How is the block driven and loaded, and with which analysis? | AC source at `in`, 10 kΩ load, AC analysis |
| **Requirement** | What must the number satisfy? | 4.6 ± 5% |
| **Conditions** | Over which variations? | temp −10…60 °C, supply 12 V ± 5%, 10 years of aging, all part tolerances |
| **Confidence** | Absolute worst case, or a yield target? | 3σ (≈ 99.87% of boards, one-sided) |

It also carries metadata: a **name**, a **rationale** ("why this limit"), a **severity** (error / warning), and optionally a **source** (e.g. "next stage's input range").

**The engineer usually writes only the measure and the requirement.**

- Conditions come from the block's assumptions (§2) and from the parts' tolerances.
- Confidence comes from a project default.
- The setup comes from the block's default setup.

Everything else can be overridden per spec.

---

## 2. Where specs live: blocks as contracts

Every block (and so every schematic sheet, since each sheet is a block) has an **I/O page** that is a contract:

- **Assumptions:** what the block expects from the world around it.
  - Supply range, temperature, lifetime.
  - Input signal level, frequency band, source impedance.
  - Load.

  **Assumptions become the range knobs** for every spec in the block.
- **Guarantees:** what the block promises. **These are the specs.**

The project itself is the top block. Project-wide environment (temperature range, lifetime) is declared there and inherited by every block unless a block narrows it.

### Composition

When blocks connect, the engine checks each connection:

> **Upstream guarantee ⊆ downstream assumption**

Example: the regulator guarantees `vout in 12V ± 3%`, and the amplifier assumes `vcc in 12V ± 5%`. 12 V ± 3% fits inside 12 V ± 5%, so the connection is fine.

At system level the engine can go further. It can use the upstream block's **actual** result (an affine form in shared knobs) instead of its declared guarantee. That gives tighter answers and keeps correlations, for example when both blocks share the temperature knob.

This is an assume–guarantee contract model (Benveniste et al., *Contracts for System Design*, 2018; to be added to the bibliography).

**Loading caveat** (v1 §5.6): a block's guarantees depend on what's connected to it. The block's assumptions must include the source and load impedance it was checked with. A composition check verifies the neighbours actually present those impedances.

---

## 3. Conditions: the knobs

Recap from v2 §A7 and walkthrough §3:

| Kind | Comes from | Treatment |
|---|---|---|
| **Range** | Block assumptions and project environment: temp, supply, life, input level, load | Always at its worst value |
| **Statistical** | Part tolerances (datasheet or library) | Worst case, or yield, depending on confidence (§7) |
| **Solver-chosen** (`?`) | Values the engineer leaves open | The solver picks them so specs pass (v2 §A9) |

Part values can be linked to shared range knobs: tempco to temp, aging to life, DC-bias curves to the operating point. These links come from part models (AI-extracted from datasheets where needed).

Knob names follow instance paths (`amp.r1.tol`, `temp`). So two copies of a block get independent part knobs but share the environment knobs.

**Per-spec overrides:**
- Narrow conditions, e.g. "this spec only at 25 °C", or "only at beginning of life".
- Add conditions, e.g. "also over input amplitude 0…100 mV".

---

## 4. Measures

A measure is an expression over **probes**, evaluated from an **analysis**.

### Probes

- `V(net)`: voltage of a net to ground. `V(a, b)`: difference between two nets.
- `I(part.pin)`: current into a pin. `P(part)`: power dissipated in a part.
- Probes are named, and appear as markers on the schematic (§10).

### Analyses

| Analysis | Status |
|---|---|
| `op` (DC operating point) | first |
| `dc` sweep | first |
| `ac` | first |
| `tran` | after DC/AC |
| `noise` | later |
| periodic steady state | later, for switching converters |

### Measurement functions

A library like SPICE `.meas`:

- **Static / DC:** value, min, max over a sweep.
- **AC:** magnitude or phase at f, −3 dB frequencies, bandwidth, phase margin, gain margin, peaking.
- **Transient:** value at t, peak, peak-to-peak, average, RMS, rise/fall time, crossing time, overshoot, settling time, THD.

The engine prefers **smooth** measures, because sensitivities need them.
- Settling time jumps when a ringing peak crosses the band edge, and a warning is shown.
- Where possible the library offers a smooth equivalent (e.g. "envelope at t" instead of "settling time").

### Expressions

Measures use the same expression language as parametric values (vision §4), with units checked. Example: `P = V(vcc) * I(vcc.pos)`.

---

## 5. Setups (benches)

A setup is how the block is exercised:
- stimuli on its inputs,
- loads on its outputs,
- analysis settings.

**A default setup is derived from the assumptions.** Example: assume an input sine up to 100 mV from ≤ 1 kΩ, and a 10 kΩ load. Then the default AC setup puts a 1 kΩ source at the input and a 10 kΩ load at the output.

A block can have several **named setups**, e.g. `audio`, `power_on`, `load_step`. Each spec says which setup it uses, or uses the default.

A setup is also where block-level simulation meets the I/O page's "signal driving" (vision §3): the same definitions drive both.

---

## 6. Requirements

| Form | Example |
|---|---|
| Upper bound | `f_low <= 30 Hz` |
| Lower bound | `phase_margin >= 45°` |
| Range | `V(out) in 4.5V..6.5V` |
| Target ± tolerance | `gain in 4.6 ± 5%` |
| For all values of a sweep variable | `for f in 20Hz..20kHz: gain(f) in 4.6 ± 1dB` |

For-all requirements are how "loops for specs" work: the engine finds the worst point along the sweep (the worst frequency, worst load current, worst input level). Treating the sweep variable as a range knob gives the same answer. The difference is only in how the result is displayed (a curve with a band, instead of a single number).

Following atopile's distinction (bibliography §13):
- A **requirement** is "the value must stay **inside** this set" (⊆).
- A **part value** is "the part may be **anywhere** in this set" (⊇).

Both are written with ranges, but they mean opposite things, and the language and editor must keep them visibly different.

---

## 7. Confidence

| Level | Meaning | Typical use |
|---|---|---|
| `worst_case` | Every knob, including every part tolerance, at its worst edge | Safety-critical, space, medical |
| `sigma 3` (≈ 99.87%, one-sided) or `yield 99.9%` (≈ 3.09σ) | Range knobs at worst; statistical knobs treated statistically | Most specs |
| `nominal` | Only the nominal simulation | Quick checks, early design |

There is a project default (open decision: `sigma 3` vs `worst_case`), overridable per block and per spec.

The engine always **shows both** worst-case and realistic results when they disagree, as with S1 in walkthrough §5.6. The confidence level only decides which one sets the verdict.

---

## 8. Automatic specs

Specs the engineer gets without writing them. They come from part models and interface types. Each can be viewed, adjusted, or waived **with a written reason** (the waiver is part of the design record).

| Source | Examples |
|---|---|
| **Part ratings** (derating) | Capacitor voltage ≤ 80% of rated; resistor power ≤ 50% of rated; transistor VCE, IC and power within derated limits at the worst corner and max temperature |
| **Absolute maximum ratings** | Every IC pin within its abs-max voltage range, at every corner and during start-up |
| **Interface compatibility** | Connected digital pins: VOH(min) ≥ VIH(min) and VOL(max) ≤ VIL(max) with margin; an I2C bus has pull-ups and meets rise time for its bus capacitance and speed mode |
| **Power** | Each supply's load current within the source's guarantee; total power budget |
| **Contract checks** | Every connection between blocks: upstream guarantee ⊆ downstream assumption (§2) |

Heuristics that can't be checked as a number ("is there a decoupling capacitor near this IC?") are **lint warnings**, not specs.

---

## 9. Results

Every evaluated spec produces:

- **Verdict:**

  | Verdict | Meaning |
  |---|---|
  | PASS (guaranteed) | A proven outer bound is inside the spec (exact methods only: the corner theorem, datasheet arithmetic) |
  | PASS (all corners) | Every corner was simulated and passes, and the inside-the-box checks found nothing worse |
  | PASS (estimated) | The worst point found passes with margin beyond the search's observed error (the `sigma(3)` search, later the loop) |
  | PASS (implied by worst case) | A `sigma(3)` side whose `worst_case` already passes: the 3σ region lies inside the tolerance box |
  | FAIL | A simulated, reachable point violates the spec (always definite, with its counterexample) |
  | UNDECIDED | The engine can't say, and it names the reason and what would settle it (`next`) |

  These are decision D-E (`engine_plan.md` §3, accepted 2026-09-27). There's no "verified". See walkthrough §4.
- **Bracket:** inner bound (values that really happen) and outer bound (guaranteed or estimated).
- **Method:** every corner enumerated (the MVP, ≤ about 12 knobs per spec side) / the 3σ-point search / guaranteed (affine arithmetic, later) / the worst-point loop (after the MVP) / Monte Carlo k of N; always with its run count.
- **Top contributors:** which knobs use up the margin, split into range and statistical.
- **Counterexample** (for FAIL): the exact knob settings, one click away from a simulation at that point.
- **Worst-case and realistic values** side by side when they differ (§7).

In the MVP, derived measures are evaluated per run, which keeps shared knobs from being counted twice (walkthrough §8). **Affine forms** in named knobs (v2 Part B) come back when results must be combined or reused at system level without re-simulating (§2): decision D-F, `engine_plan.md` §10.

---

## 10. How specs look in the schematic editor

- **Probes on the schematic.** In simulate mode (vision §2):
  - click a net to probe its voltage,
  - click two nets for a difference,
  - click a part for its current or power.

  The probe gets a name and a marker.
- **Status in place.** Each probe shows its specs' status next to it: ✓ / ✗ / ?, and the worst value. This matches the vision's "results appear on the schematic itself".
- **The I/O page** has three panels:
  1. **Assumptions:** the block's range knobs, stimuli and loads, with setups.
  2. **Spec table:**

     ```
     ┌ Amp · Specs ──────────────────────────────────────────────────────────────┐
     │ name  measure              analysis  require      confidence  status      │
     │ bias  V(out)               DC        4.5…6.5 V    3σ          ✓ 6.28      │
     │ gain  V(out)/V(in) @1kHz   AC        4.6 ± 5%     3σ          ✓ 4.52…4.66 │
     │ bass  f_low(−3 dB)         AC sweep  ≤ 30 Hz      99.9%       ✗ 1.1% fail │
     │ auto: C_in voltage         DC        ≤ 80% rated  worst       ✓ 18%       │
     └───────────────────────────────────────────────────────────────────────────┘
     ```
  3. **Automatic specs:** grouped and collapsible, each with a waive option.
- **Selecting a failing spec** highlights the parts that contribute most and offers the counterexample. It's also the natural context for "select to ask" (vision §1): the AI gets the spec, its result, the contributors, and the counterexample.

---

## 11. How the engine evaluates a spec

Summary of v2 Part B, as applied to specs:

| The measure depends on… | Method | Verdict can be |
|---|---|---|
| Hand-written formulas only | Affine arithmetic with splitting | PASS (guaranteed) / FAIL |
| A linear part of the circuit only | Corner theorem, with directions proven | PASS (guaranteed, exact) / FAIL |
| Anything through a nonlinear device, ≤ about 12 knobs per spec side (the MVP) | `worst_case`: every corner, plus an interior check at the worst one. `sigma(3)`: the 3σ-point search from two starts (`engine_plan.md` §2) | PASS (all corners) / PASS (estimated) / PASS (implied by worst case) / FAIL / UNDECIDED |
| The same, with more knobs (after the MVP) | Worst-point loop: nominal → slopes → predicted worst point → simulate → re-linearize, gated on agreeing with enumeration | PASS (estimated) / FAIL / UNDECIDED |
| (sign-off) | Monte Carlo at the worst range corner | "k of N fail", with a confidence statement |

**When it runs** (`engine_plan.md` §1.1):
- **In the MVP:** only on `spicy check`, a full cold check (about 0.4 s for the CE amp on ngspice).
- **In the editor:** a re-check on save or idle. There is no per-edit estimate from saved slopes: re-checking is fast enough, and stored lines were badly wrong for real edits (`research/engine_synthesis.md` C11).
- **On demand:** transient specs.
- **At sign-off:** Monte Carlo.

---

## 12. Text form

The syntax is `language.md` §8 (v0.2, the syntax of `research/contract_syntax_v5.md`). The concepts above map onto it like this:

```rust
env ambient: Temperature in -10°C..=60°C;       // a range knob of the whole project
env life: Duration in 0y..=10y;

/// The world the block is checked in: its setup's ranges are range knobs.
setup Operating for Amp {
    vcc: Supply { v: 12V ± 5% },
    input: Signal { v: 0V..=100mV, z: ..=1kΩ },
    out: Load { r: 10kΩ },
    temp: ambient,
}

contract Amp {
    setup = Operating;
    let h = ac(out.v / input.v);

    /// ±1 V swing both ways.
    pub spec bias: dc(out.v) within 4.5V..=6.5V;
    spec gain: h.at(1kHz).mag() within 4.6 ± 5%;
    #[confidence(yield(99.9%))]
    spec bass: h.f_low(-3dB) <= 30Hz;
}
```

Transient specs (`tran(…)`, `thd`), confidence other than the default, and `life` as a knob are outside the MVP (`syntax_v5_plan.md`).

---

## 13. Open decisions

1. **Default confidence:** `sigma 3` or `worst_case`.
2. **Setups:** in the block's assumptions, or in separate testbench blocks that instantiate the design (the Cadence ADE style)? **Decided by v5:** a setup is its own item, `setup S for X { … }`, and a contract names its default one (`setup = S;`). It sets what's around the block without instantiating it.
3. **Probe identity:** probes named by net (`V(out)`), or first-class named objects that survive renames?
4. **Default distribution** for statistical knobs when a datasheet gives only min/max (e.g. transistor β): truncated normal with σ = range/6, or uniform?
5. **Beginning vs end of life:** one spec over a `life` range knob (proposed), or separate specs?
6. **How far automatic specs go** before they become noise: which are on by default?
