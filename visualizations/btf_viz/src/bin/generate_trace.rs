use spicy_simulate::solver::matrix::builder::MatrixBuilder;
use std::path::PathBuf;

// Bring in the `code` module tree so paths like `crate::code::...` resolve
#[path = "../code/mod.rs"]
mod code;

use crate::code::recorder::Recorder;
use crate::code::btf_max_transversal::btf_max_transversal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a test matrix - using the chain example from tests
    // Column adjacency (by rows):
    // c0: r0
    // c1: r0, r1
    // c2: r1, r2
    // c3: r2, r3
    // c4: r3, r4
    let mut builder = MatrixBuilder::new(5, 5);
    builder.push(0, 0, 1.0)?;
    builder.push(1, 0, 1.0)?;
    builder.push(1, 1, 1.0)?;
    builder.push(2, 1, 1.0)?;
    builder.push(2, 2, 1.0)?;
    builder.push(3, 2, 1.0)?;
    builder.push(3, 3, 1.0)?;
    builder.push(4, 3, 1.0)?;
    builder.push(4, 4, 1.0)?;
    
    let matrix = builder.build_csc()?;
    
    // Create recorder
    let trace_path = PathBuf::from("assets/traces/sample_5x5.json");
    let mut recorder = Recorder::new(&trace_path);
    
    // Run algorithm with recorder
    let (_matches, _permutations) = btf_max_transversal(&matrix, &mut recorder);
    
    // Write trace file
    recorder.flush()?;
    
    println!("Trace written to: {}", trace_path.display());
    
    Ok(())
}
