# `spicy_circuit` for the Engine and the Exporter: Conditions, Options, Names

> 2026-09-30 · Checks the proposed additions to `crates/spicy_circuit` against what the bounds engine (M3) and the SPICE exporter (M1f) need, before they're built.
> Reads with: `circuit.md`, `pipeline.md` §3 and §6, `engine_plan.md` §1.3–1.4, §4, §5; `engine_types.md` (T1–T11); `research/next_synthesis.md` (B2, E9, T1–T6); `research/contract_v2_review_engine.md`; `research/contract_syntax_v5.md`.
> Code read: `crates/spicy_circuit/src/{lib,params,analysis}.rs`, `crates/spicy_netlist/src/reader/{lower,parser_utils,expr,netlist_models}.rs`, `crates/spicy_netlist/src/reader/devices/bjt.rs`, `crates/spicy_simulate/src/lib.rs`, `crates/spicy_model/src/flat.rs`, `circuits/*`.
> References read at the source: ngspice, Xyce, Gnucap, VACASK (in `/tmp/refs`), Cranelift, the rustc dev guide. Citations are file:line.
> **Numbers marked [E]** come from a small Python emulation of IEEE-754 double arithmetic written for this note (not kept). No cargo was run.

---

## Summary

**The verdict.** The proposed shape is right in its main lines. `Conditions` next to `Params`, the four new `BjtModel` fields, a `SolverOptions` type and model names in `CircuitNames` are all needed. One placement should change (solver options belong to the request, not the circuit). Two things are missing: a field address now, and a BJT temperature offset later. And three problems outside the crate would break M1f's round trip.

**The recommended shape, in one list:**

1. **`Conditions { temp, tnom }` in `Lowered`**, one copy per run like `Params`. `temp` is what the temperature knob moves. `tnom` is fixed for a design; no knob touches it. Keep it here anyway, because every reference keeps a circuit-wide TNOM next to the temperature, and every temperature-dependent model defaults to it (§4).
2. **`BjtModel` gains `xtb`, `xti`, `eg` and `tnom: Option<f64>`,** exactly as proposed (`None` = `Conditions::tnom`).
3. **`SolverOptions { reltol, vntol, abstol }` lives in `spicy_circuit`, but as a *request*, not as part of the circuit.** `Lowered.options` holds only what the source file asked for, the way `Lowered.analyses` already does. The engine never reads it. The exporter and our simulator always take the options as an **argument**, so the engine can pass Engine or Tight, and later one set per analysis kind (§3).
4. **Model names in `CircuitNames`,** one list per kind, indexed like `Params`' model tables.
5. **Missing: a field address, `ParamRef`.** The engine deck should be printed from `Lowered` plus a knob binding, not rebuilt from `FlatDesign` as `engine_plan.md` §5.1 sketches. Then part kinds are expanded in one place only (M1e), and the deck's "`{k7}` here" needs to know which field is which knob. Five variants cover the MVP (§5).
6. **Missing later: `BjtParams.temperature`.** Temperature zones become per-device offsets, and the BJT is the one kind that can't carry one today (§2.3).

**Three things would break M1f's round trip as the code stands.** None of them is in `spicy_circuit` itself:
- **The SPICE lowering merges model cards by value.** Two transistors with their own cards (`QM_q1`, `QM_q2`) and equal nominal values come back as one shared model. The round trip fails on the two-stage amplifier, and a β knob would move both transistors. Fix: one model entry per `.model` card, keyed by the card's name, as ngspice does (§6.2).
- **`spicy_netlist` doesn't read numbers exactly.** It parses the digits, then multiplies by `10^exponent`. 28% of random doubles, printed in shortest exponent form, parse back one bit off **[E]**. For example, RC = 4692.07 Ω, a plausible σ-search value, prints as `4.69207e3` and comes back as 4692.070000000001. Every CE amp value at nominal and at the corners survives, so the MVP round trip passes by luck. Fix: parse the whole literal in one decimal parse, as `spicy_lang`'s lexer already does (§6.3).
- **Node numbers change on export.** Lowering numbers nodes by first appearance, and the exporter writes devices grouped by kind. So "the same `Circuit`" must mean the same *after matching nodes by name* (§6.1).

**The MVP needs** items 1–5 in `spicy_circuit`, the parser changes above, and a two-line mapping from `SolverOptions` to the simulator's `SimulationConfig` (§8). Zones, origins, vendor subcircuits, new device kinds, transient profiles and a content hash for the cache key come later.

---

## 1. What exists today

| Piece | Today | What matters here |
|---|---|---|
| `Lowered` | `circuit`, `params`, `names`, `analyses` (`lib.rs:75-80`) | No conditions, no options |
| `Params` | Per kind, a model table and an instance table (`params.rs:13-26`) | R, C, L and D carry `DeviceTemperature` (`Offset` = SPICE `dtemp`, `Fixed` = `temp`). **`BjtParams` doesn't** (`params.rs:226-238`) |
| `BjtModel` | polarity, `is`, `bf`, `br`, `nf`, `nr` (`params.rs:197-209`) | No temperature fields |
| `CircuitNames` | title, nodes, one name per device (`lib.rs:145-156`) | **No model names** |
| SPICE lowering | Merges identical model cards **by value** (`lower.rs:171-199`, `bjt_model_key`) | The parser copies each card into every instance and loses its name (`devices/bjt.rs:5-17` has no model name) |
| Number parsing | Digits parsed as `f64`, then `value *= 10.0.powf(exponent)`, then `*= suffix.scale()` (`parser_utils.rs:175-185`, `expr.rs:37-46`) | Two roundings (§6.3) |
| Our simulator | `simulate_op(circuit, params, &SimulationConfig)`; `NewtonConfig { abs_tol: 1e-6, rel_tol: 1e-3, max_iters: 50 }` (`spicy_simulate/src/lib.rs:33-47`) | Solver settings are **already a per-call argument**, not part of the circuit |

---

## 2. Where each knob lands

### 2.1 The CE amp's eight knobs, traced

After M1e, `ce_amp.spl`'s devices sit in `FlatDesign` order: each placement's parts in name order (`flat.rs`), so `c_in, q1, r1, r2, rc, re`. The default bench adds `V_vcc` and `V_input`.

