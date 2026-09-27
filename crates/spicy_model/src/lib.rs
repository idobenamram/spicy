//! The design model (roadmap §2.4): what the engine, the backends and the editor read.
//! No syntax trees, no simulators (roadmap §2.3).
//!
//! Design: `docs/ecad/model.md`.
//! - [`units`]: dimensions, SI values, spreads.
//! - [`prelude`]: the standard part kinds and signal types.
//! - [`design`]: the `Design`, each block once, as written.
//! - [`span`]: byte ranges in the source, shared with the language front end.

pub mod design;
pub mod prelude;
pub mod span;
pub mod units;
