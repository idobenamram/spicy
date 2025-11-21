use spicy_simulate::solver::matrix::builder::MatrixBuilder;
use std::path::PathBuf;

// Bring in the `code` module tree so paths like `crate::code::...` resolve
#[path = "../code/mod.rs"]
mod code;

use crate::code::btf_max_transversal::btf_max_transversal;
use crate::code::recorder::Recorder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a test matrix that triggers backtracking in the augmenting-path search.
    // Column adjacency (rows per column), ordered to explore a failing branch first:
    // c0: r0, r2
    // c1: r0, r1
    // c2: r1, r2, r3
    // c3: r3, r2, r4  (r3 first so col3 matches r3 earlier; r4 reachable only via a deeper branch)
    // c4: r0, r1, r2  (no direct access to r4; forces exploring matched columns and backtracking)
    let mut builder = MatrixBuilder::new(5, 5);
    // c0
    builder.push(0, 0, 1.0)?;
    builder.push(2, 0, 1.0)?;
    // c1
    builder.push(0, 1, 1.0)?;
    builder.push(1, 1, 1.0)?;
    // c2
    builder.push(1, 2, 1.0)?;
    builder.push(2, 2, 1.0)?;
    builder.push(3, 2, 1.0)?;
    // c3
    builder.push(3, 3, 1.0)?;
    builder.push(2, 3, 1.0)?;
    builder.push(4, 3, 1.0)?;
    // c4
    builder.push(0, 4, 1.0)?;
    builder.push(1, 4, 1.0)?;
    builder.push(2, 4, 1.0)?;

    let matrix = builder.build_csc()?;

    // Create recorder
    let trace_path = PathBuf::from("visualizations/btf_viz/assets/traces/sample_5x5.json");
    let mut recorder = Recorder::new(&trace_path);

    // Run algorithm with recorder
    let (_matches, _permutations) = btf_max_transversal(&matrix, &mut recorder);

    // Write trace file
    recorder.flush()?;

    println!("Trace written to: {}", trace_path.display());

    Ok(())
}
