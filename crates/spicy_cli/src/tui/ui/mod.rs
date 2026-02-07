use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders};

use crate::tui::app::App;

mod config;
mod help;
mod netlist;
mod output;
mod utils;

pub use utils::format_error_snippet;

#[derive(Clone, Copy, Debug)]
pub struct NetlistLayout {
    pub header: Rect,
    pub body: Rect,
    pub inner: Rect,
}

pub fn main_layout(area: Rect) -> [Rect; 2] {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);
    [chunks[0], chunks[1]]
}

pub fn netlist_layout(left: Rect) -> NetlistLayout {
    let [header, body] = utils::split_v(left, 3);
    let inner = Block::default().borders(Borders::ALL).inner(body);
    NetlistLayout { header, body, inner }
}

pub fn ui(f: &mut Frame, app: &App) {
    let [left, right] = main_layout(f.area());

    netlist::draw_netlist(f, left, app);
    output::draw_outputs(f, right, app);

    if app.is_config() {
        config::draw_config(f, f.area(), app);
    } else if app.is_help() {
        help::draw_help(f, f.area());
    }
}
