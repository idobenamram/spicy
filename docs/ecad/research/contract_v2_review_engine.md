# Draft 2, Seen from Elaboration and the Engine

> 2026-09-30 · Review of `contract_syntax_v2.md` (draft 2) from below: what elaboration makes of it, what the engine receives, what it costs, and what can't be honoured soundly.
> Reads with: `model.md` (E1–E26), `engine_plan.md`, `engine_types.md` (T1–T11), `engine.md` v4, `research/contract_hierarchy.md` ([H]), `research/next_synthesis.md` (seams T1, T2, E5, E7), `research/contract_options.md` §1, `research/contract_option_c_interface.md` §4 (the closest earlier lowering).
> Code read (read-only): `crates/spicy_model/src/{design,flat}.rs`, `flatten/`, `crates/spicy_lang/src/{parser,resolve}/`.
> Run counts here are **estimates** from knob counts and the measured per-run costs in the plan and [H]. No new simulation was done.

---

## Summary

Draft 2 lowers cleanly. Most of it maps onto seams the engine already planned. Five decisions make it work:

1. **A setup is a scenario.** Each setup item, after inheritance, becomes one `Setup` in the engine's plan: a bench (deck), a subset of knobs, pinned values and its analyses. `BenchId` (seam T2) becomes `SetupId`, and next_synthesis's `Scenario` (E5) is the same thing. One concept, not three.
2. **Knob identity comes from where a field is written.** A setup field with a range is a range knob owned by the setup item that writes it. A derived setup that doesn't override the field uses the **same** knob. So `AtZero` and `AtFull` share `usb.v`, `usb.z` and `temp`, and differ only in `sensor.v`, which each pins. Part knobs are always shared: one board. Globals are one knob for the whole check.
3. **A spec is a measure program over one or more slots.** The body is a straight-line expression graph: `let`s, arithmetic, built-in measure methods, `on <setup>`, then one or more `require`s. Each `require` gives one or two sides. A spec with *k* setup parameters evaluates each point as *k* runs of the same board (seam E7). No `if` on measured values, no loops except over constants, no user functions that simulate.
4. **Generics are monomorphized per distinct argument tuple, not per placement.** `GainStage<Mcp6001, 10.09>` placed three times is one instantiation, one child table. The body is checked once against the bounds, as rustc does.
5. **The child cache key is content, never names or places:** interface + chosen implementation + generic arguments + the effective `accepts` setup (globals resolved to values) + the children's keys + model defaults, policy and backend. `rev` for a root check becomes this key.

**Problems to fix before they're built** (§8). The ones that would make the engine confidently wrong:
- An `accepts` field that isn't written must mean **the ideal value** (a zero-impedance supply, an open output), never "anything". Otherwise a two-way check passes on missing information.
- Overrides in derived setups must merge **field by field** (Modelica modifiers). `AtZero`'s `sensor: Signal { v: 0V }` must keep Operating's `z`, not drop it.
- A setup must not reach inside a child (`mcu.load`). It breaks the child's cache and encapsulation.
- `require … for f in band` and `v.at(end)` hide a sampling grid and a stop time. Both need explicit meaning.
- Quantized measures (`mcu.code`) are step functions. The inside-the-box guards and the σ nudges assume smoothness.
- The draft's own board fails its two-way check at `amp.vout`: the RC anti-alias filter is not a `Load { r: 10kΩ.., c: ..=1nF }`. That's the check working, but it means every amp spec falls back to in-context runs.

**Cost.** Children are cheap: a few thousand block runs, about 30 s, most of it transients. Board specs blow up: `sensitivity` has a 14-knob DC cone with two slots (32,768 runs flat, over the 2^12 budget), and `wake_noise` is a ~19-knob transient. Both need composition or the loop. Multi-setup specs double every point unless their slots merge into one DC sweep.

**The MVP slice** (§9): one `block`, exactly one `impl Circuit`, one `setup` without inheritance, `impl Contract` with `accepts` and single-setup specs whose body is `let`s plus `require`, and `global` range knobs. Built with the multi-slot, multi-setup and multi-impl seams in the types from day one, so the CE amp runs 579 runs on the unchanged deck and nothing is rewritten later.

---

## 1. What exists today

| Where | State | What draft 2 changes |
|---|---|---|
| `spicy_model::design::Block` | name, ports, **nets, instances, merges, tainted**: interface and body in one struct | Split: `Block` keeps the interface; the body moves to `Impl` |
| `Design.contracts: Vec<Option<Contract>>` | `Contract {}` is empty (M1d-5 not started) | Filled with `accepts`, `rated`, `specs` |
| `Instance.fields` | "Empty for a placed block (blocks have no params yet)" | Placements carry generic arguments |
| `flat::Knob` / `KnobSource::Field` | Part knobs only; `KnobKind { Statistical, Range }` with Range reserved for contracts | `KnobSource` gains `SetupField` and `Global` |
| `FlatInstance { block, origin }` | Keyed by `BlockId` | Needs the implementation and the instantiation |
| Parser | `block`, `contract` with `assume` and `spec name: relation;` | New items: `impl … for`, `setup … for`, `global`; spec bodies |

The good news: the flatten passes (recursion, roots, tree, joins, naming, devices, checks) don't care where a body comes from. Only the lookup of "the body of this placement" changes.

---

## 2. What elaboration produces

### 2.1 The `Design`: one entry per item, folded

