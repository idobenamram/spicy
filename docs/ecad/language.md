# Language Design v0.1

> 2026-09-25 · Merges `research/language_core.md`, `research/language_specs.md` and `research/language_editor_mapping.md`, using the decisions in `research/synthesis.md` §4.3 (all recommendations accepted).
> The research reports hold the longer arguments and more examples. Engine concepts are in `engine.md`.
> Name and file extension are still open; examples use `.spl`.

---

## 0. The shape on one page

```rust
use std::prelude::*;                  // Resistor, Capacitor, Electrolytic, Npn, Power, Ground, Analog, …
use yageo::RC0603FR;                  // a real resistor family (§6)

/// Common-emitter audio stage. Gain ≈ RC/RE ≈ 4.6, f_L ≈ 20 Hz.
pub block CeAmp {
    port vcc: Power<In>;              // connection points on the block's boundary (§3)
    port gnd: Ground;
    port input: Analog<In>;
    port output: Analog<Out>;

    net base;                         // internal nodes
    net emitter;

    /// Divider holds the base near 2.1 V, which sets IC ≈ 1.4 mA.
    let r1 = Resistor { a: vcc, b: base, value: 47k ± 1%, part: RC0603FR };
    let r2 = Resistor { a: base, b: gnd, value: 10k ± 1%, part: RC0603FR };
    /// Gain ≈ RC/RE. RC also puts VC near mid-supply.
    let rc = Resistor { a: vcc, b: output, value: 4.7k ± 1%, part: RC0603FR };
    let re = Resistor { a: emitter, b: gnd, value: 1k ± 1%, part: RC0603FR };
    /// f_L = 1/(2π·C_in·R_in) ≈ 20 Hz.
    let c_in = Electrolytic { p: base, n: input, value: 1uF ± 20%, rating: 25V };
    let q1 = Npn { c: output, b: base, e: emitter, part: onsemi::MMBT3904 };
}

contract CeAmp {                      // the I/O page (§8)
    assume vcc.v in 12V ± 5%;
    assume input in Sine { amp: ..=100mV, freq: 20Hz..=20kHz };
    assume output.z_load >= 10kΩ;

    let h = ac(output.v / input.v);

    /// Room for the output to swing ±1 V without clipping.
    spec bias: dc(output.v) in 4.5V..=6.5V;
    /// The next stage expects this level.
    spec gain: h.at(1kHz).mag() in 4.6 ± 5%;
    /// Don't cut the bass.
    #[confidence(yield(99.9%))]
    spec bass: h.f_low(-3dB) <= 30Hz;
}
```

The ideas:

- **`block`:** a circuit you can place many times. It has ports and a body.
- **`contract`:** the block's I/O page: what it assumes about the world and what it guarantees.
- **Connecting:** every part binds each of its pins to a named net, in the part's own statement.
- **Values:** carry units and tolerances.
- **Real parts:** a `part:` field pins a manufacturer part, whose datasheet data the engine then uses.
- **Results never live in the file.** The editor shows verdicts next to the specs.

---

## 1. Principles

| # | Principle | What it means in practice |
|---|---|---|
| P1 | **If it looks like Rust, it behaves like Rust** | We may accept *more* than Rust, never change the meaning of something Rust accepts. So `+-` and `--` are rejected (in Rust they mean `+ (-…)` and `- (-…)`) |
| P2 | **One gesture ↔ one statement** | Placing a part = one `let`. Wiring a pin = one field in that `let`. Editing a spec = one `spec` line |
| P3 | **Names are identity** | `board.amp.r1`, `prot.r_s[3]`. Used by the layout sidecar, knob names, AI edits, diffs |
| P4 | **Order-independent bodies** | A block body is a set of named declarations. Feedback loops need no forward declarations |
| P5 | **Types catch what needs no numbers; the engine catches what does** | Shorted supplies are a type error; "5 V into a 3.3 V pin" is an engine check with tolerances |
| P6 | **Structure is decided at build time** | `for`, `if`, array sizes depend only on exact values, never on toleranced ones |
| P7 | **Small** | No lifetimes, borrowing, closures, macros or user traits in v1 |
| P8 | **Written for the AI too** | One canonical format, one way to write each thing, fix-its in every diagnostic |

---

## 2. Items: what you can write at the top level

| Item | What it is | In the schematic |
|---|---|---|
| `block` | A circuit you can place many times: ports, params, body | A **sheet** (its body) and a **symbol** (a box with its ports as pins) |
| `contract` | A block's assumptions, measures, specs and benches | The block's **I/O page** |
| `part` | A leaf component: pins, datasheet data, simulation model, ordering data | A library **symbol** |
| `family` | A series of parts that differ only in value (e.g. all 1% 0603 resistors) | The part picker |
| `fn` | Pure math on values | Nothing. Shows up in value cards ("R_bot = e96(…)") |
| `signal`, `interface` | Port types and bundles (std, or your own) | Pin types, buses |
| `env` | A project-wide range knob (temperature, lifetime) | The project's I/O page |
| `const`, `enum`, `type` | As in Rust | — |
| `mod`, `use` | As in Rust: files, libraries and namespaces | The project tree |

### 2.1 Is a block a struct? Is it a mod?

**Neither.** It's closest to a **function that builds circuitry**. Compare what each Rust item is:

| Rust item | What it is | Is a block like it? |
|---|---|---|
| `mod` | A namespace for organizing code. Exists once. No parameters. Can't be instantiated | **No.** You place two `CeAmp`s on a board, and each gets its own parts. We keep `mod`/`use` for what they mean in Rust: files and libraries |
| `struct` | A data type with named fields. Many instances. No body | **Partly:** a block has named ports and params, and many instances |
| `fn` | Parameters plus a body that runs each time it's called | **Partly:** a block's body runs once per placement and creates that placement's parts and nets. Internal nets are private, like local variables |

