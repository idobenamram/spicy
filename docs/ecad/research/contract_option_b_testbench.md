# Contract Option B: Testbench-First

> 2026-09-29 · One of three contract designs for the comparison (A, B, C). This one takes the **testbench-first** philosophy: setups are the primary objects, as in Cadence ADE tests, UVM, VHDL-AMS portless testbenches and Modelica test models.
> Reads with: `research/contract_references.md` (cited "refs §n"), `research/contract_ee_practice.md` ("ee §n"), `research/contract_hierarchy.md` ("hier §n"), `research/contract_tool_gaps.md` ("gaps"), `language.md` v0.1, `engine_types.md` (M3a), `research/next_synthesis.md` (L/T/E/M findings).
> **Where the numbers come from.** **[S]** = simulated in ee (contract_ee_practice.md). **[H]** = simulated in hier (contract_hierarchy.md). **[T]** = simulated for this report on ngspice-42, on the ee §4 board decks, by the scripts in `/root/.claude/jobs/443154a8/tmp/engine4/b/` (`fault.py`, `fault2.py`, `fault3.py`, `fault5.py`, `hotplug.py`). These use nominal part values, so they are single points, not verdicts. **[DS]** = datasheet, as cited in ee §7. **[M]** = from memory, not re-read for this report.

---

## Summary

In this option **the bench is the unit of meaning**. A block declares only its **interface**: its ports, and on each port the numbers its role owns and publishes (an input publishes the load it presents, an output publishes its source impedance, a supply input publishes its current draw). Everything else is a **bench**: a named setup that places the block with instruments (a `Supply`, a `Signal`, a `Load`, or a fixture such as a USB host with its cable) and holds the specs that are checked in it. Every block has one special bench, **`home`**. It is the datasheet's header, "test conditions unless otherwise noted". Its instruments are written as ranges, and those ranges are the block's **envelope**: what any neighbour must stay inside. That replaces `assume`. The word goes away because the thing it described is now a physical object on the bench. Other benches start from `home` and override a few fields, exactly like a datasheet row. Most are one line, because the standard setups come from **bench templates** written for a **socket** (a named port signature such as `Regulator` or `AmpStage`). Any block that fits the socket gets the standard benches, and the I/O page lists the ones it hasn't configured yet. Hierarchy follows UVM: **checks travel, stimulus stays home**. When a block is placed, its own benches are not re-run. Their cached verdicts carry over if the parent proves, at every connection and in both directions, that the neighbours' published numbers lie inside the block's `home` envelope. If a proof fails, the engine re-runs only that block's benches under the conditions the parent really provides. The block's `home` specs also ride along as monitors on the parent's own runs, at no extra cost. Parent-level specs are simulated on the parent's benches, or composed from cached two-port tables where that is exact. Faults are bench stimuli with triggers (a cable that opens for 0–100 µs), so they get tolerances, corners and "during/after" windows like everything else. On the §4 reference board the design is 8 blocks and one USB fixture, 19 block benches (11 of them one-line template instances), and 9 board benches, including a fault, a brown-out, a reversed connector and a hot-plug one-off. The CE amp MVP file changes by three lines. The main weaknesses: specs are spread across benches (the spec table becomes a generated view), the `home` bench does two jobs (default setup and envelope), templates add a generic-like concept to a deliberately small language, and board-level specs still cost flat simulation.

---

## 1. The philosophy and its core concepts

### 1.1 The philosophy in one paragraph

A spec means nothing without its setup: "gain 10.09 ± 3%" is a different claim into 10 kΩ than into 470 Ω + 100 nF, and ee §1.3 shows the CE amp's gain falling from 4.59 to 2.88 when the load changes. So this option makes the setup the thing you write first and name, and puts each spec inside the setup it is measured in. It is what an engineer does in the lab: put the board on the bench, connect the supply, the signal generator and the load, then write down what the scope must show. The block stays pure design (parts, nets, ports). A bench never edits the block; it only surrounds it, the way a VHDL-AMS testbench is an entity with no ports (refs §10.1) and an ADE test is a separate cell (refs §3). Reuse comes from templates (the ADE "copy the test" pain, G3, solved by generics over a port signature), and from inheritance (the datasheet's "unless otherwise noted", refs §6). Hierarchy uses the one rule every working reference agrees on (refs §16, §18.1 item 9): a child's **checks** travel into the parent, its **stimulus** stays home, and its **conditions** become obligations on its neighbours.

### 1.2 The core concepts

| Concept | Written as | What it is | Reference precedent |
|---|---|---|---|
| **Block** | `block Ldo3v3: Regulator { … }` | The design: ports, parts, nets. No setup, no specs | language §2.1 (unchanged) |
| **Published characteristic** | `port output: Analog<Out> { z_src: ..=5Ω over 20Hz..=1kHz };` | A number the block promises its neighbours, on the port. Only fields the port's role owns (v0.1's `#[by(Source)]` / `#[by(Sink)]`). Checked automatically by a standard port bench | Liberty pin attributes; IBIS; v0.1 §3.2 |
| **Socket** | `socket Regulator { port vin: Power<In>; … standard [LineReg, …]; }` | A named port signature. Blocks that fit it get its standard benches | JEDEC standard test methods; ADE test reuse; UVM agents per interface |
| **Bench** | `bench psrr for Ldo3v3: Psrr { … }` | A named setup: the block under test (DUT), instruments on its ports, analysis settings, measures and specs | ADE test; UVM test; SysML `verification def` |
| **Home bench** | `bench home for Ldo3v3 { … }` | The block's header setup. Its port instruments, written as ranges, are the block's **envelope** | Datasheet header conditions; ADE global variables |
| **Instrument** | `Supply { … }`, `Signal { … }`, `Load { … }` | Bench-side models of what the world connects: a source with its impedance and disturbances, a load with ranges and steps | Lab bench; ee §1.6 |
| **Fixture** | `#[fixture] block UsbHost { … }` | A reusable bench-side block (a USB host and cable, an electronic load). Never in the product BOM. May declare its own events and faults | ADE `analogLib`; UVM agent |
| **Bench template** | `pub bench LoadStep<D: Regulator> { param from: Amp; … pub let dip = …; }` | A generic bench over a socket. Defines setup and measures, never limits | UVM sequences; Modelica `extends` |
| **Monitor** | the specs in a block's `home` bench | They travel: re-evaluated on every parent bench that exercises them | UVM passive monitors; SVA `bind`; Modelica_Requirements |
| **Event / fault** | `event vbus.droop at 100us;` `fault vbus.unplug at 100us for 0us..=100us;` | A timed disturbance or topology change inside a transient bench. Its timing parameters are knobs | Simscape faults; gaps P15 |
| **Window** | `.during(f)`, `.after(f, 1.5s..)` | A measure axis tied to a fault's timing | language §8.6 `.window()` |
| **Plan** (optional) | `plan SensorBoard { #[req("SYS-4")] require home.code_gain; tier quick: home; }` | Traceability and run tiers. References specs; never copies a limit | ADE Verifier (without its second copy of the limit) |

### 1.3 What replaces `assume`

`assume` did two jobs (refs §18.3): it made a range knob for the block's specs, and it put an obligation on the neighbours. In this option both jobs belong to the **`home` bench's instruments**:

```rust
// v0.1                                    // Option B
contract CeAmp {                           bench home for CeAmp {
    assume temp in -10°C..=60°C;               temp: -10°C..=60°C;
    assume vcc.v in 12V ± 5%;                  vcc: Supply { v: 12V ± 5% };
    assume output.z_load >= 10kΩ;              output: Load { r: 10kΩ.. };
    …                                          …
}                                          }
```

- **Job 1, the range knob:** every range in `home` is a range knob, "for all" as before (language §8.6).
- **Job 2, the obligation:** when the block is placed, the parent must show that what really connects to each port lies inside the `home` instrument on that port. The neighbour's published numbers are compared with it, both ways (§4.2).

Why this reads better: an instrument is a physical noun. `vcc: Supply { v: 12V ± 5%, z: ..=1Ω }` says what is connected, which is what an engineer draws on a test-setup sketch. The quantifier needs no keyword, because a range in a bench is always "for every value in here". And the role rule of v0.1 becomes a split with no exceptions:

