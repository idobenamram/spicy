# Language Design v0.2

> 2026-09-30 · v0.2: the syntax of `research/contract_syntax_v5.md`, as decided in `syntax_v5_plan.md`. v0.1 (2026-09-25) merged `research/language_core.md`, `research/language_specs.md` and `research/language_editor_mapping.md`, using the decisions in `research/synthesis.md` §4.3.
> The research reports hold the longer arguments and more examples, in v0.1's syntax. Grammar details are in `research/contract_v4_review_implementation.md` ("[IR]") §2, elaboration in its §3. Engine concepts are in `engine.md`.
> **What's built:** the parser reads the MVP subset (plan steps 1–4; the table is in §2.2). Where a construct is only parsed, or not parsed yet, its section says so. What v5 doesn't decide is kept from v0.1 and marked *not decided in v5*.
> Name and file extension are still open; examples use `.spl`.

---

## 0. The shape on one page

```rust
/// The temperature the amplifier works in, anywhere it's used.
env ambient: Temperature in -10°C..=60°C;

/// Common-emitter audio stage. Gain ≈ RC/RE ≈ 4.6, f_L ≈ 20 Hz.
pub block CeAmp { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }

circuit CeAmp {
    net base;                         // internal nodes
    net emitter;

    /// Divider holds the base near 2.1 V, which sets IC ≈ 1.4 mA.
    let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };
    let r2 = Resistor { a: base, b: gnd, value: 10k ± 1% };
    /// Gain ≈ RC/RE. RC also puts VC near mid-supply.
    let rc = Resistor { a: vcc, b: output, value: 4.7k ± 1% };
    let re = Resistor { a: emitter, b: gnd, value: 1k ± 1% };
    /// f_L = 1/(2π·C_in·R_in) ≈ 20 Hz.
    let c_in = Electrolytic { p: base, n: input, value: 1uF ± 20% };
    let q1 = Npn { c: output, b: base, e: emitter, beta: 100..=300 };
}

/// The amplifier as it's used: a 12 V rail, a quiet input, an open output.
setup Operating for CeAmp {
    vcc: Supply { v: 12V ± 5% },
    input: Signal { v: 0V },
    output: Load {},
    temp: ambient,
}

contract CeAmp {                      // the I/O page (§8)
    setup = Operating;                // specs run here unless they say otherwise

    let h = ac(output.v / input.v);

    /// Room for the output to swing ±1 V without clipping.
    spec bias: dc(output.v) within 4.5V..=6.5V;
    /// The next stage expects this level.
    spec gain: h.at(1kHz).mag() within 4.6 ± 5%;
    /// Don't cut the bass.
    #[confidence(yield(99.9%))]
    spec bass: h.f_low(-3dB) <= 30Hz;
    /// Internal: it names a net, so a parent can't rely on it.
    spec base_bias: dc(base.v) within 1.9V..=2.3V;
}
```

The ideas:

- **`block`:** the interface: a name and its ports (§3). You can place it many times.
- **`circuit`:** the implementation: the block's nets and parts (§4). One per block.
- **`setup`:** a test environment for one block: what drives each input, what loads each output, the temperature (§8.2). It's written outside the contract, and reusable.
- **`contract`:** the block's promises: its default setup and the specs that must hold (§8). A `pub spec` is one a parent may rely on.
- **`env`, `const`:** project-level. An `env` is a shared environment range the engine searches; a `const` is a fixed value.
- **Connecting:** every part binds each of its pins to a named net, in the part's own statement.
- **Values:** carry units and tolerances.
- **Real parts:** a `part:` field pins a manufacturer part, whose datasheet data the engine then uses (§6).
- **Results never live in the file.** The editor shows verdicts next to the specs.

