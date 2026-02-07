use anyhow::Result;
use crossbeam_channel::Sender;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, ConfigField, Tab};
use crate::tui::worker::SimCmd;
use spicy_simulate::{LinearSolver, TransientIntegrator, solver::klu::KluConfig};

fn clear_config_edit(app: &mut App) {
    app.config_edit = None;
    app.config_error = None;
}

fn toggle_solver(app: &mut App) {
    app.config.solver = match app.config.solver {
        LinearSolver::Klu { .. } => LinearSolver::Blas,
        LinearSolver::Blas => LinearSolver::Klu {
            config: KluConfig::default(),
        },
    };
}

fn toggle_integrator(app: &mut App) {
    app.config.integrator = match app.config.integrator {
        TransientIntegrator::BackwardEuler => TransientIntegrator::Trapezoidal,
        TransientIntegrator::Trapezoidal => TransientIntegrator::BackwardEuler,
    };
}

fn start_config_edit(app: &mut App) {
    app.config_error = None;
    let value = match app.config_field {
        ConfigField::AbsTol => format!("{:e}", app.config.newton.abs_tol),
        ConfigField::RelTol => format!("{:e}", app.config.newton.rel_tol),
        ConfigField::MaxIters => app.config.newton.max_iters.to_string(),
        _ => String::new(),
    };
    app.config_edit = Some(value);
}

fn apply_config_edit(app: &mut App) {
    let Some(input) = app.config_edit.as_deref() else {
        return;
    };
    let trimmed = input.trim();
    if trimmed.is_empty() {
        app.config_error = Some("value required".to_string());
        return;
    }
    match app.config_field {
        ConfigField::AbsTol => match trimmed.parse::<f64>() {
            Ok(v) if v.is_finite() && v > 0.0 => {
                app.config.newton.abs_tol = v;
                clear_config_edit(app);
            }
            _ => app.config_error = Some("abs_tol must be a positive number".to_string()),
        },
        ConfigField::RelTol => match trimmed.parse::<f64>() {
            Ok(v) if v.is_finite() && v > 0.0 => {
                app.config.newton.rel_tol = v;
                clear_config_edit(app);
            }
            _ => app.config_error = Some("rel_tol must be a positive number".to_string()),
        },
        ConfigField::MaxIters => match trimmed.parse::<usize>() {
            Ok(v) if v > 0 => {
                app.config.newton.max_iters = v;
                clear_config_edit(app);
            }
            _ => app.config_error = Some("max_iters must be a positive integer".to_string()),
        },
        _ => {}
    }
}

fn handle_config_key(k: KeyEvent, app: &mut App) -> Result<bool> {
    if let Some(buffer) = app.config_edit.as_mut() {
        match k.code {
            KeyCode::Esc => {
                clear_config_edit(app);
            }
            KeyCode::Enter => {
                apply_config_edit(app);
            }
            KeyCode::Backspace => {
                buffer.pop();
            }
            KeyCode::Char(c) => {
                let allow = match app.config_field {
                    ConfigField::MaxIters => c.is_ascii_digit(),
                    _ => c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '-' | '+'),
                };
                if allow {
                    buffer.push(c);
                }
            }
            _ => {}
        }
        return Ok(false);
    }

    match k.code {
        KeyCode::Esc => {
            app.show_config = false;
            clear_config_edit(app);
        }
        KeyCode::Up | KeyCode::Char('k') => app.config_field = app.config_field.prev(),
        KeyCode::Down | KeyCode::Char('j') => app.config_field = app.config_field.next(),
        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => match app.config_field {
            ConfigField::Solver => toggle_solver(app),
            ConfigField::Integrator => toggle_integrator(app),
            _ => {}
        },
        KeyCode::Enter => match app.config_field {
            ConfigField::Solver => toggle_solver(app),
            ConfigField::Integrator => toggle_integrator(app),
            ConfigField::AbsTol | ConfigField::RelTol | ConfigField::MaxIters => {
                start_config_edit(app);
            }
        },
        _ => {}
    }
    Ok(false)
}

