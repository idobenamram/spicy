# Block and Contract Syntax, Draft 4

> 2026-09-30 · The version agreed with the project lead after draft 3 (`contract_syntax_v3.md`) and a walkthrough. It supersedes drafts 2 and 3. **Status:** agreed as a good start, going to a final review round.

---

## 0. Decisions in this draft

| Decision | Choice |
|---|---|
| Interface | `block X { ports }`: ports only, no parts (not `struct`) |
| Implementation | `circuit X { parts, nets }`: one per block for now (second implementations parked) |
| Test environments | `setup S for X { … }`, defined **outside** the contract, reusable |
| Promises | `contract X { … }` |
| The default setup | `setup = Operating;` in the contract. Specs use it unless they say `on`; parents are checked against it (replaces `assume`) |
| Specs | one-liners by default; a function form (`spec name(a: A, b: B) { …; ensure …; }`) for multi-step and multi-setup specs |
| The final condition keyword | `ensure` |
| Published specs | `pub spec`: parents may rely on it (port facts such as z_out, z_in, current) |
| Internal specs | allowed: a spec may name internal nets; it's checked normally but never published or used for composition |
| Setup derivation | Rust's struct update, `..Operating`, plus field paths (`vin.wave: …`) that change one field; fields merge one by one |
| Unwritten setup fields | ideal (0 Ω source, open load); the formatter writes them out |
| Project-level items | `const` for fixed values (Rust's meaning); `env` for the shared environment ranges, which the engine searches |
| Absolute maximum | `rated`, checked on every setup, faults included |
| Modes | `mode Name { … }` inside a setup; every spec is checked in each mode; `in Name` narrows a spec |
| Differential inputs | `(a, b): Pair { dm: …, cm: …, z: … }` |
| Block-published values and events | `observe` and `emits` in the block, so contracts never reach inside a child |
| Faults | `#[fault]` setups built around an `event`; specs use `.during(ev)` / `.after(ev.end)`; may be `pub` |
| One-offs | `#[outside(reason = …)]` setups; tagged characterization; never `pub` |

---

## 1. The rules on one page

### Items

```
const NAME: Type = value;                     project: a fixed value
env name: Type in range;                      project: a shared environment range (a knob)
pub block X { port: Kind<Role>, …, observe o: Type, emits e: … }
circuit X { net n; let part = Kind { pin: net, …, field: value }; }
setup S for X { field: shape, …, mode M { … }, event e: …, window: …, ..Base }
contract X { setup = S; rated …; let m = <measure>; spec …; pub spec …; }
trait T { … }   impl T for X {}              families of blocks (std setups and specs)
```

### Specs

```
spec name: <measure> <limit> [with <pins>] [for <axis> in <range>] [on <setup>] [in <mode>];
spec name(a: SetupA, b: SetupB) { let …; ensure <measure> <limit>; }
pub spec …
```

| Part | Written | Meaning |
|---|---|---|
| Limit | `within <range>`, `<=`, `>=` | what the measured value must satisfy |
| Fixed test point | `with x = v` | this knob is pinned to one value for this spec |
| Swept axis | `for x in r` | swept inside the measurement (frequency, an input span) |
| Setup | `on S` | use setup S instead of the default |
| Mode | `in M` | only in mode M |
| In a multi-setup spec | `m[s]` | measure m in setup s |

### Multi-setup specs: the sharing rule

In one spec, every setup describes **the same board at the same moment**:
- statistical knobs (part tolerances) are always shared;
- range knobs (temperature, supply) are shared unless a setup overrides them.

### Measures

- **Kinds of simulation:** `dc(x)` steady value; `ac(x)` small-signal response vs frequency; `tran(x)` waveform vs time.
- **Reduce to one number:** `.at(f)`, `.mag()`, `.db()`, `.f_low(-3dB)`, `.f_high(-3dB, ref: dc)`, `.min()`, `.max()`, `.settle(1%)`, `.crossing(rising v)`, `.deviation()`, `.span(for x)`.
- **Time:** `.during(ev)`, `.after(ev.end)`.
- **Port quantities:** `p.v`, `p.i`, `p.z(f)`.

---

## 2. The full example

```rust
// ───────────────────────────── project.spl ─────────────────────────────
env   ambient:    Temperature in -10°C..=60°C;
env   life:       Duration    in 0y..=10y;
const confidence: Confidence  = sigma(3);

// ───────────────────────────── ce_amp.spl ──────────────────────────────
pub block CeAmp { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }

circuit CeAmp {
    net base;  net emitter;
    let r1   = Resistor     { a: vcc, b: base,    value: 47k ± 1% };
    let r2   = Resistor     { a: base, b: gnd,    value: 10k ± 1% };
    let rc   = Resistor     { a: vcc, b: output,  value: 4.7k ± 1% };
    let re   = Resistor     { a: emitter, b: gnd, value: 1k ± 1% };
    let c_in = Electrolytic { p: base, n: input,  value: 1uF ± 20% };
    let q1   = Npn          { c: output, b: base, e: emitter, beta: 100..=300 };
}

setup Operating for CeAmp { vcc: Supply { v: 12V ± 5% }, temp: ambient }

contract CeAmp {
    setup = Operating;
    let h = ac(output.v / input.v);

    spec bias:      dc(output.v)       within 4.5V..=6.5V;
    spec gain:      h.at(1kHz).mag()   within 4.6 ± 5%;
    spec bass:      h.f_low(-3dB)      <= 30Hz;
    spec base_bias: dc(base.v)         within 1.9V..=2.3V;      // internal: names a net
}

// ───────────────────────────── ldo.spl ─────────────────────────────────
pub block Ldo3v3: Regulator { vin: Power<In>, vout: Power<Out>, gnd: Ground }

circuit Ldo3v3 {
    let u1    = Tlv755p   { vin, vout, gnd };
    let c_in  = Capacitor { a: vin,  b: gnd, value: 1uF ± 10% };
    let c_out = Capacitor { a: vout, b: gnd, value: 1uF ± 10% };
}

setup Operating for Ldo3v3 {
    vin:  Supply { v: 4.3V..=5.5V, z: ..=0.5Ω },
    vout: Load   { c: ..=20uF },
    temp: -40°C..=125°C,
    mode Run   { vout.i: 5mA..=50mA }
    mode Sleep { vout.i: 0.1mA..=1mA }
}
setup LoadStep for Ldo3v3 { vout.i: Step { 5mA -> 30mA, edge: 1us }, window: 300us, ..Operating }

#[outside(reason = "datasheet dropout row: defined at the rated 500 mA")]
setup DropoutRow for Ldo3v3 { vout.i: 500mA, vin.v: Sweep(4.5V -> 3.0V), ..Operating }

contract Ldo3v3 {
    setup = Operating;
    rated vin.v within -0.3V..=6.5V;

    spec output:    dc(vout.v)                           within 3.3V ± 2%;
    spec psrr:      ac(vout.v / vin.v).at(100kHz).db()   <= -36dB   with vin.v = 4.3V;
    spec dip:       tran(vout.v).min()                   >= 3.25V   on LoadStep in Run;
    spec quiescent: dc(vin.i - vout.i)                   <= 50uA    in Sleep;
    spec dropout:   vin.v.first(vout.v < 3.267V) - 3.267V <= 250mV  on DropoutRow;   // characterization

    pub spec output_z: vout.z(f)                         <= 2Ω      for f in 10Hz..=1MHz  in Run;
}

// ───────────────────────────── gain_stage.spl ──────────────────────────
pub block GainStage<A: OpAmp = Mcp6001, const GAIN: f64 = 10.09> {
    vin: Analog<In>, vout: Analog<Out>, vdd: Power<In>, gnd: Ground,
}

circuit GainStage {
    net fb;  net mid;
    let u1  = A        { inp: vin, inn: fb, out: vout, vdd, gnd };
    let ref = MidRail  { vdd, gnd, out: mid };
    let r_f = Resistor { a: vout, b: fb,  value: ((GAIN - 1) * 10k) ± 1% };
    let r_g = Resistor { a: fb,   b: mid, value: 10k ± 1% };
}

setup Operating for GainStage {
    vin:  Signal { z: 100Ω..=10kΩ },
    vout: Load   { r: 10kΩ.., c: ..=1nF },
    vdd:  Supply { v: 3.2V..=3.4V, z: ..=20Ω },
    temp: -40°C..=85°C,
}
setup InputStep for GainStage { vin.wave: Step { 0V -> 100mV, edge: 1us }, window: 200us, ..Operating }

#[check(A = [Mcp6001, Tlv9001])]
contract GainStage {
    setup = Operating;
    let h = ac(vout.v / vin.v);

    spec gain:      h.at(1kHz).mag()          within GAIN ± 3%;
    spec bandwidth: h.f_high(-3dB, ref: dc)   >= 20kHz;
    spec settling:  tran(vout.v).settle(1%)   <= 50us     on InputStep;
    spec mid_ok:    dc(mid.v)                 within 1.6V..=1.7V;          // internal

    pub spec input_z:  vin.z(1kHz)            >= 400kΩ;
    pub spec output_z: vout.z(f)              <= 100Ω     for f in 10Hz..=100kHz;
    pub spec supply:   dc(vdd.i)              <= 150uA;
}

// ───────────────────────────── in_amp.spl (a pair) ─────────────────────
pub block InAmp { inp: Analog<In>, inn: Analog<In>, out: Analog<Out>, vdd: Power<In>, gnd: Ground }

setup Operating for InAmp {
    (inp, inn): Pair { dm: -10mV..=10mV, cm: 2.5V ± 5%, z: 350Ω ± 0.1% },
    vdd:  Supply { v: 5V ± 5% },
    temp: -40°C..=85°C,
}
setup Diff   for InAmp { (inp, inn).dm.ac: 1V, ..Operating }
setup Common for InAmp { (inp, inn).cm.ac: 1V, ..Operating }

contract InAmp {
    setup = Operating;
    spec cmrr(dm: Diff, cm: Common) {
        let a_dm = ac(out.v)[dm].at(60Hz).mag();
        let a_cm = ac(out.v)[cm].at(60Hz).mag();
        ensure db(a_dm / a_cm) >= 100dB;
    }
}

// ───────────────────────────── board.spl ───────────────────────────────
pub block Stm32Adc {
    vdd: Power<In>, ain: Analog<In>, gnd: Ground,
    observe code: Integer,                                    // the ADC reading
    emits wake: Step on vdd.i { 5mA -> 30mA, edge: 1us },     // an event a parent may trigger
}

pub block SensorBoard { usb: Power<In>, sensor: Analog<In>, gnd: Ground }

circuit SensorBoard {
    net v3v3;  net vdda;  net amp_out;  net adc_in;
    let ldo    = Ldo3v3                 { vin: usb, vout: v3v3, gnd };
    let r_filt = Resistor               { a: v3v3, b: vdda, value: 10Ω ± 5% };
    let c_filt = Capacitor              { a: vdda, b: gnd,  value: 10uF ± 20% };
    let amp    = GainStage<A = Mcp6001> { vin: sensor, vout: amp_out, vdd: vdda, gnd };
    let r_aa   = Resistor               { a: amp_out, b: adc_in, value: 470Ω ± 1% };
    let c_aa   = Capacitor              { a: adc_in,  b: gnd,    value: 100nF ± 10% };
    let mcu    = Stm32Adc               { vdd: v3v3, ain: adc_in, gnd };
}

setup Operating for SensorBoard {
    usb:    Supply { v: 4.40V..=5.25V, z: 0.1Ω..=0.5Ω },
    sensor: Signal { v: 0V..=100mV, z: 100Ω..=10kΩ },
    temp:   ambient,
    mode Measuring { mcu.state: Run }
    mode Idle      { mcu.state: Sleep }
}
setup McuWakes for SensorBoard { event: mcu.wake at 1ms, window: 5ms, ..Operating }
setup AtZero   for SensorBoard { sensor.v: 0V,    ..Operating }
setup AtFull   for SensorBoard { sensor.v: 100mV, ..Operating }

#[fault]
setup Unplug for SensorBoard {
    event drop: usb.open { at: 1ms, for: 0us..=10ms },
    window: until drop.end + 1s,
    ..Operating
}
#[fault]
setup Reversed for SensorBoard { usb.polarity: reversed, ..Operating }

contract SensorBoard {
    setup = Operating;

    spec sensitivity(zero: AtZero, full: AtFull) {
        let per_mv = (mcu.code[full] - mcu.code[zero]) / 100mV;
        ensure per_mv within 12.0/mV ± 7%;
    }
    spec wake_noise: tran(mcu.code).deviation()               <= 2    on McuWakes   in Measuring;
    spec rail_ok:    dc(vdda.v)                               within 3.2V..=3.4V;   // internal
    pub spec holdup:   tran(mcu.code).during(drop).deviation() <= 4    on Unplug;
    pub spec recovers: tran(v3v3.v).after(drop.end).settle(1%) <= 1s   on Unplug;
    // Reversed: no spec needed; every `rated` limit is checked on every setup, faults included.
}
```

---

## 3. Still open (settle while writing the language)

1. The exact spelling of spec one-liners and of measure methods (`.settle`, `.deviation`, `.first`).
2. The field-path shorthand inside `..` updates (`vin.wave: …`), a small extension of Rust.
3. How a board's modes pick child states (`mcu.state: Run`), and how a model block declares per-state current.
4. Second implementations (parked).
5. The MVP slice (from `contract_v2_review_engine.md`):
   - one block, one circuit, one setup without derivation;
   - `setup =`, single-setup specs, `env`.

   The seams for the rest exist from day one.

## 4. Where it came from

- `contract_options.md` (the common core) and its research: `contract_references.md`, `contract_ee_practice.md`, `contract_hierarchy.md`, `contract_tool_gaps.md`.
- Drafts `contract_syntax_v2.md` and `contract_syntax_v3.md`.
- The four reviews of draft 2: `contract_v2_review_{semantics,syntax,engine,usability}.md`.
