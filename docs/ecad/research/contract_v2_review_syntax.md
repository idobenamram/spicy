# Review of Draft 2: The Open Syntax Questions

> 2026-09-30 · A review of `contract_syntax_v2.md` from one angle: the syntax it leaves open. For each question it shows concrete alternatives on the same example (the gain stage and the sensor board), with references, pros and cons, then recommends one coherent set and rewrites the gain stage, the LDO, the board and the CE amp in it. **Status:** a proposal for the lead. Nothing here is decided.

## Summary

Draft 2 uses `impl` for three different things: `impl Circuit for X` (what's inside), `impl Contract for X` (the promises) and `impl Regulator for Ldo3v3` (a trait). A Rust reader thinks all three are traits, and an engineer can't read any of them. Rust also allows only one `impl` of a trait per type, so the draft can't name a second implementation. That was the reason for splitting the items in the first place (decision 2).

The recommended set drops `impl` entirely. Every item that belongs to a block follows one pattern, **`<item> [Name] for <Block>`**:

| Question | Recommendation | One-line reason |
|---|---|---|
| Implementation | `circuit for GainStage { … }`; named ones `circuit Ideal for GainStage { … }` | The engineer's word for "what's inside", and it reads as English: "the circuit for GainStage" |
| Choosing one | the unnamed circuit is the default; a placement picks another with `GainStage::Ideal { … }`; a setup swaps one for a run with `amp.circuit: Ideal` | VHDL architectures and configurations, Cadence cellviews and config views: per-placement and per-run are two real needs |
| Contract | `contract for GainStage { … }` | Keeps v0.1's word. `for` ties it to the block exactly as `circuit for` and `setup … for` do |
| Setup declaration | `setup Operating for GainStage { … }` (as drafted) | Already reads well |
| Setup inheritance | `setup InputStep for GainStage extends Operating { vin.wave: Step {…} }` | Modelica's word. The base comes first, the way a datasheet puts its header first. Dotted fields change one value, not a whole source |
| Generic setups | `setup LoadStep for Regulator extends Operating { … }`: a trait after `for` makes it generic | No `<Self>`, no angle brackets for the common case |
| Picking setups | nothing means the accepted setup; `on InputStep` for another one; named parameters `(zero: AtZero, full: AtFull)` only when a spec compares runs | 10 of the draft's 13 specs say `env: Operating`. That's noise |
| Accepts | `accepts Operating;` as the contract's first line, next to `rated` (as drafted) | Operating range and absolute maximum sit together, as they do in a datasheet |
| Final condition | `require`, exactly one per spec | SysML v2's `require constraint`, edg's `require`, and the engineer's word "requirement" |
| Fixed-point conditions | on the spec header, `with vin.v = 4.3V` | They change the run, so they go next to the setup (datasheet "test conditions"). `.at()` stays the measure's axis |
| Port names | bare (`vout.v`), never `self.` | v0.1, Spade, VHDL and Verilog all do this. No clash is possible |
| Confidence | attribute `#[confidence(yield(99.9%))]` on the spec, project default in `global` | v0.1 and Rust. It stays out of the condition line |
| Traits | `trait Regulator`; conformance on the block header, `block Ldo3v3: Regulator`; bounds `A: OpAmp` | `interface`, `kind` and `family` already mean other things in our language. `:` always reads "is a" |

The rest of this file argues each row. §7 has the full rewrite next to the draft.

---

## 1. What is wrong with `impl` here

Here are the three uses in the draft:

```rust
impl Circuit  for GainStage { … }     // a body: nets and parts
impl Contract for GainStage { … }     // promises: accepts, rated, specs
impl Regulator for Ldo3v3 {}          // a trait: "Ldo3v3 is a regulator"
```

1. **Only the third is a trait.** `Circuit` and `Contract` aren't traits anyone can define, list or implement differently. So the syntax suggests a uniformity that isn't there. A Rust programmer will ask "where is `trait Circuit` declared?". An engineer will ask "what does impl mean?".
2. **Rust's coherence rule contradicts decision 2.** Rust allows exactly one `impl Trait for Type`. The draft splits the interface from the implementation so that a block can have *several* implementations. `impl Circuit for GainStage` has no slot for a name, so a second implementation needs either a name bolted on (`impl Circuit as Ideal for GainStage`) or a break with what `impl` means.
3. **`impl` in Rust means "methods on a type".** Spade, the closest Rust-flavoured HDL, keeps exactly that meaning. Its hardware unit is an `entity`, and `impl` only adds methods to structs (`impl X { fn … }`, `impl<T> X<T> for S {}` in `spade-tests`). Using `impl` for a netlist body goes further from Rust than Spade does.
4. **The lead is hesitant, and engineers are the audience.** A novice reads `circuit`, `contract` and `setup` without help. They don't read `impl`.

So each item gets its own word. The shared shape then carries the "belongs to this block" link, `<item> [Name] for <Block>`, which `setup Operating for GainStage` already uses.

---

## 2. The implementation item's keyword

### 2.1 The options, on the gain stage

Each option is shown with its header, the first body line, and how the board places the stage. The interface is the same in every option:

```rust
pub block GainStage<A: OpAmp = Mcp6001, const GAIN: f64 = 10.09> {
    vin: Analog<In>, vout: Analog<Out>, vdd: Power<In>, gnd: Ground,
}
```

**A. `impl Circuit for X`** (the draft)

```rust
impl Circuit for GainStage {
    net fb;  net mid;
    let u1 = A { inp: vin, inn: fb, out: vout, vdd, gnd };
    …
}
// board
let amp = GainStage<Mcp6001> { vin: sensor, vout: amp_out, vdd: vdda, gnd };
```

- Pro: familiar to Rust programmers; one keyword for all "attached" items.
- Con: see §1. No room for a name; `Circuit` looks like a trait but isn't one.

**B. `impl X`** (Rust inherent impl)

```rust
impl GainStage {
    net fb;  net mid;
    …
}
```

- Pro: short.
- Con: in Rust, `impl X { }` holds methods. Here it would hold parts, which is a new meaning under an old word. There's still no name, and the contract then needs a different word anyway.

**C. `circuit X` / `circuit for X` / `circuit Name for X`**

```rust
circuit for GainStage {                 // the default implementation
    net fb;  net mid;
    let u1 = A { inp: vin, inn: fb, out: vout, vdd, gnd };
    …
}
circuit Ideal for GainStage {           // a second, named one
    let e1   = Vcvs     { inp: vin, inn: gnd, out: vout, gain: GAIN };
    let r_in = Resistor { a: vin, b: gnd, value: 1GΩ };
}
// board
let amp = GainStage<Mcp6001>        { vin: sensor, vout: amp_out, vdd: vdda, gnd };   // default
let amp = GainStage::Ideal<Mcp6001> { vin: sensor, vout: amp_out, vdd: vdda, gnd };   // named
```

- Pro: "circuit" is the engineer's word for what's inside a box. It reads aloud as "the circuit for GainStage". The name slot is natural. It is also the same shape as `setup Operating for GainStage`.
- Con: a purely behavioral implementation (an ideal VCVS, an imported Verilog-A model) is still called a "circuit". That's acceptable: an ideal VCVS *is* a circuit of ideal elements, and SPICE calls such models subcircuits.
- Variant `circuit GainStage { }` (without `for`) reads as if it declared a new thing called GainStage. `for` removes the doubt.

**D. `design X`**

```rust
design for GainStage { … }
design Ideal for GainStage { … }
```

- Pro: plain English.
- Con: "design" also means the whole project ("the design") and the act of designing. In an AI-native editor, "the agent changed the design" becomes ambiguous. It's a weak noun for one block's body.

**E. `architecture Name of X`** (VHDL)

```rust
architecture discrete of GainStage { … }
architecture ideal    of GainStage { … }
// board
let amp = GainStage(discrete)<Mcp6001> { … };       // VHDL: entity work.gain_stage(discrete)
```

- Reference: IEEE 1076: `entity` = interface, `architecture rtl of e` = one of several bodies, `configuration` = which body each instance uses (contract_references.md §10.1).
- Pro: the exact precedent for "separate interface, several named bodies". Digital engineers know it.
- Con: analog and board engineers don't. It's long, a name is mandatory even when there is only one body, and in everyday speech "architecture" means the block diagram, not the netlist.

**F. `inside X`**

```rust
inside GainStage { … }
inside GainStage as Ideal { … }
```

- Pro: a novice gets it instantly: "what's inside GainStage".
- Con: a preposition used as an item keyword. There's no noun to use in speech ("open the inside"?), and a named variant reads badly.

**G. `schematic X`**

```rust
schematic for GainStage { … }
schematic Ideal for GainStage { … }
```

- Reference: Cadence cellviews. A cell has a `symbol`, a `schematic`, a `veriloga`, a `layout` view (contract_references.md §3).
- Pro: it's how the editor shows the body (v0.1 §2: a block's body is a *sheet*).
- Con: it names the *view*, not the thing. A text-only behavioral body isn't a schematic, and Cadence uses a *different* view type (`veriloga`) for exactly that. It also collides with the editor's own "schematic view" of any item.

