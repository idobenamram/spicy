# Conditions, options, names and external parts: what compilers and modeling tools do

> Research report, 2026-09-30. Question: where should the proposed additions to `spicy_circuit` live? They are the run's temperature and the default model temperature (`Conditions { temp, tnom }`), new `BjtModel` temperature fields, Newton tolerances (`SolverOptions`), model names, and later device origins, opaque vendor subcircuits and new device kinds.
> Builds on `staged_ir_survey.md` and `data_model_survey.md`, which already cover rustc's side tables, rust-analyzer's source maps, Cranelift's entity maps, MLIR's attributes and interning, and JAX's static arguments. This report cites them rather than repeating them.
> **Sources.** Read from the clones in `/tmp/refs`: MLIR docs (paths below are relative to `llvm/mlir/docs/`), Cranelift (`wasmtime/cranelift/codegen/src/`), the rustc-dev-guide (`rdg/src/`), rust-analyzer, ngspice, Xyce, VACASK, Gnucap and OpenVAF.
> Fetched on 2026-09-30 and read as raw text: LLVM `LangRef.md` and `SourceLevelDebugging.md` (llvm-project main), rustc `compiler/rustc_session/src/options.rs` and `compiler/rustc_middle/src/mir/syntax.rs`, the Modelica Standard Library sources, the FMI 3.0.1 specification and the DWARF 5 standard.
> Read through a summarizing fetch, so quotes may be paraphrased: LLVM's DeveloperPolicy and HowToUpdateDebugInfo, the Modelica 3.6 spec (annotations chapter), the OpenModelica simulation flags, the CIRCT HW dialect, the MLIR builtin dialect, ECMA-426 (source maps), the Rust Reference and the salsa book.

---

## 0. Summary

**The line every mature system draws.** Every system read splits configuration three ways:
- **What the program means** travels inside the IR. Examples: LLVM's data layout, MLIR's data-layout attribute, a Modelica transistor's `Tnom`.
- **How to compile or solve it** travels with the request. Examples: rustc's `Session` options, Cranelift's `Flags`, MLIR's pass options, FMI's tolerance argument, Modelica's `experiment` annotation.
- **Where things came from** travels in a side table the compute path never reads. Examples: DWARF, source maps, Cranelift's `SourceLoc`, rust-analyzer's source maps.

A setting enters the IR only when the IR's own numbers depend on it, or when the IR outlives the request that made it (LLVM's LTO).

**A test for spicy:** *if every tolerance went to zero, would this value still change the answer?*
- `temp`: yes. It's physics.
- `tnom`: yes. It's part of what a model's numbers mean.
- `reltol`, `vntol`, `abstol`: no. They only decide how close Newton gets to the answer.

**Recommendations:**

| # | Proposed item | Recommendation | Change from the proposal |
|---|---|---|---|
| 1 | `Conditions.temp` | A field `temp: f64` (K) of `Params`, the per-run numbers. Bound by the `temp` knob like any other field | **No separate `Conditions` struct** |
| 2 | `Conditions.tnom` + `BjtModel.tnom: Option<f64>` | `tnom: f64` on each model, **resolved at lowering**. SPICE's `.options tnom` is a dialect default, folded in like `scale`. R, C, L and diode models get the same field in the same change, so `.options tnom` isn't dropped for them | **No global `tnom`, no `Option`** |
| 3 | `BjtModel` gains `xtb`, `xti`, `eg` | As proposed. These are exactly the Modelica library's per-model NPN fields | — |
| 4 | `SolverOptions` in `Lowered` | The type lives in `spicy_circuit` next to `Analysis`. Its fields are `Option`s: what the file asked for. The simulator never reads it; it reads the run-time `SimulationConfig` it already takes. The engine picks a tolerance profile per request, over one `Plan` | Same place; clarified as a **default request**, never part of `Circuit` or `Params` |
| 5 | Model names in `CircuitNames` | Yes: one name per model entry, parallel to the model tables. Merge identical model cards only when they are **the same card** | The merge key changes (§7 D5) |
| 6 | Origins | Later, when a reader exists. One opaque `Origin(u32)` per device in `CircuitNames`, interpreted by the front-end that made it | — |
| 7 | Opaque vendor subcircuits | Later. A declaration table (subcircuit name, library, ports, parameter names) and instances that point at it, like LLVM's `declare` and CIRCT's `hw.module.extern`. Backends that can't run them refuse them when checking the circuit | — |
| 8 | New kinds and parameters | Closed per-kind tables. No `#[non_exhaustive]`, which would *hide* new kinds from our own crates. Destructure exhaustively where every kind or field must be handled | — |
| 9 | Versioning | None until something is stored on disk. Then use a version marker that rejects mismatches, as Cranelift does | — |

**Why this is simpler and no slower than the proposal:**
- `simulate_*(circuit, params, config)` keeps its three arguments.
- A run is still one `Params` copy.
- There is one fewer struct to round-trip.
- `tnom` costs nothing per run.
- Tolerances never touch the `Plan`.

---

## 1. The question in spicy's terms

**Today:**
- `Lowered { circuit, params, names, analyses }` (`spicy_circuit/src/lib.rs`).
- The simulator already takes its solver settings as a **run-time argument**: `simulate_op(circuit, params, &SimulationConfig)`. `SimulationConfig` holds the linear solver, the integrator and `NewtonConfig { abs_tol, rel_tol, max_iters }` (`spicy_simulate/src/lib.rs:33-84`).

**The engine's needs:**
- Every request names a tolerance profile: `Request { point, analyses, tolerance: Tolerance::{Engine, Tight} }` (`engine_types.md` §4).
- Decisive points are re-run at `reltol 1e-9` (`engine_plan.md` §2.7).
- On ngspice, tolerances must sit in the deck text, because `reset` silently undoes control-mode `option reltol` (`engine_plan.md` §4.3 rule 2). So the ngspice backend loads two decks. That's a quirk of one backend, handled inside the exporter, which already takes options as an input (`engine_plan.md` §5.1).

