# Engine Types and Interfaces (M3a)

> 2026-09-28 · The design note for roadmap step M3a. It turns `engine_plan.md` (accepted 2026-09-27) into the types, traits and files M3b–M3f build.
> Reads with: `engine_plan.md` (the what and why; cited "plan §n"), `engine.md` v4, `model.md` (the `FlatDesign`, `KnobTable` and `FlatContract` the engine reads), `circuit.md`, `pipeline.md`.
> **Status:** draft for review. The crates are created only after this note is reviewed (the other session is also editing the workspace `Cargo.toml`).

---

## 0. What this note decides

The plan fixed the algorithm, the verdicts and the backend. This note fixes how the code is cut. Eleven decisions, each with its reason, marked **T1–T11**:

| # | Decision | Section |
|---|---|---|
| T1 | The engine's core works on its own `KnobSpace` and `Plan`; one adapter builds them from `spicy_model` | §2 |
| T2 | A statistical knob's distribution comes from an engine policy, keyed on the knob's kind | §2.2 |
| T3 | The `Backend` trait is batch-in, results-in-order, generic over its input | §4 |
| T4 | A `Run` is plain arrays indexed by the plan's `Needs`, never by names | §4.2 |
| T5 | Every stage is a resumable state machine; one driver merges their requests into rounds | §5 |
| T6 | One run table per check, indexed per cone by physical values | §6 |
| T7 | Measures compile from `MExpr` into small programs over the run's arrays | §7 |
| T8 | Each verdict table is one function, one match arm per row, one test per row | §8 |
| T9 | The report is a `serde` struct, schema `spicy.check/0`, stored per `rev` | §9 |
| T10 | The ngspice worker is a separate binary that loads libngspice with `libloading` and speaks length-prefixed binary frames | §10 |
| T11 | Suite cases are a deck plus a JSON case file plus a checked-in answer key, until M3f brings `.spl` | §11 |

---

## 1. The data flow, with types, on one page

```
 spicy_model (M1d)                      M3f adapter               spicy_engine core (M3b–M3e test these alone)
 ─────────────────                      ───────────               ─────────────────────────────────────────────
 KnobTable ──────────────────────────► from_model() ──► KnobSpace   8 knobs, ε ∈ [-1, 1]
 FlatContract ───────────────────────►             ──► Plan        5 sides · 3 measures · cones · Needs
 FlatDesign ──► M1f engine_deck() ──► EngineDeck (spicy_backends::ngspice)
                                          │
                                          ▼
                             Backend::prepare(&EngineDeck, &Needs) ──► Prepared   deck loaded in a worker
                                          │
 check(&KnobSpace, &Plan, &backend, &prepared, &Settings)
   │
   ├─ stages (state machines):  Nominal → Enumerate → Inside → Sigma → Band
   │     each yields Vec<Request> ──► Driver: merge, dedup against RunTable ──► Backend::run(batch)
   │     each resumes with the rows it asked for                        ◄── Vec<Result<Run, RunError>>
   │
   ├─ measures: Program::eval(&Run) ──► Measured per (row, measure)
   ├─ SideEvidence per side × confidence ──► worst_case_verdict() / sigma_verdict()
   └─ Record per side × confidence ──► CheckReport ──► table · JSON · .spicy/checks/<rev>.json
```

**The CE amp, corner 102, through every type** (numbers from plan §1.5):

| Type | Value |
|---|---|
| `Point` | ε = (−1, +1, +1, −1, −1, +1, +1, −1) over (temp, vcc.v, r1, r2, rc, re, c_in, β) |
| `Request` | that point, `AnalysisSet { op, ac_points: [1 kHz], sweep: 0.1 Hz–100 kHz at 50/dec }`, `Tolerance::Engine` |
| worker | `alterparam k0=-10 … k7=100 · reset · op · ac lin 1 1k 1k · ac dec 50 0.1 1e5 · destroy all` |
| `Run` | `readback: [263.15, 12.6, 47470, 9900, 4653, 1010, 1.2e-6, 100]` (K, V, Ω, Ω, Ω, Ω, F, 1); `op: [6.5595, …]`; `devices: [q1 active]`; `ac_points: [h(1 kHz)]`; `sweep: 301 points` |
| `Measured` | vc 6.5595 V · gain 4.4748 · f_low 17.77 Hz |
| `RunTable` | row 102; bias's index keys it by 7 physical values (no c_in), gain's and bass's by 8 |
| `Record` | bias max, `worst_case`: FAIL, value 6.5595 V, c_in `any` |

