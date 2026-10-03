//! What every bench file shares: the tool settings of its groups.

// Each bench file is a crate of its own that uses only some of these.
#![allow(dead_code)]

use gungraun::prelude::*;
use gungraun::{Callgrind, Dhat, DhatMetric, EventKind, FlamegraphConfig};

/// A stage on the typical design: Callgrind's instructions and estimated cycles (cache
/// simulation on, gungraun's default) with a flame graph of each (and a differential
/// one against `--baseline`), and DHAT's allocations and peak bytes.
pub fn typical() -> LibraryBenchmarkConfig {
    let mut callgrind = Callgrind::default();
    callgrind
        .format([EventKind::Ir, EventKind::EstimatedCycles])
        .flamegraph(FlamegraphConfig::default());
    let mut dhat = Dhat::default();
    dhat.format([
        DhatMetric::TotalBlocks,
        DhatMetric::TotalBytes,
        DhatMetric::AtTGmaxBytes,
    ]);
    let mut config = LibraryBenchmarkConfig::default();
    config.tool(callgrind).tool(dhat);
    config
}

/// What a re-parse or re-elaboration throws away: Callgrind only (DHAT sees no
/// allocation made inside, so it reports nothing).
pub fn drop() -> LibraryBenchmarkConfig {
    let mut callgrind = Callgrind::default();
    callgrind.format([EventKind::Ir, EventKind::EstimatedCycles]);
    let mut config = LibraryBenchmarkConfig::default();
    config.tool(callgrind);
    config
}

/// One shape at two sizes: only the instructions are compared, so no cache simulation
/// (it about halves the run time).
pub fn scaling() -> LibraryBenchmarkConfig {
    let mut callgrind = Callgrind::with_args(["--cache-sim=no"]);
    callgrind.format([EventKind::Ir]);
    let mut config = LibraryBenchmarkConfig::default();
    config.tool(callgrind);
    config
}
