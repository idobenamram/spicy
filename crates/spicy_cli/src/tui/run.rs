use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use crossbeam_channel::unbounded;
use crossterm::event::{self, Event as CEvent};

use crate::tui::app::App;
use crate::tui::input::handle_key;
use crate::tui::term::setup_terminal;
use crate::tui::ui::ui;
use crate::tui::worker::{SimCmd, SimMsg, apply_sim_update, worker_loop};

pub fn run_tui(path: &str) -> Result<()> {
    let input = fs::read_to_string(path).with_context(|| format!("Failed to read {path}"))?;
    let mut terminal = setup_terminal().context("Failed to initialize terminal")?;

    let (tx_cmd, rx_cmd) = unbounded::<SimCmd>();
    let (tx_msg, rx_msg) = unbounded();

    // Spawn worker thread
    let netlist_path = Path::new(path).to_path_buf();
    std::thread::spawn(move || worker_loop(netlist_path, rx_cmd, tx_msg));

    let mut app = App::new(path.to_string(), input);
    let mut fatal_error: Option<String> = None;

    loop {
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

        if fatal_error.is_some() {
            break;
        }
    }
    if let Some(err) = fatal_error {
        return Err(anyhow::Error::msg(err));
    }
    Ok(())
}