---

## 2. Inputs: the engine's own knob space and plan (T1, T2)

**T1. The core never takes `spicy_model` types.** `check()` takes a `KnobSpace` and a `Plan`, both owned by `spicy_engine`. One function builds them from the model:

```rust
// spicy_engine::adapter (M3f)
pub fn from_model(knobs: &KnobTable, contract: &FlatContract, design: &FlatDesign, policy: &Policy)
    -> Result<(KnobSpace, Plan), AdaptError>;
```

*Why:* the plan runs M3b–M3e on hand-written decks and hand-built contracts (plan §9.1), so the core must be testable before M1d's contract flattening exists. And it keeps the engine's types independent of how the language evolves: a new contract feature changes the adapter, not the search.
*Precedent:* rustc lowers its AST to HIR and then MIR, and the borrow checker works on MIR only, never on syntax. `spicy_simulate` reads `spicy_circuit`, never a parser's `Deck` (pipeline.md §3).

### 2.1 `KnobSpace`

```rust
pub struct KnobId(u32);                          // index into KnobSpace::knobs
pub struct KnobSpace { pub knobs: Vec<KnobSpec> }
pub struct KnobSpec {
    pub id: KnobId,
    pub path: Path,                              // "r1.value", "vcc.v", "temp" (model.md E16)
    pub unit: Unit,                              // SI; temp in K
    pub lo: f64, pub hi: f64, pub nominal: f64,  // physical values; nominal = midpoint in the MVP
    pub kind: KnobKind,
}
pub enum KnobKind { Range, Statistical(Dist) }
pub enum Dist { TruncNormal { edges_at_sigma: f64 }, Uniform }
pub struct Point(pub Box<[f64]>);                // ε per knob, in KnobId order
```

