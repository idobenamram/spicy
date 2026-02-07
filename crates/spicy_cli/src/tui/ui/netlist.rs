use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::tui::app::App;

use super::utils::{render_netlist_lines, split_v};

pub(super) fn draw_netlist(f: &mut Frame, area: Rect, app: &App) {
    let [hdr, body] = split_v(area, 3);

    let mut hdr_text = format!(" {} ", app.path);
    if app.nvim.is_some() {
        hdr_text.push_str("[NVIM] ");
    }
    if let Some(warn) = &app.nvim_warning {
        hdr_text.push_str(&format!("[{warn}] "));
    }
    let hdr_style = if app.focus_right {
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
    let inner = block.inner(body);
    f.render_widget(block, body);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if let Some(nvim) = app.nvim.as_ref().filter(|state| state.is_alive()) {
        let show_cursor = !app.focus_right;
        nvim.render(f.buffer_mut(), inner, show_cursor);
    } else {
        let view = render_netlist_lines(
            &app.raw_netlist,
            &app.netlist,
            app.scroll,
            inner.height as usize,
            &app.diags,
        );
        let wrap = Wrap { trim: false };
        f.render_widget(Paragraph::new(view).wrap(wrap), inner);
    }
}
