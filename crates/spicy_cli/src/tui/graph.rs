use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType};

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

static mut FIRST_RENDER: bool = true;
// Simple file logger utility for debugging
#[allow(dead_code)]
fn log_to_file(msg: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;
    if !unsafe { FIRST_RENDER } {
        return;
    }
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("spicy_cli.log")
    {
        let _ = writeln!(file, "{}", msg);
    }
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

        log_to_file(&format!(
            "area.width: {:?}, area.height: {:?}",
            area.width, area.height
        ));
        // TOOD: this could probably be done better
        // Compute labels
        let x_count = if self.x_label_count == 0 {
            ((area.width as usize) / 12).clamp(3, 10)
        } else {
            self.x_label_count
        };
        let x_ticks = compute_ticks(self.x_bounds[0], self.x_bounds[1], x_count.max(2));
        let x_labels = make_labels(
            x_ticks,
            if self.x_is_time {
                LabelKind::Time
            } else {
                LabelKind::Number
            },
        );
        log_to_file(&format!("x_count: {}", x_count));

        let y_count = if self.y_label_count == 0 {
            ((area.height as usize) / 3).clamp(3, 8)
        } else {
            self.y_label_count
        };
        let y_ticks = compute_ticks(self.y_bounds[0], self.y_bounds[1], y_count.max(2));
        log_to_file(&format!("y_ticks: {:?}", y_ticks));
        let y_labels = make_labels(y_ticks, LabelKind::Number);

        unsafe {
            FIRST_RENDER = false;
        }

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
enum LabelKind {
    Time,
    Number,
}

fn make_labels(ticks: Vec<f64>, kind: LabelKind) -> Vec<UiSpan<'static>> {
    ticks
        .into_iter()
        .map(|t| match kind {
            LabelKind::Time => UiSpan::raw(format_time(t)),
            LabelKind::Number => UiSpan::raw(format_si(t)),
        })
        .collect()
}

fn compute_ticks(min: f64, max: f64, desired: usize) -> Vec<f64> {
    let step = (max - min) / (desired as f64 - 1.0);
    (0..desired).map(|i| min + step * (i as f64)).collect()
}

fn format_time(t: f64) -> String {
    // Choose unit based on magnitude
    let at = t.abs();
    if at >= 1.0 {
        format!("{:.3}s", t)
    } else if at >= 1e-3 {
        format!("{:.3}ms", t * 1e3)
    } else if at >= 1e-6 {
        format!("{:.3}µs", t * 1e6)
    } else {
        format!("{:.3}ns", t * 1e9)
    }
}

fn format_si(x: f64) -> String {
    let ax = x.abs();
    if ax == 0.0 {
        return "0".to_string();
    }
    let (scale, suffix) = if ax >= 1e9 {
        (1e-9, "G")
    } else if ax >= 1e6 {
        (1e-6, "M")
    } else if ax >= 1e3 {
        (1e-3, "k")
    } else if ax >= 1.0 {
        (1.0, "")
    } else if ax >= 1e-3 {
        (1e3, "m")
    } else if ax >= 1e-6 {
        (1e6, "µ")
    } else if ax >= 1e-9 {
        (1e9, "n")
    } else {
        (1e12, "p")
    };
    format!("{:.3}{}", x * scale, suffix)
}

impl Series {
    pub fn from_times_and_values(
        name: String,
        color: Color,
        times: &[f64],
        values: &[f64],
    ) -> Self {
        let points: Vec<(f64, f64)> = times.iter().copied().zip(values.iter().copied()).collect();
        Self {
            name,
            color,
            points,
        }
    }
}

pub fn compute_y_bounds(series: &[Series]) -> [f64; 2] {
    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for s in series {
        for &(_, y) in &s.points {
            if y < min_v {
                min_v = y;
            }
            if y > max_v {
                max_v = y;
            }
        }
    }
    if min_v == f64::INFINITY {
        return [0.0, 1.0];
    }
    if (max_v - min_v).abs() < 1e-9 {
        return [min_v - 0.5, max_v + 0.5];
    }
    [min_v, max_v]
}
