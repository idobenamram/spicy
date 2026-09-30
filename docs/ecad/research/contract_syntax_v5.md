# Block and Contract Syntax, Draft 5 (final for now)

> 2026-09-30 · Draft 4 (`contract_syntax_v4.md`) plus the fixes from its three reviews: `contract_v4_review_consistency.md`, `contract_v4_review_novice.md`, `contract_v4_review_implementation.md`. **Status:** final for now. This is the target for the language implementation. Open points are listed in §4 and left as in draft 4.

---

## 0. Decisions

| Decision | Choice |
|---|---|
| Interface | `block X { ports }`: ports (and observables and events it publishes) only |
| Implementation | `circuit X { parts, nets }`: one per block (second implementations parked) |
| Test environments | `setup S for X { … }`, defined outside the contract, reusable |
| Promises | `contract X { … }` |
| The default setup | `setup = Operating;` in the contract. Specs use it unless they say `on`; parents are checked against it |
| Specs | one-liners; a function form `spec name(a: A, b: B) { …; ensure …; }` for multi-step and multi-setup specs |
| Final condition | `ensure` |
| Published specs | `pub spec`: parents may rely on it |
| Internal specs | allowed; checked normally; never published or used for composition |
| Setup derivation | Rust's struct update `..Operating`, plus field paths (`vin.wave: …`) |
| Project level | `const` (a fixed value), `env` (a shared environment range, searched) |
| Absolute maximum | `rated`, checked on every setup, faults included |
| Modes | `mode Name { … }` inside a setup; every spec is checked in each mode; `in Name` narrows |
| Differential inputs | `(a, b): Pair { dm: …, cm: …, z: … }` |
| Block-published values and events | `observe`, `emits` |
| Faults | `#[fault]` setups with an `event` |
| One-offs | `#[outside(reason = …)]` setups: characterization, never `pub` |
| Limits | `within <range>`, `<=`, `>=` (`in` is no longer a limit operator) |

---

## 1. Rules

### 1.1 Items

```
const NAME: Type = value;
env name: Type in range;
pub block X { port: Kind<Role>, …, observe o: Type, emits e: Shape { … } }
circuit X { net n; let part = Kind { pin: net, …, field: value }; }
setup S for X { port: Shape { … }, path: value, temp: …, mode M { … }, event e: Shape { … }, window: …, ..Base }
contract X { setup = S; rated …; let m = <measure>; spec …; pub spec …; }
trait T { … }   impl T for X {}
```

### 1.2 Specs

```
spec name: <measure> <limit> [with <pins>] [for <axis> in <range>] [on <setup>] [in <mode>];
spec name(a: SetupA, b: SetupB) [with …] [in <mode>] { let …; ensure <measure> <limit>; }
```

- **The one-liner is shorthand** for the function form with the default setup and a single `ensure`. Both forms accept the same clauses.
- **Limits:** `within <range>`, `<=`, `>=`. **Pins:** `with x = v`. **Swept axis:** `for x in r`. **Setup:** `on S`. **Mode:** `in M`.
- **In a multi-setup spec,** `m[s]` is measure `m` in setup `s`.

### 1.3 Setups

1. **Every port except ground appears in the default setup.** Signal and supply voltages have **no ideal default**, so they must be written. Impedances left unwritten are ideal (0 Ω source, open load), and the formatter writes them out.
2. **Fields merge one by one.** A path (`vin.wave: …`) changes one field; a whole shape (`vin: Signal { … }`) replaces the port's fields.
3. **Order when setups combine:** the base's fields first, then the mode's, then the derived setup's own. The later one wins, field by field.
4. **A derived setup stays inside the default setup** in every mode a spec uses it in, unless it is `#[fault]` or `#[outside]`.
5. **Inside a setup, a path names a field of the port's shape:** `vin.v` there is the source's setting. **Inside specs and `rated`,** `p.v` and `p.i` are measured pin quantities.
6. **In one multi-setup spec, every setup is the same board at the same moment:** part tolerances are always shared, and range knobs are shared unless a setup overrides them.
7. **A `Pair`'s two legs get separate tolerance knobs** (the imbalance between them matters, e.g. for CMRR).
8. **Events** are shapes with named fields: `Step { a -> b, edge: …, at: … }`, `Open { port: …, at: …, lasts: … }`, `Trigger { of: child.event, at: … }`. **`window: ..=t`** is how long to simulate.

### 1.4 Specs and publishing

1. **A `pub spec` may name only the block's own ports and observables.** A spec naming an internal net or a child's value is internal.
2. **Port quantities:** `p.v` is the pin's voltage to ground; `p.i` is the current **into** the block at that pin (positive in); `p.z(f)` is the impedance looking into the pin. A `Load`'s `i` is the current the load draws.
3. **`rated` limits are pin quantities,** checked on every run of every setup.

### 1.5 Measures

- **Kinds of simulation:** `dc(x)`, `ac(x)`, `tran(x)`.
- **Reduce to one number:** `.at(f)`, `.mag()`, `.db()`, `.f_low(-3dB)`, `.f_high(-3dB, ref: dc)`, `.min()`, `.max()`, `.settle(1%)`, `.crossing(rising v)`, `.deviation()`, `.span(for x)`.
- **Time:** `.during(ev)`, `.after(ev.end)`.
- **Units in expressions:** a quantity divided by a unit is written with a number: `12.0 / 1mV`.

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

setup Operating for CeAmp {
    vcc:    Supply { v: 12V ± 5% },
    input:  Signal { v: 0V },
    output: Load   { },
    temp:   ambient,
}

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
setup LoadStep for Ldo3v3 {
    vout.i: Step { 5mA -> 30mA, edge: 1us, at: 50us },      // the step replaces the mode's range (rule 1.3.3)
    window: ..=300us,
    ..Operating
}

