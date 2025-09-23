use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType};
use ratatui::Frame;

pub struct Series {
    pub name: String,
    pub color: Color,
    pub points: Vec<(f64, f64)>,
}

pub struct Graph<'a> {
    pub title: &'a str,
    pub x_label: &'a str,
    pub y_label: &'a str,
    pub x_bounds: [f64; 2],
    pub y_bounds: [f64; 2],
    pub series: Vec<Series>,
    pub x_is_time: bool,
    pub x_label_count: usize,
    pub y_label_count: usize,
}

impl<'a> Graph<'a> {
    pub fn render(&self, f: &mut Frame, area: Rect) {
        // Build datasets
        let datasets: Vec<Dataset> = self
            .series
            .iter()
            .map(|s| {
                Dataset::default()
                    .name(s.name.as_str())
                    .marker(Marker::Braille)
                    .style(Style::default().fg(s.color))
                    .graph_type(GraphType::Line)
                    .data(&s.points)
            })
            .collect();

        // Compute labels
        let x_count = if self.x_label_count == 0 { ((area.width as usize) / 12).clamp(3, 8) } else { self.x_label_count };
        let y_count = if self.y_label_count == 0 { ((area.height as usize) / 3).clamp(3, 8) } else { self.y_label_count };
        let x_labels = make_labels(self.x_bounds[0], self.x_bounds[1], x_count, if self.x_is_time { LabelKind::Time } else { LabelKind::Number });
        let y_labels = make_labels(self.y_bounds[0], self.y_bounds[1], y_count, LabelKind::Number);

        let chart = Chart::new(datasets)
            .x_axis(
                Axis::default()
                    .title(self.x_label)
                    .bounds(self.x_bounds)
                    .labels(x_labels)
                    .style(Style::default().fg(Color::Gray)),
            )
            .y_axis(
                Axis::default()
                    .title(self.y_label)
                    .bounds(self.y_bounds)
                    .labels(y_labels)
                    .style(Style::default().fg(Color::Gray)),
            )
            .block(Block::default().borders(Borders::ALL).title(self.title));

        f.render_widget(chart, area);
    }
}

use ratatui::prelude::Span as UiSpan;

#[derive(Clone, Copy)]
enum LabelKind { Time, Number }

fn make_labels(min: f64, max: f64, desired: usize, kind: LabelKind) -> Vec<UiSpan<'static>> {
    let ticks = compute_ticks(min, max, desired.max(2));
    ticks
        .into_iter()
        .map(|t| match kind {
            LabelKind::Time => UiSpan::raw(format_time(t)),
            LabelKind::Number => UiSpan::raw(format_si(t)),
        })
        .collect()
}

fn nice_num(range: f64, round: bool) -> f64 {
    let exponent = range.abs().log10().floor();
    let fraction = range / 10f64.powf(exponent);
    let nice_fraction = if round {
        if fraction < 1.5 { 1.0 } else if fraction < 3.0 { 2.0 } else if fraction < 7.0 { 5.0 } else { 10.0 }
    } else {
        if fraction <= 1.0 { 1.0 } else if fraction <= 2.0 { 2.0 } else if fraction <= 5.0 { 5.0 } else { 10.0 }
    };
    nice_fraction * 10f64.powf(exponent)
}

fn compute_ticks(min: f64, max: f64, desired: usize) -> Vec<f64> {
    let mut lo = min.min(max);
    let mut hi = max.max(min);
    if (hi - lo).abs() < f64::EPSILON { hi = lo + 1.0; }
    let range = nice_num(hi - lo, false);
    let d = nice_num(range / (desired as f64 - 1.0), true);
    let graph_lo = (lo / d).floor() * d;
    let graph_hi = (hi / d).ceil() * d;
    let mut ticks = Vec::new();
    let mut x = graph_lo;
    while x <= graph_hi + d * 0.5 {
        ticks.push(x);
        x += d;
    }
    ticks
}

fn format_time(t: f64) -> String {
    // Choose unit based on magnitude
    let at = t.abs();
    if at >= 1.0 { format!("{:.3}s", t) }
    else if at >= 1e-3 { format!("{:.3}ms", t * 1e3) }
    else if at >= 1e-6 { format!("{:.3}µs", t * 1e6) }
    else { format!("{:.3}ns", t * 1e9) }
}

fn format_si(x: f64) -> String {
    let ax = x.abs();
    if ax == 0.0 { return "0".to_string(); }
    let (scale, suffix) = if ax >= 1e9 { (1e-9, "G") }
        else if ax >= 1e6 { (1e-6, "M") }
        else if ax >= 1e3 { (1e-3, "k") }
        else if ax >= 1.0 { (1.0, "") }
        else if ax >= 1e-3 { (1e3, "m") }
        else if ax >= 1e-6 { (1e6, "µ") }
        else if ax >= 1e-9 { (1e9, "n") }
        else { (1e12, "p") };
    format!("{:.3}{}", x * scale, suffix)
}

impl Series {
    pub fn from_times_and_values(name: String, color: Color, times: &[f64], values: &[f64]) -> Self {
        let points: Vec<(f64, f64)> = times
            .iter()
            .copied()
            .zip(values.iter().copied())
            .collect();
        Self { name, color, points }
    }
}

pub fn compute_y_bounds(series: &[Series]) -> [f64; 2] {
    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for s in series {
        for &(_, y) in &s.points {
            if y < min_v { min_v = y; }
            if y > max_v { max_v = y; }
        }
    }
    if min_v == f64::INFINITY { return [0.0, 1.0]; }
    if (max_v - min_v).abs() < 1e-9 { return [min_v - 0.5, max_v + 0.5]; }
    let pad = (max_v - min_v) * 0.05;
    [min_v - pad, max_v + pad]
}


