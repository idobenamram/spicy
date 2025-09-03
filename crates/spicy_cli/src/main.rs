use std::env;
use std::fs;

use spicy_parser::parser::parse;
use spicy_simulate::simulate;

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: spicy_cli <netlist.spicy>");
        std::process::exit(1);
    });

    let input = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {}", path, e);
        std::process::exit(1);
    });

    println!("input: {}", input);
    let deck = parse(&input);
    simulate(deck);
}


