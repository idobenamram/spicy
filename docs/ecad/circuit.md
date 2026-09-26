# The Circuit Data Model (`spicy_circuit`)

> 2026-09-26 · Design note for pipeline step 3 (`pipeline.md` §9). **Status: all recommendations agreed (2026-09-26); nothing is implemented yet.** It revises three earlier recommendations (§8 lists them).
> Grounded in `research/data_model_survey.md`: ngspice, Xyce, Gnucap and OSDI/VACASK read at the source level, plus MLIR and data-oriented compilers (Cranelift, rustc, rust-analyzer, Zig, Carbon, JAX). Citations here are shortened to file:line; the survey has the full paths.

This note answers four questions:
1. How does a circuit flow through our code **today**?
2. What will a **bigger simulator** demand from the data model?
3. For each design choice, **what do established simulators and compilers do**, and do we follow them or depart, and why?
4. What exactly does **`spicy_circuit`** hold, and how does the example below flow through it?

---

## 1. The example used throughout

```
ce stage
VCC vcc 0 DC 12
R1 vcc base 47k
R2 base 0 10k
RC vcc out 4.7k
RE emit 0 2k m=2
Q1 out base emit QN
.model QN NPN bf=200
.op
```

`RE … 2k m=2` is two 2 kΩ resistors in parallel, 1 kΩ in effect. Today's simulator solves it to:

| V(base) | V(emit) | V(out) | I(VCC) |
|---|---|---|---|
| 2.053 V | 1.273 V | 6.047 V | −1.478 mA |

---

## 2. Today, step by step

| # | Stage | What happens to the example | Code |
|---|---|---|---|
| 1 | Parse | Statements → includes → `{}` expressions → subcircuits collected and flattened (`X1.mid`) → instances | `spicy_parser/src/lib.rs:149-162` |
| 2 | Nodes | Numbered by first appearance: ground 0, vcc 1, base 2, out 3, emit 4 | `node_mapping.rs:45-66` |
| 3 | Branch rows | **The parser** gives each voltage source and inductor an MNA branch row as it reads it: VCC → branch 1 | `instance_parser.rs:634, 957` |
| 4 | Models | `.model QN` is looked up by name and **cloned into each instance** that uses it | `instance_parser.rs:774, 863` |
| 5 | Deck | `ResistorSpec { name, span, nodes, resistance: Some(2k), m: Some(2), … }`, and so on: every value an `Option<Value>` | `spicy_parser/src/devices/` |
| 6 | Devices, **per analysis** | `Devices::from_spec` resolves defaults (IS = 1e-16), folds `m`/`scale`/`area` (RE → 1000 Ω), clones names | `spicy_simulate/src/devices/mod.rs:29-47` |
| 7 | Pattern, **per analysis** | `setup_pattern` builds the sparsity pattern and writes matrix positions into each device's `stamp` field | `matrix.rs:80-105`, `setup_pattern.rs` |
| 8 | Newton loop | Each iteration: clear the matrix, stamp every device (linear ones too), KLU analyze (first time) → factor or refactor, solve, check convergence | `trans.rs:62-100`, `dc.rs:21-50` |
| 9 | Results | Vectors in MNA order, named from the `NodeMapping` | `node_mapping.rs:110-135` |

Three special paths:
- **DC sweep** writes the swept source's value into the device between points, with no rebuild (`dc.rs:81-87`). It's the only place values change without rebuilding. A missing source name panics (`dc.rs:78`).
- **Transient** keeps trapezoidal history in a `HashMap<&str, f64>` keyed by capacitor name (`trans.rs:113, 152, 165`).
- **AC** is separate: dense matrices rebuilt per frequency, and only R, C, L and sources are stamped (`ac.rs:79-122`).

**What holds us back:**
1. Everything below the Deck is rebuilt for every analysis. Every engine run would rebuild it again.
2. The parser allocates MNA branch rows: a solver detail in the front-end.
3. There's one mutable set of devices, so two parameter sets can't run at once.
4. Strings (names, spans) sit in the device structs and in the transient history.
5. Model data is copied into every instance.

