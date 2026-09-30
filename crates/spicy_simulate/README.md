# Spicy Simulate

MNA-based circuit analyzers for Spicy. Reads a `spicy_circuit` circuit (`Circuit` + `Params`) and computes results for various analyses.

## Analyses

- Operating point (DC): node voltages and currents through voltage sources/inductors
- DC sweep: sweep a single source (V or I) over a range
- AC small-signal: real 2×2 block expansion of the small-signal MNA (limited input phasor support)

## API sketch

```rust
use spicy_netlist::reader::{ParseOptions, lower, parse};
use spicy_simulate::{SimulationConfig, dc::simulate_op};

let mut options = ParseOptions::new_with_source("divider.spicy", netlist);
let lowered = lower(&parse(&mut options)?)?;
let op = simulate_op(&lowered.circuit, &lowered.params, &SimulationConfig::default())?;
```

## Example netlists

- `tests/op_dc/simple_resistor.spicy`
- `tests/op_dc/simple_voltage_source.spicy`
- `tests/op_dc/simple_inductor_capacitor.spicy`

Run tests:

```bash
cargo test -p spicy_simulate
```

## License

The overall project is MIT (see the repository `LICENSE`), but this crate includes
solver code derived from SuiteSparse (AMD/BTF/KLU) under BSD-3-Clause and
LGPL-2.1-or-later. See the repository `THIRD_PARTY_NOTICES.md`.