So a block gets its own keyword. The definition reads like a declaration with a body. Placing one uses Rust's **struct-literal syntax**, because that's Rust's only syntax for named arguments, and pins must be bound by name:

```rust
let amp = CeAmp { vcc: v12, gnd, input: guitar, output: to_adc };   // `gnd` is shorthand for `gnd: gnd`, as in Rust
```

Read it as "build a CeAmp here, with these connections". It is not "make a struct".

The same idea goes by other names elsewhere:
- Spade calls it `entity` (function-like, placed with `inst`).
- atopile and Verilog call it `module`. In hardware languages "module" means an instantiable template, which is the opposite of Rust's `mod`.

The contract sits next to the block the way `impl` sits next to `struct`.

**Alternative considered:** function syntax, `block CeAmp(vcc: Power<In>, …)` placed with `CeAmp(vcc: rail, …)`. Rejected for two reasons: Rust has no named function arguments, and ICs have dozens of ports.

---

## 3. Ports and signal types

### 3.1 What a port is

A **port** is a named connection point on a block's boundary, the thing the parent wires to. The same port shows up in four places:

- **Inside the block**, it behaves exactly like a net: you bind part pins to it.
- **On the block's symbol**, it's a pin.
- **In a hierarchical schematic**, it's the sheet pin.
- **On the I/O page**, it's where assumptions and guarantees attach (`vcc.v`, `output.z_load`).

Electrically a port is just a node. Its **type** says what kind of signal it carries.

### 3.2 What a type like `Power` gives you

Electrically `Power` is a single wire, so why type it? Because the type is where five things come from:

1. **Checks that need no numbers.**
   - Two `Power<Out>` on one net: "two supplies shorted".
   - An IC's `Power<In>` pin on a net with no source: "unpowered".
   - A `Power` net wired into an `Analog<In>` signal pin: warning.
2. **Who owns which number.** A rail's voltage `v` is set by its **source**; its current `i` is drawn by its **sinks**. From that alone, the engine generates checks at every connection:
   - source guarantee ⊆ every sink's assumption (the regulator's 3.3 V ± 5% vs the sensor's "3.3 V ± 3%");
   - the sum of sink currents ≤ the source's `i_max`.

   Without the type, the tool can't know which side sets which number.
3. **The default test setup.** In standalone simulation:
   - a `Power<In>` port automatically gets a DC source swept over its assumed range;
   - an `Analog<In>` port gets the assumed signal;
   - an `Analog<Out>` port gets the assumed load.
4. **Drawing.** Power pins go on top of the symbol, ground at the bottom, inputs left, outputs right. Rails are drawn as power symbols. Types map to KiCad pin types.
5. **Context for the AI.** It knows what each pin *is*.

### 3.3 Where the types come from

From the **standard library**, written in the language itself. They're not built into the compiler, so you can read them and define your own. A sketch of the std definitions:

```rust
// std/signals.spl (sketch)

/// Direction of a single conductor, from the point of view of the part that owns the pin.
pub enum Role { In, Out, InOut, OpenDrain, Passive }

/// A plain conductor with no meaning attached (passive part pins).
pub signal Pin;

/// The reference node. Always bound explicitly; there are no global nets.
pub signal Ground;

/// A supply rail. The source sets the voltage; sinks draw current.
pub signal Power<R: Role> {
    #[by(Source)]    v: Volt,        // sinks assume it, the source guarantees it
    #[by(Source)]    i_max: Amp,     // the source's current capability
    #[by(Source)]    z_src: Ohm,     // source impedance (for the loading check)
    #[by(Sink, sum)] i: Amp,         // drawn by the sinks, summed over the net
}

/// An analog signal.
pub signal Analog<R: Role> {
    #[by(Source)] v: Volt,
    #[by(Source)] z_src: Ohm,
    #[by(Sink)]   z_load: Ohm,
    #[by(Sink)]   c_load: Farad,
}

/// A logic signal. Thresholds come from the pins on the net.
pub signal Logic<R: Role> {
    #[by(Source)] voh: Volt,  #[by(Source)] vol: Volt,
    #[by(Sink)]   vih: Volt,  #[by(Sink)]   vil: Volt,
    #[by(Sink, sum)] c_in: Farad,
}
```

`Power<In>` is a sink and `Power<Out>` a source. **Inside** a block, its own ports are seen flipped: the block's `vcc: Power<In>` acts as the source for the parts inside it.

**You can define your own:** a `Reference` (a precision voltage with a noise spec), a `CurrentLoop` (4–20 mA), a `Bias`. A user-defined signal gets the same checks, benches and drawing rules as the std ones.

### 3.4 Interfaces: bundles of signals

```rust
// std/interfaces.spl (sketch)
pub interface I2c<R: I2cRole> {
    scl: Logic<OpenDrain>,
    sda: Logic<OpenDrain>,
    #[by(Target)] addr: u8,
    rules {
        exactly_one(Controller);
        unique(addr) else "two I2C targets share an address";
        pull_up(scl) && pull_up(sda) else "I2C lines need pull-ups";
    }
}
```

A whole bundle connects in one binding: `let mcu = Attiny85 { vcc: v3v3, gnd, i2c };`, where `i2c` is a net of type `I2c`. Individual lines stay reachable as `i2c.scl`. Other std bundles: `Spi`, `Uart`, `Usb2`, `Diff<S>`, `Swd`.

### 3.5 Arrays and buses

```rust
port raw: [Analog<In>; N];
net ain: [Analog; 8];
```

Arrays bind element-wise; a single net broadcasts to every element (§7.2).

---

## 4. Instances and connections

### 4.1 Nets and pin binding

```rust
net base;
let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };
let r2 = Resistor { a: base, b: gnd, value: 10k ± 1% };
```

