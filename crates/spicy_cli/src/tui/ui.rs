use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Span as UiSpan;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs, Table, Row, Cell};
use spicy_simulate::{DcSweepResult, OperatingPointResult};

use crate::tui::app::{App, Tab};

pub fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(f.size());

    draw_netlist(f, chunks[0], app);
    draw_outputs(f, chunks[1], app);
}

pub fn draw_netlist(f: &mut Frame, area: Rect, app: &App) {
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

    // diagnostics map: line -> messages
    let mut diag_lines = std::collections::HashMap::<usize, Vec<&str>>::new();
    for d in &app.diags {
        diag_lines.entry(d.line).or_default().push(d.msg.as_str());
    }

    let view = render_netlist_lines(&app.netlist, app.scroll, body.height as usize, &diag_lines);
    f.render_widget(
        Paragraph::new(view).block(Block::default().borders(Borders::ALL)),
        body,
    );
}

pub fn draw_op(f: &mut Frame, area: Rect, op: &OperatingPointResult) {
    use std::collections::{BTreeSet, HashMap};

    let mut names: BTreeSet<String> = BTreeSet::new();
    for (n, _) in &op.voltages { names.insert(n.clone()); }
    for (n, _) in &op.currents { names.insert(n.clone()); }

    let vmap: HashMap<&str, f64> = op.voltages.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    let imap: HashMap<&str, f64> = op.currents.iter().map(|(n, i)| (n.as_str(), *i)).collect();

    let header = Row::new(vec![
        Cell::from("node"),
        Cell::from("voltage (V)"),
        Cell::from("current (A)"),
    ]).style(Style::default().add_modifier(Modifier::BOLD));

    let rows = names.into_iter().map(|name| {
        let v_str = match vmap.get(name.as_str()) {
            Some(v) => format!("{:.6}", v),
            None => "-".to_string(),
        };
        let i_str = match imap.get(name.as_str()) {
            Some(i) => format!("{:.6}", i),
            None => "-".to_string(),
        };
        Row::new(vec![
            Cell::from(name),
            Cell::from(v_str),
            Cell::from(i_str),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(50),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title("operating point"));

    f.render_widget(table, area);
}

pub fn draw_outputs(f: &mut Frame, area: Rect, app: &App) {
    let [tabs_area, body] = split_v(area, 3);

    let titles = vec![
        Line::from(vec![
            UiSpan::styled("⚡ ", Style::default().fg(Color::Yellow)),
            UiSpan::raw("op"),
        ]),
        Line::from(vec![
            UiSpan::styled("↯ ", Style::default().fg(Color::Cyan)),
            UiSpan::raw("dc"),
        ]),
    ];

    let tabs = Tabs::new(titles)
        .select(match app.tab {
            Tab::Op => 0,
            Tab::DC => 1,
        })
        .block(Block::default().borders(Borders::ALL));

    let tabs_style = if app.focus_right {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    f.render_widget(tabs.style(tabs_style), tabs_area);

    if app.tab == Tab::Op
        && let Some(op) = &app.op
    {
        draw_op(f, body, op);
    } else if app.tab == Tab::DC
        && let Some(dc) = &app.dc
    {
        draw_dc(f, body, dc);
    } else {
        f.render_widget(
            Paragraph::new("no results").block(Block::default().borders(Borders::ALL)),
            body,
        );
    }
}

pub fn draw_dc(f: &mut Frame, area: Rect, _dc: &DcSweepResult) {
    f.render_widget(
        Paragraph::new(" DC sweep (rendering TBD) ").block(Block::default().borders(Borders::ALL)),
        area,
    );
}

pub fn split_v(area: Rect, top: u16) -> [Rect; 2] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(top), Constraint::Min(0)])
        .split(area);
    [chunks[0], chunks[1]]
}

pub fn render_netlist_lines(
    netlist: &Vec<String>,
    scroll: usize,
    height: usize,
    diags: &std::collections::HashMap<usize, Vec<&str>>,
) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let gutter_width = ((scroll + height).max(netlist.len()).saturating_sub(1) + 1)
        .to_string()
        .len()
        .max(2);
    for (idx, raw) in netlist.iter().enumerate().skip(scroll).take(height) {
        let ln = idx + 1;
        let has_diag = diags.get(&ln).is_some();
        let gutter = if has_diag {
            format!("{ln:>w$}! ", w = gutter_width)
        } else {
            format!("{ln:>w$}  ", w = gutter_width)
        };
        let style = if has_diag {
            Style::default().fg(Color::Red)
        } else {
            Style::default()
        };
        let mut spans = Vec::new();
        spans.push(UiSpan::styled(gutter, Style::default().fg(Color::DarkGray)));
        spans.push(UiSpan::styled(raw.clone(), style));
        lines.push(Line::from(spans));
    }
    Text::from(lines)
}
