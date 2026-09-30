# Red-team: the simulator-facing circuit and the stages around it

> Research report, 2026-09-25. It red-teams the plan to give the simulator its own input type (`spicy_circuit`), against the owner's requirements: a separate reason to exist for every representation, per-kind layout for speed, clean crate boundaries, and native language input.
> It is based on the code at commit `6f64e57`. Measurements were made in a temporary copy of the repo; no repository files were changed. Conclusions are merged into `../pipeline.md`.

## 0. Bottom line

- **There never was a single definition.** Each device already has two parameter representations: `ResistorSpec`/`ResistorModel` in the parser and `spicy_simulate::Resistor` (`devices/resistor.rs`).
  - The simulator struct also mixes three lifetimes: resolved parameters, derived constants (`thermal_voltage`, `bjt.rs`) and stamp locations (`NodePairStamp`).
- **Recommendation: a staged design.**
  - Per-kind plain-data parameter structs, with wiring kept apart from values.
  - No knobs in the circuit.
  - The simulator owns its plan, derived-values, workspace and state stages.
  - The SPICE parser keeps its specs and lowers into the core types, which are designed for the language and engine.
- **Topology rework is 23–27% of an operating-point run.** Warm starts give about 10–15×. At scale, the dense AC path dominates, not topology.
- **Zero existing snapshots change** through the refactor. The first deliberate change comes with temperature.

## 1. Code facts that change the framing

| Fact | Where | Consequence |
|---|---|---|
| The simulator depends on the parser: 42 import sites, 24 distinct paths | `spicy_simulate/Cargo.toml` | A boundary violation today. `clap` is a library dependency too |
| Resolution happens per analysis: `from_spec` → `create_matrix` → analyze | `dc.rs`, `trans.rs`, `ac.rs` | Topology is rebuilt for every analysis, and `NodeMapping` is cloned into the matrix |
| `m`/`scale` are resolved but never stamped | `resistor.rs` | Checked: `R 1k m=2` draws 1 mA, not 2 mA |
| Silent defaults | `resistor.rs` (missing R = 1 mΩ); `bjt.rs` (IS = 1e-14, where SPICE uses 1e-16) | They'd show up as ngspice cross-check differences |
| `.param` provenance is already lost | `parser_utils.rs` | Expressions are evaluated at instance parse; specs keep literal formatting only |
| Some defaults depend on other parameters or the analysis | `ac = r` (`resistor.rs`); pulse `tr = dt`, `pw = tstop` (`netlist_waveform.rs`) | Resolving them once at lowering goes stale when the parameter is a knob, so they must be resolved per run |
| Phase deg/rad lives in `Value.suffix` | `sources.rs`, `expr.rs` | Lowering must convert to radians |
| The DC sweep mutates device values and reuses the matrix | `dc.rs` | This is a parameter layer in miniature. An immutable circuit needs a per-run override path even for `.dc` |
| Trapezoidal state is keyed by device name (`HashMap<&str, f64>`) | `trans.rs` | State must be index-based once devices carry no strings |
| **Subcircuits:** names come out as `"1_R1"`, internal nodes aren't prefixed, the X-line span is lost, expansion is one level only | `expr.rs`, `instance_parser.rs`, `subcircuit_phase.rs` | **Checked:** two `DIV` instances share node `mid` |
| The parser allocates MNA branch rows | `node_mapping.rs`, `instance_parser.rs` | An MNA concept inside a front-end; it fixes the result order |
| KLU `factor` takes `&mut KluSymbolic` and writes `lower_nz` | `factor.rs` | The symbolic analysis can't be shared read-only across threads |
| AC is dense, has no BJT linearization, allocates per frequency, and prints per frequency | `ac.rs` | Unusable in an engine loop as written |
| The TUI uses only `deck.commands` | `spicy_cli/src/tui/worker.rs` | Dropping specs would cost the TUI nothing |
| The simulator library owns CLI concerns | `lib.rs`: `write_raw`, `output_base`, `get_output_base(&Deck)`, the `simulate()` dispatcher | These belong in `spicy_cli` |

## 2. Measurements

CE amplifier, `.op`, KLU, release build, median of 7 batches.
- "×N" means N independent copies of the stage sharing the supply.
- The cold Newton solve takes **18 iterations**, because there's only crude clamping and no junction limiting.
- VC = 5.5216 V.

| ns per call | ×1 (dim 5) | ×10 (dim 32) | ×100 (dim 302) |
|---|---|---|---|
| parse (text → Deck) | 4,760 | 31,790 | 281,882 |
| `Devices::from_spec` | 182 | 2,473 | 24,742 |
| `NodeMapping` clone | 118 | 1,186 | 12,236 |
| pattern + stamp indices | 711 | 4,691 | 55,739 |
| KLU analyze | 500 | 1,398 | 9,151 |
| **topology total** | **1,511** | **9,748** | **101,868** |
| one iteration: stamp + factor + solve | 787 | 3,896 | 31,056 |
| one iteration: stamp + refactor + solve | 191 | 1,393 | 13,548 |
| Newton OP on a prepared matrix | 4,590 | 28,611 | 264,139 |
| **`simulate_op` end to end** | **6,615** | **40,913** | **378,280** |
| **topology share** | **22.8%** | **23.8%** | **26.9%** |
| warm start from nominal x, refactor only | 450 (2 iterations) | 2,884 | 27,354 |
| AC, one frequency (dense) | 1,163 | 19,731 | **1,813,829** |