| Port side | The block **publishes** on the port (its role owns it) | The `home` bench **supplies** (the other role owns it) |
|---|---|---|
| signal input `Analog<In>` | `z_load`: the load it presents (its input impedance), leakage `i` | `Signal { wave, z }`: what drives it, with its source impedance and disturbances |
| signal output `Analog<Out>` | `v` (its range), `z_src`: its output impedance | `Load { r, c, i }`, `Load::Series`, `Load::Model(Block)` |
| power input `Power<In>` | `i`: its current draw; optionally `z_load`, the impedance it presents (a new field on v0.1's `Power`) | `Supply { v, z, ripple, droop, ramp }` |
| power output `Power<Out>` | `v`, `i_max`, `z_src` | `Load { i, c }`, including steps |

The compiler enforces it, the way v0.1's `E-role` did:

```
error[E-role]: a block publishes only what its port's role owns
 --> gain_stage.spl:4:30
  |
4 |     port input: Analog<In> { z_src: ..=10kΩ };
  |                              ^^^^^ `z_src` belongs to whatever drives `input`
  = help: describe the source in the home bench: `input: Signal { z: ..=10kΩ };`
```

### 1.4 Loads on outputs; disturbances belong to the source

The lead asked whether an input's disturbance is part of the "source". The physics says yes (ee §1.6). A load is something that draws current depending on voltage: a resistor, a capacitor, the ADC's sampling capacitor. A source sets a voltage through an impedance. When "the previous stage misbehaves" (noise, ripple, a droop, a slow edge, a wrong DC level, a weak drive), what changes is the **signal that arrives**, so it is a property of the source. Putting it on the input as a "load" would add an impedance that changes the block, which is wrong physics.

This option writes it the same way:
- **Outputs get a `Load`** (ranges, families, steps, or another block's input model).
- **Inputs get a `Signal` or a `Supply`** that carries its disturbances: `Supply { v: 4.40V..=5.25V, z: 0.1Ω..=0.5Ω, ripple: Ripple { pp: 100mV, over: 100Hz..=1MHz }, droop: Droop { depth: 330mV, edge: 1us } }`.
- A **fixture** bundles a real source with everything it can do wrong. `UsbHost` (§3.9) is the USB port and cable, with its voltage range, cable resistance, ripple, droop, and its two failure modes (unplugged, reversed). "The source owns its misbehaviour" becomes a reusable object.
- The block's own input impedance is **published on its port**, because it is what this block does to its source. The only inputs that need a load model in the *upstream* bench are the dynamic or nonlinear ones (ee §1.6 exceptions): the ADC's sampling capacitor, a clamp diode. For those the upstream bench writes `Load::Model(AdcInput)`.

### 1.5 The three rules the whole design rests on

1. **Every bench starts from `home`.** Field precedence is: port-type defaults < `home` < template < the bench's own fields. That is the datasheet header and its per-row overrides (ee §2.8), and ADE's global → test variables (refs §3.1).
2. **Every bench stays inside `home`.** A field that pins a condition must lie inside the `home` range for it, or the bench must say `#[beyond(field, reason = "…")]`, which the report lists. This is the check no tool has (G4, refs §18.2).
3. **`where` narrows; a new bench changes the circuit.** `spec … where vout.i >= 5mA` picks a sub-box of the same bench's knobs: the same deck, a subset of the same rows. A different excitation, a step element or a different load family is a new bench. These are the two kinds of "different setup" ee §2.8 found, and they cost differently.

---

## 2. The standard library this option relies on

Written in the language itself, like v0.1's signal types (language §3.3). These are sketches.

### 2.1 Sockets

```rust
// std/sockets.spl
/// A voltage regulator: raw supply in, regulated supply out.
pub socket Regulator {
    port vin:  Power<In>;
    port vout: Power<Out>;
    port gnd:  Ground;
    standard [LineReg, LoadReg, Dropout, Psrr, LoadStep, LineStep, StartUp];
}
/// A passive or active filter on a supply rail.
pub socket RailFilter { port vin: Power<In>; port vout: Power<Out>; port gnd: Ground; standard [RailRejection, RailStep]; }
/// A single-ended amplifier stage with its own supply.
pub socket AmpStage {
    port vdd: Power<In>; port input: Analog<In>; port output: Analog<Out>; port gnd: Ground;
    standard [StepResponse, Swing, SupplyRejection, TwoPort];
}
/// A signal filter.
pub socket Filter { port input: Analog<In>; port output: Analog<Out>; port gnd: Ground; standard [SampledSettling, TwoPort]; }
/// A sampling converter input.
pub socket Converter { port input: Analog<In>; port vref: Power<In>; port gnd: Ground; standard []; }
/// Anything that only draws supply current.
pub socket Consumer { port vdd: Power<In>; port gnd: Ground; standard []; }
```

A block fits a socket by writing `block Ldo3v3: Regulator`. The port names must match, or be mapped: `block Buck: Regulator(vin = vbat)`. A socket is a structural signature only. It has no methods and no bounds, so it stays inside P7 ("no user traits in v1").

### 2.2 Instruments

```rust
// std/bench.spl (sketch). Instruments are std parts that attach to a port by its type.
pub instrument Supply for Power<In> {
    v: Volt,                              // required: a supply input has no sensible default
    z: Ohm = 0Ω,                          // source impedance
    ripple: Ripple = Ripple::none(),      // Ripple { pp, over: band }: the AC excitation band, a sine in transient
    droop: Droop = Droop::none(),         // Droop { depth, edge, width }: an event, triggered in a bench
    ramp: Time = 0s,                      // power-up ramp: an event, `plug`
    ac: Volt = 0V,                        // set by templates that move the AC excitation onto the supply
}
pub instrument Signal for Analog<In> {
    wave: Wave = Wave::Ac,                // Ac | Dc(v) | Sine { amp, freq } | Step { from, to, edge, at } | Band { … }
    z: Ohm = 0Ω,
}
pub instrument Load for Analog<Out> | Power<Out> {
    r: Ohm = open, c: Farad = 0F,         // shunt R ∥ C to ground
    i: Draw = Draw::dc(0A),               // a current sink: a range, Draw::step(from, to, edge:, at:) or Draw::pulse(…, width:)
}
pub instrument Load::Series for Analog<Out> { r: Ohm, c: Farad }   // R then C to ground (an RC filter input)
pub instrument Load::Model<B>  for Analog<Out> | Power<Out>          // the neighbour's input, as a block
pub instrument Signal::Model<B> for Analog<In>                        // a block as the driver
```

- **The same family names describe published input impedances.** `z_load: Load::Series { r: 470Ω ± 1%, c: 100nF ± 10% }` on a port says "my input looks like this family". `Load::Sampled { r, c, rate, window }` is the family for a switched-capacitor input (an ADC). Using one vocabulary for "what a bench connects" and "what a block presents" is what makes containment a parameter comparison (§4.2).

- **Alternatives** are written with `|`, as in Rust patterns: `Load::Series { … } | Load { … }`. They become a **mode knob**: both families are checked.
- **A port with no instrument** gets its type's default: `Analog<In>` gets `Signal { wave: Ac, z: 0Ω }`, `Analog<Out>` gets an open load, `Power<Out>` gets no load. A `Power<In>` port with no `Supply` is an error with a fix-it.
- **Probes on instruments.** `sensor.emf` is the signal generator's open-circuit voltage, and `sensor.v` is the pin voltage. That's ee §1.4's "gain from where?" made concrete: the source is an object in the bench, so both are nameable.

### 2.3 Bench templates

A template is a generic bench over a socket. It sets up the circuit and defines measures. It never sets a limit: limits are design intent, and they belong to the block's own bench (refs §18.2: "one keyword for set and check" is atopile's mistake).

```rust
// std/benches/regulator.spl (two shown in full; the rest are listed)

/// Supply rejection: the AC excitation moves onto vin (ee §2.4, TLV755P PSRR rows).
pub bench Psrr<D: Regulator> {
    param vin_at: Volt;                    // the datasheet fixes the input and the load
    param load: Amp;
    vin:  Supply { v: vin_at, ac: 1V };
    vout: Load { i: Draw::dc(load) };
    excite: vin;                           // which source carries the AC analysis
    pub let rejection = -ac(vout.v / vin.v).db();      // a curve over frequency
}

/// Load transient: a current pulse on vout (ee §2.4, TLV755P Fig. 5-8).
pub bench LoadStep<D: Regulator> {
    param from: Amp; param to: Amp; param edge: Time;
    vout: Load { i: Draw::pulse(from, to, edge: edge, at: 100us, width: 100us) };
    tran: 0s..=300us;
    let v = tran(vout.v);
    let v0 = v.at(99us);
    pub let dip  = v0 - v.window(100us..=200us).min();
    pub let bump = v.window(200us..=300us).max() - v0;
}

// LineReg { at }        dc_sweep of vin over its home range at a fixed load   → pub let reg
// LoadReg { over }      dc_sweep of vout.i over a range                      → pub let slope
// Dropout { at, drop }  #[beyond(vin)]: sweeps vin below the home range      → pub let dropout
// LineStep { step, edge }, StartUp { ramp }                                   → pub let dev, t_up, peak
```

A template's measures are named, so an instance's spec can be one line: `spec dip <= 50mV;` is the canonical short form of `spec dip: dip <= 50mV;`, used when the spec takes the measure's name.

### 2.4 Standard port benches (for published characteristics)

Each published field has a std bench that measures it under `home` conditions. The user never writes these; the report lists them as `Block.home.<port>.<field>`.

| Published field | Std bench | Method |
|---|---|---|
| `z_src` on an output | `ZOut` | null the signal, inject 1 A AC into the port (ee §1.5, injection) |
| `z_load` on an input | `ZIn` | measure v/i at the port over the band; for a family (`Series { r, c }`), check the curve lies inside the family's envelope |
| `i` on a power input | `SupplyDraw` | the DC (or averaged) current into the port, over every `home` corner |
| `v` on an output | `Level` | `dc(port.v)` over every `home` corner |

So a published characteristic **is** a spec, with a fixed method and one home. A bench spec on the same quantity (`spec zo: output.z_src …`) is an error, with the fix-it "publish it on the port". The limit is stored once (G2).

### 2.5 The characterization bench `TwoPort`

For any socket with one `Analog<In>` and one `Analog<Out>`, the std `TwoPort` bench runs two AC analyses per corner (input driven with the output open; input shorted with 1 A into the output). This gives the block's ABCD matrix at every frequency (hier §2.1). It has no specs. Its cached table is what a parent composes to compute its own AC specs without simulating the child (§4.4).

---

## 3. The reference board, written in full

The board is ee §4: USB 5 V → 3.3 V LDO → op-amp gain stage → RC anti-alias filter → MCU ADC, with an MCU load on the rail. Values and ranges are ee §4.3's. One file per block, as a real project would have them.

```
                        ┌──────────────────────────────── SensorBoard ─────────────────────────────────┐
   bench home:          │                                                                               │
   UsbHost fixture ─────┼─● vbus ─[ldo: Ldo3v3]── v3 ──┬──────────[vdda: VddaFilter]── va ──┬──────┐     │
   (4.40–5.25 V,        │  c_bus 4.7µF                 │ c_dec 100nF                        │      │     │
    cable 0.1–0.5 Ω,    │                         [mcu: McuLoad] 5–30 mA                    │      │vref │
    ripple, droop,      │                                                                   │      │     │
    unplug, reversed)   │         ┌─────────── amp: GainStage ───────────┐                   │      │     │
   Signal (sensor) ─────┼─● sensor┤ c_in ─ inp ─(+)Mcp6001 ─ output ────├─ oa ─[aa: AaFilter]─ adc ─[conv: AdcInput]
   100 Ω–10 kΩ,         │         │  r_b ─ vmid [bias: MidRef]  (−) r_f/r_g/c_g │  470 Ω / 100 nF    code = 4096·adc/va
   ≤ 100 mV, 20 Hz–1 kHz│         └──────────────────────────────────────┘                                   │
                        └───────────────────────────────────────────────────────────────────────────────┘
```

### 3.1 Two IC parts (abridged)

Part records follow language §6.8. Only what the benches need is shown.

```rust
// parts/tlv755p.spl
#[source(datasheet = "https://www.ti.com/lit/ds/symlink/tlv755p.pdf", extracted = "ai", reviewed = false)]
pub part Tlv755p {
    port vin: Power<In>; port vout: Power<Out>; port gnd: Ground;
    operating    { vin.v in 1.45V..=5.5V; temp in -40°C..=125°C; }
    absolute_max { vin.v in -0.3V..=6.5V; }                     // [M]: from the abs-max table, not re-read
    v_ref: 1.2025V ± 1%,                                        // the ±1% knob of ee §4.3
    model: spice("models/ee_board.lib", subckt: "ldo", params: { vref: v_ref }, fidelity: behavioral),
}

// parts/mcp6001.spl
pub part Mcp6001 {
    port inp: Analog<In>; port inn: Analog<In>; port vdd: Power<In>; port vss: Ground; port out: Analog<Out>;
    operating    { vdd.v in 1.8V..=6.0V; }
    absolute_max { vdd.v in -0.3V..=7.0V; inp.v in vss.v - 0.3V..=vdd.v + 0.3V; }   // [M]
    v_os: 0V ± 4.5mV,
    model: spice("models/ee_board.lib", subckt: "opamp", params: { vos: v_os }, fidelity: behavioral),
}
```

A part's `operating { }` block is the part-level equivalent of a `home` envelope. Parts come from datasheets and have no benches, so they keep v0.1's form.

### 3.2 `Ldo3v3`: a regulator with the standard benches

```rust
// ldo3v3.spl
/// 5 V → 3.3 V rail around a TLV755P-class LDO. Rated like the part: up to 500 mA.
pub block Ldo3v3: Regulator {
    port vin:  Power<In>  { i: ..=vout.i + 50uA };                       // load plus ground current (25 µA typ)
    port vout: Power<Out> {
        v: 3.3V ± 2%,
        i_max: 500mA,
        /// Light loads make the output impedance peak (ee §3.3): 3.2 Ω at 1 mA, 9.2 Ω at 100 µA.
        z_src: ..=1Ω over 10Hz..=1MHz where vout.i >= 5mA,
    };
    port gnd: Ground;

    let u1    = Tlv755p   { vin, vout, gnd };
    let c_in  = Capacitor { a: vin,  b: gnd, value: 1uF ± 10% };
    let c_out = Capacitor { a: vout, b: gnd, value: 1uF ± 10%, esr: 5mΩ };
}

/// The header: what any board may connect to this regulator.
bench home for Ldo3v3 {
    temp: 0°C..=70°C;
    vin:  Supply { v: 3.6V..=5.5V, z: ..=0.5Ω };
    vout: Load   { i: Draw::dc(0.1mA..=500mA), c: ..=10uF };
}

bench line  for Ldo3v3: LineReg  { at: 30mA;                spec reg <= 5mV; }
bench load  for Ldo3v3: LoadReg  { over: 0.1mA..=500mA;     spec slope <= 0.1V/A; }
bench drop  for Ldo3v3: Dropout  { at: 500mA; drop: 1%;     spec dropout <= 250mV; }
bench psrr  for Ldo3v3: Psrr     { vin_at: 4.3V; load: 50mA; spec rejection.band(100Hz..=1MHz) >= 36dB; }
bench step  for Ldo3v3: LoadStep { from: 5mA; to: 30mA; edge: 1us;  spec dip <= 50mV; spec bump <= 50mV; }
bench lstep for Ldo3v3: LineStep { step: 1V; edge: 1us;     spec dev <= 20mV; }
bench start for Ldo3v3: StartUp  { ramp: 100us;             spec t_up <= 1ms; spec peak <= 3.4V; }
```

How this reads against the datasheet: `home` is the TLV755P header ("VIN = …, IOUT = …, unless otherwise noted"), each one-line bench is a row with its test conditions, and the published `vout` is the accuracy row. The bench names are the row names. What ee §4.4 simulated for these rows: line 1.0 mV, load 56 mV/A, dropout 215 mV, PSRR 56.7 / 40.0 / 50.0 dB at 1 kHz / 100 kHz / 1 MHz, step −19.9 / +16.4 mV, line step +9.3 / −8.3 mV, start 385 µs with a 3.299 V peak **[S]**. ee simulated them at nominal parts over VIN 4.40–5.25 V; the wider `home` range here is this option's choice, and the engine would check it.

The `where vout.i >= 5mA` on `z_src` is ee §4.4's lesson: as a block guarantee for every load down to 100 µA it fails, because the output impedance peaks at light load. The published number carries its condition, as a Liberty table carries its load range. A parent must then show that its rail never carries less than 5 mA (§4.2).

### 3.3 `VddaFilter`: a small block

```rust
// vdda_filter.spl
/// RC filter for the analog rail (ST AN2834). Corner 1.59 kHz.
pub block VddaFilter: RailFilter {
    port vin:  Power<In>  { i: vout.i, z_load: Load::Series { r: 10Ω ± 1%, c: 10uF ± 10% } };
    port vout: Power<Out> { v: vin.v - (0V..=5mV), z_src: ..=10.2Ω };
    port gnd:  Ground;
    let r_fa = Resistor  { a: vin,  b: vout, value: 10Ω ± 1% };
    let c_fa = Capacitor { a: vout, b: gnd,  value: 10uF ± 10% };
}

bench home for VddaFilter {
    vin:  Supply { v: 3.3V ± 3%, z: ..=1Ω };
    vout: Load   { i: Draw::dc(0A..=0.4mA) };          // 10 Ω × 0.4 mA = 4 mV ≤ the published 5 mV drop
}
bench reject for VddaFilter: RailRejection { spec attenuation.at(10kHz) >= 14dB; }   // 15.1 dB at R −1 %, C −10 % (arithmetic)
```

### 3.4 `MidRef`: no socket needed

```rust
// mid_ref.spl
/// Mid-rail bias: vmid = vdd/2, high impedance, filtered.
pub block MidRef {
    port vdd: Power<In>   { i: ..=20uA };                       // 3.3 V / 200 kΩ = 16.5 µA
    port mid: Analog<Out> { v: vdd.v / 2 ± 1%, z_src: ..=51kΩ };
    port gnd: Ground;
    let r_m1  = Resistor  { a: vdd, b: mid, value: 100k ± 1% };
    let r_m2  = Resistor  { a: mid, b: gnd, value: 100k ± 1% };
    let c_mid = Capacitor { a: mid, b: gnd, value: 1uF ± 10% };
}

bench home for MidRef {
    vdd: Supply { v: 3.3V ± 3% };
    /// The bias feeds a CMOS input through R_b and a coupling cap: no DC current, some AC.
    mid: Load::Series { r: 400kΩ.., c: ..=1uF };
}
```

A tiny block needs a `home` bench and its published numbers. Nothing else. The `Load::Series` family matters. The real load (R_b 470 kΩ, then C_in 220 nF to the sensor) carries no DC current, but it does carry AC: about 100 mV / 470 kΩ ≈ 0.2 µA. A DC-only load description would miss that, and the containment check would be proving the wrong thing.

### 3.5 `GainStage`

```rust
// gain_stage.spl
/// Non-inverting op-amp stage, gain 1 + R_f/R_g = 10.09, AC-coupled in, DC gain 1 (C_g).
pub block GainStage: AmpStage {
    port vdd:    Power<In>   { i: ..=150uA };                              // Iq 100 µA + MidRef 16.5 µA
    port input:  Analog<In>  { z_load: 450kΩ.. over 20Hz..=1kHz };         // R_b sets it: 470–473 kΩ [S]
    port output: Analog<Out> { v: 25mV..=vdd.v - 25mV, z_src: ..=5Ω over 20Hz..=1kHz };   // 3.0 Ω at 1 kHz [S]
    port gnd:    Ground;

    net inp; net inn; net ng; net vmid;
    let bias = MidRef    { vdd, gnd, mid: vmid };
    let c_in = Capacitor { a: input, b: inp,  value: 220nF ± 10% };
    let r_b  = Resistor  { a: inp,   b: vmid, value: 470k ± 1% };
    let u2   = Mcp6001   { inp, inn, vdd, vss: gnd, out: output };
    let r_f  = Resistor  { a: output, b: inn, value: 9.09k ± 1% };
    let r_g  = Resistor  { a: inn,    b: ng,  value: 1k ± 1% };
    let c_g  = Capacitor { a: ng,     b: gnd, value: 47uF ± 20% };
}

bench home for GainStage {
    vdd:    Supply { v: 3.3V ± 3%, z: ..=12Ω, ripple: Ripple { pp: 10mV, over: 100Hz..=1MHz } };
    input:  Signal { wave: Sine { amp: ..=100mV, freq: 20Hz..=1kHz }, z: ..=10kΩ };
    /// Either an RC filter behind its own series resistor (the resistor isolates the op-amp
    /// from the capacitor, ee §3.4), or a plain high-impedance input.
    output: Load::Series { r: 470Ω ± 5%, c: ..=220nF } | Load { r: 10kΩ.., c: ..=1nF };

    let h = ac(output.v / input.v);                  // pin to pin: unchanged by the source impedance (ee §1.4)
    spec gain:   h.at(1kHz).mag() in 10.09 ± 3%;     // 10.089 into 10 kΩ ∥ 60 pF, 10.106 into 470 Ω + 100 nF [S]
    spec f_low:  h.f_low(-3dB)  <= 5Hz;
    spec f_high: h.f_high(-3dB) >= 50kHz;            // 60.6 kHz into 470 Ω + 100 nF, 94.6 kHz into 10 kΩ [S]
}

bench step  for GainStage: StepResponse    { amp: 50mV;           spec overshoot <= 10%; }       // 2.4 % [S]
bench swing for GainStage: Swing           { f: 1kHz; amp: 100mV; spec thd <= 1%; }
bench psrr  for GainStage: SupplyRejection {                      spec rejection.band(100Hz..=100kHz) >= 60dB; }   // 66 dB [S]
```

The load alternatives are the stage's promise about what it can drive. `step` is checked over both families and every value in them, so the 100 nF-direct case that rings at 69% (ee §3.4) is simply outside the promise. If a board connects it, containment fails at that connection, with both lines shown.

### 3.6 `AaFilter`

```rust
// aa_filter.spl
/// Anti-alias RC: 470 Ω / 100 nF, f_c = 3.39 kHz. The 470 Ω also isolates the op-amp from the 100 nF.
pub block AaFilter: Filter {
    port input:  Analog<In>  { z_load: Load::Series { r: 470Ω ± 1%, c: 100nF ± 10% } };
    port output: Analog<Out> { z_src: ..=480Ω };
    port gnd:    Ground;
    let r_aa = Resistor  { a: input, b: output, value: 470Ω ± 1% };
    let c_aa = Capacitor { a: output, b: gnd,   value: 100nF ± 10% };
}

bench home for AaFilter {
    input:  Signal { z: ..=10Ω };                    // it must be driven stiffly: a source impedance adds to R (ee §2.3)
    output: Load::Model(AdcInput);                   // the real ADC input is the load
    let h = ac(output.v / input.v);
    spec f_c:   h.f_high(-3dB) in 3.39kHz ± 15%;
    spec alias: (h.at(1kHz).mag() / h.at(25kHz).mag()).db() >= 15dB;     // 17.7 dB [S]
}

/// Settling with the sample clock: only a transient with the real ADC model sees the
/// ADC's average current through R_aa (ee §2.6: the same AC plot can be −0.47 or −45 LSB).
bench drive for AaFilter: SampledSettling {
    input: Signal { wave: Step { from: 0V, to: 3.3V, edge: 1us, at: 10us }, z: ..=5Ω };
    spec error.lsb().abs() <= 1;                     // −0.47 LSB [S]
}
```

### 3.7 `AdcInput`: a model block

```rust
// adc_input.spl
/// STM32F103 ADC input, as a block so it can be a load in other benches (DS5319 §5.3.18).
pub block AdcInput: Converter {
    port input: Analog<In> {
        z_load: Load::Sampled { r: ..=1kΩ, c: ..=8pF, rate: f_s, window: t_s },
        i: -1uA..=1uA,                               // injection / leakage
    };
    port vref: Power<In> { i: 0A };                  // not modeled on the §4 board (ee's model omits the ADC's supply current)
    port gnd:  Ground;

    param f_s: Hertz = 50kHz;
    param t_s: Time = 1.125us;                       // 13.5 cycles at 12 MHz
    net cs;
    let sw   = Switch        { a: input, b: cs, r_on: 1kΩ, clock: Clock { rate: f_s, on: t_s } };
    let c_s  = Capacitor     { a: cs, b: gnd, value: 8pF, reset: Clock { rate: f_s, to: 0V } };   // worst previous channel: 0 V
    let leak = CurrentSource { p: input, n: gnd, i: -1uA..=1uA };

    /// The 12-bit result, ratiometric to vref.
    pub let code = 4096 * input.v / vref.v;
}

bench home for AdcInput {
    /// DS5319's R_AIN rule: 13.5 / (12 MHz · 8 pF · ln 2^14) − 1 kΩ = 13.5 kΩ.
    input: Signal { z: ..=13.5kΩ };
    vref:  Supply { v: 3.3V ± 3% };
}
```

`AdcInput` has no specs of its own: it is a model with an envelope. Its published `z_load` is checked by the std `ZIn` bench against its own body, so the model can't claim a load it doesn't present. `pub let code` is a **published measure**: any bench that contains the block can use it.

### 3.8 `McuLoad`: a firmware-owned range

```rust
// mcu_load.spl
/// Stands in for the MCU's digital supply current.
pub block McuLoad: Consumer {
    port vdd: Power<In> { i: draw };
    port gnd: Ground;
    #[owner(firmware)]
    param draw: Draw = Draw::dc(5mA..=30mA);
    /// Collapses with the rail below 1.8 V (brown-out). An ideal current sink would pull an
    /// unpowered rail to −18.8 V in the fault bench [T]: a load model needs a compliance limit.
    let sink = CurrentSink { p: vdd, n: gnd, i: draw, full_above: 1.8V };
}
bench home for McuLoad { vdd: Supply { v: 3.3V ± 3% }; }
```

The `#[owner(firmware)]` range is a firmware-owned knob (engine.md §2.1). A PASS that depends on it reads "PASS given draw 5–30 mA", and the range is exported as a firmware constraint. A bench may set `mcu.draw` to anything inside its declared range, including a step. A bench may drive the DUT's **range** knobs (port conditions, the environment, owned params), never its statistical ones, which are the parts.

### 3.9 `UsbHost`: the source and the cable, as a fixture

```rust
// fixtures/usb_host.spl
pub enum UsbPort { LowPower, HighPower }

/// A USB 2.0 host port and its cable, seen from the device's connector.
/// The source owns its misbehaviour: range, cable, ripple, droop, and two faults.
#[fixture]
pub block UsbHost {
    port vbus: Power<Out>;
    port gnd:  Ground;

    param kind: UsbPort = UsbPort::LowPower;
    param cable_r: Range<Ohm>   = 0.1Ω..=0.5Ω;       // VBUS + GND wires and contacts
    param cable_l: Range<Henry> = 0H;                // 0 for DC and AC benches; set for hot-plug
    net src; net rtn;

    let host = VSource {
        p: src, n: rtn,
        dc: match kind { LowPower => 4.40V..=5.25V, HighPower => 4.75V..=5.25V },   // USB 2.0 §7.2.2
        ripple: Ripple { pp: 100mV, over: 100Hz..=1MHz },                           // a design assumption (USB gives none)
    };
    let cable = Cable { a: src, b: vbus, r: cable_r, l: cable_l, ret: (rtn, gnd) };

    pub let emf = host.v;
    event droop: host.step(-330mV, edge: 1us, width: 200us);    // USB 2.0 §7.2.4.2: another device plugged in
    event plug:  host.ramp(0V, to: host.dc, over: 100us);
    fault unplug:   cable.open();                             // knocked out, or a bad contact
    fault reversed: cable.swap(vbus, gnd);                    // a miswired cable or bench lead
}
```

`event` and `fault` may appear only in `#[fixture]` blocks and in benches. A product block never fails on its own; faults are imposed on it from outside. Fault templates can still target product structure (§3.10, `reversed`).

### 3.10 `SensorBoard` and its benches

```rust
// sensor_board.spl
/// USB-powered sensor front end (ee §4).
pub block SensorBoard {
    port vbus:   Power<In>  { i: ..=100mA };                      // one unit load before configuration (USB 2.0 §7.2.1)
    port gnd:    Ground;
    port sensor: Analog<In> { z_load: 450kΩ.. over 20Hz..=1kHz }; // implied by amp.input's: checked, not trusted

    net v3; net va; net oa; net adc;
    let c_bus = Capacitor  { a: vbus, b: gnd, value: 4.7uF ± 10% };  // USB 2.0 §7.2.4.1: ≤ 10 µF at attach
    let ldo   = Ldo3v3     { vin: vbus, vout: v3, gnd };
    let c_dec = Capacitor  { a: v3, b: gnd, value: 100nF ± 10% };
    let mcu   = McuLoad    { vdd: v3, gnd };
    let vdda  = VddaFilter { vin: v3, vout: va, gnd };
    let amp   = GainStage  { vdd: va, input: sensor, output: oa, gnd };
    let aa    = AaFilter   { input: oa, output: adc, gnd };
    let conv  = AdcInput   { input: adc, vref: va, gnd };

    pub let code = conv.code;
}

/// The product's envelope: what the world connects to the board.
bench home for SensorBoard {
    confidence: worst_case;
    temp:   0°C..=70°C;
    vbus:   UsbHost { kind: LowPower };
    sensor: Signal { wave: Sine { amp: ..=100mV, freq: 20Hz..=1kHz }, z: 100Ω..=10kΩ };

    /// Codes per mV of the sensor's EMF: source loading (R_b vs R_s), stage gain,
    /// filter droop and V_REF all in one number. Exists only at board level.
    let g = ac(code) / ac(sensor.emf) * 1mV;
    spec code_gain: g.band(20Hz..=1kHz).mag() in 12.0 ± 7%;      // 11.35–12.71 over 256 corners × R_s × f [S]
    /// Ratiometric: V_REF's tolerance cancels; V_OS and the MidRef mismatch don't.
    spec midscale:  dc(code) in 2048 ± 20;                          // ±16.8 codes worst-case sum (ee §4.5)
    spec usb_cap:   vbus.c_total() <= 10uF;                         // structural: no simulation
    #[info] let inrush = tran(vbus.i).peak();                       // informative (424 mA with a 100 µs ramp [S])
}

/// VBUS ripple through LDO PSRR into V_REF: a path no block owns.
bench ripple for SensorBoard {
    excite: vbus;
    sensor: Signal { wave: Dc(0V) };
    let r = ac(code) / ac(vbus.emf) * vbus.ripple.pp;                // codes pp for the fixture's ripple
    spec ripple_to_code: r.band(100Hz..=1MHz).mag() <= 1;           // 0.08 LSB with the VDDA filter [S]
}

/// The MCU wakes up: its supply step moves the ADC's reference through the shared rail.
bench mcu_step for SensorBoard {
    sensor:   Signal { wave: Dc(0V) };
    mcu.draw: Draw::step(5mA, 30mA, edge: 1us, at: 100us);
    tran: 0s..=600us;
    let c = tran(code);
    spec disturbance: (c - c.at(99us)).window(100us..=600us) in 0 ± 4;   // −0.4/+2.1 with the filter; −10/+12 without [S]
}

bench droop for SensorBoard {
    sensor: Signal { wave: Dc(0V) };
    event vbus.droop at 100us;
    tran: 0s..=600us;
    let c = tran(code);
    spec droop_to_code: (c - c.at(99us)).window(100us..=600us) in 0 ± 4;  // 2047.9–2048.3 [S]
}

bench power_up for SensorBoard {
    sensor: Signal { wave: Dc(0V) };
    event vbus.plug at 0s;
    tran: 0s..=2s;
    /// Valid data within 1.5 s of plug-in; dominated by C_g·(R_f + R_g) = 0.47 s.
    spec t_valid: tran(code).window(1.5s..=2s) in 2048 ± 4;          // 2048.05 at 1.49 s [S]
}
```

The board has no `alias_rejection` spec and no `usb_current` spec. `aa.alias` is a monitor that travels from `AaFilter.home` and is evaluated on the board's AC runs (§4.3). `usb_current` is the board's published `vbus.i ≤ 100 mA`, checked by the std `SupplyDraw` bench (≈ 30.2 mA at 30 mA **[S]**). Nothing is written twice.

### 3.11 The fault: the USB cable is knocked out mid-run

What the physics says first. When VBUS disconnects, the board runs on the charge stored in C_bus (4.7 µF). The LDO keeps the 3.3 V rail up until C_bus falls to about 3.3 V plus the dropout (≈ 13 mV at 30 mA). At 30 mA that takes about 4.7 µF × (4.39 − 3.32) V / 30.3 mA ≈ 166 µs. After that the rail sags, V_REF with it, and the ratiometric code jumps, because vmid is held up by C_mid (τ = 50 ms) while V_REF falls. When VBUS comes back, the LDO's error amplifier has wound up, and the rail overshoots.

```rust
/// A knock on the cable: VBUS opens briefly, then reconnects.
/// The disconnect's length is a range knob, so the engine finds the worst one.
bench unplug for SensorBoard {
    sensor: Signal { wave: Dc(0V) };
    fault vbus.unplug at 100us for 0us..=100us;
    tran: 0s..=1.2ms;
    let c = tran(code);
    /// A knock shorter than 100 µs must not disturb a reading, during it or 1 ms after.
    spec hold: c.window(vbus.unplug.start..=vbus.unplug.end + 1ms) in 2048 ± 4;
}

/// A longer loss of power: the board browns out and must come back on its own.
bench brownout for SensorBoard {
    sensor: Signal { wave: Dc(0V) };
    fault vbus.unplug at 100us for 1ms..=10ms;
    tran: 0s..=1.7s;
    spec recover:      tran(code).after(vbus.unplug, 1.5s..) in 2048 ± 4;
    /// The STM32's VDD must stay in its operating range (≤ 3.6 V [M]) as the LDO recovers.
    spec no_overshoot: tran(v3.v).after(vbus.unplug).max() <= 3.6V;
}

/// A miswired cable or bench lead: VBUS and GND swapped from t = 0.
#[expect(fail, reason = "the §4 board has no reverse-polarity protection; add a series Schottky or a P-FET")]
bench reversed for SensorBoard {
    fault vbus.reversed;
    // No specs needed: every part's abs-max monitor runs in every bench, fault benches included.
}
```

What the simulations say (nominal parts, VBUS at both ends, I_mcu = 30 mA unless noted) **[T]**:

| Case | Result | Verdict it implies |
|---|---|---|
| Disconnect 20 / 50 / 100 µs, VBUS 4.40 V | code 2047.98–2048.03 / 2047.96–2048.19 / 2047.91–2048.47 | `hold` PASS at 100 µs (margin ≈ 3.5 codes) |
| Disconnect 200 µs, VBUS 4.40 V | code up to **2062.4** (+14.5), rail down to 3.176 V | would FAIL if the knob allowed 200 µs |
| Longest disconnect that keeps ±4 codes | **176 µs** at 4.40 V / 30 mA; 307 µs at 5.25 V / 30 mA; 1.03 ms at 4.40 V / 5 mA | the worst case is low VBUS and high MCU current together; the hand estimate above gives 166 µs |
| After a 1 ms disconnect | rail peaks at 3.422 V (4.40 V) and 3.456 V (5.25 V); code back within ±4 after 105 ms / 94 ms | `no_overshoot` PASS (3.456 ≤ 3.6 V) |
| After a 10 ms disconnect (full brown-out) | rail falls to 0.06 V; code back within ±4 after **674 ms** | `recover` PASS (≤ 1.5 s) |
| Same with an ideal current sink as the MCU | the rail is pulled to **−18.8 V** | not physics: the load needs its `full_above: 1.8V` compliance |
| `reversed` | VIN = −5.25 V against the TLV755P's −0.3 V abs-max **[M]** | FAIL by arithmetic; the engine confirms it with one op per corner |

The engine sweeps the tolerances on top of these. C_bus at −10% shortens the hold-up by about 10%, to ≈ 160 µs by the hand formula. That is still above 100 µs, but it is exactly the kind of margin the corners must confirm.

### 3.12 The one-off: hot-plug through a real cable

An edge case that no template covers. Plugging into a live host puts a 5 V step through the cable's inductance into the board's ceramic C_bus. An inductor into a capacitor with little resistance is a resonant circuit, so VBUS rings up to nearly twice the step (the problem Linear Technology's AN88 describes, "Ceramic Input Capacitors Can Cause Overvoltage Transients" **[M]**).

```rust
/// One-off: hot-plug through a 0.25–1 m cable (0.5–2 µH) with a fast contact edge.
/// Written free-form: the bench places the board itself, like a VHDL-AMS portless testbench.
#[beyond(board.vbus, reason = "a 10 ns contact edge through a cable inductance: faster than the home `plug` ramp")]
#[expect(fail, reason = "needs a TVS on VBUS, or a bulk electrolytic whose ESR damps the ringing")]
bench hot_plug {
    net vbus; net gnd: Ground; net sensor; net src;
    let board = SensorBoard { vbus, gnd, sensor };
    let host  = VSource { p: src, n: gnd, wave: Step { from: 0V, to: 5.25V, edge: 10ns, at: 1us } };
    let cable = Cable   { a: src, b: vbus, r: 0.1Ω..=0.5Ω, l: 0.5uH..=2uH };
    sensor: Signal { wave: Dc(0V), z: 1kΩ };
    tran: 0s..=60us;
    spec ringing: tran(vbus.v).max() <= 6.0V;           // TLV755P abs-max 6.5 V [M], less a margin
}
```

Simulated **[T]**: VBUS peaks at **8.45 V / 8.95 V / 9.33 V** with 0.5 / 1 / 2 µH at 0.1 Ω, and 5.35 / 5.91 / 6.63 V at 0.5 Ω. The peak inrush current is 4.9–12.9 A. The one-off FAILs, which is the point: it finds a real board defect that the templated benches never exercise.

How it stays checked: a free-form bench is a **parent context**. The board placed in it gets the same containment checks as in any parent (§4.2). Here they would fail on `vbus`, so the bench must carry `#[beyond(…)]`, and the report lists it among the out-of-envelope benches. The same bench could be written as `bench hot_plug for SensorBoard { vbus: UsbHost { cable_l: 0.5uH..=2uH, … }; … }`. The free form is there for fixtures that don't fit an instrument at all.

### 3.13 The spec table, as the editor generates it

Specs live in benches, but the I/O page shows them as one datasheet-style table. The "Setup" column is generated from `home` plus the bench's overrides:

| Spec path | Setup (generated) | Limit | Confidence | Verdict |
|---|---|---|---|---|
| `GainStage.home.output.z_src` (published) | ZOut; home | ≤ 5 Ω, 20 Hz–1 kHz | sigma(3) | … |
| `GainStage.home.input.z_load` (published) | ZIn; home | ≥ 450 kΩ, 20 Hz–1 kHz | sigma(3) | … |
| `GainStage.home.gain` | home: vdd 3.3 V ± 3 %, input ≤ 100 mV, load RC or ≥ 10 kΩ | 10.09 ± 3 % | sigma(3) | … |
| `GainStage.step.overshoot` | StepResponse 50 mV; home loads | ≤ 10 % | sigma(3) | … |
| `GainStage.psrr.rejection` | SupplyRejection; AC on vdd | ≥ 60 dB, 100 Hz–100 kHz | sigma(3) | … |
| *(standard, not configured)* | `AmpStage` also lists `Swing`: configured | — | — | — |

The last kind of row is the socket's checklist. A `Regulator` with no `StartUp` bench shows "StartUp: not configured". That turns "analyses skipped because nothing asked for them" (the Aerospace finding behind G4) into something visible.

### 3.14 The CE amp MVP file

```rust
// circuits/ce_amp.spl, Option B
/// Common-emitter audio stage (walkthrough §1).
block CeAmp {
    port vcc: Power<In>;
    port gnd: Ground;
    port input: Analog<In>;
    port output: Analog<Out>;

    net base;
    net emitter;

    /// Divider holds the base near 2.1 V.
    let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };
    let r2 = Resistor { a: base, b: gnd, value: 10k ± 1% };
    let rc = Resistor { a: vcc, b: output, value: 4.7k ± 1% };
    let re = Resistor { a: emitter, b: gnd, value: 1k ± 1% };
    let c_in = Electrolytic { p: base, n: input, value: 1uF ± 20% };
    let q1 = Npn { c: output, b: base, e: emitter, beta: 100..=300 };
}

bench home for CeAmp {
    temp: -10°C..=60°C;
    vcc: Supply { v: 12V ± 5% };

    let h = ac(output.v / input.v);

    /// Room for the output to swing ±1 V without clipping.
    spec bias: dc(output.v) in 4.5V..=6.5V;
    /// The next stage expects this level.
    spec gain: h.at(1kHz).mag() in 4.6 ± 5%;
    /// Don't cut the bass.
    spec bass: h.f_low(-3dB) <= 30Hz;
}
```

Against roadmap §4.2, three lines change: `contract CeAmp` → `bench home for CeAmp`, and the two `assume` lines become fields. `input` and `output` take the port-type defaults (a 1 V AC source with 0 Ω, and an open output). The editor shows them greyed out as part of the header, like a datasheet's "unless otherwise noted". The engine sees exactly what M3 expects: the same 8 knobs, 3 specs and 5 sides, one bench (`BenchId(0)`), and the same deck as plan §1.4. The simple case stays simple, and the MVP doesn't need a socket, a template or a published characteristic.

---

## 4. Hierarchy

### 4.1 What travels when a block is placed

```
   child: GainStage                                   placed in SensorBoard as `amp`
   ────────────────                                   ────────────────────────────────
   ports + published   ─────── travel as DATA ──────► compared with the neighbours' home envelopes (both ways)
     (z_load, z_src, i, v)
   home envelope       ─────── travels as OBLIGATION ► every neighbour's published numbers must lie inside it
     (Supply, Signal, Load)
   home specs          ─────── travel as MONITORS ───► re-evaluated on the board's own runs (free)
     (gain, f_low, f_high)
   other benches       ─────── STAY HOME ────────────► their cached verdicts carry over if every obligation holds
     (step, swing, psrr, TwoPort, the published-number benches)
   TwoPort table       ─────── travels as a CACHE ───► used by the board to compose its own AC specs
```

This is UVM vertical reuse (refs §8.3), made analog:
- **Stimulus stays home.** The child's drivers (its benches' instruments) are not instantiated in the parent. In the parent, the real neighbours drive the child.
- **Checks travel.** The `home` specs are monitors. So are the automatic checks (abs-max, derating) of every part inside the child. A monitor is evaluated in a parent bench whenever that bench exercises it:
  - a DC monitor (`bias`-type) in every bench with an operating point;
  - an AC ratio monitor (`gain`) in an AC bench whose excitation reaches the child's input (checked by the engine: the ratio's denominator must not be ≈ 0);
  - a transient monitor in a transient bench.

  Otherwise the monitor is reported as **not exercised** in that bench, never as a PASS (G14).