| Knob | Kind | Lands in | `ParamRef` (§5.2) | Engine deck | Read-back |
|---|---|---|---|---|---|
| `temp` (`ambient` in v5) | range | `Conditions.temp` | `Temp` | `.temp {k0}` (°C) | `@q_q1[temp]` |
| `vcc.v` | range | `params.vsources[0].waveform = Dc(v)` | `VsourceDc(#0)` | `V_vcc vcc 0 DC {k1}` | `@v_vcc[dc]` |
| `r1.value` … `re.value` | statistical | `params.resistors[0..=3].r` | `ResistorR(#0…#3)` | `R_r1 vcc base {k2}` … | `@r_r1[resistance]` … |
| `c_in.value` | statistical | `params.capacitors[0].c` | `CapacitorC(#0)` | `C_c_in base input {k6}` | `@c_c_in[capacitance]` |
| `q1.beta` | statistical | `params.bjt_models[0].bf` | `BjtModelBf(#0)` | `.model QM_q1 NPN(… BF={k7} …)` | `@qm_q1[bf]` |

**One run, corner 102** (ε = −1, +1, +1, −1, −1, +1, +1, −1; plan §1.5). The engine copies the nominal `Params` and `Conditions` and writes eight numbers:

```
conditions.temp          = 263.15     (−10 °C)
params.vsources[0]       = Dc(12.6)
params.resistors[0..=3]  .r = 47470, 9900, 4653, 1010
params.capacitors[0].c   = 1.2e-6
params.bjt_models[0].bf  = 100
```

The `Circuit` is untouched. On ngspice the same eight writes are eight `alterparam`s; on our simulator (M4) they're eight `ParamRef::set` calls. Either way the run gives VC = 6.5595 V, the bias FAIL of plan §2.4.

### 2.2 β: each placement needs its own model

β is a model parameter in SPICE (`BF` on the `.model` card), and ngspice has no instance-level β. So a per-placement β knob needs a per-placement model entry. `circuit.md` §4.2 already says so ("`q1: Npn { beta: 100..=300 }` … each part gets its own model"). The rule there, "per-run mismatch goes in instance fields, never into new models", means never *create* models per run. Models made once at lowering, one per placement, are fine.

**Why sharing would be wrong.** Take the two-stage CE amp (next_synthesis §9, ladder #2), with `q1.beta` and `q2.beta` each 100..=300. Independent, the β corners are four: (100, 100), (100, 300), (300, 100), (300, 300). If both transistors pointed at one model, the engine could only reach (100, 100) and (300, 300). The box would be half the real one, and a spec whose worst case is a mismatch (one weak and one strong transistor, as in a differential pair) would PASS falsely. Parts that really share β, from one wafer, are a *lot* knob (M6), written on purpose.

**The rule for M1e.** Every placed `Npn`/`Pnp` gets its own `BjtModel` entry, always. That's simpler than "only when a model field is a knob", and a BJT model is ten numbers. (Vendor models with hundreds of parameters, which placements should share, arrive with parts in M5.)

**What this needs from `spicy_circuit`:** nothing new. Model tables with a `BjtModelId` per instance already express it. What it needs from the SPICE lowering is that it stop merging those entries back together (§6.2).

### 2.3 Temperature zones: per-device offsets

A "zone" is a group of parts that runs hotter than the room: say every part near a regulator at ambient + 0…20 K. It lowers to one knob, `zone.dt`, with several targets: `DeviceTemperature::Offset(dt)` on each device in the zone (next_synthesis M3, "a knob with several targets").

- R, C, L and D already carry `temperature: DeviceTemperature` (`params.rs:78, 100, 119, 168`).
- **`BjtParams` doesn't**, and the transistor is the part whose temperature matters most. ngspice has instance `temp` and `dtemp` on the BJT (`bjt.c:76-77`; the device runs at `CKTtemp + dtemp`, `bjttemp.c:76-80`), and `spicy_netlist` doesn't accept them yet (`devices/bjt.rs:5-17`).
- **Add `temperature: DeviceTemperature` to `BjtParams` when zones or the parser's BJT `temp`/`dtemp` arrive.** Not MVP: v5 has no zone syntax, and the rule in `params.rs` is to carry what a front-end accepts.
- Self-heating (a junction heated by its own dissipation) is a thermal network, not an offset. It's out of scope here (next_power's load switch).

One exporter consequence: the ambient knob's read-back uses a device whose temperature follows the circuit's. On the CE amp that's `@q_q1[temp]`. With a zone on every device there's none, and the exporter must read `temp − dtemp` instead.

### 2.4 Supplies, loads and the rest of a v5 setup

Setups (`contract_syntax_v5.md` §1.3) lower into circuits in M1e. Here's where each construct lands:

| v5 construct | Lowers to | Change in `spicy_circuit` |
|---|---|---|
| `temp: ambient` | `Conditions.temp`, bound to the global knob | `Conditions` (MVP) |
| `temp: 25°C` | `Conditions.temp` fixed | — |
| `Supply { v: 12V ± 5% }` | V source, `Waveform::Dc`, bound | — |
| `Supply { z: ..=0.5Ω }` | series resistor, bound | — |
| `Signal { v: 0V }`, unwritten AC | V source `DC 0 AC 1` | — |
| `Load { c: ..=1nF }` | capacitor, bound | — |
| `Load { i: … }` | I source, bound | — |
| `Load { r: 10kΩ.. }` | a conductance g ∈ 0…100 µS (contract_v2_review P13) | **VCCS kind, later** |
| `Step { … }`, `edge: 1us..=10us` | a PWL breakpoint, bound | **`Waveform::Pwl`, later** |
| `window: ..=300us` | `Transient.stop`, in the request | — |
| `mode` that only changes ranges | same `Circuit`, other knob ranges | — |
| `#[fault] Open { port, at, lasts }` | a timed switch | **switch kind, later** |
| `usb.polarity: reversed`, a plug-in module | **another `Circuit`** | — |
| `rated …` | monitors on results, not the circuit | — |

### 2.5 The answer: one `Circuit`, per-run `Params` + `Conditions`

**Yes, for every run of one setup:** the MVP's four knob kinds all land in `Params` or `Conditions`. Four things sit elsewhere by design:
- **Structure that differs** (a fault that flips a source, a module present or absent, a knob that crosses RS = 0) is another `Circuit`: one per setup, or per structure key (`circuit.md` §4.3; contract_v2_review §5.2 has one deck per setup).
- **What to run** (analyses) and **how precisely** (solver options) come with each request (§3).
- **A knob that targets an expression** (a tempco link: R = R₀·(1 + tc·(T − 25 °C))) is computed by the binding, which writes the result through `ParamRef`.
- **Zones** need `BjtParams.temperature` (§2.3).

---

## 3. Solver options: in the circuit, or in the request?