This is `circuits/ce_amp.spl` plus two things: the internal spec `base_bias` (v5 §2 has it; the circuit file leaves it out so the engine's pinned numbers hold, plan decision 3), and the `#[confidence]` attribute. **MVP:** the parser reads all of it. Resolve elaborates the block and its circuit, and matches the setup and the contract to `CeAmp` by name. Their contents are elaborated in the next phase (plan §3).

---

## 1. Principles

| # | Principle | What it means in practice |
|---|---|---|
| P1 | **If it looks like Rust, it behaves like Rust** | We may accept *more* than Rust, never change the meaning of something Rust accepts. So `+-` and `--` are rejected (in Rust they mean `+ (-…)` and `- (-…)`). `..Base` is Rust's struct update, `a..` is Rust's `RangeFrom` |
| P2 | **One gesture ↔ one statement** | Placing a part = one `let`. Wiring a pin = one field in that `let`. Editing a spec = one `spec` line. Changing a condition = one setup field |
| P3 | **Names are identity** | `board.amp.r1`, `prot.r_s[3]`. Used by the layout sidecar, knob names, AI edits, diffs |
| P4 | **Order-independent bodies** | A circuit is a set of named declarations. Feedback loops need no forward declarations. A spec's `for f in …` may come after the measure that uses `f` |
| P5 | **Types catch what needs no numbers; the engine catches what does** | Shorted supplies are a type error; "5 V into a 3.3 V pin" is an engine check with tolerances |
| P6 | **Structure is decided at build time** | `for`, `if`, array sizes and generic arguments depend only on exact values, never on toleranced ones |
| P7 | **Small** | No lifetimes, borrowing, closures or macros in v1. v5 adds `trait` for families of blocks; it's reserved, not designed yet (§7.3) |
| P8 | **Written for the AI too** | One canonical format, one way to write each thing, fix-its in every diagnostic |

---

## 2. Items: what you can write at the top level

| Item | What it is | In the schematic | Today (§2.2) |
|---|---|---|---|
| `block` | The interface: a name and its ports; later its generics, and the values and events it publishes | A **symbol** (a box with its ports as pins) | parsed and elaborated |
| `circuit` | The implementation: nets and parts. One per block | The block's **sheet** | parsed and elaborated |
| `setup` | A test environment for one block: sources, loads, temperature | A form on the block's **I/O page** | parsed, matched to its block |
| `contract` | The block's default setup, measures and specs | The spec table on the block's **I/O page** | parsed, matched to its block |
| `env` | A project-wide environment range the engine searches (temperature, lifetime) | The project's I/O page | parsed |
| `const` | A fixed project-wide value | — | parsed |
| `trait`, `impl` | Families of blocks, which bring std setups and specs (v5 §1.1) | — | reserved |
| `part` | A leaf component: pins, datasheet data, simulation model, ordering data | A library **symbol** | not parsed |
| `family` | A series of parts that differ only in value (e.g. all 1% 0603 resistors) | The part picker | reserved |
| `fn` | Pure math on values | Nothing. Shows up in value cards ("R_bot = e96(…)") | reserved |
| `signal`, `interface` | Port types and bundles (std, or your own) | Pin types, buses | reserved |
| `enum`, `type` | As in Rust | — | reserved |
| `mod`, `use` | As in Rust: files, libraries and namespaces | The project tree | reserved |

The items from `part` down aren't in v5's item list. Their design is v0.1's, unchanged.

**`pub`:** only a block or a spec can be `pub` (the parser's rule). `pub circuit`, `pub setup`, `pub contract`, `pub env` and `pub const` are errors whose fix removes the `pub`. A circuit, a setup and a contract belong to their block, and an `env` or a `const` is project-wide. How library items (parts, families, `fn`s, signals) are exported isn't decided in v5, so the examples below write them without `pub` (§12).

### 2.1 Is a block a struct? Is it a mod?

**Neither.** A block and its circuit together are closest to a **function that builds circuitry**: the block is the signature, the circuit is the body. Compare what each Rust item is:

| Rust item | What it is | Is a block like it? |
|---|---|---|
| `mod` | A namespace for organizing code. Exists once. No parameters. Can't be instantiated | **No.** You place two `CeAmp`s on a board, and each gets its own parts. We keep `mod`/`use` for what they mean in Rust: files and libraries |
| `struct` | A data type with named fields. Many instances. No body | **Partly:** a block has named ports, and many instances |
| `fn` | Parameters plus a body that runs each time it's called | **Partly:** a block's circuit runs once per placement and creates that placement's parts and nets. Internal nets are private, like local variables |

So a block gets its own keyword. Placing one uses Rust's **struct-literal syntax**, because that's Rust's only syntax for named arguments, and pins must be bound by name:

```rust
let amp = CeAmp { vcc: v12, gnd, input: guitar, output: to_adc };   // `gnd` is shorthand for `gnd: gnd`, as in Rust
```

Read it as "build a CeAmp here, with these connections". It is not "make a struct".

**Why the header and the circuit are two items** (v5 §0):
- Everyone else reads only the header: a parent placing the block, the block's own setups and `pub spec`s, the symbol. The circuit is read by the block itself and by its internal specs.
- It leaves room for a second implementation of the same interface, which is parked. VHDL does this with one entity and several architectures (`research/contract_references.md`).
- The circuit, the setups and the contract each name their block, the way an `impl` names its type in Rust.

The same idea goes by other names elsewhere:
- Spade calls it `entity` (function-like, placed with `inst`).
- atopile and Verilog call it `module`. In hardware languages "module" means an instantiable template, which is the opposite of Rust's `mod`.

**Alternative considered:** function syntax, `block CeAmp(vcc: Power<In>, …)` placed with `CeAmp(vcc: rail, …)`. Rejected for two reasons: Rust has no named function arguments, and ICs have dozens of ports.

### 2.2 What's built today

The front end reads the MVP subset (plan steps 1–4). Resolve elaborates only blocks and circuits so far (plan decision 4).

| Construct | Parsed | Elaborated |
|---|---|---|
| `[pub] block X { port: Type, … }` | yes | yes: the ports |
| `circuit X { net …; let …; }` | yes | yes, into the same design v0.1's block body gave |
| `setup S for X { key: value, … }`, keys a port, a field path or `temp` | yes | matched to its block by name |
| `env name: Type in range;`, `const NAME: Type = value;` | yes | next phase |
| `contract X { setup = S; let …; [pub] spec name: m within\|<=\|>= b; }` | yes | matched to its block by name |
| open ranges, strings, named call arguments, `m[s]`, named generic arguments at a placement | yes | a placement's generic arguments are reported as not supported yet |
| generics and trait bounds in a block header, `observe`, `emits` | no | — |
| `..Base`, `mode`, `event`, pairs, attribute arguments `name = value` | no | — |
| `rated`, the spec clauses (`with`, `for x in`, `on`, `in M`), the function form, `ensure` | no: `rated` and `ensure` are reserved | — |
| `trait`, `impl`, `fn`, `for` loops, `use`, `mod`, part records | no: reserved, or not an item | — |

Next come setups, `env`, `const` and contracts, elaborated in the MVP subset of [IR] §4.4, then the exporter, then the engine (plan §3).

---

## 3. Ports and signal types

### 3.1 What a port is

A **port** is a named connection point on a block's boundary, the thing the parent wires to. Ports are written in the block's header, as `name: Type` entries separated by commas:

```rust
pub block CeAmp { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }
```

A long header takes one port per line, with a trailing comma. A doc comment or an attribute attaches to one port:

```rust
pub block Ldo3v3 {
    /// 4.3–5.5 V from USB.
    vin: Power<In>,
    vout: Power<Out>,
    gnd: Ground,
}
```

The same port shows up in four places:

- **In the circuit**, it behaves exactly like a net: you bind part pins to it.
- **On the block's symbol**, it's a pin.
- **In a hierarchical schematic**, it's the sheet pin.
- **On the I/O page**, it's where setups and specs attach. A setup gives it a source or a load (`vcc: Supply { v: 12V ± 5% }`), and a spec measures it (`dc(output.v)`).

Electrically a port is just a node. Its **type** says what kind of signal it carries.

**Published values and events.** Later the header also lists what a parent may read or trigger, so contracts never reach inside a child (v5 §0):

```rust
pub block Stm32Adc {
    vdd: Power<In>, ain: Analog<In>, gnd: Ground,
    observe code: Integer,                                            // the ADC reading
    emits wake: Step { on: vdd.i, from: 5mA, to: 30mA, edge: 1us },   // a current step a parent may trigger
}
```

`observe` and `emits` are keywords only at the start of a header entry, so they stay usable as names. **Not in the MVP:** neither is parsed yet. Whether `emits` becomes `event` is open (v5 §4.1).

### 3.2 What a type like `Power` gives you

Electrically `Power` is a single wire, so why type it? Because the type is where five things come from:

1. **Checks that need no numbers.**
   - Two `Power<Out>` on one net: "two supplies shorted".
   - An IC's `Power<In>` pin on a net with no source: "unpowered".
   - A `Power` net wired into an `Analog<In>` signal pin: warning.
2. **Who owns which number.** A rail's voltage `v` is set by its **source**; its current `i` is drawn by its **sinks**. From that alone, the engine generates checks at every connection:
   - the source's guarantee ⊆ every sink's setup (the regulator's `pub spec` of 3.3 V ± 5% vs the sensor's `Supply { v: 3.3V ± 3% }`);
   - the sum of sink currents ≤ the source's `i_max`.

   Without the type, the tool can't know which side sets which number.
3. **The shape a setup gives the port.** The role decides which shape a port takes in a setup ([IR] §3.4):
   - `Power<In>` gets a `Supply { v, z }`: a source, with its voltage and source impedance;
   - `Analog<In>` gets a `Signal { v, z }`;
   - any `<Out>` port gets a `Load { r, c, i }`;
   - `Ground` gets nothing: it's the reference, and a setup key on it is an error.

   A field path on a port the setup gave no shape (`vout.i: 5mA..=50mA`) creates the role's shape. **MVP:** shapes are parsed as ordinary struct literals; they're checked when setups are elaborated.
4. **Drawing.** Power pins go on top of the symbol, ground at the bottom, inputs left, outputs right. Rails are drawn as power symbols. Types map to KiCad pin types.
5. **Context for the AI.** It knows what each pin *is*.

### 3.3 Where the types come from

From the **standard library**, written in the language itself. They're not built into the compiler, so you can read them and define your own. A sketch of the std definitions (v0.1's design; `signal` and `enum` are reserved words, *not decided in v5*):

```rust
// std/signals.spl (sketch)

/// Direction of a single conductor, from the point of view of the part that owns the pin.
enum Role { In, Out, InOut, OpenDrain, Passive }

/// A plain conductor with no meaning attached (passive part pins).
signal Pin;

/// The reference node. Always bound explicitly; there are no global nets.
signal Ground;

/// A supply rail. The source sets the voltage; sinks draw current.
signal Power<R: Role> {
    #[by(Source)]    v: Volt,        // sinks' setups give it, the source guarantees it
    #[by(Source)]    i_max: Amp,     // the source's current capability
    #[by(Source)]    z_src: Ohm,     // source impedance (for the loading check)
    #[by(Sink, sum)] i: Amp,         // drawn by the sinks, summed over the net
}

/// An analog signal.
signal Analog<R: Role> {
    #[by(Source)] v: Volt,
    #[by(Source)] z_src: Ohm,
    #[by(Sink)]   z_load: Ohm,
    #[by(Sink)]   c_load: Farad,
}

/// A logic signal. Thresholds come from the pins on the net.
signal Logic<R: Role> {
    #[by(Source)] voh: Volt,  #[by(Source)] vol: Volt,
    #[by(Sink)]   vih: Volt,  #[by(Sink)]   vil: Volt,
    #[by(Sink, sum)] c_in: Farad,
}
```

`Power<In>` is a sink and `Power<Out>` a source. **Inside** a block, its own ports are seen flipped: the block's `vcc: Power<In>` acts as the source for the parts in its circuit.

**You can define your own:** a `Reference` (a precision voltage with a noise spec), a `CurrentLoop` (4–20 mA), a `Bias`. A user-defined signal gets the same checks, setup shapes and drawing rules as the std ones.

### 3.4 Interfaces: bundles of signals

v0.1's design; `interface` is a reserved word, *not decided in v5*.

```rust
// std/interfaces.spl (sketch)
interface I2c<R: I2cRole> {
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

v0.1's design, *not decided in v5*. Neither array ports nor typed nets are parsed yet.

```rust
pub block Mux8 { ain: [Analog<In>; 8], out: Analog<Out>, vdd: Power<In>, gnd: Ground }
net bus: [Analog; 8];
```

Arrays bind element-wise; a single net broadcasts to every element (§7.4).

---

## 4. Instances and connections

Parts and nets live in the block's `circuit`, an item with the block's name.

### 4.1 Nets and pin binding

```rust
circuit CeAmp {
    net base;
    let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };
    let r2 = Resistor { a: base, b: gnd, value: 10k ± 1% };
    …
}
```

**Rules:**
- A circuit belongs to the block of its name. A circuit with no block is an error, and so is a second circuit for one block (second implementations are parked, v5 §0).
- A block with no circuit is only an interface. Declaring it is fine; placing it is an error, because model blocks aren't supported yet ([IR] §3.8).
- Nets are declared, and always named.
- **Every pin is bound exactly once**, inside its part's statement. Leaving a pin out is an error unless it has a default (`nc` for pins marked optional in the part).
- A block's ports are nets inside its circuit.
- **Merging two existing nets** is rare, and has its own statement: `net x = [a, b];`.
- There are no global nets. Ground is a port, bound explicitly.
- A circuit holds only `net` and `let`. A `setup =` or a `spec` there is reported ("`spec` doesn't belong in a `circuit`") and parsed anyway.

"What's on `base`?" is answered by the editor and the language server (hover and highlight), not by the syntax.

**MVP:** all of this is parsed and elaborated. Resolve reads the ports from the header and the body from the circuit, and builds the same design as v0.1 did from a block body (plan step 3).

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
let gain = GainStage<A = Mcp6001, GAIN = 20> { vin: sensor, vout: amp_out, vdd: vdda, gnd };
```

