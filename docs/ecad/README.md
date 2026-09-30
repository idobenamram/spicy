# ECAD Editor Design Docs

Design documents for the AI-native schematic and simulation editor built on spicy.

## Reading order

1. **[vision.md](vision.md)**: what we're building and why.
2. **[walkthrough.md](walkthrough.md)**: one amplifier followed through the whole engine, with real numbers. The best introduction to how the engine thinks.
3. **[engine.md](engine.md)**: the engine design (current: v4, on ngspice: every corner, inside-the-box guards, the 3σ-point search).
   - **[engine_plan.md](engine_plan.md)**: the engine MVP plan on ngspice, from three research rounds (accepted 2026-09-27: decisions D-A to D-F).
   - **[engine_types.md](engine_types.md)**: the M3a design note: the engine's types, the `Backend` trait, stages and driver, run table, verdict functions, report schema, the ngspice worker, and the test suite layout (draft for review).
4. **[language.md](language.md)**: the circuit language (v0.1).
   - **[grammar.md](grammar.md)**: the exact MVP grammar: tokens, keywords, EBNF, operator precedence, unit literals, syntax errors.
   - **[model.md](model.md)**: elaboration, from syntax tree to flat design: `Design`, `FlatDesign`, knobs, nets, units, checks; compared with compilers, hardware elaborators, Modelica and circuit tools.
   - **[ast.md](ast.md)**: the syntax tree and parser: storage, error nodes and recovery, what the parser rejects, tests; compared with Spade, atopile, rustc, rust-analyzer and Zig.
   - **[lexer.md](lexer.md)**: the lexer design: two passes, token layout, errors, and how it's tested, compared with Spade, atopile, Rust and Zig.
   - **[stage_review.md](stage_review.md)**: the cleanup process every stage goes through (a reusable prompt): the readability standard, the subagents (reference researcher, flow reviewers, type auditor), and the checks.
