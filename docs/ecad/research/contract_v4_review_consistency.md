# Draft 4 Review: Consistency and Soundness

> 2026-09-30 · A fresh-eyes review of `contract_syntax_v4.md` (draft 4). Angle: **does every line in the example follow the rules in §1, and do the rules say enough to give one meaning to every line?** The §0 decisions are the lead's and are not reopened; every fix below is meant to fit inside them.
> Reads with: `contract_syntax_v3.md`, the four draft-2 reviews (`contract_v2_review_{semantics,syntax,engine,usability}.md`, cited as **SEM**, **SYN**, **ENG**, **USE** with their item numbers), and `contract_options.md` §1 (the common core, **core #n**).
> No simulations. The numbers in §9 are hand estimates and say so.

---

## Summary

Draft 4 is coherent at the level of its decisions. The problems are in the **joints between decisions**, where two rules meet and the draft doesn't say which one wins. The example hits most of those joints, so several of its lines have two readings today, and a few contradict §0 outright.

The issues that matter most:

1. **Modes and derivation collide, and the draft has no precedence rule.** `LoadStep` writes `vout.i: Step {…}`, but the modes it inherits from `Operating` also write `vout.i`. In `dip … on LoadStep in Run`, either the step wins or `Run`'s `5mA..=50mA` wins, and the second reading measures a load step with no step in it. The same collision hits `DropoutRow` (`vout.i: 500mA`). Nothing says whether a derived setup must stay inside every inherited mode (`LoadStep` in `Sleep` steps to 30 mA, far outside `Sleep`'s 1 mA).
2. **Two `pub spec`s break the internal-spec rule.** `recovers` names `v3v3`, an internal net, and `holdup` names `mcu.code`, a child's observable. §0 says specs that name internals are never published. The setups `McuWakes` and the board's modes also reach into children (`mcu.wake`, `mcu.state`), and `mcu.state` isn't even declared on `Stm32Adc`.
3. **"Unwritten = ideal" is kept, but "every port is covered" and "a signal needs a level" were lost.** `CeAmp`'s setup has no `input` or `output`. `GainStage`'s `vin: Signal { z: … }` has no DC level, so under "ideal" it accepts exactly 0 V, and the board's `sensor.v: 0V..=100mV` then fails its own connection check. An unwritten `Supply` voltage has no ideal value at all.
4. **Port quantities have no stated meaning, and the example depends on the missing rules.** `quiescent: dc(vin.i - vout.i)` is right only if output current is positive *out of* the block, while `supply: dc(vdd.i)` assumes positive *into* it. `vin.v` means the source's setting in `with vin.v = 4.3V` and in setup fields, but the pin voltage in `rated` and in measures. On `DropoutRow` the difference is up to 250 mV, the size of the spec's own limit.
5. **The one-liner and the function form aren't equivalent.** The function form has nowhere to write `in <mode>`, `with` or `for`, so `sensitivity` is checked in the board's `Idle` mode too, where the ADC is asleep. The sharing rule doesn't say that the slots share the mode, and `mcu.code[full]` indexes a bare observable, with no `dc(…)` around it.
6. **Composition rules are thinner than the decisions need.** Nothing says which child verdicts stay valid in a parent (only the default setup is "checked against"), how a `pub spec … in Run` is used when the parent's rail is sometimes in `Sleep`, or how a `pub` fault spec is discharged. The containment rule for derived setups (SEM P7) isn't in v4.
7. **The time vocabulary has three spellings of a step and two of an event,** and no rule for when a step happens, what `.settle` measures from, or what happens when the window is too short.

A few smaller consistency items (the trait spelling, `const confidence`, `Sweep` in a setup, `.first`, `db()` as a function) and a lost-findings table follow. §9 also flags a **circuit error in the example**: `GainStage` returns `r_g` to `mid` (1.65 V) while the sensor is ground-referenced, so the output sits at the negative rail. The engine would catch it, but it's meant to be the test case.

§10 ranks the fixes. Most of them are one sentence in §1.

---

## How to read this

Each issue has three parts:
- **Example:** a line from draft 4, or a small variation of it.
- **Why it matters:** the wrong verdict, the second reading, or the editor/agent failure.
- **Fix:** the smallest rule that respects §0.

Issues are grouped by topic (§1–§8), lost findings are in §8, numbers in §9, the ranking in §10.

---

## 1. Setups: derivation, modes and containment

### C1. A derived setup's field vs an inherited mode's field: which wins?

**Example.**
```rust
setup Operating for Ldo3v3 {
    vout: Load { c: ..=20uF },
    mode Run   { vout.i: 5mA..=50mA }
    mode Sleep { vout.i: 0.1mA..=1mA }
}
setup LoadStep for Ldo3v3 { vout.i: Step { 5mA -> 30mA, edge: 1us }, window: 300us, ..Operating }

spec dip: tran(vout.v).min() >= 3.25V   on LoadStep in Run;
```
The effective setup for `(LoadStep, Run)` has two writers of `vout.i`: `LoadStep` itself, and `Run`, which `LoadStep` inherits. Two readings:
- (a) the base's fields, then the mode, then the derived setup's own fields: the step wins;
- (b) the base, then the derived fields, then the mode (modes are applied last, "per mode"): `Run`'s DC range wins and `dip` measures a load with no step.

`DropoutRow { vout.i: 500mA, … ..Operating }` has the same collision, twice (once per mode).

**Why it matters.** Reading (b) gives a confident PASS on a transient that never happened. A form or the agent editing `Run`'s range would change `dip`'s meaning under one reading and not the other.

