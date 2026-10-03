# spicy_model

The design model: what the engine, the backends and the editor read ([what `spicy_model` holds](../../ARCHITECTURE.md#spicy-model)). It knows no syntax trees and no simulators ([who owns what](../../ARCHITECTURE.md#ownership)).

Design: `docs/ecad/model.md`.

- `units`: dimensions (exponents of s, m, kg, A, K, rad), quantities in SI, spreads (`± 1%`, `± 50mV`, `100..=300`).
- `prelude`: the standard part kinds (pins and fields, each field with the unit it expects) and signal types.
- `design`: the `Design`, each block once as written (the folded form), with positions in a separate `DesignSourceMap`.

Built by `spicy_lang::resolve`. Flattening (`FlatDesign`, knobs) comes next.