**H. `block X { ports + body }` with a separate interface item**

```rust
pins GainStage { vin: Analog<In>, … }                 // some other word for the interface
block GainStage { net fb; let u1 = …; }                // body and block together
```

- Pro: one item per implementation, like v0.1.
- Con: it contradicts decisions 1 and 2 (`block` is the interface; interface and body are separate). It's listed only for completeness.

### 2.2 Several implementations, and how a placement picks one

The references show **two different needs**:

| Need | Example | Reference |
|---|---|---|
| **Per placement:** this instance uses that body | the board uses the discrete gain stage; a variant board uses the ideal one | VHDL `entity work.x(arch)` in the instance; Modelica `redeclare` at the use site |
| **Per run:** swap bodies to go faster or to compare | simulate the board with an ideal amp while tuning the filter | VHDL `configuration`; SystemVerilog `config … instance top.a2 liblist gateLib`; Cadence ADE test → **config view** (hierarchy editor) picks a cellview per instance |

Options for the per-placement choice:

```rust
let amp = GainStage::Ideal<Mcp6001> { … };            // (a) path: "GainStage's Ideal circuit"
let amp = GainStage(Ideal)<Mcp6001> { … };            // (b) VHDL-style
#[circuit(Ideal)] let amp = GainStage<Mcp6001> { … }; // (c) attribute
let amp = GainStage<Mcp6001> as Ideal { … };          // (d) `as`
```

- (a) The path form reads like the rest of our paths (`yageo::RC0603FR`, `onsemi::MMBT3904`). A form edits one token. **Recommended.**
- (b) is precise but crowds the header.
- (c) keeps the placement line clean, but in v0.1 attributes are metadata (lints, confidence), not netlist-changing choices. Changing the body changes the netlist, so it deserves real syntax.
- (d) reads well, but `as` is Rust's cast keyword and may later mean unit conversion.

For the per-run choice, a setup overrides it with a dotted field, the same as any other setup override (§4.2):

