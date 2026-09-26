# spicy_lang

The language front-end: `.spl` text → tokens (→ syntax tree, later), with rustc-style diagnostics.

**Owns:** turning text into `spicy_model`, with diagnostics. **Must not know about:** simulators or the engine (`docs/ecad/roadmap.md` §2.3).

Today it has the lexer (roadmap M1b, design in `docs/ecad/lexer.md`):

- `lexer::scan`: pass 1. Text → `Tokens`. Never fails; every byte, including whitespace and comments, lands in exactly one token.
- `lexer::check`: pass 2. Every problem, as data with a suggested fix, rendered through `codespan-reporting`.
- `lexer::decode_quantity`: `47k`, `4k7`, `1uF`, `10°C` → value and unit.

Tests:

- `cargo test -p spicy_lang`: unit tests, the case files in `test_data/lexer/{ok,err}` (snapshots under `src/lexer/snapshots`, reviewed with `cargo insta review`), and the invariants on 10 000 random inputs.
- Speed: `cargo test -p spicy_lang --release -- --ignored --nocapture speed`.
- Fuzzing (needs a C++ compiler and cargo-fuzz): `cargo +nightly fuzz run fuzz_spicy_lang_lexer` from `fuzz/`. The repo pins stable, hence `+nightly`.
