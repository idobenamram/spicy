use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::tui::app::App;

use super::netlist_layout;
use super::utils::render_netlist_lines;

pub(super) fn draw_netlist(f: &mut Frame, area: Rect, app: &App) {
    let layout = netlist_layout(area);
    let hdr = layout.header;
    let body = layout.body;
    let inner = layout.inner;

    let mut hdr_text = format!(" {} ", app.path);
    if app.nvim.is_some() {
        hdr_text.push_str("[NVIM] ");
    }
    if let Some(warn) = &app.nvim_warning {
        hdr_text.push_str(&format!("[{warn}] "));
    }
    let hdr_style = if app.right_pane_focused() {
        Style::default()
    } else {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    };
    f.render_widget(
        Paragraph::new(hdr_text)
            .style(hdr_style)
            .block(Block::default().borders(Borders::ALL)),
        hdr,
    );

    let block = Block::default().borders(Borders::ALL);
    f.render_widget(block, body);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if let Some(nvim) = app.nvim.as_ref() {
        let show_cursor = app.left_pane_focused();
        nvim.render(f.buffer_mut(), inner, show_cursor);
    } else {
        let view = render_netlist_lines(
            &app.raw_netlist,
            app.scroll,
            inner.height as usize,
            &app.diags,
        );
        let wrap = Wrap { trim: false };
        f.render_widget(Paragraph::new(view).wrap(wrap), inner);
    }
}
