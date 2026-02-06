use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::tui::app::App;

use super::utils::{render_netlist_lines, split_v};

pub(super) fn draw_netlist(f: &mut Frame, area: Rect, app: &App) {
    let [hdr, body] = split_v(area, 3);

    let hdr_text = format!(" {} ", app.path);
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

    let view = render_netlist_lines(
        &app.raw_netlist,
        &app.netlist,
        app.scroll,
        body.height as usize,
        &app.diags,
    );
    let wrap = Wrap { trim: false };
    f.render_widget(
        Paragraph::new(view)
            .block(Block::default().borders(Borders::ALL))
            .wrap(wrap),
        body,
    );
}