- **Assumptions become assertions.** The `home` envelope is checked against the context. This is the SVA `assume → assert` step at integration (refs §8.2).

Why `home` specs travel and other benches' specs stay: the `home` bench describes the block as it is used, and its specs are its everyday behaviour. The other benches are special tests (a moved excitation, an injected current, a step element). They mean nothing under a parent's stimulus, so they are unit tests.

### 4.2 Proving the child's conditions at each connection, both ways

At every net where two blocks meet, the engine builds **containment checks** from v0.1's ownership rule (language §3.2), now with benches:
- **Forward (source → sink):** the driver's published source-side fields (`v`, `z_src`, disturbances) must lie inside the sink's `home` `Signal` / `Supply`.
- **Backward (sink → source):** the sinks' published sink-side fields (`z_load`, `i`, summed over the net) must lie inside the driver's `home` `Load`.
- **Environment:** the parent's `temp` (and other `env` knobs) must lie inside each child's `home` range. Inside the parent it is one shared knob (the join, §4.4).

Containment is interval or family arithmetic on published numbers. It costs **zero runs**, because the published numbers were verified by the neighbours' own std port benches (cached).

**`amp` (GainStage) in SensorBoard**, all nine checks:

| # | Connection | Published by the neighbour (verified) | GainStage `home` requires | Result |
|---|---|---|---|---|
| 1 | vdd ← `vdda.vout` (fwd) | v = v3 − (0..5 mV), with v3 = `ldo.vout.v` 3.3 V ± 2 % → 3.229–3.366 V | 3.3 V ± 3 % = 3.201–3.399 V | ✓ |
| 2 | vdd ← `vdda.vout` (fwd) | z_src ≤ 10.2 Ω (plus the LDO's ≤ 1 Ω, whose `where vout.i >= 5mA` holds: `mcu.draw` ≥ 5 mA) → ≤ 11.2 Ω | z ≤ 12 Ω | ✓ |
| 3 | vdd ripple (fwd) | VBUS 100 mVpp through the LDO's rejection, then the filter | ≤ 10 mVpp | **not provable from published numbers**: see §4.3 |
| 4 | vdd draw → `vdda` (bwd) | amp i ≤ 150 µA + conv i = 0 → ≤ 150 µA | VddaFilter home load ≤ 0.4 mA | ✓ |
| 5 | input ← board port `sensor` (fwd) | the board's home Signal: z 100 Ω–10 kΩ, ≤ 100 mV, 20 Hz–1 kHz | z ≤ 10 kΩ, ≤ 100 mV, 20 Hz–1 kHz | ✓ (the parent's own conditions flow down) |
| 6 | output → `aa.input` (bwd) | Series { r: 465.3–474.7 Ω, c: 90–110 nF } | Series { r: 446.5–493.5 Ω, c: 0–220 nF } | ✓ (same family: parameter containment) |
| 7 | output → `aa.input` (fwd) | amp z_src ≤ 5 Ω | AaFilter home: input z ≤ 10 Ω | ✓ |
| 8 | board port `sensor` publishes z_load ≥ 450 kΩ | amp.input z_load ≥ 450 kΩ | (a parent claim implied by a child's) | ✓ |
| 9 | temp | board 0–70 °C | GainStage has no `temp`: inherits the project's range | ✓ |

With checks 1–2 and 4–9 passing, GainStage's cached verdicts for `step`, `swing`, `psrr`, `TwoPort` and its published numbers carry over. The report records what each one rests on (`Record.rests_on`, next_synthesis R1):

```
SensorBoard · amp: GainStage                           cache gs-3f9a41c0 (0 runs)
  step.overshoot      PASS (all corners)  rests on: containment 1, 2, 6, 7, 9
  psrr.rejection      PASS (all corners)  rests on: containment 1, 2, 9
  home.gain           monitor on board runs: 10.106 (standalone 10.089 … 10.106)
  home.f_high         monitor on board runs: 60.6 kHz
  containment 3       vdd ripple: context re-run of ldo.psrr, see below
```

**Why both directions matter, with the numbers.** Check 6 is the one that makes the gain claim sound. hier §2.2 measured what happens without it: the CE amp's standalone guarantee, reused with no load check, predicts a two-stage gain of 19.10–23.33 where the truth is 12.27–13.93. With a load range as a bench knob plus this check, the prediction encloses the truth. With the wrong range (≥ 10 kΩ against a real 7.4–8.1 kΩ load) the check FAILs, and that is what catches the error. Check 7 is the source-side twin: hier §2.3's filter checked with an ideal source predicts a corner of ≥ 3040 Hz, where the truth goes down to 2101 Hz.

**When families differ.** `ldo.vout` has a `home` load of `Load { i, c ≤ 10 µF }`, a shunt capacitor. On the board it really sees c_dec (100 nF) plus the VddaFilter's input, which is a 10 µF capacitor *behind* 10 Ω (a `Load::Series`). A series RC is not a shunt C, so parameter containment can't decide. The engine does not guess. It takes the next rung, below.

### 4.3 The context re-run: the parent as the child's bench

When a containment check fails or can't be decided, the child's verdicts don't carry over. The engine doesn't jump to a flat simulation of the board, though. It builds the child's **context bench**: the same child benches, with the instruments on the failed ports replaced by what the parent really provides.

```
   Ldo3v3 · bench step, as written                      Ldo3v3 · bench step, in SensorBoard's context
   ┌──────────────────────────────────┐                 ┌──────────────────────────────────────────────┐
   │ vin : Supply 3.6–5.5 V, z ≤ 0.5 Ω │                 │ vin : UsbHost 4.40–5.25 V, cable 0.1–0.5 Ω,   │
   │ vout: Load  pulse 5→30 mA,        │    ───────►     │       c_bus 4.7 µF ± 10 %   (from the board)   │
   │       c ≤ 10 µF (shunt)           │                 │ vout: Load pulse 5→30 mA                        │
   │                                   │                 │       + c_dec 100 nF + Load::Model(VddaFilter  │
   └──────────────────────────────────┘                 │         with its published sinks)              │
        cached verdict: doesn't apply here              └──────────────────────────────────────────────┘
                                                          LDO-sized runs: ~64 transients, not the board's
```

- Which benches re-run: only those whose measures depend on the failed port. For `ldo.vout`'s load family that is `home.vout.z_src`, `step` and `start`. `line`, `load`, `drop` and `psrr` are DC sweeps or fixed points, so their verdicts still carry over.
- **Containment 3 (ripple)** is the same move. The LDO's `psrr` spec was measured at a fixed point (4.3 V, 50 mA), as datasheets do. A spec measured at one point can't discharge an obligation over the board's range (VIN 4.39–5.25 V, 5–30 mA). So the engine re-runs `Ldo3v3.psrr` with `vin` and `vout` taken from the context. ee measured 40.0 dB at 100 kHz and 56.7 dB at 1 kHz at the datasheet point **[S]**. If the context re-run confirms ≥ 36 dB over the band, the ripple at v3 is ≤ 100 mV × 10^(−36/20) ≈ 1.6 mVpp. The filter's rejection only helps, so it is ≤ 1.6 mVpp at va, which is inside GainStage's 10 mVpp. That discharges check 3, resting on the re-run.
- **The context re-run is sound** only when the context conditions are themselves verified. They are: they come from neighbours' published numbers, which are specs checked by std port benches. When a condition can only come from the neighbour's full behaviour (a real transient interaction), the rung is flat simulation.

This is the "range shrinks in context" point of ee §3.1, made mechanical. The block is rated wide, and the board proves the narrow range it uses. The LDO's `z_src ≤ 1 Ω where vout.i ≥ 5 mA` is the same idea, discharged by a containment check alone.

### 4.4 Cached child results: keys, reuse, invalidation

**One cache entry per bench, not per design.** Every bench, after template expansion and `home` merge, has a `BenchKey`:

```
BenchKey = hash(  DUT definition after elaboration: parts, params, nets, part records, model file bytes
                + bench definition after expansion: instruments, fixtures, events, faults, analyses, measures, limits
                + knob ranges as physical values (the rows are keyed by value, T6)
                + policy, engine settings, backend identity )
NOT in the key: where the DUT is placed, the parent, the neighbours, other placements
```

It is hier §4.3's key, scoped to a bench. Stored as `.spicy/benches/<BenchKey>.json` (report + run table). The design's `rev` becomes the hash of its bench keys plus its containment results.

| Edit | What is invalidated | What is kept |
|---|---|---|
| A value inside GainStage (R_f) | every GainStage bench; the board benches whose cone contains `amp.r_f` | every other block's benches |
| GainStage's `home` bench | all GainStage benches (they inherit `home`); containment checks naming GainStage re-run (instant) | other blocks |
| A limit in `GainStage.step` | nothing simulated: the rows are reused, only the verdict is recomputed | everything |
| The std template `StepResponse` | every bench instantiated from it, in every block (like a library update) | benches from other templates |
| AaFilter (C_aa 100 → 82 nF) | AaFilter's benches (~60 runs); containment 6 re-checked (82 nF ⊆ 0–220 nF ✓), so GainStage stays cached; board benches whose cone contains `aa.c_aa` | GainStage, Ldo3v3, VddaFilter, MidRef, AdcInput, McuLoad |
| The MCP6001 model file | every bench of every block that contains an Mcp6001 (its bytes are in the key) | blocks without it |
| The project's temperature range | benches of blocks that inherit `temp`; blocks with their own `temp` keep their cache and get a containment re-check | blocks that state `temp` |
| Narrowing a knob's range | nothing re-runs: rows are keyed by physical value, so the narrower box is a subset of stored rows (T6) | all rows |

**Two placements share one cache.** Two `GainStage` placements with the same parameters have the same bench keys, so they are simulated once. A placement with a changed parameter (`GainStage { …, r_f: 20k }`) gets new keys for itself only. This is hier's measured payoff: one CE amp table served every placement in H1, H2 and H4 (0 new runs; H4's 2^20 corners composed in 0.3 s against about 266 s flat **[H]**).

