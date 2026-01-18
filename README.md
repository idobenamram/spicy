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

### Vibe Coding
A lot of the surrounding code in this project is vibe coded, like:
1. the binaries klu_mtx.rs klu_solve_cmp, some scripts
2. spicy_cli, visualizations

Those files are not for the faint of heart, human eyes should not lay eyes on them.

The parser and main simulation code are hand crafted by human intelligence,
and only assisted by ai. Those should be readable.

# TODO:

## Parser
- [x] output raw files
- [x] support libs in parser
- [x] support models in parser
- [ ] add diodes
- [ ] add BJT transistor


## Simulation
- [x] implementation of KLU
- [x] refactor errors and metrics in klu
- [x] test KLU implementation
      - [x] make sure the output is bit exact to c
      - [x] make sure solve close 
      - [x] make sure unroll optimization doesn't lose numerical stability
- [x] hook up to simulation code
- [x] cleanup device function use
- [ ] implement gmin

## KLU
- [ ] implement klu statistics and use them to know when to fully factorize the matrix again.
- [ ] implement bench marks for all algorithms and the full algorithms
      - [x] analyze
- [ ] make sure singular matricies work (when not using halt_if_singular)
- [ ] refactor the functions and structs of KLU (mostly numeric) to something a little nicer
- [ ] support KLU complex?

### Optimizations
- [ ] create spicyVec for boundary checks

## visualizations
- [ ] merge the recorder macro
- [ ] generate nice visualizations for btf and amd

## License

MIT (see `LICENSE`).

This repository also includes solver code derived from SuiteSparse (AMD/BTF/KLU)
under BSD-3-Clause and LGPL-2.1-or-later; see `THIRD_PARTY_NOTICES.md`.