#[outside(reason = "datasheet dropout row: defined at the rated 500 mA")]
setup DropoutRow for Ldo3v3 { vout.i: 500mA, vin.v: Sweep(4.5V -> 3.0V), ..Operating }

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

// ───────────────────────────── gain_stage.spl ──────────────────────────
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

setup Operating for GainStage {
    vin:  Signal { v: 0V..=100mV, z: 100Ω..=10kΩ },
    vout: Load   { r: 10kΩ.., c: ..=1nF },
    vdd:  Supply { v: 3.2V..=3.4V, z: ..=20Ω },
    temp: -40°C..=85°C,
}
setup InputStep for GainStage {
    vin.wave: Step { 0V -> 100mV, edge: 1us, at: 10us },
    window:   ..=200us,
    ..Operating
}

#[check(A = [Mcp6001, Tlv9001])]
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

// ───────────────────────────── in_amp.spl (a pair) ─────────────────────
pub block InAmp { inp: Analog<In>, inn: Analog<In>, out: Analog<Out>, vdd: Power<In>, gnd: Ground }

setup Operating for InAmp {
    (inp, inn): Pair { dm: -10mV..=10mV, cm: 2.5V ± 5%, z: 350Ω ± 0.1% },   // each leg has its own z knob
    out:  Load   { r: 10kΩ.. },
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
        ensure (a_dm / a_cm).db() >= 100dB;
    }
}

// ───────────────────────────── board.spl ───────────────────────────────
pub block Stm32Adc {
    vdd: Power<In>, ain: Analog<In>, gnd: Ground,
    observe code: Integer,                                            // the ADC reading
    emits wake: Step { on: vdd.i, 5mA -> 30mA, edge: 1us },           // a current step a parent may trigger
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
}
setup McuWakes for SensorBoard { event wake: Trigger { of: mcu.wake, at: 1ms }, window: ..=5ms, ..Operating }
setup AtZero   for SensorBoard { sensor.v: 0V,    ..Operating }
setup AtFull   for SensorBoard { sensor.v: 100mV, ..Operating }

#[fault]
setup Unplug for SensorBoard {
    event drop: Open { port: usb, at: 1ms, lasts: 0us..=10ms },
    window: ..=(drop.end + 1s),
    ..Operating
}
#[fault]
setup Reversed for SensorBoard { usb.polarity: reversed, ..Operating }

contract SensorBoard {
    setup = Operating;

    spec sensitivity(zero: AtZero, full: AtFull) {
        let per_mv = (mcu.code[full] - mcu.code[zero]) / 100mV;
        ensure per_mv within (12.0 / 1mV) ± 7%;
    }
    spec wake_noise: tran(mcu.code).deviation()                <= 2     on McuWakes;
    spec rail_ok:    dc(vdda.v)                                within 3.2V..=3.4V;   // internal
    spec holdup:     tran(mcu.code).during(drop).deviation()   <= 4     on Unplug;   // internal: reads a child's value
    spec recovers:   tran(v3v3.v).after(drop.end).settle(1%)   <= 1s    on Unplug;   // internal: names a net
    // Reversed: no spec needed. Every `rated` limit is checked on every setup, faults included.
    // As drawn, this board FAILS it: a reversed connector puts -1.53 V on the LDO's input,
    // below its -0.3 V rating. A series Schottky diode fixes it.
}
```

---

## 3. For the implementation

**The MVP subset** (from `contract_v4_review_implementation.md`). Everything else is parsed and reported as "not supported yet":
- blocks with ports; one `circuit`;
- a `setup` with no derivation, modes or events;
- `env` and `const`;
- `contract { setup = S; let …; [pub] spec name: <measure> within | <= | >= <bound>; }`, with internal-spec detection and the `pub` rule of §1.4.

`ce_amp.spl` in this syntax gives the same 8 knobs (`temp` becomes `ambient`) and 3 specs (plus the internal `base_bias` if kept), so the engine's MVP check is unchanged.

**The migration, in the order that throws nothing away** (details in the implementation review):
1. new tokens and keywords;
2. `within` replaces `in` as a limit;
3. the `block` header + `circuit`, with elaboration rebuilding today's structures from the two items, so the span-free design dumps must come out byte-identical;
4. parse all of this draft's grammar;
5. resolve contracts in the MVP subset.

**Parser notes** (implementation review):
- `setup` and `pub` can start both an item and a contract statement, so the parser needs two tokens of lookahead;
- `spec cmrr(` needs a look past the `)` for `{`;
- `mode`, `event`, `observe`, `emits`, `with`, `on` are recognized only in their positions, so they stay usable as names;
- named generic arguments in a `let` are recognized by `< name =`.

---

## 4. Open, left as in draft 4

1. `emits` might become `event` in block interfaces (novice review).
2. Setup fields might read `z: <= 0.5Ω` instead of `z: ..=0.5Ω` (novice review).
3. `#[outside]` might get a clearer name (novice review).
4. How a board's modes select a child's state (e.g. an MCU's Run or Sleep current). This was removed from the example until it's designed.
5. Second implementations (parked).
6. The exact spelling of measure methods, to settle while writing the language.
7. The implementation review's 7 grammar questions (§8 of that file), to settle when the front-end reaches them.
8. **When specs run** (not MVP). The likely addition is one attribute, `#[run(ci | nightly | manual)]`, on top of automatic tiers from the engine's cost estimate. Design: `runs_language.md`; measurements: `runs_cost_tiers.md`.
