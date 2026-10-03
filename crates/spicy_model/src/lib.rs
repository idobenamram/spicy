//! The design model (roadmap §2.4): what the engine, the backends and the editor read.
//! No syntax trees, no simulators (roadmap §2.3).
//!
//! Design: `docs/ecad/model.md`.
//! - [`units`]: dimensions, SI values, spreads.
//! - [`prelude`]: the standard part kinds, signal types and value types.
//! - [`design`]: the `Design`, each block once, as written.
//! - [`measure`]: what a contract's measures mean, and their types.
//! - [`flat`]: the `FlatDesign`, one root with every placement expanded, and the `Flat`
//!   handle that reads it with its `Design`.
//! - [`flatten`]: `Design` → one `FlatDesign` per root.

pub mod design;
pub mod flat;
pub mod flatten;
pub mod measure;
pub mod prelude;
pub mod units;
