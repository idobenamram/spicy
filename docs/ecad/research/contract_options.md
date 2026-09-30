# Contracts, Rethought: Three Options Compared

> 2026-09-29 · Compares three designs for the contract layer (specs, setups, conditions, sources and loads, faults, hierarchy). Each design wrote the same reference board in full and checked key numbers on ngspice.
> Options: `contract_option_a_datasheet.md` (**A**), `contract_option_b_testbench.md` (**B**), `contract_option_c_interface.md` (**C**).
> Research behind them: `contract_references.md` (how tools do it), `contract_ee_practice.md` (the physics, the reference board), `contract_hierarchy.md` (reuse and caching), `contract_tool_gaps.md` (what tools can't do, G1–G19).

---

## 0. Summary

The three philosophies started far apart and landed close together. **About two-thirds of the design is common to all three**:
- sources on inputs, loads on outputs, each with its impedance;
- ranges and fixed test points written differently;
- every connection checked in both directions;
- cached child results keyed by the child's definition;
- faults as timed stimuli with during/after windows;
- one-offs marked when they leave the declared range.

That core is well supported by the research, and we can adopt it whatever we choose (§1).

The real choice is **where a spec lives and what the organizing unit is**:
- **A** puts it in a datasheet table;
- **B** puts it in a named bench;
- **C** puts it on the ports.

Each is strongest in a different place: A for reading and the spec-table UI, B for setups and faults, C for composition and big boards. A hybrid that takes C's port model, A's spec rows and B's bench templates looks strongest; see §6. It's a recommendation for you to weigh, not a conclusion.

---

## 1. What all three agree on (the common core)

| # | Idea | Why | Source |
|---|---|---|---|
| 1 | **An input is described by its source** (signal, impedance, disturbances such as ripple, droop, slow edges). **An output by its load** (R, C, current range, steps). "The previous stage misbehaves" is source behaviour, never a load on the input | Physics: every connection is a divider between source impedance and input impedance | ee §1 |
| 2 | **`Supply`, `Signal` and `Load` shapes** with ranges: `Supply { v: 4.4V..=5.25V, z: ..=0.5Ω }`, `Load { i: 0.1mA..=500mA, c: ..=10uF }` | The same vocabulary appears in all three and in datasheet headers | A §1.5, B §2.2, C §1.4 |
| 3 | **A block publishes its own port characteristics**: `z_out`, `z_in`, supply current, levels. They're guarantees, checked like specs | Needed for any reuse (the two-way check) | hierarchy §1; references §13 |
| 4 | **Three kinds of condition look different in the text**: a fixed test point (`= 30mA`), a range the spec must hold over, and a variable swept inside the measure (line regulation's VIN span) | Datasheets use all three; mixing them up is a known source of error | references §3 |
| 5 | **Defaults once, overrides per spec**: A's `conditions`, B's `home` bench, C's accepted environment. All three are the datasheet header, "unless otherwise noted" | Every tool that works well has this | references §2 |
| 6 | **The declared range replaces `assume`** and does two jobs: the range the block's own specs hold over, and an obligation checked on its neighbours | That double job is exactly what `assume` tried to do, badly named | references §13 |
| 7 | **Two-way checks at every connection**: the driver's published values must fall inside the receiver's accepted range, and the receivers' summed load inside what the driver accepts | The only sound basis for reusing a child's verdicts (hierarchy method b) | hierarchy §2 |
| 8 | **Cached child results keyed by the child's definition**, never its placement. Shared environment knobs (temperature, a rail) are **joined**, not taken separately | Joining fixed a false alarm (6.5255 → 6.3646 V; truth 6.3640) | hierarchy §2.3 |
| 9 | **Flat simulation is still needed** for board transients, faults, shared-rail interactions and confirming a FAIL | Cached tables are exact for small-signal AC, not for large-signal behaviour | hierarchy §2.4 |
| 10 | **Faults are timed stimuli** (a cable drop, a brown-out, hot-plug) with `during`/`after` windows, their timing a knob | No board tool has this (G19) | gaps §3 |
| 11 | **One-offs are allowed but marked** when they leave the declared range (`#[beyond]`, `#[outside]`), and their verdicts never feed composition | Flexibility without silent unsoundness | B §1.3, C §2.2 |
| 12 | **The engine gets one deck per distinct circuit**, scenarios on it, and knobs in five kinds: environment, per-part, interface (load and source ranges), bench-local (step size), fault timing | Matches the seams in next_synthesis T1/T2/E5 | A §4, B §5, C §4 |

**Findings all three surfaced independently** (from simulating the board):
- **A reversed USB connector** puts −1.53 V on the LDO input; a series Schottky fixes it. A and C catch it with the automatic absolute-maximum check, with no spec written.
- **The example's midscale spec was wrong:** two ±1% divider resistors move the ratio ±1%, not ±0.5%. Midscale then FAILs at worst case (−27/+26 codes) and passes at 3σ. It's a good demonstration that the engine catches hand-arithmetic mistakes.
- **The LDO's output impedance peaks at light load** (3.2 Ω at 1 mA, 1.46 Ω at 5 mA). A "≤ 1 Ω" guarantee must say "for loads ≥ N mA", and the parent must prove its rail never carries less.
- **The filter's source requirement vs the op-amp's output.** The filter's spec assumes a ≤ 10 Ω source, but the op-amp stage presents 73.5 Ω at 25 kHz. Composition correctly refuses to reuse the filter's verdict and falls back to a context run.

---

## 2. The three options, the same block

The LDO's contract, condensed from each option (full versions in each file, §2).

**A. Datasheet-style:** the contract *is* the table.

```rust
contract Ldo3v3 {
    ratings    { vin.v: -0.3V..=6.5V; }
    operating  {                                              // replaces `assume`
        vin:  Supply { v: 3.6V..=5.5V, z: ..=0.5Ω };
        vout: Load   { i: 0.1mA..=500mA, c: ..=10uF };
        temp: -40°C..=125°C;
    }
    conditions {                                              // "unless otherwise noted"
        vout: Load { step: Step { from: 5mA, to: 30mA, edge: 1us } };
        typical: temp = 25°C, vin.v = 5V, vout.i = 30mA;
    }
    specs {
        vout_dc:  dc(vout.v)                                in 3.3V ± 2%  typ 3.3V with vout.i in 0.1mA..=50mA;
        line_reg: dc(vout.v).sweep(vin.v, 4.4V..=5.25V).span()  typ 1mV  max 5mV  with vout.i = 30mA;
        psrr:     -ac(vout.v / vin.ripple).at(100kHz).db()      min 36dB typ 40dB with vin.v = 4.3V, vout.i = 50mA;
        load_step: tran(vout.v).during(vout.step).deviation()   max 50mV;
        z_out:    vout.z_out.band(10Hz..=1MHz).max()            max 2Ω            with vout.i in 5mA..=500mA;
        t_start:  tran(vout.v).after(vin.ramp).crossing(3.2V)   max 1ms           on StartUp;
    }
    fixture StartUp { start: zero, tran: 0ms..=2ms }          // "see Figure"
}
```

**B. Testbench-first:** the block publishes its ports; benches carry everything else.

```rust
pub block Ldo3v3: Regulator {
    port vin:  Power<In>  { i: ..=vout.i + 50uA };
    port vout: Power<Out> { v: 3.3V ± 2%, i_max: 500mA, z_src: ..=1Ω over 10Hz..=1MHz where vout.i >= 5mA };
    …
}
bench home for Ldo3v3 {                                       // replaces `assume`: the envelope + defaults
    temp: 0°C..=70°C;
    vin:  Supply { v: 3.6V..=5.5V, z: ..=0.5Ω };
    vout: Load   { i: Draw::dc(0.1mA..=500mA), c: ..=10uF };
}
bench line  for Ldo3v3: LineReg  { at: 30mA;                 spec reg <= 5mV; }
bench psrr  for Ldo3v3: Psrr     { vin_at: 4.3V; load: 50mA;  spec rejection.band(100Hz..=1MHz) >= 36dB; }
bench step  for Ldo3v3: LoadStep { from: 5mA; to: 30mA; edge: 1us; spec dip <= 50mV; spec bump <= 50mV; }
bench start for Ldo3v3: StartUp  { ramp: 100us;              spec t_up <= 1ms; spec peak <= 3.4V; }
```

**C. Interface-first:** the contract lives on the ports; setups are derived.

```rust
contract Ldo3v3 {
    accepts temp: -40°C..=125°C;
    port vin {
        accepts Supply { v: 4.3V..=5.5V, z_src: ..=0.5Ω, ripple: Ripple { vpp: ..=100mV, f: 100Hz..=1MHz } };
        event   power_up: Ramp { from: 0V, rise: 100us };
        presents i: ..=(vout.i + 50uA);
    }
    port vout {
        accepts Load { i: 0.1mA..=50mA, c: ..=20uF };
        event   load_step: Step { i: 5mA -> 30mA, edge: 1us };
        presents v: 3.3V ± 2%;
        presents z_out: ..=1Ω over 10Hz..=1MHz where vout.i >= 5mA;
    }
    spec line_reg: dc(vout.v).span(vin.v) <= 5mV where vout.i == 30mA;
    spec psrr:     ac(vout.v / vin.v).at(100kHz).db() <= -36dB where vin.v == 4.3V, vout.i == 50mA;
    spec dip:      tran(vout.v).deviation() <= 50mV under vout.load_step;
    spec start:    tran(vout.v).crossing(rising 3.2V).since(vin.power_up) <= 1ms under vin.power_up;
}
```

**Reading them side by side:**
- The **numbers and shapes are the same** in all three (the common core).
- **A** groups by kind of statement (ratings, operating, specs), as a datasheet does. You scan one table for every limit.
- **B** groups by setup. The one-line benches come from a template library (`LineReg`, `Psrr`, `LoadStep`), so a standard regulator gets its standard benches almost for free, and the editor can list standard benches you haven't configured yet.
- **C** groups by port. Everything about `vout` (what it accepts, what it guarantees, its events) is in one place, which is exactly what a parent needs when it connects to it.

---

## 3. Where each is strong and where it hurts

### 3.1 By situation

| Situation | A: datasheet | B: testbench | C: interface |
|---|---|---|---|
| **Reading a block's contract** | **Best:** one table, the familiar datasheet form | Specs spread over ports, `home` and benches; the spec table must be a generated view | Good for ports; board-level specs sit apart from them |
| **The editor's spec-table UI** | **Best:** the text is the table | A generated view | Port cards read like a datasheet pin table |
| **Many setups, fixtures, test circuits** | Board specs with several stimuli strain the row form | **Best:** a bench is exactly a setup; templates make standard ones one line | Setups are derived; extra ones need `bench` |
| **Composition and big boards** | Good: the parent reads the child's operating conditions. But scalar ranges are pessimistic for frequency-dependent ports | Good: containment against `home`; context re-runs of only the failing child | **Best:** every port is the interface; nets are joined, not added (midscale 2021–2074 codes joined vs 1903–2202 separately) |
| **Library IP and vendor parts** | **Strong:** a vendor part's datasheet rows can check its vendor model (G15) | Strong: templates per socket type | Strong: reusable environments (`Usb2LowPower`) hold a standard's numbers once |
| **Faults** | Grafted on: a `faults` section beside the table | **Strong:** a fault is a bench stimulus with a timed knob | **Strong at ports** (`vbus` may drop out); weaker for faults inside the board |
| **Quick experiments** | A new row, or a fixture | **Best:** a one-off bench, marked `#[beyond]` if outside `home` | A `bench { …, ..env }`, marked `#[outside]` |
| **A novice user** | Familiar if you read datasheets; many sections for a small block | Benches are concrete ("here's what's connected"); 6 new concepts, including generics | Port environments are abstract at first; defaults are written out by the formatter so nothing is hidden |
| **AI editing** | Good: one row per spec, easy to diff and generate | Good: one line per bench | Good: port-local edits |
| **Diffs and review** | Clear: a limit changes on its row | Clear: one-line limit diffs | A named environment can hide a change from the diff |
| **The simple CE amp** | Two sections; same length as today | 3 lines change (`assume` → `vcc: Supply { … }`) | Similar length |
| **Engine cost** | ~10,000 runs for the board (estimated, transient-dominated) | Similar; board benches stay flat and transient-heavy | Declared ranges inflate cones (the gain stage has 13 knobs, over the 12 budget) |

### 3.2 The honest weaknesses, one line each

- **A:**
  - Scalar port ranges are too pessimistic when impedance depends on frequency.
  - Multi-stimulus board specs don't fit the row form well.
  - Faults don't fit the datasheet metaphor.
  - Copying fixed datasheet test points can hide the ranges that matter (a lint flags it).
- **B:**
  - The spec table is a generated view, not the text.
  - `home` does two jobs (default setup and envelope).
  - The most new concepts of the three (sockets, instruments, templates, generics).
- **C:**
  - Containment is only decidable when both sides use the same impedance shape.
  - Cones grow with declared ranges.
  - Named environments add a layer where changes hide.
  - Board-only transient specs gain nothing special.

---

## 4. Hierarchy: what each does when the gain stage is placed on the board

| Step | A | B | C |
|---|---|---|---|
| What travels with the child | Its rows, restricted to its own test conditions | Its `home` specs, as monitors on the parent's runs (UVM-style); other benches stay home with cached verdicts | Its `presents` and specs, over everything it `accepts` |
| The check at each connection | The child's rows against the neighbours' `operating` lines, both ways | Each neighbour's published numbers inside the child's `home` | Driver `presents` ⊆ receiver `accepts`; summed receivers ⊆ driver's `accepts` |
| When the check passes | 28 child rows carry over, 0 runs | Cached bench verdicts carry over | The gain stage's verdicts carry over, 0 runs |
| When it fails | Cached tables or in-context runs; the report says why (the filter's ≤ 10 Ω source vs 73.5 Ω) | Re-run only that child's benches under the real context | Tables or flat runs; shapes that don't match → flat |
| Cache key | Child definition | Child bench (definition + bench) | Child definition |

All three implement the same theory (hierarchy method b, falling back to c and d). The differences are in bookkeeping and presentation, not soundness.

---

## 5. Gaps (G1–G19): what each improves

All three improve the core gaps the same way, because of the common core:
- **G1:** specs have a home in the design text.
- **G3:** setups are reusable.
- **G6:** a real worst case, via the engine.
- **G10:** traceable verdicts.
- **G14:** hierarchy is checked.
- **G19:** faults are declared.

The differences:
- **G15, model gaps:** A is strongest, because a vendor part's datasheet rows check its own model.
- **G3/G4, bench reuse and maintenance:** B is strongest (templates per socket).
- **G12/G14, interface assumptions never checked:** C is strongest (ports are the contract).
- **G19, faults inside the board, not at a port:** only a path in C; natural in B.

Each option's §6 has the full table.

---

## 6. A hybrid to consider

The options are more compatible than their names suggest. One combination keeps each one's strength:

| From | Take | Because |
|---|---|---|
| **C** | Port contracts: `accepts` (the source or load it tolerates) and `presents` (what it guarantees), plus events on ports | The best composition story, and it answers "what can connect here" in one place |
| **A** | Specs as **rows** with min/typ/max and conditions, grouped in one table per block; `ratings` for absolute maximums | The best reading and UI; the text *is* the spec table |
| **B** | **Named benches and bench templates** for setups the ports can't express (load steps, start-up, faults, fixtures), usable across a product family | The best setup reuse and fault story |
| all | The common core of §1 | Settled by the research |

What that might look like for the LDO (a sketch, not a proposal yet):

```rust
contract Ldo3v3 {
    port vin  { accepts Supply { v: 4.3V..=5.5V, z_src: ..=0.5Ω }; presents i: ..=(vout.i + 50uA); }
    port vout { accepts Load { i: 0.1mA..=50mA, c: ..=20uF };
                presents v: 3.3V ± 2%;  presents z_out: ..=1Ω over 10Hz..=1MHz where vout.i >= 5mA; }
    ratings   { vin.v: -0.3V..=6.5V; }

    specs {                                                    // one table, datasheet-style rows
        line_reg:  dc(vout.v).span(vin.v)            max 5mV   with vout.i = 30mA;
        psrr:      ac(vout.v / vin.v).at(100kHz).db() max -36dB with vin.v = 4.3V, vout.i = 50mA;
        dip:       tran(vout.v).deviation()          max 50mV  on step;
        t_start:   tran(vout.v).crossing(3.2V)       max 1ms   on start;
    }
    bench step:  LoadStep { from: 5mA, to: 30mA, edge: 1us }   // templates for what ports can't say
    bench start: StartUp  { ramp: 100us }
}
```

**The risk of a hybrid** is that it inherits every option's concepts. It has to be cut down deliberately, not just summed. That's the design work to do next if you like this direction.

---

## 7. Decisions for you

1. **Adopt the common core (§1) now?** It's independent of the choice below, and several items feed the M3a seams (benches, events, port quantities).
2. **Which organizing unit:** A (table), B (bench), C (ports), or the hybrid of §6?
3. **Faults:** as bench stimuli (B), port events (C), or both?
4. **The block syntax** is also on your list to revisit. Whichever option wins, the port declarations (`port vout: Power<Out>`) and the contract's port sections should be designed together, maybe as one item.
