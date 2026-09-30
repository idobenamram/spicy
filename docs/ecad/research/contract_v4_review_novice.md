# Draft 4 Read by a Novice: Misreadings, Two New Circuits, Errors, Tweaks

> 2026-09-30 · A final review of `contract_syntax_v4.md` from the seat of a **novice electrical engineer**: someone who knows Ohm's law, dividers, RC corners and how to read a datasheet, but has written little code and no Rust. The §0 decisions of draft 4 stand. This review points out real defects where it finds them, and otherwise proposes only wording, formatter, error and editor fixes.

---

## 0. Summary

**Verdict.** A novice can read draft 4 well enough to guess *what* each spec checks. Where they go wrong is *what surrounds it*: which setup applies, which knobs are searched, where the AC source sits, and what a PASS covers. The one-line spec is the language's best feature for novices. It reads like a datasheet row, and most of the misreadings below are in setups and blocks, not in specs.

**The worst misreadings** (§1):

| # | Text | A novice thinks | Truth |
|---|---|---|---|
| 1 | `emits wake: Step on vdd.i {…}` | the MCU *sends out* a wake pulse | the parent *may cause* a current step in the MCU. Nothing is emitted |
| 2 | `rated vin.v within …` | "rated" = the normal operating range (a 16 V-rated cap) | absolute maximum (damage), checked on every setup |
| 3 | `setup = Operating;` | "picks the default bench" | also a **promise precondition**: every parent is checked to stay inside it |
| 4 | `… for f in 10Hz..=1MHz in Run` | "f in (10 Hz..1 MHz in Run)"? | a frequency sweep, then a mode filter. Three different `in`s in one line |
| 5 | `z: ..=0.5Ω, …, ..Operating` | `..Operating` = "up to Operating" | two unrelated meanings of `..` inside the same braces |
| 6 | `#[outside(reason = …)]` | outdoors, or "outside the contract, so not checked" | checked, but tagged as characterization and never published |
| 7 | `// Reversed: no spec needed` | the board survives reversed USB | as drawn it **fails**: the LDO's `rated vin.v ≥ −0.3 V` sees about −5 V |
| 8 | `pub spec` vs `spec` | `pub` = important, and unmarked specs aren't checked | both are checked. `pub` = a parent may rely on it |

**Real defects found** (not wording; each needs a rule, none reopens a decision):

- **D1.** A derived setup and a mode set the same field (`LoadStep` sets `vout.i: Step`, `mode Run` sets `vout.i: 5mA..=50mA`). No rule says which one wins.
- **D2.** Events have two spellings in setups: `event: mcu.wake at 1ms` and `event drop: usb.open { at: 1ms, … }`.
- **D3.** dB has two spellings: `.db()` as a method and `db(a / b)` as a function.
- **D4.** The `Reversed` comment implies a pass, but the verdict as drawn is FAIL.
- **D5.** `Pair { z: 350Ω ± 0.1% }` doesn't say whether the two legs draw their tolerances **independently**. CMRR depends almost entirely on that.
- **D6.** Where the AC stimulus goes is implicit: the ratio's denominator in `ac(vout.v / vin.v)`, but a field in `.dm.ac: 1V`. There is no rule for a ratio over an internal net, which a novice writes naturally and which gives a wrong answer (§2.2).
- **D7.** No sign convention is given for `p.i`.
- **D8.** No way is shown to measure a *part's* current or power (`r1.i`, `r1.p`), or to say whether part ratings are checked automatically.
- **D9.** The symbols `Ω`, `°C`, `µ` and `±` are hard to type, and the example mixes `47k` with `10kΩ`.

**The tweaks** (§4), all within the decisions:
- rename `emits` to `event`;
- spell events one way;
- rename the event field `for:` to `lasting:`;
- one dB form;
- put `for x in` last in the canonical clause order;
- write one-sided field ranges as `<= 0.5Ω` / `>= 10kΩ`;
- `Sweep { … }` in braces, like `Step`;
- accept ASCII unit aliases;
- fix the `Reversed` comment;
- add a std `AdcPin` load, so the likeliest silent false PASS can be written at all.

**The two new circuits** (§2) took 14 guesses between them. The LED driver needed only small ones. The ADC input was the hard one: the novice couldn't express the ADC's sampling load. The natural "filter corner" spec gave 1.6 kHz when the real corner is 167–532 Hz.

---

## 1. The v4 example, item by item, as a novice reads it

Key: ✓ read correctly · **✗ misread** · ? had to guess.

### 1.1 Project file

