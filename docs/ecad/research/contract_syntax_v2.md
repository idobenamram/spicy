# Block and Contract Syntax, Draft 2

> 2026-09-30 · The proposal as it stands after the walkthrough of `contract_options.md` with the project lead. It is the target for the review agents. **Status:** draft, being iterated.

## Decided (by the project lead, 2026-09-30)

1. **`block` is the keyword for the interface.** Not `struct`: it has no meaning in electronics.
2. **Interface, implementation and contract are separate items** (variation C). This allows more than one implementation of the same block interface, each checked against the same contract.
3. **Setups are defined outside the contract,** as named, reusable items.
4. **Specs are functions inside the contract:** a body that computes, then a final condition. They can take several setups at once.
5. **The common core of `contract_options.md` §1 stands:**
   - sources on inputs, loads on outputs;
   - published port facts;
   - three kinds of condition;
   - operating range + absolute maximum;
   - two-way connection checks;
   - cached child results;
   - faults as timed events;
   - marked one-offs.

## Not decided yet

- **The keyword for the implementation and contract items.** `impl Circuit for X` / `impl Contract for X` copies Rust closely; the lead is hesitant.
- **Setup syntax:** how a setup is declared, inherits, refers to the block's ports and internal nodes, and is chosen by a spec.
- **The spec's final condition keyword:** `require`, `check`, `ensure` or `assert`.
- **Bare port names** inside specs (`vout.v`) or `self.vout.v`.
- **How a placement picks an implementation** when a block has several.
- **The spec line syntax details** (the lead wants to write some of the language before deciding).

## The draft, on the sensor board

```rust
// ───────────────────────────── project.spl ─────────────────────────────
global ambient: Temperature in -10°C..=60°C;
global life:    Duration    in 0y..=10y;
global confidence = sigma(3);

// ───────────────────────────── gain_stage.spl ──────────────────────────
/// The interface: what's visible from outside.
pub block GainStage<A: OpAmp = Mcp6001, const GAIN: f64 = 10.09> {
    vin:  Analog<In>,
    vout: Analog<Out>,
    vdd:  Power<In>,
    gnd:  Ground,
}

/// The implementation: what's inside. (Keyword undecided.)
impl Circuit for GainStage {
    net fb;  net mid;
    let u1  = A        { inp: vin, inn: fb, out: vout, vdd, gnd };
    let ref = MidRail  { vdd, gnd, out: mid };
    let r_f = Resistor { a: vout, b: fb,  value: (GAIN - 1) * 10k ± 1% };
    let r_g = Resistor { a: fb,   b: mid, value: 10k ± 1% };
}

/// The operating environment; also what the block accepts (replaces `assume`).
setup Operating for GainStage {
    vin:  Signal { z: 100Ω..=10kΩ },
    vout: Load   { r: 10kΩ.., c: ..=1nF },
    vdd:  Supply { v: 3.2V..=3.4V },
    temp: ambient,
}
setup InputStep for GainStage: Operating {
    vin: Signal { z: 100Ω..=10kΩ, wave: Step { 0V -> 100mV, edge: 1us } },
}

/// The promises. (Keyword undecided.)
impl Contract for GainStage {
    accepts Operating;
    rated temp within -40°C..=85°C;

    spec gain(env: Operating) {
        let h = ac(vout.v / vin.v);
        require h.at(1kHz).mag() within GAIN ± 1%;
    }
    spec bandwidth(env: Operating) {
        require ac(vout.v / vin.v).f_high(-3dB) >= 20kHz;
    }
    spec settling(env: InputStep) {
        let v = tran(vout.v);
        require v.time_to_within(1% of v.at(end)) <= 50us;
    }
    // published port facts: ordinary specs, used for composition
    spec input_z(env: Operating)  { require vin.z(1kHz) >= 400kΩ; }
    spec output_z(env: Operating) { require vout.z(f) <= 100Ω for f in 10Hz..=100kHz; }
    spec supply(env: Operating)   { require dc(vdd.i) <= 150uA; }
}

// ───────────────────────────── ldo.spl ─────────────────────────────────
pub block Ldo3v3 { vin: Power<In>, vout: Power<Out>, gnd: Ground }
impl Regulator for Ldo3v3 {}                               // gets the standard regulator setups

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
    spec output(env: Operating)       { require dc(vout.v) within 3.3V ± 2%; }
    spec psrr(env: Operating)         { require ac(vout.v / vin.v).at(100kHz).db() <= -36dB at vin.v = 4.3V; }
    spec dip(env: LoadStep<Self>)     { require tran(vout.v).min() >= 3.25V; }      // generic setup from the std library
    spec start(env: PowerUp<Self>)    { require tran(vout.v).time_to(3.2V) <= 1ms; }
}

// ───────────────────────────── board.spl ───────────────────────────────
pub block SensorBoard { usb: Power<In>, sensor: Analog<In>, gnd: Ground }

impl Circuit for SensorBoard {
    net v3v3;  net vdda;  net amp_out;  net adc_in;
    let ldo    = Ldo3v3             { vin: usb, vout: v3v3, gnd };
    let r_filt = Resistor           { a: v3v3, b: vdda, value: 10Ω ± 5% };
    let c_filt = Capacitor          { a: vdda, b: gnd,  value: 10uF ± 20% };
    let amp    = GainStage<Mcp6001> { vin: sensor, vout: amp_out, vdd: vdda, gnd };
    let r_aa   = Resistor           { a: amp_out, b: adc_in, value: 470Ω ± 1% };
    let c_aa   = Capacitor          { a: adc_in,  b: gnd,    value: 100nF ± 10% };
    let mcu    = Stm32Adc           { vdd: v3v3, ain: adc_in, gnd };
}
setup Operating for SensorBoard {
    usb:    Supply { v: 4.40V..=5.25V, z: 0.1Ω..=0.5Ω },
    sensor: Signal { v: 0V..=100mV, z: 100Ω..=10kΩ },
    temp:   ambient,
}
setup McuWakes for SensorBoard: Operating { mcu.load: Load { step: 5mA -> 30mA, edge: 1us } }
setup AtZero   for SensorBoard: Operating { sensor: Signal { v: 0V } }
setup AtFull   for SensorBoard: Operating { sensor: Signal { v: 100mV } }

impl Contract for SensorBoard {
    accepts Operating;
    spec sensitivity(zero: AtZero, full: AtFull) {
        let per_mv = (mcu.code on full - mcu.code on zero) / 100mV;
        require per_mv within 12.0/mV ± 7%;
    }
    spec wake_noise(env: McuWakes) { require tran(mcu.code).deviation() <= 2; }
}
```

