# Staged device data: what compilers and simulators do

> Research report, 2026-09-25. Question: how should data pass between the stages of our pipeline (language → design model → simulator → runs), given that several representations of a device are fine if each has a reason to exist?
> Sources were read from cloned repositories (ngspice, Xyce, Gnucap, OpenVAF, VACASK, cranelift-entity) and from the rustc-dev-guide and rust-analyzer docs, except where §5 says otherwise. Conclusions are merged into `../pipeline.md`.

## 0. Bottom line

- **The same five stages everywhere.** Every simulator read keeps five kinds of device data: as-written, resolved, temperature-derived, topology/matrix locations, and per-iteration/per-step state. They are separated by **how often each one changes**.
  - ngspice keeps them all in one struct, so it needs "given" bits to re-run a stage safely.
  - OSDI and VACASK make each stage explicit.
  - Compilers split their IRs along the same line, plus wherever the vocabulary changes.
- **For spicy, three moves matter most:**
  1. Move stamp indices, names/spans and transient history **out of the device structs**, into side tables owned by the stage that computes them.
  2. Make the simulator-facing circuit a **template whose values change per run**. Topology, the sparsity pattern and the KLU symbolic analysis are then built once per design and shared read-only by parallel runs.
  3. Keep every SPICE-ism inside `spicy_parser`'s lowering: `Option` specs, `.model` inheritance, dialect defaults.
- **Correction to the brief:** ngspice does *not* change values without re-setup. Every `run` rebuilds the matrix. Xyce `.STEP` and VACASK do avoid the rebuild.

## 1. Compilers

