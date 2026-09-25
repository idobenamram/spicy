# Specs, Contracts, Benches and Automatic Checks in the Language

> Design proposal v0.1 · 2026-09-25
> Builds on `../specs.md` (the five parts of a spec), `../walkthrough.md` (the CE amplifier and its numbers), `../archive/engine_v2.md` (the engine) and `../vision.md` (the I/O page).
> **Scope:** how specs, contracts, benches (setups) and automatic checks are written in the language, and how they appear in the schematic editor.
> **Not in scope:** the core language (items, ports, nets, literals, generics). A sibling proposal owns that. The circuit syntax here is a minimal placeholder, marked `// (core syntax: placeholder)`.
>
> References cite the cloned sources:
> - atopile at commit `619eda7`, cited as `atopile/<path>:<line>` (relative to `externals/atopile/`).
> - Spade at commit `177e5c4`, cited as `spade/<path>:<line>` (relative to `externals/spade/`).
> - PolymorphicBlocks at commit `588b73b`, cited as `PB/<path>:<line>` (github.com/BerkeleyHCI/PolymorphicBlocks).

---

## 0. Decisions at a glance

| # | Decision | Section |
|---|---|---|
| S1 | A block's contract is its own item, `contract Amp { … }`, next to `block Amp { … }`, the way `impl` sits next to `struct` in Rust. It holds assumptions, measures, specs and simple benches, and it is the text form of the I/O page. | §2 |
| S2 | **Your guarantee is my spread.** A range after `in` (or `<=`, `>=`) in `assume`/`spec` is a requirement (⊆). A range in a value position (`Resistor(47kΩ ± 1%)`) is a spread (⊇). Interface roles make the compiler reject `spec` on a quantity you don't control, and `assume` on one you do. | §3, §5.3 |
| S3 | **A measure is a value.** `let vc = dc(output.v);` has the same kind of type (`Volt`, an affine form in knobs) as any parametric value, so measured and hand-written quantities mix without double counting. | §6 |
| S4 | Probes are field accessors (`net.v`, `part.pin.i`, `part.p`). Analyses are functions (`dc`, `ac`, `tran`, `noise`). The measurement library is typed methods on their results, so the editor can offer the right measures after a `.`. | §6 |
| S5 | A spec is `spec name: measure relation bound [on bench] [where conditions];`. The rationale is a `///` doc comment. Confidence and severity are attributes. Everything else defaults. | §5 |
| S6 | Every spec-table column has **exactly one home** in the code, and a canonical formatter prints it. That is what makes table ↔ code editing lossless. | §5.1, §12 |
| S7 | Benches: an **automatic default bench** is derived from the assumptions. **Form benches** override a few ports (`..default`, Rust's struct-update syntax) and are edited as a form on the I/O page. **Sheet benches** (`bench X for Amp { … }`) are separate schematic sheets that instantiate the design, in the Cadence ADE style, for complex fixtures. | §7 |
| S8 | Every range knob is universally quantified, so "for all load currents" needs no syntax. Analysis axes are restricted with `.band(…)` and `.window(…)`, and `.vs(knob)` changes only the display. Rust `for` means only elaboration-time generation (`spec gain[i]`), as in Rust. | §8 |
| S9 | **Every automatic check is an assumption of a part or interface,** discharged by its context. Datasheet sections map directly: absolute maximum ratings → `absolute_max { }`, recommended operating conditions → `operating { }`, electrical characteristics → spreads. | §9 |
| S10 | Levels and waivers follow Rust's lint model: `#[allow(derating::capacitor::v_rated, reason = "…")]`, `#[expect]`, `forbid`, and a project `[checks]` table like Cargo's `[lints]`. | §9.6 |
| S11 | Interfaces carry **role-tagged quantities** with combine rules (`#[by(Source)] v`, `#[by(Sink, sum)] i`). Connection checks and the loading caveat (`z_src`, `z_load`) fall out of these declarations. | §9.2, §10 |
| S12 | Composition uses an evidence ladder: **declared** guarantees → **characterized** results (affine forms, with the downstream forms substituted for the upstream's port knobs) → **in-context** simulation. | §10 |
| S13 | Results never live in the source. They appear as inlay hints and diagnostics. A counterexample is printed as a pasteable `corner`. | §11 |
| S14 | Default confidence: `sigma(3)` for user specs and `worst_case` for automatic checks. This answers open decision 1 of `../specs.md`. | §5.5 |

---

## 1. Principles

1. **The contract belongs to the block; the bench belongs to the measurement.** A spec is a claim about the block. A bench is one way of checking it. They live in different places and point at each other by name.
2. **Your guarantee is my spread.** When the party that controls a quantity writes a range, it's a promise (⊆ on their side) and a spread for everyone else (⊇ on the user's side). Keywords make the side visible: `assume`, `spec`, `absolute_max` and `operating` are followed by `in` or a relation (requirement); values are followed by `=` (spread).
3. **A measure is a value.** It has units and a range, and the engine carries it as an affine form in named knobs. It can appear in formulas, in bounds, and in other blocks' checks.
4. **One mechanism for automatic checks.** Ratings, abs-max, logic levels and pull-ups are assumptions of parts and interfaces. The user doesn't learn a second system.
5. **Every table column has one home in the code.** If a value can be edited in the I/O page, there is exactly one place for it in the text, and the formatter decides the layout.
6. **If it looks like Rust, it behaves like Rust** (v1 §7.5). `for` iterates at elaboration time; it never samples a continuous range. `a..=b` is closed. Lint attributes mean what they mean in Rust.
7. **Results are not source.** Verdicts are shown in the source but never written into it.

---

## 2. Placement: `block` + `contract`

```rust
// ce_amp.spc                                     (core syntax: placeholder)
block CeAmp(vcc: Power, input: Analog<In>, output: Analog<Out>) {
    let r1   = Resistor(47kΩ ± 1%);
    let r2   = Resistor(10kΩ ± 1%);
    let rc   = Resistor(4.7kΩ ± 1%);
    let re   = Resistor(1kΩ ± 1%);
    let c_in = Capacitor(1µF ± 20%, aging: -20% at life.end);
    let q1   = Npn(hfe: 100..=300);
    // … nets …
}

contract CeAmp {
    // assumptions, measures, specs, benches (§3–§7)
}
```

**Why a separate item:**
- **Drawn blocks.** D6 says each block's circuit is either drawn or coded. A contract has no layout, so it round-trips losslessly between the I/O page table and text **whether the circuit is drawn or coded**. A drawn block keeps its circuit in the sheet and its contract in text.
- **The I/O page is one item.** One page maps to one item, which keeps diffs small and predictable.
- **Rust familiarity.** `struct` + `impl` is the pattern every Rust reader knows. As with `impl`, a contract can see the block's private internals (nets, parts). Other blocks see only ports and `pub` items.

**Convention:** the contract follows the block in the same file. A drawn block has `ce_amp.sch` + `ce_amp.spc` (contract only).

**Alternative considered:** specs inside the `block` body. This is simpler for coded blocks, but drawn blocks then need a second home anyway, and a long block mixes the circuit with its I/O page.

**The project is the top block.** Its contract states the product's operating envelope:

```rust
contract Top {
    assume temp in -10°C..=60°C;
    assume life in 0y..=10y;
}
```

---

## 3. Assumptions

An assumption is what a block needs from its surroundings. It plays two roles at once. This is the assume–guarantee duality of contract theory [BCN+18]:

| Seen from… | An assumption is… | The engine uses it as… |
|---|---|---|
| **Inside** the block | the set of environments the block must work in | **range knobs** for verifying the block's own specs |
| **Outside** (the context) | a requirement the context must meet | a **check** at each connection (§10) |

### 3.1 Environment: `temp`, `life`, user-defined

```rust
// std prelude
env temp: Temperature;     // global range knob; drives tempcos, Vbe, β(T)
env life: Time;            // global range knob; drives aging links (0 = new)

// project-specific
env altitude: Length;
```

- An `env` is one knob shared by every block, so everything driven by temperature moves together (walkthrough §3). Env knobs are declared once. Their ranges come from `assume` statements.
- `Top`'s contract gives the product range. **A block that says nothing inherits the range of its context.**
- A block **may** state its own range, e.g. a library block rated −40…85 °C:
  ```rust
  contract Ldo3v3 { assume temp in -40°C..=85°C; }
  ```
  Then:
  - **Standalone** verification (the block's own I/O page) uses −40…85 °C. This is the reusable guarantee.
  - **In-context** verification uses the context's range (−10…60 °C). The result is tighter and specific to the project.
  - A connection check confirms context ⊆ assumption. If the project said −40…125 °C, the instance would get an error: *"Ldo3v3 assumes temp ∈ [−40, 85] °C; used where temp ∈ [−40, 125] °C."*

**"Narrowing" has two legitimate meanings, and both are explicit:**

1. **The instance really sees a different environment.** It sits next to a hot regulator, or inside an oven. The parent maps the env at the instance site. The local knob stays an affine function of the global one, so the correlation is kept:
   ```rust
   #[env(temp = temp + 15K, reason = "next to the buck inductor")]
   let amp = CeAmp(…);
   ```
2. **One spec is only claimed over part of the range** (e.g. "at 25 °C only", "beginning of life only"). That's a spec condition (§5.4), not an assumption.

A block **cannot** silently narrow its context. Assuming 0…50 °C inside a −10…60 °C product is a connection error, because that is the whole point of the check.

### 3.2 Port quantities and roles

Ports have interface types (§9.2). Each interface quantity is **produced** by one role and **consumed** by the other. The `Power` rail voltage is produced by the source; its current is produced by the sinks. A block may `assume` only quantities its role consumes, and `spec` only quantities its role produces:

```rust
contract CeAmp {
    assume vcc.v in 12V ± 5%;        // CeAmp is a Power sink: the source sets v, so we assume it
    spec   vcc.i <= 2mA;             // …and we set our own draw, so we guarantee it
}
```

```
error[E-role]: `assume` on a quantity this block produces
  --> ce_amp.spc:4:12
   |
 4 |     assume output.v in 4.5V..=6.5V;
   |            ^^^^^^^^ `output: Analog<Out>`: this block drives `output.v`
   = help: a range on a quantity you drive is a guarantee: `spec output.v in 4.5V..=6.5V;`
```

This check is how the ⊆/⊇ distinction becomes more than a naming convention (§5.3).

### 3.3 Input signals are patterns

A signal assumption says which family of signals the block must accept. It is written like a **Rust struct pattern with range fields**:

```rust
assume input in Sine { amp: ..=100mV, freq: 20Hz..=20kHz };
assume input.z_src <= 1kΩ;           // source impedance of whatever drives us
```

Each range field becomes a range knob: `input.amp`, `input.freq`, `input.z_src`. The standard signal families are:

| Family | Fields | Typical use |
|---|---|---|
| `Dc` | `level` | bias inputs, references |
| `Sine` | `amp`, `freq`, `offset` | audio and analog chains |
| `Band` | `rms`, `band` | noise-like or wideband inputs |
| `Step` / `Pulse` | `from`, `to`, `rise`, `width`, `period` | control inputs, load steps |
| `Logic` | `rate`, `pattern` | digital inputs (levels come from the interface, §9.2) |

For AC analysis, amplitude doesn't matter (small-signal) and `freq` becomes the default AC band. For transient analysis, `amp` and `freq` are ordinary range knobs, so a clipping spec is checked at the largest assumed amplitude.

### 3.4 Loads and source impedance (the loading caveat)

```rust
assume output.z_load >= 10kΩ;        // whatever we drive presents ≥ 10 kΩ
assume output.c_load <= 100pF;       // …and at most 100 pF
assume vcc.z_src <= 1Ω;              // the rail is stiff
```

- `z_src` and `z_load` are ordinary interface quantities (§9.2), so the loading caveat (v1 §5.6) becomes a normal connection check (§10.3).
- Internally the engine parametrizes a load in **conductance** (`g_load ∈ [0, 100 µS]` for `z_load ≥ 10 kΩ`). This keeps "open circuit" at a finite end of the range, and a single element's conductance enters bilinearly, so the corner theorem applies (v2 A5).

### 3.5 From assumptions to knobs

| Written | Knob(s) | Kind | Default bench realization (§7.2) |
|---|---|---|---|
| `assume temp in -10°C..=60°C` | `temp` (global) | range | simulator temperature; drives tempcos |
| `assume life in 0y..=10y` | `life` (global) | range | drives aging links in part models |
| `assume vcc.v in 12V ± 5%` | `vcc.v` | range | DC source whose value is the knob |
| `assume vcc.z_src <= 1Ω` | `vcc.z_src` | range | series resistance |
| `assume input in Sine { amp: ..=100mV, freq: 20Hz..=20kHz }` | `input.amp`, `input.freq` | range | sine source; `freq` also sets the AC band |
| `assume input.z_src <= 1kΩ` | `input.z_src` | range | series resistance of the input fixture |
| `assume output.z_load >= 10kΩ` | `output.g_load` | range | load conductance |
| `assume mode in {Run, Sleep}` | `mode` | discrete range | enumerated |
| `Resistor(47kΩ ± 1%)` (not an assumption) | `r1.tol` | statistical | part parameter |

- **Knob names** follow instance paths (`amp.vcc.v`, `amp.r1.tol`), as in spec_design §3.
- **In context, port knobs disappear.** The engine substitutes the upstream's actual quantity for them (§10.2). Two amps on the same rail then share that rail's form, and so they are correlated.
- **Standalone, each port knob is independent.**

### 3.6 Lifting and inferring assumptions (the editor's minimal mode)

- **Lifting.** When a child's port connects to its parent's port, the child's assumption on it **lifts**: the parent's I/O page shows `vcc.v ∈ 12 V ± 5% (required by amp)` greyed out. If the parent states its own assumption, it must be ⊆ the child's (the parent must deliver what the child needs). If it doesn't, the parent's assumption is the **intersection** of what its children require.
- **Inferring.** A project-specific block may state **no** port assumptions at all. It is then verified in context only: its knob ranges come from the upstream guarantees or the upstream's actual results. The I/O page shows the inferred ranges greyed out, with a **pin** button that writes the `assume` line. This makes the common case zero-effort, and one click turns the block into a reusable, standalone-verifiable block.

A block with neither declared nor inferable assumptions on an input gets a lint (`contract::unassumed_input`, warn): *"`input` has no signal assumption; the default bench drives it with 0 V."*

### 3.7 Three tiers, straight from datasheets

Datasheets already separate three things, and the language keeps them apart. PolymorphicBlocks conflates the first two into one `voltage_limits` per port: its `AnalogSink` docstring distinguishes damage limits from functional limits (`PB/edg/electronics_interfaces/AnalogPort.py:201-202`), but digital pins use abs-max values while power pins use recommended values, and a `Nonstrict3v3Compatible` mixin exists to paper over the tension (`PB/edg/abstract_parts/Nonstrict3v3Compatible.py:6-14`).

| Datasheet section | Meaning | Language | Checked… |
|---|---|---|---|
| Absolute Maximum Ratings | survival | `absolute_max { … }` → `#[check(absmax)] assume` | in every analysis, every time point, **worst case** |
| Recommended Operating Conditions | the part works | `operating { … }` → `#[check(operating)] assume` | same, worst case |
| Electrical Characteristics | the part's own guarantees under test conditions | spreads (`let vref = 0.8V ± 1%;`), i.e. the part's statistical knobs | not checked: they're the model |

So **a datasheet is a contract**: ROC and AMR are the part's assumptions, and the EC table holds its guarantees. This is also the target format for AI datasheet extraction (§9.3).

---

## 4. The running example, complete

```rust
contract CeAmp {
    // ── Assumptions ─────────────────────────────── (temp, life: inherited from Top)
    assume vcc.v in 12V ± 5%;
    assume input in Sine { amp: ..=100mV, freq: 20Hz..=20kHz };
    assume input.z_src <= 1kΩ;
    assume output.z_load >= 10kΩ;

    // ── Measures ────────────────────────────────────
    let h = ac(output.v / input.v);            // transfer function, used twice

    // ── Guarantees ──────────────────────────────────
    /// Room for the output to swing ±1 V without clipping.
    spec bias: dc(output.v) in 4.5V..=6.5V;

    /// The next stage expects this level.
    spec gain: h.at(1kHz).mag() in 4.6 ± 5%;

    /// Don't cut the bass.
    #[confidence(yield(99.9%))]
    spec bass: h.f_low(-3dB) <= 30Hz;

    /// Inaudible distortion at full input.
    #[confidence(worst_case)]
    spec clip: tran(output.v).thd(1kHz) < 1% on loud;

    // ── Benches ─────────────────────────────────────
    bench loud {
        input: Sine { amp: 100mV, freq: 1kHz },
        tran: 0ms..=20ms,
        ..default
    }
}
```

What the editor shows next to the specs as inlay hints. The numbers are from walkthrough §9; nothing here is written to the file:

```
spec bias: dc(output.v) in 4.5V..=6.5V;            ✓ max 6.34 V (3σ) · worst-case 6.59 ✗ · temp, β
spec gain: h.at(1kHz).mag() in 4.6 ± 5%;           ✓ 4.52…4.67 (3σ) · RC + RE ≈ 80%
spec bass: h.f_low(-3dB) <= 30Hz;                  ✗ 31.7 Hz (3σ) · 0.9 % of boards · C_in
spec clip: tran(output.v).thd(1kHz) < 1% on loud;  ▶ not run (transient runs on demand)
```

The rest of this document explains each line.

---

## 5. Guarantees (specs)

### 5.1 One grammar; every part has one home

```
spec_item  := attrs 'spec' name ':' measure relation bound on? where? else? ';'
            | attrs 'spec' path relation bound on? where? else? ';'   // name = path (e.g. `spec vout.v in …`)
relation   := 'in' | '<=' | '>=' | '<' | '>'
on         := 'on' bench                  // default: `default`
where      := 'where' condition (',' condition)*
condition  := knob '==' expr | knob 'in' set
else       := 'else' "message"            // custom failure text (Spade's `where … else "…"`)
attrs      := '///' doc | #[confidence(..)] | #[warn] | #[expect(fail, reason = "..")] | #[req("ID")]
```

| Part / column | Home in the code | Default when absent |
|---|---|---|
| **Name** | `spec NAME:` | required (or the path, in the `spec path …` form) |
| **Measure** | the expression left of the relation | — |
| **Analysis** | the `dc` / `ac` / `tran` / `noise` call inside the measure | — |
| **Setup (bench)** | `on BENCH` | `default` (derived from the assumptions, §7.2) |
| **Requirement** | relation + bound | — |
| **Conditions** | `where …` | all knobs over their assumed ranges |
| **Confidence** | `#[confidence(…)]` | contract → project default (§5.5) |
| **Severity** | `#[warn]` / `#[expect(fail, …)]` | error (`deny`) |
| **Rationale** | `///` doc comment | none (a lint suggests one on failing specs) |
| **Traceability** | `#[req("PRD-012")]` | none |
| **Status** | *not in the code* (inlay hint, §11) | — |

### 5.2 Minimal writing

The common case is one line with a name, a measure and a requirement:

```rust
spec bias: dc(output.v) in 4.5V..=6.5V;
```

Everything else is filled in:
- **Bench:** the default bench from the assumptions: sources at the assumed levels and impedances, loads at the assumed loads.
- **Conditions:** all knobs, i.e. temp, life, supply, input, load and every part tolerance.
- **Confidence:** the project default.
- **Severity:** error.

For a port quantity, the path is the name:

```rust
spec input.z_load >= 20kΩ;           // "our input presents ≥ 20 kΩ to whatever drives it"
```

### 5.3 Relations, bounds, and ⊆ vs ⊇ made visible

| Form | Example | Meaning |
|---|---|---|
| Range | `in 4.5V..=6.5V` | value stays inside the closed interval |
| Target ± tolerance | `in 4.6 ± 5%` | same, centered form |
| Upper / lower bound | `<= 30Hz`, `>= 45°` | one-sided |
| Strict | `< 1%` | kept strict. atopile silently turns `>` into `>=` (v2 Part F); we don't. A value exactly on the edge is reported as UNDECIDED, never as a pass |
| Relative bound | `in h.at(1kHz).db() ± 0.1dB` | the bound is itself a measure |
| Curve in a band | `h.band(20Hz..=20kHz).db() in g0 ± 1dB` | holds at every frequency in the band (§8.1) |

**Rule:** everything in one spec is evaluated **at the same knob point**. A relative bound like `h(f) − h(1kHz)` is correlated, not interval-subtracted. The affine forms make this cheap (walkthrough §8).

**Closed ranges.** Specs use `a..=b`, which means closed in Rust. `a..b` is half-open in Rust. For continuous quantities the difference only shows on the boundary, but rule 6 says we don't reuse `..` with a different meaning. The formatter prints `..=`, and a lint suggests `..=` when `..` is used in a requirement.

**Spec vs part ranges.** The same literal `47kΩ ± 1%` means opposite things depending on where it appears. The position is always visible:

| Position | Example | Relation | Reads as |
|---|---|---|---|
| value (constructor argument, `let … =`) | `Resistor(47kΩ ± 1%)` | ⊇ spread | "r1 may be anywhere in 46.5…47.5 kΩ" |
| after `in` in `spec` | `spec gain: … in 4.6 ± 5%` | ⊆ requirement | "gain must stay inside 4.37…4.83" |
| after `in` in `assume` | `assume vcc.v in 12V ± 5%` | ⊆ for the context, spread (knob) for this block | "give me 11.4…12.6 V; I'll work anywhere in it" |
| after `in` in `absolute_max` / `operating` | `vin.v in -0.3V..=40V` | ⊆ requirement on the context | "never go outside" |
| solver choice | `Resistor(? in 10kΩ..=100kΩ)` | ∃ search space | "pick one value from here" |

- **In the editor:** a spread renders as `47 kΩ ±1%` with a distribution glyph. A requirement renders as a band with hard edges `[4.37 ━━ 4.83]`, and the result bracket is drawn inside it. An assumption renders as a band with a knob glyph.
- **In the type checker:** a requirement's right-hand side has type `Set<T>` and is only legal after `in`. Interface roles decide which of `assume` / `spec` is legal (§3.2).

**Contrast with atopile.** atopile uses one syntax, `x = 10kohm +/- 5%`. It lowers to `IsSubset` (a requirement) inside a `module` (`atopile/src/atopile/compiler/ast_visitor.py:846`), but to `IsSuperset` (a part spread) inside a `component` (`ast_visitor.py:861`). The chosen relation is then used for every assignment (`ast_visitor.py:1231`). The meaning flips with the kind of enclosing block, and nothing on the line shows it. That is precisely what we want to avoid.

### 5.4 Conditions: `where`

```rust
/// Datasheet-style number, at room temperature, new parts.
spec gain_25: h.at(1kHz).mag() in 4.6 ± 2% where temp == 25°C, life == 0y;

/// Beginning of life only: aging is covered by `bass` itself.
spec bass_bol: h.f_low(-3dB) <= 27Hz where life == 0y;

/// Only claim efficiency where the converter is meant to run.
spec eff: p_out / p_in >= 88% where vout.i in 0.5A..=2A;
```

- `knob == value` pins a knob. `knob in set` narrows it. Conditions must stay **inside** the assumptions. Widening is an error: *"`where temp in -40°C..=85°C` exceeds the contract (−10…60 °C); a spec can't be verified outside the block's assumptions."* To deliberately stress a design outside its contract, use a bench with `#[allow(bench::outside_contract, reason = "…")]` (§7.6).
- A condition may introduce a knob that isn't an assumption (e.g. a trimmer position: `where trim.pos in 0..=1`).
- **Beginning vs end of life** (open decision 5 in spec_design): one spec over the `life` knob is the default, because failures that only appear in the field are the point (walkthrough §6). `where life == 0y` expresses a day-1-only claim.
- **In the editor:** the conditions column shows only the narrowed knobs ("temp = 25 °C; life = new"). Blank means "everything assumed".

### 5.5 Confidence

```rust
#[confidence(worst_case)]      // every knob at its worst edge, statistical ones included
#[confidence(sigma(3))]        // range knobs worst; statistical knobs at 3σ worst-case distance [AGW94]
#[confidence(yield(99.9%))]    // range knobs worst; ≥ 99.9% of boards pass (MC or importance sampling at sign-off)
#[confidence(nominal)]         // nominal simulation only; quick checks, early design
```

- **Defaults** are set in the manifest and can be overridden with an inner attribute for a whole contract (`#![confidence(worst_case)]`) or with an attribute on one spec:
  ```toml
  # Spicy.toml
  [specs]
  confidence = "sigma(3)"            # user specs
  [checks]
  confidence = "worst_case"          # automatic checks: stress checks are conventionally worst case
  ```
- **Proposal for open decision 1:** `sigma(3)` for user specs and `worst_case` for automatic checks. A stress check done "at 3σ" would surprise any reliability engineer.
- As in spec_design §7, the engine **always** shows both worst-case and realistic results when they disagree. The confidence only picks which one sets the verdict (see `bias` in §4).

### 5.6 Rationale, severity, messages

**Rationale is a `///` doc comment.**
- It's Rust-native (`///` is `#[doc = "…"]`), can be multi-line, and supports markdown.
- It shows on hover, fills the table's "why" column, and goes to the AI with every question about the spec.
- **Alternative considered:** a trailing `because "…"` clause (the spec_design strawman). It makes lines long, it isn't Rust, and it can't hold a paragraph. A lint suggests adding a rationale to specs that fail or that are waived.

**Severity** follows Rust's lint levels:

| Attribute | Meaning |
|---|---|
| (none) = `#[deny]` | a FAIL is an error and blocks sign-off |
| `#[warn]` | a FAIL is reported and doesn't block (a "target", not a limit) |
| `#[expect(fail, reason = "…")]` | a known, accepted failure. The engine **warns when it starts passing**, so stale waivers get cleaned up. This mirrors Rust's `#[expect(lint)]`, stabilized in Rust 1.81 together with `reason = "…"` on lint attributes |

A `pub let` measure without a spec is an **info row**: it's measured and shown, with no verdict (§6.5).

**Messages:** `else "…"` replaces the default failure text. It's borrowed from Spade's `where N == … else "Integer cannot be divided into bytes"` (`spade/spade-compiler/stdlib/conv.spade:518`, parsed at `spade/spade-parser/src/lib.rs:2141-2152` into `if_unsatisfied: Option<String>`, `spade/spade-ast/src/lib.rs:229-235`). It's mostly for library checks (§9), where the author knows what the violation means for the user.

### 5.7 Envelope specs, and what gets published

A **bare signal** in a contract statement means its **envelope**: the range it covers over every bench, every large-signal analysis (DC operating point, DC sweeps, transient) and every time point. AC and noise are small-signal perturbations and don't count toward it.

```rust
spec vout.v in 3.3V ± 4%;                   // envelope: what downstream blocks may rely on, always
spec accuracy: dc(vout.v) in 3.3V ± 1.5%;   // DC accuracy only: a characteristic, not the interface guarantee
```

- **Small-signal interface quantities** such as `z_src` and `z_load` (defined by an `impedance(…)` measure, §9.2) have no large-signal envelope. Used bare, they mean the worst value over the port's assumed signal band. So `spec input.z_load >= 20kΩ` holds at every assumed frequency, at every knob point.
- **Publishing.** An envelope spec on a produced port quantity is the block's **published guarantee** for that quantity. It's what connection checks use at the "declared" level (§10.2).
- **The same rule makes automatic checks natural.** `absolute_max { vin.v in -0.3V..=40V; }` constrains the envelope, i.e. every analysis and every instant, including start-up transients.
- **Caveat: an envelope is only as good as its benches.** A block whose only bench is DC has a DC-only envelope. The result card lists which benches contributed ("envelope from: default (dc), load_step (tran)"). Bench coverage is an open question (§16).

### 5.8 Port specs vs internal specs

- Specs on **port quantities** are the public contract. Parents see them, and composition uses them.
- Specs on **internal** nets or parts (e.g. `dc(q1.c.v - q1.e.v) >= 1V`) are the block's own design checks. They show in its table with an *internal* tag and are invisible to parents, like private fields in Rust.

---

## 6. Measures

### 6.1 Probes

Probes are **field accessors** on circuit objects. That's how the editor thinks: click a net, a pin, a part.

| Probe | Meaning | Type |
|---|---|---|
| `out.v` | voltage of a net (to ground) or a port (to its reference) | `Signal<Volt>` |
| `a.v - b.v` | differential; plain arithmetic, since probes are linear | `Signal<Volt>` |
| `q1.c.i` | current **into** a pin | `Signal<Amp>` |
| `r1.v`, `r1.i` | across / through a two-terminal part (pin 1 → pin 2) | `Signal<Volt>`, `Signal<Amp>` |
| `r1.p`, `q1.p` | power dissipated in a part | `Signal<Watt>` |
| `vcc.i`, `vout.p` | interface quantities with a **role-relative sign**: into a sink, out of a source. So `vcc.i` on a Power sink is the supply current, and `vout.i` on a Power source is the load current | per interface |
| `q1.vce` | derived probes declared by part models (`let vce = c.v - e.v;`) | `Signal<Volt>` |
| `impedance(input)` | small-signal impedance at a port: the engine replaces the fixture with a test current | `Signal<Ohm>` (AC) |
| `loop_gain(fb)` | loop gain by voltage injection at net `fb` (Middlebrook's method): the engine inserts the injection source in a copy of the bench | `Signal<Ratio>` (AC) |

**Why fields rather than SPICE's `V(out)`.**
- In Rust, `V(out)` reads as a tuple-struct constructor, and it collides visually with the unit `V` in `5V`.
- `out.v` is how an interface quantity is named anyway. `vcc.v` in `assume vcc.v in 12V ± 5%` and `vcc.v` in a measure are **the same physical quantity**, constrained in one place and observed in the other. That's S2/S3 in one line.
- **Cost:** `.v`, `.i` and `.p` are reserved accessor names on nets, pins, parts and ports.

**Probe identity** (open decision 3 in spec_design): probes are expressions over **symbols**, resolved by the compiler, not strings.
- Renaming a net is a refactor: LSP rename updates every probe.
- A probe that should survive restructuring is a `let` binding (`let vc = dc(output.v);`). The schematic marker is attached to the binding name, and its placement goes in the sidecar file (v1 §7.5 rule 4).

**Encapsulation:** a parent can probe a child's ports and its `pub let` measures, but not its internals, like Rust privacy. Debug access to internals is allowed in benches and flagged with a lint.

### 6.2 Analyses and result types

| Call | Returns | Notes |
|---|---|---|
| `dc(x)` | `T` (a scalar) | operating point of the bench |
| `ac(x / y)` | `Response<T>` (a function of frequency) | **The denominator names the excitation.** `ac(output.v / input.v)` excites the input fixture with all other AC sources off. `ac(output.v / vcc.v)` excites the supply fixture, i.e. PSRR |
| `tran(x)` | `Waveform<T>` | uses the bench's `tran` settings |
| `noise(x)` | `Density<T>` | output noise; `.referred_to(input)` for input-referred |
| `BENCH.dc(x)` etc. | same | explicit bench, for measures that mix benches |

- **Analyses are functions.** A signal only becomes a number through an analysis (or through the envelope rule in contract statements, §5.7). Writing `spec gain: output.v / input.v in 4.6 ± 5%` gives an error with quick fixes (`dc(…)` / `ac(…).at(…)` / `tran(…)`), rather than guessing.
- **On-demand runs.** `ac` and `tran` runs are shared between specs that use the same bench. Settings are derived (AC band from the assumed signal band ×/÷ 10; transient stop time from the stimulus timing plus settling) and can be overridden in the bench (§7.5).

### 6.3 The measurement library

These are methods on the result types, so after `ac(…).` the editor offers exactly the AC measures. The type checker rejects `.rise_time()` on a frequency response.

**Smoothness classes:**
- **S:** smooth in the knobs.
- **P:** piecewise smooth (max/min: differentiable wherever the extreme point is unique).
- **C:** a crossing. It's smooth while the crossing is transversal, since dt*/dp = −(∂x/∂p)/ẋ [Galán et al. 1999, bibliography §7]. It becomes undefined when the crossing vanishes.
- **D:** discontinuous.

| Method | On | Returns | SPICE `.meas` analog | Class |
|---|---|---|---|---|
| (value) | `dc(x)` | T | `.meas op` / `FIND` | S |
| `.vs(knob)` | any scalar measure | `Curve<K, T>` (display only) | `.step` + `.meas` | S |
| `.at(f)` / `.mag()` / `.db()` / `.phase()` | `Response` | complex, T, dB, ° | `FIND … AT=` | S |
| `.band(f1..=f2)` | `Response` | restricted `Response` | `FROM= TO=` | — |
| `.max()` / `.min()` / `.peaking()` | `Response`, `Waveform` | T | `MAX`, `MIN` | P |
| `.f_low(db)` / `.f_high(db)` / `.bandwidth(db)` | `Response` | Hz | `WHEN … CROSS=` | C |
| `.unity_gain_freq()` | `Response` (loop gain) | Hz | `WHEN mag=1` | C |
| `.phase_margin()` / `.gain_margin()` | `Response` (loop gain) | °, dB | two `.meas` + arithmetic | C |
| `.at(t)` | `Waveform` | T | `FIND … AT=` | S |
| `.window(t1..=t2)` / `.window(t1..)` | `Waveform` | restricted `Waveform` | `FROM= TO=` | — |
| `.avg()` / `.rms()` / `.integral()` | `Waveform` | T, T, T·s | `AVG`, `RMS`, `INTEG` | S |
| `.pp()` | `Waveform` | T | `PP` | P |
| `.cross(level, Rising \| Falling \| Either, n \| Last, td: t)` | `Waveform`, `Response` | s, Hz | `WHEN … RISE= / FALL= / CROSS= n\|LAST TD=` | C |
| `.rise_time(lo, hi)` / `.fall_time(lo, hi)` | `Waveform` | s | `TRIG … TARG …` | C |
| `.delay(from: sig, level)` | `Waveform` | s | `TRIG … TARG …` | C |
| `.overshoot()` | `Waveform` | % | `MAX` + arithmetic | P |
| `.settling_time(band, from)` | `Waveform` | s | (none built in) | **D** |
| `.thd(f0)` | `Waveform` (steady state) | % | `.four` | S (with whole periods) |
| `.time_outside(set)` | `Waveform` | s (per excursion) | (none) | C |
| `.density_at(f)` / `.integrated(band)` | `Density` | V/√Hz, V rms | `.noise` | S |

- **Semantics follow `.meas`.** Edge counting, `Last`, trigger delay, windows, and AC comparisons on the real part, as in ngspice §11.4, so imported `.meas` decks mean the same thing.
- **Pointwise arithmetic** (`+ − × ÷`, `.abs()`, `.dev_from_final()`) works on waveforms and responses and keeps the result type, e.g. `tran(x).window(3ms..).abs().max()`.
- **User measure functions** are plain `fn`s over these types (§6.5).

### 6.4 Smooth preferred, non-smooth flagged and rewritten

The worst-point loop needs slopes (v2 A8), so the engine prefers smooth measures. A non-smooth measure gets a hint underline. Where an **equivalent requirement** on a smooth measure exists, the engine evaluates that instead and shows the result in the form the engineer wrote:

| Written | Evaluated as | Equivalent when |
|---|---|---|
| `x.settling_time(1%, from: t0) <= T` | `x.window(t0+T..).dev_from_final().abs().max() <= 1%` | always (a restatement of the definition) |
| `x.overshoot() <= 10%` | `x.max() <= 1.1 * x.final()` | always |
| `h.f_high(-3dB) >= f0` | `h.at(f0).db() >= h.ref_db() - 3dB` | \|H\| decreases monotonically past its reference (checked on the nominal sweep; otherwise no rewrite) |
| `x.rise_time(10%, 90%) <= T` | kept (class C) | flagged if the slope at a crossing is near zero at some evaluated point |

- A requirement on a **D** measure with no rewrite (e.g. a count of ringing peaks) is evaluated at the requested confidence by simulation only, and its verdict is labeled *"non-smooth: worst point not guaranteed"*.
- **A missing crossing is never a pass.** If a class-C measure has no crossing at some knob point (the gain never drops 3 dB inside the swept band, the output never reaches 90%), the verdict is UNDECIDED with *"no crossing in the analysis range"* and a hint to widen it. Modelica's requirement library reports such a never-exercised check as *untested* for the same reason (§14).
- **Crossings need time-step control.** Verilog-AMS `cross()` and `above()` also control the simulator's time step (§14). A crossing located on a coarse fixed-step waveform is a simulator artifact, and today's transient uses a fixed step (v2 Part D, item 5). Class-C measures therefore register their levels with the transient analysis, which must refine the step around them. This is a simulator requirement that comes from the spec language.
- `if` on a **measured** value inside a measure (e.g. `if dc(x) > 0 {…}`) is non-smooth and gets a lint. `if` on elaboration-time values is fine (§8.2).

### 6.5 Measures as values, and published measures

```rust
let vc       = dc(output.v);
let headroom = vcc.v - vc;          // VCC appears twice; the shared knob cancels (walkthrough §8)
spec headroom >= 3V;

pub let i_supply = dc(vcc.i);       // published characteristic: parents can use it (§10.2)

fn flatness(h: Response<Ratio>, band: Range<Hz>) -> dB {
    h.band(band).db().max() - h.band(band).db().min()
}
spec flat: flatness(h, 20Hz..=20kHz) <= 1dB;
```

- Measures and hand-written formulas share one namespace, one unit system and one representation, so `vcc.v - vc` is correlated correctly.
- **Hand-written equations are for design intent the circuit doesn't contain** (v2 A6): power budgets, derived limits. They are never a restatement of the circuit.

---

## 7. Benches (setups)

### 7.1 Decision: the contract lives in the block; benches are values of type `Bench<Block>`

| Aspect | Setups inside the block | Separate testbench blocks (ADE) | **Proposal** |
|---|---|---|---|
| Common-case effort | low | high: a testbench for every block | **zero**: the default bench is derived |
| Where the engineer looks | one page | the design, a testbench, and a spec view | the I/O page lists every bench; form benches are edited in place; sheet benches open as tabs |
| Complex fixtures (a real neighbor, a LISN, several DUTs, lab-supply models) | awkward | natural | sheet bench |
| Design sheet stays clean | risky (sources drawn into the design) | clean | clean: fixtures are never drawn in the design sheet |
| Consistency with assumptions | automatic | manual, and it drifts | **checked**: a bench must stay inside the assumptions (§7.6) |
| Composes into systems | no | benches don't compose | default benches derive from contracts, which compose (§10) |
| AI context | one file | must discover the testbench | both are named in the contract |
| Precedent | Modelica keeps simulation settings in the model (`experiment` annotation, Modelica 3.6 §18.4) | Cadence ADE: tests reference a testbench cell. Spade keeps testbenches outside the language entirely: cocotb Python files with a `#top = module::entity` header (`spade/swim_tests/test/array_mut_indexing.py:1`), found through `[simulation] testbench_dir` (`spade/swim_tests/swim.toml:10-11`) | both, as one type |

**So:** specs and assumptions are in the contract (they're claims about the block). A spec points at a bench by name (`on loud`), because the bench is only one way to check the claim. Benches come in three sizes, all of type `Bench<CeAmp>`:

1. **The default bench:** always present, derived from the assumptions, never written.
2. **Form benches:** written inside the contract as a struct literal that overrides a few ports and starts from `..default`.
3. **Sheet benches:** separate `bench X for CeAmp { … }` items with their own schematic sheet. The `for` mirrors Rust's `impl Trait for Type`.

### 7.2 The default bench

For each port, the default bench attaches the **fixture implied by the interface type and the assumptions**:

| Port | Fixture | From |
|---|---|---|
| `Power` sink | DC source, value = knob `vcc.v`, series `z_src` | `assume vcc.v …`, `assume vcc.z_src …` |
| `Power` source | load, current = knob over `assume vout.i …`, plus `c_load` | assumptions on `i`, `c_load` |
| `Analog<In>` | signal source of the assumed family (knobs `amp`, `freq`), series `z_src`; AC excitation for `ac(…/input.v)` | `assume input in Sine {…}`, `assume input.z_src …` |
| `Analog<Out>` | load conductance knob `g_load`, `c_load` | `assume output.z_load …` |
| `Digital<In>` | driver with the worst-case levels allowed by the assumption | interface levels (§9.2) |
| `I2c<Target>` | controller model at the assumed `mode` | interface (§9.2) |
| env | `temp`, `life` knobs | inherited or assumed |

It gives every block a **block-level simulation** (vision §3) with zero setup, and it is exactly the "signal driving" of the I/O page: the same definitions drive both.

### 7.3 Form benches

```rust
bench loud {
    input: Sine { amp: 100mV, freq: 1kHz },     // one port overridden
    tran: 0ms..=20ms,                            // analysis settings
    ..default                                     // every other port as assumed
}
```

- The fields are the DUT's **port names** (each typed by its interface and role, so a sine can't be attached to a `Power` source) plus bench fields: `dc`, `ac`, `tran`, `noise`, `env`, `at`, `models`, `simulator`, `options`.
- `..default` is Rust's struct-update syntax, and it's required, as in Rust. The editor inserts it. It tells the reader "everything else as assumed".
- **Range fields are knobs:** `input: Sine { amp: 10mV..=100mV, freq: 1kHz }` checks every amplitude in the range.
- **Parametric benches** are benches with parameters, like functions:
  ```rust
  bench overload(k: usize) {
      inp: one_hot(k, Sine { amp: 2V, freq: 1kHz }, Dc(0V)),
      tran: 0ms..=5ms,
      ..default
  }
  spec recover[i]: tran(out[(i + 1) % 8].v).window(3ms..).abs().max() <= 10mV on overload(i);
  ```
- **In the editor:** a form bench is a form on the I/O page, one row per port, with the default shown greyed. Opening it shows the DUT symbol surrounded by auto-drawn fixtures, **read-only** (like generated blocks, v1 §7.5 rule 5), with a "convert to sheet bench" action.

### 7.4 Sheet benches

For fixtures that aren't a port override: a real upstream block, a filter network, a lab supply with cable inductance, or two DUTs:

```rust
bench on_mainboard for Buck {                               // (core syntax: placeholder)
    let dut  = Buck(…);
    let src  = LabSupply(v: 12V ± 10%, cable: 1µH ± 20%);
    let load = Emmc5Load();                  // a realistic load model, not a current source
    net vin  = [src.out, dut.vin];
    net vout = [dut.vout, load.vdd];
    tran: 0ms..=5ms;
}
spec vout.v in 3.3V ± 5% on on_mainboard;
```

- It's a block with no ports that instantiates `dut: Buck`, drawn as a normal sheet, i.e. the Cadence ADE testbench.
- It must still satisfy the DUT's assumptions (§7.6). This is how "system-level simulation" (vision §3) works at the top: `Top` has benches too.

### 7.5 Analysis settings

| Field | Example | Default |
|---|---|---|
| `dc` | `dc: Op` or `dc: Sweep(vin.v in 0V..=15V)` | operating point |
| `ac` | `ac: 1Hz..=10MHz, points: 20/decade` | assumed signal band ×/÷ 10, or what the measures need |
| `tran` | `tran: 0ms..=3ms, max_step: 10ns` | stimulus timing + settling estimate |
| `noise` | `noise: 20Hz..=20kHz` | assumed band |
| `models` | `models: Averaged` | `Default` (e.g. switching). Selects model variants; see the buck in §13.1 |
| `simulator`, `options` | `simulator: "spicy"`, `options: { reltol: 1e-6 }` | project default (vision §3: "simulator choice") |

### 7.6 Benches within the contract; corners; counterexamples

- **A bench must stay inside the DUT's assumptions.** A fixed stimulus or load must lie in the assumed set, and range fields must be ⊆ it. Otherwise the spec would be measured outside the contract. A deliberate stress test says so: `#[allow(bench::outside_contract, reason = "abuse test at 2× input")]`.
- **Corners** are named partial knob assignments, as in Cadence:
  ```rust
  corner cold_old { temp: -10°C, life: 10y }
  bench worst_bass { at: cold_old, ..default }   // other knobs still vary
  ```
- **Counterexamples** print as full corners (§11), and "pin as corner" stores them. Pinned corners are re-simulated first on every run: a cheap inner bound, i.e. a regression test.

---

## 8. Loops, for-all and parametric specs

### 8.1 For-all without syntax

- **Every range knob is universally quantified.** With `assume vout.i in 0A..=2A`, the spec `spec accuracy: dc(vout.v) in 3.3V ± 1.5%` already holds for all load currents. The engine finds the worst one. Treating a sweep variable as a range knob gives the same answer; only the display differs (spec_design §6).
- **Display as a curve:** `.vs(knob)` changes the display only (a curve with a band) and allows a `where` narrowing:
  ```rust
  spec accuracy: dc(vout.v).vs(vout.i) in 3.3V ± 1.5%;       // shown as vout vs load, with the band
  spec clip: tran(output.v).thd(1kHz).vs(input.amp) < 1% on loud;
  ```
- **Analysis axes** (frequency, time) aren't knobs. They are restricted with `.band(…)` / `.window(…)`, and a requirement on the restricted result holds **at every point**:
  ```rust
  /// Flat within ±1 dB of the 1 kHz gain across the audio band.
  spec flat: h.band(20Hz..=20kHz).db() in h.at(1kHz).db() ± 1dB;
  ```
  The result reports the worst frequency and the worst knob point together.
- **Why not `for f in 20Hz..=20kHz { … }`:** in Rust that iterates, and over a continuous range it would suggest sampling. The engine searches for the worst point; it doesn't sample. By rule 6, `for` stays elaboration-time only.
- A general quantifier is possible later as a closure, `(20Hz..=20kHz).all(|f| …)`, and limit masks as `in mask![…]`. Neither is needed for v1.

### 8.2 Elaboration-time loops generate specs

```rust
for i in 0..8 {
    spec gain[i]: ac(out[i].v / inp[i].v).at(1kHz).mag() in 10 ± 1%;
}
```

- `for` and `if` over elaboration-time values run at compile time (v1 §7.5 rule 2), so no Spade-style `gen if` (`spade/spade-parser/src/lib.rs:797-814`) is needed. Everything in a contract body is elaboration-time except the measures themselves.
- **Indexed names** (`gain[i]`, `xtalk[i][j]`) give stable row identities.
- **In the editor:** a loop becomes a **group row**, `gain[0..8]`, showing the worst element (`gain[5] 9.93 ✓`) and expanding to eight rows. Loop-generated rows are read-only in the table ("edit in code"), the same rule as generated blocks.

### 8.3 Per-instance specs vs loop specs

- If the property belongs to **one channel**, write the spec once in `contract Channel`. Every instance gets it (`ch[3].gain`), with independent part knobs and shared env knobs (spec_design §3).
- Use loop specs in the array's contract only for **relations between channels**: matching, crosstalk, skew. This is where affine forms pay off: `gain[i] / gain[0]` cancels the shared knobs (temperature, a matched network's lot knob) exactly (§13.3).

### 8.4 Parametric specs

Contracts take the block's generic and value parameters, like `impl<const N: usize> Afe<N>`:

```rust
contract<const N: usize> Afe<N> {
    for i in 0..N { spec gain[i]: … in G_NOM ± 1%; }
}
contract Channel {
    spec gain: h.at(1kHz).mag() in self.g_nom ± 1%;      // bound from a block parameter
    spec swing: tran(output.v).pp() >= 2 * self.g_nom * input.amp.max() on loud;
}
```

---

## 9. Automatic checks

### 9.1 The mechanism: assumptions of parts and interfaces

A capacitor's voltage rating is the **capacitor's assumption** about its surroundings: "the voltage across me stays below V_rated". An I2C bus's pull-up requirement is the **bus's assumption**. So automatic checks are `assume` items declared in part models and interface types, tagged with a check group:

```rust
#[check(derating)]
assume v_rated: self.v.abs() <= derated(self.v_rated) else "capacitor overstressed";
```

- **Discharged by the context's simulation.** The part's `self.v` is a bare signal, so it means the envelope over every bench the parent runs (§5.7). That is exactly PSpice Smoke's "stress over the simulation" idea, applied to our benches.
- **Two kinds of assumption, one syntax.** The compiler tells them apart:
  - An assumption on a quantity **the other side produces** (a port's supply voltage, input signal, load) is an **input assumption**. It becomes a knob when the block is verified standalone.
  - An assumption on a quantity **jointly determined** by the circuit (the voltage across a capacitor, a pin's voltage in a loaded net) is a **constraint assumption**. It is checked by the context's simulation.
- **Blocks and parts are the same thing here.** A part is a leaf block, and its datasheet is its contract (§3.7).

### 9.2 Interface types: role-tagged quantities

An interface declares its nets, its roles, and **for each quantity, which role produces it and how several producers or consumers combine**. The connection checks are generated from these declarations (§10.1).

```rust
interface Power {
    roles: Source, Sink;
    hi: Net,
    lo: Net,
    /// Rail voltage. The source guarantees it; each sink assumes a range.
    #[by(Source)]       v:     Volt  = hi.v - lo.v,
    /// Current drawn. Each sink guarantees its draw; the source assumes the total.
    #[by(Sink, sum)]    i:     Amp   = hi.i,
    /// Output impedance of the rail (loading, ripple).
    #[by(Source)]       z_src: Ohm   = impedance(self),
    /// Capacitance hung on the rail by the sinks (stability of the source).
    #[by(Sink, sum)]    c_load: Farad,
}
```

Combine rules:
- `one`: exactly one producer; the Spade-style single driver.
- `sum`: currents, capacitances.
- `parallel`: impedances.
- `intersect`: capabilities, e.g. supported bus speeds.
- `collect`: addresses.

These generalize atopile's `is_alias_bus_parameter` (voltage) and `is_sum_bus_parameter` (currents) on `ElectricPower` (`atopile/src/faebryk/library/ElectricPower.py:65-69`). In atopile the sum check is still a placeholder: the docstring says `Sum(Sinks.param) <= Sum(Sources.param)`, but the code builds the tautology `Sum(all) <= Sum(all)` with a TODO (`atopile/src/faebryk/library/is_sum_bus_parameter.py:20,45-61`). PolymorphicBlocks implements the real check: `VoltageLink` sums `current_draw` over sinks and requires it within the source's `current_limits` (`PB/edg/electronics_interfaces/VoltagePorts.py:66-71`).

**Role type parameters** follow the Rust embedded typestate idiom, e.g. `Pin<Output<PushPull>>` in the stm32 HALs: `Power<Source>`, `Digital<OpenDrain>`, `I2c<Target>`. The role decides which of `assume` / `spec` is legal on each quantity (§3.2), and which connections are legal (one `Source` per `Power` net).

**Digital I/O** carries logic levels. The input thresholds are expressions in the reference rail, so they share its knob:

```rust
interface Digital {
    roles: Out, In, Io, OpenDrain;
    line: Net,
    reference: Power,                           // like atopile's ElectricLogic.reference
    #[by(Out)]          voh: Volt,               // guaranteed output-high level
    #[by(Out)]          vol: Volt,               // guaranteed output-low level
    #[by(Out)]          iol: Amp,                // sink capability at vol
    #[by(In)]           vih: Volt = 0.7 * reference.v,
    #[by(In)]           vil: Volt = 0.3 * reference.v,
    #[by(In, sum)]      c_in: Farad,
    #[by(In, sum)]      i_leak: Amp,

    link {                                       // evaluated once per connected net
        for d in self.drivers() {
            for r in self.receivers() {
                #[check(logic)] assume high: d.voh >= r.vih else "{d} can't reach {r}'s input-high threshold";
                #[check(logic)] assume low:  d.vol <= r.vil else "{d} can't pull below {r}'s input-low threshold";
            }
        }
    }
}
```

- An `OpenDrain` driver's high level isn't driven. The link computes it from the pull-up: `voh = rail.v − Rp·Σi_leak`.
- **Why affine forms matter here.** With driver and receiver on the same 3.3 V rail, `VOH = VDD − 0.4 V` and `VIH = 0.7·VDD` share the VDD knob. The check reduces to 0.3·VDD ≥ 0.4 V, which is exactly right. Interval arithmetic would pair VDD-low on one side with VDD-high on the other.
- **Comparison.** atopile's `ElectricLogic` has a line, a reference and a push-pull enum, but no thresholds (`atopile/src/faebryk/library/ElectricLogic.py:33-38`). PolymorphicBlocks checks `output_thresholds ⊇ input_thresholds` over all participants, but by interval hulls, i.e. uncorrelated (`PB/edg/electronics_interfaces/DigitalPorts.py:107-117`).

**I2C** is a bus of two open-drain lines. The pull-up window is **derived from the actual bus**, not fixed:

```rust
interface I2c {
    roles: Controller, Target;
    scl: Digital<OpenDrain>,
    sda: Digital<OpenDrain>,
    #[by(Controller)]          mode: I2cMode,          // Standard / Fast / FastPlus
    #[by(Target, intersect)]   modes: Set<I2cMode>,
    #[by(Target, collect)]     address: u7,

    link {
        for line in [scl, sda] {
            let rp = r_pullup(line);    // measured: DC resistance from the line to its rail, all drivers released
            let cb = c_total(line);     // Σ pin capacitances (#[by(In, sum)] c_in) + trace estimate
            #[check(i2c)] assume pulled_up: rp <= 100kΩ else "{line} has no pull-up";
            #[check(i2c)] assume rp_min: rp >= (line.reference.v - min(line.vol)) / min(line.iol)
                else "pull-up on {line} too strong for the weakest driver";
            #[check(i2c)] assume rise: 0.8473 * rp * cb <= mode.t_rise_max()
                else "{line} rises too slowly for {mode}: lower the pull-up or the bus capacitance";
        }
        #[check(i2c)] assume speed: mode in modes else "a target doesn't support {mode}";
        #[check(i2c)] assume addresses: all_unique(self.targets().address);
    }
}
```

- **Where the formulas come from.** They are the sizing relations of the I2C specification (NXP UM10204 Rev. 7.0, §7.1; also TI SLVA689 Eqs. 1 and 6): Rp(min) = (VDD − VOL(max))/IOL and Rp(max) = t_r/(0.8473·C_b). The 0.8473 is ln(0.7/0.3), i.e. the RC time from the 0.3·VDD to the 0.7·VDD threshold.
- **Per-mode limits** in the `I2cMode` library type come from UM10204:
  - t_r max 1000 / 300 / 120 ns and C_b max 400 / 400 / 550 pF for Standard / Fast / Fast-mode Plus (Table 11);
  - VOL ≤ 0.4 V at 3 mA sink (20 mA for Fast-mode Plus), VIL ≤ 0.3·VDD and VIH ≥ 0.7·VDD (Table 10).
- **Better than a lookup table:** our version uses the participants' **actual** guaranteed `vol` / `iol`, and `rp` is **measured** by the engine. It sees pull-ups inside sub-blocks, resistor networks and level shifters, not just resistors wired to the line.
- **Comparison with atopile.** atopile's `requires_pulls` trait checks a fixed window of 0.9…11 kΩ (`atopile/src/faebryk/library/I2C.py:71-79`). Its result is three-valued: `RequiresPullNotFulfilled` vs `RequiresPullMaybeUnfulfilled` (`atopile/src/faebryk/library/requires_pulls.py:21-73`). In this version `I2C.requires_pulls()` is never called, `MakeChild` is marked `# FIXME: broken` (`requires_pulls.py:122`), and the address-uniqueness check is commented out (`I2C.py:122-167`).
- **Comparison with PolymorphicBlocks.** It requires a pull-up (or a controller with one), at most one pull-up, and unique addresses, but it has no rise-time or bus-capacitance model (`PB/edg/electronics_interfaces/I2cPort.py:35-44`; `PB/edg/circuits/I2cPullup.py:10` carries a TODO).

**Analog** carries levels and the loading pair:

```rust
interface Analog {
    roles: Out, In;
    sig: Net, reference: Net,
    #[by(Out)]            v: Volt = sig.v - reference.v,
    #[by(Out)]            z_src: Ohm = impedance(self),
    #[by(In, parallel)]   z_load: Ohm = impedance(self),
    #[by(In, sum)]        c_load: Farad,
}
```

PolymorphicBlocks hard-codes a 10× impedance-ratio rule on its analog link (`PB/edg/electronics_interfaces/AnalogPort.py:55-67`). Here the ratio isn't a rule of thumb: the producer **assumed** a load range when its specs were verified, and the consumer **guarantees** its input impedance.

### 9.3 Part models

```rust
part Capacitor {                                    // category (trait-like; the sibling decides)
    pins: a, b;
    let c: Farad;
    let v_rated: Volt;                              // rating: part of the model's contract
    let polarized: bool = false;
    let v_reverse_max: Volt = 0V;

    #[check(derating)]
    assume v_rated: self.v.abs() <= derated(self.v_rated);

    if self.polarized {                             // elaboration-time `if` (§8.2): the check exists only for polarized parts
        #[check(polarity)]
        assume reverse: self.v >= -self.v_reverse_max;
    }
}

part Tps54x: BuckController {                       // a concrete IC, as the AI would extract it
    pins: vin, gnd, en, boot, sw, fb, comp;
    absolute_max {
        vin.v in -0.3V..=42V;
        sw.v  in -1V..=42V;
        sw.v  in -3V..=42V, for < 10ns;             // transient abs-max: per-excursion duration
    }
    operating {
        vin.v in 4.5V..=36V;
        tj    in -40°C..=150°C;
    }
    let vref    = 0.8V ± 1%;                        // electrical characteristics: spreads (knobs)
    let f_sw    = 400kHz..=600kHz;
    let i_limit = 3.5A..=5A;
    let theta_ja = 40K/W ± 20%;
    let tj = temp + theta_ja * self.p.avg();        // derived probe used by `operating`
}
```

- `absolute_max { … }` is sugar for one `#[check(absmax)] assume` per line, and `operating { … }` for `#[check(operating)] assume`. The sections exist so a datasheet transcribes almost line by line. This matters because the AI does the transcribing (vision §5).
- **Visible difference inside a part:** `vin.v in …` (a requirement) vs `let vref = …` (a spread).
- **Transient abs-max** (`for < 10ns`) uses `.time_outside(set)` per excursion (§6.3). It is how a datasheet's "−3 V for 10 ns" on a switch node becomes checkable against a ringing transient.
- **Thermal:** `tj = temp + θja·P`. It uses the average power over each bench run, assuming thermal time constants much longer than the run. The shared `temp` knob keeps it correlated with everything else. Electro-thermal coupling (P depends on Tj) is an open question.

**Comparison.**
- atopile's `Capacitor` asserts `max_voltage >= power.voltage`, with no derating and only through the `power` alias interface (`atopile/src/faebryk/library/Capacitor.py:38-45`).
- PolymorphicBlocks applies a 2× voltage margin when **selecting** ceramic capacitors ("2x is the general rule of thumb", `PB/edg/abstract_parts/Capacitor.py:37`) and filters parts tables on it. It doesn't re-check derating against the actual stress.

### 9.4 Catalogue: what's automatic, from what, and whether it's on

| Group | Check | Declared in | Measured from | Default level |
|---|---|---|---|---|
| `absmax` | every pin within Absolute Maximum Ratings, at every corner, in every analysis, incl. start-up and transient excursions | part `absolute_max {}` | envelope of pin voltages / currents | **deny** |
| `operating` | every part within its Recommended Operating Conditions (supply range, Tj, input common-mode range…) | part `operating {}` | envelope | **deny** |
| `polarity` | reverse voltage on polarized parts (electrolytic, tantalum, diodes' reverse rating) | part category | envelope of `self.v` | **deny** |
| `contract` | every connection: producer guarantee ⊆ consumer assumption, for every role-tagged quantity | interface `#[by]` fields | declared → characterized → in-context (§10) | **deny** |
| `logic` | VOH ≥ VIH and VOL ≤ VIL for every driver/receiver pair, correlated through shared rails | `Digital` interface | interface quantities (affine) | **deny** |
| `power` | Σ sink currents within the source's assumed capability, per rail | `Power` interface | sink guarantees or characterized forms | **deny** |
| `i2c` | pull-up present; Rp window from VOL/IOL; rise time vs mode; speed; address uniqueness | `I2c` interface | measured Rp, summed Cb | **deny** (rise time: warn) |
| `derating` | capacitor voltage, resistor power (with temperature derating curve), inductor saturation current, semiconductor V/I/P | part category + project derating policy | envelope; `derated()` from the policy | **warn** |
| `thermal` | Tj = temp + θja·P within derated Tj max | part `theta_ja`, `operating { tj … }` | power measure + temp knob | **warn** |
| `budget` | total power per rail and per project (a *report*, not a pass/fail) | `Power` sums | characterized forms | info (a `pub let`, §6.5) |
| `lint` | heuristics with no number: decoupling cap near each IC supply pin, unused inputs floating | library lints | structure | **warn** |
| `bench` | bench outside the DUT's assumptions (§7.6); input without assumption (§3.6) | compiler | — | **warn** |

**Why these defaults.**
- Anything that **destroys** parts or makes a connection **not work** is an error: abs-max, ROC, polarity, contracts, logic levels, supply capacity, missing pull-ups.
- **Derating** is policy, not physics, and projects legitimately differ (consumer vs space), so it's a warning.
- **Heuristics** are lints, never specs (spec_design §8).

### 9.5 The derating policy

```toml
# Spicy.toml
[derating]
policy = "commercial"            # named policy shipped with the library; or "ecss", "custom"

[derating.custom]                # only with policy = "custom"
capacitor.ceramic.voltage  = 0.5
capacitor.aluminium.voltage = 0.8
resistor.power             = { factor = 0.5, above = "70°C", slope = "linear to 0 at t_max" }
semiconductor.tj           = "125°C"
```

- **Ratings are data; derating is policy.** This is PSpice Smoke's split ("MOC × derating factor = SOL", §14). Ratings live in part models (`v_rated`, `absolute_max`). Factors live in a swappable project policy.
- `derated(x)` asks the policy for the factor that applies to this part's category and conditions. The answer can depend on knobs (e.g. resistor power derating above 70 °C), so the hot corner is automatically the one that matters.
- Unlike Smoke, whose default is no derating, a new project starts with the `commercial` policy at level `warn`.
- The numbers above are illustrative. The shipped policies must be sourced from the standards: ECSS-Q-ST-30-11 (space derating), NASA EEE-INST-002, and a documented "commercial" rule set. See open question 9.

### 9.6 Levels, adjustments and waivers: the Rust lint model

Check names are paths, `group::category::name`, e.g. `derating::capacitor::v_rated`, like `clippy::needless_return`. The controls are the ones Rust already has:

```toml
# Spicy.toml: project levels, like Cargo's [lints] table (Rust 1.74)
[checks]
absmax    = "forbid"     # forbid: can't be waived anywhere below (safety projects)
derating  = "warn"
thermal   = "warn"
lint      = "allow"
```

```rust
#[allow(derating::capacitor::v_rated,
        reason = "X7R 25 V at 13.2 V (53%): DC-bias loss is modeled; accepted in review 2026-09-12")]
let c_in = Capacitor(10µF ± 10%, v_rated: 25V, dielectric: X7R);

#![warn(i2c::rise)]            // inner attribute: this contract only
```

| Action | Code | Scope |
|---|---|---|
| **Waive** | `#[allow(check, reason = "…")]`. The reason is **required**; the editor won't insert a waiver without one | the instance, the block, or the project |
| **Adjust** | `#[derating(voltage = 60%, reason = "…")]`. Overrides the policy for one instance; the check remains | instance |
| **Change level** | `#[warn(check)]` / `#[deny(check)]` / `#![…]` / `[checks]` | nested, innermost wins, as in Rust |
| **Expect** | `#[expect(check, reason = "…")]`. Known violation; warns if it disappears | instance |

Waivers are part of the design record (spec_design §8): they're in the source, in git blame, and in the sign-off report.

### 9.7 What the user sees

```
┌ CeAmp · Automatic checks ─────────────────────────── 11 pass · 0 warn · 0 fail · 0 waived ┐
│ group      instance  check              stress (worst)   limit                status level │
│ derating   c_in      v_rated            2.2 V            25 V → 20 V (80%)    ✓ 11%  warn  │
│ derating   rc        p_rated            9.0 mW           125 mW → 62 mW (50%) ✓ 14%  warn  │
│ thermal    q1        tj                 61 °C            150 → 125 °C         ✓      warn  │
│ absmax     q1        vce                5.3 V            40 V                 ✓      deny  │
│ polarity   c_in      reverse            +2.0 V           ≥ −1 V               ✓      deny  │
│ contract   output    z_load vs next     (in context)     —                    ✓      deny  │
│  ▸ show all 11     [waive…]  [adjust…]  [change level…]  [go to part]                     │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

(The stress values are illustrative, derived from the walkthrough's operating point.)

- Checks are **grouped and collapsed**: passing groups show one line, failing rows expand.
- Clicking a row selects the part on the schematic and shows its stress marker.
- **Waive** asks for a reason and a scope (this instance / this block / project), then inserts the attribute.
- Failing checks carry a counterexample, like any spec (§11).

---

## 10. Composition

### 10.1 A check at every connection, generated from the interface

For every connected net and every `#[by]` quantity of its interface:

```
combine(producers' guarantees on q)  ⊆  combine(consumers' assumptions on q)
```

| Quantity | Example | Check |
|---|---|---|
| `Power.v` (`one`) | regulator guarantees `vout.v in 12V ± 3%`; amp assumes `vcc.v in 12V ± 5%` | [11.64, 12.36] ⊆ [11.4, 12.6] ✓ |
| `Power.i` (`sum`) | amp draws ≤ 2 mA, filter ≤ 5 mA; regulator assumes `vout.i in 0A..=150mA` | Σ ⊆ ✓ |
| `Analog.z_load` (`parallel`) | next stage guarantees `input.z_load >= 20kΩ`; amp assumes `output.z_load >= 10kΩ` | ✓ |
| `Digital` levels | link checks (§9.2) | per driver/receiver pair |

This is the assume–guarantee composition rule [BCN+18]: a guarantee discharges the neighbor's assumption. An assumption that isn't discharged inside a parent **lifts** to the parent (§3.6).

### 10.2 Evidence ladder: declared → characterized → in-context

| Level | Uses | Cost | Keeps correlation? | When |
|---|---|---|---|---|
| **1. Declared** | the producer's published envelope spec (§5.7) vs the consumer's assumption | set comparison, instant | no | first, always |
| **2. Characterized** | the producer's **actual** result: an affine form in its knobs, with each port knob **substituted** by the neighbor's form (e.g. the regulator's load-current knob ← Σ of the downstream supply-current forms) | arithmetic on forms, no simulation | **yes**: shared `temp`, shared rails | when level 1 fails or is undecided, or no guarantee is published |
| **3. In-context** | flat simulation of the parent with both blocks | simulations | yes, exact | when level 2 is undecided; always before sign-off for connections flagged dynamic |

**Why substitution is sound here.**
- A block's characterized form is valid over the knob ranges it was verified on, which are its assumptions.
- The connection check (level 1 or 2) proves the substituted quantity stays inside those assumptions.
- So **reuse is valid exactly when the connection check passes**: no extrapolation beyond the characterized box. The first-order error term travels with the form, since the bracket carries `err`.

The result card's "method" column says which level decided each check ("composed from characterized results, no simulation").

**System-level specs reuse block results the same way:**

```rust
contract SensorBoard {
    /// Battery life budget.
    spec v3v3.i <= 12mA;       // summed from children's characterized `vdd.i` forms: no board simulation needed
}
```

### 10.3 The loading caveat

- A block's guarantees depend on what's connected to it (v1 §5.6). Here that dependence is **declared**: the producer's specs were verified over its assumed `z_load` / `c_load` range, and the consumer's input impedance is a guarantee (measured by `impedance(input)`).
- The connection check verifies the neighbors actually present those impedances. Static loading is fully covered by levels 1–2.
- **Dynamic interaction** (the regulator's output impedance resonating with a downstream input capacitor, two feedback loops interacting) isn't captured by scalar impedances. Such connections are marked **dynamic**, by a library rule (e.g. `Power` with a regulating source and capacitive sinks) or by the user, and they always get level 3 before sign-off.

### 10.4 Contract algebra, used for real features

Following [BCN+18] (equations as in Inria RR-8147, §VII-A):
- **Contract:** C = (A, G). Its **saturated** form replaces G by G ∨ ¬A, i.e. behaviour outside the assumptions is unconstrained. That's why a spec's conditions can't widen the contract (§5.4).
- **Refinement:** C′ ≼ C iff A′ ⊇ A and G′ ⊆ G. The refining contract assumes less and guarantees more.
- **Composition:** G = G₁ ∧ G₂ and A = (A₁ ∧ A₂) ∨ ¬(G₁ ∧ G₂). Assumptions that the other component's guarantees discharge drop out, and the rest lift (§3.6).

In the product:

| Feature | Contract operation |
|---|---|
| Connection checks | composition compatibility |
| Lifted assumptions on the parent's I/O page | undischarged assumptions of the composite |
| **Part substitution / second source** ("can this 25 V, ±10% cap replace that one?") | refinement check between the two parts' datasheet contracts: wider operating conditions, tighter characteristics. Instant, no simulation |
| Library block reuse | a block verified standalone over wide assumptions refines its use in any narrower context |

---

## 11. Results in the language

### 11.1 Inlay hints, hover, diagnostics

- **Inlay hints** (LSP) after each `spec` and `assume` line show the status glyph, the bracket, the confidence and the top contributors (§4). Staleness shows as a dimmed hint with ⟳ when the design changed since the last run.
- **Verdicts:** the four of spec_design §9 (PASS guaranteed / PASS estimated / FAIL / UNDECIDED). These map onto SysML v2's {pass, fail, inconclusive, error} for export (§14).
- **Margin:** the signed distance from the bracket to the bound. This is STL's robustness idea (§14), in the spec's units and as a percentage of the limit.
- **Near:** a pass within 10% of a limit is flagged *near*, like PSpice Smoke's yellow 90–100% band. For automatic checks the table shows % of the (derated) limit, as Smoke does.
- **Not exercised:** a spec whose conditions or crossings never occur in its bench is UNDECIDED with *"not exercised"*, never PASS.
- **Hover** shows the full result card:
  - nominal, and worst-case vs realistic;
  - method (exact corners / guaranteed AA / worst-point loop with N simulations / Monte Carlo k of N);
  - the contributors table;
  - bench coverage of the envelope;
  - the counterexample.
- **Diagnostics** follow the Spade/rustc style. Spade's own assert reports the source span with "This expression is false" (`spade/spade-mir/src/assertion_codegen.rs:12-22`), and it lowers `assert` to a simulation-time SystemVerilog assertion (`spade/spade-mir/src/codegen/mod.rs:1262-1308`). We keep the source-span presentation and add the engine's evidence:

```
error[spec::bass]: `bass` fails at 99.9% yield: f_low reaches 31.7 Hz (limit 30 Hz)
  --> ce_amp.spc:21:5
   |
21 |     spec bass: h.f_low(-3dB) <= 30Hz;
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ 0.9 % of boards fail (Monte Carlo 177 / 20 000 at the cold, end-of-life corner)
   |
note: almost all of the spread comes from `c_in`: its ±20 % tolerance plus −20 % aging at end of life
  --> ce_amp.spc:6:16
   |
 6 |     let c_in = Capacitor(1µF ± 20%, aging: -20% at life.end);
   |                ---------------------------------------------- tolerance + aging
   = counterexample: corner { temp: -10°C, life: 10y, c_in.tol: -20%, q1.hfe: 100, .. }  →  33.4 Hz
   = help: f_low ∝ 1/C_in: 2.2 µF → worst case 15.2 Hz; 1 µF film ±5 % → 22.5 Hz
```

(Numbers from walkthrough §6.)

### 11.2 Counterexamples link back

- A counterexample is a **knob assignment**, printed as a `corner` literal that can be pasted into a bench (`at: …`).
- **Code actions** on a failing spec:
  - **Simulate at counterexample:** opens the schematic in simulate mode with the knobs set, and the waveforms/values in place (vision §3).
  - **Pin as corner:** adds `corner bass_fail { … }`, re-checked first on every run (§7.6).
  - **Show contributors on schematic:** highlights the parts, sized by share.
  - **Ask AI:** sends the spec, its rationale, the result, the contributors and the counterexample (spec_design §10).
  - **Try a value:** what-if from the saved sensitivities (the "fixes the engine can check instantly" of walkthrough §6), previewed without editing.

### 11.3 Where results live

- Results are never written to the source.
- They're cached per `(spec path, design hash, engine version)` in the project's results store.
- A **sign-off snapshot** (`signoff.lock`, committed, like `Cargo.lock`) records the verdicts, methods and waivers for a released revision. Opening an old revision shows its signed-off results.
- `spicy check` prints the same diagnostics in the terminal, and `--json` gives the AI a structured form.

---

## 12. The editor

### 12.1 The I/O page is the contract

| Vision I/O-page item | Language construct | Panel |
|---|---|---|
| Valid ranges | `assume` on port quantities; `absolute_max` / `operating` from parts | Assumptions |
| Required inputs | `assume input in Sine {…}` patterns | Assumptions → Inputs |
| Expected outputs | `spec` on produced port quantities | Guarantees |
| Signal driving | benches and fixtures | Benches |
| Composition | generated connection checks, lifted assumptions | Connections |
| Simulation config | bench analysis fields | Benches |

```
┌ CeAmp · Contract ───────────────────────────────────────────────────────────────────────┐
│ ASSUMPTIONS                                                  from            knob       │
│  temp     −10 … 60 °C                                         Top (inherited) range      │
│  life     0 … 10 y                                            Top (inherited) range      │
│  vcc.v    12 V ± 5 %                                          this block      range      │
│  input    sine ≤ 100 mV, 20 Hz … 20 kHz · Zs ≤ 1 kΩ           this block      range ×3   │
│  output   Zload ≥ 10 kΩ                                       this block      range      │
│ GUARANTEES                                                                               │
│  name  measure              bench    require       conf    status                        │
│  bias  dc(output.v)         default  4.5 … 6.5 V   3σ      ✓ max 6.34 · worst-case ✗     │
│  gain  |H(1 kHz)|           default  4.6 ± 5 %     3σ      ✓ 4.52 … 4.67                 │
│  bass  H.f_low(−3 dB)       default  ≤ 30 Hz       99.9 %  ✗ 0.9 % fail                  │
│  clip  thd(output.v)        loud     < 1 %         wc      ▶ run                          │
│ AUTOMATIC CHECKS   11 pass · derating 4 · absmax 2 · thermal 1 · polarity 1 · contract 3 │
│ BENCHES            default (auto) · loud (form)                                          │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

### 12.2 On the schematic

- **Probes.** In simulate mode (vision §2), clicking a net creates `let <name> = dc(<net>.v);` and a marker. Two nets make a difference. A part gives its current or power. The marker is attached to the binding name.
- **Status in place.** Each probe shows its specs' glyph and worst value (spec_design §10). Parts with automatic-check stress show a small gauge (stress / limit).
- **Selecting a failing spec** highlights its contributors and offers the counterexample. This is the context for "select to ask" (vision §1).

### 12.3 Table ↔ code, both ways

1. **Identity.** Each row is one item (`assume`, `spec`, `bench`, `corner`), keyed by name. Renaming in the table is a rename refactor (updates references and waivers).
2. **One home per column** (§5.1). A table edit becomes a **minimal AST edit** followed by the canonical formatter, like rust-analyzer assists followed by rustfmt. Examples:

   | Table edit | Code change |
   |---|---|
   | gain: `4.6 ± 5 %` → `4.6 ± 3 %` | `… in 4.6 ± 3%;` |
   | gain: confidence → worst case | inserts `#[confidence(worst_case)]` above `spec gain` |
   | gain: why → "next stage expects this level" | inserts or replaces the `///` line |
   | clip: bench → `default` | removes ` on loud` |
   | bass: condition → "new parts only" | appends ` where life == 0y` |
   | auto check: waive | inserts `#[allow(…, reason = "…")]` at the chosen scope |

3. **Shared measures.** If a spec's measure is a shared binding (`h`, used by `gain` and `bass`), editing the *measure* cell asks "change `h` (affects gain, bass) or give `gain` its own measure?". Benches don't have this problem, because `on BENCH` is a clause, not part of the measure (§5.1).
4. **Canonical order** inside a contract: assumptions → measures → guarantees → benches → corners. New rows go in their section.
5. **Read-only rows.** What the table can't represent losslessly (loop-generated specs, computed bounds, `fn` measures) shows as read-only (grouped) rows with "edit in code", the same rule as generated blocks (v1 §7.5 rule 5).
6. **Provenance.** Inherited and lifted values show greyed out with their source ("Top", "required by amp"). Pinning one writes the `assume` line.
7. **Undo/redo** is one history across the table, the code and the schematic, since all three edit one model (D5).

### 12.4 What this gives the AI

The language is the AI's main interface, and these choices serve it:
- **One canonical form per concept**, enforced by the formatter.
- **Types that make valid measures discoverable** (§6.3).
- **Role errors that catch ⊆/⊇ mistakes** (§3.2).
- **Contracts local to each block**, so reasoning is per block.
- **Counterexamples as pasteable code** (§11).
- **`///` rationales** that say *why* a limit exists.

---

## 13. Worked examples

### 13.1 Buck converter

```rust
block Buck(vin: Power, vout: Power<Source>) {                  // (core syntax: placeholder)
    let ctrl  = Tps54x();                                      // §9.3
    let l1    = Inductor(10µH ± 20%, i_sat: 3.2A, dcr: 30mΩ ± 10%);
    let c_out = [Capacitor(22µF ± 10%, v_rated: 10V, dielectric: X7R); 2];
    let c_in  = Capacitor(10µF ± 10%, v_rated: 25V, dielectric: X7R);
    let r_top = Resistor(31.2kΩ ± 0.1%);                       // 0.8 V · (1 + 31.2/10) = 3.296 V
    let r_bot = Resistor(10kΩ ± 0.1%);
    net fb = [ctrl.fb, r_top.b, r_bot.a];
    // … compensation, bootstrap, nets …
}

contract Buck {
    // ── Assumptions ──
    assume temp in -20°C..=85°C;                 // wider than the product: a reusable block
    assume vin.v in 9V..=15V;
    assume vin.z_src <= 50mΩ;
    assume vout.i in 0A..=2A;
    assume vout.c_load <= 100µF;                 // downstream decoupling allowed on the rail

    // ── Measures ──
    let p_out = tran(vout.p).window(2.5ms..=3ms).avg();
    let p_in  = tran(vin.p).window(2.5ms..=3ms).avg();

    // ── Guarantees ──
    /// Published envelope: what downstream blocks can rely on at all times,
    /// including ripple and load steps.
    spec vout.v in 3.3V ± 4%;

    /// Reference ±1% plus divider ±0.1%, plus load regulation.
    spec accuracy: dc(vout.v).vs(vout.i) in 3.3V ± 1.5% on averaged;

    /// The ADC reference downstream tolerates 20 mV p-p.
    spec ripple: tran(vout.v).window(2.5ms..=3ms).pp() <= 20mV on steady;

    /// Downstream brown-out threshold is 3.1 V.
    spec droop: tran(vout.v).window(1ms..).min() >= 3.3V - 100mV on load_step;

    /// Back within 1% in 100 µs. Non-smooth as written; evaluated via §6.4.
    spec recovery: tran(vout.v).settling_time(1%, from: 1ms) <= 100µs on load_step;

    /// Loop stability across Cout tolerance, DC bias and aging.
    #[confidence(worst_case)]
    spec phase_margin: ac(loop_gain(fb)).phase_margin() >= 45° on averaged;
    #[confidence(worst_case)]
    spec gain_margin:  ac(loop_gain(fb)).gain_margin()  >= 10dB on averaged;

    /// Thermal budget of the enclosure.
    #[warn]
    spec efficiency: p_out / p_in >= 88% on steady where vout.i in 0.5A..=2A;

    // ── Benches ──
    bench steady    { vout: Load::current(0A..=2A), tran: 0ms..=3ms, ..default }
    bench load_step { vout: Load::step(from: 0.2A, to: 2A, at: 1ms, rise: 1µs), tran: 0ms..=1.5ms, ..default }
    bench averaged  { models: Averaged, ..default }       // AC and DC on the averaged switch model
}
```

**What's automatic here (no lines written):**

| Check | What it finds |
|---|---|
| `absmax::Tps54x::sw` | the switch node's ringing in `load_step`, via `.time_outside(-1V..=42V)` per excursion vs "−3 V for < 10 ns" |
| `derating::inductor::i_sat` | peak inductor current (2 A + half the ripple) vs the policy's derated I_sat. This is exactly the kind of check that comes out UNDECIDED and makes the engine simulate the worst corner |
| `derating::capacitor::v_rated` on `c_in` | 15 V on a 25 V X7R = 60% against a 50% ceramic policy → **warn**. The engineer waives with a reason (§9.6) or picks a 35 V part |
| `operating::Tps54x::tj` | temp + θja·P_ctrl at 85 °C and 2 A |
| `contract` at `vout` | at the parent: every sink's assumed `vcc.v` ⊇ [3.168, 3.432] V, and Σ sink `i` ⊆ [0, 2] A |

**Notes:**
- **Two model levels, one design.** The `averaged` bench selects the controller's averaged model, so AC loop gain is meaningful. Ripple and droop use the switching model. This sidesteps, rather than settles, the averaged-vs-periodic-steady-state question (v1 open question); PSS would add a `pss(…)` analysis later.
- `vout.p` has the role-relative sign (§6.1): positive power is delivered **out** of a source port.
- Efficiency is a `#[warn]` target. Below 0.5 A it's excluded by `where`, which the table shows as a condition.

### 13.2 MCU + I2C sensor

```rust
block SensorNode(v3v3: Power) {                                   // (core syntax: placeholder)
    let mcu    = Mcu();               // I2C pins: Digital<OpenDrain>, VIH = 0.7·VDD, 10 pF (illustrative)
    let sensor = TempSensor();        // VDD 1.62…3.6 V; SDA abs-max = VDD + 0.3 V (illustrative)
    let ldo18  = Ldo(1.8V);
    let rp_scl = Resistor(10kΩ ± 1%);
    let rp_sda = Resistor(10kΩ ± 1%);
    let bus    = I2c::bus(mode: Fast);
    // v1: sensor on the 1.8 V LDO, pull-ups to 3.3 V
    net v18 = [ldo18.vout, sensor.vdd];
    connect(mcu.i2c, bus, sensor.i2c);
    net pu  = [v3v3.hi, rp_scl.a, rp_sda.a];
    // …
}

contract SensorNode {
    assume v3v3.v in 3.3V ± 5%;
    assume v3v3.z_src <= 0.5Ω;
    assume bus.trace_c in 10pF..=30pF;           // layout estimate; a knob like any other

    /// Coin-cell budget: the upstream LDO is rated 150 mA, the battery wants ≤ 12 mA.
    spec v3v3.i <= 12mA;

    /// Sleep current for the battery-life estimate.
    spec sleep_i: dc(v3v3.i) <= 20µA on sleep;

    bench sleep { models: Sleep, ..default }      // parts' `Sleep` model variants
}
```

The engineer wrote a handful of lines, none of them about logic levels or pull-ups. The automatic checks do the rest. On the first version they report:

```
error[absmax::TempSensor::sda]: `sensor.sda` exceeds its absolute maximum
   = SDA is pulled to v3v3 (3.465 V max); abs-max is VDD + 0.3 V = 2.01 V at VDD = 1.71 V
   = counterexample: corner { v3v3.v: 3.465V, ldo18.vout.tol: -5%, .. }
   = help: pull up to the sensor's rail, or add a level translator

warning[i2c::rise]: SCL rises too slowly for Fast mode
   = 0.8473 · Rp · Cb = 0.8473 · 10.1 kΩ · 50 pF = 428 ns > 300 ns
   = Cb: mcu.scl 10 pF + sensor.scl 10 pF + trace 10…30 pF
   = help: Rp ≤ 7.08 kΩ at Cb = 50 pF
```

The obvious fix is to pull up to 1.8 V. The next check then catches the MCU side:

```
error[logic::high]: `sensor.sda` (open drain, pulled to 1.8 V) can't reach `mcu.sda`'s input-high threshold
   = voh = 1.71 V (1.8 V − 5 %, leakage negligible)  <  vih = 0.7 · v3v3.v = 2.43 V (at 3.465 V)
```

The real fix is to run the sensor from `v3v3` (within its 1.62…3.6 V ROC: `operating` ✓) with 4.7 kΩ pull-ups. After that fix:

| Check | Result |
|---|---|
| `i2c::rp_min` | Rp ≥ (3.465 − 0.4) / 3 mA = 1.02 kΩ ✓ |
| `i2c::rise` | 0.8473 · 4.75 kΩ · 50 pF = 201 ns ≤ 300 ns ✓ |
| `logic::high` / `low` | ✓ on both lines, correlated through the shared rail |
| `power` at the upstream LDO | Σ sink currents ⊆ its 150 mA ✓ |
| `spec v3v3.i` | composed from the MCU's and sensor's datasheet current spreads plus the pull-up current with lines low (2 × 3.465 V / 4.65 kΩ) ✓ |

(Part values are illustrative, not from specific datasheets. The I2C limits are UM10204's.)

### 13.3 8-channel parametric front end

```rust
block Channel(input: Analog<In>, output: Analog<Out>, vdd: Power, g_nom: f64) { … }

contract Channel {
    assume input in Sine { amp: ..=200mV, freq: 0Hz..=10kHz };
    assume output.z_load >= 5kΩ;

    let h = ac(output.v / input.v);
    spec gain: h.at(1kHz).mag() in self.g_nom ± 1%;
    /// The downstream ADC's digital filter assumes a flat passband.
    spec flat: h.band(10Hz..=10kHz).db() in h.at(1kHz).db() ± 0.1dB;
}

block Afe<const N: usize>(vdd: Power, inp: [Analog<In>; N], out: [Analog<Out>; N]) {
    let ch = [Channel(g_nom: 10.0); N];                             // (core syntax: placeholder)
    let rn = ResistorNetwork::<N>(10kΩ ± 0.1%, ratio_match: 0.01%); // shared lot knob (v2 Part B)
    // …
}

contract<const N: usize> Afe<N> {
    assume vdd.v in 5V ± 5%;

    let g = |i: usize| ac(out[i].v / inp[i].v).at(1kHz).mag();

    for i in 1..N {
        /// Differential measurements downstream need matched channels.
        spec matching[i]: g(i) / g(0) in 1 ± 0.05%;
    }
    for i in 0..N {
        for j in 0..N {
            if i != j {
                spec xtalk[i][j]: ac(out[j].v / inp[i].v).band(10Hz..=10kHz).db() <= -80dB;
            }
        }
    }
    for i in 0..N {
        /// A neighbour's overload must not disturb this channel.
        spec recover[i]: tran(out[(i + 1) % N].v).window(3ms..).abs().max() <= 10mV on overload(i);
    }

    #[allow(bench::outside_contract, reason = "overload test: 10× the assumed amplitude")]
    bench overload(k: usize) {
        inp: one_hot(k, Sine { amp: 2V, freq: 1kHz }, Dc(0V)),
        tran: 0ms..=5ms,
        ..default
    }
}
```

- **Per-channel specs** (`gain`, `flat`) are written once in `Channel`. They appear as `ch[0..8].gain` in `Afe`'s table: one group row, showing the worst channel.
- **Loop specs** are relations between channels:
  - `matching[1..8]` (7 rows);
  - `xtalk[i][j]` (56 rows, one group);
  - `recover[0..8]`.
- **Why `matching` passes with a network and not with discrete parts.** Each gain depends on its channel's resistors. With a matched network, the network's shared lot knob moves all channels together, so it cancels exactly in `g(i) / g(0)`. Only the small per-part ratio knob remains. With eight independent ±0.1% resistor pairs, nothing cancels.
  - The affine forms get both cases right automatically (walkthrough §8).
  - An interval method gets the discrete case right, because every resistor appears once. It gets the network case wrong, because the lot knob appears in every term and would be counted as independent each time.
- **Crosstalk needs no special bench.** `ac(out[j].v / inp[i].v)` excites only `inp[i]` (the denominator names the excitation, §6.2). The engine gets all N² transfer functions from one factorization per frequency, reusing the adjoint idea of v2 A8.
- **The transient crosstalk** (`recover`) needs a parametric bench, `overload(k)`, which is a bench with a parameter.
- **Parametric** in `N`: `Afe::<4>` gets 3 + 12 + 4 loop rows, with no edits.

---

## 14. Prior art

Each entry below was checked against a primary source (vendor documentation, standard, paper or source code). Items marked *unverified* could not be confirmed.

### 14.1 Comparison

| | Measure | Bench | Requirement | Variations | Statistics | Waivers / severity | **What we take** |
|---|---|---|---|---|---|---|---|
| **Cadence ADE** (Explorer / Assembler / Verifier) | calculator expressions, e.g. `bandwidth(VF("/OUT"), 3, "low")` | test = testbench + analyses; several testbenches per spec set | specs on outputs, color-coded status | corners, sweeps, run plans | Monte Carlo, yield, k-sigma corners | — | spec ≠ testbench; statistical samples promoted to named corners (→ `corner`, §7.6); a Verifier layer mapping top-level requirements to tests (→ `#[req]`) |
| **PSpice AA Smoke** | average / RMS / peak stress | transient run only | %Max = actual / SOL | — | — | derating files; `SMOKE_ON_OFF=OFF` per instance or block, inherited | part limits (data) ≠ derating (policy) (§9.5); report **% of limit** with a *near* band; scoped exclusion (→ `#[allow]`, but with a required reason) |
| **SPICE `.meas`** (ngspice, LTspice) | TRIG/TARG, FIND/WHEN/AT, AVG/RMS/MIN/MAX/PP/INTEG/DERIV, FROM/TO | the netlist | none built in (ternary `param`) | `.step` (LTspice) | — | — | the measurement vocabulary and its exact semantics (§6.3) |
| **ngspice SOA** (`.option warn=1`) | per-device limits checked at each Newton convergence | any .op / .dc / .tran | model params `Vce_max`, `Pd_max`, … | — | — | `maxwarns` | precedent for envelope stress checks; an import path from model cards into `absolute_max` |
| **Verilog-AMS** | `cross`, `above`, `last_crossing` events | the module | procedural checks + `$error/$warning/$info` | — | — | severity tasks | crossing checks must **control the time step** (§6.4) |
| **PSL / SVA** | temporal properties | testbench / formal | `assert` (design) vs `assume` (environment) vs `cover` (reachable) | — | — | — | the assume/assert split (= `assume`/`spec`); `cover` → vacuity and coverage (open question 1) |
| **STL** | temporal logic over real-valued signals | — | robustness = signed distance to violation | — | — | — | margin as signed distance (§11.1); a later extension for sequencing (open question 7) |
| **atopile** | parameters (no simulation) | — | `assert x within / is / < / >` | worst-case over ranges | — | three-valued design checks | ⊆ vs ⊇ as distinct relations, but made **visible** (§5.3); three-valued outcomes |
| **PolymorphicBlocks** | port parameters (no simulation) | — | `self.require(expr, "msg")`; link checks | interval worst case | — | — | interfaces with source/sink parameters and link aggregation (§9.2); `actual_*` vs spec naming shows the ⊆/⊇ split |
| **SysML v2** | constraint expressions | `verification def` | `requirement def` with `subject`, `assume constraint`, `require constraint`; `satisfy`, `verify` | — | — | verdict {pass, fail, inconclusive, error} | the vocabulary aligns 1:1; an export path for traceability |
| **A/G contracts** | — | — | C = (A, G); refinement; composition | — | — | — | the semantic backbone (§3, §10) |
| **Modelica Requirements** | blocks over signals | the model; `experiment` annotation | `Property` = {Violated, Undecided, Satisfied}; `during(cond, check)` | — | — | — | "never exercised" ≠ pass; settings travel with the test as data |
| **Simulink Requirements Table** | — | the model | rows: precondition → postcondition | — | — | — | a separate **Assumptions** tab: a tabular A/G UI, like our I/O page |

### 14.2 Notes and sources

**Cadence Virtuoso ADE.**
- The ADE Assembler datasheet (Cadence 16275, 05/21) defines a specification as "all required tests, analyses, and operating conditions for validation against a measured set of goals". It lists:
  - "Support for corners, parametric sweeps, Monte Carlo, and reliability analysis across multiple tests";
  - "worst-case corners and statistically derived corners";
  - run plans that chain tests;
  - "quick color-coded feedback of all results against target specifications".
- ADE Verifier "is designed to match the highest level circuit specifications with individual analysis tests".
- ADE design variables "are always global to the design" (ADE L User Guide 6.1).
- *Unverified:* the exact spec operators (`<`, `>`, `range`, `tol`, …) and the definition of "near".
- **Lesson:** ADE's unit is spec = measure + bound, evaluated over tests × corners × sweeps × MC. We copy that structure, but as text, not opaque GUI state.

**PSpice Advanced Analysis** (User's Guide v10.5, 2005).
- **Smoke** computes Safe Operating Limits as "MOC × derating factor = SOL" and reports %Max = actual/SOL. Its bars are red above the SOL, yellow at 90–100%, green below 90%, and grey when no limit is known.
- It runs on **transient only**. The default is no derating, and custom `.drt` files hold factors per parameter.
- Parameter examples: resistor `RMAX` (power, default 0.25 W), `RTMAX`; capacitor `CMAX` (voltage, 50 V), `CTMAX`; BJT `IC`, `VCE`, `PDM`, `TJ`.
- `SMOKE_ON_OFF=OFF` excludes an instance or block, and the parent's value takes priority.
- **Lesson:** we keep that split (ratings in part models, factors in a policy) and the %-of-limit display. We add what Smoke lacks: every large-signal analysis rather than transient only, worst-case over knobs, and a written reason on every exclusion.

**SPICE `.meas`.**
- ngspice manual v47, §11.4 "Measurements after AC, DC and Transient Analysis" (numbered §15.4 in older manuals):
  - `TRIG … TARG` with `RISE / FALL / CROSS = n | LAST` and `TD`;
  - `FIND … WHEN / AT`;
  - `AVG | MIN | MAX | PP | RMS | MIN_AT | MAX_AT … FROM TO`, `INTEG`, `DERIV`;
  - `param='expr'` over earlier results. The manual's own pass/fail idiom is a ternary `param`.
- LTspice `.MEASURE` runs once per `.step` and tabulates the results. It has no pass/fail keyword.
- ngspice §11.5 adds **SOA checks**: `.option warn=1` checks model-card limits (`Vce_max`, `Ic_max`, `Pd_max`, `Vds_max`, …, all defaulting to infinity) after every Newton convergence, i.e. at each transient step.
- **Lesson:** our measurement methods adopt `.meas` semantics exactly (edge counts, `LAST`, `TD`, windows, and AC comparisons on the real part where `.meas` does so). We add typed bounds and statuses on top.

**Verilog-AMS LRM 2.4.0** (Accellera, 2014).
- `cross(expr, dir, time_tol, expr_tol)` (§5.10.3.1) and `above` (§5.10.3.2) are monitored events that **also control the simulator's time step**.
- `$bound_step` (§9.17.2) caps it. Severity tasks `$fatal / $error / $warning / $info` are §9.7.3.
- The LRM has no `assert property` / `assume` / `cover`.
- **Lesson:** a crossing measured on a coarse fixed-step waveform is a simulator artifact. Class-C measures must be able to request step control around their crossings (§6.4).

**PSL (IEEE 1850-2010) / SVA (IEEE 1800-2023).**
- The standard directive split: `assert` is an obligation on the design, `assume` a constraint on the environment, and `cover` a scenario that must be reachable.
- `restrict` constrains the formal state space only.
- *Unverified:* clause numbers (the standards are paywalled).
- **Lesson:** our `assume` / `spec` is exactly this split. `cover` suggests reporting vacuous or unexercised specs (§11.1, open question 1).

**STL.**
- Maler & Nickovic, "Monitoring Temporal Properties of Continuous Signals," FORMATS/FTRTFT 2004, LNCS, pp. 152–166, doi:10.1007/978-3-540-30206-3_12.
- Robust satisfaction: Donzé & Maler, FORMATS 2010, pp. 92–106, doi:10.1007/978-3-642-15297-9_9.
- **Lesson:** robustness (signed distance to violation) is the principled meaning of "margin".

**atopile** (commit `619eda7`).
- `assert` takes exactly one comparison: `within` lowers to `IsSubset`; `is` with a literal is deprecated in favor of `within` (`atopile/src/atopile/compiler/ast_visitor.py:1305-1353`). Literals are `a to b` and `a +/- tol` (`atopile/src/atopile/compiler/parser/AtoParser.g4:271-275`).
- Assignments are ⊆ in modules and ⊇ in components (§5.3).
- Design checks run in stages (`POST_INSTANTIATION_*`, `POST_SOLVE`, `POST_PCB`) and can raise a definite or a *maybe* failure (`atopile/src/faebryk/library/implements_design_check.py:21-35`).
- Library equations are hand-rearranged "for the solver" (`atopile/examples/equations/equations.ato:39-45`), which is the dependency problem of v2 A2.
- **Lesson:** keep the ⊆/⊇ relations and the three-valued outcome, but make the relation visible on the line.

**PolymorphicBlocks** (UIST '20, doi:10.1145/3379337.3415860; code `588b73b`).
- Ports carry device parameters: sources have `voltage` and `current_limits`, sinks have `voltage_limits` and `current_draw`.
- Links aggregate and assert:
  - `VoltageLink` requires `voltage_limits.contains(voltage)` and `current_limits.contains(current_draw)` (`PB/edg/electronics_interfaces/VoltagePorts.py:66-71`);
  - `DigitalLink` requires `output_thresholds.contains(input_thresholds)` (`DigitalPorts.py:107-117`);
  - `I2cLink` requires a pull-up and unique addresses (`I2cPort.py:35-44`).
- Blocks add checks with `self.require(expr, "message")` (`PB/edg/core/Blocks.py:495-508`).
- Spec vs part is a naming convention (`resistance` vs `actual_resistance`, `PB/edg/abstract_parts/Resistor.py:56-61`), with opposite containment for tolerance-type and rating-type checks.
- **Limits:** everything is static interval worst case (no simulation, no correlation; `a - b` widens). The paper's user study needed a `ForcedCurrentDraw` override because worst-case current sums over-constrained.
- **Lesson:** the interface model is the right shape. Our additions are simulation, correlation through shared knobs, and the abs-max / operating split.

**SysML v2** (OMG SysML 2.0, formal, September 2025).
- Requirements are `requirement def` with a `subject`, `assume constraint { … }` and `require constraint { … }`, and a `doc` comment.
- A design claims one with `satisfy R by design`. A `verification def` with `objective { verify R; }` returns a `VerdictKind` of {pass, fail, inconclusive, error}. Example: `training/32. Requirements/Requirement Definitions.sysml` in github.com/Systems-Modeling/SysML-v2-Release.
- **Lesson:** the mapping is direct. Contract ≈ requirement def with subject = block; `assume` / `spec` ≈ `assume` / `require constraint`; bench ≈ verification def; `///` ≈ `doc`; PASS / FAIL / UNDECIDED / error ≈ the four verdicts. We get an export for systems-engineering traceability without adopting SysML's ceremony in the editor.

**Assume–guarantee contracts [BCN+18].**
- Definitions from the open precursor, Inria RR-8147 (2012), §VII-A:
  - A contract is C = (A, G), with saturated form G′ = G ∨ ¬A.
  - Refinement: C′ ≼ C iff A′ ⊇ A and G′ ⊆ G.
  - Composition: G = G₁ ∧ G₂ and A = (A₁ ∧ A₂) ∨ ¬(G₁ ∧ G₂).
  - Conjunction: (A₁ ∨ A₂, G₁ ∧ G₂).
- The closest electrical application is Nuzzo et al., "A Contract-Based Methodology for Aircraft Electric Power System Design," *IEEE Access* 2:1–25, 2014, doi:10.1109/ACCESS.2013.2295764. It places contracts "at the articulation points in the design flow".
- **Lesson:**
  - Saturation is the formal version of "a block isn't responsible outside its assumptions" (§5.4: conditions can't widen the contract).
  - Composition is our lifting rule (§3.6).
  - Refinement gives the part-substitution check (§10.4).

**Modelica.**
- Otter et al. (13 authors), "Formal Requirements Modeling for Simulation-Based Verification," 11th Int. Modelica Conf., 2015, pp. 625–635, doi:10.3384/ecp15118625:
  - a three-valued `Property = enumeration(Violated, Undecided, Satisfied)`;
  - `during(condition, check)`;
  - a requirement that stays Undecided for a whole run is reported as *untested*;
  - an electrical example, `Requirement R2(property = during(MPSVoltage < 160, check = Off), …)`.
- Modelica 3.6 §18.4: the `experiment(StartTime, StopTime, Interval, Tolerance)` annotation carries simulation settings in the model, and `TestCase(shouldPass = …)` marks test models.
- **Lesson:** "never exercised" must not read as PASS (§6.4, §11.1), and bench settings travel as data (§7.5).

**Simulink Requirements Table** (R2022a+). Rows run precondition → postcondition, and a separate **Assumptions** tab "constrain[s] requirements based on the physical limitations of your model". It's a tabular assume–guarantee UI, and the closest commercial analogue to our I/O page.

**What nobody has.** No tool found combines:
- interface-level contracts (PolymorphicBlocks, SysML);
- simulation-based measures with worst-case and statistical evaluation (ADE, PSpice);
- stress checks with written, scoped waivers (Smoke has scope but no reason);
- all of the above in one textual, diffable language that the schematic editor edits as a table.

That combination is the proposal.

---

## 15. Alternatives considered

| Alternative | Why not (or not yet) |
|---|---|
| **Specs as test functions** (`#[test] fn gain() { assert!(…) }`, like Rust tests or Spade's cocotb benches) | Imperative; the engine can't see the measure/requirement structure it needs for bounds, and there is no table mapping |
| **atopile's `assert x within y` everywhere**, with ⊆/⊇ decided by block kind | The meaning flips invisibly (§5.3) |
| **SPICE-style probes `V(out)`, `I(R1)`** | Look like Rust constructors, collide with the unit `V`, and duplicate interface quantities (§6.1). The formatter could accept them as input aliases |
| **`because "…"` clause for rationale** | Not Rust, single-line, table-hostile (§5.6) |
| **Bench as a method receiver only** (`loud.tran(x)`) | Ties shared measures to one bench and makes the table's bench column a measure edit. Kept as the explicit form for measures that mix benches |
| **Only ADE-style testbenches** | Too heavy for the common case, and they drift from the assumptions (§7.1) |
| **Only in-block setups** | Can't express realistic fixtures or multi-DUT benches (§7.1) |
| **`for f in band` as a quantifier** | Rust `for` iterates; it would read as sampling (§8.1) |
| **Separate keywords `check` / `rating` for automatic specs** | A second mechanism. Assumptions with `#[check(group)]` cover it, and the part sections (`absolute_max`, `operating`) give the datasheet-shaped sugar |
| **SysML v2-style requirement objects** (`requirement def` + `satisfy`) | Separates requirements from the design, which is good for traceability but heavy for the circuit engineer's loop. `#[req("ID")]` gives the link without the ceremony |
| **Signal temporal logic (STL) / PSL for transient specs** | Powerful but alien to most EEs; the measurement library covers the common specs. STL is the natural extension for sequences (power sequencing: "rail B rises ≥ 1 ms after rail A") and is listed as open question 7 |
| **Specs in a side table (YAML / ADE state)** | Loses types and refactoring; the AI can't edit them with the same tools |

---

## 16. Open questions

1. **Bench coverage of envelopes.** An envelope spec or automatic check is only as good as the benches that feed it (§5.7). Should the engine require, for example, a start-up transient before signing off abs-max, or report "coverage" like digital verification? SVA's `cover` directive is the precedent: a scenario that must be shown reachable.
2. **Roles that are neither producer nor consumer.** A clamp at an input effectively guarantees something about a quantity its role doesn't produce. Allow `spec` on a consumed quantity with an explicit `#[allow(role)]`, or model clamps differently?
3. **Circular assumptions.** Block A's guarantee depends on B's output and vice versa (a supply whose output depends on the load, whose current depends on the supply). Levels 1–2 of §10.2 can loop. Options: a fixed-point iteration on forms, or forcing level 3.
4. **Interface impedance vs frequency.** `z_src` / `z_load` as scalars cover static loading. Should interfaces carry impedance *curves* so AC loading is also checkable at level 2?
5. **Per-spec yield vs product yield.** `yield(99.9%)` per spec doesn't bound the probability that *all* specs pass. Is a block- or product-level yield target needed?
6. **Discrete modes.** `mode in {Run, Sleep}` as a discrete range knob works for enumerated modes. For many modes (an MCU's peripherals), is a per-mode bench better?
7. **Temporal specs.** Power sequencing and protocol timing need ordering ("B after A by ≥ 1 ms"). Adopt a small STL subset later, or extend the measurement library (`.delay(from: …)` already covers two-event cases)?
8. **Electro-thermal coupling.** `tj = temp + θja·P` is one-way. When P depends on Tj (thermal runaway, LDOs at dropout), iterate or add a thermal network to the simulation?
9. **Derating policies.** Which standards to ship (ECSS-Q-ST-30-11, NASA EEE-INST-002, a documented "commercial" set), with what provenance, and how a company imports its own table.
10. **Default distribution** for statistical knobs with min/max-only datasheet data (spec_design open decision 4). It affects every `sigma`/`yield` verdict, including matching specs.
11. **Syntax the sibling owns** and this proposal depends on: role type parameters (`Power<Source>`), `±` and its ASCII fallback, `?`, indexed item names (`gain[i]`), closures for measure helpers (`|i| …`), and part categories (trait-like or not).
12. **Transient abs-max qualifiers** (`for < 10ns`): per excursion, cumulative, or duty-cycle based? Datasheets vary.
13. **Characterized-form validity.** Level-2 composition is first-order plus `err`. When should it be trusted for sign-off vs only for fast feedback?
14. **Where the default bench's AC excitation goes** when a block has several inputs and a measure's denominator isn't a port (e.g. a ratio of two internal nets).

---

## Appendix A. What this proposal adds to the core language

For the sibling proposal, this is everything this document needs from the grammar and the prelude.

| Kind | Items |
|---|---|
| **Items** | `contract B { … }` (with generics: `contract<const N: usize> B<N>`); `bench name { …, ..default }` (inside a contract; may take parameters); `bench name for B { … }` (a sheet bench, top level); `corner name { knob: value, … }`; `env name: Type;`; `interface I { roles: …; … link { … } }`; part sections `absolute_max { … }` and `operating { … }` |
| **Statements** | `assume [name:] quantity relation bound [where …] [else "…"];`; `assume signal in Pattern { field: range, … };`; `spec name: measure relation bound [on bench] [where …] [else "…"];`; `spec path relation bound …;`; `let` / `pub let` measure bindings; elaboration-time `for` and `if` around any of these |
| **Relations** | `in`, `<=`, `>=`, `<`, `>` (no `==` on real quantities: a lint suggests `in x ± tol`) |
| **Attributes** | `#[confidence(worst_case \| sigma(k) \| yield(p) \| nominal)]`, `#[warn]`, `#[deny]`, `#[expect(…, reason)]`, `#[allow(…, reason)]`, `#[forbid(…)]`, `#[check(group)]`, `#[by(Role[, combine])]`, `#[env(knob = expr, reason)]`, `#[derating(…, reason)]`, `#[req("ID")]`; inner forms `#![…]`; `///` doc comments |
| **Probes** | `.v`, `.i`, `.p` on nets, pins, parts and ports; `impedance(port)`; `loop_gain(net)` |
| **Analyses** | `dc`, `ac`, `tran`, `noise`, and `bench.dc(…)` etc. |
| **Types** | `Signal<T>`, `Response<T>`, `Waveform<T>`, `Density<T>`, `Curve<K, T>`, `Set<T>`, `Bench<B>`; role markers (`Source`, `Sink`, `In`, `Out`, `Io`, `OpenDrain`, `Controller`, `Target`) |
| **Library functions** | `derated(x)`, `r_pullup(line)`, `c_total(line)`, `one_hot(k, on, off)`, `all_unique(xs)` |
| **Manifest** | `[specs] confidence`; `[checks]` levels (and `confidence`); `[derating]` policy |

---

## References

- **[BCN+18]** Benveniste, Caillaud, Nickovic, Passerone, Raclet, Reinkemeier, Sangiovanni-Vincentelli, Damm, Henzinger, Larsen, "Contracts for System Design," *Foundations and Trends in Electronic Design Automation* 12(2–3):124–400, 2018, doi:10.1561/1000000053. Equation numbers from the open precursor Inria RR-8147 (2012), §VII-A.
- Nuzzo, Xu, Ozay, Finn, Sangiovanni-Vincentelli, Murray, Donzé, Seshia, "A Contract-Based Methodology for Aircraft Electric Power System Design," *IEEE Access* 2:1–25, 2014, doi:10.1109/ACCESS.2013.2295764.
- Otter et al., "Formal Requirements Modeling for Simulation-Based Verification," Proc. 11th Int. Modelica Conf., 2015, pp. 625–635, doi:10.3384/ecp15118625. Modelica Language Specification 3.6, §18.4 (`experiment`, `TestCase`).
- Maler & Nickovic, "Monitoring Temporal Properties of Continuous Signals," FORMATS/FTRTFT 2004, pp. 152–166, doi:10.1007/978-3-540-30206-3_12. Donzé & Maler, "Robust Satisfaction of Temporal Logic over Real-Valued Signals," FORMATS 2010, pp. 92–106, doi:10.1007/978-3-642-15297-9_9.
- Accellera Verilog-AMS LRM 2.4.0 (2014). IEEE 1850-2010 (PSL). IEEE 1800-2023 (SystemVerilog/SVA).
- OMG SysML 2.0 (formal, 2025); examples from github.com/Systems-Modeling/SysML-v2-Release.
- Cadence Virtuoso ADE Assembler and ADE Explorer datasheets (2021). Cadence ADE L User Guide 6.1.
- *PSpice Advanced Analysis User's Guide* v10.5 (2005), ch. 5 (Smoke). ngspice User's Manual v47, §11.4 (`.meas`) and §11.5 (SOA checks). LTspice help, `.MEASURE`.
- MathWorks Simulink Requirements Toolbox: Requirements Table (R2022a+).
- **[AGW94]**, **[Graeb07]**, **[ECSS11]**, Galán et al. 1999: see `../bibliography.md`.
- PolymorphicBlocks: Lin, Ramesh, Chi, Jain, Nuqui, Dutta, Hartmann, "Polymorphic Blocks: Unifying High-level Specification and Low-level Control for Circuit Board Design," UIST '20, pp. 529–540, doi:10.1145/3379337.3415860. Code at commit `588b73b`.
- atopile: `externals/atopile` at commit `619eda7`. Spade: `externals/spade` at commit `177e5c4`.
- NXP UM10204, *I2C-bus specification and user manual*, Rev. 7.0, 1 Oct 2021: §7.1 (pull-up sizing), Table 10 (VIL/VIH/VOL/IOL), Table 11 (t_r, C_b). TI SLVA689 (Arora, 2015), Eqs. 1 and 6.
- Rust 1.81.0 release notes (2024-09-05): `#[expect(lint)]` and lint `reason`. Rust 1.74.0 release notes (2023-11-16): `[lints]` in `Cargo.toml`.
- stm32f4xx-hal `gpio` module docs: typestate pin modes (`Output<PushPull>`, `Output<OpenDrain>`).
