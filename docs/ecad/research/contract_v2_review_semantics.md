# Contract Syntax v2: A Red-Team Review of the Semantics

> 2026-09-30 · Review of `contract_syntax_v2.md` (draft 2). Angle: **semantics**. What does each line mean, where is it ambiguous, what can't be said, and what breaks hierarchy reuse. Syntax taste (keywords, `self.`) is left to other reviews unless it changes meaning.
> Reads with: `contract_options.md` §1 (the common core), `contract_hierarchy.md` (H), `contract_ee_practice.md` (EE), `language.md` v0.1 (L), `model.md` (E-numbers), `next_synthesis.md` (L/M/E/T numbers), `engine_plan.md`.
> No new simulations. The two numbers in §9 are hand estimates and say so.

---

## Summary

The draft reads well. Its three decided moves (interface, implementation and contract as separate items; named setups; specs as functions) are sound. But the draft leaves the meaning of several lines open, and some of the gaps would break the two things the project cares most about: **one contract holding for every implementation**, and **cheap, sound reuse of child results**.

The five problems that matter most:

1. **The contract reaches into the implementation.** `SensorBoard`'s contract reads `mcu.code`, and its setup `McuWakes` drives `mcu.load`. `mcu` exists only in one implementation. With a second implementation the contract no longer means anything. Fix: the contract sees **ports and declared observables only**; checks on internals live with the implementation.
2. **`accepts Operating` is under-defined.** It doesn't say what an unwritten field means (is `vin` of `GainStage` allowed any amplitude?), whether the other setups (`InputStep`, `LoadStep<Self>`, `PowerUp<Self>`) are inside the promise, or how modes fit. Fix: `accepts` lists the **envelope**, every spec's setup must be **derived from and inside** an accepted setup (checked), events are written as **ranges**, and unwritten port fields are an error or a written-out default.
3. **Setup inheritance has two readings in the draft itself.** `InputStep` restates `z:` (as if an override replaces the whole port), `AtZero` omits it (as if fields merge). The two readings give different circuits. Fix: **field-level merge**, always, and the editor shows the resolved setup.
4. **Multi-setup specs don't say which knobs are shared.** In `sensitivity(zero: AtZero, full: AtFull)`, can the engine pick USB = 4.40 V for `zero` and 5.25 V for `full`? If yes, the ratiometric spec gets a false alarm. Fix: **one board, one moment**: every knob a setup does not override is shared across the spec's setups.
5. **`rated` does two jobs** (an operating temperature on `GainStage`, an absolute maximum on `Ldo3v3`), and `temp: ambient` ties a library block's verdict to the project's global. Fix: `rated` = absolute maximum only; library setups write numbers; the root binds globals.

Around these are about twenty smaller problems: published port facts are picked out by an unstated pattern; `for f in …` inside a `require` contradicts L §8.6; generics have no stated checking rule; implementation choice at placement is open; faults have no home; `temp` as a setup field collides with a port named `temp`; and model.md's knob creation (E16) assumes `assume`, which v2 removed. Each has a concrete example and a fix below. §10 ranks the fixes.

---

## How to read this

Each problem has the same four parts:
- **Example:** a line from the draft, or a small variation of it.
- **The question:** what the line could mean.
- **Why it matters:** the wrong verdict, the unsound reuse, or the editor/AI failure it causes.
- **Fix:** a proposal that fits the spirit (text as truth, readable by novices, checkable, cheap hierarchy).

Problems are grouped by topic (§1–§9). Numbering is for reference only; the ranking is in §10.

---

## 1. Scope: what a setup and a contract may name

### P1. The contract reads internals, so "one contract, many implementations" breaks

**Example.**
```rust
impl Contract for SensorBoard {
    spec wake_noise(env: McuWakes) { require tran(mcu.code).deviation() <= 2; }
}
```
`mcu` is a `let` inside `impl Circuit for SensorBoard`. Decision 2 says a block can have several implementations, each checked against the same contract. Now write a second implementation that uses an external ADC (`let adc = Ads1115 { … }`). It has no `mcu`.

**The question.** Is `wake_noise` an error for the second implementation, silently skipped, or a spec of the first implementation only?

**Why it matters.** It's the whole point of decision 2. A contract that names internals is really a contract of one implementation. It also breaks caching: H §4.3 keys child results by the child's definition; a contract that depends on internal names has to be re-resolved per implementation, and a rename inside the implementation (`mcu` → `u_mcu`) silently changes the contract.

**Fix.** Two tiers, visibly separate:
- **The contract sees the interface only**: ports, generic parameters, and **declared observables**. An observable is a named, typed output that is not an electrical port, such as the ADC code:
  ```rust
  pub block SensorBoard {
      usb: Power<In>, sensor: Analog<In>, gnd: Ground,
      observe code: Code<12>,           // not a pin; each implementation must provide it
  }
  impl Circuit for SensorBoard {
      …
      let mcu = Stm32Adc { vdd: v3v3, ain: adc_in, gnd };
      provide code = mcu.code;          // binds the observable
  }
  ```