**Rules:**
- Nets are declared, and always named.
- **Every pin is bound exactly once**, inside its part's statement. Leaving a pin out is an error unless it has a default (`nc` for pins marked optional in the part).
- A block's ports are nets inside the block.
- **Merging two existing nets** is rare, and has its own statement: `net x = [a, b];`.
- There are no global nets. Ground is a port, bound explicitly.

"What's on `base`?" is answered by the editor and the language server (hover and highlight), not by the syntax.

### 4.2 How Spade and atopile do connections, and why ours looks like this

| | **Spade** | **atopile** | **Ours** |
|---|---|---|---|
| How you connect | No connect statement. **A connection is a variable:** pass the same wire into several `inst` calls. Outputs come back as return values (`let y = inst f(x)`). `port()` makes a two-ended channel, and `set` drives its inverted side | **Explicit statements:** `a ~ b` joins two pins or two interfaces. `power.hv ~> r1 ~> led ~> power.lv` chains through two-terminal parts ("bridge connect", experimental) | **A connection is a net variable** passed into each part's pin fields, like Spade, but with named pins, like atopile |
| Direction | Directed: every wire driven exactly once | Undirected | Undirected; roles are types, checked per net |
| Pin identity | Positional (named form `$(a: x)`) | Named, except generic parts: `resistor.unnamed[0]` | Always named: `a`/`b`, `p`/`n`, `c`/`b`/`e` |
| Where a connection is written | In the instantiation | Anywhere, in separate statements | In the part's own statement: one place per pin |

Why not atopile's `~` and `~>`:
- A chain like `vcc ~> r1 ~> base` hides which pin went where. That's harmless for a resistor but wrong for a diode, an electrolytic or a transistor.
- Statements scattered anywhere make one schematic gesture (placing a part) touch many lines.

Why not Spade's directed ports: analog nets have no driver. One node joins N terminals.

Our form reads like a **SPICE element line with named pins**, which both engineers and the AI already know.

### 4.3 Placing a block

```rust
let amp = CeAmp { vcc: v12, gnd, input: guitar, output: to_adc };
```

Params use the same syntax: `InputProtect { raw: sense, safe: ain, vdd: v3v3, gnd, r: 2.2k ± 1% }`. The difference is visible in the I/O page: ports are pins, params are value fields.

### 4.4 Stable identity

Every instance and net has a path: `board.amp.r1`, `board.amp.base`, `prot.r_s[3]`. Paths are what the layout sidecar, knob names, AI edits and version-control diffs refer to. Renames go through one refactor command that updates every reference; a hand rename is detected by matching part type and connections.

---

## 5. Values

### 5.1 Units and literals

- **Quantities are typed by physical dimension:** `Volt`, `Ohm`, `Farad`, `Amp`, `Watt`, `Hertz`, `Celsius`, … Dimensions are checked at compile time (`Volt * Amp == Watt`; `Ohm * Farad == Second`).
- **Literals:** `47k`, `3.3V`, `100nF`, `4k7`, `1uF` (or `1µF`), `10kΩ`. A bare `47k` takes its unit from where it's used (`value:` of a `Resistor` expects `Ohm`).
- **Prefixes are case-sensitive:** `m` is milli, `M` is mega. `1Meg` (the SPICE habit) gets a fix-it. `1K` is kelvin, not kilo.

### 5.2 Tolerances and ranges

- **Tolerance:** `47k ± 1%`, or ASCII `47k +/- 1%`. Also absolute: `3.3V ± 50mV`.
- **Ranges** are closed and use Rust's `..=`: `100..=300`, `4.5V..=6.5V`, `..=100mV` (up to), `10kΩ..` (at least). Rust's `..` stays the half-open integer range, for loops.
- **Precedence:** `in` and comparisons bind loosest, then `..=`, then `±`, then arithmetic. An unparenthesized `+ - * /` expression as an operand of `±` is an error, because `capacity / 2h ± 10%` has two plausible readings (grammar.md §4).
- **A tolerance is its own type,** `Tol<Ohm>`, distinct from an exact `Ohm`. That's how a param like `r: Tol<Ohm> = 1k ± 1%` says "each part built from this gets its own independent spread".

### 5.3 Where a range appears decides what it means (⊆ vs ⊇)

The same `4.5V..=6.5V` can mean "must stay inside" or "can be anywhere inside". The keyword in front always tells you which:

| Written | Meaning | Quantifier |
|---|---|---|
| `value: 47k ± 1%`, `beta: 100..=300` (a value position) | **Spread:** the real part can be anywhere in here | for all (statistical) |
| `assume vcc.v in 12V ± 5%` | **Condition:** the world can be anywhere in here | for all (range) |
| `spec bias: dc(output.v) in 4.5V..=6.5V` | **Limit:** the result must stay inside | ⊆ |
| `value: ?` | **Search:** the solver picks a value | exists |

On top of that, the compiler enforces **"your guarantee is my spread"**: a block may only `assume` quantities its role *consumes*, and only `spec` quantities its role *produces* (§8.2).

### 5.4 Knobs come from the language

| Written | Knob | Kind |
|---|---|---|
| `env temp: Celsius;` plus `assume temp in -10°C..=60°C` in the top contract | `temp` (shared by every block) | range |
| `assume vcc.v in 12V ± 5%` | `amp.vcc.v` | range |
| `assume input in Sine { amp: ..=100mV, … }` | `amp.input.amp`, `amp.input.freq` | range |
| `assume mode in {Run, Sleep}` | `mode` | discrete (mode) |
| `value: 47k ± 1%`, or the tolerance from a pinned `part:` | `amp.r1.value` | statistical |
| `value: ?` | `amp.r1.value` | solver |

**Links** connect part values to shared knobs. They come from part records (§6), or can be written directly: `tc: ±100ppm/K` links to `temp`, `aging: -20%` links to `life`.

