# Spicy 🌶️🌶️🌶️

Spicy is a small Rust project for  running basic circuit simulations using Modified Nodal Analysis (MNA).

![Tui example](assets/tui_example.png)

## Crates

- Netlists (SPICE reader and writer): see `crates/spicy_netlist` ([README](crates/spicy_netlist/README.md))
- Simulator: see `crates/spicy_simulate` ([README](crates/spicy_simulate/README.md))
- Simulator input (one flat, numeric circuit): `crates/spicy_circuit`
- CLI/TUI: see `crates/spicy_cli` ([README](crates/spicy_cli/README.md))
- Circuit language front-end (`.spl`): see `crates/spicy_lang` ([README](crates/spicy_lang/README.md))
- Design model: see `crates/spicy_model` ([README](crates/spicy_model/README.md))
- Shared by the stages: `crates/spicy_span` (source spans), `crates/spicy_index` (typed indices), `crates/spicy_errors` (diagnostics)

## Docs

- How the crates fit together: [`ARCHITECTURE.md`](ARCHITECTURE.md). The project's terms: [`docs/glossary.md`](docs/glossary.md).
- The design of the circuit language, the engine and the editor: [`docs/ecad/README.md`](docs/ecad/README.md). The current state is in the [roadmap](docs/ecad/roadmap.md#status).
- How to judge a code change: [`docs/code_quality.md`](docs/code_quality.md). How to write docs and comments: [`docs/writing_docs.md`](docs/writing_docs.md).

## Quickstart

The simulator needs the system OpenBLAS (Debian/Ubuntu: `sudo apt-get install libopenblas-dev`).

1) Run the TUI on a sample netlist

```bash
cargo run -p spicy_cli -- --tui crates/spicy_simulate/tests/op_dc/simple_resistor.spicy
```

## Testing

Run the test suites:

```bash
cargo test -p spicy_netlist
cargo test -p spicy_simulate
```

we use cargo-insta for snapshot testing in a lot of the parser tests. to update the snapshot use:
```bash
cargo insta review
```

The simulation result snapshots (`spicy_simulate`) keep full precision but compare numbers
with a small tolerance, so last-digit differences between machines don't fail them. When one
fails, the actual output is written next to it as `.snap.new`; to accept it use:
```bash
SPICY_UPDATE_SNAPSHOTS=1 cargo test -p spicy_simulate
```

Fuzzing support exists under `fuzz/` (requires `cargo-fuzz`).

Check the links in the docs and the doc references in code comments, with [lychee](https://github.com/lycheeverse/lychee) 0.24.2 (CI runs the same check):
```bash
scripts/check_links.sh
```

### Vibe Coding
A lot of the surrounding code in this project is vibe coded, like:
1. the binaries klu_mtx.rs klu_solve_cmp, some scripts
2. spicy_cli, visualizations

Those files are not for the faint of heart, human eyes should not lay eyes on them.

The parser and main simulation code are hand crafted by human intelligence,
and only assisted by ai. Those should be readable.

## TODO

The open items are in the roadmap's [follow-up tables](docs/ecad/roadmap.md#readme-follow-ups).

## License

MIT (see `LICENSE`).

This repository also includes solver code derived from SuiteSparse (AMD/BTF/KLU)
under BSD-3-Clause and LGPL-2.1-or-later; see `THIRD_PARTY_NOTICES.md`.