- **ε ↔ value:** `x = mid + half · ε`, with `mid = (lo + hi)/2` and `half = (hi − lo)/2`. Every MVP knob has its nominal at the midpoint (`model.md` E16: `±` is symmetric, a range's nominal is its midpoint), so ε = 0 is nominal. An asymmetric nominal later gets its own mapping; `KnobSpec::nominal` is there for it.
- **One conversion, in one place:** `KnobSpace::value(id, ε) -> f64`. The run table's keys (§6) and the backend's `alterparam` values both come from it, so a key and the simulated value can't disagree.

### 2.2 Where a distribution comes from (T2)

`model.md`'s `KnobTable` records kind, nominal, lo and hi, but no distribution. The engine supplies one:

```rust
pub struct Policy {
    pub statistical: Dist,            // default TruncNormal { edges_at_sigma: 3.0 }  (plan §1.3, engine.md §2.4)
    pub robustness: Option<Dist>,     // Some(Uniform): the uniform-spread check (plan §2.6 step 5)
    pub confidence: Confidence,       // default Sigma(3.0) for user specs (engine.md D11)
    pub settings: Settings,           // tolerances, sweep density, nudge h, budgets (plan §1.3)
}
```

*Why:* a distribution isn't something the MVP language can write, and part records (M5) will carry one per part. Until then one engine-wide default is honest, provided it's printed: every report lists the policy it used (plan §1.3: "each one is written into the check report"). When M5 adds `KnobTable::dist`, the adapter prefers it over the policy.

### 2.3 `Plan`

```rust
pub struct Plan {
    pub sides: Vec<Side>,
    pub measures: Vec<MeasureDef>,
    pub cones: Vec<Cone>,                        // distinct cones; ConeId = index
    pub needs: Needs,                            // everything any run must return
}
pub struct Side {
    pub id: SideId, pub spec: SpecName,          // "bias"
    pub sense: Sense,                            // Max (value ≤ bound) | Min (value ≥ bound)
    pub bound: f64, pub measure: MeasureId,
    pub confidence: Confidence,                  // WorstCase | Sigma(k); the headline
}
pub struct MeasureDef { pub name: String, pub program: Program, pub cone: ConeId, pub unit: Unit }
pub struct Cone(pub Vec<KnobId>);                // sorted
pub struct Needs {
    pub probes: Vec<Probe>,                      // node voltages the op must return: v(output) …
    pub devices: Vec<DeviceNeed>,                // q1: ic, ib, vbe, vbc, gm, gmu (region rule, plan §4.6)
    pub ac_points: Vec<f64>,                     // exact frequencies for at(f): [1000.0]
    pub sweep: Option<Sweep>,                    // for f_low: 0.1 Hz–100 kHz, 50 per decade
}
```

- **A spec becomes one or two sides:** `in lo..=hi` gives a Max and a Min side, `<=` a Max, `>=` a Min. The CE amp has 5 sides from 3 specs.
- **Both confidences are computed on every side** (D7); `Side::confidence` only picks the headline.
- **The cone rule in M3 is the capacitor rule** (plan §6.2): a measure that needs only the operating point doesn't depend on capacitor or inductor values. So bias's cone is 7 knobs, and gain's and bass's are 8. The influence graph comes later with the loop.
- **Needs are the union over measures,** so one run serves every measure (plan §2.4). A transient measure will get its own `AnalysisSet`, never folded into every run (plan §6.3).

---

## 3. The check's entry point

```rust
pub fn check<B: Backend>(space: &KnobSpace, plan: &Plan, backend: &B, prepared: &B::Prepared,
                         policy: &Policy) -> CheckReport;
```

- **One call, one cold check** (plan §1.1). No state survives between calls except what the CLI stores (§9.2).
- **It never fails as a whole.** A failed run becomes data (W4, S5). A binding mismatch stops the check, but still returns a report whose records say UNDECIDED (binding) with the knob named (W0, plan §4.4).

---

## 4. The `Backend` trait (T3, T4)

```rust
// spicy_engine::backend
pub trait Backend {
    type Input;                                   // ngspice: EngineDeck. Our simulator (M4): Circuit + Params + Binding
    type Prepared;
    fn prepare(&self, input: &Self::Input, needs: &Needs, space: &KnobSpace) -> Result<Self::Prepared, BackendError>;
    fn run(&self, prepared: &Self::Prepared, batch: &[Request]) -> Vec<Result<Run, RunError>>;   // same order
    fn identity(&self) -> BackendIdentity;        // "ngspice-42, KLU": part of rev (plan §1.2)
}
pub struct Request { pub point: Point, pub analyses: AnalysisSet, pub tolerance: Tolerance }
pub struct AnalysisSet { pub op: bool, pub ac_points: bool, pub sweep: bool }   // over Needs
pub enum Tolerance { Engine, Tight }             // two decks loaded (plan §4.2): 1e-6 and 1e-9 reltol
```

**T3. Batch in, results in order, generic input.**
- *Batch:* a round is one call, so the backend decides how to spread it over workers, and the engine stays sequential and deterministic (plan §6.3; scale §3.3 measured 6× on 16 workers).
- *In order:* `run(batch)[i]` answers `batch[i]`. The driver never matches results by content.
- *Generic input:* the engine never constructs a netlist or a `Circuit`. The CLI builds the backend's input (M1f's `EngineDeck`, or a hand-written deck in M3b–M3e) and calls `prepare`. So adding our simulator later (M4) adds an `impl Backend`, and nothing in the engine changes.

*Precedent:* the plan's `Backend` (§6.2) plus round 2's batch `run` (engine_flows §7). The associated `Input` follows Rust's `Iterator::Item` style: each implementation names its own.

### 4.1 Errors

```rust
pub enum RunError {
    Failed(String),                               // ngspice reported an error, after the retry ladder
    Implausible { probe: ProbeId, value: f64 },   // e.g. 999,999,999.99 V (plan §4.3 rule 3)
    WorkerDied,                                   // restarted and retried once; still dead
}
pub enum BackendError { Load(String), MissingVector(String), Library(String) }
```

