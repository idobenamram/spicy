use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use crossbeam_channel::unbounded;
use crossterm::event::{self, Event as CEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::tui::app::App;
use crate::tui::input::handle_key;
use crate::tui::nvim::{NvimEvent, NvimState};
use crate::tui::term::setup_terminal;
use crate::tui::ui::ui;
use crate::tui::worker::{SimCmd, SimMsg, apply_sim_update, worker_loop};
use spicy_parser::{ParseOptions, SourceMap, parse};

fn netlist_grid_size(term: Rect) -> (u16, u16) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(term);
    let left = chunks[0];
    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(left)[1];
    let width = body.width.saturating_sub(2).max(1);
    let height = body.height.saturating_sub(2).max(1);
    (width, height)
}

fn refresh_netlist(app: &mut App, path: &Path) {
    let input = match fs::read_to_string(path) {
        Ok(input) => input,
        Err(err) => {
            app.nvim_warning = Some(format!("netlist refresh failed: {err}"));
            return;
        }
    };

    app.raw_netlist = input.clone();
    app.netlist = input.lines().map(|line| line.to_string()).collect();
    app.scroll = app.scroll.min(app.netlist.len().saturating_sub(1));

    let source_map = SourceMap::new(path.to_path_buf(), input);
    let mut parse_options = ParseOptions {
        work_dir: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
        source_path: path.to_path_buf(),
        source_map,
        max_include_depth: 10,
    };
    match parse(&mut parse_options) {
        Ok(_) => app.diags.clear(),
        Err(err) => app.diags = vec![err],
    }
}

fn clear_config_edit(app: &mut App) {
    app.config_edit = None;
    app.config_error = None;
}

fn toggle_help(app: &mut App) {
    app.show_help = !app.show_help;
    if app.show_help {
        app.show_config = false;
        clear_config_edit(app);
    }
}

fn toggle_config(app: &mut App) {
    app.show_config = !app.show_config;
    if app.show_config {
        app.show_help = false;
    }
    clear_config_edit(app);
}

pub fn run_tui(path: &str) -> Result<()> {
    let input = fs::read_to_string(path).with_context(|| format!("Failed to read {path}"))?;
    let mut terminal = setup_terminal().context("Failed to initialize terminal")?;

    let (tx_cmd, rx_cmd) = unbounded::<SimCmd>();
    let (tx_msg, rx_msg) = unbounded();

    // Spawn worker thread
    let netlist_path = Path::new(path).to_path_buf();
    std::thread::spawn(move || worker_loop(netlist_path, rx_cmd, tx_msg));

    let mut app = App::new(path.to_string(), input);
    let term_size: Rect = terminal
        .size()
        .context("Failed to read terminal size")?
        .into();
    let (grid_width, grid_height) = netlist_grid_size(term_size);
    match NvimState::spawn(path, grid_width, grid_height) {
        Ok(state) => {
            app.nvim = Some(state);
            app.nvim_warning = None;
        }
        Err(err) => {
            app.nvim_warning = Some(format!("nvim unavailable: {err}"));
        }
    }
    let mut fatal_error: Option<String> = None;
    let mut quit_requested = false;

    loop {
        let term_size: Rect = terminal
            .size()
            .context("Failed to read terminal size")?
            .into();
        let (grid_width, grid_height) = netlist_grid_size(term_size);

        let mut saved_paths: Vec<Option<String>> = Vec::new();
        let events = app
            .nvim
            .as_mut()
            .map(|nvim| nvim.poll_events())
            .unwrap_or_default();
        let nvim_dead = app
            .nvim
            .as_ref()
            .is_some_and(|nvim| !nvim.is_alive());
        for event in events {
            match event {
                NvimEvent::Saved(path) => saved_paths.push(path),
                NvimEvent::Help => toggle_help(&mut app),
                NvimEvent::Config => toggle_config(&mut app),
                NvimEvent::Quit => quit_requested = true,
            }
        }

        for saved_path in saved_paths {
            let path = saved_path.unwrap_or_else(|| app.path.clone());
            refresh_netlist(&mut app, Path::new(&path));
        }

        if nvim_dead {
            app.nvim_warning = Some("nvim exited".to_string());
            app.nvim = None;
        }

        if let Some(nvim) = app.nvim.as_mut() {
            if let Err(err) = nvim.resize_if_needed(grid_width, grid_height) {
                app.nvim_warning = Some(format!("nvim resize failed: {err}"));
            }
        }

        if quit_requested {
            break;
        }

        terminal.draw(|f| ui(f, &app))?;

        // non-blocking input
        if event::poll(Duration::from_millis(16))?
            && let CEvent::Key(k) = event::read()?
            && handle_key(k, &mut app, &tx_cmd)?
        {
            break;
        }

        // handle simulator messages
        while let Ok(msg) = rx_msg.try_recv() {
            match msg {
                SimMsg::FatalError(err) => {
                    fatal_error = Some(err);
                    app.running = false;
                }
                other => apply_sim_update(&mut app, other),
            }
        }

        if fatal_error.is_some() || quit_requested {
            break;
        }
    }
    if let Some(nvim) = app.nvim.as_mut() {
        nvim.quit();
    }
    if let Some(err) = fatal_error {
        return Err(anyhow::Error::msg(err));
    }
    Ok(())
}