**Lots:** `let net_r = lot(RN_0603_4x10k)` makes parts that share one lot knob plus small per-part knobs (resistor networks, same reel).

### 5.5 Exact values vs design values

- **Exact:** known at build time, e.g. `8`, `Clamp::Tvs`, `4.7k` without tolerance.
- **Design:** carries knobs, `?`, or simulated results.

Structure (`if`, `for`, `match`, array sizes) may depend only on exact values. `if gain > 10 { … }` on a toleranced `gain` is a compile error. This is Spade's type-level vs runtime split.

### 5.6 A committed value plus how it was derived

```rust
let r_fb_bot = Resistor { a: fb, b: gnd, value: 22.1k ± 1% from e96(fb_bottom(3.3V, u1.v_ref.nom(), 100k)) };
```

`22.1k` is the value the design uses. `from …` records how it was derived. When an input changes, the vision's two propagation modes work on this pair:
- **cascade:** rewrite `22.1k`, and show a diff;
- **flag:** keep `22.1k`, and mark the derivation stale.

A `///` doc comment is the rationale, shown on the part's value card.

**Bare values:** `47k` with no tolerance means ideal. A lint warns when a physical part has no tolerance and no pinned `part:`.

---

## 6. Parts: primitives, part kinds, and real manufacturer parts

### 6.1 The standard part kinds

| Kind | Pins | Notes |
|---|---|---|
| `Resistor` | `a`, `b` | value, power rating |
| `Capacitor` | `a`, `b` | non-polarized; `dielectric: C0G / X7R / X5R / Y5V / Film` |
| `Electrolytic`, `Tantalum`, `Polymer` | `p` (+), `n` (−) | polarized |
| `Inductor` | `a`, `b` | saturation current, DCR |
| `Diode`, `Zener`, `Schottky`, `Led`, `Tvs` | `a` (anode), `k` (cathode) | |
| `Npn`, `Pnp` | `c`, `b`, `e` | |
| `Nmos`, `Pmos` | `d`, `g`, `s` | |
| `VoltageSource`, `CurrentSource` | `p`, `n` | for benches |

**Pin names encode polarity:** non-polarized parts have `a`/`b`, polarized parts have `p`/`n`.

Each kind maps to a simulator device, so it simulates as soon as it has a value. Each is also a generic, pickable part (§6.6).

### 6.2 What is `Electrolytic`, and why a separate kind?

It's the standard kind for aluminium electrolytic capacitors. A separate kind (instead of `Capacitor { dielectric: Electrolytic }`) because it differs in ways that matter to the engine, not just in how it reads:

- **Different pins.** `p`/`n` instead of `a`/`b`, with an automatic check that the voltage from `p` to `n` never goes meaningfully negative, at every corner and during start-up.
- **Different default knobs.**
  - Electrolytics lose capacitance and gain ESR as they age (a link to `life`), and lose capacitance when cold.
  - Ceramics instead lose capacitance with DC voltage (evaluated in the device model at the operating point).
- **Different derating rules** and a different catalog to pick from.

So `Electrolytic` is a real type with its own fields and checks. Non-polarized dielectrics differ mainly in parameters, so they're a field on `Capacitor`.

### 6.3 Generic parts vs real parts

There are two layers:

1. **What the circuit needs:** `Resistor { value: 47k ± 1% }`. This is a **budget**. The engine can analyze with it immediately, using ±1% as the spread.
2. **What you actually buy:** a specific manufacturer part number (MPN). Its **part record** carries the datasheet data:
   - tolerance, tempco, power and voltage ratings, derating curve;
   - drift and aging;
   - for capacitors: dielectric and DC-bias curve;
   - package, footprint, symbol, simulation model, ordering data;
   - **where every number came from.**

When a part is pinned, the engine uses **the part's data**, so derating uses the real voltage rating and drift uses the real endurance data.

### 6.4 Pinning a manufacturer part

```rust
// a whole family: the compiler picks the member with value 47k → MPN "RC0603FR-0747KL"
let r1 = Resistor { a: vcc, b: base, value: 47k ± 1%, part: yageo::RC0603FR };

// one exact part
let c_dec = Capacitor { a: v3v3, b: gnd, value: 100nF ± 10%, part: murata::GRM188R71E104KA01 };

// the part fixes the value; writing it anyway documents intent and is checked
let q1 = Npn { c: output, b: base, e: emitter, part: onsemi::MMBT3904 };
```

The compiler checks that **the part fits the budget.** A ±1% part fits `47k ± 1%`. A ±5% part is an error: *"`yageo::RC0603JR` is ±5%, looser than the design budget ±1%"*.

The written value stays the design intent. The part record supplies everything else (tempco, ratings, drift, model).

### 6.5 Part records and families

Part records are ordinary language files in the project's `parts/` folder (version-controlled, reviewable), or in shared packages. A sketch. **The numbers below are illustrative; a real record is extracted from the datasheet with page references:**

```rust
// parts/yageo/rc0603.spl
/// Yageo RC0603 thick-film chip resistors, ±1% series.
#[source(datasheet = "https://www.yageo.com/…/PYu-RC_Group_51_RoHS_L.pdf", extracted = "ai", reviewed = false)]
pub family RC0603FR: Resistor {
    values: E96 in 1Ω..=10MΩ,
    mpn: "RC0603FR-07{code}L",               // {code}: 47k → "47K"
    package: R0603,

    #[from(page = 2, table = "Electrical characteristics")]
    tolerance: ± 1%,                          // tested limit
    #[from(page = 2)]
    tcr: ±100ppm/K,                           // statistical coefficient × temp (engine slices over temp)

    operating {
        power <= 0.1W derated linearly to 0W between 70°C..=155°C;
        voltage <= 75V;
        temp in -55°C..=155°C;
    }
    #[from(page = 4, table = "Endurance")]
    drift: ±1% after 1000h at (power: rated, temp: 70°C);   // stress-test data, not a lifetime figure
}
```