A binding mismatch is **not** a `RunError`: the backend returns the read-back values, and the engine compares them (§5.3). The check, not the backend, owns the verdict rules.

### 4.2 `Run`: plain arrays in `Needs` order (T4)

```rust
pub struct Run {
    pub readback: Box<[f64]>,                    // per KnobId, physical value as the simulator used it
    pub op: Box<[f64]>,                          // per Needs::probes
    pub devices: Box<[DeviceOp]>,                // per Needs::devices
    pub ac_points: Box<[C64]>,                   // per Needs::ac_points: v(out)/v(in) inputs, see §7
    pub sweep: Option<SweepData>,                // frequencies + complex values per probe
}
pub struct DeviceOp { pub ic: f64, pub ib: f64, pub vbe: f64, pub vbc: f64, pub gm: f64, pub gmu: f64 }
pub struct C64 { pub re: f64, pub im: f64 }      // our own 2-field type: |z|, arg, division. No dependency
```

**T4. Indexes, not names.** The names live once, in the deck's probe map (plan §5.1). The backend resolves them at `prepare` and fails there if a vector is missing, so a missing probe is found on the nominal run, not on corner 200.
*Precedent:* pipeline.md's rule, "results by index (fast); names attached only at the edge".

---

## 5. Stages and the driver (T5)

### 5.1 A stage is a state machine

```rust
pub trait Stage {
    /// Requests for the next round, or Done. Called again with the rows it asked for.
    fn poll(&mut self, ctx: &Ctx, answered: &[RowId]) -> Step;
}
pub enum Step { Need(Vec<Request>), Done }
pub struct Ctx<'a> { pub space: &'a KnobSpace, pub plan: &'a Plan, pub table: &'a RunTable, pub policy: &'a Policy }
```

The five stages of plan §2, in order:

