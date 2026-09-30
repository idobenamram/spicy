# Draft 2 Under Load: Five Circuits Written Out

> 2026-09-30 · A usability review of `contract_syntax_v2.md`. Five post-MVP circuits (from `next_synthesis.md` §9 and its reports) are written in draft 2 as literally as it allows, then measured for length, clarity and edit locality. Nothing here was simulated; numbers quoted come from the cited reports (**PA**, **MS**, **PW**, **TD**).

## 0. Summary

**What holds up.** The three-item split (block / circuit / contract) reads well. `setup` as a struct literal of port shapes is the clearest part of the draft: a novice can read `vout: Load { i: 1mA..=300mA, c: ..=10uF }` aloud. Specs as small functions handle the hard cases (two-setup CMRR, 3-run calibration) that one-line specs can't.

**What breaks, by size of the problem:**

| # | Problem | Hit in | Cost today |
|---|---|---|---|
| U1 | **No discrete choices in setups** (modes, module fitted or not). Each choice multiplies setups and specs: `vout_run` / `vout_sleep`, `rise_sda` / `rise_sda_fitted` / … | LDO, I2C | 2× specs, 2–4× setups; `accepts` has to name two setups |
| U2 | **Single inheritance, whole-field override.** Adding one field to a `Sine` restates the whole `Sine`; combining "Fitted" with "Release" copies lines | all five | the largest source of duplication |
| U3 | **One port per setup field.** A differential pair, a transformer secondary and a bus line's layout capacitance don't belong to one port. Per-port ranges on `inp`/`inn` silently widen the differential input from ±10 mV to ±510 mV: **a wrong verdict**, not just verbosity | in-amp, rectifier, I2C | wrong verdict |
| U4 | **`(env: Operating)` on almost every spec,** plus `accepts Operating;`, plus `temp: ambient` in every setup | all | 6 of 6 Sallen-Key specs; 8 of 35 overall |
| U5 | **Multi-setup specs have no sharing rule.** Is `reading on zero` the same board as `reading on env`? At the same `vexc`? At the same temperature? The draft doesn't say, and CMRR and calibration need different answers | in-amp, SensorBoard | wrong verdict if guessed wrong |
| U6 | **No time vocabulary:** simulation window, "last cycle", initial state, fault events and "time since the event" are all missing | LDO, rectifier, I2C | blocks |
| U7 | **`require` means the opposite in contract literature** (Eiffel, Ada/SPARK `Pre`, Solidity: a precondition). A spec's final line is a postcondition. The setup is the `require` side | all | misreading by experts |
| U8 | **Three spellings of a step** (`wave: Step { a -> b, edge }`, `step: a -> b, edge:`, language.md's `Load::Step { from, to, rise }`) and two meanings of `at` (`.at(1kHz)` and `at vin.v = 4.3V`) | LDO, in-amp | novice confusion |
| U9 | **Measures that nobody can write:** f0, Q, "−3 dB vs the passband", loop gain, "at DC", settling | Sallen-Key, LDO, in-amp | blocks, or a confident verdict on the wrong quantity (PA: 1188 vs 1389 Hz) |
| U10 | **Generic blocks don't say which instantiations were checked.** `SallenKeyLp<A: OpAmp>` passes for its default `Opa171`; nothing tells the reader that a `Tl072` placement is unchecked | Sallen-Key, in-amp | wrong verdict by omission |

**Line counts** (draft 2, as written below; non-blank lines):

| Circuit | Lines | Specs | Setups | Lines after the proposals (§8) |
|---|---|---|---|---|
| 1. Sallen-Key | 45 | 6 | 1 | contract 21 → 12 |
| 2. I2C bus | 57 | 7 | 7 | 57 → 29 |
| 3. LDO, Run/Sleep | 49 | 11 | 8 | 49 → 27 (circuit body elided in both) |
| 4. Rectifier + fault | 45 | 6 | 4 | ≈ 33 (estimated, not rewritten) |
| 5. In-amp + load cell | 82 | 5 | 6 | ≈ 70 (estimated, not rewritten) |

**The proposals** (§7, each shown before/after), in the order they pay off: one-line specs with a default setup (P1); field-path overrides and setup sums (P2); modes inside a setup (P3); paired sources and fields on internal paths (P4); a sharing rule for multi-setup specs (P5); a time vocabulary (P6); `ensure` for the final condition, `circuit`/`contract` for the items, `with` for fixed points (P7); contract-level `let` (P8); checked instantiations for generics (P9); named generic arguments (P10).

**Legend in the code:** `✗` = draft 2 has no syntax for this; the line shows an invented form. `?` = the draft allows it but its meaning is unclear.

---

## 1. Sallen-Key low-pass, generic over the op-amp

PA §2.B: f0 = 1 kHz, Q ≈ 1.3, peaking 3.0 dB ± 0.5 dB, stopband ≥ 38 dB at 10 kHz.

```rust
/// Unity-gain Sallen-Key low-pass, single supply, input biased at mid-rail.
pub block SallenKeyLp<A: OpAmp = Opa171, const F0: Hertz = 1kHz, const Q: f64 = 1.3> {
    vin:  Analog<In>,
    vout: Analog<Out>,
    vdd:  Power<In>,
    gnd:  Ground,
}

impl Circuit for SallenKeyLp {
    net n1;  net n2;
    const R: Ohm = 10k;                                                    // ? a const inside impl Circuit
    let r1 = Resistor  { a: vin, b: n1,   value: R ± 1% };
    let r2 = Resistor  { a: n1,  b: n2,   value: R ± 1% };
    let c1 = Capacitor { a: n1,  b: vout, value: 2 * Q / (2π * F0 * R) ± 5% };       // 41.4 nF: not an E-series value
    let c2 = Capacitor { a: n2,  b: gnd,  value: 1 / (2 * Q * 2π * F0 * R) ± 5% };   // 6.12 nF
    let u1 = A         { inp: n2, inn: vout, out: vout, vdd, gnd };
}

setup Operating for SallenKeyLp {
    vin:  Signal { v: vdd.v / 2, z: ..=100Ω },       // ? is `v` the DC level or the amplitude? ✗ refers to another port's knob
    vout: Load   { r: 10kΩ.., c: ..=100pF },
    vdd:  Supply { v: 3.3V ± 3% },
    temp: ambient,
}

impl Contract for SallenKeyLp {
    accepts Operating;

    spec cutoff(env: Operating) {
        let h = ac(vout.v / vin.v);
        require h.f_high(-3dB, ref: dc) within 1.39 * F0 ± 7%;       // ✗ `ref:`; D5 measures vs the peak (1188 Hz, not 1389)
    }
    spec peaking(env: Operating) {
        let h = ac(vout.v / vin.v);
        require h.peak().db() - h.at(1Hz).db() within 3.0dB ± 0.5dB;  // ? "at DC" spelled as 1 Hz
    }
    spec shape(env: Operating) {
        let p = ac(vout.v / vin.v).fit(biquad);                      // ✗ no measure yields f0 or Q
        require p.f0 within F0 ± 5%;
        require p.q  within Q ± 10%;                                 // ? two requires: one verdict or two?
    }
    spec stopband(env: Operating) {
        require ac(vout.v / vin.v).at(10 * F0).db() <= -38dB;
    }
    spec output_z(env: Operating) { require vout.z(f) <= 10Ω for f in 10Hz..=10 * F0; }
    spec supply(env: Operating)   { require dc(vdd.i) <= 2mA; }
}

// placing it: positional generics
let lp = SallenKeyLp<Tl072, 2kHz, 0.707> { vin: raw, vout: filt, vdd: v3v3, gnd };
```

**45 lines.** Issues:

- **`(env: Operating)` six times,** and `ac(vout.v / vin.v)` five times. The block has one setup; nothing is gained by naming it on each spec. Lets are per spec, so `h` can't be shared.
- **f0 and Q can't be measured.** They're pole properties, not points on the curve. Without `.poles()` or a fit, the author writes `f_high(-3dB)` and gets PA's trap: D5 gives 1188 Hz (vs the peak), the filter engineer means 1389 Hz (vs DC) or 1000 Hz (f0).
- **"At DC" is spelled `.at(1Hz)`.** An AC analysis has no 0 Hz point; everyone will invent their own stand-in.
- **Two `require`s in `shape`:** is that one verdict or two? The report and the spec table need one row per verdict.
- **`Signal { v: vdd.v / 2 }`**: `v` is the DC level here, but in `SensorBoard` `v: 0V..=100mV` reads like an amplitude. A setup field that refers to another port's knob (`vdd.v`) isn't in the draft either.
- **Generic over `A`, checked for one `A`.** The contract passes for `Opa171`. A placement `SallenKeyLp<Tl072, …>` gets a different circuit whose verdicts nobody computed. `A: OpAmp` also can't say "unity-gain stable", which this topology needs.
- **Computed values aren't buildable.** `2*Q/(2π*F0*R)` = 41.4 nF, not an E-series value. language.md §5.6's "committed value `from` formula" is the answer; draft 2 doesn't show it.
- **Positional generics** at the placement: `SallenKeyLp<Tl072, 2kHz, 0.707>`. Which number is Q?

---

## 2. I2C bus: pull-ups, an optional module, layout-owned capacitance

MS §2.1: 2.2 kΩ pull-ups, three pins of ≤ 10 pF, a module with 4.7 kΩ and 40–80 pF, trace capacitance owned by layout, rise ≤ 300 ns (Fast mode). MS found the useful answer is a constraint: "layout may add up to 121.1 pF with the module fitted".

```rust
pub block SensorI2c<const SPEED: I2cSpeed = Fast> {
    vdd: Power<In>,
    gnd: Ground,
    ext: I2c<Target>,                    // the connector where the plug-in module may sit
}

impl Circuit for SensorI2c {
    net sda = ext.sda;  net scl = ext.scl;                          // ✗ naming a port bundle's member as a net
    let mcu  = Stm32I2c { vdd, gnd, sda, scl };                     // controller, open-drain
    let t1   = Tmp117   { vdd, gnd, sda, scl };
    let t2   = Bme280   { vdd, gnd, sda, scl };
    let rp_d = Resistor { a: vdd, b: sda, value: 2.2k ± 1% };
    let rp_c = Resistor { a: vdd, b: scl, value: 2.2k ± 1% };
}

setup Operating for SensorI2c {
    vdd:  Supply { v: 3.3V ± 3% },
    ext:  Absent,                                                   // ✗ "nothing plugged in"
    #[owner(layout)] sda: Stray { c: 20pF..=100pF },                // ✗ internal net, layout-owned, not a port
    #[owner(layout)] scl: Stray { c: 20pF..=100pF },
    temp: -40°C..=85°C,
}
setup Fitted for SensorI2c: Operating {
    ext: I2cModule { pull_up: 4.7k ± 5% to vdd, c: 40pF..=80pF },  // ✗ a load that pulls UP, to another port
}
setup ReleaseSda for SensorI2c: Operating {
    mcu.sda: Drive { low -> released, at: 1us },                    // ? a stimulus on a child's pin
}
setup ReleaseSdaFitted for SensorI2c: Fitted {                      // single inheritance: copy the line
    mcu.sda: Drive { low -> released, at: 1us },
}
setup ReleaseScl for SensorI2c: Operating {
    mcu.scl: Drive { low -> released, at: 1us },
}
setup ReleaseSclFitted for SensorI2c: Fitted {
    mcu.scl: Drive { low -> released, at: 1us },
}
setup HoldSda for SensorI2c: Fitted { mcu.sda: Drive { low } }     // heaviest sink: both pull-ups

impl Contract for SensorI2c {
    accepts Operating;
    accepts Fitted;                                                 // ✗ `accepts` names one setup in the draft

    spec rise_sda(env: ReleaseSda) {
        require tran(sda.v).rise_time(0.3 * vdd.v, 0.7 * vdd.v) <= SPEED.t_r_max;
    }
    spec rise_sda_fitted(env: ReleaseSdaFitted) {
        require tran(sda.v).rise_time(0.3 * vdd.v, 0.7 * vdd.v) <= SPEED.t_r_max;
    }
    spec rise_scl(env: ReleaseScl) {
        require tran(scl.v).rise_time(0.3 * vdd.v, 0.7 * vdd.v) <= SPEED.t_r_max;
    }
    spec rise_scl_fitted(env: ReleaseSclFitted) {
        require tran(scl.v).rise_time(0.3 * vdd.v, 0.7 * vdd.v) <= SPEED.t_r_max;
    }
    spec fall_sda(env: ReleaseSda) {                                // ✗ needs a "pull low" edge, not "release"
        require tran(sda.v).fall_time(0.7 * vdd.v, 0.3 * vdd.v) >= 20ns * vdd.v / 5.5V;
    }
    spec vol(env: HoldSda)  { require dc(sda.v) <= 0.4V; }
    spec sink(env: HoldSda) { require dc(mcu.sda.i) <= 3mA; }      // the driver's tested datasheet row
}
```

**57 lines, 7 setups for 3 real stimuli.** Issues:

- **The variant multiplies everything.** {Bare, Fitted} × {release SDA, release SCL} = 4 setups and 4 identical specs that differ in one word. Single inheritance forces `ReleaseSdaFitted` to copy `ReleaseSda`'s line.
- **`accepts` can name one setup.** The block tolerates both "module absent" and "module fitted".
- **Is the module a build variant or the environment?** Here it's on a connector, so it's a setup (environment). If it were a DNP footprint on this board, it would be a circuit variant. Draft 2 has neither, and should say which is which.
- **A load that pulls up to another port** (`pull_up: 4.7k to vdd`) doesn't fit `Load { r, c }`, which is implicitly to ground.
- **Layout-owned capacitance sits on an internal net,** not a port. The draft's setup fields are ports (plus the unexplained `mcu.load:` in `McuWakes`). `#[owner(layout)]` has to mean "report the constraint (≤ 121.1 pF), not a verdict"; nothing says so.
- **The rise-time spec duplicates an automatic check.** language.md §8.7 lists "I2C pull-ups and rise time (from UM10204)" as an interface rule. Written by hand, it can drift from the automatic one.
- **`SPEED.t_r_max`** works because SPEED is a build-time const. If the speed were a mode, the bound would depend on a knob (model.md E24).
- **Stimuli on a child's pins** (`mcu.sda: Drive {…}`) are needed but undefined: does the setup override the part's own behavior, or add a source?

---

## 3. LDO with Run and Sleep modes

PW §2: a discrete LDO; the load is 1–300 mA in Run and 10–100 µA in Sleep. Phase margin is 61.2° at 1 mA (Run) and 32.5° in Sleep, so one merged range hides which mode fails.

```rust
pub block Ldo3v3 { vin: Power<In>, vout: Power<Out>, gnd: Ground }

impl Circuit for Ldo3v3 {
    net pb;  net db;  net e2;  net fb;  net comp;
    let q1     = Pnp       { e: vin,  b: pb,   c: vout, beta: 100..=300 };
    let r_be   = Resistor  { a: vin,  b: pb,   value: 470 ± 1% };
    let q2     = Npn       { c: pb,   b: db,   e: e2,   beta: 100..=300 };
    let r_e2   = Resistor  { a: e2,   b: gnd,  value: 330 ± 1% };
    let r_bias = Resistor  { a: vin,  b: db,   value: 4.7k ± 1% };
    let u1     = Tlv431    { k: db,   a: gnd,  ref: fb };
    let r_c    = Resistor  { a: db,   b: comp, value: 47k ± 1% };
    let c_c    = Capacitor { a: comp, b: fb,   value: 2.2nF ± 10% };
    let r_top  = Resistor  { a: vout, b: fb,   value: 16.5k ± 1% };
    let r_bot  = Resistor  { a: fb,   b: gnd,  value: 10k ± 1% };
    let c_out  = Capacitor { a: vout, b: gnd,  value: 10uF ± 20%, esr: 10mΩ..=1Ω };
}

setup Run for Ldo3v3 {
    vin:  Supply { v: 3.6V..=5.5V, z: ..=0.2Ω },
    vout: Load   { i: 1mA..=300mA, c: ..=10uF },
    temp: -20°C..=60°C,
    #[owner(layout)] theta_ja: 60K/W..=100K/W,                     // ✗ not a port: where does it live?
}
setup Sleep for Ldo3v3: Run {                                       // reads "Sleep is a kind of Run"
    vout: Load { i: 10uA..=100uA, c: ..=10uF },                     // ? replaces the whole Load, so `c` is restated
}
setup RippleRun   for Ldo3v3: Run   { vin: Supply { v: 3.6V..=5.5V, z: ..=0.2Ω, ac: 1V } };   // ✗ AC on a supply
setup RippleSleep for Ldo3v3: Sleep { vin: Supply { v: 3.6V..=5.5V, z: ..=0.2Ω, ac: 1V } };
setup LoopRun     for Ldo3v3: Run   { break: r_top.a };             // ✗ where the loop is broken
setup LoopSleep   for Ldo3v3: Sleep { break: r_top.a };
setup Step        for Ldo3v3: Run   { vout: Load { step: 1mA -> 300mA, edge: 1us }, tran: 300us };  // ✗ `tran:` window
setup Release     for Ldo3v3: Run   { vout: Load { step: 300mA -> 1mA, edge: 1us }, tran: 3ms };

impl Contract for Ldo3v3 {
    accepts Run;
    accepts Sleep;                                                  // ✗ one setup only
    rated vin.v within -0.3V..=6V;

    spec vout_run(env: Run)       { require dc(vout.v) within 3.3V ± 3%; }
    spec vout_sleep(env: Sleep)   { require dc(vout.v) within 3.3V ± 3%; }
    spec psrr_run(env: RippleRun)     { require ac(vout.v / vin.v).at(100kHz).db() <= -40dB; }
    spec psrr_sleep(env: RippleSleep) { require ac(vout.v / vin.v).at(100kHz).db() <= -40dB; }
    spec pm_run(env: LoopRun)     { require ac(loop_gain).phase_margin() >= 45°; }   // ✗ `loop_gain`
    spec pm_sleep(env: LoopSleep) { require ac(loop_gain).phase_margin() >= 45°; }   // 32.5° in PW: FAIL
    spec undershoot(env: Step)    { require tran(vout.v).min() >= 3.1V; }
    spec settle(env: Step) {
        let v = tran(vout.v);
        require v.time_to_within(1% of v.at(end)) <= 200us;
    }
    spec overshoot(env: Release)  { require tran(vout.v).max() <= 3.45V; }         // PW: 3.43 V after 3 ms
    spec iq(env: Sleep)           { require dc(vin.i) - vout.i <= 3mA; }           // ? `vout.i`: knob or probe?
    spec tj(env: Run)             { require temp + theta_ja * dc(q1.power) <= 125°C; }
}
```

**49 lines; 11 specs, of which 3 pairs are the same spec in two modes.** Issues:

- **Modes as setups double the specs** (`vout_run`/`vout_sleep`, `psrr_*`, `pm_*`) and the derived setups (`RippleRun`/`RippleSleep`, `LoopRun`/`LoopSleep`). Adding a third mode (`Standby`) touches 8 places.
- **`Sleep: Run`** reads as "Sleep is a kind of Run". It's a sibling, not a child.
- **Override granularity.** `vout: Load { i: … }` in `Sleep` either replaces the whole `Load` (dropping `c`) or merges. The draft doesn't say, so the author restates `c` to be safe.
- **`ac: 1V` added to a supply** restates the supply's range. Again U2.
- **The simulation window** (`tran: 300us`) has nowhere to live; draft 2 dropped v0.1's `tran: 0ms..=20ms` bench field.
- **`vout.i`: knob or probe?** In `iq`, `vout.i` means the load setting (a knob). `dc(vin.i)` is a measurement. Both are `port.i`.
- **`theta_ja`** belongs to `q1`'s mounting and is owned by layout. It isn't a port, and bare `theta_ja` in `tj` resolves to nothing visible.
- **Hidden setups.** The draft's own LDO uses `LoadStep<Self>` and `PowerUp<Self>` from `impl Regulator`. A reader can't see the step size, and with modes the template can't know which mode's range to step across.
- **Three meanings of `impl`**: `impl Regulator for` (a trait: gets templates), `impl Circuit for` (the insides), `impl Contract for` (the promises). A novice can't tell which one is "the circuit".

---

## 4. Bridge rectifier + reservoir: steady state, inrush vs turn-on phase, a cable drop

TD §2: 17 V pk ± 10%, 47–63 Hz, 2200 µF ± 20%. Surge corners give 10.29 A; turn-on at 90° gives **30.71 A against 30 A**. The fault: the mains cord is pulled for 10–100 ms and plugged back in.

```rust
pub block Rectifier {
    ac:  AcPair<In>,                     // ✗ one floating source across two terminals (a transformer secondary)
    out: Power<Out>,
    gnd: Ground,
}

impl Circuit for Rectifier {
    let d1    = Diode        { a: ac.a, k: out };
    let d2    = Diode        { a: ac.b, k: out };
    let d3    = Diode        { a: gnd,  k: ac.a };
    let d4    = Diode        { a: gnd,  k: ac.b };
    let c_res = Electrolytic { p: out,  n: gnd, value: 2200uF ± 20%, esr: 25mΩ..=100mΩ };
}

setup Mains for Rectifier {
    ac:   Sine { amp: 17V ± 10%, freq: 47Hz..=63Hz, z: 0.45Ω..=0.55Ω },
    out:  Load { r: 95Ω..=105Ω },
    temp: -10°C..=60°C,
}
setup Steady for Rectifier: Mains {
    tran: 8 cycles,                                                 // ✗ a window in cycles of a knob
}
setup Inrush for Rectifier: Mains {
    ac:    Sine { amp: 17V ± 10%, freq: 47Hz..=63Hz, z: 0.45Ω..=0.55Ω,   // restated to add one field
                  phase: 0°..=180° },                                     // worst at 90°: interior
    start: discharged,                                              // ✗ an initial condition
    tran:  1 cycle,
}
#[fault]                                                            // ✗ marks a setup outside `accepts`
setup CableDrop for Rectifier: Steady {
    ac:   Sine { amp: 17V ± 10%, freq: 47Hz..=63Hz, z: 0.45Ω..=0.55Ω,
                 drop: Open { at: 8 cycles, for: 10ms..=100ms } },  // ✗ a timed event on the source
    tran: 8 cycles + 100ms + 4 cycles,                              // ✗ window depends on a knob and the fault
}

impl Contract for Rectifier {
    accepts Mains;

    spec v_min(env: Steady)  { require tran(out.v).last_cycle().min() >= 12V; }       // ✗ last_cycle()
    spec ripple(env: Steady) { require tran(out.v).last_cycle().pp() <= 1V; }
    spec d_peak(env: Steady) { require tran(d1.a.i).last_cycle().max() <= 2A; }
    spec surge(env: Inrush)  { require tran(d1.a.i).max() <= 30A; }                   // corners 10.3 A; 90°: 30.7 A
    spec holdup(env: CableDrop) {
        require tran(out.v).crossing(falling 9.5V).since(ac.drop.start) >= 20ms;      // ✗ event times
    }
    spec replug(env: CableDrop) {
        require tran(d1.a.i).after(ac.drop.end).max() <= 30A;
    }
}
```

**45 lines.** Issues:

- **A floating two-terminal source.** `AcPair<In>` is invented. Two `Analog<In>` ports each get their own grounded source in the default bench (TD §2.3); a transformer secondary is one source across both.
- **Adding one field restates the `Sine`** (twice: `Inrush`, `CableDrop`). A typo in the restated amplitude silently changes the range.
- **The worst case lives inside a range.** `phase: 0°..=180°` in a setup means "for all", the same as any range, and the engine checks its corners (0°, 180°) and passes. The text can't mark it as a swept axis. The draft already has the form for it (`for f in …` in `output_z`), but only for the measure's own axis, not a setup field.
- **Windows in cycles of a knob.** 8 cycles are 170 ms at 47 Hz and 127 ms at 63 Hz. `tran: 8 cycles`, `last_cycle()` and `at: 8 cycles` are all invented.
- **Initial state** (`start: discharged`) has no syntax.
- **The fault is a setup marked `#[fault]`.** The core says faults are timed events with `during`/`after` windows; draft 2 gives them no syntax. Measures need the event's times: `.since(ac.drop.start)`, `.after(ac.drop.end)`.
- **An expert trap in `replug`:** the phase at which mains returns is set by `for` mod the period, so the corners of `for` (10 ms, 100 ms) both return at a zero crossing at 50 Hz. The 90° re-plug is interior again.
- **`surge ≤ 30 A` is the 1N4007's IFSM.** That's a part rating, which language.md §8.7 makes an automatic check. But IFSM is defined for one 8.3 ms half-sine, so the automatic check needs a pulse-shaped rating, not a flat abs-max.

---

## 5. In-amp with CMRR, and a load cell with 2-point calibration

PA §2.C: a 3-op-amp in-amp (G = 100) on a 350 Ω bridge; CMRR ≥ 100 dB at DC and 60 Hz; total error ≤ ±200 ppm FS after a 2-point calibration at 25 °C, over −40…85 °C and Vexc ± 5%.

```rust
// ───────────── in_amp.spl ─────────────
pub block InAmp<A: OpAmp = Tl072, const GAIN: f64 = 100> {
    inp:  Analog<In>,
    inn:  Analog<In>,
    vref: Analog<In>,
    out:  Analog<Out>,
    vpos: Power<In>,
    vneg: Power<In>,
    gnd:  Ground,
}

impl Circuit for InAmp {
    net o1;  net o2;  net g1;  net g2;  net dn;  net dp;
    let a1  = A        { inp: inp, inn: g1, out: o1, vpos, vneg };  // ✗ a1, a2 are one TL072: no "dual"
    let a2  = A        { inp: inn, inn: g2, out: o2, vpos, vneg };
    let a3  = A        { inp: dp,  inn: dn, out: out, vpos, vneg };
    let rf1 = Resistor { a: o1, b: g1, value: 10k ± 0.1%, tc: ±25ppm/K };
    let rf2 = Resistor { a: o2, b: g2, value: 10k ± 0.1%, tc: ±25ppm/K };
    let rg  = Resistor { a: g1, b: g2, value: 20k / (GAIN - 1) ± 0.1%, tc: ±25ppm/K };   // 202.02 Ω
    let r4  = Resistor { a: o2, b: dn,   value: 10k ± 0.1%, tc: ±25ppm/K };   // ✗ r4..r7 are one matched
    let r5  = Resistor { a: dn, b: out,  value: 10k ± 0.1%, tc: ±25ppm/K };   //   network: no lot / ratio
    let r6  = Resistor { a: o1, b: dp,   value: 10k ± 0.1%, tc: ±25ppm/K };
    let r7  = Resistor { a: dp, b: vref, value: 10k ± 0.1%, tc: ±25ppm/K };
}

setup Operating for InAmp {
    inp:  Signal { v: 2.25V..=2.76V, z: 350Ω ± 0.1% },   // ✗ per-port ranges lose the pairing:
    inn:  Signal { v: 2.25V..=2.76V, z: 350Ω ± 0.1% },   //   inp − inn now spans ±510 mV, not ±10 mV
    vref: Signal { v: 0V, z: ..=1Ω },
    out:  Load   { r: 10kΩ.., c: ..=100pF },
    vpos: Supply { v: 12V ± 5% },
    vneg: Supply { v: -12V ± 5% },
    temp: -40°C..=85°C,
}
setup Diff for InAmp: Operating {                        // AC across the pair
    inp: Signal { v: 2.5V, ac: 0.5V,  z: 350Ω ± 0.1% },
    inn: Signal { v: 2.5V, ac: -0.5V, z: 350Ω ± 0.1% },  // ? a negative AC amplitude as "180° apart"
}
setup Common for InAmp: Operating {                      // AC on both, in phase
    inp: Signal { v: 2.5V, ac: 1V, z: 350Ω ± 0.1% },
    inn: Signal { v: 2.5V, ac: 1V, z: 350Ω ± 0.1% },
}

impl Contract for InAmp {
    accepts Operating;
    spec gain(env: Diff) {
        require ac(out.v / (inp.v - inn.v)).at(1kHz).mag() within GAIN ± 0.5%;
    }
    spec cmrr(dm: Diff, cm: Common) {                    // ? same board? same temperature?
        let adm = ac(out.v).at(f) on dm;                 // ? what does `on` bind to?
        let acm = ac(out.v).at(f) on cm;
        require (adm / acm).db() >= 100dB for f in [1Hz, 60Hz];   // ✗ a set of points; "at DC" again
    }
    spec offset(env: Operating) { require dc(out.v) / GAIN within ±3mV at inp.v - inn.v = 0V; }
}

// ───────────── load_cell.spl ─────────────
pub block LoadCell { vexc: Power<In>, vpos: Power<In>, vneg: Power<In>, adc: Analog<Out>, gnd: Ground }

impl Circuit for LoadCell {
    net inp;  net inn;
    let bridge = StrainBridge { exc: vexc, gnd, outp: inp, outn: inn, r: 350Ω ± 0.1% };
    let amp    = InAmp<Tl072, 100> { inp, inn, vref: gnd, out: adc, vpos, vneg, gnd };
}

setup Operating for LoadCell {
    vexc: Supply { v: 5V ± 5% },
    vpos: Supply { v: 12V ± 5% },
    vneg: Supply { v: -12V ± 5% },
    adc:  Load   { r: 1MΩ.., c: ..=20pF },
    bridge.x: 0..=0.004,                                 // ✗ the measurand: a part's field, not a port
    temp: -40°C..=85°C,
}
setup CalZero for LoadCell: Operating { bridge.x: 0,     temp: 25°C }
setup CalSpan for LoadCell: Operating { bridge.x: 0.004, temp: 25°C }

impl Contract for LoadCell {
    accepts Operating;
    spec total(zero: CalZero, span: CalSpan, env: Operating) {   // ? is vexc shared with the factory runs?
        let reading = dc(adc.v) / vexc.v;                          // ? a value, or a measure to evaluate later?
        let r0 = reading on zero;
        let r1 = reading on span;
        let x  = (reading on env - r0) / (r1 - r0) * 0.004;
        require (x - bridge.x) / 0.004 within ±200ppm;             // ? `bridge.x` from which setup? `±x` alone
    }
    spec noise(env: Operating) {
        require noise(adc.v).band(0.1Hz..=10Hz).rms() <= 0.2ppm * 0.004 * 5V * 100;   // ✗ referred to FS
    }
}
```

**82 lines.** Issues:

- **Per-port ranges lose the pairing (wrong verdict).** The bridge gives cm = 2.5 V ± 5% and dm ≤ 10 mV. Written per port, `inp` and `inn` range independently, so the engine checks `inp − inn` up to ±510 mV, and the G = 100 stage saturates: a **false FAIL** on every spec. The only fix inside draft 2 is to model the bridge as a part inside the block, which defeats the in-amp's own contract.
- **Differential AC as `ac: 0.5V` / `ac: -0.5V`.** A negative amplitude as a phase flip is a trick, not a statement.
- **The sharing rule is missing (U5).** CMRR needs `dm` and `cm` on **the same board at the same temperature and supply**; calibration needs the same board but **factory temperature (25 °C) vs field temperature**. And `vexc`: the factory's excitation isn't the field's. Draft 2 says nothing, so both readings are possible and they give different verdicts.
- **`let reading = …` then `reading on zero`.** In a one-setup spec a `let` is a value; here it's a template evaluated later in three setups. Same syntax, different meaning.
- **`on` precedence.** `mcu.code on full - mcu.code on zero` (the draft's own line) works only if `on` binds tighter than `-`. `ac(out.v).at(f) on dm` then applies to the whole chain. Nobody will guess this.
- **`bridge.x`** (the measurand) is a part parameter. Setting it from a setup needs a path to a part field; the final `require` reads `bridge.x` without saying from which of the three setups.
- **`at inp.v - inn.v = 0V`** (offset): `at` again, now meaning a fixed test point, one character away from `.at(1kHz)`.
- **Matched networks and duals.** r4…r7 are one 0.01%-matched network; a1 and a2 are one TL072. PA shows the verdict hangs on the dual's drift correlation (∓54,000 ppm vs 0). Neither is expressible, and an expert will look for it first.
- **CMRR "at DC"** is a DC-sweep measure (Δout / Δcm), not an AC point at 1 Hz.

---

## 6. Across the five

### 6.1 Patterns that repeat

| Pattern | Count in the five | Sugar (§7) |
|---|---|---|
| `(env: Operating)` or `(env: <the only setup>)` | 8 of 35 specs; 6 of 6 in the Sallen-Key | P1: no parameter = the accepted setup |
| `spec x(env: S) { require <one expr>; }` (a body holding one expression) | 29 of 35 | P1: `spec x: <expr> on S;` |
| `temp: ambient` / `temp: <range>` in every root setup | 5 of 5 | inherit the project's `ambient` unless narrowed (language.md §8.2 already says this) |
| a derived setup restating a whole shape to change one field | about 10 lines | P2: `ac.phase: 0°..=180°` |
| two setups that differ only in their parent | 4 pairs (I2C 2, LDO 2) | P2: `on Release + Fitted`; P3: modes |
| the same spec per mode or per variant | 5 pairs (LDO 3, I2C 2) | P3 |
| the same spec per bus line | 2 (I2C) | `for line in [sda, scl]` (language.md §8.6) |
| `ac(vout.v / vin.v)` repeated | 6 | P8: contract-level `let` |
| `setup X for Block` repeating the block name | 26 | keep (setups are named per block), but the derived form can drop it: `setup Ripple: Operating` |
| `impl Circuit for` / `impl Contract for` | 2 per block | P7: `circuit X` / `contract X` |

### 6.2 What a novice would misread

| Text | Misreading | Truth |
|---|---|---|
| `require dc(vout.v) within …` | "the spec requires this from the outside" | it's what the block **guarantees** |
| `spec cmrr(dm: Diff, cm: Common)` | two boards, two measurements | one board, two benches (and it must be) |
| `Signal { v: 0V..=100mV }` | the signal's amplitude | a DC level (range knob); AC has its own field |
| `vout: Load { r: 10kΩ.. }` | "a load of at least 10 k is light, so safe" | the worst case for stability is often the lightest load, or an interior C (ee §3.3) |
| `phase: 0°..=180°` | every phase is checked | only 0° and 180° are, unless marked as an axis |
| `setup Sleep: Run` | Sleep is a variant of Run | they're siblings |
| `impl Regulator for Ldo3v3 {}` | an empty, useless line | it silently brings in `LoadStep` and `PowerUp` |
| `accepts Operating` vs `setup Operating` | the same thing said twice | the setup is a set of conditions; `accepts` says "and my neighbors must stay inside it" |
| `rated` vs `accepts` | two words for ranges | abs-max (damage) vs operating (specs hold) |
| a PASS | "every board works" | at `sigma(3)` (the default), 99.87% one-sided per spec, and nothing in the text shows the confidence |
| `x.at(1kHz)` and `… at vin.v = 4.3V` | the same `at` | a point on an axis vs a fixed test condition |
| `GAIN ± 1%` and `3.0dB ± 0.5dB` and `1% of v.at(end)` | three kinds of `±` | relative, absolute, and a new `of` operator |

### 6.3 What an expert would miss

- **Which instantiation was checked** (U10), and whether the part models respond to temperature (PA: the OPA171 doesn't).
- **The confidence** per spec: `sigma(3)` by default, invisible in the text. Datasheet readers expect min/typ/max columns.
- **Interior worst cases:** phase margin vs C_load (47.6° at 944 pF), surge vs phase, re-plug phase.
- **Correlations:** matched networks, dual op-amp halves, a stereo pair (`lot`).
- **Measure definitions:** which −3 dB, what "settled" means, where the loop is broken, CMRR at DC.
- **Loads that pull up** (open-drain, pull-ups) and loads to a reference other than ground (the op-amp's VL = VDD/2 in the MCP6001 fixture).
- **Source-side disturbances** as part of the source (ripple on a supply, a cable drop), which the core says belong to the source (ee §1.6) and which draft 2 can't attach to a `Supply`.
- **What the verdict rests on:** a layout budget, a typical-only number (TCVos), a fault outside `accepts`.

### 6.4 How forms and the agent would edit it

| Edit | Draft 2: lines touched | Local? | After §7 |
|---|---|---|---|
| Change the load range | 1 (setup field) | yes | 1 |
| Change a spec's bound | 1 | yes | 1 |
| Add a spec in the main setup | 1–4 (a body) | yes | 1 row |
| Add a third mode (`Standby`) to the LDO | 1 setup + 4 derived setups + 4 specs + `accepts` | **no** | 1 line (`mode Standby {…}`) |
| Add the module variant to the I2C | 3 setups + 2 specs + `accepts` | **no** | 1 line |
| Change the mains amplitude | 3 (restated in `Inrush`, `CableDrop`) | **no, and a missed copy is silent** | 1 |
| Rename a setup | every spec parameter that names it | no (a refactor) | fewer (default setup) |
| Make a spec hold in Sleep only | rename + move to the Sleep setup | no | append `in Sleep` |
| Swap the op-amp | 1 (placement) | yes, but the verdict it shows was for the default `A` | 1 + the `#[check]` list |

**The spec table form.** A one-line spec maps to a row: *name · measure · relation · bound · setup · mode*. A body spec (CMRR, calibration) can't be a row; the form shows it as a "custom" row that opens the text. That's fine for the 6 in 35 that need a body; today all 35 are bodies.

**The agent.** Edits the agent makes are safest when each fact has one home. Today the mains amplitude has three homes and a mode has eight. The agent must either find every copy (and a missed one changes a verdict silently), or refactor. A rule to adopt with the proposals: **every fact has exactly one line**, and the formatter rejects a restated field that equals the parent's.

---

## 7. Proposed improvements, before and after

### P1. One-line specs; no parameter means the accepted setup

```rust
// before
spec gain(env: Operating) {
    let h = ac(vout.v / vin.v);
    require h.at(1kHz).mag() within GAIN ± 1%;
}
spec settling(env: InputStep) {
    let v = tran(vout.v);
    require v.time_to_within(1% of v.at(end)) <= 50us;
}

// after
spec gain:     ac(vout.v / vin.v).at(1kHz).mag() within GAIN ± 1%;
spec settling: tran(vout.v).settle(1%) <= 50us  on InputStep;
```

The body form stays for multi-step and multi-setup specs. `on S` picks a setup; no `on` means every setup in `accepts`. This is language.md §8.3's grammar, kept.

### P2. Field-path overrides, and setup sums

```rust
// before
setup Inrush for Rectifier: Mains {
    ac: Sine { amp: 17V ± 10%, freq: 47Hz..=63Hz, z: 0.45Ω..=0.55Ω, phase: 0°..=180° },
}
setup ReleaseSdaFitted for SensorI2c: Fitted { mcu.sda: Drive { low -> released, at: 1us } }

// after
setup Inrush for Rectifier: Mains { ac.phase: 0°..=180°, start: discharged }
spec rise: … on Release + Fitted;           // a sum: fields of both; a conflict is an error
```

Rule: a derived setup **merges field by field**; a whole shape (`vout: Load {…}`) replaces, a path (`vout.i: …`) merges. The formatter prefers paths.

### P3. Modes inside a setup

```rust
// before: 2 root setups, 4 derived, 6 specs for 3 facts, two `accepts`
setup Run   for Ldo3v3 { vout: Load { i: 1mA..=300mA, c: ..=10uF }, … }
setup Sleep for Ldo3v3: Run { vout: Load { i: 10uA..=100uA, c: ..=10uF } }
setup LoopRun   for Ldo3v3: Run   { break: r_top.a };
setup LoopSleep for Ldo3v3: Sleep { break: r_top.a };
spec pm_run(env: LoopRun)     { require ac(loop_gain).phase_margin() >= 45°; }
spec pm_sleep(env: LoopSleep) { require ac(loop_gain).phase_margin() >= 45°; }

// after
setup Operating for Ldo3v3 {
    vout: Load { c: ..=10uF },
    mode Run   { vout.i: 1mA..=300mA }
    mode Sleep { vout.i: 10uA..=100uA }
}
setup Loop for Ldo3v3: Operating { break: r_top.a }         // inherits both modes
spec pm: ac(loop_gain).phase_margin() >= 45°  on Loop;      // checked in Run and in Sleep
spec iq: dc(vin.i) - dc(vout.i) <= 3mA        in Sleep;     // narrowed to one mode
```

The report prints one row per mode: `pm [Sleep]: FAIL 32.5°`. A mode is exactly PW §5.3's scenario: a discrete knob enumerated in full, never nudged, that sets other ranges. The I2C module is the same construct (`mode Bare { ext: open }`, `mode Fitted {…}`). A **build** variant (a DNP footprint on this board) is different and belongs in the circuit: `#[fit(Full)] let rm = …;`.

### P4. Paired sources, and fields on internal paths

```rust
// before: independent ports, wrong range
inp: Signal { v: 2.25V..=2.76V, z: 350Ω ± 0.1% },
inn: Signal { v: 2.25V..=2.76V, z: 350Ω ± 0.1% },

// after: one source across two ports
(inp, inn): Pair { dm: -10mV..=10mV, cm: 2.5V ± 5%, z: 350Ω ± 0.1% },
setup Diff   for InAmp: Operating { (inp, inn).dm.ac: 1V }
setup Common for InAmp: Operating { (inp, inn).cm.ac: 1V }

// the transformer secondary is the same shape
(ac.a, ac.b): Sine { amp: 17V ± 10%, freq: 47Hz..=63Hz, z: 0.45Ω..=0.55Ω },

// and a setup may name an internal net or a part field, marked by owner
#[owner(layout)] sda.stray: Capacitor { c: 20pF..=100pF },   // reported as a constraint: ≤ 121.1 pF
#[owner(layout)] q1.theta_ja: 60K/W..=100K/W,
bridge.x: 0..=0.004,                                          // the measurand
```

### P5. A sharing rule for multi-setup specs

State it once in the language: **in one spec, every setup is the same board** (all statistical knobs shared). **Range knobs are shared too, unless a setup pins them.** `fresh` opts out.

```rust
// before: meaning undefined
spec total(zero: CalZero, span: CalSpan, env: Operating) {
    let reading = dc(adc.v) / vexc.v;
    let x = (reading on env - reading on zero) / (reading on span - reading on zero) * 0.004;
    require (x - bridge.x) / 0.004 within ±200ppm;
}

// after: pinned temp and x differ by setup; vexc is the factory's, not the field's
spec total(zero: CalZero, span: CalSpan, env: Operating) fresh vexc {
    let reading = dc(adc.v) / vexc.v;                               // a measure, evaluated per setup
    let x = (reading[env] - reading[zero]) / (reading[span] - reading[zero]) * 0.004;
    ensure (x - bridge.x[env]) / 0.004 within 0 ± 200ppm;
}
spec cmrr(dm: Diff, cm: Common) {                                   // same board, same temp, same supply
    ensure (ac(out.v)[dm] / ac(out.v)[cm]).at(60Hz).db() >= 100dB;
}
```

`x[setup]` replaces `x on setup`: it has an obvious precedence, and it reads as "x in that setup".

### P6. A time vocabulary: windows, cycles, start state, events

```rust
// before
setup Steady for Rectifier: Mains { tran: 8 cycles }
spec v_min(env: Steady) { require tran(out.v).last_cycle().min() >= 12V; }
setup CableDrop for Rectifier: Steady {
    ac: Sine { amp: 17V ± 10%, freq: 47Hz..=63Hz, z: 0.45Ω..=0.55Ω, drop: Open { at: 8 cycles, for: 10ms..=100ms } },
    tran: 8 cycles + 100ms + 4 cycles,
}

// after
setup Steady for Rectifier: Mains { window: 8 cycles of ac }        // `cycle` = 1 / ac.freq, per run
spec v_min: tran(out.v).last(1 cycle).min() >= 12V   on Steady;

#[fault]
setup CableDrop for Rectifier: Steady {
    event drop: ac.open { at: 8 cycles, for: 10ms..=100ms },       // the source opens, then returns
    window: until drop.end + 4 cycles,
}
spec holdup: tran(out.v).crossing(falling 9.5V) - drop.start >= 20ms   on CableDrop;
spec replug: tran(d1.a.i).after(drop.end).max() <= 30A                 on CableDrop;
```

A swept axis inside a spec stays `for`, now allowed on a setup field:

```rust
spec surge: tran(d1.a.i).max() <= 30A  on Inrush  for ac.phase in 0°..=180°;   // searched, not cornered
```

And one step spelling everywhere: `Step { a -> b, edge: 1us, at: 50us }`, as a value of the field it steps (`vout.i: Step {…}`, `mcu.sda: Step { low -> released }`).

### P7. Keywords: `circuit`, `contract`, `ensure`, `with`

```rust
// before
impl Circuit for GainStage { … }
impl Contract for GainStage { … }
spec psrr(env: Operating) { require ac(vout.v / vin.v).at(100kHz).db() <= -36dB at vin.v = 4.3V; }

// after
circuit GainStage { … }
contract GainStage { … }
spec psrr: ac(vout.v / vin.v).at(100kHz).db() <= -36dB  with vin.v = 4.3V;
```

- `impl X for Y` stays for traits only (`impl Regulator for Ldo3v3`), so it has one meaning.
- `ensure`: the setup is the precondition, the spec the postcondition, exactly Eiffel's `require`/`ensure`. `require` would read backwards to anyone who knows design by contract.
- `with` for a fixed test point (the core's first kind of condition), `for … in` for a swept axis (the third), the setup for ranges (the second). `at` is left to `.at(f)`.

### P8. Contract-level `let`

```rust
// before: ac(vout.v / vin.v) in five specs
// after
contract SallenKeyLp {
    let h = ac(vout.v / vin.v);
    spec peaking:  h.peak_db(ref: dc) within 3.0dB ± 0.5dB;
    spec stopband: h.at(10 * F0).db() <= -38dB;
}
```

A contract `let` is a measure (evaluated per run, D-F), not a number. The editor shows it once, as a named measure column.

### P9. Checked instantiations for generic blocks

```rust
// before: checked for the default A only, silently
pub block SallenKeyLp<A: OpAmp = Opa171, …> { … }

// after
#[check(A = [Opa171, Tl072, Mcp6001])]
contract SallenKeyLp { … }
// a placement with an A outside the list: warning "SallenKeyLp<Lm358>: contract not checked for this A"
```

### P10. Named generic arguments; committed values

```rust
// before
let lp = SallenKeyLp<Tl072, 2kHz, 0.707> { … };
let c1 = Capacitor { a: n1, b: vout, value: 2 * Q / (2π * F0 * R) ± 5% };

// after
let lp = SallenKeyLp<A = Tl072, F0 = 2kHz, Q = 0.707> { … };
let c1 = Capacitor { a: n1, b: vout, value: 41.2nF ± 5% from 2 * Q / (2π * F0 * R) };   // language.md §5.6
```

### Measures to add (not syntax, but every circuit hit one)

| Need | Proposed | Circuit |
|---|---|---|
| f0, Q | `h.poles().f0`, `.q` | Sallen-Key |
| −3 dB against a named reference | `h.f_high(-3dB, ref: dc \| peak)` | Sallen-Key (PA: 1188 vs 1389 Hz) |
| "at DC" | `h.dc()` (from the operating point's small-signal gain, not a 1 Hz point) | Sallen-Key, in-amp |
| loop gain | `loop_gain` with `break:` in the setup | LDO |
| settling | `.settle(1%)`, flagged non-smooth | LDO, GainStage |
| rise/fall between levels | `.rise_time(lo, hi)` | I2C |
| event-relative windows | `.since(ev)`, `.after(ev.end)`, `.last(1 cycle)` | rectifier |

---

## 8. Two circuits, rewritten with P1–P10

### The LDO: 49 → 27 lines, 11 specs → 8 (each with a row per mode)

```rust
pub block Ldo3v3 { vin: Power<In>, vout: Power<Out>, gnd: Ground }

circuit Ldo3v3 {
    // … the same 13 lines as before …
}

setup Operating for Ldo3v3 {
    vin:  Supply { v: 3.6V..=5.5V, z: ..=0.2Ω },
    vout: Load   { c: ..=10uF },
    #[owner(layout)] q1.theta_ja: 60K/W..=100K/W,
    mode Run   { vout.i: 1mA..=300mA }
    mode Sleep { vout.i: 10uA..=100uA }
}
setup Ripple  for Ldo3v3: Operating { vin.ac: 1V }
setup Loop    for Ldo3v3: Operating { break: r_top.a }
setup Step    for Ldo3v3: Operating in Run { vout.i: Step { 1mA -> 300mA, edge: 1us }, window: 300us }
setup Release for Ldo3v3: Operating in Run { vout.i: Step { 300mA -> 1mA, edge: 1us }, window: 3ms }

contract Ldo3v3 {
    accepts Operating;
    rated vin.v within -0.3V..=6V;

    spec vout_dc:    dc(vout.v) within 3.3V ± 3%;
    spec psrr:       ac(vout.v / vin.v).at(100kHz).db() <= -40dB    on Ripple;
    spec pm:         ac(loop_gain).phase_margin() >= 45°            on Loop;
    spec undershoot: tran(vout.v).min() >= 3.1V                     on Step;
    spec settle:     tran(vout.v).settle(1%) <= 200us               on Step;
    spec overshoot:  tran(vout.v).max() <= 3.45V                    on Release;
    spec iq:         dc(vin.i) - dc(vout.i) <= 3mA                  in Sleep;
    spec tj:         temp + q1.theta_ja * dc(q1.power) <= 125°C     in Run;
}
```

Adding `Standby` is one line. Every spec is one row in the table form.

### The I2C bus: 57 → 29 lines, 7 setups → 4

```rust
pub block SensorI2c<const SPEED: I2cSpeed = Fast> { vdd: Power<In>, gnd: Ground, ext: I2c<Target> }

circuit SensorI2c {
    net sda = ext.sda;  net scl = ext.scl;
    let mcu  = Stm32I2c { vdd, gnd, sda, scl };
    let t1   = Tmp117   { vdd, gnd, sda, scl };
    let t2   = Bme280   { vdd, gnd, sda, scl };
    let rp_d = Resistor { a: vdd, b: sda, value: 2.2k ± 1% };
    let rp_c = Resistor { a: vdd, b: scl, value: 2.2k ± 1% };
}

setup Operating for SensorI2c {
    vdd:  Supply { v: 3.3V ± 3% },
    temp: -40°C..=85°C,
    #[owner(layout)] sda.stray: Capacitor { c: 20pF..=100pF },
    #[owner(layout)] scl.stray: Capacitor { c: 20pF..=100pF },
    mode Bare   { ext: open }
    mode Fitted { ext: I2cModule { pull_up: 4.7k ± 5%, c: 40pF..=80pF } }
}
setup Release for SensorI2c: Operating { mcu: Drive { sda, scl: low -> released, at: 1us } }
setup Pull    for SensorI2c: Operating { mcu: Drive { sda: released -> low, at: 1us } }
setup Hold    for SensorI2c: Operating { mcu: Drive { sda: low } }

contract SensorI2c {
    accepts Operating;
    for line in [sda, scl] {
        spec rise[line]: tran(line.v).rise_time(0.3 * vdd.v, 0.7 * vdd.v) <= SPEED.t_r_max  on Release;
    }
    spec fall: tran(sda.v).fall_time(0.7 * vdd.v, 0.3 * vdd.v) >= 20ns * vdd.v / 5.5V     on Pull;
    spec vol:  dc(sda.v) <= 0.4V                                                           on Hold;
    spec sink: dc(mcu.sda.i) <= 3mA                                                        on Hold;
}
```

The report reads `rise[sda] [Fitted]: PASS if sda.stray ≤ 121.1 pF (layout)`. `Release` drives both lines at once: they are separate nets, so one run serves both specs.

### The Sallen-Key contract: 21 → 12 lines

```rust
#[check(A = [Opa171, Tl072, Mcp6001])]                 // the instantiations this contract is checked for
contract SallenKeyLp {
    accepts Operating;
    let h = ac(vout.v / vin.v);

    spec cutoff:   h.f_high(-3dB, ref: dc) within 1.39 * F0 ± 7%;
    spec peaking:  h.peak_db(ref: dc) within 3.0dB ± 0.5dB;
    spec f0:       h.poles().f0 within F0 ± 5%;
    spec q:        h.poles().q  within Q ± 10%;
    spec stopband: h.at(10 * F0).db() <= -38dB;
    spec output_z: vout.z(f) <= 10Ω for f in 10Hz..=10 * F0;
    spec supply:   dc(vdd.i) <= 2mA;
}
```

---

## 9. The path

1. **Decide the small words now** (they touch every line): `circuit` / `contract` items, `ensure`, `with` for fixed points, `x[setup]` for multi-setup reads. (P7, P5's notation.)
2. **Adopt the one-line spec with a default setup** (P1) and contract `let` (P8). They cut most lines and make the spec-table form possible.
3. **Specify setup merging** (P2): field-by-field, paths merge, `A + B` sums. It kills the silent copies.
4. **Add modes to setups** (P3). It's the E5 `Scenario` seam in the language, and it fixes the LDO and the I2C variant together.
5. **Widen setup fields** (P4): pairs across ports, internal nets and part fields, `#[owner(…)]`. The pair fixes a wrong verdict, so it isn't optional.
6. **Write the sharing rule** (P5) before any multi-setup spec is implemented.
7. **Then the time vocabulary** (P6) with the transient tier, and the measures table, starting with `poles()` and `ref:` for the Sallen-Key (ladder step 1).
8. **Write these five circuits again** in the next draft as its test suite; each proposal above should show up as a shorter, single-home line there.