Ports are the placement's fields. Generic arguments go in `<>`, by name (§7.2). The I/O page shows the difference: ports are pins, generic arguments are value fields.

**MVP:** a placement with generic arguments is parsed; resolve reports the arguments as not supported yet, and still checks the rest of the placement.

### 4.4 Stable identity

Every instance and net has a path: `board.amp.r1`, `board.amp.base`, `prot.r_s[3]`. Paths are what the layout sidecar, knob names, AI edits and version-control diffs refer to. Renames go through one refactor command that updates every reference; a hand rename is detected by matching part type and connections.

---

## 5. Values

### 5.1 Units and literals

- **Quantities are typed by physical dimension:** `Volt`, `Ohm`, `Farad`, `Amp`, `Watt`, `Hertz`, … Dimensions are checked at compile time (`Volt * Amp == Watt`; `Ohm * Farad == Second`). An `env` declares its type: `Temperature`, `Duration`.
- **Literals:** `47k`, `3.3V`, `100nF`, `4k7`, `1uF` (or `1µF`), `10kΩ`, `-10°C`, `10y`, `2h`. A bare `47k` takes its unit from where it's used (`value:` of a `Resistor` expects `Ohm`).
- **Prefixes are case-sensitive:** `m` is milli, `M` is mega. `1Meg` (the SPICE habit) gets a fix-it. `1K` is kelvin, not kilo.
- **A unit must touch a number.** A value per unit is written `12.0 / 1mV`, not `12.0/mV`: `mV` alone is a name (v5 §1.5).
- **Strings:** `"…"` on one line. The only escapes are `\"` and `\\`. They're for attributes such as `#[outside(reason = "…")]`, whose `name = value` arguments aren't parsed yet.

### 5.2 Tolerances and ranges

- **Tolerance:** `47k ± 1%`, or ASCII `47k +/- 1%`. Also absolute: `3.3V ± 50mV`.
- **Ranges** are closed and use Rust's `..=`. Either end may be left out:

  | Written | Meaning |
  |---|---|
  | `4.5V..=6.5V` | from 4.5 V to 6.5 V |
  | `..=0.5Ω` | at most 0.5 Ω: no lower end |
  | `10kΩ..` | at least 10 kΩ: no upper end |

  All three are one range node whose ends are optional, as rustc's `ExprKind::Range` (plan step 2).
- **`a..` has no upper end only where none can follow:** before `,` `}` `)` `]` `;`, or the `>` that closes generic arguments (`parser/ok/open_ranges.spl`).
- **`a..b` is an error**, with the fix `..=` (`100..300` → `100..=300`). Rust's `a..b` leaves out `b`, and a spread always includes its ends. v0.1 kept `..` for loop ranges; loops aren't in v5 yet (§7.4).
- **Precedence:** `within` and comparisons bind loosest, then `..=` (and the open forms), then `±`, then arithmetic, then the postfixes `.f`, `(…)` and `[s]`. An unparenthesized `+ - * /` expression as an operand of `±` is an error, because `capacity / 2h ± 10%` has two plausible readings (grammar.md §4).
- **Shape rules** catch a spread where one number belongs:
  - a range or a tolerance can't be the left side of `within`, or either side of `<=` and `>=`;
  - a range's end can't carry a tolerance (`1V ± 1%..=2V`, `..=2V ± 1%`);
  - relations and ranges don't chain (`x < y < z`, `1..=2..=3`, `..=1..=2`).
- **A tolerance is its own type,** `Tol<Ohm>`, distinct from an exact `Ohm`. That's how a value can say "each part built from this gets its own independent spread" (§7.1).

### 5.3 Where a range appears decides what it means (⊆ vs ⊇)

The same `4.5V..=6.5V` can mean "must stay inside" or "can be anywhere inside". Where it's written always tells you which:

| Written | Meaning | Quantifier |
|---|---|---|
| `value: 47k ± 1%`, `beta: 100..=300` (a part field in a circuit) | **Spread:** the real part can be anywhere in here | for all (statistical) |
| `vcc: Supply { v: 12V ± 5% }`, `temp: ambient` (a setup field) | **Condition:** the world can be anywhere in here | for all (range) |
| `spec bias: dc(output.v) within 4.5V..=6.5V` | **Limit:** the result must stay inside | ⊆ |
| `rated vin.v within -0.3V..=6.5V` | **Absolute limit:** inside on every run of every setup | ⊆ |
| `with vin.v = 4.3V` (a spec clause) | **Pin:** one point, for this spec only | — |
| `value: ?` | **Search:** the solver picks a value | exists |

**`within` is the only limit operator.** `in` isn't an operator any more. It binds a name to a range (`env x: T in r`, `for f in r`) or names a mode (`in Run`), so it never means "must stay inside".

On top of that, a setup describes only what the world does to the block: a source on each input, a load on each output (§3.2). A range on something the block drives is what a spec checks, so it's written as a spec (§8.2).

<a name="knobs-from-language"></a>
### 5.4 Knobs come from the language

| Written | Knob | Kind |
|---|---|---|
| `env ambient: Temperature in -10°C..=60°C;`, read by a setup's `temp: ambient` | `ambient` (shared by every block) | range |
| `temp: -40°C..=125°C` in a setup | `temp` | range |
| `vcc: Supply { v: 12V ± 5% }` in a setup | `vcc.v` | range |
| `mode Run { vout.i: 5mA..=50mA }` in a setup | `vout.i@Run` | range, in that mode |
| a field a derived setup writes (`vout.i` in `LoadStep`) | `LoadStep::vout.i` | range, its own |
| `event drop: Open { …, lasts: 0us..=10ms }` | `Unplug::drop.lasts` | range, in that setup only |
| `(inp, inn): Pair { dm: …, cm: …, z: 350Ω ± 0.1% }` | `(inp, inn).dm`, `(inp, inn).cm`, and `inp.z`, `inn.z`: one per leg | range |
| the modes of a setup | each spec runs once per mode | discrete |
| `value: 47k ± 1%`, or the tolerance from a pinned `part:` | `amp.r1.value` | statistical |
| `value: ?` | `amp.r1.value` | solver |
| `with vin.v = 4.3V`, a `const` | none: a point | — |

The printed names follow [IR] §3.5. An `env` that no used setup reads makes no knob, and gets a warning.

**Sharing** (v5 rule 1.3.6, [IR] §3.5):
- part knobs are always shared, across every setup and every spec;
- an inherited range field is the same knob as its base's;
- a field a derived setup writes is its own knob;
- in one multi-setup spec, range knobs are shared unless a setup overrides them, and the mode is shared across its setups.

**Links** connect part values to shared knobs. They come from part records (§6), or can be written directly: `tc: ±100ppm/K` links to the temperature, `aging: -20%` links to `life`. *Not decided in v5:* how a link finds its knob, since the temperature is now a setup's `temp` field, and `env life` is read by no setup key.

**Lots:** `let net_r = lot(RN_0603_4x10k)` makes parts that share one lot knob plus small per-part knobs (resistor networks, same reel). v0.1's design, *not decided in v5*.

**MVP:** knobs come only from part fields until setups and `env`s are elaborated. `ce_amp.spl` then gives 8 knobs: its 6 part knobs, `vcc.v` and `ambient` ([IR] §4.2).

### 5.5 Exact values vs design values

- **Exact:** known at build time, e.g. `8`, `Clamp::Tvs`, `4.7k` without tolerance, a `const`, a generic argument.
- **Design:** carries knobs, `?`, or simulated results.

Structure (`if`, `for`, `match`, array sizes) may depend only on exact values. `if gain > 10 { … }` on a toleranced `gain` is a compile error. This is Spade's type-level vs runtime split.

### 5.6 A committed value plus how it was derived

v0.1's design, *not decided in v5*; `from` isn't parsed yet.

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

This section is v0.1's design. v5 changes only its syntax: no `port` statements, `within` for a limit, and no `pub` on records (§2).

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
| `VoltageSource`, `CurrentSource` | `p`, `n` | a setup's shapes are lowered to these ([IR] §3.9) |

