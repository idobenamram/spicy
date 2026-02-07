use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::Span as UiSpan;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::utils::centered_rect;

fn help_section(title: &str) -> Line<'static> {
    Line::from(UiSpan::styled(
        title.to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    ))
}

fn help_line(key: &str, desc: &str) -> Line<'static> {
    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let key_label = format!("{:<14}", key);
    Line::from(vec![
        UiSpan::styled(key_label, key_style),
        UiSpan::raw(desc.to_string()),
    ])
}

fn help_text() -> Text<'static> {
    let lines = Vec::from([
        help_section("general"),
        help_line("q", "quit (tui mode)"),
        help_line("h / ?", "toggle help (tui mode)"),
        help_line("c", "toggle config (tui mode)"),
        help_line("Space h/c/q", "help / config / quit (nvim leader)"),
        help_line("Esc", "close help"),
        Line::from(""),
        help_section("focus"),
        help_line("Alt-h / Alt-l", "focus left/right pane"),
        help_line("Tab", "toggle left/right pane (tui mode)"),
        Line::from(""),
        help_section("netlist (left)"),
        help_line("nvim keys", "edit netlist when focused"),
        Line::from(""),
        help_section("tabs"),
        help_line("Left / Right", "previous/next tab"),
        help_line("1 / 2 / 3", "op / dc / tran"),
        Line::from(""),
        help_section("run"),
        help_line("r", "run current tab"),
        Line::from(""),
        help_section("transient (right)"),
        help_line("Up / Down", "select node"),
        help_line("Enter", "toggle node"),
    ]);
    Text::from(lines)
}

pub(super) fn draw_help(f: &mut Frame, area: Rect) {
    let popup = centered_rect(70, 70, area);
    let block = Block::default().borders(Borders::ALL).title("help");
    let wrap = ratatui::widgets::Wrap { trim: false };
    let help = Paragraph::new(help_text()).block(block).wrap(wrap);
    f.render_widget(Clear, popup);
    f.render_widget(help, popup);
}