---

## 3. Where the simulator is going

The data model shouldn't need rework when these arrive:

| Future feature | What it demands |
|---|---|
| **Gummel–Poon BJT, diode RS** | Internal nodes that exist only when a resistance is nonzero (ngspice `bjtsetup.c:453-504`). `area` scales ~15 quantities by different rules: IS ×area, RB ÷area, … (`bjttemp.c:91-316`). Junction charges need per-device state (a BJT has 34 state slots in ngspice, `bjtdefs.h:384`) |
| **BSIM MOSFET** | ~890 model parameters vs ~44 instance parameters (ngspice `b4.c:31-1081`). A size-dependent cache shared by instances with the same L, W and NF. Mode selectors (`rgateMod`, `rbodyMod`) that add internal nodes |
| **Temperature** | Values derived per model and per instance, computed once per run, not in every Newton iteration |
| **Controlled sources, mutual inductors** | One device referring to another: F/H to a voltage source's current, K to two inductors |
| **Verilog-A through OSDI** | Opaque model and instance blobs; parameters by id; instance parameters that can be set on the model; `$mfactor` applied by the compiled model; node collapsing decided at setup |
| **The engine loop** | Thousands of runs with the same wiring and different values, on parallel threads, maybe later SIMD across runs |
| **Sensitivities** | Addressing one parameter field by id (∂ output / ∂ field) |
| **Big circuits** | Parallel device evaluation inside one run |

---

## 4. Decisions

Each decision gives what the simulators do, what compilers do, and the recommendation. **←** marks a change from an earlier recommendation.

### 4.1 Keep structure and values in separate storage

**Simulators.** None keeps a per-run value set apart from the structure. Values, derived values and matrix locations live in the device objects: ngspice's instance struct (`resdefs.h:30-60`), Xyce's instance members, OSDI's instance blob, Gnucap's shared commons (mutated in place by sweeps). They separate the stages **in time** instead:
- ngspice redoes setup on every run (`cktdojob.c:137-170`), except `.dc` sweeps, which write the value and re-run the temperature routine (`dctrcurv.c:89-99`).
- Xyce builds topology once, then applies `.STEP` and sampling values with `setParam` + `processParams` (`N_ANP_SweepParam.C:530-541`).
- VACASK re-runs setup only for flagged instances, and rebuilds unknowns only if node collapsing changed (`cirparams.cpp:255-512`).
- Gnucap re-runs only its `precalc` passes after a `.param` change (`u_sim_data.cc:264-266`).

The cost of one mutable value set shows in Xyce's embedded sampling: `processParams` re-runs for every sample on every matrix load (`N_LOA_ESLoader.C:212-251`).

**Compilers.** Cranelift splits a function into a hashable `FunctionStencil` and `FunctionParameters` that "don't matter when caching" (`function.rs:64-82`). JAX's `jit` compiles once per structure and re-runs with new values; `vmap` batches values. MLIR shares immutable data and gives each thread its own mutable state.

**Recommendation.** Three pieces: `Circuit` (structure, shared read-only), `Params` (values, one per run) and `CircuitNames` (a side table, §4.9). We depart from the simulators' storage because we run many value sets over one structure, concurrently. That's the problem JAX's split solves, and one mutable instance set can't.

### 4.2 Models are shared, not copied into instances ←

**Simulators.** All four share one model among its instances:
- **ngspice:** one `GENmodel` per `.model`, with an instance list and back-pointers (`gendefs.h:18-48`). An instance without a model uses a default model for its letter (`inp2r.c:160-222`).
- **Xyce:** one `Model` per `.model`, owning its instances (`N_DEV_DeviceMaster.h:450-451`), plus a default model (`:487-500`).
- **Gnucap:** reference-counted parameter objects (`COMMON_COMPONENT`); changing one component's parameter clones its common first (copy-on-write, `e_compon.cc:473-486`).
- **OSDI:** separate model and instance blobs. VACASK groups Device → Models → Instances.

