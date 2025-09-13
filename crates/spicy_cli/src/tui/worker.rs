use crossbeam_channel::{Receiver, Sender};
use spicy_simulate::{DcSweepResult, OperatingPointResult, simulate_dc, simulate_op};

use crate::tui::app::{App, Diagnostic, Tab};
use spicy_parser::{netlist_types::Command, parser::parse};

#[derive(Clone, Debug)]
pub enum SimCmd {
    RunCurrentTab(Tab),
}

#[derive(Debug)]
pub enum SimMsg {
    Diagnostics(Vec<Diagnostic>),
    SimulationStarted,
    Op(OperatingPointResult),
    Dc(DcSweepResult),
    Done,
}

pub fn apply_sim_update(app: &mut App, msg: SimMsg) {
    match msg {
        SimMsg::Diagnostics(d) => app.diags = d,
        SimMsg::SimulationStarted => app.running = true,
        SimMsg::Op(op) => app.op = Some(op),
        SimMsg::Dc(dc) => app.dc = Some(dc),
        SimMsg::Done => app.running = false,
    }
}

pub fn worker_loop(netlist: String, rx: Receiver<SimCmd>, tx: Sender<SimMsg>) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            SimCmd::RunCurrentTab(tab) => {
                // Parse
                let deck = match parse(&netlist) {
                    Ok(deck) => deck,
                    Err(e) => {
                        let mut diags = Vec::new();
                        if let Some(span) = e.error_span() {
                            let line = offset_to_line(&netlist, span.start);
                            diags.push(Diagnostic {
                                line,
                                msg: e.to_string(),
                            });
                        } else {
                            diags.push(Diagnostic {
                                line: 1,
                                msg: e.to_string(),
                            });
                        }
                        let _ = tx.send(SimMsg::Diagnostics(diags));
                        continue;
                    }
                };

                for command in &deck.commands {
                    match command {
                        Command::Op(_) => {
                            let op = simulate_op(&deck);
                            let _ = tx.send(SimMsg::Op(op));
                            continue;
                        }
                        Command::Dc(command_params) => {
                            let dc = simulate_dc(&deck, &command_params);
                            let _ = tx.send(SimMsg::Dc(dc));
                            continue;
                        }
                        _ => {}
                    }
                }

                let _ = tx.send(SimMsg::Done);
            }
        }
    }
}

fn offset_to_line(src: &str, byte_offset: usize) -> usize {
    let offset = byte_offset.min(src.len());
    let prefix = &src[..offset];
    prefix.chars().filter(|&c| c == '\n').count() + 1
}