**The three quantities, in plain terms, on the CE amplifier:**

| Quantity | What it is | Does it change the answer? |
|---|---|---|
| `temp` | The temperature the circuit is at while it runs. At a fixed current, a transistor's base–emitter voltage drops about 2 mV per °C | Yes: it's physics |
| `tnom` | The temperature at which a model's numbers were measured. `IS = 1e-14` means "1e-14 A at `tnom`"; the simulator converts each number from `tnom` to `temp` | Yes. With TNOM 27 °C instead of 25 °C, the nominal collector voltage moves from 5.5032 V to 5.5410 V (`engine_plan.md` §1.3, row "TNOM") |
| `reltol`, `vntol`, `abstol` | When Newton stops: once a step changes each voltage by less than `reltol·|v| + vntol` (currents use `abstol`) | No. The exact answer doesn't move; only how close we get. Default tolerances left up to 9.5e-5 V of error over the 256 corners; engine tolerances cut it to 6e-13 V (`engine_plan.md` §4.5) |

---

## 2. "Program" vs "configuration": where each system puts it

### 2.1 LLVM: the module

**What the module holds.** "Each module consists of functions, global variables, and symbol table entries. Modules may be combined together with the LLVM linker, which merges function (and global variable) definitions, resolves forward declarations" (`LangRef.md`, Module Structure).

**Four kinds of configuration live in or near it:**
- **Data layout** (`target datalayout = "…"`): in the IR, because the IR already depends on it. "This string is used by the mid-level optimizers to improve code… There is no way to generate IR that does not embed this target-specific detail into the IR" (Data Layout).
- **Target triple**: in the IR, but a request can override it. The triple "is passed along to the backend… It's possible to override this on the command line with the `-mtriple` command-line option" (Target Triple).
- **Module flags**: key/value pairs, because "information about the module as a whole is difficult to convey to LLVM's subsystems." Each flag carries a *merge behavior* (Error, Warning, Require, Override, Append, AppendUnique, Max, Min), applied "when two (or more) modules are merged together" (Module Flags Metadata).
- **Function attributes**: these carry command-line flags into the IR. "The single attribute group will capture the important command line flags used to build that file" (Attribute Groups). Examples are `"target-cpu"` and `optnone`.

Attributes and module flags exist because modules are **linked and code-generated later, in another process** (LTO), where the original command line is gone. The optimization pipeline itself (`-O2`) is not in the module: the driver chooses it.

**Lesson.** LLVM puts configuration in the IR for one of two reasons:
1. The IR's numbers already depend on it (the data layout). For us, that's `tnom`: a model's `IS` means nothing without it.
2. The IR outlives the request (LTO). For us, that's an **exported deck**, which is why the exporter writes `.options`. It's not a reason for `Circuit` to hold them.

### 2.2 MLIR: attributes, data layout, pass options

- **Attributes are constants; run-time values are operands.** Discardable attributes carry metadata defined outside the op (MLIR `LangRef.md:815-819`). The earlier survey covers this tier split (`data_model_survey.md`, MLIR §3).
- **The data layout is a module attribute** (`module attributes { dlti.dl_spec = … }`, `DataLayout.md:266-267`). It is scoped: "data layout properties are _scoped_ to regions belonging to either operations that implement the `DataLayoutOpInterface` or `ModuleOp` operations" (`DataLayout.md:30-32`). It has a natural default: "Types are also expected to have a default, 'natural' data layout… This ensures that data layout queries always have a valid result" (`:39-42`). Queries walk the scopes and are cached.
- **Pass options belong to the pipeline, not the IR.** "Options are parsed at pass construction time independently for each instance of the pass" (`PassManagement.md:497-501`).

**Lesson.** MLIR has the same split: meaning goes in an IR attribute with a default, transformation settings go with the pass. The scoped default is MLIR's answer to "a module-wide default that an item may override": exactly `.options tnom` versus a card's own `TNOM`. MLIR needs lookup machinery because its IR nests. Our `Circuit` is flat, with one scope, so resolving the default once at lowering does the same job with no machinery.

### 2.3 rustc: `Session` beside HIR and MIR

- **Options live in the `Session`, never in the IR.** Passes read them through `tcx.sess.opts…` (`rdg/src/implementing-new-features.md:288-289`).
- **Every option says whether it can change results.** From `options.rs:332-356`:
  - `[TRACKED]`: "a change in the given field will cause the compiler to completely clear the incremental compilation cache";
  - `[TRACKED_NO_CRATE_HASH]`: tracked, but kept out of the crate hash;
  - `[UNTRACKED]`: "incremental compilation is not influenced by this option";
  - "If in doubt, specify `[TRACKED]`, which is always 'correct' but might lead to unnecessary re-compilation."
- The guide states the rule: "Use `[UNTRACKED]` for flags that only affect diagnostics or debugging output, and use `[TRACKED]` when changing the flag can change compilation results" (`implementing-new-features.md:282-283`).

**Lesson.** Options sit beside the IR, and the caches know which options affect results. For us: tolerances affect results but not the `Plan`, so they belong in the result key and not in the plan key (§6).

### 2.4 Cranelift: `Flags` passed at compile time

- **Settings are an argument, not part of the function.** "The `Flags` struct is immutable once it has been created" (`settings.rs:9-10`). It reaches the compiler inside `&dyn TargetIsa`, as an argument to `Context::compile(isa, …)` (`context.rs:220-223`), and passes read `isa.flags().opt_level()` (`context.rs:174`).
- **The cache key includes them.** The key of *compiled code* is the stencil plus the ISA's name, triple, shared flags and ISA flags (`incremental_cache.rs:135-147`), because the machine code depends on them.