| Stage | Instances | Asks for | CE amp runs |
|---|---|---|---|
| `Nominal` | 1 | the nominal point; checks every read-back path exists | 1 |
| `Enumerate` | 1 per distinct cone within budget | all 2^\|cone\| corners (shared: the 8-knob cone's corners cover the 7-knob one) | 256 |
| `Inside` | 1 per side | tangent nudges at the worst corner, 8 audit points; then the ascent if anything improves | 46 |
| `Sigma` | 2 per side × spread (starts A and B) | search rounds, cross-corner checks, range nudges | 265 |
| `Band` | 1 per side | the decisive points at `Tolerance::Tight` | 11 |

**T5. Why state machines.** A σ search needs 3–7 rounds, and ten of them run at once (5 sides × 2 starts). As state machines, their requests merge into shared rounds: about 19 rounds instead of 65 (plan §2.8). And the engine stays free of I/O: a stage never calls the backend, so every stage is unit-testable with hand-made rows.
*Precedent:* the "sans-IO" style of Rust protocol crates (quinn's `quinn-proto`, `rustls`): the state machine takes inputs and emits outputs, and a separate driver does the I/O. Round 2 proposed the same (engine_flows §7 rec. 3).

### 5.2 The driver

```
loop:
  for each live stage, in plan order:  step = stage.poll(ctx, its answered rows)
  batch = the Need requests, de-duplicated, minus points the RunTable already has
  if batch is empty and every stage is Done: stop
  results = backend.run(prepared, batch)                 one round
  append each result to the RunTable (a row even when it failed)
  hand each stage the RowIds it asked for, in its own request order
```

- **Deterministic:** stages are polled in plan order, and rows are appended in batch order. The same inputs give the same rows, the same report and the same bytes (plan §8.1: exact comparisons within one build).
- **Stage order is also data order:** `Sigma` starts after `Enumerate` and `Inside` finish, because start B and the S2/S7/S8 rules read their results.

### 5.3 Read-back and plausibility, checked in one place

After each round the driver compares every row's `readback` with `KnobSpace::value(id, ε)`. A relative difference above 1e-9 marks the check `binding: Some(knob)`, and every record becomes UNDECIDED (binding) (W0/S0). This caught the dead temperature knob after one run (plan §4.4).

---

## 6. The run table (T6)

```rust
pub struct RowId(u32);
pub struct Row { pub point: Point, pub tolerance: Tolerance, pub result: Result<Run, RunError>, pub measured: Box<[Measured]> }
pub struct RunTable {
    rows: Vec<Row>,
    exact: HashMap<(Key, Tolerance), RowId>,     // dedup: every knob
    by_cone: Vec<HashMap<Key, Vec<RowId>>>,      // one index per distinct cone (ConeId)
}
pub struct Key(Box<[u64]>);                      // f64::to_bits of the physical values, in knob order
```

**T6. Keys are physical values over a cone.**
- **Physical, not ε:** a key survives an edit to another knob's range, which the cross-revision cache will need (scale §7.3: a β-edge edit kept 1,401 of 2,608 AC runs). The one canonical conversion (§2.1) makes equal points give equal bits.
- **Per cone:** bias's index ignores c_in, so the two corners that differ only in c_in fall under one key. The side reads one of them, and a debug assertion checks they agree (the capacitor rule says they must).
- **Measured once:** each row's measures are evaluated when the row arrives and stored in `Row::measured`, so the stages read numbers, never raw vectors.

The plan's independent review found that its first draft keyed one index for two cones, and mixed ε and physical values. This layout fixes both.

---

## 7. Measures (T7)

```rust
pub enum Measured { Value(f64), Undefined(&'static str) }       // Beyond later (C9)
pub enum Program {                                              // compiled from model.md's MExpr
    Probe(ProbeId),                                             // dc(output.v)
    AcAt { num: ProbeId, den: ProbeId, point: usize },          // h.at(1kHz)  (complex)
    Mag(Box<Program>),
    FLow { num: ProbeId, den: ProbeId, db: f64 },               // h.f_low(-3dB)
    Binary(Op, Box<Program>, Box<Program>), Const(f64),         // headroom = vcc.v - dc(output.v)
}
impl Program { pub fn eval(&self, run: &Run, space: &KnobSpace, point: &Point) -> Measured; }
```

- **Derived measures are evaluated per run** (D-F): `vcc.v − dc(output.v)` reads `vcc.v` from the same point's knob value and VC from the same run. That's the no-double-counting property affine forms were for (plan §10.1 D-F).
- **`h` is a ratio of two probes,** `v(output)/v(input)`. The engine never assumes the AC source is exactly 1 V (plan §1.4).
- **`f_low(L)`:** the reference is max |H| over the swept band; the answer is the crossing nearest below the peak, at max|H| + L dB, found by cubic interpolation in (ln f, dB) on the 50-per-decade sweep (plan §4.6, D5). No crossing inside the band gives `Undefined`, which can never pass.
- **Type checking is the model's job:** `model.md` E24 resolves and unit-checks `MExpr`. The compiler into `Program` only maps probes to `ProbeId`s and rejects functions M3 lacks (`tran`, `noise`) with a clear error.

---

## 8. Evidence and verdicts (T8)

```rust
pub struct SideEvidence {                        // per side × confidence × spread
    pub inner: Option<(f64, RowId)>,             // worst in-box value and where
    pub eps_num: Option<f64>,
    pub over_budget: bool,
    pub failed_runs: Vec<RowId>,
    pub ascent: AscentOutcome,                   // NotNeeded | Converged | Budget
    pub search: Option<SearchOutcome>,           // sigma: converged?, bracket, starts
    pub region_changes: bool, pub interior_flag: bool,
    pub uniform: Option<Box<SideEvidence>>,      // sigma, where plan §2.6 step 5 applies
}
pub fn worst_case_verdict(side: &Side, ev: &SideEvidence, binding: Option<KnobId>) -> Verdict;   // rows W0–W7
pub fn sigma_verdict(side: &Side, ev: &SideEvidence, worst_case: &Verdict, binding: Option<KnobId>) -> Verdict; // S0–S12

pub enum Verdict { Fail { at: RowId }, Pass(PassKind), Undecided(Reason) }
pub enum PassKind { AllCorners, Estimated, ImpliedByWorstCase }  // Guaranteed later
pub enum Reason { Binding(KnobId), Budget, Numerics, Simulator, AscentBudget, Search,
                  DeviceRegionChanges, InteriorPeak, Distribution, UniformSpread(Box<Reason>), Bracket }
```

**T8. One function per table, one arm per row, one test per row.** Plan §3.1's tables say "the first matching row wins", which is exactly a `match` on guards in row order. Each row gets a unit test with hand-made evidence that reaches it and no earlier row, so a reordering is caught.
*Why a function and not data:* the rows read different evidence fields, and some compare numbers (`I > B + ε_num`). As code, the compiler checks every field exists; as data, a typo would be a silent never-matching row.

The CE amp, derived from the tables as written (plan §3.1): bias max is `Fail` by W2 at `worst_case` and `Pass(Estimated)` by S11 at `sigma(3)`; the other four sides are `Pass(AllCorners)` by W6 and `Pass(ImpliedByWorstCase)` by S2.

---

## 9. The report (T9)

### 9.1 Types

```rust
#[derive(Serialize, Deserialize)]
pub struct CheckReport {
    pub schema: &'static str,                    // "spicy.check/0" (unstable until the editor reads it)
    pub rev: Rev,                                // §9.3
    pub policy: Policy,                          // every default that moved a number (plan §1.3)
    pub model_defaults: Vec<String>,             // "q1: bare Npn → IS=1e-14 BF={beta} XTB=1.5 XTI=3 EG=1.11, TNOM 25 °C"
    pub not_modeled: Vec<String>,                // "q1: no VAF (no Early effect)", "q1: no junction capacitances"
    pub records: Vec<Record>,
    pub binding: Option<String>,                 // "asked −10 °C, ngspice ran 27 °C"
    pub cost: Cost,                              // runs by stage, rounds, wall time, backend
}
#[derive(Serialize, Deserialize)]
pub struct Record {
    pub spec: String, pub side: Sense, pub confidence: Confidence, pub headline: bool,
    pub verdict: Verdict, pub value: Option<f64>, pub unit: Unit, pub margin: Option<f64>,
    pub eps_num: Option<f64>,
    pub counterexample: Vec<(String, KnobValue)>,        // path → value, or Any outside the cone
    pub regions: Vec<(String, Region)>,                  // q1: Active | Saturated | Cutoff
    pub method: Method,                                  // AllCorners { runs } | SigmaSearch { starts, rounds } …
    pub tags: Vec<Tag>,
    pub claim: String,                                   // the sentence (plan §3.5)
    pub next: Option<Next>,                              // what would settle an UNDECIDED, with its cost
}
pub enum KnobValue { At(f64), Any }
```

**T9.** The report is data first, text second: the terminal table, `--explain` and the JSON all render one `CheckReport`, so they can't disagree. `claim` is generated once, in the engine, and the agent may paraphrase it but never strengthen it (plan §3.5). The CE amp's bias-max record, as JSON:

```json
{ "spec": "bias", "side": "max", "confidence": "worst_case", "headline": false,
  "verdict": { "fail": { "at": 102 } }, "value": 6.559537, "unit": "V", "margin": -0.059537,
  "eps_num": 6.6e-9,
  "counterexample": [["temp", {"at": 263.15}], ["vcc.v", {"at": 12.6}], ["r1.value", {"at": 47470}],
                     ["r2.value", {"at": 9900}], ["rc.value", {"at": 4653}], ["re.value", {"at": 1010}],
                     ["c_in.value", "any"], ["q1.beta", {"at": 100}]],
  "regions": [["q1", "active"]], "method": { "all_corners": { "runs": 256 } }, "tags": [],
  "claim": "bias max FAILS at worst case: 6.5595 V > 6.5 V at temp −10 °C, vcc.v 12.6 V, r1 47.47k, r2 9.9k, rc 4.653k, re 1.01k, q1.beta 100 (c_in: any). Reproduces at reltol 1e-9 within 7e-9 V.",
  "next": null }
```

### 9.2 The store

`spicy check` writes `.spicy/checks/<rev>.json`: the report plus the run table (≈ 0.25 MB for the CE amp, plan §1.1). `--explain <spec>` reads it when the file's current `rev` matches (0 runs), and otherwise runs the check first. A stored report of another `rev` is shown as `stale`, never as current (plan §3.6).

### 9.3 `rev`

A hash of a canonical byte encoding, written by the engine, of: the `KnobSpace`, the `Plan`, the deck text and knob map, the `Policy` and the backend identity. Spans are excluded, so a comment edit keeps the `rev` (model.md E22). The hash is **FNV-1a 64-bit**: stable across Rust versions, which `std`'s `DefaultHasher` doesn't promise, and enough for a cache key (it guards against stale files, not attackers). No new dependency.

---

## 10. The ngspice backend (T10)

### 10.1 Files

```
crates/spicy_backends/
  src/lib.rs
  src/ngspice/mod.rs        NgspiceBackend: impl Backend; worker pool (P = 1 by default)
  src/ngspice/deck.rs       EngineDeck, KnobBinding, ProbeMap (plan §5.1); parsing a hand-written deck's header
  src/ngspice/protocol.rs   frames, shared by both ends
  src/ngspice/batch.rs      the `ngspice -b` cross-check path (tests only)
  src/bin/spicy-ngspice-worker.rs   loads libngspice, serves frames on stdin/stdout
```

### 10.2 Loading libngspice

The worker opens `libngspice.so.0` at run time with the `libloading` crate. It uses the five functions the prototypes used (`ngSpice_Init`, `ngSpice_Circ`, `ngSpice_Command`, `ngGet_Vec_Info`, `ngSpice_CurPlot`) and three callbacks (output, status, exit).

**T10, the loading half.** Run-time loading needs only the runtime package (Ubuntu's `libngspice0` ships `libngspice.so.0` and no development symlink), no headers and no build script. If the library is missing, the error names the package. *Alternatives:* linking at build time needs `libngspice0-dev` on every machine that builds `spicy`, even one that never runs the engine; raw `dlopen` through `libc` is the same thing with more `unsafe`. `libloading` is small and has no other dependencies.

### 10.3 The protocol

Length-prefixed binary frames on the worker's stdin/stdout, little-endian, `f64` as raw bits:

| Frame | Direction | Content |
|---|---|---|
| `Load` | → worker | deck text, tolerance tag |
| `Loaded` / `Error` | ← worker | ok, or ngspice's message |
| `Run` | → worker | knob values (physical, deck units after the binding's conversion), analysis set |
| `Result` | ← worker | read-backs, op values, device records, AC points, sweep: in `Needs` order |
| `Shutdown` | → worker | — |

**T10, the protocol half.** Binary keeps every `f64` exact (no 15-digit text round trips, plan §10.1 D-B) and costs nothing to parse. A dead worker is detected by a closed pipe, restarted (18–19 ms), and the run retried once; then it's `WorkerDied` (plan §4.1).

### 10.4 The rules, where they live

Plan §4.3's eleven rules each have one home: `destroy all`, `reset`, `.temp`/`.options` and exact `ac lin 1 f f` in the worker's run sequence; plausibility and read-back in the driver (§5.3); TNOM, model names and read-back paths in the deck (M1f's contract, plan §5); "never `sens`, never `meas`" by construction (the worker has no code path that sends them). Each has a test (§11.3).

---

## 11. Tests (T11)

### 11.1 Unit tests, in each module's `mod tests`

| Module | What the tests pin |
|---|---|
| `knob` | ε ↔ value round trip; the exact truncated-normal map (Φ, Φ⁻¹ against known values) |
| `plan` | spec → sides; cones by the capacitor rule; needs as a union |
| `measure` | `f_low` on an analytic high-pass (exact answer known); `Undefined` with no crossing; `at(f)` |
| `runs` | dedup; per-cone keys ignore out-of-cone knobs; equal points give equal bits |
| `verdict` | one test per row of W0–W7 and S0–S12 |
| `stages` | each state machine against hand-made rows, through a closure backend in `test_utils` |
| `ngspice::protocol` | frame round trips |

The closure backend is for unit tests of the driver and stages only. The suite runs on real ngspice decks (D9).

### 11.2 The suite (T11)

Until M3f can read `.spl`, each suite case is three files in `crates/spicy_backends/test_data/suite/`:
- `<case>.cir`: an engine deck, hand-written in exactly M1f's format (plan §1.4);
- `<case>.case.json`: the knob table (path, kind, lo, hi, `.param` name), the measures and specs, the policy;
- `<case>.key.json`: the answer key, generated once by a `--regenerate-keys` test mode (all corners, plus a dense interior search for power-type measures, plus the ball-search σ key) and checked in.

**T11. Why checked-in keys:** a key takes seconds to minutes to compute, and a test that recomputes its own oracle with the code under test proves little. Regenerating is an explicit step whose diff a reviewer sees. *Why JSON:* `serde_json` is already in the workspace; the case format dies in M3f, when `ce_amp` moves to the `.spl` pipeline.

The cases are plan §8.2's: `mvp` (XTB 1.5 and 0), `hiz`, `band`, `pq`, `tuned5`, `tuned20`, `tuned20_b20`, `deadT` and `diffamp`, 7 circuits and 24 sides. The acceptance criteria are plan §8.3's: no false PASS, `worst_case` equal to the key, σ within 1e-4 of the σ key, the listed UNDECIDEDs exactly, run counts pinned. The silent no-DC-solution result gets its own backend-rule test (§11.3), not a suite case.

### 11.3 Backend-rule tests

One test per rule of plan §4.3, on tiny decks: `reset` undoes `option temp` (so the deck must use `.temp`); runs slow down without `destroy all` (timing, marked slow); no DC solution returns 999,999,999.99 V (so plausibility must catch it); `ac dec` misses 1 kHz; a netlist error kills the worker and the pool restarts it. These pin ngspice's behavior, so an ngspice upgrade that changes one is noticed.

### 11.4 Without ngspice

Tests that need libngspice skip with a printed reason when it's missing, as the roadmap planned (§3). CI installs `ngspice` and `libngspice0`, so there they always run.

---

## 12. Modules and crates

```
spicy_engine/src/
  lib.rs          check()
  knob.rs         KnobSpace, KnobSpec, Dist, Point, the maps
  plan.rs         Plan, Side, Cone, Needs, MeasureDef
  measure.rs      Program, Measured, f_low
  backend.rs      Backend, Request, Run, RunError
  runs.rs         RunTable, Key
  driver.rs       Stage, Step, the round loop, read-back
  stages/         nominal.rs · enumerate.rs · inside.rs · sigma.rs · band.rs
  verdict.rs      SideEvidence, the two verdict functions
  report.rs       Record, CheckReport, claim text, the table renderer
  adapter.rs      from_model (M3f)
  test_utils.rs   the closure backend, row builders
```

- **Dependencies:** `spicy_engine` → `spicy_model` (for the adapter's input types only), `serde`, `serde_json`, `thiserror`. `spicy_backends` → `spicy_engine`, `libloading`. The engine never depends on a simulator crate (roadmap §2.2).
- **Size:** minimal estimated its version at about 1,850 Rust lines plus 400 of test harness; the plan adds the read-back, the ascent, the audit, the second σ start, the ball search, the records and the store on top (plan Appendix D). The prototypes that proved each piece were 270–1,000 lines of Python.
- **Creating the crates** (after this note's review): two workspace members, `spicy_engine` and `spicy_backends`, added to `Cargo.toml` in coordination with the other session's edits there.

---

## 13. Open questions

1. **The worker count.** P = 1 is enough for the CE amp (0.4 s). Should the default be the core count once a check takes over a second?
2. **Where the case format lives after M3f.** Keep `case.json` for the adversarial decks that have no `.spl` (hiZ, pq, tuned), or write them in `.spl` once the language has the parts they need?
3. **`rev` and the deck text.** Hashing the deck text ties `rev` to M1f's exact output, so an exporter change (say, a comment) changes every `rev`. Hash the deck without comments instead?
4. **`Policy` in the report.** All of it, or only the fields that differ from the defaults?