```rust
pub struct Design {
    pub blocks:    Vec<Block>,             // interfaces: name, generics, ports       BlockId
    pub impls:     Vec<Impl>,              // bodies: today's nets/instances/merges   ImplId
    pub setups:    Vec<SetupDef>,          // named environments                      SetupDefId
    pub contracts: Vec<Option<Contract>>,  // indexed like blocks, as today
    pub globals:   Vec<Global>,            // GlobalId
}
pub struct Block {
    pub name: String,
    pub generics: Vec<GenericParam>,       // A: OpAmp = Mcp6001 · const GAIN: Ratio = 10.09
    pub ports: Vec<Port>,
    pub impls: Vec<ImplId>,                // every `impl Circuit for` it has
}
pub struct Impl {                          // exactly today's body fields
    pub block: BlockId, pub name: Option<String>,   // a name only when there are several
    pub nets: Vec<Net>, pub instances: Vec<Instance>, pub merges: Vec<Merge>,
    pub tainted: Option<Reported>,
}
pub enum GenericParam { Kind { name: String, bound: TraitId, default: Option<PartOrBlock> },
                        Const { name: String, ty: FieldType, default: Option<Quantity> } }
```

- **Ports stay the first nets of each `Impl`** (the rule in `design.rs` today), so flatten's `base[instance] + NetId` entries don't change.
- **An `Instance` of a block gets `args: Vec<GenericArg>`** and, later, `imp: ImplChoice` (§6.3).
- **`impl Regulator for Ldo3v3 {}`** is a trait impl: it records that the block has the trait's ports and brings the trait's generic setups into scope. It has no body and no knobs.

### 2.2 Setups: resolved, then made effective

```rust
pub struct SetupDef {
    pub name: String, pub for_block: BlockId,
    pub base: Option<SetupDefId>,          // `: Operating`
    pub fields: Vec<SetupField>,           // only what this item writes
}
pub struct SetupField { pub target: SetupTarget, pub value: SetupValue }
pub enum SetupTarget { Port(PortId, FieldPath), Temp, Activity(ChildPath, Name) /* §8 P3: restricted */ }
pub enum SetupValue { Shape(Shape), Global(GlobalId), Value(Value) }
pub enum Shape {
    Supply { v: Slot, z: Slot },
    Signal { v: Slot, z: Slot, wave: Option<Wave> },
    Load   { r: Slot, c: Slot, i: Slot, step: Option<Wave> },
}
pub enum Slot { Unwritten, Given(Value) }  // Unwritten ≠ "any": see §8 P1
```

**Effective setup.** Resolve computes, for each setup, every field with the setup item that wrote it:

```
Operating for SensorBoard      usb.v 4.40..=5.25 V (Operating) · usb.z 0.1..=0.5 Ω (Operating)
                               sensor.v 0..=100 mV (Operating) · sensor.z 100..=10k Ω (Operating) · temp = ambient
AtZero : Operating             usb.v (Operating) · usb.z (Operating) · sensor.v = 0 V (AtZero) · sensor.z (Operating) · temp
```