```rust
// parts/murata/grm188r71e104ka01.spl
/// 100 nF, 25 V, X7R, 0603 MLCC.
#[source(datasheet = "…", extracted = "ai", reviewed = true, reviewer = "ido")]
pub part GRM188R71E104KA01: Capacitor {
    value: 100nF ± 10%,
    dielectric: X7R,
    package: C0603,
    operating { voltage <= 25V; temp in -55°C..=125°C; }
    /// Capacitance vs DC bias, from the manufacturer's characterization data.
    dc_bias: table { 0V: 0%, 5V: -8%, 10V: -20%, 16V: -35%, 25V: -52% },   // illustrative
    tc: X7R,                                  // ±15% over −55…125 °C, as an envelope
}
```

Pieces of this sketch:

- **`family`** covers a whole series in one record, because nobody wants a file per resistor value.
- **`#[source]`** and **`#[from]`** hold provenance for the record and for each field.
- **`operating { }`** and **`absolute_max { }`** hold the part's own assumptions. They become automatic checks (§8.8).
- **Shapes** (tables, envelopes, aging laws) are how the engine's knob model gets its data (engine v3 §2).

### 6.6 Adding a part from an MPN or a URL

From the editor (`:part add RC0603FR-0747KL` or `:part add https://…/datasheet.pdf`), or by asking the AI:

1. **Fetch.** Get the datasheet PDF and, where available, distributor parametric data. The catalog sources are pluggable, so the tool isn't tied to one distributor.
2. **Extract.** The AI writes a part record. Every number carries `#[from(page, table)]`, so a human can check it against the source in one click.
3. **Draw.** The symbol and footprint come from KiCad libraries, or are generated. A vendor SPICE model is attached if one is available, with a fidelity tag (typical-only, temperature-aware, …).
4. **Review.** The record starts as `reviewed = false`. Any verdict that relies on unreviewed data carries a tag saying so, until an engineer confirms the record in the review view.

This fills the gap atopile leaves open. Its auto-generated part files hold only ordering and footprint data; the electrical facts ("1uF ±10% 25V") appear only as a comment (`examples/esp32_minimal/parts/…CL05A105KA5NQNC.ato`). So its checks can't use the chosen part's real tolerance or rating.

### 6.7 Letting the tool pick

```rust
let r1 = Resistor { a: vcc, b: base, value: 47k ± 1%, part: ? };     // pick a part that fits
let r_top = Resistor { a: vout, b: fb, value: ?, part: ? };          // pick value and part together
```

- The picker searches the catalogs for parts that fit the budget **and** pass the automatic checks (voltage rating, power at the worst corner), then ranks them by price, availability and preferred vendors.
- The choice is written back as a committed value with its derivation (`part: yageo::RC0603FR from pick(…)`), so it's reviewable and stable until re-picked.

### 6.8 ICs as parts

```rust
// parts/ti/tps54302.spl (abridged; values from the TPS54302 datasheet, SLVSDG6C)
#[mpn = "TPS54302DDCR"] #[footprint = "SOT-23-6"]
#[source(datasheet = "https://www.ti.com/lit/ds/symlink/tps54302.pdf", extracted = "ai", reviewed = false)]
pub part Tps54302 {
    port vin: Power<In>;
    port gnd: Ground;
    port sw: Power<Out>;
    port boot: Pin;
    port fb: Analog<In>;
    port en: Logic<In>;
    pins { gnd: 1, sw: 2, vin: 3, fb: 4, en: 5, boot: 6 }

    operating { vin.v in 4.5V..=28V; temp in -40°C..=125°C; }
    absolute_max { /* from the datasheet's Absolute Maximum Ratings table */ }

    /// Electrical characteristics: the part's own spreads (statistical knobs).
    v_ref: 0.581V..=0.611V,      // typ 0.596 V
    f_sw: 290kHz..=510kHz,       // typ 400 kHz

    model: spice("models/tps54302_trans.lib", subckt: "TPS54302_TRANS", fidelity: typical_only),
}
```

The datasheet maps onto the part directly. It is the part's contract:

| Datasheet section | In the record | Becomes |
|---|---|---|
| Absolute Maximum Ratings | `absolute_max { }` | worst-case survival checks, at every time point |
| Recommended Operating Conditions | `operating { }` | worst-case "it works" checks |
| Electrical Characteristics | the part's spreads | statistical knobs |

---

## 7. Generics and loops (build time)

### 7.1 Two kinds of parameters

**If it changes the drawing, it goes in `<>`. If it changes a number on the drawing, it's a `param`.**

```rust
pub block InputProtect<const N: usize = 8>
where N >= 1 && N <= 32 else "InputProtect supports 1–32 channels"      // from Spade
{
    port raw: [Analog<In>; N];
    port safe: [Analog<Out>; N];
    port vdd: Power<In>;
    port gnd: Ground;

    /// Limits clamp current to ≈ 8 mA for a ±12 V fault.
    param r: Tol<Ohm> = 1k ± 1%;
    param c: Tol<Farad> = 10nF ± 10%;

    for i in 0..N {
        let r_s[i]  = Resistor  { a: raw[i], b: safe[i], value: r };
        let c_f[i]  = Capacitor { a: safe[i], b: gnd, value: c, dielectric: C0G };
        let d_hi[i] = Schottky  { a: safe[i], k: vdd, part: nexperia::BAT54 };
        let d_lo[i] = Schottky  { a: gnd, k: safe[i], part: nexperia::BAT54 };
    }
}
```

- `let r_s[i] = …` gives each instance a stable path: `prot.r_s[3]`.
- Each resistor gets its **own** knob, even though they share the param `r`, because `r` is a `Tol<Ohm>`.
- Editing `r` changes a number. Editing `N` regenerates the sheet.

