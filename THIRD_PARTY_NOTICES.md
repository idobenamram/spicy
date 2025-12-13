# Third-party notices (SuiteSparse)

This repository is primarily licensed under the MIT License (see `LICENSE`).

However, parts of the sparse solver implementation are derived from **SuiteSparse**
by Timothy A. Davis and collaborators, and are licensed under their respective
licenses. Those files **retain their original licenses** and you must comply
with them when redistributing source or binaries.

Upstream: `https://github.com/DrTimothyAldenDavis/SuiteSparse/tree/dev`

## AMD (Approximate Minimum Degree) — BSD-3-Clause

SuiteSparse AMD is licensed under **BSD-3-Clause** (see `LICENSES/BSD-3-Clause.txt`).

Upstream header (example): `AMD/Source/amd_1.c` (`SPDX-License-Identifier: BSD-3-clause`).

Files in this repository that are derived from / based on AMD:

- `crates/spicy_simulate/src/solver/amd.rs` (Rust port based on SuiteSparse AMD sources)
- `crates/spicy_simulate/src/solver/aat.rs` (Rust port based on SuiteSparse AMD `amd_aat`)
- `crates/spicy_simulate/src/solver/klu/amd.rs` (uses the AMD implementation/ports)

Copyright:
- AMD, Copyright (c) 1996-2022, Timothy A. Davis, Patrick R. Amestoy, and Iain S. Duff.

## BTF (Block Triangular Form / SCC / MAXTRANS) — LGPL-2.1-or-later

SuiteSparse BTF is licensed under **LGPL-2.1-or-later** (see `LICENSES/LGPL-2.1-or-later.txt`).

Upstream header (example): `BTF/Include/btf.h` (`SPDX-License-Identifier: LGPL-2.1+`).

Files in this repository that are derived from / based on BTF:

- `crates/spicy_simulate/src/solver/btf_scc.rs` (Rust implementation based on SuiteSparse BTF)
- `crates/spicy_simulate/src/solver/btf_max_transversal.rs` (Rust implementation based on SuiteSparse BTF)
- `crates/spicy_simulate/src/solver/klu/btf.rs` (BTF-related KLU preprocessing)

Copyright:
- BTF, Copyright (c) 2004-2024, University of Florida. Author: Timothy A. Davis.

## KLU — LGPL-2.1-or-later

SuiteSparse KLU is licensed under **LGPL-2.1-or-later** (see `LICENSES/LGPL-2.1-or-later.txt`).

Upstream header (example): `KLU/Source/klu.c` (`SPDX-License-Identifier: LGPL-2.1+`).

Files in this repository that are derived from / based on KLU:

- `crates/spicy_simulate/src/solver/klu/` (Rust implementation + vendored upstream C headers/sources)
  - Rust ports / adaptations in the same directory (tagged with SPDX headers in this repo):
    - `crates/spicy_simulate/src/solver/klu/analyze.rs`
    - `crates/spicy_simulate/src/solver/klu/factor.rs`
    - `crates/spicy_simulate/src/solver/klu/kernel.rs`
    - `crates/spicy_simulate/src/solver/klu/scale.rs`
    - `crates/spicy_simulate/src/solver/klu/solve.rs`
    - `crates/spicy_simulate/src/solver/klu/mod.rs`

Copyright:
- KLU, Copyright (c) 2004-2024, University of Florida.
  Authors: Timothy A. Davis and Ekanathan Palamadai.

## Practical note for redistributing binaries

If you redistribute binaries that include LGPL-licensed components (BTF/KLU),
ensure you comply with LGPL-2.1-or-later requirements. This repository includes
the corresponding license text and the full corresponding source for the
LGPL-covered portions.