5. **[specs.md](specs.md)**: spec concepts (contracts, confidence, automatic checks). The syntax lives in language.md §8.
6. **[pipeline.md](pipeline.md)**: the data pipeline: which struct holds a circuit at each stage, which crate owns it, when it's built, and why it exists.
   - **[circuit.md](circuit.md)**: the circuit data model (`spicy_circuit`): today's flow traced through the code, and each design choice checked against ngspice, Xyce, Gnucap, OSDI/VACASK and compilers.
   - **[netlist_writer.md](netlist_writer.md)**: the SPICE netlist writer (M1f's numeric export): names, numbers, what gets written, and the round-trip test (decided 2026-09-30).
7. **[roadmap.md](roadmap.md)**: what gets built, in what order, and how each step is reviewed.

## Reference

- **[bibliography.md](bibliography.md)**: verified references. Other docs cite keys like `[AGW94]`.
- **[research/](research/)**: the research round behind engine v3 and language v0.1.
  - [research/synthesis.md](research/synthesis.md): the merged findings. Start here.
  - Engine stress tests:
    - [engine_power.md](research/engine_power.md)
    - [engine_precision_analog.md](research/engine_precision_analog.md)
    - [engine_mixed_signal.md](research/engine_mixed_signal.md)
    - [engine_method_redteam.md](research/engine_method_redteam.md)
  - Engine in depth (round 2, toward the M3 plan):
    - [engine_synthesis.md](research/engine_synthesis.md): the merged findings, decisions D1–D10 and the revised M3 plan. Start here.
    - [engine_flows.md](research/engine_flows.md): every flow from an edit to a verdict, with the data, caching and cost of each
    - [agent_flows.md](research/agent_flows.md): how the AI agent handles each user request through the engine, and the tool interface it needs
    - [engine_math_worst_case.md](research/engine_math_worst_case.md): the `worst_case` loop as pseudocode, against the 256-corner answer key
    - [engine_math_statistical.md](research/engine_math_statistical.md): `sigma(k)`, distributions, yield and Monte Carlo checks
    - [engine_math_affine_measures.md](research/engine_math_affine_measures.md): affine forms, contributors, measures and slopes by nudging
    - [engine_math_guards.md](research/engine_math_guards.md): adversarial circuits, the safety net, guards and the verdict table
  - Engine positions (round 3, on ngspice; each with a prototype engine and agent flow diagrams):
    - [engine_position_minimal.md](research/engine_position_minimal.md): the smallest trustworthy engine; enumerate corners up to 12 knobs
    - [engine_position_soundness.md](research/engine_position_soundness.md): a verdict must never lie; the adversarial suite on ngspice
    - [engine_position_agent.md](research/engine_position_agent.md): the engine as the AI agent's instrument; every agent flow end to end
    - [engine_position_scale.md](research/engine_position_scale.md): 8 to 302 knobs; cones, worker pools, transient cost
    - [engine_position_alternatives.md](research/engine_position_alternatives.md): the loop against screening, response surfaces and sampling
  - After the MVP (round 4: next circuits through the current design, on ngspice):
    - [next_synthesis.md](research/next_synthesis.md): the findings by layer (language, model, export, engine, types, backend, report), each with problem, scenario, impact, fix; the seams to add now; the circuit ladder. Start here.
    - [next_precision_analog.md](research/next_precision_analog.md), [next_power.md](research/next_power.md), [next_mixed_signal.md](research/next_mixed_signal.md), [next_time_domain.md](research/next_time_domain.md), [next_boards.md](research/next_boards.md)
  - Contracts rethought (round 5: specs, setups, loads, faults, hierarchy):
    - [contract_options.md](research/contract_options.md): the three options compared, the common core, a possible hybrid. Start here.
    - Options: [contract_option_a_datasheet.md](research/contract_option_a_datasheet.md), [contract_option_b_testbench.md](research/contract_option_b_testbench.md), [contract_option_c_interface.md](research/contract_option_c_interface.md)
    - Research: [contract_references.md](research/contract_references.md), [contract_ee_practice.md](research/contract_ee_practice.md) (sources, loads and setups for non-specialists; the reference board), [contract_hierarchy.md](research/contract_hierarchy.md) (reusing a child's results), [contract_tool_gaps.md](research/contract_tool_gaps.md) (what today's tools can't do, G1–G19, faults)
  - Block and contract syntax (round 6):
    - [contract_syntax_v5.md](research/contract_syntax_v5.md): **final for now** (2026-09-30), the target for the language implementation: rules, full example, MVP subset, migration order. Start here.
    - [contract_syntax_v4.md](research/contract_syntax_v4.md) and its reviews: [consistency](research/contract_v4_review_consistency.md), [novice](research/contract_v4_review_novice.md), [implementation](research/contract_v4_review_implementation.md)
    - [contract_syntax_v3.md](research/contract_syntax_v3.md): the previous draft, with what changed from draft 2 and why.
    - [contract_syntax_v2.md](research/contract_syntax_v2.md) and its reviews: [semantics](research/contract_v2_review_semantics.md), [syntax](research/contract_v2_review_syntax.md), [engine](research/contract_v2_review_engine.md), [usability](research/contract_v2_review_usability.md)
  - When and how specs run (round 7):
    - [runs_language.md](research/runs_language.md): three options and the recommendation ("gate by meaning, schedule by cost"), the result store, CI and the agent. Start here.
    - [runs_cost_tiers.md](research/runs_cost_tiers.md): measured costs on ngspice, the cost estimator, the tiers, incremental re-checks
    - [runs_references.md](research/runs_references.md): how software and hardware verification decide what runs when
  - Data pipeline:
    - [staged_ir_survey.md](research/staged_ir_survey.md): how compilers and circuit simulators pass data between stages
    - [circuit_redteam.md](research/circuit_redteam.md): our code, red-teamed and measured
    - [data_model_survey.md](research/data_model_survey.md): how ngspice, Xyce, Gnucap, OSDI/VACASK, MLIR and data-oriented compilers lay out circuit data, with citations
    - Temperatures, solver options and names in `spicy_circuit`, checked before they were built (the `Conditions` struct they review became `Params.temp` plus a TNOM per model, `engine_plan.md` §5.2):
      - [circuit_conditions_simulators.md](research/circuit_conditions_simulators.md): how ngspice, Xyce, Gnucap and VACASK hold the circuit temperature, TNOM, options and names
      - [circuit_conditions_compilers.md](research/circuit_conditions_compilers.md): where compilers and modeling tools keep conditions, options and names
      - [circuit_conditions_engine.md](research/circuit_conditions_engine.md): what the engine and the exporter need from `spicy_circuit`
  - Language proposals:
    - [language_core.md](research/language_core.md)
    - [language_specs.md](research/language_specs.md)
    - [language_editor_mapping.md](research/language_editor_mapping.md)
  - Elaboration (behind `model.md`):
    - [model_compilers.md](research/model_compilers.md): rustc, rust-analyzer and Spade (name resolution, stable ids, lowering)
    - [model_hdl.md](research/model_hdl.md): slang, Yosys and FIRRTL (elaboration, flattening, net merging)
    - [model_modelica.md](research/model_modelica.md): the Modelica spec and OpenModelica (instantiation, connection sets, units)
    - [model_circuit_tools.md](research/model_circuit_tools.md): atopile, ngspice, Xyce, KiCad and our SPICE parser (nets, naming, identity)
    - [model_units.md](research/model_units.md): uom, F#, Unitful, pint (dimensions, temperature, %, dB)
    - These reports are kept as written. Where they disagree with the main docs, the main docs win. One known case: they propose a `logos` lexer and a lossless `rowan` tree; we chose a hand-written lexer and a typed AST with spans plus the full token list (roadmap.md §4.4).
- **[archive/](archive/)**: superseded versions, kept for history ([engine_v1.md](archive/engine_v1.md), [engine_v2.md](archive/engine_v2.md), [engine_v3.md](archive/engine_v3.md)).

## Conventions

- Paths to code (`crates/spicy_simulate/src/…`) are relative to the repository root.
- Paths to other docs are relative to the doc they appear in.
- Section references look like `engine.md §5.2`.