### 7.2 The drawable form of the same thing

Array instances need no loop, so they stay editable as a drawing:

```rust
let r_s[0..N] = Resistor { a: raw, b: safe, value: r };        // element-wise binding
let c_f[0..N] = Capacitor { a: safe, b: gnd, value: c };       // gnd broadcast to every element
```

These render as arrayed symbols labeled `×N`.

---

## 8. Contracts

### 8.1 The `contract` item

It lives in the same file as its block, and holds four things:
- assumptions (§8.2),
- measures (§8.4),
- specs (§8.3),
- benches (§8.5).

It can see the block's internals (nets, parts). Other blocks see only its ports and `pub` items.

It is the text form of the I/O page. Every column of the spec table has exactly one place in the code, and a canonical formatter prints it, which is what makes editing the table and editing the text interchangeable.

**The project is the top block.** Its contract holds the product's operating envelope:

```rust
contract Board {
    assume temp in -10°C..=60°C;
    assume life in 0y..=10y;
}
```

### 8.2 Assumptions

```rust
assume vcc.v in 12V ± 5%;                                        // port quantity the source sets
assume input in Sine { amp: ..=100mV, freq: 20Hz..=20kHz };      // signal family, as a pattern
assume input.z_src <= 1kΩ;                                       // what drives us
assume output.z_load >= 10kΩ;                                    // what we drive
assume mode in {Run, Sleep};                                     // discrete modes
```

- **Each becomes a range knob** for the block's own specs, and a **check** wherever the block is used.
- **Signal families:** `Dc`, `Sine`, `Band`, `Step`, `Pulse`, `Logic`.
- **Inheritance:** a block that says nothing about `temp` inherits the project's range.
- **Narrowing:** a block may declare its own rated range (a library LDO rated −40…85 °C). A connection check confirms the project's range fits inside it.
- **Different local conditions:** an instance that really sees a different environment is mapped at the placement:
  ```rust
  #[env(temp = temp + 15K, reason = "next to the buck inductor")]
  let amp = CeAmp { … };
  ```

**Roles decide what you may assume and what you may guarantee.** A block may `assume` only quantities its role consumes, and `spec` only quantities it produces:

```
error[E-role]: `assume` on a quantity this block produces
 --> ce_amp.spl:31:12
   |
31 |     assume output.v in 4.5V..=6.5V;
   |            ^^^^^^^^ `output: Analog<Out>`: this block drives `output.v`
   = help: a range on a quantity you drive is a guarantee: `spec output_range: output.v in 4.5V..=6.5V;`
```

### 8.3 Specs

The grammar is `spec name: measure relation bound [on bench] [where condition];`.

```rust
/// Room for the output to swing ±1 V without clipping.      ← the rationale
#[confidence(worst_case)]                                    ← optional attributes
spec bias: dc(output.v) in 4.5V..=6.5V;
```

- **Relations:** `in` (range or `x ± tol`), `<=`, `>=`, `<`, `>`.
- **Attributes:**
  - `#[confidence(worst_case | sigma(3) | yield(99.9%) | nominal)]`
  - `#[warn]` (a warning, not an error)
  - `#[expect(fail, reason = "…")]` (a known failure, tracked)
- **Conditions** narrow a single spec: `where temp == 25°C` or `where life == 0y`.
- **Default confidence:** `sigma(3)` for your specs (one-sided 3σ ≈ 99.87%), `worst_case` for automatic checks.

### 8.4 Measures and probes

**A measure is a value.** `let vc = dc(output.v);` has type `Volt`. So measures mix with hand formulas without double-counting shared knobs:

```rust
let headroom = vcc.v - dc(output.v);      // shares the vcc.v knob correctly
spec top_room: headroom >= 2V;
```

**How the engine keeps that promise** (decision D-F, `engine_plan.md` §10): in the MVP a derived measure is **evaluated per run**. `headroom` takes `vcc.v` and `dc(output.v)` from the same simulated board, so one knob can't be counted twice, and over the corners the answer is exact (5.1951 V on the CE amp). Affine forms in named knobs (engine v3 §3.1) return when results must be combined without re-simulating: hierarchy, calibration, error budgets and datasheet arithmetic.

**Probes are field accessors:**

| Probe | Meaning |
|---|---|
| `net.v`, `port.v` | voltage to ground |
| `a.v - b.v` | differential |
| `q1.c.i` | current into a pin |
| `r1.power` | power dissipated in a part |

**Analyses:** `dc(…)`, `ac(…)`, `tran(…)`, `noise(…)`, later `pss(…)`.

**Measurement methods are typed by result,** so the editor offers the right ones after a `.`:
- **AC:** `.at(f)`, `.mag()`, `.phase()`, `.f_low(-3dB)`, `.bandwidth()`, `.phase_margin()`
- **Transient:** `.peak()`, `.pp()`, `.rms()`, `.overshoot()`, `.crossing(…)`, `.thd(f)`

**Smooth measures are preferred.** Non-smooth ones (settling time) are flagged, and rewritten to a smooth equivalent where one exists (a windowed maximum). A missing crossing is never treated as a pass. A crossing shown to lie **beyond** the searched band (e.g. an f_low below its lowest frequency) is a one-sided bound, not a missing crossing: it can decide the passing side of a spec, never the failing side (`research/engine_synthesis.md` C9).

### 8.5 Benches (test setups)

- **Default bench:** derived from the assumptions, never written. `Power<In>` ports get swept DC sources, signal inputs get the assumed source, outputs get the assumed loads.
- **Form bench:** overrides a few things, using Rust's struct-update syntax. It is edited as a form on the I/O page:
  ```rust
  bench loud { input: Sine { amp: 100mV, freq: 1kHz }, tran: 0ms..=20ms, ..default }
  spec clip: tran(output.v).thd(1kHz) < 1% on loud;
  ```