**Pin names encode polarity:** non-polarized parts have `a`/`b`, polarized parts have `p`/`n`.

Each kind maps to a simulator device, so it simulates as soon as it has a value. Each is also a generic, pickable part (§6.7).

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
family RC0603FR: Resistor {
    values: E96 in 1Ω..=10MΩ,                 // `in` as in `env`: the series, over this range
    mpn: "RC0603FR-07{code}L",               // {code}: 47k → "47K"
    package: R0603,

    #[from(page = 2, table = "Electrical characteristics")]
    tolerance: ± 1%,                          // tested limit
    #[from(page = 2)]
    tcr: ±100ppm/K,                           // statistical coefficient × temp (engine slices over temp)

    operating {
        power <= 0.1W derated linearly to 0W between 70°C..=155°C;
        voltage <= 75V;
        temp within -55°C..=155°C;
    }
    #[from(page = 4, table = "Endurance")]
    drift: ±1% after 1000h at (power: rated, temp: 70°C);   // stress-test data, not a lifetime figure
}
```

```rust
// parts/murata/grm188r71e104ka01.spl
/// 100 nF, 25 V, X7R, 0603 MLCC.
#[source(datasheet = "…", extracted = "ai", reviewed = true, reviewer = "ido")]
part GRM188R71E104KA01: Capacitor {
    value: 100nF ± 10%,
    dielectric: X7R,
    package: C0603,
    operating { voltage <= 25V; temp within -55°C..=125°C; }
    /// Capacitance vs DC bias, from the manufacturer's characterization data.
    dc_bias: table { 0V: 0%, 5V: -8%, 10V: -20%, 16V: -35%, 25V: -52% },   // illustrative
    tc: X7R,                                  // ±15% over −55…125 °C, as an envelope
}
```

Pieces of this sketch:

- **`family`** covers a whole series in one record, because nobody wants a file per resistor value.
- **`#[source]`** and **`#[from]`** hold provenance for the record and for each field.
- **`operating { }`** and **`absolute_max { }`** hold the part's own conditions. They become automatic checks (§8.7). A block says the same two things with its default setup and `rated` (§6.8).
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

The ports are written as in a block header (§3.1). The rest of the record is v0.1's sketch.

```rust
// parts/ti/tps54302.spl (abridged; values from the TPS54302 datasheet, SLVSDG6C)
#[mpn = "TPS54302DDCR"] #[footprint = "SOT-23-6"]
#[source(datasheet = "https://www.ti.com/lit/ds/symlink/tps54302.pdf", extracted = "ai", reviewed = false)]
part Tps54302 {
    vin: Power<In>, gnd: Ground, sw: Power<Out>, boot: Pin, fb: Analog<In>, en: Logic<In>,
    pins { gnd: 1, sw: 2, vin: 3, fb: 4, en: 5, boot: 6 }

    operating { vin.v within 4.5V..=28V; temp within -40°C..=125°C; }
    absolute_max { /* from the datasheet's Absolute Maximum Ratings table */ }

    /// Electrical characteristics: the part's own spreads (statistical knobs).
    v_ref: 0.581V..=0.611V,      // typ 0.596 V
    f_sw: 290kHz..=510kHz,       // typ 400 kHz

    model: spice("models/tps54302_trans.lib", subckt: "TPS54302_TRANS", fidelity: typical_only),
}
```

The datasheet maps onto the part directly. It is the part's contract, and v5 gives a block the same three layers:

| Datasheet section | In a part record | In a block (v5) | Becomes |
|---|---|---|---|
| Absolute Maximum Ratings | `absolute_max { }` | `rated …;` in its contract | worst-case survival checks, on every run of every setup |
| Recommended Operating Conditions | `operating { }` | its default setup | worst-case "it works" checks |
| Electrical Characteristics | the part's spreads | spreads, and `pub spec`s | statistical knobs, and what parents rely on |

*Not decided in v5:* whether an IC is a part record, or a block with no circuit. v5's `Stm32Adc` (§3.1) is the second kind, a model block, and placing one is an error until model blocks are designed ([IR] §3.8).

---

## 7. Generics and loops (build time)

### 7.1 Generic parameters

A block's parameters are generics, in `<>` after its name. v0.1's `param` statement is gone: a parameter is a generic (plan step 1). There are two kinds:

- **a type parameter** picks a part or a block for a slot: `A: OpAmp = Mcp6001`, with a bound and a default;
- **a const parameter** is an exact number: `const GAIN: f64 = 10.09`.

```rust
/// Non-inverting amplifier for a ground-referenced sensor.
pub block GainStage<A: OpAmp = Mcp6001, const GAIN: f64 = 10.09> {
    vin: Analog<In>, vout: Analog<Out>, vdd: Power<In>, gnd: Ground,
}

circuit GainStage {
    net fb;
    let u1  = A        { inp: vin, inn: fb, out: vout, vdd, gnd };
    let r_f = Resistor { a: vout, b: fb,  value: ((GAIN - 1) * 10k) ± 1% };
    let r_g = Resistor { a: fb,   b: gnd, value: 10k ± 1% };
}
```

- `A` is placed like any part: `let u1 = A { … }`.
- Both kinds are exact and decided at build time (P6). Editing `A` swaps a part and its symbol; editing `GAIN` changes a number.
- A const default in a header ends at `>`, as a generic argument does (§7.2).

*Not decided in v5:* a toleranced parameter. v0.1 wrote `param r: Tol<Ohm> = 1k ± 1%`, and each part built from `r` got its own knob. A generic const is exact, and a project `const` with a spread is an error ([IR] §3.2), so this needs its own form.

**Not in the MVP:** generics in a block header aren't parsed yet (plan step 3).

### 7.2 Named generic arguments at a placement

```rust
let amp = GainStage<A = Mcp6001> { vin: sensor, vout: amp_out, vdd: vdda, gnd };
let hi  = GainStage<A = Tlv9001, GAIN = 20> { vin: sensor, vout: hi_out, vdd: vdda, gnd };
```

- Arguments are **named only**. `< name =` starts them. That's a three-token look-ahead, and it's unambiguous because `=` is never an operator. Everywhere else `<` is less-than ([IR] G4).
- Each value ends at `>`, so a comparison inside one needs parentheses: `G<N = (a > b)> {}`.
- A parameter left out takes its default (`GAIN` in the first line).
- *Precedent:* Rust needs the turbofish `::<>` in expressions for exactly this reason. The named form avoids it, because `=` marks the argument.

**MVP:** parsed; resolve reports the arguments as not supported yet.

### 7.3 Checked instantiations, and traits

```rust
#[check(A = [Mcp6001, Tlv9001])]
contract GainStage { … }
```

The contract is checked with each listed op-amp. Which other instantiations are checked (the defaults, every one a placement uses) isn't stated in v5 (`research/contract_v4_review_consistency.md` C28).

**Traits** name families of blocks: the `OpAmp` in `A: OpAmp`. v5 lists `trait T { … }` and `impl T for X {}` as items that bring std setups and specs to a block. Its example also writes the membership in the header: `pub block Ldo3v3: Regulator { … }`. Which spelling stays is open (consistency review C29).

**Not in the MVP:** `trait` and `impl` are reserved; `#[check(A = […])]` needs attribute arguments `name = value`, which aren't parsed yet.

### 7.4 Loops and arrays

v0.1's design, *not decided in v5*. `for` in a circuit is reported as not supported yet, and array ports and `where` aren't parsed.

```rust
pub block InputProtect<const N: usize = 8>
where N >= 1 && N <= 32 else "InputProtect supports 1–32 channels"      // from Spade
{
    raw: [Analog<In>; N], safe: [Analog<Out>; N], vdd: Power<In>, gnd: Ground,
}

circuit InputProtect {
    /// 1 kΩ limits clamp current to ≈ 8 mA for a ±12 V fault.
    for i in 0..N {
        let r_s[i]  = Resistor  { a: raw[i], b: safe[i], value: 1k ± 1% };
        let c_f[i]  = Capacitor { a: safe[i], b: gnd, value: 10nF ± 10%, dielectric: C0G };
        let d_hi[i] = Schottky  { a: safe[i], k: vdd, part: nexperia::BAT54 };
        let d_lo[i] = Schottky  { a: gnd, k: safe[i], part: nexperia::BAT54 };
    }
}
```

- `let r_s[i] = …` gives each instance a stable path: `prot.r_s[3]`.
- Each resistor gets its **own** knob, because each part's spread is its own.
- Editing a value changes a number. Editing `N` regenerates the sheet.
- The loop's `0..N` is Rust's half-open range, which is an error everywhere else today (§5.2). How a loop spells its range is open.

