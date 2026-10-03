# Plan: setups, `env`, `const` and contracts, resolved (the MVP subset)

> 2026-10-01 · The phase after `syntax_v5_plan.md`. Resolve reads what's inside `env`, `const`, `setup` and `contract`, in the MVP subset of `research/contract_v4_review_implementation.md` §4.4 (cited "[IR]"). Flatten turns the root's default setup into knobs. Then `circuits/ce_amp.spl` gives model.md §5's 8 knobs and 3 checkable specs.
>
> **Status:** approved 2026-10-01. Step 1 committed (2026-10-02); step 2 built and awaiting review (its outcome is under step 2); steps 3–6 are to do.
>
> **After it:** the exporter (a flat design and its setup become an ngspice deck: `netlist_writer.md`, `spicy_netlist::writer`), then the engine.
>
> **References,** at the commit read: rustc `012c0bd`, rust-analyzer `1ad44dc` (hir-def), Spade `177e5c4`, atopile `619eda7`, ngspice `56a152c` (ngspice-42 installed, for runs), Xyce `6243c62`, Gnucap `874f2d5`, the Modelica spec (master). **[V]** means read in the source or run; **[R]** means recalled.

---

## 0. `ce_amp.spl` all the way through

### 0.1 What this phase reads

These are the parts of `circuits/ce_amp.spl` this phase reads. The circuit is resolved and flattened as today.

```rust
env ambient: Temperature in -10°C..=60°C;

pub block CeAmp { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }

setup Operating for CeAmp {
    vcc: Supply { v: 12V ± 5% },
    input: Signal { v: 0V },
    output: Load {},
    temp: ambient,
}

contract CeAmp {
    setup = Operating;
    let h = ac(output.v / input.v);
    spec bias: dc(output.v) within 4.5V..=6.5V;
    spec gain: h.at(1kHz).mag() within 4.6 ± 5%;
    spec bass: h.f_low(-3dB) <= 30Hz;
}
```

### 0.2 Resolve

**`env ambient`.**
- Its type, `Temperature`, says what the range holds: an absolute temperature, in K.
- The range is typed by the same code as a part's `beta: 100..=300` (`resolve/value.rs`), so its nominal is the midpoint (E16, §2.4).

```rust
Env {
    name: "ambient",
    value: Ok(Value {
        nominal: Quantity::kelvin_point(298.15),               // 25 °C, the midpoint
        spread: Spread::Range { lo: 263.15, hi: 333.15 },      // -10 °C ..= 60 °C
    }),
}
```

**`setup Operating`.**
- It has one entry per port of `CeAmp`, by `PortId`, in the header's order: `vcc`, `gnd`, `input`, `output`.
- Each port with a role gets a *shape*: a source or a load.
- A shape's fields come from a schema, the same kind a part has (`prelude.rs`).

```rust
Setup {
    name: "Operating",
    block: BlockId(0),                                                    // CeAmp
    ports: vec![
        Some(PortSetup { shape: Shape::Supply, fields: vec![Given(12 V ± 5%), Unset] }),    // vcc: v, z
        None,                                                                            // gnd: a Ground takes nothing
        Some(PortSetup { shape: Shape::Signal, fields: vec![Given(0 V), Unset] }),          // input: v, z
        Some(PortSetup { shape: Shape::Load,   fields: vec![Unset, Unset, Unset] }),        // output: r, c, i
    ],
    temp: Ok(Temp::Env(EnvId(0))),                                        // ambient
    tainted: None,
}
```

- **`Unset` is ideal:** a 0 Ω source, or no load (v5 rule 1.3.1). It gets no knob and no element.
- **Every port with a role is written, and so is `temp`** (rule 1, §2.2). `vcc`'s and `input`'s `v` are written, because a voltage has no ideal default.
- **Each shape fits its port's role:** `Supply` on a `Power<In>`, `Signal` on an `Analog<In>`, `Load` on an `Analog<Out>`.

**The contract.**
- `setup = Operating;` is looked up among `CeAmp`'s setups.
- Each `let` is a *measure*.
- Each spec has a measure that gives one number, a limit, and a bound in the measure's unit (E12).

```rust
Contract {
    default_setup: Ok(SetupId(0)),
    measures: vec![Measure {
        name: "h",
        // `output` is net 3. `input` is port 2, an input, so its source is the one excited.
        expr: Ok(MExpr::Ac { out: NetId(3), input: PortId(2) }),          // a response, V/V
    }],
    specs: vec![
        Spec { name: "bias", public: false,
               measure: Ok(MExpr::Dc(Box::new(MExpr::Voltage(NetId(3))))),           // V
               limit: Ok(Limit { op: LimitOp::Within, bound: 5.5 V (4.5 V..=6.5 V) }) },
        Spec { name: "gain", public: false,
               measure: Ok(mag(at(1000.0, MExpr::Measure(MeasureId(0))))),            // a ratio
               limit: Ok(Limit { op: LimitOp::Within, bound: 4.6 ± 5% }) },
        Spec { name: "bass", public: false,
               measure: Ok(f_low(-3.0, MExpr::Measure(MeasureId(0)))),               // Hz
               limit: Ok(Limit { op: LimitOp::AtMost, bound: 30 Hz }) },
    ],
    tainted: None,
}
```

`mag(…)`, `at(…)` and `f_low(…)` stand for `MExpr::Method { method: Method::Mag, of: … }` and so on.

- **All three are interface specs:** every probe is on a port (`output`, `input`). None is `pub`, so the `pub` rule (§2.6) has nothing to check.
- **The units check out:**
  - `dc(output.v)` is in V, so `4.5V..=6.5V` must be too.
  - `h` is V/V, so `.mag()` is a plain number, and so is `4.6 ± 5%` (4.37 … 4.83).
  - `.f_low(-3dB)` is in Hz, so `30Hz` must be too.

The resolve dump (`testing::dump_design`) gains:

```
env ambient = 298.15 K (25 °C) (263.15 K..=333.15 K)
setup Operating for CeAmp
  vcc: Supply  v = 12 V ± 5%
  input: Signal  v = 0 V
  output: Load
  temp = ambient
contract CeAmp
  setup = Operating
  let h = ac(output.v / input.v)
  spec bias: dc(output.v) within 5.5 V (4.5 V..=6.5 V)
  spec gain: h.at(1000 Hz).mag() within 4.6 ± 5%
  spec bass: h.f_low(-3 dB) <= 30 Hz
```

### 0.3 Flatten

Flatten does what it does today: 6 devices, 6 nets, 6 part knobs. Then one new pass reads the root's default setup.

```rust
FlatDesign {
    knobs: vec![
        /* k0..k5: c_in.value, q1.beta, r1.value, r2.value, rc.value, re.value, as today */
        Knob { source: KnobSource::SetupField { port: PortId(0), field: 0 },  // k6  vcc.v
               value: 12 V ± 5%, kind: KnobKind::Range },
        Knob { source: KnobSource::Env(EnvId(0)),                             // k7  ambient
               value: 298.15 K (263.15 K..=333.15 K), kind: KnobKind::Range },
    ],
    // The root's nets, by the root's own NetId (vcc, gnd, input, output, base, emitter):
    // where a probe (`output.v`) or a source lands.
    root_nets: vec![N3, N0, N1, N2, N4, N5],
    setup: Some(FlatSetup {
        setup: SetupId(0),
        ports: vec![
            Some(FlatPortSetup { shape: Shape::Supply, fields: vec![Knob(k6), Unset] }),
            None,
            Some(FlatPortSetup { shape: Shape::Signal, fields: vec![Exact(0 V), Unset] }),
            Some(FlatPortSetup { shape: Shape::Load,   fields: vec![Unset, Unset, Unset] }),
        ],
        temp: FlatField::Knob(k7),
        tainted: None,
    }),
    /* instances, nets, devices, grounds, tainted: as today */
}
```

- **A setup field with a spread becomes a Range knob** (`vcc.v`), as a part field with a spread becomes a Statistical one (E16). A point stays `Exact` (`input.v` is 0 V).
- **`temp: ambient` becomes the env's knob,** `ambient`.
- **Setup knobs come after the part knobs,** so adding a contract renumbers no part knob (§2.3).
- **The specs aren't copied.** They stay in the `Design`. Their probes are the root's `NetId`s, which `root_nets` maps to flat nets (`output` → N2).

The flat dump gains:

```
  knob vcc.v Range 12 V ± 5%
  knob ambient Range 298.15 K (25 °C) (263.15 K..=333.15 K)
  setup Operating
    vcc: Supply  v=knob vcc.v
    input: Signal  v=0 V
    output: Load
    temp=knob ambient
```

### 0.4 The result: 8 knobs and 3 checkable specs

model.md §5's table, in id order:

| Id | Knob | Kind | Nominal | Spread | From |
|---|---|---|---|---|---|
| k0 | `c_in.value` | statistical | 1 µF | ± 20% | part |
| k1 | `q1.beta` | statistical | 200 | 100 … 300 | part |
| k2 | `r1.value` | statistical | 47 kΩ | ± 1% | part |
| k3 | `r2.value` | statistical | 10 kΩ | ± 1% | part |
| k4 | `rc.value` | statistical | 4.7 kΩ | ± 1% | part |
| k5 | `re.value` | statistical | 1 kΩ | ± 1% | part |
| k6 | `vcc.v` | range | 12 V | ± 5% (11.4 … 12.6 V) | setup `Operating` |
| k7 | `ambient` | range | 298.15 K (25 °C) | 263.15 … 333.15 K | env, through `temp:` |