- **Sheet bench:** for complex fixtures, a separate schematic that places the design, like a Cadence ADE test:
  ```rust
  bench LoadStep for Buck3v3 { /* electronic load, probes, … */ }
  ```

Every bench is checked to stay inside the block's assumptions.

### 8.6 "For all" needs no syntax

Every range knob is already universally quantified, so "for all load currents" or "for all inputs up to 100 mV" is automatic. Analysis axes are restricted with `.band(20Hz..=20kHz)` and `.window(1ms..=5ms)`.

Rust `for` means only build-time generation, as in Rust:

```rust
for i in 0..N {
    spec gain[i]: ac(safe[i].v / raw[i].v).at(1kHz).mag() in 1 ± 1%;
}
```

### 8.7 Automatic checks

Every automatic check is an **assumption of a part or interface**, discharged by the circuit around it:

| Source | Examples | Default |
|---|---|---|
| `absolute_max { }` in part records | every IC pin within abs-max, at every corner and during start-up | error |
| `operating { }` in part records | supply in range, temperature in range | error |
| Polarized kinds | electrolytic/tantalum reverse voltage | error |
| Signal types | logic levels (VOH ≥ VIH, VOL ≤ VIL, with margin), rail source capability, one source per rail | error |
| Interface rules | I2C pull-ups and rise time (from UM10204), unique addresses | error |
| Derating policy | capacitor voltage, resistor power, semiconductor power and temperature | warning |
| Thermal | junction temperature at worst corner (electrothermal fixed point) | warning |
| Connections | upstream guarantee ⊆ downstream assumption, at every block connection | error |

**Levels and waivers follow Rust's lint model:**

```rust
#[allow(derating::capacitor::voltage, reason = "X7R at 60% of rating is fine for this life")]
let c_out = Capacitor { … };
```

There's also a project-wide `[checks]` table (like Cargo's `[lints]`) to choose the derating policy (commercial, NASA, ECSS) and set levels.

### 8.8 Composition

At every connection between blocks:

- **The check:** upstream guarantee ⊆ downstream assumption. Example: the buck guarantees `vout.v in 3.3V ± 5%`, the sensor node assumes `3.3V ± 3%`, and the check fails, with both lines shown.
- **Lifting:** a child's assumption on a port it shares with its parent shows greyed out on the parent's I/O page, with a button that writes it as the parent's own `assume`.
- **Evidence ladder:** in context, the engine can go beyond declared guarantees, to the upstream block's **characterized** results (affine forms, so shared knobs stay correlated), and finally to **in-context simulation**.

---

## 9. Types and checks

**Three tiers:**

1. **Shape (type checker, instant).** Wrong port type, a missing pin binding, unit mismatch, `if` on a toleranced value.
2. **Roles (net checker, after build-time expansion).** Two sources on a rail, an unpowered IC, fighting outputs, missing pull-ups, duplicate I2C addresses. Nets joined through an inductor, fuse or ferrite count as one rail, so a buck's switch node feeding its output through the inductor passes.
3. **Numbers (engine).** Logic levels, derating, abs-max, contract checks. These are reported instantly when every input is exact, and otherwise by the engine with a verdict and a range.

Sample diagnostics:

```
error[E-rail]: two sources drive rail `v3v3`
 --> board.spl:14:5
   |
12 |     let buck = Buck3v3 { vin: v12, vout: v3v3, gnd };
   |                                      ---- source 1: `buck.vout: Power<Out>`
14 |     let ldo = Ldo3v3 { vin: v5, vout: v3v3, gnd };
   |                                   ^^^^ source 2: `ldo.vout: Power<Out>`
   = help: rename one rail, or add an ORing diode / ideal-diode controller between them
```

```
error[E-unit]: expected `Ohm`, found `Farad`
 --> amp.spl:9:52
  |
9 |     let r2 = Resistor { a: base, b: gnd, value: 10nF ± 1% };
  |                                                 ^^^^ this is a capacitance
```

```
error[E-part]: part is looser than the design budget
 --> amp.spl:8:61
  |
8 |     let r1 = Resistor { a: vcc, b: base, value: 47k ± 1%, part: yageo::RC0603JR };
  |                                                 --------        ^^^^^^^^^^^^^^^ ±5%
  |                                                 budget ±1%
  = help: use `yageo::RC0603FR` (±1%), or widen the budget if ±5% is acceptable
```

---

## 10. In the editor

- **Drawable = the flat subset.** A block with no `for`/`if`/`match` (array instances allowed) opens as an editable schematic. It is stored as ordinary language text, plus a **layout sidecar** that holds only geometry. Delete the sidecar and the netlist, values, verdicts and SPICE deck are byte-identical; the editor just re-places the symbols.
- **Generated blocks** (with loops) show read-only structure. Values that trace back to a single literal stay editable in place.
- **Modes:**
  - *Edit mode* edits the **block** (parts, nets, values).
  - *Simulate mode* edits the **contract** (probes, specs, assumptions, benches).
- **Every gesture is a named statement edit.** Drawing, the `:` command line, paste and AI edits all become the same edits to the text, so the text is always the truth.
  - **How an edit reaches the text:** the parser keeps every token (comments and whitespace included) and a byte span on every syntax node. Changing r1's value replaces the bytes of that one value expression; the formatter then re-prints only that statement, so the rest of the file is untouched. No lossless syntax tree is needed for this (roadmap §4.4).
- **Results never live in the source.** Verdicts appear:
  - as badges on the schematic,
  - in the spec table,
  - as inlay hints in the code:
    ```
    spec bass: h.f_low(-3dB) <= 30Hz;     ✗ 31.2 Hz (3σ) · 1.1% of boards · C_in
    ```
  - A counterexample is printed as a pasteable `corner` for re-simulation.