- **Checks on internals live with the implementation** (derating of `r_f`, a node voltage, a part's own abs-max). They are ordinary specs written in (or next to) the implementation item, are checked for that implementation only, and **never feed composition**. The formatter prints them in the same spec table, marked with the implementation's name.

The EE board already treats the ADC code as "the output is the ADC code (a measure)" (EE §4.2). `observe` gives that a declared home.

### P2. A setup that drives a child's internal (`mcu.load`)

**Example.**
```rust
setup McuWakes for SensorBoard: Operating { mcu.load: Load { step: 5mA -> 30mA, edge: 1us } }
```
Three problems in one line:
1. `mcu` is internal (P1 again, now in a setup, which the draft says lives *outside* the contract).
2. `Stm32Adc` has ports `vdd`, `ain`, `gnd`. There is no port `load`. What `mcu.load` really means is **the MCU's own supply current**, a behaviour of the MCU, not a load attached to one of its outputs.
3. A `Load` belongs on an output (core item 1). `mcu.vdd` is an input. Writing a `Load` there contradicts the core.

**Why it matters.** A setup is "the environment" (what the block accepts). The MCU's wake-up current is not environment. It is the MCU's own behaviour, and it is the kind of thing the LDO must accept from its neighbours. If the parent can reach in and set it, nothing checks that the value is one the MCU really produces, and the LDO's composition check has nothing to read.

**Fix.** The child **declares** the behaviour as a range (a mode or an event it produces, option C's `emits`), and the parent's setup may only **choose among what the child declares**:
```rust
// in the MCU's contract
impl Contract for Stm32Adc {
    accepts Operating;
    emits vdd.i: Step { from: 5mA..=10mA, to: 20mA..=30mA, edge: 1us.. } as wake;
}
// in the parent
setup McuWakes for SensorBoard: Operating { mcu: wake }      // names the child's declared event
```
`mcu: wake` names a *published item of the child's interface*, not an internal. It's still a path through an internal placement, so it belongs to implementation-level specs (P1's second tier), not to the interface contract, unless the parent re-exports it as its own event.

### P3. A setup field named `temp` collides with a port named `temp`

**Example.** A thermistor board: `pub block Thermo { temp: Analog<In>, … }`. Its setup:
```rust
setup Operating for Thermo { temp: Signal { v: 0V..=1V }, temp: ambient }   // two `temp:` fields
```
**Why it matters.** Port names and environment quantities share one namespace in the draft. The collision is real for sensor boards.
**Fix.** Environment quantities go in their own group, or use a reserved sigil. Suggested:
```rust
setup Operating for Thermo {
    temp: Signal { v: 0V..=1V },
    env { temp: -10°C..=60°C, life: 0y..=10y },
}
```
`env` is a keyword, so no port can be named that way. A lint suggests moving a stray `temp:` field into `env`.

### P4. Who may write a setup for a block (the orphan question)

**Example.** `setup Hot for Ldo3v3 { … }` written in `board.spl`, not in `ldo.spl`.
**The question.** Can the board then write `accepts Hot` on the LDO? Can its specs use it?
**Why it matters.** If any file can add accepted setups to a library block, the block's promise depends on which files are loaded, and cached results can't be keyed by the block's definition.
**Fix.** Rust's orphan rule, adapted:
- setups are **associated items** of their block (`Ldo3v3::Operating`, written `Self::Operating` inside);
- only the block's own module may `accepts` a setup or define its contract;
- a setup for a foreign block is a **one-off experiment**: allowed, its verdicts are reported, but it is marked (`#[beyond]` or equivalent) and never feeds composition (core item 11).

### P5. Spec parameters are named but not used

**Example.** `spec gain(env: Operating) { let h = ac(vout.v / vin.v); … }`. `env` is never mentioned. In `sensitivity(zero: AtZero, full: AtFull)` the names are used: `mcu.code on full`.
**The question.** In a multi-setup spec, what does a bare `vout.v` mean? Which setup?
**Fix.** One rule:
- a single-setup spec may use bare probes; they mean "under this setup";
- a multi-setup spec must qualify **every** probe; a bare probe is an error ("which setup? `zero` or `full`");
- use one qualification form. `full.code` (field access, like every other probe in L §8.4) is more regular than `code on full`, and an AI edits it more reliably. If the lead prefers `on`, then `on` everywhere, including `dc(code on full)`.

A single-setup spec could also drop the name: `spec gain on Operating { … }`. That reads better for novices than an unused parameter.

---

## 2. What `accepts` means

### P6. Unwritten fields: does `accepts` promise too much, or nothing?

**Example.**
```rust
setup Operating for GainStage {
    vin:  Signal { z: 100Ω..=10kΩ },          // no amplitude, no DC level
    vout: Load   { r: 10kΩ.., c: ..=1nF },
    vdd:  Supply { v: 3.2V..=3.4V },          // no source impedance
    temp: ambient,
}
```
**The question.** Three readings of a missing field:
- (a) **anything**: `vin` may carry any amplitude. Then `accepts Operating` promises the stage works with a 5 V input on a 3.3 V rail. False, and unprovable.
- (b) **ideal / zero**: `vdd` has 0 Ω source impedance. Then on the board, where `vdda` comes through `r_filt = 10Ω`, the connection check fails at every placement, because no real source is 0 Ω.
- (c) **a default written somewhere else**: hidden meaning, which violates "the text is the source of truth".

**Why it matters.** The connection check (core item 7) compares the parent's actual source against these fields. Each reading gives a different verdict on the same text. With (a) the check passes and the amp clips unnoticed. With (b) it fails for a reason the user didn't write.

**Fix.**
- Every non-ground port must appear in the **accepted** setup; a missing port is an error with a fix-it (`vout: Open` if really unloaded).
- A missing *field* means **ideal for this block's own check** (reading b), and the formatter **writes it out** (`z: 0Ω`) so the reader sees it, as option C proposed. The connection check then fails with a useful message: "`vdda` has 10 Ω source impedance; `amp` accepts `z: 0Ω`. Widen: `vdd: Supply { v: 3.2V..=3.4V, z: ..=20Ω }`."
- For signal inputs, the **amplitude and DC level** are required fields. `Signal { z: … }` alone is an error: nothing bounds what the input may carry.

### P7. Which setups are inside the promise?

**Example.** `GainStage` accepts `Operating`. Its specs also use `InputStep`. `Ldo3v3` uses `LoadStep<Self>` and `PowerUp<Self>`, whose numbers are not in the file.

**The question.**
- Is `InputStep` inside `Operating`? It derives from it, but a derived setup can widen: `setup Wide for GainStage: Operating { vin: Signal { z: 100Ω..=1MΩ } }` inherits syntactically and yet leaves the envelope.
- `PowerUp` ramps `vin` from 0 V. That is below the accepted 4.3 V. Is the `start` spec a one-off, or part of the promise?
- If a board's MCU steps the rail 5 → 30 mA, can the board reuse the LDO's `dip` verdict? Only if `LoadStep<Self>` covers that step. Its numbers are hidden.

**Why it matters.** Composition reuses a child's verdict only if the verdict was checked over everything the parent can present (H §4.2). A spec whose setup is outside the envelope, or whose event is one fixed step, can't be reused. The draft can't tell which is which.

**Fix.** Define the envelope explicitly and check containment:
1. **`accepts` lists everything the block promises to handle**: the static operating setup, plus named **events** it tolerates in normal use.
   ```rust
   accepts Operating, PowerUp, LoadStep;
   ```
2. **Every spec's setup must be derived from an accepted setup and lie inside it** (field by field, ranges ⊆). The compiler checks it. A spec outside is allowed only with a mark (`#[beyond(reason = "…")]`), its verdict is reported and never composed.
3. **Events must be written as ranges** to be composable: `Step { from: 0.1mA..=10mA, to: 10mA..=50mA, edge: 1us.. }`, not one datasheet step. A fixed-point event is allowed, but it can only discharge a parent event with exactly that shape. The spec table shows "composable" per spec.
4. **Start-up is an event, not a one-off.** `PowerUp` is accepted as an event; during it the operating range is suspended and only `rated` (absolute maximum, P16) applies.

### P8. `accepts` with several operating modes

**Example.** EE and next_synthesis L3/E5: the LDO's phase margin is 61° in Run (1–50 mA) and 32° in Sleep (≤ 1 mA). Two setups, `Run` and `Sleep`.
**The question.** `accepts Run, Sleep` could mean (a) the union (the neighbour may be anywhere in either), or (b) a discrete choice (the neighbour is in one of them at a time, and the board says which).
**Why it matters.** (a) merges the ranges into one knob and hides which mode fails (next_synthesis E5 calls this a wrong verdict). (b) needs a shared mode knob that ties the MCU's sleep to the LDO's light load.
**Fix.** Modes are a **discrete environment quantity**, shared and joined like `temp`:
```rust
setup Operating for Ldo3v3 {
    vin:  Supply { v: 4.3V..=5.5V, z: ..=0.5Ω },
    vout: match mode {
        Run   => Load { i: 1mA..=50mA,  c: ..=20uF },
        Sleep => Load { i: 0.1mA..=1mA, c: ..=20uF },
    },
    env { temp: -40°C..=125°C, mode: Run | Sleep },
}
```
The `match` is on an exact, discrete value, so it's allowed by L §5.5. The engine enumerates modes (E5's `KnobKind::Mode`). The board's `mode` binds the children's `mode`, which is how "MCU asleep ⇒ LDO lightly loaded" becomes one correlated fact.

### P9. The connection check has nothing to compare when primitives sit between blocks

**Example.** On the board, `amp.vout` drives `r_aa` (470 Ω) and `c_aa` (100 nF), which are primitives, not a block. The amp accepts `vout: Load { r: 10kΩ.., c: ..=1nF }`.
**Hand estimate.** At 1 kHz the RC presents |Z| ≈ 470 + 1/(2π · 1 kHz · 100 nF) ≈ 470 + 1592 ≈ 1.66 kΩ, far below 10 kΩ. At DC it's open. So the stage's own accepted envelope is violated by the board it was drawn for. Yet EE shows this exact arrangement is stable (2.4% overshoot), and it's *why* the 470 Ω is there.
**Why it matters.** Two things the draft doesn't say:
- The load shape `Load { r, c }` means R ∥ C. The real load is R in series with C. The shapes don't match, so containment is either a false alarm or undecidable (option C's known weak spot).
- There is no block on the other side to publish facts. The "two-way check" is only defined block-to-block.
**Fix.**
- The parent computes the **port-equivalent of any primitive network** between block ports (a small AC/DC analysis of the parent's own primitives, with children replaced by their published facts). That's rung 1–2 of H §4.6, applied to the glue.
- Load and source shapes include **impedance over a band** (`z: ..=1kΩ over 10Hz..=100kHz`) and **isolated capacitive** (`Iso { r: 400Ω.., c: ..=1uF }`, option C). A mismatch in shape falls back to in-context simulation and says so, never to a silent pass.

---

## 3. Setups: inheritance, defaults and generic setups

### P10. Override replaces or merges? The draft uses both

**Example.**
```rust
setup InputStep for GainStage: Operating {
    vin: Signal { z: 100Ω..=10kΩ, wave: Step { … } },     // restates z: suggests "replace the port"
}
setup AtZero for SensorBoard: Operating { sensor: Signal { v: 0V } }   // omits z: suggests "merge"
```
**Why it matters.** Under "replace", `AtZero`'s sensor loses its 100 Ω–10 kΩ source impedance and becomes ideal, so `sensitivity` is checked with a different circuit than the user thinks. The diff shows nothing.
**Fix.** **Always merge, field by field** (Rust struct-update, applied recursively). To drop an inherited field, write it: `z: 0Ω`. The editor's setup card shows the resolved setup with inherited fields greyed, so novices see the whole environment. A lint flags a restated field whose value equals the inherited one (as in `InputStep`), because it suggests the author thought it was needed.

### P11. Generic std setups hide numbers and guess ports

**Example.**
```rust
impl Regulator for Ldo3v3 {}                                   // "gets the standard regulator setups"
spec dip(env: LoadStep<Self>) { require tran(vout.v).min() >= 3.25V; }
```
**The questions.**
- Which port does `LoadStep<Self>` step? "The only `Power<Out>`" works here; a dual-output regulator has two.
- Step from what to what, with what edge? Nowhere in the text.
- Is the step inside `Operating`'s 0.1–50 mA? If the std step goes to 100 mA, the spec is outside the envelope (P7) and nobody sees it.
**Why it matters.** The text is supposed to be the source of truth, and diffs are supposed to show changes. A std library update that changes `LoadStep`'s default edge would change the LDO's verdict with no diff in the project. Option C already listed "named environments hide changes" as a weakness.
**Fix.** Std setups are **templates with required parameters**, derived from the block's accepted setup:
```rust
setup Step for Ldo3v3 = LoadStep { on: vout, from: 5mA, to: 30mA, edge: 1us, ..Operating };
```
- No hidden defaults for numbers that change verdicts; the formatter writes them out.
- `impl Regulator for Ldo3v3 {}` may **suggest** the standard setups in the editor (as a to-do list, B's strength), but not create them silently.

### P12. Setups have no home for analysis settings or initial state

**Example.** `settling` uses `v.at(end)`. `dip` takes `tran(vout.v).min()`. Neither says the time window, the step size, or whether the transient starts from the operating point or from zero.
**Why it matters.** `tran(vout.v).min()` over a window that includes power-up is dominated by start-up (0 V), and `dip` fails for a reason unrelated to load steps. `end` depends on a window nobody wrote.
**Fix.** A setup that includes a time-domain stimulus also declares the analysis window and start state, with written-out defaults:
```rust
setup Step for Ldo3v3: Operating {
    vout: Load { step: 5mA -> 30mA, edge: 1us, at: 1ms },
    tran { window: 0ms..=3ms, start: operating_point },
}
```
Measures get explicit windows relative to events: `tran(vout.v).after(vout.step).min()` (option A/C's `during`/`after`).

### P13. Fixed points: two ways to say it, and a name clash with `.at()`

**Example.** A fixed point in a setup: `setup AtZero … { sensor: Signal { v: 0V } }`. A fixed point inline: `require ac(…).at(100kHz).db() <= -36dB at vin.v = 4.3V`.
**The questions.** Does the trailing `at` bind to the whole `require` or to the measure? Is `at` the method `.at(f)` or a condition? When should you use a setup and when `at`?
**Why it matters.** Core item 4 demands that the three kinds of condition look different. Here the fixed-point kind has two spellings, and one of them shares a word with an axis evaluation.
**Fix.** One spelling per kind:
- **range**: in the setup (for all);
- **fixed point**: `where vin.v == 4.3V` at the end of the `require` (L v0.1's word), checked to lie inside the setup's range; a named setup only when several specs share it;
- **measure axis**: `.over(vin.v)` inside the measure (P14).

### P14. The third kind of condition (a measure axis) can't be written

**Example.** Line regulation, from EE and all three options: `dc(vout.v).span(vin.v)`. The v2 draft has no line regulation, and no construct for "sweep `vin.v` inside one measure".
**Why it matters.** It's core item 4's third kind. Its quantifier is nested: *for every* other knob value, the *spread over* `vin.v` must be ≤ 5 mV. Writing it as an ordinary range gives "for every `vin.v`, `vout.v` is in a range", which is a different (weaker or stronger) claim.
**Fix.** `dc(vout.v).over(vin.v).span()` (or `.sweep(vin.v)`), where `vin.v` must be a range field of the spec's setup. Semantics: the engine evaluates the inner sweep per point of the other knobs, then the outer worst case over them.

---

## 4. Specs as functions: what the body may contain

### P15. The body has no stated rules

**Examples of lines the draft doesn't settle:**
```rust
spec a(env: Operating) {
    let r = 10k ± 1%;                                   // a spread in a spec: a new knob? a limit?
    require dc(vout.v) / r <= 1mA;
}
spec b(env: Operating) {
    if dc(vdd.v) > 3.3V { require … }                   // branch on a simulated value
}
spec c(env: Operating) {
    require vout.z(f) <= 100Ω for f in 10Hz..=100kHz;   // `for` as a quantifier over an axis
}
spec d(env: Operating) {
    require gain();                                     // calling another spec
    require bandwidth() >= 20kHz;
}
```
**Why each matters.**
- (a) L §5.3 says a range's meaning is decided by its position. A `let` inside a spec is a position the table doesn't list. If it becomes a knob, a spec can silently add variation that isn't a part. If it's exact, the `± 1%` is ignored.
- (b) A branch on a simulated value makes the verdict partial ("for all points where vdd > 3.3 V") with an unknown boundary, and it's non-smooth, which the engine's search avoids (L §8.4).
- (c) L §8.6 says `for` is build-time only, and axes are restricted with `.band()`. A continuous `for f in` has no defined sampling.
- (d) A spec is a verdict, not a value. Calling it has no meaning; reusing a computation needs a function.

**Fix: specs are pure, per-run functions.** Written down as rules:
1. **Evaluated per run** (D-F). The body sees one simulated board at one knob point (or one board under several setups, P18). Nothing survives between runs. Statement order doesn't matter beyond `let` dependencies (E4).
2. **No new knobs.** A spread literal in a spec body is an error with a fix-it: make it a part field or a `param`. The only `±` allowed is in the bound of `require … within`, where it is a limit.
3. **`if`, `match`, `for` only on exact values** (L §5.5), i.e. build-time. `for i in 0..N { … }` generating one spec per channel stays (L §8.6), each with its own path `gain[3]`.
4. **Axes use measure methods**: `vout.z().band(10Hz..=100kHz).max() <= 100Ω`.
5. **Helpers are `fn`**, pure functions over measures, callable from specs:
   ```rust
   fn gain_at(f: Hertz) -> Ratio { ac(vout.v / vin.v).at(f).mag() }
   spec gain(env: Operating) { require gain_at(1kHz) within GAIN ± 2%; }
   ```
   Specs are never called.
6. **Exactly one `require`, and it is last.** One spec, one verdict, one row in the table, one `SpecPath` (next_synthesis M1). Several conditions → several specs. This keeps the table and the text one-to-one (L §8.1).
7. **Conditional requirements** ("if in Sleep then …") are written with discrete environment quantities (`where mode == Sleep`), not with runtime `if`.

### P16. Bare names inside specs can be shadowed

**Example.** `let vin = dc(vin.v);` inside a spec.
**Fix.** Keep bare port names (they read best), and make it an error for a `let` or `fn` parameter to shadow a port, observable, generic parameter or setup name. The AI and the reader can then always resolve a name without looking at context.

### P17. Port quantity meaning in the presence of the setup's source and load

**Example.** `spec output_z(env: Operating) { require vout.z(f) <= 100Ω … }`. Under `Operating`, `vout` has a load of 10 kΩ ∥ 1 nF attached.
**The question.** Is `vout.z` the block's own output impedance (load removed, Thévenin of the block), or the impedance of the node (block ∥ load)?
**Why it matters.** The node impedance with a 1 nF load at 100 kHz is at most 1.6 kΩ ∥ z_out, so it would pass with a bad block. Composition needs the block's own impedance.
**Fix.** Define: `port.z` is **looking into the block with the setup's source or load at that port removed**; the other ports keep their setup. Similarly `port.i` is positive **into** the block. Write the definitions in the language reference once.

---

## 5. Multi-setup specs

### P18. Which knobs are shared between `zero` and `full`?

**Example.**
```rust
spec sensitivity(zero: AtZero, full: AtFull) {
    let per_mv = (mcu.code on full - mcu.code on zero) / 100mV;
    require per_mv within 12.0/mV ± 7%;
}
```
**The question.** Both setups inherit `usb: 4.40V..=5.25V` and `temp`. Part tolerances are clearly shared (same physical board; next_synthesis E7). But:
- if `usb.v` is independent per setup, the engine can pick 4.40 V at zero and 5.25 V at full, which is not a real measurement (two readings a second apart), and the ratiometric design gets a false alarm;
- for a **drift** spec (`code at Cold` vs `code at Hot`), `temp` must be independent.
**Why it matters.** Without a rule, the worst case is either too pessimistic (independent) or wrong for drift (shared).
**Fix: one board, one moment, unless the setup says otherwise.**
- Every knob **not overridden** by any of the spec's setups is **shared**: one value across all of them. That includes part tolerances, `temp`, `usb.v`, the sensor's source impedance.
- A knob **overridden** by a setup takes that setup's value, separately.
- So `AtZero`/`AtFull` share `usb.v` and `temp` (only `sensor.v` differs). A drift spec with `setup Cold: Operating { env { temp: -10°C } }` and `Hot` gets two temperatures, because both override `temp`.
- If an author really wants an un-overridden range free per setup, they say so: `spec s(zero: AtZero, full: AtFull) independent usb.v { … }`. Rare, and visible.

The engine side is next_synthesis E7's seam: a measure reads several runs that share one board.

### P19. A bare probe with no analysis

**Example.** `mcu.code on full` has no `dc(…)` or `tran(…)` around it; `wake_noise` writes `tran(mcu.code)`.
**Fix.** Either every probe needs an analysis, or a bare probe means `dc(…)`. Pick the second only if the language reference says it once; otherwise require `dc(full.code)`. The ADC code is quantized and non-smooth; the spec table should flag it (L §8.4).

### P20. Can a multi-setup child spec be reused by a parent?

**Example.** A block's calibration spec over `AtZero`/`AtFull`, placed in a larger board.
**Fix.** Reuse only if **each** of its setups is inside the child's accepted envelope, and the parent's context is inside each (P7). It is never a port fact (P22). The sharing rule (P18) is part of what the verdict means, so it can't be recomputed differently in the parent.

---

## 6. Globals, temperature zones and `rated`

### P21. `temp: ambient` makes a library block depend on the project

**Example.**
```rust
global ambient: Temperature in -10°C..=60°C;           // project.spl
setup Operating for GainStage { …, temp: ambient }      // gain_stage.spl, maybe a library
```
**The questions.**
- Is `GainStage`'s verdict "PASS over −10…60 °C"? Then moving the block to an automotive project (−40…125 °C) silently changes what it promises, and the cached result (keyed by definition, H §4.3) is wrong unless the key includes the global.
- Zones: L §8.2 had `#[env(temp = temp + 15K)]` at a placement. With setups naming `ambient` directly, a zone must rebind `ambient` for a subtree, which is dynamic scoping. Hard to read, hard to cache.
- Which global drives the simulator's temperature? The type `Temperature`? Then a second `Temperature` global (`board_hot`) is ambiguous.
- `global confidence = sigma(3)` is a policy, not a quantity. Same keyword, different nature.
- Two libraries each declaring `global ambient`: which one?

**Why it matters.** Globals are exactly the shared environment knobs that must be **joined** across children (core item 8; H §2.4: 6.5255 V false alarm vs 6.3646 V joined). Their scoping decides whether the join is right.

**Fix.**
- **Built-in environment quantities**, one of each: `temp`, `life`, and later `mode` (P8). They are the join keys. No user-declared globals of those kinds.
- **A block's setup writes its own numbers**: `env { temp: -40°C..=85°C }`. That is what the block promises over, independent of any project.
- **The root binds them**: the project's top setup gives the real range (`env { temp: -10°C..=60°C }`). At each placement, the parent's value is checked ⊆ the child's (the narrowing check of L §8.2, now automatic).
- **Zones are bindings at the placement**, as in L §8.2: `#[env(temp = temp + 15K, reason = "…")] let amp = …`. They move the join for that subtree (H §5 item 3).
- **Policies are settings, not globals**: `confidence` goes in the project's `[checks]` table (L §8.7) or a `#![confidence(sigma(3))]` inner attribute, overridable per block and per spec.
- **User `const`s** (with module paths, `project::VREF`) cover the other uses of `global`.

### P22. Confidence must travel with a reused verdict

**Example.** The LDO's `output` spec PASSes at `sigma(3)`. The board requires its `usb_current` at `worst_case`, and composition uses the LDO's `vout.v` fact.
**Why it matters.** A 3σ fact used as if it were a hard bound makes the board's worst-case PASS unsound. Several 3σ facts combined also lose yield (a union bound).
**Fix.** A reused child verdict carries its confidence. Composition may use it for a parent claim only at the same or weaker confidence; otherwise the child is re-checked at the stronger level (or in context). The report's `rests_on` (H §4.7) lists the confidences.

### P23. `rated` does two jobs

**Example.**
```rust
rated temp within -40°C..=85°C;        // GainStage: an operating temperature?
rated vin.v within -0.3V..=6.5V;       // Ldo3v3: an absolute maximum
```
**The question.** For `GainStage`, are specs checked over −40…85 °C, or over `ambient` (−10…60 °C), with −40…85 only a survival limit? The two lines use one keyword for the two different things the core (item 5) keeps apart.
**Why it matters.**
- If `rated temp` is the operating range, it contradicts `temp: ambient` in the setup. If it's a survival limit, there is no way left to say the operating temperature of a library block (P21).
- The abs-max check is what caught the reversed USB connector (−1.53 V on the LDO input) in all three options. It must be unambiguous.
**Fix.**
- **`rated` = absolute maximum only**: the block survives, no performance promised.
- The operating range is the accepted setup (P7, P21).
- `rated` has two duties, both checked:
  1. **a promise**: for every value inside `rated` (with other ports in the accepted setup), no internal part exceeds its own absolute maximum. The LDO's `-0.3V..=6.5V` is then *checked* against `Tlv755p`'s part record, not just copied;
  2. **an obligation on neighbours**: at every corner, during every accepted event, and during every fault (§7), the neighbour keeps the port inside `rated`.
- Voltages in `rated` are to the block's `Ground` port; the reference says so once. Duration-limited ratings (7 V for 10 ms) come later.

---

## 7. Faults

### P24. Faults have no syntax, and their natural spelling breaks the envelope rule

The draft lists "faults as timed events" as decided, but writes none. Two cases need different homes.

**Example 1, at a port** (the USB cable pulled out):
```rust
fault setup Unplug for SensorBoard: Operating {
    usb: Supply { v: 5V -> open, at: 1ms..=2ms },       // timing is a knob (core item 10)
}
spec holdup(env: Unplug) { require tran(code).after(usb.open).time_to_error(4) >= 100us; }
```
**Example 2, inside the board** (a shorted bypass capacitor): needs to name `c_dec`, an internal.

**The questions.**
- A fault is outside the operating envelope by nature. Under P7 its setup would need `#[beyond]` and could never compose. But a child's fault verdict ("the LDO survives a shorted output") *should* compose when the parent's fault presents that short to it.
- Internal faults name internals, which P1 forbids in the contract.

**Fix.**
- A **`fault` setup** is a marked kind: it derives from an accepted setup, is exempt from the ⊆ check, and is always subject to `rated` (P23) outside its fault window.
- A block may declare the faults it **tolerates**, as ranges: `tolerates OutputShort { r: ..=0.1Ω, for: ..=1s };`. A parent fault that presents a short at the child's port is discharged by the child's `tolerates` if contained. That's the fault version of the two-way check.
- **Internal faults** are implementation-level (P1's second tier): written next to the implementation, never composed.
- Measures get windows relative to the fault event: `.during(usb.open)`, `.after(usb.open)`.

---

## 8. Several implementations and generics

### P25. Which implementation does a placement use?

**Example.** `GainStage` gets a second implementation (an instrumentation-amp version). The board writes `let amp = GainStage<Mcp6001> { … }`.
**The questions.** Which body? Is the contract checked per implementation? What is cached?
**Fix.**
- **Implementations are named** when there is more than one; a block with one needs no name. One may be marked default.
  ```rust
  impl Circuit for GainStage as NonInverting { … }    // keyword undecided; the name is the point
  impl Circuit for GainStage as InAmp { … }
  ```
- **A placement picks one**: `let amp = GainStage::InAmp { … }`, or takes the default.
- **The contract is checked for every implementation**, each separately. The cache key (H §4.3) includes the implementation name and the generic arguments.
- **Composition uses the contract's bounds, not an implementation's measured values.** Both implementations publish `vout.z <= 100Ω`; the parent may rely only on that, so swapping implementations never invalidates the parent's reuse. (Rung-3 tables, H §4.6, are per implementation.)
- **Second sourcing** is free: `#[variants(NonInverting, InAmp)] let amp = GainStage { … }` means the board must PASS with *either* body, as a discrete knob enumerated by the engine (E5). That's a real EE need (alternates on the BOM) that no board tool checks.
- **Two mechanisms overlap**: the generic `A: OpAmp = Mcp6001` is also an "implementation choice". Rule of thumb for the reference: generics choose **parts or sizes inside one topology**; implementations choose **topologies**. Both enter the cache key.

### P26. Generics: is the contract over any `A: OpAmp`, or per chosen `A`?

**Example.** `GainStage<A: OpAmp = Mcp6001, const GAIN: f64 = 10.09>` with `spec bandwidth … f_high(-3dB) >= 20kHz`.
**Hand estimate.** With a 1 MHz GBW op-amp, the closed-loop bandwidth is about GBW / GAIN. At `GAIN = 10.09` that's ≈ 99 kHz: PASS. At `GAIN = 100` it's ≈ 10 kHz: FAIL. So the contract is **not true for every GAIN**, and "for every `A: OpAmp`" can't be checked at all (the set of op-amps is open).
**The question.** What does the standalone check (model.md q. 2: every block with a contract is checkable alone) prove?
**Fix.**
- **Monomorphize.** A contract is checked **per instantiation that exists**: the defaults for the standalone check, plus every instantiation a placement uses (`GainStage<Mcp6001, 10.09>`). This matches Rust and Spade, and model.md's elaboration already applies params before flattening.
- The report says which instantiations are proven: "PASS for `<Mcp6001, 10.09>`; `<Opa333, 50>` not checked yet".
- **`where` bounds on generics** state where the contract is meant to hold, and the compiler rejects a placement outside them: `where GAIN >= 1 && GAIN <= 20`.
- **Later: trait facts.** A trait bound can carry published facts (`trait OpAmp { fact gbw >= 1MHz; fact vos <= 5mV; … }`). A contract over any `A: OpAmp` would then be provable **from the trait's facts alone** (rung 1), not per part. Not needed now, but the design should leave room: it's what makes `impl Regulator for Ldo3v3` meaningful (a trait = required facts + suggested setups, P11), rather than a bag of hidden setups.
- **Impl headers must name the generics** (`impl<A: OpAmp, const GAIN: f64> Circuit for GainStage<A, GAIN>`, or a lighter form the lead picks). The draft's `impl Circuit for GainStage` leaves `A` and `GAIN` unbound in the body, where `u1 = A { … }` uses them.

### P27. A generic parameter in a setup

**Example.** The input must stay below the rail divided by the gain: `vin: Signal { v: ..=3.2V / GAIN, … }`.
**Fix.** Allow exact generic parameters in setup expressions (they're exact values, L §5.5). Then the setup is per instantiation too, consistent with P26.

---

## 9. Published port facts, and model.md's knobs

### P28. How composition finds the port facts

**Example.** The draft says published port facts are "ordinary specs, used for composition":
```rust
spec input_z(env: Operating)  { require vin.z(1kHz) >= 400kΩ; }
spec output_z(env: Operating) { require vout.z(f) <= 100Ω for f in 10Hz..=100kHz; }
spec supply(env: Operating)   { require dc(vdd.i) <= 150uA; }
```
**The questions.**
- How does composition know `input_z` is a fact about `vin`, and not a behavioural spec? By pattern (the measure is a single port quantity compared with a constant)? Then `let z = vin.z(1kHz); require z >= 400kΩ;` is the same claim, but is it still a fact? A harmless refactor would silently remove it from composition.
- `input_z` holds at 1 kHz only. What does the parent assume at 20 Hz?
- `supply` is one-sided. The LDO accepts `Load { i: 0.1mA..=50mA }`, two-sided. The EE board found the LDO's output impedance peaks at light load, so the **minimum** load matters, and a `<=` fact can't prove it.
**Fix.**
- **An explicit, restricted form** for facts, so a reader, the AI and the engine all see which specs compose:
  ```rust
  fact vin.z  >= 400kΩ   over 10Hz..=100kHz;
  fact vout.z <= 100Ω    over 10Hz..=100kHz;
  fact vdd.i  in 50uA..=150uA;
  ```
  A `fact` is sugar for a spec with one port quantity, an optional axis range and a bound, under the accepted setup. Anything more complex is an ordinary spec and doesn't compose.
- **Lint: a one-sided fact can't discharge a two-sided acceptance.** "`ldo` accepts `i: 0.1mA..`; nothing on `v3v3` publishes a minimum current."
- **Facts use the same shapes as acceptances** (P9), so containment is decidable when the shapes match, and falls back to in-context simulation, visibly, when they don't.

### P29. model.md's knob creation (E16) assumes `assume`, which v2 removes

**Current rule (E16):** knobs come from leaf part fields, from `assume` on top-level port quantities (range), and from `temp`. E24 resolves `assume` into a condition plus a knob.

**What v2 changes, and the open questions it creates:**
1. **Setups create the range knobs now.** Which setups? For a block checked as the root (E3), knobs come from **every setup its specs use**. A child's setups create no knobs in the parent: the child's port ranges become **bindings** to its neighbours (H §4.5).
2. **Knob identity across setups.** `Operating` and `InputStep` both have `vin.z`. Proposal: a knob is named by its **port quantity path** (`vin.z`, `vout.r`, `vdd.v`), and a setup only gives it a **range per scenario** (E5's `Scenario`: mode, range overrides, bench). Then T6's run rows, keyed by physical values, are shared between setups whenever the points coincide.
3. **Knob kind.** Every setup range is a **range** knob (for all), never statistical, so `sigma(3)` doesn't spread it. E16's "the kind follows where it came from" already implies this; write it down.
4. **Generated devices.** `vout: Load { r: 10kΩ.., c: ..=1nF }` needs a load resistor and capacitor in the deck. Flatten (or a bench stage before M1e lowering) must create them with **deterministic, reserved paths** (`bench.vout.r`) that can't collide with user names and stay stable under edits (E15, E16).
5. **Unbounded ends and the midpoint rule.** `r: 10kΩ..` has no midpoint, and model.md q. 3 makes the nominal a midpoint. It needs a rule: an open end is simulated as its limit (open circuit) at the corner (H §6 q. 3), and the nominal of a half-open range is its finite end, or a declared `nom`. For `100Ω..=10kΩ` a log midpoint (1 kΩ) is closer to intent than 5050 Ω (next_synthesis M4).
6. **Implementations and generics in elaboration.** The `Design` holds several bodies per block; flatten picks one per placement (P25) and applies the generic arguments (P26). Knob paths stay placement-based (`amp.r_f.value`), and the definition key gains the implementation and arguments.
7. **Multi-setup specs.** E24's "typed expression over probes" becomes an expression over (setup, probe) pairs; the engine's `Program::eval` reads several runs sharing one board (P18, next_synthesis E7).
8. **Contracts as instances with paths** (next_synthesis M1): `amp.gain`, and with implementations, the implementation name in the record.

### P30. A small correctness note on the draft's own numbers

Not semantics, but worth fixing before the draft is used as a test case:
- `value: (GAIN - 1) * 10k ± 1%` has an unparenthesized `*` as an operand of `±`, which grammar.md §4 and L §5.2 make an error. Write `((GAIN - 1) * 10k) ± 1%`.
- `gain … within GAIN ± 1%` likely FAILs. Hand estimate: with two ±1% resistors, `R_f / R_g` moves up to about ±2%, so the gain moves about ±1.8% at worst case (9.09 × 2% / 10.09). An op-amp with 1 MHz GBW has only about 1000× open-loop gain at 1 kHz, which takes another ≈ 1% off. This is the same kind of hand-arithmetic slip as the midscale spec in `contract_options.md` §1. The engine would catch it, which is a good demo, but the draft should say whether it's intentional.

---

## 10. Ranked recommendations

Ranked by how much each one protects soundness and reuse, then by how early it has to be decided (things that change the grammar first).

| # | Recommendation | Problems | Why this rank |
|---|---|---|---|
| 1 | **The contract names only the interface**: ports, generic parameters, declared observables (`observe code`). Checks on internals live with the implementation and never compose. | P1, P2, P24 | Without it, decision 2 (several implementations, one contract) has no meaning, and cache keys aren't stable |
| 2 | **Define `accepts` as the envelope**: it lists the operating setup and the accepted events; every spec's setup must derive from an accepted one and lie inside it (checked); events are ranges; otherwise `#[beyond]`, reported, never composed | P7, P11, P20 | This is what decides which child verdicts a parent may reuse |
| 3 | **No hidden meaning in setups**: every non-ground port appears; missing fields mean ideal and are written out by the formatter; signal inputs need amplitude and DC level; std setups are templates with written parameters | P6, P11, P12 | Without it, the connection check gives a different answer per reading of the same text |
| 4 | **Setup inheritance merges field by field**; the editor shows the resolved setup | P10 | The draft already uses both readings |
| 5 | **Multi-setup specs: one board, one moment**. Un-overridden knobs are shared; overridden ones are per setup; `independent` for the rare exception. Every probe qualified in a multi-setup spec | P5, P18, P19 | Decides false alarms vs missed drift; needed for calibration (E7) |
| 6 | **`rated` = absolute maximum only**, checked as a promise (against internal parts) and as an obligation (at corners, events and faults). Operating temperature goes in the setup | P23 | Removes a double meaning in a decided keyword; protects the abs-max finding |
| 7 | **Environment quantities are built-in (`temp`, `life`, `mode`) in an `env { }` group**; library setups write numbers; the root binds them; zones are placement bindings; `confidence` is a policy setting; reused verdicts carry their confidence | P3, P21, P22 | Makes the join (core item 8) well-defined and keeps library blocks portable |
| 8 | **Port facts get an explicit form** (`fact vout.z <= 100Ω over …`), shapes shared with acceptances, one-sided facts can't discharge two-sided acceptances, and `port.z` / `port.i` defined once | P9, P17, P28 | Composition must not depend on a pattern a refactor can break |
| 9 | **Spec bodies are pure, per-run functions** with one final `require`; no new knobs; `if`/`for` on exact values only; axes via `.band()` / `.over()`; helpers are `fn`; no shadowing of ports | P13, P14, P15, P16 | Needed before anyone writes many specs; mostly restates v0.1 rules for the new form |
| 10 | **Implementations are named, chosen at the placement, all checked against the contract, keyed separately in the cache**; `#[variants(…)]` checks alternates | P25 | Open item in the draft; the proposal is small and composes with #1 |
| 11 | **Generics are monomorphized**: the contract is checked per instantiation in use; `where` bounds; room for trait facts later; impl headers bind the generics | P26, P27 | Standard answer; says what a standalone check proves |
| 12 | **Modes are a discrete, shared environment quantity** with `match` in setups | P8 | Needed for the LDO Run/Sleep case (L3, E5); can follow #7 |
| 13 | **Faults**: `fault setup`, `tolerates` for composable faults, internal faults at implementation level, `.during()` / `.after()` windows | P24 | Decided in principle; syntax can come after #1–#2 |
| 14 | **Update model.md**: setups create range knobs named by port quantity path, scenarios per setup, generated bench devices with reserved paths, a rule for half-open ranges, implementations and generic arguments in the definition key | P29 | Needed at contract resolution (M1d-5); follows from #2, #3, #10, #11 |
| 15 | Fix the draft's `±` precedence error and state whether the `GAIN ± 1%` FAIL is intended | P30 | Cheap; keeps the draft usable as a test case |

**Suggested path.**
1. Settle #1–#4 first. They change what a setup and a contract *are*, and every later choice depends on them.
2. Then #5–#8, which decide composition. Rewrite the sensor board with them; the board exercises all of them (MCU observable, rail sharing, RC load shape, ratiometric sensitivity).
3. Then #9–#13 as the lead writes more of the language (the draft says the spec-line syntax waits for that).
4. Fold #14 into model.md when contract resolution starts.