**What they are, in plain words.** A simulator solves the circuit by guessing and correcting (Newton's method) until the corrections are small. `reltol`, `vntol` and `abstol` say how small: relative, in volts, in amperes. Tighter gives a more exact answer and costs more steps. They change *how precisely* the same circuit is solved, not *what* the circuit is.

**What the engine does with them.** It solves one circuit at two precisions: Engine (reltol 1e-6, vntol 1e-9, abstol 1e-15, plan §1.3) for every run, and Tight (reltol 1e-9, plan §2.7) to measure the numerical band. Later it needs one set per analysis kind: those values abort the rectifier's transient at 46.6 ms, and a transient's tight run should shrink the time step, not reltol (next_synthesis B2, E9).

### 3.1 What the references do

| Source | Temperature | Tolerances |
|---|---|---|
| **ngspice** | `TSKtemp`, `TSKnomTemp` in the task struct (`tskdefs.h:19-20`) | `TSKreltol`, `TSKabstol`, `TSKvoltTol`, also `TSKgmin`, `TSKrshunt` in the same struct (`tskdefs.h:43-57`). All copied into the circuit at the start of every job (`cktdojob.c:52-75`). In practice they come from the deck, and `reset` re-applies the deck's `.options` (the plan's trap 2) |
| **Xyce** | `.OPTIONS DEVICE TEMP TNOM` (`N_DEV_DeviceOptions.C:99-100`) | **One group per analysis kind:** `NONLIN` for DC, RELTOL 1e-3 / ABSTOL 1e-12 (`N_NLS_NLParams.C:440-446`); `NONLIN-TRAN` for the transient's Newton, RELTOL 1e-2 / ABSTOL 1e-6 (`N_NLS_Manager.C:1074-1080`); `TIMEINT` for time-step error, RELTOL 1e-2 (`N_TIA_TIAParams.C:420-427`) |
| **Gnucap** | **Per analysis command**: `.tran … temperature=`, `.ac …`, `.dc …` (`s_tr_set.cc:180-197`, `s_ac.cc:158-163`, `s_dc.cc:425-428`), stored in the per-run sim data (`u_sim_data.h:43`) | Global `OPT::reltol`, `abstol`, `vntol` (`u_opt.h:93-95`) |
| **VACASK** | Options `temp`, `tnom` | Options too, but **classified by what they invalidate**: `temp`/`tnom` change parameters, tolerances change only tolerances (`options.cpp:357-405`) |
| **Cranelift** | — | Settings (`Flags`) live outside the `Function` IR, and the incremental cache key adds them to the function's content (`incremental_cache.rs:135-147`) |
| **rustc** | — | Session options sit outside HIR/MIR; each one is marked `[TRACKED]` if it can change results (then it feeds the incremental hash) or `[UNTRACKED]` (rustc dev guide, `implementing-new-features.md:282-283`) |

The pattern: **no reference makes tolerances part of the circuit.** They sit with the job (ngspice), the analysis kind (Xyce) or the session (Gnucap, compilers), and the cache key adds them from outside (Cranelift, rustc).

### 3.2 Three placements

**Option 1: in `Lowered` only** (the proposal read literally).

```rust
let lowered = spicy_netlist::reader::lower(&deck)?;              // lowered.options from `.options`
simulate_op(&lowered.circuit, &lowered.params, &SimulationConfig::from(&lowered.options))?;
// engine, band stage: clone the whole Lowered and overwrite .options to export the Tight deck
let mut tight = lowered.clone(); tight.options.reltol = 1e-9;
```
- *For:* one place; the round trip covers it.
- *Against:* the engine edits a front-end's output to change precision. One `Lowered` holds one set, so "transient differs from DC" has no place. A cache key that hashes `Lowered` would mix circuit identity with run precision.

**Option 2: only in the request.** The parser has nowhere to put `.options reltol=1e-6`.
- *For:* the circuit stays pure.
- *Against:* a SPICE file's `.options` is dropped silently, which breaks the rule of `params.rs` ("nothing the netlist says is dropped silently"), and the round trip loses it.

**Option 3 (recommended): the type in `spicy_circuit`, two roles.**

```rust
// spicy_circuit: the type lives next to Analysis, so the exporter needn't import the simulator
pub struct Lowered { …, pub analyses: Vec<Analysis>, pub options: SolverOptions }   // what the SOURCE asked for

// every consumer takes options as an argument and never reads lowered.options on its own
pub fn numeric(l: &Lowered, analyses: &[Analysis], options: &SolverOptions) -> Exported;          // M1f
pub fn engine_deck(l: &Lowered, b: &Binding, options: &SolverOptions) -> Result<EngineDeck, …>;   // M1f
impl SimulationConfig { pub fn with_options(self, o: &SolverOptions) -> Self }                   // spicy_simulate

// the CLI passes what the file said
simulate_op(&l.circuit, &l.params, &SimulationConfig::default().with_options(&l.options))?;
// the engine passes its own, per deck (ngspice) or per call (our simulator)
let decks = [Tolerance::Engine, Tolerance::Tight].map(|t| engine_deck(&l, &b, &policy.options(t, Kind::Dc)));
```

- **`Lowered.options` works exactly like `Lowered.analyses` today:** it records what the source asked for. The CLI honors it, the round trip carries it, and the engine ignores it and sends its own. One pattern, two fields.
- **Fields are plain `f64`, with ngspice's defaults** (1e-3, 1e-6, 1e-12; `cktntask.c:99-102`), like every other default in `params.rs`. These happen to equal today's `NewtonConfig` (rel 1e-3, abs 1e-6), so mapping `reltol → rel_tol` and `vntol → abs_tol` keeps every simulator snapshot unchanged. `abstol` is "Not simulated yet": our Newton has one absolute tolerance for every unknown until M2e splits it.
- **The exporter always writes all three,** as it always writes `tnom`: the deck then means the same thing whatever the reader's defaults.

### 3.3 How the options reach each consumer

```
SPICE file ─parse─► .options reltol=1e-6 tnom=25 ─lower─► Lowered.options {1e-6, 1e-6, 1e-12}   Conditions.tnom 298.15
                                                               │ CLI
                                                               ▼
                                   SimulationConfig::default().with_options(&l.options) ─► simulate_*

engine Policy ─(Tolerance, analysis kind)─► SolverOptions ─┬─► engine_deck(&l, &b, &o) ─► ".options tnom=25 reltol=1e-6 vntol=1e-9 abstol=1e-15"
                                                            │     one deck per distinct set (Engine, Tight; later +Tran)
                                                            └─► our simulator (M4): with_options(&o) per simulate_* call
```