**Lesson.** Configuration is a run-time argument, and it joins the cache key of any output that depends on it. Our `Plan` (sparsity pattern, KLU symbolic analysis) doesn't depend on tolerances; our results do.

### 2.5 Modelica: temperature is a model parameter; tolerance is an annotation

- **Device temperatures are parameters of the model** (Modelica Standard Library):
  - `Resistor` has `parameter SI.Temperature T_ref=300.15 "Reference temperature"` (`Electrical/Analog/Basic/Resistor.mo:5`).
  - Its operating temperature is `parameter SI.Temperature T=293.15 "Fixed device temperature if useHeatPort = false"` (`Interfaces/ConditionalHeatPort.mo:7-8`).
- **The measurement temperature is per model.** `Semiconductors.NPN` has `Tnom=300.15 "Parameter measurement temperature"`, `XTI=3`, `XTB=0` and `EG=1.11` (`Semiconductors/NPN.mo:22-25`). These are **the exact fields the proposal adds to `BjtModel`**, each carried by the model, not by a global.
- **Structural switches are marked.** `useHeatPort` carries `annotation(Evaluate=true)` (`ConditionalHeatPort.mo:5-6`). With `Evaluate = true`, "it is not possible to change the parameter value after symbolic pre-processing" (spec 3.6 §18.3).
- **Tolerance is not part of the equations.** It sits in an annotation: "The experiment annotation defines the default start time (StartTime)…, the default stop time (StopTime)…, and the default relative integration tolerance (Tolerance) for simulation experiments to be carried out with the model" (spec 3.6 §18.4).
- **Both are overridable at run time.** OpenModelica's compiled executable reads a setup file. `-override var1=start1,…` overrides parameters, and `-tolerance` "Specifies solver tolerance" (OpenModelica simulation flags). So parameters and solver settings are both inputs to the compiled model, from different places.

**Lesson.** This is the split recommended here:
- `temp` is a value the equations read.
- `tnom` belongs to each model.
- The tolerance is a default the file may suggest and the tool may override.

### 2.6 FMI 3.0: parameters, structural parameters, and the experiment

- **The default experiment is a suggestion.** "`<DefaultExperiment>` consists of the optional default start time, stop time, relative tolerance, and step size for a simulation run. **A tool may ignore this information**" (§2.4.6).
- **The real tolerance is a run-time argument:** `fmi3EnterInitializationMode(instance, toleranceDefined, tolerance, startTime, stopTimeDefined, stopTime)` (§2.3.2).
- **Parameters are classified by when they may change** (§2.4.7.4):
  - `fixed`: "fixed in super state Initialized";
  - `tunable`: "may be changed only in Event Mode" or at communication points;
  - `structuralParameter`: "can only be changed in Configuration Mode or Reconfiguration Mode" (they size arrays).

**Lesson.** FMI's three rates map onto spicy's existing split (`circuit.md` §4.3):
- structural parameters are the structure key;
- fixed and tunable parameters are `Params`;
- the experiment is the run request.

Temperature, in an FMU, is a parameter like any other.

### 2.7 Cross-check: what the simulators do (not covered by the earlier surveys)

| Simulator | `temp` | `tnom` | `reltol` / `vntol` / `abstol` | Sources |
|---|---|---|---|---|
| ngspice | Held in the task (`TSKtemp`) and copied into the circuit at job start. `.dc temp` sweeps it the way a source's value is swept | Held in the task (`TSKnomTemp`). A model without its own `TNOM` takes it at setup | Held in the task, copied at job start | `tskdefs.h:19-50`; `cktdojob.c:52-71`; `bjttemp.c:42`; `ressetup.c:29`; `dctrcurv.c:215-218` |
| Xyce | An "artificial parameter" of the device manager, so `.STEP TEMP` sets it like a device parameter | `.OPTIONS DEVICE TNOM` | In the nonlinear-solver and time-integrator packages, outside the devices | `N_DEV_DeviceMgr.C:239`; `N_DEV_DeviceOptions.C:209-219`; `N_TIA_TIAParams.C:283` |
| VACASK | An option, exposed to expressions as `$temp` | An option (`$tnom`) | Options. The control block "is separated from the circuit description" | `docs/cmd-options-temp.md`; `cmd-options-special.md`; `cmd-overview.md:3` |
| Gnucap | Process-global statics | Same | Same | `include/u_opt.h:93-122` |
| OSDI | An argument of `setup_instance(…, temperature, …)` | A simulator parameter | Simulator parameters (name/value lists) passed to `setup_*` | `osdi_0_3.h:74-79, 176-180` |

**What the simulators agree on.** They file `temp` under "options", next to the tolerances. In operation, though, they treat it as a **per-run value that can be swept**:
- ngspice sweeps it in `.dc`;
- Xyce steps it like a parameter;
- VACASK re-evaluates expressions when it changes.

Every one resolves a model's missing `TNOM` from the global default **once, at setup** (ngspice writes it into the model). None keeps tolerances in device or model data.

### 2.8 What falls out

| Item | Changes the answer? | Changes structure? | Who sets it, how often | Home |
|---|---|---|---|---|
| `temp` | yes | no | a knob, every run | with the per-run numbers: `Params` |
| `tnom` | yes | no | the model card; never per run | each model, resolved at lowering |
| `xtb`, `xti`, `eg` | yes | no | the model card; a knob may vary them | each model (`Params`) |
| `reltol`, `vntol`, `abstol` | no (precision only) | no* | the request: the CLI, or the engine's profile | the run request (`SimulationConfig`); the file's value is a default carried in `Lowered` |
| names, origins | no | no | the source text | `CircuitNames` |

\* Except through compiled OSDI models (§6.3).

---

## 3. Names and origins as side tables