It matters more as the simulator grows:
- **Memory:** a BSIM4 model has ~890 parameters (ngspice `b4.c:134-1081`), about 7 KB of numbers; an instance has ~44.
- **Shared precomputation:** model-level temperature values (`bjttemp.c:42-69`), and BSIM's size-dependent parameters, shared by instances with the same L, W and NF (ngspice `b4temp.c:419-453`, Xyce `B4p82.C:767-803`).
- **Process corners** change model parameters, and every instance must follow.

**Compilers.** Interning. MLIR uniques attributes and types in its context, so equality is a pointer compare and duplicates are stored once. rustc interns types. Zig's InternPool cut Ghostty's peak memory from 1130 MiB to 463 MiB. MLIR's own warning: never intern values that change on every run.

**Recommendation.**
- Per device kind, a **model table** and an **instance table**. Each instance holds a `ModelId`. An instance without a model points to the kind's default model.
- **Identical model cards are merged** at lowering.
- An instance that overrides a model parameter gets **its own model entry**, made at lowering (Gnucap's copy-on-write). The language's `q1: Npn { beta: 100..=300 }` works this way: each part gets its own model.
- **Per-run mismatch** (Monte Carlo, local variation) goes in instance fields, never into new models. BSIM4 does the same with its instance-level `delvto`.

This reverses what I proposed before (copy `.model` values into each device). The earlier survey had already marked it "a judgement call, not precedent" (`staged_ir_survey.md` §5), and it doesn't hold up against the sources.

### 4.3 Internal nodes and branch rows belong to the simulator

**Simulators.**
- **ngspice:** the parser numbers only external nodes. Each device's setup creates its internal nodes and branch rows (`CKTmkVolt`, `CKTmkCur`), and whether they exist depends on values. Diode RS = 0 means no internal node (`diosetup.c:390-443`); likewise BJT RB/RC/RE (`bjtsetup.c:453-504`); BSIM4's selectors, and even whether a `.noise` analysis exists (`b4set.c:58-72`).
- **Xyce:** each instance declares its internal-node count from its values, and collapses its local stamp with maps (`Diode.C:497-537`, `N_DEV_DeviceInstance.C:1165-1190`). Topology is built once, and nothing guards a later value change that would change the count (`B4p82.C:293-306`).
- **OSDI:** the descriptor lists internal nodes. `setup_instance` decides which to collapse, from parameters and temperature, never from the operating point. VACASK compares the collapse pattern after setup and rebuilds unknowns and sparsity only if it changed (`osdiinstance.cpp:318-381`).
- **Gnucap** decides at the first elaboration and never again, so structure goes stale after a sweep (`mg_out_dev.cc:316-344`).

**Compilers.** JAX re-traces when a *static* argument changes. Cranelift caches compiled code by a structural key. MLIR analyses declare what invalidates them.

**Recommendation.**
- `Circuit` holds **external terminals only**. The simulator allocates branch rows and internal nodes. Today's branch allocation in the parser (`NodeMapping::insert_branch`) moves out.
- A few **structural parameters** decide the structure (RS = 0 or not, mode selectors). The simulator's plan records a **structure key** computed from them. A `Params` with a different key gets another plan, built once and cached (JAX's static-argument rule). Never silently reuse a plan built for another key (Xyce's gap), and never freeze the structure at the first build (Gnucap's).
- For the engine: if a knob's range crosses a structural value (RS from 0 to 10 Ω), the key comes from the knob's range, not its nominal value.

### 4.4 The simulator owns the order of unknowns ←

