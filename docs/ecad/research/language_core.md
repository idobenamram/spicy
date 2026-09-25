# Core Language and Type System: Design Proposal v0.1

> Working draft · 2026-09-25 · research thread "language core"
> Builds on `../vision.md`, `../archive/engine_v1.md` (v1, §6–§7), `../archive/engine_v2.md` (Part E), `../walkthrough.md` (the CE amplifier) and `../specs.md` (contracts).
> Sibling threads: **spec/contract syntax** (owned elsewhere; this document keeps spec syntax minimal and marks every spec line as "spec thread") and **editor mapping** (`language_editor_mapping.md`, whose hard requirements R1–R21 are answered in §12.1).
> References: `externals/atopile`, `externals/spade` (local clones), PolymorphicBlocks (cloned for this study at commit HEAD of `BerkeleyHCI/PolymorphicBlocks`; paths below are repo-relative), and the datasheets cited inline.

---

## 0. Summary

### 0.1 The decisions

| # | Decision | Choice | Main reason |
|---|---|---|---|
| L1 | Overall shape | Standalone Rust-like language; 8 new keywords (`block part port param net env signal interface`), everything else is Rust syntax with Rust meaning | "Feels like simple Rust" and rule 1 of v1 §7.5 |
| L2 | Items | `block` (hierarchical, drawable), `part` (leaf: pins, BOM, sim model), `fn` (pure math), plus `const`, `enum`, `type`, `env`, `signal`, `interface` | Each item kind has exactly one schematic presence (§3) |
| L3 | Block header | Ports and params are declared at the top of the block body (`port …;`, `param …;`). They *are* the I/O page | Parameters render as fields, ports as pins; the I/O page is a view of these lines |
| L4 | Instantiation | `let name = Type { field: value, … };`: a Rust struct literal whose fields are the type's ports and params | Named pins, one statement per placed symbol, stable names |
| L5 | Connection | **Pins bind to nets**, inside the instance literal: `Resistor { p: vcc, n: base, value: 47k ± 1% }`. Every pin is bound exactly once. Nets are declared (`net base;`). No chain syntax, no `connect()` statement | Reads like a SPICE element line with named pins; canonical by construction; no forward-reference ceremony |
| L6 | Nets | Undirected, order-independent, always named. A net's *kind* is inferred from what is bound to it. Joining two existing nets is a rare, keyed `net x = [a, b];` | Nets are what probes, specs, the AI and labels point at |
| L7 | Signals and roles | Single-conductor signal types `Pin`, `Ground`, `Power`, `Analog`, `Logic`, with roles as typestate generics: `Power<In>`, `Logic<OpenDrain>` | Same idiom as embedded-Rust HALs (`PA5<Output<PushPull>>`); roles map 1:1 to KiCad pin types and decide symbol sides |
| L8 | Bundles | Nominal `interface` types (`I2c`, `Spi`, `Uart`, `Usb2`, `Diff<S>`), with roles (`I2c<Controller>`) and bus-level rules | atopile's one-statement bus connection, PolymorphicBlocks' link rules |
| L9 | Ground | A separate `Ground` port, bound explicitly like any other port. No global nets, no implicit binding | Avoids atopile's `.reference` wiring burden and action at a distance |
| L10 | Units | Dimensioned quantity types (`Volt`, `Ohm`, …), dimension vectors checked at compile time (`Volt * Amp == Watt`); SI literals `10k`, `3.3V`, `100nF`, `4k7`; case-sensitive prefixes (`m` milli, `M` mega) | v1 rule 3; SPICE's `M`=milli is the classic trap |
| L11 | Tolerance | `47k ± 1%` or ASCII `47k +/- 1%`. `+-` is rejected with a fix-it because in Rust it means `+ (-…)` | Rule 1 of v1 §7.5 |
| L12 | Ranges | Closed ranges `4.5V..=6.5V`. `..` stays Rust's half-open integer range (`for i in 0..8`) | Rust meaning preserved |
| L13 | ⊆ vs ⊇ | The **site** sets the quantifier, and each quantifier has its own keyword: part field = *budget* (∀, statistical), `env`/port assumption = *condition* (∀, range), `?` = *search* (∃), `require … in` = *limit* (⊆). A tolerance is its own type, `Tol<T>`, distinct from a value `T` | atopile decides ⊆/⊇ by enclosing block kind (invisible); here it is always visible |
| L14 | Knobs | Created where a budget meets a physical part, named by path (`amp.r1.value`); shared knobs are `env` items; correlated lots are `lot(…)` values | Engine v2 Part B naming; independent copies by default |
| L15 | Staging | Two value stages: **exact** (known at elaboration) and **design** (carries knobs, `?`, or simulated results). Structure (`if`, `for`, `match`, array sizes) may depend only on exact values | Prevents "if on a toleranced value"; Spade's type-level vs runtime split |
| L16 | Generics | **If it changes the drawing, it goes in `<>`** (`const N: usize`); **if it changes a number on the drawing, it is a `param`**. `where … else "msg"` (from Spade) for compile-time conditions | Editing a param never restructures a sheet; editing a const generic regenerates it |
| L17 | Loops | Elaboration-time `for`/`if`/`match`; instances created in a loop use an **indexed let**, `let r_s[i] = …`, which gives the stable path `r_s[3]`. Vectorized array instances `let r_s[0..N] = …` stay drawable | Stable identity (editor R2), visible in the code exactly as in the sidecar |
| L18 | Type checking tiers | (1) **shape** errors from the type checker, (2) **role** errors from the net checker after elaboration (shorted supplies, unpowered IC, fighting outputs, missing pull-ups, address clashes), (3) **level** checks (5 V into a 3.3 V pin) as automatic specs in the engine, reported instantly when every input is exact | Types catch what needs no numbers; numbers carry tolerance, temperature and four-valued verdicts |
| L19 | Traits | No user traits in v1. What atopile does with traits splits into attributes (metadata), roles and interface rules (checks), and, later, `trait` as a *block signature* for abstract blocks and refinement | atopile's traits mix five unrelated jobs (§8.4) |
| L20 | Inference | Rust-level: local only. Item signatures (ports, params, fn signatures) are fully annotated; literals take units from the expected type; const generics are inferred from bound arrays; roles are never inferred on ports | Local errors, self-describing I/O pages |
| L21 | Drawability | A block is drawable iff its body is in the **flat subset** (§3.8). Drawn blocks are stored as flat code plus a geometry sidecar | One semantic format; D6 becomes a syntactic property |

### 0.2 What changed from the v1 sketch (v1 §7.4)

| v1 sketch | Now | Why |
|---|---|---|
| `block AdcInput(vin: Power, out: Analog)`: ports as fn parameters | `port vin: Power<In>;` lines at the top of the body | Ports are not positional arguments; ICs have dozens; params and ports must look different (editor R19) |
| `Resistor(r_top +- 1%, "0402")` | `Resistor { p: vcc, n: mid, value: r_top ± 1%, package: R0402 }` | Named pins, no magic strings, and no `+-` |
| `vin.pos -- r1 -- out -- r2 -- vin.gnd` | pins bind to named nets in each literal | `--` is valid Rust (`a - (-b)`), and chains hide pin identity (v2 Part E) |
| `require vin.v * r_bot / (r_top + r_bot) in 3.3V +- 2%` | `require dc(V(out)) in 3.3V ± 2%` (spec thread) | Specs point at the circuit (v2 A6) |
| `Power` = `{pos, gnd}` bundle, direction implicit | `Power<In>` / `Power<Out>` single conductor + separate `Ground` | Roles drive ERC and symbol layout; ground is shared, not per rail |
| `?`, `let r_top: Ohm = ?` | kept | Established in every design doc |

---

## 1. Principles

| # | Principle | Consequence |
|---|---|---|
| P1 | **If it looks like Rust, it behaves like Rust** (v1 §7.5). Deviations may only *accept more* than Rust does, never change the meaning of something Rust accepts | `+-`, `--`, `a..b` on reals are rejected; three documented relaxations (§12.3) |
| P2 | **One gesture ↔ one keyed statement.** Every schematic action maps to editing one statement, keyed by a name | Placing a part = one `let`; wiring a pin = one field in that `let` |
| P3 | **Names are identity.** Instances, nets, ports and knobs are addressed by paths (`board.amp.r1`, `prot.r_s[3]`) | Sidecar keys, knob names, AI references, diffs |
| P4 | **Declarative and order-independent inside a block.** A block body is a set of keyed declarations | Insert anywhere; feedback needs no forward declarations |
| P5 | **Types catch what needs no numbers; the engine catches what does.** | Three tiers of checks (§8.1) |
| P6 | **Structure is decided at elaboration, numbers in the engine.** | Two value stages (§6.7) |
| P7 | **Small surface.** No lifetimes, borrowing, closures, runtime mutation, macros or user traits in v1 | A language an EE can learn in an afternoon, and that the AI can't misuse |
| P8 | **Written for the AI too.** Canonical formatting, one way to write each thing, diagnostics with machine-applicable fix-its, doc comments as model data | The language is the AI's main interface |

---

## 2. A first look: the CE amplifier

The walkthrough amplifier (walkthrough §1), written as it would be stored:

```rust
// src/amp.spl
use std::prelude::*;        // Resistor, Capacitor, Electrolytic, Npn, Power, Ground, Analog, …

/// Common-emitter audio stage. Gain ≈ RC/RE ≈ 4.6, f_L ≈ 20 Hz (walkthrough §1).
pub block CeAmp {
    // ── I/O page ─────────────────────────────────────────────────────────────
    port vcc: Power<In> { v: 12V ± 5% };   // assumption → range knob `vcc.v`
    port gnd: Ground;
    port input: Analog<In>;
    port output: Analog<Out>;

    // ── sheet ────────────────────────────────────────────────────────────────
    /// Bias node, ≈ 2.1 V.
    net base;
    net emitter;

    /// Divider holds the base near 2.1 V, which sets IC ≈ 1.4 mA.
    let r1 = Resistor { p: vcc, n: base, value: 47k ± 1% };
    let r2 = Resistor { p: base, n: gnd, value: 10k ± 1% };

    /// Gain ≈ RC/RE. RC also puts VC near mid-supply for symmetric swing.
    let rc = Resistor { p: vcc, n: output, value: 4.7k ± 1% };
    let re = Resistor { p: emitter, n: gnd, value: 1k ± 1% };

    /// f_L = 1/(2π·C_in·R_in) ≈ 20 Hz. Electrolytic: base side is the positive one;
    /// loses up to 20 % of its capacitance by end of life.
    let c_in = Electrolytic { p: base, n: input, value: 1uF ± 20%, drift: -20% };

    let q1 = Npn { c: output, b: base, e: emitter, beta: 100..=300, beta_tc: 0.5%/K };

    // ── contract (spec thread owns this syntax; shown for context) ──────────
    require dc(V(output)) in 4.5V..=6.5V else "leave ±1 V of swing both ways";
    require ac(V(output) / V(input), 1kHz) in 4.6 ± 5%;
    require ac(V(output) / V(input)).f_low(-3dB) <= 30Hz;
}
```

And the project root declares the shared conditions once:

```rust
// src/main.spl
pub env temp: Celsius in -10°C..=60°C;
/// 0 = new, 1 = end of life (10 years).
pub env life: Ratio in 0..=1;
```

### 2.1 What the compiler derives

