use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Span as UiSpan;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table};
use spicy_simulate::{LinearSolver, TransientIntegrator};

use crate::tui::app::{App, ConfigField, CONFIG_FIELDS};

use super::utils::centered_rect;

fn config_value(app: &App, field: ConfigField) -> String {
    if let Some(edit) = &app.config_edit
        && app.config_field == field
    {
        return format!("{}_", edit.buffer);
    }
    match field {
        ConfigField::Solver => match app.config.solver {
            LinearSolver::Klu { .. } => "klu".to_string(),
            LinearSolver::Blas => "blas".to_string(),
        },
        ConfigField::Integrator => match app.config.integrator {
            TransientIntegrator::BackwardEuler => "backward_euler".to_string(),
            TransientIntegrator::Trapezoidal => "trapezoidal".to_string(),
        },
        ConfigField::AbsTol => format!("{:.3e}", app.config.newton.abs_tol),
        ConfigField::RelTol => format!("{:.3e}", app.config.newton.rel_tol),
        ConfigField::MaxIters => app.config.newton.max_iters.to_string(),
    }
}

fn config_label(field: ConfigField) -> &'static str {
    match field {
        ConfigField::Solver => "solver",
        ConfigField::Integrator => "integrator",
        ConfigField::AbsTol => "abs_tol",
        ConfigField::RelTol => "rel_tol",
        ConfigField::MaxIters => "max_iters",
    }
}

fn config_help_text(app: &App) -> Text<'static> {
    let mut lines = if app.config_edit.is_some() {
        vec![
            Line::from("editing: type value, Enter apply, Esc cancel"),
            Line::from("Backspace deletes"),
            Line::from("Esc or c: close config"),
        ]
    } else {
        vec![
            Line::from("Up/Down select, Left/Right toggle"),
            Line::from("Enter edit numeric values"),
            Line::from("Esc or c: close config"),
        ]
    };
    if let Some(err) = app.config_edit.as_ref().and_then(|edit| edit.error.as_ref()) {
        lines.extend([
            Line::from(""),
            Line::from(UiSpan::styled(
                err.to_string(),
                Style::default().fg(Color::Red),
            )),
        ]);
    }
    Text::from(lines)
}

pub(super) fn draw_config(f: &mut Frame, area: Rect, app: &App) {
    let popup = centered_rect(70, 70, area);
    let block = Block::default().borders(Borders::ALL).title("config");

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(6)])
        .split(popup);

    let rows = CONFIG_FIELDS.iter().copied().map(|field| {
        let label = config_label(field);
        let value = config_value(app, field);
        let mut row = Row::new(vec![Cell::from(label), Cell::from(value)]);
        if field == app.config_field {
            let mut style = Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD);
            if app.config_edit.is_some() {
                style = style.add_modifier(Modifier::REVERSED);
            }
            row = row.style(style);
        }
        row
    });

    let table = Table::new(rows, [Constraint::Length(14), Constraint::Min(0)])
        .header(
            Row::new(vec![Cell::from("setting"), Cell::from("value")])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(block);

    let wrap = ratatui::widgets::Wrap { trim: false };
    let help = Paragraph::new(config_help_text(app))
        .block(Block::default().borders(Borders::ALL).title("keys"))
        .wrap(wrap);

    f.render_widget(Clear, popup);
    f.render_widget(table, sections[0]);
    f.render_widget(help, sections[1]);
}