### 3.1 What each system does

| System | Where locations live | Can the compute path ignore them? | Cite |
|---|---|---|---|
| DWARF | A separate section (`.debug_line`), referenced from the compilation unit. Conceptually "a large matrix, with one row for each instruction". An inlined call records "the coordinates of the call". Split DWARF moves it all to `.dwo` files | Yes: the executable runs without it | DWARF 5 §6.2 (p. 148), §3.3.8.2, App. F |
| LLVM | `!dbg` metadata on each instruction (`DILocation`), which passes must preserve, merge or drop | By design: debug info "should have very little impact on the rest of the compiler. No transformations, analyses, or code generators should need to be modified because of debugging information." Upgrades drop debug metadata | `SourceLevelDebugging.md:19-21`; HowToUpdateDebugInfo; DeveloperPolicy |
| MLIR | A mandatory `Location` attribute on every op, uniqued, so the op holds one pointer. Kinds: `NameLoc`, `CallSiteLoc` (callee + caller), `FusedLoc`, `OpaqueLoc` ("a pointer to some data structure that is external to MLIR"), `UnknownLoc` | Not optional: "If a transformation replaces an operation by another, that new operation must still have a location attached" | `Tutorials/Toy/Ch-2.md:102-109`; builtin dialect docs |
| rustc | A `Span` on HIR nodes; a 12-byte `SourceInfo` per MIR statement | Spans hurt caching: "'span' information is very volatile, so including it in query result will increase the chance that the result won't be reusable." There is even `-Z incremental-ignore-spans` | `rdg/src/queries/incremental-compilation-in-detail.md:555-557`; `options.rs:2579-2580` |
| rust-analyzer | Bodies and their source maps are separate structures | Yes: typing whitespace doesn't invalidate types | `data_model_survey.md`, data-oriented §5 |
| Source maps (JS) | A separate file (`sources`, `names`, `mappings`), linked from the generated file by a `sourceMappingURL` comment or a `sourcemap` HTTP header | Yes: the generated code runs without it | ECMA-426 |
| Cranelift | `srclocs: SecondaryMap<Inst, RelSourceLoc>`, "not interpreted by Cranelift, only preserved". Locations are *relative* to a base kept in `FunctionParameters`, outside the hashed stencil, so a function that moves within its file keeps its cache key | Yes | `ir/function.rs:72-75, 183-186`; `ir/sourceloc.rs:10-14, 55-58` |

### 3.2 Why LLVM, MLIR and rustc keep locations inline, and why that reason doesn't apply here

They **rewrite** their IR in dozens of passes, and a location has to move with the instruction it describes. That's why LLVM has rules for preserving, merging and dropping locations, and MLIR requires one on every new op.