**The 3 specs are checkable.** `Flat::checkable_specs` returns a spec when the root, its contract and its setup are sound and the spec itself resolved. They give the engine its 5 sides (engine_types §2.3):

| Spec | Measure | Unit | Sides | Engine needs |
|---|---|---|---|---|
| `bias` | `dc(output.v)` | V | max ≤ 6.5 V, min ≥ 4.5 V | op; `v(output)` |
| `gain` | `h.at(1kHz).mag()` | 1 | max ≤ 4.83, min ≥ 4.37 | AC at 1 kHz; `AC 1` on `input`'s source |
| `bass` | `h.f_low(-3dB)` | Hz | max ≤ 30 Hz | AC sweep |

---

## 1. The steps

**Each step ends green,** as the v5 plan's did:
- tests pass, and you've reviewed every snapshot change;
- `cargo clippy --workspace --all-targets` and `cargo fmt` are clean;
- `fuzz_spicy_lang_parser` runs clean for 10 minutes. It checks the resolve and flatten invariants too (`testing::check_parse_invariants`). The step's new case files go in its seed corpus;
- the benchmark: the suite (`crates/spicy_bench`, added after step 2) against a baseline of the step's base commit: the typical design with and without env, setup and contract (R1, R2, F1, F2), no slowdown above about 3% where the step adds nothing, and every scaling pair still linear;
- `/stage-review` has run on the stage the step touched, and you've reviewed the result.

**Where the code goes** (the crate rules):
- **`spicy_model`** gets every new type:
  - the `Design`'s new items;
  - the shapes and value types (`prelude.rs`);
  - what measures mean (a new `measure.rs`);
  - the flat setup (`flat.rs`, `flatten/setup.rs`).

  It still has no syntax, no spans in the data (E22) and no simulator.
- **`spicy_lang::resolve`** turns the syntax tree into them, in three new files, as [IR] §4.1 planned: `resolve/env.rs`, `resolve/setup.rs`, `resolve/contract.rs`.
- **The engine reads `Flat`**, the flat design with its `Design`. It never sees the syntax tree.

**16 new error kinds and one warning** (`UncheckedSpecs`, §3.1), each with a case file. Every new kind goes in `ResolveErrorKind::ALL_NAMES`, so the coverage test requires its case. The fix test applies every new fix.

### Step 1: `env` and `const`