fn panel_switch_from_key(code: KeyCode, modifiers: KeyModifiers) -> Option<bool> {
    if modifiers.contains(KeyModifiers::ALT) {
        if let KeyCode::Char(c) = code {
            match c.to_ascii_lowercase() {
                'h' => return Some(false),
                'l' => return Some(true),
                _ => {}
            }
        }
    }

    match code {
        KeyCode::Char('\u{02D9}') => Some(false), // Option-h on macOS US layout
        KeyCode::Char('\u{00AC}') => Some(true),  // Option-l on macOS US layout
        _ => None,
    }
}

pub fn handle_key(k: KeyEvent, app: &mut App, tx: &Sender<SimCmd>) -> Result<bool> {
    let nvim_active = app.nvim.is_some() && !app.focus_right;
    if let Some(focus_right) = panel_switch_from_key(k.code, k.modifiers) {
        app.focus_right = focus_right;
        return Ok(false);
    }
    if k.code == KeyCode::Char('q') && !nvim_active {
        return Ok(true);
    }

    if app.show_help {
        if matches!(k.code, KeyCode::Esc) {
            app.show_help = false;
        }
        return Ok(false);
    }
    if app.show_config {
        return handle_config_key(k, app);
    }

    if nvim_active {
        if let Some(nvim) = app.nvim.as_mut() {
            nvim.send_key(k)?;
        }
        return Ok(false);
    }

    match k.code {
        KeyCode::Char('h') | KeyCode::Char('?') => {
            app.show_help = !app.show_help;
            if app.show_help {
                app.show_config = false;
                clear_config_edit(app);
            }
            return Ok(false);
        }
        KeyCode::Char('c') => {
            app.show_config = !app.show_config;
            if app.show_config {
                app.show_help = false;
            }
            clear_config_edit(app);
            return Ok(false);
        }
        // movement and navigation (tui-only)
        KeyCode::Char('j') if !app.focus_right && app.nvim.is_none() => {
            app.scroll = app.scroll.saturating_add(1);
        }
        KeyCode::Char('k') if !app.focus_right && app.nvim.is_none() => {
            app.scroll = app.scroll.saturating_sub(1);
        }
        KeyCode::Char('g')
            if !app.focus_right
                && app.nvim.is_none()
                && k.modifiers.contains(KeyModifiers::SHIFT) =>
        {
            app.scroll = app.netlist.len().saturating_sub(1)
        }
        KeyCode::Char('g') if !app.focus_right && app.nvim.is_none() => app.scroll = 0,
        KeyCode::Left => app.tab = app.tab.prev(),
        KeyCode::Right => app.tab = app.tab.next(),
        // transient tab node selection
        KeyCode::Down if app.focus_right && matches!(app.tab, Tab::Trans) => {
            app.trans_list_index = app.trans_list_index.saturating_add(1);
        }
        KeyCode::Up if app.focus_right && matches!(app.tab, Tab::Trans) => {
            app.trans_list_index = app.trans_list_index.saturating_sub(1);
        }
        KeyCode::Enter if app.focus_right && matches!(app.tab, Tab::Trans) => {
            if let Some(tr) = &app.trans
                && !tr.node_names.is_empty()
            {
                let idx = app
                    .trans_list_index
                    .min(tr.node_names.len().saturating_sub(1));
                if let Some(pos) = app.trans_selected_nodes.iter().position(|&i| i == idx) {
                    app.trans_selected_nodes.remove(pos);
                } else {
                    app.trans_selected_nodes.push(idx);
                }
            }
        }
        KeyCode::Char('1') => app.tab = Tab::Op,
        KeyCode::Char('2') => app.tab = Tab::DC,
        KeyCode::Char('3') => app.tab = Tab::Trans,
        // KeyCode::Char('3') => app.tab = Tab::Ac,
        KeyCode::Char('r') => {
            tx.send(SimCmd::RunCurrentTab {
                tab: app.tab,
                config: app.config.clone(),
            })?;
            app.running = true;
        }
        _ => {}
    }
    Ok(false)
}
