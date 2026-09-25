# Schematic Editor ↔ Language Mapping: Design Proposal

> Working draft · 2026-09-25 · research thread "schematic editor ↔ language"
> Builds on: `../vision.md` (goals), `../archive/engine_v1.md` (v1: D5, D6, §6–7), `../archive/engine_v2.md` (engine), `../specs.md` (specs, §10 editor view), `../walkthrough.md` (the CE amplifier).
> **Syntax disclaimer.** The core syntax is owned by the language thread. Every code sample here is a *minimal Rust-like strawman* chosen to make the mapping concrete. What this document asks of the language is collected in §10 as hard requirements, stated independently of the syntax.

---

## 0. Summary

The editor and the language are designed as one system. Ten decisions:

1. **Code is the only semantic store.** Every fact the engine uses (instances, nets, parameters, knobs, assumptions, specs, probes, setups, waivers) lives in language text. Graphics live in a per-block **layout sidecar** that is *never* semantic: delete it and you lose only aesthetics. This is the **"delete-the-sidecar test"**, and it is the invariant everything else leans on.
2. **Names are identity.** An entity is identified by its hierarchical name path (`board.amp.q1`, `board.amp.base`, `ladder.rs[3]`). No hidden UUIDs in the code. Renames are explicit refactors that update code, sidecars and chat references atomically. Hand renames in text are caught by a rename detector.
3. **Drawn blocks are stored as code, in a restricted "flat" subset**, with graphics in the sidecar. So D6's "export to code" is not a conversion. The drawn block *is* code, formatted to read as if hand-written.
4. **"Drawable" is a syntactic property, not a file type.** A block is drawable when its body uses only the flat subset: declarations, net statements, parameter `let`s, contract items, plus *vectorized* instance arrays (Virtuoso/Altium style). Drawable blocks round-trip in both views. Blocks that use `for`/`if`/computed structure are **generated**: their structure is read-only in the schematic, but values that trace to a single literal stay editable in place, and structural edits are routed to code, optionally written by the AI.
5. **Connectivity is net-centric in the canonical form.** One `net` statement per net, holding a set of pins, in a unique canonical formatting. Wires, labels, rail symbols and junctions are presentation choices stored in the sidecar. Drawing a wire is an edit to one `net` statement.
6. **Every statement in a block body is keyed** (by instance, net, parameter, spec, probe or setup name). Edits from the schematic, the command line, the clipboard and the AI are *name-keyed upserts* of language statements, not line diffs. That is what makes AI edits reliable.
7. **Modes map onto regions of the language.** Edit mode edits structure and values (the body). Simulate mode edits probes, specs, setups and assumptions (the contract). The `:` command line takes language statements, and yank/paste registers hold language text.
8. **Select-to-ask sends a compilable language slice**, plus a rendered image with the same labels, plus results for exactly the entities in the slice. The AI answers with keyed statement patches. The editor applies them to a *proposal* model, re-evaluates the specs, and shows the diff in the schematic together with the verdict change.
9. **Parametric values are "committed value + derivation"** (`4.7k from snap(E24, r_e * gain)`). Cascade and flag are two policies over the same data: cascade rewrites the committed literals as a reviewable patch, and flag leaves them and marks mismatches. The cascade diff is therefore a real text diff, plus a spec-verdict delta.
10. **Auto-placement is a deterministic, convention-driven layered layout** (signal left→right, power up, ground down, recognized analog idioms placed as templates), with user tweaks stored as *pins* keyed by name. Regeneration moves only unpinned, affected entities.

---

## 1. Principles

These are the invariants the rest of the design derives from. Each is testable.

| # | Principle | Test |
|---|---|---|
| P1 | **Semantics live only in code.** | Delete every `.layout` file and recompile. The netlist, the parameter values, the spec verdicts and the SPICE deck are byte-identical. |
| P2 | **Names are identity.** | Any entity can be addressed by a path string that survives reformatting, reordering, moving the part on the sheet, and re-importing. |
| P3 | **Every view edit is a local text edit.** | A single gesture in the schematic (move excluded) changes at most the statements whose keys it touches. Moving a part changes only the sidecar. |
| P4 | **Drawability is decidable from syntax.** | The compiler reports `drawable: yes/no` per block, with the offending span when no. |
| P5 | **Presentation is never semantic.** | Showing a net as a wire, as labels, or as a rail symbol never changes connectivity. (In KiCad it can: two labels with the same text connect.) |
| P6 | **Computed things are caches.** | Evaluated values, affine forms, results and flags can be deleted and regenerated. They are never committed as design data. |
| P7 | **One edit pipeline.** | Schematic gestures, `:` commands, paste, AI patches and text edits all go through `parse → keyed patch → CST edit → re-elaborate`. There is no second path that mutates the model directly. |

P1 and P5 are the sharpest departures from KiCad-style tools, where wires and labels *are* the connectivity. They are what makes a code-first model compatible with a schematic-first user.

---

## 2. The canonical model

### 2.1 Entities

The canonical model is what the compiler produces from the language and what both editors display. The engine never sees text or graphics.

| Entity | What it is | Key fields | Stored in |
|---|---|---|---|
| **Project** | The top block plus project settings | top block, env defaults (temp, life), default confidence, propagation policy (cascade/flag), simulator backend | `project.toml` (settings) + code (env) |
| **BlockDef** | A reusable circuit type (a "sheet") | qualified name, ports, parameters (generics), body, drawable flag, doc | code |
| **PartDef** | A primitive: an element with a SPICE model | pins (name, electrical kind), parameters, model binding, default symbol class, sub-unit groups | code (library) |
| **InterfaceDef** | A named bundle of pins with roles | members, role pairs (source/sink), interface checks (auto-specs) | code (library) |
| **Port** | A pin or interface on a block boundary | name, type, role/direction | code (side/order on the symbol: sidecar) |
| **Instance** | A use of a BlockDef or PartDef | local name, type, parameter bindings | code (position: sidecar) |
| **Pin** | A terminal of a part | `inst.pin`, electrical kind | derived from PartDef |
| **Net** | A set of connected pins and ports | name (minted once, then sticky), members | code (wires/labels/rail style: sidecar) |
| **Parameter** | A named value | expression, unit, committed value, derivation, rationale (doc), validity range and margin, policy override | code (evaluated form: model-only) |
| **Knob** | An independent source of variation (v2 A7) | path name, kind (range / statistical / solver `?`), range, distribution, correlation group | code (explicit: `assume`, `matched`; implicit: tolerance literals) |
| **Assumption** | Contract input: env range, stimulus, load, source impedance | target, range or waveform, rationale | code (contract section) |
| **Spec** | Contract output (spec doc §1) | name, measure, setup, requirement, margin, conditions, confidence, severity, rationale | code (results: model-only) |
| **Probe** | A named measurement point | name, `V(net)`, `V(a,b)`, `I(inst.pin)`, `P(inst)` | code (badge position: sidecar) |
| **Setup** | A bench: stimuli, loads, analysis | name, sources, loads, analysis settings | code |
| **Waiver** | A disabled automatic spec, with its reason | target auto-spec, reason, author | code |
| **Placement** | Geometry of one sheet | symbol positions and orientation, wire routes, label and badge positions, net style, notes, frames, pins (locks) | sidecar |
| **Symbol** | The graphic for a PartDef or a BlockDef | pins→graphic pin mapping, body graphics | library symbol files (KiCad `.kicad_sym` compatible) + auto-generated block symbols |
| **Result** | Engine output per spec, probe and parameter | nominal, bracket, verdict, method, contributors, counterexample, affine form | model-only cache (`.spicy/cache`) |

Two notes on the table.

- **Pins are typed, ports are typed, and interfaces are types.** The ERC is part of type checking (v1 §7.2, borrowed from Spade), and those same types decide how things render (a bus stub versus a pin, a rail versus a wire).
- **Nets are first-class.** In SPICE and KiCad a net is whatever falls out of the wiring. Here a net has a name and a statement. It is what probes, specs, labels and the AI refer to. §3.4 explains why this matters.

### 2.2 Identity

| Entity | Identity key | Example | Notes |
|---|---|---|---|
| BlockDef | module path + name | `amps::CeAmp` | File moves update `use` paths through the refactor tool |
| Instance | parent path + local name | `board.amp.q1` | Local name unique within its parent block |
| Generated instance | parent path + array index | `board.ladder.rs[3]`, `board.bank.ch[2].r1` | Deterministic. Growing N appends and never renumbers |
| Pin | instance + pin name | `board.amp.q1.b` | Pin names come from the PartDef, never from numbers only |
| Port | block + port name | `CeAmp::vcc`, `CeAmp::vcc.gnd` | Interface members are addressable |
| Net | block + net name | `CeAmp::base` | Auto-names are **minted once and then sticky** (below) |
| Parameter, spec, probe, setup | block + name | `CeAmp::gain`, `CeAmp::bass` | One namespace per kind per block |
| Knob | path name | `board.amp.rc.tol`, `temp`, `life` | v2 Part B convention. Two instances of a block get separate part knobs but share environment knobs |

**Why names rather than UUIDs.** KiCad identifies everything by UUID (and so does its hierarchical instance-path scheme), so names are free to change without breaking links. The cost is that the file is unreadable to people and to the AI, and a UUID means nothing in a code review or a chat. atopile identifies components by address (their hierarchical path) instead (§4.6). Names give us:

- diffs a reviewer can read (`- let c_in = Capacitor(1u …)` / `+ … 2.2u …`);
- AI references that stay valid across sessions ("q1's base current");
- chat transcripts that stay meaningful after the design changes;
- sidecar entries that a human can repair by hand.

**Costs and mitigations.**

1. *Hand renames in the text view orphan the sidecar entry.* On re-elaboration, the editor matches orphaned layout keys to new unplaced instances of the same type whose connectivity overlaps (Jaccard similarity of net membership over 0.5, same PartDef). It re-links them and reports "`r1` → `r_top`: layout carried over". This is the same idea as git's rename detection. Unmatched orphans are kept for one session so undo and redo can restore them.
2. *Renames in the schematic or LSP* go through one refactor command (`:rename r1 r_top`). It rewrites every reference in code, the sidecar keys, probe and spec references, and the project's chat index (an alias table, so old transcripts still resolve).
3. *Unnamed nets.* When the editor creates a net (by drawing a wire), it mints a name at once, by rule. A net touching a port takes the port's name. A net touching a transistor terminal takes a role name (`base`, `coll`, `emit`, `gate`, `drain`…) if that name is free, else `q1_b`. Otherwise it is `n1`, `n2`, … The minted name is written to the code and then never regenerated. Merging two nets keeps the name of the more significant one (user-named beats minted beats port-derived…), and the editor tells the user which name it dropped. This gives us KiCad's "unnamed nets are OK" convenience without KiCad's unstable `Net-(R1-Pad2)` names.

### 2.3 What lives where

| Information | Code | Layout sidecar | Model-only (cache) | User-local |
|---|---|---|---|---|
| Block, ports, port types and roles | ✔ | port side and order on the symbol | | |
| Instances and their parameter bindings | ✔ | position, rotation, mirror, symbol variant override, field label offsets | evaluated values | |
| Nets and their members | ✔ | wire routes, junctions, style (wire / label / rail), label positions | SPICE node numbers | |
| Parameters: expression, committed value, derivation, rationale, margin | ✔ | which fields are shown on the sheet | evaluated affine form, dependency graph, flag state | |
| Knobs: `assume`, `matched`, tolerance literals | ✔ | | contributions, sensitivities | |
| Specs, probes, setups, waivers | ✔ | badge positions | results, brackets, counterexamples | |
| Ad-hoc probes ("what's the voltage here?") | | | | ✔ (promote to a named probe with one key) |
| Notes attached to one part | as a doc comment (rationale) | anchor offset | | |
| Free sheet notes, frames, title block | | ✔ | | |
| Groups ("bias network" frame) | | ✔ (frame + member list) | | |
| No-connect markers, net ties | ✔ (ERC-semantic: `nc u1.7;`, `tie gnd_a gnd_d;`) | marker position | | |
| KiCad provenance (UUIDs from an import) | | ✔ | | |
| Zoom, open sheets, panel sizes | | | | ✔ |
| Project-bound chats | `chats/` (committed, references by path) | | | |

Two judgment calls in this table:

- **Part-attached notes become doc comments.** A note like "C_in sized for f_L ≈ 20 Hz" is design rationale. It belongs where diffs, the AI and the parameter card will find it. Free-floating sheet text is presentation.
- **Ad-hoc probes are not design data.** Clicking around in simulate mode should not dirty the design file. A probe becomes design data (code) the moment a spec or a saved plot refers to it. That happens with one key (`gp`, "promote probe"), which also names the net if it was unnamed.

### 2.4 Project on disk

```
preamp/
├── project.toml            # top = "board::Board", confidence = "sigma3", propagation = "flag", simulator = "spicy"
├── src/
│   ├── board.ckt           # drawn  (top level)
│   ├── board.layout
│   ├── ce_amp.ckt          # drawn  (the walkthrough amplifier)
│   ├── ce_amp.layout
│   ├── ladder.ckt          # generated (for-loops)
│   └── ladder.layout       # only pins and tweaks; everything else is auto-placed
├── lib/                    # project parts, interfaces, models (.ckt), SPICE .model/.lib files
├── symbols/                # custom symbols (.kicad_sym format)
├── chats/                  # project-bound chats: one Markdown file per thread, entity refs by path
└── .spicy/                 # gitignored: results cache, sensitivities, per-user view state
```

`.ckt` and `.layout` are placeholder extensions. One block per file is **required for drawn blocks**, because the editor owns that file's formatting and one file per sheet keeps the ownership clear. Generated blocks may share a file.

### 2.5 The running example as stored

The CE amplifier (walkthrough §1) as a **drawn** block, which is exactly what the editor writes after someone draws it:

```rust
//! Common-emitter gain stage for the guitar input (walkthrough §1).

use std::parts::{Resistor, Capacitor, CapKind, Npn};
use std::iface::{PowerIn, AnalogIn, AnalogOut};
use models::q2n3904;

/// Single-transistor CE stage. Gain ≈ R_C / R_E thanks to emitter degeneration.
block CeAmp(vcc: PowerIn, input: AnalogIn, out: AnalogOut) {
    // ── contract (the I/O page) ──────────────────────────────────────────
    assume temp in -10°C..60°C;
    assume vcc.v in 12V ± 5%;
    assume life in 0yr..10yr;
    assume source(input) = sine(amp: ..100mV, freq: 20Hz..20kHz, r: ..1k);
    assume load(out) = 10k;

    /// Output must swing ±1 V both ways without clipping.
    spec bias = dc(vc) in 4.5V..6.5V, margin 0.2V;
    /// The next stage expects this level.
    spec gain_ok = ac(V(out) / V(input), 1kHz) in gain ± 5%;
    /// Don't cut the bass.
    spec bass = ac(V(out) / V(input)).f_low(-3dB) <= 30Hz, yield 99.9%;

    probe vc = V(coll);

    // ── design parameters ────────────────────────────────────────────────
    /// Mid-band gain target.
    let gain = 4.6;
    /// Sets I_C ≈ (V_B − V_BE) / R_E ≈ 1.4 mA.
    let r_e = 1k;

    // ── parts ────────────────────────────────────────────────────────────
    /// Divider holds V_B ≈ 2.1 V.
    let r1 = Resistor(47k ± 1%);
    let r2 = Resistor(10k ± 1%);
    /// Gain ≈ R_C / R_E, snapped to E24.
    let rc = Resistor(4.7k ± 1% from snap(E24, r_e * gain));
    let re = Resistor(r_e ± 1%);
    /// f_L = 1 / (2π · C_in · R_in) ≈ 20 Hz with R_in ≈ 7.9 kΩ.
    let c_in = Capacitor(1u ± 20%, kind: CapKind::Electrolytic);
    let q1 = Npn(model: q2n3904);

    // ── nets ─────────────────────────────────────────────────────────────
    net vcc_rail = [vcc.pos, r1.a, rc.a];
    net gnd      = [vcc.gnd, r2.b, re.b];
    net in_ac    = [input, c_in.p];
    net base     = [c_in.n, r1.b, r2.a, q1.b];
    net coll     = [out, rc.b, q1.c];
    net emit     = [re.a, q1.e];
}
```

And its sidecar, `ce_amp.layout`. The format is line-oriented, keyed and sorted, so a moved part is a one-line diff (§6.2):

