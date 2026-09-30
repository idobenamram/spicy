# Block and Contract Syntax, Draft 3

> 2026-09-30 · Draft 2 (`contract_syntax_v2.md`) revised after four reviews: `contract_v2_review_semantics.md` (**SEM**), `contract_v2_review_syntax.md` (**SYN**), `contract_v2_review_engine.md` (**ENG**), `contract_v2_review_usability.md` (**USE**).
> **Status:** draft for the project lead. The decisions of 2026-09-30 stand: `block` for the interface; interface, circuit and contract as separate items; setups outside the contract; specs as functions.

---

## 0. What changed from draft 2, and why

| # | Change | Why | From |
|---|---|---|---|
| 1 | **`circuit X { … }` and `contract X { … }`**; `impl` only for traits | Draft 2 used `impl` for three different things, and Rust's `impl Trait for Type` can't name a second implementation | SYN, USE |
| 2 | **Named second circuits: `circuit X::Ideal { … }`,** placed as `X::Ideal { … }` | Declaration and use look the same; VHDL's architectures, Cadence's config views | SYN |
| 3 | **One-line specs by default:** `spec gain: <measure> within …;`. The body form stays for multi-step and multi-setup specs | 29 of 35 specs in USE's five circuits were one expression in a body | USE |
| 4 | **No `on` = the accepted setup** (every mode of it); `on S` picks another | 10 of draft 2's 13 specs repeated `env: Operating` | SYN, USE |
| 5 | **`with` for a fixed test point, `for x in` for a swept axis;** `.at()` only for a frequency or time axis | `at` meant two things | SYN, USE |
| 6 | **The contract names only ports, generic parameters and declared observables** (`observe`, `emits` in the block's interface) | `mcu.code` and `mcu.load` reached inside a child: meaningless with a second circuit, and it breaks the child's cache | SEM, ENG |
| 7 | **Unwritten setup fields mean ideal** (0 Ω source, open load), and the formatter writes them out; every port is covered | Otherwise the two-way connection check passes on no information | SEM, ENG |
| 8 | **Setup inheritance merges field by field;** field paths (`vin.z: …`) override one field; `A + B` combines two setups | Draft 2 read inheritance two ways; restating a whole shape to change one field hides copies | SEM, ENG, USE |
| 9 | **`extends` for inheritance** | A colon claims "is a", which is false for a fault setup | SYN |
| 10 | **Modes inside a setup:** `mode Run { … } mode Sleep { … }`; a spec narrows with `in Sleep` | Modes had no syntax: each mode doubled setups and specs | USE; PW's scenarios |
| 11 | **Paired sources** `(inp, inn): Pair { dm: …, cm: … }`, and **fields on internal paths** marked by owner (`#[owner(layout)] sda.stray: …`) | Per-port fields checked an in-amp at ±510 mV instead of ±10 mV: a false FAIL on every spec | USE |
| 12 | **The sharing rule for multi-setup specs:** one board, one moment. Statistical knobs are always shared; range knobs are shared unless a setup overrides them; `fresh x` opts out. `m[setup]` reads a measure in a setup | Undefined in draft 2; ENG and SEM derived the same rule independently | SEM, ENG, USE |
| 13 | **`rated` means absolute maximum only.** A block's operating temperature is a setup field | `rated` did two jobs | SEM |
| 14 | **Library setups write numbers; only the project binds globals** (`temp: ambient` only at the root) | Library blocks mustn't depend on one project | SEM |
| 15 | **`pub spec` publishes a spec for composition** (port facts: z_out, z_in, current) | Which specs a parent may rely on must be visible; `pub` already means "visible outside" | this draft (SEM asked for an explicit form) |
| 16 | **Contract-level `let` measures** | The same `ac(vout.v / vin.v)` appeared in five specs | USE |
| 17 | **`#[check(A = [..])]` on generic contracts;** named generic arguments | Otherwise only the default instantiation is checked, silently | USE, ENG |
| 18 | **A time vocabulary:** `window`, `cycles of x`, `start: discharged`, `event`, `.since(ev)`, `.after(ev.end)` | Needed by every transient spec | USE |
| 19 | **Fixed in the example:** `(GAIN - 1) * 10k ± 1%` is a precedence error under the grammar (it needs parentheses); `GAIN ± 1%` fails at worst case (about ±1.8% from the resistors, plus about 1% from the op-amp's finite gain), so it's now ± 3% | Both found by SEM | SEM |

---

## 1. The rules, on one page

**Items:**
```
block X { ports, observables, events }         the interface
circuit X { … }  /  circuit X::Name { … }       an implementation (the first is the default)
setup S for X { … }  /  setup S for X extends T { … }
contract X { accepts S; rated …; let …; spec …; }
trait T { … }  /  impl T for X {}               families of blocks (std library setups and specs)
global g: Type in range;                        project level
```

**A spec:**
```
spec name: <measure> <limit> [with <fixed points>] [for <axis> in <range>] [on <setup>] [in <mode>];
spec name(a: SetupA, b: SetupB) { let …; require <measure> <limit>; }
pub spec …                                      published: parents may rely on it
```

**Limits:** `within <range>`, `<=`, `>=`. **Conditions:** `with x = v` (pinned), `for x in r` (swept axis). **Ranges** come from the setup.

---

## 2. The sensor board in draft 3

```rust
// ───────────────────────────── project.spl ─────────────────────────────
global ambient:    Temperature in -10°C..=60°C;
global life:       Duration    in 0y..=10y;
global confidence: Confidence  = sigma(3);

// ───────────────────────────── gain_stage.spl ──────────────────────────
/// Non-inverting amplifier around a mid-rail reference.
pub block GainStage<A: OpAmp = Mcp6001, const GAIN: f64 = 10.09> {
    vin:  Analog<In>,
    vout: Analog<Out>,
    vdd:  Power<In>,
    gnd:  Ground,
}

circuit GainStage {
    net fb;  net mid;
    let u1  = A        { inp: vin, inn: fb, out: vout, vdd, gnd };
    let ref = MidRail  { vdd, gnd, out: mid };
    let r_f = Resistor { a: vout, b: fb,  value: ((GAIN - 1) * 10k) ± 1% };
    let r_g = Resistor { a: fb,   b: mid, value: 10k ± 1% };
}

/// Also a second implementation, for early system work: an ideal gain block.
circuit GainStage::Ideal {
    let e1 = Vcvs { in_p: vin, in_n: gnd, out_p: vout, out_n: gnd, gain: GAIN };
}

setup Operating for GainStage {
    vin:  Signal { z: 100Ω..=10kΩ },
    vout: Load   { r: 10kΩ.., c: ..=1nF },
    vdd:  Supply { v: 3.2V..=3.4V, z: ..=20Ω },
    temp: -40°C..=85°C,
}
setup InputStep for GainStage extends Operating {
    vin.wave: Step { 0V -> 100mV, edge: 1us },
    window:   200us,
}

#[check(A = [Mcp6001, Tlv9001])]
contract GainStage {
    accepts Operating;

    let h = ac(vout.v / vin.v);

    spec gain:      h.at(1kHz).mag()          within GAIN ± 3%;
    spec bandwidth: h.f_high(-3dB, ref: dc)   >= 20kHz;
    spec settling:  tran(vout.v).settle(1%)   <= 50us       on InputStep;

    pub spec input_z:  vin.z(1kHz)            >= 400kΩ;
    pub spec output_z: vout.z(f)              <= 100Ω       for f in 10Hz..=100kHz;
    pub spec supply:   dc(vdd.i)              <= 150uA;
}

// ───────────────────────────── ldo.spl ─────────────────────────────────
pub block Ldo3v3 { vin: Power<In>, vout: Power<Out>, gnd: Ground }
impl Regulator for Ldo3v3 {}                    // brings the std setups LoadStep, PowerUp, Ripple

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

contract Ldo3v3 {
    accepts Operating;
    rated vin.v within -0.3V..=6.5V;

    spec output:    dc(vout.v)                            within 3.3V ± 2%;
    spec line_reg:  dc(vout.v).span(for vin.v)            <= 5mV     with vout.i = 30mA;
    spec psrr:      ac(vout.v / vin.v).at(100kHz).db()    <= -36dB   on Ripple;
    spec dip:       tran(vout.v).min()                    >= 3.25V   on LoadStep in Run;
    spec start:     tran(vout.v).crossing(rising 3.2V)    <= 1ms     on PowerUp;
    spec quiescent: dc(vin.i - vout.i)                    <= 50uA    in Sleep;

    pub spec output_z: vout.z(f)                          <= 2Ω      for f in 10Hz..=1MHz  in Run;
}

// ───────────────────────────── stm32_adc.spl ───────────────────────────
/// A model block: the MCU as its rail load and its ADC input.
pub block Stm32Adc {
    vdd: Power<In>,
    ain: Analog<In>,
    gnd: Ground,
    observe code: Integer,                               // the ADC reading: visible to contracts
    emits wake: Step on vdd.i { 5mA -> 30mA, edge: 1us },  // an event a parent can choose to trigger
}

// ───────────────────────────── board.spl ───────────────────────────────
pub block SensorBoard { usb: Power<In>, sensor: Analog<In>, gnd: Ground }

circuit SensorBoard {
    net v3v3;  net vdda;  net amp_out;  net adc_in;
    let ldo    = Ldo3v3                    { vin: usb, vout: v3v3, gnd };
    let r_filt = Resistor                  { a: v3v3, b: vdda, value: 10Ω ± 5% };
    let c_filt = Capacitor                 { a: vdda, b: gnd,  value: 10uF ± 20% };
    let amp    = GainStage<A = Mcp6001>    { vin: sensor, vout: amp_out, vdd: vdda, gnd };
    let r_aa   = Resistor                  { a: amp_out, b: adc_in, value: 470Ω ± 1% };
    let c_aa   = Capacitor                 { a: adc_in,  b: gnd,    value: 100nF ± 10% };
    let mcu    = Stm32Adc                  { vdd: v3v3, ain: adc_in, gnd };
}

setup Operating for SensorBoard {
    usb:    Supply { v: 4.40V..=5.25V, z: 0.1Ω..=0.5Ω },
    sensor: Signal { v: 0V..=100mV, z: 100Ω..=10kΩ },
    temp:   ambient,                                     // the project root binds the global
}
setup McuWakes for SensorBoard extends Operating { event: mcu.wake at 1ms, window: 5ms }
setup AtZero   for SensorBoard extends Operating { sensor.v: 0V }
setup AtFull   for SensorBoard extends Operating { sensor.v: 100mV }

#[fault]
setup Unplug for SensorBoard extends Operating {
    event drop: usb.open { at: 1ms, for: 0us..=10ms },
    window: until drop.end + 1s,
}

contract SensorBoard {
    accepts Operating;

    /// ADC codes per millivolt. Two runs of the same board: parts, temperature and USB shared.
    spec sensitivity(zero: AtZero, full: AtFull) {
        let per_mv = (mcu.code[full] - mcu.code[zero]) / 100mV;
        require per_mv within 12.0/mV ± 7%;
    }
    spec wake_noise: tran(mcu.code).deviation()             <= 2      on McuWakes;
    spec holdup:     tran(mcu.code).during(drop).deviation() <= 4      on Unplug;
    spec recovers:   tran(v3v3.v).after(drop.end).settle(1%) <= 1s     on Unplug;
}
```

## 3. The CE amp in draft 3

```rust
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
    accepts Operating;
    let h = ac(output.v / input.v);

    spec bias: dc(output.v)            within 4.5V..=6.5V;
    spec gain: h.at(1kHz).mag()        within 4.6 ± 5%;
    spec bass: h.f_low(-3dB)           <= 30Hz;
}
```

Same 8 knobs and 3 specs as today. The engine's MVP check is unchanged (579 runs); only the temperature knob's path moves from `temp` to `ambient` (ENG).

## 4. Still open (small)

1. **The body condition keyword:**
   - `require` (SysML v2; the engineer's "requirement"; SYN);
   - `ensure` (design by contract: the setup is the precondition, the spec the postcondition; USE).

   It only appears in body-form specs.
2. **`extends`** (SYN) vs `: Operating` (USE) vs `..Operating`. This draft uses `extends`.
3. **Named circuit syntax:** `circuit X::Ideal` (this draft) vs `circuit Ideal for X` (SYN).
4. **`pub spec`** as the way to publish port facts, a new idea in this draft that no review has checked yet.
5. **What goes in the MVP slice** (ENG §MVP):
   - one block, one circuit, one setup without inheritance;
   - `accepts`, single-setup specs, `global`.

   Everything else comes later, with its seam in place from day one.
