use std::fs;

use clap::Parser;
use spicy_parser::{ParseOptions, parse};
use spicy_simulate::{SimulationConfig, simulate};

use crate::tui::ui::format_error_snippet; // kept for non-TUI mode

mod tui;

#[derive(Parser, Debug)]
#[command(name = "spicy_cli", about = "Spicy circuit simulator", version)]
struct Args {
    /// Run interactive TUI
    #[arg(long)]
    tui: bool,

    /// Write LTSpice .raw output alongside input name
    #[arg(long)]
    raw: bool,

    /// Input netlist file
    #[arg(value_name = "NETLIST", required_unless_present = "tui")]
    netlist: Option<String>,
}

fn main() {
    let args = Args::parse();

    let path = args.netlist.unwrap_or_else(|| {
        let message = if args.tui {
            "--tui requires a <netlist.spicy> argument"
        } else {
            "Missing <netlist.spicy> argument"
        };
        eprintln!("{message}");
        std::process::exit(1);
    });

    if args.tui {
        let result = tui::run_tui(&path);
        // try to gracefully restore terminal
        let _ = tui::term::restore_terminal();
        if let Err(e) = result {
            eprintln!("{}", e);
            std::process::exit(1);
        }
        return;
    }

    let input = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {}", path, e);
        std::process::exit(1);
    });
    let mut parser_options = ParseOptions::new_with_source(std::path::Path::new(&path), input);

    match parse(&mut parser_options) {
        Ok(deck) => {
            let base = std::path::Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "spicy".to_string());
            let sim_config = SimulationConfig {
                write_raw: args.raw,
                output_base: Some(base),
                ..Default::default()
            };
            if let Err(e) = simulate(deck, sim_config) {
                eprintln!("Simulation error: {}", e);
                std::process::exit(3);
            }
        }
        Err(e) => {
            eprintln!("Parse error: {}", e);
            if let Some(span) = e.error_span() {
                let input_path = parser_options.source_map.get_path(span.source_index);
                eprintln!();
                let input = fs::read_to_string(input_path).unwrap_or_else(|e| {
                    eprintln!("Failed to read {}: {}", input_path.display(), e);
                    std::process::exit(1);
                });
                if let Some(snippet) = format_error_snippet(&input, span) {
                    eprint!("{snippet}");
                }
            }
            std::process::exit(2);
        }
    }
}