- **AI "select to ask"** sends:
  - a slice of the model that compiles on its own, as language text;
  - a rendered image with the same labels;
  - the relevant results.

  The AI answers with named edits, which are shown as a schematic diff together with how the verdicts would change, before you accept. Every verdict in that diff comes from a re-check of the edited design, with its verdict word (PASS (all corners), PASS (estimated), PASS (implied by worst case), FAIL, UNDECIDED) and its status (`simulated`, or `stale` once the design changes). Nothing is shown as "verified" or as an unchecked prediction (`engine_plan.md` §3, §7).

Full detail, including auto-placement and KiCad import/export: `research/language_editor_mapping.md`.

---

## 11. More examples

### 11.1 A buck converter block

```rust
use std::prelude::*;
use ti::Tps54302;

/// Bottom feedback resistor for V_out = V_ref · (1 + R_top/R_bot).
pub fn fb_bottom(v_out: Volt, v_ref: Volt, r_top: Ohm) -> Ohm { r_top * v_ref / (v_out - v_ref) }

/// 8–28 V → 3.3 V / 2 A buck around TI TPS54302 (400 kHz, internal compensation).
pub block Buck3v3 {
    port vin: Power<In>;
    port vout: Power<Out>;
    port gnd: Ground;

    net sw;
    net boot;
    net fb;
    net en;

    let u1 = Tps54302 { vin, gnd, sw, boot, fb, en };

    let c_in    = Capacitor { a: vin, b: gnd, value: 10uF ± 10%, dielectric: X7R, rating: 50V };
    let c_boot  = Capacitor { a: boot, b: sw, value: 100nF ± 10%, dielectric: X7R, rating: 16V };
    /// UVLO divider: starts at ≈ 1.23 V · (1 + 511k/105k) ≈ 7.2 V.
    let r_en_top = Resistor { a: vin, b: en, value: 511k ± 1% };
    let r_en_bot = Resistor { a: en, b: gnd, value: 105k ± 1% };

    let l1     = Inductor  { a: sw, b: vout, value: 6.8uH ± 20%, i_sat: 6A };
    let c_out1 = Capacitor { a: vout, b: gnd, value: 22uF ± 20%, dielectric: X7R, rating: 10V };
    let c_out2 = Capacitor { a: vout, b: gnd, value: 22uF ± 20%, dielectric: X7R, rating: 10V };

    let r_fb_top = Resistor { a: vout, b: fb, value: 100k ± 1% };
    let r_fb_bot = Resistor { a: fb, b: gnd, value: 22.1k ± 1% from e96(fb_bottom(3.3V, u1.v_ref.nom(), 100k)) };
    let c_ff     = Capacitor { a: vout, b: fb, value: 47pF ± 5%, dielectric: C0G };
}

contract Buck3v3 {
    assume vin.v in 8V..=28V;
    assume vout.i in 0A..=2A;                 // what the loads may draw

    /// Output accuracy. ±3% would fail: V_ref alone spans ±2.5%.
    spec vout_dc: dc(vout.v) in 3.3V ± 5%;
    spec capability: vout.i_max >= 2A;
    spec ripple: tran(vout.v).pp() <= 30mV on steady;
    spec transient: tran(vout.v).deviation() <= 5% on load_step;
    spec loop_pm: ac(u1.loop_gain()).phase_margin() >= 45°;

    bench steady    { vout: Load::Dc(2A), tran: 0ms..=2ms, ..default }
    bench load_step { vout: Load::Step { from: 0.2A, to: 2A, rise: 1us }, ..default }
}
```

The engine reports a worst case of 3.16 … 3.43 V for `vout_dc` (research/language_core.md §10.1). That passes ±5% and fails ±3%, which is exactly what the doc comment claims, and now it's checked instead of asserted. Automatic checks add current limit vs peak inductor current, inductor saturation vs current limit, and capacitor voltage derating (engine v3 §4).

### 11.2 A board, where a contract fails at a connection

```rust
pub env temp: Celsius;
pub env life: Time;

pub block Board {
    net gnd: Ground;
    net v12;
    net v3v3;
    net audio_in;
    net audio_out;

    let j1   = DcJack { vbus: v12, gnd };
    let amp  = CeAmp { vcc: v12, gnd, input: audio_in, output: audio_out };
    let buck = Buck3v3 { vin: v12, vout: v3v3, gnd };
    let node = SensorNode { v3v3, gnd };      // assumes v3v3.v in 3.3V ± 3%
}

contract Board {
    assume temp in -10°C..=60°C;
    assume life in 0y..=10y;
    assume j1.vbus.v in 12V ± 5%;             // the external adapter
}
```

```
error[E-contract]: `buck.vout` guarantees 3.3 V ± 5%, but `node.v3v3` assumes 3.3 V ± 3%
  = help: tighten the buck (0.1% feedback resistors in a `lot`, or a better reference), or widen SensorNode's assumption
```

The MCU + I2C sensor example (ATtiny85 + two TMP117s, with pull-up and address checks) is in `research/language_core.md` §10.2.

---

## 12. Open questions

1. **Name and file extension.**
2. **Part record details:** the exact syntax for families, tables, envelopes and aging laws. Should follow the engine v3 knob model as it gets implemented.
3. **Catalog sources:** which distributor and parametric APIs first, and how to cache them for reproducible builds.
4. **Multi-function MCU pins** (PB0 = SDA or MOSI): how a pin's role is selected.
5. **Variants and do-not-populate:** per-build differences.
6. **Which parts join rails by default** (inductor, ferrite, fuse, shunt), and the escape hatch.
7. **Behavioral models in the language** vs SPICE files only.
8. **How much of the interface `rules { }` language users can write** in v1.
9. **Temporal specs** (power sequencing order and delays): a small signal-temporal-logic subset, later.
10. **Formatting control** the editor takes over drawn files.
