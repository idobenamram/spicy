use std::path::PathBuf;

use crossbeam_channel::{Receiver, Sender};
use spicy_simulate::{
    DcSweepResult, OperatingPointResult, SimulationConfig, TransientResult,
    dc::{simulate_dc, simulate_op},
    trans::simulate_trans,
};

use crate::tui::app::App;
use crate::tui::ui::format_error_snippet;
use spicy_parser::{ParseOptions, SourceMap, error::SpicyError, netlist_types::Command, parse};

#[derive(Clone, Debug)]
pub enum SimCmd {
    RunCurrentTab { config: SimulationConfig },
}

#[derive(Debug)]
pub enum SimMsg {
    SimulationStarted,
    Op(OperatingPointResult),
    Dc(DcSweepResult),
    Transient(TransientResult),
    FatalError(String),
    Done,
}

pub fn apply_sim_update(app: &mut App, msg: SimMsg) {
    match msg {
        SimMsg::SimulationStarted => {
            app.op = None;
            app.dc = None;
            app.trans = None;
            app.trans_selected_nodes.clear();
            app.trans_list_index = 0;
        }
        SimMsg::Op(op) => app.op = Some(op),
        SimMsg::Dc(dc) => app.dc = Some(dc),
        SimMsg::Transient(tr) => app.trans = Some(tr),
        _ => {}
    }
    app.ensure_visible_tab();
}

fn format_parse_error(error: &SpicyError, source_map: &SourceMap) -> String {
    let mut out = format!("Parse error: {error}");
    if let Some(span) = error.error_span() {
        let path = source_map.get_path(span.source_index);
        out.push_str(&format!("\n--> {}", path.display()));
        if let Some(snippet) = format_error_snippet(source_map.get_content(span.source_index), span) {
            out.push('\n');
            out.push_str(&snippet);
        }
    }
    out
}

pub fn worker_loop(netlist_path: PathBuf, rx: Receiver<SimCmd>, tx: Sender<SimMsg>) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            SimCmd::RunCurrentTab { config } => {
                let sim_config = config;
                let input = match std::fs::read_to_string(&netlist_path) {
                    Ok(input) => input,
                    Err(err) => {
                        let _ = tx.send(SimMsg::FatalError(format!(
                            "Failed to read netlist: {}",
                            err
                        )));
                        continue;
                    }
                };
                let mut parse_options = ParseOptions::new_with_source(&netlist_path, input);

                let deck = match parse(&mut parse_options) {
                    Ok(deck) => deck,
                    Err(e) => {
                        let _ = tx.send(SimMsg::FatalError(format_parse_error(
                            &e,
                            &parse_options.source_map,
                        )));
                        continue;
                    }
                };

                let _ = tx.send(SimMsg::SimulationStarted);

                for command in &deck.commands {
                    match command {
                        Command::Op(_) => {
                            match simulate_op(&deck, &sim_config) {
                                Ok(op) => {
                                    let _ = tx.send(SimMsg::Op(op));
                                }
                                Err(e) => {
                                    let _ = tx.send(SimMsg::FatalError(format!(
                                        "Simulation error: {}",
                                        e
                                    )));
                                }
                            }
                            continue;
                        }
                        Command::Dc(command_params) => {
                            let dc = simulate_dc(&deck, command_params, &sim_config);
                            let _ = tx.send(SimMsg::Dc(dc));
                            continue;
                        }
                        Command::Tran(command_params) => {
                            match simulate_trans(&deck, command_params, &sim_config) {
                                Ok(tr) => {
                                    let _ = tx.send(SimMsg::Transient(tr));
                                }
                                Err(e) => {
                                    let _ = tx.send(SimMsg::FatalError(format!(
                                        "Simulation error: {}",
                                        e
                                    )));
                                }
                            }
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