**Simulators.** None keeps netlist order:
- **ngspice** sets up device types in its table order (…, `ind`, …, `vsrc`), with instances in *reverse* netlist order (`cktsetup.c:100-106`, `cktcrte.c:62-64`).
- **Xyce** uses the reversed breadth-first order of the circuit graph (`N_TOP_CktGraphBasic.C:185-205`).
- **VACASK** sorts by name, ground first (`circuit.cpp:1203-1231`).
- **Gnucap** uses reverse flat order (`u_sim_data.cc:189-239`).

KLU reorders the matrix anyway: AMD plus BTF by default (`klu/mod.rs:81-82`). So the MNA order only changes the order of results and last-digit rounding.

**Recommendation.** The simulator numbers unknowns however suits it. Results are keyed by id and printed in the circuit's order: nodes by first appearance, devices by kind and index. This drops the `order` list I proposed last time. Snapshots where voltage sources and inductors interleave will list branch currents in a new order, as one reviewed change.

### 4.5 `m`, `area` and `scale` ← (revises `pipeline.md` decision 7)

**Simulators.**
- **`area` is device physics everywhere.** ngspice applies it in the temperature routine, to many quantities by different rules (`bjttemp.c:91-316`). Gnucap applies it in the model's computed parameters (`d_diode.model:112-135`).
- **`m` is applied inconsistently in ngspice:** folded in the temperature routine for R and diodes, applied at load for C, L, BJT and BSIM4 (`restemp.c:107-110`, `capload.c:44`, `bjtload.c:142`).
- **Xyce applies `m` by hand at every stamp site,** which has already caused a bug: BJT noise includes `m` for RE but omits it for RC (`BJT.C:2656-2668`).
- **Gnucap applies `m` in one place,** at every load (`e_elemnt.h:199`). **OSDI** compiled models apply `$mfactor` to every residual, Jacobian entry and noise source (`builder.rs:523-548`).

**Compilers.** MLIR: don't drop information a later stage needs. If a value is folded early, every run that changes the underlying parameter must fold it again.

**Recommendation.**
- `m` and `area` stay **instance parameters** in `Params`. No front-end folds them.
- `area` is applied by each device kind's own derive step (§4.7), because its rule is physics.
- `m` is applied by the simulator **in one place per device kind**. Compiled OSDI models apply their own.
- The instance `scale` on R, C and L is SPICE dialect (the language has none), so the SPICE lowering folds it into the value.

Step 1's work carries over: `from_spec` *is* the simulator's derive step today. Its folding code and tests move into the new derive step.

### 4.6 What `Params` holds, and how the engine addresses it

**Simulators.**
- ngspice keeps a "given" bit next to each value, because defaults overwrite the same fields (`ressetup.c:29-42`). It needs extra copies to stay idempotent (`captemp.c:75`).
- Xyce has a declarative parameter table per device (name, default, member pointer, given flag, scaling flags). That gives generic addressing by name, like `"R1:R"` (`N_DEV_DeviceEntity.C:590-660`), but `given()` is a string lookup (`:1922`).
- OSDI addresses parameters by id. `setup_instance` resolves each one as instance value, else model value, else default (`setup.rs:239-259`).

**Compilers.** MLIR moved op data from uniqued dictionaries into typed inline "Properties", because every setter copied, re-hashed and leaked a dictionary. Carbon and Zig store fixed-size rows, with side arrays for variable-size data. Cranelift and the "array of variant arrays" pattern keep one dense table per kind, with static dispatch.

**Recommendation.**
- **Built-in kinds:** plain `Copy` structs in SI units, **fully resolved** at lowering. `Option` only for a default that depends on another value or on the analysis: `r_ac` defaults to `r`; a pulse's rise time defaults to the time step.
- **A field table per kind** (name → field id), generated by a small macro. It's Xyce's table, but resolved once: the engine's binding turns `amp.q1.beta` into (kind, model or instance, index, field) at setup, and each run does a direct write.
- **OSDI kinds** are the exception. Their defaults and fallbacks live inside the compiled model, so their `Params` hold (parameter id, value) pairs and resolution happens in `setup_instance`.
- **Only fields the solver reads today.** Today's unused fields (`temp`, `dtemp`, `noisy`, `off`, BJT `ic`, `tc1`, `tc2`) come back with the feature that uses them.