**Joined environment knobs.** Rows store `temp` (and other `env` knobs) as physical values. When a parent uses two children's rows together, it picks rows at the same `temp`. Without this join, hier H3 reports a false alarm (6.5255 V against the 6.5 V limit), and with it the answer matches the truth (6.3646 vs 6.3640 V) **[H]**.

**Composition from `TwoPort` tables** (for the parent's own AC specs). `code_gain` can be computed without simulating the board: sensor source → `amp` two-port → `aa` two-port → `conv` load, as an ABCD product per joined row, times the V_REF ratio from `ldo`'s DC rows. hier measured this algebra as exact to about 1e-14 on linear AC measures **[H]**. It is the same arithmetic ngspice does on the flat matrix, in a different order. It is opt-in per spec (`#[evidence(composed)]`) until composition is built and trusted; the default is flat.

### 4.5 When flat simulation is still required

| Situation | Board example | Why composition or containment can't decide it |
|---|---|---|
| Specs about shared things (a rail, a reference) in transient | `mcu_step_to_code`, `droop_to_code`, `t_valid` | the MCU step moves the reference, not the signal; no block owns both (ee §4.7 item 6) |
| Faults and events | `unplug`, `brownout`, `hot_plug` | topology changes mid-run; large-signal, nonlinear |
| A 3-port path (signal plus supply) with no 3-port table yet | `ripple_to_code` (LDO PSRR → V_REF) | hier §6 q5: supply ports as ports are open |
| Any composed FAIL | — | composition proves PASS only; one flat run at the worst corner is the counterexample (hier §4.4) |
| A containment check that fails, when the context re-run needs the neighbour's full behaviour | a DC-coupled chain that moves operating points | the re-run's instruments would have to be the neighbours themselves: that is flat |
| Coupling outside the ports | thermal, shared ground return | not in any published number |
| Regression of the cache | periodic spot checks | a key can miss a change (a backend quirk) |

In all these cases the child's monitors still ride on the flat runs at no extra cost, so the flat simulation also re-checks every child's `home` specs in context.

---

## 5. What the engine does with it

### 5.1 Lowering: from benches to decks, analyses and runs

```
 .spl ──parse──► AST ──resolve──► benches (templates expanded, `home` merged, fields checked against the envelope)
                                      │
                    each bench is elaborated as a PORTLESS TOP BLOCK: the DUT is a placement named `dut`,
                    instruments and fixtures are parts. model.md's flatten runs unchanged.
                                      │
                         FlatDesign + KnobTable + FlatContract   (one per bench)
                                      │
             M1f exporter ──► EngineDeck (one per distinct topology; benches that differ only in fixed values share it)
                                      │
             adapter ──► KnobSpace (one per check; shared knobs have one id) + Plan { benches, sides, containments }
```

A bench being "a top block with no ports" is the implementation win: resolve, flatten and export need no new concept (VHDL-AMS benches are exactly this, refs §10.1).

| Bench kind | Deck | Analyses (`Needs.analyses`, next_synthesis T1) |
|---|---|---|
| `home` of a block | DUT + default or `home` instruments | Op (DC specs, published `v`, `i`); Ac with the default excitation (ratio specs) |
| std port benches (`ZOut`, `ZIn`) | DUT + a 1 A AC current source on the port, signal nulled | Ac { excitation: the injection source, sweep over the `over` band } |
| `LineReg`, `LoadReg`, `Dropout` | the `home` deck (same topology) | DcSweep { source: vin or the load sink } |
| `Psrr`, `SupplyRejection`, `ripple` | the `home` deck with `AC 1` moved to the supply | Ac { excitation: the supply } |
| `LoadStep`, `LineStep`, `mcu_step`, `droop` | `home` deck with a PULSE / PWL source | Tran { stop, step, events } |
| `StartUp`, `power_up` | `home` deck, sources start at 0 | Tran { uic: true } |
| `SampledSettling`, `AaFilter.drive` | DUT + `Load::Model(AdcInput)` with its clock | Tran { stop: n samples } |
| fault benches | `home` deck with the fault element (a smooth conductance, next_synthesis B-series) | Tran { events: [fault with a duration knob] } |
| `TwoPort` | DUT, two excitation patterns | Ac × 2 per corner (hier §2.1) |
| structural specs (`usb_cap`) | none | simulator-free (E8) |

For the LDO this gives 6 decks: op/AC, ZOut, DC sweeps, AC-on-VIN, transient steps and start-up. ee §2.4 counted about 4 distinct circuits for the TLV755P's rows, which matches, plus the injection and start-up decks.

### 5.2 How the knobs are affected

| Knob kind | Declared in Option B by | In the child's own benches | In the parent | Key and join |
|---|---|---|---|---|
| **Environment** (`temp`, `life`) | the top `home` bench (project envelope); a child `home` may narrow it | Range knob, origin `Env` | the **same** knob for every child, one value per run | stored in every row; the **join key** across children |
| **Per-placement part knobs** | `± tol` in blocks, part records | Statistical, path `u1.v_ref` | path `ldo.u1.v_ref`; independent per placement (E16) | rows; each placement picks its own row |
| **Interface knobs** | `home` instruments on ports (`vin.v`, `vout.i`, `output` load `r`, `c`, `input.z`) | Range knobs, origin `Interface { port, qty }` | **gone**: bound to the neighbour (flat) or discharged by containment | not a join key: determined by the neighbour |
| **Load-family alternatives** | `A \| B` in an instrument | `KnobKind::Mode(["Series", "Shunt"])` | gone, as above | — |
| **Bench-local knobs** | template params given as ranges; free-bench values (`cable.l: 0.5uH..=2uH`) | Range, origin `Bench { param }`; exist in that bench only | not visible to other benches | in that bench's key only |
| **Fault and event knobs** | `fault … for 0us..=100us`; event times given as ranges | Range, origin `Fault { id, param }`; the fault itself is on for the whole bench | a fault bench of the parent is a new bench | in the fault bench's key; the `Record` carries the fault id (for FMEA) |
| **Owned knobs** | `#[owner(firmware)] param draw` | Range, owner `Firmware` | stays a knob (the board still doesn't know the firmware) | verdict "PASS given …", exported as a constraint |
| **Measure axes** | `dc_sweep(vin.v)`, `.band()`, `.window()`, `.during()` | not knobs: axes inside one run | — | — |

A knob changes kind across a level exactly as hier §4.5 predicts. The child's `vdd.v` is a range knob in `GainStage.home`, and in SensorBoard it is bound to `vdda.vout`, a function of `ldo.u1.v_ref`, the LDO's load and `temp`. `temp` stays a knob at every level, which is why it is the join key.

### 5.3 How many runs, roughly

Counts are corners per side (2^|cone|) plus about 20% for the inside-the-box guards, with the M3 capacitor cone rule. `temp` is in every cone even though the behavioural models ignore it (the dead-knob read-back of plan §4.4 would flag it). These are estimates, not measurements.

| Scope | Benches | Largest cone | Runs, first check | Runs after an unrelated edit |
|---|---|---|---|---|
| Ldo3v3 | home (3 published) + 7 | 7 (ZOut: temp, vin, i, v_ref, c_in, c_out, load c) | ≈ 450, of which ≈ 160 transients (≈ 3 s) | 0 (cache) |
| VddaFilter, MidRef, AdcInput, McuLoad | home + 1 | 3–5 | ≈ 100 together | 0 |
| GainStage | home (3 published + 3 specs) + step, swing, psrr, TwoPort | ≈ 14 per load family (9 part knobs, vdd, z, input z, load r/c, temp) | over the 12-knob budget: needs the topological cone rule or the loop (E4). ≈ 3–5 k AC runs (≈ 1–2 s) | 0 |
| AaFilter | home + drive + TwoPort | 4 | ≈ 60 (16 transients with the sample clock) | 0 |
| Board, AC benches (`home`, `ripple`) | flat | 10–12 | ≈ 1–2 k (ee ran 256 corners × 2 R_s × 2 f **[S]**); `code_gain` 0 if composed from cached TwoPort tables | re-run if the cone is touched |
| Board, transient benches (`mcu_step`, `droop`, `power_up`) | flat | 12–14 | ≈ 5–15 k transients: loop territory, tens of seconds to minutes | re-run if the cone is touched |
| Board, faults (`unplug`, `brownout`, `reversed`, `hot_plug`) | flat | 12–15 + fault knobs | ≈ 5–10 k; `reversed` stops at its first abs-max FAIL | re-run if touched |
| Context re-runs (`ldo.step`, `ldo.start`, `ldo.z_src`, `ldo.psrr` in context) | child-sized | 5–8 | ≈ 250 | re-run if the LDO or its context changes |

The shape matters more than the totals. Block benches are paid once and cached across every placement and every design that uses the block. Board benches are paid per board edit, and transients dominate. The CE amp MVP stays at plan §2.8's 579 runs, in one bench.

### 5.4 What changes in `engine_types.md`

Most of it was already asked for as seams in next_synthesis §8. This option fixes their shape.

```rust
pub struct Plan {
    pub benches: Vec<BenchPlan>,                 // NEW: one per bench after expansion
    pub sides: Vec<Side>,
    pub measures: Vec<MeasureDef>,
    pub cones: Vec<Cone>,
    pub containments: Vec<Containment>,          // NEW: zero-run checks at connections
    pub compositions: Vec<Composition>,          // NEW, later: parent specs from cached child tables
}
pub struct BenchPlan {
    pub id: BenchId,
    pub path: BenchPath,                         // "Ldo3v3.psrr"
    pub key: BenchKey,                           // the cache identity of §4.4
    pub deck: DeckId,                            // benches that differ only in fixed values share a deck
    pub knobs: Vec<KnobId>,                      // this bench's sub-space of the check's KnobSpace
    pub fixed: Vec<(KnobId, f64)>,               // points the bench pins (vin 4.3 V, load 50 mA)
    pub needs: Needs,                            // Needs moves from Plan to BenchPlan
    pub faults: Vec<FaultPlan>,
    pub beyond: Vec<(KnobId, String)>,           // #[beyond] fields, printed in the report
}
pub struct Side {
    pub id: SideId,
    pub spec: SpecPath,                          // CHANGED: "Ldo3v3.psrr.rejection" (was a bare name; M1)
    pub bench: BenchId,                          // NEW
    pub sense: Sense, pub bound: f64, pub measure: MeasureId, pub confidence: Confidence,
    pub monitor_of: Option<Placement>,           // NEW: a child's home spec evaluated here
}
pub struct Needs {
    pub analyses: Vec<AnalysisSpec>,             // CHANGED (T1)
    pub probes: Vec<Probe>, pub devices: Vec<DeviceNeed>,
}
pub enum AnalysisSpec {
    Op,
    Ac { excitation: SourceRef, points: Vec<f64>, sweep: Option<Sweep> },
    DcSweep { source: SourceRef, from: f64, to: f64, points: u32 },
    Tran { stop: f64, step: f64, uic: bool, events: Vec<EventSpec> },
}
pub struct EventSpec { pub target: ElementRef, pub action: EventAction, pub at: TimeRef, pub duration: Option<TimeRef> }
pub enum TimeRef { Fixed(f64), Knob(KnobId) }
pub struct FaultPlan { pub id: FaultId, pub kind: FaultKind, pub event: EventSpec }   // Open | Short | Swap | Fixture(name)
pub struct Request {
    pub bench: BenchId,                          // NEW (T2): part of the run-table key
    pub point: Point, pub analyses: AnalysisSet, pub tolerance: Tolerance,
    pub care: ConeId,                            // T6
}
pub struct KnobSpec {
    pub id: KnobId, pub path: Path, pub unit: Unit, pub lo: f64, pub hi: f64, pub nominal: f64,
    pub kind: KnobKind,                          // Range | Statistical(Dist) | Mode(Vec<String>)   (E5)
    pub origin: KnobOrigin,                      // NEW (M2)
    pub scale: Scale,                            // Linear | Log   (M4)
}
pub enum KnobOrigin { Env, Part { placement: Path }, Interface { port: Path, qty: PortQty },
                      Bench { param: String }, Fault { id: FaultId, param: String }, Owned(Owner) }
pub struct Containment {
    pub at: NetPath, pub dir: Direction,         // Forward | Backward | Env
    pub published: (Path, PortQty, Envelope),    // interval, or a family with parameter ranges
    pub required:  (Path, PortQty, Envelope),
    pub result: ContainmentResult,               // Holds | Fails | Undecidable(FamilyMismatch | PointSpec)
}
pub struct Record { /* as T9, plus */ pub bench: BenchId, pub fault: Option<FaultId>,
                    pub rests_on: Vec<Evidence>, pub exercised: bool }
pub enum Evidence { Containment(usize), CachedBench(BenchKey), ContextRerun(BenchId), Composed(usize) }
```

The store moves from `.spicy/checks/<rev>.json` to `.spicy/benches/<BenchKey>.json`, plus a small per-design index.

### 5.5 Two specs, filled in

**Spec 1: `Ldo3v3.psrr.rejection` (AC, excitation on the supply, two fixed points).**

```rust
// knobs of this bench, inside the check's KnobSpace
KnobSpec { id: k0, path: "temp",          kind: Range,                     origin: Env,                                   lo: 273.15, hi: 343.15 }
KnobSpec { id: k1, path: "u1.v_ref",      kind: Statistical(TruncNormal3), origin: Part { placement: "" },                lo: 1.190475, hi: 1.214525 }
KnobSpec { id: k2, path: "c_in.value",    kind: Statistical(TruncNormal3), origin: Part { placement: "" },                lo: 0.9e-6, hi: 1.1e-6 }
KnobSpec { id: k3, path: "c_out.value",   kind: Statistical(TruncNormal3), origin: Part { placement: "" },                lo: 0.9e-6, hi: 1.1e-6 }
KnobSpec { id: k4, path: "vout.load.c",   kind: Range, scale: Log,         origin: Interface { port: "vout", qty: C },    lo: 1e-12, hi: 10e-6 }
KnobSpec { id: k5, path: "vin.v",         kind: Range,                     origin: Interface { port: "vin", qty: V },     lo: 3.6, hi: 5.5 }    // pinned below
KnobSpec { id: k6, path: "vout.load.i",   kind: Range, scale: Log,         origin: Interface { port: "vout", qty: I },    lo: 1e-4, hi: 0.5 }   // pinned below

BenchPlan {
    id: BenchId(7), path: "Ldo3v3.psrr", key: BenchKey(0x6c1e…), deck: DeckId(3),
    knobs: [k0, k1, k2, k3, k4],
    fixed: [(k5, 4.3), (k6, 0.050)],                       // inside home: 4.3 ∈ 3.6–5.5 V, 50 mA ∈ 0.1–500 mA ✓
    needs: Needs { analyses: [Ac { excitation: "V_vin", points: [], sweep: Some(100 Hz..=1 MHz, 50/dec) }],
                   probes: [v(vout), v(vin)], devices: [] },
    faults: [], beyond: [],
}
Side { spec: "Ldo3v3.psrr.rejection", bench: BenchId(7), sense: Min, bound: 36.0 /* dB */,
       measure: m_rej /* min over band of −20·log10|v(vout)/v(vin)| */, confidence: Sigma(3.0), monitor_of: None }
Request { bench: BenchId(7), point: ε over (k0..k4), analyses: { ac_sweep }, tolerance: Engine, care: cone(k0..k4) }
```

```
* Ldo3v3 · bench psrr (std::Psrr) · engine deck
* k0 = temp  k1 = u1.v_ref  k2 = c_in.value  k3 = c_out.value  k4 = vout.load.c   (fixed: vin.v 4.3 V, vout.load.i 50 mA)
.param k0=25 k1=1.2025 k2=1e-6 k3=1e-6 k4=3.16e-9
.temp {k0}
V_vin   vin  0 DC 4.3 AC 1          ; bench: Supply on vin, the AC excitation
I_load  vout 0 DC 50m               ; bench: Load { i: 50mA }
C_load  vout 0 {k4}                 ; bench: Load { c }
X_u1 vin vout 0 ldo vref={k1}
C_c_in  vin 0 {k2}
C_c_out vout co {k3}
R_c_out_esr co 0 5m
```

Cone: 5 knobs → 32 corners + guards ≈ 40 runs. `k4` is a log knob (M4): a linear midpoint of 5 µF would be meaningless for 1 pF–10 µF. In SensorBoard this bench's containment on `vin` / `vout` is `Undecidable(PointSpec)`, so the context re-run (§4.3) creates `BenchId(31) "SensorBoard/ldo.psrr@context"`. It has the same measure, `k5` and `k6` as range knobs over the context's 4.39–5.25 V and 5–30.4 mA, and its own key.

**Spec 2: `SensorBoard.unplug.hold` (transient with a fault whose duration is a knob).**

```rust
KnobSpec { id: k20, path: "vbus.unplug.duration", kind: Range, origin: Fault { id: "vbus.unplug", param: "duration" }, lo: 0.0, hi: 100e-6 }
KnobSpec { id: k21, path: "vbus.host.dc",         kind: Range, origin: Interface { port: "vbus", qty: V },           lo: 4.40, hi: 5.25 }
KnobSpec { id: k22, path: "vbus.cable_r",         kind: Range, origin: Bench { param: "cable_r" },                   lo: 0.1, hi: 0.5 }
KnobSpec { id: k23, path: "mcu.draw",             kind: Range, origin: Owned(Firmware),                              lo: 5e-3, hi: 30e-3 }
KnobSpec { id: k24, path: "c_bus.value",          kind: Statistical(TruncNormal3), origin: Part { placement: "" },   lo: 4.23e-6, hi: 5.17e-6 }
//   … plus ldo.u1.v_ref, ldo.c_in, ldo.c_out, c_dec, vdda.r_fa, vdda.c_fa, amp.bias.r_m1/r_m2/c_mid, temp: ≈ 16 in all

BenchPlan {
    id: BenchId(18), path: "SensorBoard.unplug", key: BenchKey(0x9a07…), deck: DeckId(11),
    knobs: [k20, k21, k22, k23, k24, …], fixed: [],
    needs: Needs { analyses: [Tran { stop: 1.2e-3, step: 100e-9, uic: false,
                   events: [EventSpec { target: "vbus.cable", action: Open, at: Fixed(100e-6), duration: Some(Knob(k20)) }] }],
                   probes: [v(adc), v(va)], devices: [] },
    faults: [FaultPlan { id: "vbus.unplug", kind: Fixture("unplug"), event: /* as above */ }], beyond: [],
}
Side { spec: "SensorBoard.unplug.hold", bench: BenchId(18), sense: Max, bound: 2052.0,
       measure: m_hold /* max over [100 µs, 100 µs + k20 + 1 ms] of 4096·v(adc)/v(va) */, confidence: WorstCase, monitor_of: None }
Side { spec: "SensorBoard.unplug.hold", bench: BenchId(18), sense: Min, bound: 2044.0, measure: m_hold_min, … }
```

```
* SensorBoard · bench unplug · the cable as a time-varying conductance (smooth 1 µs edges, so ngspice converges)
.param k20=50e-6 k21=4.825 k22=0.3 k23=17.5e-3 k24=4.7e-6 …
B_cable usb vbus I=(v(usb)-v(vbus))*((1/{k22})*(1-min(max((time-100u)/1u,0),1)*(1-min(max((time-100u-{k20})/1u,0),1)))+1e-9)
B_mcu   v3 0 I={k23}*min(max(v(v3)/1.8,0),1)
```

This is the element the [T] simulations used. The measure's window depends on a knob (`k20`), so `Program` needs a window node whose ends are knob expressions: one more reason for next_synthesis E7/E8's "measures can read knob values". The worst case sits at the corner (longest disconnect, lowest VBUS, highest draw, smallest C_bus), which is what the simulations show (176 µs hold-up at the 4.40 V / 30 mA corner vs 1.03 ms at 5 mA). The engine's enumeration finds it without being told. The records carry `fault: Some("vbus.unplug")`, so a report generator can emit an FMEA row per fault (gaps P15 item 6).

---

## 6. Escape hatches, and how they stay checked

| Hatch | Written as | What keeps it honest |
|---|---|---|
| **A one-off derived bench** | `bench cold_probe for GainStage { … }` with no template | rule 2: every field inside `home`, or `#[beyond(field, reason)]`, listed in the report |
| **A free-form bench** | `bench hot_plug { let board = SensorBoard { … }; … }` (§3.12) | it is a parent context: every block it places gets the full containment checks and monitors |
| **A raw SPICE fixture** | see below | port map declared; knob read-back; probes checked on the nominal run; file hash in the key; the envelope can't be checked, so every verdict is tagged |
| **A scratch bench** (quick experiment) | a bench with no specs: `bench scratch for GainStage { input: Signal { wave: Sine { amp: 200mV, freq: 1kHz } }; tran: 0s..=5ms; #[info] let y = tran(output.v).pp(); }` | no verdicts; `#[info]` measures only; never counted as evidence |
| **A known failure** | `#[expect(fail, reason = "…")]` on a bench or spec | tracked like v0.1; it turns into an error if it starts to PASS |
| **Point conditions** | `spec … where vout.i == 50mA` | a sub-box of the same bench (rule 3); reuses rows |

**The raw SPICE fixture.** For a vendor's eval-board deck, or a Middlebrook loop-injection fixture that nobody wants to rewrite:

```rust
/// LDO loop gain by Middlebrook injection, from the vendor's own fixture deck.
bench loop for Ldo3v3 {
    fixture: spice("fixtures/tlv755_loop.cir", dut: X_DUT, ports: { vin: "vin", vout: "vout", gnd: "0" },
                   sources: { vin: "V_IN", load: "I_LOAD" });      // named, so read-back can check them
    let t = ac(v("inj_b") / v("inj_a"));                           // measures in our language, over the deck's node names
    spec pm: t.phase_margin() >= 45°;
}
```

The fixture file supplies the circuit around the DUT. The DUT's subcircuit is still generated from our block, so the design isn't duplicated. The engine:
1. checks that each mapped port exists, and that each probe answers on the nominal run (next_synthesis B3's read-back rule);
2. reads back the mapped sources' values, and compares them with `home` where it can. Anything it can't map makes the verdict tagged `raw-fixture: envelope unchecked`;
3. hashes the file's bytes into the bench key, so an edit to the fixture makes the result `stale`;
4. still runs every part's abs-max monitor inside it.

---

## 7. Gap coverage

"Improves" means this option's design addresses the gap. "Path" means it keeps a clear route without building it now. "Doesn't" means the gap is outside what a contract syntax can fix. Rows marked *engine* are fixed by the shared engine, whatever the option.

| Gap | Verdict | How, in Option B |
|---|---|---|
| G1 No spec home | **Improves** | specs are declarations in benches; the spec table is a generated view (§3.13); every spec has a verdict |
| G2 Limit duplicated | **Improves** | published characteristics are the only home of port numbers; templates carry measures, never limits; `plan` references `SpecPath`s; child specs travel instead of being re-typed at board level (`alias`, `usb_current`) |
| G3 Benches not reusable | **Improves (the core)** | templates over sockets; benches place the block by its ports, so they survive edits inside it; one line per standard test; a user can write sockets and templates for a product family |
| G4 Bench outside the assumptions; analyses skipped | **Improves** | rule 2 (every field inside `home`, or `#[beyond]`); the socket's `standard` list shows unconfigured standard benches |
| G5 Corner explosion | *engine* | enumeration per side, cones, the loop |
| G6 Worst case is a guess | *engine*; B adds | abs-max monitors run in every bench, including faults and transients: "stress at nominal only" disappears |
| G7 Unchecked distributions | *engine* | provenance on knobs; B's `#[owner]` marks ranges nobody measured |
| G8 No correlation | *engine* + **Improves** | `env` knobs joined across benches and cached children (§4.4); lots: path |
| G9 MC cost, no proof | *engine* | — |
| G10 Sub-blocks trusted | **Improves** | containment both ways from published numbers and `home` envelopes; monitors in context; context re-run; `rests_on` in every record |
| G11 Behavioural model drift | **Path (strong)** | the benches are the equivalence criterion: a behavioural implementation of `GainStage` runs the same benches and must pass them within a band. Bench-first makes this natural; no mechanism yet |
| G12 IP handoff is paper | **Improves / path** | a block ships with its sockets, published numbers, `home`, benches, cached verdicts and `TwoPort` tables. An IP-protected block can ship benches, results and tables without its body (containment and composition still work); packaging is not designed |
| G13 No trace, no change impact | **Partly** | stable `SpecPath` (block.bench.spec); `#[req]` in `plan`; changed `BenchKey`s show which verdicts moved; `home` edits are shown as requirement edits, apart from design edits |
| G14 No "not tested" state | **Improves** | monitors report `exercised: false`; unconfigured standard benches; `#[beyond]` list; composed vs simulated evidence named |
| G15 Vendor models | **Doesn't** (partly) | a raw fixture can run a vendor eval deck; encrypted models need another backend (P5) |
| G16 Datasheet data | **Path** | templates mirror datasheet test conditions, so an imported datasheet row can become a template instance with its conditions intact, instead of a bare number |
| G17 No diffs, merges, CI | **Improves (partly)** | text; a limit edit is one line in one bench; per-bench result files diff; `--base` gating is a path |
| G18 AI verifies single points | **Improves** | the agent adds benches from templates chosen by socket; every verdict comes from the engine; agent edits to `home` are flagged as requirement changes |
| G19 Faults hand-wired | **Improves (design)** | faults are bench stimuli owned by fixtures (`UsbHost.unplug`) or generated from structure (a `PinFaults` template over a connector); timing as knobs; `.during` / `.after` windows; abs-max in every fault bench; records carry the fault id (FMEA) |

**Where this option is weak on the gaps:**
- **G1 and "specs are central".** Specs are central in the UI (the table) but not in the text. To read all of a block's specs you read its `home`, its benches and the published ports, three places, or you open the view. Option A-style spec-first designs will read better here.
- **G10 at board level.** Containment reuses the child's *unit tests*. The board's own specs still need board benches, which are flat unless a spec opts into composition. The lead's "without simulating the whole design" is fully met for child verdicts and only partly met for parent specs.
- **G13.** Bench inheritance means one `home` edit can move many verdicts. The trace shows it, but a reviewer has more to follow.

---

## 8. Strengths and weaknesses

### 8.1 Where this option shines

| Situation | Why |
|---|---|
| **The editor's form UI** | A bench *is* a form: the DUT, one instrument card per port (listed from the socket), the analysis, then a spec table. "Add standard test" is a menu of the socket's templates, and templates can carry their datasheet figure. It is ADE's test editor with the copy problem solved |
| **A novice user** | "A bench is what you set up on your lab bench: supply, signal generator, load, scope." No `assume`, no quantifiers, no role theory up front. The socket's checklist tells a novice which standard tests an LDO needs |
| **Library IP** | A block ships like a datasheet: `home` is the header, benches are the rows, published ports are the pin table, and cached tables let a parent reuse it without its body |
| **Quick experiments** | A bench with no specs is an LTspice-style setup. It costs nothing to write, and it can grow into a checked bench by adding a line |
| **Faults and events** | They are stimuli, and benches are where stimuli live. Fixtures own their failure modes, so the USB host brings its unplug and reversal with it |
| **Reuse across a product family** | One `Regulator` socket's benches serve every regulator. A company writes its own `SensorChannel` socket once |
| **Diffs of limits** | A limit change is one line in one bench |

### 8.2 Where it hurts

| Situation | Why | Mitigation |
|---|---|---|
| **Seeing a block's promises at a glance** | spread over published ports, `home` and N benches | the generated spec table; `spicy bench expand Ldo3v3.psrr` prints a fully expanded bench |
| **`home` does two jobs** | narrowing `home` to debug something silently narrows the envelope, and placements then fail containment | the editor warns "this edit narrows the envelope: 2 placements now fail containment"; debug in a scratch bench |
| **Big boards** | block benches cache well, but board benches are flat, and board transients dominate (≈ 5–15 k runs per bench here) | composition for linear AC specs; the loop; `plan` tiers (a quick tier on save, a sign-off tier later) |
| **Language size** | sockets, templates, instruments, fixtures, events and faults are six new concepts, and templates are generics in a language that promised to be small (P7) | std ships most sockets and templates; users can stay with `home` plus one-off benches |
| **Template opacity** | `bench psrr for Ldo3v3: Psrr { … }` hides where the AC source is | expansion view; templates are ordinary `.spl` files the user can read |
| **AI editing** | an agent can "fix" a failing spec by softening `home` | `home` edits are requirement edits, shown apart from design edits and never auto-accepted (agent_flows G6) |
| **Diffs of conditions** | one `home` field change ripples into every derived bench | the diff view lists affected benches and verdicts |
| **Novice confusion about monitors** | "why is my gain spec evaluated on the board?" | the report shows standalone and in-context values side by side |

---

## 9. Open questions for this option

1. **`home` as the envelope, or a separate `rated` section?** Splitting them removes weakness 2 above, but it brings back a second place for conditions, closer to Option A.
2. **Which `home` specs travel.** All of them, with "not exercised" where a bench can't evaluate them (this design), or only those marked?
3. **Family containment.** Parameter containment works inside one family. Across families (a series RC against a shunt C), should the engine compare |Z(f)| envelopes, or always re-run in context (this design)?
4. **Sockets written by users in v1**, or std only until generics are designed properly?
5. **Default evidence for parent AC specs**: flat (this design) or composed where exact?
6. **Fault campaigns**: a `PinFaults { connector }` template can generate open / short / reversed faults per pin. How is the campaign budgeted against corners (faults × corners, gaps P15 item 4)?
