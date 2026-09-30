# Flatten: decisions for the user

> 2026-09-28 · The open questions from the `/stage-review flatten` round. Each has:
> - a worked example;
> - what the references do;
> - the options, with the code each one gives;
> - the pros and cons, and what each costs;
> - a recommendation.
>
> **Sources.** The six review reports and one reference round. `[V]` means read in the source at file:line, `[M]` means measured (ngspice-42), `[R]` means recalled but not checked. The fetched sources are in the session scratchpad's `ref/` and `ref2/`.
>
> **The code as it stands** (applied, not committed): `crates/spicy_model/src/{flat.rs, flatten/}`.

There are 15 questions in three groups:
- **A. The data model** (A1–A6): what the flat design stores.
- **B. How problems read** (B1–B3).
- **C. What the checks mean** (C1–C6).

**Status (2026-09-29): all decided and applied**, with the recommendations taken, except A4 (deferred). Where the implementation refined a recommendation:
- **C3:** counting "per port" first cut the circuit into levels at the power ports (Modelica's connection sets). That counted a supply's regulator twice when the supply also ties a plain `sense` pin to its output. The rule applied is KiCad's: **only the innermost members of a net count** (a placed block's port with another member inside the block is passing it on). Where two sources meet is found by making the net's joins again, each placement's inside before the placement: `o2: seven` for two outputs of one block tied on a board, `out: v5` inside a block that ties two regulators. The second review round made "innermost" per port rather than per placement (an output with nothing behind it next to one fed by a regulator is two sources), reported each meeting once per placement instead of once per join (which had been quadratic), and took each "comes in" span from the join itself.
- **B2:** the same `innermost` rule lists the sinks. The error points where the first innermost sink comes in at the net's top (`vcc: rail`), not deep inside the block that takes the power.
- **B1:** every problem inside a block has a scope, the placement it's in: for sources, the join where they meet; for an unpowered net, its top; for a shorted part, its placement; for isolated nets, the placement they're all in. Paths are written from the scope, and the grouping counts that block's placements (over every root for flatten's own checks, over the one root for a simulation check).
- **C1:** `FlatDesign::grounds` holds the facts, and `FlatDesign::tainted` the proof that it's broken. `check_simulation(flat, map, errors) -> Result<FlatNetId, Reported>` is what lowering calls first. The tests call it on every root, so every error kind still has a case.
- **A6:** a required field not written is `FieldValue::Invalid(Reported)` too, not `Unset` (three resolve snapshots show `value = <invalid>`). The first of two blocks with one name is tainted with the duplicate's proof (C4 (c)).
- **C5:** a root past the limit isn't in `Flattened::roots`; its `E-size` error is at the placement that went over.

