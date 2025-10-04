# Spicy 🌶️🌶️🌶️

Spicy is a small Rust project for  running basic circuit simulations using Modified Nodal Analysis (MNA).

![Tui example](assets/tui_example.png)

## Crates

- Parser: see `crates/spicy_parser` ([README](crates/spicy_parser/README.md))
- Simulator: see `crates/spicy_simulate` ([README](crates/spicy_simulate/README.md))
- CLI/TUI: see `crates/spicy_cli` ([README](crates/spicy_cli/README.md))

## Quickstart

1) Run the TUI on a sample netlist

```bash
cargo run -p spicy_cli -- --tui crates/spicy_simulate/tests/simple_resistor.spicy
```

## Testing

Run the test suites:

```bash
cargo test -p spicy_parser
cargo test -p spicy_simulate
```

we use cargo-insta for snapshot testing in a lot of the parser tests. to update the snapshot use:
```bash
cargo insta review
```

Fuzzing support exists under `fuzz/` (requires `cargo-fuzz`).


# TODO:
- [ ] output raw files
- [ ] suport libs and models in parser
- [ ] add diodes
- [ ] add BJT transistor


## License

MIT