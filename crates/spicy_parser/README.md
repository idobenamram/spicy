# Spicy Parser

Netlist parser for Spicy. 

## Logical flow
- lexer (lexer.rs): tokenize the input
- Statement phase (statement_phase.rs): split tokens into statements with spans
- Include Libraries (libs_phase.rs): look for include and lib commands to add to statements
- Expression phase (expression_phase.rs): switch {} expressions with placeholders with ids
- Subcircuit phase (subcircuit_phase.rs): collect and expand subcircuits an parameters
- Instance parser (instance_parser.rs): parse the expanded instances and commands into a final Deck

## Errors and spans

Most parser errors include a span which shows the position of the error. The CLI/TUI can underline the exact range to help debugging.

## Tests

Run parser tests and snapshot checks:

```bash
cargo test -p spicy_parser
```