What the numbers say:
- **Topology reuse alone gives 1.44×.** Warm start plus skipping the first full factor gives about **14×**.
- **Regenerating SPICE text and reparsing every run would add 72%** on top of the whole OP run.
- **Dense AC dominates** any AC spec at scale.
- **For the MVP, speed is not the deciding factor.** The whole CE-amp job is about 12 ms. Decide on boundaries and correctness, but don't close off plan reuse, warm starts or sparse AC.

## 3. Candidate designs

| | Resistor representations | Each has a distinct job? | Simulator free of parser | Topology reuse | Language-first | Existing snapshots changed |
|---|---|---|---|---|---|---|
| D0 status quo | Spec, sim resistor (+ stamps) | ✗ the sim struct mixes 3 lifetimes | ✗ | ✗ | ✗ the language must fake SPICE specs | 0 |
| D1 `Vec<Device>` enum | Spec, circuit enum, sim resistor | ✗ circuit and sim structs share a lifetime | ✓ | ✗ | ~ | 0 |
| D2 POD circuit | Spec, circuit resistor, stamps[] | ✓ | ✓ | only with a same-topology guarantee | ✓ | 0 |
| D3 parser emits circuit | circuit resistor, stamps[] | ✓, but no staging for SPICE precedence | ✓ | as D2 | ✗ core types shaped by the parser | 10 parser snapshots |
| D4 topology + values now | Spec, topology + values with knob refs, stamps[] | knob refs in the circuit break the crate rules | ✓ | ✓ | ✓ | 0 |

**Recommended:** D2's plain-data per-kind parameter structs, plus D4's split of wiring from values. Knob bindings stay out of the circuit, in the backends.

## 4. Recommended stages

| Stage | Resistor / BJT | Owner | Lifetime | Job only it can do |
|---|---|---|---|---|
| SPICE spec | `ResistorSpec` + `ResistorModel`; `BjtSpec` + `BjtModel` (unchanged) | spicy_netlist | parse → lower | As-written SPICE; precedence staging; given-ness |
| Language | schema-driven instance with value `47k ± 1%` | spicy_model | per edit | Spreads, knobs, spans |
| Given parameters | `ResistorParams { r, tc1, tc2, … }`, `BjtParams { polarity, is, bf, … }`: `Copy`, SI units, no strings. Wiring is stored separately | spicy_circuit | params per run; wiring per design | Backend-neutral simulator input; the unit of knob variation; the ngspice export level |
| Derived | `ResistorEval { g, … }`, `BjtEval { is_t, bf_t, … }` | spicy_simulate | per run × temperature | Temperature equations, and defaults that depend on other parameters, computed once per run. **Only added once temperature exists** |
| Stamp locations | parallel arrays in a plan | spicy_simulate | per topology × solver | Where each device writes. Without it, pattern + analyze are rebuilt every run |
| State | limiting history; charge/current history | spicy_simulate | per iteration or step | Junction limiting; integration history (a name-keyed map today) |

**Layout.** Keep an array of structs within each kind (`Vec<ResistorParams>`), with the other stages in parallel arrays zipped together in the stamp loop. Splitting every field into its own `Vec<f64>` wouldn't help: a stamp reads all of a device's fields together.

## 5. Per-run cost (CE amp ×1)

| | Today | Recommended, cold | Recommended, warm |
|---|---|---|---|
| Build values | ~300 ns (`from_spec` + `NodeMapping` clone) | ~100 ns (plain struct fill; estimate) | ~100 ns |
| Topology | 1,211 ns | 0 (built once) | 0 |
| Newton | 4,590 ns | 4,590 ns | 450 ns |
| **Total** | **~6.6 µs** | **~4.7 µs** | **~0.55 µs** |

## 6. Migration path (each step behavior-preserving)

1. **Guard.** Golden tests for the raw writer, and a dump of today's `from_spec` output.
2. **Create `spicy_circuit`,** unused at first.
3. **Add `spicy_netlist::reader::lower(&Deck)`,** copying `from_spec` verbatim, bugs included. A bitwise differential test checks it against step 1's dump.
4. **Inside the simulator:** stamp indices move to parallel arrays; the trapezoidal map becomes an indexed `Vec`.
5. **The simulator reads the circuit.** The dispatcher, raw-file config and `Deck` handling move to `spicy_cli`. Simulator tests use the parser as a dev-dependency, so no `.snap` changes.
6. **Drop the parser from the simulator's dependencies.** CI checks the dependency direction.
7. **One plan and workspace per circuit,** shared by all analyses.
8. **Temperature adds the derived stage.** The first deliberate snapshot changes: VT becomes kT/q.

## 7. Risks and open questions

1. **Branch rows:** keep the parser's order (identical results order), or let the plan allocate them?
2. **`m`/`scale`/`area`:** fold into parameters at lowering, or keep them as parameters (possible knobs)?
3. **Addressable fields** (`ParamRef { kind, idx, field }`) for knob bindings and, later, sensitivities.
4. **KLU symbolic across threads:** clone per thread, or change `factor`.
5. **Subcircuit fixes** (internal-node leak, `1_R1` naming) are behavior changes.
6. **Default mismatches** with ngspice would show up in cross-checks.
7. **Warm starts** need deterministic seeds (always the nominal solution) and tighter Newton tolerances first.
8. **Where analysis requests live:** proposed in `spicy_circuit`, so an ngspice exporter doesn't import the simulator.
