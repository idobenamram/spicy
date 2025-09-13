use anyhow::Result;
use crossbeam_channel::Sender;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, Tab, prev_tab, next_tab};
use crate::tui::worker::SimCmd;

pub fn handle_key(k: KeyEvent, app: &mut App, tx: &Sender<SimCmd>) -> Result<bool> {
    match k.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Tab => app.focus_right = !app.focus_right,
        KeyCode::Char('j') if !app.focus_right => app.scroll = app.scroll.saturating_add(1),
        KeyCode::Char('k') if !app.focus_right => app.scroll = app.scroll.saturating_sub(1),
        KeyCode::Char('g') if !app.focus_right && k.modifiers.contains(KeyModifiers::SHIFT) => app.scroll = app.netlist.len().saturating_sub(1),
        KeyCode::Char('g') if !app.focus_right => app.scroll = 0,
        KeyCode::Char('h') => app.tab = prev_tab(app.tab),
        KeyCode::Char('l') => app.tab = next_tab(app.tab),
        // KeyCode::Char('J') => app.out_idx = (app.out_idx + 1).min(app.outputs.len().saturating_sub(1)),
        // KeyCode::Char('K') => app.out_idx = app.out_idx.saturating_sub(1),
        KeyCode::Char('1') => app.tab = Tab::Op,
        KeyCode::Char('2') => app.tab = Tab::DC,
        // KeyCode::Char('3') => app.tab = Tab::Ac,
        KeyCode::Char('r') => { tx.send(SimCmd::RunCurrentTab(app.tab))?; app.running = true; }
        _ => {}
    }
    Ok(false)
}