```
# layout for amps::CeAmp. Presentation only: delete this file and the editor re-places everything.
layout 1  grid 50mil
port  input      at  0  18  side left
port  out        at 62  14  side right
port  vcc        at 32   0  side top
sym   c_in       at 12  18  rot 0
sym   q1         at 40  18  rot 0
sym   r1         at 24   8  rot 90
sym   r2         at 24  28  rot 90
sym   rc         at 40   8  rot 90
sym   re         at 40  28  rot 90
net   base       wire (14,18)-(24,18)-(38,18); (24,12)-(24,24)
net   coll       wire (40,14)-(62,14)
net   gnd        rail ground
net   vcc_rail   rail power  label "VCC"
field rc         show value,derivation
badge vc         at 46  12
frame "bias"     members r1 r2  at 18 4 34 32
pin   q1 r1 r2
```

Everything in the sidecar is presentation (P1). The `gnd` and `vcc_rail` nets are drawn as rail symbols, but they are the same nets as in the code. The `pin` line records which symbols the user placed by hand. The auto-placer will not move them (§5.3).

---

## 3. How each construct appears and is edited

Each subsection gives the rendering, the gestures, and the text edit each gesture produces.

The CE amp from §2.5 as it appears in EDIT mode. Every label is a model name, so what you see is what the code says:

```
┌ board › amp : CeAmp · drawn ───────────────────────────────────────────── EDIT ┐
│                                                                                │
│                               VCC ▲                                            │
│                ┌──────────────────┴───────┐                                    │
│               ┌┴┐ r1                     ┌┴┐ rc                                │
│               │ │ 47k ±1%                │ │ 4.7k ±1% ← r_e·gain               │
│               └┬┘                        └┬┘                                   │
│                │                          ├──────────────────────● out ▷       │
│    c_in        │ base                   │╱                                     │
│ ▷ input ───┤├──●────────────────────────┤  q1  2N3904                          │
│    1µ ±20%     │                        │╲                                     │
│                │                          ▼ emit                               │
│               ┌┴┐ r2                     ┌┴┐ re                                │
│               │ │ 10k ±1%                │ │ r_e = 1k ±1%                      │
│               └┬┘                        └┬┘                                   │
│                ▽ GND                      ▽ GND                                │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│ EDIT  amp.rc · Resistor 4.7 kΩ ±1% · ce_amp.ckt:36        specs 1✓ 1✗ 1◐   ⚑ 0 │
└────────────────────────────────────────────────────────────────────────────────┘
```

The status line always shows the entity under the cursor, its source location (`ce_amp.ckt:36`), the spec summary for the block (✓ pass, ✗ fail, ◐ "engineer's call"), and the flag count ⚑ (values whose derivation no longer holds, §9).

### 3.1 Blocks: hierarchical sheets and symbols

A BlockDef has **two faces**:

- **Sheet** (inside view): its instances and nets, drawn. Every block is a sheet (vision §3).
- **Symbol** (outside view): a box whose pins are its ports, drawn wherever the block is instantiated.

**Symbol generation.** The block's symbol is auto-generated from its port list and port roles unless a custom symbol exists:

- inputs on the left and outputs on the right;
- power sinks on top and ground on the bottom;
- interface ports as thick bus stubs with a type badge (`PowerIn`, `I2c`);
- block parameters as fields under the name (`gain = 4.6`).

Port order and side overrides are sidecar data (`port input side left order 1`). A custom symbol (for example an op-amp triangle for an op-amp macro-model block) lives in `symbols/` and maps symbol pins to port names.

**Definition vs instance.** The sheet shows the *definition*: one drawing, shared by every instance. Results, however, are *per instance*. The breadcrumb always shows an instance path:

```
 board › amp_l : CeAmp   (1 of 2 instances: amp_l, amp_r)          [n]ext instance
```

- A structure edit made from this view edits `CeAmp` and so affects both instances. The status line says `editing CeAmp (used 2×)`.
- Instance-specific values are **parameters bound at instantiation** (`let amp_l = CeAmp(gain: 4.6);`). They are edited on the *parent's* sheet, by selecting the symbol and changing its field.
- In simulate mode the overlays show `amp_l`'s voltages. `]i` cycles to `amp_r`.

**Navigation.** On a block symbol, `gd` ("go to definition") descends into its sheet. `<C-o>` / `gu` ascends. `gc` opens the code view at the block's declaration. This is the same grammar as code navigation in vim and rust-analyzer, applied to sheets.

**Creating a block.**

- `:block Filter` creates a new file with an empty body and an empty sheet.
- Selecting parts on a sheet and running `:extract Filter` does the equivalent of a "move to module" refactor. It creates the BlockDef, moves the statements into it, turns the nets that cross the selection boundary into ports (with types inferred from pin kinds), replaces the selection with one instance, and moves the layout entries to the new sidecar.

The inverse, `:inline`, is available for drawn blocks.

### 3.2 Pins, pin types, ERC

Every PartDef pin has an **electrical kind**. Kinds are used both by the ERC and by the renderer:

| Kind | Examples | ERC rule (sketch) | Rendering |
|---|---|---|---|
| `passive` | R, C, L terminals | none alone | plain pin |
| `polarized` | electrolytic `p`/`n`, diode `a`/`k` | reverse-bias check becomes an automatic spec (DC) | `+` mark |
| `power_in` / `power_out` | IC supply pins, regulator output | ≥ 1 source per power net; no two `power_out` without an explicit `parallel` | arrow into or out of the body |
| `analog_in` / `analog_out` | op-amp inputs and output | an output drives each net at most once | triangle |
| `digital_in` / `digital_out` / `open_drain` / `bidir` | GPIO | level compatibility becomes an automatic spec | triangle and label |
| `nc` | not connected | flag if connected | × |