**The drawable form.** Array instances need no loop, so they stay editable as a drawing:

```rust
let r_s[0..N] = Resistor { a: raw, b: safe, value: 1k ± 1% };        // element-wise binding
let c_f[0..N] = Capacitor { a: safe, b: gnd, value: 10nF ± 10% };    // gnd broadcast to every element
```

These render as arrayed symbols labeled `×N`.

---

## 8. Contracts

### 8.1 The `contract` item

v5 splits v0.1's contract in two:
- **setups** (§8.2) are items of their own, outside the contract: the environments a block is checked in;
- **the contract** holds the promises: its default setup, `rated` limits, measures (§8.4) and specs (§8.3).

```rust
contract Ldo3v3 {
    setup = Operating;                                 // the default setup
    let out = dc(vout.v);                              // a measure
    spec output: out within 3.3V ± 2%;                 // a spec
    /// Parents may rely on it.
    pub spec quiescent: dc(vin.i + vout.i) <= 50uA;    // a published spec
}
```

- It lives in the same file as its block, and names it.
- It can see the block's circuit (nets, parts). A parent sees only the block's header (ports, and later `observe`, `emits` and modes) and its `pub spec`s.
- The block's header, its setups and its contract are together the text form of the I/O page. Every column of the spec table has exactly one place in the code, and a canonical formatter prints it, which is what makes editing the table and editing the text interchangeable.

**`setup` and `pub` start both items and statements** (`setup S for X` vs `setup = S;`, `pub block` vs `pub spec`). The parser reads an item only when the next token is an item's own: `setup` before a name, `pub` before an item keyword. Anything else is a statement, so a typo inside a contract is that statement's error and doesn't end the body. rustc reads `union` as an item only before a name for the same reason (plan step 4; [IR] G1).

**The project is the top block.** Its operating envelope is `env`s, which every setup can read, and its fixed values are `const`s:

```rust
env ambient: Temperature in -10°C..=60°C;
env life:    Duration    in 0y..=10y;
const confidence: Confidence = sigma(3);
```

**MVP:** `setup = S;`, `let` and `[pub] spec name: m within|<=|>= b;` are parsed. `rated`, the clauses, the function form and `ensure` aren't parsed yet. Resolve matches each contract to its block by name; what it holds is elaborated in the next phase. A project is one file for now, so how several files share an `env` is open ([IR] D11).

### 8.2 Setups

A **setup** describes the world around a block: what drives each input, what loads each output, the temperature. It's an item: named, for one block, and reusable. It replaces v0.1's `assume` statements and its benches (§8.5).

```rust
setup Operating for Ldo3v3 {
    vin: Supply { v: 4.3V..=5.5V, z: ..=0.5Ω },
    vout: Load { c: ..=20uF },
    vout.i: 5mA..=50mA,
    temp: -40°C..=125°C,
}
```

#### Keys and values

Entries are `key: value`, separated by commas. A key is:
- **a port**, given a whole shape: `vin: Supply { v: 4.3V..=5.5V, z: ..=0.5Ω }`;
- **a field path** into a port's shape, which sets one field: `vout.i: 5mA..=50mA`, `vin.wave: Step { … }`;
- **`temp`**: a range, a point, or an `env` name (`temp: ambient`);
- **`window`**: how long a transient runs, `window: ..=300us`.

A value is a point (`0V`), a tolerance (`12V ± 5%`), a range or an open range, an `env` name, or a shape: `Supply { v, z }`, `Signal { v, z }`, `Load { r, c, i }` ([IR] §4.4).

`temp` and `window` are ordinary names, not keywords. A broken entry is skipped to the next `,`, and the entries after it are kept.

#### Rules

From v5 §1.3:

1. **Every port except ground appears in the default setup.** Signal and supply voltages have **no ideal default**, so they must be written. Impedances left unwritten are ideal (a 0 Ω source, an open load), and the formatter writes them out.
2. **Fields merge one by one.** A path (`vin.wave: …`) changes one field; a whole shape (`vin: Signal { … }`) replaces the port's fields.
3. **Inside a setup, a path names a field of the port's shape:** `vin.v` there is the source's setting. Inside specs and `rated`, `vin.v` is the measured pin voltage.

**MVP:** setups with port, path and `temp` keys are parsed. Resolve matches each setup to its block by name, and a setup's name is given once per block (`Operating` exists once *per block*). Keys, values and shapes are checked in the next phase.

#### The default setup

`setup = Operating;` in the contract names it.
- Specs use it unless they say `on`.
- Parents are checked against it: wherever the block is placed, the parent's circuit must keep each port inside it (§8.8).
- It's the envelope every other setup of the block stays inside (rule 6 below).

A block that sets its own range narrows the project's. A library LDO rated −40…125 °C writes `temp: -40°C..=125°C`, and wherever it's placed, the parent's temperature must fit inside it. v0.1 also let a block that says nothing about temperature inherit the project's range; v5 doesn't say what an unwritten `temp` means.

#### Roles decide what a setup may set

A setup sets only what the world does to the block. A `Load` has no `v`, because the block drives its output's voltage. A sketch of the diagnostic (setups are checked in the next phase):

```
error[E-role]: a setup sets a quantity this block drives
 --> ce_amp.spl:24:5
   |
24 |     output.v: 4.5V..=6.5V,
   |     ^^^^^^^^ `output: Analog<Out>`: this block drives `output.v`; a setup gives it a `Load { r, c, i }`
   = help: a range on a quantity you drive belongs in a spec: `spec output_range: dc(output.v) within 4.5V..=6.5V;`
```

#### Derived setups

A derived setup reuses a base with Rust's struct update syntax:

```rust
setup LoadStep for Ldo3v3 {
    vout.i: Step { from: 5mA, to: 30mA, edge: 1us, at: 50us },
    window: ..=300us,
    ..Operating
}
```

4. **`..Base` comes last, and the `,` before it is required**, as in Rust (Reference §8.2.9, functional update syntax). Without it, `vout.i: 500mA` followed by `..Operating` on the next line would read as the range `500mA..Operating` (plan step 4).
5. **Order when setups combine:** the base's fields first, then the mode's, then the derived setup's own. The later one wins, field by field (v5 rule 1.3.3). So `LoadStep`'s step replaces the `vout.i` of every mode it inherits. Modelica's modifiers merge the same way (Modelica spec §7.2).
6. **A derived setup stays inside the default setup** in every mode a spec uses it in, unless it is `#[fault]` or `#[outside]` (v5 rule 1.3.4). [IR] §3.4 proposes that stimulus fields (`wave`, `ac`, steps, events, `window`) are left out of this check, because they say how a test is run, not where the product lives. That's still open. A step in a load current is a level: its `from` and `to` must lie inside the mode's range.

#### Modes

Modes are named variants inside a setup:

```rust
setup Operating for Ldo3v3 {
    vin:  Supply { v: 4.3V..=5.5V, z: ..=0.5Ω },
    vout: Load   { c: ..=20uF },
    temp: -40°C..=125°C,
    mode Run   { vout.i: 5mA..=50mA }
    mode Sleep { vout.i: 0.1mA..=1mA }
}
```

