use std::fs;

use clap::Parser;
use spicy_parser::Span;
use spicy_parser::parser::parse;
use spicy_simulate::{simulate, SimulateOptions};

use crate::tui::ui::LineDiagnostic; // kept for non-TUI mode

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

    if args.tui {
        let path = args.netlist.unwrap_or_else(|| {
            eprintln!("--tui requires a <netlist.spicy> argument");
            std::process::exit(1);
        });
        if let Err(e) = tui::run_tui(&path) {
            // try to gracefully restore terminal
            let _ = tui::term::restore_terminal();
            eprintln!("TUI error: {}", e);
            std::process::exit(1);
        }
        let _ = tui::term::restore_terminal();
        return;
    }

    let path = args.netlist.unwrap_or_else(|| {
        eprintln!("Missing <netlist.spicy> argument");
        std::process::exit(1);
    });

    let input = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {}", path, e);
        std::process::exit(1);
    });

    match parse(&input) {
        Ok(deck) => {
            let base = std::path::Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "spicy".to_string());
            let opts = SimulateOptions { write_raw: args.raw, output_base: Some(base) };
            simulate(deck, opts);
        }
        Err(e) => {
            eprintln!("Parse error: {}", e);
            if let Some(span) = e.error_span() {
                eprintln!("");
                render_error_snippet(&input, span);
            }
            std::process::exit(2);
        }
    }
}

fn render_error_snippet(src: &str, span: Span) {
    let Some(ld) = LineDiagnostic::new(src, span) else {
        return;
    };
    let prefix = &src[ld.line_start..span.start];
    let highlight = &src[span.start..=span.end];
    let line = &src[ld.line_start..ld.line_end];
    let col = prefix.chars().count();
    let width = highlight.chars().count().max(1);

    // optionally include line number
    let line_no = src[..ld.line_start].chars().filter(|&c| c == '\n').count() + 1;
    eprintln!("{:>4} | {}", line_no, line);
    let underline = "~".repeat(width);
    eprintln!(
        "     | {:space$}\x1b[31m{}\x1b[0m",
        "",
        underline,
        space = col
    );
}
