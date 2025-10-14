use anyhow::Result;
use crossbeam_channel::Sender;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, Tab};
use crate::tui::worker::SimCmd;

pub fn handle_key(k: KeyEvent, app: &mut App, tx: &Sender<SimCmd>) -> Result<bool> {
    match k.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Tab => app.focus_right = !app.focus_right,
        KeyCode::Char('j') if !app.focus_right => app.scroll = app.scroll.saturating_add(1),
        KeyCode::Char('k') if !app.focus_right => app.scroll = app.scroll.saturating_sub(1),
        KeyCode::Char('g') if !app.focus_right && k.modifiers.contains(KeyModifiers::SHIFT) => app.scroll = app.netlist.len().saturating_sub(1),
        KeyCode::Char('g') if !app.focus_right => app.scroll = 0,
        KeyCode::Char('h') => app.tab = app.tab.next(),
        KeyCode::Char('l') => app.tab = app.tab.prev(),
        // transient tab node selection
        KeyCode::Down if app.focus_right && matches!(app.tab, Tab::Trans) => {
            app.trans_list_index = app.trans_list_index.saturating_add(1);
        }
        KeyCode::Up if app.focus_right && matches!(app.tab, Tab::Trans) => {
            app.trans_list_index = app.trans_list_index.saturating_sub(1);
        }
        KeyCode::Enter if app.focus_right && matches!(app.tab, Tab::Trans) => {
            if let Some(tr) = &app.trans
                && !tr.node_names.is_empty() {
                    let idx = app.trans_list_index.min(tr.node_names.len().saturating_sub(1));
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
        KeyCode::Char('r') => { tx.send(SimCmd::RunCurrentTab(app.tab))?; app.running = true; }
        _ => {}
    }
    Ok(false)
}

