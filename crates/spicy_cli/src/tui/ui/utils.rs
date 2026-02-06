use std::collections::HashMap;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Span as UiSpan;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use spicy_parser::Span;
use spicy_parser::error::SpicyError;

pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);

    horizontal[1]
}

pub(crate) fn split_v(area: Rect, top: u16) -> [Rect; 2] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(top), Constraint::Min(0)])
        .split(area);
    [chunks[0], chunks[1]]
}

pub struct LineDiagnostic {
    pub line_start: usize,
    pub line_end: usize,
    pub line_index: usize,
    pub span_start_in_line: usize,
    pub span_end_in_line: usize,
}

impl LineDiagnostic {
    pub fn new(src: &str, span: Span) -> Option<Self> {
        let len = src.len();
        if len == 0 {
            return None;
        }
        let start = span.start.min(len.saturating_sub(1));
        let end = span.end.min(len.saturating_sub(1));
        if start > end {
            return None;
        }

        // find line bounds
        let line_start = src[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);

        let line_index = src[..line_start].chars().filter(|&c| c == '\n').count() + 1;
        let line_end = src[end + 1..]
            .find('\n')
            .map(|i| end + 1 + i)
            .unwrap_or(len);

        let span_start_in_line = start - line_start;
        let span_end_in_line = (end - line_start).max(span_start_in_line) + 1;
        Some(Self {
            line_start,
            line_index,
            span_start_in_line,
            span_end_in_line,
            line_end,
        })
    }
}

pub fn format_error_snippet(src: &str, span: Span) -> Option<String> {
    let ld = LineDiagnostic::new(src, span)?;
    let len = src.len();
    let start = span.start.min(len.saturating_sub(1));
    let end = span.end.min(len.saturating_sub(1));
    let prefix = &src[ld.line_start..start];
    let highlight = &src[start..=end];
    let line = &src[ld.line_start..ld.line_end];
    let col = prefix.chars().count();
    let width = highlight.chars().count().max(1);
    let line_no = ld.line_index;

    let underline = "~".repeat(width);
    Some(format!(
        "{:>4} | {}\n     | {:space$}\x1b[31m{}\x1b[0m\n",
        line_no,
        line,
        "",
        underline,
        space = col
    ))
}

pub(crate) fn render_netlist_lines(
    raw_netlist: &str,
    netlist: &[String],
    scroll: usize,
    height: usize,
    diags: &[SpicyError],
) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let gutter_width = ((scroll + height).max(netlist.len()).saturating_sub(1) + 1)
        .to_string()
        .len()
        .max(2);

    // for each diagnostic, find the line number
    let mut diags_by_line: HashMap<usize, (LineDiagnostic, &SpicyError)> =
        std::collections::HashMap::new();
    for diag in diags {
        if let Some(span) = diag.error_span()
            && let Some(ld) = LineDiagnostic::new(raw_netlist, span)
        {
            diags_by_line.insert(ld.line_index, (ld, diag));
        } else {
            // simply display it at the top
            let spans = vec![
                UiSpan::styled("! ".to_string(), Style::default().fg(Color::Red)),
                UiSpan::styled(diag.to_string(), Style::default().fg(Color::Red)),
            ];
            lines.push(Line::from(spans));
        }
    }

    for (idx, raw) in netlist.iter().enumerate().skip(scroll).take(height) {
        let ln = idx + 1;
        let gutter = format!("{ln:>w$}  ", w = gutter_width);
        let mut spans = Vec::new();
        let gutter_len = gutter.chars().count();
        spans.push(UiSpan::styled(gutter, Style::default().fg(Color::DarkGray)));

        match diags_by_line.get(&ln) {
            Some((ld, diag)) => {
                let err_style = Style::default().fg(Color::Red);
                let pre = &raw[..ld.span_start_in_line];
                let mid = &raw[ld.span_start_in_line..ld.span_end_in_line];
                let post = &raw[ld.span_end_in_line..];
                spans.push(UiSpan::styled(pre.to_string(), Style::default()));
                spans.push(UiSpan::styled(mid.to_string(), err_style));
                spans.push(UiSpan::styled(post.to_string(), Style::default()));
                lines.push(Line::from(spans));

                let mut diag_spans = Vec::new();
                let pre_len = pre.chars().count();
                diag_spans.push(UiSpan::styled(
                    " ".repeat(gutter_len + pre_len),
                    Style::default(),
                ));
                diag_spans.push(UiSpan::styled(
                    "^".repeat(mid.len()),
                    err_style.add_modifier(Modifier::BOLD),
                ));
                diag_spans.push(UiSpan::styled(format!(" {}", diag), err_style));
                lines.push(Line::from(diag_spans));
            }
            None => {
                spans.push(UiSpan::styled(raw.clone(), Style::default()));
                lines.push(Line::from(spans));
            }
        }
    }
    Text::from(lines)
}