Spicy's `Circuit` is built once by lowering and never rewritten. The simulator's `Plan` adds internal nodes on its own side (`circuit.md` §4.3). So an entry pushed into a side table **in the same loop iteration** as its device (as `lower.rs` already does for names) can't drift from it. The tools that don't rewrite (DWARF's final tables, source maps, rust-analyzer's source maps, Cranelift's secondary map) all use side tables indexed like the items.

**Answer:** yes, origins are per-item side tables indexed like the items. They go in `CircuitNames`, never in `Circuit`, so a future plan-cache key never sees them (rustc's volatility lesson; Cranelift's base kept outside the stencil).

### 3.3 What an origin *is*: a handle the front-end interprets

The two front-ends mean different things by "where it came from":
- **SPICE:** a file, a span, and the chain of `X` lines that placed a subcircuit's device. That chain is MLIR's `CallSiteLoc` and DWARF's call coordinates.
- **Language:** a `FlatDeviceId`. Its `LocalInstance { at, instance }` leads to the `let` statement and its span through `DesignSourceMap` (`spicy_model/src/flat.rs:140-155`). The path is rebuilt from parent pointers, never stored (`model.md` §9, item 3).

Two precedents keep the IR out of this entirely:
- Cranelift's `SourceLoc` is "an opaque 31-bit number… Cranelift does not interpret source locations in any way" (`ir/sourceloc.rs:10-14`). Wasmtime interprets it as a Wasm offset.
- MLIR's `OpaqueLoc` points "to some data structure that is external to MLIR".

So: `Origin(u32)`, meaningful only to the front-end that made it.

One detail for later: `spicy_span::Span` has no file id. Once `.include` matters, rustc's approach keeps spans at 8 bytes: one byte-offset space across every file, with a `SourceMap` to find the file (`rdg/src/appendix/code-index.md:23-24`).

### 3.4 Model names

**Who reads them:**
- the numeric exporter, which must write `.model QN NPN(…)` and `Q1 c b e QN`;
- error messages;
- the report's model line (`engine_types.md` §9).

**Two problems with today's lowering:**
1. **The name is lost before lowering.** The parser clones each `.model` card into every instance, without its name (`netlist_models.rs:289-296`, `devices/bjt.rs:5-17`).
2. **Merging by value can join two differently named cards.** Lowering merges identical models by their numbers (`lower.rs:176-197`). If `.model QA` and `.model QB` have the same numbers, they become one entry, and there's no single name to give it.

**The precedent.** LLVM merges two constants by content only when they are marked `unnamed_addr`, meaning "the address is not significant, only the content" (LLVM `LangRef.md:836-841`). MLIR never puts names or per-run values in a uniquing key (`data_model_survey.md`, MLIR §2).

A model's name is significant here:
- ngspice's `altermod QA` changes one card;
- a knob can target one part's model (`circuit.md` §4.2, "each part gets its own model");
- an export should read like its input.

**So:** merge only entries that come from the same card (key = card id + resolved values), and store that card's name. Merging still does its real job, which is regrouping the instances the parser cloned from one card. Two cards that happen to have identical numbers stay two entries: rare, and a few dozen bytes each.

---

## 4. Opaque external references

### 4.1 What each system does

| System | The black box | How it's resolved | Cite |
|---|---|---|---|
| LLVM | `declare i32 @puts(ptr …)`: a signature and attributes, no body | The linker "resolves forward declarations". `available_externally` even carries a body for optimization without emitting it | `LangRef.md`, Module Structure; Linkage Types |
| MLIR | A symbol op that is a *declaration*: "Declarations do not define a new symbol but reference a symbol defined outside the visible IR"; a declaration can't be `public`. Unregistered ops are "completely opaque" and "must be treated conservatively" | Later tools, by symbol name | `SymbolsAndSymbolTables.md:60-62, 184-197`; `Tutorials/Toy/Ch-2.md:133-141` |
| CIRCT (hardware) | `hw.module.extern`: "an external reference to a Verilog module, including a given name and a list of ports", with optional parameters and a `verilogName`. An `hw.instance` refers to it by symbol and passes parameters | The Verilog toolchain, from the named file | CIRCT HW dialect docs |
| Cranelift | `ExtFuncData { name: ExternalName, signature, colocated }`. `UserExternalName { namespace: u32, index: u32 }`: "Cranelift does not interpret these numbers in any way" | The embedder's linker | `ir/extfunc.rs:292-315`; `ir/extname.rs:69-79` |
| rustc | A dependency's `rmeta`: signatures and metadata, no machine code; "`rmeta` files do not support linking" | The linker, from the `rlib` | `rdg/src/backend/libs-and-metadata.md:51-59` |

### 4.2 The common shape

1. **A declaration, stored once:**
   - the name the external tool knows it by;
   - its interface (ports or signature, parameter names);
   - where the body lives.
2. **Instances refer to the declaration by id,** plus their own connections and parameter values.
3. **The body is resolved by a later tool.** For us, that's ngspice reading `.include`.
4. **Everything else treats it conservatively.** A consumer that can't handle it refuses it explicitly.

### 4.3 For spicy

A vendor op-amp is `.include "lm358.lib"` plus `X1 in+ in- vcc vee out LM358`. The body is SPICE our simulator may not support (behavioral sources, extra model parameters).

There are two paths:
- **The engine deck** is exported from `FlatDesign` (`engine_plan.md` §5.1). It can pass a vendor part through without `spicy_circuit` knowing anything about it.
- **The numeric export** reads `Circuit + Params` (`pipeline.md` §6c). So `Circuit` needs an opaque kind once a design with a vendor part is exported numerically, or once a SPICE file whose included body we can't parse must round-trip. When the SPICE front-end *can* parse the body, it keeps inlining and flattening it, as it does today (`libs_phase.rs`, `subcircuit_phase.rs`), and nothing is opaque.

Build it when the first vendor part reaches the numeric exporter (sketch in §7 D7).

---

## 5. Extensibility and versioning

### 5.1 Open vs closed

- **MLIR is open.** "There is no fixed list of operations" (MLIR `LangRef.md`, Operations), and unregistered ops round-trip. MLIR itself advises against leaning on that: unregistered ops are "useful for bootstrapping purposes, but… generally advised against in mature systems" (`Tutorials/Toy/Ch-2.md:137-139`). The earlier survey already rejected open genericity for our hot loop (`data_model_survey.md`, MLIR §9).
- **LLVM is closed with escape hatches.** Its instruction set is closed. It extends through intrinsics (named calls) and external declarations.
- **rustc is closed.** Its IR enums are closed and carry no `#[non_exhaustive]`: MIR's `StatementKind` derives the usual traits and nothing more (`rustc_middle/src/mir/syntax.rs:304-306`). Every consumer lives in-tree, so a new variant is a compile error wherever it isn't handled.

**Spicy already has this shape** (`circuit.md` §4.11):
- a closed set of built-in kinds;
- one open kind by id (compiled OSDI models);
- one open kind by name (opaque subcircuits, §4).

That's LLVM's arrangement: instructions, intrinsics and declarations.

### 5.2 `#[non_exhaustive]` is the wrong tool inside a workspace

"Within the defining crate, `non_exhaustive` has no effect." Outside it, "non-exhaustive enums must be matched with a wildcard arm", and structs can't be built with a struct expression (Rust Reference, type-system attributes).

`spicy_simulate`, `spicy_netlist` and the exporter are *other crates*. The attribute would force a `_ =>` arm on them, and that arm silently swallows the next device kind: the opposite of what we want. The attribute is for published crates with consumers you can't update. Every consumer of `spicy_circuit` is ours.

### 5.3 Making the compiler find every consumer

Consumers that loop over kinds or fields don't break when a kind or field is added; they just miss it. The fix is exhaustive destructuring, with no `..`, in each consumer that must see everything:

```rust
// spicy_backends::ngspice::export (numeric): a new kind or model field is a compile error here
let Circuit { node_count, resistors, capacitors, inductors, diodes, bjts, vsources, isources } = circuit;
let BjtModel { polarity, is, bf, br, nf, nr, xtb, xti, eg, tnom } = *model;
```

The round-trip test (`export → spicy_netlist → lower`) catches a missed field only when its value differs from the default. The destructuring catches it always.

### 5.4 Versioning

| System | Policy | Cite |
|---|---|---|
| LLVM | "The current LLVM version supports loading any bitcode since version 3.0." "The textual format is not backwards compatible." Non-debug metadata is "defined to be safe to drop" | DeveloperPolicy |
| MLIR bytecode | "Versioned and stable", with per-dialect versions and an `upgradeFromVersion` hook; the promise holds only "assuming immutable dialects" | `BytecodeFormat.md:1-22` |
| Cranelift | Rejects the mismatch: "Expected a clif ir function for version {}, found one for version {}". The cache key changes with the version | `ir/function.rs:35-60`; `incremental_cache.rs:133-134` |

`Lowered` lives in memory, between crates built together, so a version field would protect nothing. Once a `Plan` or `Lowered` is cached on disk, use Cranelift's policy: a version marker, and on mismatch throw the cache away and recompute. Our inputs are source files, so rebuilding is always possible, which is why LLVM's and MLIR's upgrade machinery isn't worth having. The engine's report already has `schema: "spicy.check/0"` (`engine_types.md` §9.1).

---

## 6. Re-running with changes

### 6.1 What each system does

| System | Fixed part | Changing part | Cite |
|---|---|---|---|
| Cranelift | `FunctionStencil`: "can be the same for two functions that would be compiled the same way"; hashed | `FunctionParameters` (the source-location base, external names), applied after compilation; the ISA `Flags` are in the key of the compiled code | `ir/function.rs:65-75, 148-157`; `incremental_cache.rs:135-147` |
| salsa / rust-analyzer | High-durability inputs: "data read from the standard library or other inputs that aren't actively being edited by the end user" | Low-durability inputs (the edited file). Queries that read only high-durability inputs skip re-validation | salsa book, Durability; `ra/docs/book/src/contributing/guide.md:79-83` |
| rustc | Cached query results | `[TRACKED]` options clear the cache; `[UNTRACKED]` ones don't | `options.rs:332-356` |
| JAX | The compiled trace, keyed on shapes and *static* arguments | Dynamic arguments | `data_model_survey.md`, data-oriented §6 |
| FMI | Structural parameters (Configuration Mode only) | `fixed`/`tunable` parameters; the tolerance argument | FMI 3.0.1 §2.4.7.4, §2.3.2 |
| Modelica / OpenModelica | Parameters marked `Evaluate=true`, folded into the executable | Everything else, via `-override`; the tolerance via `-tolerance` | spec §18.3; OpenModelica flags |

### 6.2 Spicy's tiers

| Tier | Spicy | Changes | Enters which key | Precedent |
|---|---|---|---|---|
| Structure | `Circuit` + structural parameters (the structure key, `circuit.md` §4.3) | a design edit | the **Plan** key | Cranelift stencil; FMI structural parameters; `Evaluate=true`; JAX static args; salsa HIGH |
| Per-run numbers | `Params`, **including `temp`** | every run | the result key (it *is* the point) | FMI parameters; JAX dynamic args; `-override`; salsa LOW |
| Per-request settings | `Analysis`, `SimulationConfig` (tolerances) | per request | the result key, **not** the Plan key | FMI tolerance argument; Cranelift flags; rustc `[TRACKED]` |
| Provenance | `CircuitNames` (names, origins) | with the source text | never hashed | Cranelift's source-location base; rustc span volatility; source maps |

Worked through the engine on our simulator (M4):
- One `Arc<Plan>` serves both tolerance profiles, because the sparsity pattern and the KLU symbolic analysis don't depend on `reltol`.
- Each request brings its `Params` (with `temp` written by the binding) and its `SimulationConfig`.
- The engine's run table already keys rows by `(point, Tolerance)` (`engine_types.md` §6).

Nothing new is needed; the proposal's `Conditions` would only have split the per-run tier into two structs.

### 6.3 One caveat: compiled models can read solver settings

OSDI hands models a list of simulator parameters (`OsdiSimParas`: names and values) and the temperature at `setup_model`/`setup_instance` (`osdi_0_3.h:74-79, 176-180`). VACASK therefore lists `tnom, temp, gmin, … reltol, vntol, abstol` as **"mapping-affecting"**: a Verilog-A model may collapse internal nodes based on them, so changing one can rebuild the equations (`docs/cmd-options-special.md`).

When the OSDI kind arrives, the values passed to setup join the structure key *for OSDI devices*. That's the one place tolerances can touch structure. Built-in kinds never read them.

---

## 7. Recommendations, as decision briefs

### D1. Where the run's temperature lives

| Option | Shape | References | Cost |
|---|---|---|---|
| **A. `Conditions` beside `Params`** (proposal) | `Lowered { …, conditions: Conditions { temp, tnom } }`; `simulate_op(&c, &p, &cond, &cfg)` | ngspice's task, VACASK's options (both also hold tolerances); OSDI's `temperature` argument | One more argument on every `simulate_*`, derive and export call. The binding needs two kinds of target. The round trip compares one more struct |
| **B. A field of `Params`** (recommended) | `Params { temp, … }`; call sites unchanged | FMI (temperature is a parameter); Modelica (`T` is a parameter); Xyce (`TEMP` is a device-manager parameter, stepped like one); ngspice's `.dc temp`; JAX (dynamic args are one value set); the pipeline's rate-of-change rule | A hand-written `Default` for `Params`, since a derived one would give 0 K |
| C. In the run request | `SimulationConfig { temp, … }` | ngspice's task | Mixes physics with numerics; the knob binding would write into the request. Rejected |

```rust
// B
pub struct Params {
    /// The circuit's temperature (K): SPICE `.temp`, the language's ambient `temp`.
    /// Each device adds its own `DeviceTemperature`. Not simulated yet.
    pub temp: f64,
    pub resistor_models: Vec<ResistorModel>,
    // … unchanged
}

// the engine's binding, run 17: temp is one more field
params.temp = 263.15;                                   // −10 °C
simulate_op(&circuit, &params, &config)?;               // same three arguments as today
```

**Recommendation: B.** Group it into a named struct only if a second circuit-wide physical value ever appears. VACASK's `scale` is folded by SPICE lowering, and `gmin` is numerics, so there's no candidate today.

### D2. `tnom`

| Option | Shape | References | Cost |
|---|---|---|---|
| A. A global default plus a per-model `Option` (proposal) | `BjtModel.tnom: Option<f64>`, `Conditions.tnom`; every derive step does `m.tnom.unwrap_or(cond.tnom)` | MLIR's scoped data layout (lookup walks the scopes) | An `Option` and a global in the IR, read on every derive |
| **B. Resolved at lowering** (recommended) | `BjtModel.tnom: f64`; `spicy_netlist::reader::lower` fills in `.options tnom` where the card has none | ngspice writes the default into the model at setup (`bjttemp.c:42`, `ressetup.c:29`); Modelica keeps `Tnom` per model (`NPN.mo:22`); `staged_ir_survey.md` §3 already lists `tnom` as a SPICE dialect default | About five lines in SPICE lowering |

```rust
// spicy_circuit (B)
pub struct BjtModel {
    // … existing fields
    /// Forward and reverse β temperature exponent. Not simulated yet.
    pub xtb: f64,
    /// Saturation-current temperature exponent. Not simulated yet.
    pub xti: f64,
    /// Energy gap (eV) for IS(T). Not simulated yet.
    pub eg: f64,
    /// Temperature (K) at which this model's parameters were measured. Not simulated yet.
    pub tnom: f64,
}

// spicy_netlist::reader::lower (B): `.options tnom` is a SPICE default, folded like `scale`
let tnom = card.tnom.map(celsius_to_kelvin).unwrap_or(deck_tnom);   // deck_tnom: 300.15 K unless `.options tnom`
```

The language's D-A model already states its own TNOM (25 °C, `engine_plan.md` §5.4), so the language never needs a global.

**Recommendation: B.** Add the same resolved `tnom` to the R, C, L and diode models in the same change. Their `tc1`/`tc2` are measured from their model's `tnom` (`restemp.c:83`), so otherwise `.options tnom` would be accepted and silently dropped for them, breaking the "nothing accepted is dropped" rule (`circuit.md` §4.6).

### D3. `xtb`, `xti`, `eg`

As proposed (`engine_plan.md` §5.2). Defaults: 0, 3, 1.11 eV, the same as Modelica's `NPN.mo:23-25` and ngspice's.

### D4. Solver options

| Option | Shape | References | Cost |
|---|---|---|---|
| A. Resolved values in `Lowered` | `options: SolverOptions { reltol: f64, … }` holding SPICE defaults | — | Can't tell "the file said 1e-3" from the default; the language front-end would have to invent numbers |
| **B. The file's request in `Lowered`; the run gets `SimulationConfig`** (recommended) | `options: SolverOptions { reltol: Option<f64>, … }` | FMI `<DefaultExperiment>` ("a tool may ignore this") plus the tolerance argument at run time; Modelica's `experiment` annotation; rustc and Cranelift keep options beside the IR | One small type |
| C. Nothing in `Lowered`; `spicy_netlist` returns options separately | a second return value | VACASK's separate control block | A second channel for what `analyses` already carries |

```rust
// spicy_circuit/src/analysis.rs: requests live here, so the exporter needn't import the simulator
/// Newton tolerances the netlist asked for (`.options`). `None`: the backend's default.
/// A default request, like Modelica's `experiment` annotation: the caller may override it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SolverOptions {
    /// Relative tolerance on node voltages and branch currents (SPICE `reltol`).
    pub reltol: Option<f64>,
    /// Absolute voltage tolerance, V (SPICE `vntol`).
    pub vntol: Option<f64>,
    /// Absolute current tolerance, A (SPICE `abstol`).
    pub abstol: Option<f64>,
}
pub struct Lowered { pub circuit: Circuit, pub params: Params, pub names: CircuitNames,
                     pub analyses: Vec<Analysis>, pub options: SolverOptions }

// spicy_cli: backend default < the file < a command-line flag
let newton = NewtonConfig::default().with(&lowered.options).with(&cli.options);
simulate_op(&lowered.circuit, &lowered.params, &SimulationConfig { newton, ..Default::default() })?;

// the engine on our simulator (M4): one Plan, the profile chosen per request
let config = match request.tolerance { Tolerance::Engine => &engine, Tolerance::Tight => &tight };

// the ngspice exporters take options as an argument (engine_plan.md §5.1 already does)
```

**Recommendation: B.** Solver options belong with the run request. Engine runs ignore the file's value and use their profiles; the report records which profile ran.

### D5. Model names

```rust
pub struct CircuitNames {
    // … existing
    /// One per entry of `Params::bjt_models`: its `.model` card's name, or a
    /// name lowering made up (a kind's default model; one part's own model: its path).
    pub bjt_models: Vec<String>,
    // + resistor_models, capacitor_models, inductor_models, diode_models
}
```

**Recommendation.** Add the names, and key the model merge on (card, resolved values) instead of values alone (§3.4).

The parser must carry the card's identity into each instance spec. An index into the deck's model table is better than a cloned `String`. The exporter turns names into valid, distinct SPICE identifiers (`engine_plan.md` §5.1 already writes `QM_<path>`).

### D6. Origins (when a reader exists)

**Readers that would justify building it:**
- the first simulator error that names a device and shows a snippet;
- the editor's jump-to-source.

Until then, names cover results (keep-it-simple rule: no reshape before a reader).

```rust
/// Where a device came from, numbered by the front-end that lowered it: an index
/// into the SPICE deck's origin table, or a `FlatDeviceId`. Never interpreted here
/// (Cranelift's `SourceLoc`, MLIR's `OpaqueLoc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Origin(pub u32);

/// A device's name and origin, pushed together with the device.
pub struct DeviceName { pub name: String, pub origin: Origin }
pub struct CircuitNames { pub resistors: Vec<DeviceName>, /* … one per kind */ }

// spicy_netlist's own table, which `Origin` indexes: MLIR's CallSiteLoc, DWARF's call coordinates
pub struct SpiceOrigin { pub span: Span, pub placed_by: Option<u32> }   // + a file id with `.include`
```

**Recommendation.** One opaque handle per device, next to its name in `CircuitNames`, and never in `Circuit`.

### D7. Opaque subcircuits (when the numeric exporter meets a vendor part)

```rust
// Structure: a declaration once (LLVM `declare`, CIRCT `hw.module.extern`), instances by id
pub struct ExternalDef {
    /// The subcircuit's name in its library (`LM358`).
    pub subckt: String,
    /// The file that defines it, and its `.lib` section (a corner), if any.
    pub library: PathBuf,
    pub section: Option<String>,
    pub port_count: u32,
    /// Parameter names from its `.subckt … params:` line, in order.
    pub params: Vec<String>,
}
pub struct External { pub def: ExternalDefId, pub first_port: u32 }  // ports: circuit.external_ports[first_port..][..port_count]
// in Circuit: external_defs: Vec<ExternalDef>, externals: Vec<External>, external_ports: Vec<NodeId>
// in Params:  externals: Vec<Box<[Option<f64>]>>   (per instance, in the def's order; None = the subcircuit's default)
// in CircuitNames: externals: Vec<DeviceName>
```

**How each backend handles it:**
- **ngspice export** writes one `.include`/`.lib` per library and `X… LM358 params: …` per instance.
- **Our simulator** refuses in `Circuit::verify` ("external subcircuit LM358 (lm358.lib) runs only on ngspice"). That's MLIR's rule that opaque ops are "treated conservatively", and the legality check of `data_model_survey.md` (MLIR §7).

The declaration's strings sit in `Circuit` on purpose: like an LLVM declaration's name, they *are* what the part means, and they must enter any plan key. They are per definition, never per instance, and the simulator never reads them.

A corner that picks a different `.lib` section changes the definition. That makes it a structural choice, like the structure key of `circuit.md` §4.3.

### D8 and D9

- **D8:** closed per-kind tables, no `#[non_exhaustive]`, and exhaustive destructuring in the exporter, the raw writer and the simulator's device build (§5).
- **D9:** no version field until something is stored on disk (§5.4).

---

## 8. Worked example: the CE amplifier, SPICE path

```
.temp 60
.options tnom=25 reltol=1e-6 vntol=1e-9 abstol=1e-15
.model QN NPN is=1e-14 bf=200 xtb=1.5
Q1 out base emit QN
```

`spicy_netlist::reader::lower` produces:

```
Circuit        bjts[0] = { c: out, b: base, e: emit, model: #0 }            structure: unchanged by any line above
Params         temp = 333.15 K                                              from .temp
               bjt_models[0] = { NPN, is 1e-14, bf 200, br 1, nf 1, nr 1,
                                 xtb 1.5, xti 3, eg 1.11, tnom 298.15 K }   tnom from .options (the card has none)
Names          bjts ["Q1"], bjt_models ["QN"]
Analyses       […]
Options        { reltol: Some(1e-6), vntol: Some(1e-9), abstol: Some(1e-15) }
```

Three consumers:
- **The CLI** builds `SimulationConfig` from the defaults, then `Options`, then any flags, and calls `simulate_op(circuit, params, config)`.
- **The numeric export** writes back:
  - `.temp 60`;
  - `.options reltol=1e-6 vntol=1e-9 abstol=1e-15` (given as an argument);
  - `.model QN NPN(is=1e-14 bf=200 br=1 nf=1 nr=1 xtb=1.5 xti=3 eg=1.11 tnom=25)`.

  Re-lowering it gives the same `Circuit`, `Params` and `Options`, because the resolved `tnom` is now on the card.
- **The engine at run 17** (our simulator, M4): the same `Arc<Plan>`; the binding writes `params.temp = 263.15` and `params.bjt_models[0].bf = …`. A decisive point re-runs with `Tolerance::Tight`: same `Plan`, same `Params`, a different `SimulationConfig`.

---

## 9. What this would change in agreed documents (not applied here)

- **`pipeline.md` §3:** the `Conditions` row becomes "`temp` is a field of `Params`; `tnom` is resolved into each model at lowering". Add a `SolverOptions` row ("the file's request; the run uses `SimulationConfig`").
- **`engine_plan.md` §5.2:**
  - `BjtModel.tnom: f64`, resolved; the "new `Conditions`" row becomes `Params.temp`;
  - R, C, L and diode models gain `tnom`.
- **`engine_plan.md` §5.3:** `.temp` → `Params.temp`; `.options tnom` → each model's `tnom`; `SolverOptions` fields are `Option`.
- **`engine_plan.md` §5.4 and roadmap M1e/M1f** (lines ~408–419):
  - "sets `Conditions.temp`" → "sets `Params.temp`";
  - the round trip compares `Circuit + Params + options`.
- **`circuit.md`:**
  - §4.2: merge identical cards only within one card (§3.4);
  - §4.9: origins are opaque front-end handles;
  - §6: the type sketch.

---

## 10. Uncertainties

- **Summarizing fetches.** The Modelica spec (§18.3, §18.4), OpenModelica's flags, the CIRCT HW docs, ECMA-426, the salsa durability page, MLIR's builtin location docs, the Rust Reference, and LLVM's DeveloperPolicy and HowToUpdateDebugInfo were read through a summarizing fetch. The quotes match the phrasing returned but weren't checked against the raw text. Everything else was read as raw text or source.
- **Not verified:** whether the OpenModelica setup XML carries the tolerance as well as parameter values. The flags page doesn't describe the file.
- **Judgement calls, not precedent:**
  - a plain `Params.temp` field rather than a one-field struct;
  - keying model merges on the card, which costs storage only when two differently named cards are identical;
  - deferring origins and opaque instances until a reader exists.
- **Not measured.** The speed claims are structural: no new argument, no per-run work, no change to the `Plan`.