- **ERC results are shown on the pin**, like a compiler squiggle. For example `q1.b ⚠ undriven` appears as an amber ring on the pin, and `K` shows the rule. Selecting the diagnostic in the diagnostics list (`]d`, `[d`, vim's diagnostic motions) jumps to the pin.
- **Where ERC runs.** Structural ERC (connectivity and roles) is type checking and runs on every edit. Electrical ERC (levels, reverse bias, derating) produces automatic specs (spec doc §8) and runs with the engine.

### 3.3 Interfaces as buses and harnesses

An interface is a nominal struct of pins with roles:

```rust
interface Power { pos: power, gnd: ground }        // PowerOut provides it; PowerIn consumes it
interface I2c   { sda: open_drain, scl: open_drain, gnd: ground }
```

In the schematic:

| Language | Rendering | Gesture | Text edit |
|---|---|---|---|
| Interface port on a block | thick stub on the symbol with a type badge | — | — |
| Two interface ports on one net: `net supply = [reg.out, amp.vcc];` | one thick **harness** line labeled `Power` | wire stub to stub (types must match; mismatch is refused with the reason) | one `net` statement |
| Member access: `vcc.gnd` inside a sheet | **breakout** (a bus entry) from the harness, labeled `gnd` | wire from a member stub, or `gb` ("break out") on the harness | a member appears in a `net` statement |
| Vector port `d: [Pin; 8]` | vector bus `d[0..8]` | same as KiCad vector buses | — |
| Interface check (for example the I2C pull-up and rise time auto-spec) | status glyph on the harness (✓ / ✗ / ?) | `K` on the harness shows the auto-spec rows | — |

**Semantics of connecting two interfaces.** Connecting two interfaces connects them member by member. That is one statement, and the model expands it to one net per member (`supply.pos`, `supply.gnd`). The canonical form keeps the single interface-level statement, so a harness is one line of code, not N.

This maps directly onto KiCad group buses (`{SDA SCL}`) and bus aliases for export (§6.4), onto atopile's interface `~` connections (§4.6), and onto Spade's structs with `inv` (backward) fields (§10).

### 3.4 Nets: wires vs labels vs rails

**The decision.** In the code, a net is a name and a set of pins. How it is *drawn* is a sidecar choice with three styles:

| Style | When (default heuristic) | Looks like |
|---|---|---|
| `wire` | local nets with fan-out ≤ 4 whose pins are near each other | orthogonal wires with junction dots |
| `label` | nets spanning far across the sheet, or crossing a frame | net-name flags at each pin cluster, KiCad local-label style |
| `rail` | nets whose members include a `power` or `ground` role | power / ground symbols: VCC ▲ above, GND ▽ below |

A net can also be *mixed*: wires within a cluster, labels between clusters. The sidecar stores one route per cluster.

- **Gestures.** `gl` toggles a net between wire and label style, and `gr` makes it a rail. None of these changes the code.
- **Drawing a wire from pin A to pin B** has three cases:
  - Both pins are unconnected: a new `net` statement with a minted name.
  - One pin is on net N: A or B is added to N's member list.
  - They are on different nets N and M: a **merge**. The editor asks which name survives if both are user-named, and otherwise merges silently and reports it. The result is one statement replacing two.
- **Deleting a wire segment** can split a net. The editor computes the connected components from the *remaining* wire geometry and writes the split: the original name stays with the larger component, and the smaller one gets a minted name.

  Splitting is the one gesture where geometry determines semantics. That is safe because the editor computes the split *before* it writes the code, and shows it (the two halves get different colors for a moment).
- **Placing a label** with an existing net's name on a pin adds the pin to that net. This is the KiCad gesture with our semantics: the label gesture is "connect to net by name". The label *shape* is then just the style.

**Why not KiCad's semantics (labels define connectivity)?**

- In KiCad, "same label text ⇒ same net" is action at a distance, and typos silently split nets.
- A text rename of a KiCad label changes connectivity. Here a rename is a refactor that keeps connectivity, and connectivity changes only through `net` statements. The AI can read every net's full membership in one line.

**Global nets.** There are none by default. A project-wide rail is an explicit port or an explicit `env`/project-level interface passed down, so that nothing connects by name across files (requirement R6). Ground is special only in that SPICE needs a node 0. The project declares which net is the reference, once.

### 3.5 Parameters and equations on parts

A part field shows the *value*. The *equation, rationale and margin* are one keystroke away, and can be pinned to show permanently.

**Display levels.** Toggle per sheet with `zv`, or per part through the sidecar `field` line:

| Level | `rc` renders as |
|---|---|
| 0 value | `4.7k` |
| 1 value + tolerance | `4.7k ±1%` |
| 2 + derivation | `4.7k ±1% ← r_e·gain` |
| 3 + check | `4.7k ±1% ← r_e·gain ✓` (⚑ if the equation no longer holds, §9) |

**Hover card (`K`):**

```
┌ rc : Resistor ──────────────────────────────────── ce_amp.ckt:36 ┐
│ value      4.7 kΩ ± 1%          committed, E24                   │
│ derived    snap(E24, r_e · gain)                                 │
│            = snap(E24, 1 kΩ · 4.6) = snap(4.6 kΩ) = 4.7k  ✓      │
│ why        Gain ≈ R_C / R_E, snapped to E24.                     │
│ knob       rc.tol  statistical ±1%   gain_ok spread share: 38%   │
│ checks     P = 9 mW ≤ 50% of 125 mW (auto)                    ✓  │
│ used by    spec gain_ok, spec bias                               │
├──────────────────────────────────────────────────────────────────┤
│ cv value   c= equation   cr rationale   gc code   gs specs       │
└──────────────────────────────────────────────────────────────────┘
```

**Edits.** Each edit changes exactly one expression or one doc comment:

| Gesture | Edit |
|---|---|
| `cv` (change value) `5.1k⏎` | Replaces the committed literal. In flag mode, if the derivation now disagrees, the part gets ⚑ (§9). |
| `c=` (change equation) | Edits the `from` expression, in an inline editor with completion over the parameters in scope and unit checking as you type. |
| `cr` (change rationale) | Edits the doc comment above the `let`. |
| `ct` (change tolerance) | Edits the `± …` term. |
| `:let gain = 5` | Upserts the parameter. Its dependents follow the propagation policy. |

A *project-wide* parameter (for example `vcc_nom` in `project.ckt`) appears on every sheet that uses it as a read-only field reference with a link. Editing it from anywhere edits the one definition and triggers propagation (§9).

### 3.6 Knobs and tolerances

- **Statistical knobs** are implied by tolerance literals (`± 1%` creates `rc.tol`). They show as the tolerance suffix on the field.
- **Range knobs** are declared in the contract (`assume temp in …`). They show on the I/O page and in the knob panel, never on the sheet.
- **Correlations** (`matched(r1, r2, 0.05%)`, same reel or lot) render as a dashed bracket joining the matched parts, labeled `≈0.05%`. This is how analog designers already annotate matching by hand.

**The knob panel** (`:knobs`, or `<space>k`) lists every knob that affects the current selection, with its contribution bars from the last analysis. The split into range and statistical follows v2 A7:

```
┌ knobs affecting: spec bass (FAIL 31.7 Hz @ 3σ) ──────────────────┐
│ range        temp      −10…60 °C   ▇▇                      3%    │
│ range        life      0…10 yr     ▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇  46%    │
│ statistical  c_in.tol  ±20%        ▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇  46%    │
│ statistical  q1.beta   100…300     ▇▇▇                     4%    │
│ statistical  r2.tol    ±1%         ▏                       1%    │
└──────────────────────────────────────────────────────────────────┘
```

Selecting a knob row highlights the parts it moves. `temp`, for example, lights up `q1` through its `β(T)` and `V_BE(T)` links.

### 3.7 Generated structures (loops)

A generated block renders read-only for **structure** and editable for **values that trace to a single literal**. Example, an R-2R ladder written with a loop:

```rust
/// N-bit R-2R ladder. Node i is rl[i].b; node N-1 is the output.
block R2rLadder<const N: usize = 8>(bits: [AnalogIn; N], out: AnalogOut, gnd: Ground) {
    /// Unit resistance; thin-film ±0.1% for DNL.
    let r = 10k;
    let rl: [Resistor; N]     = [Resistor(2 * r ± 0.1%); N];      // 2R leg from each bit
    let rs: [Resistor; N - 1] = [Resistor(r ± 0.1%); N - 1];      // R between adjacent nodes
    let rt = Resistor(2 * r ± 0.1%);                              // 2R termination at the LSB node
    connect(rt.a, rl[0].b);
    connect(rt.b, gnd);
    for i in 0..N {
        connect(bits[i], rl[i].a);
        if i + 1 < N { connect(rl[i].b, rs[i].a); connect(rs[i].b, rl[i + 1].b); }
        else         { connect(rl[i].b, out); }
    }
}
```

How it renders:

```
┌ ladder : R2rLadder<8> ────── generated · ladder.ckt:9–17 · structure read-only ┐
│ ┊ for i in 0..N ┊  (N = 8)                                                     │
│    rt         rs[0]         rs[1]                   rs[6]                      │
│ ▽──[2R]──●────[ R ]────●────[ R ]────●── ··· ──●────[ R ]────●──── out ▷       │
│          │             │             │         │             │                 │
│         [2R]          [2R]          [2R]      [2R]          [2R]               │
│          │             │             │         │             │                 │
│       ◁bits[0]      ◁bits[1]      ◁bits[2]    ◁bits[6]    ◁bits[7]             │
│                                                                                │
│ legs rl[i] = 2R · series rs[i] = R · rt = 2R termination · node i = rl[i].b    │
└────────────────────────────────────────────────────────────────────────────────┘
```

Rules for generated blocks:

1. **Frame and provenance.** Generated instances sit inside a frame labeled with the generating construct and its lines. `gc` on any instance (`rs[3]`) jumps to the statement that created it. The compiler records the source span of every instance and every connection.
2. **Structure edits** (adding a part, drawing or deleting a wire) are refused with a banner: `structure comes from code: open ladder.ckt:12 (gc), or ask the AI (ga)`. `ga` sends the attempted gesture (for example "connect `rs[7].b` to `bits[7]`") plus the block's code to the AI, which returns a *code* patch. The editor shows the patch together with the re-rendered result (§8). This is where the AI bridges the gap that makes general two-way editing hard (§4.5).
3. **Value edits** work when the value's provenance is a single literal:
   - `cv` on `rs[3]` offers to edit `let r = 10k`, warning `affects 16 instances (rs[*], rl[*], rt)`.
   - If the value is computed from several literals, the card shows the chain and lets the user choose which literal to edit, as in Sketch-n-Sketch's trace-based updates (§4.6).
   - If the value varies with the loop index, the edit opens the code.
4. **Placement tweaks** are allowed and stored in the sidecar by generated path (`sym rs[3] at …`). When N changes, new instances are auto-placed next to their neighbors and pinned entries stay put (§5.3).
5. **Collapsed view.** `zc` collapses a homogeneous array into one stacked symbol labeled `rs[0..8] ×8`, like Altium's stacked Repeat symbols and Virtuoso's iterated instances (§4.5).

### 3.8 Probes and spec status in place

Simulate mode (§7) overlays results. The spec doc §10 layout, applied to the CE amp after the loop has run (numbers from walkthrough §9):

```
┌ board › amp : CeAmp ─────────────────────────────────── SIMULATE · setup audio ┐
│                                                                                │
│                               VCC ▲ 12 V (11.4…12.6)                           │
│                ┌──────────────────┴───────┐                                    │
│               ┌┴┐ r1                     ┌┴┐ rc                                │
│               │ │ 47k                    │ │ 4.7k                              │
│               └┬┘                        └┬┘                                   │
│                │                          ├─◉ vc 5.50 V ────● out ▷ ┄ load 10k │
│   c_in 1µ      │ 2.10 V                 │╱      bias    ◐ 4.66 … 6.59 V        │
│ ▷ input ───┤├──●────────────────────────┤  q1   gain_ok ✓ 4.52 … 4.67 (3σ)     │
│ ✗ bass 31.7 Hz │                        │╲      I_C 1.38 mA                    │
│   0.9% fail    │                          ▼ 1.45 V                             │
│               ┌┴┐ r2                     ┌┴┐ re                                │
│               │ │ 10k                    │ │ 1k                                │
│               └┬┘                        └┬┘                                   │
│                ▽                          ▽                                    │
│ ┄ src: sine 100 mV, 1 kΩ   (bench ghost from setup audio)                      │
├────────────────────────────────────────────────────────────────────────────────┤
│ SIM  1✓ 1✗ 1◐ · bass ✗ 31.7 Hz > 30 Hz · top: life 46%, c_in.tol 46%   :why    │
└────────────────────────────────────────────────────────────────────────────────┘
```

- **Named probes** (◉) are code. They show their value and the status of every spec that uses them. ◐ marks "engineer's call": the absolute worst case fails but the realistic check passes (walkthrough §6).
- **Ad-hoc probes** (◌ markers, user-local) come from clicking a net or pin in simulate mode. They show value only. `gp` promotes one to a named probe (a code edit).
- **Node voltages and pin currents** appear as small grey annotations on nets and pins when `zr` (results) is on. Density adapts to zoom.
- **Spec badges attach to the part that contributes most**, not just to the probe. That is why `bass ✗` sits on `c_in`: the spec's measure is on `out`, but the fix is on `c_in`. The engine's top-contributors list (spec doc §9) drives this placement.
- **Counterexample.** `gx` on a failing spec loads its counterexample (−10 °C, end of life, `c_in` −20%, β = 100). The sheet re-annotates with the values at that point and every knob's position shows next to its part. This is "one click away from a simulation at that point" (spec doc §9), made visual.
- **Bench ghosts.** The active setup's stimuli and loads are drawn as *ghost* symbols at the block's ports (`src: sine 100 mV, 1 kΩ` at `input`, `load 10k` at `out`). They are defined in the contract, so they are shown but are not part of the sheet's structure. Editing one edits the `setup` or `assume` statement.

### 3.9 The I/O page

The I/O page is a *view of the contract section of the block's code* (spec doc §2, §10). Every row is one keyed statement, and editing a row is a keyed upsert of that statement.

```
┌ CeAmp · I/O ───────────────────────────────────────────────────────────────── ce_amp.ckt ┐
│ PORTS            type        role     side   checks                                      │
│  vcc             PowerIn     sink     top    auto: supply current ≤ source guarantee ✓   │
│  input           AnalogIn    in       left                                               │
│  out             AnalogOut   out      right                                              │
│ ASSUMPTIONS (range knobs)                                                                │
│  temp            −10 … 60 °C                         inherited from project, narrowed    │
│  vcc.v           12 V ± 5%                           ⊇ reg.vout 12 V ± 3% ✓ (board)      │
│  life            0 … 10 yr                                                               │
│  source(input)   sine ≤ 100 mV, 20 Hz…20 kHz, ≤ 1 kΩ                                     │
│  load(out)       10 kΩ                                                                   │
│ SETUPS           audio (default, derived) · power_on · [+]                               │
│ SPECS            measure                  analysis require        conf.  status          │
│  bias            V(coll)                  DC       4.5…6.5 V ±0.2 3σ     ◐ 6.34 / 6.59   │
│  gain_ok         V(out)/V(input) @1kHz    AC       gain ± 5%      3σ     ✓ 4.52…4.67     │
│  bass            f_low(−3 dB)             AC swp   ≤ 30 Hz        99.9%  ✗ 0.9% fail     │
│ AUTOMATIC (6)    c_in voltage ≤ 80% rated ✓ 18% · rc power ✓ · q1 V_CE ✓ … [expand]      │
│ WAIVERS          none                                                                    │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Row gestures.** `o` adds a row, which opens a `:spec …` command line pre-filled with the row's kind. `cc` changes a row. `dd` deletes it. `gd` on a spec opens its plot. `gx` loads its counterexample.
- **Composition checks.** Upstream guarantee ⊆ downstream assumption (spec doc §2) is shown on the assumption row, with the upstream block named. Here `vcc.v` is checked against `reg.vout` on the parent sheet.
- **The symbol and the I/O page share the port list.** Reordering ports on the page with `J`/`K`-style moves updates the sidecar `port … order` lines. Changing a port's *type* or *role* is a code edit, and it is checked at every instantiation site, the way changing a function signature is.
- **Block-level simulation** (vision §3) is `:run` from the I/O page. It runs every spec of the block under its setups, with the bench ghosts as the drivers.

---

## 4. Drawn vs coded blocks (D6)

### 4.1 The proposal: keep D6, redefine "drawn" as a syntactic subset

D6 says each block is drawn or coded. Coded blocks render auto-placed and read-only, with a placement sidecar, and drawn blocks can be exported to code. This proposal keeps the *user-visible* split but changes what it is made of:

| | D6 as written | This proposal |
|---|---|---|
| Drawn block storage | unspecified (a schematic file?) | **language text in the flat subset** + a layout sidecar |
| Coded block storage | language text + sidecar | unchanged |
| What makes a block drawn | a per-block choice | **derived**: the compiler reports `drawable` when the body is flat (R12) |
| Export drawn → code | a conversion | **nothing to do**: it is already code, formatted to read as hand-written |
| Text-editing a drawn block | not allowed / breaks the drawing | **allowed**. It stays drawn while the text stays flat. The schematic follows live. |
| Adding a `for` to a drawn block | — | the block becomes **generated** (structure read-only on the sheet), with a banner and one-key revert (`u`) |
| Coded → drawn | — | `:materialize`: unrolls to flat code, keeping the current auto-placement as pinned layout. One-way, with a confirmation. |

The block's *mode*, then, is a fact about its text, not a flag that can disagree with it. The per-block granularity D6 chose stays exactly where it was. What changes is that "drawn" no longer means "the schematic file is the truth". It means "the code is simple enough that the schematic *is* a faithful editor for it".

**Gesture feedback.** A gesture renders at once as a *speculative* result, drawn hatched, until re-elaboration confirms it, usually within milliseconds. edg-ide uses the same technique. If the compiler rejects the edit (a type or ERC error), the hatch turns red and the gesture is undone with the diagnostic shown.

**Why this is safe, and why it is not "full two-way editing" by the back door.** The hard part of two-way editing is generative code (v1 §6.2). A drag in the drawing has no unique inverse through a loop or a conditional. The flat subset has **no generative constructs**, so every model entity corresponds to exactly one statement, and every gesture has a unique, local text edit (P3). That is the subset every graphical Modelica tool has round-tripped for two decades (§4.6).

### 4.2 Where a drawn block's graphics live: four options

| Option | Precedent | For | Against |
|---|---|---|---|
| **A. Graphics as annotations inside the code** | Modelica (`annotation(Placement(…))`, a `Line` annotation on every `connect`); Ptolemy II MoML (`_location` properties); Umple (`position` mixins) | One file per block, so no desync. A rename carries the geometry with it. A long, proven history. | Coordinates are interleaved with semantics in every diff. Moving a part shows as a code change in review. The AI's context fills with coordinates it must preserve. Merges conflict on geometry lines next to semantic lines. Preserving the user's formatting through graphical edits is hard (OpenModelica, §4.6). |
| **B. Graphics in a trailer of the same file** | Zener/pcb (`# pcb:sch R1 x=… y=… rot=…` comment block at the end, rewritten sorted); Umple (layout after a `//$?[End_of_model]$?` marker); Enso (`#### METADATA ####` JSON trailer) | One file, no desync on file moves. The semantic part stays readable, and the trailer can be rewritten without touching the body (Zener never touches the Starlark). | Data in comments is fragile. Umple warns that copying the visible text silently drops the layout. Enso's span-indexed IDs break on a stray newline. The AI still sees the trailer unless it is stripped. |
| **C. A separate schematic file is the truth; code is generated from it** | KiCad, Simulink `.slx`, LabVIEW `.vi` (binary) | The drawing is fully expressive. | Two representations of semantics, so drift. The AI must read schematics for drawn blocks and code for coded ones. Drawn-block diffs are unreadable: MathWorks tells users to register even text `.mdl` as binary in version control. "Export to code" becomes a converter that must be kept in sync forever. |
| **D. Code (flat subset) + per-block sidecar keyed by name** | tscircuit `manual-edits.json` (placements keyed by selectors such as `"R1"`, with `relative_to`); JITX `design-info/schematic.design`; atopile's `atopile_address` join to the PCB; Zener's newer reconciliation with a user-owned KiCad schematic, keyed by component path | Semantics in one place (P1). The sidecar is safe to lose. Clean diffs (moves touch only the sidecar). The AI context is pure semantics. Merges split into a semantic merge (careful) and a geometry merge (auto-resolvable). | Two files per block can drift: a rename done by hand in a plain text editor orphans the entry. Tooling must handle the pairing. |

**Decision: D**, with A's main benefit (no desync) recovered by tooling:

- The rename refactor updates both files.
- Rename detection re-links orphans (§2.2).
- The sidecar tolerates unknown and missing keys by design: missing means auto-place, unknown means orphan kept for one session.
- A git merge driver unions sidecars by key (§6.3).

Two further arguments decide against A (Modelica-style):

1. **The AI is a primary editor of this code** (§8). LLM schematic work points the same way: pin-name wiring outperforms geometric representations (SchGen, §8.4). Annotations in Modelica are verbose, around 60–120 characters of geometry per component and per connection. That would roughly double the token count of a drawn block and put coordinates into every patch the AI must write. Keeping geometry out of the language keeps the AI's edits semantic.
2. **Code review is where designs get checked** by the team. A reviewer must be able to see "C_in changed from 1 µF to 2.2 µF" without scrolling past "C_in moved 3 grid units left". Option D makes the semantic diff and the cosmetic diff two different files.

What we *do* take from Modelica:

- the idea that a diagram tool edits a textual model;
- a graphical layer and an icon layer per class (our sheet and symbol, §3.1);
- one annotation for a whole component array (Modelica spec §18.2), which is our stacked-symbol rendering of T1 arrays (§4.5);
- above all, **OpenModelica's hard-won lessons on keeping text formatting through graphical edits** (§4.6). OMEdit re-unparses the whole class and then tree-diffs it against the original to restore formatting. Its author reports three failure modes:
  - edited connections can't be matched because `connect` equations have no names;
  - default values leak into the text because the compiler doesn't track which values were defaults;
  - some merges fail catastrophically, so a sanity check has to fall back to a full unparse.

  He names the cleaner alternative, editing the concrete syntax tree directly, as requiring "a full redesign". We are starting from scratch, so we build it in: named nets (R3), keyed statements (R4), a lossless CST with local patches (R14), and a writer that emits only what the user set.

### 4.3 What "export to code" produces, and whether it reads as hand-written

Because drawn blocks are stored as code, "export" happens in only two places:

- **Importing from KiCad or another schematic tool**, where the code is written for the first time (§6.4).
- **Writing statements for new gestures.**

In both cases the output must read as if a careful engineer wrote it. Rules for the canonical writer:

1. **Sections in a fixed order:** contract (assumptions, specs, probes, setups), then parameters, then parts, then nets. Within a section, the user's existing order is kept. New statements are appended to their section. For a fresh import, parts are ordered by **signal flow** (the auto-placer's layer order: inputs first, then left to right, top to bottom), so the file reads input → output as the drawing does.
2. **Names.** The user's names are kept as typed. Imported reference designators become lowercase names (`R1` → `r1`). An *optional* AI pass proposes semantic renames (`r1` → `r_bias_top`) as an ordinary reviewable patch.
3. **Nets** are written one statement per net, with members sorted by a fixed rule: ports first, then instances in section order, then pin order. Net names are minted by the §2.2 rule.
4. **Values** are written as the user wrote them. Expressions stay expressions; the writer never folds `r_e * gain` into `4.6k`.
5. **Rationale.** Notes attached to parts become doc comments above the part's statement.
6. **No noise.** No coordinates, UUIDs or generated-by banners. The file header says nothing about the drawing, since the sidecar documents itself.

The CE amp in §2.5 is exactly this output. It reads like a hand-written netlist with names, types and reasons, which is what engineers write by hand when they write SPICE carefully. For a large flat sheet (100+ parts), flat code is long but honest. The editor offers `:extract` to factor it into sub-blocks, which is the right fix for both the drawing and the code.

### 4.4 Mixing: drawn contains coded, coded contains drawn

Both directions work, because the boundary is the **block instance**, and a block is referenced by its type, not by how its body is written:

```
board (drawn)
├── reg   : Ldo           (drawn)
├── amp   : CeAmp         (drawn)       ← the walkthrough amplifier
└── gain  : R2rLadder<8>  (generated)   ← for-loops; read-only structure
      └── (parts only)

bank (generated: for ch in 0..4 { let ch_amp[ch] = CeAmp(gain: g[ch]); … })
└── ch_amp[0..4] : CeAmp  (drawn)       ← opening one shows the drawn sheet, with parent-bound params shown read-only
```

- **A generated block's symbol** on a drawn sheet is an ordinary block symbol. Wiring to it is editing the *parent's* net statements, so it is fully editable.
- **A drawn block instantiated by generated code.** Descending into `ch_amp[2]` opens the drawn `CeAmp` sheet, which is fully editable because editing the *definition* is allowed. Parameters bound by the generator (`gain: g[2]`) show as read-only fields with a "bound by parent: `bank.ckt:7`" link.
- **The I/O page is the meeting point.** Its port list and contract are the same text regardless of the body's kind.

### 4.5 How far can two-way editing go? A tiered answer

Full two-way editing of arbitrary code is too hard: a gesture through a loop has no unique inverse. But the space is not binary. Four tiers:

| Tier | Constructs | Schematic structure edits | Schematic value edits | Precedent |
|---|---|---|---|---|
| **T0 Flat** | instances, `net` statements, `let` parameters, contract items | ✔ full round-trip, including delete and rename | ✔ | Modelica editors (declarations + `connect`); edg-ide (insert-only, because of Python's dynamism) |
| **T1 Vectorized** | instance **arrays** `[Resistor; 128]`, vector nets, slices, concatenation, broadcast (`net taps = [v_top, v_ref[1..128]] ~ r[..].a`) | ✔ drawn as one iterated symbol with bus labels, round-trips | ✔ | Cadence Virtuoso iterated instances (`R_string<127:0>` with bus labels `vTop,vRef<127:1>`); Altium multi-channel `Repeat(…)` sheet symbols and `Repeat(port)` sheet entries |
| **T2 Generative** | `for`, `if`, recursion, computed structure | ✗ read-only; routed to code or to the AI (§3.7) | ✔ when the value traces to one literal; a choice of literal when it traces to several; ✗ when index-dependent | Sketch-n-Sketch (literal-only edits through traces, with heuristics for loops); edg-ide's speculative single-instance assumption inside loops |
| **T3 Foreign** | SPICE subcircuits, behavioral models | ✗ (opaque symbol) | model parameters only | KiCad `Sim.*` fields |

**T1 is the new, important tier.** The Virtuoso idiom shows that many "loops" in analog design are really *uniform arrays with shifted bus hookups*: a resistor string, a DAC ladder, a bank of identical channels. These can be written and drawn declaratively.

- **The R-2R ladder of §3.7 is T1 when written without its loop.** The nodes form a vector net `node[0..N]` with `node[i] ∋ rl[i].b`. The legs are one broadcast statement (`bits[..] ~ rl[..].a`). The series resistors are two slice statements, `rs[..].a ~ node[0..N-1]` and `rs[..].b ~ node[1..N]`. The ends are ordinary single-element statements (`rt` on `node[0]`, `out` on `node[N-1]`). Every statement is uniform over its slice, which is exactly the Virtuoso `vRef<127:1>` idiom. So the same circuit can be written in two tiers: the loop version is generated and read-only, and the vectorized version is drawable.
- Altium's documented restrictions mark exactly where T1 ends. With `Repeat`, "it is not possible to pass an individual net to just one channel", and harnesses are not supported. Uniform hookups are drawable; per-element exceptions push you to T2.
- atopile draws the line in the same place from the language side. Its `for` loops may only *alias* existing array elements, never create instances (`new T[N]` creates them), so its instance set is always declarative (`AtoParser.g4:153-160`, `ast_visitor.py:1790-1862`).

**Recommendation to the language thread:** make T1 expressible without `for`, meaning vectorized connection syntax over arrays and slices (R8, R20). Every structure written in T1 instead of T2 stays editable in the schematic.

**What about T2 → T0 round-trip via "unroll, edit, re-roll"?** Rejected. Re-rolling means synthesizing a loop from an edited unrolled structure. That is program synthesis, and it fails unpredictably. Sketch-n-Sketch's experience (§4.6) is that even value edits through loops need heuristics and user disambiguation. Structural synthesis is harder still. The AI bridge (`ga` on a refused gesture, §3.7) gives the same capability with a reviewable diff and without the promise of determinism.

### 4.6 Prior art: how existing tools relate drawings and text

| Tool | Where graphics live | Editing directions | Subset / limits | Identity | Lesson for us |
|---|---|---|---|---|---|
| **Modelica** (OMEdit, Dymola) | annotations in the `.mo` code (spec ch. 18). Tools must keep unknown vendor annotations intact. | both | graphical edits touch declarations, `connect` and annotations. An array has one placement. `connect` in `for`/`if`: drawing behavior not confirmed | component names; connections are **unnamed** | Formatting preservation is the hard part. Unnamed connections defeat diff/merge. |
| **OpenModelica diff/merge** (ticket #2905; Sjölund 2021) | — | GUI → text | "limited to programmatically modified text"; about 0.55 s per 100 kB file, so edits are batched | — | Build CST patching in from day one |
| **LabVIEW** | binary `.vi` (graphics are the program) | graphical only | — | internal | Graphical diff/merge tools (LVCompare, LVMerge) are needed when there is no text |
| **Simulink** | `.slx` (zipped XML); `.mdl` text treated as binary | GUI, plus programmatic `add_block`/`add_line` | `arrangeSystem` auto-layout "changes the line handles" | unmodifiable SID (`model:number`) | The comparison tool **hides block repositioning by default**, so semantic and cosmetic diffs are separated |
| **tscircuit** | `schX`/`schY` props in TSX, **plus `manual-edits.json`** (placements keyed by selector, relative to the group center) | code → schematic; drags → sidecar only | the TSX is never rewritten by drags | string selectors (`"R1"`) | The sidecar pattern works in production. `relative_to` makes group moves cheap. |
| **Zener / pcb** | trailing `# pcb:sch <id> x= y= rot=` comments, rewritten sorted; newer: a user-owned KiCad schematic reconciled by connectivity equivalence | code → schematic; the KiCad path preserves "user reorganizations" | reconciliation is "minimal, checked, reversible" | canonical component path; **deterministic UUIDv5** from it | UUIDv5-from-path for exports (§6.5). Connectivity-equivalence reconciliation for KiCad re-export. |
| **JITX** | `design-info/schematic.design`, generated from code | code → schematic; UI group and net edits | — | dotted group paths | Generated schematics were publicly called "nowhere close to human-readable, let alone modifyable" (HN 2021). Readability is the product. |
| **PolymorphicBlocks edg-ide** (UIST 2021, "Weaving Schematics and Code") | not stored; ELK layered layout every time | GUI actions **insert** code (PSI writes, then the formatter) | **no deletion, no rename, no parameterization from the GUI** ("accurate static analysis of Python code is difficult"); the diagram updates speculatively (hatched) until recompiled | Python object paths | The closest prior design, and its limits come from Python's dynamism. A declarative, keyed language removes them (R4–R6). |
| **SKiDL** | regenerated each run | code → KiCad schematic, one way | force-directed placement; the author: "you can't route your way out of a bad placement". 2.3.0 (July 2026) turns hard nets into labels and emits power symbols | reference designators | Label/rail conversion is necessary. Placement quality dominates. |
| **Flux.ai** | cloud project, snapshot history | graphical, plus an AI agent | no text form; MCP: "every design change is made by the Flux AI agent" | — | Without a text form, the AI edits an opaque model and users can't review its changes as diffs |
| **Umple** | `position` mixins after an end-of-model marker, in the same file | both (UmpleOnline) | non-model code (method bodies) "omitted from the diagram"; **auto-layout by default since 2026** | class names | Hidden trailers get lost on copy. The field is moving to auto-layout with tweaks. |
| **SVG-PCB** (SCF 2022) | literals in the JS source | both: dragging a handle rewrites the literal; dropping a part adds code under "ADD COMPONENT" | literal-level only | source locations | Geometry-as-literals works for PCB outlines; it is wrong for schematics (P1) |
| **Ptolemy II / Xcos** | `_location` in MoML XML / `graphics` structs | graphical | "not propagated to the compiled representation" (Xcos) | names | Even when graphics sit in the model file, compilers discard them. They are presentation. |
| **Sketch-n-Sketch** (PLDI 2016; UIST 2019) | program output | both, via trace-based synthesis | "we attempt only to change value literals"; loop ambiguity handled by a "fair" heuristic that "will not always make choices that the user would prefer"; literals can be frozen with `!` | provenance traces | Our value editing through provenance (§3.7) is this idea restricted to the unambiguous cases |
| **JetBrains MPS** | positions as model properties (projectional; "no parsing is necessary") | both (it edits the AST) | not text-first; built-in diagramming deprecated | node IDs | Projectional editing sidesteps parsing, but gives up plain-text diffs and AI-friendly text |

**What the prior art settles.**

1. **Every system that turns GUI edits into code supports only a subset.** PolymorphicBlocks says "the visualization will necessarily be a subset of the HDL" and "graphical editing operations do not need to cover all conceivable code edits". Sketch-n-Sketch edits only literals. Umple omits method bodies. Our tiers (§4.5) state the subset explicitly, and make it *checkable* (R12).
2. **Identity is where round-tripping breaks.** OpenModelica could not match unnamed connections. atopile loses layout on a rename (see the atopile notes below). Spade's generated Verilog instance names are `<unit>_<counter>`, which are unstable under insertion. Named nets and name paths (R1–R4) are the fix.
3. **The language decides how far two-way editing can go.** edg-ide stopped at insert-only because Python can't be statically analyzed well enough to delete or rename. A declarative, order-independent, keyed language (R4–R6) makes delete and rename local and safe. That is why the flat tier can offer *full* two-way editing where edg-ide could not.
4. **Layout belongs outside the semantic diff.** Simulink hides repositioning in comparisons by default. tscircuit keeps drags out of the TSX. Umple and Enso show that same-file trailers are fragile.

**atopile, specifically** (from `externals/atopile`, commit 619eda7):

- **The link.** The `.ato` code is the truth for *what* is in the circuit, and the `.kicad_pcb` is the truth for *where*. They are joined by one hidden footprint property, `atopile_address`, which holds the instance path (`r_chain[2]`, `leds[4].decoupling_cap`). It is written in `src/faebryk/libs/part_lifecycle.py:684-689` and matched in `src/faebryk/exporters/pcb/kicad/transformer.py:300-330`.
- **Renames lose identity.** There is no fallback match. The renamed part is re-inserted at the auto-placement point, and the old footprint is deleted ("Removing outdated component…", `transformer.py:2098-2116`).
- **Net identity is inferred.** A net keeps its KiCad name only if more than 80% of its pads already carry it (`src/faebryk/libs/nets.py:16-63`).
- **Layout reuse.** Sub-module layouts are copied by anchoring on the footprint with the most pads, with rotation marked as a TODO (`src/faebryk/exporters/pcb/layout/layout_sync.py`).
- **Loops never create instances** (`AtoParser.g4:153-160`, `ast_visitor.py:1790-1862`). `new T[N]` creates them, named `x[i]`. This is T1 in all but name.
- **No schematics.** atopile generates and syncs none. Its graph viewer uses force-directed ForceAtlas2 and does not persist positions.

We take the address-keyed join and the array-only instance creation. We fix renames (refactor + detection), net identity (named nets instead of 80% pad voting), and schematics (which atopile avoids entirely).

---

## 5. Auto-placement and readability

Generated blocks, imports without geometry, and new instances added from text or by the AI all need automatic placement. Readability is the whole point: JITX's generated schematics are the cautionary tale (§4.6), and "loses what a good schematic conveys" is the main community objection to code-first design (v1 §2).

### 5.1 Conventions become constraints

The layout engine encodes the conventions analog engineers already follow, as hard or soft constraints:

| Convention | Constraint | Source of the information |
|---|---|---|
| Signal flows left → right | layered layout along the signal graph. Block inputs are pinned to the left boundary layer, outputs to the right. | port roles (R7), pin kinds |
| Positive supply up, ground down | power and ground nets are **removed from the layering graph** and drawn as rail symbols. Two-terminal parts bridging a signal net and a rail are oriented vertically, with the rail end toward its rail. | interface roles (`power`, `ground`) |
| Feedback flows right → left | edges from a later layer to an earlier one are routed above (or below) the forward path, not through it. | back edges found by cycle breaking in the layering step |
| Functional grouping | parts belonging to one sub-structure (a divider, an RC filter, a current mirror) are placed as a compact **template**. | idiom recognition (below) and user frames |
| Symmetry | differential pairs, current mirrors and bridges are placed mirror-symmetric around an axis. | idiom recognition |
| Readable labels | long-span nets use labels, not wires (§3.4). Wires never pass through symbols. | net span after placement |
| Stable reading order | ties are broken by source order, then by instance name, so the same input always gives the same output. | determinism (R2) |

**Idiom recognition** is a small pattern library matched on the netlist before layout. Each idiom has a canonical sub-layout:

| Idiom | Pattern | Canonical placement |
|---|---|---|
| Divider | R–R series between two different nets, with the midpoint driving something | vertical stack, midpoint to the right |
| RC low-pass / high-pass | R in series and C to a rail, or the reverse | series element horizontal, shunt element vertical |
| Emitter/source degeneration | R from a transistor's e/s to a rail | directly below the transistor |
| Collector/drain load | R or current source from c/d to the upper rail | directly above the transistor |
| Differential pair | two same-type transistors sharing e/s | mirror pair, tail below |
| Current mirror | two transistors sharing b/g, one diode-connected | mirror pair, diode-connected side on the left |
| Decoupling cap | C across a rail pair next to an IC's power pins | small, next to the pins, excluded from the signal layering |

For the CE amplifier, the recognizer finds a divider (`r1`, `r2`, midpoint `base`), a collector load (`rc`), a degeneration resistor (`re`) and a coupling cap (`c_in`, series on the signal path). The result is the textbook drawing in §1 of the walkthrough, which is the point: **the auto-placer should draw what the engineer would have drawn.**

### 5.2 Pipeline

```
netlist + roles + previous layout + pins
  │ 1. classify nets: ground / supply / signal. Rails leave the layout graph and
  │    become rail symbols (flags)
  │ 2. recognize idioms (subgraph match) → rigid template groups with fixed-position pins
  │ 3. layered layout per sheet (Sugiyama; ELK Layered): cycle breaking (feedback →
  │    back edges), layering from port roles (inputs first, outputs last),
  │    crossing minimization with FIXED_POS pins, coordinate assignment;
  │    source order as tie-breaker, no randomness
  │ 4. vertical pass: supply-side parts up, ground-side parts down
  │    (layer/partition constraints or a small constrained second pass)
  │ 5. apply pins (user-placed symbols), then remove overlaps
  │    (VPSC scan or constrained stress with pinned nodes fixed)
  │ 6. orthogonal hyperedge routing around the fixed geometry;
  │    choose wire vs label per net (§3.4)
  │ 7. verify: extract the connectivity a reader (or KiCad) would see in the drawing
  │    and compare it with the model; on mismatch, switch patterns off one by one
  ▼
layout (new entries written to the sidecar; pinned entries never rewritten)
```

- **Steps 3 and 6 are solved problems.** ELK's *Layered* algorithm implements the Sugiyama pipeline with **port constraints**: `portConstraints = FIXED_POS` for symbol pins, orthogonal edge routing by default, `hierarchyHandling = INCLUDE_CHILDREN` for cross-hierarchy edges, and a hyperedge-merging step for multi-terminal nets (Schulze, Spönemann, von Hanxleden 2014; Domrös et al. 2023). netlistsvg uses elkjs to draw Yosys netlists. For routing, libavoid (adaptagrams) implements orthogonal hyperedge routing (Wybrow, Marriott, Stuckey 2009/2012).
- **Steps 1, 2 and 4 are what make it an analog schematic.** The closest working precedent is **Weave** (Gulgonul, arXiv 2607.03835, July 2026, MIT-licensed, bundles elkjs):
  - signal components laid out left to right with ELK Layered;
  - ground and supply turned into flags rather than wires;
  - feedback loops, divider legs and shunts handled as **placement patterns outside the layout graph**;
  - a **round-trip verifier** that re-reads the drawing and checks connectivity net by net;
  - a "safe-mode ladder" that disables patterns until the verifier passes. It reports 100% verified connectivity on 117 public circuits and 88.4% on a 3,460-circuit corpus. For comparison, 76% of the LLM-based Schemato's outputs compile, which is a weaker metric.

  Our step 7 is Weave's verifier. In our design connectivity never comes *from* the drawing (P1). Even so, a drawing that *looks* connected where it isn't (a wire crossing a pin, a T without a dot) is a readability bug, and the same check guarantees the KiCad export is right.
- **Template groups** (step 2) are how every recent tool gets analog idioms right. tscircuit's `matchpack` separates "strong" pin-to-pin connections, which drive placement, from "weak" connections to GND/VCC, which only set orientation: a capacitor faces up toward VCC and down toward GND. Its `schematic-match-adapt` matches partitions against a corpus of hand-drawn templates. Its README says generic flow layout fails because "readable schematics require … orienting ground net labels down and V+ labels up". The analog-specific literature (Green & Andersen 1990, functional clustering then expansion; Arsintescu 1996, symmetry extraction; Hsu & Lin 2022, building-block classification) all converge on the same structure. Idiom recognition can reuse subgraph matching (SubGemini) or the hierarchical basic-structure library of the sizing-rules method (Massier, Graeb, Schlichtmann 2008), which recognizes current mirrors and differential pairs.
- **Implementation.** ELK and elkjs are Java/JavaScript, adaptagrams is C++, and I found no Rust-native equivalent. Options: elkjs in the (web) front-end, a Rust port of the few ELK Layered phases we need, or a WASM build. This is a decision for whoever builds the renderer. Deterministic output is a hard requirement whichever option is chosen.

### 5.3 Keeping user tweaks stable across regenerations

The goal is the **mental map** property: a small change to the circuit gives a small change to the drawing (Misue, Eades, Lai, Sugiyama 1995). User studies confirm that preserving it helps people stay oriented (Archambault & Purchase 2013). For schematics the stakes are higher than for generic graphs. Petre & Green's observational studies of expert hardware designers found that experts read schematics through **secondary notation**: layout cues with no formal meaning, such as grouping, alignment and flow direction. A regenerated drawing that reshuffles parts destroys exactly the information experts rely on.

Mechanisms:

1. **Everything is keyed by name** (§2.2). Regeneration is a function of `(netlist, previous layout, pins)`, never of the netlist alone.
2. **Hard pins.** A symbol the user moved by hand is recorded in the sidecar's `pin` line. It is applied after layout as a fixed node; overlaps are removed around it and routing goes around it. `gP` unpins, and `:relayout` on a selection unpins and re-places it. (Of the Graphviz engines, only neato and fdp support pinning. Constraint layouts such as WebCola/libcola do too.)
3. **Soft memory of the previous auto-layout.** Unpinned entities are not free either. Their previous positions are fed back as ELK's interactive strategies: `layering.strategy = INTERACTIVE` (layers from previous coordinates) and `crossingMinimization.semiInteractive = true` (previous order within a layer). "Model order" (source order) is the tie-breaker. The unchanged parts of a sheet then stay where they were when the netlist changes.
4. **Group-relative coordinates.** Entries inside a frame or an idiom group are stored relative to the group's origin, as tscircuit's `ManualEditsFile` does (`schematic_placements` with `{selector, relative_to: "group_center", center}`). Moving a whole group is then a one-line sidecar diff, and a group can be re-placed as a unit.
5. **Incremental placement of new instances.** A new instance goes in the free slot nearest the centroid of its connected neighbors, using an idiom template if it completes one: adding `re` next to `q1.e` snaps it below `q1`. Existing unpinned symbols move only to avoid overlap, and by the smallest displacement.
6. **Deletions leave gaps.** Deleting instances never triggers a global relayout. The user compacts on request (`:compact`).
7. **Deterministic tie-breaking.** Same input gives the same drawing, so CI can render schematics and diff them (§6.3). The ELK authors report that users are frustrated by layouts they can't control, and that they themselves now avoid randomness.

**Generated structures.** Tweaks go under generated paths (`sym rs[3] …`), stored relative to the generating frame.

- When `N` shrinks, entries beyond the bound become orphans, kept for one session and then dropped.
- When `N` grows, new indices continue the pattern of the previous elements. If `rs[0..8]` sit on a row with a constant pitch, `rs[8]` continues the row. The heuristic is to fit a constant stride to the last k siblings.

**Hints in the language?** SchGen (arXiv 2605.30345) and Lcapy (Hayes 2022) take the opposite approach: *relative placement written in the code* (`right`, `down`, "place X right of Y's pin"). Lcapy then solves x and y separately from those hints. This is attractive for coded blocks because the hints survive regeneration by construction. It is rejected as a *language* feature (R15, P1). It fits naturally, though, as a sidecar entry kind (`rel r2 below r1`), which is presentation data that constrains the layout without fixing coordinates. Adding a `rel` line kind is a cheap extension if pins prove too rigid for generated blocks.

### 5.4 Readability: what to measure

Graph-drawing studies find crossings have the largest single effect on understanding (Purchase 1997), with path continuity next (Ware et al. 2002). Domain rules, however, override generic aesthetics. Experts drawing energy-system diagrams tolerated crossings but minimized bends (Helmke et al. 2024), and schematic conventions (rails, flow direction, idioms) are domain rules of the same kind. I found no controlled user study that isolates crossings, bends and symmetry for circuit schematics.

The evaluation plan is open question 12 (§12):

- a corpus of textbook analog circuits with reference drawings;
- metrics: idiom compliance, crossings, bends, signal-flow monotonicity, and similarity to the reference drawing (the SSIM and graph-edit-distance measures used by Schemato and Weave);
- round-trip connectivity at 100%, as a gate.

---

## 6. File format and interop

### 6.1 Native format: the language *is* the format

The vision's open item (prefer standard formats, custom if needed) resolves as follows:

- **Semantics:** our language (`.ckt`). It is plain, diff-friendly text, and there is exactly one semantic format.
- **Geometry:** the layout sidecar (`.layout`). It is line-oriented and sorted by key.
- **Symbols:** KiCad's `.kicad_sym` format, read and written. This gives us the KiCad symbol library, which is the part-library-coverage bottleneck v1 §2 worried about, without inventing a symbol format.
- **Settings:** `project.toml`.
- **Results:** a gitignored cache.
- **Interop:** `.kicad_sch` import and export, and SPICE netlist export (our `.spicy` dialect and standard SPICE). There is no KiCad round-trip as a *storage* format.

A standard format as the primary store was considered and rejected. `.kicad_sch` cannot hold equations, rationale, margins, knobs, specs or typed interfaces except by smuggling them into free-form fields. Its connectivity is defined by wire geometry and label text, which contradicts P1 and P5. And its UUID-heavy S-expressions make poor diffs (§6.3).

### 6.2 The layout sidecar format

Design goals: one entity per line, sorted keys, no floating-point noise (grid units), stable under regeneration. Grammar sketch:

```
file     := header line*
header   := "layout" VERSION ("grid" LENGTH)?
line     := "sym"   KEY "at" X Y ("rot" DEG)? ("mirror")? ("symbol" LIBID)? ("variant" NAME)?
          | "port"  KEY "at" X Y "side" SIDE ("order" N)?
          | "net"   KEY ("wire" ROUTE | "label" AT* | "rail" KIND) ("label" STR)?
          | "field" KEY "show" FIELDS ("at" DX DY)?
          | "badge" KEY "at" X Y
          | "frame" STR "members" KEY* "at" X1 Y1 X2 Y2
          | "note"  STR "at" X Y
          | "pin"   KEY*
          | "kicad" KEY UUID             # import provenance (§6.4)
          | "orphan" line                # kept one session for rename recovery
```

- **Keys are model keys:** instance names, generated paths (`rs[3]`), net names, port names, probe names.
- **Coordinates are integers in grid units.** Dragging a part produces a diff of exactly one line.
- **The sidecar never contains** values, connectivity or anything else the engine reads (P1).
- **The sidecar may be absent.** Everything missing is auto-placed.

### 6.3 Version control: diffs, merges, reviews

**Diffs.** A typical change to the CE amp, "swap C_in for 2.2 µF and move it", is a two-file diff:

```diff
--- a/src/ce_amp.ckt
+++ b/src/ce_amp.ckt
-    /// f_L = 1 / (2π · C_in · R_in) ≈ 20 Hz with R_in ≈ 7.9 kΩ.
-    let c_in = Capacitor(1u ± 20%, kind: CapKind::Electrolytic);
+    /// f_L worst case 15.2 Hz with −20% tolerance and −20% aging.
+    let c_in = Capacitor(2.2u ± 20%, kind: CapKind::Electrolytic);
--- a/src/ce_amp.layout
+++ b/src/ce_amp.layout
-sym   c_in       at 12  18  rot 0
+sym   c_in       at 10  18  rot 0
```

In `.kicad_sch` the same change is a property edit inside a nested S-expression, next to UUIDs for every pin. Other sources of churn in KiCad files:

- Every schematic embeds a full copy of each library symbol it uses (`lib_symbols`), so a library update touches every schematic.
- KiCad version upgrades reformat whole files (the KiCad 8 prettifier, KiCad 10's property format).
- Instance data used to flip between instances of reused sheets (KiCad issue #11035, fixed in 7.0.3).

KiCad does save items sorted by type and then UUID, so order is stable. Still, the community's diff tools (KiDiff, KiRI) are all *visual*: none does semantic diffs or merges.

**Merges.**

- `.ckt` files merge as ordinary text. Keyed statements and one-net-per-line keep conflicts local: two people editing different parts never conflict.
- `.layout` files get a **git merge driver** (`spicy merge-layout`). It unions entries by key. If both sides moved the same key it takes *theirs* and records the dropped position as a note in the merge output. Geometry conflicts are never worth a human's time.
- After any merge, the editor re-validates. Keys in the sidecar that no longer exist in code become orphans, and code entities without layout get auto-placed.

**Visual diff.** `spicy diff <rev>` (and the editor's `:diff <rev>`) renders both revisions with the §8.3 overlay: green added, red removed, amber changed, and dashed re-routed. The two sides are matched by name keys, not by geometry, so a part that moved *and* changed shows as one amber part with a motion arrow. Deterministic layout (§5.3) makes the renders reproducible in CI, so a PR bot can post "schematic diff" images next to the text diff.

### 6.4 KiCad import (`.kicad_sch` → drawn blocks)

Target KiCad 8, 9 and 10. KiCad 10.0 shipped 2026-03-20, and its schematic `version` is 20260306; KiCad 9's is 20250114 and KiCad 8's is 20231120. The published S-expression spec is stale: it lacks `exclude_from_sim`, `dnp`, `bus_alias` and `netclass_flag`. The writer, `sch_io_kicad_sexpr.cpp`, is the real spec (§13).

**Connectivity from KiCad itself.** Do not re-implement KiCad's label, bus and hierarchy resolution. Run `kicad-cli sch export netlist --format kicadsexpr`. It gives fully resolved nets plus each component's UUID (`tstamps`), sheet path, fields and pin types. Parse the `.kicad_sch` files only for geometry. This removes the riskiest part of import: getting net-name priority (global label > power symbol > local label > hierarchical label > sheet pin) and bus expansion subtly wrong.

| KiCad | Ours | Notes |
|---|---|---|
| Hierarchical sheet file | BlockDef (one `.ckt` + `.layout`) | Sheet name → block name (PascalCase) |
| Sheet instance (`sheet` + instance path) | Instance of that block | Multiple instances of one sheet file become multiple instances of one block |
| Sheet pins / hierarchical labels | Ports | Direction from the pin type (input/output/bidir/passive) → role. A bus-typed pin becomes a vector or interface port. |
| Symbol instance | Instance | Reference `R1` → name `r1`. `lib_id` → PartDef via a mapping table (`Device:R` → `Resistor`), with unknown symbols becoming generic parts with named pins. |
| `Value` field | Parameter literal | `10k` → `10k`. Tolerance is taken from a `Tolerance` field if present. |
| `Sim.*` fields | Model binding and parameters | `Sim.Device`/`Type`/`Params`/`Pins`/`Library`/`Name` map onto the PartDef's model binding. `Sim.Pins` ("1=+ 2=-") gives the pin-name map. The `exclude_from_sim` attribute (formerly `Sim.Enable`) is honored. |
| Per-instance values of a reused sheet | Parameters bound at instantiation | KiCad 8/9 store only reference and unit per sheet instance; Value is shared. The known workaround, `Value="${RGAIN}"` resolved from a field on each sheet symbol, maps exactly onto a block parameter (`CeAmp(gain: …)`). KiCad 10 variants map to per-instance overrides. |
| `.wbk` simulation workbook (JSON: tabs with `commands`, `traces`, `measurements`) | `setup` + ad-hoc probes | Each tab becomes a setup. Traces become probes, promoted to named probes only if a measurement uses them. |
| Wires + junctions + labels | **Net statements** (connectivity computed exactly as KiCad would) + **net styles and routes** (geometry → sidecar) | Label-named nets keep their names. Unnamed nets get minted names (§2.2). |
| Global labels, power symbols | Nets passed as ports / rail style | Since KiCad 8, power symbols connect globally by their Value. Global connectivity is made explicit (R6): each global net becomes a port threaded down the hierarchy, or a project-level interface. The import report lists every such rewrite. |
| Bus, bus alias, group bus `{…}` | Vector port / **interface type candidate** | A named group bus `vcc{PWR}` expands in KiCad to nets `vcc.pos`, `vcc.gnd`, the same dotted naming as our interface members. A bus alias (a `bus_alias` token in KiCad 6–9, `.kicad_pro` `schematic.bus_aliases` in KiCad 10) is proposed as an `interface` declaration, which the user confirms. Direction and roles have to be added by hand; KiCad buses are untyped. |
| Positions, rotations, mirroring | Sidecar `sym` lines | KiCad mm → our grid (50 mil = 1.27 mm matches KiCad's default schematic grid) |
| Symbol UUIDs | Sidecar `kicad` provenance lines | Lets a re-import from an updated KiCad file merge by UUID instead of duplicating |
| Text notes | Sidecar notes, or doc comments when anchored inside a part's bounding area | |
| SPICE directives in text items | A `setup` statement where parseable, otherwise a note | |

Import is lossless **for everything that has meaning in our model**, and the geometry is kept. Lost: title blocks and paper size (kept as a sidecar note), symbol field *positions* for fields we don't render, graphic lines and shapes. The latter are kept as sidecar `note`-level decorations only if we add a graphics primitive, which is deferred.

### 6.5 KiCad export (drawn or generated blocks → `.kicad_sch`)

For people who need a KiCad schematic, e.g. to continue into PCB layout, which is out of our scope (vision):

- **One `.kicad_sch` per BlockDef**, with sheets for instances and sheet pins for ports.
- **Deterministic UUIDs:** UUIDv5(project namespace, instance path). Re-exporting after a change modifies only the affected symbols, so the *exported* files diff well too, and KiCad-side data that keys on UUIDs survives re-export. Zener/pcb does exactly this: it derives symbol UUIDs from the canonical component path (`crates/pcb-kicad-sch/src/identity.rs`).
- **Re-exporting into a KiCad schematic the user has since rearranged in KiCad.** Adopt Zener's reconciliation model (`pcb-kicad-sch/DESIGN.md`). Accept any KiCad hierarchy whose connectivity is *equivalent* to ours. Keep the user's reorganizations. Apply only minimal, checked, reversible repairs where connectivity differs. The only metadata needed is the instance path on each symbol (our `Spicy.Path` field).
- **Nets:** our routes are written as KiCad wires. Nets drawn as labels or rails become local labels or power symbols. Every net also gets an explicit label with our net name, so KiCad's netlist names match ours.
- **Interfaces** become KiCad bus aliases plus group buses on sheet pins, where KiCad can express them. Otherwise they are expanded to individual pins with `iface.member` names.
- **Values:** `Value` is the committed value. Custom fields carry `Tolerance`, `Equation` (the derivation), `Rationale` (the doc comment) and `Spicy.Path` (the instance path). `Sim.*` fields make the result simulatable in KiCad's ngspice too.
- **Block parameters** go on the sheet symbol as custom fields, with the child's `Value` fields written as `${PARAM}` references. KiCad resolves text variables symbol → sheet-instance fields → ancestor sheets → project (`SCH_SYMBOL::ResolveTextVar`), so a block used twice with different `gain` exports correctly and even simulates in KiCad. Project-wide parameters go in `.kicad_pro` `text_variables`.
- **Setups and named probes** are exported to a KiCad `.wbk` workbook: one tab per setup, `.tran`/`.ac` commands, probes as traces, measures as `measurements` where they are expressible in KiCad's syntax.
- **Specs and knobs** have no KiCad equivalent. They are exported as a text block on the root sheet, human-readable only.
- **The generator name** is our own (KiCad asks third-party writers not to claim `"eeschema"`). **A KiCad round trip is not a design goal.** Re-importing an exported file re-links through `Spicy.Path` and the UUIDs, but anything edited in KiCad beyond values and geometry is imported as a proposal for review (§8.3), not merged silently.

### 6.6 What survives each direction

| Information | Ours → KiCad | KiCad → ours |
|---|---|---|
| Connectivity | ✔ exact | ✔ exact (computed from wires and labels) |
| Hierarchy, multi-instance sheets | ✔ | ✔ |
| Instance names | ✔ as reference + `Spicy.Path` | ✔ from the reference |
| Geometry | ✔ | ✔ |
| Values (committed) | ✔ | ✔ |
| Equations, rationale, tolerance | as custom fields (text) | ✔ if our custom fields are present; otherwise tolerance from the `Tolerance` field only |
| Interfaces | as bus aliases / group buses, or expanded | proposed from bus aliases |
| SPICE models | ✔ `Sim.*` | ✔ `Sim.*` |
| Setups, named probes | `.wbk` workbook tabs and traces | ✔ from `.wbk` |
| Block parameters (per instance) | sheet-symbol fields + `${VAR}` | ✔ when that pattern is used |
| Knobs, assumptions, specs | text note only | — |
| Generated structure (loops) | unrolled | — (arrives as flat, drawable code) |
| Pin types for ERC | ✔ from PartDef pin kinds (KiCad has its own pin types) | ✔ KiCad pin electrical types → our pin kinds |

---

## 7. The vim-modal workflow

### 7.1 Modes are regions of the language

The vision has two major modes, edit and simulate. The design choice is that **each major mode owns a region of the language**, so the same verbs (`c`, `d`, `o`, `y`, `p`) act on different statements depending on the mode:

| Major mode | Edits these statements | Overlays shown | Typical verbs |
|---|---|---|---|
| **EDIT** | the body: instances, nets, parameters, knobs (`matched`) | values, equations, ERC | place, wire, change value, rename, extract |
| **SIMULATE** | the contract and benches: `assume`, `spec`, `probe`, `setup`, `waive` | results, spec badges, bench ghosts | probe, add spec, change setup, run, counterexample |

Inside each major mode there are vim's minor modes:

| Minor mode | Key | In EDIT | In SIMULATE |
|---|---|---|---|
| Normal | `Esc` | navigate and select; operators | navigate; operators on probes and specs |
| Visual | `v` (region), `V` (whole net), `<C-v>` (rectangle) | select parts and nets for operators or select-to-ask | select probes, or a region to ask about |
| Place ("insert") | `a` / `i` | fuzzy part picker, then the ghost follows the cursor | place a probe or bench element |
| Wire | `w` | route from the pin under the cursor | draw a differential probe (`V(a, b)`) |
| Command | `:` | language statements and editor commands | the same, usually `spec`/`setup` statements |

`<Tab>` switches major mode. The status line always shows `EDIT` or `SIM`, the block path, dirty state, and a spec summary (`1✓ 1✗ 1◐`).

**Why tie modes to language regions?**

- It answers vision §2's "what does each mode edit" with a rule rather than a list.
- It gives protection against accidental edits: clicking around while inspecting results cannot move a part or change a value.
- It keeps the contract, which is what reviews and the AI focus on, from being edited by stray structural gestures.

The cost: occasionally you want to tweak a value while looking at results. `cv` stays available in SIMULATE for parameters, as a deliberate exception, because "change a value and watch the spec" is the core loop (walkthrough §2, "while editing").

### 7.2 Keyboard-first placement and wiring

| Keys | Action | Resulting text edit |
|---|---|---|
| `a r 4.7k ⏎` | add a resistor at the cursor with value 4.7k; name minted (`r3`), editable inline | `let r3 = Resistor(4.7k);` |
| `a` then fuzzy text (`npn 3904`) | part picker over the library, with sim-model availability shown | `let q2 = Npn(model: q2n3904);` |
| `w` + hint letters | wire from the pin under the cursor; each reachable pin shows a 1–2 letter hint (easymotion/hop style), so you type the hint to finish | `net` member add, create, or merge |
| `w` + `hjkl` | manual routing: route segments, `⏎` to finish | geometry only (sidecar), plus the net edit |
| `r` / `R` / `m` | rotate / rotate back / mirror | sidecar only |
| `hjkl`, `HJKL` | move the cursor / move the selection by one grid step (pins it) | sidecar only |
| `cv`, `c=`, `ct`, `cr`, `cn` | change value, equation, tolerance, rationale, name | one expression, one doc comment, or a rename refactor |
| `dd`, `dw` | delete instance, delete wire segment | remove a statement, or edit or split a net |
| `yy`, `p` | yank / paste | registers hold **language text** (§7.4) |
| `.` | repeat the last edit (e.g. "add 100n decoupler to this pin") | same patch, re-targeted |
| `u`, `<C-r>` | undo / redo | text history of the block (shared with the code view) |
| `gd`, `gu`, `gc`, `gs` | into sheet, up, to code, to the specs touching the selection | — |
| `]d` `[d`, `]f` `[f`, `]h` `[h` | next diagnostic, flag (⚑), proposal hunk | — |
| `K` | hover card | — |
| `ga` | ask the AI about the selection (§8) | — |

**Undo is text undo.** Each gesture is one text patch, so the undo stack *is* the code history. The code view (the embedded neovim, as `spicy_cli` already embeds `nvim --embed`, `crates/spicy_cli/src/tui/nvim.rs`) and the schematic share it. Undoing in either view undoes the last patch from either.

### 7.3 The command line is a language prompt

Anything typed after `:` is either an editor command (`:rename`, `:extract`, `:run`, `:set propagation=cascade`) or **a language statement, upserted into the current block**:

```
:let gain = 5                          → upsert parameter gain; propagation per policy
:let c2 = Capacitor(100n, kind: X7R)   → new instance, auto-placed near the cursor
:net base += c2.a                      → add a member (sugar for editing the net statement)
:spec thd = tran(audio).thd(V(out)) < 1%   (SIM)
:assume temp in 0°C..50°C             (SIM) → narrows the range knob
:probe ve = V(emit)                    (SIM)
```

The prompt has the compiler's completion, and it shows type and unit errors inline before `⏎`. That makes it a REPL scoped to the block. It also means **there is no separate command vocabulary to learn** for anything the language can say, and every command in the history is a valid line of code.

### 7.4 Registers hold language text

- **Yank** on a selection puts into the register the statements of the selected instances, their internal nets, and a comment listing the boundary nets. It is a slice in the §8.1 format.
- **Paste into a sheet** upserts those statements. Name clashes are renamed (`r1` → `r1_2`) and boundary nets are left unconnected, highlighted. The layout comes along if the register also carries the sidecar lines, which yank adds as a trailing comment block that paste understands.
- **Paste into the code view or a chat** gives plain, readable code.

So copying a bias network from one project into another, into an email, or into the AI chat all use the same text. This also works as a quiet tutorial: users see the language every time they copy.

---

## 8. AI integration

### 8.1 What "select to ask" sends

Selecting a region (`v`, then `ga`, or a mouse drag) produces a **context bundle** with four parts:

1. **Model slice, as language text.** A compilable fragment containing:
   - the selected instances' statements;
   - the `net` statements touching them, with members outside the selection elided and counted;
   - the transitive closure of the parameters their values depend on (pruned at project level, which is summarized);
   - the contract items whose measures touch the selected nets.

   Every line keeps its source location, so the AI can cite it and the patch can be anchored. A `..` in a net marks members outside the slice (Rust's rest-pattern reading). The slice compiles with those nets treated as open stubs (R17).
2. **Rendered image** of the selection as the user sees it: same labels, same overlays, SVG rasterized. For a multimodal model the picture carries the topology in the form engineers read. The instance labels in the image are the same names as in the text, so the two can be cross-referenced.
3. **Results for exactly those entities:** operating point of the selected nets, spec results (bracket, verdict, method, contributors, counterexample; spec doc §9), relevant sensitivities, active flags and ERC diagnostics.
4. **Context:** block path and instance, the block's I/O page as text, project env and confidence default, the chat thread so far.

For the CE amp, selecting `c_in`, `r1`, `r2`, `q1` and asking "why does bass fail?" sends:

```rust
// slice of amps::CeAmp (src/ce_amp.ckt) · instance board.amp · selection: c_in r1 r2 q1
assume temp in -10°C..60°C;                                    // :10
assume life in 0yr..10yr;                                      // :12
/// Don't cut the bass.
spec bass = ac(V(out) / V(input)).f_low(-3dB) <= 30Hz, yield 99.9%;   // :21
/// Divider holds V_B ≈ 2.1 V.
let r1 = Resistor(47k ± 1%);                                   // :33
let r2 = Resistor(10k ± 1%);                                   // :34
/// f_L = 1 / (2π · C_in · R_in) ≈ 20 Hz with R_in ≈ 7.9 kΩ.
let c_in = Capacitor(1u ± 20%, kind: CapKind::Electrolytic);   // :39
let q1 = Npn(model: q2n3904);                                  // :40
net vcc_rail = [r1.a, ..];    // +2 outside: vcc.pos, rc.a     // :43
net gnd      = [r2.b, ..];    // +2 outside: vcc.gnd, re.b     // :44
net in_ac    = [input, c_in.p];                                // :45
net base     = [c_in.n, r1.b, r2.a, q1.b];                     // :46
net coll     = [q1.c, ..];    // +2 outside: out, rc.b         // :47
net emit     = [q1.e, ..];    // +1 outside: re.a              // :48
```
```yaml
results:
  op: { base: 2.10 V, emit: 1.45 V, coll: 5.50 V, q1.ic: 1.38 mA }
  bass: { verdict: FAIL, nominal: 20.1 Hz, realistic_3sigma: 31.7 Hz, abs_worst: 33.4 Hz,
          mc: "177/20000 fail at (temp=-10°C, life=end)",
          contributors: { life: 46%, c_in.tol: 46%, q1.beta: 4%, temp: 3% },
          counterexample: { temp: -10°C, life: end, c_in.tol: -20%, q1.beta: 100 } }
```

**Why a language slice and not JSON or a netlist:**

- It is the same representation the AI will edit, so it reads and writes one format.
- It carries the doc comments (the *why*), which netlists drop.
- It is compact: the whole CE amp is about 40 lines.
- It is checkable: the editor can compile the slice and prove the AI saw a consistent fragment.

### 8.2 How AI edits flow back

The AI replies with a **keyed patch**: language statements plus explicit deletes and renames. It does not send line-number diffs.

```rust
patch amps::CeAmp {
    /// f_L worst case 15.2 Hz with −20% tolerance and −20% aging (was 1 µF: 33.4 Hz).
    let c_in = Capacitor(2.2u ± 20%, kind: CapKind::Electrolytic);
}
```

(`patch` is tool-protocol framing, not necessarily language syntax.) Applying it follows three rules:

- **Same key → replace that statement in place.** The CST edit keeps the surrounding comments and position. If the AI omits the doc comment, the old one is kept. If it writes one, it replaces the old one.
- **New key → insert** into the right section. New instances are auto-placed next to the instances they connect to.
- **`delete k;`** removes a statement. **`rename a -> b;`** runs the rename refactor.

The patch is applied to a **proposal**: a copy-on-write fork of the model. The proposal is compiled and ERC-checked, and the specs are re-evaluated on it: at once from saved slopes, and in the background with the loop. Only then is it shown.

### 8.3 Reviewing a proposal in the schematic

```
┌ board › amp : CeAmp · PROPOSAL from AI · "fix bass" · 1 hunk ───────────────── SIM ┐
│                                     VCC ▲                                          │
│                      ┌──────────────────┴───────┐                                  │
│                     ┌┴┐ r1                     ┌┴┐ rc                              │
│                     └┬┘                        └┬┘                                 │
│ ┏━━━━━━━━━━━━━━━━━┓  │                          ├──────────● out ▷                 │
│ ┃ c_in 1µ → 2.2µ  ┃  │                        │╱                                   │
│ ┗━━━━━━━━━━━━━━━━━┛  │                        │                                    │
│ ▷ input ──┤├─────────●────────────────────────┤  q1                                │
│                      │                        │╲                                   │
│                     ┌┴┐ r2                     ┌┴┐ re                              │
│                     └┬┘                        └┬┘                                 │
│                      ▽                          ▽                                  │
├────────────────────────────────────────────────────────────────────────────────────┤
│ spec     before                  after                    how                      │
│ bass     ✗ 31.7 Hz (0.9% fail)   ✓ 15.2 Hz abs. worst     f_L ∝ 1/C, loop re-run   │
│ bias     ◐                       ◐                        unchanged (no DC effect) │
│ gain_ok  ✓                       ✓                        unchanged                │
│ auto     c_in voltage ≤ 80% ✓    ✓                        new part's rating: 25 V  │
├────────────────────────────────────────────────────────────────────────────────────┤
│ a accept · x reject · ]h next hunk · gc code diff · e edit before accepting        │
└────────────────────────────────────────────────────────────────────────────────────┘
```

- Added instances are drawn green. Removed ones are red and struck through. Changed values are amber, showing old → new. Re-routed nets are dashed.
- **The verdict delta is part of the diff.** An AI edit that fixes one spec and breaks another shows the breakage *before* acceptance.
- Accepting commits the patch as one undoable edit. The chat keeps a link to the edit, and the edit keeps a link to the chat message, so project-bound chats (vision §1) stay tied to design history.
- **Per-hunk review** (`]h`, `a`, `x`) works like `git add -p`. Hunks are per keyed statement, so they are naturally independent.

### 8.4 Why the language must be designed for this

AI edits are reliable when the edit's *text* determines its *effect*, locally. Each property below maps to a requirement in §10:

| Property | Why it matters for AI | Requirement |
|---|---|---|
| Stable, explicit names for every entity | The AI refers to `q1`, `base`, `bass` across turns. Positional or anonymous things can't be referenced. | R1, R2, R4 |
| Keyed statements | Patches apply by name, immune to line drift and reformatting | R4 |
| Order independence | The AI can put a statement anywhere, and the effect doesn't depend on placement | R5 |
| No action at a distance | A patch's effect is visible in the patch: no implicit connection by name, no global mutation, no reaching into another block's internals | R6 |
| Canonical formatting | The same meaning always gives the same text, so diffs are minimal and review is honest | R14 |
| Units and tolerances in literals | `4.7k` vs `4.7`, or `1u` vs `1`, can't be confused silently. The compiler catches it. | R9 |
| Doc comments as rationale | The AI reads *why* before changing, and writes *why* with its change | R9 |
| Symbolic references (no strings for names) | `V(coll)` survives a rename; `V("coll")` wouldn't | R13 |
| Partial compilation | The AI's slice compiles standalone with stubs, and a broken patch gets a precise error to retry against | R17 |

**Evidence that semantic, name-based text is the right AI interface:**

- SchGen (Luo et al., arXiv 2605.30345, May 2026) generates schematics with an LLM. It found that a "semantically grounded code representation … with relative placement and pin-name-based wiring" beats geometry-focused representations on connectivity accuracy and functional correctness.
- Schemato (arXiv 2411.13899) has an LLM emit LTspice `.asc` directly, a format coupled to geometry. 76% of its outputs compile. The deterministic, layout-engine-based Weave reports 100% *verified connectivity* on the same kind of task (117 public circuits; 88.4% on a 3,460-circuit corpus). The metrics differ, but the direction is clear.

The lesson matches §4.2: let the model speak in names and connections, and let a deterministic layout engine own the coordinates.

**AI and generated blocks.** Structural edits on generated blocks (§3.7) go through the same channel. The AI receives the generator code and the gesture the user attempted, and returns a patch to the *generator*. The proposal view shows the code diff next to the re-rendered schematic, so the user checks the result visually, not only the code.

---

## 9. Parametric values and change propagation

### 9.1 Representation: committed value + derivation

Vision §4 asks for values that carry an equation, a comment and margins, with cascade or flag propagation. The representation that makes both policies fall out naturally:

```rust
/// Gain ≈ R_C / R_E, snapped to E24.                      ← rationale (doc comment)
let rc = Resistor(4.7k ± 1% from snap(E24, r_e * gain));
//                ^^^^         ^^^^^^^^^^^^^^^^^^^^^^^^^
//                committed    derivation: why the value is what it is
```

A parameter takes one of four forms:

| Form | Text | Value used by the engine | When |
|---|---|---|---|
| **Literal** | `47k ± 1%` | the literal | a free choice |
| **Live expression** | `r_e * gain` | recomputed every time | internal parameters that are not purchased parts (targets, derived currents) |
| **Committed + derivation** | `4.7k from snap(E24, r_e * gain)` | the committed literal | purchased parts: you can't buy 4.6 kΩ, and the BOM must be explicit |
| **Solver-chosen** | `?` (or `? from …` as a starting point) | the solver's choice, committed on accept | v1 §4.3, v2 A9 |

The committed form is the key. The text holds *both* what is built (4.7k) and why (the derivation). The engine uses the committed value. The derivation is a **check**: it holds if `eval(derivation)` equals the committed value, with a `snap(...)` meaning "equal after snapping", or, without `snap`, within a tolerance the project sets (default: inside the part's own tolerance).

### 9.2 Cascade and flag are policies over the same data

Suppose `gain` changes from 4.6 to 5.

- **Flag.** Nothing rewrites. `rc`'s derivation now evaluates to `snap(E24, 5.0k) = 5.1k` ≠ 4.7k, so `rc` gets ⚑ on the sheet and in the diagnostics list (`]f` jumps through flags). The hover card shows the new value, and `:accept` (or `A` on the card) rewrites that literal.
- **Cascade.** The editor computes the same rewrites for every out-of-date committed value, in dependency order, and **applies them as one patch**. The patch is shown as a diff, and it is one undo step.

So:

- The difference between the policies is *whether the rewrite patch is auto-applied or proposed*. The data model is identical, and switching policy never changes a file.
- The **cascade diff is a real text diff**, so it is reviewable in git, blame-able, and revertible.
- Live expressions (no committed literal) always cascade, by definition. The diff view still lists their before/after values (below), but there is no text change.

Policy scope: a project default (`project.toml: propagation = "flag"`), a per-block override, and a per-parameter override (an attribute, e.g. `#[propagate(cascade)]`). The recommended default is **flag for committed values**. Purchased-part values changing silently is the failure mode engineers fear most, and the flag list doubles as a to-do list.

### 9.3 The cascade diff view

After `:let gain = 5` in cascade mode:

```
┌ cascade from gain: 4.6 → 5 ─────────────────────────────────────────────── 1 patch ┐
│ VALUES                                                                             │
│  gain           4.6        → 5          edited                                     │
│  rc             4.7k       → 5.1k       snap(E24, r_e·gain) = snap(5.0k)           │
│  spec gain_ok   4.6 ± 5%   → 5 ± 5%     limit follows the parameter                │
│ RESULTS  (fast re-evaluation from saved slopes; full loop running…)                │
│  bias     min 4.66 V ✓   →  ≈ 4.09 V ✗   limit 4.5 V      ← NEW FAILURE            │
│  gain_ok  4.59 ✓         →  ≈ 4.98 ✓                                               │
│  bass     ✗ 31.7 Hz      →  ✗ 31.7 Hz    unchanged (R_C is not in R_in)            │
├────────────────────────────────────────────────────────────────────────────────────┤
│ u undo cascade · gc view patch · ]c next changed part · gs open failing spec       │
└────────────────────────────────────────────────────────────────────────────────────┘
```

(The result numbers come from scaling the walkthrough model, for illustration only: V_C,min ≈ 11.4 V − 1.434 mA × 5.1 kΩ.)

- On the sheet, changed parts get an amber ring for a few seconds, and they stay listed under `]c`.
- The dependency graph is navigable. `gD` on a parameter lists everything that depends on it, and highlights those parts on the sheet (as "find references" does).
- The diff shows **value changes and verdict changes together**. A cascade is judged by its effect on the specs, not by the literals.

### 9.4 Margins

"Design margins (offsets and buffer zones)" (vision §4) become two things in the text:

- **Margin on a check**: `spec bias = dc(vc) in 4.5V..6.5V, margin 0.2V;`
  - Inside 4.7…6.3 V it is PASS.
  - Inside the band it is PASS-in-margin, shown amber, and it is reported as margin consumed.
  - Outside the band it is FAIL.
  - The same applies to a parameter's validity range: `let v_e = … where v_e in 1V..2V, margin 0.1V`.
- **Offset on a target**, e.g. aiming for a gain of 4.7 to leave room around 4.6. This is just an expression (`let gain_design = gain * 1.02;`, with a doc comment saying why). It gets no special syntax, so it is visible as a formula.

Editor rendering: a check with a margin shows a **gauge**, `4.5 ▕▒▒│████████████│▒▒▏ 6.5` with the result bracket drawn on it, in the I/O page and in hover cards. Margin consumed is also what the engine's contributor ranking explains: "`life` uses 46% of the margin".

---

## 10. Hard requirements on the language (hand-off to the language thread)

Each requirement is stated independently of syntax, and says which editor feature breaks without it.

**Identity and naming**

- **R1. Every instance has an explicit, unique local name, and instance creation is syntactically distinct from a pure function call.** (Spade's `inst` keyword is the precedent. Its compiler rejects a missing or extra `inst` with a fix-it.) There are no anonymous instances in drawable blocks: no chain syntax that creates unnamed parts (v1's `a -- r1 -- b` style), and no inline construction inside a connection. *Breaks without it:* sidecar keys, probes, AI references, select-to-ask.
- **R2. Generated instances get deterministic index paths** (`rs[3]`, `ch[2].r1`). Growing a bound appends and never renumbers existing indices. Elaboration is deterministic: no dependence on hash-map iteration order. *Breaks:* placement stability of generated blocks (§5.3).
- **R3. Nets are first-class and nameable.** There is a net-centric statement (`net name = [pins…]`) with set semantics, so connection order and duplicates don't matter. That statement has a **unique canonical formatting**: one statement per net, members sorted by a fixed rule. Edge-style `connect(a, b)` may exist for generated code, but the canonical form of a drawable block uses only net statements. *Breaks:* canonical export (§4.3), wire gestures as local edits (§3.4), minimal diffs.
- **R4. Every statement in a block body is keyed**, by instance, net, parameter, spec, probe, setup, knob group or waiver name, with one namespace per kind per block. Keys are what patches, upserts, sidecar entries and the proposal-hunk review operate on. *Breaks:* AI patches, paste, `:` upsert.

**Semantics that keep edits local**

- **R5. Order independence inside a block.** Declarations may appear in any order and refer to each other, like Rust items, not like `let` statements. Cycles in parameter definitions are errors. Circuits are full of feedback. Spade, whose `let` is sequential, needs a `decl x;` forward declaration for every feedback loop. Net statements with set semantics make that ceremony unnecessary. *Breaks:* insertion of new statements anywhere, name-keyed upsert.
- **R6. No action at a distance.**
  - No implicit connection by matching names (no global nets by label text).
  - No implicit global state except the explicitly declared environment knobs.
  - No writing into another block's internals from outside. If overrides must exist, they are explicit statements at the instantiation site, and the editor shows them on both sheets.
  - No semantics that depend on file layout.

  *Breaks:* AI reliability, reviewability, P5.
- **R7. Pins and ports are typed for ERC.** Every PartDef pin has an electrical kind. Every port has a role or direction (source/sink, in/out/bidir). Connecting incompatible roles is a type error, with the pin spans in the diagnostic. *Breaks:* ERC-in-place (§3.2), auto-placement's signal-flow direction (§5), symbol side assignment.
- **R8. Interfaces are nominal bundle types.**
  - Named members, usable as ports and as net members.
  - Connecting two interfaces is one statement meaning member-wise connection.
  - Member access works (`vcc.gnd`).
  - Vector ports and vector buses exist (`[Pin; 8]`), with slicing and concatenation (`v_ref[1..8]`, `[v_top, v_ref[1..8]]`).
  - Interfaces can declare automatic checks.

  *Breaks:* bus and harness rendering (§3.3), KiCad bus mapping (§6.4), drawable arrays (§4.5).

**Values**

- **R9. Physical values.**
  - Units in types.
  - SI-prefixed literals with unit inference from the expected type (`Resistor(4.7k)`).
  - Tolerances in literals (`± 1%`).
  - `?` for solver-chosen values.
  - The **committed-value + derivation** form (`4.7k from expr`).
  - **Doc comments attach to the next item and are part of the model** (rationale).
  - `margin` on checks.

  *Breaks:* parameter cards (§3.5), cascade/flag (§9).
- **R10. Knobs are explicit or derivable by rule.**
  - Range knobs are declared (`assume`/`env`).
  - Statistical knobs are implied by tolerance literals, named by path.
  - Correlations (`matched`) are keyed statements.

  *Breaks:* the knob panel (§3.6), contributor display.
- **R11. The contract is part of the block.** Assumptions, specs, probes, setups and waivers are keyed statements inside the block, with measure expressions over symbolic probes, nets and pins. Separate bench blocks may *also* exist. *Breaks:* the I/O page as a view (§3.9).

**Tooling-grade compiler**

- **R12. Drawability is decidable and reported per block.** The flat subset is a syntactic class:
  - declarations, net statements, parameter `let`s, contract items;
  - array instances with *vectorized* connections (slices, concatenation, broadcast);
  - no `for`, no `if`, no calls that produce structure.

  *Breaks:* D6 as designed here.
- **R13. Every model entity carries its source span, and every value carries provenance.** Provenance means which literal(s) it came from, and whether any of them depend on a loop index. References to entities are symbolic, never strings. *Breaks:* jump-to-code, errors on the sheet, value editing in generated blocks (§3.7), rename safety.
- **R14. A lossless concrete syntax tree** (comments and whitespace kept; rowan-style, as in rust-analyzer) **and an official, idempotent formatter.** The formatter is stable under small edits: editing one statement never reflows others. Editor edits are CST patches. *Breaks:* round-trip of drawable blocks without destroying hand formatting, and readable diffs.
- **R15. No placement or graphics in the language.** Attributes are for semantic metadata only (part number, sim model, propagation policy). The only presentation-adjacent thing the language owns is port *role*, which the editor uses to infer symbol side. *Breaks:* P1 and diff hygiene.
- **R16. Part definitions bind pins to symbols and models.** A PartDef maps its named pins to symbol pins and to SPICE node order, and declares sub-unit groups for multi-unit parts (e.g. `u1.a`, `u1.b`, `u1.pwr` for a dual op-amp). Enum-valued parameters may select a symbol *variant* (e.g. `CapKind::Electrolytic` gives the polarized symbol). *Breaks:* rendering, KiCad interop.
- **R17. Error-tolerant elaboration.** A file with a syntax or type error still elaborates to a partial model, with the broken statements marked, so the schematic keeps rendering while the user types in the code view. Slices compile standalone with stub boundary nets. *Breaks:* live two-view editing, AI retry loops.
- **R18. Stable rename refactoring is expressible.** Every reference to a name is resolvable, and there are no dynamic name constructions except index paths. That gives the LSP and the editor a complete rename. *Breaks:* identity (§2.2).

**Where these requirements come from**

| Source | Idea | Requirement |
|---|---|---|
| Spade | `inst` marks instantiation; `fn` stays pure | R1 |
| Spade | mixed-direction bundles are structs with `inv` fields; a per-field linear check reports "resource.x is unused (an inverted wire which must be set)" | R7, R8 (a power sink left unconnected is the same error, with the same per-member precision) |
| Spade | `where … else "message"` | spec and check messages (`spec … else "…"`, v2 Part E) |
| Spade | `(* src = "file:line" *)` on every generated instance; an `InstanceMap` in the saved compiler state; the Surfer plugin maps a hierarchical path back to source types and names | R13, and results-in-place (§3.8): the same path→source map drives our overlays |
| Spade (negative) | generated instance names are `<unit>_<counter>`: unstable under insertion | R2 |
| Spade | named arguments (`$(a: x)`, recently also `{ … }`) with `b` as shorthand for `b: b` | readable part instantiation, `Resistor { r: 4.7k }` |
| atopile | `new T[N]` creates instances, and loops only alias them | R12's T1 tier, R20 |
| atopile (negative) | layout keyed by address; a rename deletes the old footprint | R18, and rename detection (§2.2) |
| atopile | interfaces connected with one `~` statement | R8 |
| OpenModelica (negative) | unnamed `connect` equations defeat diff/merge; defaults leak into the text | R3, R4, R14 |
| edg-ide (negative) | no GUI delete or rename, because Python can't be statically analyzed | R4–R6, R18 |

**Nice to have**

- **R19.** A block parameter list distinct from its port list. Parameters render as symbol fields, ports as pins.
- **R20.** Instance arrays of *blocks* (not just parts) with vectorized port hookup, the Altium Repeat equivalent (§4.5).
- **R21.** A `use`/module system where file = module, so the editor can map sheet ↔ file trivially.

---

## 11. Alternatives considered

| # | Question | Chosen | Rejected alternative | Why |
|---|---|---|---|---|
| A1 | Identity | Name paths + rename refactor + rename detection | Hidden UUIDs in code (KiCad), or a UUID sidecar map | UUIDs make text unreadable to reviewers and the AI, and mean nothing in chats. atopile shows address-only identity works in practice. Its gap (rename loses the layout) is closed here by the refactor and the detector. |
| A2 | Drawn-block storage | Flat-subset code + sidecar | Modelica inline annotations; Zener trailing comments; a schematic file as truth | §4.2: AI token budget, review hygiene, one semantic format |
| A3 | Canonical connectivity | Net-centric statements | Edge-centric (`a ~ b`) or chains (`a -- r -- b`) | Edge sets have many equivalent texts, so there is no canonical export and diffs are noisy. Chains hide pins (v2 Part E) and create anonymous parts. |
| A4 | Label semantics | Labels are presentation; names live in `net` | KiCad: same label text ⇒ same net | Action at a distance, silent typos (R6, P5) |
| A5 | Generative two-way editing | Values through provenance; structure via code or AI | Unroll → edit → re-roll (synthesis) | Unpredictable. Sketch-n-Sketch needs heuristics even for values. |
| A6 | Loops in drawings | Vectorized arrays (T1) drawable | All arrays read-only | Virtuoso and Altium show uniform arrays are a drawing idiom engineers already use |
| A7 | AI edit format | Keyed statement patches (unified diff as fallback) | Line diffs only; JSON edit ops only | Line diffs break on drift. JSON ops are a second language for the AI to learn. Keyed language patches get both robustness and a single format. |
| A8 | AI context format | Compilable language slice + image + results | JSON model dump; netlist; image only | Same format in and out, carries rationale, compact, checkable |
| A9 | Contract location | In the block (the I/O page is a view of it) | Separate testbench blocks only (Cadence ADE) | The I/O page is the contract (vision §3). Bench blocks remain possible for system benches. |
| A10 | Mode semantics | Major modes own language regions | Modeless editor; modes as UI panels | Gives a rule rather than a list, and prevents accidental edits |
| A11 | Auto-layout family | Layered (Sugiyama/ELK) + analog idioms + rails | Force-directed (atopile's visualizer uses ForceAtlas2) | Force layouts ignore signal direction and pin sides, so they can't produce schematic conventions |
| A12 | Propagation data | Committed value + derivation (one model, two policies) | Separate "cascade" and "flag" parameter kinds | Switching policy would rewrite files, and cascades wouldn't produce text diffs |
| A13 | Native format | Language + sidecar; KiCad for interop | `.kicad_sch` as the store | KiCad can't hold the semantics; geometry-defined connectivity; poor diffs |
| A14 | Ad-hoc probes | User-local until promoted | Always code | Exploratory clicking would dirty the design and pollute diffs |

---

## 12. Open questions

1. **Is rename detection good enough, or do we need optional stable IDs?** Bulk renames done outside our tools (sed, an external refactor) could orphan many layout entries at once. The detector handles one-to-one renames well, but many simultaneous renames with similar connectivity are ambiguous. A fallback: an opt-in `id` column in the sidecar (a UUID per entry), used only to disambiguate. It never appears in code.
2. **A `#[drawable]` guard?** An attribute that makes non-flat constructs a compile error in that block, the way `#![forbid(unsafe_code)]` does in Rust. It would stop a hand edit from silently turning a drawn block read-only. Proposed as optional; the language thread should say whether attributes like this fit their model.
3. **How much formatting authority does the editor take in drawn files?** The proposal only formats statements it writes and leaves hand-formatted ones alone (CST patches). Is it acceptable that a drawn file slowly mixes the user's style with the canonical style? The alternative is "drawn files are always canonically formatted", like gofmt: simpler, but the editor becomes the owner.
4. **T1 syntax and scope.** Will the language thread adopt slices, concatenation and broadcast in net statements (§4.5)? Which common analog arrays fall just outside T1 and deserve a construct? The R-2R ladder fits, because its ends are single-element statements. Arrays whose element *values* vary with the index (binary-weighted resistors) fit if parameters can be arrays. Conditional topology does not fit. A `chain(rs, from, to)` built-in that stays drawable would cover series strings.
5. **Setups: in the block or in bench blocks?** (spec doc §13.2). This proposal puts the default setup in the block, because the I/O page needs it, and allows bench blocks for system-level runs. The editor needs a rendering for bench blocks (a sheet with the DUT as a symbol) either way.
6. **Probe identity** (spec doc §13.3). This proposal: named probes are code objects that reference nets and pins symbolically, so the rename refactor keeps them valid. Ad-hoc probes are user-local. Is that split intuitive, or will users expect every probe to persist?
7. **Default propagation policy** (flag recommended for committed values) and **margin semantics** (a band on checks). Both need a user study with real designers.
8. **Multi-unit parts and pin swapping.** A dual op-amp is one PartDef with sub-units (`u1.a`, `u1.b`, `u1.pwr`). On the sheet these are drawn separately. How unit assignment interacts with KiCad export (separate `symbol` items sharing a reference) and with the AI (which should see `u1.a` as its own thing) needs a concrete design.
9. **Per-instance structural variants** (DNP on one instance of a reused block, KiCad 10 "variants"). Parameters handle value variation. Structural variation needs either `if` (which makes the block generated) or a variant mechanism the flat subset can express.
10. **Chat reference stability.** Chats refer to entities by path. After a rename, the alias table resolves old names. After a *delete*, the chat still shows the old slice text. Should chats snapshot the slice they were given, so that history is self-contained? (Probably yes: store the context bundle with each message.)
11. **Live elaboration performance.** Re-elaborating on every keystroke in the code view needs incremental compilation (salsa, v1 §7.6) for large projects. Up to what design size is a full re-elaboration per change acceptable?
12. **An auto-placement quality bar.** We need a corpus (textbook analog circuits with their "canonical" drawings) and metrics (crossings, bends, idiom recognition rate, signal-flow monotonicity) before claiming readability. Candidate corpus: the walkthrough amplifier, the `circuits/*.spicy` examples, and a set of classic topologies.
13. **Symbol library licensing.** Reusing KiCad's symbol library needs a check of its license terms for redistribution inside our tool. It has not been checked in this thread.
14. **No-connect markers and net ties.** These are ERC-semantic, so they go in code (`nc u1.7;`, `tie gnd_a gnd_d;`), not in the sidecar. The language thread should confirm they are keyed statements.

---

## 13. References

Verification status: "(opened)" means the page or file was read in this research thread. DOIs marked as confirmed were checked through Crossref by the research sub-threads. Anything unverified is marked. Paths into `externals/` refer to the local clones.

### 13.1 Local sources

- Project docs: `../vision.md`, `../archive/engine_v1.md` (D5, D6, §6–7), `../archive/engine_v2.md` (Parts A, E), `../specs.md` (§1–13), `../walkthrough.md` (§1, §5–9; all CE amplifier numbers).
- Existing embedded-neovim editor: `crates/spicy_cli/src/tui/nvim.rs` (`nvim --embed`, save notifications over RPC).
- atopile (`externals/atopile`, commit 619eda7):
  - `atopile_address` join: `src/faebryk/exporters/pcb/kicad/transformer.py:300-330`, `src/faebryk/libs/part_lifecycle.py:684-689`
  - rename and removal: `transformer.py:2098-2116`
  - net inference: `src/faebryk/libs/nets.py:16-63`
  - layout reuse: `src/faebryk/exporters/pcb/layout/layout_sync.py`, `src/atopile/layout.py:54-161`
  - grammar: `src/atopile/compiler/parser/AtoParser.g4` (loops :153-160, arrays :132-137, connect/bridge :97-106)
  - loop restrictions: `src/atopile/compiler/ast_visitor.py:1790-1862`
  - addresses: `src/atopile/address.py`
  - visualizer (ForceAtlas2, no persistence): `src/atopile/visualizer/web/lib/layoutEngine.ts`
- Spade (`externals/spade`, v0.20.0):
  - `inst` and named arguments: `spade-parser/src/lib.rs:621-683, 1122-1187`
  - `inst`/kind checks: `spade-hir-lowering/src/lib.rs:1996-2076`
  - `inv` ports: `spade-parser/src/lib.rs:1214-1239`; `set`: `spade-parser/src/statements.rs:302-331`
  - linear check: `spade-hir-lowering/src/linear_check/mod.rs:29-73`
  - `where … else`: `spade-parser/src/lib.rs:2070-2178`, `spade-compiler/stdlib/num.spade:72-74`
  - `gen if` only: `spade-parser/src/lib.rs:798-815`
  - `decl`: `spade-parser/src/statements.rs:207-246`
  - instance naming: `spade-mir/src/unit_name.rs:81-158`
  - `src` attributes: `spade-mir/src/codegen/mod.rs:38-43`
  - Surfer plugin: `spade-surfer-plugin/src/lib.rs`
  - doc comments: `spade-ast/src/token.rs:273-276`

### 13.2 Graphical ↔ textual prior art

- Modelica Language Specification, ch. 18 "Annotations" (§18.2 arrays, §18.9 graphical objects): https://specification.modelica.org/master/annotations.html
- OpenModelica ticket #2905, "Provide comment-preserving parsing and unparsing" (fixed 2016): https://trac.openmodelica.org/OpenModelica/ticket/2905
- M. Sjölund, "Evaluating a Tree Diff Algorithm for Use in Modelica Tools," Modelica Conference 2021: https://ecp.ep.liu.se/index.php/modelica/article/download/231/191/134
- OMEdit component arrays forum thread (2014): https://openmodelica.org/forum/default-topic/1451-management-of-arrays-of-components-in-omedit-an-issue
- **Unverified:** how OMEdit and Dymola draw `connect` inside `for`/`if`, and how Dymola preserves formatting.
- LabVIEW VI format: https://labviewwiki.org/wiki/VI
- Simulink:
  - model file formats: https://www.mathworks.com/help/simulink/ug/save-models.html
  - `add_line`: https://www.mathworks.com/help/simulink/slref/add_line.html
  - `arrangeSystem`: https://www.mathworks.com/help/simulink/slref/simulink.blockdiagram.arrangesystem.html
  - SID: https://www.mathworks.com/help/simulink/slref/simulink.id.getsid.html
  - comparison filters (repositioning hidden by default): https://www.mathworks.com/help/simulink/slref/filter-simulink-model-comparisons-programmatically.html
- tscircuit:
  - layout props: https://raw.githubusercontent.com/tscircuit/docs/main/docs/guides/tscircuit-essentials/layout-properties.mdx
  - manual edits: https://raw.githubusercontent.com/tscircuit/docs/main/docs/guides/tscircuit-essentials/manual-edits.mdx
  - placement schema: https://raw.githubusercontent.com/tscircuit/props/main/lib/manual-edits/manual_schematic_placement.ts
  - matchpack: https://github.com/tscircuit/matchpack
  - schematic-trace-solver: https://github.com/tscircuit/schematic-trace-solver
- Zener / pcb (Diode Computers):
  - spec, "Schematic position comments": https://github.com/diodeinc/pcb/blob/main/docs/pages/spec.mdx
  - position parser: https://github.com/diodeinc/pcb/blob/main/crates/pcb-sch/src/position.rs
  - KiCad schematic reconciliation design: https://github.com/diodeinc/pcb/blob/main/crates/pcb-kicad-sch/DESIGN.md
  - UUIDv5 identity: https://github.com/diodeinc/pcb/blob/main/crates/pcb-kicad-sch/src/identity.rs
- JITX:
  - design hierarchy / schematic groups: https://docs.jitx.com/en/latest/essentials/design/design-hierarchy.html
  - project files: https://docs.jitx.com/en/latest/getting-started/project-basics.html
  - HN discussion (Jan 2021), including the readability criticism: https://news.ycombinator.com/item?id=25915634
- PolymorphicBlocks:
  - R. Lin et al., "Polymorphic Blocks: Unifying High-level Specification and Low-level Control for Circuit Board Design," UIST 2020. doi:10.1145/3379337.3415860
  - R. Lin, R. Ramesh, P. Jain, J. Koe, R. Nuqui, P. Dutta, B. Hartmann, "Weaving Schematics and Code: Interactive Visual Editing for Hardware Description Languages," UIST 2021. doi:10.1145/3472749.3474804 (PDF: https://people.eecs.berkeley.edu/~bjoern/papers/lin-weaving-uist2021.pdf)
  - edg-ide: https://github.com/BerkeleyHCI/edg-ide
- SKiDL:
  - "Generating editable schematics" (2023): https://devbisme.github.io/skidl/generating-editable-schematics-2023-07-14.html
  - 2.3.0 release (July 2026): https://devbisme.github.io/skidl/skidl-two-dot-three-dot-zero-release-2026-07-28.html
- Flux.ai:
  - version control: https://docs.flux.ai/tutorials/version-control---deep-dive
  - MCP server: https://docs.flux.ai/reference/flux-mcp-server
- Umple:
  - grammar (`position` mixins): https://cruise.umple.org/umple/UmpleGrammar.html
  - UmpleOnline (layout-loss warning; auto-layout in 2026): https://cruise.umple.org/umple/UsingUmpleOnline.html
  - philosophy: https://cruise.umple.org/umple/Philosophy.html
- Enso metadata trailer (current status unverified): https://github.com/enso-org/enso/blob/develop/docs/language-server/protocol-language-server.md
- SVG-PCB: https://github.com/leomcelroy/svg-pcb · doi:10.1145/3559400.3562004
- Ptolemy II MoML: https://ptolemy.berkeley.edu/publications/papers/00/moml/ · Xcos graphics: https://help.scilab.org/scicos_graphics
- Eclipse GLSP source model: https://eclipse.dev/glsp/documentation/sourcemodel/
- R. Chugh, B. Hempel, M. Spradlin, J. Albers, "Programmatic and Direct Manipulation, Together at Last," PLDI 2016. doi:10.1145/2908080.2908103 · https://arxiv.org/abs/1507.02988
- B. Hempel, J. Lubin, R. Chugh, "Sketch-n-Sketch: Output-Directed Programming for SVG," UIST 2019. doi:10.1145/3332165.3347925 · https://arxiv.org/abs/1907.10699
- C. Omar et al., "Filling Typed Holes with Live GUIs," PLDI 2021. doi:10.1145/3453483.3454059
- JetBrains MPS basic notions: https://www.jetbrains.com/help/mps/basic-notions.html · diagramming: https://www.jetbrains.com/help/mps/diagramming-editor.html
- J. N. Foster et al., "Combinators for bidirectional tree transformations," TOPLAS 2007. doi:10.1145/1232420.1232424
- A. Bohannon et al., "Boomerang: Resourceful Lenses for String Data," POPL 2008. doi:10.1145/1328438.1328487

### 13.3 Schematic tools with array and loop idioms

- Altium Designer, "Creating a Multi-channel Design": `Repeat(<ChannelIdentifier>, first, last)` sheet symbols and `Repeat(port)` sheet entries. Restrictions: no individual net to one channel, no harnesses. https://www.altium.com/documentation/altium-designer/schematic/creating-multi-channel-design (opened)
- Cadence Virtuoso iterated instances (`R_string<127:0>` with bus labels `vTop,vRef<127:1>` / `vRef<127:1>,vBot`). Secondary source: "Top Ten: Tips on schematic entry in Cadence", Mixed-Signal Comments, 2011, tip #9. https://mixedsignal.wordpress.com/2011/03/13/top-ten-tips-on-schematic-entry-in-cadence/ (opened; the primary Virtuoso manual was not accessed)

### 13.4 KiCad and other formats (verified by the interop research thread against KiCad sources, Sept 2026)

- S-expression schematic format (stale, last modified 2025-01-17): https://dev-docs.kicad.org/en/file-formats/sexpr-schematic/
- The real spec, the writer: https://github.com/KiCad/kicad-source-mirror/blob/9.0/eeschema/sch_io/kicad_sexpr/sch_io_kicad_sexpr.cpp. Version history: `eeschema/sch_file_versions.h` (KiCad 8 = 20231120, 9 = 20250114, 10 = 20260306).
- KiCad 10.0.0 release (2026-03-20): https://www.kicad.org/blog/2026/03/Version-10.0.0-Released/
- Simulation fields: https://github.com/KiCad/kicad-source-mirror/blob/9.0/eeschema/sim/sim_model.h. Simulator chapter of the manual: https://docs.kicad.org/9.0/en/eeschema/eeschema.html. Workbook (`.wbk`) JSON: `eeschema/sim/simulator_frame_ui.cpp` (`SaveWorkbook`).
- Bus aliases moved to `.kicad_pro` in KiCad 10: https://github.com/KiCad/kicad-source-mirror/blob/10.0/common/project/project_file.cpp. Bus net naming QA data: `qa/data/eeschema/netlists/prefix_bus_alias`.
- Text-variable resolution through sheet-instance fields: `SCH_SYMBOL::ResolveTextVar`, `SCH_SHEET::ResolveTextVar` (`eeschema/sch_sheet.cpp`, 9.0).
- Multi-instance field churn: https://gitlab.com/kicad/code/kicad/-/issues/11035
- kicad-cli (netlist formats including `kicadsexpr`, `spice`, `spicemodel`): https://docs.kicad.org/9.0/en/cli/cli.html
- IPC API (PCB only in KiCad 9/10): https://dev-docs.kicad.org/en/apis-and-binding/ipc-api/for-addon-developers/
- Visual diff tools: KiDiff https://github.com/INTI-CMNB/KiDiff · KiRI https://github.com/leoheck/kiri
- LTspice `.asc` import (format keywords): https://dev-docs.kicad.org/en/import-formats/ltspice/

### 13.5 Auto-placement and graph drawing (DOIs confirmed through Crossref by the layout research thread unless marked)

- Arya, Kumar, Swaminathan, Misra, "Automatic generation of digital system schematic diagrams," DAC 1985, pp. 388–395. doi:10.1109/DAC.1985.1585970
- Swinkels & Hafer, "Schematic generation with an expert system," IEEE TCAD 9(12):1289–1306, 1990. doi:10.1109/43.62774
- Green & Andersen, "Automated generation of analog schematic diagrams," ISCAS 1990, pp. 3197–3200. doi:10.1109/ISCAS.1990.112692
- Frezza & Levitan, "SPAR: a schematic place and route system," IEEE TCAD 12(7):956–973, 1993. doi:10.1109/43.238032
- Arsintescu, "A method for analog circuits visualization," ICCD 1996, pp. 454–459. doi:10.1109/ICCD.1996.563593
- Hsu & Lin, "Automatic analog schematic diagram generation based on building block classification and reinforcement learning," MLCAD 2022, pp. 43–48. doi:10.1109/MLCAD55463.2022.9900093
- Yang et al., "A review of automatic schematic generation techniques and their application to printed circuit boards," FITEE 26(9):1534–1550, 2025. doi:10.1631/FITEE.2400612
- Gulgonul, "Weave: Verified Netlist-to-Schematic Conversion via Layered Graph Layout," arXiv 2607.03835, July 2026 (opened). Code: https://github.com/senolgulgonul/weave
- Matsuo, Uhlich et al., "Schemato – An LLM for Netlist-to-Schematic Conversion," arXiv 2411.13899; MLCAD 2025.
- Luo, Ma, Zhang, Qiu, "SchGen: PCB Schematic Generation with Semantic-Grounded Code Representations," arXiv 2605.30345, May 2026 (opened)
- Liu & Chitnis, "EEschematic," arXiv 2510.17002, 2025.
- Sugiyama, Tagawa, Toda, "Methods for visual understanding of hierarchical system structures," IEEE TSMC 11(2):109–125, 1981. doi:10.1109/TSMC.1981.4308636
- Spönemann, Fuhrmann, von Hanxleden, Mutzel, "Port constraints in hierarchical layout of data flow diagrams," GD 2009. doi:10.1007/978-3-642-11805-0_14
- Schulze, Spönemann, von Hanxleden, "Drawing layered graphs with port constraints," JVLC 25(2):89–106, 2014. doi:10.1016/j.jvlc.2013.11.005
- Domrös, von Hanxleden, Spönemann, Rüegg, Schulze, "The Eclipse Layout Kernel," arXiv 2311.00533, 2023. ELK option reference: https://eclipse.dev/elk/reference.html
- Domrös & von Hanxleden, model-order layout, VISIGRAPP 2022. doi:10.5220/0010833800003124
- netlistsvg (Yosys netlists → SVG with elkjs): https://github.com/nturley/netlistsvg
- Wybrow, Marriott, Stuckey, "Orthogonal connector routing," GD 2009, doi:10.1007/978-3-642-11805-0_22; "Orthogonal hyperedge routing," Diagrams 2012, doi:10.1007/978-3-642-31223-6_10. libavoid: https://github.com/mjwybrow/adaptagrams
- Dwyer, Koren, Marriott, "IPSep-CoLa," IEEE TVCG 12(5):821–828, 2006. doi:10.1109/TVCG.2006.156. WebCola: https://github.com/tgdwyer/WebCola
- tscircuit: `matchpack`, `schematic-match-adapt`, and `ManualEditsFile` (`schematic_placements` with `selector` / `relative_to` / `center`) in `@tscircuit/props` `lib/manual-edits`. https://github.com/tscircuit
- SKiDL schematic generation (`src/skidl/schematics/place.py`, `route.py`): https://github.com/devbisme/skidl
- Hayes, "Lcapy: symbolic linear circuit analysis with Python," PeerJ CS 2022. doi:10.7717/peerj-cs.875
- Massier, Graeb, Schlichtmann, "The sizing rules method for CMOS and bipolar analog integrated circuit synthesis," IEEE TCAD 27(12):2209–2222, 2008. doi:10.1109/TCAD.2008.2006143
- Ohlrich et al., "SubGemini: identifying subcircuits using a fast subgraph isomorphism algorithm," DAC 1993. doi:10.1145/157485.164556
- Misue, Eades, Lai, Sugiyama, "Layout adjustment and the mental map," JVLC 6(2):183–210, 1995. doi:10.1006/jvlc.1995.1010
- Archambault & Purchase, "The 'map' in the mental map," IJHCS 71:1044–1055, 2013. doi:10.1016/j.ijhcs.2013.08.004
- Petre & Green, "Learning to read graphics: some evidence that 'seeing' an information display is an acquired skill," JVLC 4(1):55–70, 1993. doi:10.1006/jvlc.1993.1004 (expert schematic readers and secondary notation)
- Purchase, "Which aesthetic has the greatest effect on human understanding?," GD 1997. doi:10.1007/3-540-63938-1_67
- Ware, Purchase, Colpoys, McGill, "Cognitive measurements of graph aesthetics," Information Visualization 1(2), 2002. doi:10.1057/palgrave.ivs.9500013
- Helmke, Doğan, Scheffler, Wrobel, Diagrams 2024. doi:10.1007/978-3-031-71291-3_4

### 13.6 Tooling

- rowan (lossless syntax trees; used by rust-analyzer): https://github.com/rust-analyzer/rowan (opened)
- git custom merge drivers (`merge.<driver>.driver` with `%O %A %B`): https://git-scm.com/docs/gitattributes (opened)