**Per analysis kind, later.** The choice is the engine's: `Policy` maps (Tolerance, analysis kind) to a `SolverOptions`, plus analysis-level step settings. `spicy_circuit` needs nothing more than the one-set type, for two reasons:
- **On ngspice,** `.options` apply to everything run on a loaded deck, and must live in the deck (trap 2). So a transient profile is simply another deck. A request that mixes op/AC (DC profile) with a transient (transient profile) is split by the backend into two jobs on two decks. M3 has no transient: two decks, as planned.
- **On our simulator,** each `simulate_*` call already takes its own config.
- **A transient's "tight" run shrinks the time step** (B2, E9). The step limit is SPICE's `.tran … tmax`, an *analysis* parameter, so it goes into `Transient` (a later `max_step: Option<f64>`), not into `SolverOptions`.

---

## 4. TNOM: in `Conditions`, in the model, or both?

**What TNOM is.** A transistor's model card gives IS and BF *as measured at some temperature*: that's TNOM. At any other temperature the simulator adjusts them (XTB bends β, XTI and EG bend IS). TNOM isn't the room temperature and isn't the knob's midpoint. It's a fact about the card (plan §2.1). Writing it wrong moves the CE amp's nominal VC from 5.5032 to 5.5410 V (plan §1.3).

**What the references do.** Every one keeps a circuit-wide TNOM that models fall back to:
- **ngspice:** `CKTnomTemp`, defaulting every temperature-dependent model's TNOM: R (`ressetup.c:29`), C (`capsetup.c:59`), L (`indsetup.c:36`), D (`diosetup.c:241`), BJT (`bjttemp.c:42`). The default is 300.15 K (`cktinit.c:70`).
- **Xyce:** `DeviceOptions::tnom` (`N_DEV_DeviceOptions.C:99, 209-210`), with per-model `TNOM`.
- **Gnucap:** `OPT::tnom_c` (`u_opt.h:101`), with per-model `_tnom_c` (`e_model.h:75`).
- **VACASK:** the option `tnom`, handed to compiled OSDI models as `$tnom` (`options.cpp:377-381`).

That last one settles it for us. A compiled model reads TNOM from the simulator, so a circuit-wide value must exist somewhere even after lowering. It can't be "resolved into every model".

**Recommendation: the proposal as it stands.** `Conditions.tnom` holds the circuit-wide value, and `BjtModel.tnom: Option<f64>` holds a card's own (`None` = the circuit's). That fits `params.rs`' rule for `Option` ("a default that depends on another value"). The doc comment should say that `tnom` is fixed for a design, and that `Conditions` holds it because devices read it, not because it varies. R, C, L and D models gain their own `tnom` later, when the parser accepts TNOM on those cards.

**Reject the alternative:** resolving TNOM into every model at lowering (`BjtModel.tnom: f64`, no circuit value).
- It removes one `Option`.
- But every temperature-dependent model then needs the field, and resistors without a card would need invented `.model` cards just to carry it.
- It departs from all four references, and it can't serve OSDI.

**One inconsistency to fix, D-A's card.** Plan §5.4 has M1e lowering the default `Npn` to `tnom: Some(298.15)`, but plan §1.4's deck writes `TNOM` only on the `.options` line. Both simulate the same, and both round-trip. Pick one. **Recommendation:** M1e sets `Some(298.15)` on the card *and* `Conditions.tnom = 298.15`. D-A defines a card, and a card that travels, pasted into another deck, should keep its meaning. The exporter then writes `TNOM=25` on the card and `.options tnom=25`.

**Temperature units.** `spicy_circuit` stays in kelvin; SPICE writes °C. Converting K → °C → K with shortest printing came back bit-exact for all 26,001 temperatures from −60 to 200 °C in 0.01 °C steps **[E]**. The printed °C isn't always pretty (253.15 K prints as `-19.99999999999997`), which is cosmetic.

---

## 5. The exporter: what it needs

### 5.1 Numeric mode (one knob point)

`numeric(&Lowered, &[Analysis], &SolverOptions) -> Exported { text, names: NameMap }`. For `ce_amp.spl` at nominal:

```
ce_amp.spl · CeAmp                                            ← names.title (must be one line)
.options tnom=25 reltol=1e-3 vntol=1e-6 abstol=1e-12          ← conditions.tnom + the options argument
.temp 25                                                      ← conditions.temp
V_vcc vcc 0 DC 12
V_input input 0 DC 0 AC 1
R_r1 vcc base 4.7e4                                           ← {:e}: shortest round-trip, never SPICE suffixes (M is milli)
…
C_c_in base input 1e-6
Q_q1 output base emitter QM_q1
.model QM_q1 NPN(IS=1e-14 BF=2e2 BR=1 NF=1 NR=1 XTB=1.5 XTI=3 EG=1.11 TNOM=25)
.op                                                           ← the analyses argument
.end
```

Everything here is in `spicy_circuit` once the proposal lands, except the card name `QM_q1`, which needs model names (§5.3).

### 5.2 Engine-deck mode: print it from `Lowered`, not from `FlatDesign`

Plan §5.1 sketches `engine_deck(design: &FlatDesign, knobs: &KnobTable, options)`. That makes the exporter redo M1e's work: Electrolytic → capacitor, the default bench, the D-A model. `pipeline.md` §6(c) gives the reason against it: "Exporting from `Circuit` means a part kind is expanded into devices in exactly one place (the language lowering)". The numeric mode already follows that rule.

**Recommendation.** M1e returns the nominal `Lowered` *and* a `Binding` (knob → targets; `pipeline.md` §3 already places `Binding` in `spicy_backends`). The engine deck is then the numeric export with one difference: a bound field prints `{k_i}` instead of its number.

```rust
// spicy_circuit: the address of one number a knob can set (circuit.md §4.6's field table, MVP subset)
pub enum ParamRef { Temp, ResistorR(ResistorId), CapacitorC(CapacitorId), VsourceDc(VsourceId), BjtModelBf(BjtModelId) }

// spicy_backends: M1e's second output
pub struct Binding { pub knobs: Vec<KnobTargets> }                 // indexed by KnobId
pub struct KnobTargets { pub targets: Vec<ParamRef> }              // one in the MVP; several for a zone (later)

// the exporter, while printing params.resistors[0].r:
match binding.knob_of(ParamRef::ResistorR(ResistorId::new(0))) { Some(k) => "{k2}", None => "4.7e4" }
```

What this buys:
- **The done-when becomes structural.** Plan §5.4 asks that the deck at nominal lower to the same result as the numeric export. When both print the same `Lowered`, they can only differ where a field is bound, and at nominal `.param k2=47000` evaluates to exactly that number.
- **The read-back paths come from the same table:**
  - `ResistorR(i)` → `@<element>[resistance]`
  - `CapacitorC(i)` → `@<element>[capacitance]`
  - `VsourceDc(i)` → `@<element>[dc]`
  - `BjtModelBf(m)` → `@<model card>[bf]`
  - `Temp` → `@<a device at circuit temperature>[temp]`
