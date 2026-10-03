# Architecture

This doc gives a bird's-eye view of spicy: how a circuit moves from text to results, what each crate holds, and the rules that keep the crates apart. It changes only when the architecture changes. The current state of the work is in the [roadmap](docs/ecad/roadmap.md#status), and the project's terms are in the [glossary](docs/glossary.md).

<a name="data-flow"></a>
## Data flow: our simulator reads our language natively

Our simulator does not need a netlist. The language produces a design model, and our simulator reads it directly, through a small, simulator-ready circuit form. SPICE text is only an **export**, for ngspice, for sharing, and for debugging.

```
.spl  ──spicy_lang──►  spicy_model  (design: blocks, knobs, contracts)
                           │
                           │  lower(design, knob point)      per run, in memory, no text
                           ▼
SPICE ──spicy_netlist──►  spicy_circuit::Circuit  ──►  spicy_simulate  (op · dc · ac · tran)
                           │
                           └──►  SPICE netlist export  ──►  ngspice (the engine's first backend, M3) / a file to share or debug
```

For the MVP (decided 2026-09-27), the engine runs on ngspice through the export ([roadmap §3](docs/ecad/roadmap.md#ngspice)). The native path stays the architecture for our simulator, which joins as the second backend (M4).

**`spicy_circuit::Circuit`** is flat, numeric and ready to simulate: named nodes, devices with fully resolved parameters, sources, and for each device its **origin** (a model path like `amp.r1`, or a SPICE line). It has no knobs and no analyses. Analyses are requests to the simulator's API. The design is in `docs/ecad/circuit.md`.

**Why native,** and not "generate a netlist and parse it again":
- **Nothing is lost in translation.** Results are keyed by model paths (`amp.q1`), and errors point at `.spl` lines, not at generated netlist lines.
- **Speed.** The engine runs dozens of simulations per spec, with no text generation or parsing per run.
- **Decoupling.** The simulator does not depend on the SPICE parser. Both front-ends (SPICE and our language) feed the same input type.

**Why the simulator does not parse `.spl` itself:** a simulator should not contain a language front-end. "Natively" means that it reads the model's lowered circuit directly. The language's job ends at `spicy_model`.

<a name="code-map"></a>
## Code map

```
crates/
  spicy_span/       `Span`: where something was written, for every stage (rustc's `rustc_span`)
  spicy_index/      the `id!` macro: typed `u32` indices (rustc's `rustc_index`)
  spicy_errors/     the problem type every stage reports (`Diag`, `Reported`) and its rendering (rustc's `rustc_errors`)
  spicy_model/      the design model (see "What spicy_model holds"); shared by language, engine and (later) the editor
  spicy_lang/       language front-end: text → spicy_model, with diagnostics
  spicy_circuit/    simulator-ready circuit form: nodes, devices, origins
  spicy_netlist/    SPICE netlists: the reader (→ spicy_circuit) and the writer
  spicy_simulate/   our simulator; reads spicy_circuit only
  spicy_cli/        CLI + TUI
  spicy_bench/      the benchmark suite (gungraun) that /stage-review uses to measure speed

  spicy_engine/     (planned, M3) knob space, measures, corner enumeration + the 3σ-point search, verdicts; defines the `Backend` trait
  spicy_backends/   (planned, M3) adapters: ngspice first; our simulator later (M4)
```

Dependencies (arrows mean "depends on"; no cycles):

```
                          spicy_cli
                  ┌──────────┼──────────────┐
             spicy_lang   spicy_engine   spicy_backends ───► spicy_simulate ──► spicy_circuit
                  │        │      │            │                                   ▲
                  └──► spicy_model ◄┘                            spicy_netlist ─────┘
                           ▲                                (SPICE front-end, used by the CLI)
                           └────────────── spicy_backends ───► spicy_engine   (implements its Backend trait)
```

Below them are three small crates (added 2026-09-29, the rustc way). `spicy_span` and `spicy_index` depend on nothing. `spicy_errors` depends on `spicy_span` and `codespan-reporting`. `spicy_model` depends on all three, `spicy_lang` on `spicy_span` and `spicy_errors`, and `spicy_circuit` on `spicy_index`.

Outside `crates/`:

```
circuits/           example designs: .spicy netlists and .spl designs (ce_amp.spl first)
docs/               the rules for judging code and writing docs, and the glossary
docs/ecad/          design docs (README.md is the index), research/, archive/
.claude/commands/   Claude Code commands, for example /stage-review
externals/          reference repos (gitignored)
```

<a name="ownership"></a>
## Who owns what

| Crate | Owns | Must not know about |
|---|---|---|
| `spicy_circuit` | The simulator's input: one concrete circuit | Knobs, specs, syntax |
| `spicy_model` | The design as written: hierarchy, values with spreads, knobs, contracts | Syntax trees, simulators, affine math, results |
| `spicy_lang` | Text → `spicy_model`, with rustc-quality diagnostics | Simulators, the engine |
| `spicy_engine` | Design + contract → verdicts. Defines what it needs from a simulator (`Backend`, capability levels) and the result types | Any specific simulator, KLU, SPICE syntax |
| `spicy_backends` | Making simulators speak `Backend`: model → circuit for ours; model → SPICE text for ngspice | Specs, verdicts |

**The rules that keep this clean:**
- The engine never imports a simulator crate.
- The language never imports the engine.
- The simulator never imports a parser.

<a name="spicy-model"></a>
## What `spicy_model` holds

The design as the engineer wrote it, after name resolution and unit checks, with no syntax left: the hierarchical `Design`, and the flat `FlatDesign` with its knobs. The types and their reasons are in `docs/ecad/model.md`.

**Not in it:** syntax trees (`spicy_lang`), simulation results and verdicts (`spicy_engine`), layout geometry (the editor, later), and a simulator's circuit (`spicy_circuit`).

<a name="shortcuts"></a>
## Deliberate MVP shortcuts

| Shortcut | Why | When it goes away |
|---|---|---|
| The prelude (`Resistor`, `Npn`, `Power`, …) is defined in Rust, in `spicy_model::prelude` | The MVP language cannot define parts or signals yet. Flatten and lowering need the prelude, and they must not depend on the language (`docs/ecad/model.md`) | Once `signal` / `part` items exist |
| Slopes by nudging each knob (L0) | Works with any backend | Sensitivities in our simulator (M8) |