- **Adds:**
  - `Design.envs` and `Design.consts`, indexed by `EnvId` and `ConstId` in source order. `DesignSourceMap.envs` and `.consts` hold each one's name span.
  - `prelude::ValueType`: the types an `env` or `const` can declare, each a `FieldType` (§2.9 d):
    - `Temperature` (a point, in K) and `Duration` (s);
    - language.md §5.1's unit names: `Volt`, `Amp`, `Ohm`, `Farad`, `Henry`, `Hertz`, `Watt`;
    - `f64` (a plain number);
    - `Confidence` and v5's other types are "not supported yet".
  - Pass 1 declares envs and consts in the file's value scope. A new pass types each value against its declared type, with `Resolver::value`.
  - **The temperature rules of E13**, which waited for this step (model.md §8):
    - a temperature's tolerance is a difference in K (`25°C ± 5K`);
    - a temperature point is written in °C.
  - **A `const` can be used where a number is expected:** `term()` looks a name up among the consts (`value: R_TOP`, and from step 2 a setup's `temp: T_LAB`). A name that isn't a const is still `NotAValue`, as today.
  - The ignored `elaborate_speed` test: ce_amp's items repeated N times, renamed apart. Its number today is the baseline. (Replaced after step 2 by the benchmark suite, `crates/spicy_bench`.)
- **New errors:**

  | Kind | Code | Example | Fix | Case file |
  |---|---|---|---|---|
  | `EnvNeedsRange` | `E-value` | `env t: Temperature in 25°C;` | none (help: a fixed value is a `const`) | `resolve/err/env.spl` |
  | `TemperatureSpread` | `E-unit` | `25°C ± 5%`; `25°C ± T_ROOM` (a named temperature, a point) | none (`± 5°C` is accepted as ±5 K, decided 2026-10-02) | `resolve/err/temperatures.spl` |
  | `TemperaturePoint` | `E-unit` | `temp: 25`; `temp: 300K` | `25°C`; `26.85°C` | `resolve/err/temperatures.spl` |

  **Reused:**
  - `Duplicate`, with `NameKind::Env` and `NameKind::Const`;
  - `UnknownName`, with a new `Namespace::Type` ("unknown type `Temperatur`", fix `Temperature`);
  - `Unsupported` (`Confidence`);
  - `SpreadNotAllowed`, for a `const` with a spread. It comes from `FieldType::exact`, with no new code;
  - the unit errors.
- **Tests:**
  - `resolve/ok/envs.spl`: `ambient`, `life`, and a const used in a part's value;
  - the two err files.
- **Done when:**
  - ce_amp's resolve dump has `env ambient = 298.15 K (25 °C) (263.15 K..=333.15 K)`;
  - the "`K` is kelvin; kilo is `k`" note no longer appears in a temperature position (§4, item 7).
- **Outcome (2026-10-01, awaiting review):** both hold. Added on the way, each with a test:
  - a range types both ends before failing, so `25..=30` in a temperature reports both;
  - a name defined twice breaks the first (`Err`), as a block defined twice does, and a part reading a broken const taints its block (rustc's `set_tainted_by_errors` on meeting an error type), with nothing reported again;
  - `EnvNeedsRange` asks whether the value varies, so `12V ± 0%` and `5V..=5V` get it too;
  - a temperature spread isn't scaled (`2 * (20°C..=30°C)` is `NotArithmetic`); a negative spread (`± -5°C`) gets no fix;
  - a fix in °C has the fewest decimals that read back as the same K (`units::to_celsius`, shared with `Quantity`'s display);
  - plain `K` reads "a temperature difference" in unit errors;
  - decided 2026-10-02 after the review: after `±`, a `°C` literal is a difference (`25°C ± 5°C` is ±5 K); a plain-number const after `±` is `Unsupported`, since a named `1%` can't show it's relative (model.md E13).
  - second stage review (2026-10-02): `Scope::declare` returns the first it redefines (`Redefined`), and each first env or const holds its own "defined again" slot (a list search was quadratic: 1.6 s for 40 000 duplicated names, now 27 ms); the taint for a broken const's reader is set where the field's placeholder is stored (no span bookkeeping); a tolerance types both sides and an env written as one number gets `EnvNeedsRange` whatever else is wrong, so no fix uncovers another error; each resolve error is checked to be reported once (E23).
  - `EnvId` arrives here, not in step 2: the redefinition rule is its first reader.

### Step 2: setups

- **Adds:**
  - `prelude::Shape` (`Supply { v, z }`, `Signal { v, z }`, `Load { r, c, i }`), a schema like `PartKind`'s:
    - `fields()` gives each field's `FieldSchema`: `v` is required, the rest are optional;
    - `for_role(SignalType)` gives the shape a port's role takes.
  - `design::{SetupId, Setup, PortSetup, Temp}`. `Design.setups` is in source order, and each setup names its block. `DesignSourceMap.setups: Vec<SetupSpans>` holds the name span, each port's entry and `temp`'s.
  - `resolve/setup.rs` resolves each setup against its block's ports, as passes in one function:
    1. **Keys:** each entry's key becomes a port (`vcc`, `vin.v`) or `temp`.
    2. **Shapes:** each port's shape, written (`vcc: Supply { … }`) or implied by its role (`vout.i: …` alone).
    3. **Roles:** the shape fits the port's role.
    4. **Fields:** the shape's fields and the paths into it, each once, typed.
    5. **`temp`:** an env, a const, a point or a range, typed as a temperature.
    6. **Complete:** every port with a role is written, and so is `temp` (rule 1, §2.2).
  - **One field binder for parts and shapes.** The field half of `BodyResolver::bind` becomes one helper that both use: it looks up the slot, catches a field given twice and an unknown field (with its rename fix), and types the value. Every existing resolve snapshot comes out byte-identical, which proves nothing changed for parts.
  - **A second definition taints the first** (syntax_v5_plan §3, settled in §3 below). A second circuit taints its block, as a second block already does. A second setup of a name taints the first setup.
  - **`Block.has_circuit`** (§3.1): set by resolve, so a block with no circuit differs from one with an empty `circuit A {}`. Nothing reads it until steps 4 and 5.
- **New errors:**

  | Kind | Code | Example | Fix | Case file |
  |---|---|---|---|---|
  | `WrongRole` | `E-role` | `vcc: Load {}` on a `Power<In>`; a key on `gnd: Ground`; `output.v: …` on an `Analog<Out>` (language.md §8.2's example) | for the first, `Supply`, the role's shape; none for the others | `resolve/err/setup_roles.spl` |
  | `IncompleteSetup` | `E-setup` | `Operating` without `input`, without `vcc`'s `v`, or without `temp`: one error lists everything missing | none | `resolve/err/setup_incomplete.spl` |
  | `OpenRange` | `E-value` | `v: ..=5V`: a voltage needs both ends | none | `resolve/err/open_ranges.spl` |

  **Reused:**
  - `UnknownName`, with a new `Namespace::Port` for keys (fix `vcc` for `vc`), and `Namespace::Kind` for a shape name (`Suply`);
  - `WrongNamespace`: a key that names a net (`base: …`), or a part kind used as a shape (`Resistor {}`);
  - `UnknownField` (`Supply { vv: 1V }`);
  - `Duplicate`, with both spans: `vcc: Supply { v: 1V }, vcc.v: 2V`;
  - `Unsupported`:
    - `window`;
    - `Step` or `Sweep` as a value;
    - `a..`;
    - a key on a `Pin` port;
    - a key deeper than `port.field`;
    - attributes;
    - an env name anywhere but `temp:`;
  - the unit and value errors.
- **Tests:**
  - `resolve/ok/setups.spl`: whole shapes, paths merged into them, an implied shape, `..=0.5Ω`, and each form of `temp` (an env, a const, a point, a range);
  - the err files;
  - `setups.spl`, `duplicates.spl`, `unknown_block.spl` and `circuits.spl` get new errors in their snapshots, and you review each one;
  - the resolve shuffle test (`statement_order_does_not_matter`) also reorders setup entries.
- **Done when:** ce_amp's setup dumps as in §0.2.
- **Outcome (2026-10-02, awaiting review):** done when holds. Built as planned, plus, each with a test or case file:
  - `NotAShape` (`vcc: 12V`), a fourth error kind the table above didn't list; a port whose written value isn't a shape at all explains its own missing fields;
  - one field binder shared by parts and shapes (`resolve/fields.rs`): a slot is given once, the shorthand isn't a value, an unknown field gets the rename when one unknown name meets one missing slot (and that slot isn't also reported missing), a required field left out holds the proof;
  - a misspelled setup key is renamed to the one slot left out that it could be (a port with a role, or `temp`), and its entry is then checked as that slot, so the fix uncovers nothing; a broken entry is the setup's one error (nothing is listed as left out);
  - a second shape for a port is checked for its own mistakes, then dropped; a second circuit taints its block (`Redefined`, as a second block);
  - the kind namespace is classified by one `kind_of` (a file's block shadows a prelude part kind, model.md E5);
  - speed (instructions, parse + resolve ×5): files without setups −3.8%, deep hierarchy −5.1% (one hash lookup per `declare`, the duplicate report out of the hot path), files with setups +7.5% (≈ 10k instructions and 11 allocations per setup; 8 of the 11 are the model's own `Vec`s).


### Step 3: the contract's default setup and measures

- **Adds:**
  - `measure.rs` (new, in `spicy_model`): `MExpr`, `Method`, `Reference`, `ArithOp`, `MeasureType`, and what each method gives (§2.5). `prelude::FieldType::level()`, a dB level, types `.f_low(-3dB)`'s argument.
  - `design::{MeasureId, Measure}`. `Contract` gets `default_setup`, `measures`, `specs` (empty until step 4) and `tainted`. `DesignSourceMap.contracts: Vec<Option<ContractSpans>>`.
  - `resolve/contract.rs` resolves each contract as passes:
    1. **The default setup:** `setup = S;`, looked up among its block's setups.
    2. **Names:** every `let`, declared in the block's value namespace with the ports, nets and instances (E5). So `let base = …` next to `net base;` is a `Duplicate`.
    3. **Measures:** each `let` becomes an `MExpr` with its `MeasureType`. They're resolved in dependency order (`h` before `h.at(…)`). The lets are taken in name order, so a cycle is reported once, and the same way after a shuffle.
  - The contract's value scope is read from the `Design` that the circuit pass returned: `Block.nets` and `.instances` by name, with spans from the source map. So `BodyResolver` keeps no state for it.
  - **`Contract.tainted`** is set when something written may be missing or unknown: the contract's body didn't parse, or it's the first of two contracts for the block (§3). A broken spec only makes that spec uncheckable, through its own `Result`s.
- **New errors:**

  | Kind | Code | Example | Fix | Case file |
  |---|---|---|---|---|
  | `NoDefaultSetup` | `E-contract` | a contract with no `setup = S;`. Not reported when the contract's body didn't parse, since that line may be the broken one | none | `resolve/err/default_setup.spl` |
  | `NotAMeasure` | `E-measure` | `let r = Resistor { … };` or `let k = 5V;` in a contract | none | `resolve/err/measures.spl` |
  | `NeedsAnalysis` | `E-measure` | `let x = output.v;` (§2.9 a) | `dc(output.v)` | `resolve/err/measures.spl` |
  | `WrongMeasureType` | `E-measure` | `h.mag()` (a response has no magnitude; `h.at(f)` has one); `dc(output.v).f_low(-3dB)`; `dc(output.v).db()` (volts aren't a ratio) | none | `resolve/err/measure_types.spl` |
  | `BadArguments` | `E-measure` | `h.at()`, `h.at(1kHz, 2kHz)`, `h.f_high(-3dB, ref: top)`, `dc(a.v, b.v)` | none | `resolve/err/measure_types.spl` |
  | `NoExcitation` | `E-measure` | `ac(output.v)`; `ac(input.v / output.v)`, with an output in the denominator | none (help: `ac(out.v / in.v)`, where `in` is an input) | `resolve/err/excitation.spl` |
  | `MeasureCycle` | `E-measure` | `let a = b.at(1kHz); let b = a;` | none | `resolve/err/measure_cycle.spl` |

  **Reused:**
  - `Duplicate`, with `NameKind::DefaultSetup` and `NameKind::Measure`;
  - `UnknownName`:
    - a new `Namespace::Setup` (fix `Operating` for `Operting`);
    - a new `Namespace::Function` (fix `mag` for `mg`; a swap like `mga` is two edits, so it gets none);
    - `Namespace::Value` (`outpt.v`);
  - `WrongNamespace`: `dc(r1.v)`, where `r1` is an instance;
  - `MixedUnits`, `NotArithmetic`, and `UnitMismatch` (`h.at(1kV)`);
  - `Unsupported`:
    - `tran`, `noise`, and v5's other methods (`.min`, `.settle`, …);
    - `m[s]`;
    - the probes `.i` and `.z(f)`;
    - part pins (`q1.c`).
- **Tests:**
  - `resolve/ok/measures.spl`: lets using other lets, out of order; `dc` of a difference; `.db()`; `.f_high(…, ref: dc)`;
  - the err files;
  - ce_amp's contract dump.
- **Done when:** ce_amp's contract has `setup = Operating` and `let h = ac(output.v / input.v)`.
- This is the largest step (the measure table and its typing). If it reads too big in review, it splits in two: `dc` first, then `ac` with its methods.
- **Outcome (2026-10-03, awaiting review):** done when holds. Built as planned, with these changes, each with a test or case file:
  - `MeasureType::Probe` (not `Signal`, which the port types already use) for `output.v` and `a.v - b.v`, as language.md §8.4 names them; the messages say "probe".
  - The second-definition rule also covers a contract's lines: the first of two `let`s of a name is `Err`, and so is anything that reads it; the first of two `setup = …;` is `Err`. The first of two contracts is tainted in pass 4, with blocks and setups.
  - Fixes are offered only where they are the single right answer: a misspelled name where a measure goes gets a note, not a fix (its type may not fit), and suggests only measures and consts; a method is suggested only if it applies to its receiver; `output.v / input.v` gets no `dc(…)` fix, because `ac(…)` takes it too; a positive level gets the fix `-3dB`.
  - `dc(5V)` is a constant, not a measure; `dc(output.v) / 0` is `DivisionByZero`, as in a value; a measure or a net as a method's argument is `WrongNamespace` ("a constant"); a port whose type is wrong doesn't add `NoExcitation`.
  - `MeasureCycle` lists the cycle (`` `a` reads `b`, and `b` reads `a` ``).
  - Stack: the recursion of one measure is cut down to `binary`, so a measure as deep as the parser allows fits a 2 MB stack in a debug build (it overflowed above 818 terms). A chain of lets each reading the next still overflows above about 450 lets (debug) or 1,100 (release): parked, see §3.
  - `spicy_index::fx::FxHasher::finish` rotates the hash as `rustc-hash` 2 does: names that differed only in their last bytes fell into 32 buckets (400,000 nets took 1.8 s; now 20 ms).
  - Speed (instructions, against `a86d532`): `resolve_file.typical` +13.1% (about 5.4k instructions per contract with one measure, against about 24k per circuit); every other benchmark within ±0.8%, except `drop_elaborated` +4.4% (the measures' heap blocks). The new pair `measure_chain` (125 and 500 measures) grows 3.6×.

### Step 4: specs and the `pub` rule

- **Adds:**
  - `design::{Spec, Limit, LimitOp}` and `Contract.specs`;
  - `ContractSpans.specs: Vec<SpecSpans>`: the spans of the name, the measure and the bound;
  - one more pass in `contract.rs`, **specs**:
    - each name once per contract;
    - the measure, which must give one number;
    - the limit (§2.7);
    - the bound, typed in the measure's unit (E12);
    - then the `pub` rule (§2.6);
    - and, for a contract with specs whose block has no circuit, the `UncheckedSpecs` warning (§3.1).
- **New errors:**

  | Kind | Code | Example | Fix | Case file |
  |---|---|---|---|---|
  | `StrictLimit` | `E-limit` | `dc(x) < 6.5V` | `<=` | `resolve/err/limits.spl` |
  | `WithinNeedsRange` | `E-limit` | `within 5V`; `within ..=6.5V` | none; `<= 6.5V` | `resolve/err/limits.spl` |
  | `PublishedInternal` | `E-contract` | `pub spec b: dc(base.v) within …`, directly or through a `let` | remove `pub` | `resolve/err/published_internal.spl` |
  | `UncheckedSpecs` (a warning) | `W-unchecked` | a contract with specs for a block that has no circuit (§3.1) | none | `resolve/err/unchecked_specs.spl` |

  **Reused:**
  - `Duplicate`, with `NameKind::Spec`;
  - `WrongMeasureType`: `spec x: h within …` measures a response, not a number;
  - `UnitMismatch`: `<= 30V` on a frequency.
- **Tests:**
  - `resolve/ok/contract.spl`;
  - `resolve/ok/internal.spl`: v5's `base_bias` (`dc(base.v) within 1.9V..=2.3V`), an internal spec that isn't `pub`, which is allowed (syntax_v5_plan decision 3);
  - the err files.
- **Done when:** ce_amp's 3 specs dump as in §0.2.

### Step 5: flatten the default setup

- **Adds:**
  - `flat::{FlatSetup, FlatPortSetup}`;
  - three new `KnobSource` variants: `SetupField { port, field }`, `SetupTemp` and `Env(EnvId)`;
  - `FlatDesign.root_nets` and `FlatDesign.setup`;
  - `Flat::{contract, root_net, checkable_specs}`, and `knob_path` for the new sources (`vcc.v`, `temp`, `ambient`).
  - `flatten/setup.rs`: one pass in `RootFlattener::flatten`, after the devices and knobs. It reads the root's default setup and makes each field `Exact` or a new Range knob, appended after the part knobs (§2.3).
  - Pass 5 (the nets) also returns `root_nets`: its `net_of`, for the root's entries.
  - A root with no contract, or whose default setup didn't resolve, gets `setup: None` and no setup knobs.
  - **Roots are the blocks nothing places that have a circuit** (§3.1). A block with no circuit isn't flattened, so it gets no "isolated nets" and none of its specs is checkable.
- **New errors:** none. Resolve already reported everything a setup or contract can get wrong.
- **Tests:**
  - `flatten/ok/setup_knobs.spl`:
    - `temp` as a range (knob `temp`) and as a point;
    - `z` and load knobs;
    - two roots that read one env, each with its own `ambient` knob;
  - ce_amp: the 8 knobs and 3 checkable specs of §0.4, asserted in `elaborate::tests::ce_amp`;
  - new invariants in `check_flatten_invariants`:
    - every knob's source is the field that refers to it, for the new sources too;
    - a `FlatSetup` has one entry per root port;
    - `root_nets` has one entry per root net;
    - every probe of a checkable spec maps to a net;
  - the shuffle test (`order_does_not_matter`) already shuffles contract statements, and the flat dump now includes the setup.
- **Done when:** roadmap M1d's "done when" holds: `ce_amp.spl` elaborates to exactly the 8 knobs and 3 specs of model.md §5.

### Step 6: the design notes

- **`model.md`:**
  - E16: setup and env knobs, and their order;
  - E24 and §3.8: what resolve builds;
  - §4: the types;
  - §5: the table of §0.4, in id order, with `ambient`'s nominal;
  - §8: the implementation notes, with the new errors.
- **`language.md` §8:** the "MVP" notes in §8.1–§8.4 say what's resolved.
- **`roadmap.md` M1d:** the status; drop "the `assume` knobs".
- **`syntax_v5_plan.md` §3:** mark each open point settled or parked (§3 here).
- **`engine_types.md` and `engine_plan.md`:**
  - `KnobTable` → `FlatDesign.knobs`;
  - `temp` → `ambient`;
  - the knob order (§4).

---

## 2. Design questions

### 2.1 Where setups, envs and the contract live in `Design`

**References**
- **rustc:** an `impl Foo` is an item of the crate that names its type. The list of a type's impls is a map built over all items:
  - `inherent_impls(DefId) -> &[DefId]` "maps a DefId of a type to a list of its inherent impls" (`rustc_middle/src/queries.rs:1149-1156`) [V];
  - it's filled by walking `tcx.hir_free_items()` and pushing each impl under its self type (`rustc_hir_analysis/src/coherence/inherent_impls.rs:21-33, 74-75`) [V];
  - into an `FxIndexMap<LocalDefId, Vec<DefId>>` (`rustc_middle/src/ty/mod.rs:2401-2404`) [V].
- **rust-analyzer:**
  - each file's `ItemTree` is a flat list of items (`hir-def/src/item_tree.rs:209-216`; `SmallModItem::Impl` at :296) [V];
  - an impl's self type is in its signature (`signatures.rs:456-462`), not nested in the type [V];
  - the per-type index of impls is built later, per crate, in hir-ty [R].
- **Spade:**
  - `ItemList { executables, types, modules, traits, impls }` is a set of flat maps (`spade-hir/src/lib.rs:1187-1199`) [V];
  - impls are indexed by their target (`impl_tab.rs:11-16`) [V];
  - nothing is nested in a unit (`lib.rs:767-774`) [V].
- **Modelica:** the simulation setup, `experiment(StartTime, StopTime, Tolerance)`, is an annotation *inside* the model class (§18.7). [V]
- **VHDL and SystemVerilog:** a `configuration C of E` is a design unit of its own that names its entity (IEEE 1076 §3.4). `bind target …` names its target from outside (IEEE 1800 §23.11). [R]
- **SPICE:** one deck is one setup. `.temp` and the sources are lines of the deck (ngspice reads `.temp` at `src/frontend/inp.c:1243-1251`). [V]

**Options**

A. **File-level tables, one per kind of item** ([IR] §3.1). Items are numbered across the file, as blocks are; what's inside an item is numbered per item (E21).
```rust
pub struct Design {
    pub blocks: Vec<Block>,
    pub contracts: Vec<Option<Contract>>,   // by BlockId, as today
    pub setups: Vec<Setup>,                 // SetupId; each names its block
    pub envs: Vec<Env>,                     // EnvId
    pub consts: Vec<Const>,                 // ConstId
}
// flatten, step 5:
let contract = design.contracts[root.index()].as_ref()?;
let setup = &design.setups[contract.default_setup.ok()?.index()];
// the editor's I/O page, later: a block's setups
let setups = design.setups.iter().filter(|s| s.block == b);
```

B. **Nested in the block** (Modelica's way).
```rust
pub struct Block { /* ports, nets, instances, merges, tainted */ pub setups: Vec<Setup>, pub contract: Option<Contract> }
let block = design.block(root);
let setup = &block.setups[block.contract.as_ref()?.default_setup.ok()?.index()];
```

C. **A side table, one entry per block.**
```rust
pub struct Design { pub blocks: Vec<Block>, pub checks: Vec<Checks>, pub envs: Vec<Env>, pub consts: Vec<Const> }
pub struct Checks { pub setups: Vec<Setup>, pub contract: Option<Contract> }    // by BlockId
let checks = &design.checks[root.index()];
let setup = &checks.setups[checks.contract.as_ref()?.default_setup.ok()?.index()];
```

**Pros and cons**
- **A:**
  - It mirrors the text: a setup is its own item and names its block, like `impl Foo` or a VHDL configuration.
  - Flatten still depends on blocks only, which is `design.rs`'s reason for keeping contracts apart.
  - An edit to a setup leaves every `Block` equal (E22, E26).
  - Envs and consts are file-level anyway.
  - "A block's setups" is a filter, and nothing reads it yet.
- **B:**
  - One place for everything about a block, with ids numbered per block.
  - But a setup edit changes the `Block` that flatten reads.
  - `Block` gets a third concern next to its interface and its circuit.
  - Envs still need a file-level table.
- **C:** ids per block without touching `Block`, but it adds a two-field wrapper, and `contracts` has to be renamed where it's used.

**Cost**
- A: three `Vec`s in `Design` and three in `DesignSourceMap`. Flatten is unchanged until step 5.
- B: `Block`, and every place that builds one.
- C: a new struct and the rename.

**Recommendation: A.**
- Setup names are still per block. Resolve keeps one setup scope per block (`Vec<Scope<SetupId>>`, which today's `check_setups` already builds), so `Operating` exists once per block.
- Each contract, setup and spec holds a `Result<…, Reported>` wherever it can be broken (E7).
- `Setup` and `Contract` each get a `tainted`, as `Block` has.

### 2.2 The effective setup

A spec's effective setup is the one it runs in. In v5 that's the default setup, unless the spec says `on S` ([IR] §3.4). `on`, `..Base` and modes aren't in the MVP, so every spec runs in the default setup.

**References**
- **Modelica:**
  - a modification environment is built by merging, "where outer modifications override inner modifications" (§7.2.2–7.2.3) [V];
  - an `experiment` annotation is inherited, and "the derived class may override individual inherited options" (§18.7) [V].

  Together, that's v5's `..Base` and its rule 1.3.2.
- **SPICE:** one deck is one setup, and a later `.temp` or `.options` line overrides an earlier one (engine_plan §5.3; ngspice `inp.c:1243-1251`). [V]
- **Rust:** the struct update syntax `S { a: 1, ..base }` takes the unwritten fields from `base` (the Reference's "functional update syntax"). v5 copies it for `..Base`. [R]

**Options**

A. **Nothing per spec.** The contract names its default setup, and every spec runs in it.
```rust
pub struct Contract { pub default_setup: Result<SetupId, Reported>, pub measures: Vec<Measure>, pub specs: Vec<Spec>, pub tainted: Option<Reported> }
pub struct Spec { pub name: String, pub public: bool, pub measure: Result<MExpr, Reported>, pub limit: Result<Limit, Reported> }
// later, with `on S`: Spec gains `on: Option<SetupId>`, where `None` means the default.
```

B. **Each spec names its setup now**, always the default: `Spec { setup: SetupId, … }`.

C. **[IR]'s slots:** `Spec { slots: Vec<Slot { setup, pins, mode }> }`, and one `FlatSetup` per (setup, mode, pins).

**Pros and cons**
- A stores nothing that can't vary yet.
- B stores the same id in every spec.
- C builds modes and pins that nothing reads. Its shape is right for later, but every field would be constant now.

**Cost:** A: none. B: one field. C: three structs and flattening them.

**Recommendation: A.** The one setup still has rules in the MVP, all in `resolve/setup.rs`:
1. **Merging within a setup.** A whole shape and the paths into it (`vout: Load { c: … }, vout.i: …`) are one port's fields. Each field is written once: a second is a `Duplicate`, with both spans (E9).
2. **The implied shape.** A path on a port with no shape entry takes the shape of the port's role ([IR] §3.4). So `vout.i: 5mA..=50mA` alone makes a `Load`.
3. **Rule 1 is checked on every setup, not only the default.** Every port with a role (`Power`, `Analog`) must be written, and so must `temp`.
   - No MVP setup has a base, so each must be complete to be simulated at all.
   - Checked on its own, a setup resolves without knowing which contract names it.
   - When `..Base` arrives, a derived setup gets its missing fields from its base.
   - `Ground` takes nothing.
   - A `Pin` port has no role. A key on one is "not supported yet", and rule 1 doesn't require it.
4. **`temp` must be written.** v5 doesn't say what an unwritten `temp` means (language.md §8.2). An error can be relaxed later. A default (27 °C, as in SPICE) can't be taken back once files rely on it.
5. **Unwritten optional fields are ideal:** they get no knob and no element.

### 2.3 Which setup fields become knobs: naming and order

**References**
- **Our flatten** [V]:
  - a part field with a spread becomes a knob, and a spread of nothing is `Exact` (`flatten/mod.rs`, `devices()`; ngspice's `agauss` returns the nominal for one);
  - a knob's path is built from its source, never stored (`Flat::knob_path`);
  - devices are numbered in name order, so a shuffle changes nothing (E15).
- **Xyce:** a random parameter becomes a per-instance global named `X1:param` (model.md E16). [R]
- **ngspice:** parameter names are lowercased, so the engine deck names knobs `k0 … kN`, with each path in a comment (engine_plan §1.4). [V, in that plan]
- **[R2] §3.1–3.2:** which setup fields are knobs. "A setup knob's identity is (the setup item that writes the field, the field). Its printed path is the field (`usb.v`), with the owner shown when two setups own a knob of the same field." [V, our research]

**Which fields.** There's no real alternative here: it's E16's rule applied to setups.

| Setup field | Knob? | Kind | Path |
|---|---|---|---|
| `v`, `z`, `r`, `c`, `i` with `±` or a closed range | yes | Range | `vcc.v`, `input.z`, `output.c` |
| `..=b` on `z`, `r`, `c` (§2.9 b) | yes, `0 ..= b` | Range | `vcc.z` |
| a point (`input: Signal { v: 0V }`) | no: `Exact` | — | — |
| unwritten | no: ideal, no element | — | — |
| `temp: ambient` | the env's knob: one per root, however many setups read it | Range | `ambient` |
| `temp: -40°C..=125°C` | yes | Range | `temp` |
| `temp: 25°C`, or a const | no: `Exact` | — | — |

**Naming options**
- (a) **The port and the field, as a probe is written:** `vcc.v`. An env's knob is the env's name, `ambient`. The setup's own range is `temp`.
- (b) **Qualified by the setup:** `Operating::vcc.v`, `Operating::temp`.
- (c) **Under a `setup` prefix:** `setup.vcc.v`.

```rust
// (a): knob_path for the new sources
KnobSource::SetupField { port, field } => HierPath(vec![root.port_name(port).into(), shape.fields()[field].name.into()]),
KnobSource::SetupTemp => HierPath(vec!["temp".into()]),
KnobSource::Env(env) => HierPath(vec![design.envs[env.index()].name.clone()]),
```

Under (a), paths stay unique without a prefix:
- a root port can't have the same name as a root `let` (they share one value namespace, E5);
- so `vcc.v` (a port, then a field) never collides with a part knob (`r1.value`, a `let`, then a field);
- `ambient` and `temp` are one segment each, and one setup never has both.

**Order options**
- (a) **Part knobs first**, in device order as today; then the setup's fields, in port order and then the shape's field order; then `temp` or its env.
- (b) **Range knobs first**, as the engine deck numbers them (`k0 = temp`, `k1 = vcc.v`, engine_plan §1.4).
- (c) **Everything by path name.**

**Pros and cons**
- **Naming:**
  - (a) is what model.md §5, the engine plan and the walkthrough print, and it's how a reader writes a probe.
  - (b) is unique across setups, which the MVP doesn't need (one setup per root). [R2] shows the owner only when two setups own one field, which only happens once `on S` arrives.
  - (c) is noise.
- **Order:**
  - (a): adding a contract renumbers no part knob, so flatten's snapshots keep their numbers. Port order is the header's (the block's interface), which no shuffle of statements or entries changes.
  - (b) renumbers every part knob whenever a setup changes.
  - (c) mixes the two kinds.

**Cost:** for (a) and (a), three match arms in `knob_path` and one append. The engine's `KnobSpace` is its own (T1), so its adapter can number knobs however it likes. Only the examples in the docs change (§4).

**Recommendation: naming (a), order (a).**

### 2.4 An env's nominal

`ambient` is `-10°C..=60°C`. Does E16's midpoint rule give it 298.15 K, or should it have no nominal? model.md §5's table says "—".

**References**
- **ngspice and Xyce:** a circuit with no `.temp` runs at 27 °C.
  - ngspice: `REFTEMP (27.0 + CONSTCtoK)` (`src/include/ngspice/const.h:54`); the defaults are written as `300.15` (`src/spicelib/devices/cktinit.c:69-70`); a run prints "Doing analysis at TEMP = 27.000000". [V]
  - Xyce: `CONSTREFTEMP (300.15) // 27 degrees C` (`src/DeviceModelPKG/Core/N_DEV_Const.h:48`), and `temp(Param("TEMP", CONSTREFTEMP))` (`N_DEV_DeviceOptions.C:99-100`). [V]
- **Modelica:** a `Real`'s `nominal` attribute "is meant to be used for scaling purposes and to define tolerances in relative terms", and it has no default (§4.9.1, §4.9.6). It isn't a typical value. [V]
- **Our docs** [V]:
  - the walkthrough's nominal is 25 °C (§1: "VBE = 0.659 V at 25 °C");
  - the engine deck starts at `.param k0=25` (engine_plan §1.4);
  - engine_types' `KnobSpec.nominal` is "the midpoint in the MVP", and ε = 0 is the nominal run (§2.1).

**Options**
- A. **The midpoint, 298.15 K (25 °C).** The existing code already does this: `value.rs`'s `range()` gives every range its midpoint.
- B. **No nominal:** `Value.nominal: Option<Quantity>`, or a `Spread` for ranges with no nominal.
- C. **SPICE's 27 °C.**
- D. **A declared nominal:** `env ambient: Temperature in -10°C..=60°C = 25°C;`, which is new syntax.

```rust
// A: nothing new. The engine's nominal run is ε = 0, the midpoint.
Env { name: "ambient", value: Ok(Value { nominal: Quantity::kelvin_point(298.15), spread: Spread::Range { lo: 263.15, hi: 333.15 } }) }
// B: every reader of Value.nominal handles None, for envs only.
pub struct Value { pub nominal: Option<Quantity>, pub spread: Spread }
```

**Pros and cons**
- **A:**
  - No code.
  - It matches the walkthrough, the engine plan and the engine's ε mapping.
  - A range knob's nominal is only where the engine starts: the verdict covers the whole range.
- **B:** honest that an operating range has no "typical" point (Modelica's view). But every reader of `Value` changes, and the engine still needs a starting point, so it would put the midpoint back.
- **C:** falls outside many ranges (`50°C..=85°C`).
- **D:** a syntax change for a value nobody has asked for. model.md §7 question 3 already said a declared nominal comes only if the midpoint isn't enough.

**Cost**
- A: a doc fix: model.md §5's "—" becomes 298.15 K.
- B: `Value` and every reader of it.
- D: the grammar, the parser and resolve.

**Recommendation: A, 298.15 K.** It's the same rule that gives `beta: 100..=300` a nominal of 200.

### 2.5 The measure table

E24 asks for "a small table of built-in measure functions with declared argument and result units". The MVP's are `dc`, `ac`, `.at`, `.mag`, `.db`, `.f_low` and `.f_high` ([IR] §4.4).

**References**
- **ngspice `.meas`** [V]:
  - its types are fixed in C: an enum (`src/frontend/com_measure2.c:56-62`) and a name table (`:221-266`): FIND, WHEN, AVG, MIN, MAX, RMS, PP, INTEG, DERIV, …;
  - the analysis is part of the line (`.meas ac …`);
  - `db` is `20·log10(|x|)` (`src/maths/cmaths/cmath1.c:283-290`), and `vdb(x)` is `db(v(x))` (`src/frontend/cpitf.c:59-60`).
- **Xyce `.MEASURE`:** a fixed list of types, by name (`src/IOInterfacePKG/Output/N_IO_MeasureManager.C:245-353`). **Gnucap:** one class per function (`apps/measure_*.cc`). [V]
- **rustc:** an intrinsic's signature is one `match` on its name that gives its inputs and output (`rustc_hir_analysis/src/check/intrinsic.rs:290`: `sym::abort => (0, 0, vec![], tcx.types.never)`). An unknown name is an error (`:816`). [V]
- **rust-analyzer:** the built-in types are an enum with a name table (`hir-def/src/builtin_type.rs:52` `all_builtin_types`, `:79` `by_name`). [V] Our `PartKind::from_name` follows the same pattern.

**The table.** The units follow E11–E13.

| Written | Takes | Gives | Arguments | Becomes (engine_types §7) |
|---|---|---|---|---|
| `dc(x)` | `x`: probes `n.v`, numbers, `+ - * /` | a number, in `x`'s unit | — | `Probe`, `Binary` over the op |
| `ac(o.v / i.v)` | a net's voltage over an **input** port's | a response over frequency, `o/i` (V/V = 1) | — | excites `i`'s source (`AC 1`) |
| `r.at(f)` | a response | a phasor, in the same unit | `f`: Hz, exact, > 0 | `AcAt { num, den, point }`; `Needs.ac_points` |
| `p.mag()` | a phasor | a number, in the same unit | — | `Mag` |
| `x.db()` | a phasor or a number with no unit | a level in dB, `20·log10|x|` | — | not in M3's `Program` yet |
| `r.f_low(L)` | a response with no unit | a number in Hz | `L`: dB, exact, < 0 | `FLow { num, den, db }`; `Needs.sweep` |
| `r.f_high(L, ref: dc)` | the same | Hz | `L` as above; `ref: dc` (the DC gain), or left out (the peak, as for `f_low`) | not in M3's `Program` yet |

- **Arithmetic follows value.rs's rules (E12):**
  - `+` and `-` need one unit (`MixedUnits`);
  - `*` and `/` combine units;
  - a bare number next to a unit in `+` or `-` is an error;
  - dB levels can't be used in arithmetic (`NotArithmetic`).
- **A spec's measure must be a number.** Its bound is typed in that number's unit and kind: a `Db` bound for `.db()`.
- **v5's other names are "not supported yet":** `tran`, `noise`, `.min`, `.max`, `.settle`, `.crossing`, `.deviation`, `.span`, `.during`, `.after`, `.phase`. Any other name is `UnknownName`, with a suggestion.

**Options**

A. **What a method means and gives is in `spicy_model::measure`; its name and arguments are read in resolve**, one `match` arm each, as in rustc's intrinsic table.
```rust
// spicy_model::measure
pub enum MExpr {
    Const(Quantity),                                  // `1V` in `dc(vcc.v) - 1V`
    Voltage(NetId),                                   // `output.v`: only inside `dc(…)`
    Dc(Box<MExpr>),
    Ac { out: NetId, input: PortId },                 // `ac(output.v / input.v)`
    Measure(MeasureId),                               // `h`
    Method { method: Method, of: Box<MExpr> },
    Neg(Box<MExpr>),
    Binary { op: ArithOp, lhs: Box<MExpr>, rhs: Box<MExpr> },
}
pub enum Method { At { hz: f64 }, Mag, Db, FLow { db: f64 }, FHigh { db: f64, reference: Reference } }
pub enum MeasureType { Signal(Dimension), Number { dim: Dimension, kind: QKind }, Response(Dimension), Phasor(Dimension) }
impl Method {
    /// What it gives on `on`, or `None` where it doesn't apply (`WrongMeasureType`).
    pub fn result(self, on: MeasureType) -> Option<MeasureType> {
        use MeasureType::*;
        match (self, on) {
            (Method::At { .. }, Response(d)) => Some(Phasor(d)),
            (Method::Mag, Phasor(d)) => Some(Number { dim: d, kind: QKind::Plain }),
            (Method::Db, Phasor(d) | Number { dim: d, kind: QKind::Plain }) if d.is_none() => {
                Some(Number { dim: Dimension::NONE, kind: QKind::Db })
            }
            (Method::FLow { .. } | Method::FHigh { .. }, Response(d)) if d.is_none() => {
                Some(Number { dim: Dimension::HERTZ, kind: QKind::Plain })
            }
            _ => None,
        }
    }
}

// spicy_lang::resolve::contract
let method = match name.text {
    "at" => Method::At { hz: self.argument(args, "f", FieldType::exact(Dimension::HERTZ))? },
    "mag" => self.no_arguments(args, Method::Mag)?,
    "db" => self.no_arguments(args, Method::Db)?,
    "f_low" => Method::FLow { db: self.argument(args, "level", FieldType::level())? },
    "f_high" => self.f_high(args)?,
    other if NOT_YET.contains(&other) => return Err(self.unsupported(name)),
    _ => return Err(self.unknown_function(name)),
};
let ty = method.result(of.ty).ok_or_else(|| self.wrong_type(name, of.ty))?;
```

B. **A data table**, `const METHODS: &[MethodSig { name, receiver, args: &[FieldType], result }]`, matched by one generic checker.

C. **Everything in `spicy_lang`**, with the engine working out the types again.

**Pros and cons**
- **A:**
  - Each method's rule is one `match` arm, and the compiler checks the match covers every method (T8's argument for verdict tables).
  - The engine compiles the same `MExpr` (T7). `Ac { out, input }` is exactly the `num`/`den` pair its `Program` needs.
  - The names stay in the language, and what they mean stays in the model.
- **B:** the receiver rules aren't uniform (`.db()` takes a phasor or a number; `.f_low` takes only a ratio), so the checker would grow special cases.
- **C:** breaks E2: the engine would need the language's names to understand a measure.

**Cost:** A: one module of about 150 lines, plus its tests.

**Recommendation: A.**
- **The arguments are exact constants,** typed by `Resolver::value`: `1kHz` is in Hz, and `-3dB` is a dB level. So each `Method` holds plain numbers.
- **`ac(…)` is narrowed to `out.v / in.v`.** That's the only form the engine's `Program` and the exporter's `AC 1` can serve; anything else is `NoExcitation`, with that form in the help.
- **This check needs no setup.** Whether a port is an input is known from its type (`Power<In>`, `Analog<In>`), and rule 1 guarantees that every input has a source in every complete setup.

### 2.6 The `pub spec` rule

v5 §1.4: "A `pub spec` may name only the block's own ports and observables. A spec naming an internal net or a child's value is internal." Resolve detects it.

**References**
- **rustc** [V]:
  - "private type in public interface" is checked by `PrivateItemsInPublicInterfacesChecker` (`rustc_privacy/src/lib.rs:1522`);
  - it walks an item's resolved generics, clauses and type (`:1367-1397`), not its syntax;
  - it runs after name resolution and type checking (`rustc_interface/src/passes.rs:1201-1226`, "misc_checking_3");
  - it's a hard error (E0446) only for associated types in impls. Everywhere else it's a lint, a warning by default (`rustc_lint_defs/src/builtin.rs:4395-4397`).
- **Modelica:** "A protected element, P, in classes and components shall not be accessed via dot notation (e.g., A.P, a.P …)" (§4.1). [V]
- **VHDL:** an architecture's signals aren't visible outside its entity; only the ports are. [R]
- **atopile:** asserts have no visibility (none in the grammar, `AtoParser.g4:162-164`). [V]

**Options**

A. **A check over what resolve built.** After a spec's measure is resolved, walk its `MExpr`, through the `let`s it uses, for a `Voltage(net)` whose net isn't a port's.
```rust
/// The first net in `expr` that isn't a port: what makes a spec internal.
fn first_internal(expr: &MExpr, measures: &[Measure], block: &Block) -> Option<NetId> {
    match expr {
        MExpr::Voltage(net) | MExpr::Ac { out: net, .. } if block.net_port(*net).is_none() => Some(*net),
        MExpr::Measure(m) => measures[m.index()].expr.as_ref().ok().and_then(|e| first_internal(e, measures, block)),
        _ => expr.children().find_map(|e| first_internal(e, measures, block)),
    }
}
// the specs pass
if spec.public && let Some(net) = first_internal(&measure, &contract.measures, block) {
    // PublishedInternal at `pub`; related: the net's `net base;` (BlockSpans.nets); fix: remove `pub`
}
```

B. **Record each name's reach while resolving** ([IR] §3.3: "each name records which layer resolved it"). Every measure returns `(MExpr, MeasureType, reach: Option<Span>)`. A `let`'s reach is the union of its operands', and the spec reads it.

C. **Check it later**, in flatten or the engine.

**Pros and cons**
- **A:** one function over finished data, as rustc's checker runs over resolved types, with nothing threaded through the measure resolver. Its related span is the net's declaration, which says what the net is ("`base` is a net of `circuit CeAmp`"), not where it's used.
- **B:** points at the use, but every measure function returns one more thing, for one check.
- **C:** it's a fact about one definition (E18 tier 1), and a parent's checks would first run on a wrong promise.

**Cost:** A: about 20 lines and one error. A `let` used by several `pub` specs is walked once per spec, which takes microseconds.

**Recommendation: A.**
- The fix removes `pub`, as the parser's `PubNotAllowed` does. It's the only edit that keeps what the spec measures.
- A setup doesn't make a spec internal ([IR] §3.3). That holds here trivially: an MVP setup can't name a placement.

### 2.7 Strict `<` and `>` in specs

- The parser accepts every relation at the top of a spec (`rel_op = "within" | "<=" | ">=" | "<" | ">"`, grammar.md §3), and the AST keeps `RelOp::Lt` and `RelOp::Gt`.
- v5 lists `within`, `<=` and `>=` as the limits.
- `<` and `>` must stay in the expression grammar for predicates later (`.first(vout.v < 3.267V)`).

**References**
- **atopile:** its grammar has all four (`AtoParser.g4:187-194`), and its front end keeps them apart (`ast_visitor.py:1332-1350`). But its solver turns `<` into `<=` and `>` into `>=` and logs a warning, "not supported by solver, converting to …" (`src/faebryk/core/solver/symbolic/canonical.py:224-227, 392`). [V]
- **ngspice `.meas`, Xyce `.MEASURE` and Gnucap `measure`** have no limits at all: a measure only computes a number. [V]
  - ngspice rejects `goal=` with "no such parameter" (`com_measure2.c:1727`, run).
  - Xyce accepts `GOAL`, but nothing reads it.
- **Our engine:** a side is `value ≤ bound` (Max) or `value ≥ bound` (Min), and a value within ε_num of the bound (about 5e-9 V on the CE amp) is UNDECIDED (numerics) (engine_types §2.3, §8). So `<` and `<=` can't give different verdicts. [V, our docs]

**Options**

A. **Resolve reports `<` and `>`, with the fix `<=` or `>=`,** and reads the spec as the non-strict one, so nothing else is reported.
```rust
fn limit_op(&mut self, op: RelOp, at: Span) -> LimitOp {
    match op {
        RelOp::Within => LimitOp::Within,
        RelOp::Le => LimitOp::AtMost,
        RelOp::Ge => LimitOp::AtLeast,
        // Reported, with the fix, then read as the non-strict limit.
        RelOp::Lt => { self.strict(at, "<="); LimitOp::AtMost }
        RelOp::Gt => { self.strict(at, ">="); LimitOp::AtLeast }
    }
}
```

B. **The parser reports it.** The AST still has to say what was written (`Lt`), so resolve maps it anyway.

C. **Accept them as synonyms,** silently or with a warning, as atopile does.

D. **Accept them with a strict meaning:** the engine fails a value equal to the bound.

**Pros and cons**
- **A:** one spelling per limit, so the spec table and the formatter have one form. The fix keeps the meaning.
- **B:** the same result, but the parser would need to know which relations a spec allows, which is a rule about meaning. Today's parser only checks shapes.
- **C:** two spellings for one meaning. atopile's warning shows its solver's authors saw the trap.
- **D:** a promise the engine can't keep, since a value equal to the bound is inside numerical noise.

**Cost:** A: one error kind, one helper and a case file. The fix test covers the fix.

**Recommendation: A.**

### 2.8 What the exporter will need

The exporter (M1e lowering, then `spicy_netlist::writer`) turns one root and its setup into a deck. From this phase it needs:
- for each root port, what drives or loads it, with each value fixed or a knob;
- where each port, probe and the ground are;
- the temperature;
- which source each `ac` measure excites.

**References**
- **ngspice** [V]:
  - a voltage source is `Vname n+ n- [DC] v [AC [mag [phase]]]`. An unlabeled leading number is the DC value (`src/spicelib/parser/inp2v.c:56-58`);
  - `dc` and `ac` are instance parameters (`devices/vsrc/vsrc.c:13-15, 45`);
  - a bare `AC` means magnitude 1 (`vsrctemp.c:38-39`);
  - current sources read the same way (`inp2i.c:55-58`, `isrc.c:13-17`);
  - run: `V1 in 0 DC 2 AC 1 45` gives dc 2, acmag 1, acphase 45.
- **Our engine deck** (engine_plan §1.4): `V_vcc vcc 0 DC {k1}`, `V_input input 0 DC 0 AC 1`, nothing on `output`, `.temp {k0}`. [V, our docs]
- **[R2] §3.1:** the deck element for each setup field: `Supply.v` is the source's DC value; `z` is a series resistor; `Load.r` is a conductance; `Load.c` is a `C`; `Load.i` is an `I`; an unwritten field is nothing. [V, our research]
- **E8:** "which device a part kind becomes is lowering's job", so `spicy_model` stays simulator-free (roadmap §2.3). [V]

**Options**

A. **The flat setup, as shapes, plus the root's nets.** The specs stay in the `Design`.
```rust
pub struct FlatDesign { /* … */ pub root_nets: Vec<FlatNetId>, pub setup: Option<FlatSetup> }
pub struct FlatSetup { pub setup: SetupId, pub ports: Vec<Option<FlatPortSetup>>, pub temp: FlatField, pub tainted: Option<Reported> }
pub struct FlatPortSetup { pub shape: Shape, pub fields: Vec<FlatField> }
// lowering (M1e), for each root port with a shape:
let net = flat.data.root_nets[root.port_net(port).index()];
match port_setup.shape {
    Shape::Supply | Shape::Signal => { /* V source on `net`: DC = field v; AC 1 if an `ac` measure divides by this port; z → a series R */ }
    Shape::Load => { /* R r, C c, I i from `net` to ground, each only if written */ }
}
```

B. **Flatten emits bench elements** ([IR] §3.9, [R2] §2.5): `bench: Vec<BenchElement>`, with `VSource { net, dc, ac }`, `Resistor { … }` and so on.

C. **Lowering reads the `Design`'s `Setup` directly**, with no flat setup, and makes the setup knobs itself.

**Pros and cons**
- **A:**
  - Flatten does for a setup what it does for a part: a value with a spread becomes a knob reference (E17).
  - Lowering chooses the elements, as it does for parts.
  - One `FlatDesign` still serves every run.
- **B:** puts SPICE choices, such as a V source plus a series resistor, or `AC 1`, into `spicy_model`.
- **C:** knobs would come from two places, flatten and lowering.

**Cost:** A: two structs, two fields, and three `KnobSource` arms (step 5).

**Recommendation: A.** For ce_amp, lowering would then write the following. None of it is built in this phase.

```
V_vcc   vcc   0 DC 12          ; vcc: Supply { v: 12V ± 5% } (knob k6 in the engine deck)
V_input input 0 DC 0 AC 1      ; input: Signal { v: 0V }; AC 1 because h = ac(output.v / input.v)
                               ; output: Load {}: nothing, an open output
.temp 25                       ; temp: ambient, at its nominal 298.15 K
```

Two things for the exporter's plan to settle, not this one:
- **Two `ac` measures that excite different ports** (a gain and a PSRR) need separate AC runs, because two `AC 1` sources at once would add up. `MExpr::Ac { input }` records which port each one excites.
- **A source with a `z`** needs a node between the source and the port, which lowering names.

### 2.9 Smaller choices

#### a. Must a probe be inside `dc(…)` or `ac(…)`?

[IR] §6 question 6 asks whether a bare quantity means `dc(…)`. This phase meets it in `let x = output.v;`.
- **References:**
  - ngspice and Xyce always name the analysis (`.meas ac …`, `.MEASURE TRAN …`) [V, the runs above];
  - grammar.md's rule: adding a feature later never changes what a file means.
- **Options:** (1) an error, with the fix `dc(output.v)`; (2) a bare probe means `dc(…)`.
- **Pros and cons:**
  - (1) can become (2) later without changing any file that's valid now; (2) can't go back.
  - (2) saves four characters, and would make `output.v` in a measure mean something different from `output.v` in a setup (v5 rule 1.3.5), with nothing to mark it.
- **Cost:** (1) is one error kind and one fix.
- **Recommendation: (1), `NeedsAnalysis`.**

#### b. Open ranges in setups (`z: ..=0.5Ω`, `r: 10kΩ..`)

- **References:**
  - [R2] §3.1: `Supply.z: ..=0.5Ω` is a knob over 0 … 0.5 Ω, and `Load.r: 10kΩ..` is a conductance knob over 0 … 100 µS, because a resistor can't be infinite;
  - atopile only writes ranges with both ends, `a to b` (`AtoParser.g4:265-282`) [V].
- **Options:**
  - (1) `..=b` means `0..=b` on fields that can't be negative (`z`, `r`, `c`), stored as an ordinary `Spread::Range`, and `a..` is "not supported yet";
  - (2) both, with a conductance knob for `r: a..`;
  - (3) neither, yet.
- **Pros and cons:**
  - (1) needs no new `Spread` and no engine decision. The knob's midpoint (0.25 Ω for `..=0.5Ω`) is only where the engine starts.
  - (2) needs a knob whose physical value is `1/r`, a mapping the engine and the deck both have to agree on, and nothing reads it yet.
  - (3) rejects the LDO example's `z: ..=0.5Ω`, which costs nothing to take.
- **Cost:** (1) is one rule in the setup's field typing, plus `OpenRange` for `v: ..=5V` and `i: ..=1mA`. A current can be negative, so `..=` gives it no lower end.
- **Recommendation: (1).**

#### c. Attributes in setups and contracts

Resolve ignores every attribute today, in circuits too. `#[confidence(…)]` is in roadmap §4.1's MVP list, but not in [IR] §4.4's.
- **References:**
  - rustc reports an attribute it doesn't know ("cannot find attribute `…` in this scope") [R];
  - the engine computes both confidences on every side, and the attribute only picks the headline (engine_types §2.3, D7) [V, our docs].
- **Options:**
  - (1) every attribute on a setup, an entry or a contract statement is "not supported yet";
  - (2) read `#[confidence(worst_case | sigma(k))]` now;
  - (3) keep ignoring them.
- **Pros and cons:**
  - (1) drops nothing silently, and `#[confidence]` arrives with its first reader, the engine's adapter (M3f).
  - (2) adds a field nothing reads until M3.
  - (3) would ignore `#[confidence(worst_case)]` without a word.
- **Cost:** (1) is one `Unsupported` per attribute.
- **Recommendation: (1).** Circuits keep ignoring their attributes, as today: changing that is a separate change.

#### d. The types an `env` or `const` can declare

- **References:**
  - language.md §5.1 names quantity types by unit (`Volt`, `Ohm`, `Farad`, `Amp`, `Watt`, `Hertz`), and an env's type by quantity (`Temperature`, `Duration`);
  - `const GAIN: f64` is v5's generic const;
  - rust-analyzer's builtin types are a name table (`builtin_type.rs:52`) [V].
- **Options:**
  - (1) a `ValueType` table in `prelude.rs`: `Temperature`, `Duration`, the six unit names plus `Henry`, and `f64`, each a `FieldType`. `Confidence` and v5's other types are "not supported yet";
  - (2) only `Temperature` and `Duration`;
  - (3) any `Dimension`, by its symbol (`V`, `Ω`).
- **Pros and cons:**
  - (1) covers the docs' examples and nothing more.
  - (2) can't write `const VREF: Volt = 1.2V`.
  - (3) invents a syntax the docs don't use.
- **Cost:** (1) is a ten-row table.
- **Recommendation: (1).** Whether a time should be called `Duration` or `Second` is a naming question for language.md, not for this phase; only `Duration` goes in the table.

---

## 3. The open points of `syntax_v5_plan.md` §3

| Point | Here |
|---|---|
| A block with no circuit that nothing places is still a flatten root | **Settled: it isn't a root** (§3.1; steps 2, 4 and 5) |
| A second circuit doesn't taint its block | **Settled: it does** (step 2). See below |
| A root with no contract (no default setup): how is it checked (E3)? | **Settled: structurally only, and no error** (§3.1) |
| `ambient`'s nominal | **Settled:** 298.15 K, the midpoint (§2.4) |
| Strict comparisons | **Settled:** an error, with the fix `<=` or `>=` (§2.7) |
| Attribute arguments `name = value` ([IR] G15) | **Parked.** Only `#[outside(reason = …)]` and `#[check(A = …)]` need them, and neither is in the MVP. Attributes in setups and contracts are "not supported yet" (§2.9 c) |
| `fn`'s return type needs `->` | **Parked** until `fn` |
| v5 writes both `impl T for X {}` and `pub block X: T` | **Parked** until traits |
| A chain of `let`s in a contract, each reading the next, overflows a 2 MB stack: about 450 in a debug build, 1,100 in release (step 3 review) | **Parked (2026-10-03).** No real contract comes near it. Meanwhile the programs that run the front end give it a large stack (rustc runs on 17 MiB, rust-analyzer on 16 MiB). If one ever does: (A) a depth limit with an error, as rustc's `depth_limit` queries (128 by default), counting expression levels too on a 2 MB stack; or (B) type the lets in dependency order with an explicit stack, as flatten's cycle search (a tested prototype is in the step 3 review), so the stack holds one expression |
| v5 §4.7 cites [IR] "§8" for its questions | **A doc fix:** they're in [IR] §6 |
| [IR] §4.2's ce_amp setup leaves out `input` and `output` | **Settled by rule 1** (§2.2): that setup would be an `IncompleteSetup`. `circuits/ce_amp.spl` has both. model.md §5 notes it |

**The second-definition rule.**
- **The rule:** when it isn't known which of two definitions was meant, the first isn't checked.
  - A second circuit taints its block (step 2), as a second block already does.
  - A second setup of a name taints the first setup (step 2).
  - A second contract taints the first (step 3).
- **Why this differs from rustc's E0428:**
  - rustc keeps checking the first definition after E0428 and attaches no proof to it [V]: `rustc_resolve/src/imports.rs:662` keeps the old declaration, and `diagnostics/impls.rs:433-584` reports the second.
  - rustc's checks can only find real errors in the first definition. Ours would run flatten's whole-circuit checks on a circuit that may be the wrong one.
  - So we follow rustc's E0119 instead, which taints the whole trait's coherence result (`rustc_trait_selection/src/traits/specialize/mod.rs:428-454`) [V].
- **Cost:** one line per kind of item.

### 3.1 Blocks with no circuit or no contract (decided 2026-10-01)

**What happens today** (probed on the step 4 code):
- a block with no circuit that nothing places is flattened as a root with only its ports, and gets "isolated nets";
- with a setup and a spec but no circuit, it flattens with no error, so the engine would run `spec s: dc(vcc.v) within 4V..=6V;` against an empty block, and it would **pass**: the setup drives `vcc`, and nothing inside disagrees;
- the `Design` can't tell "no circuit" from an empty `circuit A {}`.

**How others do it** [R]: an interface alone is fine, using it as if it had an inside is an error, and no one checks behavior against an empty inside.
- VHDL analyzes an entity with no architecture, but can't elaborate or place it.
- Modelica's `partial model` can be extended, never instantiated or simulated.
- Cadence stops netlisting at a symbol with no schematic, unless it's a model-backed stop view.
- Rust's test harness runs "0 tests" without an error.

**Decided:**

| Block | What happens | Step |
|---|---|---|
| No circuit, placed | An error, `NoCircuit` (as today). When parts with models arrive (language.md §6.8), its help can say "or make it a `part` with a model" | — |
| No circuit, nothing places it | **Not a root:** flatten doesn't flatten it. Its setups and contract are still resolved (names, units, the `pub` rule). If its contract has specs, one warning: "`A`'s specs aren't checked: it has no circuit" (`UncheckedSpecs`, `W-unchecked`). With no contract, it's an interface and nothing is said | 2, 4, 5 |
| A circuit, no contract | **No error.** Flatten's structural checks run (power, shorts) and nothing else; its parent's specs cover it, if it's placed. `spicy check` says "`A`: no contract, only structural checks" when it lands; nothing in this phase | 5 |
| Two circuits | The second taints the block (the second-definition rule above) | 2 |

**What the steps add for it:**
- **Step 2:** `Block.has_circuit: bool`, set by resolve, so "no circuit" and `circuit A {}` differ.
- **Step 4:** the `UncheckedSpecs` warning, in the specs pass: a contract with specs whose block has no circuit. Its case file is `resolve/err/unchecked_specs.spl`.
- **Step 5:** `flatten`'s roots are the blocks nothing places **that have a circuit**. `Flat::checkable_specs` then can't return a spec of a block with no circuit, since such a block has no flat design. A root with no contract, or whose default setup doesn't resolve, gets `FlatDesign.setup = None`, no setup knobs and no checkable specs.
- **The engine's verdicts** (later) need a third outcome besides pass and fail: "not checked", with the reason.

---

## 4. Where the docs disagree with the code, or with each other

1. **model.md §5's knob table isn't in id order.**
   - It lists `r1 … re, c_in, q1.beta, vcc.v, ambient`, and [IR] §4.2 calls that "in id order".
   - Flatten numbers devices by name, so the ids are `c_in.value, q1.beta, r1.value, r2.value, rc.value, re.value` (the ce_amp flatten snapshot), then `vcc.v` and `ambient` (§0.4).
2. **model.md §5 gives `ambient` no nominal ("—").** E16's midpoint rule, `value.rs`'s `range()`, the walkthrough (25 °C) and the engine deck (`.param k0=25`) all give 298.15 K.
3. **The engine docs number knobs differently, and call the temperature `temp`.**
   - engine_plan §1.2 and §1.4, and engine_types §1, have `k0 = temp`, `k1 = vcc.v`, then the parts.
   - The model has the parts first, then `vcc.v`, then `ambient`.
   - The adapter may renumber (T1), but the examples should say which order they use.
4. **`KnobTable` is still read** in engine_types §1–§2 (`from_model(knobs: &KnobTable, …)`) and engine_plan §5.1 (`engine_deck(…, knobs: &KnobTable, …)`). model.md dropped it on 2026-09-29: knobs live in `FlatDesign.knobs`. [IR] §5 noted this too.
5. **roadmap M1d** still says contracts "add the `assume` knobs". roadmap §2.4's table still has `Contract { assumptions, … }` and `Assumption`, from v0.1.
6. **model.md §4 sketches `FlatDesign.contract: FlatContract`.** This plan builds something else: the specs stay in the `Design`, and flatten adds `root_nets` and `setup` (§2.8).
7. **The "`°C` or `K`" message** (`describe_expected` in `resolve/error.rs`):
   - It says a temperature may be written in `K`.
   - But a `K` literal types as a temperature *difference*: `Unit::Kelvin` gives a plain quantity (`units.rs`).
   - So `temp: 300K` is rejected, and then gets the "`K` is kelvin; kilo is lowercase `k`" note, which is wrong advice there.
   - Step 1's `TemperaturePoint` replaces both.
8. **engine_plan §1.3's default bench is v0.1's:** "an `AC 1` source on each `Analog<In>`; outputs unloaded". In v5:
   - the setup says what's on each port;
   - voltages must be written;
   - `AC 1` goes on the port that an `ac` measure divides by (§2.8).
9. **roadmap §4.1 lists `#[confidence(…)]` in the MVP.** [IR] §4.4 doesn't, and resolve silently ignores every attribute today (§2.9 c).
10. **E24 says a spec becomes "its relation (`within`, `<=`, `>=`), bound and confidence attribute".** But the parser also accepts `<` and `>` (§2.7), and nothing reads the attribute (§2.9 c).
11. **The header of `resolve/mod.rs`** says contracts and setups are "only matched to their blocks here". That changes in step 2.

---

## 5. Not in this phase

- **Spec clauses and the rest of v5's setup and contract syntax:** `on S`, `with`, `for` and `in M`; `..Base`; modes; events; pairs; `rated`; the function form; `ensure`; `m[s]`. Each is parsed now or will be, and resolve reports it as "not supported yet" wherever it meets it.
- **More measures:** `tran`, `noise` and the other methods; the probes `.i` and `.z(f)`, and probes of part pins.
- **The hierarchy's contracts:** children's contracts are resolved and kept but not flattened (E24), and there's no composition check yet (language.md §8.8).
- **Several files,** and how they share an env ([IR] D11).
- **The rationale doc comments on specs.** They stay in the syntax tree, and the source map can point at them once the spec table needs them. Nothing reads them yet.
- **Spans for each node inside `MExpr`.** Flatten reports nothing about measures, so nothing would read them yet.