- **Our simulator (M4) uses the same binding:** `ParamRef::set` on a per-thread `Params` + `Conditions`. That also corrects `engine_types.md` §4, whose `Backend::Input` for M4 reads "Circuit + Params + Binding" and leaves out `Conditions`, which the temperature knob writes. It should be `Lowered` + `Binding`.
- **Cost:** one small enum with `get`/`set` in `spicy_circuit` (§9). It grows into `circuit.md` §4.6's generated field tables when more fields become knobs.

### 5.3 What's missing, item by item

| Asked about | Needed? | Where | MVP? |
|---|---|---|---|
| **Model names** | **Yes.** A `.model` card needs a name. From SPICE, the card's name (`QN`, `2N3904`); from the language, empty for a per-placement model (the exporter writes `QM_<instance>`), or a part's model name from M5 | `CircuitNames.{resistor,capacitor,inductor,diode,bjt}_models`, indexed like `Params` | yes |
| **Per-instance model ownership** | Not as data. It can be derived (count the instances per `ModelId`), and what the deck needs is *which field is which knob*: the binding | `Binding` + `ParamRef` | yes (`ParamRef`) |
| **The analysis list** | Exists (`Lowered.analyses`). The numeric mode takes analyses as an argument, so the cross-check can pass the engine's; the engine deck has none. `ac lin 1 f f` is `AcSweep { Linear(1), f, f }`, fine as a type (our simulator panics on it, `pipeline.md` §11) | — | — |
| **Node names as paths** | Exist as strings (`CircuitNames.nodes`); M1e fills them with net paths (`output`, `amp.base`). The probe map is path → node id → exported name. Keep strings (`keep-it-simple`) | — | — |
| **A title** | Exists. M1e sets it (`ce_amp.spl · CeAmp`); the exporter must keep it to one line (ngspice reads line 1 as the title) | — | — |
| **Name hygiene** | The exporter's job, recorded in its `NameMap`: node 0 → `0`; a non-ground net named `gnd` renamed (ngspice rewrites `gnd` to ground, `pipeline.md` §11); names that collide after ngspice lowercases them renamed; a model name never equal to an instance name (plan App. B #11) | exporter | yes |
| **Bench or part?** | Nice to have: the deck's comments ("default bench: `Power<In>` port vcc") and the report want it. Comes with origins | origins, later | no |
| **Includes, dialect, start condition** | For vendor models (next_synthesis X1). They belong in `EngineDeck`, plus an opaque subcircuit kind in the `Circuit` | later | no |

---

## 6. The round trip

### 6.1 What must survive, and what "the same" means

Roadmap M1f asks for SPICE → `Circuit` → SPICE → `Circuit` on every `circuits/*.spicy`, and `ce_amp.spl` → export → lower giving "the same `Circuit` + `Params` + `Conditions`". Here's what that has to mean:

| Part | Must survive | How it's compared |
|---|---|---|
| Nodes | count; ground is node 0 | **Matched by name.** Lowering numbers nodes by first appearance, the exporter writes devices grouped by kind, and M1e numbers them in net-name order, so ids can differ while the circuit is the same (renumbering changes nothing physical: KLU reorders anyway, `circuit.md` §4.4) |
| Devices | count per kind; order within a kind; terminals; model id | exactly, after the node match. The exporter writes each kind in index order, and lowering reads in file order |
| Model tables | count; order; every value | exactly. Lowering numbers models by first use, which the exporter's instance order preserves, **provided entries are keyed by card name** (§6.2) |
| `Params` | every field, bit-exact | `==`, **provided numbers print and parse exactly** (§6.3) |
| `Conditions`, `Lowered.options`, `analyses` | every field | `==` (a DC sweep's source goes out by name and comes back to the same index) |
| Names | title and node names verbatim; device and model names through the exporter's name map | not in `==`; the map must be a bijection |

**A second, simpler check:** after one round, the export is a fixed point. `export(lower(export(lower(x)))) == export(lower(x))`, byte for byte. That holds because the name map is idempotent (`R_r1` stays `R_r1`, and `R1` stays `R1`) and the order is canonical after one export.

### 6.2 Hazard 1: merging model cards by value

The SPICE lowering stores each distinct card once, *by value* (`lower.rs:171-199`), because the parser copies a card into every instance and loses its name. On the two-stage amp:

```
M1e:     bjt_models = [QM for q1 {bf 200 …}, QM for q2 {bf 200 …}]   bjts[1].model = #1
export:  .model QM_q1 NPN(BF=2e2 …)   .model QM_q2 NPN(BF=2e2 …)
lower:   bjt_models = [{bf 200 …}]                                     bjts[1].model = #0   ← not the same
```

And the engine deck at nominal would give the same wrong answer.

**Recommendation: one entry per card, keyed by the card's name,** as ngspice does (one `GENmodel` per `.model`, `gendefs.h:18-48`). Two instances naming one card share it; two cards stay two even when their numbers match. R, C and L models built from instance values (`tc1=` on the line) have no card name, and keep merging by value as today.

- *Cost:* the parser's `BjtSpec` and `DiodeSpec` carry the card name, which model names in `CircuitNames` need anyway; the `ModelTable` key changes.
- *Change of decision:* this revises the wording of `circuit.md` §4.2 and decision 2 ("identical model cards are merged") to "one entry per model card". Merging by value was a workaround for the lost name, not a goal.
- *Results don't move:* the numbers are the same either way; only the table count differs.

### 6.3 Hazard 2: numbers that don't come back exactly

`spicy_netlist` reads `4.69207e3` as `4.69207 × 10.0.powf(3)`, and `4.7k` as `4.7 × 1000` (`parser_utils.rs:175-185`, `expr.rs:37-46`). That's two roundings where one decimal parse gives the correctly rounded value.

| Value | Printed (`{:e}`) | Parsed back **[E]** |
|---|---|---|
| 47000, 1e-14, 12.6, 200 (nominal) | `4.7e4`, `1e-14`, `1.26e1`, `2e2` | exact |
| 47470, 9900, 4653, 1010, 100 (corner 102) | `4.747e4` … | exact |
| 111.5, 47101.23, 9980.4 (σ-search style) | `1.115e2` … | exact |
| **4692.07** (an RC value at a σ point) | `4.69207e3` | **4692.070000000001** |
| 200,000 random doubles, 1e-15 … 1e6 | shortest exponent form | **55,828 differ (28%)** |

- **The CE amp's MVP round trip passes by luck:** its nominal and corner values are round.
- **A numeric export at a σ point** (plan §5.4 exports "a knob point") fails about one field in four.
- **Suffixes have the same flaw.** `spicy_lang` measured it for its own decision L6 (`lexer.md`): multiplying by an inexact power of ten is off in the last bit for 26 of 112 common value × prefix pairs (`100.0 * 1e-9` is `1.0000000000000001e-07`).
- **Fix, test-first:** parse the literal once. Fold the exponent and the SPICE suffix (`k` = e3, `meg` = e6, …) into one decimal exponent and parse correctly rounded, as `spicy_lang`'s lexer does (L6: Clinger's fast path, else `str::parse::<f64>`). The exporter prints `{:e}` (Rust's shortest round-trip form) and never uses suffixes.

### 6.4 Smaller round-trip traps (existing types)

- **Angles.** Lowering turns degrees into radians (`expr.rs:48-61`). 386 of 7,201 phases from −360° to 360° in 0.1° steps don't survive radians → degrees → radians **[E]**. Radians are denser than degrees near 1, so some radian values have no exact degree string. The repo's circuits use 0° and 180°, which survive. Either compare phases to one ulp, or keep the degree value the source wrote. Not MVP: the CE amp's phases are 0.
- **`Option` fields in positional lists.** In `PULSE(V1 V2 TD TR TF …)`, a `None` rise time can only be written by leaving it out, and that's impossible when a later field is given. Writing `0` comes back as `Some(0.0)`. The lowering should normalize where ngspice treats 0 as "use the default", or the test compares after normalizing. The repo's circuits have no `PULSE`.

---

## 7. Cache keys

Two keys, with different jobs (`engine_types.md` §6 and §9.3; contract_v2_review §6.2):
- **Row key:** "is this the same simulation?" Rows can be reused across revisions (T6), so it must change only when something a run reads changes.
- **`rev` / `BlockKey`:** "is this the same check?" It marks a stored report stale.

What of `Lowered` goes into each:

| Part of `Lowered` | Row key (with the cone's knob values) | `rev` | Why |
|---|---|---|---|
| `circuit` (all) | yes | yes | the structure every run reads |
| `params`, fields no knob drives | yes, bit-exact (with −0.0 normalized) | yes | fixed numbers the runs read, "Not simulated yet" ones included: ngspice simulates them |
| `params`, fields a knob drives | **masked** | through `KnobSpace` | the point supplies them. Hashing their nominal would make a range edit invalidate every row, defeating T6 |
| `conditions.tnom` | yes | yes | physics |
| `conditions.temp` | masked if bound, else yes | yes | as `params` |
| the `Binding` (by knob path) | yes | yes | the same point with another target is another run |
| `names` (title, nodes, devices, models) | **no** | **no** | see below |
| origins (later) | no | no | spans move on every edit above them (rustc keeps spans out of cached results; `circuit.md` §4.9) |
| `analyses` (the source's) | no | no | the engine sends its own; each request's `AnalysisSet` is part of the row's identity |
| `options` (the source's) | no | no | the engine passes its own; the request's `Tolerance` is already in the row key (`engine_types.md` §6), and its numbers are in `Policy` → `rev` |
| backend identity + exporter version | yes | yes | another build or another exporter can change a run |

**Why names stay out.**
1. **They don't change the physics** once the exporter neutralizes the three ways they could on ngspice: `gnd`, case collisions, model = instance.
2. **Child results must be reusable by definition.** A block placed twice has devices named `left.r1` and `right.r1`. With names in the key, the two placements could never share a table (contract_v2_review §6.2: "not in the key: the placement path"). This is Cranelift's split exactly: names and source locations sit in `FunctionParameters`, outside the hashed `FunctionStencil`.

A rename that reorders nets (`FlatDesign` orders nets by name) still changes the `Circuit`'s bytes, and so the key. That's a false miss, which only costs a re-run, never a wrong reuse, so it's accepted rather than canonicalized away.

**MVP vs later.**
- **M3:** `rev` hashes the deck text and knob map, as `engine_types.md` §9.3 already says. The names inside only cause false misses, which is harmless at 0.4 s per check.
- **With the cross-revision cache and child tables:** hash `Lowered` as in the table. Put the encoder in `spicy_circuit`, and have it destructure every struct exhaustively (`let Params { resistor_models, resistors, … } = self;`), so a new field is a compile error until it's hashed. This also answers `engine_types.md` open question 3: hash what the deck is printed *from*, not its text.

---

## 8. The MVP, and what waits

**MVP: for `ce_amp.spl`'s export (M1e, M1f) and M3f.** Each one test-first (§10):

| # | Crate | Change | Needed by |
|---|---|---|---|
| 1 | `spicy_circuit` | `BjtModel` + `xtb`, `xti`, `eg`, `tnom: Option<f64>` | D-A (M1e), the export |
| 2 | `spicy_circuit` | `Conditions { temp, tnom }` in `Lowered` | the temperature knob, `.temp`, `.options tnom` |
| 3 | `spicy_circuit` | `SolverOptions { reltol, vntol, abstol }`; `Lowered.options` = the source's request | `.options`; the exporter's argument |
| 4 | `spicy_circuit` | model names in `CircuitNames` | `.model` card names |
| 5 | `spicy_circuit` | `ParamRef` (5 variants) with `get`/`set` | the engine deck from `Lowered` + `Binding` (M1f engine mode, M3f); M4 later |
| 6 | `spicy_netlist` | XTB/XTI/EG/TNOM on `.model`; `.temp`; `.options` (plan §5.3); keep card names; one model entry per card; exact number parsing | the round trip |
| 7 | `spicy_simulate` / CLI | `SimulationConfig::with_options`; `Conditions` carried but not simulated (M2c). Optionally warn when `.temp` ≠ 27 °C, since the thermal voltage is fixed | honoring `.options`; snapshots unchanged |

Strictly, the CE amp alone would pass without the card-name keying and the exact parse: one transistor, round values. They're in the MVP because they're the round trip's done-when for `circuits/*.spicy` and for any numeric export at a σ point, and the card name is needed for item 4 anyway.

**Later, each with its first reader:**
- `BjtParams.temperature` and BJT `temp`/`dtemp` in the parser: with zones.
- Origins in `CircuitNames` (device → `FlatDeviceId` or bench port; spans): with the editor chain (`pipeline.md` §6(d)) and the deck's bench comments.
- An opaque subcircuit kind, plus `EngineDeck` includes and dialect: with vendor models (next_synthesis X1).
- New device kinds: VCCS (loads), a timed switch (faults), `Waveform::Pwl` (steps), a diode default model, MOSFET, controlled sources with typed ids (`circuit.md` §4.8).
- Transient tier: `Transient.max_step` and `tstart`; `trtol`, `chgtol`, `gmin` in `SolverOptions`; `rshunt`; the `Policy` table by analysis kind (B2, E9).
- `tnom` on R, C, L and D models: when their tempcos are simulated, or TNOM is accepted on their cards.
- The `Lowered` content hash for the row key and `BlockKey` (§7).
- Generated field tables replacing the MVP `ParamRef` enum (`circuit.md` §4.6).
- Exact phase round trip (§6.4).

---

## 9. The recommended shape

```rust
// ─── crates/spicy_circuit/src/lib.rs ───────────────────────────────────────────────

/// Everything a front-end produces for one circuit (one setup).
#[derive(Debug, Clone, PartialEq)]
pub struct Lowered {
    pub circuit: Circuit,
    pub params: Params,
    /// The conditions at the nominal point. Per run, a copy, like `params`.
    pub conditions: Conditions,
    pub names: CircuitNames,
    /// What the source asked to run. The engine sends its own analyses with each request.
    pub analyses: Vec<Analysis>,
    /// How precisely the source asked to solve (SPICE `.options`). Consumers take options as an
    /// argument: the CLI passes these, the engine passes its own.
    pub options: SolverOptions,
}

id!(
    /// Index into [`Circuit::resistors`].
    ResistorId
);
id!(
    /// Index into [`Circuit::capacitors`].
    CapacitorId
);

/// Names for results, messages and exports, indexed like the [`Circuit`] and [`Params`].
/// The simulator never reads it, and it's never part of a cache key.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CircuitNames {
    pub title: String,
    /// Indexed by node id. Node 0 is ground, named `"0"`.
    pub nodes: Vec<String>,
    /// Indexed like [`Params::resistor_models`]: the model card's name, or empty for a model no
    /// card named (a default, one built from instance values, or a per-placement model).
    pub resistor_models: Vec<String>,
    pub resistors: Vec<String>,
    pub capacitor_models: Vec<String>,
    pub capacitors: Vec<String>,
    pub inductor_models: Vec<String>,
    pub inductors: Vec<String>,
    pub diode_models: Vec<String>,
    pub diodes: Vec<String>,
    pub bjt_models: Vec<String>,
    pub bjts: Vec<String>,
    pub vsources: Vec<String>,
    pub isources: Vec<String>,
}

// ─── crates/spicy_circuit/src/params.rs ────────────────────────────────────────────

/// The run's conditions: the numbers device physics reads that belong to no one device.
/// One copy per run, like [`Params`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conditions {
    /// The run's temperature (K): SPICE `.temp`, a setup's `temp`. A device's
    /// [`DeviceTemperature`] is relative to it. Not simulated yet: every device runs at the
    /// thermal voltage of 27 °C until temperature support.
    pub temp: f64,
    /// The temperature (K) model parameters were measured at, for models that don't give their
    /// own: SPICE `.options tnom`. Fixed for a design; no knob sets it. Not simulated yet.
    pub tnom: f64,
}

impl Default for Conditions {
    /// ngspice's defaults, 27 °C for both (`cktinit.c:69-70`).
    fn default() -> Self {
        Self { temp: 300.15, tnom: 300.15 }
    }
}

/// Ebers–Moll transistor model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BjtModel {
    pub polarity: Polarity,
    pub is: f64,
    pub bf: f64,
    pub br: f64,
    pub nf: f64,
    pub nr: f64,
    /// Forward and reverse β temperature exponent. Not simulated yet: temperature support (M2c).
    pub xtb: f64,
    /// Saturation-current temperature exponent. Not simulated yet.
    pub xti: f64,
    /// Energy gap (eV) for the saturation current's temperature dependence. Not simulated yet.
    pub eg: f64,
    /// Temperature (K) the parameters were measured at. `None` means [`Conditions::tnom`].
    /// Not simulated yet.
    pub tnom: Option<f64>,
}
// Default: today's fields, plus xtb 0.0, xti 3.0, eg 1.11, tnom None
// (ngspice bjtsetup.c:156-163; bjttemp.c:42).

// LATER, with zones and the parser's BJT temp/dtemp:
// pub struct BjtParams { …, pub temperature: DeviceTemperature }

/// The address of one number of a run that a knob can set. The engine's binding writes through
/// it, and the exporter prints a bound field as `{k}`. The MVP's subset of circuit.md §4.6's
/// field tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamRef {
    /// [`Conditions::temp`].
    Temp,
    /// [`ResistorParams::r`].
    ResistorR(ResistorId),
    /// [`CapacitorParams::c`].
    CapacitorC(CapacitorId),
    /// The value of a DC source. Lowering binds only sources whose waveform is [`Waveform::Dc`].
    VsourceDc(VsourceId),
    /// [`BjtModel::bf`].
    BjtModelBf(BjtModelId),
}

impl ParamRef {
    pub fn get(self, params: &Params, conditions: &Conditions) -> f64 { … }
    pub fn set(self, params: &mut Params, conditions: &mut Conditions, value: f64) { … }
}

// ─── crates/spicy_circuit/src/analysis.rs ──────────────────────────────────────────

/// How precisely to solve: SPICE `.options`. Not part of the circuit, because one circuit is
/// solved at several precisions (the engine's two profiles, and later one per analysis kind).
/// Every consumer takes it as an argument.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolverOptions {
    /// Relative tolerance of the Newton iteration.
    pub reltol: f64,
    /// Absolute tolerance on node voltages (V).
    pub vntol: f64,
    /// Absolute tolerance on branch currents (A). Not simulated yet: our Newton uses one absolute
    /// tolerance, `vntol`, for every unknown until M2e.
    pub abstol: f64,
}

impl Default for SolverOptions {
    /// ngspice's defaults (`cktntask.c:99-102`). They equal today's `NewtonConfig`, so no
    /// simulator result moves.
    fn default() -> Self {
        Self { reltol: 1e-3, vntol: 1e-6, abstol: 1e-12 }
    }
}
```

Beside it, outside `spicy_circuit`, for context:

```rust
// spicy_simulate
impl SimulationConfig {
    pub fn with_options(mut self, o: &SolverOptions) -> Self {
        self.newton.rel_tol = o.reltol;
        self.newton.abs_tol = o.vntol;
        self
    }
}

// spicy_backends: M1e and M1f
pub struct Binding { pub knobs: Vec<KnobTargets> }            // indexed by KnobId
pub struct KnobTargets { pub targets: Vec<ParamRef> }
pub fn lower(/* the flat design, its knobs */) -> Result<(Lowered, Binding), LowerError>;
pub fn numeric(l: &Lowered, analyses: &[Analysis], options: &SolverOptions) -> Exported;
pub fn engine_deck(l: &Lowered, b: &Binding, options: &SolverOptions) -> Result<EngineDeck, ExportError>;

// spicy_engine: Policy picks the options; spicy_circuit only has the type
pub struct Tolerances { pub dc: SolverOptions /* , pub tran: SolverOptions  (later, B2) */ }
```

**Why this shape:**
- **Split by what changes when** (`pipeline.md` §1): the circuit once per setup; `Params` + `Conditions` per run; options and analyses per request; names never in the hot path or a key.
- **Follows every reference on temperature and TNOM** (§4), and puts tolerances outside the circuit, as every reference and compiler does (§3.1).
- **One expansion of part kinds** (M1e), and one printer with two modes (§5.2).
- **No speed cost.** `Conditions` adds 16 bytes to each per-run copy; `ParamRef::set` is a match and one store; names stay off the Newton loop.

---

## 10. Tests, test-first

Each test goes in its module's `#[cfg(test)] mod tests`.

**`spicy_circuit`**
1. `Conditions::default()` is 300.15 K for both fields (ngspice `cktinit.c:69-70`).
2. `SolverOptions::default()` is (1e-3, 1e-6, 1e-12) (`cktntask.c:99-102`).
3. `BjtModel::default()` adds xtb 0, xti 3, eg 1.11, tnom `None` (`bjtsetup.c:156-163`).
4. `ParamRef`: `set` then `get` returns the value for every variant; `Temp` writes only `Conditions`; `ResistorR(1)` changes only `resistors[1].r`.

**`spicy_netlist` (lowering)**
5. `.temp 60` → `Conditions.temp` 333.15; `.param t=25` + `.temp {t}` → 298.15; `.temp 25 60` → error "one value"; no `.temp` → 300.15.
6. `.options tnom=25 reltol=1e-6 vntol=1e-9 abstol=1e-15` → `Conditions.tnom` 298.15 and those `SolverOptions`; an unknown key → an error listing the accepted keys; no `.options` → defaults.
7. `.model Q NPN xtb=1.5 xti=3 eg=1.11 tnom=25` → the four fields, `tnom` `Some(298.15)`; without `tnom` → `None`.
8. **Two cards with equal numbers and different names give two model entries; one card used by two instances gives one** (fails today).
9. `CircuitNames.bjt_models` holds the card names in model-table order.
10. **Exact numbers:** 10,000 seeded random doubles printed with `{:e}` parse back bit-exact; `4.69207e3` gives 4692.07; `4.7k` equals `4.7e3` bit for bit (fails today, §6.3).
11. Every existing parser and simulator snapshot is unchanged.

**`spicy_simulate` / CLI**
12. `SimulationConfig::default().with_options(&SolverOptions::default())` equals `SimulationConfig::default()`'s Newton settings.
13. A netlist with `.options reltol=1e-6` runs with that tolerance (its Newton iteration count differs from the default run).

**Exporter (M1f, `spicy_backends`)**
14. Snapshot of `ce_amp.spl`'s numeric export at nominal (`.temp 25`, `.options tnom=25 …`, the D-A card with `TNOM=25`).
15. **Round trip,** `ce_amp.spl`: M1e → export → parse → lower passes `assert_same_lowered` (nodes matched by name; everything else bit-exact).
16. The same at corner 102 and at the bias-max σ point (non-round values).
17. Every `circuits/*.spicy`: lower → export → lower is the same, and the second export equals the first byte for byte.
18. Temperatures from −60 to 200 °C in 0.01 °C steps survive K → °C → K bit-exact.
19. **Two BJTs with equal nominal cards** (two-stage) keep two model entries through the round trip.
20. Engine deck: at nominal it lowers to the same `Lowered` as the numeric export; exactly the bound fields print as `{k_i}`; one read-back path per target; the Engine and Tight decks differ only in the `.options` line.
21. Name map: bijective; `gnd` on a non-ground net renamed; names colliding case-insensitively renamed; no model name equals an instance name.
22. (Needs ngspice; skips when it's absent) the read-back of every knob equals the requested value at nominal and at corner 102 (plan §5.4).

**Engine (M3)**
23. `rev` changes when a `Policy` tolerance changes; it doesn't change when a comment is edited.

---

## 11. Decisions for the user

| # | Question | Recommendation | Main reason | Changes |
|---|---|---|---|---|
| 1 | Where solver options live | The type in `spicy_circuit`; `Lowered.options` = the source's request (like `analyses`); every consumer takes options as an argument; per-kind profiles in the engine's `Policy` | No reference puts tolerances in the circuit (§3.1); the engine solves one circuit at several precisions | refines plan §5.3 |
| 2 | Where TNOM lives | As proposed: `Conditions.tnom` + `BjtModel.tnom: Option<f64>` | All four references; OSDI reads it from the simulator (§4) | — |
| 3 | D-A's card | `tnom: Some(298.15)` on the card *and* `Conditions.tnom = 298.15` | A card keeps its meaning when copied | plan §1.4's example deck gains `TNOM=25` |
| 4 | Model identity in the SPICE lowering | One entry per card name, not merged by value | Per-placement models must survive the round trip (§6.2); ngspice does it | revises `circuit.md` §4.2 / decision 2 |
| 5 | Engine deck source | `Lowered` + `Binding`, with a small `ParamRef` | Part kinds expanded once (`pipeline.md` §6(c)); the done-when becomes structural (§5.2) | changes plan §5.1's signature |
| 6 | Round-trip equality | Nodes matched by name; numbers bit-exact; names through the map; exact number parsing in `spicy_netlist` | Node ids legitimately change; 28% of doubles fail today **[E]** (§6.3) | sharpens roadmap M1f's done-when |
| 7 | Cache keys | Names, origins, the source's analyses and options stay out; bound fields masked; M3 keeps hashing the deck text | Children reusable across placements; false misses only (§7) | answers `engine_types.md` open question 3 |

## 12. Doc changes, if accepted (not applied here)

- **`circuit.md`:**
  - §4.2 and decision 2: "one model entry per card; identical cards are no longer merged by value";
  - §6: add `Conditions`, `SolverOptions`, `ParamRef` and model names.
- **`engine_plan.md`:**
  - §1.4: `TNOM=25` on the card;
  - §5.1: `engine_deck(&Lowered, &Binding, &SolverOptions)`;
  - §5.2: note `BjtParams.temperature` as later, for zones;
  - §5.3: `Lowered.options` is the source's request, and consumers take options as an argument.
- **`engine_types.md`:**
  - §4: `Backend::Input` for our simulator = `Lowered` + `Binding` (`Conditions` was missing);
  - §9.3 and open question 3: as §7 here.
- **`pipeline.md`** §3: the `Conditions` row says `tnom` is fixed per design; add `SolverOptions` as a per-request row.
- **`roadmap.md`** M1f: the round trip's equality (§6.1) and the parser's exact number parsing as a "needs first".
