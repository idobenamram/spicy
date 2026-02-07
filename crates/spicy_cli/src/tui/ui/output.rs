use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Span as UiSpan;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs};
use spicy_simulate::{DcSweepResult, OperatingPointResult};

use crate::tui::app::{App, Tab};
use crate::tui::graph::{Graph, Series, compute_y_bounds};

use super::utils::split_v;

fn palette_color(index: usize) -> Color {
    // Stable color mapping for series
    const COLORS: [Color; 8] = [
        Color::Yellow,
        Color::Cyan,
        Color::LightMagenta,
        Color::Green,
        Color::Blue,
        Color::LightRed,
        Color::LightCyan,
        Color::Magenta,
    ];
    COLORS[index % COLORS.len()]
}

pub(super) fn draw_outputs(f: &mut Frame, area: Rect, app: &App) {
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
        Line::from(vec![
            UiSpan::styled("⏱ ", Style::default().fg(Color::LightMagenta)),
            UiSpan::raw("tran"),
        ]),
    ];

    let tabs = Tabs::new(titles)
        .select(app.tab as u8 as usize)
        .block(Block::default().borders(Borders::ALL));

    let tabs_style = if app.right_pane_focused() {
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
    } else if app.tab == Tab::Trans
        && let Some(tr) = &app.trans
    {
        draw_tran(f, body, app, tr);
    } else {
        f.render_widget(
            Paragraph::new("no results").block(Block::default().borders(Borders::ALL)),
            body,
        );
    }
}

fn draw_op(f: &mut Frame, area: Rect, op: &OperatingPointResult) {
    use std::collections::{BTreeSet, HashMap};

    let mut names: BTreeSet<String> = BTreeSet::new();
    for (n, _) in &op.voltages {
        names.insert(n.clone());
    }
    for (n, _) in &op.currents {
        names.insert(n.clone());
    }

    let vmap: HashMap<&str, f64> = op.voltages.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    let imap: HashMap<&str, f64> = op.currents.iter().map(|(n, i)| (n.as_str(), *i)).collect();

    let header = Row::new(vec![
        Cell::from("node"),
        Cell::from("voltage (V)"),
        Cell::from("current (A)"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows = names.into_iter().map(|name| {
        let v_str = match vmap.get(name.as_str()) {
            Some(v) => format!("{:.6}", v),
            None => "-".to_string(),
        };
        let i_str = match imap.get(name.as_str()) {
            Some(i) => format!("{:.6}", i),
            None => "-".to_string(),
        };
        Row::new(vec![Cell::from(name), Cell::from(v_str), Cell::from(i_str)])
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
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("operating point"),
    );

    f.render_widget(table, area);
}

fn draw_dc(f: &mut Frame, area: Rect, _dc: &DcSweepResult) {
    f.render_widget(
        Paragraph::new(" DC sweep (rendering TBD) ").block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn draw_tran(
    f: &mut Frame,
    area: Rect,
    app: &crate::tui::app::App,
    tr: &spicy_simulate::trans::TransientResult,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Left: Chart with GraphType::Line
    let show_indices = if app.trans_selected_nodes.is_empty() {
        vec![0usize]
    } else {
        app.trans_selected_nodes.clone()
    };

    let mut datasets: Vec<Series> = Vec::new();

    for (index, output_index) in show_indices.iter().enumerate() {
        let values: Vec<f64> = tr
            .samples
            .iter()
            .map(|s| {
                if *output_index < s.len() {
                    s[*output_index]
                } else {
                    0.0
                }
            })
            .collect();

        let name = tr
            .node_names
            .get(*output_index)
            .cloned()
            .unwrap_or_else(|| format!("n{}", output_index));

        datasets.push(Series::from_times_and_values(
            name,
            palette_color(index),
            &tr.times,
            &values,
        ));
    }

    let [y_min, y_max] = compute_y_bounds(&datasets);

    let g = Graph {
        title: "transient",
        x_label: "time",
        y_label: "V",
        x_bounds: [
            *tr.times.first().unwrap_or(&0.0),
            *tr.times.last().unwrap_or(&1.0),
        ],
        y_bounds: [y_min, y_max],
        series: datasets,
        x_is_time: true,
        x_label_count: 0,
        y_label_count: 0,
    };
    g.render(f, chunks[0]);

    draw_tran_node_list(f, chunks[1], app, tr);
}

fn draw_tran_node_list(
    f: &mut Frame,
    area: Rect,
    app: &crate::tui::app::App,
    tr: &spicy_simulate::trans::TransientResult,
) {
    let mut rows: Vec<Row> = Vec::new();
    let current = app
        .trans_list_index
        .min(tr.node_names.len().saturating_sub(1));
    for (i, name) in tr.node_names.iter().enumerate() {
        let selected = app.trans_selected_nodes.contains(&i);
        let is_current = i == current;
        let marker = if selected { "[x]" } else { "[ ]" };
        let sel_cell = if is_current {
            format!(">{}", marker)
        } else {
            format!(" {}", marker)
        };
        let mut row = Row::new(vec![Cell::from(sel_cell), Cell::from(name.clone())]);
        if is_current {
            let style = if app.right_pane_focused() {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            };
            row = row.style(style);
        }
        rows.push(row);
    }
    let node_table = Table::new(rows, [Constraint::Length(6), Constraint::Min(0)])
        .header(
            Row::new(vec![Cell::from("sel"), Cell::from("node")])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(Block::default().borders(Borders::ALL).title("nodes"));
    f.render_widget(node_table, area);
}
