# ECAD Editor Design Docs

Design documents for the AI-native schematic and simulation editor built on spicy.

## Reading order

1. **[vision.md](vision.md)**: what we're building and why.
2. **[walkthrough.md](walkthrough.md)**: one amplifier followed through the whole engine, with real numbers. The best introduction to how the engine thinks.
3. **[engine.md](engine.md)**: the engine design (current: v3).
4. **[language.md](language.md)**: the circuit language (v0.1).
   - **[grammar.md](grammar.md)**: the exact MVP grammar: tokens, keywords, EBNF, operator precedence, unit literals, syntax errors.
   - **[ast.md](ast.md)**: the syntax tree and parser: storage, error nodes and recovery, what the parser rejects, tests; compared with Spade, atopile, rustc, rust-analyzer and Zig.
   - **[lexer.md](lexer.md)**: the lexer design: two passes, token layout, errors, and how it's tested, compared with Spade, atopile, Rust and Zig.
5. **[specs.md](specs.md)**: spec concepts (contracts, confidence, automatic checks). The syntax lives in language.md §8.
6. **[pipeline.md](pipeline.md)**: the data pipeline: which struct holds a circuit at each stage, which crate owns it, when it's built, and why it exists.
   - **[circuit.md](circuit.md)**: the circuit data model (`spicy_circuit`): today's flow traced through the code, and each design choice checked against ngspice, Xyce, Gnucap, OSDI/VACASK and compilers.
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
  - Data pipeline:
    - [staged_ir_survey.md](research/staged_ir_survey.md): how compilers and circuit simulators pass data between stages
    - [circuit_redteam.md](research/circuit_redteam.md): our code, red-teamed and measured
    - [data_model_survey.md](research/data_model_survey.md): how ngspice, Xyce, Gnucap, OSDI/VACASK, MLIR and data-oriented compilers lay out circuit data, with citations
  - Language proposals:
    - [language_core.md](research/language_core.md)
    - [language_specs.md](research/language_specs.md)
    - [language_editor_mapping.md](research/language_editor_mapping.md)
    - These reports are kept as written. Where they disagree with the main docs, the main docs win. One known case: they propose a `logos` lexer and a lossless `rowan` tree; we chose a hand-written lexer and a typed AST with spans plus the full token list (roadmap.md §4.4).
- **[archive/](archive/)**: superseded versions, kept for history ([engine_v1.md](archive/engine_v1.md), [engine_v2.md](archive/engine_v2.md)).

## Conventions

- Paths to code (`crates/spicy_simulate/src/…`) are relative to the repository root.
- Paths to other docs are relative to the doc they appear in.
- Section references look like `engine.md §5.2`.