- **Merge rule: field by field.** An override replaces the fields it writes and keeps the rest (Modelica §7.2 modifiers; model.md E9 already follows it for part fields).
- **Checks at resolve, once per definition (E18 tier 1):** the base is for the same block; no inheritance cycle; each field fits its port's role (`Supply` on `Power<In>`, `Load` on an output, as language §8.2 does for `assume`); units.
- **Envelope check:** a spec's setup must lie inside the contract's `accepts`, field by field (`0 V ∈ 0..=100 mV`). A setup that leaves it is a one-off; its verdicts never feed composition (common core #11). Same shapes make this decidable at resolve.

### 2.3 The contract and its specs

```rust
pub struct Contract {
    pub accepts: SetupDefId,
    pub rated: Vec<Rating>,                // rated temp within …; rated vin.v within …
    pub specs: Vec<SpecDef>,
}
pub struct SpecDef {
    pub name: String,
    pub params: Vec<(String, SetupRef)>,   // (env, Operating), or (zero, AtZero), (full, AtFull)
    pub lets: Vec<(String, MExpr)>,        // the body, in dependency order
    pub requires: Vec<Require>,            // usually one
}
pub enum SetupRef { Def(SetupDefId), Generic { template: SetupTemplateId, args: Vec<BlockId> } }  // LoadStep<Self>
pub struct Require { pub measure: MExpr, pub relation: Relation, pub bound: Bound, pub pins: Vec<Pin> }
pub enum MExpr {
    Probe { slot: u8, probe: Probe },      // slot = which parameter; `on full` → slot 1
    Knob(KnobRef), Const(Quantity), Generic(ConstParamId),
    Call(MeasureFn, Vec<MExpr>), Binary(Op, Box<MExpr>, Box<MExpr>),
    Local(LetId),
}
```

- **`MExpr` is model.md E24's, with a slot on every probe.** A single-setup spec has slot 0 everywhere.
- **`on <name>` resolves to a slot.** In a spec with one parameter it's optional. With several, every probe must say which slot, and a probe with none is an error ("which run: `zero` or `full`?"). A slot inherited through a `let` is fine: `let v = tran(vout.v)` in a one-slot spec is slot 0.
- **`require … at vin.v = 4.3V`** (the LDO's `psrr`) is sugar for an anonymous setup derived from the parameter with that field pinned. It must lie inside `accepts`.
- **Published port facts** (`input_z`, `output_z`, `supply`) are ordinary specs. After resolve, a spec whose only measure is one port quantity (`port.z(f)`, `port.i`, `port.v`) on the `accepts` setup, with a constant bound, is also recorded as a `PortFact` (§6.1). Anything else is a spec, not a fact.

### 2.4 Globals

```rust
pub struct Global { pub name: String, pub value: GlobalValue }
pub enum GlobalValue { Range(Value), Policy(PolicyItem) }   // ambient, life · confidence
```

| Global | Becomes |
|---|---|
| `ambient: Temperature in -10°C..=60°C` | **one range knob** per check, path `ambient`, bound to the circuit temperature wherever a setup says `temp: ambient`. The join key across the hierarchy ([H] §4.5) |
| `life: Duration in 0y..=10y` | a range knob **only if** a part model reads it (aging, M5). Until then: a warning "declared, read by nothing" and no knob. A knob no run reads would be a dead knob in every cone |
| `confidence = sigma(3)` | `Policy.confidence` (engine_types T2), not a knob. Printed in the report's policy |

### 2.5 Flatten: setups become knobs

Flatten stays a pure function of the `Design` and a root. It now also takes the root's contract and produces one `FlatSetup` per setup the root's specs use:

```rust
pub struct FlatContract {
    pub setups: Vec<FlatSetup>,            // one per effective setup used (anonymous pins included)
    pub specs: Vec<FlatSpec>,              // SpecPath = root placement path + spec name
}
pub struct FlatSetup {
    pub def: SetupRef, pub pins: Vec<(KnobId, Quantity)>,
    pub knobs: Vec<KnobId>,                // this setup's subset: every part knob + its own range knobs
    pub bench: Vec<BenchElement>,          // what lowering (M1e) turns into sources and loads
}
pub enum KnobSource {
    Field { device: FlatDeviceId, field: usize },                 // today
    SetupField { owner: SetupDefId, target: SetupTarget },        // usb.v, vout.c
    Global(GlobalId),                                             // ambient
}
```

The knob's kind still follows its source (the rule in `flat.rs` today): `Field` → Statistical, `SetupField` and `Global` → Range. So `12V ± 5%` in a setup is a range, and `47k ± 1%` on a part is statistical, as now.

---

## 3. Knobs: which, and who shares them

### 3.1 Which setup fields become knobs

| Setup field | Knob? | Kind, origin | Deck element (M1e) |
|---|---|---|---|
| `Supply.v: 3.2V..=3.4V` or `12V ± 5%` | yes | Range, Interface | `V` DC `{k}` |
| `Supply.z: ..=0.5Ω` | yes (0 … 0.5 Ω) | Range, Interface | series `R {max(k, 1µΩ)}`: the floor is a printed default |
| `Signal.z: 100Ω..=10kΩ` | yes | Range, Interface | series `R` after the source |
| `Signal.v: 0V..=100mV` | yes | Range, Interface | the source's DC value |
| `Signal.v: 0V` (a point) | no: a pin | — | the same source, fixed |
| `Load.r: 10kΩ..` (unbounded) | yes, **as a conductance** g ∈ 0 … 100 µS | Range, Interface | `G` (VCCS) from the port to ground, `{k}`: g = 0 is open, legal in SPICE; a resistor can't be infinite |
| `Load.c: ..=1nF` | yes (0 … 1 nF) | Range, Interface | `C {k}`. C = 0 must pass the read-back test on ngspice |
| `Load.i: 0.1mA..=50mA` | yes | Range, Interface | `I` DC `{k}`. Log scale wanted (seam M4) |
| `wave: Step { 0V -> 100mV, edge: 1us }` | no: bench-local points | — | `PWL` |
| `edge: 1us..=10us` (a range in a wave) | yes | Range, origin Bench, **that setup only** | the `PWL` breakpoint |
| `temp: ambient` | the global's knob | Range, Global | `.temp {k}` |
| `temp: 25°C` | no: a pin | — | `.temp 25` |
| Unwritten on an `Analog<In>` | no | — | ideal `AC 1` source: today's default bench |
| Unwritten on an output | no | — | open |

### 3.2 Identity: the owner of the field

**A setup knob's identity is (the setup item that writes the field, the field).** Its printed path is the field (`usb.v`), with the owner shown when two setups own a knob of the same field (`AtZero::sensor.v`).

Consequences, on the board:

| Knob | Owner | In Operating | In AtZero | In AtFull | In McuWakes |
|---|---|---|---|---|---|
| every part knob (`amp.r_f.value` …) | the part | ✓ | ✓ same | ✓ same | ✓ same |
| `ambient` | the global | ✓ | ✓ same | ✓ same | ✓ same |
| `usb.v`, `usb.z`, `sensor.z` | Operating | ✓ | ✓ same | ✓ same | ✓ same |
| `sensor.v` 0 … 100 mV | Operating | ✓ | pinned 0 V | pinned 100 mV | ✓ |
| `mcu.load.step` | McuWakes | — | — | — | points only |

### 3.3 Shared vs per-setup, and why this rule

The question the draft raises: in `sensitivity(zero: AtZero, full: AtFull)`, is `usb.v` one value for both runs, or two?

- **Part knobs: always one value.** It's one board. Two setups of a spec are two measurements of the same physical parts. This is what E7 (multi-run measures) needs, and what calibration needs ([PA]'s bridge).
- **Globals: one value.** One room.
- **Range fields inherited unchanged: one value.** Inheritance reads as "the same as". A two-point measurement is taken with the board sitting in one environment, and only the named things change. It's also what the text says: `AtZero` didn't write `usb.v`.
- **Range fields a setup writes: its own knob.** `AtZero` writing `usb: Supply { v: 4.4V..=4.6V }` means "in this run, the supply is somewhere else", independently.

**Soundness.** Sharing is the smaller space, so it gives the tighter answer. That's only right if the text means it. Two protections:
- The report prints the joins of every multi-setup spec: "sensitivity: `zero` and `full` share every part, `ambient`, `usb.v`, `usb.z`, `sensor.z`".
- Two setups with no common base share only parts and globals. A lint says so, because it's rarely meant.

To force independence without changing the range, a setup re-writes the field (`usb: Supply { v: ..base }` style syntax, to be designed). No new keyword is needed in the engine.

### 3.4 The engine's `KnobSpace` for a multi-setup spec

One `KnobSpace` per check (T1), the union of every setup's knobs. Each `Setup` lists its subset. A spec's cone is the union of its slots' cones, with shared knobs once. For `sensitivity`: parts ∪ {ambient, usb.v, usb.z, sensor.z}, with `sensor.v` pinned differently per slot. A point is one ε vector; slot *s* reads the subset `Setup[s].knobs` of it, plus its pins.

---

## 4. Specs become measure programs

### 4.1 The allowed body

| Allowed | Lowers to | Why the limit |
|---|---|---|
| `let x = <measure expr>` | a node in the program's DAG | — |
| probes `vout.v`, `vdd.i`, port quantities `vin.z(f)` | `Probe { slot, probe }`, or a job with its own excitation | T4: indexes into `Run` |
| built-in methods (`at`, `mag`, `db`, `f_high`, `f_low`, `min`, `time_to`, `time_to_within`, `deviation`) | `Program` nodes with declared units and a **measure kind** (smooth / kink / quantized) | E1's guards need the kind |
| arithmetic, `on <slot>` | `Binary`, slot index | D-F: per-run evaluation keeps correlation |
| knob and generic reads (`GAIN`, `vcc.v`) | `Knob(KnobId)`, `Const` | E8 |
| `require m <rel> bound` | one or two `Side`s | T8 |
| `require … for f in band` | `Band { lo, hi, reduce: Max }` over the sweep: **a grid, not a proof** (§8 P4) | |
| `if` on `Exact` values, `for` over constants | unrolled at resolve (E25) | structure must be `Exact` |

**Not allowed**, each with an error that says why:
- `if` on a measured value. It makes the measure discontinuous in the knobs, so the corner theorem, the tangent check and the σ nudges don't apply.
- Loops over measured values, recursion, user functions that call analyses.
- A bound that depends on a measured value. `require a <= b` is fine: it lowers to `a − b <= 0`, evaluated per run.
- A probe of a slot that isn't a parameter of the spec.

### 4.2 From `require` to sides

- `within lo..=hi` and `within x ± t` give a Max and a Min side. `<=` gives Max, `>=` gives Min (engine_types §2.3).
- Several `require`s in one spec give sides named `spec#0`, `spec#1`, … under one `SpecPath`.
- The bound must be `Exact` after monomorphization: `GAIN ± 1%` with `GAIN = 10.09` is 9.989 … 10.191.

### 4.3 `on <setup>`: how slots map to runs

A spec with parameters (s₀ … s_{k−1}) is an **experiment** of *k* slots. For a point ε:

```
point ε ──► Request { point: ε, setup: S(AtZero), … }   ──► Run r0
        └─► Request { point: ε, setup: S(AtFull), … }   ──► Run r1
measure(ε) = Program.eval(&[r0, r1], space, ε)            evaluated once both rows exist
```

- Each slot's request is an ordinary request. It dedups against the run table like any other, so a slot shared with another spec costs nothing extra.
- **Merging slots, for speed.** When the slots' setups differ only in one source's DC value (AtZero vs AtFull: `sensor.v` 0 V vs 100 mV), the adapter turns the pair into **one request with a DC sweep** over that source at the two values (seam T1's `DcSweep`). One ngspice run, two answers. It halves `sensitivity`'s cost.

---

## 5. What changes in the engine's types

### 5.1 Model side (`model.md`)

| Type | Change |
|---|---|
| `Design` | `+ impls`, `+ setups`, `+ globals`; `Block` keeps the interface only (§2.1) |
| `Instance` (of a block) | `+ args: Vec<GenericArg>`; later `+ imp: ImplChoice` |
| `FlatInstance` | `block` → `inst: InstantiationId` (block + args + impl, interned) |
| `KnobSource` | `+ SetupField { owner, target }`, `+ Global(GlobalId)` |
| `FlatContract` | `setups: Vec<FlatSetup>`, `specs: Vec<FlatSpec>` with `SpecPath` (next_synthesis M1) |
| `MExpr` | `Probe` gains `slot` |
| E16 | "Every `assume` on a top-level port quantity becomes a range knob" → "every range-valued field of an effective setup used by a spec" |
| E24 | "In the MVP only the top block's contract is analyzed" stands. Children's contracts are resolved, monomorphized and kept for §6 |

### 5.2 Engine side (`engine_types.md`)

```rust
pub struct Plan {
    pub setups: Vec<Setup>,                // SetupId = index. Was: one implicit bench
    pub experiments: Vec<Experiment>,      // ExperimentId = index
    pub measures: Vec<MeasureDef>,
    pub sides: Vec<Side>,
    pub cones: Vec<Cone>,
}
pub struct Setup {                         // = next_synthesis E5's Scenario, = T2's BenchId
    pub id: SetupId,
    pub name: String,                      // "SensorBoard::AtZero"
    pub deck: DeckId,                      // one deck per distinct circuit (common core #12)
    pub knobs: Vec<KnobId>,                // subset of the KnobSpace
    pub pins: Vec<(KnobId, f64)>,          // sensor.v = 0.0
    pub needs: Needs,                      // per setup: its probes and analyses
    pub mode: Option<ModeLevel>,           // E5, later
}
pub struct Experiment { pub slots: Vec<SetupId> }   // len 1 for every MVP spec
pub struct MeasureDef { pub name: String, pub program: Program, pub experiment: ExperimentId,
                        pub cone: ConeId, pub unit: Unit, pub kind: MeasureKind }
pub enum MeasureKind { Smooth, Kink, Quantized }    // E1; Quantized: §8 P5
pub struct Side { pub id: SideId, pub spec: SpecPath, pub clause: u16, pub sense: Sense,
                  pub bound: f64, pub measure: MeasureId, pub confidence: Confidence }
pub struct Needs { pub probes: Vec<Probe>, pub devices: Vec<DeviceNeed>,
                   pub analyses: Vec<AnalysisSpec> }                   // seam T1
pub enum AnalysisSpec { Op, Ac { excitation: Excitation, grid: AcGrid }, DcSweep { source: SourceId, values: Vec<f64> },
                        Tran { stop: f64, step: f64, window: (f64, f64) }, Noise { .. } }
pub enum Excitation { Source(PortRef), Inject(PortRef) }              // `vin.z(f)` injects
pub struct Request { pub point: Point, pub setup: SetupId, pub analyses: AnalysisSet,
                     pub tolerance: Tolerance, pub care: Option<ConeId> }
pub enum Program { Probe { slot: u8, probe: ProbeId }, AcAt { slot: u8, .. }, FLow { slot: u8, .. },
                   Knob(KnobId), Band { .. }, Crossing { .. }, Binary(..), Const(f64), .. }
impl Program { pub fn eval(&self, runs: &[&Run], space: &KnobSpace, point: &Point) -> Measured; }
```

| Where | Change | Why |
|---|---|---|
| `RunTable` key | `(SetupId, Key)`; `exact` dedup over `(SetupId, Key, Tolerance)` | two setups at one point are two circuits (T2) |
| `Row::measured` | moves to `Evaluations: HashMap<(MeasureId, Key), Measured>` over the measure's cone | a multi-slot measure belongs to a point, not a row. For one slot it's filled from the row as it arrives, as today |
| `Backend::prepare` | takes every setup's deck; `Prepared` holds them by `DeckId`; `run` dispatches on `Request.setup` | one worker pool, several decks |
| `KnobSpec` | `+ origin: Part | Interface | Global | Bench` | report, joins (§6) |
| `Record` | `spec: SpecPath`, `+ setups: Vec<String>`, `+ joins` for multi-slot specs | the reader sees which runs a verdict rests on |
| `rev` | the root's `BlockKey` (§6.2) plus the policy | one hash for standalone and cached checks |

The CE amp fills all of this with one setup, one experiment of one slot, and a key of `(S0, Key)`: numerically nothing changes.

---

## 6. Hierarchy

### 6.1 The two-way check, reading `accepts` and the published facts

For each placement *p* of block *B*, and each port of *B*:

```rust
pub struct PortEnvelope { pub port: PortId, pub accepted: Shape, pub facts: Vec<PortFact> }
pub struct PortFact { pub spec: SpecPath, pub qty: PortQty, pub at: At, pub range: Interval, pub verdict: VerdictRef }
pub enum PortQty { V, I, Zin, Zout }
pub enum At { Dc, Freq(f64), Band(f64, f64) }
pub struct ConnectionCheck { pub net: FlatNetId, pub port: (FlatInstanceId, PortId), pub field: FieldPath,
                             pub presented: Presented, pub accepted: Interval, pub result: Containment }
pub enum Presented { Fact(PortFact), ParentSetup(KnobId), Context { runs: u32, range: Interval }, Missing }
pub enum Containment { Holds, Fails, Undecided(Why) }            // Why: shape mismatch, missing fact, frequency not covered
```

1. **Into the child:** what the net presents to the port (the driver's published `v` and `z_out`, or the parent's own setup field for a parent port) must lie inside the child's accepted field.
2. **Out of the child:** the child's published facts (`z_in`, supply current) summed with the other receivers must lie inside what the driver accepts.
3. **Where the presented value comes from:** a neighbour's `PortFact`; the parent's setup (for the parent's own ports); or, for parent-local parts on the net (`r_aa`, `c_aa`), a small **context characterization**: the impedance into the rest of the net over those parts' knobs (2^3 runs here, or a closed form).
4. **Result:** the child's verdicts carry over, 0 runs, recorded in `Record.rests_on`, when every field holds both ways. Otherwise the child's specs are checked in context (rung 4, or rung 3 with tables).
5. **A composed PASS rests on the child's facts being PASS.** A fact that FAILs or is UNDECIDED at its own check can't be used.

### 6.2 The cache key

```
BlockKey(inst) = hash( interface(block)                         ports, generic bounds
                     + impl body after monomorphization          spans stripped (E22)
                     + generic args, canonical                   Mcp6001, 10.09 (SI bits)
                     + BlockKey of every child instantiation     transitive: an edit deep down changes it
                     + effective `accepts` setup, by content     globals resolved to their values
                     + part-kind and model defaults (D-A)
                     + policy + backend identity )
SetupKey = hash(effective setup content)                         renaming a setup keeps the rows
rows: (SetupKey, physical values of the cone's knobs)            T6: a range edit keeps covered rows
```

- **Not in the key:** the placement path, the parent, the neighbours, the setup's name.
- **Several implementations:** the impl is part of the key. Each impl has its own table and its own verdicts against the one contract.
- **Generics:** each distinct argument tuple is its own key. Three placements of `GainStage<Mcp6001, 10.09>` share one. `GainStage<Tlv9001>` is another.
- **Globals:** by value. A library block that says `temp: ambient` gets a different key in a project whose `ambient` differs. That's correct, but it means a library block should write its own range (§8 P7).

### 6.3 Generics and implementations: when they're resolved

| Question | Answer | From |
|---|---|---|
| When is `GainStage<A, GAIN>` checked? | Once, generically, at resolve: `A`'s pins come from the bound `OpAmp`; `GAIN`'s unit from its type | rustc checks generic bodies once against bounds |
| When is it expanded? | At flatten, once per distinct `(block, args)` tuple: an interned `InstantiationId` | slang caches bodies per (definition, parameter values); Yosys `$paramod` |
| Are `GAIN`'s uses `Exact`? | Yes (E25): a const generic can shape values and bounds, never carry a spread | Modelica's variability order |
| Which impl does a placement get? | MVP: the only one; two impls and no choice is an error. Later: an explicit choice at the placement, recorded in the instantiation | — |

**A property worth keeping:** a parent verdict composed at rung 1–2 rests on the child's *contract*, so it holds for any impl that passes that contract. A rung-4 verdict rests on the chosen impl. The report should say which.

### 6.4 The worked connection: `board.amp`

`GainStage<Mcp6001>` accepts `Operating`. Checked at `SensorBoard`:

| Port · field | Accepted | Presented | From | Result |
|---|---|---|---|---|
| `vin` · source z | 100 Ω … 10 kΩ | 100 Ω … 10 kΩ | the board's own `Operating` (`sensor.z`) | Holds |
| `vdd` · v | 3.2 … 3.4 V | 3.234 … 3.366 V (LDO `output`) − ≤ 150 µA × 10.5 Ω = 3.232 … 3.366 V | LDO fact + amp's `supply` fact + `r_filt` | Holds |
| `vdd` · z | **unwritten = ideal (0 Ω)** | 9.5 … 10.5 Ω (`r_filt`) + the LDO's z_out | `r_filt` | **Fails** (P1) |
| `vout` · load | `Load { r ≥ 10 kΩ, c ≤ 1 nF }` (parallel R ∥ C) | 470 Ω in series with 100 nF, then the ADC input | `r_aa`, `c_aa`, `mcu` | **Undecided (shape)**, and above 1 kHz it's under 10 kΩ anyway |
| `temp` | `ambient` | `ambient` | same global | Holds, join key |

The amp's specs are not carried. They're re-checked in context. The report says why, with both lines, as [H] §4.2 asks. The fix in the text is a `vdd` source impedance in `Operating` and a load shape that describes an RC filter.

---

## 7. Cost on the sensor board

Estimates. Knob counts assume `MidRail` has two resistors and one capacitor, `Mcp6001` has one statistical `vos`, `Tlv755p` one `vref`, and `Stm32Adc` two (gain, offset). Costs per run: ~1 ms op/AC, 10–40 ms transient (plan §4.1, option C §4.4).

| Check | Setup | Knobs → cone | Runs | Time |
|---|---|---|---|---|
| GainStage: gain, bandwidth, input_z, output_z, supply | Operating (one run set, four analyses per run) | 6 parts + 5 setup (vin.z, vout.g, vout.c, vdd.v, ambient) = 11 | 2^11 = 2,048 | ~2 s |
| GainStage: settling | InputStep (tran) | 11 | 2,048 | 20–80 s |
| Ldo3v3: output | Operating | 3 parts + 5 setup = 8 | 256 | 0.3 s |
| Ldo3v3: psrr | anonymous, vin.v pinned | 7 | 128 | 0.1 s |
| Ldo3v3: dip, start | LoadStep, PowerUp (tran) | 8 each | 512 | 5–20 s |
| Board: connection checks | — | — | 0 + ~8 context runs | — |
| Board: sensitivity, flat | AtZero + AtFull | DC cone: 14 of 19 (the capacitor rule drops 5 capacitors) | 2 × 16,384 = 32,768; **over the 2^12 budget** | UNDECIDED (budget) in M3 |
| same, slots merged into one DC sweep | — | 14 | 16,384 | still over budget |
| Board: wake_noise, flat | McuWakes (tran) | ~19 | 2^19 × 40 ms: out of reach | needs the loop (~300 runs, ~12 s) |

**What blows up, in order:**
1. **Board specs over a chain.** Every board spec here has the LDO, the amp and the ADC in its cone. Cones don't shrink, and the topological cone rule (E4) doesn't help on a single chain. Composition (rung 3: a DC transfer table for `sensitivity`) or the loop is the only way.
2. **Multi-setup specs** multiply every point by *k*. The DC-sweep merge (§4.3) takes it back to 1 when the slots differ in one source value.
3. **Transients** dominate child time: 2,048 settling runs are 10–40× the AC runs.
4. **Interface knobs.** Each written range adds a knob to the child: five here. `Load.r` and `Load.c` are also axes where the worst case can sit inside the range (phase dips, [EE] §3.3), which the guards must search.
5. **Generics** are cheap unless a design places many distinct argument tuples. Each is a new table.

The children stay well under a minute, and are computed once per key.

---

## 8. Problems, and what to restrict

| # | Construct | Why it's unsound or impossible | Restriction |
|---|---|---|---|
| **P1** | An `accepts` field left unwritten (`vdd` has no `z`) | If unwritten means "anything", the two-way check passes on no information, and the child's verdict is reused against a 10 Ω rail it never saw | **Unwritten = ideal**: a supply's `z` = 0, an input's source `z` = 0, an output's load open. The check then fails honestly, and the fix is to write the range |
| **P2** | Whole-shape overrides (`sensor: Signal { v: 0V }`) | If the override replaces the shape, `z` silently becomes ideal in `AtZero` and the two slots see different source impedances | **Field-by-field merge**; the formatter or `--explain` prints the effective setup |
| **P3** | A setup that reaches into a child (`mcu.load: Load { step … }`) | The child's table is keyed by its definition. A parent that changes its internals invalidates every cached result, and a port-level two-way check can't see it | Setups write only the block's own ports, `temp` and globals. A child model may declare an **activity** (a supply draw with a step) as part of its interface, which the parent's setup selects by name |
| **P4** | `require vout.z(f) <= 100Ω for f in 10Hz..=100kHz` | A continuous quantifier checked on a 50/decade grid misses a peak between points | Lower to `Band { reduce: Max }`, print the grid, and let the E1 interior check refine around the worst grid point. A narrow resonance still needs a declared density |
| **P5** | Quantized measures (`mcu.code`, `deviation()` of a code) | A step function: the tangent check, the audit and the σ nudges assume a smooth measure | `MeasureKind::Quantized`: `worst_case` by enumeration only; `sigma(k)` is computed on the analog value before quantization, or UNDECIDED (numerics). Better: specs on the analog input, with the code as a derived measure |
| **P6** | `v.at(end)` and transient windows | `end` depends on a stop time the text doesn't give. A short window reads an unsettled value as the final one | `final` = a DC operating point at the settled stimulus (one op run), not the transient's last sample. Stop time and window are setup fields, printed. "Never settles" is `Beyond` (E6) |
| **P7** | `temp: ambient` in a library block | The block's meaning, and its cache key, depend on the project it's used in | Allowed, but library blocks should write their own range. The containment check then compares the project's `ambient` with it |
| **P8** | `rated temp within …`, `rated vin.v within …` | Absolute maximums are not specs over the envelope; they must hold in every run, faults included | Lower to **monitors** on every run of every setup (option C's `PortMonitor`), plus a static check that each setup's range lies inside the rating. A violation is a FAIL with its run |
| **P9** | Generic setups (`LoadStep<Self>`) with values from a trait | The step size and edge come from the library, not the text | Monomorphize at resolve; the report and `--explain` print every value used. A generic setup's defaults are part of its key |
| **P10** | Two setups with no common base in one spec | Only parts and globals are shared: usually not what's meant | A lint, and the joins in the record (§3.3) |
| **P11** | Port facts at one frequency (`vin.z(1kHz)`) | A parent needing Z at 50 kHz can't use it | The check says `Undecided(frequency not covered)` and falls back. Facts over a band are the useful form |
| **P12** | `const GAIN: f64` | Unitless by declaration; `GAIN ± 1%` works, but `const R: f64` used as ohms would pass unchecked | Const generics take a unit type (`const GAIN: Ratio`, `const R: Ohm`), checked like fields (E11) |
| **P13** | Unbounded ranges (`r: 10kΩ..`) | "Up to open" has no finite ε mapping | Lower as a conductance from 0 (§3.1). For measures that get worse with a *lighter* load (stability), the open end is a real corner, so it must be in the box |

---

## 9. The MVP slice

What M1d-5, M1e/M1f and M3 build now so `ce_amp.spl` runs and nothing is rewritten when the rest arrives.

### 9.1 Language and elaboration (M1d-5)

**In:**
- `block X { port: Type, … }` (interface only; no generics).
- Exactly **one** `impl Circuit for X` per block (keyword provisional). Two is an error "several implementations need a choice at the placement (not yet)".
- `setup S for X { port: Supply { v: … }, temp: <global or range> }`, **no inheritance** yet. Shapes: `Supply { v, z }`, `Signal { z }`, `Load { r, c }`, all fields optional with the ideal default (P1).
- `impl Contract for X { accepts S; spec name(env: S) { let …; require …; } }`. **Every spec's setup is the `accepts` setup.** Body: `let`s and one or more `require`s with `within`, `<=`, `>=`. Measures: `dc`, `ac`, `.at`, `.mag`, `.f_low`, `.f_high`, `.db` (E24's table).
- `global name: Type in range;` as range knobs. `global confidence` can wait.

**Built as seams, used with one element:**
- `Design { blocks, impls, setups, contracts, globals }`; `Block.impls: Vec<ImplId>`.
- `SpecDef.params: Vec<…>`, `MExpr::Probe { slot }` (always 0).
- `SetupDef.base: Option<…>` (always `None`).
- `KnobSource::{Field, SetupField, Global}`.
- `FlatContract.setups: Vec<FlatSetup>` (one); `SpecPath` on every spec.

### 9.2 Engine (M3a types, M3f adapter)

- `SetupId` on `Request` and in the run-table key (one setup, `S0`).
- `Plan.setups` and `Plan.experiments` (one each); `Program` probes carry a slot.
- `Needs.analyses: Vec<AnalysisSpec>` with `Op` and `Ac` only; `Tran` and `DcSweep` exist as variants and return "not in M3".
- Measured values in `Evaluations` keyed by `(MeasureId, Key)`, filled from single rows.
- `KnobSpec.origin`.
- The cache key's shape (§6.2) as `rev`, with no child entries yet.

**Not in the MVP:** inheritance, multi-setup specs, `on`, pins with `at`, generics, several impls, published facts, connection checks, child tables, `rated`, transients, generic setups.

### 9.3 The CE amp, filled in

```
ce_amp.spl (draft 2)
  block CeAmp { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }
  impl Circuit for CeAmp { … 6 parts … }
  setup Operating for CeAmp { vcc: Supply { v: 12V ± 5% }, temp: ambient }
  impl Contract for CeAmp { accepts Operating; spec bias(env: Operating) {…} gain {…} bass {…} }
  (project) global ambient: Temperature in -10°C..=60°C;
```

**Resolve → `Design`**

| Item | Value |
|---|---|
| `Block#0 CeAmp` | 4 ports; `impls: [Impl#0]` |
| `Impl#0` | 6 nets (4 port nets first, `base`, `emitter`), 6 part instances: today's `Block` contents unchanged |
| `SetupDef#0 Operating` | for `CeAmp`, base `None`; `vcc`: `Supply { v: Given(12 V ± 5%), z: Unwritten }`; `temp`: `Global#0`; `input`, `output`: unwritten → ideal source, open |
| `Global#0 ambient` | `Range(263.15 K ..= 333.15 K)` |
| `Contract` | `accepts: SetupDef#0`; 3 specs, each `params: [("env", Def(#0))]` |
| `bias` | `requires: [Require { measure: Call(Dc, [Probe { slot: 0, output.v }]), relation: Within, bound: 4.5 V ..= 6.5 V }]` |
| `gain` | `Call(Mag, [Call(At, [Call(Ac, [Probe{0, output.v} / Probe{0, input.v}]), 1 kHz])])` within 4.6 ± 5% = 4.37 … 4.83 |
| `bass` | `Call(FLow, [Call(Ac, […]), −3 dB])` `<=` 30 Hz |

**Flatten → `FlatDesign` + `FlatContract`**

| Knob | Source | Kind | lo … hi | nominal |
|---|---|---|---|---|
| `ambient` | `Global#0` | Range | 263.15 … 333.15 K | 298.15 K |
| `vcc.v` | `SetupField { owner: Operating, Port(vcc).v }` | Range | 11.4 … 12.6 V | 12 V |
| `r1.value` … `re.value` | `Field` | Statistical | ± 1% | 47k, 10k, 4.7k, 1k |
| `c_in.value` | `Field` | Statistical | ± 20% | 1 µF |
| `q1.beta` | `Field` | Statistical | 100 … 300 | 200 |

`FlatSetup#0`: knobs all 8; pins none; bench `[V_vcc DC {vcc.v} on vcc, V_input AC 1 on input]`, output open, `vcc.z` ideal. Specs: `CeAmp.bias`, `CeAmp.gain`, `CeAmp.bass`.

The knob list is model.md §5's, with `temp` renamed `ambient` (the global's name; the target is still the circuit temperature). The suite's case JSON changes that one path.

**Adapter → `KnobSpace` + `Plan`**

| Type | Value |
|---|---|
| `KnobSpace` | 8 knobs; origins Global, Interface, Part × 6 |
| `Plan.setups` | `[Setup { id: S0, name: "CeAmp::Operating", deck: D0, knobs: k0…k7, pins: [], needs: { probes: [v(output), v(input)], devices: [q1], analyses: [Op, Ac { Source(input), Points [1 kHz] }, Ac { Source(input), Sweep 0.1 Hz–100 kHz, 50/dec }] } }]` |
| `Plan.experiments` | `[Experiment { slots: [S0] }]` |
| `Plan.measures` | m0 `Probe{0, v(output)}` Smooth · m1 `Mag(AcAt{0, output/input, 1 kHz})` Smooth · m2 `FLow{0, −3 dB}` Kink |
| `Plan.sides` | `CeAmp.bias#0` Max 6.5 · `CeAmp.bias#0` Min 4.5 · `CeAmp.gain#0` Max 4.83 · Min 4.37 · `CeAmp.bass#0` Max 30 Hz: 5 sides |
| cones | bias: 7 (capacitor rule drops c_in); gain, bass: 8 |
| deck `D0` | engine_plan §1.4's deck, unchanged: `.temp {k0}`, `V_vcc vcc 0 DC {k1}`, `V_input input 0 DC 0 AC 1` |
| `Request` (corner 102) | `{ point: ε(−1,+1,+1,−1,−1,+1,+1,−1), setup: S0, analyses: {op, ac}, tolerance: Engine, care: None }` |
| run-table key | `(S0, bits of the cone's physical values)` |
| `Evaluations` | `(m0, key₇) → 6.5595 V`, `(m1, key₈) → 4.4748`, `(m2, key₈) → 17.77 Hz` |
| `Record` | `CeAmp.bias#0` max, `worst_case`: FAIL at 6.5595 V; setups `["Operating"]`; counterexample `ambient −10 °C, vcc.v 12.6 V, …, c_in any` |
| cost | 579 runs, 0.4 s: the plan's numbers, unchanged |

---

## 10. The path

| Step | Adds | First circuit | Engine seams used |
|---|---|---|---|
| **1. Now** (M1d-5, M3a, M3f) | §9: items, one setup, single-setup specs, globals | CE amp | `SetupId`, slots, `Evaluations`, `SpecPath`, origin |
| **2.** | Setup inheritance (field-wise), pins (`at`), `Tran` and `DcSweep`, windows and `final` (P6), multi-setup specs with `on` and the DC-sweep merge | LDO (`psrr`, `dip`, `start`); a two-point calibration | T1, E5, E6, E7 |
| **3.** | Generics (monomorphized), `rated` as monitors, child contracts monomorphized and kept | GainStage alone | E1 kinds, monitors |
| **4.** | Published facts, connection checks (rung 1), the block-scoped run table under `BlockKey` | Two-stage CE amp ([H] H1) | `rests_on`, `Record.status` |
| **5.** | Several impls with a placement choice; ABCD and DC tables (rung 3); the loop for board specs | Sensor board | the loop (E4), composition |

Each step adds variants and fills vectors that already exist. None changes a type that step 1 shipped.

---

## 11. Open questions for the lead

1. **Inherited range knobs shared across the slots of one spec** (§3.3): accept "inheritance means the same value", with the joins printed?
2. **Unwritten `accepts` fields mean ideal** (P1): accept? It makes every current example fail its two-way check until ranges are written, which is the honest result.
3. **Activities instead of setup fields inside children** (P3): is a child model's declared activity (the MCU's supply step) the right replacement for `mcu.load`?
4. **`final` as an operating point** (P6) rather than the last sample of the window?
5. **The knob path of a global:** `ambient` (the global) or `temp` (its target)? This review uses `ambient`, which changes one path in the suite.