**Nets** (members come from the bindings; a net's kind is the join of its members, §8.2):

| Net | Members | Kind |
|---|---|---|
| `vcc` (port) | `r1.p`, `rc.p` | Power |
| `gnd` (port) | `r2.n`, `re.n` | Ground |
| `input` (port) | `c_in.n` | Analog |
| `output` (port) | `rc.n`, `q1.c` | Analog |
| `base` | `r1.n`, `r2.p`, `c_in.p`, `q1.b` | Pin (passive) |
| `emitter` | `re.p`, `q1.e` | Pin (passive) |

**Knobs**: exactly the walkthrough's nine (walkthrough §3):

| Knob path | Kind | Created by |
|---|---|---|
| `temp` | range | `env` |
| `life` | range | `env` |
| `vcc.v` | range | port assumption `{ v: 12V ± 5% }` |
| `r1.value`, `r2.value`, `rc.value`, `re.value` | statistical | `± 1%` meeting a part field |
| `c_in.value` | statistical | `± 20%` (its `drift` links it to `life`) |
| `q1.beta` | statistical | `100..=300` (its `beta_tc` links it to `temp`) |

When a parent instantiates `let amp = CeAmp { … }`, the part knobs become `amp.r1.value` etc. A second instance gets its own part knobs and shares `temp` and `life` (v2 Part B).

**Role checks** pass: `vcc` is an input port, so inside the block it acts as the rail's source (§4.3). **Shape checks** pass. **Automatic specs** added by the part models include electrolytic polarity (`V(base) − V(input) ≥ −0.3 V` at every corner), resistor power and capacitor voltage derating (spec design §8).

### 2.2 How it looks in the editor

The block has one **symbol** (auto-generated from the roles: inputs left, outputs right, `Power<In>` top, `Ground` bottom) and one **sheet** (this body). Because the body is in the flat subset (§3.8), the sheet is editable as a drawing; every drawing edit rewrites one of the statements above.

```
          vcc                         ┌ CeAmp · I/O ──────────────────────────────┐
       ┌───┴───┐                      │ Ports                                     │
input ─┤ CeAmp ├─ output              │   vcc     Power<In>    expects 11.4–12.6 V│
       └───┬───┘                      │   gnd     Ground                          │
          gnd                         │   input   Analog<In>                      │
                                      │   output  Analog<Out>                     │
                                      │ Params    —                               │
                                      │ Specs     (spec thread)                   │
                                      └───────────────────────────────────────────┘
```

Doc comments (`///`) are model data: they appear as the rationale on the part's value card and the net's hover (vision §4).

---

## 3. Items

### 3.1 Overview and schematic presence

| Item | What it is | Schematic presence | Has a body? |
|---|---|---|---|
| `block` | Hierarchical circuit with ports and params | A **sheet** (its body) and a **symbol** (a box whose pins are its ports; custom symbol optional) | Yes |
| `part` | Leaf component: ports, pin map, BOM data, simulation model | A **symbol** from the part library, with pin numbers | No |
| primitive | A `part` in `std::prim` whose model is a simulator device (R, C, L, D, BJT, MOSFET, sources) | A standard symbol (zig-zag, etc.) with a value label | No |
| `fn` | Pure function over values | None. Appears in the value card when a value uses it ("R_bot = e96(fb_bottom(…))") | Expression body |
| `const` | Exact named value | None (value cards show the name) | — |
| `enum` | Closed set of choices (dielectric, clamp style, address strap) | Drop-downs in property panels | — |
| `type` | Alias (`type SlewRate = Volt / Second;`) | None | — |
| `env` | Project-wide range knob (`temp`, `life`) | The top block's I/O page, "Conditions" panel | — |
| `signal` | Single-conductor port type with roles and net rules (library-level) | Pin electrical type; symbol side | Rules |
| `interface` | Nominal bundle of signals with roles and bus rules (library-level) | A bus port / thick bus wire with an entry | Rules |

Users write blocks all the time, parts occasionally (most are generated from datasheets, vision §5), `fn`s sometimes, and `signal`/`interface` almost never (the standard library provides them).

### 3.2 `block`

```rust
/// Doc comment → block description on the I/O page.
pub block Name<const N: usize = 8>           // const generics: structure (§7.1)
where N >= 1 else "at least one channel"     // compile-time conditions (§7.3)
{
    // I/O page: ports and params come first (the compiler enforces this order)
    port a: Power<In> { v: 4.5V..=5.5V };    // port with contract parameters (§4.7)
    port b: [Analog<In>; N];                 // array port
    port en: Logic<In> = nc;                 // optional port: may be left unbound
    param gain: Ratio = 2.0;                 // value parameter with a default
    param r: Tol<Ohm> = 1k ± 1%;             // a tolerance spec (a budget), see §6.5

    // Sheet: nets, instances, values; in any order (P4)
    net mid;
    let r1 = Resistor { p: a, n: mid, value: r };
    let v_mid: Volt = 2.5V;                  // a named design value (an equation, §6.8)

    // Contract: assumptions, specs, setups (spec thread)
}
```

**Rationale.**

- **Ports and params in the body, not in a parenthesized header.** A block is "a struct whose fields are its ports and params, plus a body". That is exactly why instantiation is a struct literal (§5.2). It also makes the I/O page a contiguous, keyed region of the file that the editor can render and edit as a form.
- **`port` and `param` keywords instead of type-directed fields.** `pub vcc: Power<In>` would be Rust-exact (public fields are the visible interface), but EEs reading the I/O page need "this is a pin" vs "this is a value" at a glance, and the editor needs it syntactically (editor R19).
- **I/O page first.** Ports and params must precede body statements. It keeps the contract readable at the top of every file and gives the formatter a fixed layout.

**Alternatives considered.**

| Alternative | Why not |
|---|---|
| v1 / Spade `entity`-style header `block Amp(vcc: Power, …)` | Positional-looking; long for ICs; mixes ports and values; instantiation becomes a positional call |
| Rust `struct` + `impl` split (`struct Amp { … } impl Amp { fn build() }`) | Two items per block, method ceremony, suggests runtime OOP |
| VHDL entity/architecture as two items | The split is right conceptually (I/O page vs sheet) but belongs in one item; alternative architectures are handled by `trait` later (§8.4) |

### 3.3 `part`

Library parts are leaves: typed ports, a pin map, BOM metadata, and one or more simulation models. Attributes carry semantic metadata only (editor R15).

```rust
/// 4.5–28 V input, 3 A synchronous buck converter, fixed 400 kHz.
#[mpn = "TPS54302DDCR"]
#[manufacturer = "Texas Instruments"]
#[datasheet = "https://www.ti.com/lit/ds/symlink/tps54302.pdf"]   // SLVSDG6C
#[footprint = "SOT-23-6"]
#[symbol = "ti:TPS54302"]
#[provenance(extracted = "ai", reviewed = false)]
pub part Tps54302 {
    port vin: Power<In> { v: 4.5V..=28V };                  // recommended operating range
    port gnd: Ground;
    /// Switch node. Sources the output rail through the inductor (a series element, §8.3).
    port sw: Power<Out>;
    port boot: Pin;
    port fb: Analog<In> { v: -0.1V..=5.5V };
    /// Float to enable (internal pull-up current).
    port en: Logic<In> { vih: 1.28V, vil: 1.1V, v: -0.1V..=5.5V, pull: Pull::Internal } = nc;

    pins { gnd: 1, sw: 2, vin: 3, fb: 4, en: 5, boot: 6 }   // datasheet Table 4-1

    param v_ref: Volt = (0.581V..=0.611V).typ(0.596V);      // §5.5 electrical characteristics
    param f_sw: Hertz = (290kHz..=510kHz).typ(400kHz);
    param i_limit: Amp = (4A..=6A).typ(5A);                 // high-side peak current limit

    model averaged = Spice {
        file: "tps54302_avg.lib", subckt: "TPS54302_AVG",
        pins: [vin, gnd, en, fb, sw, boot],                  // SPICE node order
    };
}
```

- **`pins { … }`** maps each port (or bundle member) to one or more package pins: `gnd: [2, 7, EP]`, `i2c: { sda: 5, scl: 7 }`, BGA names as strings (`"A1"`). The symbol is looked up by `#[symbol]`, and its pins are matched by number (editor R16).
- **Datasheet triples** (min/typ/max) are written `(min..=max).typ(x)`: a budget whose nominal is not the center. `.nom()` reads the nominal back as an exact value.
- **Several models** (`model averaged = …; model switching = …;`) are selected by the setup (spec thread). A model can also be a `block` written in this language (controlled-source macro-models).
- **Multi-unit parts** (dual op-amps) declare each unit as a bundle-typed port with `#[unit]`: `#[unit] port a: OpAmp;` gives `u1.a.out`, drawn as a separate symbol unit (editor R16).
- **Provenance attributes** mark AI-extracted data. The engine shows unreviewed parameters in its "how it was computed" column (spec design §9).

### 3.4 Primitives (`std::prim`)

Primitives are parts whose model is a device in our simulator (`crates/spicy_simulate/src/devices/`: resistor, capacitor, inductor, diode, BJT, sources). They are also **generic parts**: `Resistor { value: 47k ± 1% }` becomes a BOM line through picking (§9.3).

| Primitive | Pins | Main fields (defaults omitted) | Simulator |
|---|---|---|---|
| `Resistor` | `p`, `n` | `value: Tol<Ohm>`, `power: Watt`, `tc: PerKelvin`, `drift: Ratio`, `package`, `series: bool` | R |
| `Capacitor` | `p`, `n` | `value: Tol<Farad>`, `rating: Volt`, `dielectric`, `esr`, `tc`, `drift` | C |
| `Electrolytic` | `p`, `n` (polarized) | as `Capacitor`, plus an automatic polarity spec | C |
| `Inductor` | `p`, `n` | `value: Tol<Henry>`, `i_sat: Amp`, `dcr: Ohm` (series element) | L |
| `Diode` | `a`, `k` | `model: DiodeModel` | D |
| `Npn`, `Pnp` | `c`, `b`, `e` | `beta`, `beta_tc`, `model: BjtModel` | Q |
| `Nmos`, `Pmos` | `d`, `g`, `s`, `b` (defaults to `s`) | `model: MosModel` | M (not yet in the simulator) |
| `Vdc`, `Idc`, `Vsin`, `Vpulse`, `Vpwl` | `p`, `n` | waveform fields | V, I (simulation-only, `#[sim_only]`) |

Pin names follow SPICE conventions: current is positive into `p`, and `V(p) − V(n)` is the element voltage. Tempco and aging links are ordinary expressions over `env` knobs inside the primitive's model:

```rust
// std::prim (abridged)
pub part Resistor {
    port p: Pin;
    port n: Pin;
    param value: Tol<Ohm>;
    param tc: PerKelvin = 0ppm/K;
    param drift: Ratio = 0%;        // relative change at end of life (life = 1)
    // …
    model = builtin::R { r: value * (1 + tc * (env::temp - 25°C)) * (1 + drift * env::life) };
}
```

### 3.5 `fn`

Pure functions over values. They cannot create instances, touch nets, or read ports. That is the same rule Spade enforces ("Only entities and pipelines can take ports as arguments", `externals/spade/spade-diagnostics/src/lib.rs:117`).

```rust
/// Bottom feedback resistor for V_out = V_ref · (1 + R_top/R_bot).
pub fn fb_bottom(v_out: Volt, v_ref: Volt, r_top: Ohm) -> Ohm {
    r_top * v_ref / (v_out - v_ref)
}
```

- A `fn` works at both stages (§6.7). Called with exact arguments, it is evaluated at elaboration. Called with design values, its body is inlined into the engine's expression graph, so affine arithmetic sees `r_top * v_ref / (v_out - v_ref)` with every knob named. This is how "hand-written formulas get guaranteed bounds" (walkthrough §8).
- `if` inside a `fn` may only branch on exact values. `min`, `max`, `abs`, `sqrt`, `exp`, `ln`, `pow`, `atan` are built in and handle design values.
- `let mut` and `for` over exact ranges are allowed inside `fn` bodies (Rust semantics). No recursion in v1.

### 3.6 `const`, `enum`, `type`, `env`

```rust
pub const F_SW: Hertz = 400kHz;                 // exact, module-level (project-wide parameter)
pub enum Clamp { Diodes, Tvs }
pub type PerKelvin = Ratio / Kelvin;            // transparent alias (dimensions are structural)
pub env temp: Celsius in -10°C..=60°C;          // range knob, shared project-wide
```

`env` items are the only global state (editor R6). The standard prelude declares `temp` at 25 °C and `life` at 0 so that `tc` and `drift` fields do nothing until the project widens them. The top block's I/O page flags the defaults ("temp: 25 °C only — set the operating range").

### 3.7 `signal` and `interface` (library level)

These define port types, their roles, and the rules that nets of that type obey. Users rarely write them. They are shown here because they are where ERC lives (§8.3).

```rust
// std::io (abridged). A sketch: the rule language is deliberately tiny.

/// One conductor that carries supply current. Voltage is measured to the ground domain.
pub signal Power {
    role In  { v: Range<Volt>, i: Amp = 0A, abs_max: Range<Volt> = v }   // stated by the load
    role Out { v: Tol<Volt>, i_max: Amp = Amp::MAX }                      // stated by the source
    domain series;                                     // merges across series elements (§8.3)
    where count(Out) <= 1 else "two sources drive this rail";               // conflict rule
    where count(In) == 0 || count(Out) == 1 else "this rail has loads but no source";  // presence rule
    require for load in In: Out.v in load.v;           // → automatic spec (engine)
    require sum(In.i) <= Out.i_max;                    // → automatic spec (engine)
}

pub interface I2c {
    scl: Logic,
    sda: Logic,
    role Controller { scl: OpenDrain, sda: OpenDrain }
    role Target     { scl: In, sda: OpenDrain; addr: u8 }
    where count(Controller) >= 1 else "I2C bus has no controller";
    where unique(Target.addr) else "I2C address conflict";
    where pulled(scl) && pulled(sda) else "I2C lines need pull-ups";
    require rise_time(scl) <= 300ns;                   // Fast-mode; → automatic spec
}
```

`where` rules are structural and exact: they run in the net checker at compile time. `require` rules need numbers: they become automatic specs in the engine (spec design §8). This is PolymorphicBlocks' "link" idea (`edg/electronics_interfaces/VoltagePorts.py:17-106`: one source, a vector of sinks, voltage limits intersected, current summed, `require`s), moved into the port type's definition.

### 3.8 The flat subset (drawable blocks)

A block is **drawable** iff its body uses only:

- `port`, `param`, `net` declarations (including `net x = [a, b];` joins);
- instance `let`s with pin bindings to nets, ports, members, elements, slices or `nc`;
- **vectorized array instances** `let r[0..N] = T { … };` (§7.2);
- value `let`s and value expressions of any complexity, including `fn` calls and `from` derivations (values are not structure);
- attributes, doc comments, and contract items (spec thread).

It may not use `for`, `if`, `match`, or indexed lets inside loops at statement level. The compiler reports `drawable: yes/no` per block, with the offending span (editor R12, P4). The editor edits a drawn block by rewriting its flat code, so D6's "export to code" is not a conversion; the drawing *is* the code plus the geometry sidecar. That makes the language the native file format (vision "Open items: file format").

---

## 4. Ports, signals, interfaces and roles

### 4.1 Three levels of port type

| Level | Types | Connects to |
|---|---|---|
| **Pin** | `Pin` (passive, untyped conductor) | Any single-conductor net |
| **Signal** (single conductor, typed, with a role) | `Ground`, `Power<R>`, `Analog<R>`, `Logic<R>` | Nets of compatible kind (§8.2) |
| **Bundle** (named members) | `I2c<R>`, `Spi<R>`, `Uart`, `Usb2<R>`, `Diff<S>`, user interfaces | Bundle nets of the same interface |

Primitive pins are `Pin`. Signal types appear on block ports and on IC pins, where the datasheet says what the pin *is*. Passive pins connect to anything, so a pull-up resistor happily binds to a `Power` rail and a `Logic` line.

### 4.2 Roles as typestate generics

```rust
port vin:  Power<In>;          // KiCad "power input"
port vout: Power<Out>;         // KiCad "power output"
port gnd:  Ground;             // role-free: a reference
port sda:  Logic<OpenDrain>;   // KiCad "open collector"
port rst:  Logic<In>;          // KiCad "input"
port gpio: Logic<InOut>;       // KiCad "bidirectional"
port sig:  Analog<Out>;        // driven analog signal
port node: Analog;             // = Analog<Passive>: analog contract data, no drive claim
port bus:  I2c<Target>;
```

| Signal | Roles | KiCad pin type |
|---|---|---|
| `Power` | `In`, `Out`, `InOut` (dual-role, e.g. USB-PD) | power input / power output |
| `Logic` | `In`, `Out` (push-pull), `InOut` (tri-state), `OpenDrain`, `OpenSource` | input / output / bidirectional / open collector / open emitter |
| `Analog` | `In`, `Out`, `Passive` (default) | input / output / passive |
| `Ground` | — | power input (reference) |
| `Pin` | — | passive |

**Why generics.** Embedded-Rust HALs already encode pin modes as typestates (`PA5<Output<PushPull>>`, `Input<PullUp>`), so Rust readers know the idiom. The role is part of the type, so `Power<In>` and `Power<Out>` are different types for hovers, errors and the I/O page. Roles also give the editor a symbol side for free (editor R7, R15).

**Alternatives considered.** VHDL/Verilog mode keywords (`port vin: in Power`) read well for signals but don't extend to bundle roles (`Controller`/`Target`). Separate type names (`PowerIn`, `VoltageSink` in PolymorphicBlocks) multiply names and lose the "same signal, different side" relation. Spade's `inv` flip (`externals/spade/output_test/test/port_construction/code.spade:4-21`) handles two-sided bundles but not multi-party buses (one controller, many targets, one pull-up set).

### 4.3 The boundary flip

A block's own ports are seen **flipped** from inside. `port vcc: Power<In>` means "this block consumes power from outside"; inside, the `vcc` net treats the port as its source. `port vout: Power<Out>` means "this block provides power"; inside, `vout` must be sourced by something in the body. This is Spade's `inv` and Chisel's `Flipped`, applied to roles rather than wire directions. Users never write the flip: the net checker applies it, and error messages speak from the user's side ("`vout` is declared as an output but nothing inside drives it").

### 4.4 Bundles, role tables and crossover

An interface lists members and, per bundle role, the member roles. Binding a bundle port to a bundle net binds every member by name. Two refinements:

- **Crossover.** `Uart { tx: Logic<Out>, rx: Logic<In> }` joined to another `Uart` would short `tx` to `tx`, and the role checker catches it (two push-pull outputs). The fix is explicit: `gps.uart: link.crossed()`, a method the interface declares (`cross { tx <-> rx }`). PolymorphicBlocks does the same crossover inside its UART link.
- **Extension.** `interface Smbus: I2c { alert: Logic<OpenDrain> }` connects wherever an `I2c` is expected (its `I2c` members join; `alert` must be bound separately). This formalizes atopile's hard-coded compatible pair `ElectricLogic`/`ElectricSignal` (`externals/atopile/src/faebryk/libs/app/erc.py:220-229`).

Partial bundle binding uses a bundle literal of nets: `bus: I2c { scl: scl_a, sda: sda_b }`. Member access works everywhere (`i2c.sda`).

### 4.5 Arrays and buses

```rust
port gpio: [Logic<InOut>; 6] = nc;          // every element optional
port ch: [Analog<In>; 8];
net ain: [Analog; 8];
net sense: [Analog; 12];
let adc = Ads7828 { ch: ain, … };           // whole array
let prot = InputProtect { raw: sense[0..8], safe: ain, … };     // slice (Rust half-open, integers)
net taps = [v_top, v_ref[1..8]];            // concatenation: an 8-element net array
```

Sizes are checked at compile time. Slicing uses Rust integer ranges. An optional element that ends up alone on its net is a no-connect.

### 4.6 Ground and reference domains

- `Ground` is its own port type and is **bound explicitly**, like any port: `let amp = CeAmp { vcc: v12, gnd, input, output };`. With Rust's field-init shorthand this costs one word per instance.
- Voltages of `Power`, `Analog` and `Logic` ports are measured to the block's ground domain: its unique `Ground` port. A block with several grounds (`agnd`, `gnd_iso`) must say which one each port refers to, with `#[ground(gnd_iso)]`, and the compiler rejects ambiguity.
- Joining two ground domains is always explicit: a `NetTie` part (which is also what layout needs) or a `net gnd = [agnd, dgnd];` join, which draws a lint suggesting a `NetTie`.

**Alternatives considered.**

| Option | Precedent | Verdict |
|---|---|---|
| `Power = { pos, gnd }` bundle | atopile `ElectricPower { hv, lv }` (`ElectricPower.py:20-21`) | Rejected. Every rail drags a ground member along, so the same ground net appears under many names (`vcc.gnd`, `v5.gnd`); binding a rail whose `gnd` is a different domain silently joins grounds; every signal needs a reference too, and atopile's own guide calls missing `.reference` wiring "a common source of bugs" (`externals/atopile/.claude/skills/ato/SKILL.md:889`) |
| Global ground net | SPICE node 0, KiCad GND symbol | Rejected. Global nets are action at a distance (editor R6) and break isolated designs |
| Implicit binding of an instance's `Ground` to the parent's unique ground | PolymorphicBlocks implicit-connect tags (`getting-started.md:623`) | Considered and dropped. It saves one word per instance and hides a connection |
| **Separate `Ground`, explicit binding** | PolymorphicBlocks `gnd` ports | **Chosen** |

### 4.7 Port contract parameters (the hook for the spec thread)

A port's role type declares which parameters it carries (`Power::In { v, i, abs_max }`, `Power::Out { v, i_max }`, `Logic::In { vih, vil, v, pull }`, `I2c::Target { addr }`). A block or part fills them in braces on the declaration:

```rust
port vin:  Power<In>  { v: 8V..=28V };                 // the block assumes this
port vout: Power<Out> { v: 3.3V ± 5%, i_max: 2A };     // the block guarantees this
```

The role decides the direction of the contract. Parameters stated by an `In` side about what arrives (`v`) are **assumptions**: range knobs inside the block, and "upstream guarantee ⊆ my assumption" checks outside. Parameters stated by an `Out` side are **guarantees**: specs inside, facts for the parent. That is spec design §2 at the port level. The spec thread may provide `assume`/`guarantee` statements that desugar to these, or replace the brace form. The core only requires that port types carry typed parameters and that the role fixes their direction.

---

## 5. Nets and connections

### 5.1 Semantics

- A **net** is an undirected set of pins and ports. Membership is order-independent and idempotent (union-find).
- Nets are **named**: ports are nets, and every other net is declared with `net name;` (plain), `net bus: I2c;` (bundle), `net data: [Pin; 8];` (array).
- A net has no direction. Roles belong to the ports bound to it, and the net checker reads them (§8.3).
- Probes and specs refer to nets by name (`V(base)`), and the path `amp.base` is readable from a parent (read-only; connections go through ports only).

### 5.2 Syntax: pins bind to nets

Every pin (and every bundle or array port) is bound **exactly once**, in its instance's literal:

```rust
net base;
let r1 = Resistor { p: vcc, n: base, value: 47k ± 1% };
let q1 = Npn { c: output, b: base, e: emitter, beta: 100..=300 };
```

Rules:

1. A binding names a net, a port, a member (`i2c.sda`), an element or slice (`ch[3]`, `ch[0..4]`), a bundle literal of nets, or `nc`.
2. **Pins never bind to other pins** (`b: r1.n` is an error: "bind both pins to a named net"). Every connection goes through a named node, as in a SPICE netlist.
3. Fields without defaults must be given, which is Rust's own rule for struct literals. An IC's power pins have no default, so an unpowered IC is first caught as `missing field 'vdd' in 'Tmp117'`.
4. Fields with defaults may be omitted. Optional pins default to `nc`; parameters default to their declared value. This is a deliberate relaxation of Rust (Rust RFC 3681, default field values, requires writing `..` to take defaults).
5. Rust's field-init shorthand works: `Tps54302 { vin, gnd, sw, boot, fb, en }`.
6. A part or block literal may appear **only** as the right-hand side of an instance `let`. It can't be nested in an expression, passed to a `fn`, or created without a name (editor R1: no anonymous instances, no inline construction).

**Joining two existing nets** (a sheet that passes a rail straight from one port to another, or a star-ground join) is the only case with no instance to hold the binding. It uses a keyed net statement:

```rust
net rail = [vin, vout];        // one net, reachable by three names; the declared name is canonical
```

### 5.3 Why this form: alternatives compared

| Form | Example | Rust meaning | Pins visible | Canonical | Verdict |
|---|---|---|---|---|---|
| Chain | `vin -- r1 -- out` | **valid Rust: `vin - (-r1) …`** | No | No | Rejected (P1; v2 Part E) |
| atopile wire / bridge | `a ~ b`, `a ~> r ~> b` (`AtoLexer.g4:54-56`) | not Rust | Bridge: no | No (edge sets) | Rejected: bridges hide pins and need a `can_bridge` trait on every part |
| Chisel bulk connect | `a <> b` | not Rust | Yes | No | Rejected: binary, invites chains, and imports Chisel's direction-inferring semantics |
| Edge statement | `connect(a, b, c);` | a call (plausible) | Yes | No: many texts per net | Considered; used by Modelica, Chisel `attach`. Dropped because it is unkeyed (editor R4) and makes one net's membership scattered |
| Net-centric | `net base = [c_in.n, r1.b, r2.a, q1.b];` | array literal | Yes | Yes | Strong (editor R3). Not chosen as the canonical form, see below |
| Assignment | `r1.p = vin;` | mutation | Yes | Yes | Rejected: suggests direction and mutation; needs `let mut` |
| SKiDL operators | `vin & r1 & gnd`, `net += r1[1]` | overloaded ops | No / yes | No | Rejected: chain-like, Python-specific |
| **Element-centric binding** | `Resistor { p: vin, n: mid, … }` | struct literal | **Yes** | **Yes: each pin appears once, in its instance** | **Chosen** (Zener's `pins = {"VCC": vcc}` is the precedent) |

**Element-centric vs net-centric**, gesture by gesture (statements touched):

| Gesture | Element-centric | Net-centric |
|---|---|---|
| Place a part onto existing wires | 1 | 1 + one per pin |
| Delete a part | 1 | 1 + one per pin |
| Change a part's value and a connection | 1 | 3 |
| Wire an unconnected pin to a named net | 1 | 1 |
| Wire two unconnected pins | 3 (new `net` + 2 instances) | 1 |
| Merge two named nets | k (rename refactor) | 2 |
| Read "what is R1 connected to?" | 1 line | search |
| Read "what is on node `base`?" | search (the editor shows it on hover) | 1 line |

Element-centric wins every part-level gesture, which are the common ones, and loses the net-level ones. Three further reasons tip it:

- **It is a SPICE element line with named pins.** `R1 vcc base 47k` becomes `let r1 = Resistor { p: vcc, n: base, value: 47k }`. EEs and LLMs both read and write SPICE fluently.
- **Selections compile standalone.** A select-to-ask slice is a set of instance statements plus the `net` names they mention. Net-centric slices need partial net statements.
- **Every pin is bound exactly once, syntactically.** A pin can't be on two nets by construction, and forgetting a pin is a compile error at the literal, with its span.

`net` declarations remain first-class, keyed statements (name, type, doc comment, attributes). They are what probes and labels refer to, which is the substance of editor R3. §12.1 lists this as the one requirement where this thread proposes a different form.

### 5.4 Implicit vs explicit nets

- **All nets are explicit.** A binding to an undeclared name is an error with a spelling suggestion ("no net named `bsae`; did you mean `base`?"), never a new net.
- **The only thing inferred is a net's kind**, from its members (§8.2). `net x: Power;` is allowed as an assertion.
- **Nets created by the editor** (drawing a wire between two free pins) get a name minted once and then kept (editor §2.2: port name, else role name such as `base`, else `n1`).
- **No-connect** is the value `nc` in a binding (`alert: nc`). It is keyed by the instance that holds it. A required input bound to `nc` is an error unless the pin declares an internal pull.

### 5.5 Stable identity

| Entity | Identity | Notes |
|---|---|---|
| Instance | parent path + local name: `board.amp.r1` | Local names are unique per block. Instances can't be shadowed (a restriction of Rust `let`) |
| Generated instance | + index: `prot.r_s[3]`, `imu.amp[X]` | Integer or enum keys. Enum keys survive insertion in the middle; integer keys append |
| Net | block + name: `CeAmp::base` | Declared or minted; never derived from connectivity at read time |
| Knob | instance path + field: `amp.r1.value`; env name: `temp` | What affine forms, contributors and counterexamples print |
| Reference designator | Separate from the name. Assigned by annotation, stored with picks in `Spicy.lock`, pinnable with `#[refdes = "R12"]` | Renumbering never touches code |

Renames go through the refactor tool (code, sidecar, probes, chat alias table), and hand renames are caught by the editor's rename detector (editor §2.2). The sidecar is keyed by **local** paths within the block definition, so every instance of a block shares one sheet layout, as with KiCad hierarchical sheets.

---

## 6. Values

### 6.1 Quantity types

A quantity type is a dimension vector over the SI base units (length, mass, time, current, temperature, amount, luminous intensity), with rational exponents so that `V/√Hz` works. Named aliases:

| Type | Literal units | | Type | Literal units |
|---|---|---|---|---|
| `Volt` | `V` | | `Hertz` | `Hz` |
| `Amp` | `A` | | `Second` | `s` (and `h`, `yr` for lifetimes) |
| `Ohm` | `Ω`, `ohm` | | `Watt` | `W` |
| `Siemens` | `S` | | `Joule` | `J` |
| `Farad` | `F` | | `Kelvin` | `K` (a temperature *difference*) |
| `Henry` | `H` | | `Celsius` | `°C`, `degC` (an absolute temperature) |
| `Ratio` | none, `%`, `ppm` | | `Decibel` | `dB` |
| `Angle` | `°`, `deg`, `rad` | | | |

- **Structural typing over dimensions.** `Volt / Ohm` *is* `Amp`; aliases are transparent. `Volt * Amp == Watt`, `Ohm * Farad == Second`, `1 / (Ohm * Farad) == Hertz`. Printing picks the best named unit.
- **Two tagged exceptions.** `Celsius` is an affine point type: `Celsius − Celsius = Kelvin`, `Celsius + Kelvin = Celsius`, `2 * 25°C` is an error. `Angle` is kept apart from `Ratio`, to stop degree/radian mix-ups.
- **Dimension inference** follows Kennedy's units-of-measure work (F#): unification over an Abelian group of exponent vectors. The standard library provides dimension-polymorphic built-ins (`sqrt`, `min`, `max`, `abs`); user `fn`s are dimension-monomorphic in v1.
- **Elaboration numbers** are Rust's `usize`, `i64`, `u8`, `bool`, plus `str` and enums.

### 6.2 Literals and lexing

A number literal is `digits(.digits)?(e[+-]?digits)?` with an optional **glued** suffix (no space): `10k`, `3.3V`, `100nF`, `2.2µF`, `47kΩ`, `1MHz`, `10mA`, `25°C`, `1%`, `100ppm`. The suffix splits as *prefix + unit* by longest unit match at the end. The same approach as Spade's glued integer suffixes (`9i8`, `externals/spade/spade-ast/src/token.rs:52`).

| Rule | Example | Note |
|---|---|---|
| Prefixes are SI and **case-sensitive** | `m` = 10⁻³, `M` = 10⁶, `k` = 10³, `u`/`µ` = 10⁻⁶ | SPICE is the opposite: `M` = milli, `Meg` = mega (our own netlist parser: `crates/spicy_parser/src/netlist_types.rs:244-246`). `1Meg` is rejected with a fix-it; `.spicy` import converts |
| Prefix-only literals take their unit from the expected type | `value: 47k` in a `Resistor` is `Ohm` | Like Rust integer literal inference. Without an expected type: error "`10k` needs a unit here" |
| `K` is kelvin, not kilo | `value: 1K` on a resistor → "expected `Ohm`, found `Kelvin`; did you mean `1k`?" | The most common habit carried over from case-insensitive SPICE |
| RKM codes (IEC 60062) | `4k7` = 4.7k, `4R7` = 4.7 Ω, `2u2` | Accepted on input |
| Compound units are glued suffixes | `100ppm/K`, `0.5%/K`, `2mV/K`, `10V/us` | Only when written without spaces and every part after `/` is a unit. The formatter spaces binary operators, so `10V / t` stays a division |
| `%`, `ppm` glued to a number are literals | `1%` = 0.01 | `%` with a space (`i % 2`) is Rust's remainder. `3%2` is an error with the fix-it `3 % 2` |
| Unicode and ASCII spellings | `Ω`/`ohm`, `µ`/`u`, `°C`/`degC`, `±`/`+/-` | Both accepted; formatter setting chooses one |

### 6.3 Tolerances

```rust
47k ± 1%            // relative
3.3V ± 50mV         // absolute
47k +/- 1%          // ASCII spelling, the same token (atopile lexes '+/-' as one token: AtoLexer.g4:103-105)
10uF ± (-20%, +80%) // asymmetric
```

- `±` binds looser than `+`/`-` and tighter than `..=` and comparisons: `a + b ± 1%` is `(a + b) ± 1%`. Mixing `±` and `..=` without parentheses is an error.
- **`+-` is rejected.** `47k +- 1%` is valid Rust meaning `47k + (-1%)`. The type checker sees `Ohm + Ratio` and emits a dedicated diagnostic with the fix-it `±` (§8.7). `+/-` has no Rust meaning (`a +/ -b` doesn't parse), so it is safe as a token.
- A bare value on a physical part field (`value: 47k`) takes the project's default tolerance class (`Spicy.toml`: `tolerance.resistor = "1%"`), shown as a hint (`47k (±1% default)`). `± 0%` means ideal, which is only allowed on `#[sim_only]` parts or with an explicit `#[allow(ideal)]`. Analysis with zero spread on a real part would be silently optimistic.

### 6.4 Ranges

`min..=max` is a closed range, as in Rust. `a..b` on quantities is an error with the fix-it `..=`. Half-open ranges on real numbers carry no useful meaning, and keeping `..` for integer loops and slices keeps it Rust-exact. This proposes a change to `../specs.md` §6 and §12, which write `4.5V..6.5V` (§12.2).

### 6.5 Budget, condition, search, limit: keeping ⊆ and ⊇ visible

atopile decides what `=` means from the *enclosing block kind*. In a `module`, `x = 10k ± 5%` is a subset constraint; in a `component`, the same line is a superset (`externals/atopile/src/atopile/compiler/ast_visitor.py:846` sets `IsSubset` for modules, `:861` sets `IsSuperset` for components). The text looks identical and means the opposite.

Here each quantifier has its own **site and keyword**, and tolerance specs have their own **type**:

| You write | Name | Meaning | Quantifier | Knob kind |
|---|---|---|---|---|
| `value: 47k ± 1%` / `beta: 100..=300` in a part field | **budget** | "this part is somewhere in here" | ∀ | statistical |
| `env temp: Celsius in -10°C..=60°C` / `port vcc: Power<In> { v: 12V ± 5% }` | **condition** | "the world is somewhere in here" | ∀ | range |
| `value: ? ± 1%` | **search** | "pick one value" | ∃ | solver |
| `require dc(V(out)) in 4.5V..=6.5V` | **limit** | "the result must stay in here" | ⊆ check | — |

**A budget is also a picking rule, and the two agree.** The declared set D is what the analysis quantifies over (∀x ∈ D). The picked part's real spread A must satisfy A ⊆ D. Analysing over D ⊇ A is conservative, so one declaration serves both jobs. This is atopile's "part ⊇" and "spec ⊆" (v2 Part F) reconciled in a single statement.

**Tolerance specs are their own type.** `47k ± 1%` has type `Tol<Ohm>`: a description of a spread (nominal, width, distribution), not a value. `r1.value` has type `Ohm`: a design value carrying the knob `r1.value`. So:

- `param r: Tol<Ohm> = 1k ± 1%` passed to eight resistors gives **eight independent knobs**, because a knob is created where a budget meets a part field. Correlation is never accidental.
- `let x = 47k ± 1%; require x * 2 < 100k;` is an error: "a tolerance spec is not a value; read a part's value (`r1.value`) or declare a condition (`env`)".
- `Range<T>` (from `..=`) converts to a budget in a part field (uniform or datasheet-default distribution, spec design open decision 4) and to a condition after `in`.

### 6.6 Knob kinds, distributions and links

| Kind | Syntax | Example |
|---|---|---|
| Statistical | a budget in a part field | `value: 47k ± 1%` |
| Distribution override | method on the budget | `value: (47k ± 1%).dist(Uniform)` (default: truncated normal, σ = tol/3, v2 A7) |
| Range | `env` item or `In`-port parameter | `env life: Ratio in 0..=1;` |
| Linked to a shared knob | part fields interpreted by the model | `tc: 100ppm/K` (→ `temp`), `drift: -20%` (→ `life`), `beta_tc: 0.5%/K` (→ `temp`) |
| Solver-chosen | `?` | `value: ? ± 1%`; `let r_top: Ohm = ?;` |
| Correlated lot | a named `lot` value | `let fb_lot = lot(0.1%);` then `Resistor { …, lot: fb_lot }` |

- **Solver search spaces** come from the type and the budget (`? ± 1%` on a resistor searches E96; `± 5%` searches E24), narrowed by ordinary `require` statements (`require r_top.value in 1k..=1M;`), as in v1 §7.4. No extra syntax.
- **Lots.** Parts sharing `lot: L` move together over their declared tolerance (one shared knob), with an independent residual of `L`'s width each. That models "narrow but offset reels" (bibliography §12) and matched networks. A resistor-network *part* declares this internally. It is a keyed `let` (editor R10).
- **Solved values** are recorded in `Spicy.lock` by path and shown as ghost values. Accepting one rewrites `?` into a committed literal with `from ?`-style provenance (§6.8). Open question 11.

### 6.7 Exact and design values (staging)

Every value is at one of two stages, inferred and shown only in hovers and errors:

| Stage | What | Examples |
|---|---|---|
| **exact** | Known at elaboration | literals without tolerance, `const`s, integers, bools, enums, strings, const generics, loop variables, `.nom()` of a budget, `fn` results on exact arguments |
| **design** | Carries knobs, `?`, or simulation results; evaluated by the engine | `r1.value`, `vcc.v`, `u1.v_ref`, any expression over those |

Exact values promote to design values freely. The reverse needs `.nom()` (the nominal of a budget or of a part's value), and `?` has no nominal until solved.

**The rule:** `if`, `for`, `match`, array sizes and const-generic arguments take exact values only. Comparing design values yields a *claim* usable in `require`, not a `bool`. This is the circuit version of Spade's split between type-level integers (`#uint N`) and runtime values, and of Chisel's Scala-time vs hardware-time values. It is what makes `if gain > 10 { … }` an error when `gain` is a design value (§8.7).

### 6.8 Committed value + derivation (`from`)

From the editor thread (editor §9, R9): a value can carry both what is committed and how it was derived.

```rust
let r_fb_bot = Resistor {
    p: fb, n: gnd,
    value: 22.1k ± 1% from e96(fb_bottom(3.3V, u1.v_ref.nom(), r_fb_top.value.nom())),
};
```

The literal `22.1k ± 1%` is what the engine uses and the schematic shows. The expression after `from` is the derivation, which must evaluate to an exact nominal of the same unit. In *flag* mode the editor marks the value when the derivation no longer yields 22.1k; in *cascade* mode it rewrites the literal as a reviewable patch. `from` binds loosest in a field value. A value with no committed literal (`value: e96(…) ± 1%`) is always derived, which means always cascade.

---

## 7. Generics and elaboration

### 7.1 Const generics vs params

> **If it changes the drawing, it goes in `<>`. If it changes a number on the drawing, it is a `param`.**

```rust
pub block InputProtect<const N: usize = 8, const CLAMP: Clamp = Clamp::Diodes> { … }   // structure
pub block Divider { param ratio: Ratio; param r_total: Ohm = 100k; … }                  // values
```

- Const generics use Rust syntax (`const N: usize = 8`, defaults allowed). Enum-valued const generics are allowed (Rust needs an unstable feature for this), and a path argument needs no braces: `TempSensor::<Add0::Vplus>`.
- They can be inferred from bindings: `InputProtect { safe: adc.ch, … }` infers `N = 8` from `adc.ch: [Analog<In>; 8]`.
- `if`/`for`/`match` in a block body may depend on const generics, consts and loop variables only, **never on params**, even exact ones. That keeps the editor promise: editing a param in the property panel never restructures a sheet; editing a const generic regenerates it. The panel shows them as "Structure" and "Values".
- A `Divider` needs no generic: `ratio` changes numbers, not the drawing. `Divider { ratio: ?, … }` hands it to the solver.

### 7.2 `for`, `if`, `match`, indexed lets and array instances

All control flow runs at elaboration; there is no runtime. Hence no Spade-style `gen` marker (`gen if`, `externals/spade/spade-compiler/stdlib/array.spade:16`): in this language every `if` is a `gen if`.

```rust
for i in 0..N {
    let r_s[i] = Resistor { p: raw[i], n: safe[i], value: r };
}
```

- **Indexed let.** An instance created inside a loop must be written `let name[key] = …`. It becomes an element of the array `name`, with path `name[3]`, reachable after the loop (`r_s[2].p`). A plain `let` of an instance inside a loop is an error with that fix-it. `let x[i]` is not valid Rust, so the deviation is visible rather than a Rust construct with a new meaning. Plain value `let`s inside loops keep Rust scoping.
- **Enum keys.** `for axis in [Axis::X, Axis::Y, Axis::Z] { let amp[axis] = … }` gives `amp[X]`. Inserting a key doesn't renumber anything (editor R2).
- **Array instances (vectorized, drawable).** `let r_s[0..N] = Resistor { p: raw, n: safe, value: r };` creates N instances in one statement. A binding to an array of matching length connects element-wise; a binding to a scalar broadcasts; anything else is an error. This is the Virtuoso/Altium arrayed-instance idiom (`R<0:7>`), and it stays in the flat subset (editor T1, R20).
- **`match` and `if`** choose structure (`match CLAMP { Clamp::Tvs => { … } … }`) and can compute bindings (`add0: match ADDR { Add0::Gnd => gnd, … }`).
- **Determinism.** Elaboration order is source order and key order, never hash order (editor R2).

atopile forbids `new` inside loops, so arrays must be created before the loop and constrained inside it (`SKILL.md:760-762`). The indexed let keeps each generated part's statement in one place instead.

### 7.3 `where … else "message"`

Borrowed from Spade (`M >= N else "Sign-extend result must be wider than the input value"`, `externals/spade/spade-compiler/stdlib/num.spade:72-73`; parser `externals/spade/spade-parser/src/lib.rs:2070-2140`):

```rust
pub block InputProtect<const N: usize = 8>
where N >= 1 && N <= 32 else "InputProtect supports 1–32 channels"
{ … }
```

`where` is for **exact** conditions, checked at compile time with a definite answer. Its engine-side sibling is `require … else "message"` for design claims, which have four-valued verdicts (v2 Part E, spec thread). Interface rules use `where` for the same reason (§3.7).

### 7.4 How generated structure renders

- A generated block's sheet is auto-placed and **read-only in structure** (v1 rule 5). Values that trace to a single literal stay editable in place (editor §3.7, R13 provenance).
- A `for` renders as a labeled frame ("`for i in 0..8`") around repeated columns, laid out from one template. The sidecar can hold a template rule instead of eight positions:

  ```toml
  # layout/InputProtect.toml: geometry only, never read by the compiler
  "r_s[0]" = { x = 20, y = 10, rot = 0 }
  "r_s[*]" = { stride = [0, 12] }     # element k is placed at r_s[0] + k·stride
  ```
- A vectorized array instance renders as one arrayed symbol labeled `×N`, as Virtuoso does.
- Instances of `InputProtect::<4>` and `::<8>` share one layout file, because keys are local and templated.

---

## 8. The type system

### 8.1 Three tiers of checks

| Tier | When | What it catches | Result |
|---|---|---|---|
| **1. Shape** (type checker) | While typing (LSP) | Wrong interface (`I2c` bound to `Spi`), dimension mismatch, unknown member, array size, missing required field, `+-`, `if` on a design value, unit typos | Hard error, precise span |
| **2. Role** (net checker, after elaboration, no numbers) | On save / continuously | Two supplies on one rail, unpowered IC, fighting push-pull outputs, floating input, open-drain net without pull-up, I2C bus without controller or pull-ups, duplicate I2C addresses, ground shorted to a rail, two ground domains joined | Hard error listing every member's binding span |
| **3. Level** (contract engine) | Background; **instantly when every input is exact** | 5 V into a 3.3 V-only pin, VOH/VIH mismatch, current budget, abs-max at corners, I2C rise time | Automatic spec with verdict, bracket and counterexample (spec design §8–§9) |

Tier 2 is Spade's "every wire driven exactly once" (a linear type check, `externals/spade/spade-hir-lowering/src/linear_check/mod.rs:30-72`) adapted to undirected nets: roles are counted **per net** rather than per value use. Analog nets have no "driven once" property, but roles do (exactly one source per power rail, at most one push-pull driver per logic net).

### 8.2 Nominal and structural typing; the signal lattice

- **Nominal:** interfaces, blocks, parts, enums, role markers. An `I2c` is not an `Smbus` unless declared as an extension (§4.4).
- **Structural:** quantity types (by dimension vector); arrays (by element type and length).
- **Signals form a small compatibility lattice.** A net's kind is the most specific kind among its members:

| Bound together | Allowed? | Net kind |
|---|---|---|
| `Pin` + anything single-conductor | yes | the other's kind |
| `Analog` + `Logic` | yes (a logic signal is an analog voltage: ADC on a GPIO) | Logic |
| `Analog<In>` / `Logic<In>` + `Power` or `Ground` | yes, as a **tie** (strap high/low, ADC measuring a rail) | Power / Ground |
| `Logic<Out>` + `Power` or `Ground` | **no**: output shorted to a rail | — |
| `Power` + `Ground` | **no**: short | — |
| `Power` + `Analog<Out>` | **no**: two drivers | — |
| Two different `Ground` domains | only via `net x = [a, b]` or a `NetTie` | Ground |
| Bundles | same interface or declared extension | the interface |

### 8.3 Role rules on nets

The net checker runs per block level, on each net's members: sub-instance ports with their declared roles, plus the block's own ports with **flipped** roles (§4.3).

**Two kinds of rule:**

- **Conflict rules** (at most one source; at most one push-pull driver; unique addresses) run at **every** level.
- **Presence rules** (needs a source; needs a driver or pull; bus needs pull-ups and a controller) run at **closed** nets, the level where the net includes none of the block's own ports. At an open net, a flipped own port satisfies what it can provide: an own `Power<In>` provides the source, an own `Logic<In>` provides the driver. Anything else must be satisfied inside: an own `Power<Out>` must be sourced inside.

**Power domains.** For power rules, nets joined through **series elements** count as one domain: `Inductor`, `FerriteBead`, `Fuse`, `Jumper`, and `Resistor { series: true }` (shunts, RC rail filters). This is what lets a buck converter pass: the controller's `sw: Power<Out>` reaches `vout` through the inductor, so the `vout` domain has exactly one source. It also lets a ferrite-filtered `vdda` pass without a special block. Plain resistors don't join domains, so a pull-up never makes a logic line part of a rail. When a rail has loads but no source and is fed through a plain resistor, the error suggests `series: true`.

**Pull-up recognition.** A resistor between a `Logic` net and a `Power`/`Ground` net is recognized structurally as a pull. The parallel pull resistance becomes a net parameter used by level rules (rise time, sink current). Engineers draw two resistors to VCC; they shouldn't have to use a special block. atopile does this with its `can_be_pulled` trait (`ElectricLogic.py:44`); PolymorphicBlocks requires an explicit pull-up port on its I2C link (`edg/electronics_interfaces/I2cPort.py:20-23`).

| Net kind | Conflict rules | Presence rules |
|---|---|---|
| Power (domain) | ≤ 1 `Out` (incl. flipped own `In`) | loads ⇒ exactly 1 source |
| Logic | ≤ 1 push-pull `Out`; no `Out` + `OpenDrain` mix (warning) | `In` needs a driver, pull or tie unless it declares an internal pull; `OpenDrain`-only needs a pull |
| Analog | ≤ 1 `Out` (warning) | none (transistor stages drive analog nets through passive pins) |
| Ground | never joined to a rail; domains joined only explicitly | — |
| Bundles | from the interface (`unique(Target.addr)`) | from the interface (`pulled`, controller count) |

### 8.4 Traits: what atopile uses them for, and our answer

atopile attaches traits to almost everything. In the files studied, they do five different jobs:

| Job | atopile traits (examples) | Our mechanism |
|---|---|---|
| Metadata | `has_designator_prefix` (`Resistor.py:59`), `is_atomic_part`, `has_part_picked`, `has_datasheet_defined` (`Texas_Instruments_TCA9548APWR.ato:11-16`), `has_usage_example` | **Attributes** (`#[mpn]`, `#[datasheet]`) and doc comments |
| Connection sugar | `can_bridge`, `can_bridge_by_name` (`Resistor.py:35-37`, `led_badge.ato:126`) | **Not needed**: there is no bridge/chain syntax |
| ERC | `is_source`/`is_sink` (`erc.py:477-483`), `requires_pulls` (`I2C.py:71-79`), `implements_design_check` | **Roles** and **interface rules** (§8.3) |
| Bus parameter aggregation | `is_alias_bus_parameter` (voltage shared), `is_sum_bus_parameter` (current summed) (`ElectricPower.py:65-69`) | **Interface rules** (`sum(In.i) <= Out.i_max`) |
| Abstraction | `Regulator` base + `from` inheritance (`regulators.ato:4-36`), retype `->` (`AtoParser.g4:93-95`) | **`trait` as a block signature**, v1.1 (below) |

atopile's own guide shows the cost: traits need a pragma gate (`#pragma experiment("TRAITS")`), and several have dedicated assignment shortcuts to hide them (`x.package = …`, `x.required = True`, SKILL.md §3.3).

**Proposal.** No user traits in v1 (consistent with v1 §7.5 rule 2). In v1.1, `trait` means one thing, a **block signature**: ports and params with no body. It supports abstract blocks and refinement (PolymorphicBlocks' `IoController` → `Stm32f103`, `getting-started.md:359-390`):

```rust
pub trait Regulator {
    port vin: Power<In>;
    port vout: Power<Out>;
    port gnd: Ground;
    param v_out: Volt;
}
impl Regulator for Buck3v3;                    // conformance, checked against Buck3v3's ports
let reg: impl Regulator = Buck3v3 { … };        // or a placeholder, refined at the top level
```

The schematic shows a placeholder symbol that can later be refined into a concrete block. Mixins (PolymorphicBlocks) and inheritance (atopile `from`) are rejected: both scatter a block's ports across several definitions.

### 8.5 Should voltage levels be types?

PolymorphicBlocks attaches electrical parameters to ports (voltage limits, output thresholds, current draw) and checks them in links with `require`s: "voltage out of limits", "incompatible digital thresholds" (`edg/electronics_interfaces/DigitalPorts.py:94,117`). It does not put voltages in *types*. We follow it, for three reasons:

1. **Levels depend on values that vary.** VOH is "vdd − 0.4 V", and vdd has a tolerance, a temperature drift and a load. The honest answer is a bracket and a four-valued verdict, which a type system can't express.
2. **Nominal logic families are too rigid.** Distinct types for `Lvcmos33` and `Lvcmos18` would reject 5 V-tolerant inputs, open-drain level shifting, and thresholds that scale with the supply (`vih: 0.7 * vplus.v`).
3. **We get the type-error experience anyway.** When every operand of a level rule is exact (a 5 V ± 5 % pull-up and a pin rated to `vdd + 0.3 V`), interval arithmetic decides it at compile time and reports a definite failure as an error, with the numbers (§8.7, example 9). Otherwise it becomes an automatic spec.

Refinement types (`Volt where self in 0V..=3.6V`) were considered and rejected for the same reason as point 1.

### 8.6 Inference level

| Thing | Inferred? | Rationale |
|---|---|---|
| `let` types | Yes, from the right-hand side | Rust |
| Units of prefix-only literals | Yes, from the expected type | `value: 47k` |
| Const generics | Yes, from bound arrays | `InputProtect { safe: adc.ch }` → `N = 8` |
| Net kinds | Yes, from members | Nets never need annotations |
| Bundle net types | Yes, from the first bundle binding, else annotate | `net i2c: I2c;` for clarity |
| Knob kinds | Yes, from the site (§6.5) | The keyword is the annotation |
| Value stage (exact/design) | Yes | Shown only in hovers and errors |
| Port types and roles | **Never** | They are the contract and the symbol |
| `fn` signatures, param types | **Never** | Rust: no inference across item boundaries; local errors |

### 8.7 Error messages

Rustc/ariadne style with stable codes (`spicy explain E0401`), primary and secondary labels, notes, and machine-applicable fix-its (P8). Spade's diagnostics crate is the model (`externals/spade/spade-diagnostics/src/lib.rs:1-230`: labels, notes, span suggestions).

**1. `+-`**
```
error[E0103]: `+-` is not a tolerance
  ┌─ src/amp.spl:21:51
   │
21 │     let r1 = Resistor { p: vcc, n: base, value: 47k +- 1% };
   │                                                     ^^ this is `47k + (-1%)`, as in Rust
   │
   = help: write `±` or `+/-`
   │
21 │     let r1 = Resistor { p: vcc, n: base, value: 47k ± 1% };
   │                                                     ~
```

**2. Case-sensitive units**
```
error[E0201]: mismatched units
   ┌─ src/amp.spl:26:53
26 │     let re = Resistor { p: emitter, n: gnd, value: 1K ± 1% };
   │                                                    ^^ expected `Ohm`, found `Kelvin`
   = note: unit prefixes are case-sensitive: `K` is kelvin, `k` is kilo
   = help: did you mean `1k`?
```

**3. Shorted supplies (role tier)**
```
error[E0401]: two power sources drive rail `v3v3`
   ┌─ src/board.spl:14:40
12 │     let buck = Buck3v3 { vin: v12, vout: v3v3, gnd };
   │                                          ---- `buck.vout` is a source (Power<Out>)
14 │     let ldo = Ldo3v3 { vin: v5, vout: v3v3, gnd };
   │                                       ^^^^ `ldo.vout` is a second source (Power<Out>)
   = note: a power rail has exactly one source
   = help: to share a load between supplies, use `std::power::OrDiodes`
```

**4. Unpowered IC (field tier, then role tier)**
```
error[E0063]: missing field `vplus` in `Tmp117`
   ┌─ src/node.spl:9:14
 9 │     let u1 = Tmp117 { gnd, bus, alert: nc };
   │              ^^^^^^ `vplus` (Power<In>) has no default: a power pin can't be left open

error[E0402]: rail `v_sns` has loads but no source
   ┌─ src/node.spl:9:31
 9 │     let u1 = Tmp117 { vplus: v_sns, gnd, bus, alert: nc };
   │                              ^^^^^ `u1.vplus` (Power<In>) is the only member besides passives
   = help: bind it to a supplied rail, or make it an input port: `port v_sns: Power<In>;`
```

**5. Rail fed through a plain resistor**
```
error[E0402]: rail `vdda` has loads but no source
   = note: `vdda` is fed from `v3v3` through `r_filt` (a plain resistor, which doesn't join rails)
   = help: if `r_filt` is part of the rail, mark it: `Resistor { …, series: true }`
```

**6. Structure depending on a design value**
```
error[E0305]: structure can't depend on a design value
   ┌─ src/filt.spl:11:8
11 │     if gain > 10 {
   │        ^^^^^^^^^ `gain` is a param: it can vary, be toleranced or be solved
   = note: `if`, `for`, `match` and array sizes decide what is drawn; they run at elaboration
   = help: make it a const generic: `block Filter<const HIGH_GAIN: bool>`
```

**7. Loop instance without an index**
```
error[E0303]: an instance created in a loop needs an index
   ┌─ src/prot.spl:22:13
22 │         let r_s = Resistor { p: raw[i], n: safe[i], value: r };
   │             ^^^ each iteration places a part; they need distinct, stable names
   = help: write `let r_s[i] = …` (the parts are named `r_s[0]`, `r_s[1]`, …)
```

**8. I2C address conflict (role tier, exact parameters)**
```
error[E0412]: I2C address 0x48 is used by two targets on `i2c`
   ┌─ src/node.spl:24:9
23 │     let t_inlet = TempSensor::<Add0::Gnd> { vdd: v3v3, gnd, bus: i2c, alert: nc };
   │         ------- 0x48 (ADD0 → GND)
24 │     let t_outlet = TempSensor::<Add0::Gnd> { vdd: v3v3, gnd, bus: i2c, alert: nc };
   │         ^^^^^^^^ also 0x48
   = help: strap it differently, e.g. `TempSensor::<Add0::Vplus>` (0x49)
```

**9. Level mismatch, decided early (contract tier)**
```
error[C0101]: `acc.u1.bus.sda` is driven above its rating at every corner
   ┌─ src/node.spl:17:31
17 │     let r_sda = Resistor { p: v5, n: i2c.sda, value: 4.7k ± 5% };
   │                               -- pulls `i2c.sda` up to `v5` (4.75 V … 5.25 V)
   = note: `acc.u1.bus.sda` is rated −0.3 V … vdd + 0.3 V = 3.50 V … 3.70 V (vdd = 3.3 V ± 3 %)
   = note: automatic spec, evaluated at compile time because every input is exact
   = help: pull up to `v3v3`, or insert a level shifter (`std::logic::I2cLevelShift`)
```

**10. Unit-less literal without context**
```
error[E0104]: `10k` needs a unit here
   ┌─ src/div.spl:8:19
 8 │     let r_total = 10k;
   │                   ^^^ no expected type to take the unit from
   = help: write `10kΩ` (or `10kohm`), or annotate: `let r_total: Ohm = 10k;`
```

Errors are **error-tolerant**: a broken statement is marked and the rest of the block still elaborates, so the schematic keeps rendering while the user types (editor R17).

---

## 9. Modules, packages and the part library

### 9.1 Modules and imports

Rust's module system, with file = module (editor R21):

```rust
mod power;                       // src/power.spl or src/power/mod.spl
use crate::power::Buck3v3;
use std::prelude::*;             // imported automatically
use ti::{Tps54302, Tmp117};      // a part package
pub block Board { … }            // private by default
```

### 9.2 Project layout and manifest

```
sensor-board/
├── Spicy.toml            # package, dependencies, defaults
├── Spicy.lock            # part picks, reference designators, accepted solver values (by path)
├── src/
│   ├── main.spl          # env items + the top block
│   ├── amp.spl
│   ├── node.spl
│   └── power/
│       ├── mod.spl
│       └── buck.spl
├── layout/               # geometry sidecars, one per block, keyed by local instance paths
│   ├── CeAmp.toml
│   └── power/Buck3v3.toml
└── models/               # extra SPICE models
```

```toml
[package]
name = "sensor-board"
version = "0.1.0"
top = "Board"

[dependencies]
std = "0.1"
ti = { package = "parts-ti", version = "0.3" }
microchip = { package = "parts-microchip", version = "0.2" }

[defaults]
confidence = "sigma3"            # spec design §7
tolerance.resistor = "1%"        # a bare `47k` means 47k ± 1% (§6.3)
tolerance.capacitor = "10%"
```

`.spl` is a placeholder extension (`.spicy` is already taken by netlists). The editor thread uses `.ckt`; we need one name (open question 1).

### 9.3 The part library

- **`std`**: primitives (`std::prim`), signals and interfaces (`std::io`), diode/BJT model constants (`std::models`, exposed by the prelude as `models`), E-series functions (`e24`, `e96`), helper blocks (`OrDiodes`, `I2cLevelShift`, `NetTie`).
- **Part packages** (`parts-ti`, …): one `part` per file with its symbol reference, pin map, parameters, models, and provenance attributes. Most are generated by AI-assisted datasheet extraction (vision §5), reviewed, and versioned like crates.
- **Picking.** Generic parts (`Resistor`, `Capacitor`, …) are resolved to MPNs by a picker that must satisfy budget ⊆ (§6.5), package, rating, and derating specs. Picks are written to `Spicy.lock` so builds are reproducible. The picker's catalog source stays pluggable (v1 open question: don't tie the tool to one distributor as atopile ties itself to JLC).
- **Symbols** come from KiCad `.kicad_sym` libraries (`#[symbol = "lib:name"]`). Blocks get auto-symbols from their port roles unless they name one.

---

## 10. Worked examples

### 10.1 Buck converter around a controller IC (drawable)

Values follow the TPS54302 datasheet (SLVSDG6C): pin map in Table 4-1, V_FB = 0.581 / 0.596 / 0.611 V, f_SW = 290 / 400 / 510 kHz, EN rising threshold 1.23 V typical, the Figure 7-1 input, bootstrap and EN-divider values, and the 3.3 V row of Table 7-2 (L = 6.8 µH, C_OUT = 44 µF, R2 = 100 kΩ, R3 = 22.1 kΩ, C8 = 47 pF). The part definition is in §3.3.

```rust
// src/power/buck.spl
use std::prelude::*;
use ti::Tps54302;

/// Bottom feedback resistor for V_out = V_ref · (1 + R_top/R_bot).
pub fn fb_bottom(v_out: Volt, v_ref: Volt, r_top: Ohm) -> Ohm {
    r_top * v_ref / (v_out - v_ref)
}

/// 8–28 V → 3.3 V / 2 A buck around TI TPS54302 (400 kHz, internal compensation).
/// Output tolerance: ±3 % would fail at worst case, because V_ref alone spans ±2.5 %.
pub block Buck3v3 {
    port vin: Power<In> { v: 8V..=28V };
    port vout: Power<Out> { v: 3.3V ± 5%, i_max: 2A };
    port gnd: Ground;

    net sw;
    net boot;
    net fb;
    net en;

    let u1 = Tps54302 { vin, gnd, sw, boot, fb, en };

    /// Input decoupling, close to VIN (Figure 7-1: 10 µF + 0.1 µF).
    let c_in = Capacitor { p: vin, n: gnd, value: 10uF ± 10%, rating: 50V, dielectric: X7R };
    let c_in_hf = Capacitor { p: vin, n: gnd, value: 100nF ± 10%, rating: 50V, dielectric: X7R };

    /// Bootstrap supply for the high-side gate driver (0.1 µF ceramic, datasheet §7.3).
    let c_boot = Capacitor { p: boot, n: sw, value: 100nF ± 10%, rating: 16V, dielectric: X7R };

    /// UVLO divider: starts at ≈ 1.23 V · (1 + 511k/105k) ≈ 7.2 V (EN pin currents ignored).
    let r_en_top = Resistor { p: vin, n: en, value: 511k ± 1% };
    let r_en_bot = Resistor { p: en, n: gnd, value: 105k ± 1% };

    /// Power stage. i_sat covers the 6 A worst-case high-side current limit.
    let l1 = Inductor { p: sw, n: vout, value: 6.8uH ± 20%, i_sat: 6A };
    let c_out1 = Capacitor { p: vout, n: gnd, value: 22uF ± 20%, rating: 10V, dielectric: X7R };
    let c_out2 = Capacitor { p: vout, n: gnd, value: 22uF ± 20%, rating: 10V, dielectric: X7R };

    /// Feedback: R_top = 100k (Table 7-2); R_bot from the reference, snapped to E96.
    let r_fb_top = Resistor { p: vout, n: fb, value: 100k ± 1% };
    let r_fb_bot = Resistor {
        p: fb, n: gnd,
        value: 22.1k ± 1% from e96(fb_bottom(3.3V, u1.v_ref.nom(), r_fb_top.value.nom())),
    };
    /// Feed-forward capacitor across R_top (Table 7-2: 47 pF for 3.3 V).
    let c_ff = Capacitor { p: vout, n: fb, value: 47pF ± 5%, dielectric: C0G };

    // spec thread
    require dc(V(vout)) in 3.3V ± 5%;
}
```

What this shows:

- **Roles close the loop.** Inside, `vin` (own `Power<In>`, flipped) sources the input rail; `u1.sw` (`Power<Out>`) sources the `vout` domain through `l1`, which is a series element (§8.3). So the own `vout: Power<Out>` is backed, and a missing inductor would be reported.
- **Budgets vs guarantees.** `u1.v_ref`'s datasheet spread (a budget in the part) and the four resistor tolerances are statistical knobs. The `vout` guarantee is checked against them by the engine, which is how the header comment's claim is verified, not merely asserted. The worst-case output is 3.16 … 3.43 V (nominal 3.293 V), so ±3 % fails and ±5 % passes.
- **Drawable.** No loops or branches, so the buck is a flat-subset block that opens as an editable schematic. The `from` derivation is a value, not structure.
- **The contract composes.** A parent binding a 12 V ± 5 % supply to `vin` gets an automatic check "11.4–12.6 V ⊆ 8–28 V"; loads on `vout` are checked against `i_max: 2A` by the `Power` rules (§3.7).

### 10.2 MCU and I2C sensors with pull-ups

Parts: Microchip ATtiny85 (SOIC-8: PB5/RESET 1, PB3 2, PB4 3, GND 4, PB0/SDA 5, PB1 6, PB2/SCL 7, VCC 8; VIL ≤ 0.3·VCC, VIH ≥ 0.6·VCC) and TI TMP117 (SCL 1, GND 2, ALERT 3, ADD0 4, V+ 5, SDA 6; V+ 1.8–5.5 V, pins rated −0.3…6 V, VIH 0.7·V+, VIL 0.3·V+, address 0x48–0x4B by ADD0 strap; datasheet Tables 5-1, 6.1 and 7-2).

```rust
// parts package (abridged)
#[mpn = "ATTINY85-20SU"]
#[manufacturer = "Microchip"]
#[footprint = "SOIC-8"]
pub part Attiny85 {
    port vcc: Power<In> { v: 2.7V..=5.5V };
    port gnd: Ground;
    /// USI two-wire interface on PB0 (SDA) and PB2 (SCL).
    port i2c: I2c<Controller> { vih: 0.6 * vcc.v, vil: 0.3 * vcc.v, abs_max: -0.5V..=vcc.v + 0.5V };
    port pb1: Logic<InOut> = nc;
    port pb3: Logic<InOut> = nc;
    port pb4: Logic<InOut> = nc;
    port reset: Logic<In> { pull: Pull::Internal } = nc;
    pins { reset: 1, pb3: 2, pb4: 3, gnd: 4, i2c: { sda: 5, scl: 7 }, pb1: 6, vcc: 8 }
}

#[mpn = "TMP117AIDRVR"]
#[manufacturer = "Texas Instruments"]
#[footprint = "WSON-6"]
pub part Tmp117 {
    port vplus: Power<In> { v: 1.8V..=5.5V, abs_max: -0.3V..=6V };
    port gnd: Ground;
    port bus: I2c<Target> { vih: 0.7 * vplus.v, vil: 0.3 * vplus.v, abs_max: -0.3V..=6V, c_in: 4pF };
    port alert: Logic<OpenDrain> = nc;
    /// Address select: GND → 0x48, V+ → 0x49, SDA → 0x4A, SCL → 0x4B.
    port add0: Pin;
    pins { bus: { scl: 1, sda: 6 }, gnd: 2, alert: 3, add0: 4, vplus: 5 }
}
```

A small generated wrapper ties the strap to the address, so the bus rule can check uniqueness at compile time:

```rust
// src/node.spl
use std::prelude::*;
use microchip::Attiny85;
use ti::Tmp117;

pub enum Add0 { Gnd, Vplus, Sda, Scl }

pub fn tmp117_addr(a: Add0) -> u8 {
    match a { Add0::Gnd => 0x48, Add0::Vplus => 0x49, Add0::Sda => 0x4A, Add0::Scl => 0x4B }
}

/// TMP117 with its address strap and local decoupling.
pub block TempSensor<const ADDR: Add0 = Add0::Gnd> {
    port vdd: Power<In>;
    port gnd: Ground;
    port bus: I2c<Target> { addr: tmp117_addr(ADDR) };
    port alert: Logic<OpenDrain> = nc;

    let u1 = Tmp117 {
        vplus: vdd, gnd, bus, alert,
        add0: match ADDR {
            Add0::Gnd => gnd,
            Add0::Vplus => vdd,
            Add0::Sda => bus.sda,
            Add0::Scl => bus.scl,
        },
    };
    /// 0.1 µF bypass, as the datasheet recommends.
    let c_dec = Capacitor { p: vdd, n: gnd, value: 100nF ± 10% };
}

/// ATtiny85 reading two TMP117 sensors over I2C, all at 3.3 V.
pub block SensorNode {
    port v3v3: Power<In> { v: 3.3V ± 3% };
    port gnd: Ground;

    net i2c: I2c;

    let mcu = Attiny85 { vcc: v3v3, gnd, i2c };
    let c_mcu = Capacitor { p: v3v3, n: gnd, value: 100nF ± 10% };

    /// Pull-ups. t_r ≈ 0.85·R·C_bus; 4.7k meets Fast-mode's 300 ns up to ≈ 75 pF.
    let r_scl = Resistor { p: v3v3, n: i2c.scl, value: 4.7k ± 5% };
    let r_sda = Resistor { p: v3v3, n: i2c.sda, value: 4.7k ± 5% };

    let t_inlet = TempSensor::<Add0::Gnd> { vdd: v3v3, gnd, bus: i2c };
    let t_outlet = TempSensor::<Add0::Vplus> { vdd: v3v3, gnd, bus: i2c };
}
```

What the checker does here:

| Check | Tier | Result |
|---|---|---|
| `mcu.i2c` (Controller) and two `bus` (Target) ports on one `I2c` net | shape | ok |
| One controller; addresses 0x48 and 0x49 unique | role (exact) | ok. With both at `Add0::Gnd`: error 8 in §8.7 |
| Pull-ups on `scl` and `sda` | role | recognized from `r_scl`, `r_sda` (resistor from Logic to Power); 4.7k ∥ … = 4.7k per line |
| `v3v3` has a source | role | inside `SensorNode`, the own `Power<In>` port; checked for real at the parent |
| High level 3.3 V ± 3 % ≥ VIH of every device (0.6·VCC, 0.7·V+) | level (exact → instant) | pass |
| Rise time ≤ 300 ns with 4.7k and C_bus = 3 × c_in + wiring estimate | level (automatic spec) | pass, with a bracket |
| `alert`, `pb1`, `pb3`, `pb4`, `reset` omitted | shape | allowed: defaults are `nc`; `reset` has an internal pull |

`SensorNode` is drawable. `TempSensor` is generated (its `match` picks structure), so its sheet is read-only, and its value cards (the 100 nF) stay editable.

### 10.3 Parametric block: an 8-channel input protection array

```rust
// src/prot.spl
use std::prelude::*;

pub enum Clamp { Diodes, Tvs }

/// N-channel analog input protection:
///   raw[i] ── R ──┬── safe[i]
///                 ├── Schottky to vdd  (or one TVS to gnd)
///                 ├── Schottky from gnd
///                 └── C to gnd          (RC low-pass)
pub block InputProtect<const N: usize = 8, const CLAMP: Clamp = Clamp::Diodes>
where N >= 1 && N <= 32 else "InputProtect supports 1–32 channels"
{
    port raw: [Analog<In>; N];
    port safe: [Analog<Out>; N];
    port vdd: Power<In>;
    port gnd: Ground;

    /// Limits clamp current: (12 V − 3.3 V − 0.4 V) / 1k ≈ 8 mA for a ±12 V fault.
    param r: Tol<Ohm> = 1k ± 1%;
    /// f_c = 1/(2π·r·c) ≈ 16 kHz with the defaults.
    param c: Tol<Farad> = 10nF ± 10%;

    for i in 0..N {
        let r_s[i] = Resistor { p: raw[i], n: safe[i], value: r, power: 125mW };
        let c_f[i] = Capacitor { p: safe[i], n: gnd, value: c, dielectric: C0G };
        match CLAMP {
            Clamp::Diodes => {
                let d_hi[i] = Diode { a: safe[i], k: vdd, model: models::BAT54 };
                let d_lo[i] = Diode { a: gnd, k: safe[i], model: models::BAT54 };
            }
            Clamp::Tvs => {
                let tvs[i] = Tvs { a: gnd, k: safe[i], v_rwm: 3.3V };
            }
        }
    }
}
```

Using it, with `N` inferred from the ADC's port array:

```rust
net sense: [Analog; 8];
net ain: [Analog; 8];
let j1 = Header8 { pins: sense, gnd };
let prot = InputProtect { raw: sense, safe: ain, vdd: v3v3, gnd };           // N = 8 inferred
let adc = Ads7828 { ch: ain, vdd: v3v3, gnd, bus: i2c, … };
let prot4 = InputProtect::<4, Clamp::Tvs> { raw: aux, safe: aux_safe, vdd: v3v3, gnd, r: 2.2k ± 1% };
```

- **Identity.** Paths are `prot.r_s[0]` … `prot.r_s[7]`, `prot.d_hi[3]`. Each resistor has its own knob `prot.r_s[3].value`; they are independent even though they share the param `r`, because `r` is a `Tol<Ohm>` (§6.5).
- **Rendering.** A read-only frame "`for i in 0..N`" with N columns from one template; `prot4` shares the same layout file.
- **The drawable alternative.** Without the `match`, the same block fits the flat subset with array instances:

  ```rust
  let r_s[0..N]  = Resistor  { p: raw, n: safe, value: r, power: 125mW };   // element-wise
  let c_f[0..N]  = Capacitor { p: safe, n: gnd, value: c, dielectric: C0G }; // gnd broadcast
  let d_hi[0..N] = Diode     { a: safe, k: vdd, model: models::BAT54 };
  let d_lo[0..N] = Diode     { a: gnd, k: safe, model: models::BAT54 };
  ```
  This renders as four arrayed symbols labeled `×N` and can be edited as a drawing. The clamp choice would then be two separate blocks, or a v1.1 variant mechanism (open question 12).

### 10.4 Composition: the top block

```rust
// src/main.spl
use std::prelude::*;
use crate::{amp::CeAmp, node::SensorNode, power::Buck3v3};
use cui::DcJack;

pub env temp: Celsius in -10°C..=60°C;
/// 0 = new, 1 = end of life (10 years).
pub env life: Ratio in 0..=1;

/// Board: 12 V barrel jack → audio stage, and → 3.3 V buck → sensor node.
pub block Board {
    net gnd: Ground;
    net v12;
    net v3v3;
    net audio_in;
    net audio_out;

    /// The external adapter is assumed to deliver 12 V ± 5 %.
    let j1 = DcJack { vbus: v12, gnd, v: 12V ± 5% };
    let amp = CeAmp { vcc: v12, gnd, input: audio_in, output: audio_out };
    let buck = Buck3v3 { vin: v12, vout: v3v3, gnd };
    let node = SensorNode { v3v3, gnd };
}
```

The checker finds exactly one source per rail (`j1.vbus` for `v12`, `buck.vout` for `v3v3`). The contract checks are "12 V ± 5 % ⊆ CeAmp's 12 V ± 5 %", "⊆ Buck3v3's 8–28 V" and "3.3 V ± 5 % ⊆ SensorNode's 3.3 V ± 3 %". The last one **fails**, and the fix is either a tighter buck (a better reference or 0.1 % feedback resistors in a `lot`) or a wider assumption in `SensorNode`. That is spec design §2 composition surfacing through ordinary port declarations. Knob paths: `temp`, `life` shared; `j1.v` (range); `amp.r1.value`, `buck.u1.v_ref`, `node.t_inlet.c_dec.value`, … per instance.

---

## 11. Comparison with other languages

| Language | Take | Reject | Why |
|---|---|---|---|
| **Modelica** | `connect()`'s acausal, undirected semantics (potential equal, flows sum to zero) as the meaning of a net; `parameter` → `param`; `replaceable`/`redeclare` → v1.1 `trait` refinement; units attached to types | Graphical `annotation(Placement(…))` inline in code; equation sections in the structural language; `extends` inheritance | Inline geometry makes diffs and AI context noisy (editor A2); device equations belong to the simulator; inheritance scatters ports. Modelica's unit checking is tool-dependent attributes on `Real`; ours is first-class dimension types |
| **VHDL-AMS / Verilog-AMS** | Terminals are directionless while signals have modes, which confirms roles as metadata on undirected nets; `nature … reference` → ground domains; `V(a,b)`, `I(x)` probe notation (already in spec design §4); entity/architecture ≈ I/O page/sheet | Verbose declarations, `<+` contribution statements, behavioral equations in structure, port lists | We want Rust's look and a structural core; behavioral models can be `block`s of controlled sources or SPICE |
| **atopile `.ato`** | Nominal interfaces joined in one statement (`i2c ~ sensor.i2c`, recursively member-wise); `±` and `+/-` as tokens; pin maps in part files (`signal A0 ~ pin 1`, `…TCA9548APWR.ato:19-42`); spec vs part bounds; three-valued results; auto-picking; pragma feature gates as a rollout tool | Python-style indentation; `new`; `~>` bridge chains (`led_badge.ato:123`); ⊆/⊇ decided by block kind (`ast_visitor.py:846,861`); hand-rearranged equations "for the solver" (`examples/equations/equations.ato:39-45`); `.reference` wiring; unnamed pins (`resistor.unnamed[0]`, `examples/auto-picking/auto-picking.ato:34`); no `if`, no `fn` | Not Rust; hides pins; invisible semantics; the dependency problem (v2 A2); boilerplate that the guide itself warns about |
| **Spade** | Standalone Rust-like language; `fn` vs instantiable unit kinds (`spade-parser/src/lib.rs:1927-1958`); `where … else "msg"`; const generics with arithmetic; exactly-once checking (as role counting per net); rustc-quality structured diagnostics; the compiler pipeline (logos lexer, hand-written recursive descent, HIR, unification-based inference; `ARCHITECTURE.md`); named-argument shorthand | Clocks, `reg`, pipelines, `&`/`inv &` wire types, `set`; `inst` keyword (`token.rs:88`); `$(…)` named arguments (`stdlib/mem.spade:229`); `gen if` marker; sequential `let` with `decl` forward declarations | Digital-specific; our struct literal already marks instantiation and names every field; every `if` is `gen` here; order independence removes `decl` |
| **Zener / pcb** | Rust toolchain; `io()`/`config()` split = `port`/`param`; pin-binding dict `pins = {"VCC": vcc}` = our struct literal; typed nets `Power(…)`; `NotConnected()` = `nc`; values with min/max | Starlark (D7); schematic placement in trailing comments (`# pcb:sch R1 x=… y=…`) | Placement in the source file couples geometry edits with code diffs and is easy for the AI to clobber; we use a sidecar |
| **PolymorphicBlocks (edg)** | Ports with roles *and* electrical parameters; links computing voltage from the source, limits by intersection and current by sum, with `require`s (`VoltagePorts.py:17-106`); one source per link; I2C link rules: one controller, ≤ 1 pull-up set, unique addresses, frequency intersection (`I2cPort.py:9-56`); abstract blocks + refinements; generators; port arrays | Python embedding; point-range (interval) propagation; mixins; implicit-connect scopes; `gpio.request('led')` automatic pin allocation (for now) | Embedding blocks GUI round-trips: its own tutorial lists array connects, refactoring, delete and mixins as "not supported with graphical edit actions" (`getting-started.md:344,431,551,607`). Our engine tracks correlation (affine forms) |
| **SKiDL** | ERC from pin electrical types (KiCad-matrix style); plain netlist generation | `&`/`|` series/parallel operators, `+=` connection, Python host, no units | Chain-like and host-bound |
| **tscircuit** | Components as composable units (React); auto-layout from code | JSX/TypeScript; stringly-typed values (`resistance="10k"`); placement props in code (`schX`, `pcbX`); CSS-selector references (`".R1 > .pin1"`) | Not Rust, untyped values, geometry in code, stringly references break renames |
| **Chisel / Amaranth** | Bundles and `Vec` → interfaces and arrays; `Flipped` / `Signature.flip()` → the boundary flip; Chisel's `Analog` type uses `attach()` (n-ary, undirected) rather than `:=`/`<>`, which is exactly our nets; Amaranth `wiring.connect` checks that each member has exactly one `Out` among the connected interfaces → role conflict rules | Host embedding (Scala, Python); `<>` bulk connect; clocked semantics | Host baggage; directional connect operators don't fit undirected nets |

---

## 12. Alignment with the other threads, and proposed changes to existing documents

### 12.1 Editor-mapping thread (`language_editor_mapping.md` §10)

| Req. | Status | How |
|---|---|---|
| R1 explicit names, instantiation distinct from calls | **Met, differently** | An instance is `let name = Type { … }`: braces, not parentheses, and only as a `let` right-hand side (§5.2 rule 6). The resolver classifies the type as part/block. No `inst` keyword, for the Rust look. If the editor needs a purely syntactic marker, `inst` is the fallback (open question 3) |
| R2 deterministic index paths | Met | Indexed lets, enum keys, source-order elaboration (§7.2) |
| R3 net-centric canonical statements | **Proposed alternative** | Element-centric bindings are canonical (each pin appears once, in its instance). `net` statements stay first-class and keyed; net joins are `net x = [a, b];`. Trade-offs in §5.3 |
| R4 every statement keyed | Met, stricter | All flat-subset statements are keyed. One namespace per block for all kinds, so a name means one thing |
| R5 order independence | Met | Block bodies are sets of declarations; `let`s may refer forward; value cycles are errors (a relaxation of Rust `let`, §12.3) |
| R6 no action at a distance | Met | No global nets, no implicit ground binding (§4.6), `env` the only global state, params only through the instance literal |
| R7 typed pins and roles | Met | §4 |
| R8 nominal interfaces, member access, arrays, slices, concatenation | Met | §4.4, §4.5 |
| R9 units, inferred literals, `± 1%`, `?`, `from`, doc comments as data | Met | §6; `from` adopted as proposed (§6.8); `margin` belongs to the spec thread |
| R10 knobs explicit or by rule | Met | §6.6; correlation is a keyed `lot` value rather than a `matched(…)` statement |
| R11 contract inside the block | Met (hook) | §4.7 port parameters; statements are the spec thread's |
| R12 drawability decidable | Met | §3.8, including vectorized array instances |
| R13 spans and provenance | Met (compiler requirement) | Every model entity keeps its span; values keep literal provenance and loop-index dependence |
| R14 lossless CST, idempotent formatter | Met (implementation) | rowan-style CST, as in v1 §7.6 plus rust-analyzer practice |
| R15 no geometry in code | Met | Attributes are semantic only; roles are the only presentation-adjacent data |
| R16 part pins → symbols and SPICE order; sub-units | Met | `pins {}`, `model … pins: […]`, `#[unit]` bundle ports (§3.3) |
| R17 error-tolerant elaboration | Met (implementation) | §8.7 |
| R18 complete rename | Met | No dynamic names except index paths |
| R19–R21 | Met | `param` vs `port`; array instances of blocks; file = module |
| nc / tie markers (their open question 14) | Met, differently | `nc` is a binding value keyed by its instance; ties are a `NetTie` part or a `net` join |

The editor thread's strawman also bundles ground into power ports (`PowerIn` with `.pos`/`.gnd`); §4.6 explains why this thread proposes a separate `Ground`.

### 12.2 Proposed changes to existing documents

| Document | Change | Reason |
|---|---|---|
| `../specs.md` §6, §12 | Ranges `4.5V..6.5V` → `4.5V..=6.5V` | `..` stays Rust's half-open integer range (§6.4) |
| `../specs.md` §2, §12 | Allow block assumptions and guarantees to live on ports (`port vcc: Power<In> { v: 12V ± 5% }`); `assume`/`spec` statements may desugar to them | The role fixes the contract direction; composition checks fall out of bindings (§4.7) |
| `../specs.md` §13.2 | Bench blocks are ordinary blocks that use `#[sim_only]` sources and instantiate the design | No new construct needed |
| `../specs.md` §13.3 | Probes should name declared nets | Nets are always named and stable (§5.4) |
| v1 notes §7.3 | `10k +- 1%` → `10k ± 1%` / `10k +/- 1%` | `+-` is valid Rust with another meaning |
| v2 Part E.2 | `env temp: -40°C ..= 85°C` → `env temp: Celsius in -40°C..=85°C`; `matched(r1, r2, ratio)` → `lot(…)` values | Typed env items; correlation as a keyed value |
| v1 D6 / §6.2 | "Drawn vs coded" becomes "flat subset vs full language"; both stored as code | §3.8 |

### 12.3 Where the language deliberately departs from Rust

All three accept more than Rust or restrict naming; none change the meaning of valid Rust.

1. **Order-independent block bodies**: `let` may refer forward (Rust items behave this way; Rust `let` does not).
2. **Omitting defaulted fields** in struct literals without `..` (Rust RFC 3681 requires `..`).
3. **Indexed lets** `let r[i] = …` and array instances `let r[0..N] = …` (not valid Rust).

Restrictions: instances can't be shadowed, and part/block literals appear only as `let` right-hand sides.

---

## 13. Open questions

1. **Name and file extension.** `.spl` here, `.ckt` in the editor thread; `.spicy` is taken by netlists.
2. **`?` or `_` for solver-chosen values?** `_` is Rust's "infer this" (`Vec<_>`) and would be semantically apt; `?` is established in every design doc and reads as "unknown" to EEs. This proposal keeps `?`.
3. **`inst` or not.** The struct-literal form relies on the resolver to tell instances from data. Is that enough for the editor's keyed-statement extraction, or do we add Spade's `inst`?
4. **Canonical connectivity (editor R3).** Element-centric (this thread) or net-centric (editor thread). A joint prototype on the walkthrough amplifier and the buck, measuring edit locality and diff size, would settle it.
5. **Default tolerance for bare values**: project default class (proposed) or ideal?
6. **Series elements.** Which parts join power domains by default, and is `Resistor { series: true }` the right escape hatch?
7. **Analog drive rules.** Should `Analog<Out>` ports carry an output-impedance contract that tier 3 checks against loads, and should two `Out`s be an error rather than a warning?
8. **MCU pin muxing.** How a part declares alternate functions (`PB0` as SDA or MOSI) and how a design selects them, without PolymorphicBlocks' automatic allocation.
9. **The interface rule language.** How much of `where`/`require`/`count`/`unique`/`pulled` is exposed to users, and whether custom link rules are allowed in v1.
10. **Negative rails.** `Power<Out> { v: -12V ± 5% }` relative to ground (proposed), or a separate convention?
11. **Solved values.** Stored only in `Spicy.lock`, or accepted into the source as committed literals with `from ?` provenance?
12. **Variants and deep overrides.** DNP, population options and per-instance structural variation without turning drawn blocks into generated ones.
13. **Behavioral models in-language.** Controlled-source `block`s as part models, vs SPICE subcircuits only.
14. **Distribution syntax.** `.dist(Uniform)` as a method, or a field on the part (`value_dist:`)?
15. **Dimension-polymorphic user functions** (`fn par<D>(a: Quantity<D>, b: Quantity<D>) -> Quantity<D>`).
16. **Unicode or ASCII as the formatter's default**, and `4k7` vs `4.7k`.
17. **Provenance display.** How unreviewed, AI-extracted part parameters (`#[provenance(reviewed = false)]`) surface in verdicts.
18. **Multiple ground domains.** Is `#[ground(gnd_iso)]` per port enough for isolated designs, or do we need domain-typed signals?

---

## Appendix A. Grammar sketch

```
file        = { attr* item }
item        = use | mod | block | part | fn | const | enum | type | env | signal | interface
block       = vis? "block" IDENT generics? where? "{" { io_decl } { body_stmt } "}"
part        = vis? "part" IDENT generics? "{" { io_decl | pins | model } "}"
generics    = "<" gparam { "," gparam } ">"
gparam      = "const" IDENT ":" type [ "=" expr ]
where       = "where" wclause { "," wclause }
wclause     = expr [ "else" STRING ]
io_decl     = doc* attr* ( "port" IDENT ":" type [ "{" field_inits "}" ] [ "=" expr ] ";"
                         | "param" IDENT ":" type [ "=" expr ] ";" )
body_stmt   = doc* attr* ( "net" IDENT [ ":" type ] [ "=" "[" exprs "]" ] ";"
                         | "let" IDENT [ "[" expr "]" ] [ ":" type ] "=" expr ";"
                         | "for" pattern "in" expr "{" { body_stmt } "}"
                         | "if" expr "{" { body_stmt } "}" [ "else" ( if_stmt | "{" … "}" ) ]
                         | "match" expr "{" { pattern "=>" ( "{" { body_stmt } "}" | expr ) "," } "}"
                         | contract_stmt )                 // spec thread
pins        = "pins" "{" pin_map { "," pin_map } "}"
model       = "model" [ IDENT ] "=" expr ";"
expr        = Rust expressions, plus:
              quantity literals · "±" / "+/-" · "?" · "nc" · "in" (claims) · "from" (derivation)
```

**Precedence**, high to low: field, index, call, method → unary `-` `!` → `*` `/` `%` → `+` `-` → `±` → `..=` `..` → comparisons and `in` → `&&` → `||` → `from`.

## Appendix B. Keywords

- **New:** `block`, `part`, `port`, `param`, `net`, `env`, `signal`, `interface`.
- **Contextual:** `role`, `pins`, `model`, `domain`, `from`, `nc`.
- **From Rust, same meaning:** `pub`, `use`, `mod`, `crate`, `self`, `super`, `fn`, `const`, `enum`, `type`, `let`, `for`, `in`, `if`, `else`, `match`, `where`, `true`, `false`.
- **Reserved:** `trait`, `impl` (v1.1 block signatures), `inst`.
- **Spec thread:** `require`, `assume`, `spec`, and whatever else it defines.

## Appendix C. Implementation notes

- Lexer: `logos` (as Spade, `externals/spade/spade-ast/src/token.rs`), with quantity literals as single tokens and `±`/`+/-` as one token.
- Parser: hand-written recursive descent over a lossless rowan CST (editor R14), error-recovering (R17).
- Name resolution → HIR → type inference: unification for bundle and role types; Abelian-group unification for dimensions.
- Elaboration: const evaluation, loops, instance arrays → the canonical model (instances, nets, knobs, expression DAG with spans and provenance).
- Net checker (tier 2) → early interval pre-check of exact level rules (tier 3 fast path) → engine.
- Incremental compilation with `salsa` once projects grow (v1 §7.6).