```rust
setup Fast for SensorBoard extends Operating {
    amp.circuit: Ideal,           // like a Cadence ADE test pointing at a config view
}
```

**Rules that make this work:**
- At most one unnamed `circuit for X`. It is the default.
- If every circuit is named, a placement must choose. Otherwise it's an error listing the names:
  ```
  error: `GainStage` has no default circuit
    = help: choose one: `GainStage::Discrete { … }` or `GainStage::Ideal { … }`
  ```
- **The contract is checked on every circuit** (decision 2). The editor shows one verdict column per circuit, so "does the ideal model still match the contract?" is visible for free.
- A verdict reached on a run where a setup swapped a circuit is marked as such. It never counts as evidence for the real circuit.

---

## 3. The contract item's keyword

The same contract, under each keyword:

```rust
impl Contract for GainStage { accepts Operating; spec gain { … } }   // A: the draft
contract GainStage          { accepts Operating; spec gain { … } }   // B: v0.1
contract for GainStage      { accepts Operating; spec gain { … } }   // C
spec GainStage              { accepts Operating; spec gain { … } }   // D
verify GainStage            { accepts Operating; spec gain { … } }   // E
datasheet for GainStage     { accepts Operating; spec gain { … } }   // F
requirements for GainStage  { accepts Operating; spec gain { … } }   // G
```

| | Pro | Con |
|---|---|---|
| A `impl Contract for` | Rust look | §1: not a trait; no name slot for a second contract (commercial vs automotive grade) |
| B `contract X` | v0.1; short; the A/G-theory word (contract_references.md §13) | reads as if it declared a new thing named X; differs in shape from `setup … for X` |
| **C `contract for X`** | v0.1's word; same shape as `circuit for` / `setup … for`; reads "the contract for GainStage"; a name slot for free (`contract Automotive for Ldo3v3`) | one more word than B |
| D `spec X` | short | `spec` is already the row inside, so `spec` inside `spec` is confusing |
| E `verify X` | SysML v2's `verify` | a verb as an item; in SysML `verify R` sits in the *verification case* and names the requirement, which is the opposite direction |
| F `datasheet for X` | every engineer knows it; v0.1 calls the contract the "I/O page" | a datasheet also has typical curves, pinouts and text; "datasheet" sounds like documentation, not checked promises; it collides with imported part datasheets (`#[source(datasheet = …)]`) |
| G `requirements for X` | SysML/INCOSE vocabulary | long; requirements are usually on the *system*, not promised by a part |

**Recommendation: C, `contract for X`.** A block may have at most one unnamed contract. Named contracts (`contract Automotive for Ldo3v3`) are allowed, but they're rare. Each gets its own verdict column, like circuits.

---

## 4. Setups

### 4.1 Declaration

The draft's form works: `setup Operating for GainStage { port: Source-or-Load { … }, temp: … }`. Its fields are struct-literal fields, which v0.1 already uses for placements, so a form can edit it directly. Keep it. Two small rules:

- **Setup names are scoped by block.** Every block can have an `Operating`. Outside the block's file it's `GainStage::Operating`.
- **A setup can reach an internal instance by path** (`mcu.load` on the board), like SVA `bind` and Cadence probes. The editor marks such fields "internal", since they depend on the circuit and not just the interface. A setup that names an internal path applies only to circuits that have that path, and the check says so.

### 4.2 Inheritance

Three options, on `InputStep`:

```rust
// A: the draft, colon in the header
setup InputStep for GainStage: Operating {
    vin: Signal { z: 100Ω..=10kΩ, wave: Step { 0V -> 100mV, edge: 1us } },
}

// B: Rust struct update, as last field (v0.1's `..default`)
setup InputStep for GainStage {
    vin: Signal { z: 100Ω..=10kΩ, wave: Step { 0V -> 100mV, edge: 1us } },
    ..Operating
}

// C: `extends` in the header (Modelica)
setup InputStep for GainStage extends Operating {
    vin.wave: Step { 0V -> 100mV, edge: 1us },
}
```

| | Reference | Pro | Con |
|---|---|---|---|
| A `: Operating` | Rust supertraits; SysML v2 `:>` (specializes) | short | `:` means "is a". A derived setup is *not* always a narrowing of its base: fault and stress setups deliberately leave the operating range. The colon would then say something false. It also clashes with `A: OpAmp` and `block X: Trait` |
| B `..Operating` | Rust struct update; v0.1 §8.5 `..default` | no new keyword; one mechanism for values and setups | the base comes **last**, so you read the change before you know what it changes. `..` is cryptic to novices |
| **C `extends Operating`** | Modelica `extends`; Java/TS | plain English; the base comes **first**, as a datasheet's "unless otherwise noted" header does; says "copy and change", not "is a" | a new keyword; the header gets long |

**Recommendation: C.** Keep `..base` for values in placements and literals, where Rust readers expect it.

**Override granularity is as important as the keyword.** In the draft, `InputStep` has to *repeat* `z: 100Ω..=10kΩ`, because overriding `vin` replaces the whole source. Rust's struct update works the same way: whole fields. The board's setups already use a dotted path (`mcu.load:`). Allow dotted paths everywhere, so an override changes exactly one value:

```rust
setup InputStep for GainStage extends Operating { vin.wave: Step { 0V -> 100mV, edge: 1us } }
setup AtZero    for SensorBoard extends Operating { sensor.v: 0V }
setup AtFull    for SensorBoard extends Operating { sensor.v: 100mV }
```

This is the datasheet's "test conditions" column exactly: one or two values that differ from the header.