**Fix.** One sentence in §1: **"The effective setup for `(S, M)` is built in this order: the base setup's fields, then mode `M`'s fields, then `S`'s own fields. A later writer wins, field by field."** So derived setups override modes. The editor shows the effective setup per mode (SEM P10's "show the resolved setup").

### C2. Must a derived setup stay inside every mode it inherits?

**Example.** Under C1's order, `LoadStep` in `Sleep` steps the load 5 → 30 mA, while `Sleep` promises only 0.1–1 mA. `dip` says `in Run`, so the `(LoadStep, Sleep)` pair is never used by a spec. But `rated` is "checked on every setup", so the pair is still simulated.

**Why it matters.** Draft 4 never states the containment rule that decides whether a setup is inside the promise. v3 relied on it implicitly, and SEM P7 made it the basis of composition: *a non-tagged setup must lie inside the default setup; otherwise it needs `#[outside]`*. Without it:
- `LoadStep` in `Sleep` silently runs outside the operating range;
- a derived setup could widen a range (`vin.z: ..=10Ω` in a setup derived from `Operating`) and its verdicts would still look like part of the promise.

**Fix.** Two sentences:
- **"A setup without `#[fault]` or `#[outside]` must lie inside the default setup, field by field, in each mode a spec uses it in. The checker reports the first field outside."**
- **"A `(setup, mode)` pair that no spec uses isn't simulated, and `rated` is checked on the pairs that are."** For a static fault like `Reversed` with no spec, see C12.

### C3. Can a derived setup add, drop or edit a mode?

**Example.** Could `LoadStep` write `mode Run { vout.i: Step {…} }` to change only `Run`? Could it write `mode Standby { … }`?

**Why it matters.** §1 shows `mode M { … }` as a setup member without saying whether derived setups may use it. If a derived setup can add a mode, `in Standby` on a spec over `LoadStep` names a mode the default setup doesn't have, and parents are never checked against it.

**Fix.** **"A derived setup inherits the base's modes. It may write `mode M { … }` for an inherited mode, merged field by field (the same rule as fields); it may not add a mode. Only the default setup declares modes."** This also gives `LoadStep` a clean alternative spelling: `mode Run { vout.i: Step {…} }`.

### C4. `..Operating` looks like Rust's struct update but merges differently

**Example.**
```rust
setup AtZero for SensorBoard { sensor: Signal { v: 0V }, ..Operating }   // written, not path form
```
In Rust, `..base` fills only the fields *not listed*; a listed `sensor` replaces the whole value, so `sensor.z` would become ideal (0 Ω). §0 says "fields merge one by one", which means `sensor.z` is kept. A Rust reader and the rule disagree.

**Why it matters.** This is ENG P2's exact bug (`AtZero` and `AtFull` would see different source impedances). The rule is decided; the text just doesn't say it where a Rust reader would look. There's also no rule for a **shape change** (`vin: Signal {…}` over a base `vin: Supply {…}`).

**Fix.** In §1, next to `..Base`:
- **"Unlike Rust, `..Base` merges into nested shapes: `sensor: Signal { v: 0V }` keeps the base's `sensor.z`. The formatter rewrites it as the path form `sensor.v: 0V`."** Then the path form is the only form in derived setups, and the Rust reading never arises.
- **"Changing a port's shape (`Supply` to `Signal`) in a derived setup is an error."**

### C5. Can a field path create a field the base doesn't write?

**Example.** `GainStage`'s `Operating` has `vin: Signal { z: … }` with no `wave`; `InputStep` writes `vin.wave: Step {…}`. `LoadStep` writes `vout.i`, and `Operating` has no top-level `vout.i` (only the modes do).

**Why it matters.** It's clearly intended, but under "unwritten = ideal" the base *does* have a value there (ideal). Say so, and the path form is always an override, never a creation.

**Fix.** **"A path names a field of the port's shape; an unwritten field has its ideal value, so every path overrides."**

---

## 2. What a contract may name: internals, `pub`, children

### C6. Two `pub spec`s name internals

**Example.**
```rust
pub spec holdup:   tran(mcu.code).during(drop).deviation() <= 4    on Unplug;   // mcu: a child
pub spec recovers: tran(v3v3.v).after(drop.end).settle(1%) <= 1s   on Unplug;   // v3v3: an internal net
```
§0: internal specs "may name internal nets … never published". `v3v3` is declared by `circuit SensorBoard`, so `recovers` is internal *and* `pub`. `mcu.code` is `Stm32Adc`'s observable, which the board's contract can see only because `mcu` exists in the board's circuit; to `SensorBoard`'s interface it's internal too.

**Why it matters.** A parent relying on `recovers` would rest on a net that a second circuit (parked, but seamed) may not have. And the rule "a spec that names an internal is internal" is what makes internal specs safe; the example teaches the opposite.

**Fix.**
- **Rule: "A `pub spec` may name only the block's own ports, its generic parameters, and its own `observe`/`emits`. A spec that names anything else (a net, a child, a child's observable) is internal, and `pub` on it is an error."**
- In the example: drop `pub` from `holdup` and `recovers`, or give `SensorBoard` an observable, `observe code: Integer`, bound in the circuit (for example `observe code = mcu.code;`; the binding syntax is new and small). `recovers` can't be public as written; `v3v3` isn't a port.

### C7. Setups that reach into children

**Example.**
```rust
setup McuWakes for SensorBoard { event: mcu.wake at 1ms, window: 5ms, ..Operating }
mode Measuring { mcu.state: Run }
```
`mcu.wake` is declared (`emits`), but it's the child's, so the setup depends on the board's circuit. `mcu.state` is **not declared anywhere**: `Stm32Adc` has `observe code` and `emits wake`, no states.

**Why it matters.** ENG P3 and SEM P2 found that a setup reaching into a child breaks encapsulation and caching. Draft 4 fixed it for contracts (`observe`, `emits`) but the example now reaches in through setups. §3 item 3 lists "how modes pick child states" as open; the example should at least not use an undeclared member.

**Fix.**
- **Rule: "A setup that names a child's `emits` or state is an internal setup. Every spec on it is internal (C6)."** This keeps `wake_noise` legal and makes its status visible.
- Declare states on the model block, even provisionally: `pub block Stm32Adc { …, state: Run | Sleep, observe code: Integer, emits wake: … }`. The per-state current stays open (§3.3).

### C8. Internal specs bind to the one circuit

**Example.** `spec base_bias: dc(base.v) …`, `spec mid_ok: dc(mid.v) …`, `spec rail_ok: dc(vdda.v) …`.

**Why it matters.** Fine today (one circuit per block). But the rules don't say where the contract's names are resolved: ports first, then the circuit's nets? What if a net and a port share a name? And an internal spec that names a net deleted in an edit must fail at resolve, not at check time.

**Fix.** **"Names in a contract resolve to ports, generic parameters and the block's observables first, then to the circuit's nets and parts. A net may not shadow a port."** This is also what the second-circuit seam needs later: internal specs attach to a circuit, public ones to the block.

---

## 3. Specs: the two forms, indexing and sharing

### C9. The function form can't say `in`, `with` or `for`

**Example.**
```rust
spec sensitivity(zero: AtZero, full: AtFull) {      // no place for `in Measuring`
    let per_mv = (mcu.code[full] - mcu.code[zero]) / 100mV;
    ensure per_mv within 12.0/mV ± 7%;
}
```
§1: "every spec is checked in each mode". So `sensitivity` is also checked in `Idle`, where `mcu.state: Sleep`. An ADC that's asleep has no meaningful code: a false FAIL, or an UNDECIDED, on a spec that's fine.

**Why it matters.** §0 says the two forms serve different needs, not different meanings; a spec shouldn't lose `in` because it needs two setups. Forms and the agent convert between them.

**Fix.** Define the one-liner **as sugar for** the function form, and give the function form the same clauses:
```rust
spec name: M L with W for F on S in K;
// means
spec name(s: S) in K { ensure M L with W for F; }
```
- `in K` goes on the function header: `spec sensitivity(zero: AtZero, full: AtFull) in Measuring { … }`.
- `with` and `for` go on `ensure`.
- `spec name() { … }` (no parameters) uses the default setup.
- State whether a body may have several `ensure`s (ENG 4.2 lowered them to sides `spec#0`, `spec#1`); recommend yes.

### C10. The sharing rule doesn't cover modes, `env` or part ranges

**Example.** `sensitivity`'s `AtZero` and `AtFull` both inherit `Measuring` and `Idle`. Nothing says the engine can't pair `zero` in `Measuring` with `full` in `Idle`. Also: `beta: 100..=300` is a part range written with `..=`, not a tolerance. Is it a "statistical knob" (always shared) or a "range knob" (shareable unless overridden)?

**Why it matters.** The rule's point is "the same board at the same moment". A mode is part of the moment; a part's beta is part of the board. Today's wording ("statistical knobs (part tolerances)", "range knobs (temperature, supply)") sorts by knob kind, when it means to sort by **where the knob lives**.

**Fix.** Reword §1's two bullets:
- **"Part knobs (every field of a part in the circuit, tolerance or range) are always shared."**
- **"Setup and `env` knobs, and the mode, are shared unless a setup writes that field. Writing a field, even with the same range, gives that setup its own knob."**

The last sentence replaces v3's `fresh x` (dropped in v4 without a replacement), using ENG §3.3's no-new-keyword route.

### C11. What `m[s]` may index, and unindexed measures in a multi-setup spec

**Example.** `ac(out.v)[dm].at(60Hz).mag()` indexes an analysis; `mcu.code[full]` indexes a bare observable with no `dc(…)`. And what does an **unindexed** measure mean inside `cmrr`?

**Why it matters.** SEM P19 asked for "every probe needs an analysis" and "every probe qualified in a multi-setup spec". Both were lost. A bare `mcu.code` could be the DC value, or the last transient sample. In a two-setup spec, an unindexed `ac(out.v)` has no setup.

**Fix.**
- **"`[s]` follows an analysis: `dc(x)[s]`, `ac(x)[s]`, `tran(x)[s]`, or a contract `let` that is one."** Rewrite `mcu.code[full]` as `dc(mcu.code)[full]`.
- **"In a spec with two or more setups, every analysis is indexed. With one setup, `[s]` is optional."**
- **"A setup parameter may not share a name with a port, net or observable."** (SEM P16; `dm`/`cm` sit next to the `Pair`'s `.dm`/`.cm` fields today, harmless but close.)

### C12. `with` and `for`: what they may name

**Example.** `psrr … with vin.v = 4.3V`. Could a spec write `with vin.v = 3.0V` (outside `Operating`)? `with ambient = 25°C`? `with r1.value = 47k` (a part)?

**Why it matters.** A `with` outside the setup's range is a one-off by the back door, dodging `#[outside]`. A `with` on a part makes the spec about one specific part, which breaks "over all tolerances".

**Fix.** **"`with` pins a setup field or an `env` knob to a value inside its range. Outside the range, write an `#[outside]` setup. Parts can't be pinned."** Also say `.span(for x)` takes its range from the `for x in r` clause or, if there is none, from the setup.

---

## 4. Port quantities: the definitions the example depends on

### C13. The current sign convention decides whether `quiescent` is right

**Example.**
```rust
spec quiescent: dc(vin.i - vout.i) <= 50uA   in Sleep;   // Ldo3v3
pub spec supply: dc(vdd.i)         <= 150uA;             // GainStage
```
`supply` assumes current is positive **into** the block (a supply draws a positive current). Under that convention, `vout.i` is negative (current flows out), so `vin.i - vout.i` = I_q + 2·I_load, not I_q. `quiescent` is right only if output current is positive **out of** the block.

**Why it matters.** At 1 mA load in `Sleep`, the two readings differ by 2 mA against a 50 µA limit: a certain FAIL under one convention, a PASS under the other.

**Fix.** **"`p.i` is positive into the block at every port"** (SPICE's terminal convention; SEM P17). Rewrite `quiescent` as `dc(vin.i + vout.i)` (equal to `-gnd.i`).

### C14. `p.v` means two things: the source's setting and the pin

**Example.**
```rust
setup DropoutRow for Ldo3v3 { vout.i: 500mA, vin.v: Sweep(4.5V -> 3.0V), ..Operating }   // Operating: vin.z up to 0.5 Ω
rated vin.v within -0.3V..=6.5V;
spec dropout: vin.v.first(vout.v < 3.267V) - 3.267V <= 250mV   on DropoutRow;
```
In the setup and in `with vin.v = 4.3V`, `vin.v` is the **source's open-circuit voltage**. In `rated` it must be the **pin** (the part sees the pin). In `dropout` it's ambiguous. At 500 mA through up to 0.5 Ω, the two differ by up to 250 mV, which is the whole limit.

**Why it matters.** A reversed or brown-out source is caught by `rated` only on the pin reading. `dropout` passes or fails depending on the reading.

**Fix.** One rule, stated once: **"In a setup field and in `with`, `p.v` and `p.i` are the source's or load's own setting. In a measure and in `rated`, they're the value at the pin. `p.z(f)` looks into the block with that port's source or load removed (SEM P17). Voltages are to the block's `Ground` port."** Then `dropout` becomes `dc(vin.v)` (the pin) with an explicit axis (C24).

### C15. Which source carries the AC excitation?

**Example.** `GainStage`: `ac(vout.v / vin.v)` on `Operating`, with no `.ac` field. `InAmp`: `ac(out.v)[dm]` on `Diff`, which writes `(inp, inn).dm.ac: 1V`. `Ldo3v3`: `ac(vout.v / vin.v)` excites the supply.

**Why it matters.** The rule is implicit, and the example uses two conventions. A novice copying `InAmp` into `GainStage` adds an `ac` field; one copying `GainStage` into `InAmp` gets a measure with no excitation, or with two.

**Fix.** **"`ac(x / p.v)` excites port `p`'s source with a unit AC and every other source with none. A non-ratio `ac(x)` needs exactly one `.ac` field in its setup."** That is exactly what the example does; it only needs saying.

---

## 5. Composition: what a parent may rely on

### C16. Which child verdicts hold in a parent, and when

**Example.** `dip` (on `LoadStep`) and `quiescent` aren't `pub`. §0: parents "are checked against" the default setup, and `pub` specs are what "parents may rely on".

**Why it matters.** Two different things are mixed:
- **(a) Does the child's own verdict still hold in this parent?** For every spec, `pub` or not, internal ones included. It holds if the parent keeps the child inside the setups that spec used.
- **(b) Can the parent use the child's spec as a fact to prove its own specs and connections?** Only `pub` specs.

Draft 4 states only (b) and the "checked against the default setup" half of (a). It doesn't say what happens to `dip` when the board's MCU steps the rail 5 → 30 mA: is the LDO's `dip` verdict reused, or is it re-checked in context? SEM P7 point 3 and core #7 need that answer.

**Fix.** Two sentences:
- **"A child's verdict holds in a parent when the parent keeps the child inside that spec's setup: the default setup, or for a derived setup, the base plus a stimulus contained in the derived one (a step of the same shape with its values inside). Otherwise the spec is re-checked in context, and the report says why."**
- **"Only `pub` specs are facts the parent may use in its own proofs."**

### C17. A `pub spec … in Run` is a conditional fact

**Example.**
```rust
pub spec output_z: vout.z(f) <= 2Ω   for f in 10Hz..=1MHz  in Run;   // Ldo3v3
```
On the board, `amp.vdd` accepts `z: ..=20Ω`, fed through `r_filt` (10 Ω) from the LDO. In `Measuring` the LDO's load is in `Run`, and the fact gives ≤ 12 Ω: holds. In `Idle` the MCU sleeps and the LDO is in `Sleep`, where it publishes no impedance (it peaks at light load: core, "3.2 Ω at 1 mA").

**Why it matters.** The rules don't say how a parent maps its own mode to a child's mode, or whether a fact `in Run` may be used when the child's port is in `Sleep`'s range. Used unconditionally, it's unsound; unusable, it makes every `in` fact dead weight.

**Fix.** **"A `pub spec … in M` is a fact only where the parent proves the child's ports are inside mode `M`'s fields. The child's mode at each parent point is the mode whose fields contain what the parent presents; a point in no mode fails the connection check."** That's the containment check applied per mode; it needs no board-to-child mode syntax (§3.3 can still add an explicit binding later).

### C18. A `pub` fault spec has no discharge rule

**Example.** `pub spec holdup … on Unplug` (setting aside C6). §0: fault specs "may be `pub`".

**Why it matters.** A `#[fault]` setup is deliberately outside the operating range, so the default-setup containment check says nothing about it. What does a parent have to show to rely on "holds up for a 0–10 ms drop"? SEM P24's answer (`tolerates`, contained events) wasn't carried forward.

**Fix.** **"A `pub` spec on a `#[fault]` setup is a fact about that event: a parent may use it only where its own fault presents the same event at the child's port with its ranges inside the child's (duration, timing). Otherwise the child is re-checked in the parent's fault."** No new keyword; the fault setup itself plays `tolerates`.

---

## 6. `rated`, faults and one-offs

### C19. Whose `rated`, on whose setups, with what verdict?

**Example.** `SensorBoard` has no `rated` line, and `Reversed` has no spec. The comment says "every `rated` limit is checked on every setup". The limit that catches the reversed USB is the **LDO's** `rated vin.v`, checked on the **board's** setup.

**Why it matters.** The rule as written reads as "the contract's own `rated`". The reversed-USB finding (core, found by all three options) depends on the hierarchical reading. Also:
- a violation needs a place in the report (which spec FAILed?);
- a setup no spec uses (`Reversed`) needs an analysis to run: DC, or a transient over its window.

**Fix.** **"Every `rated` line of every block in the hierarchy, and every part's absolute maximum, is monitored at the pins in every run of every setup and mode (C2's used pairs, plus every `#[fault]` setup). A setup with a `window` is watched over the whole window; one without is checked at its DC operating point. A violation is a FAIL of `rated <line>` on that setup, reported with its run."** (ENG P8's monitors.)

### C20. `#[fault]` "built around an event", but `Reversed` has none

**Example.** `#[fault] setup Reversed for SensorBoard { usb.polarity: reversed, ..Operating }`.

**Why it matters.** It contradicts §0's wording, and a reader will wonder whether `Reversed` is valid.

**Fix.** Amend the rule: **"A `#[fault]` setup changes something outside the operating range: a static change (`usb.polarity: reversed`) or an `event`."**

### C21. Does a FAIL on an `#[outside]` spec fail the block?

**Example.** `spec dropout … on DropoutRow; // characterization`.

**Why it matters.** "Tagged characterization" doesn't say whether it counts toward the block's verdict. If it does, a datasheet row the part was never meant to meet can block the board. If it doesn't, the reader needs to see that in the report.

**Fix.** **"Specs on `#[outside]` setups are reported in their own section. They count toward the block's verdict but are never facts or reused in a parent."** (Or "don't count"; either is fine, it just has to be one.)

### C22. The default setup can't be a fault or a one-off

**Fix.** **"`setup =` names a setup without `#[fault]` or `#[outside]`, not derived."** One line; otherwise `setup = Unplug;` is legal.

---

## 7. Time: steps, events, windows

### C23. Three spellings of a step, two of an event

**Example.**
```rust
vout.i:   Step { 5mA -> 30mA, edge: 1us }                // the value of the field it steps
vin.wave: Step { 0V -> 100mV, edge: 1us }                // a separate `wave` field
emits wake: Step on vdd.i { 5mA -> 30mA, edge: 1us }     // `on <path>` between the kind and the body

event: mcu.wake at 1ms                                   // unnamed, `at` outside braces
event drop: usb.open { at: 1ms, for: 0us..=10ms }        // named, `at` inside braces
```
§1 shows only `event e: …`.

**Why it matters.** USE U8 found three spellings of a step in draft 2 and proposed one ("as a value of the field it steps"). Draft 4 is back to three. The agent and forms must pattern-match all of them; a novice can't tell whether they differ.

**Fix.**
- A step is **the value of the field it steps**: `vin.v: Step { 0V -> 100mV, edge: 1us }`; `emits wake: vdd.i: Step { 5mA -> 30mA, edge: 1us }` (or `emits wake: Step { … } on vdd.i`; pick one).
- An event is always **`event name: <what> { at: …, … }`**: `event wake: mcu.wake { at: 1ms }`.

### C24. When does a step happen, and what does `.settle` measure from?

**Example.** `LoadStep` and `InputStep` give no `at:`. `settling: tran(vout.v).settle(1%) <= 50us on InputStep`: settle from the step's edge, or from t = 0? Settle to what final value?

**Why it matters.** ENG P6 ("`final` = a DC operating point at the settled stimulus; stop time and window are setup fields; never settles is `Beyond`") was lost. A step at t = 0 also has no pre-step steady state to measure a dip against.

**Fix.**
- **"A step without `at:` happens at `at: 0` after a DC operating point at its first value."**
- **"Time measures (`.settle`, `.crossing`) count from the setup's single event or step. With several, `.after(ev)` is required. `.settle(x%)` settles to the DC operating point at the final stimulus."**
- **"`tran(…)` needs a setup with a `window`."**

### C25. A measure that needs more time than the window

**Example.** `recovers: … .settle(1%) <= 1s on Unplug`, with `window: until drop.end + 1s`. A rail that takes 1.2 s isn't observed settling.

**Why it matters.** "Didn't settle in the window" must not read as a small number or a PASS.

**Fix.** **"A time measure that isn't reached inside the window is FAIL with the reason `not reached by window end`."** The formatter may warn when a `<=` limit equals the remaining window.

---

## 8. Items, generics, and consistency with §1

### C26. `const confidence` is a policy, spelled as a value

**Example.** `const confidence: Confidence = sigma(3);`. §1 shows `const NAME`, and the generic `const GAIN` is upper case; Rust's `const` is upper case by convention. Nothing in the example reads `confidence`; the engine must know the name.

**Why it matters.** §0 says `const` has Rust's meaning (a fixed value used in expressions). A magic lower-case name that changes the check policy isn't that. SEM P21 made the same point about v2's `global confidence`.

**Fix.** Keep `const` for values, and write policy as a crate-level attribute, like `#[check]` and `#[fault]`: `#![confidence(sigma(3))]` in `project.spl`, with `#[confidence(…)]` per contract or spec.

### C27. Where `env` may appear, and the unused `life`

**Example.** `env life: Duration in 0y..=10y;` is never used; no setup field binds it. `CeAmp`'s `Operating` uses `temp: ambient`.

**Why it matters.** Draft 3 had a rule (v3 #14: library setups write numbers, only the root binds globals) that draft 4 dropped. Also unsaid: can a bound, a part value or a `const` use `ambient`? It's a knob, so no.

**Fix.**
- **"`env` values appear only as setup field values (`temp: ambient`, `life: life`). They aren't exact, so they can't appear in a bound, a part value, a `const` or a generic argument."**
- **"A setup of a block placed as a child should write numbers. With an `env`, its verdict depends on the project (and the cache key does too); a lint says so."**
- Name the setup field `life` binds, or drop `env life` from the example until aging exists.

### C28. `const` vs generic `const`

**Example.** `const GAIN: f64 = 10.09` (generic) vs `const confidence: Confidence` (project), `env ambient: Temperature`.

**Why it matters.**
- ENG P12: `f64` is unitless while every other item has a unit type. `const GAIN: Ratio` would be checked like every field.
- `#[check(A = [Mcp6001, Tlv9001])]` doesn't say what's checked besides the list. SEM P26: every instantiation a placement uses is checked too. Draft 4 doesn't state it.

**Fix.**
- Unit types on generic consts (`const GAIN: Ratio = 10.09`).
- **"A contract is checked for its `#[check]` list, for its default arguments, and for every instantiation a placement uses. Parameters not listed take their defaults. A project `const` may be used as a generic default."**

### C29. Trait membership: `: Regulator` vs `impl Regulator for`

**Example.** The example writes `pub block Ldo3v3: Regulator { … }`. §1 lists only `trait T { … } impl T for X {}`.

**Fix.** Pick one and use it in both places. `impl Regulator for Ldo3v3 {}` matches §1, draft 3 and Rust. If the header form stays, add it to §1 and say what `Regulator` brings. If it brings a std `LoadStep`, that name clashes with the example's own `LoadStep`: say a local setup shadows the trait's, or forbid the clash.

### C30. Two ways to sweep: `Sweep(…)` in a setup and `for x in r` in a spec

**Example.** `DropoutRow { vin.v: Sweep(4.5V -> 3.0V) }` with `vin.v.first(vout.v < 3.267V)`.

**Why it matters.**
- core #4 and §1's own table give a swept axis one home: `for x in r` on the spec.
- `.first(…)` isn't in §1's measure list (only in §3 item 1).
- `vin.v.first(…)` has no analysis around it (C11).

**Fix.** Write it with the existing vocabulary:
```rust
setup DropoutRow for Ldo3v3 { vout.i: 500mA, ..Operating }
spec dropout: dc(vin.v).at_crossing(dc(vout.v), falling 3.267V) - 3.267V <= 250mV
              for vin.v in 3.0V..=4.5V   on DropoutRow;
```
The method name is §3 item 1's to settle; the point is that the axis is a `for`, and the setup stays static.

### C31. Small items

| Where | What | Fix |
|---|---|---|
| `cmrr` | `db(a_dm / a_cm)` as a function; §1 has only `.db()` | `(a_dm / a_cm).db()`, or list `db()` too |
| §1 setup line vs example | `mode M { … },` has a comma in §1, none in the example; `event …,` has one | Say modes are members, not fields, with no comma; the formatter decides |
| `for f in 10Hz..=1MHz in Run` | `in` right after a range reads as part of it (`in` also appears in `env … in range`) | Canonical clause order `[on S] [in M] [with …] [for …]`, so `in M` never follows a range. That changes §1's order, not a decision |
| `Pair { …, z: 350Ω ± 0.1% }`, `Supply { v: 12V ± 5% }` | `±` is a statistical tolerance in a circuit and a range in a setup | **"Every value in a setup is a range, never statistical: `±` there is range sugar"** (SEM P29 point 3). Say whether `Pair.z` is per leg |
| `h.f_high(-3dB, ref: dc)` vs `h.f_low(-3dB)` | `f_low`'s default reference is unsaid; `ref: dc` reuses the analysis name | §3 item 1 |
| `Stm32Adc`, `InAmp` | Blocks with no `circuit`. §0 says one per block | **"A block with no circuit is a model block: its behaviour comes from a model record."** For `InAmp` it's just elided; say so in a comment |
| `setup S for X` | Who may write a setup for `X`? (SEM P4) | **"A block's contract may use only setups for that block; any file may declare them."** |
| `observe code: Integer` | a quantized measure (ENG P5) | Engine rule, not syntax. Keep ENG's `MeasureKind::Quantized` in the engine plan |

### C32. Earlier findings: still handled, partly, or lost

| Finding | Source | In v4? | Where it's needed |
|---|---|---|---|
| Unwritten field = ideal, written out by the formatter | SEM P6, ENG P1 | **Kept** (§0) | — |
| Every non-ground port appears in the default setup | SEM P6, v3 #7 | **Lost.** `CeAmp`'s `Operating` has no `input`/`output`; `InAmp` has no `out` | C33 |
| Signal inputs need a level; `Supply` needs a voltage | SEM P6 | **Lost.** `GainStage`'s `vin` has no `v` | C33 |
| Field-by-field merge | SEM P10, ENG P2 | **Kept**, but the Rust spelling contradicts it | C4 |
| Derived setups must lie inside the accepted one | SEM P7 | **Lost** | C2 |
| Events as ranges, for reuse | SEM P7 | **Lost** (every step is a fixed point) | C16 |
| Contracts name only the interface | SEM P1, v3 #6 | **Replaced** by internal specs, with a hole for `pub` | C6, C7 |
| Sharing rule, and `fresh` | SEM P18, ENG §3.3, v3 #12 | **Partly**: rule kept, `fresh` dropped with no replacement, modes unsaid | C10 |
| Every probe has an analysis; every probe indexed in a multi-setup spec | SEM P19 | **Lost** | C11 |
| Spec body rules (pure, no `if` on measured values, one or more `ensure`s) | SEM P15, ENG 4.1 | **Lost** | C9; restate ENG 4.1's table in §1 |
| `port.z` and `port.i` defined | SEM P17 | **Lost**, and the example depends on it | C13, C14 |
| Library setups write numbers; `env` only at the root | SEM P21, v3 #14 | **Lost** | C27 |
| Policy isn't a global | SEM P21 | **Lost** (`const confidence`) | C26 |
| Transient window, final value, "never settles" | ENG P6, USE P6 | **Partly**: `window` kept; final value and "not reached" lost | C24, C25 |
| `rated` as monitors on every run, faults included | ENG P8, SEM P23 | **Kept** in spirit; hierarchy and reporting unsaid | C19 |
| One step spelling | USE U8/P6 | **Lost** | C23 |
| Generics checked per instantiation in use | SEM P26, ENG §6.3 | **Partly** (`#[check]` only) | C28 |
| One-sided facts can't discharge two-sided acceptance | SEM P28 | **Lost** (`supply <= 150uA` vs `Sleep`'s 0.1 mA minimum) | A lint; §10 #14 |
| Facts at one frequency | ENG P11 | **Unchanged** (`vin.z(1kHz)`) | The check falls back, visibly; fine for now |
| Confidence travels with a reused verdict | SEM P22 | **Unsaid** | Engine rule; note it in the plan |
| Sampling grid for `for f in` | ENG P4 | **Unsaid** | Engine rule |

### C33. Port coverage, and the ideal value of a signal

**Example.**
```rust
setup Operating for CeAmp     { vcc: Supply { v: 12V ± 5% }, temp: ambient }   // no input, no output
setup Operating for GainStage { vin: Signal { z: 100Ω..=10kΩ }, … }            // no level
setup Operating for SensorBoard { sensor: Signal { v: 0V..=100mV, … }, … }     // drives amp.vin
```
Under "unwritten = ideal", `GainStage` accepts `vin.v` = exactly 0 V (the ideal DC level). The board presents 0–100 mV, so `board.amp`'s connection check **fails on the draft's own board**, for a reason the author didn't write. For `CeAmp`, "ideal" gives an ideal AC source on `input` and an open `output`, which matches ENG's table but isn't visible. For a `Supply`, there's no ideal voltage to fall back on.

**Why it matters.** The purpose of "unwritten = ideal" (SEM P6, ENG P1) was to make the connection check fail *honestly* on a missing field. For a signal's level or a supply's voltage, "ideal" isn't a meaningful value, and the check fails for no visible reason.

**Fix.** Three sentences:
- **"Every non-ground port appears in the default setup. The formatter writes an unwritten one as its ideal (`output: Open`, `input: Signal { v: 0V, z: 0Ω }`)."**
- **"`Signal.v` and `Supply.v` have no ideal: leaving them out is an error with a fix-it."**
- Fix the example: `vin: Signal { v: 0V..=100mV, z: 100Ω..=10kΩ }` in `GainStage` (see also §9), and write `input`/`output` in `CeAmp`.

---

## 9. Numbers in the example (hand estimates)

These aren't language issues, but the example is the test case, so they're worth fixing or labelling.

**N1. `GainStage` returns `r_g` to `mid`, so a ground-referenced input saturates the output.** With an ideal op-amp, V(fb) = V(vin), and the current through `r_g` equals the current through `r_f`:

(V_in − V_mid) / R_g = (V_out − V_in) / R_f, so V_out = V_in · (1 + R_f/R_g) − V_mid · R_f/R_g.

With R_f/R_g = 9.09 and V_mid ≈ 1.65 V: V_out = 10.09 · V_in − 15.0 V. For the board's `sensor.v` of 0–100 mV that's −15.0 to −14.0 V, so the output sits at the negative rail (0 V). `sensitivity` then reads a code near 0 at both points: FAIL. Under "unwritten = ideal", `GainStage`'s own check (with `vin` at 0 V) biases the op-amp in saturation too, so `gain` and `bandwidth` FAIL. Fix: return `r_g` to `gnd` (a single-supply non-inverting stage for a ground-referenced sensor). Or keep `mid` and bias `vin` at mid in both setups (`v: 1.6V..=1.7V`), which the board's sensor doesn't do. Draft 3 had the same circuit; SEM P30 checked the gain arithmetic but not the bias point.

**N2. `holdup` likely FAILs for most of its drop range.** After the unplug, the LDO's input has only `c_in` (1 µF). At the MCU's 5–30 mA, the input falls from about 4.4 V to the LDO's dropout (≈ 3.3 V + 0.25 V) in roughly 1 µF × 0.85 V / 30 mA ≈ 28 µs (up to ≈ 170 µs at 5 mA). The ADC's reference is `v3v3` itself, so the code moves with the rail. At full scale (≈ 1,250 codes), 4 codes is 0.3%, or about 10 mV on `v3v3`, and the output capacitance (≈ 11 µF, mostly behind 10 Ω) loses 10 mV in well under a millisecond. So a drop longer than about 0.2 ms FAILs, and `for: 0us..=10ms` includes 10 ms. That may be intended, as a demonstration. If so, say so in a comment. If not, narrow the drop or add hold-up capacitance.

**N3. `wake_noise` pairs a wake event with a running MCU.** `McuWakes` fires `mcu.wake` (5 → 30 mA), and the spec is `in Measuring`, where `mcu.state: Run`. A wake from `Run` is odd. Either the spec belongs `in Idle`, or the event interacts with the mode, which no rule covers yet (it belongs with §3 item 3).

---

## 10. Ranked fixes

Ranked by how much each protects soundness (a wrong verdict first), then by how early it must be settled (grammar before engine rules). Each one is a sentence or two in §1, plus the example edits noted.

| # | Fix | Issues | Why this rank |
|---|---|---|---|
| 1 | **Effective setup order:** base fields → mode `M` → the derived setup's own fields, later writer wins per field. Derived setups may edit inherited modes, not add them | C1, C3 | `dip` has two readings today, one of which measures no step |
| 2 | **Containment:** a setup without `#[fault]`/`#[outside]` lies inside the default setup in each mode a spec uses it in; unused `(setup, mode)` pairs aren't simulated | C2 | Decides what is inside the promise; lost since SEM P7 |
| 3 | **`pub` names only the interface;** a spec or setup that names a net, a child or a child's emits/state is internal. Drop `pub` from `holdup`/`recovers` (or add `observe code` to `SensorBoard`); declare `state` on `Stm32Adc` | C6, C7, C8 | The example contradicts §0 |
| 4 | **Port definitions:** `p.i` positive into the block; `p.v` in setups and `with` is the source's setting, in measures and `rated` the pin; `p.z` with that port's source or load removed; voltages to `Ground`. Rewrite `quiescent` as `dc(vin.i + vout.i)` | C13, C14 | Two example specs change verdict with the reading |
| 5 | **Port coverage:** every non-ground port appears; `Signal.v`/`Supply.v` have no ideal. Fix `GainStage`'s `vin` and `CeAmp`'s ports | C33 | The draft's own board fails its connection check for an invisible reason |
| 6 | **One-liner = sugar for the function form;** the header takes `in M`, `ensure` takes `with`/`for`; add `in Measuring` to `sensitivity` | C9 | A multi-setup spec can't be narrowed to a mode: false FAIL in `Idle` |
| 7 | **Sharing by where the knob lives:** part knobs always shared; setup, `env` and mode shared unless a setup writes the field | C10 | Closes the mode hole and replaces `fresh` |
| 8 | **Composition:** child verdicts hold when the parent keeps the child inside the spec's setup (stimulus contained); `pub … in M` is a fact only where the child is in `M`; a `pub` fault spec discharges a contained parent fault | C16, C17, C18 | What makes reuse sound; the decisions already imply it |
| 9 | **Indexing:** `[s]` follows an analysis; every analysis indexed in a multi-setup spec; no parameter shadows a port. Write `dc(mcu.code)[full]` | C11 | Otherwise a probe has no setup or no analysis |
| 10 | **`rated` across the hierarchy,** at the pins, on every used pair and every fault setup; DC or over the window; reported as `rated <line>` on that setup. `#[fault]` = a static change or an event | C19, C20 | Protects the reversed-USB finding |
| 11 | **Time:** one step spelling (the field's value), one event spelling (`event name: … { at: … }`); a step without `at:` is at 0 after an operating point; time measures count from the setup's event; `.settle` settles to the final operating point; `tran` needs a `window`; not reached = FAIL | C23, C24, C25 | Needed by every transient spec |
| 12 | **AC excitation rule** (`ac(x / p.v)` excites `p`; otherwise exactly one `.ac` field); **`with`** pins setup or `env` fields inside their range, never parts | C12, C15 | Implicit today; one sentence each |
| 13 | **Items:** policy as `#![confidence(…)]`; `env` only as setup values, a lint for `env` in child setups; unit types on generic consts; instantiations checked = `#[check]` + defaults + placements; one trait spelling; the default setup is plain; outside-spec reporting | C21, C22, C26–C29 | Consistency with §0's "Rust's meaning" and with §1 |
| 14 | **Smaller items:** one swept-axis home (`for`, no `Sweep` in setups); `.db()` only; canonical clause order `[on] [in] [with] [for]`; `±` in setups is a range; model blocks; lint for one-sided facts | C30, C31, C32 | Cheap, and each removes a second spelling |
| 15 | **Example numbers:** return `r_g` to `gnd` (or bias `vin` at mid); label or fix `holdup`; move `wake_noise` or say how events meet modes | N1–N3 | The example is the first test case; N1 fails every amp spec |

**Suggested order.** Fixes 1–5 change what a setup and a spec mean; settle them before the parser grows setups. Fixes 6–9 are needed when multi-setup specs and hierarchy land. 10–14 can be written as the reference is. Fix 15 is an edit to the example, today. None of this touches the MVP slice (§3 item 5) except fix 5 (the CE amp's `input` and `output` get written out) and fix 4's sign convention, which the CE amp's specs don't exercise.