## The simple case: the CE amp

```rust
pub block CeAmp { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }

impl Circuit for CeAmp {
    net base;  net emitter;
    let r1   = Resistor     { a: vcc, b: base,    value: 47k ± 1% };
    let r2   = Resistor     { a: base, b: gnd,    value: 10k ± 1% };
    let rc   = Resistor     { a: vcc, b: output,  value: 4.7k ± 1% };
    let re   = Resistor     { a: emitter, b: gnd, value: 1k ± 1% };
    let c_in = Electrolytic { p: base, n: input,  value: 1uF ± 20% };
    let q1   = Npn          { c: output, b: base, e: emitter, beta: 100..=300 };
}

setup Operating for CeAmp { vcc: Supply { v: 12V ± 5% }, temp: ambient }

impl Contract for CeAmp {
    accepts Operating;
    spec bias(env: Operating) { require dc(output.v) within 4.5V..=6.5V; }
    spec gain(env: Operating) { require ac(output.v / input.v).at(1kHz).mag() within 4.6 ± 5%; }
    spec bass(env: Operating) { require ac(output.v / input.v).f_low(-3dB) <= 30Hz; }
}
```

## Context

- The research behind the common core: `contract_options.md` and its sources (`contract_references.md`, `contract_ee_practice.md`, `contract_hierarchy.md`, `contract_tool_gaps.md`).
- Today's language: `language.md` v0.1, `grammar.md`, `model.md` (elaboration), `roadmap.md` §4.2.
- The engine: `engine.md` v4, `engine_plan.md`, `engine_types.md`.
- Post-MVP needs: `next_synthesis.md`.