**Checked either way:** every setup is checked against the contract's `accepts` (v0.1 §8.5). A setup meant to go outside it (faults, abs-max stress) says so with `#[outside(accepts, reason = "…")]`, which is the draft's "marked one-offs".

### 4.3 Generic setups over traits

The draft writes `spec dip(env: LoadStep<Self>)`. The options for the std library's side:

```rust
// A: Rust-style generic parameter
pub setup LoadStep<R: Regulator> for R extends R::Operating {
    vout.load: Step { from: R::Operating.vout.i.min, to: R::Operating.vout.i.max, edge: 1us },
}

// B: a trait after `for` makes it generic
pub setup LoadStep for Regulator extends Operating {
    vout.load: Step { from: Operating.vout.i.min, to: Operating.vout.i.max, edge: 1us },
}

// C: an explicit `any`
pub setup LoadStep for any Regulator extends Operating { … }
```

- A is the most explicit, but every name in the body needs `R::`, and the spec side still needs `LoadStep<Self>`.
- **B is recommended.** After `for`, a block name means "this block" and a trait name means "every block of this trait". Inside, `Operating` and the ports mean the block's own, since the trait requires them (§6). The spec just writes `on LoadStep`. The subject is already known, so no `<Self>` is needed.
- C makes the genericity visible to a reader who can't tell a trait from a block. The editor colours traits differently anyway, so the extra word isn't worth it. Keep C in mind if user testing says otherwise.

