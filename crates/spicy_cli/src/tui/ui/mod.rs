use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};

use crate::tui::app::App;

mod config;
mod help;
mod netlist;
mod output;
mod utils;

pub use utils::{LineDiagnostic, format_error_snippet};

pub fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(f.size());

    netlist::draw_netlist(f, chunks[0], app);
    output::draw_outputs(f, chunks[1], app);

    if app.show_config {
        config::draw_config(f, f.size(), app);
    } else if app.show_help {
        help::draw_help(f, f.size());
    }
}
