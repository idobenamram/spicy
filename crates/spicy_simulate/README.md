# Spicy Simulate

MNA-based circuit analyzers for Spicy. Consumes a `spicy_parser::parser::Deck` and computes results for various analyses.

## Analyses

- Operating point (DC): node voltages and currents through voltage sources/inductors
- DC sweep: sweep a single source (V or I) over a range
- AC small-signal: real 2×2 block expansion of the small-signal MNA (limited input phasor support)

## API sketch

```rust
use spicy_parser::parser::parse;
use spicy_simulate::{simulate_op, simulate_dc, simulate_ac};

let deck = parse(include_str!("../tests/simple_resistor.spicy"))?;
let op = simulate_op(&deck);
println!("OP voltages: {:?}", op.voltages);

// For DC sweep, extract the .DC command from the deck
// let dc_res = simulate_dc(&deck, &dc_cmd);

// For AC, call simulate_ac(&deck, &ac_cmd)
```

## Example netlists

- `tests/simple_resistor.spicy`
- `tests/simple_voltage_source.spicy`
- `tests/simple_inductor_capacitor.spicy`

Run tests:

```bash
cargo test -p spicy_simulate
```