**rustc.**
- **The chain:**
  - AST: "pretty much exactly what the user wrote".
  - HIR: desugared, names resolved; kept for the whole compilation.
  - THIR: fully typed, method calls turned into plain calls, bodies only, "dropped as soon as it's no longer needed".
  - MIR: a control-flow graph, "still generic".
  - LLVM IR.

  Sources: [overview](https://rustc-dev-guide.rust-lang.org/overview.html), [THIR](https://rustc-dev-guide.rust-lang.org/thir.html).
- **New types only where the vocabulary changes.** MIR's phases (`mir_built` … `optimized_mir`) share one type, "modified in place (which helps to keep things efficient)" ([MIR passes](https://rustc-dev-guide.rust-lang.org/mir/passes.html)).
- **Side tables instead of annotations.** Types aren't written into HIR nodes. [`TypeckResults`](https://doc.rust-lang.org/nightly/nightly-rustc/rustc_middle/ty/struct.TypeckResults.html) is a side table (`node_types: ItemLocalMap<Ty>`) keyed by the local part of a `HirId`.
- **Local numbering.** A `HirId` is an owner plus a local id ([HIR](https://rustc-dev-guide.rust-lang.org/hir.html)).
- **Interning.** Types are allocated in arenas, interned, and compared by pointer ([memory](https://rustc-dev-guide.rust-lang.org/memory.html)).
- **Spans.** Every MIR statement carries a 12-byte [`SourceInfo { span, scope }`](https://doc.rust-lang.org/nightly/nightly-rustc/rustc_middle/mir/struct.SourceInfo.html).
- **Queries.** Results are cached and recomputed when their inputs change, so the lower IRs are derived data, never edited.

**rust-analyzer** ([architecture](https://github.com/rust-lang/rust-analyzer/blob/master/docs/book/src/contributing/architecture.md)).
- "Syntax tree is a value type… doesn't store semantic info."
- The `hir-*` crates have "a strong ECS flavor… work with raw ids".
- Bodies and source maps are split so that types aren't recomputed "whenever some whitespace is typed" (`crates/hir-def/src/expr_store/body.rs`).

**LLVM and MLIR.** Source for this paragraph: the MLIR paper, [arXiv:2002.11054](https://arxiv.org/abs/2002.11054) §2.
- **LLVM** is one IR, which the paper calls "normalization-only". Clang still lowers through fixed levels: AST → LLVM IR → SelectionDAG → MachineInstr → MCInst.
- **MLIR** lowers progressively, "in small steps", across dialects.
- **Its key principle:** "Attempts to raise semantics once lowered are fragile… The loss of structure is then conscious and happens only where the structure is no longer needed." It also lists "source location tracking and traceability" as a goal.
- **When a new level is justified:** when a consumer needs information the lower level can't hold. For us, that's "this number is the knob `amp.r1.value`".

**Cranelift** (`cranelift/entity/src/lib.rs`).
- Entity references are `u32` newtypes, for "improved type safety" and "smaller indexes".
- `PrimaryMap` owns entities; `SecondaryMap` is a dense side table.
- `Function.srclocs: SecondaryMap<Inst, RelSourceLoc>` is "not interpreted by Cranelift, only preserved".

**Principles.**
1. A stage gets a new type when the vocabulary or the lifetime changes. Otherwise annotate or mutate.
2. Data with a different change rate or owner goes in a side table keyed by a dense ID.
3. One source of truth. Only the source is edited; every lower form is a function of the higher forms plus explicit inputs.
4. Stable IDs for identity (`HirId`; our `Path`); dense indices for speed.
5. Provenance is a small `Copy` handle carried through every lowering.

## 2. Circuit simulators

**ngspice.**
- **The device table.** `SPICEdev` (`src/include/ngspice/devdefs.h`) is a table of per-type function pointers: `DEVparam`, `DEVsetup`, `DEVtemperature`, `DEVload`, `DEVaccept`, `DEVtrunc`, `DEVbindCSC`, …
  - `CKTtemp` calls `DEVtemperature` once per device type, over that type's model list (`analysis/ckttemp.c`).
- **One struct holds every stage.** `RESinstance` (`devices/res/resdefs.h`) holds:
  - as-given values with flags (`RESresGiven`, `REStc1Given`);
  - the derived `RESconduct`;
  - matrix pointers (`RESposPosPtr`);
  - a state offset.
- **The stages are functions over that struct:**
  - `resparam.c` sets `RESresist` and `RESresGiven`.
  - `ressetup.c` fills model defaults and allocates matrix entries (`TSTALLOC`).
  - `restemp.c` lets instance tc1/tc2 override the model's and falls back to 1 mΩ when no resistance is given. It computes `RESconduct`, and writes the computed default back into `RESresist`.
- **Given bits exist because input and derived values share storage.**
- **Loading.** `resload.c` adds `RESconduct` through the pointers. Under KLU, the pointers first address triplet entries and `RESbindCSC` rebinds them to CSC positions. That's the same temp → final step as our `setup_pattern`.
- **History** lives in `CKTstates[8]` (`cktdefs.h`), with per-instance offsets (`CAPqcap = CAPstate`).
- **`alter` doesn't avoid re-setup:**
  - It writes through `DEVparam`.
  - Every `run` goes `CKTdoJob(reset=1)` → `CKTunsetup`/`NIdestroy` → `CKTsetup` → `CKTtemp` (`cktdojob.c` ~L160–170).
  - `alterparam` re-reads the deck (`frontend/inp.c` L1815–1821).

**Xyce.**
- **Masters.** `DeviceMaster<Traits>` owns every model and instance of one type (`N_DEV_DeviceMaster.h`). `Master::loadDAEMatrices` loops over all of them with direct field access (`OpenModels/N_DEV_Resistor.C` L1306).
- **Netlist input.** The parser emits generic `InstanceBlock`/`ModelBlock` records: a name, a `vector<Param>` and a `NetlistLocation` (`N_DEV_DeviceBlock.h`).
- **Topology.** Instances declare a local `jacStamp`. Topology hands back LIDs, and `setupPointers` caches matrix pointers.
- **`.STEP` doesn't rebuild topology.** It calls `loader.setParam` → `processParams` → `updateTemperature` (`N_ANP_SweepParam.C` L541; `N_DEV_DeviceMgr.C` ~L6598–6605).
- **Defaults are dialect choices.** A missing R defaults to 1 kΩ in Xyce and 1 mΩ in ngspice.

**Gnucap.**
- **Stages.** `CARD` (`include/e_card.h`) separates:
  - elaboration: `precalc_first`, `expand`, `precalc_last`, `map_nodes`;
  - DC/transient: `tr_iwant_matrix`, `tr_begin`, `tr_advance`, `do_tr`, `tr_load`, `tr_review`, `tr_accept`;
  - AC.
- **Shared parameters.** They live in ref-counted, shareable `COMMON_COMPONENT`s (`lib/e_compon.cc` L61).
- **`PARAMETER<T>`** keeps the expression text plus its value, evaluated in `precalc_last`.
- **Front-ends.** SPICE, Spectre and Verilog are sibling plugins (`apps/lang_*.cc`) that build the same card list.

**OSDI / OpenVAF / VACASK.** The OSDI 0.3 descriptor (`openvaf/osdi/header/osdi_0_3.h`; [manual](https://openvaf.semimod.de/osdi/osdi_v0p3.pdf) §2–3) makes the SPICE stage split explicit:
- **Data.** The simulator allocates opaque `model_size`/`instance_size` blobs.
- **Parameter input.** Parameters are written with `access(…, ACCESS_FLAG_SET)`, because "later simulation stages must know which parameters have been explicitly set".
- **Setup.** `setup_model` and `setup_instance(…, temperature, …)` apply defaults, bounds, model → instance inheritance and all temperature code. They also decide node collapsing: "node collapsing can not depend on the operating point".
- **Topology.** The simulator writes node mappings, Jacobian pointers and state indices into the instance at the offsets the descriptor gives.
- **Per iteration.** `eval` runs once per Newton iteration, then `load_*` scatters the results.
- **Re-running.** Setup must re-run "whenever the model or instance parameters are changed".

How simulators relate to it:
- VACASK re-runs `setup_instance` only for instances flagged `NeedsSetup`, and rebuilds equations only if the collapse pattern changed (`lib/osdiinstance.cpp`, `docs/cir-elaboration.md`).
- OpenVAF's README says rust-analyzer and rustc "heavily inspired the design".

**Distilled.**

| Stage | Examples | Changes per |
|---|---|---|
| As-written (+ given) | ngspice `*Given`, Xyce `InstanceBlock`, OSDI `access(SET)` | design edit |
| Resolved (defaults, model → instance) | `ressetup.c`/`restemp.c`, `processParams`, `setup_model` | design edit |
| Condition-derived | `DEVtemperature`, `updateTemperature`, `setup_instance(T)` | temperature (per run for us) |
| Topology, stamps, state slots | `TSTALLOC` + `bindCSC`, LIDs, OSDI offsets | structure |
| Linearize and load | `DEVload`, `eval` + `load_*`, `tr_load` | Newton iteration |
| History, accept/truncate | `CKTstates`, `prev/next_state`, `tr_accept` | time step |

The stages have different inputs and change at different rates. Merging them is what forces given bits and dirty flags.

## 3. Applied to spicy (summary)

The full proposal, merged with the red-team's measurements, is `../pipeline.md`. The key points of this report:

**Representations:**
- SPICE `Deck` stays the as-written form.
- `Design` + a source-map side table.
- `FlatDesign` + knobs.
- A simulator-facing circuit (per-kind POD arrays, origins as IDs).
- A per-run values input.
- `Topology`/Plan (stamps, patterns, KLU symbolic; shared read-only).
- `Derived` (temperature stage).
- A per-thread `Workspace`.
- `TranState` (a ring of state vectors).
- Results by ID, with names added at the edge.

**Rules:**
- **SPICE defaults and `.model` merging** live in `spicy_parser`'s lowering.
  - Equation defaults (e.g. Gummel–Poon BR = 1) are `Default` impls on the shared parameter structs.
  - Dialect defaults (the missing-R fallback, precedence, `m`, `scale`, `tnom`) belong only to the SPICE front-end.
- **Don't lower SPICE into the design model.** A SPICE → `.spl` importer is the right tool if needed.
- **A part kind is not a device.** The mapping is one-to-many: an electrolytic can become a C plus an ESR R on an internal node, plus leakage. This alone justifies separate types.
- **Structure must never depend on the knob point:**
  - decide node collapsing from a knob's whole range;
  - cache one topology per assignment of connectivity-changing mode knobs.
- **Thread safety:**
  - Share `Circuit`/`Topology` behind `Arc`.
  - Keep values, derived data, workspace and state per worker.
  - `klu::factor` takes `&mut KluSymbolic` today (it revises `lower_nz`), so clone the symbolic per worker or change `factor`.

## 4. Summary table

| # | Representation | Crate | Built | Lifetime | Precedent |
|---|---|---|---|---|---|
| 1 | Lossless syntax tree | `spicy_lang` | parse | until next edit | rust-analyzer `syntax` |
| 2 | `Design` + source map | `spicy_model` | after parse | design revision | HIR; `BodySourceMap` |
| 3 | `FlatDesign` + knobs | `spicy_model` | elaborate | design revision | monomorphization; VACASK elaboration |
| 4 | Simulator-facing circuit | `spicy_circuit` | lower (both front-ends) | design revision | Xyce `InstanceBlock`; OSDI parameter input |
| 5 | Topology / plan | `spicy_simulate` | per structure × solver | shared by all runs | `DEVsetup`/`bindCSC`; Xyce LIDs; OSDI offsets |
| 6 | Per-run values | `spicy_circuit` (filled by `spicy_backends`) | per run | one run | `alter`; Xyce `setParam`; OSDI `access(SET)` |
| 7 | Derived (temperature) | `spicy_simulate` | per run | one run | `DEVtemperature`; `setup_instance(T)` |
| 8 | Workspace | `spicy_simulate` | per worker | reused across runs | `DEVload`; OSDI `eval`/`load` |
| 9 | Transient state | `spicy_simulate` | per step | one transient | `CKTstates`; OSDI `prev/next_state` |
| 10 | Results by ID → named | `spicy_simulate` → `spicy_backends` | per analysis | the engine's | rustc `SourceInfo`; Cranelift `srclocs` |
| — | SPICE `Deck` | `spicy_parser` | parse | until lowered | ngspice given bits |

## 5. Uncertainties

- **Read through a fetch tool, not raw source:** the `SourceInfo` and `TypeckResults` docs.
- **Not re-verified:** that KLU `refactor` reuses the previous pivot order (standard KLU behavior, [KLU paper]).
- **Not researched:** Modelica.
- **Judgement calls, not precedent:**
  - per-instance parameters instead of a `.model` table;
  - exporting SPICE from the simulator-facing circuit;
  - deciding node collapsing from knob ranges;
  - span side tables.
