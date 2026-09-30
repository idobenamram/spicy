# Spicy Netlist

SPICE netlists for Spicy. The reader turns a netlist into a `spicy_circuit::Lowered`, the form the simulator and the exporters read.

## Reader (`src/reader/`)

- lexer (lexer.rs): tokenize the input
- Statement phase (statement_phase.rs): split tokens into statements with spans
- Include Libraries (libs_phase.rs): look for include and lib commands to add to statements
- Expression phase (expression_phase.rs): switch {} expressions with placeholders with ids
- Subcircuit phase (subcircuit_phase.rs): collect and expand subcircuits an parameters, also collect .model commands and store them for instance parser
- Instance parser (instance_parser.rs): parse the expanded instances and commands into a final Deck
- Lowering (lower.rs): apply SPICE's rules to the Deck, giving a `spicy_circuit::Lowered`

## Errors and spans

Most reader errors include a span which shows the position of the error. The CLI/TUI can underline the exact range to help debugging.

## Tests

Run the tests and snapshot checks:

```bash
cargo test -p spicy_netlist
```