- Every spec is checked in each mode. `in Run` narrows a spec to one (§8.3).
- A derived setup inherits its base's modes: `on LoadStep in Run` is valid.
- A `mode … { }` entry may leave out its comma, like a Rust match arm with a block body ([IR] G11).
- `mode` is a keyword only at the start of a setup entry, so a part can still have a `mode:` field.
- How a board's modes select a child's state (an MCU's Run or Sleep current) isn't designed yet (v5 §4.4).

#### Events and faults

An event is a shape with named fields, declared with a name so specs can refer to it:

```rust
#[fault]
setup Unplug for SensorBoard {
    event drop: Open { port: usb, at: 1ms, lasts: 0us..=10ms },
    window: ..=(drop.end + 1s),
    ..Operating
}

#[fault]
setup Reversed for SensorBoard { usb.polarity: reversed, ..Operating }
```

- **Shapes:** `Step { from, to, edge, at }`, `Sweep { from, to, step }`, `Open { port, at, lasts }`, and `Trigger { of, at }`, which fires a child's `emits`: `event wake: Trigger { of: mcu.wake, at: 1ms }`.
- **A step's or a sweep's levels are named fields**, like every other field, so a struct literal has no positional parts and needs no `->` (plan step 2, replacing v5's `5mA -> 30mA`).
- **`window: ..=t`** is how long to simulate. It may depend on an event (`drop.end`).
- **`#[fault]`** setups may leave the envelope. No spec needs to run on one for it to matter: every `rated` limit is checked on it (§8.3).
- `event` is a keyword only at the start of a setup entry.

#### One-offs

`#[outside(reason = "…")]` marks a setup that's outside the envelope on purpose: characterization, like a datasheet row. Its specs are tagged, and are never `pub`.

```rust
#[outside(reason = "datasheet dropout row: defined at the rated 500 mA")]
setup DropoutRow for Ldo3v3 {
    vout.i: 500mA,
    vin.v: Sweep { from: 4.5V, to: 3.0V, step: 10mV },
    ..Operating
}
```

`#[outside]` might get a clearer name (v5 §4.3).

#### Pairs

A differential input is written as a pair of ports:

```rust
setup Operating for InAmp {
    (inp, inn): Pair { dm: -10mV..=10mV, cm: 2.5V ± 5%, z: 350Ω ± 0.1% },   // each leg has its own z knob
    out:  Load   { r: 10kΩ.. },
    vdd:  Supply { v: 5V ± 5% },
    temp: -40°C..=85°C,
}
setup Diff   for InAmp { (inp, inn).dm.ac: 1V, ..Operating }
setup Common for InAmp { (inp, inn).cm.ac: 1V, ..Operating }
```

A pair's two legs get separate tolerance knobs (v5 rule 1.3.7). With one knob both legs would always be equal, and that would hide the imbalance that sets a bridge's CMRR ([IR] §3.7).

#### Local conditions

An instance that really sees a different environment is mapped at the placement. v0.1's design, *not decided in v5*:

```rust
#[env(ambient = ambient + 15K, reason = "next to the buck inductor")]
let amp = CeAmp { … };
```

**Not in the MVP:** `..Base`, `mode`, `event`, pairs and attribute arguments `name = value` aren't parsed yet (plan step 4). A `Step`, a `Sweep` or a `window` parses as an ordinary entry, but [IR] §4.4 leaves them out of the first elaboration.

### 8.3 Specs

#### The one-liner

```
spec name: <measure> <limit> [with <pins>] [for <axis> in <range>] [on <setup>] [in <mode>];
```

```rust
contract Ldo3v3 {
    setup = Operating;
    rated vin.v within -0.3V..=6.5V;

    spec output:    dc(vout.v)                            within 3.3V ± 2%;
    spec psrr:      ac(vout.v / vin.v).at(100kHz).db()    <= -36dB   with vin.v = 4.3V;
    spec dip:       tran(vout.v).min()                    >= 3.25V   on LoadStep in Run;
    spec quiescent: dc(vin.i + vout.i)                    <= 50uA    in Sleep;     // currents are positive into the block
    spec dropout:   dc(vin.v - vout.v).first(vout.v < 3.267V)   <= 250mV   on DropoutRow;   // characterization

    pub spec output_z: vout.z(f)                          <= 2Ω      for f in 10Hz..=1MHz  in Run;
}
```

- **Limits:** `within <range>` (a range or `x ± tol`), `<=`, `>=`. An open range is a one-sided limit and is written as one: `within ..=6.5V` is an error with the fix `<= 6.5V`, and `<`, `>` get the fix `<=`, `>=` (contracts_plan.md §2.7, step 4).
- **A doc comment** above a spec is its rationale, shown in the spec table.

#### Clauses

| Clause | Written | Meaning |
|---|---|---|
| Pin | `with vin.v = 4.3V` | this knob is fixed at one value, for this spec only (v0.1's `where temp == 25°C`) |
| Swept axis | `for f in 10Hz..=1MHz` | swept inside the measurement; the limit holds at every point (v0.1's `.band(…)`) |
| Setup | `on LoadStep` | use this setup instead of the default |
| Mode | `in Run` | only in this mode |

- They come in this order. Out of order is an error with a reorder fix ([IR] G14).
- `with` and `on` are keywords only in this position, so they stay usable as names.
- A `for` variable is used in the measure before it's declared (`vout.z(f) … for f in …`). Resolve collects it first, as it collects names before bodies (P4). It may not shadow a port or a net.
- `in` after a relation always names a mode. `for f in 10Hz..=1MHz in Run` reads the range up to the second `in`, with no look-ahead. The formatter keeps the clause order, and `--explain` spells the mode out ("in mode Run").

#### The function form

For multi-step and multi-setup specs:

```rust
contract InAmp {
    setup = Operating;
    spec cmrr(dm: Diff, cm: Common) {
        let a_dm = ac(out.v)[dm].at(60Hz).mag();
        let a_cm = ac(out.v)[cm].at(60Hz).mag();
        ensure (a_dm / a_cm).db() >= 100dB;
    }
}
```

- The parameters are setups. `m[s]` is measure `m` in setup `s`.
- `ensure` states the final condition.
- The one-liner is shorthand for this form, with the default setup and a single `ensure`. Both forms take the same clauses.
- **Every setup in one spec is the same board at the same moment** (v5 rule 1.3.6): part tolerances are always shared, range knobs are shared unless a setup overrides them, and the mode is shared across them.
- `spec cmrr(` starts like the missing-name mistake `spec dc(out.v) within …;`, so the parser looks past the `)` for a `{` ([IR] G3).

A two-point calibration reads the same way:

```rust
spec sensitivity(zero: AtZero, full: AtFull) {
    let per_mv = (mcu.code[full] - mcu.code[zero]) / 100mV;
    ensure per_mv within (12.0 / 1mV) ± 7%;
}
```

Whether a bare `mcu.code[full]` means `dc(mcu.code)[full]`, or the analysis must be written, is open ([IR] §6 Q6).

#### `pub spec` and internal specs

- **A `pub spec`** is one parents may rely on. It may name only the block's own ports and observables (v5 §1.4).
- **An internal spec** names an internal net (`base.v`), a part, or a child's value (`mcu.code`). It's allowed and checked normally, but never published and never used for composition.
- **The rule, as resolve will apply it** ([IR] §3.3): a spec is internal when its measure reaches the circuit, through a probe or through a `let` it uses. `pub` on an internal spec is an error:

  ```
  error[E-contract]: `recovers` can't be `pub`
    = note: it measures `v3v3`, a net of `circuit SensorBoard`; a parent can't rely on it
  ```

- **A setup doesn't make a spec internal.** A mode's name is part of the interface, even when the mode reaches into the circuit.
- **Which setups a `pub spec` may use** is open. [IR] §3.3 proposes the default setup (any mode, any `with` pin) and `#[fault]` setups only. An `#[outside]` setup never (v5 §0).
- Only a block or a spec can be `pub` (§2).

```rust
contract GainStage {
    setup = Operating;
    let h = ac(vout.v / vin.v);

    spec gain:      h.at(1kHz).mag()          within GAIN ± 3%;
    spec bandwidth: h.f_high(-3dB, ref: dc)   >= 20kHz;
    spec settling:  tran(vout.v).settle(1%)   <= 50us     on InputStep;
    spec loop_ok:   dc(fb.v - vin.v)          within 0V ± 1mV;            // internal: the loop is closed

    pub spec input_z:  vin.z(1kHz)            >= 400kΩ;
    pub spec output_z: vout.z(f)              <= 100Ω     for f in 10Hz..=100kHz;
    pub spec supply:   dc(vdd.i)              <= 150uA;
}
```

#### `rated`

```rust
rated vin.v within -0.3V..=6.5V;
```

- An absolute maximum on a pin quantity (v5 §1.4). It's checked on every run of every setup, faults included, and on setups no spec uses. Always worst case: an absolute maximum isn't a yield.
- It reaches down the hierarchy: a child's `rated` lines are monitored in its parent's runs, through the port bindings ([IR] §5).
- v5's sensor board shows why. No spec runs on `Reversed`, but a reversed connector puts −1.53 V on the LDO's input, below its −0.3 V rating. So the board fails until a series Schottky diode is added.

#### Attributes and confidence

- `#[confidence(worst_case | sigma(3) | yield(99.9%) | nominal)]`
- `#[warn]` (a warning, not an error)
- `#[expect(fail, reason = "…")]` (a known failure, tracked)
- **Default confidence:** `sigma(3)` for your specs (one-sided 3σ ≈ 99.87%), `worst_case` for automatic checks. v5 sets the project's default with `const confidence: Confidence = sigma(3);`. The engine would read that `const` by its name, which is open ([IR] D10).

**MVP:** the one-liner without clauses is parsed, `pub` included, with its doc comments and attributes. The clauses, the function form, `ensure` and `rated` aren't parsed yet. The internal and `pub` check comes with contract elaboration, which [IR] §4.4 puts in the MVP.

### 8.4 Measures and probes

**A measure is a value.** `let vc = dc(output.v);` has type `Volt`. So measures mix with hand formulas without double-counting shared knobs:

```rust
let headroom = dc(vcc.v) - dc(output.v);      // shares the vcc.v knob correctly
spec top_room: headroom >= 2V;
```

**How the engine keeps that promise** (decision D-F, `engine_plan.md` §10): in the MVP a derived measure is **evaluated per run**. `headroom` takes `vcc.v` and `output.v` from the same simulated board, so one knob can't be counted twice, and over the corners the answer is exact (5.1951 V on the CE amp). Affine forms in named knobs (engine v3 §3.1) return when results must be combined without re-simulating: hierarchy, calibration, error budgets and datasheet arithmetic.

**Probes are field accessors:**

| Probe | Meaning |
|---|---|
| `net.v`, `port.v` | voltage to ground |
| `a.v - b.v` | differential |
| `port.i` | current **into** the block at that pin, positive in |
| `port.z(f)` | impedance looking into the pin |
| `q1.c.i` | current into a part's pin |
| `r1.power` | power dissipated in a part |
| `mcu.code` | a child's `observe` value (internal: it names a placement) |

A `Load`'s `i` is the current the load draws (v5 §1.4).

**Analyses:** `dc(…)`, `ac(…)`, `tran(…)`, `noise(…)`, later `pss(…)`.

**Measurement methods are typed by result,** so the editor offers the right ones after a `.`. From v5 §1.5:
- **Reduce to one number:** `.at(f)`, `.mag()`, `.db()`, `.f_low(-3dB)`, `.f_high(-3dB, ref: dc)`, `.min()`, `.max()`, `.settle(1%)`, `.crossing(rising v)`, `.deviation()`, `.span(for x)`.
- **Time windows:** `.during(ev)`, `.after(ev.end)`.
- **From v0.1:** `.phase()`, `.bandwidth()`, `.phase_margin()`, `.peak()`, `.pp()`, `.rms()`, `.overshoot()`, `.thd(f)`.

The exact spelling of every method is still to settle (v5 §4.6). Call arguments can be named (`ref: dc`), and resolve reads `ref:` against a small set of references, not as a value ([IR] G9).

**Which source `ac(…)` excites:** an explicit `.ac` in the setup wins; otherwise the port in the ratio's denominator; otherwise it's an error, "which source is excited?" ([IR] §5).

**Smooth measures are preferred.** Non-smooth ones (settling time) are flagged, and rewritten to a smooth equivalent where one exists (a windowed maximum). A missing crossing is never treated as a pass. A crossing shown to lie **beyond** the searched band (e.g. an f_low below its lowest frequency) is a one-sided bound, not a missing crossing: it can decide the passing side of a spec, never the failing side (`research/engine_synthesis.md` C9).

**MVP:** named arguments and `m[s]` are parsed. The first elaboration takes `dc`, `ac`, `.at`, `.mag`, `.db`, `.f_low` and `.f_high` ([IR] §4.4).

<a name="benches"></a>
### 8.5 Benches are setups

v0.1 had three kinds of bench. v5 writes each as a setup, and `bench` is no longer a reserved word:

| v0.1 | v5 |
|---|---|
| **Default bench:** derived from the assumptions, never written | **The default setup:** written out, and named by `setup = Operating;` |
| **Form bench:** `bench loud { …, ..default }`, edited as a form | **A derived setup:** `setup Loud for CeAmp { …, ..Operating }`, edited as a form on the I/O page |
| **Sheet bench:** a separate schematic that places the design, like a Cadence ADE test | *not decided in v5* |

v0.1's loud-input bench, in v5:

```rust
setup Loud for CeAmp { input.wave: Sine { amp: 100mV, freq: 1kHz }, window: ..=20ms, ..Operating }
```

```rust
spec clip: tran(output.v).thd(1kHz) <= 1% on Loud;
```

- Every setup is checked to stay inside the default setup, unless it's `#[fault]` or `#[outside]` (§8.2, rule 6).
- `Sine` is v0.1's signal family. v5 names only `Step` and `Sweep` as waves; `Sine`, `Band`, `Pulse` and `Logic` are *not decided in v5*.
- A fixture with parts of its own (an electronic load, a probe network) has no v5 form yet. VHDL writes one as an entity with no ports whose architecture places the design (`research/contract_references.md`).

### 8.6 "For all" needs no syntax

Every range knob is already universally quantified, so "for all load currents" or "for all inputs up to 100 mV" is automatic.

- The axis swept inside one measurement is the `for f in 10Hz..=1MHz` clause.
- A transient's span is the setup's `window`, and `.during(ev)` and `.after(ev.end)` narrow it to an event.

`for` is a keyword with two jobs today: `setup S for X`, and the clause `for f in r`. v0.1's build-time loops (§7.4) would be a third. v0.1 used one to write a spec per channel. *Not decided in v5*:

```rust
for i in 0..N {
    spec gain[i]: ac(safe[i].v / raw[i].v).at(1kHz).mag() within 1 ± 1%;
}
```

### 8.7 Automatic checks

Every automatic check is a **condition of a part, an interface or a child block**, discharged by the circuit around it:

| Source | Examples | Default |
|---|---|---|
| `absolute_max { }` in part records | every IC pin within abs-max, at every corner and during start-up | error |
| `rated` in a block's contract | every rated pin quantity, on every run of every setup, faults included | error |
| `operating { }` in part records | supply in range, temperature in range | error |
| Polarized kinds | electrolytic/tantalum reverse voltage | error |
| Signal types | logic levels (VOH ≥ VIH, VOL ≤ VIL, with margin), rail source capability, one source per rail | error |
| Interface rules | I2C pull-ups and rise time (from UM10204), unique addresses | error |
| Derating policy | capacitor voltage, resistor power, semiconductor power and temperature | warning |
| Thermal | junction temperature at worst corner (electrothermal fixed point) | warning |
| Connections | upstream `pub spec` ⊆ downstream default setup, at every block connection | error |

**Levels and waivers follow Rust's lint model:**

```rust
#[allow(derating::capacitor::voltage, reason = "X7R at 60% of rating is fine for this life")]
let c_out = Capacitor { … };
```

There's also a project-wide `[checks]` table (like Cargo's `[lints]`) to choose the derating policy (commercial, NASA, ECSS) and set levels.

### 8.8 Composition

At every connection between blocks:

- **The check:** the upstream block's `pub spec` ⊆ the downstream block's default setup. Example: the buck publishes `dc(vout.v) within 3.3V ± 5%`, the sensor node's `Operating` has `v3v3: Supply { v: 3.3V ± 3% }`, and the check fails, with both lines shown (§11.2).
- **Only `pub spec`s take part.** Internal specs are part of the block's own check and never enter composition ([IR] §5).
- **Lifting:** a child's setup field on a port it shares with its parent shows greyed out in the parent's setup, with a button that writes it as the parent's own field.
- **Evidence ladder:** in context, the engine can go beyond published specs, to the upstream block's **characterized** results (affine forms, so shared knobs stay correlated), and finally to **in-context simulation**.

---

## 9. Types and checks

**Three tiers:**

1. **Shape (type checker, instant).** Wrong port type, a missing pin binding, unit mismatch, `if` on a toleranced value, a setup key that isn't a port of its block, a `pub spec` that reaches the circuit.
2. **Roles (net checker, after build-time expansion).** Two sources on a rail, an unpowered IC, fighting outputs, missing pull-ups, duplicate I2C addresses. Nets joined through an inductor, fuse or ferrite count as one rail, so a buck's switch node feeding its output through the inductor passes.
3. **Numbers (engine).** Logic levels, derating, abs-max and `rated`, contract checks. These are reported instantly when every input is exact, and otherwise by the engine with a verdict and a range.

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

- **Drawable = the flat subset.** A circuit with no `for`/`if`/`match` (array instances allowed) opens as an editable schematic. It is stored as ordinary language text, plus a **layout sidecar** that holds only geometry. Delete the sidecar and the netlist, values, verdicts and SPICE deck are byte-identical; the editor just re-places the symbols.
- **Generated circuits** (with loops) show read-only structure. Values that trace back to a single literal stay editable in place.
- **Modes:**
  - *Edit mode* edits the **circuit** (parts, nets, values).
  - *Simulate mode* edits the **setups and the contract** (conditions, probes, specs).
- **Every gesture is a named statement edit.** Drawing, the `:` command line, paste and AI edits all become the same edits to the text, so the text is always the truth.
  - **How an edit reaches the text:** the parser keeps every token (comments and whitespace included) and a byte span on every syntax node. Changing r1's value replaces the bytes of that one value expression; the formatter then re-prints only that statement, so the rest of the file is untouched. No lossless syntax tree is needed for this (roadmap §4.4).
- **Results never live in the source.** Verdicts appear:
  - as badges on the schematic,
  - in the spec table,
  - as inlay hints in the code:
    ```
    spec bass: h.f_low(-3dB) <= 30Hz;     ✗ 31.2 Hz (3σ) · 1.1% of boards · C_in
    ```
  - A counterexample is printed as a pasteable point for re-simulation. v0.1 called it a `corner`; its v5 spelling is *not decided*.
- **AI "select to ask"** sends:
  - a slice of the model that compiles on its own, as language text;
  - a rendered image with the same labels;
  - the relevant results.

  The AI answers with named edits, which are shown as a schematic diff together with how the verdicts would change, before you accept. Every verdict in that diff comes from a re-check of the edited design, with its verdict word (PASS (all corners), PASS (estimated), PASS (implied by worst case), FAIL, UNDECIDED) and its status (`simulated`, or `stale` once the design changes). Nothing is shown as "verified" or as an unchecked prediction (`engine_plan.md` §3, §7).

Full detail, including auto-placement and KiCad import/export: `research/language_editor_mapping.md` (in v0.1's syntax).

---

## 11. More examples

### 11.1 A buck converter block

```rust
use std::prelude::*;
use ti::Tps54302;

/// Bottom feedback resistor for V_out = V_ref · (1 + R_top/R_bot).
fn fb_bottom(v_out: Volt, v_ref: Volt, r_top: Ohm) -> Ohm { r_top * v_ref / (v_out - v_ref) }

/// 8–28 V → 3.3 V / 2 A buck around TI TPS54302 (400 kHz, internal compensation).
pub block Buck3v3 { vin: Power<In>, vout: Power<Out>, gnd: Ground }

circuit Buck3v3 {
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

setup Operating for Buck3v3 {
    vin:  Supply { v: 8V..=28V },
    vout: Load   { i: 0A..=2A },            // what the loads may draw
    temp: ambient,                          // the project's env (§11.2)
}
setup Steady for Buck3v3 { vout.i: 2A, window: ..=2ms, ..Operating }
setup LoadStep for Buck3v3 {
    vout.i: Step { from: 0.2A, to: 2A, edge: 1us, at: 1ms },
    window: ..=2ms,
    ..Operating
}

contract Buck3v3 {
    setup = Operating;

    /// Output accuracy. ±3% would fail: V_ref alone spans ±2.5%.
    pub spec vout_dc: dc(vout.v) within 3.3V ± 5%;
    spec ripple:    tran(vout.v).pp()                   <= 30mV    on Steady;
    spec transient: tran(vout.v).deviation()            <= 165mV   on LoadStep;   // 5% of 3.3 V
    spec loop_pm:   ac(u1.loop_gain()).phase_margin()   >= 45°;                   // internal: names a part
}
```

- The engine reports a worst case of 3.16 … 3.43 V for `vout_dc` (research/language_core.md §10.1). That passes ±5% and fails ±3%, which is exactly what the doc comment claims, and now it's checked instead of asserted.
- `vout_dc` is `pub`, so a parent may rely on it (§11.2). `loop_pm` names the part `u1`, so it's internal.
- v0.1 also wrote `spec capability: vout.i_max >= 2A`. Here the default setup's `vout: Load { i: 0A..=2A }` says it: every spec must hold for loads up to 2 A.
- Automatic checks add current limit vs peak inductor current, inductor saturation vs current limit, and capacitor voltage derating (engine v3 §4).
- `fn` and `from` are v0.1's design. `fn`'s Rust-style return type (`-> Ohm`) would need the `->` token the plan dropped (step 1); *not decided in v5*.

### 11.2 A board, where a contract fails at a connection

```rust
env ambient: Temperature in -10°C..=60°C;

pub block Board { v12: Power<In>, gnd: Ground }

circuit Board {
    net v3v3;
    net audio_in;
    net audio_out;

    let amp  = CeAmp { vcc: v12, gnd, input: audio_in, output: audio_out };
    let buck = Buck3v3 { vin: v12, vout: v3v3, gnd };
    let node = SensorNode { v3v3, gnd };      // its Operating setup: v3v3: Supply { v: 3.3V ± 3% }
}

setup Operating for Board {
    v12:  Supply { v: 12V ± 5% },             // the external adapter
    temp: ambient,
}

contract Board {
    setup = Operating;
}
```

```
error[E-contract]: `buck.vout` guarantees 3.3 V ± 5%, but `node.v3v3` needs 3.3 V ± 3%
  = note: `pub spec vout_dc` in `contract Buck3v3`; `v3v3: Supply { v: 3.3V ± 3% }` in `setup Operating for SensorNode`
  = help: tighten the buck (0.1% feedback resistors in a `lot`, or a better reference), or widen SensorNode's setup
```

The MCU + I2C sensor example (ATtiny85 + two TMP117s, with pull-up and address checks) is in `research/language_core.md` §10.2, in v0.1's syntax.

### 11.3 More in v5

v5's full example is `research/contract_syntax_v5.md` §2: the LDO with modes, a load step and a dropout row, the generic gain stage, the in-amp pair, and a sensor board with faults. Write its steps and sweep with named `from`/`to` fields (plan step 2). `parser/ok/v5_mvp.spl` is the part of it that parses today.

---

## 12. Open questions

**Still open from v0.1:**

1. **Name and file extension.**
2. **Part record details:** the exact syntax for families, tables, envelopes and aging laws. Should follow the engine v3 knob model as it gets implemented. Also whether an IC is a part record or a model block (§6.8).
3. **Catalog sources:** which distributor and parametric APIs first, and how to cache them for reproducible builds.
4. **Multi-function MCU pins** (PB0 = SDA or MOSI): how a pin's role is selected.
5. **Variants and do-not-populate:** per-build differences.
6. **Which parts join rails by default** (inductor, ferrite, fuse, shunt), and the escape hatch.
7. **Behavioral models in the language** vs SPICE files only. It now includes how a model block computes its `observe` values and its per-mode supply current ([IR] §3.8).
8. **How much of the interface `rules { }` language users can write** in v1.
9. **Temporal specs** (power sequencing order and delays). Events, `.during` and `.after` cover a fault's timing; a small signal-temporal-logic subset may still come later.
10. **Formatting control** the editor takes over drawn files.

**Open in v5 (v5 §4):**

11. `emits` might become `event` in block headers.
12. Setup fields might read `z: <= 0.5Ω` instead of `z: ..=0.5Ω`.
13. `#[outside]` might get a clearer name.
14. How a board's modes select a child's state (an MCU's Run or Sleep current).
15. Second implementations of one block (parked). Draft 3 had `circuit X::Ideal { … }`, after VHDL's architectures.
16. The exact spelling of measure methods (§8.4).
17. From [IR] §6: which setups a `pub spec` may use (§8.3); whether stimulus fields are outside the envelope check (§8.2); whether a bare quantity in a measure means `dc(…)` (§8.3). Its other questions are answered: v5's constructors for events, its rules 1.3.3 (derived fields win over modes) and 1.3.7 (a `z` knob per leg), and plan decision 3 (`ce_amp.spl` keeps three specs).
18. **When specs run** (not MVP): likely one attribute, `#[run(ci | nightly | manual)]`, on top of automatic tiers from the engine's cost estimate (`research/runs_language.md`).

**Open from this rewrite** (v5 and the plan don't decide them):

19. **Trait membership:** `pub block Ldo3v3: Regulator { … }` or `impl Regulator for Ldo3v3 {}` (§7.3).
20. **Library items:** only a block or a spec can be `pub`, so how are parts, families, `fn`s and signals exported (§2)? And an `fn`'s return type, `-> Ohm` in Rust, needs a `->` token, which the plan dropped (§11.1).
21. **Block parameters:** a toleranced parameter (v0.1's `param r: Tol<Ohm>`, §7.1), and whether a const generic's type is `f64` or a unit or `Number` (`research/contract_v4_review_novice.md`).
22. **Loops and arrays:** `for` in a circuit, how its range is spelled now that `a..b` is an error, `where` bounds, array ports and typed nets (§3.5, §7.4).
23. **Strict limits:** v5's limits are `within`, `<=` and `>=`. The parser also accepts `<` and `>` as a spec's relation. Keep them, or reject them with a fix.
24. **Several files:** how `env`s and `const`s are shared across a project ([IR] D11), and whether the engine should read `const confidence` by its name ([IR] D10).
25. **Environment links:** how a part's tempco and aging find the `temp` field and `env life` (§5.4), and per-placement conditions such as `#[env(…)]` (§8.2).