**A note for the engine docs** (A2/A4; they're another session's, so not edited here): `KnobTable` and `FlatRoot` no longer exist. The knobs are `FlatDesign::knobs`, and a knob's path is `Flat::knob_path(k)`, built from `KnobSource`. The planned signatures `engine_deck(design: &FlatDesign, knobs: &KnobTable, …)` (engine_plan.md:549) and `from_model()` (engine_types.md:34) would take one `Flat<'_>`. Also affected: engine_plan.md:23, 29, 92, 597, 609, 869, 877; engine_types.md:4, 34, 72, 101, 112; engine.md:57; research/engine_flows.md:77, 104, 108, 145, 222, 267, 290, 327. A4's recommendation (reuse `KnobId`, `HierPath` and `KnobKind` from `spicy_model`) stands for that session to take up.

---|---|
| **Decided** | A1 (ids only, read through one `Flat<'_>` handle), A6 (a `Reported` token where `None` meant "broken") |
| **Follows from those** | A3 (a stored knob path is a copied name, which A1 removes), C4 (the taint becomes rustc's `tainted: Option<Reported>`; marking the first of two same-named blocks is a fix) |
| **Clear; the recommendation is applied unless you say otherwise** | A2, A5, B2, B3, C2, C5, C6 |
| **Needs you** | B1 (how every problem in a reused block reads), C1 (rewritten: it's wider than "no parts", and it decides the type of `ground`), C3 (makes a design that passes today an error) |
| **Deferred** | A4: only the engine docs, which another session owns; a note for it, no code now |

---

## A. The data model

### A1. Names copied into the flat design, or looked up in the `Design`?

**Now.** `FlatInstance.name`, `FlatDevice.name` and `NetName.name` are `String`s copied from the `Design` (flat.rs:76, 99, 106). A path is built from them: `flat.path(at)`, `flat.net_path(net)`, `flat.device_path(d)`.

**The alternative.** Store only ids and look each name up through the id the flat item already holds: `origin: (parent, InstanceId)` for placements and devices, `(at, NetId)` for local nets.

```rust
// now
pub struct FlatInstance { pub name: String, pub block: BlockId, pub origin: Option<(FlatInstanceId, InstanceId)> }
pub struct NetName     { pub at: FlatInstanceId, pub net: NetId, pub name: String }
let p = flat.path(at);                       // needs only the FlatDesign

// ids only
pub struct FlatInstance { pub block: BlockId, pub origin: Option<(FlatInstanceId, InstanceId)> }
pub struct NetName     { pub at: FlatInstanceId, pub net: NetId }
let p = flat.path(&design, at);              // the name of `let` i in the parent's block
```

**Who reads names:**
- the dump and the error messages;
- lowering, for SPICE node and device names;
- the engine, for report rows. Its knob names are stored in `Knob.path`, so it doesn't need these.

Every one of them already holds the `Design`:
- `FlatDesign` refers to it by `BlockId`, `InstanceId` and `NetId`;
- the root block's name is only in the `Design` (the dump reads `design.block(flat.root).name`);
- port types are only in the `Design` (the checks read them there).

**References:**
- **Pointing back to definitions.** Compilers keep pointers and build the string only when they print:
  - slang holds `string_view name` plus `parentScope`, and `getHierarchicalPath()` computes the path on demand (`Symbol.h:147,157,180`) [V];
  - OpenModelica builds a flat name from an `InstNode` pointer plus a shared `restCref` prefix, so there is one cell per level, shared (`NFComponentRef.mo:70-78`) [V];
  - rust-analyzer uses item-tree ids plus interned names [R].
- **Copied strings:** Yosys (`IdString`, made unique, plus the `hdlname` attribute, flatten.cc:45-105), CIRCT (`inst_`-prefixed names, whose separator is ambiguous), KiCad's SPICE exporter, ngspice (`r.x2.r1`) and Xyce (`X1:PARAM`) [V]. They copy because they throw the hierarchy away afterwards, and the simulator or file format consumes strings.

| | Copied names (now) | Ids only |
|---|---|---|
| Reading a path | `flat.path(at)` | `flat.path(&design, at)`: one more argument at about 15 sites |
| Memory, speed | one `String` per placement, device and local net | measured: flatten 6–30% faster, 55–70% fewer allocations |
| Where names live | in two places: the `Design` and the copy | in one place |
| `FlatDesign ==` after a net rename | notices it | doesn't; a cache must also depend on the `Design` (salsa does that automatically) |
| Reading a flat design without the `Design` | possible for names, but not for anything else (ids, port types) | not possible, as today |

**Cost:** about 15 call sites, plus two planned signatures in the engine docs (`engine_deck`, `from_model`) that gain `&Design`. No crate-layering change: the `Design` lives in `spicy_model` too.

**Recommendation: ids only.** It's the project's rule that "names are looked up only at the edges" (pipeline.md §1; model.md §9 "ids, not copied spans"). No reader of a `FlatDesign` exists without the `Design`, and it's measurably faster.

**Decided (2026-09-29): ids only, read through one handle, rustc's shape.**
- rustc's monomorphized item is `Instance { def: InstanceKind, args }`, with no name (`instance.rs:33-36`) [V]. Names come from the context: `tcx.def_path_str(def_id)` (`collector.rs:483, 679`) [V]. A path is rebuilt from `DefKey { parent: Option<DefIndex>, … }` [R].
- So the flat design stays plain data, and one `Copy` handle pairs it with its `Design`, as `TyCtxt` does. It is made once, so a flat design can't be read against the wrong `Design`:
  ```rust
  #[derive(Clone, Copy)]
  pub struct Flat<'d> { pub design: &'d Design, pub data: &'d FlatDesign }
  impl Flat<'_> { pub fn path(self, at: FlatInstanceId) -> HierPath { … } /* net_path, device_path, knob_path */ }
  lower(flat); flat.path(at)          // not flat.path(&design, at) at every site
  ```

---

### A2. Knobs inside `FlatDesign`, or next to it?

**Now.**
```rust
pub struct Flattened { pub roots: Vec<FlatRoot>, pub errors: Vec<FlattenError> }
pub struct FlatRoot  { pub design: FlatDesign, pub knobs: KnobTable }
pub struct KnobTable { pub knobs: Vec<Knob> }
root.knobs.knobs[k]                          // the stutter, at 5 sites
```
**Folded.**
```rust
pub struct Flattened  { pub roots: Vec<FlatDesign>, pub errors: Vec<FlattenError> }
pub struct FlatDesign { /* …, */ pub knobs: Vec<Knob> }
flat.knobs[k]
```

**Why they were split** (pipeline.md §3, E17): one `FlatDesign` serves every run, and each run only changes numbers.

That reason doesn't need a separate struct:
- `KnobTable` holds the *written* `Value`s, which change exactly when the design does.
- The per-run numbers are the engine's point in knob space, which is outside both.
- model.md §4 already puts `contract` inside `FlatDesign`, even though the `assume` knobs (M1d-5) come from the contract.

**References:**
- **Xyce** keeps random parameters in the flat circuit's own parameter table, as globals named `X1:param` (`N_IO_CircuitContext.C:1072-1112`) [V].
- **OpenModelica**'s flat model holds parameters as ordinary variables of the flat class, with their variability (`NFFlatten.mo:657-751`) [V].
- **Spectre** declares statistical variables in a `statistics` block, apart from the netlist, but inside the same design file [D].

None of them keeps a separate object next to the flat netlist for the values that can vary.

| | Keep the pair | Fold |
|---|---|---|
| Types | `FlatRoot`, `KnobTable`, `FlatDesign` | `FlatDesign` |
| Call sites | `root.design.…`, `root.knobs.knobs[k]` | `flat.…`, `flat.knobs[k]` |
| Docs | as written: about 15 lines name the pair (pipeline.md:80,183; engine_plan.md; engine_types.md:34,72,101,112; model.md:24,31,369,453; roadmap.md:490) | those 15 lines change |
| The engine's input | `(&FlatDesign, &KnobTable)` | `&FlatDesign` |

**Recommendation: fold, and update the docs in the same change.** Two fewer types, and the split has no reason left.

**Not recommended:** keeping the pair only because the docs name it. The docs were written before either type existed.

---

### A3. A knob's path: stored, or built when it's needed?

**Now.** `Knob { path: Path, value, kind }`. The path is stored in full: device path plus field.

**The problem:** the path grows with depth, so a design with a toleranced part at every level is quadratic.

| Nested levels, each with a toleranced part | Time |
|---|---|
| 1,000 | 19 ms |
| 2,000 | 79 ms |
| 4,000 | 408 ms |
| 5,000 | 1.76 s, with about 106 KB of path per knob on average |

With the path built on demand, the same designs take 0.25, 0.51 and 1.09 ms (1,000 / 2,000 / 4,000 levels), linear. Ordinary designs also get 12–42% faster, with 70–77% fewer allocations; a knob drops from about 200–700 B to about 90 B.

**The alternative.** A knob keeps where it came from, and its path is built from that:
```rust
pub struct Knob { pub source: KnobSource, pub value: Value, pub kind: KnobKind }
pub enum KnobSource {
    Field { device: FlatDeviceId, field: usize },  // `left.r1.value`
    // M1d-5: Assume { … } for `vcc.v`, and Temp
}
let path = flat.knob_path(&design, k);           // device path + the field's name
```

**References:**
- **Spectre:** a statistical variable is declared once. A mismatch variable's identity is its instance path plus the parameter, and it's drawn "a different value … for every subcircuit instance" [D, O'Riordan's methodology paper]. Instances are named by path in `correlate dev=[R1 R2]`.
- **Xyce** stores the path per parameter as a string. Its own comment admits the cost: "the entire subcircuit must be re-resolved every time it is expanded" (`N_IO_CircuitContext.C:1094-1101`) [V].
- **ngspice** bakes each drawn value into the flat device. No named variable exists, so a knob can't be named or replayed [M].

| | Stored path (now) | Built from its source |
|---|---|---|
| Deep hierarchies | quadratic | linear |
| A knob's name outside flatten (engine, reports, `--at`) | read directly | built through `flat.knob_path(&design, k)` |
| M1d-5 knobs (`vcc.v`, `temp`) | `path` is just set | `KnobSource` gains two variants |
| The stable identity (E21) | the path | still the path, only computed |

**Recommendation: build it from its source.** It's the same fix that took `FlatInstance` from 20 s to 4 ms. If A1 goes to "ids only", the path functions already take `&Design`.

---

### A4. The planned engine types: reuse the model's, or define their own?

**Now (docs only; `spicy_engine` doesn't exist yet).** engine_types.md §2.1 and engine_plan.md §6.2 define `spicy_engine::{KnobId, Path, KnobKind { Range, Statistical(Dist) }, KnobSpec { id, lo, hi, nominal, … }}`.

That repeats three of the model's types: `KnobId`, `Path` and `KnobKind`. The engine already depends on `spicy_model` (engine_types §12), and the model depends on nothing.

**Options:**
- **(a) Reuse the model's types.**
  - Use `spicy_model::flat::{KnobId, KnobKind, Path}`, and drop `KnobSpec.id`, which is the knob's own index.
  - Keep `KnobSpec`'s SI `lo`/`hi`/`nominal`: those are numbers derived for the engine, genuinely different from a `Value`.
  - T1's wording "the core never takes spicy_model types" narrows to "never the flat design or the contract".
- **(b) Keep the engine's own types.** The engine core stays free of the model, at the cost of converting at the boundary and keeping two copies in step.

**References:**
- rustc shares one index type crate-wide (`rustc_index`) rather than one copy per pass.
- slang uses one `Symbol` hierarchy for elaboration and analysis.

Where a boundary really wants isolation (an FFI, a file format), it gets its own types.

**Recommendation: (a).** `Statistical(Dist)` also repeats the engine's `Policy`, which T2 says picks the distribution from the kind. This touches only the engine docs, which the other session owns. I'd write the change as a note for that session rather than edit its docs.

---

### A5. Rename `flat::Path` to `HierPath`?

**What `flat::Path` is.** The name of one thing in the flattened circuit, as the list of placement names from the root down to it (`Path(pub Vec<String>)`, flat.rs:34), printed joined with `.`. In `Stereo` (`test_data/flatten/ok/stereo.spl`), which places `Amp` twice, as `left` and `right`:

| Path | Names |
|---|---|
| `left` | a placement |
| `left.r1` | a device (becomes a SPICE element name in lowering) |
| `left.base` | a net (in messages, and as a SPICE node name) |
| `left.r1.value` | a knob: its stable identity (E21), what `--at left.r1.value=1.02k` and a replayed run match against |

With A1 it's built on demand by `flat.path(at)`, not stored.

**Now.** There are two types called `Path`:
- `ast::Path` (parser): a `::` namespace path of borrowed identifiers with spans, like `a::b`;
- `flat::Path` (model): a `.` hierarchy path of owned segments, like `left.r1`.

They never meet in one file today. The engine and contract code will both need the hierarchy one.

**References:**
- rustc keeps `ast::Path` and `DefPath` apart by name;
- slang calls the second kind a "hierarchical path" (`getHierarchicalPath`);
- CIRCT has `hw.hierpath`.

**Options:**
- **(a) Rename to `HierPath`.** It touches two source files and a few doc lines.
- **(b) Keep `Path`,** and qualify it where both appear.

**Recommendation: (a), mildly.** Do it now, before the engine and contract code start saying `Path`. It's cheap, and it's the name the references use.

---

### A6. `None` for "missing" and `None` for "broken": a `Reported` token

**Decided 2026-09-29.** Today `Option` means two things in the flat design:

| Field | What `None` means | An error? |
|---|---|---|
| `FlatInstance.origin` | the root: nothing placed it | no. Like rustc's `DefKey.parent`, `None` only for the crate root [R] |
| `FlatField::Unset` | not written; lowering applies the default | no |
| `FlatDevice.pins[i]` | the binding was missing, or not a net | yes, always, reported by resolve |
| `FlatField::Invalid` | written, but wrong | yes |
| `FlatDesign.ground` | 0 or 2+ ground nets | yes today; C1 decides whether it should be |
| `Block::has_errors: bool` | the block had an error | it's the error flag |

rustc uses `Option` only for real absence. For "broken" it uses `ErrorGuaranteed`, a zero-size proof that can only be made by reporting an error (`rustc_errors lib.rs:433-437`) [V]:
- `Result<(DefKind, DefId), ErrorGuaranteed>` in typeck's tables (`typeck_results.rs:37`) [V];
- `tainted_by_errors: Option<ErrorGuaranteed>` (`typeck_results.rs:167`) [V].

model.md §8 planned this as a `Reported` token, "until several stages create placeholders"; resolve and flatten now both do.

```rust
pub struct Reported(());                                   // no public constructor
impl<K: DiagKind> Diag<K> {
    pub fn report(self, sink: &mut Vec<Diag<K>>) -> Reported { sink.push(self); Reported(()) }
}
pub struct FlatDevice { …, pub pins: Vec<Result<FlatNetId, Reported>> }
pub enum   FlatField  { Unset, Exact(Quantity), Knob(KnobId), Invalid(Reported) }
pub enum   InstanceOf { Part(PartKind), Block(BlockId), Error(Reported) }
pub struct Block      { …, pub tainted: Option<Reported> }  // replaces has_errors
```

- A later stage has to meet `Err(Reported)` to get past a pin, so lowering can't quietly simulate a broken circuit (rustc: no codegen after an error, `passes.rs:1304-1309`) [V].
- A broken design still flattens, so the editor can show it (E7).
- **Cost:** about 20 sites, where `errors.push(d)` becomes `d.report(&mut errors)`. No speed effect: the token has no size.

---

## B. How problems read

### B1. A problem in a block placed several times: what does the message say?

**Now** (after the grouping fix), one message per problem, with a count:
```
error[E-power]: nothing powers net `a.rail` (in 2 placements)
```
The path shown is the first placement's.

**References:**
- **slang** is the closest.
  - When the problem is in **every** placement, it prints once with **no path** at all: it's the definition's problem (`ASTDiagMap.cpp:96-108`) [V].
  - When it's in **some** placements: `in {N} instances, e.g. <path>` (`TextDiagnosticClient.cpp:74-78`) [V]. The example is the last placement not directly under the root, because a one-level path "seems silly" (`ASTDiagMap.cpp:35-36, 78-83`).
  - No reference prints "N of M".
- **rustc:** a generic's error is reported once, at the definition [R]. After monomorphization, each instance gets its own error plus "the above error was encountered while instantiating …" (`mono_diagnostics.rs:81`) [V].
- **KiCad** puts one marker per sheet instance, and **atopile** one error per instance with its path (`errors.py:626-635`) [V]. Neither prints a count, and both flood the list.

**Options:**
- **(a) slang's rule.**
  - In every placement: no path.
    ```
    nothing powers net `rail` (in `Mid`)
    ```
  - In some placements:
    ```
    nothing powers net `rail` (in 2 of 3 placements of `Mid`, e.g. `board.a`)
    ```
  - The "of 3" goes one step past slang. It's cheap: flatten knows how many placements a block has.
- **(b) Now:** "(in N placements)" plus the first placement's path.
- **(c) A note listing every placement.**

| | (a) | (b) | (c) |
|---|---|---|---|
| Says whether it's the definition's problem | yes | no | yes, if you count |
| Length | short | short | grows with placements |
| Which placements | one example, plus a count of how many | the first only | all |

**Recommendation: (a).** Paths inside the message (`a.rail`) become paths relative to the block (`rail`), and the placement goes in the "in …" part.

---

### B2. "Nothing powers net …": which sinks does it list?

**Now.** It lists every sink on the net. With pass-through levels that includes each one, for example `s.v12` *and* `s.left.vcc` for one physical load.

**References:**
- **KiCad** puts one marker per net by default, on the first power-in pin sorted by reference and then pin number. A "show all errors" option marks every pin (`erc.cpp:1765-1776, 2009-2022`; `dialog_erc.cpp:245-247`) [V].
- Sheet pins are never listed: only real pins are.
- The error already points at the first sink, like KiCad's default.

**Options:**
- **(a) Only the innermost sinks:** the ones that really take power (`s.left.vcc`). Pass-through ports are dropped, as KiCad lists pins, not sheet pins.
- **(b) All sinks, as now.**
- **(c) The first sink, plus "and N others".**

**Recommendation: (a).** A pass-through port isn't a load. This is the same "per level" reasoning the source count already uses.

---

### B3. Message edits

Each of these changes a snapshot line, and none is applied yet:

1. **`SeveralGrounds` names its root,** as `NoGround` does: "`TwoGrounds` has more than one ground net".
2. **Rename `TwoSources` to `SeveralSources`,** since it means "two or more", like `SeveralGrounds`.
3. **A clearer `NoSource` help.** The "it" in "bind it to a `Power<In>` port of this block, or to a `Power<Out>`" is ambiguous. Proposed: "connect `rail` to a `Power<In>` port of this block, or to a placed block's `Power<Out>`".
4. **Cap `IsolatedNets`' list** in rustc's "and N others" style. On the 100,000-part benchmark the message lists about 100,000 nets.
5. **Optional:** match resolve's label words ("second definition" / "first defined here") in "a second source" / "the first source".

**Recommendation: 1–4 yes. 5 optional; I'd leave it,** since the power labels already read naturally.

---

## C. What the checks mean

### C1. Which roots must be a complete circuit? (rewritten 2026-09-29)

The first version asked only about a root with no parts. The same false error hits any correct library block that nothing places in its own file, because under E3 every unplaced block is a root, and today every root must have exactly one ground.

**Example** (a library file; nothing in it places these blocks, so each is a root):
```
block Connector { port a: Pin; port b: Pin; }                                   // no parts
block Snubber   { port a: Pin; port b: Pin; net m;                              // an RC across two pins
                  let r = Resistor { a, b: m, value: 100 }; let c = Capacitor { a: m, b, value: 10n }; }
block Adc       { port agnd: Ground; port dgnd: Ground; … }                     // two grounds, joined by the board
```
Today:
- `Connector` gets `E-ground`, no ground.
- `Snubber` gets `E-ground`, no ground. This is the `NoGround` case in `test_data/flatten/err/ground.spl`: two `Pin`s and a resistor.
- `Adc` gets `E-ground`, several grounds.

All three blocks are correct. They only become part of a circuit once a board places them (`adc = Adc { agnd: gnd, dgnd: gnd, … }`).

**References:**
- **Modelica** has two tiers [V]:
  - "All non-partial model and block classes must be locally balanced" (`mls_ch4.txt:2287`). That's a check of each class on its own, whatever uses it.
  - "All simulation models and blocks are globally balanced" (`:2299`). That holds only for the model you choose to simulate.
  - A `partial` class "cannot be instantiated in a simulation model" (`:406`).
- **KiCad** has no ground check, and neither does **atopile's** ERC (`erc.cpp:350-3185`, `erc.py:413-416`) [V].
- **ngspice and Xyce** need node 0 only in the deck you simulate. Xyce warns "no DC path to ground" there (`N_TOP_Topology.C:1590-1811`) [V]. An empty deck with a `.op` card crashes ngspice-42 on an assertion (`dotcards.c:225`) [M], so lowering must never emit one.

**Our checks sort into the same two tiers:**
- **About the block itself** (true wherever it's used): power sources (`NoSource`, `TwoSources`) and shorted two-pin parts.
- **About simulating it:** exactly one ground (node 0), and isolated nets (ngspice's singular matrix). Neither means anything for a block that isn't simulated.

**Options:**
- **(a) Every root is a complete circuit (now).**
  ```rust
  pub struct FlatDesign { …, pub ground: Result<FlatNetId, Reported> }   // A6: every Err was reported
  ```
- **(b) Skip roots with no parts** (the first version's recommendation). This fixes `Connector` only.
- **(c) Modelica's two tiers.**
  - Flatten runs the block's own checks on every root.
  - The simulation checks run when a root is simulated. That's lowering's precondition (M1e), with `--top` or the editor's "simulate" picking the root. After M1d-5, a root with a contract counts too: E3 says every block with a contract is meant to be checkable on its own.
  - Flatten only records which ground nets it found, as plain data.
  ```rust
  pub struct FlatDesign { …, pub grounds: Vec<FlatNetId> }     // the facts: 0, 1 or several

  /// When a root is simulated: lowering calls it first; the tests call it on every root.
  pub fn simulation_checks(flat: Flat<'_>, errors: &mut Vec<FlattenError>) -> Result<FlatNetId, Reported> {
      let ground = match flat.data.grounds[..] {
          [one] => one,
          [] => return Err(no_ground(flat).report(errors)),
          [_, second, ..] => return Err(several_grounds(flat, second).report(errors)),
      };
      isolated_nets(flat, ground, errors);                       // a warning
      Ok(ground)
  }
  ```

| | (a) every root | (b) skip roots with no parts | (c) two tiers |
|---|---|---|---|
| `Connector` | E-ground | passes | passes |
| `Snubber` | E-ground | E-ground | passes; E-ground only if you simulate it on its own |
| `Adc` (`agnd` + `dgnd`) | E-ground | E-ground | passes; simulating it on its own asks you to tie them |
| A board that forgot its ground | reported while you edit | the same | reported when you simulate it; while you edit once it has a contract (M1d-5) |
| `ground`'s type | `Result<_, Reported>` | the same | `grounds: Vec<FlatNetId>` (facts); the `Result` belongs to lowering |
| Cost | none | one line | two checks move behind one function; the tests call it on every root, so the snapshots stay the same |

**The downside of (c):** until contracts exist, a board that forgot its ground shows no error until you ask to simulate it.

**Recommendation: (c).** It's Modelica's rule, and the simulators check at the same point. A library file of correct blocks has no errors in it. It also settles A6's open case: under (c), a missing ground is a fact about the block, not an error, so it isn't an `Option` that secretly means "broken".

---

### C2. The root's output ports in the isolated-net check

**Example:**
```
block P { port g: Ground; port o: Analog<Out>; let r = Resistor { a: o, b: x }; net x; }
```
Today only the root's `In` ports count as driven by the bench. So `o` and `x`, connected only to each other, are reported as isolated.

**What our language says:** language.md §8.5: "`Power<In>` ports get swept DC sources, signal inputs get the assumed source, **outputs get the assumed loads**." So the default bench does put something on an output: a load to ground, which is a DC path.

**References:**
- **Modelica:** top-level outputs are *observed*, not loaded. An unconnected flow variable gets `flow = 0`, an open circuit (spec §9.2; OpenModelica `NFFlatten.mo:2634-2640`) [V].
- **Xyce** has no top-level ports. It warns "connected to only 1 device terminal" and "no DC path to ground", with an opt-in to add resistors to ground (`N_TOP_Topology.C:1590-1811`) [V].
- **Yosys** `check` counts inputs as drivers and outputs as used (`check.cc:355-366`) [V].
- **ngspice-42** [M]:
  - a node hanging on one resistor simulates fine;
  - an island gives "singular matrix: check node a", then carries on and prints `v(a) = 0`. That's a silently wrong answer, not a failure. model.md §7 is corrected to say so.

**Options:**
- **(a) Every root port with a role counts as connected** (`In` and `Out`, not `Pin`), following §8.5: the bench drives inputs and loads outputs.
- **(b) Only inputs, as now** (Modelica's rule).
- **(c) Outputs count only when the contract assumes a load.** This is the exact rule, but it needs contracts (M1d-5).

**Recommendation: (a) now, and (c) once contracts exist.** Our bench does load outputs, so today's warning is a false positive under our own language. Under (a), a `Pin` port or an internal net cut off from everything is still reported.

---

### C3. Several `Power<Out>` ports of one placed block on one net

**Example:** a regulator block with two outputs, `o1` and `o2`, both bound to one rail. Today the "levels" rule counts a placed block as **one** source to its parent, however many of its outputs are on the net. So this passes.

**References:** all three count per port, not per part.
- **KiCad** checks per pin pair. Two power-out pins on one net are an error by default (`erc_settings.cpp:54`) [V]. "Stacked" pins (the same symbol, position and name) are exempt (`sch_pin.cpp:582-601`) [V]. Differently named outputs are not stacked, so they're an error.
- **Modelica:** "at most … one inside output connector", counted per connector, per level (§9.3, `mls_connectors.txt:1329-1332`) [V]. Two outputs of one sub-block on one set are two sources, so an error.
- **atopile:** each interface marked `is_source` counts; two or more gives "Power sources shorted" (`erc.py:464-484`) [V].

**Options:**
- **(a) Count per port, the references' rule.**
  - Two outputs of one placed block on one net is an error, reported at the second binding.
  - Passing a rail *up* a level (`sup.v5` over `sup.reg.out`) still counts once. That's the levels rule, and it's what KiCad's sheet pins and Modelica's faces do.
- **(b) Count per placement, as now.** More forgiving: a block with two identical outputs meant to be tied together (a dual-output regulator used as one) passes.

| | (a) per port | (b) per placement |
|---|---|---|
| Two regulators behind one placed output | still caught (at the inner level) | still caught |
| One block with two outputs on one rail | error | passes |
| A "tie these outputs" design | needs a way to say so (for example a future `#[allow]`) | passes silently |

**Recommendation: (a).** It's what every reference does. Tying two outputs of one block is either a mistake or a deliberate choice worth writing down.

---

### C4. How much an error in a block silences the whole-circuit checks

**Now.** Any lexer, parser or resolve error inside a block marks it (`Block::has_errors`). A root that places a marked block skips the whole-circuit checks. That includes a harmless mistake like a missing `value:`.

**References:**
- **rustc** taints *per body*.
  - `TypeckResults::tainted_by_errors` (`typeck_results.rs:164-167`) [V];
  - borrowck skips only the tainted body, "Skipping borrowck because of tainted body" (`borrowck_lib.rs:135-137`) [V];
  - every other body is still checked;
  - inside a body, `ty::Error` placeholders stop cascades [R].
- **slang** uses placeholders per expression (`InvalidExpression`, `bad()`), and its driver analysis skips bad ones (`DriverTracker.cpp:32`) [V]. It stops the whole design only for hierarchy problems: depth, recursion (`ElabVisitors.h:330-352`) [V].

**Options:**
- **(a) Per-block taint, as now.** A root is our "body", so this is rustc's rule.
  - It's simple, and a statement the parser dropped can't be marked any other way.
  - The cost: one small error anywhere under a root hides that root's circuit checks until it's fixed.
- **(b) A placeholder at each place resolve drops something,** slang's rule. For example `Merge.with: Vec<Option<NetId>>`, or a `Dropped` statement marker.
  - Finer: an unrelated mistake doesn't hide a real short elsewhere.
  - But every drop site needs its own placeholder, and a parser-dropped statement still needs the taint.
- **(c) (a), plus mark the first definition of a block that's defined twice.** Today only the second one is reported, and the first isn't marked.

**Recommendation: (a) with (c).** It matches rustc. The whole-circuit checks only mean something on a design that resolved cleanly. (b) can come later if an editor needs checks on a broken design.

---

### C5. A limit on how large a flattened design may get

**The problem:** 18 blocks that each place the previous one twice give 262,000 placements in 63 ms. About 30 such lines of source would exhaust memory. The language server will flatten on every edit.

**References** (full table in the research note):

| Tool | What it limits | Default |
|---|---|---|
| rustc | `recursion_limit` (depth) | 128 |
| rustc | `type_length_limit` (size) | 2²⁴, off unless a flag is set |
| slang | `--max-hierarchy-depth` | 128, fatal |
| slang | `--max-instance-array` | 65,535 |
| slang | `--max-generate-steps` | 131,072 |
| Verilator | `--module-recursion-depth` | 100 |
| ngspice | `MAXNEST` | 21, and buggy [M] |

- **No tool limits the total number of instances;** they limit depth or the size of one array or loop.
- **Our recursion check is exact,** so we need no depth limit. Only a size guard would be new.

**Options:**
- **(a) A limit on total flat placements** (for example 1,000,000, configurable later).
  - Past it, one error at the root ("`Top` flattens to more than 1 000 000 placements"), with no flat design.
  - Checked while the tree grows, so memory stays bounded.
- **(b) No limit.** Rely on the editor's timeout.

**Recommendation: (a).** It costs one counter. Its message should name the placement that went over, so you can find the doubling.

---

### C6. Zero spreads: knobs or exact values?

**Example:** `value: 1k ± 0%`, or `beta: 200..=200`. Today each becomes a knob.

**References:**
- **ngspice:** `agauss`/`gauss` return the nominal without drawing when the variation is zero (`xpressn.c:45-64`) [V, M].
- **Xyce `.SAMPLING`** still samples a zero-deviation parameter, and that shifts the random stream for every later parameter (`N_ANP_UQSupport.C:711-736`) [V].
- **No reference normalizes this during elaboration.**

**Options:**
- **(a) Treat a zero spread as `Exact` in flatten.** No knob, no dimension wasted, no shifted stream.
- **(b) Keep it a knob.** The knob set stays stable while you edit (`± 0%` → `± 1%` doesn't add a knob).
- **(c) (a), plus a lint:** "`± 0%` is exact; write `1k`".

**Recommendation: (a).** A zero spread isn't a variation. For (b), the knob set changes anyway when you add or remove a spread.

---

## Confirmed, no decision needed

- **Shorted three-pin parts are not reported.** No reference flags a partial short:
  - atopile checks only R, C and fuses (`erc.py:374-386`) [V];
  - KiCad has no such check;
  - Xyce removes a device only when *all* its terminals are on one node [V];
  - ngspice doesn't warn [M].

  A collector–base short is a valid diode-connected transistor. Our rule, two-pin parts only and as a warning, is the right scope.
- **ngspice on an isolated net:** it warns and returns 0 V, not a failure [M]. model.md §7 is corrected.