| Text | Novice reading | Mark | Truth, and the fix |
|---|---|---|---|
| `env ambient: Temperature in -10°C..=60°C;` | "the temperature range of the project" | ✓ | right. `..=` is new but guessable ("up to and including") |
| `env life: Duration in 0y..=10y;` | "aging over ten years"… used where? | ? | nothing in the example uses it. The novice can't tell whether it's searched everywhere or only where named. Docs: "every part with an aging model is searched over `life`" (if that's the rule) |
| `const confidence: Confidence = sigma(3);` | "just a constant; nobody uses it" | **✗** | if the engine reads it **by name**, it's a magic name. Make it visible: an error if two exist, a hover that says "all verdicts: 3σ, 99.87% one-sided per spec", and the verdict panel shows it |
| `env` vs `const` | "a variable vs a constant" | ✓ roughly | `env` is **searched** over its whole range. Worth one sentence in the docs: "`env` = the engine tries every value; `const` = one value" |

### 1.2 `CeAmp`

| Text | Novice reading | Mark | Truth, and the fix |
|---|---|---|---|
| `pub block CeAmp { vcc: Power<In>, … }` | "a symbol with pins; `pub` means… public?" | ? | `pub` here is Rust's "visible to other files". Later `pub spec` means "a parent may rely on it". One word, two meanings (§1.6) |
| `Power<In>`, `Analog<Out>`, `Ground` | "power comes in; the analog signal goes out" | ✓ | the angle brackets are odd but harmless |
| `net base; net emitter;` | "wires with names" | ✓ | the ports are nets too, without being declared: say so in the docs |
| `let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };` | "R1, 47 k, 1 %, between vcc and base" | ✓ | the best line in the language for a novice. But `47k` has no `Ω` while `10kΩ..` does: are both allowed? (D9) |
| `beta: 100..=300` | "beta anywhere from 100 to 300" | ? | is a range on a part searched like temperature (worst case), or treated statistically like `± 1%`? It matters for a 3σ verdict. Docs: "a range is searched; a `±` is statistical" (if that's the rule) |
| `Electrolytic { p: base, n: input, … }` | "+ at base" | ✓ | pin names vary by part (`a/b`, `p/n`, `c/b/e`). The novice needs completion to know them (§5) |
| `setup Operating for CeAmp { vcc: Supply { v: 12V ± 5% }, temp: ambient }` | "the bench: 12 V ±5 %" | ✓ | **but** "where's the input signal?" Unwritten = an ideal 0 Ω source. The novice doesn't know that until the formatter writes it out, so the formatter should always do so |
| `setup = Operating;` | "assign Operating to a variable `setup`" | **✗ half** | it *is* the contract's `setup` field, so the assignment reading is fine. The hidden half: **parents are checked to stay inside Operating**. A novice who widens Operating to make a spec pass has also loosened the promise to every parent without noticing. Hover text (§5) |
| `let h = ac(output.v / input.v);` | "the gain, as a function of frequency" | ✓ | the AC source goes at the denominator's port, implicitly (D6) |
| `spec bias: dc(output.v) within 4.5V..=6.5V;` | "the DC output stays between 4.5 and 6.5 V" | ✓ | + "at every temperature and supply": the novice gets this from the docs, not the text. Show it in the verdict |
| `spec gain: h.at(1kHz).mag() within 4.6 ± 5%;` | "gain at 1 kHz, 4.6 ± 5 %" | ✓ | |
| `spec bass: h.f_low(-3dB) <= 30Hz;` | "−3 dB low corner below 30 Hz" | ✓ ? | −3 dB relative to what: the peak, or the gain at 1 kHz? `f_high` has `ref:` but `f_low` shows none. Docs |
| `spec base_bias: dc(base.v) within …; // internal` | "fine, a test point" | ✓ | the comment helps. Without it, the novice wouldn't know internal specs are never published |

### 1.3 `Ldo3v3`

| Text | Novice reading | Mark | Truth, and the fix |
|---|---|---|---|
| `pub block Ldo3v3: Regulator { … }` | "Ldo3v3 is a kind of Regulator" | ? | what does `: Regulator` add? It brings std setups and specs, but they're invisible. Hover: "adds setups `LoadStep`, `PowerUp` and specs …" |
| `Tlv755p { vin, vout, gnd }` | "the chip's pins vin/vout/gnd go to my nets vin/vout/gnd" | ✓ | the shorthand is guessable once seen |
| `vin: Supply { v: 4.3V..=5.5V, z: ..=0.5Ω }` | "supply 4.3 to 5.5 V; impedance… about 0.5 Ω?" | **✗** | `..=0.5Ω` is "at most 0.5 Ω". A novice reads `..=` with no left side as "approximately". Tweak T6: `z: <= 0.5Ω` |
| `vout: Load { c: ..=20uF }` | "a load of up to 20 µF" | ✓ ? | why does the load have no resistance? The current is set per mode. OK once explained |
| `mode Run { vout.i: 5mA..=50mA }` | "in Run the load draws 5–50 mA" | ✓ | the missing comma after `}` is a surprise (commas everywhere else) |
| `setup LoadStep for Ldo3v3 { vout.i: Step { 5mA -> 30mA, edge: 1us }, window: 300us, ..Operating }` | "a load step; `..Operating` = up to Operating?" | **✗** | struct update, "everything else as in Operating". In the same braces as `..=0.5Ω` it's the most confusing `..` in the draft. And **D1**: Operating's `mode Run` also sets `vout.i`. Which wins in `LoadStep in Run`? |
| `#[outside(reason = "datasheet dropout row …")]` | "outdoors?" / "outside the contract, so not checked" | **✗** | checked, tagged characterization, never `pub`. Hover (§5) |
| `vin.v: Sweep(4.5V -> 3.0V)` | "sweep vin down" | ✓ | but `Sweep(…)` uses parentheses and `Step { … }` braces. And a *setup* sweep vs a *spec* `for x in` sweep: when to use which? (T8) |
| `rated vin.v within -0.3V..=6.5V;` | "the LDO is rated for −0.3 to 6.5 V in" | **✗** | in EE speech "rated" usually means the **operating** rating ("a 16 V-rated cap", "rated current"). The datasheet table is "**Absolute** Maximum Ratings". A novice will paste the *Recommended Operating Conditions* here (4.3–5.5 V), and every setup then fails. Error C0040 and a hover |
| `spec output: dc(vout.v) within 3.3V ± 2%;` | "3.3 V ± 2 %" | ✓ | checked in *each mode*: novices won't guess that without a hint in the verdict ("2 modes × …") |
| `spec psrr: … <= -36dB with vin.v = 4.3V;` | "at 4.3 V in, PSRR better than 36 dB" | ✓ | `with` reads well. The novice asks why this isn't a derived setup like `AtZero`: docs, "`with` for one spec; a setup when several specs or a multi-setup spec need it" |
| `spec dip: tran(vout.v).min() >= 3.25V on LoadStep in Run;` | "on the LoadStep bench, in Run, the dip stays above 3.25 V" | ✓ | the order `on … in …` reads naturally |
| `spec quiescent: dc(vin.i - vout.i) <= 50uA in Sleep;` | "ground current in sleep" | ✓ ? | only if `vin.i` is positive *into* the block and `vout.i` positive *out of* it. Otherwise the subtraction is a sum. **D7** |
| `spec quiescent … in Sleep` vs `env … in -10°C..=60°C` | "`in` means 'inside a range'" | **✗** | here `in` means "in mode". §1.7 counts three `in`s |
| `vin.v.first(vout.v < 3.267V) - 3.267V` | ? | ? | not in the measure list (open item 1). The novice can't read it. Maybe `vin.v.when(vout.v crosses below 3.267V)` |
| `pub spec output_z: vout.z(f) <= 2Ω for f in 10Hz..=1MHz in Run;` | "a for-loop over f… and 'in Run' belongs to the range?" | **✗** | `f` is used before it's introduced, and `in Run` right after a range reads as part of it. T5 puts the sweep last |
| `output` is not `pub`, `output_z` is | "the output voltage is the thing a parent needs most! Why isn't it published?" | **✗ / ?** | by §0, `pub` is for port facts parents compose with. `vout.v` is a port fact. Either the example should publish `output`, or the docs must say why not (e.g. "the parent re-simulates the child, so it needs only the impedances"). **D9-adjacent: a novice can't apply the `pub` rule from this example** |

### 1.4 `GainStage`

| Text | Novice reading | Mark | Truth, and the fix |
|---|---|---|---|
| `GainStage<A: OpAmp = Mcp6001, const GAIN: f64 = 10.09>` | "a template… f64?" | ? | `f64` means nothing to an EE. Allow `Number` (or `Ratio`) as a synonym, or let the type be inferred from the default |
| `value: ((GAIN - 1) * 10k) ± 1%` | "R_f from the gain" | ✓ | nice |
| `vin: Signal { z: 100Ω..=10kΩ }` | "a signal source with 100 Ω–10 k impedance… what amplitude?" | ? | unwritten = ? For AC the level doesn't matter (small signal). Docs |
| `vout: Load { r: 10kΩ.., c: ..=1nF }` | "≥ 10 k, light, safe" | **✗ (physics)** | the lightest load can be the worst for stability (ee §3.3). The text can't fix this. The verdict should say *which* corner failed |
| `vin.wave: Step { 0V -> 100mV, edge: 1us }` | "a step on the input" | ✓ | `wave` vs `v` vs `ac`: which fields a `Signal` has is only learnable from completion (§5) |
| `#[check(A = [Mcp6001, Tlv9001])]` | "test with both op-amps" | ✓ | |
| `h.f_high(-3dB, ref: dc)` | "−3 dB from the DC gain" | ✓ | |
| `tran(vout.v).settle(1%) <= 50us` | "settles to 1 % within 50 µs" | ✓ ? | 1 % of the final value, or of the step? Docs |
| `pub spec input_z: vin.z(1kHz) >= 400kΩ;` | "input impedance" | ✓ | |
| `pub spec supply: dc(vdd.i) <= 150uA;` | "supply current" | ✓ | |

### 1.5 `InAmp`

| Text | Novice reading | Mark | Truth, and the fix |
|---|---|---|---|
| `(inp, inn): Pair { dm: …, cm: …, z: … }` | "dm? cm? a tuple?" | **✗ / ?** | "differential mode / common mode" is textbook but not novice vocabulary. Hover: "dm = inp − inn; cm = (inp + inn)/2" |
| `z: 350Ω ± 0.1%` | "each leg is 350 Ω ± 0.1 %" | ? | **D5**: do the legs vary *independently*? A 0.1 % leg mismatch is exactly what limits CMRR. If they share one draw, the check is optimistic by orders of magnitude |
| `(inp, inn).dm.ac: 1V` | "1 V AC into a ±10 mV input? It'll clip!" | **✗** | in small-signal AC the amplitude is a unit reference, not a real 1 V. Docs, plus an error if it's used in `tran` |
| `spec cmrr(dm: Diff, cm: Common) { … }` | "two boards, two measurements" | **✗** | one board on two benches (the sharing rule). The rule is in §1 of the draft. A hover on the parameter list should repeat it |
| `ac(out.v)[dm]` | "array index?" | ? | "measured in setup `dm`". Once explained, it reads fine |
| `ensure db(a_dm / a_cm) >= 100dB;` | "ok… but elsewhere it's `.db()`" | ? | **D3**: two forms |

### 1.6 `Stm32Adc` and `SensorBoard`

| Text | Novice reading | Mark | Truth, and the fix |
|---|---|---|---|
| `observe code: Integer` | "a pin called code?" / "the block observes something" | **✗** | the block *exposes* its reading, so parents can observe it. Hover: "a value this block reports; not a pin" |
| `emits wake: Step on vdd.i { 5mA -> 30mA, edge: 1us }` | "the MCU emits a wake signal" | **✗✗** | it's a current step the parent *may cause*. Nothing leaves the MCU. And the syntax `Step on vdd.i { … }` differs from the setup's `vout.i: Step { … }`. Tweak T1 |
| `let amp = GainStage<A = Mcp6001> { … }` | "use the MCP6001 version" | ✓ | |
| `mode Measuring { mcu.state: Run }` | "in Measuring, the MCU runs" | ✓ ? | where is `state` declared on `Stm32Adc`? (open item 3). And `Run` here is an MCU state while `Run` in `Ldo3v3` is an LDO mode. Same word, two owners |
| `setup McuWakes … { event: mcu.wake at 1ms, window: 5ms, … }` | "the MCU wakes at 1 ms" | ✓ | **D2**: `Unplug` writes events differently |
| `setup AtZero … { sensor.v: 0V, ..Operating }` | "sensor at zero" | ✓ | |
| `#[fault] setup Unplug { event drop: usb.open { at: 1ms, for: 0us..=10ms }, window: until drop.end + 1s, … }` | "unplug for up to 10 ms" | ✓ ? | `for:` is a field name here, and a keyword in `setup S for X` and `for f in`. Tweak T3. `window: 5ms` vs `window: until …`: two forms, both fine |
| `#[fault]` vs `#[outside]` | "both are 'weird setups'; what's the difference?" | **✗** | fault = an abnormal event the board **must survive** (specs may be `pub`). outside = conditions **beyond** the promise, used to characterize (never `pub`). A two-line docs table (§5) |
| `ensure per_mv within 12.0/mV ± 7%;` | "12 per mV?" | ? | 12 codes per mV. A unit on a bare count is unusual. Hover: "codes/mV" |
| `spec wake_noise: tran(mcu.code).deviation() <= 2 …` | "deviation from what?" | ? | from the pre-event value? the mean? Open item 1 |
| `pub spec holdup: tran(mcu.code).during(drop).deviation() <= 4 on Unplug;` | "during the dropout, the reading moves ≤ 4 codes" | ✓ | reads very well |
| `// Reversed: no spec needed; every rated limit is checked on every setup` | "handled; the board survives" | **✗** | with no protection part, `ldo.vin` sees about −4.4…−5.25 V against `rated −0.3 V`, so the verdict is **FAIL**. That's the point of the setup, but the comment reads as reassurance. T9 |

### 1.7 Keywords that mean two or three things

| Word | Meanings in v4 |
|---|---|
| `in` | `env x: T in range` (a range) · `for f in range` (a sweep) · `in Sleep` (a mode) |
| `for` | `setup S for X` (target block) · `for f in …` (sweep) · `{ for: 0us..=10ms }` (duration field) |
| `..` | `10kΩ..` / `..=20uF` (open ranges) · `..Operating` (everything else from) |
| `pub` | `pub block` (visible to other files) · `pub spec` (parents may rely on it) |
| `setup` | `setup S for X { … }` (define) · `setup = S;` (choose the default and promise it) |
| `event` | `event drop: usb.open {…}` (declare and place) · `event: mcu.wake at 1ms` (place a declared one) |
| `db` | `.db()` method · `db(x)` function |

The most expensive of these for a novice are `in` (three meanings, often in one line) and `..` (two meanings in the same braces). §4 treats both without touching a decision.

---

## 2. Two new circuits, written as a novice would

I wrote each one **the way a novice would, first try**, using only draft 4. I list every guess, then give a corrected version that respects v4 and the physics.

### 2.1 LED from 5 V through a resistor

**The circuit.** 5 V ± 5 % → R1 → red LED → ground. Target about 20 mA.
- A red LED's datasheet gives V_F = 1.8–2.2 V at 20 mA, with a tempco of about −2 mV/°C (a typical figure).
- Hand calculation: R = (5 − 2.0) / 20 mA = 150 Ω.

**First try (novice):**

```rust
pub block LedIndicator { vcc: Power<In>, gnd: Ground }

circuit LedIndicator {
    net anode;
    let r1 = Resistor { a: vcc, b: anode, value: 150 ± 1% };            // guess 1: Ω needed?
    let d1 = Led      { a: anode, k: gnd, part: "LTST-C191KRKT" };     // guess 2, 3: pin names? how to pick the model?
}

setup Operating for LedIndicator {
    vcc = Supply { v: 5V ± 5% },                                      // mistake: `=` instead of `:`
}

contract LedIndicator {
    setup = Operating;
    spec current: dc(d1.i) = 20mA ± 20%   for temp in -10°C..=60°C;   // mistakes: `=` limit; temp is not a sweep axis
    spec r_power: dc(r1.p) <= 100mW;                                   // guess 4: can I measure a part's power?
    rated d1.i <= 30mA;                                                // guess 5: rated on a part? or automatic?
}
```

**Where I got stuck, and what I guessed:**

| # | Question | What v4 says | My guess |
|---|---|---|---|
| 1 | Does `150` need `Ω`? The example has `47k` bare and `10kΩ` with a unit | nothing | allowed bare for `value` of a resistor |
| 2 | LED pin names: `a/k`? `p/n`? `anode/cathode`? | only the pin names of Resistor, Electrolytic, Npn and Tlv755p appear | `a`, `k` |
| 3 | How do I choose the LED model (V_F spread, tempco)? | parts are types (`Tlv755p`), so an LED is probably a type too | `Ltst_C191Krkt { … }` as its own kind |
| 4 | Can I measure `r1.i` or `r1.p`? | measures list `p.v`, `p.i` and `p.z(f)` for **ports**, and `.v` on nets | no. Use `vcc.i` (one path, so the port current is the LED current) |
| 5 | Is the LED's 30 mA abs max checked for me? Can `rated` name a part? | `rated` is shown only on a port (`vin.v`) | the part model carries its own ratings, checked on every setup (**D8**, needs saying) |
| 6 | Is `vcc.i` positive into the block? | not stated (**D7**) | yes, for `Power<In>` |
| 7 | How do I say "over temperature"? | a `temp` field in the setup | `temp: ambient`. A `for temp in` gives P0022 (§3) |
| 8 | Should `current` be `pub`? | "port facts such as current" | yes: a parent needs the supply draw |

**Corrected (v4 rules):**

```rust
pub block LedIndicator { vcc: Power<In>, gnd: Ground }

circuit LedIndicator {
    net anode;
    let r1 = Resistor     { a: vcc, b: anode, value: 150Ω ± 1% };
    let d1 = LtstC191Krkt { a: anode, k: gnd };                      // model: V_F, tempco, abs max 30 mA
}

setup Operating for LedIndicator { vcc: Supply { v: 5V ± 5% }, temp: ambient }

contract LedIndicator {
    setup = Operating;
    pub spec current: dc(vcc.i) within 15mA..=25mA;                // the LED current: one path
}
```

**The worked check the engine performs** (so the novice can see why the band passes). V_F spans 1.8–2.2 V at 25 °C. Over `ambient` (−10…60 °C) at −2 mV/°C, that becomes 1.73–2.27 V.
- I_min = (4.75 V − 2.27 V) / 151.5 Ω = **16.4 mA**
- I_max = (5.25 V − 1.73 V) / 148.5 Ω = **23.7 mA**

Both are inside 15–25 mA: PASS. **R1's power** at I_max is 23.7 mA² × 150 Ω = **84 mW**. That's 84 % of a 0603's 100 mW rating, and the rating derates above 70 °C. So the part's own `rated` must be checked automatically (guess 5), and the verdict should show "84 % of r1's power rating". Otherwise the novice never learns the resistor is close to its limit.

**Verdict:** easy once guesses 2–5 are answered. All four are library and docs questions, not syntax.

### 2.2 Resistive sensor divider → RC filter → ADC pin

**The circuit.**
- An NTC thermistor (10 kΩ at 25 °C, B = 3950) to ground, and a 10 kΩ top resistor to 3.3 V.
- The divider midpoint goes through 1 kΩ to a 100 nF cap, which is the ADC pin.
- The ADC samples at 50 kSPS with an 8 pF sample cap (ee §2.6), and its reference is the same 3.3 V (ratiometric).

**First try (novice):**

```rust
pub block NtcInput { vref: Power<In>, gnd: Ground, adc: Analog<Out> }

circuit NtcInput {
    net mid;
    let r_top = Resistor   { a: vref, b: mid, value: 10kΩ ± 1% };
    let ntc   = Thermistor { a: mid,  b: gnd, r25: 10kΩ ± 1%, beta: 3950 ± 1% };   // guess: a part whose R follows `temp`
    let r_f   = Resistor   { a: mid,  b: adc, value: 1kΩ ± 1% };
    let c_f   = Capacitor  { a: adc,  b: gnd, value: 100nF ± 10% };
}

setup Operating for NtcInput {
    vref: Supply { v: 3.3V ± 2% },
    adc:  Load   { c: 8pF, r: 1MΩ.. },                            // guess: what an ADC pin looks like
    temp: ambient,
}

contract NtcInput {
    setup = Operating;
    spec in_range: dc(adc.v)                       within 0.3V..=3.0V;
    spec at_25:    dc(adc.v) / dc(vref.v)          within 0.5 ± 1%       with temp = 25°C;
    spec corner:   ac(adc.v / mid.v).f_high(-3dB)  within 1.2kHz..=2.0kHz;
    pub spec draw: dc(vref.i)                      <= 500uA;
}
```

**Where I got stuck, and what I guessed:**

| # | Question | What v4 says | My guess, and what happens |
|---|---|---|---|
| 1 | Is the sensor a *part* (inside) or a *port* (off-board, two wires)? | no example of a resistive sensor | a part, so `temp` drives it. If off-board, `sensor: Load { r: 2.5kΩ..=58kΩ }` on an *input* port? `Load` on an input felt wrong |
| 2 | What does an ADC pin look like as a load? | `Load { r, c }` only | `Load { c: 8pF, r: 1MΩ.. }`. **This silently misses the real failure**: the sample kick and average current only show in a transient with a sample clock (ee §2.6). The draft offers nothing better |
| 3 | Ratio of two DC values: `dc(a) / dc(b)` or `dc(a / b)`? | `ac(a / b)` is shown; DC ratios aren't | both? (needs one rule) |
| 4 | Can `with` pin `temp`? | `with x = v` for "a knob"; `temp` is a setup field | yes |
| 5 | The filter corner: what's the AC source? | the ratio's denominator, implicitly (D6) | `ac(adc.v / mid.v)`. **This gives the wrong answer**, below |
| 6 | Is a spec on `mid` (internal) allowed with an AC source there? | internal specs may name nets; stimulus placement unstated | allowed. It shouldn't be, in this form (P0061) |
| 7 | `adc` is `Analog<Out>` but it's the *input* of the ADC. Which direction? | Role is from the block's view | `Out`, from the block's view. Confusing but right |

**What guess 5 does.** Driving `mid` with an ideal AC source shorts out the divider's Thévenin resistance. The engine would report 1 / (2π · 1 kΩ · 100 nF) = **1.59 kHz**, and the spec passes. In the real circuit, the source resistance is 1 kΩ + (10 kΩ ∥ R_ntc):

| Temp | R_ntc | R_source | Corner |
|---|---|---|---|
| −10 °C | 58.2 kΩ | 9.53 kΩ | **167 Hz** |
| 25 °C | 10.0 kΩ | 6.0 kΩ | 265 Hz |
| 60 °C | 2.49 kΩ | 2.99 kΩ | **532 Hz** |

The novice's spec passes on a number that's off by 3–10×. The correct measure drives a real port: `ac(adc.v / vref.v).f_high(-3dB, ref: dc)`.

**What guess 2 misses.** The ADC's average current C_s·V·f_s ≈ 8 pF × 3.3 V × 50 kHz = 1.3 µA. Add up to ±1 µA leakage, and flow that through up to 9.5 kΩ: an error of roughly **10–20 mV**, which is 12–25 LSB at 12 bits. A static 8 pF load shows none of it.

**Guess 2 also breaks `at_25`.** The divider's error budget is ±1 % from R_top and r25 at ±1 % each (0.5 × 2 %), with no room left. The guessed `r: 1MΩ..` load then pulls a further −0.6 % (6 kΩ into 1 MΩ). The spec fails for a load that was a guess.

**Corrected (v4 rules plus one std shape, `AdcPin`, proposed in T10):**

```rust
pub block NtcInput { vref: Power<In>, gnd: Ground, adc: Analog<Out> }

circuit NtcInput { /* as above */ }

setup Operating for NtcInput {
    vref: Supply { v: 3.3V ± 2% },
    adc:  AdcPin { c_s: ..=8pF, r_sw: ..=1kΩ, f_s: 50kHz, t_s: 1.125us, leak: -1uA..=1uA },  // proposed std shape
    temp: ambient,
}

contract NtcInput {
    setup = Operating;
    spec in_range: dc(adc.v)                                within 0.6V..=2.9V;
    spec at_25:    dc(adc.v / vref.v)                       within 0.5 ± 2%    with temp = 25°C;   // ratiometric
    spec corner:   ac(adc.v / vref.v).f_high(-3dB, ref: dc) within 150Hz..=600Hz;
    spec sampled:  tran(adc.v).deviation()                  <= 1.6mV;          // ≈ 2 LSB at 12 bits
    pub spec draw: dc(vref.i)                               <= 500uA;
}
```

**Checks:**
- `in_range`: 0.64 V at 60 °C (with −2 %) to 2.87 V at −10 °C (with +2 %). PASS.
- `draw`: worst case 3.37 V / 12.4 kΩ = 0.27 mA. PASS.
- `sampled`: likely **FAIL** at −10 °C, from the average-current error above. That's the verdict the novice needs: the fix is a buffer, a smaller R_top, or a lower f_s.

**Verdict:** the syntax was learnable. The novice was stuck on **what to put at the ADC port** and **where the AC source goes**. Both lead to a confident false PASS, not an error.

---

## 3. Error messages the compiler must give

In rustc style, most likely first. Codes are placeholders (`S` = setup, `P` = spec, `C` = contract, `B` = block/circuit, `L` = lint).

### 3.1 Punctuation from other languages

```
error[S0001]: expected `:` in a setup field, found `=`
  --> led.spl:10:9
   |
10 |     vcc = Supply { v: 5V ± 5% },
   |         ^ help: setup fields use `:` like part fields: `vcc: Supply { … }`
   = note: `=` is used once in a contract, in `setup = Operating;`
```

```
error[P0002]: expected a limit (`within`, `<=`, `>=`), found `=`
  --> led.spl:15:34
   |
15 |     spec current: dc(d1.i) = 20mA ± 20%;
   |                            ^ help: use `within 20mA ± 20%`
   = note: a spec states a range the value must stay in, over every tolerance, temperature and supply;
           an exact `=` would fail on any real board
```

```
error[P0003]: this range excludes its upper end
  --> ce_amp.spl:12:40
   |
12 |     spec bias: dc(output.v) within 4.5V..6.5V;
   |                                    ^^^^^^^^^ help: include it: `4.5V..=6.5V`
   = note: `a..b` stops just below `b`; limits are almost always inclusive
```

Also give the `help:` for `4.5V - 6.5V`, `4.5V to 6.5V` and `[4.5V, 6.5V]`: "write a range as `4.5V..=6.5V`".

### 3.2 Units and quantities

```
error[P0010]: the limit is a current, but `dc(output.v)` is a voltage
  --> ce_amp.spl:12:33
   |
12 |     spec bias: dc(output.v) within 4.5mA..=6.5mA;
   |                ------------        ^^^^^^^^^^^^^ current
   |                voltage
```

```
error[P0011]: this limit has no unit, but `dc(vout.v)` is a voltage
   |
   |     spec output: dc(vout.v) within 3.3 ± 2%;
   |                                    ^^^ help: `3.3V ± 2%`
```

```
error[P0012]: `<= -36` compares a dB value with a plain number
   |     spec psrr: ac(vout.v / vin.v).at(100kHz).db() <= -36;
   |                                                      ^^^ help: `-36dB`
```

```
warning[L0013]: `.db()` applied to a value already in dB
```

### 3.3 Setups, modes and sweeps (the `in`/`for` family)

```
error[P0020]: `Sleep` is not a mode of setup `LoadStep`
  --> ldo.spl:24:60
   |
24 |     spec dip: tran(vout.v).min() >= 3.25V on LoadStep in Sleep;
   |                                                          ^^^^^ not a mode here
   = note: `LoadStep` inherits the modes `Run`, `Sleep` from `Operating`, but it overrides `vout.i`,
           so `Sleep`'s `vout.i: 0.1mA..=1mA` is replaced by the step          (depends on the D1 rule)
```

```
error[P0021]: `in` after a spec takes a mode name, found a range
   |     spec quiescent: dc(vin.i - vout.i) <= 50uA in 0.1mA..=1mA;
   |                                                   ^^^^^^^^^^^ expected a mode
   = help: to restrict a knob for this spec, pin it with `with vout.i = 1mA`,
           or use a mode: `mode Sleep { vout.i: 0.1mA..=1mA }` then `in Sleep`
```

```
error[P0022]: `temp` is a setup knob, not a sweep axis
   |     spec current: dc(vcc.i) within 15mA..=25mA for temp in -10°C..=60°C;
   |                                                ^^^^^^^^^^^^^^^^^^^^^^^^^^
   = note: every knob in the setup (`temp`, supplies, tolerances) is already searched for the worst case
   = help: to narrow it, set `temp: -10°C..=60°C` in the setup; to test one value, `with temp = 25°C`
   = note: `for x in` is only for axes swept *inside* one measurement (frequency, an input span)
```

```
error[P0023]: `f` is used but not defined
   |     pub spec output_z: vout.z(f) <= 2Ω;
   |                               ^ help: add `for f in 10Hz..=1MHz`, or use a fixed point: `vout.z(1kHz)`
```

```
warning[L0024]: sweep variable `f` is never used in the measure
```

```
error[P0025]: `with vin.v = 3.0V` is outside setup `Operating` (`vin.v` ∈ 4.3V..=5.5V)
   = help: a value outside the promised range belongs in a separate setup marked `#[outside(reason = "…")]`
```

```
warning[S0026]: `LoadStep` sets `vout.i`, which mode `Run` of its base also sets
  --> ldo.spl:17:31
   = note: in `LoadStep`, the step replaces the mode's `vout.i` in every mode   (the D1 rule, whichever it is)
   = help: to keep the modes' currents, set the step's levels from them: …
```

### 3.4 Setup derivation

```
error[S0030]: `..Operating` must come last
   |     setup LoadStep for Ldo3v3 { ..Operating, vout.i: Step { … } }
   |                                 ^^^^^^^^^^^ move to the end: `{ vout.i: Step { … }, ..Operating }`
   = note: `..Operating` means "every field not written here, as in `Operating`"
```

```
error[S0031]: `..Operating` refers to a setup for `CeAmp`, but this setup is for `Ldo3v3`
```

```
error[S0032]: unknown port `vdd` on `Ldo3v3`
   |     vdd: Supply { v: 5V ± 5% },
   |     ^^^ help: a port with a similar name exists: `vin`
```

```
error[S0033]: `Load` can't drive `vin: Power<In>`
   = help: a power input takes a `Supply { v, z }`; `Load` goes on outputs
```

```
warning[S0034]: supply `vcc` has an exact voltage `5V`
   = note: real supplies vary; the check covers only 5.000 V
   = help: write the tolerance (`5V ± 5%`), or silence this with `#[exact]` if the rail really is fixed
```

### 3.5 `rated`, `pub`, internal names

```
error[C0040]: `rated vin.v` (4.3V..=5.5V) is narrower than setup `Operating` (`vin.v` up to 5.5V, `z` drop included)
   = note: `rated` is the **absolute maximum** (damage) limit, checked on every setup, faults included
   = help: the datasheet's "Absolute Maximum Ratings" row goes here (e.g. `-0.3V..=6.5V`);
           "Recommended Operating Conditions" go in the setup
```

(Or a warning when `rated` merely equals the setup range: "no margin between the operating range and damage".)

```
error[C0041]: a `pub spec` may name only ports
   |     pub spec mid_ok: dc(mid.v) within 1.6V..=1.7V;
   |                         ^^^ `mid` is an internal net
   = help: remove `pub`; the spec is still checked, only not published to parents
```

```
error[C0042]: `ldo.u1` is inside a child block; a contract sees only a child's ports, `observe` and `event`
   |     spec x: dc(ldo.u1.vout.v) …
   = help: use the child's port net (`v3v3.v`), or add `observe` to the child's block
```

```
error[C0043]: a spec on `#[outside]` setup `DropoutRow` can't be `pub`
   = note: `#[outside]` setups characterize the block beyond its promise; parents never rely on them
```

```
error[C0044]: event `drop` is not placed in the default setup `Operating`
   |     pub spec holdup: tran(mcu.code).during(drop).deviation() <= 4;
   |                                            ^^^^ help: add `on Unplug`
```

### 3.6 Multi-setup specs

```
error[P0050]: in a spec with several setups, say which one: `ac(out.v)[dm]` or `ac(out.v)[cm]`
```

```
error[P0051]: a spec body must end with `ensure <measure> <limit>;`
   = help: found `return`; the spec's condition is written `ensure db(a_dm / a_cm) >= 100dB;`
```

### 3.7 Measures and circuits

```
error[P0060]: `.p` is not a measure    (or allowed, if D8 settles it)
   = help: power in a part is checked against its own rating automatically; see the verdict's "ratings" row
```

```
error[P0061]: the AC source for `ac(adc.v / mid.v)` would sit on internal net `mid`
   = note: driving an internal net from an ideal source removes the impedance behind it, and changes the answer
   = help: put the denominator on a port: `ac(adc.v / vref.v)`
```

```
warning[L0062]: `adc` is loaded only by a static `Load`; an ADC input draws charge each sample
   = note: settling and DC error from the sample capacitor appear only with the sample clock (see std `AdcPin`)
```

```
error[B0063]: pin `k` of `d1` is not connected
error[B0064]: unknown pin `cathode` on `Led`; its pins are `a`, `k`
warning[B0065]: net `anode` has only one connection
```

---

## 4. Tweaks, before and after

All of these keep the §0 decisions. The first four fix D2, D3, D4 and a misreading. The rest are optional polish.

**T1. `emits` → `event` in the block** (the keyword matches the setup's `event`; kills "the MCU sends a pulse")

```rust
// before
emits wake: Step on vdd.i { 5mA -> 30mA, edge: 1us },
// after
event wake: vdd.i: Step { 5mA -> 30mA, edge: 1us },      // an event a parent may place in a setup
```

The field form `vdd.i: Step { … }` is the one setups already use for `vout.i: Step { … }`, so a step is spelled one way everywhere.

**T2. One spelling for placing an event** (D2)

```rust
// before
setup McuWakes for SensorBoard { event: mcu.wake at 1ms, window: 5ms, ..Operating }
// after
setup McuWakes for SensorBoard { event wake: mcu.wake { at: 1ms }, window: 5ms, ..Operating }
```

Same shape as `event drop: usb.open { at: 1ms, … }`, and specs can then say `.during(wake)`.

**T3. The event field `for:` → `lasting:`** (`for` already has two jobs)

```rust
// before
event drop: usb.open { at: 1ms, for: 0us..=10ms },
// after
event drop: usb.open { at: 1ms, lasting: 0us..=10ms },
```

**T4. One dB form** (D3). Methods are used everywhere else.

```rust
// before
ensure db(a_dm / a_cm) >= 100dB;
// after
ensure (a_dm / a_cm).db() >= 100dB;
```

**T5. Canonical clause order: the sweep goes last.** This is a formatter rule, not a grammar change, so `in M` never follows a range.

```rust
// before
pub spec output_z: vout.z(f) <= 2Ω for f in 10Hz..=1MHz in Run;
// after
pub spec output_z: vout.z(f) <= 2Ω in Run for f in 10Hz..=1MHz;
```

The order becomes `<limit> [with …] [on S] [in M] [for x in r]`: the conditions first, the sweep that `f` refers to last, next to its use. Update the rule line in §1 of the draft to match. (If the lead wants more, `in mode Run` is the heavier alternative. I don't recommend it; T5 is enough.)

**T6. One-sided ranges in setup fields use the spec's words**

```rust
// before
vin:  Supply { v: 4.3V..=5.5V, z: ..=0.5Ω },
vout: Load   { r: 10kΩ.., c: ..=1nF },
// after
vin:  Supply { v: 4.3V..=5.5V, z: <= 0.5Ω },
vout: Load   { r: >= 10kΩ, c: <= 1nF },
```

- A novice then learns one way to say "at most", and it's the same in specs and setups.
- `..` keeps a single job inside braces, `..Operating`.
- Two-sided ranges stay `a..=b`.
- The parser is unambiguous: `<=`/`>=` can't start any other field value.

**T7. `Sweep` in braces, like `Step`**

```rust
// before
vin.v: Sweep(4.5V -> 3.0V)
// after
vin.v: Sweep { 4.5V -> 3.0V }
```

**T8. ASCII unit aliases, normalized by the formatter** (D9)

```rust
// typed
value: 47kohm +- 1%,  temp: -40degC..=125degC,  c: <= 1uF
// formatted
value: 47kΩ ± 1%,     temp: -40°C..=125°C,      c: <= 1uF
```

`µ` and `u` are both accepted. The formatter also writes the unit on a bare `47k` in a resistor's `value`, so the example stops mixing forms.

**T9. The `Reversed` comment states the verdict** (D4)

```rust
// before
#[fault]
setup Reversed for SensorBoard { usb.polarity: reversed, ..Operating }
// Reversed: no spec needed; every `rated` limit is checked on every setup, faults included.
// after
#[fault]
setup Reversed for SensorBoard { usb.polarity: reversed, ..Operating }
// Reversed: no spec needed; every `rated` limit is checked here too.
// As drawn this FAILS `ldo.rated vin.v` (about −5 V < −0.3 V): add a series Schottky or a P-FET.
```

**T10. A std ADC input shape** (closes the silent false PASS of §2.2)

```rust
// before: nothing a novice can write except a static load
adc: Load { c: 8pF },
// after: a std shape, with the MCU's datasheet values
adc: AdcPin { c_s: <= 8pF, r_sw: <= 1kΩ, f_s: 50kHz, t_s: 1.125us, leak: -1uA..=1uA },
```

An ADC model block (`Stm32Adc`) could publish this shape for its `ain` pin. A parent that places the MCU then gets it for free, and a block tested alone (like `NtcInput`) writes it directly.

**T11. Allow `Number` for plain numeric generics**

```rust
// before
pub block GainStage<A: OpAmp = Mcp6001, const GAIN: f64 = 10.09>
// after
pub block GainStage<A: OpAmp = Mcp6001, const GAIN: Number = 10.09>
```

**Rules to settle (not syntax):**
- **D1.** A derived setup's field replaces the same field in every mode it inherits. The formatter warns (S0026).
- **D5.** In `Pair`, each leg's `z` tolerance is drawn **independently**; `z_match:` could override that.
- **D6.** The AC source sits at the denominator's port. An internal net there is an error (P0061). `.ac` fields name the source explicitly.
- **D7.** `p.i` is positive in the port's role direction: into an `In` port, out of an `Out` port. This makes `vin.i - vout.i` the ground current.
- **D8.** Every part's own ratings (power, current, voltage) are checked on every setup like `rated`, and shown as "% of rating" in the verdict. Part currents (`r1.i`) are measurable in internal specs.
- **`pub` in the example.** Either publish `Ldo3v3`'s `output`, or add one line explaining why a voltage spec isn't published.

---

## 5. Documentation and editor hints for the rest

### 5.1 Hovers (one line each, shown on the keyword)

| On | Hover |
|---|---|
| `setup = S;` | "Default setup: specs use `S` unless they say `on`. **Parents must keep this block inside `S`.**" |
| `rated` | "Absolute maximum (damage). Checked on every setup, faults included. Not the operating range." |
| `pub spec` | "Published: parents may rely on this. Unmarked specs are checked too, just not published." |
| `..Operating` | "Every field not written here is as in `Operating`." |
| `in Run` (spec) | "Only in mode `Run`. Without it, the spec is checked in every mode." |
| `for f in …` | "Swept inside the measurement; the limit must hold at every `f`." |
| `with x = v` | "This knob is pinned to `v` for this spec only; others are still searched." |
| `observe` | "A value this block reports for parents to measure; not a pin." |
| `event` (block) | "An event a parent may place in a setup; the block doesn't cause it." |
| `#[fault]` | "An abnormal event the board must survive. Specs on it may be `pub`." |
| `#[outside]` | "Conditions beyond the promise, for characterization. Checked, never published." |
| `Pair` | "dm = inp − inn; cm = (inp + inn)/2; z = each leg's source impedance, independent." |
| `.dm.ac: 1V` | "Small-signal reference level; results are per volt. Not a real 1 V." |
| a PASS | "Every value of every `env` and range knob; 3σ on tolerances (99.87 % one-sided). Worst corner: …" |

### 5.2 Inlay hints (grey text the editor shows but doesn't save)

- After `setup = Operating;`, list the modes: `· modes: Run, Sleep`.
- After a spec with no `in`: `· × 2 modes`.
- After `z: <= 0.5Ω` (or the current `..=0.5Ω`): `≤ 0.5 Ω`.
- After `value: 150Ω ± 1%`, the computed range: `148.5–151.5 Ω`.
- Next to each spec, after a check: the worst value and its margin, e.g. `23.7 mA (margin 1.3 mA) at vcc 5.25 V, −10 °C`.

### 5.3 Completion

- **Pin names per part.** The most frequent guess in §2.
- **Shape fields per port kind:** `Supply {v, z}`, `Signal {v, z, wave, ac}`, `Load {r, c, i}`, `AdcPin {…}`.
- **Measures per value:** `.at`, `.mag`, `.f_high`…, each with its one-line meaning.
- **Setups and modes** after `on` and `in`.

### 5.4 A novice page in the docs (about one screen)

1. **"A spec is a datasheet row that must hold everywhere."** Walk through the LED current of §2.1 with the worked numbers.
2. **Three words that look like code:** `setup =`, `..Base`, `pub`, each with a one-line EE meaning.
3. **Where knobs come from:**
   - `env` and setup ranges are searched;
   - `±` tolerances are statistical at 3σ;
   - `with` pins one knob;
   - `for` sweeps inside a measurement.
4. **The three `in`s and the three `for`s** (§1.7), side by side.
5. **Faults vs outside** (the two hovers above, as a table).
6. **Two traps the engine catches for you, and one it can't:**
   - it catches **abs max** (`rated`, part ratings) and the **lightest-load instability** (the verdict names the corner);
   - it can't catch the **wrong bench**, such as a static load on an ADC pin, or an AC source on an internal net. Use the std shapes.

---

## 6. Sources

- `contract_syntax_v4.md`: the draft under review (§0 decisions, §1 rules, §2 example).
- `contract_ee_practice.md`: §2.6 ADC driver (sample kick, average current, the STM32F103 input model, DS5319 §5.3.18); §3.1–3.3 fixed vs range conditions, worst loads.
- `contract_v2_review_usability.md` §6.2: the draft-2 novice misreadings, which draft 4 fixed (`require`, `accepts`, the multi-at) or which recur here (`Signal.v`, the light load, the invisible confidence).
- rustc diagnostic style (`error[E…]`, `-->`, `help:`, `note:`): the Rust compiler's diagnostic guide.
- LED figures: typical red SMD LED datasheet values (V_F 1.8–2.2 V at 20 mA, about −2 mV/°C, abs max 30 mA); 0603 thick-film resistor, 100 mW at 70 °C.
- NTC figures: B-parameter equation R(T) = R25 · exp(B(1/T − 1/298.15 K)), B = 3950.