A block that wants a different load step writes its own `setup LoadStep for Ldo3v3 …`. The block's own setup overrides the trait's, and the editor lists "standard setups you haven't configured" (contract_options.md §3, option B's strength).

### 4.4 How a spec picks its setups

```rust
// A: parameter (the draft)
spec gain(env: Operating) { require … }
spec sensitivity(zero: AtZero, full: AtFull) { … mcu.code on full … }

// B: `on` (v0.1 §8.3, `on bench`)
spec gain on Operating { require … }

// C: attribute
#[setup(Operating)] spec gain { require … }

// D: default + `on` + named parameters only to compare runs  (recommended)
spec gain { require … }                                   // the accepted setup(s)
spec settling on InputStep { require … }                  // one other setup
spec sensitivity(zero: AtZero, full: AtFull) { … }        // several, compared by name
```

- **A** is uniform, but the name `env` is never used in 12 of the 13 specs. It's pure noise, and a novice will wonder what `env` is.
- **B** reads like a datasheet row ("… under test conditions X"), and `on` is the same word the draft already uses to read a probe from a named run (`mcu.code on full`). So **`on` always means "in the run of this setup"**.
- **C** hides a meaning-changing choice in metadata (same objection as §2.2 (c)).
- **D: recommended.** The common case costs nothing. The next case reads as English. Names appear only when the body compares two runs and so needs to call them something. The rule fits in one line: *"name setups only when you compare them."*

A spec with no setup runs on **every** accepted setup. If a block accepts two (say `Run` and `Sleep` modes), the spec must hold on both, which is the "for all" of v0.1 §8.6.

### 4.5 How "accepts" is declared

```rust
// A: the draft, first line of the contract
contract for GainStage { accepts Operating; rated temp within -40°C..=85°C; … }

// B: contract header
contract for GainStage accepts Operating { … }

// C: attribute on the setup
#[accepts] setup Operating for GainStage { … }

// D: a magic name: the setup called `Operating` is the accepted one
```

- **A is recommended** (as drafted). `accepts` (operating range) and `rated` (absolute maximum) are siblings, core item 4, and should sit together, as "Recommended operating conditions" and "Absolute maximum ratings" sit together at the top of every datasheet. It also allows several (`accepts Run, Sleep;`).
- B puts the A/G pair visibly in the header ("promises G, given A"), but then separates `accepts` from `rated`.
- C puts a contract fact on the setup, which is outside the contract. The contract is no longer the one place that lists what the block promises and under what conditions.
- D is invisible and breaks when someone renames a setup.

**A consistency rule worth writing down.** The draft already uses `in` for "a thing that ranges over this" (`global ambient: Temperature in …`) and `within` for checks (`rated … within`, `require … within`). Make that the rule, so conditions and checks never share a word (atopile's `assert … is` vs `assert … within` mistake, contract_references.md §14.1).

---

## 5. The spec function

### 5.1 Signature style

With §4.4 settled, the signature is `spec name [on Setup | (a: SetupA, b: SetupB)] [with fixed-point] { body; require condition; }`. It's still a function in the sense the lead decided: a body that computes, then one final condition. Parentheses appear only when there are parameters. `spec gain() { … }` with empty parentheses was considered and rejected: it adds two characters to most specs and says nothing.

### 5.2 The final-condition keyword

| Word | What engineers hear | What programmers hear | Precedent |
|---|---|---|---|
| **`require`** | "requirement": exactly what a spec row is | Eiffel / Solidity: a **pre**condition, the caller's duty | **SysML v2 `require constraint`** (what must hold); edg `self.require(…)`; INCOSE requirements |
| `ensure` | "make sure": sounds like an instruction to do something, and in an AI editor, maybe an instruction to the agent to fix it | Eiffel: a **post**condition, the guarantee (exactly right for programmers) | CRML `during … ensure …` (contract_references.md §11.3) |
| `check` | "a test step": neutral, clear | a runtime check | the editor's "automatic checks" (v0.1 §8.7): a clash, because those are generated, not written |
| `assert` | verification engineers: SVA, VHDL `assert … severity` | a debug assertion that aborts | SysML v2 `assert constraint` (an invariant on a usage); atopile `assert`, which uses it for both "set" and "check" |
| `expect` | "expected value": sounds like *typical*, which a datasheet keeps apart from limits (contract_references.md §18.1 point 6) | test frameworks (Jest `expect`) | clashes with v0.1's `#[expect(fail, reason = …)]` |
| `must` / `shall` | requirements-writing style (RFC 2119, INCOSE "shall") | nothing | grammatically awkward: `must dc(vout.v) within …` |
| *(none)* | the last expression is the condition, as a Rust tail expression | idiomatic Rust | invisible to novices; a form or the agent can't find "the condition" by keyword |

**Recommendation: `require`.** It's the engineer's word, SysML v2's word for exactly this slot, and edg's. The Eiffel "precondition" reading can't cause confusion inside our language, because our precondition has its own word, `accepts`. So `accepts` = what the block needs, `require` = what the block must deliver.

**Exactly one `require` per spec.** A spec is one row in the spec table with one verdict. Two conditions means two rows:

```
error: a spec has one `require`
  --> ldo.spl:12:9
   = help: split it: `spec output_min { … }` and `spec output_max { … }`, or join with `&&` if they must pass together
```

### 5.3 Conditions: `at`

The draft writes `require ac(vout.v / vin.v).at(100kHz).db() <= -36dB at vin.v = 4.3V;`. That one line uses `at` for two of the three kinds of condition (contract_references.md §18.1 point 5): `.at(100kHz)` is a **measure axis**, and `at vin.v = 4.3V` is a **fixed point**. The references say to keep them visibly apart. `output_z` then adds a third form for a range, `for f in 10Hz..=100kHz`, which v0.1 reserves for build-time loops.

The options for the fixed point:

```rust
spec psrr { require …at(100kHz).db() <= -36dB at vin.v = 4.3V; }       // A: draft, trailing `at`
spec psrr { require …at(100kHz).db() <= -36dB where vin.v == 4.3V; }   // B: v0.1 §8.3 `where`, trailing
spec psrr with vin.v = 4.3V { require …at(100kHz).db() <= -36dB; }     // C: header `with` (option A's word)
setup Psrr for Ldo3v3 extends Operating { vin.v: 4.3V }                // D: a named setup
spec psrr on Psrr { require … }
```

- **C is recommended.** A fixed point changes *the run*, not the measure, so it belongs next to the setup ("a fixed point belongs in the bench", §18.1). It reads as a datasheet row: "PSRR, with V_IN = 4.3 V". It's short enough that D is only needed when several specs share the point. `with` also avoids `where`, which v0.1 already uses for build-time constraints on generics (`where N >= 1 && N <= 32`, from Spade).
- `=` (not `==`): it *sets* a value, like a datasheet's "V_IN = 4.3 V". It's checked to lie inside `accepts`.

And the other two kinds:
- **measure axis:** methods on the measure, `.at(1kHz)`, `.band(10Hz..=100kHz)`, `.window(1ms..=5ms)` (v0.1 §8.6). So `output_z` becomes `require vout.z().band(10Hz..=100kHz).max() <= 100Ω;`.
- **range ("for all"):** needs no syntax. Every range in the setup is already quantified over.

### 5.4 Bare port names or `self.`

```rust
require dc(vout.v) within 3.3V ± 2%;         // bare
require dc(self.vout.v) within 3.3V ± 2%;    // self.
```

**Bare is recommended.** v0.1 does it, circuits do it (`let u1 = A { inp: vin, … }`), Spade entities do it (ports are parameters, used by name), and VHDL and Verilog architectures do it. Inside a contract, ports, internal nets and instances are all in scope. `self.` would add five characters to every probe to resolve a clash that can't happen: a `let` that shadows a port is an error ("`vout` is a port of `Ldo3v3`; pick another name"). In a generic setup, `Self` names the block type if it's ever needed.

### 5.5 Attributes (confidence)

```rust
#[confidence(yield(99.9%))]                       // A: attribute (v0.1)
spec bass { require …f_low(-3dB) <= 30Hz; }

spec bass { require …f_low(-3dB) <= 30Hz at 3σ; }  // B: inline clause
spec bass #[yield(99.9%)] { … }                   // C: inline attribute
```

**A is recommended.** It's how v0.1 and Rust do it, and it keeps the "how well" part (CRML's HOW WELL) apart from the "what" part, as CRML asks. The project-wide default stays `global confidence = sigma(3);`. `#[warn]` and `#[expect(fail, reason = "…")]` sit in the same place. The doc comment above the spec is its rationale, shown in the spec table.

---

## 6. Traits and generics

### 6.1 The word

```rust
pub trait Regulator { vin: Power<In>, vout: Power<Out>, gnd: Ground, setup Operating; }
pub kind Regulator { … }
pub interface Regulator { … }
```

| Word | Pro | Con |
|---|---|---|
| **`trait`** | Rust and Spade; plain English ("a trait of the block") | unfamiliar to engineers the first time |
| `kind` | the engineer's everyday word ("what kind of regulator?") | v0.1 §6.1 already calls `Resistor`, `Capacitor`, `Electrolytic` "part **kinds**", and those are concrete blocks you place, not requirements |
| `interface` | Java/TS/SystemVerilog | v0.1 §3.4 uses `interface` for signal bundles (`I2c`, `Spi`) |
| `family` | engineer's word | v0.1 §6.5 uses it for part families |
| `role` | | used for `In`/`Out` |
| `class` / `concept` | | wrong baggage (OOP classes, C++20 concepts) |

**Recommendation: `trait`.** Every friendlier word is already taken in our own language, for something close enough to cause real confusion. The editor can label it "Kind: Regulator" in the UI without changing the text.

### 6.2 What a trait holds

A trait holds required ports (by name and type) and required setups:

```rust
/// Anything that turns a supply into a regulated rail.
pub trait Regulator {
    vin:  Power<In>,
    vout: Power<Out>,
    gnd:  Ground,
    setup Operating;          // every regulator must say where it works
}
```

The std setups written `for Regulator` (§4.3) then work on every regulator.

### 6.3 Saying a block has a trait

```rust
impl Regulator for Ldo3v3 {}                 // A: the draft
pub block Ldo3v3: Regulator { … }            // B: on the block header
pub block Ldo3v3 is Regulator { … }          // C: `is`
```

**B is recommended** (option B of contract_options.md already wrote `block Ldo3v3: Regulator`). It's one place, visible where the ports are, and it removes the last `impl`. The colon keeps one meaning throughout the language, **"is a"**:

```rust
vin: Power<In>             // vin is a Power input
A: OpAmp                   // A is an op amp
block Ldo3v3: Regulator    // Ldo3v3 is a regulator
```

This is also why setups use `extends` and not `:` (§4.2).

### 6.4 Generic parameters

Keep the draft's `GainStage<A: OpAmp = Mcp6001, const GAIN: f64 = 10.09>`. It's Rust and Spade, and v0.1 §7.1's rule applies: "if it changes the drawing, it goes in `<>`". Several bounds use `+` (`A: OpAmp + RailToRail`). Longer constraints go in a `where` clause, as v0.1 already does.

---

## 7. The recommended set, written out

Each example is shown in draft form, then in the recommended form.

### 7.1 The gain stage

**Draft:**

```rust
pub block GainStage<A: OpAmp = Mcp6001, const GAIN: f64 = 10.09> {
    vin: Analog<In>, vout: Analog<Out>, vdd: Power<In>, gnd: Ground,
}
impl Circuit for GainStage {
    net fb;  net mid;
    let u1  = A        { inp: vin, inn: fb, out: vout, vdd, gnd };
    let ref = MidRail  { vdd, gnd, out: mid };
    let r_f = Resistor { a: vout, b: fb,  value: (GAIN - 1) * 10k ± 1% };
    let r_g = Resistor { a: fb,   b: mid, value: 10k ± 1% };
}
setup Operating for GainStage {
    vin:  Signal { z: 100Ω..=10kΩ },
    vout: Load   { r: 10kΩ.., c: ..=1nF },
    vdd:  Supply { v: 3.2V..=3.4V },
    temp: ambient,
}
setup InputStep for GainStage: Operating {
    vin: Signal { z: 100Ω..=10kΩ, wave: Step { 0V -> 100mV, edge: 1us } },
}
impl Contract for GainStage {
    accepts Operating;
    rated temp within -40°C..=85°C;
    spec gain(env: Operating) {
        let h = ac(vout.v / vin.v);
        require h.at(1kHz).mag() within GAIN ± 1%;
    }
    spec bandwidth(env: Operating) { require ac(vout.v / vin.v).f_high(-3dB) >= 20kHz; }
    spec settling(env: InputStep) {
        let v = tran(vout.v);
        require v.time_to_within(1% of v.at(end)) <= 50us;
    }
    spec input_z(env: Operating)  { require vin.z(1kHz) >= 400kΩ; }
    spec output_z(env: Operating) { require vout.z(f) <= 100Ω for f in 10Hz..=100kHz; }
    spec supply(env: Operating)   { require dc(vdd.i) <= 150uA; }
}
```

**Recommended:**

```rust
/// A non-inverting stage around mid-rail.
pub block GainStage<A: OpAmp = Mcp6001, const GAIN: f64 = 10.09> {
    vin:  Analog<In>,
    vout: Analog<Out>,
    vdd:  Power<In>,
    gnd:  Ground,
}

/// What's inside: the default circuit.
circuit for GainStage {
    net fb;  net mid;
    let u1  = A        { inp: vin, inn: fb, out: vout, vdd, gnd };
    let ref = MidRail  { vdd, gnd, out: mid };
    let r_f = Resistor { a: vout, b: fb,  value: (GAIN - 1) * 10k ± 1% };
    let r_g = Resistor { a: fb,   b: mid, value: 10k ± 1% };
}

/// An ideal model, for fast board runs. Checked against the same contract.
circuit Ideal for GainStage {
    let e1   = Vcvs     { inp: vin, inn: gnd, out: vout, gain: GAIN };
    let r_in = Resistor { a: vin, b: gnd, value: 1GΩ };
}

/// Where it's designed to work.
setup Operating for GainStage {
    vin:  Signal { z: 100Ω..=10kΩ },
    vout: Load   { r: 10kΩ.., c: ..=1nF },
    vdd:  Supply { v: 3.2V..=3.4V },
    temp: ambient,
}
/// A 100 mV input step; everything else as in Operating.
setup InputStep for GainStage extends Operating {
    vin.wave: Step { 0V -> 100mV, edge: 1us },
}

contract for GainStage {
    accepts Operating;
    rated temp within -40°C..=85°C;

    /// The ADC range is set for this gain.
    spec gain {
        let h = ac(vout.v / vin.v);
        require h.at(1kHz).mag() within GAIN ± 1%;
    }
    spec bandwidth { require ac(vout.v / vin.v).f_high(-3dB) >= 20kHz; }
    spec settling on InputStep {
        let v = tran(vout.v);
        require v.time_to_within(1% of v.at(end)) <= 50us;
    }

    // published port facts: ordinary specs, used for composition
    spec input_z  { require vin.z(1kHz) >= 400kΩ; }
    spec output_z { require vout.z().band(10Hz..=100kHz).max() <= 100Ω; }
    spec supply   { require dc(vdd.i) <= 150uA; }
}
```

What changed: `impl` became `circuit for` and `contract for`; a second circuit shows the name slot; `: Operating` became `extends Operating` with a one-value override; the `(env: Operating)` noise is gone from six specs; `for f in …` became `.band(…)`.

### 7.2 The LDO

**Draft:**

```rust
pub block Ldo3v3 { vin: Power<In>, vout: Power<Out>, gnd: Ground }
impl Regulator for Ldo3v3 {}
impl Circuit for Ldo3v3 {
    let u1    = Tlv755p   { vin, vout, gnd };
    let c_in  = Capacitor { a: vin,  b: gnd, value: 1uF ± 10% };
    let c_out = Capacitor { a: vout, b: gnd, value: 1uF ± 10% };
}
setup Operating for Ldo3v3 {
    vin:  Supply { v: 4.3V..=5.5V, z: ..=0.5Ω },
    vout: Load   { i: 0.1mA..=50mA, c: ..=20uF },
    temp: ambient,
}
impl Contract for Ldo3v3 {
    accepts Operating;
    rated vin.v within -0.3V..=6.5V;
    spec output(env: Operating)   { require dc(vout.v) within 3.3V ± 2%; }
    spec psrr(env: Operating)     { require ac(vout.v / vin.v).at(100kHz).db() <= -36dB at vin.v = 4.3V; }
    spec dip(env: LoadStep<Self>) { require tran(vout.v).min() >= 3.25V; }
    spec start(env: PowerUp<Self>){ require tran(vout.v).time_to(3.2V) <= 1ms; }
}
```

**Recommended** (the std part first, so the generic setups are visible):

```rust
// ── std/regulator.spl ──
pub trait Regulator {
    vin:  Power<In>,
    vout: Power<Out>,
    gnd:  Ground,
    setup Operating;
}
/// Load current steps from the lowest to the highest accepted load.
pub setup LoadStep for Regulator extends Operating {
    vout.load: Step { from: Operating.vout.i.min, to: Operating.vout.i.max, edge: 1us },
}
/// The input ramps up from zero to its highest accepted value.
pub setup PowerUp for Regulator extends Operating {
    vin.wave: Ramp { 0V -> Operating.vin.v.max, rise: 100us },
}

// ── ldo.spl ──
pub block Ldo3v3: Regulator { vin: Power<In>, vout: Power<Out>, gnd: Ground }

circuit for Ldo3v3 {
    let u1    = Tlv755p   { vin, vout, gnd };
    let c_in  = Capacitor { a: vin,  b: gnd, value: 1uF ± 10% };
    let c_out = Capacitor { a: vout, b: gnd, value: 1uF ± 10% };
}

setup Operating for Ldo3v3 {
    vin:  Supply { v: 4.3V..=5.5V, z: ..=0.5Ω },
    vout: Load   { i: 0.1mA..=50mA, c: ..=20uF },
    temp: ambient,
}

contract for Ldo3v3 {
    accepts Operating;
    rated vin.v within -0.3V..=6.5V;

    spec output                  { require dc(vout.v) within 3.3V ± 2%; }
    spec psrr with vin.v = 4.3V  { require ac(vout.v / vin.v).at(100kHz).db() <= -36dB; }
    spec dip   on LoadStep       { require tran(vout.v).min() >= 3.25V; }
    spec start on PowerUp        { require tran(vout.v).time_to(3.2V) <= 1ms; }
}
```

What changed: `impl Regulator for Ldo3v3 {}` became `: Regulator` on the header; `LoadStep<Self>` became `on LoadStep`; the trailing `at vin.v = 4.3V` moved to the header as `with vin.v = 4.3V`, so `at` is left meaning only the measure axis. The four specs now line up as a table: name, conditions, requirement.

### 7.3 The board

**Draft:** see `contract_syntax_v2.md`, lines 112–141.

**Recommended:**

```rust
pub block SensorBoard { usb: Power<In>, sensor: Analog<In>, gnd: Ground }

circuit for SensorBoard {
    net v3v3;  net vdda;  net amp_out;  net adc_in;
    let ldo    = Ldo3v3             { vin: usb, vout: v3v3, gnd };
    let r_filt = Resistor           { a: v3v3, b: vdda, value: 10Ω ± 5% };
    let c_filt = Capacitor          { a: vdda, b: gnd,  value: 10uF ± 20% };
    let amp    = GainStage<Mcp6001> { vin: sensor, vout: amp_out, vdd: vdda, gnd };
    //  amp    = GainStage::Ideal<Mcp6001> { … }     ← how this placement would pick the ideal circuit
    let r_aa   = Resistor           { a: amp_out, b: adc_in, value: 470Ω ± 1% };
    let c_aa   = Capacitor          { a: adc_in,  b: gnd,    value: 100nF ± 10% };
    let mcu    = Stm32Adc           { vdd: v3v3, ain: adc_in, gnd };
}

setup Operating for SensorBoard {
    usb:    Supply { v: 4.40V..=5.25V, z: 0.1Ω..=0.5Ω },
    sensor: Signal { v: 0V..=100mV, z: 100Ω..=10kΩ },
    temp:   ambient,
}
setup McuWakes for SensorBoard extends Operating { mcu.load: Step { 5mA -> 30mA, edge: 1us } }   // internal path
setup AtZero   for SensorBoard extends Operating { sensor.v: 0V }
setup AtFull   for SensorBoard extends Operating { sensor.v: 100mV }
/// For tuning only: swaps in the ideal amp. Its verdicts don't count for the real board.
setup Fast     for SensorBoard extends Operating { amp.circuit: Ideal }

contract for SensorBoard {
    accepts Operating;

    /// Counts per millivolt, end to end.
    spec sensitivity(zero: AtZero, full: AtFull) {
        let per_mv = (mcu.code on full - mcu.code on zero) / 100mV;
        require per_mv within 12.0/mV ± 7%;
    }
    spec wake_noise on McuWakes { require tran(mcu.code).deviation() <= 2; }
}
```

What changed: `AtZero` and `AtFull` shrank to one value each; the only spec that compares runs keeps named parameters, and `on` means the same thing in its header and its body; the two ways to choose a circuit are shown.

### 7.4 The CE amp (the simple case)

**Draft:**

```rust
pub block CeAmp { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }
impl Circuit for CeAmp { … }
setup Operating for CeAmp { vcc: Supply { v: 12V ± 5% }, temp: ambient }
impl Contract for CeAmp {
    accepts Operating;
    spec bias(env: Operating) { require dc(output.v) within 4.5V..=6.5V; }
    spec gain(env: Operating) { require ac(output.v / input.v).at(1kHz).mag() within 4.6 ± 5%; }
    spec bass(env: Operating) { require ac(output.v / input.v).f_low(-3dB) <= 30Hz; }
}
```

**Recommended:**

```rust
/// Common-emitter audio stage. Gain ≈ RC/RE ≈ 4.6, f_L ≈ 20 Hz.
pub block CeAmp { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }

circuit for CeAmp {
    net base;  net emitter;
    let r1   = Resistor     { a: vcc, b: base,    value: 47k ± 1% };
    let r2   = Resistor     { a: base, b: gnd,    value: 10k ± 1% };
    let rc   = Resistor     { a: vcc, b: output,  value: 4.7k ± 1% };
    let re   = Resistor     { a: emitter, b: gnd, value: 1k ± 1% };
    let c_in = Electrolytic { p: base, n: input,  value: 1uF ± 20% };
    let q1   = Npn          { c: output, b: base, e: emitter, beta: 100..=300 };
}

setup Operating for CeAmp { vcc: Supply { v: 12V ± 5% }, temp: ambient }

contract for CeAmp {
    accepts Operating;
    /// Room for the output to swing ±1 V without clipping.
    spec bias { require dc(output.v) within 4.5V..=6.5V; }
    /// The next stage expects this level.
    spec gain { require ac(output.v / input.v).at(1kHz).mag() within 4.6 ± 5%; }
    /// Don't cut the bass.
    #[confidence(yield(99.9%))]
    spec bass { require ac(output.v / input.v).f_low(-3dB) <= 30Hz; }
}
```

This is the novice test. Every word in it is either an electronics word (`block`, `circuit`, `setup`, `Supply`, `contract`, `spec`) or plain English (`for`, `accepts`, `require`, `within`). There's nothing to look up. It has four items where v0.1 had two. The price of decision 2 is two short extra headers, and no noise inside them.

---

## 8. Why each choice, in one place

1. **One shape for attached items: `<item> [Name] for <Block>`.** `circuit for`, `contract for`, `setup Operating for`, `setup LoadStep for Regulator`. A reader learns it once. The optional name is the multiple-implementation story (VHDL architectures) at no extra cost.
2. **`circuit`**, because it's the engineer's noun for what's inside, and it isn't a view (`schematic`), a whole project (`design`) or a digital-only word (`architecture`).
3. **Two ways to choose a circuit, because the references have two needs.** Per placement is a design choice, written in the placement (`GainStage::Ideal`). Per run is a simulation choice, written in a setup (`amp.circuit: Ideal`), the way a Cadence ADE test points at a config view.
4. **`contract for`** keeps v0.1's word and gains the shared shape.
5. **`extends`** puts the base first and doesn't claim "is a". **Dotted overrides** make a derived setup as short as a datasheet's test-conditions cell.
6. **A trait after `for` makes a setup generic**, so specs write `on LoadStep`, not `LoadStep<Self>`.
7. **Specs name setups only when they need to.** The accepted setup is the default, `on X` handles the next case, and parameters are for comparing runs. `on` has one meaning everywhere.
8. **`require`, exactly one per spec**: SysML v2's slot word, the engineer's "requirement", one verdict per row. `accepts` (what I need) and `require` (what I deliver) never blur.
9. **Three kinds of condition, three visibly different forms:** `with x = v` (fixed point, header), `.at()` / `.band()` / `.window()` (measure axis), nothing (range, always quantified). `for` stays build-time only.
10. **`in` declares a range, `within` checks one.** Conditions and checks never share a word.
11. **Bare names, attributes for confidence.** Both are v0.1 as it stands.
12. **`trait`, conformance on the header, `:` means "is a"**: no `impl` anywhere, and the friendlier words stay free for the things v0.1 already uses them for.

## 9. Left for the lead

- **`extends` vs `..Operating`.** This is the closest call. `..` is more Rust and needs no new keyword; `extends` is more readable and puts the base first. It's worth writing a few more setups in both forms before deciding.
- **The per-placement choice.** `GainStage::Ideal<Mcp6001>` puts the path before the generic arguments. Try it on a block with long generics before committing.
- **Named contracts** (`contract Automotive for Ldo3v3`): allow them now, or wait until a real grade split shows up?
- **Implicit setup for specs:** should a spec with no setup and a block with several accepted setups run on all of them (proposed), or should that be an error that asks you to choose?
- **Setups that name internal paths** (`mcu.load`): allow them in general (proposed, marked "internal"), or only in setups of the top block?