### 4.7 Derived values come in three levels (arrives with temperature, step 5)

**Simulators.**
- **ngspice** derives per model (`bjttemp.c:42-69`), per size bucket (BSIM4's `pParam`), and per instance.
- **Xyce** derives in the model's `processParams`, the instance's `updateTemperature`, and a BSIM size cache keyed by temperature too.
- **OSDI** derives per instance only (no model level), so model-level values are duplicated in every instance.
- **Gnucap** recomputes temperature values on every evaluation (`mg_out_model.cc:717-719`).

**Recommendation.** A `Derived` set per run, with a model-level table, later a size-bucket table for BSIM, and an instance-level table. It's computed once per run from `Params` and the run's conditions (temperature). The Newton loop reads only `Derived`.

### 4.8 Cross-device references are typed ids

**Simulators.**
- **ngspice** stores the controlling device's name at parse time and resolves it on every setup (`cktfbran.c:20-33`, `mutsetup.c:46-61`).
- **Xyce** rewrites F/H sources as behavioral sources and resolves the dependency to an unknown index once (`N_TOP_Topology.C:263-380`). It bundles K into one device at parse time.
- **Gnucap** looks names up during elaboration (`e_ccsrc.cc:27-51`).

**Compilers.** MLIR refers across isolated regions by name (symbols), because isolated code can't hold pointers into other regions. The MLIR report's lesson for us: our shared `Circuit` is immutable, so resolve names once at lowering into typed ids.

**Recommendation.** `Cccs { out: [NodeId; 2], control: VsourceId }` and `Mutual { a: InductorId, b: InductorId }`, resolved at lowering. A missing name or wrong kind is a lowering error with a span. The simulator turns ids into matrix rows.

### 4.9 Names live in a side table

**Simulators.** All four keep names in the device objects and look them up through hash tables (ngspice `cktdefs.h:330-331`; Xyce's string addressing).

**Compilers.**
- rust-analyzer separates position-free data from its source map, so edits don't invalidate it (`item_tree.rs:12-14`).
- Cranelift keeps names in a side table, and its name → id map exists only while building (`function.rs:77-81`).
- rustc notes that spans change so often that keeping them in cached results makes the results hard to reuse.

**Recommendation.** Lowering returns `CircuitNames` beside the `Circuit`: node names, device paths (`X1.R1`, `amp.r1`), and origins (spans). The simulator never reads a string. Results are by id, and names are attached at the edge (CLI, raw writer, engine).

### 4.10 State, parallelism and batching

- **State.** Every simulator uses flat state vectors plus per-instance offsets: ngspice's `CKTstates` (`capsetup.c:102-103`), Xyce's state indices, OSDI's `state_idx`. → Our transient state becomes flat vectors with offsets in the plan; the name-keyed map goes.
- **Across runs (first).** Share `Circuit` and the plan behind `Arc`. Give each thread its own `Params`, `Derived` and workspace (matrix values, KLU numeric factorization, solution), cleared and reused between runs. That follows rustc's per-worker arenas and Cranelift's reused `Context`. No locks in the Newton loop.
- **Inside a run (later).** ngspice's BSIM4 OpenMP path evaluates instances in parallel into per-instance fields, then adds them into the matrix serially (`b4ld.c:81-87, 5436-5460`). OSDI's separate `eval` and `load` allow the same. → Keep device evaluation separate from the matrix scatter.
- **SIMD across runs (maybe).** It's plausible, because every run has the same sparsity pattern and KLU symbolic analysis (NVIDIA cuDSS's "uniform batch" and JAX's `vmap` work this way). The risk is that Newton iteration counts and pivots diverge between runs. → Not now. Keeping per-run values in their own tables, indexed like the circuit, keeps it possible later.

### 4.11 Device kinds: a closed set plus compiled models

**Simulators.**
- **Xyce** lets hot devices replace the generic per-instance virtual calls with concrete loops (`N_DEV_Resistor.C:1211-1350`).
- **VACASK** makes one virtual call per device type, then loops over its instances (`circuit.cpp:1450-1478`).

**Compilers.** The MLIR report names MLIR's open-world genericity (string op names, attribute dictionaries, virtual interfaces) as the wrong fit for a hot loop. It recommends closed enums and one table per kind, dispatched once per batch.

**Recommendation.** Built-in kinds as per-kind tables with monomorphized loops. One more kind, "compiled model", for OSDI: a module id plus opaque blobs, dispatched once per batch of instances.

### 4.12 A flat circuit, with analyses beside it

- **Flat.** ngspice and Xyce flatten subcircuits. VACASK and Gnucap keep the hierarchy during elaboration but evaluate flat lists (`circuit.cpp:686-700`). → `Circuit` is flat; hierarchy lives above it (the Deck's subcircuits, the language's `Design`).
- **Analyses** live in `spicy_circuit` with ids resolved: a DC sweep names a `VsourceId`, not a string. The ngspice exporter can then read them without importing the simulator, and a bad name becomes a lowering error instead of today's panic.

---

## 5. The proposed flow, with the example

```
parse ─► Deck                  unchanged: SPICE as written
          │ spicy_parser::lower                                 once per netlist
          ▼
Circuit   nodes 0..4 (0 = ground)                               structure, shared
          resistors  #0 1–2   #1 2–0   #2 1–3   #3 4–0          all use ResistorModel #0 (default)
          bjts       #0 c=3 b=2 e=4                             uses BjtModel #0
          vsources   #0 1–0
Params    resistor models  [#0 defaults]                        values, one per run
          resistors        [{r 47k, m 1}, {10k, 1}, {4.7k, 1}, {2k, m 2}]
          bjt models       [#0 {NPN, is 1e-16, bf 200, br 1, nf 1, nr 1}]
          bjts             [{area 1, m 1}]
          vsources         [{dc 12 V, ac 0}]
Names     nodes ["0" "vcc" "base" "out" "emit"]; R1 R2 RC RE; Q1; VCC; spans
Analyses  [Op]
          │ spicy_simulate
          ▼
Plan      structure key: no internal nodes (this BJT has no parasitic resistors)
          unknowns: 4 node voltages + 1 branch current (VCC); stamp slots; pattern; KLU symbolic
Derived   resistors g = m/r = [21.3 µS, 100 µS, 213 µS, 1 mS]; bjt IS·area = 1e-16, m = 1
Workspace Newton → V(base) 2.053  V(emit) 1.273  V(out) 6.047 V  I(VCC) −1.478 mA
          │ named through Names
          ▼
          raw file / TUI / engine measures
```

**An engine run** (run 17, thread 3): copy the nominal `Params`; the binding writes `bjt models[0].bf = 287`; compute a new `Derived`; reuse the same plan; solve in thread 3's workspace. No rebuild, no strings.

**The language path** produces the same shape: `ce_amp.spl`'s `q1` becomes `bjts[0]` with its own `BjtModel`, named `amp.q1`.

**Why this is fast:**
- The Newton loop reads only precomputed matrix slots and `Derived` rows: contiguous per-kind arrays, static dispatch, no strings, no hash lookups.
- Nothing structural is rebuilt per run; temperature and area math runs once per run, not per iteration.
- Threads share the read-only structure and never lock.
- Large BSIM models are stored once, not per instance.

---

## 6. The types (sketch)

```rust
// Structure: built once by lowering, shared behind Arc.
pub struct NodeId(u32);                 // 0 is ground
pub struct Circuit {
    pub node_count: u32,
    pub resistors: Vec<Resistor>,       // index = resistor id
    pub capacitors: Vec<Capacitor>,
    pub inductors: Vec<Inductor>,
    pub diodes: Vec<Diode>,
    pub bjts: Vec<Bjt>,
    pub vsources: Vec<Vsource>,
    pub isources: Vec<Isource>,
}
pub struct Resistor { pub p: NodeId, pub n: NodeId, pub model: ResistorModelId }
pub struct Bjt { pub c: NodeId, pub b: NodeId, pub e: NodeId, pub model: BjtModelId }

// Values: one per run.
pub struct Params {
    pub resistor_models: Vec<ResistorModel>, pub resistors: Vec<ResistorInst>,
    pub bjt_models: Vec<BjtModel>,           pub bjts: Vec<BjtInst>,
    // … one model table and one instance table per kind
}
pub struct ResistorInst { pub r: f64, pub r_ac: Option<f64>, pub m: f64 }
pub struct BjtModel { pub polarity: Polarity, pub is: f64, pub bf: f64, pub br: f64, pub nf: f64, pub nr: f64 }
pub struct BjtInst { pub area: f64, pub m: f64 }

// Names and origins: never read by the simulator.
pub struct CircuitNames { pub nodes: Vec<String>, pub resistors: Vec<Origin>, /* … */ }

// Requests.
pub enum Analysis {
    Op,
    DcSweep { source: SourceId, start: f64, stop: f64, step: f64 },
    Ac { sweep: AcSweep, fstart: f64, fstop: f64 },
    Tran { step: f64, stop: f64, uic: bool },
}
```

---

## 7. What step 3 builds, and what waits

**Step 3:**
- The `spicy_circuit` crate: ids, `Circuit`, `Params` (model and instance tables for today's kinds), `CircuitNames`, `Analysis`.
- `spicy_parser::lower(&Deck)`: SPICE precedence and defaults, merged model cards, `scale` folded, names resolved (the DC sweep source) with span errors. The parser stops allocating branch rows.
- The simulator builds its devices from `Circuit` + `Params` (its derive step applies `m`, `area`, `1/r`) and drops its parser dependency. It assigns branch rows itself. Matrix positions stay inside its devices for now (step 2 is deferred).
- Snapshots stay identical, except the reviewed reordering of branch currents (§4.4).

**Later, each with its feature:** the shared plan with a structure key and side tables (steps 2 and 4); `Derived` levels and temperature conditions (step 5); field tables and the engine binding (M2b, M3); controlled sources and K; the OSDI kind; parallel evaluation and SIMD.

---

## 8. Decisions (agreed 2026-09-26)

| # | Decision | Recommendation | Change |
|---|---|---|---|
| 1 | Separate storage for structure (`Circuit`), values (`Params`) and names (`CircuitNames`) | Yes | — |
| 2 | Models | Shared per-kind model tables; a `ModelId` per instance; identical cards merged; overrides get their own model; per-run mismatch on instances | **Reverses** "copy `.model` values into each instance" |
| 3 | Internal nodes and branch rows | The simulator allocates them, with a structure key | Moves branch rows out of the parser |
| 4 | Order of unknowns | The simulator's choice; results printed in circuit order; one reviewed snapshot reorder | **Revises** `pipeline.md` decision 6; drops the `order` list |
| 5 | `m`, `area`, `scale` | `m` and `area` stay parameters, applied by the simulator; SPICE lowering folds `scale` | **Revises** `pipeline.md` decision 7 |
| 6 | `Params` contents | Resolved `Copy` structs; `Option` only for dependent defaults; field tables; OSDI as (id, value) pairs | — |
| 7 | Derived values | Three levels, once per run (with step 5) | — |
| 8 | Cross-device references | Typed ids, resolved at lowering | — |
| 9 | Names | A side table from lowering | — |
| 10 | State, threads, SIMD | Offsets in the plan; per-thread workspaces; SIMD kept possible | — |
| 11 | Device kinds | A closed set plus one compiled-model (OSDI) kind | — |
| 12 | Circuit shape | Flat, with analyses in `spicy_circuit` | — |
