use std::env;
use std::fs;

use spicy_parser::parser::parse;
use spicy_parser::Span;
use spicy_simulate::simulate; // kept for non-TUI mode

mod tui;

fn main() {
    // Simple arg parsing: `--tui <file>` enables TUI; otherwise legacy CLI
    let args = env::args().skip(1).collect::<Vec<_>>();

    if !args.is_empty() && args[0] == "--tui" {
        let path = args.get(1).cloned().unwrap_or_else(|| {
            eprintln!("Usage: spicy_cli --tui <netlist.spicy>");
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

    // Legacy non-TUI path
    let path = args.get(0).cloned().unwrap_or_else(|| {
        eprintln!("Usage: spicy_cli <netlist.spicy>  or  spicy_cli --tui <netlist.spicy>");
        std::process::exit(1);
    });

    let input = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {}", path, e);
        std::process::exit(1);
    });

    match parse(&input) {
        Ok(deck) => simulate(deck),
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
    let len = src.len();
    if len == 0 { return; }
    let start = span.start.min(len.saturating_sub(1));
    let end = span.end.min(len.saturating_sub(1));
    if start > end { return; }

    // find line bounds
    let line_start = src[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = src[end + 1..].find('\n').map(|i| end + 1 + i).unwrap_or(len);
    let line = &src[line_start..line_end];

    // compute columns by char count
    let prefix = &src[line_start..start];
    let highlight = &src[start..=end];
    let col = prefix.chars().count();
    let width = highlight.chars().count().max(1);

    // optionally include line number
    let line_no = src[..line_start].chars().filter(|&c| c == '\n').count() + 1;
    eprintln!("{:>4} | {}", line_no, line);
    let underline = "~".repeat(width);
    eprintln!("     | {:space$}\x1b[31m{}\x1b[0m", "", underline, space = col);
}

